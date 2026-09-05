use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::ast::{CommonTableExpression, Query, WithClause};
use crate::tql::executor::QueryExecutor;
use std::collections::HashMap;
use std::sync::Arc;

pub struct CteExecutor {
    cte_results: HashMap<String, Vec<serde_json::Value>>,
    store: Arc<HybridPersistentStore>,
}

impl CteExecutor {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        CteExecutor {
            cte_results: HashMap::new(),
            store,
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
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {}", e))?;
        let results = rt
            .block_on(QueryExecutor::execute_query(&self.store, query.clone()))
            .map_err(|e| format!("CTE query execution failed: {}", e))?;
        Ok(results.into_iter().map(|r| r.properties.clone()).collect())
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
        panic!("CteExecutor::default() not supported")
    }
}

pub struct RecursiveQueryExecutor {
    max_depth: usize,
    current_depth: usize,
    store: Arc<HybridPersistentStore>,
}

impl RecursiveQueryExecutor {
    pub fn new(max_depth: usize, store: Arc<HybridPersistentStore>) -> Self {
        RecursiveQueryExecutor {
            max_depth,
            current_depth: 0,
            store,
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
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {}", e))?;
        let results = rt
            .block_on(QueryExecutor::execute_query(&self.store, query.clone()))
            .map_err(|e| format!("CTE query execution failed: {}", e))?;
        Ok(results.into_iter().map(|r| r.properties.clone()).collect())
    }
}

impl Default for RecursiveQueryExecutor {
    fn default() -> Self {
        panic!("RecursiveQueryExecutor::default() not supported")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cte_executor() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(HybridPersistentStore::open(dir.path().join("test")).unwrap());
        let mut executor = CteExecutor::new(store);

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
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(HybridPersistentStore::open(dir.path().join("test")).unwrap());
        let mut executor = RecursiveQueryExecutor::new(5, store);

        let base_query = Query::default();
        let recursive_query = Query::default();

        let filter = |_: &serde_json::Value| true;

        let result = executor.execute_recursive(&base_query, &recursive_query, filter);
        assert!(result.is_ok());
    }
}
