//! # Space Manager для TQL v3.0
//!
//! Управление пространствами данных - контейнерами для узлов, рёбер и стримов

use crate::tql::ast::{
    AlterOperation, AlterSpace, Duration, EdgeTypeDef, NodeTypeDef, SpaceConfig, SpaceDef,
    StreamDef, StreamSchema,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Менеджер пространств данных
pub struct SpaceManager {
    spaces: Arc<RwLock<HashMap<String, SpaceDefinition>>>,
    current_space: Arc<RwLock<String>>,
}

/// Определение пространства
#[derive(Debug, Clone)]
pub struct SpaceDefinition {
    pub name: String,
    pub node_types: HashMap<String, NodeTypeDef>,
    pub edge_types: HashMap<String, EdgeTypeDef>,
    pub streams: HashMap<String, StreamDefinition>,
    pub config: SpaceConfig,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Определение стрима
#[derive(Debug, Clone)]
pub struct StreamDefinition {
    pub name: String,
    pub topic: String,
    pub schema: StreamSchema,
    pub retention: Option<Duration>,
    pub created_at: u64,
}

impl Default for SpaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SpaceManager {
    pub fn new() -> Self {
        let mut spaces = HashMap::new();

        // Создаём default пространство
        let default_space = SpaceDefinition {
            name: "default".to_string(),
            node_types: HashMap::new(),
            edge_types: HashMap::new(),
            streams: HashMap::new(),
            config: SpaceConfig {
                shards: 4,
                replication_factor: 1,
                storage_backend: None,
                embedding_model: None,
            },
            created_at: get_timestamp(),
            updated_at: get_timestamp(),
        };

        spaces.insert("default".to_string(), default_space);

        Self {
            spaces: Arc::new(RwLock::new(spaces)),
            current_space: Arc::new(RwLock::new("default".to_string())),
        }
    }

    /// Создаёт новое пространство
    pub fn create_space(&self, space_def: SpaceDef) -> Result<(), String> {
        let mut spaces = self.spaces.write().map_err(|e| e.to_string())?;

        if spaces.contains_key(&space_def.name) {
            return Err(format!("Space '{}' already exists", space_def.name));
        }

        let now = get_timestamp();

        // Создаём пространство
        let space = SpaceDefinition {
            name: space_def.name.clone(),
            node_types: space_def
                .nodes
                .into_iter()
                .map(|n| (n.name.clone(), n))
                .collect(),
            edge_types: space_def
                .edges
                .into_iter()
                .map(|e| (e.name.clone(), e))
                .collect(),
            streams: space_def
                .streams
                .into_iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        StreamDefinition {
                            name: s.name.clone(),
                            topic: s.topic,
                            schema: s.schema,
                            retention: s.retention,
                            created_at: now,
                        },
                    )
                })
                .collect(),
            config: space_def.config,
            created_at: now,
            updated_at: now,
        };

        spaces.insert(space_def.name, space);
        Ok(())
    }

    /// Получает пространство по имени
    pub fn get_space(&self, name: &str) -> Option<SpaceDefinition> {
        let spaces = self.spaces.read().ok()?;
        spaces.get(name).cloned()
    }

    /// Получает текущее пространство
    pub fn get_current_space(&self) -> String {
        self.current_space
            .read()
            .map_or_else(|_| "default".to_string(), |s| s.clone())
    }

    /// Устанавливает текущее пространство
    pub fn set_current_space(&self, name: &str) -> Result<(), String> {
        let spaces = self.spaces.read().map_err(|e| e.to_string())?;
        if !spaces.contains_key(name) {
            return Err(format!("Space '{name}' does not exist"));
        }
        drop(spaces);

        let mut current = self.current_space.write().map_err(|e| e.to_string())?;
        *current = name.to_string();
        Ok(())
    }

    /// Добавляет тип узла в пространство
    pub fn add_node_type(&self, space_name: &str, node_type: NodeTypeDef) -> Result<(), String> {
        let mut spaces = self.spaces.write().map_err(|e| e.to_string())?;

        let space = spaces
            .get_mut(space_name)
            .ok_or_else(|| format!("Space '{space_name}' not found"))?;

        if space.node_types.contains_key(&node_type.name) {
            return Err(format!(
                "Node type '{}' already exists in space '{}'",
                node_type.name, space_name
            ));
        }

        space.node_types.insert(node_type.name.clone(), node_type);
        space.updated_at = get_timestamp();

        Ok(())
    }

    /// Добавляет тип ребра в пространство
    pub fn add_edge_type(&self, space_name: &str, edge_type: EdgeTypeDef) -> Result<(), String> {
        let mut spaces = self.spaces.write().map_err(|e| e.to_string())?;

        let space = spaces
            .get_mut(space_name)
            .ok_or_else(|| format!("Space '{space_name}' not found"))?;

        if space.edge_types.contains_key(&edge_type.name) {
            return Err(format!(
                "Edge type '{}' already exists in space '{}'",
                edge_type.name, space_name
            ));
        }

        space.edge_types.insert(edge_type.name.clone(), edge_type);
        space.updated_at = get_timestamp();

        Ok(())
    }

    /// Создаёт стрим в пространстве
    pub fn create_stream(&self, space_name: &str, stream_def: StreamDef) -> Result<(), String> {
        let mut spaces = self.spaces.write().map_err(|e| e.to_string())?;

        let space = spaces
            .get_mut(space_name)
            .ok_or_else(|| format!("Space '{space_name}' not found"))?;

        if space.streams.contains_key(&stream_def.name) {
            return Err(format!(
                "Stream '{}' already exists in space '{}'",
                stream_def.name, space_name
            ));
        }

        let now = get_timestamp();
        let stream = StreamDefinition {
            name: stream_def.name.clone(),
            topic: stream_def.topic,
            schema: stream_def.schema,
            retention: stream_def.retention,
            created_at: now,
        };

        space.streams.insert(stream_def.name, stream);
        space.updated_at = now;

        Ok(())
    }

    /// Изменяет пространство
    pub fn alter_space(&self, alter: AlterSpace) -> Result<(), String> {
        let mut spaces = self.spaces.write().map_err(|e| e.to_string())?;

        let space = spaces
            .get_mut(&alter.name)
            .ok_or_else(|| format!("Space '{}' not found", alter.name))?;

        for operation in alter.operations {
            match operation {
                AlterOperation::AddNode(node_type) => {
                    space.node_types.insert(node_type.name.clone(), node_type);
                }
                AlterOperation::AddEdge(edge_type) => {
                    space.edge_types.insert(edge_type.name.clone(), edge_type);
                }
                AlterOperation::DropNode(name) => {
                    space.node_types.remove(&name);
                }
                AlterOperation::DropEdge(name) => {
                    space.edge_types.remove(&name);
                }
                AlterOperation::AddField { node_type, field } => {
                    if let Some(nt) = space.node_types.get_mut(&node_type) {
                        nt.fields.push(field);
                    }
                }
                AlterOperation::DropField {
                    node_type,
                    field_name,
                } => {
                    if let Some(nt) = space.node_types.get_mut(&node_type) {
                        nt.fields.retain(|f| f.name != field_name);
                    }
                }
            }
        }

        space.updated_at = get_timestamp();
        Ok(())
    }

    /// Получает список всех пространств
    pub fn list_spaces(&self) -> Vec<String> {
        let spaces = self.spaces.read().unwrap();
        spaces.keys().cloned().collect()
    }

    /// Удаляет пространство
    pub fn drop_space(&self, name: &str) -> Result<(), String> {
        if name == "default" {
            return Err("Cannot drop default space".to_string());
        }

        let mut spaces = self.spaces.write().map_err(|e| e.to_string())?;

        if !spaces.contains_key(name) {
            return Err(format!("Space '{name}' does not exist"));
        }

        spaces.remove(name);
        Ok(())
    }

    /// Валидирует тип узла в текущем пространстве
    pub fn validate_node_type(&self, node_type_name: &str) -> Result<bool, String> {
        let current_space = self.get_current_space();
        let spaces = self.spaces.read().map_err(|e| e.to_string())?;

        let space = spaces
            .get(&current_space)
            .ok_or_else(|| format!("Current space '{current_space}' not found"))?;

        Ok(space.node_types.contains_key(node_type_name))
    }

    /// Валидирует тип ребра в текущем пространстве
    pub fn validate_edge_type(&self, edge_type_name: &str) -> Result<bool, String> {
        let current_space = self.get_current_space();
        let spaces = self.spaces.read().map_err(|e| e.to_string())?;

        let space = spaces
            .get(&current_space)
            .ok_or_else(|| format!("Current space '{current_space}' not found"))?;

        Ok(space.edge_types.contains_key(edge_type_name))
    }

    /// Получает тип узла из текущего пространства
    pub fn get_node_type(&self, node_type_name: &str) -> Option<NodeTypeDef> {
        let current_space = self.get_current_space();
        let spaces = self.spaces.read().ok()?;
        let space = spaces.get(&current_space)?;
        space.node_types.get(node_type_name).cloned()
    }

    /// Получает тип ребра из текущего пространства
    pub fn get_edge_type(&self, edge_type_name: &str) -> Option<EdgeTypeDef> {
        let current_space = self.get_current_space();
        let spaces = self.spaces.read().ok()?;
        let space = spaces.get(&current_space)?;
        space.edge_types.get(edge_type_name).cloned()
    }
}

fn get_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_space() {
        let manager = SpaceManager::new();

        let space_def = SpaceDef {
            name: "test_space".to_string(),
            nodes: vec![],
            edges: vec![],
            streams: vec![],
            config: SpaceConfig {
                shards: 4,
                replication_factor: 1,
                storage_backend: None,
                embedding_model: None,
            },
        };

        assert!(manager.create_space(space_def).is_ok());
        assert!(manager.get_space("test_space").is_some());
    }

    #[test]
    fn test_add_node_type() {
        let manager = SpaceManager::new();

        let node_type = NodeTypeDef {
            name: "Document".to_string(),
            fields: vec![],
        };

        assert!(manager.add_node_type("default", node_type).is_ok());
        assert!(manager.validate_node_type("Document").unwrap());
    }

    #[test]
    fn test_list_spaces() {
        let manager = SpaceManager::new();

        let spaces = manager.list_spaces();
        assert!(spaces.contains(&"default".to_string()));
    }
}
