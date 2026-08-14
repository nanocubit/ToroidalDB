use crate::tql::ast::{WindowExpression, WindowFunction, WindowSpec};
use std::collections::HashMap;

pub struct WindowFunctionExecutor {
    partition_cache: HashMap<String, Vec<usize>>,
}

impl WindowFunctionExecutor {
    pub fn new() -> Self {
        WindowFunctionExecutor {
            partition_cache: HashMap::new(),
        }
    }

    pub fn execute(
        &mut self,
        data: &mut Vec<Vec<serde_json::Value>>,
        window_expr: &WindowExpression,
    ) -> Result<Vec<serde_json::Value>, String> {
        let empty_partition: Vec<String> = vec![];
        let partition = window_expr
            .window_spec
            .as_ref()
            .map(|spec| &spec.partition_by)
            .unwrap_or(&empty_partition);

        if partition.is_empty() {
            self.execute_non_partitioned(data, &window_expr.function)
        } else {
            self.execute_partitioned(data, &window_expr.function, partition)
        }
    }

    fn execute_non_partitioned(
        &self,
        data: &[Vec<serde_json::Value>],
        function: &WindowFunction,
    ) -> Result<Vec<serde_json::Value>, String> {
        match function {
            WindowFunction::RowNumber => Ok(data
                .iter()
                .enumerate()
                .map(|(i, _)| serde_json::json!((i + 1) as i64))
                .collect()),
            WindowFunction::Rank => Ok(self.compute_rank(data)),
            WindowFunction::DenseRank => Ok(self.compute_dense_rank(data)),
            WindowFunction::Lag(field) => Ok(self.compute_lag(data, field, 1)),
            WindowFunction::Lead(field) => Ok(self.compute_lead(data, field, 1)),
            WindowFunction::FirstValue(field) => {
                let first = data
                    .first()
                    .and_then(|row| row.iter().find(|v| !v.is_null()))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Ok(vec![first; data.len()])
            }
            WindowFunction::LastValue(field) => {
                let last = data
                    .last()
                    .and_then(|row| row.iter().find(|v| !v.is_null()))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Ok(vec![last; data.len()])
            }
            WindowFunction::Sum(field) => {
                self.compute_aggregate(data, field, |acc, v| acc + v.as_f64().unwrap_or(0.0))
            }
            WindowFunction::Avg(field) => {
                let sum =
                    self.compute_aggregate(data, field, |acc, v| acc + v.as_f64().unwrap_or(0.0))?;
                let count = data.len() as f64;
                let avg = sum.first().and_then(|v| v.as_f64()).unwrap_or(0.0) / count;
                Ok(vec![serde_json::json!(avg); data.len()])
            }
            WindowFunction::Count => {
                let count = serde_json::json!(data.len() as i64);
                Ok(vec![count; data.len()])
            }
        }
    }

    fn execute_partitioned(
        &self,
        data: &[Vec<serde_json::Value>],
        function: &WindowFunction,
        partition_by: &[String],
    ) -> Result<Vec<serde_json::Value>, String> {
        let mut partitions: HashMap<String, Vec<Vec<serde_json::Value>>> = HashMap::new();

        for row in data {
            let key = partition_by
                .iter()
                .enumerate()
                .map(|(i, _)| row.get(i).map(|v| v.to_string()).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("|");

            partitions
                .entry(key)
                .or_insert_with(Vec::new)
                .push(row.clone());
        }

        let mut results = Vec::with_capacity(data.len());

        for row in data {
            let key = partition_by
                .iter()
                .enumerate()
                .map(|(i, _)| row.get(i).map(|v| v.to_string()).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("|");

            if let Some(partition) = partitions.get(&key) {
                let result = match function {
                    WindowFunction::RowNumber => {
                        let pos = partition.iter().position(|r| r == row);
                        serde_json::json!((pos.unwrap_or(0) + 1) as i64)
                    }
                    WindowFunction::Rank => {
                        let pos = partition.iter().position(|r| r == row).unwrap_or(0);
                        serde_json::json!((pos + 1) as i64)
                    }
                    WindowFunction::DenseRank => {
                        let unique_rows: Vec<_> = partition.iter().unique().collect();
                        let pos = unique_rows.iter().position(|r| *r == row).unwrap_or(0);
                        serde_json::json!((pos + 1) as i64)
                    }
                    WindowFunction::Lag(field) => {
                        let pos = partition.iter().position(|r| r == row).unwrap_or(0);
                        if pos > 0 {
                            partition[pos - 1]
                                .iter()
                                .find(|v| !v.is_null())
                                .cloned()
                                .unwrap_or(serde_json::Value::Null)
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    WindowFunction::Lead(field) => {
                        let pos = partition.iter().position(|r| r == row).unwrap_or(0);
                        if pos < partition.len() - 1 {
                            partition[pos + 1]
                                .iter()
                                .find(|v| !v.is_null())
                                .cloned()
                                .unwrap_or(serde_json::Value::Null)
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    WindowFunction::FirstValue(field) => {
                        partition
                            .first()
                            .and_then(|row| row.iter().find(|v| !v.is_null()))
                            .cloned()
                            .unwrap_or(serde_json::Value::Null)
                    }
                    WindowFunction::LastValue(field) => {
                        partition
                            .last()
                            .and_then(|row| row.iter().find(|v| !v.is_null()))
                            .cloned()
                            .unwrap_or(serde_json::Value::Null)
                    }
                    WindowFunction::Sum(field) => {
                        let sum: f64 = partition
                            .iter()
                            .flat_map(|row| row.iter().filter_map(|v| v.as_f64()))
                            .sum();
                        serde_json::json!(sum)
                    }
                    WindowFunction::Avg(field) => {
                        let values: Vec<f64> = partition
                            .iter()
                            .flat_map(|row| row.iter().filter_map(|v| v.as_f64()))
                            .collect();
                        if values.is_empty() {
                            serde_json::Value::Null
                        } else {
                            serde_json::json!(values.iter().sum::<f64>() / values.len() as f64)
                        }
                    }
                    WindowFunction::Count => {
                        serde_json::json!(partition.len() as i64)
                    }
                };
                results.push(result);
            } else {
                results.push(serde_json::Value::Null);
            }
        }

        Ok(results)
    }

    fn compute_rank(&self, data: &[Vec<serde_json::Value>]) -> Vec<serde_json::Value> {
        let mut ranks: Vec<(usize, i64)> = data.iter().enumerate().map(|(i, _)| (i, 1)).collect();

        for i in 0..ranks.len() {
            let mut same = 1;
            for j in (i + 1)..ranks.len() {
                if data[i] == data[j] {
                    same += 1;
                }
            }
            ranks[i].1 = (i + 1) as i64;
        }

        ranks
            .into_iter()
            .map(|(_, r)| serde_json::json!(r))
            .collect()
    }

    fn compute_dense_rank(&self, data: &[Vec<serde_json::Value>]) -> Vec<serde_json::Value> {
        let mut sorted: Vec<(usize, Vec<serde_json::Value>)> = data
            .iter()
            .enumerate()
            .map(|(i, row)| (i, row.clone()))
            .collect();

        sorted.sort_by(|a, b| {
            for (av, bv) in a.1.iter().zip(b.1.iter()) {
                if let (Some(av_cmp), Some(bv_cmp)) = (av.as_str(), bv.as_str()) {
                    let cmp = av_cmp.cmp(bv_cmp);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                if let (Some(an), Some(bn)) = (av.as_i64(), bv.as_i64()) {
                    let cmp = an.cmp(&bn);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                if let (Some(an), Some(bn)) = (av.as_f64(), bv.as_f64()) {
                    let cmp = an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
            }
            std::cmp::Ordering::Equal
        });

        let mut rank = 1i64;
        let mut results = vec![serde_json::Value::Null; data.len()];

        for (i, (orig_idx, _)) in sorted.iter().enumerate() {
            if i > 0 && sorted[i].1 != sorted[i - 1].1 {
                rank += 1;
            }
            results[*orig_idx] = serde_json::json!(rank);
        }

        results
    }

    fn compute_lag(
        &self,
        data: &[Vec<serde_json::Value>],
        field: &str,
        offset: usize,
    ) -> Vec<serde_json::Value> {
        let field_idx = field
            .strip_prefix('$')
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        let mut results = Vec::with_capacity(data.len());

        for (i, row) in data.iter().enumerate() {
            if i >= offset {
                results.push(
                    row.get(field_idx)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
            } else {
                results.push(serde_json::Value::Null);
            }
        }

        results
    }

    fn compute_lead(
        &self,
        data: &[Vec<serde_json::Value>],
        field: &str,
        offset: usize,
    ) -> Vec<serde_json::Value> {
        let field_idx = field
            .strip_prefix('$')
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        let mut results = Vec::with_capacity(data.len());

        for (i, row) in data.iter().enumerate() {
            if i + offset < data.len() {
                results.push(
                    data[i + offset]
                        .get(field_idx)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
            } else {
                results.push(serde_json::Value::Null);
            }
        }

        results
    }

    fn compute_aggregate<F>(
        &self,
        data: &[Vec<serde_json::Value>],
        field: &str,
        op: F,
    ) -> Result<Vec<serde_json::Value>, String>
    where
        F: Fn(f64, &serde_json::Value) -> f64,
    {
        let field_idx = field
            .strip_prefix('$')
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        let mut acc = 0.0;
        for row in data {
            if let Some(val) = row.get(field_idx) {
                acc = op(acc, val);
            }
        }

        Ok(vec![serde_json::json!(acc); data.len()])
    }
}

impl Default for WindowFunctionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_number() {
        let mut executor = WindowFunctionExecutor::new();
        let data = vec![
            vec![serde_json::json!("a")],
            vec![serde_json::json!("b")],
            vec![serde_json::json!("c")],
        ];

        let window_expr = WindowExpression {
            function: WindowFunction::RowNumber,
            window_spec: None,
            alias: "rn".to_string(),
        };

        let result = executor.execute(&mut data.clone(), &window_expr).unwrap();
        assert_eq!(result[0], serde_json::json!(1));
        assert_eq!(result[1], serde_json::json!(2));
        assert_eq!(result[2], serde_json::json!(3));
    }

    #[test]
    fn test_lag() {
        let mut executor = WindowFunctionExecutor::new();
        let data = vec![
            vec![serde_json::json!(1)],
            vec![serde_json::json!(2)],
            vec![serde_json::json!(3)],
        ];

        let window_expr = WindowExpression {
            function: WindowFunction::Lag("$0".to_string()),
            window_spec: None,
            alias: "prev".to_string(),
        };

        let result = executor.execute(&mut data.clone(), &window_expr).unwrap();
        assert_eq!(result[0], serde_json::Value::Null);
        assert_eq!(result[1], serde_json::json!(1));
        assert_eq!(result[2], serde_json::json!(2));
    }

    #[test]
    fn test_lead() {
        let mut executor = WindowFunctionExecutor::new();
        let data = vec![
            vec![serde_json::json!(1)],
            vec![serde_json::json!(2)],
            vec![serde_json::json!(3)],
        ];

        let window_expr = WindowExpression {
            function: WindowFunction::Lead("$0".to_string()),
            window_spec: None,
            alias: "next".to_string(),
        };

        let result = executor.execute(&mut data.clone(), &window_expr).unwrap();
        assert_eq!(result[0], serde_json::json!(2));
        assert_eq!(result[1], serde_json::json!(3));
        assert_eq!(result[2], serde_json::Value::Null);
    }
}
