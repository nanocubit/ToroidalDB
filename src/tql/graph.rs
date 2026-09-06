use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::ast::{ConnectedClause, WithinClause};
use crate::tql::executor::QueryResult;
use std::collections::{HashMap, HashSet, VecDeque};

pub struct GraphOperations;

impl GraphOperations {
    /// Реализует bidirectional BFS для поиска узлов в пределах заданного количества шагов
    pub async fn bidirectional_bfs(
        store: &HybridPersistentStore,
        start_nodes: &[u64],
        min_hops: u32,
        max_hops: u32,
        rel_type: &str,
    ) -> Result<Vec<u64>, String> {
        // Инициализируем очереди и хэш-таблицы для обхода
        let mut forward_queue: VecDeque<(u64, u32)> = VecDeque::new();
        let mut backward_queue: VecDeque<(u64, u32)> = VecDeque::new();

        let mut forward_visited: HashMap<u64, u32> = HashMap::new();
        let mut backward_visited: HashMap<u64, u32> = HashMap::new();

        // Инициализация прямого обхода
        for &node_id in start_nodes {
            forward_queue.push_back((node_id, 0));
            forward_visited.insert(node_id, 0);
        }

        // Инициализация обратного обхода - начинаем с целевых узлов
        // В упрощенной реализации считаем, что целевые узлы - это все узлы в хранилище
        let all_nodes = store
            .get_all()
            .map_err(|e| format!("Failed to get all nodes: {e}"))?;

        for node in &all_nodes {
            // В реальной системе здесь будет фильтрация по метке или свойствам
            // Пока добавляем все узлы в обратный обход
            backward_queue.push_back((node.id, 0));
            backward_visited.insert(node.id, 0);
        }

        let mut meeting_points = Vec::new();

        // Выполняем BFS в обоих направлениях
        while !forward_queue.is_empty() || !backward_queue.is_empty() {
            // Шаг вперед (если не превышена максимальная глубина)
            if !forward_queue.is_empty() && forward_queue.front().unwrap().1 <= max_hops {
                if let Some((current_node, current_depth)) = forward_queue.pop_front() {
                    if current_depth >= max_hops {
                        continue;
                    }

                    // Получаем соседей текущего узла
                    match store.get_neighbors(current_node) {
                        Ok(neighbors) => {
                            for neighbor in neighbors {
                                // Проверяем, является ли ребро нужного типа
                                if Self::has_relationship_of_type(&neighbor, rel_type) {
                                    let new_depth = current_depth + 1;

                                    // Проверяем, не встретились ли мы с обратным обходом
                                    if let Some(&backward_depth) =
                                        backward_visited.get(&neighbor.id)
                                    {
                                        let total_hops = new_depth + backward_depth;
                                        if total_hops >= min_hops && total_hops <= max_hops {
                                            meeting_points.push(neighbor.id);
                                        }
                                    }

                                    // Добавляем в очередь, если еще не посещали на этом уровне
                                    if !forward_visited.contains_key(&neighbor.id)
                                        || forward_visited[&neighbor.id] > new_depth
                                    {
                                        forward_visited.insert(neighbor.id, new_depth);
                                        if new_depth <= max_hops {
                                            forward_queue.push_back((neighbor.id, new_depth));
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => return Err(format!("Failed to get neighbors: {e}")),
                    }
                }
            }

            // Шаг назад (если не превышена максимальная глубина)
            if !backward_queue.is_empty() && backward_queue.front().unwrap().1 <= max_hops {
                if let Some((current_node, current_depth)) = backward_queue.pop_front() {
                    if current_depth >= max_hops {
                        continue;
                    }

                    // Получаем соседей текущего узла
                    match store.get_neighbors(current_node) {
                        Ok(neighbors) => {
                            for neighbor in neighbors {
                                // Проверяем, является ли ребро нужного типа
                                if Self::has_relationship_of_type(&neighbor, rel_type) {
                                    let new_depth = current_depth + 1;

                                    // Проверяем, не встретились ли мы с прямым обходом
                                    if let Some(&forward_depth) = forward_visited.get(&neighbor.id)
                                    {
                                        let total_hops = forward_depth + new_depth;
                                        if total_hops >= min_hops && total_hops <= max_hops {
                                            meeting_points.push(neighbor.id);
                                        }
                                    }

                                    // Добавляем в очередь, если еще не посещали на этом уровне
                                    if !backward_visited.contains_key(&neighbor.id)
                                        || backward_visited[&neighbor.id] > new_depth
                                    {
                                        backward_visited.insert(neighbor.id, new_depth);
                                        if new_depth <= max_hops {
                                            backward_queue.push_back((neighbor.id, new_depth));
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => return Err(format!("Failed to get neighbors: {e}")),
                    }
                }
            }
        }

        // Убираем дубликаты
        let mut unique_meeting_points = HashSet::new();
        for point in meeting_points {
            unique_meeting_points.insert(point);
        }

        Ok(unique_meeting_points.into_iter().collect())
    }

    /// Проверяет, имеет ли узел ребро заданного типа
    fn has_relationship_of_type(node: &crate::hybrid_storage::Node, rel_type: &str) -> bool {
        // Проверяем, есть ли у узла ребро с указанным типом связи
        node.edges.iter().any(|edge| edge.relation_type == rel_type)
    }

    /// Находит узлы, соединённые с заданными узлами через указанную связь
    pub async fn find_connected_nodes(
        store: &HybridPersistentStore,
        start_nodes: &[u64],
        connected_clause: &ConnectedClause,
        within_clause: &WithinClause,
    ) -> Result<Vec<QueryResult>, String> {
        // Выполняем bidirectional BFS для поиска узлов в пределах hops
        let connected_ids = Self::bidirectional_bfs(
            store,
            start_nodes,
            within_clause.min_hops,
            within_clause.max_hops,
            &connected_clause.relationship_type,
        )
        .await?;

        // Преобразуем ID в QueryResult
        let mut results = Vec::new();
        for node_id in connected_ids {
            if let Ok(Some(node)) = store.get(node_id) {
                // Проверяем, соответствует ли узел фильтру по свойствам
                if Self::matches_property_filter(&node, &connected_clause.property_filter) {
                    // В реальной системе оценка будет зависеть от топологического расстояния
                    let score =
                        f32::midpoint(within_clause.min_hops as f32, within_clause.max_hops as f32);
                    results.push(QueryResult {
                        id: node.id,
                        score,
                        properties: node.properties,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Проверяет, соответствует ли узел фильтру по свойствам
    fn matches_property_filter(
        node: &crate::hybrid_storage::Node,
        filter: &Option<(String, crate::tql::ast::PropertyValue)>,
    ) -> bool {
        match filter {
            Some((property_name, expected_value)) => {
                // Проверяем, есть ли свойство у узла
                if let Some(value) = node.properties.get(property_name) {
                    // Сравниваем значения (упрощённая реализация)
                    match expected_value {
                        crate::tql::ast::PropertyValue::String(expected_str) => {
                            value.as_str().is_some_and(|s| s == expected_str)
                        }
                        crate::tql::ast::PropertyValue::Number(expected_num) => value
                            .as_f64()
                            .is_some_and(|n| (n - expected_num).abs() < f64::EPSILON),
                        crate::tql::ast::PropertyValue::Boolean(expected_bool) => {
                            value.as_bool() == Some(*expected_bool)
                        }
                    }
                } else {
                    false
                }
            }
            None => true, // Если фильтр не задан, узел подходит
        }
    }
}
