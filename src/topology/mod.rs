//! Топологическая алгебра для операций над торами
//! Основано на фундаментальной группе π₁(𝕋ⁿ) = ℤⁿ и гомотопических инвариантах

pub mod algebra;
pub mod edges;
pub mod functions;
pub mod metrics;
pub mod ricci_flow;
pub mod topological_indices;
pub mod topology_metrics;

pub use algebra::*;
pub use edges::*;
pub use functions::*;
pub use metrics::*;
pub use ricci_flow::*;
pub use topological_indices::*;
pub use topology_metrics::{TopologyChanges, TopologyMetricsCollector, TopologyMetricsSnapshot};
