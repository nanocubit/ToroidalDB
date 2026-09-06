use crate::math::MatryoshkaDim;
use serde::{Deserialize, Serialize};

/// Уровень иерархии матрешки как топологическое пространство
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToroidalLevel {
    D384 = 384,   // Внешний тор (глобальный контекст)
    D768 = 768,   // Средний тор (тематический контекст)
    D1024 = 1024, // Промежуточный тор
    D1536 = 1536, // Внутренний тор (локальный контекст)
}

impl ToroidalLevel {
    pub fn size(&self) -> usize {
        *self as usize
    }

    pub fn from_dim(dim: MatryoshkaDim) -> Self {
        match dim {
            MatryoshkaDim::D384 => ToroidalLevel::D384,
            MatryoshkaDim::D768 => ToroidalLevel::D768,
            MatryoshkaDim::D1024 => ToroidalLevel::D1024,
            MatryoshkaDim::D1536 => ToroidalLevel::D1536,
        }
    }
}

/// Гомотопический класс пути между точками на торе
/// Определяет "как соединены" точки с учётом заворачивания через края
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HomotopyClass {
    /// Прямой путь без заворачивания через края
    Direct,
    /// Путь с заворачиванием по одной или нескольким координатам
    /// Вектор (x, y, z, ...) где каждая компонента = число оборотов по соответствующей оси
    /// Положительное = заворачивание вперёд, отрицательное = назад
    Wrapped(Vec<i32>),
    /// Нетривиальный путь с самопересечением (требует разрезания тора)
    /// Используется для сложных топологических связей
    Nontrivial,
}

impl HomotopyClass {
    /// Вычисляет "стоимость" гомотопического класса (минимальное число заворачиваний)
    pub fn cost(&self) -> usize {
        match self {
            HomotopyClass::Direct => 0,
            HomotopyClass::Wrapped(vec) => vec.iter().map(|&x| x.unsigned_abs() as usize).sum(),
            HomotopyClass::Nontrivial => usize::MAX,
        }
    }
}

/// Меж-торовое ребро: связь между точками РАЗНЫХ уровней иерархии
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterToroidalEdge {
    /// Исходная точка: (уровень тора, идентификатор узла)
    pub source: (ToroidalLevel, u64),
    /// Целевая точка: (уровень тора, идентификатор узла)
    /// Может быть на ДРУГОМ уровне иерархии!
    pub target: (ToroidalLevel, u64),
    /// Семантический тип связи
    pub relation_type: String,
    /// Топологическая дистанция (не евклидова!)
    /// Вычисляется через кратчайший путь с учётом гомотопического класса
    pub topological_distance: f32,
    /// Гомотопический класс пути между точками
    /// Сохраняется как часть данных для восстановления топологии
    pub homotopy_class: HomotopyClass,
    /// Дополнительные свойства ребра (вес, временные метки и т.д.)
    pub properties: serde_json::Value,
}

impl InterToroidalEdge {
    /// Создаёт меж-торовое ребро с автоматическим вычислением гомотопического класса
    pub fn new(
        source: (ToroidalLevel, u64),
        target: (ToroidalLevel, u64),
        relation_type: String,
        source_vector: &[f32],
        target_vector: &[f32],
        properties: serde_json::Value,
    ) -> Self {
        // Вычисляем топологическую дистанцию и гомотопический класс
        let (distance, homotopy) =
            compute_topological_distance(source_vector, target_vector, source.0, target.0);

        Self {
            source,
            target,
            relation_type,
            topological_distance: distance,
            homotopy_class: homotopy,
            properties,
        }
    }

    /// Проверяет, соединяет ли ребро разные уровни иерархии
    pub fn is_inter_level(&self) -> bool {
        self.source.0 != self.target.0
    }
}

/// Вычисляет топологическую дистанцию с учётом всех возможных путей заворачивания
pub fn compute_topological_distance(
    a: &[f32],
    b: &[f32],
    level_a: ToroidalLevel,
    level_b: ToroidalLevel,
) -> (f32, HomotopyClass) {
    // Находим общий уровень для проекции (минимальный общий тор)
    let common_size = level_a.size().min(level_b.size());

    // Проектируем векторы на общий уровень
    let a_proj = if a.len() > common_size {
        &a[..common_size]
    } else {
        a
    };

    let b_proj = if b.len() > common_size {
        &b[..common_size]
    } else {
        b
    };

    // Генерируем все возможные комбинации заворачиваний для каждой координаты
    // Для каждой координаты возможны 3 варианта: без заворачивания, +1 оборот, -1 оборот
    // Для 384 измерений это 3^384 вариантов — НЕВОЗМОЖНО перебрать все
    // Поэтому используем эвристику: заворачиваем ТОЛЬКО координаты с разницей > 0.9

    let _best_distance = f32::MAX;
    let _best_homotopy = HomotopyClass::Direct;
    let _best_wrapping = vec![0i32; common_size];

    // Эвристика: проверяем только координаты с большой разницей
    let critical_coords: Vec<usize> = a_proj
        .iter()
        .zip(b_proj.iter())
        .enumerate()
        .filter(|(_, (&x, &y))| {
            let diff = (x - y).abs();
            diff > 0.9 || (1.0 - diff) > 0.9
        })
        .map(|(i, _)| i)
        .collect();

    // Перебираем все комбинации заворачиваний для критических координат
    // Максимум 10 критических координат → 3^10 = 59049 вариантов (приемлемо)
    let max_critical = 10;
    let critical_coords = if critical_coords.len() > max_critical {
        &critical_coords[..max_critical]
    } else {
        &critical_coords[..]
    };

    // Рекурсивный перебор комбинаций заворачиваний
    fn explore_wrappings(
        a: &[f32],
        b: &[f32],
        critical_coords: &[usize],
        current_wrapping: &mut Vec<i32>,
        idx: usize,
        best: &mut (f32, Vec<i32>),
    ) {
        if idx >= critical_coords.len() {
            // Вычисляем расстояние для текущей комбинации заворачиваний
            let mut wrapped_b = b.to_vec();
            for (coord_idx, &coord) in critical_coords.iter().enumerate() {
                let wrap = current_wrapping[coord_idx];
                if wrap != 0 {
                    wrapped_b[coord] = (wrapped_b[coord] + wrap as f32).rem_euclid(1.0);
                }
            }

            let dist = crate::math::toroidal_distance(a, &wrapped_b);
            if dist < best.0 {
                best.0 = dist;
                best.1.clone_from_slice(current_wrapping);
            }
            return;
        }

        // Три варианта заворачивания: -1, 0, +1
        for wrap in [-1i32, 0, 1] {
            current_wrapping[idx] = wrap;
            explore_wrappings(a, b, critical_coords, current_wrapping, idx + 1, best);
        }
    }

    let mut current_wrapping = vec![0i32; critical_coords.len()];
    let mut best = (f32::MAX, vec![0i32; critical_coords.len()]);

    explore_wrappings(
        a_proj,
        b_proj,
        critical_coords,
        &mut current_wrapping,
        0,
        &mut best,
    );

    // Формируем полный вектор заворачиваний
    let mut full_wrapping = vec![0i32; common_size];
    for (i, &coord) in critical_coords.iter().enumerate() {
        full_wrapping[coord] = best.1[i];
    }

    // Определяем гомотопический класс
    let homotopy = if full_wrapping.iter().all(|&x| x == 0) {
        HomotopyClass::Direct
    } else {
        HomotopyClass::Wrapped(full_wrapping)
    };

    (best.0, homotopy)
}
