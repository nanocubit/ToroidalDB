use serde_json::json;

// Тестовые данные для графовых обходов
pub struct GraphTestData {
    pub users: Vec<User>,
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub vector: Vec<f32>,
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub from_id: u64,
    pub to_id: u64,
    pub relation_type: String,
    pub weight: f32,
}

impl Default for GraphTestData {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphTestData {
    pub fn new() -> Self {
        let users = vec![
            User {
                id: 1,
                name: "Alice".to_string(),
                vector: vec![0.1, 0.2, 0.3],
                properties: json!({"age": 25, "location": "NYC", "interests": ["tech", "music"]}),
            },
            User {
                id: 2,
                name: "Bob".to_string(),
                vector: vec![0.15, 0.25, 0.35],
                properties: json!({"age": 30, "location": "LA", "interests": ["sports", "tech"]}),
            },
            User {
                id: 3,
                name: "Charlie".to_string(),
                vector: vec![0.2, 0.3, 0.4],
                properties: json!({"age": 35, "location": "Chicago", "interests": ["books", "travel"]}),
            },
            User {
                id: 4,
                name: "Diana".to_string(),
                vector: vec![0.25, 0.35, 0.45],
                properties: json!({"age": 28, "location": "Boston", "interests": ["art", "music"]}),
            },
            User {
                id: 5,
                name: "Eve".to_string(),
                vector: vec![0.3, 0.4, 0.5],
                properties: json!({"age": 32, "location": "Seattle", "interests": ["tech", "travel"]}),
            },
        ];

        let connections = vec![
            Connection {
                from_id: 1,
                to_id: 2,
                relation_type: "FOLLOWS".to_string(),
                weight: 0.9,
            },
            Connection {
                from_id: 2,
                to_id: 3,
                relation_type: "FOLLOWS".to_string(),
                weight: 0.8,
            },
            Connection {
                from_id: 1,
                to_id: 4,
                relation_type: "FOLLOWS".to_string(),
                weight: 0.7,
            },
            Connection {
                from_id: 3,
                to_id: 5,
                relation_type: "FOLLOWS".to_string(),
                weight: 0.85,
            },
            Connection {
                from_id: 4,
                to_id: 5,
                relation_type: "FOLLOWS".to_string(),
                weight: 0.75,
            },
            Connection {
                from_id: 1,
                to_id: 3,
                relation_type: "WORKS_WITH".to_string(),
                weight: 0.95,
            },
            Connection {
                from_id: 2,
                to_id: 4,
                relation_type: "FRIENDS_WITH".to_string(),
                weight: 0.88,
            },
        ];

        GraphTestData { users, connections }
    }

    pub fn populate_store(
        &self,
        store: &crate::storage::PersistentStore,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Добавляем пользователей в хранилище
        for user in &self.users {
            let node = crate::storage::Node {
                id: user.id,
                vector: user.vector.clone(),
                properties: user.properties.clone(),
                edges: vec![],
            };
            store.insert(node)?;
        }

        // Добавляем связи
        for conn in &self.connections {
            store.add_edge(
                conn.from_id,
                conn.to_id,
                conn.relation_type.clone(),
                conn.weight,
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::storage::PersistentStore;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_graph_data_population() {
        let store = Arc::new(PersistentStore::open("./graph_data_test").unwrap());
        let test_data = GraphTestData::new();

        // Заполняем хранилище тестовыми данными
        test_data.populate_store(&store).unwrap();

        // Проверяем, что все пользователи добавлены
        for user in &test_data.users {
            let retrieved = store.get(user.id).unwrap();
            assert!(retrieved.is_some());
            let node = retrieved.unwrap();
            assert_eq!(node.id, user.id);
        }

        // Проверяем, что у первого пользователя есть связи
        let alice_node = store.get(1).unwrap().unwrap();
        assert!(alice_node.edges.len() >= 2); // У Alice должно быть минимум 2 связи

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./graph_data_test").ok();
    }
}
