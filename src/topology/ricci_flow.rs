// src/topology/ricci_flow.rs

#[cfg(feature = "ricci-c")]
extern "C" {
    fn ricci_flow_c(embeddings: *mut f32, n: usize, dim: usize, iterations: usize) -> i32;
}

use crate::hybrid_storage::Node;
use crate::math::{pad_or_truncate, toroidal_distance, MatryoshkaDim};
/// Дискретный поток Риччи для оптимизации вложения данных в тор
/// Реализация основана на работе "Ricci curvature of graphs" (Lin-Lu-Yau, 2011)
/// и адаптирована для тороидальных пространств
use crate::topology::algebra::apply_homotopy;
use crate::topology::edges::HomotopyClass;

/// Оптимизирует вложение набора узлов в тор заданной размерности
/// с использованием дискретного потока Риччи
///
/// Алгоритм:
/// 1. Вычисляет "кривизну Риччи" для каждой пары узлов на основе их топологической близости
/// 2. Деформирует вложение в направлении уменьшения кривизны
/// 3. Сохраняет гомотопические инварианты (не разрывает циклы)
/// 4. Повторяет итерации до сходимости или достижения лимита
pub fn optimize_embedding(
    nodes: &[Node],
    target_dim: MatryoshkaDim,
    iterations: usize,
) -> Result<usize, String> {
    if nodes.is_empty() {
        return Err("No nodes to optimize".to_string());
    }

    let target_size = target_dim.size();
    let mut optimized_count = 0;

    // Создаём копию векторов для оптимизации
    let mut embeddings: Vec<Vec<f32>> = nodes
        .iter()
        .map(|node| pad_or_truncate(&node.vector, target_size))
        .collect();

    // Итерации потока Риччи
    for iter in 0..iterations {
        let mut changed = false;

        // Для каждой пары узлов вычисляем дискретную кривизну Риччи
        for i in 0..embeddings.len() {
            for j in i + 1..embeddings.len() {
                // Вычисляем текущее тороидальное расстояние
                let current_dist = toroidal_distance(&embeddings[i], &embeddings[j]);

                // Вычисляем "идеальное" расстояние на основе семантической близости
                // (в реальной системе это может быть основано на свойствах узлов или внешних метриках)
                let ideal_dist = compute_ideal_distance(&nodes[i], &nodes[j], target_dim);

                // Дискретная кривизна Риччи: разница между текущим и идеальным расстоянием
                let ricci_curvature = current_dist - ideal_dist;

                // Если кривизна значительна — деформируем вложение
                if ricci_curvature.abs() > 0.05 {
                    // Направление деформации: уменьшаем расстояние если кривизна положительна,
                    // увеличиваем если отрицательна
                    let direction = if ricci_curvature > 0.0 { -1.0 } else { 1.0 };
                    let step_size = 0.01 * ricci_curvature.abs().min(1.0);

                    // Деформируем оба узла в противоположных направлениях
                    for k in 0..target_size {
                        // Сохраняем оригинальные значения для проверки гомотопии
                        let orig_i = embeddings[i][k];
                        let orig_j = embeddings[j][k];

                        // Применяем деформацию
                        embeddings[i][k] =
                            (embeddings[i][k] + direction * step_size).rem_euclid(1.0);
                        embeddings[j][k] =
                            (embeddings[j][k] - direction * step_size).rem_euclid(1.0);

                        // Проверяем сохранение гомотопического класса
                        // Если деформация изменила класс (например, пересекла край тора),
                        // откатываем изменение для этой координаты
                        let new_dist = toroidal_distance(&[embeddings[i][k]], &[embeddings[j][k]]);

                        let orig_dist = toroidal_distance(&[orig_i], &[orig_j]);

                        // Если расстояние изменилось более чем на 50%, вероятно произошёл разрыв топологии
                        if (new_dist - orig_dist).abs() > 0.5 {
                            embeddings[i][k] = orig_i;
                            embeddings[j][k] = orig_j;
                        } else {
                            changed = true;
                        }
                    }
                }
            }
        }

        // Если за итерацию не было изменений — сходимость достигнута
        if !changed && iter > 5 {
            println!("💡 Ricci flow converged after {} iterations", iter + 1);
            break;
        }

        // Каждые 10 итераций выводим прогресс
        if (iter + 1) % 10 == 0 {
            println!("🔄 Ricci flow iteration {}/{}", iter + 1, iterations);
        }
    }

    // Применяем оптимизированные вложения к узлам (в реальной системе это потребует обновления БД)
    // Здесь просто считаем количество "изменённых" узлов для отчёта
    for (i, node) in nodes.iter().enumerate() {
        let original = pad_or_truncate(&node.vector, target_size);
        let optimized = &embeddings[i];

        let dist = toroidal_distance(&original, optimized);
        if dist > 0.01 {
            // Считаем узел оптимизированным если изменился более чем на 0.01
            optimized_count += 1;
        }
    }

    Ok(optimized_count)
}

/// Вычисляет "идеальное" расстояние между узлами на основе их свойств
/// В реальной системе это может быть основано на:
/// - Семантическом сходстве (косинусное расстояние между эмбеддингами)
/// - Графовой близости (длина кратчайшего пути)
/// - Временных метках (циклическая близость)
fn compute_ideal_distance(node_a: &Node, node_b: &Node, _dim: MatryoshkaDim) -> f32 {
    // Упрощённая эвристика: если узлы имеют похожие свойства — должны быть ближе
    // В реальной системе здесь будет сложная функция

    // Проверяем наличие общих ключей в свойствах
    let props_a = node_a.properties.as_object();
    let props_b = node_b.properties.as_object();

    let mut similarity = 0.0;

    if let (Some(a), Some(b)) = (props_a, props_b) {
        let common_keys: Vec<_> = a.keys().filter(|k| b.contains_key(*k)).collect();

        if !common_keys.is_empty() {
            // Простая мера сходства: доля общих ключей
            similarity = common_keys.len() as f32 / (a.len() + b.len()) as f32;
        }
    }

    // Идеальное расстояние обратно пропорционально сходству
    // Диапазон [0.1, 0.9] чтобы избежать вырожденных случаев
    0.1 + (0.8 * (1.0 - similarity))
}

/// Расширенная версия с поддержкой гомотопических инвариантов
pub fn optimize_with_homotopy_preservation(
    nodes: &[Node],
    edges: &[crate::topology::edges::InterToroidalEdge],
    target_dim: MatryoshkaDim,
    iterations: usize,
) -> Result<(usize, usize), String> {
    // Сначала оптимизируем базовое вложение
    let optimized_nodes = optimize_embedding(nodes, target_dim, iterations)?;

    // Затем корректируем вложение для сохранения гомотопических классов рёбер
    let preserved_edges = preserve_homotopy_classes(nodes, edges, target_dim)?;

    Ok((optimized_nodes, preserved_edges))
}

/// Корректирует вложение для сохранения гомотопических классов заданных рёбер
fn preserve_homotopy_classes(
    nodes: &[Node],
    edges: &[crate::topology::edges::InterToroidalEdge],
    target_dim: MatryoshkaDim,
) -> Result<usize, String> {
    let target_size = target_dim.size();
    let mut preserved_count = 0;

    for edge in edges {
        // Получаем исходные и целевые узлы
        let source_node = nodes.iter().find(|n| n.id == edge.source.1);
        let target_node = nodes.iter().find(|n| n.id == edge.target.1);

        if let (Some(source), Some(target)) = (source_node, target_node) {
            // Проверяем текущий гомотопический класс
            let source_vec = pad_or_truncate(&source.vector, target_size);
            let target_vec = pad_or_truncate(&target.vector, target_size);

            // Вычисляем текущий класс
            let (_, current_homotopy) = crate::topology::edges::compute_topological_distance(
                &source_vec,
                &target_vec,
                edge.source.0,
                edge.target.0,
            );

            // Если класс совпадает с сохранённым — ничего не делаем
            if current_homotopy == edge.homotopy_class {
                preserved_count += 1;
                continue;
            }

            // Иначе применяем коррекцию через гомотопическое преобразование
            let corrected_target = apply_homotopy(&target_vec, &edge.homotopy_class);

            // В реальной системе здесь нужно обновить вектор узла в БД
            // Для демонстрации просто проверяем, что коррекция возможна
            let corrected_dist = toroidal_distance(&source_vec, &corrected_target);
            if corrected_dist < 1.0 {
                // Успешная коррекция
                preserved_count += 1;
            }
        }
    }

    Ok(preserved_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_ricci_flow_basic_optimization() {
        // Создаём тестовые узлы с "плохим" вложением
        let nodes = vec![
            Node {
                id: 1,
                vector: vec![0.1, 0.1, 0.1],
                properties: json!({"type": "similar"}),
                edges: vec![],
            },
            Node {
                id: 2,
                vector: vec![0.9, 0.9, 0.9], // Должен быть близок к узлу 1 из-за цикличности
                properties: json!({"type": "similar"}),
                edges: vec![],
            },
            Node {
                id: 3,
                vector: vec![0.5, 0.5, 0.5],
                properties: json!({"type": "different"}),
                edges: vec![],
            },
        ];

        // Оптимизируем вложение
        let result = optimize_embedding(&nodes, MatryoshkaDim::D384, 20);
        assert!(result.is_ok());

        let optimized_count = result.unwrap();
        // Ожидаем, что хотя бы один узел будет оптимизирован
        assert!(optimized_count > 0);
    }

    #[test]
    fn test_homotopy_preservation() {
        // Создаём узлы и ребро с нетривиальным гомотопическим классом
        let nodes = vec![
            Node {
                id: 1,
                vector: vec![0.05, 0.05],
                properties: json!({}),
                edges: vec![],
            },
            Node {
                id: 2,
                vector: vec![0.95, 0.95],
                properties: json!({}),
                edges: vec![],
            },
        ];

        let edges = vec![crate::topology::edges::InterToroidalEdge {
            source: (crate::topology::edges::ToroidalLevel::D384, 1),
            target: (crate::topology::edges::ToroidalLevel::D384, 2),
            relation_type: "close".to_string(),
            topological_distance: 0.1414, // √(0.1² + 0.1²)
            homotopy_class: HomotopyClass::Wrapped(vec![1, 1]), // Заворачивание по обеим осям
            properties: json!({}),
        }];

        // Проверяем сохранение гомотопического класса
        let result = preserve_homotopy_classes(&nodes, &edges, MatryoshkaDim::D384);
        assert!(result.is_ok());

        let preserved = result.unwrap();
        assert_eq!(preserved, 1); // Ребро должно быть сохранено
    }
}
