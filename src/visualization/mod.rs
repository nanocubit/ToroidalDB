//! Модуль визуализации для ToroidalDB
//! 
//! Содержит:
//! - Графовые визуализации
//! - Топологические карты
//! - Векторные проекции (TSNE, UMAP)
//! - Интеграцию с BI-инструментами
//! - ETL-функции для потоковой обработки

pub mod graph_visualizer;
pub mod topology_mapper;
pub mod vector_projector;
pub mod bi_connectors;
pub mod etl_pipeline;

pub use graph_visualizer::*;
pub use topology_mapper::*;
pub use vector_projector::*;
pub use bi_connectors::*;
pub use etl_pipeline::*;