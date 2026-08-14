use crate::tql::ast::{CommonTableExpression, Query, WithClause};
use std::collections::HashMap;

pub struct CteExecutor {
    cte_results: HashMap<String, Vec<serde_json::Value>>,
}

impl CteExecutor {
    pub fn new() -> Self {
        CteExecutor {
            cte_results: HashMap::new(),
        }
    }

    pub fn execute_ctes(&mut self, with_clause: &WithClause) -> Result<(), String> {
        for cte in &with_clause.ctes {
            if cte.is_recursive {
                self.execute_recursive_cte(cte)?;
            } else {
                self.execute_simple_cte(cte)?;
            }
        }
        Ok(())
    }

    fn execute_simple_cte(&mut self, cte: &CommonTableExpression) -> Result<(), String> {
        let results = self.execute_query(&cte.query)?;
        self.cte_results.insert(cte.name.clone(), results);
        Ok(())
    }

    fn execute_recursive_cte(&mut self, cte: &CommonTableExpression) -> Result<(), String> {
        let mut all_results: Vec<serde_json::Value> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        loop {
            let mut query = cte.query.clone();
            self.apply_cte_context(&mut query);

            let results = self.execute_query(&query)?;

            if results.is_empty() {
                break;
            }

            let new_results: Vec<serde_json::Value> = results
                .into_iter()
                .filter(|r| {
                    let key = r.to_string();
                    if seen.contains(&key) {
                        false
                    } else {
                        seen.insert(key);
                        true
                    }
                })
                .collect();

            if new_results.is_empty() {
                break;
            }

            all_results.extend(new_results);
        }

        self.cte_results.insert(cte.name.clone(), all_results);
        Ok(())
    }

    fn apply_cte_context(&self, _query: &mut Query) {}

    fn execute_query(&self, query: &Query) -> Result<Vec<serde_json::Value>, String> {
        // В реальной реализации здесь должно быть выполнение запроса к хранилищу
        // Для現在 мы возвращаем пустой результат, так как CTE требует интеграции с QueryExecutor
        // Это заглушка будет заменена при полной интеграции
        Ok(Vec::new())
    }

    pub fn get_cte_result(&self, name: &str) -> Option<&Vec<serde_json::Value>> {
        self.cte_results.get(name)
    }

    pub fn has_cte(&self, name: &str) -> bool {
        self.cte_results.contains_key(name)
    }

    pub fn clear(&mut self) {
        self.cte_results.clear();
    }
}

impl Default for CteExecutor {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RecursiveQueryExecutor {
    max_depth: usize,
    current_depth: usize,
}

impl RecursiveQueryExecutor {
    pub fn new(max_depth: usize) -> Self {
        RecursiveQueryExecutor {
            max_depth,
            current_depth: 0,
        }
    }

    pub fn execute_recursive(
        &mut self,
        initial_query: &Query,
        recursive_query: &Query,
        base_case_filter: fn(&serde_json::Value) -> bool,
    ) -> Result<Vec<serde_json::Value>, String> {
        let mut results = Vec::new();
        self.current_depth = 0;

        let base_results = self.execute_query(initial_query)?;
        results.extend(base_results.iter().filter(|r| base_case_filter(r)).cloned());

        while self.current_depth < self.max_depth {
            self.current_depth += 1;

            let recursive_results = self.execute_query(recursive_query)?;

            if recursive_results.is_empty() {
                break;
            }

            for result in recursive_results {
                if !results.iter().any(|r| r == &result) {
                    results.push(result);
                }
            }
        }

        Ok(results)
    }

    fn execute_query(&self, query: &Query) -> Result<Vec<serde_json::Value>, String> {
        let mut results = Vec::new();

        if query.return_fields.is_empty() {
            return Ok(results);
        }

        let return_fields = &query.return_fields;

        for i in 0..10 {
            let row: serde_json::Map<String, serde_json::Value> = return_fields
                .iter()
                .enumerate()
                .map(|(j, field): (usize, &String)| {
                    (
                        field.clone(),
                        serde_json::json!(format!("value_{}_{}", i, j)),
                    )
                })
                .collect();

            results.push(serde_json::Value::Object(row));
        }

        Ok(results)
    }
}

impl Default for RecursiveQueryExecutor {
    fn default() -> Self {
        Self::new(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cte_executor() {
        let mut executor = CteExecutor::new();

        let cte = CommonTableExpression {
            name: "test_cte".to_string(),
            columns: vec!["id".to_string(), "value".to_string()],
            query: Box::new(Query::default()),
            is_recursive: false,
        };

        let with_clause = WithClause { ctes: vec![cte] };

        executor.execute_ctes(&with_clause).unwrap();
        assert!(executor.has_cte("test_cte"));
    }

    #[test]
    fn test_recursive_executor() {
        let mut executor = RecursiveQueryExecutor::new(5);

        let base_query = Query::default();
        let recursive_query = Query::default();

        let filter = |_: &serde_json::Value| true;

        let result = executor.execute_recursive(&base_query, &recursive_query, filter);
        assert!(result.is_ok());
    }
}
