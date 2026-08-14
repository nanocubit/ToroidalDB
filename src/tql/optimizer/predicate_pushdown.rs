use crate::tql::ast::{PropertyValue, WhereCondition};
use serde_json::Value;

pub trait Predicate: Send + Sync {
    fn evaluate(&self, row: &serde_json::Map<String, Value>) -> bool;
    fn estimate_selectivity(&self) -> f64;
}

pub struct PropertyPredicate {
    pub property: String,
    pub operator: String,
    pub value: PropertyValue,
}

impl PropertyPredicate {
    pub fn new(property: String, operator: String, value: PropertyValue) -> Self {
        PropertyPredicate {
            property,
            operator,
            value,
        }
    }

    pub fn from_where(where_clause: &WhereCondition) -> Option<Box<dyn Predicate>> {
        match where_clause {
            WhereCondition::PropertyFilter {
                property,
                operator,
                value,
            } => Some(Box::new(PropertyPredicate::new(
                property.clone(),
                operator.clone(),
                value.clone(),
            ))),
            _ => None,
        }
    }
}

impl Predicate for PropertyPredicate {
    fn evaluate(&self, row: &serde_json::Map<String, Value>) -> bool {
        let prop_value = match row.get(&self.property) {
            Some(v) => v,
            None => return false,
        };

        let json_value = match &self.value {
            PropertyValue::String(s) => Value::String(s.clone()),
            PropertyValue::Number(n) => Value::Number(
                serde_json::Number::from_f64(*n).unwrap_or(serde_json::Number::from(0)),
            ),
            PropertyValue::Boolean(b) => Value::Bool(*b),
        };

        match self.operator.as_str() {
            "=" | "==" => Some(prop_value) == Some(&json_value),
            "!=" | "<>" => Some(prop_value) != Some(&json_value),
            ">" => {
                if let (Some(p), Some(v)) = (prop_value.as_f64(), json_value.as_f64()) {
                    p > v
                } else {
                    false
                }
            }
            ">=" => {
                if let (Some(p), Some(v)) = (prop_value.as_f64(), json_value.as_f64()) {
                    p >= v
                } else {
                    false
                }
            }
            "<" => {
                if let (Some(p), Some(v)) = (prop_value.as_f64(), json_value.as_f64()) {
                    p < v
                } else {
                    false
                }
            }
            "<=" => {
                if let (Some(p), Some(v)) = (prop_value.as_f64(), json_value.as_f64()) {
                    p <= v
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn estimate_selectivity(&self) -> f64 {
        match self.operator.as_str() {
            "=" | "==" => 0.01,
            "!=" | "<>" => 0.99,
            ">" | ">=" => 0.25,
            "<" | "<=" => 0.25,
            _ => 0.5,
        }
    }
}

pub struct ConjunctionPredicate {
    pub predicates: Vec<Box<dyn Predicate>>,
}

impl ConjunctionPredicate {
    pub fn new(predicates: Vec<Box<dyn Predicate>>) -> Self {
        ConjunctionPredicate { predicates }
    }
}

impl Predicate for ConjunctionPredicate {
    fn evaluate(&self, row: &serde_json::Map<String, Value>) -> bool {
        self.predicates.iter().all(|p| p.evaluate(row))
    }

    fn estimate_selectivity(&self) -> f64 {
        self.predicates
            .iter()
            .map(|p| p.estimate_selectivity())
            .product()
    }
}

pub struct DisjunctionPredicate {
    pub predicates: Vec<Box<dyn Predicate>>,
}

impl DisjunctionPredicate {
    pub fn new(predicates: Vec<Box<dyn Predicate>>) -> Self {
        DisjunctionPredicate { predicates }
    }
}

impl Predicate for DisjunctionPredicate {
    fn evaluate(&self, row: &serde_json::Map<String, Value>) -> bool {
        self.predicates.iter().any(|p| p.evaluate(row))
    }

    fn estimate_selectivity(&self) -> f64 {
        1.0 - self
            .predicates
            .iter()
            .map(|p| 1.0 - p.estimate_selectivity())
            .product::<f64>()
    }
}

pub struct PredicateOptimizer;

impl PredicateOptimizer {
    pub fn reorder_predicates(predicates: Vec<Box<dyn Predicate>>) -> Vec<Box<dyn Predicate>> {
        let mut predicates = predicates;
        predicates.sort_by(|a, b| {
            a.estimate_selectivity()
                .partial_cmp(&b.estimate_selectivity())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        predicates
    }
}
