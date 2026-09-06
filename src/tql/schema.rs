use crate::tql::ast::{DataType, EdgeTypeDef, FieldDef, NodeTypeDef};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Registry for managing node and edge type schemas
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SchemaRegistry {
    nodes: RwLock<HashMap<String, NodeTypeDef>>,
    edges: RwLock<HashMap<String, EdgeTypeDef>>,
}

impl SchemaRegistry {
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            edges: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new node type
    pub fn create_node_type(&self, def: NodeTypeDef) -> Result<(), SchemaError> {
        let mut nodes = self.nodes.write().map_err(|_| SchemaError::LockError)?;

        if nodes.contains_key(&def.name) {
            return Err(SchemaError::TypeAlreadyExists(def.name.clone()));
        }

        // Validate field definitions
        Self::validate_node_fields(&def.fields)?;

        nodes.insert(def.name.clone(), def);
        Ok(())
    }

    /// Register a new edge type
    pub fn create_edge_type(&self, def: EdgeTypeDef) -> Result<(), SchemaError> {
        let mut edges = self.edges.write().map_err(|_| SchemaError::LockError)?;

        if edges.contains_key(&def.name) {
            return Err(SchemaError::TypeAlreadyExists(def.name.clone()));
        }

        // Validate that referenced node types exist
        {
            let nodes = self.nodes.read().map_err(|_| SchemaError::LockError)?;
            if !nodes.contains_key(&def.from) {
                return Err(SchemaError::UnknownNodeType(def.from.clone()));
            }
            if !nodes.contains_key(&def.to) {
                return Err(SchemaError::UnknownNodeType(def.to.clone()));
            }
        }

        edges.insert(def.name.clone(), def);
        Ok(())
    }

    /// Drop a node type
    pub fn drop_node_type(&self, name: &str) -> Result<(), SchemaError> {
        let mut nodes = self.nodes.write().map_err(|_| SchemaError::LockError)?;

        if !nodes.contains_key(name) {
            return Err(SchemaError::TypeNotFound(name.to_string()));
        }

        // Check if any edge types reference this node type
        let edges = self.edges.read().map_err(|_| SchemaError::LockError)?;
        for (_, edge) in edges.iter() {
            if edge.from == name || edge.to == name {
                return Err(SchemaError::TypeInUse(name.to_string()));
            }
        }

        nodes.remove(name);
        Ok(())
    }

    /// Drop an edge type
    pub fn drop_edge_type(&self, name: &str) -> Result<(), SchemaError> {
        let mut edges = self.edges.write().map_err(|_| SchemaError::LockError)?;

        if !edges.contains_key(name) {
            return Err(SchemaError::TypeNotFound(name.to_string()));
        }

        edges.remove(name);
        Ok(())
    }

    /// Get node type definition
    pub fn get_node_type(&self, name: &str) -> Result<Option<NodeTypeDef>, SchemaError> {
        let nodes = self.nodes.read().map_err(|_| SchemaError::LockError)?;
        Ok(nodes.get(name).cloned())
    }

    /// Get edge type definition
    pub fn get_edge_type(&self, name: &str) -> Result<Option<EdgeTypeDef>, SchemaError> {
        let edges = self.edges.read().map_err(|_| SchemaError::LockError)?;
        Ok(edges.get(name).cloned())
    }

    /// Validate that a field exists in a node type
    pub fn validate_field(
        &self,
        node_type: &str,
        field_name: &str,
    ) -> Result<DataType, SchemaError> {
        let node = self.get_node_type(node_type)?;

        match node {
            Some(def) => def
                .fields
                .iter()
                .find(|f| f.name == field_name)
                .map(|f| f.data_type.clone())
                .ok_or_else(|| SchemaError::FieldNotFound(field_name.to_string())),
            None => Err(SchemaError::UnknownNodeType(node_type.to_string())),
        }
    }

    /// Validate that a field is a vector type with correct dimension
    pub fn validate_vector_field(
        &self,
        node_type: &str,
        field_name: &str,
        expected_dim: Option<u32>,
    ) -> Result<u32, SchemaError> {
        let data_type = self.validate_field(node_type, field_name)?;

        match data_type {
            DataType::Vector(dim) => {
                if let Some(expected) = expected_dim {
                    if dim != expected {
                        return Err(SchemaError::DimensionMismatch {
                            field: field_name.to_string(),
                            expected,
                            actual: dim,
                        });
                    }
                }
                Ok(dim)
            }
            _ => Err(SchemaError::TypeMismatch {
                field: field_name.to_string(),
                expected: "VECTOR".to_string(),
                actual: format!("{data_type:?}"),
            }),
        }
    }

    /// Get all registered node types
    pub fn list_node_types(&self) -> Result<Vec<String>, SchemaError> {
        let nodes = self.nodes.read().map_err(|_| SchemaError::LockError)?;
        Ok(nodes.keys().cloned().collect())
    }

    /// Get all registered edge types
    pub fn list_edge_types(&self) -> Result<Vec<String>, SchemaError> {
        let edges = self.edges.read().map_err(|_| SchemaError::LockError)?;
        Ok(edges.keys().cloned().collect())
    }

    /// Validate node fields
    fn validate_node_fields(fields: &[FieldDef]) -> Result<(), SchemaError> {
        let mut pk_count = 0;

        for field in fields {
            if field.is_primary_key {
                pk_count += 1;
            }

            // Validate vector constraints
            if let DataType::Vector(dim) = field.data_type {
                if dim == 0 {
                    return Err(SchemaError::InvalidDimension(field.name.clone()));
                }
            }
        }

        if pk_count > 1 {
            return Err(SchemaError::MultiplePrimaryKeys);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchemaError {
    LockError,
    TypeAlreadyExists(String),
    TypeNotFound(String),
    UnknownNodeType(String),
    FieldNotFound(String),
    TypeInUse(String),
    TypeMismatch {
        field: String,
        expected: String,
        actual: String,
    },
    DimensionMismatch {
        field: String,
        expected: u32,
        actual: u32,
    },
    InvalidDimension(String),
    MultiplePrimaryKeys,
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaError::LockError => write!(f, "Failed to acquire lock"),
            SchemaError::TypeAlreadyExists(name) => write!(f, "Type '{name}' already exists"),
            SchemaError::TypeNotFound(name) => write!(f, "Type '{name}' not found"),
            SchemaError::UnknownNodeType(name) => write!(f, "Unknown node type '{name}'"),
            SchemaError::FieldNotFound(name) => write!(f, "Field '{name}' not found"),
            SchemaError::TypeInUse(name) => write!(f, "Type '{name}' is in use by edges"),
            SchemaError::TypeMismatch {
                field,
                expected,
                actual,
            } => {
                write!(f, "Field '{field}': expected {expected}, got {actual}")
            }
            SchemaError::DimensionMismatch {
                field,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Field '{field}': expected dimension {expected}, got {actual}"
                )
            }
            SchemaError::InvalidDimension(name) => {
                write!(f, "Invalid dimension for field '{name}'")
            }
            SchemaError::MultiplePrimaryKeys => write!(f, "Multiple primary keys not allowed"),
        }
    }
}

impl std::error::Error for SchemaError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tql::ast::{DataType, FieldConstraint, FieldDef};

    #[test]
    fn test_create_node_type() {
        let registry = SchemaRegistry::new();

        let node_def = NodeTypeDef {
            name: "Document".to_string(),
            fields: vec![
                FieldDef {
                    name: "id".to_string(),
                    data_type: DataType::Int,
                    is_primary_key: true,
                    constraints: vec![FieldConstraint::NotNull],
                },
                FieldDef {
                    name: "content_t3".to_string(),
                    data_type: DataType::Vector(1536),
                    is_primary_key: false,
                    constraints: vec![FieldConstraint::VectorIndex { phi: 5.71 }],
                },
            ],
        };

        assert!(registry.create_node_type(node_def).is_ok());
        assert!(registry.get_node_type("Document").unwrap().is_some());
    }

    #[test]
    fn test_duplicate_node_type() {
        let registry = SchemaRegistry::new();

        let node_def = NodeTypeDef {
            name: "Document".to_string(),
            fields: vec![],
        };

        registry.create_node_type(node_def.clone()).unwrap();
        assert!(matches!(
            registry.create_node_type(node_def),
            Err(SchemaError::TypeAlreadyExists(_))
        ));
    }

    #[test]
    fn test_validate_vector_field() {
        let registry = SchemaRegistry::new();

        let node_def = NodeTypeDef {
            name: "Document".to_string(),
            fields: vec![FieldDef {
                name: "content_t3".to_string(),
                data_type: DataType::Vector(1536),
                is_primary_key: false,
                constraints: vec![],
            }],
        };

        registry.create_node_type(node_def).unwrap();

        // Valid field
        assert_eq!(
            registry
                .validate_vector_field("Document", "content_t3", Some(1536))
                .unwrap(),
            1536
        );

        // Dimension mismatch
        assert!(matches!(
            registry.validate_vector_field("Document", "content_t3", Some(768)),
            Err(SchemaError::DimensionMismatch { .. })
        ));
    }
}
