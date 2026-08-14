use super::{Query, AggregationFunction};
use serde_json::{json, Value};

// Типы для запросов и результатов
#[derive(Debug, serde::Deserialize)]
pub struct TqlRequest {
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct TqlResponse {
    pub success: bool,
    pub message: String,
    pub results: Vec<TqlResult>,
    pub execution_time_ms: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct TqlResult {
    pub id: u64,
    pub fields: Vec<String>,
    pub values: Vec<serde_json::Value>,
    pub properties: serde_json::Value,
}

// TQL обработчик
pub struct TqlHandler;

impl TqlHandler {
    pub fn new() -> Self {
        Self {}
    }
    
    // Исполнение TQL запроса
    pub async fn execute_tql(
        state: &super::ServerState,
        Json(request): TqlRequest,
    ) -> super::JsonResponse {
        let start_time = std::time::Instant::now();
        
        let parse_result = super::parser::parse_query(&request.query);
        
        let results = match parse_result {
            Ok(query) => {
                let execution_result = super::tql::executor::QueryExecutor::execute_query(&state.store, query).await;
                
                let end_time = start_time.elapsed().as_millis();
                
                let results_with_metadata = execution_result.results
                    .into_iter()
                    .map(|result| {
                        TqlResult {
                            id: result.id,
                            fields: result.return_fields.iter()
                                .map(|field| field.clone())
                                .collect(),
                            values: result.return_values.iter()
                                .map(|value| value.clone())
                                .collect(),
                            properties: result.return_properties.clone(),
                        }
                    })
                    .collect();
                
                let execution_time_ms = end_time;
                
                TqlResponse {
                    success: true,
                    message: "Запрос выполнен успешно".to_string(),
                    results: results_with_metadata,
                    execution_time_ms,
                }
            }
            Err(err) => {
                TqlResponse {
                    success: false,
                    message: format!("Ошибка парсинга: {}", err),
                    results: Vec::new(),
                    execution_time_ms: 0,
                }
            }
        };
        
        response
    }
    
    // Валидация TQL запроса
    pub fn validate_tql(query: &str) -> Result<(), String> {
        match super::parser::parse_query(query) {
            Ok(_) => Ok(()),
            Err(err) => Err(format!("Ошибка в TQL запросе: {}", err)),
        }
    }
}

// Агрегационные функции TQL
pub fn apply_aggregation(
    results: &mut Vec<TqlResult>,
    aggregation: &AggregationFunction,
    limit: Option<usize>,
) {
    let mut grouped_results: std::collections::HashMap<String, Vec<&mut TqlResult>> = std::collections::HashMap::new();
        
        for result in results.iter_mut() {
            let key = aggregation.name.clone();
            grouped_results
                .entry(key).or_insert(result);
        }
        
        for (aggregation_name, grouped_results_list) in grouped_results {
            if let Some(results_list) = grouped_results.remove(&aggregation_name) {
                let sorted_results = results_list.iter()
                    .sorted_by(|a, b| a.field.distance.partial_cmp(&b.field.distance, f32::cmp))
                        .take(limit.unwrap_or(results_list.len()));
                
                results_list.truncate(sorted_results.len());
                
                let result_value = match aggregation.function {
                    AggregationFunction::Count => json!(sorted_results.len()),
                    AggregationFunction::Sum => json!(sorted_results.iter().map(|r| r.values.iter().sum()).sum()),
                    AggregationFunction::Avg => json!(sorted_results.iter().map(|r| r.values.iter().sum() as f64 / sorted_results.len() as f64)),
                    AggregationFunction::Min => json!(sorted_results.iter().map(|r| r.values.iter().fold(f64::MAX, | |r| 0).min()),
                    AggregationFunction::Max => json!(sorted_results.iter().map(|r| r.values.iter().fold(f64::MIN, | |r|0.0).max()),
                    _ => json!(sorted_results.iter().map(|r| serde_json::Value::Null)),
                };
                
                for (i, result) in results.iter_mut().enumerate() {
                    if let Some(grouped) = grouped_results.get_mut(&aggregation_name) {
                        let result_value = result_value.as_f64().unwrap_or(0.0);
                        result.values.push(serde_json::Value::Number(result_value));
                        result.aggregation = Some(aggregation_function.clone());
                    }
                }
        }
    }
}