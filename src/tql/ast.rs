use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WindowFunction {
    RowNumber,
    Rank,
    DenseRank,
    Lag(String),
    Lead(String),
    FirstValue(String),
    LastValue(String),
    Sum(String),
    Avg(String),
    Count,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowSpec {
    pub partition_by: Vec<String>,
    pub order_by: Option<OrderByClause>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowExpression {
    pub function: WindowFunction,
    pub window_spec: Option<WindowSpec>,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommonTableExpression {
    pub name: String,
    pub columns: Vec<String>,
    pub query: Box<Query>,
    pub is_recursive: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WithClause {
    pub ctes: Vec<CommonTableExpression>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AggregationFunction {
    Count,
    Sum(String), // field name
    Avg(String), // field name
    Min(String), // field name
    Max(String), // field name
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregationField {
    pub function: AggregationFunction,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderByClause {
    pub field: String,
    pub ascending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubQuery {
    pub query: Box<Query>,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionOperation {
    CreateNode(NodePattern),
    UpdateNode(NodePattern, Vec<(String, PropertyValue)>),
    DeleteNode(NodePattern),
    CreateEdge(String, String, String), // source, target, relationship
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub operations: Vec<TransactionOperation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Query {
    pub with_clause: Option<WithClause>,
    pub match_clause: Option<MatchClause>,
    pub where_clause: Option<WhereCondition>,
    pub connected_clause: Option<ConnectedClause>,
    pub within_clause: Option<WithinClause>,
    pub return_fields: Vec<String>,
    pub aggregation_fields: Vec<AggregationField>,
    pub window_functions: Vec<WindowExpression>,
    pub order_by: Option<OrderByClause>,
    pub subqueries: Vec<SubQuery>,
    pub transaction: Option<Transaction>,
    pub limit: u32,
    pub distributed: bool,
    pub hints: Option<QueryHints>,
    pub query_hash: u64,
    // TQL v3.0: Stream support
    pub from_stream: Option<StreamQuery>,
    pub group_by: Vec<String>,
    pub having: Option<WhereCondition>,
    // TQL v3.0: Graph analytics
    pub analytics: Vec<GraphAnalyticsFunction>,
    /// Query vector for similarity search (passed at runtime, not parsed).
    pub query_vector: Option<Vec<f32>>,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            with_clause: None,
            match_clause: None,
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: Vec::new(),
            aggregation_fields: Vec::new(),
            window_functions: Vec::new(),
            order_by: None,
            subqueries: Vec::new(),
            transaction: None,
            limit: 10,
            distributed: false,
            hints: None,
            query_hash: 0,
            from_stream: None,
            group_by: Vec::new(),
            having: None,
            analytics: Vec::new(),
            query_vector: None,
        }
    }
}

// ==================== TQL v2.2: Query Hints ====================

/// Hints for query optimization and execution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct QueryHints {
    pub force_gpu: bool,
    pub scatter_shards: Option<u32>,  // HINT SCATTER N SHARDS
    pub prefer_local_shard: bool,     // HINT PREFERR LOCAL_SHARD
    pub backend: Option<BackendHint>, // USING GPU / AVX512 / AVX2 / SCALAR
    pub prefetch_hops: Option<u32>,   // HINT PREFETCH GRAPH_HOPS N
}

/// Backend execution hints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BackendHint {
    Auto,
    Gpu,
    Avx512,
    Avx2,
    Scalar,
}

// ==================== TQL v3.0: Complete DDL Types ====================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Query(Query),
    Explain(Query),
    Ddl(DdlStatement),
    Subscribe(Subscription),
    CreateSpace(SpaceDef),
    CreateStream(StreamDef),
    CreatePipeline(PipelineDef),
    CreateView(ViewDef),
    CreatePattern(PatternDef),
    AlterSpace(AlterSpace),
}

// ==================== TQL v3.0: Spaces ====================

/// Пространство данных - контейнер для узлов, рёбер и стримов
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceDef {
    pub name: String,
    pub nodes: Vec<NodeTypeDef>,
    pub edges: Vec<EdgeTypeDef>,
    pub streams: Vec<StreamDef>,
    pub config: SpaceConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceConfig {
    pub shards: u32,
    pub replication_factor: u32,
    pub storage_backend: Option<String>,
    pub embedding_model: Option<String>,
}

/// Изменение пространства
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlterSpace {
    pub name: String,
    pub operations: Vec<AlterOperation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlterOperation {
    AddNode(NodeTypeDef),
    AddEdge(EdgeTypeDef),
    DropNode(String),
    DropEdge(String),
    AddField {
        node_type: String,
        field: FieldDef,
    },
    DropField {
        node_type: String,
        field_name: String,
    },
}

// ==================== TQL v3.0: Streams ====================

/// Определение стрима событий
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamDef {
    pub name: String,
    pub topic: String,
    pub schema: StreamSchema,
    pub retention: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamSchema {
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Duration {
    pub value: u64,
    pub unit: String, // "s", "m", "h", "d"
}

// ==================== TQL v3.0: Pipelines ====================

/// Конвейер обработки данных
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineDef {
    pub name: String,
    pub from_stream: String,
    pub transform: TransformDef,
    pub into: PipelineOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformDef {
    pub model: Option<String>,
    pub function: Option<String>,
    pub batch_size: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PipelineOutput {
    Update { node_type: String, field: String },
    Insert { node_type: String },
    Stream { stream_name: String },
}

// ==================== TQL v3.0: Views & Patterns ====================

/// Представление - материализованный запрос
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewDef {
    pub name: String,
    pub query: Box<Query>,
    pub materialized: bool,
    pub refresh_policy: Option<RefreshPolicy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RefreshPolicy {
    Immediate,
    Periodic(Duration),
    OnDemand,
}

/// Паттерн - переиспользуемый запрос
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternDef {
    pub name: String,
    pub query: Box<Query>,
    pub parameters: Vec<String>,
}

// ==================== TQL v3.0: Subscriptions ====================

/// Подписка на изменения
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Option<String>,
    pub query: Box<Query>,
    pub emit: EmitClause,
    pub where_condition: Option<WhereCondition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EmitClause {
    Changes,
    Events,
    WebSocket { endpoint: String },
    Webhook { url: String },
    Grpc { service: String },
}

// ==================== TQL v3.0: Stream Query ====================

/// Запрос к стриму
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamQuery {
    pub stream_name: String,
    pub window: Option<StreamWindowSpec>,
    pub filter: Option<WhereCondition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamWindowSpec {
    pub duration: Duration,
    pub slide: Option<Duration>,
    pub watermark: Option<String>,
}

// ==================== TQL v3.0: Graph Analytics ====================

/// Графовые аналитические функции
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GraphAnalyticsFunction {
    Centrality {
        node: String,
        algorithm: CentralityAlgorithm,
    },
    PageRank {
        node: String,
        damping: f32,
        iterations: u32,
    },
    CommunityDetection {
        algorithm: CommunityAlgorithm,
    },
    Similarity {
        node1: String,
        node2: String,
        algorithm: String,
    },
    PathFinding {
        from: String,
        to: String,
        algorithm: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CentralityAlgorithm {
    Degree,
    Betweenness,
    Closeness,
    Eigenvector,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommunityAlgorithm {
    Louvain,
    LabelPropagation,
    ConnectedComponents,
}

// ==================== TQL v2.x: Existing Types ====================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DdlStatement {
    CreateNodeType(NodeTypeDef),
    CreateEdgeType(EdgeTypeDef),
    DropNodeType(String),
    DropEdgeType(String),
    ShowSchema,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeTypeDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeTypeDef {
    pub name: String,
    pub from: String,
    pub to: String,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub data_type: DataType,
    pub is_primary_key: bool,
    pub constraints: Vec<FieldConstraint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataType {
    Int,
    Float,
    Bool,
    Text,
    Timestamp,
    Vector(u32), // размерность вектора
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldConstraint {
    NotNull,
    Unique,
    Index,
    VectorIndex { phi: f32 }, // параметр для тороидального индекса
}

// ==================== Core Types ====================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePattern {
    pub alias: String,
    pub label: String,
    pub properties: Option<Vec<(String, PropertyValue)>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PropertyValue {
    String(String),
    Number(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipPattern {
    pub type_: String,
    pub direction: Direction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchClause {
    pub source: NodePattern,
    pub relationship: Option<RelationshipPattern>,
    pub target: Option<NodePattern>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WhereCondition {
    ToroidalDistance {
        field: String,
        threshold: f32,
    },
    SimilarTo {
        field: String,
        threshold: f32,
        // Uses HNSW backend (cosine/euclidean), not toroidal distance
    },
    PropertyFilter {
        property: String,
        operator: String,
        value: PropertyValue,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnClause {
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitClause {
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectedClause {
    pub target_label: String,
    pub relationship_type: String,
    pub property_filter: Option<(String, PropertyValue)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WithinClause {
    pub min_hops: u32,
    pub max_hops: u32,
}
