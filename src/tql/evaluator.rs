//! Expression evaluator for TQL conditions and triggers.
//!
//! AST для выражений и полноценный evaluator с поддержкой:
//! - `FieldAccess` (поля узлов)
//! - Literal (константы)
//! - `BinaryOp` (сравнения, логические операции)
//! - `FunctionCall` (TOROIDALCOSINE, CONTAINS, `ARRAY_CONTAINS`)
//! - `ToroidalDistance` (специализированная функция)

use crate::tql::ast::PropertyValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Expression AST.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    FieldAccess {
        field: String,
    },
    Literal(PropertyValue),
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BinaryOperator {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
}

/// Context for evaluation.
#[derive(Debug, Clone)]
pub struct EvalContext {
    /// Row data (fields of the current node/edge).
    pub row: HashMap<String, PropertyValue>,
    /// Global variables (e.g., $`breakthrough_centroid`).
    pub globals: HashMap<String, PropertyValue>,
}

impl EvalContext {
    pub fn new(row: HashMap<String, PropertyValue>) -> Self {
        Self {
            row,
            globals: HashMap::new(),
        }
    }

    pub fn with_globals(
        row: HashMap<String, PropertyValue>,
        globals: HashMap<String, PropertyValue>,
    ) -> Self {
        Self { row, globals }
    }

    pub fn get_field(&self, field: &str) -> Option<&PropertyValue> {
        self.row.get(field).or_else(|| self.globals.get(field))
    }
}

/// Expression evaluator.
pub struct ExpressionEvaluator;

impl Default for ExpressionEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpressionEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// Evaluate an expression in the given context.
    pub fn eval(&self, expr: &Expression, ctx: &EvalContext) -> Result<PropertyValue, String> {
        match expr {
            Expression::Literal(val) => Ok(val.clone()),
            Expression::FieldAccess { field } => ctx
                .get_field(field)
                .cloned()
                .ok_or_else(|| format!("Field '{field}' not found")),
            Expression::BinaryOp { left, op, right } => {
                let left_val = self.eval(left, ctx)?;
                let right_val = self.eval(right, ctx)?;
                self.eval_binary_op(&left_val, *op, &right_val)
            }
            Expression::FunctionCall { name, args } => {
                let args_val: Result<Vec<PropertyValue>, _> =
                    args.iter().map(|arg| self.eval(arg, ctx)).collect();
                let args_val = args_val?;
                self.eval_function(name, &args_val)
            }
            Expression::ToroidalDistance {
                left,
                right,
                phi: _,
            } => {
                let left_vec = self.eval_vec(left, ctx)?;
                let right_vec = self.eval_vec(right, ctx)?;
                let distance = f64::from(crate::math::toroidal_distance(&left_vec, &right_vec));
                Ok(PropertyValue::Number(distance))
            }
        }
    }

    fn eval_binary_op(
        &self,
        left: &PropertyValue,
        op: BinaryOperator,
        right: &PropertyValue,
    ) -> Result<PropertyValue, String> {
        match (left, right) {
            (PropertyValue::Number(l), PropertyValue::Number(r)) => {
                Ok(PropertyValue::Boolean(match op {
                    BinaryOperator::Eq => (l - r).abs() < 1e-6,
                    BinaryOperator::Neq => (l - r).abs() >= 1e-6,
                    BinaryOperator::Lt => l < r,
                    BinaryOperator::Lte => l <= r,
                    BinaryOperator::Gt => l > r,
                    BinaryOperator::Gte => l >= r,
                    _ => return Err("Invalid operator for numbers".into()),
                }))
            }
            (PropertyValue::String(l), PropertyValue::String(r)) => {
                Ok(PropertyValue::Boolean(match op {
                    BinaryOperator::Eq => l == r,
                    BinaryOperator::Neq => l != r,
                    _ => return Err("Invalid operator for strings".into()),
                }))
            }
            (PropertyValue::Boolean(l), PropertyValue::Boolean(r)) => {
                Ok(PropertyValue::Boolean(match op {
                    BinaryOperator::Eq => l == r,
                    BinaryOperator::Neq => l != r,
                    BinaryOperator::And => *l && *r,
                    BinaryOperator::Or => *l || *r,
                    _ => return Err("Invalid operator for booleans".into()),
                }))
            }
            _ => Err(format!("Type mismatch: {left:?} vs {right:?}")),
        }
    }

    fn eval_function(&self, name: &str, args: &[PropertyValue]) -> Result<PropertyValue, String> {
        match name.to_uppercase().as_str() {
            "TOROIDALCOSINE" => {
                if args.len() != 2 {
                    return Err("TOROIDALCOSINE expects 2 arguments".into());
                }
                let left = self.extract_vec(&args[0])?;
                let right = self.extract_vec(&args[1])?;
                let cosine = toroidal_cosine(&left, &right, 5.71);
                Ok(PropertyValue::Number(f64::from(cosine)))
            }
            "CONTAINS" => {
                if args.len() != 2 {
                    return Err("CONTAINS expects 2 arguments".into());
                }
                let haystack = self.to_string(&args[0])?;
                let needle = self.to_string(&args[1])?;
                Ok(PropertyValue::Boolean(haystack.contains(&needle)))
            }
            "ARRAY_CONTAINS" => {
                if args.len() != 2 {
                    return Err("ARRAY_CONTAINS expects 2 arguments".into());
                }
                let arr = self.extract_array(&args[0])?;
                let needle = self.to_string(&args[1])?;
                Ok(PropertyValue::Boolean(arr.iter().any(|v| {
                    self.to_string(v).map(|s| s == needle).unwrap_or(false)
                })))
            }
            _ => Err(format!("Unknown function: {name}")),
        }
    }

    fn eval_vec(&self, expr: &Expression, ctx: &EvalContext) -> Result<Vec<f32>, String> {
        match self.eval(expr, ctx)? {
            PropertyValue::String(s) => {
                // Try to parse as JSON array of floats
                serde_json::from_str::<Vec<f32>>(&s)
                    .map_err(|_| format!("Cannot parse vector from: {s}"))
            }
            val => Err(format!("Expected vector, got {val:?}")),
        }
    }

    fn extract_vec(&self, val: &PropertyValue) -> Result<Vec<f32>, String> {
        match val {
            PropertyValue::String(s) => serde_json::from_str::<Vec<f32>>(s)
                .map_err(|_| format!("Cannot parse vector from: {s}")),
            PropertyValue::Number(n) => Ok(vec![*n as f32]),
            _ => Err(format!("Expected vector, got {val:?}")),
        }
    }

    fn extract_array(&self, val: &PropertyValue) -> Result<Vec<PropertyValue>, String> {
        // For now, arrays are serialized as JSON strings in PropertyValue
        match val {
            PropertyValue::String(s) => {
                let arr: Vec<serde_json::Value> =
                    serde_json::from_str(s).map_err(|_| format!("Cannot parse array from: {s}"))?;
                Ok(arr
                    .into_iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => PropertyValue::String(s),
                        serde_json::Value::Number(n) => {
                            PropertyValue::Number(n.as_f64().unwrap_or(0.0))
                        }
                        serde_json::Value::Bool(b) => PropertyValue::Boolean(b),
                        _ => PropertyValue::String(v.to_string()),
                    })
                    .collect())
            }
            _ => Err(format!("Expected array, got {val:?}")),
        }
    }

    fn to_string(&self, val: &PropertyValue) -> Result<String, String> {
        match val {
            PropertyValue::String(s) => Ok(s.clone()),
            PropertyValue::Number(n) => Ok(n.to_string()),
            PropertyValue::Boolean(b) => Ok(b.to_string()),
        }
    }
}

/// Simple toroidal cosine similarity (wrapper around existing math).
fn toroidal_cosine(a: &[f32], b: &[f32], _phi: f32) -> f32 {
    let dist = crate::math::toroidal_distance(a, b);
    1.0 / (1.0 + dist)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal() {
        let eval = ExpressionEvaluator::new();
        let ctx = EvalContext::new(HashMap::new());
        let expr = Expression::Literal(PropertyValue::Number(42.0));
        assert_eq!(eval.eval(&expr, &ctx).unwrap(), PropertyValue::Number(42.0));
    }

    #[test]
    fn test_field_access() {
        let eval = ExpressionEvaluator::new();
        let mut row = HashMap::new();
        row.insert(
            "title".to_string(),
            PropertyValue::String("hello".to_string()),
        );
        let ctx = EvalContext::new(row);
        let expr = Expression::FieldAccess {
            field: "title".to_string(),
        };
        assert_eq!(
            eval.eval(&expr, &ctx).unwrap(),
            PropertyValue::String("hello".to_string())
        );
    }

    #[test]
    fn test_binary_op_gt() {
        let eval = ExpressionEvaluator::new();
        let ctx = EvalContext::new(HashMap::new());
        let expr = Expression::BinaryOp {
            left: Box::new(Expression::Literal(PropertyValue::Number(10.0))),
            op: BinaryOperator::Gt,
            right: Box::new(Expression::Literal(PropertyValue::Number(5.0))),
        };
        assert_eq!(
            eval.eval(&expr, &ctx).unwrap(),
            PropertyValue::Boolean(true)
        );
    }

    #[test]
    fn test_and_or() {
        let eval = ExpressionEvaluator::new();
        let ctx = EvalContext::new(HashMap::new());
        let expr = Expression::BinaryOp {
            left: Box::new(Expression::BinaryOp {
                left: Box::new(Expression::Literal(PropertyValue::Number(5.0))),
                op: BinaryOperator::Gt,
                right: Box::new(Expression::Literal(PropertyValue::Number(3.0))),
            }),
            op: BinaryOperator::And,
            right: Box::new(Expression::Literal(PropertyValue::Boolean(true))),
        };
        assert_eq!(
            eval.eval(&expr, &ctx).unwrap(),
            PropertyValue::Boolean(true)
        );
    }

    #[test]
    fn test_contains() {
        let eval = ExpressionEvaluator::new();
        let ctx = EvalContext::new(HashMap::new());
        let expr = Expression::FunctionCall {
            name: "CONTAINS".to_string(),
            args: vec![
                Expression::Literal(PropertyValue::String("quantum computing".to_string())),
                Expression::Literal(PropertyValue::String("quantum".to_string())),
            ],
        };
        assert_eq!(
            eval.eval(&expr, &ctx).unwrap(),
            PropertyValue::Boolean(true)
        );
    }
}
