# Детальный план реализации TQL v2.2 → v2.3

## 📋 ПРИОРИТЕТ P0: QueryExecutor Integration

### Задача 1.1: Исправить QueryExecutor

**Файл**: `src/tql/executor.rs`

```rust
use crate::storage::{StorageBackend, ShardResult};
use crate::toroidal_metrics::{toroidal_distance, ToroidalMetrics};
use tokio::sync::Semaphore;
use std::sync::Arc;
use std::collections::BinaryHeap;

pub struct QueryExecutor<S: StorageBackend> {
    storage: Arc<S>,
    metrics: Arc<ToroidalMetrics>,
    max_concurrent_shards: usize,
}

impl<S: StorageBackend> QueryExecutor<S> {
    pub fn new(storage: Arc<S>, metrics: Arc<ToroidalMetrics>) -> Self {
        Self {
            storage,
            metrics,
            max_concurrent_shards: 16,
        }
    }

    pub async fn execute_query(
        &self,
        query: &Query,
        query_vector: Option<Vec<f32>>,
    ) -> Result<Vec<QueryResult>, String> {
        let query_hash = self.calculate_query_hash(query, query_vector.as_ref());
        
        // 1. Route to shards
        let shard_ids = self.route_query(&query, query_hash).await?;
        
        // 2. Scatter to shards
        let partial_results = self.scatter_to_shards(&query, &shard_ids, query_vector.as_ref().map(|v| v.as_slice())).await?;
        
        // 3. Gather top-K
        let results = self.gather_results(partial_results, query.limit as usize);
        
        // 4. Graph traversal if needed
        let final_results = if query.connected_clause.is_some() || query.within_clause.is_some() {
            self.graph_traversal(&query, &results).await?
        } else {
            results
        };

        Ok(final_results)
    }

    fn calculate_query_hash(&self, query: &Query, query_vector: Option<&[f32]>) -> u64 {
        // Простой hash на основе query pattern + vector
        let mut hasher = xxhash_rust::xxh3::Xxh3::default();
        hasher.update(query.match_clause.source.label.as_bytes());
        if let Some(vec) = query_vector {
            hasher.update(bytemuck::cast_slice(vec));
        }
        hasher.digest64()
    }

    async fn route_query(&self, query: &Query, query_hash: u64) -> Result<Vec<u32>, String> {
        // Hash ring routing - все шарды для начала
        // TODO: Улучшить до selective routing
        let total_shards = self.storage.get_shard_count().await?;
        Ok((0..total_shards).collect())
    }

    async fn scatter_to_shards(
        &self,
        query: &Query,
        shard_ids: &[u32],
        query_vector: Option<&[f32]>,
    ) -> Result<Vec<Vec<ShardResult>>, String> {
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent_shards));
        let mut tasks = tokio::task::JoinSet::new();
        let query_arc = Arc::new(query.clone());
        let vector_arc: Arc<Option<Vec<f32>>> = Arc::new(query_vector.map(|v| v.to_vec()));

        for &shard_id in shard_ids {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let q = Arc::clone(&query_arc);
            let vec_clone = Arc::clone(&vector_arc);

            tasks.spawn(async move {
                let _permit = permit;
                self.local_shard_search(shard_id, &q, vec_clone.as_deref()).await
            });
        }

        let mut all_results = Vec::new();
        while let Some(res) = tasks.join_next().await {
            match res {
                Ok(Ok(shard_results)) => all_results.push(shard_results),
                Ok(Err(e)) => eprintln!("shard error: {e}"),
                Err(e) => eprintln!("task error: {e}"),
            }
        }

        Ok(all_results)
    }

    async fn local_shard_search(
        &self,
        shard_id: u32,
        query: &Query,
        query_vector: Option<&[f32]>,
    ) -> Result<Vec<ShardResult>, String> {
        let td_filter = query
            .where_clause
            .as_ref()
            .and_then(|w| w.toroidal_distance.as_ref())
            .ok_or_else(|| "No TOROIDALDISTANCE in query".to_string())?;

        let threshold = td_filter.threshold;
        let phi = td_filter.phi;

        let qvec = query_vector.ok_or_else(|| "No query vector".to_string())?;

        // Load candidates from storage
        let candidates = self.storage.get_shard_candidates(shard_id).await?;

        let mut results = Vec::new();
        for candidate in candidates {
            let dist = toroidal_distance(&candidate.vector, qvec, phi);
            if dist <= threshold {
                results.push(ShardResult {
                    node_id: candidate.id,
                    distance: dist,
                    shard_id,
                    execution_time: 0, // TODO: measure
                });
            }
        }

        Ok(results)
    }

    fn gather_results(&self, partial_results: Vec<Vec<ShardResult>>, limit: usize) -> Vec<QueryResult> {
        #[derive(Debug)]
        struct HeapResult {
            result: QueryResult,
        }

        impl PartialEq for HeapResult {
            fn eq(&self, other: &Self) -> bool {
                self.result.score == other.result.score
            }
        }
        impl Eq for HeapResult {}

        impl PartialOrd for HeapResult {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                other.result.score.partial_cmp(&self.result.score)
            }
        }

        impl Ord for HeapResult {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                other.result.score.cmp(&self.result.score)
            }
        }

        let mut heap = BinaryHeap::new();

        for shard_results in partial_results {
            for r in shard_results {
                let score = 1.0 - r.distance as f32;
                let result = QueryResult {
                    row: {
                        let mut m = std::collections::HashMap::new();
                        m.insert("id".into(), PropertyValue::Int(r.node_id as i64));
                        m
                    },
                    score,
                    shard_id: r.shard_id,
                    execution_time_ns: r.execution_time,
                };

                if heap.len() < limit {
                    heap.push(HeapResult { result });
                } else if let Some(min) = heap.peek() {
                    if result.score > min.result.score {
                        heap.pop();
                        heap.push(HeapResult { result });
                    }
                }
            }
        }

        let mut out = Vec::with_capacity(heap.len());
        while let Some(h) = heap.pop() {
            out.push(h.result);
        }
        out.reverse();
        out
    }

    async fn graph_traversal(
        &self,
        query: &Query,
        vector_results: &[QueryResult],
    ) -> Result<Vec<QueryResult>, String> {
        if vector_results.is_empty() {
            return Ok(Vec::new());
        }

        // Реализация bidirectional BFS для CONNECTEDTO
        // TODO: Использовать существующий bidirectional_bfs из examples
        Ok(vector_results.to_vec())
    }
}
```

---

## 📋 ПРИОРИТЕТ P1: Cost-based Optimizer

### Задача 2.1: Добавить статистику шардов

**Файл**: `src/tql/optimizer.rs`

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct ShardStats {
    pub vectors_count: u64,
    pub avg_degree: f32,
    pub last_qps: f32,
    pub avg_latency_ms: f64,
    pub last_updated: std::time::Instant,
}

pub struct QueryOptimizer {
    shard_stats: Arc<RwLock<HashMap<u32, ShardStats>>>,
    default_cost: f64,
}

impl QueryOptimizer {
    pub fn new() -> Self {
        Self {
            shard_stats: Arc::new(RwLock::new(HashMap::new())),
            default_cost: 100.0,
        }
    }

    pub fn update_stats(&self, shard_id: u32, stats: ShardStats) {
        self.shard_stats.write().unwrap().insert(shard_id, stats);
    }

    pub fn estimate_plan_cost(&self, plan: &LogicalPlan) -> f64 {
        let mut total_cost = 0.0;

        for step in &plan.steps {
            total_cost += match step {
                PlanStep::RouteShards { .. } => 10.0,
                PlanStep::LocalVectorSearch { limit, .. } => {
                    // Cost пропорционален количеству векторов и limit
                    50.0 + (*limit as f64) * 2.0
                }
                PlanStep::GraphTraversal { max_hops } => {
                    20.0 * (*max_hops as f64)
                }
                PlanStep::TopKMerge { k } => {
                    (*k as f64) * 0.5
                }
                PlanStep::Filter => 5.0,
                PlanStep::Aggregate => 15.0,
                PlanStep::Sort => 10.0,
                PlanStep::Project => 2.0,
            };
        }

        total_cost
    }

    pub fn select_shards(&self, query: &Query) -> Vec<u32> {
        // Выбираем шарды с наименьшей нагрузкой
        let stats = self.shard_stats.read().unwrap();
        let mut shards: Vec<_> = stats.iter().collect();
        
        shards.sort_by(|a, b| {
            a.1.last_qps.partial_cmp(&b.1.last_qps).unwrap()
                .then(a.0.cmp(b.0))
        });

        shards.into_iter()
            .take(8) // Берем топ-8 шардов
            .map(|(&id, _)| id)
            .collect()
    }

    pub fn estimate_selectivity(&self, where_clause: &Option<WhereClause>) -> f64 {
        // Оценка selectivity для фильтров
        match where_clause {
            Some(w) => {
                if w.toroidal_distance.is_some() {
                    0.1 // Тороидальный поиск обычно селективен
                } else if !w.conditions.is_empty() {
                    0.5 // Условные фильтры
                } else {
                    1.0
                }
            }
            None => 1.0,
        }
    }
}
```

### Задача 2.2: Улучшить PlanBuilder

```rust
pub struct PlanBuilder<'a> {
    query: &'a Query,
    optimizer: Arc<QueryOptimizer>,
}

impl<'a> PlanBuilder<'a> {
    pub fn build(query: &'a Query, optimizer: Arc<QueryOptimizer>) -> LogicalPlan {
        let mut steps = Vec::new();
        
        // 1. Route shards с учётом статистики
        let shard_ids = optimizer.select_shards(query);
        steps.push(PlanStep::RouteShards { shard_ids });

        // 2. Vector search
        if let Some(where_clause) = &query.where_clause {
            if where_clause.toroidal_distance.is_some() {
                let selectivity = optimizer.estimate_selectivity(Some(where_clause));
                steps.push(PlanStep::LocalVectorSearch {
                    limit: query.limit,
                    threshold: where_clause.toroidal_distance.as_ref().unwrap().threshold,
                    selectivity,
                });
            }
        }

        // 3. Graph traversal
        if query.connected_clause.is_some() || query.within_clause.is_some() {
            steps.push(PlanStep::GraphTraversal {
                max_hops: query.within_clause.as_ref().map(|w| w.max_hops).unwrap_or(2),
            });
        }

        // 4. Filters
        if let Some(where_clause) = &query.where_clause {
            if !where_clause.conditions.is_empty() {
                steps.push(PlanStep::Filter);
            }
        }

        // 5. Aggregation
        if !query.group_by.is_empty() {
            steps.push(PlanStep::Aggregate {
                group_by: query.group_by.clone(),
                aggregates: query.aggregates.clone(),
            });
        }

        // 6. Sort
        if !query.order_by.is_empty() {
            steps.push(PlanStep::Sort {
                fields: query.order_by.clone(),
            });
        }

        // 7. Project
        if !query.return_clause.is_empty() {
            steps.push(PlanStep::Project {
                fields: query.return_clause.clone(),
            });
        }

        // 8. Top-K merge
        steps.push(PlanStep::TopKMerge { k: query.limit });

        let estimated_cost = optimizer.estimate_plan_cost(&LogicalPlan {
            steps: steps.clone(),
            estimated_rows: 0,
            estimated_cost: 0.0,
        });

        LogicalPlan {
            steps,
            estimated_rows: query.limit as u64,
            estimated_cost,
        }
    }
}
```

---

## 📋 ПРИОРИТЕТ P1: Query Hints

### Задача 3.1: Добавить синтаксис hints

**Файл**: `src/tql/ast.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHints {
    pub force_gpu: bool,
    pub scatter_shards: Option<u32>,
    pub prefer_local_shard: bool,
    pub backend: Option<BackendHint>,
    pub prefetch_hops: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackendHint {
    Auto,
    Gpu,
    Avx512,
    Avx2,
    Scalar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub match_clause: MatchClause,
    pub where_clause: Option<WhereClause>,
    pub return_clause: Vec<ReturnItem>,
    pub order_by: Vec<OrderBy>,
    pub group_by: Vec<String>,
    pub limit: u32,
    pub offset: Option<u32>,
    pub hints: Option<QueryHints>,
    pub query_hash: u64,
}
```

### Задача 3.2: Парсинг hints

**Файл**: `src/tql/parser.rs`

```rust
fn parse_hints(&mut self) -> Result<Option<QueryHints>, String> {
    if !self.consume_keyword("USING") && !self.consume_keyword("HINT") {
        return Ok(None);
    }

    let mut hints = QueryHints {
        force_gpu: false,
        scatter_shards: None,
        prefer_local_shard: false,
        backend: None,
        prefetch_hops: None,
    };

    loop {
        if self.consume_keyword("GPU") {
            hints.force_gpu = true;
        } else if self.consume_keyword("SCATTER") {
            let shards = self.parse_number::<u32>()?;
            hints.scatter_shards = Some(shards);
        } else if self.consume_keyword("LOCAL_SHARD") {
            hints.prefer_local_shard = true;
        } else if self.consume_keyword("AVX512") {
            hints.backend = Some(BackendHint::Avx512);
        } else if self.consume_keyword("AVX2") {
            hints.backend = Some(BackendHint::Avx2);
        } else if self.consume_keyword("SCALAR") {
            hints.backend = Some(BackendHint::Scalar);
        } else {
            break;
        }

        if !self.consume(",") {
            break;
        }
    }

    Ok(Some(hints))
}
```

### Задача 3.3: Использовать hints в executor

```rust
impl<S: StorageBackend> QueryExecutor<S> {
    fn choose_backend(&self, query: &Query, vector_dim: usize) -> BackendHint {
        if let Some(hints) = &query.hints {
            if hints.force_gpu {
                return BackendHint::Gpu;
            }
            if let Some(backend) = hints.backend {
                return backend;
            }
        }

        // Автоматический выбор
        if vector_dim >= 1024 {
            BackendHint::Gpu
        } else if std::is_x86_feature_detected!("avx512f") {
            BackendHint::Avx512
        } else if std::is_x86_feature_detected!("avx2") {
            BackendHint::Avx2
        } else {
            BackendHint::Scalar
        }
    }
}
```

---

## 📋 ПРИОРИТET P2: Expression Evaluator

### Задача 4.1: AST для выражений

**Файл**: `src/tql/expressions.rs`

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    Literal(PropertyValue),
    FieldAccess { field: String },
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },
    ToroidalDistance {
        left: Box<Expression>,
        right: Box<Expression>,
        phi: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BinaryOperator {
    Eq, Neq, Lt, Lte, Gt, Gte,
    And, Or,
    Add, Sub, Mul, Div,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PropertyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Vector(Vec<f32>),
    Array(Vec<PropertyValue>),
}
```

### Задача 4.2: Evaluator

```rust
pub struct ExpressionEvaluator {
    pub globals: HashMap<String, PropertyValue>,
}

impl ExpressionEvaluator {
    pub fn new() -> Self {
        Self {
            globals: HashMap::new(),
        }
    }

    pub fn eval(&self, expr: &Expression, row: &HashMap<String, PropertyValue>) -> Result<PropertyValue, String> {
        match expr {
            Expression::Literal(val) => Ok(val.clone()),
            Expression::FieldAccess { field } => {
                row.get(field)
                    .cloned()
                    .ok_or_else(|| format!("Field {} not found", field))
            }
            Expression::BinaryOp { left, op, right } => {
                let left_val = self.eval(left, row)?;
                let right_val = self.eval(right, row)?;
                self.eval_binary_op(&left_val, op, &right_val)
            }
            Expression::FunctionCall { name, args } => {
                let args_val: Result<Vec<PropertyValue>, _> = args
                    .iter()
                    .map(|arg| self.eval(arg, row))
                    .collect();
                self.eval_function(name, &args_val?)
            }
            Expression::ToroidalDistance { left, right, phi } => {
                let left_vec = self.eval_vector(left, row)?;
                let right_vec = self.eval_vector(right, row)?;
                let distance = toroidal_distance(&left_vec, &right_vec, *phi);
                Ok(PropertyValue::Float(distance))
            }
        }
    }

    fn eval_binary_op(&self, left: &PropertyValue, op: &BinaryOperator, right: &PropertyValue) -> Result<PropertyValue, String> {
        match (left, right) {
            (PropertyValue::Float(l), PropertyValue::Float(r)) => {
                Ok(PropertyValue::Bool(match op {
                    BinaryOperator::Eq => (l - r).abs() < 1e-6,
                    BinaryOperator::Neq => (l - r).abs() >= 1e-6,
                    BinaryOperator::Lt => l < r,
                    BinaryOperator::Lte => l <= r,
                    BinaryOperator::Gt => l > r,
                    BinaryOperator::Gte => l >= r,
                    BinaryOperator::Add => PropertyValue::Float(l + r),
                    BinaryOperator::Sub => PropertyValue::Float(l - r),
                    BinaryOperator::Mul => PropertyValue::Float(l * r),
                    BinaryOperator::Div => PropertyValue::Float(l / r),
                    _ => return Err("Invalid operator for floats".into()),
                }))
            }
            (PropertyValue::Int(l), PropertyValue::Int(r)) => {
                Ok(PropertyValue::Int(match op {
                    BinaryOperator::Add => l + r,
                    BinaryOperator::Sub => l - r,
                    BinaryOperator::Mul => l * r,
                    BinaryOperator::Div => l / r,
                    _ => return Err("Invalid operator for ints".into()),
                }))
            }
            (PropertyValue::Bool(l), PropertyValue::Bool(r)) => {
                Ok(PropertyValue::Bool(match op {
                    BinaryOperator::Eq => l == r,
                    BinaryOperator::Neq => l != r,
                    BinaryOperator::And => *l && *r,
                    BinaryOperator::Or => *l || *r,
                    _ => return Err("Invalid operator for bools".into()),
                }))
            }
            _ => Err("Type mismatch".into()),
        }
    }

    fn eval_function(&self, name: &str, args: &[PropertyValue]) -> Result<PropertyValue, String> {
        match name {
            "TOROIDALCOSINE" | "TOROIDAL_DISTANCE" => {
                if args.len() < 2 {
                    return Err(format!("{} expects at least 2 arguments", name));
                }
                let left_vec = self.extract_vector(&args[0])?;
                let right_vec = self.extract_vector(&args[1])?;
                let phi = args.get(2).and_then(|v| v.as_float()).unwrap_or(5.71);
                
                let distance = toroidal_distance(&left_vec, &right_vec, phi);
                let cosine = 1.0 - distance.clamp(0.0, 1.0);
                
                Ok(PropertyValue::Float(cosine))
            }
            "CONTAINS" => {
                if args.len() != 2 {
                    return Err("CONTAINS expects 2 arguments".into());
                }
                let haystack = self.extract_text(&args[0])?;
                let needle = self.extract_text(&args[1])?;
                Ok(PropertyValue::Bool(haystack.contains(&needle)))
            }
            "ARRAY_CONTAINS" => {
                if args.len() != 2 {
                    return Err("ARRAY_CONTAINS expects 2 arguments".into());
                }
                let arr = self.extract_array(&args[0])?;
                let needle = self.extract_text(&args[1])?;
                Ok(PropertyValue::Bool(arr.iter().any(|v| self.extract_text(v).ok().as_ref() == Some(&needle))))
            }
            "NOW" => {
                Ok(PropertyValue::Float(chrono::Utc::now().timestamp() as f64))
            }
            _ => Err(format!("Unknown function: {}", name)),
        }
    }

    fn eval_vector(&self, expr: &Expression, row: &HashMap<String, PropertyValue>) -> Result<Vec<f32>, String> {
        match self.eval(expr, row)? {
            PropertyValue::Vector(vec) => Ok(vec),
            _ => Err("Expected vector".into()),
        }
    }

    fn extract_vector(&self, val: &PropertyValue) -> Result<Vec<f32>, String> {
        match val {
            PropertyValue::Vector(vec) => Ok(vec.clone()),
            _ => Err("Expected vector".into()),
        }
    }

    fn extract_text(&self, val: &PropertyValue) -> Result<String, String> {
        match val {
            PropertyValue::Text(s) => Ok(s.clone()),
            PropertyValue::Int(i) => Ok(i.to_string()),
            PropertyValue::Float(f) => Ok(f.to_string()),
            _ => Err("Cannot convert to text".into()),
        }
    }

    fn extract_array(&self, val: &PropertyValue) -> Result<Vec<PropertyValue>, String> {
        match val {
            PropertyValue::Array(arr) => Ok(arr.clone()),
            _ => Err("Expected array".into()),
        }
    }
}

impl PropertyValue {
    pub fn as_float(&self) -> Option<f64> {
        match self {
            PropertyValue::Float(f) => Some(*f),
            PropertyValue::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
}
```

---

## 📋 ПРИОРИТET P2: Basic Subscriptions

### Задача 5.1: Subscription Manager

**Файл**: `src/tql/subscription.rs`

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use uuid::Uuid;
use crate::tql::{Query, QueryResult};

#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: String,
    pub name: Option<String>,
    pub query: Query,
    pub tx: broadcast::Sender<SubscriptionEvent>,
}

#[derive(Debug, Clone)]
pub enum SubscriptionEvent {
    Insert { result: QueryResult },
    Update { old: QueryResult, new: QueryResult },
    Delete { result: QueryResult },
}

pub struct SubscriptionManager<S: StorageBackend> {
    subs: RwLock<HashMap<String, Subscription>>,
    executor: Arc<dyn QueryExecutorTrait<S>>,
}

#[async_trait::async_trait]
pub trait QueryExecutorTrait<S: StorageBackend> {
    async fn execute_query(&self, query: &Query) -> Result<Vec<QueryResult>, String>;
}

impl<S: StorageBackend> SubscriptionManager<S> {
    pub fn new(executor: Arc<dyn QueryExecutorTrait<S>>) -> Self {
        Self {
            subs: RwLock::new(HashMap::new()),
            executor,
        }
    }

    pub async fn subscribe(&self, name: Option<String>, query: Query) -> Result<String, String> {
        let id = Uuid::new_v4().to_string();
        let (tx, _rx) = broadcast::channel(100);

        let sub = Subscription {
            id: id.clone(),
            name,
            query,
            tx,
        };

        self.subs.write().unwrap().insert(id.clone(), sub);
        Ok(id)
    }

    pub fn unsubscribe(&self, id: &str) -> Result<(), String> {
        self.subs.write().unwrap().remove(id).ok_or("Subscription not found".to_string())?;
        Ok(())
    }

    pub fn list_subscriptions(&self) -> Vec<(String, Option<String>)> {
        self.subs.read().unwrap()
            .iter()
            .map(|(id, sub)| (id.clone(), sub.name.clone()))
            .collect()
    }

    pub async fn on_change(&self, event: &ChangeEvent) {
        let subs = self.subs.read().unwrap();
        
        for (_, sub) in subs.iter() {
            if self.is_relevant(&sub.query, event) {
                let results = self.executor.execute_query(&sub.query).await.unwrap_or_default();
                
                // Emit changes
                for result in results {
                    let _ = sub.tx.send(SubscriptionEvent::Insert { result });
                }
            }
        }
    }

    fn is_relevant(&self, query: &Query, event: &ChangeEvent) -> bool {
        // Простая проверка релевантности по типу узла
        match event {
            ChangeEvent::NodeInserted { node_type, .. } => {
                query.match_clause.source.label.as_deref() == Some(node_type)
            }
            ChangeEvent::EdgeInserted { edge_type, .. } => {
                query.match_clause.relationship.as_ref()
                    .map(|r| r.type_.as_str() == edge_type)
                    .unwrap_or(false)
            }
            _ => true,
        }
    }

    pub fn subscribe_to_changes(&self, id: &str) -> Result<broadcast::Receiver<SubscriptionEvent>, String> {
        self.subs.read().unwrap()
            .get(id)
            .ok_or("Subscription not found".to_string())
            .map(|sub| sub.tx.subscribe())
    }
}

#[derive(Debug, Clone)]
pub enum ChangeEvent {
    NodeInserted { node_id: u64, node_type: String },
    NodeUpdated { node_id: u64, node_type: String },
    NodeDeleted { node_id: u64, node_type: String },
    EdgeInserted { from: u64, to: u64, edge_type: String },
    EdgeDeleted { from: u64, to: u64, edge_type: String },
}
```

### Задача 5.2: Интеграция в TqlEngine

```rust
pub struct TqlEngine<S: StorageBackend> {
    schema_registry: Arc<SchemaRegistry>,
    executor: Arc<QueryExecutor<S>>,
    optimizer: Arc<QueryOptimizer>,
    subscription_mgr: Arc<SubscriptionManager<S>>,
}

impl<S: StorageBackend> TqlEngine<S> {
    pub async fn execute(&self, sql: &str) -> Result<TqlResult, TqlError> {
        let statement = self.parser.parse(sql)?;
        
        match statement {
            Statement::Query(query) => {
                let plan = PlanBuilder::build(&query, self.optimizer.clone());
                let results = self.executor.execute_query(&query, None).await?;
                Ok(TqlResult::Query(results))
            }
            Statement::Subscribe { name, query } => {
                let id = self.subscription_mgr.subscribe(name, query).await?;
                Ok(TqlResult::SubscribeSuccess(id))
            }
            Statement::Unsubscribe { id } => {
                self.subscription_mgr.unsubscribe(&id)?;
                Ok(TqlResult::UnsubscribeSuccess)
            }
            Statement::Explain(query) => {
                let plan = PlanBuilder::build(&query, self.optimizer.clone());
                let explanation = self.format_explain(&plan);
                Ok(TqlResult::Explain(explanation))
            }
            Statement::Ddl(stmt) => self.execute_ddl(stmt).await,
        }
    }
}
```

---

## 📋 Roadmap реализации

| Фаза | Компоненты | Сроки | Зависимости |
|------|------------|-------|-------------|
| P0 | QueryExecutor | 2-3 недели | StorageBackend API |
| P1.1 | Cost-based Optimizer | 2 недели | QueryExecutor |
| P1.2 | Query Hints | 1 неделя | AST, Parser |
| P2.1 | Expression Evaluator | 3 недели | - |
| P2.2 | Basic Subscriptions | 3 недели | QueryExecutor |
| P3 | ML Pipeline | 6+ недель | External deps |

---

## 📁 Файлы для создания/изменения

**Создать:**
- `src/tql/executor.rs`
- `src/tql/optimizer.rs`
- `src/tql/expressions.rs`
- `src/tql/subscription.rs`

**Изменить:**
- `src/tql/ast.rs` - добавить QueryHints
- `src/tql/parser.rs` - добавить парсинг hints
- `src/tql/engine.rs` - интегрировать subscription manager
- `src/tql/mod.rs` - экспорты новых модулей
