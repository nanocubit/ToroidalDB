use crate::hybrid_storage::HybridPersistentStore;
use crate::math::MatryoshkaDim;
use crate::tql::ast::{
    AggregationField, AggregationFunction, OrderByClause, Query, WhereCondition,
};
use crate::tql::cost_optimizer::CostBasedOptimizer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub id: u64,
    pub score: f32,
    pub properties: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AggregatedResult {
    pub count: Option<u64>,
    pub sum: Option<f64>,
    pub avg: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub properties: serde_json::Value,
}

pub struct QueryExecutor;

impl QueryExecutor {
    pub async fn execute_query(
        store: &HybridPersistentStore,
        query: Query,
    ) -> Result<Vec<QueryResult>, String> {
        // Оптимизируем запрос через CostBasedOptimizer
        let optimizer = CostBasedOptimizer::new();
        let optimized_query = optimizer.optimize_query(&query);

        // Проверяем, есть ли транзакция в запросе
        if let Some(transaction) = &optimized_query.transaction {
            // Выполняем транзакцию
            match Self::execute_transaction(store, transaction.clone()).await {
                Ok(()) => {
                    // Возвращаем пустой результат для транзакции
                    return Ok(vec![]);
                }
                Err(e) => return Err(format!("Transaction execution failed: {e}")),
            }
        }

        // Проверяем, есть ли агрегации в запросе
        if !optimized_query.aggregation_fields.is_empty() {
            // Выполняем агрегации отдельно, чтобы избежать рекурсии
            return Self::execute_aggregations_only(store, optimized_query).await;
        }

        // Проверяем, есть ли специфичные для тороидальной топологии клаузы
        if let (Some(_match_clause), Some(connected_clause), Some(within_clause)) = (
            &optimized_query.match_clause,
            &optimized_query.connected_clause,
            &optimized_query.within_clause,
        ) {
            // Клонируем необходимые поля чтобы избежать move
            let connected_clause = connected_clause.clone();
            let within_clause = within_clause.clone();
            let query_clone = optimized_query.clone();
            // Выполняем специфичный для тороидальной топологии запрос
            return Self::execute_connected_query(
                store,
                query_clone,
                &connected_clause,
                &within_clause,
            )
            .await;
        }

        // Extract query vector from WHERE clause if present
        let (search_vector, threshold) = Self::extract_query_vector(&optimized_query)?;

        // Вызываем оптимизированный matryoshka_search
        let search_results = store
            .matryoshka_search(&search_vector, MatryoshkaDim::D384, threshold)
            .map_err(|e| format!("Search error: {e}"))?;

        let mut query_results = Vec::new();
        for (node_id, distance) in &search_results {
            if let Ok(Some(node)) = store.get(*node_id) {
                query_results.push(QueryResult {
                    id: node.id,
                    score: *distance,
                    properties: node.properties.clone(),
                });
            }
        }

        // Apply ORDER BY if present
        if let Some(ref order_by) = optimized_query.order_by {
            query_results.sort_by(|a, b| {
                if order_by.ascending {
                    Self::compare_by_field(a, b, order_by)
                } else {
                    Self::compare_by_field(a, b, order_by).reverse()
                }
            });
        } else {
            // Default sort by score (ascending - lower distance is better)
            query_results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());
        }

        // Apply LIMIT
        query_results.truncate(optimized_query.limit as usize);

        Ok(query_results)
    }

    // Extract query vector and threshold from WHERE clause
    fn extract_query_vector(query: &Query) -> Result<(Vec<f32>, f32), String> {
        // 1. Use query_vector from Query struct if provided (runtime)
        if let Some(ref qv) = query.query_vector {
            let threshold = match &query.where_clause {
                Some(WhereCondition::ToroidalDistance { threshold, .. }) => *threshold,
                _ => 0.3,
            };
            return Ok((qv.clone(), threshold));
        }

        // 2. Fallback: check WHERE clause for threshold only
        if let Some(WhereCondition::ToroidalDistance {
            field: _,
            threshold: _,
        }) = &query.where_clause
        {
            // No query vector provided — use properties from a random stored node as best-effort
            // This is a degraded mode; the proper way is to pass query_vector via API
            return Err(
                "TOROIDALDISTANCE requires a query vector. Pass it via the query_vector field or API parameter.".to_string()
            );
        }

        // No WHERE clause with vector search - use default
        Err("No TOROIDALDISTANCE condition in query".to_string())
    }

    // Выполнение агрегаций
    fn perform_aggregations(
        results: &[QueryResult],
        aggregations: &[AggregationField],
    ) -> Vec<AggregatedResult> {
        let mut aggregated_results = Vec::new();

        for agg_field in aggregations {
            let mut agg_result = AggregatedResult {
                count: None,
                sum: None,
                avg: None,
                min: None,
                max: None,
                properties: serde_json::Value::Null,
            };

            match &agg_field.function {
                AggregationFunction::Count => {
                    agg_result.count = Some(results.len() as u64);
                }
                AggregationFunction::Sum(field_name) => {
                    let sum = results
                        .iter()
                        .filter_map(|result| {
                            result
                                .properties
                                .get(field_name)
                                .and_then(serde_json::Value::as_f64)
                        })
                        .sum();
                    agg_result.sum = Some(sum);
                }
                AggregationFunction::Avg(field_name) => {
                    let values: Vec<f64> = results
                        .iter()
                        .filter_map(|result| {
                            result
                                .properties
                                .get(field_name)
                                .and_then(serde_json::Value::as_f64)
                        })
                        .collect();
                    if !values.is_empty() {
                        let avg = values.iter().sum::<f64>() / values.len() as f64;
                        agg_result.avg = Some(avg);
                    }
                }
                AggregationFunction::Min(field_name) => {
                    let min = results
                        .iter()
                        .filter_map(|result| {
                            result
                                .properties
                                .get(field_name)
                                .and_then(serde_json::Value::as_f64)
                        })
                        .fold(f64::INFINITY, f64::min);
                    if min != f64::INFINITY {
                        agg_result.min = Some(min);
                    }
                }
                AggregationFunction::Max(field_name) => {
                    let max = results
                        .iter()
                        .filter_map(|result| {
                            result
                                .properties
                                .get(field_name)
                                .and_then(serde_json::Value::as_f64)
                        })
                        .fold(f64::NEG_INFINITY, f64::max);
                    if max != f64::NEG_INFINITY {
                        agg_result.max = Some(max);
                    }
                }
            }

            aggregated_results.push(agg_result);
        }

        aggregated_results
    }

    // Расширенная версия для гибридного поиска (вектор + граф)
    pub async fn execute_hybrid_query(
        store: &HybridPersistentStore,
        query: Query,
    ) -> Result<Vec<QueryResult>, String> {
        // Enable caching using the public hybrid_query_cache
        let cache_key = Self::hash_hybrid_query(&query);
        if let Some(cached_result) = store.hybrid_query_cache.get(cache_key) {
            let results: Vec<QueryResult> = cached_result
                .iter()
                .map(|(id, score)| QueryResult {
                    id: *id,
                    score: *score,
                    properties: serde_json::Value::Null,
                })
                .collect();
            return Ok(results);
        }

        // Шаг 1: Получаем начальные узлы из графового обхода
        let start_nodes = Self::get_start_nodes_for_hybrid_search(store, &query).await?;

        // Шаг 2: Выполняем гибридный поиск, объединяя векторный и графовый подходы
        let mut all_results = Vec::new();

        // Для каждого стартового узла выполняем:
        // 1. Графовый обход (поиск соседей)
        // 2. Векторный поиск (поиск похожих узлов)
        for start_node_id in start_nodes {
            if let Ok(Some(start_node)) = store.get(start_node_id) {
                // Получаем соседей стартового узла (графовый компонент)
                let neighbors = store
                    .get_neighbors(start_node_id)
                    .unwrap_or_else(|_| Vec::new());

                // Выполняем векторный поиск от вектора стартового узла
                let vector_search_results = store
                    .matryoshka_search(&start_node.vector, MatryoshkaDim::D384, 0.3)
                    .unwrap_or_else(|_| Vec::new());

                // Объединяем результаты: учитываем как графовую близость (соседи),
                // так и векторную близость (похожие векторы)
                for neighbor in &neighbors {
                    // Повышаем релевантность соседних узлов
                    if let Some((_, vector_distance)) = vector_search_results
                        .iter()
                        .find(|(id, _)| *id == neighbor.id)
                    {
                        // Если узел является и соседом, и векторно близким, увеличиваем его релевантность
                        all_results.push(QueryResult {
                            id: neighbor.id,
                            score: *vector_distance * 0.7, // Повышаем релевантность за графовую близость
                            properties: neighbor.properties.clone(),
                        });
                    } else {
                        // Если узел только сосед, но не векторно близкий
                        all_results.push(QueryResult {
                            id: neighbor.id,
                            score: 0.5, // Средняя релевантность за графовую близость
                            properties: neighbor.properties.clone(),
                        });
                    }
                }

                // Добавляем результаты векторного поиска (но не дублируем соседей)
                let neighbor_ids: std::collections::HashSet<u64> =
                    neighbors.iter().map(|n| n.id).collect();

                for (node_id, distance) in &vector_search_results {
                    if !neighbor_ids.contains(node_id) {
                        // Узел найден только векторным поиском
                        if let Ok(Some(node)) = store.get(*node_id) {
                            all_results.push(QueryResult {
                                id: node.id,
                                score: *distance,
                                properties: node.properties,
                            });
                        }
                    }
                }
            }
        }

        // Убираем дубликаты результатов
        let mut unique_results = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for result in all_results {
            if !seen_ids.contains(&result.id) {
                seen_ids.insert(result.id);
                unique_results.push(result);
            }
        }

        // Сортировка по оценке (чем меньше score, тем выше релевантность)
        unique_results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());

        // Ограничиваем количество результатов
        let final_results: Vec<QueryResult> = unique_results
            .into_iter()
            .take(query.limit as usize)
            .collect();

        // Cache the results
        let cache_values: Vec<(u64, f32)> = final_results
            .iter()
            .map(|result| (result.id, result.score))
            .collect();
        store.hybrid_query_cache.put(cache_key, cache_values);

        Ok(final_results)
    }

    // Получение стартовых узлов для гибридного поиска
    async fn get_start_nodes_for_hybrid_search(
        store: &HybridPersistentStore,
        query: &Query,
    ) -> Result<Vec<u64>, String> {
        // Если в запросе есть графовая часть, используем её для получения стартовых узлов
        if let Some(ref match_clause) = query.match_clause {
            // В текущей реализации возвращаем все узлы или узлы с определённой меткой
            // В будущем можно будет реализовать более сложную логику сопоставления по метке

            let all_nodes = store
                .get_all()
                .map_err(|e| format!("Failed to get all nodes: {e}"))?;

            // Фильтруем узлы по метке, если она указана в match_clause
            let start_nodes: Vec<u64> = all_nodes
                .iter()
                .filter(|_node| {
                    // В реальной реализации здесь будет проверка метки узла
                    // Сейчас просто возвращаем все узлы
                    true
                })
                .map(|node| node.id)
                .collect();

            Ok(start_nodes)
        } else {
            // Если нет графовой части, возвращаем все узлы
            let all_nodes = store
                .get_all()
                .map_err(|e| format!("Failed to get all nodes: {e}"))?;

            let start_nodes: Vec<u64> = all_nodes.iter().map(|node| node.id).collect();

            Ok(start_nodes)
        }
    }

    // Хеширование гибридного запроса для кэширования
    fn hash_hybrid_query(query: &Query) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Хешируем основные компоненты запроса
        if let Some(ref match_clause) = query.match_clause {
            format!("{match_clause:?}").hash(&mut hasher);
        }

        if let Some(ref where_clause) = query.where_clause {
            format!("{where_clause:?}").hash(&mut hasher);
        }

        query.limit.hash(&mut hasher);
        query.distributed.hash(&mut hasher);

        hasher.finish()
    }

    // Выполнение запроса с CONNECTEDTO и WITHIN HOPS
    pub async fn execute_connected_query(
        store: &HybridPersistentStore,
        query: Query,
        connected_clause: &crate::tql::ast::ConnectedClause,
        within_clause: &crate::tql::ast::WithinClause,
    ) -> Result<Vec<QueryResult>, String> {
        // Получаем стартовые узлы из match_clause
        let start_nodes = Self::get_start_nodes(store, &query).await?;

        // Выполняем bidirectional BFS для поиска соединенных узлов
        let connected_results = crate::tql::GraphOperations::find_connected_nodes(
            store,
            &start_nodes,
            connected_clause,
            within_clause,
        )
        .await?;

        Ok(connected_results)
    }

    // Получение стартовых узлов из match_clause
    async fn get_start_nodes(
        store: &HybridPersistentStore,
        _query: &Query,
    ) -> Result<Vec<u64>, String> {
        // В упрощённой реализации возвращаем все узлы или узлы с определённой меткой
        // В реальной реализации нужно будет анализировать match_clause

        let all_nodes = store
            .get_all()
            .map_err(|e| format!("Failed to get all nodes: {e}"))?;

        let start_nodes: Vec<u64> = all_nodes.iter().map(|node| node.id).collect();

        Ok(start_nodes)
    }

    // Сравнение результатов по полю для сортировки
    fn compare_by_field(
        a: &QueryResult,
        b: &QueryResult,
        order_by: &OrderByClause,
    ) -> std::cmp::Ordering {
        let field_name = &order_by.field;

        // Получаем значения полей для сравнения
        let val_a = a
            .properties
            .get(field_name)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let val_b = b
            .properties
            .get(field_name)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);

        if order_by.ascending {
            val_a
                .partial_cmp(&val_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            val_b
                .partial_cmp(&val_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    }

    // Выполнение транзакции
    pub async fn execute_transaction(
        store: &HybridPersistentStore,
        transaction: crate::tql::ast::Transaction,
    ) -> Result<(), String> {
        crate::tql::TransactionManager::execute_transaction(store, transaction).await
    }

    // Выполнение только агрегаций без рекурсии
    async fn execute_aggregations_only(
        store: &HybridPersistentStore,
        query: Query,
    ) -> Result<Vec<QueryResult>, String> {
        // Extract query vector from WHERE clause if present
        let (search_vector, threshold) = Self::extract_query_vector(&query)?;

        // Вызываем оптимизированный matryoshka_search
        let search_results = store
            .matryoshka_search(&search_vector, crate::math::MatryoshkaDim::D384, threshold)
            .map_err(|e| format!("Search error: {e}"))?;

        let mut query_results = Vec::new();
        for (node_id, distance) in &search_results {
            if let Ok(Some(node)) = store.get(*node_id) {
                query_results.push(QueryResult {
                    id: node.id,
                    score: *distance,
                    properties: node.properties.clone(),
                });
            }
        }

        // Выполняем агрегации
        let aggregated_values =
            Self::perform_aggregations(&query_results, &query.aggregation_fields);

        // Преобразуем агрегированные значения в QueryResult
        let mut result = Vec::new();
        for agg_result in aggregated_values {
            result.push(QueryResult {
                id: 0,      // Для агрегаций id не применим
                score: 0.0, // Для агрегаций score не применим
                properties: serde_json::to_value(agg_result).map_err(|e| e.to_string())?,
            });
        }

        Ok(result)
    }
}
