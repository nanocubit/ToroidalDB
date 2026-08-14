#[cfg(test)]
mod mcp_cli_tests {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::mcp::{
        MCPHandler, ModelConfiguration, OptimizationSettings, PerformanceSettings,
        SystemConfiguration, TopologySettings,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn test_system_configuration_creation() {
        let mcp = MCPHandler::new();

        // Получаем конфигурацию по умолчанию
        let config = mcp.get_system_config().await;

        // Проверяем, что значения по умолчанию установлены правильно
        assert_eq!(config.server_port, 8443);
        assert_eq!(config.backup_retention_days, 7);
        assert_eq!(config.cache_size, 1000);
        assert_eq!(config.enable_metrics, true);

        println!("✅ System configuration creation test passed");
    }

    #[tokio::test]
    async fn test_system_configuration_update() {
        let mcp = MCPHandler::new();

        // Создаем новую конфигурацию
        let mut new_config = SystemConfiguration::default();
        new_config.server_port = 9443;
        new_config.cache_size = 2000;
        new_config.backup_retention_days = 14;

        // Обновляем конфигурацию
        mcp.update_system_config(new_config.clone()).await.unwrap();

        // Проверяем, что изменения сохранились
        let updated_config = mcp.get_system_config().await;
        assert_eq!(updated_config.server_port, 9443);
        assert_eq!(updated_config.cache_size, 2000);
        assert_eq!(updated_config.backup_retention_days, 14);

        println!("✅ System configuration update test passed");
    }

    #[tokio::test]
    async fn test_model_configuration_management() {
        let mcp = MCPHandler::new();

        // Создаем конфигурацию модели
        let model_config = ModelConfiguration {
            name: "test_model".to_string(),
            version: "1.0.0".to_string(),
            dimensions: vec![384, 768],
            default_threshold: 0.3,
            optimization_settings: OptimizationSettings {
                enable_caching: true,
                cache_strategy: "LRU".to_string(),
                enable_parallel_search: true,
                early_termination: true,
                batch_size: 100,
            },
            topology_settings: TopologySettings {
                enable_toroidal_topology: true,
                enable_ricci_flow: false,
                ricci_iterations: 50,
                homotopy_class_tracking: true,
            },
            performance_settings: PerformanceSettings {
                thread_pool_size: 4,
                memory_limit_mb: 1024,
                disk_cache_enabled: true,
                disk_cache_path: "./cache".to_string(),
            },
        };

        // Устанавливаем конфигурацию модели
        mcp.set_model_config(model_config.clone()).await.unwrap();

        // Получаем конфигурацию модели
        let retrieved_config = mcp.get_model_config("test_model").await.unwrap();

        // Проверяем, что конфигурация сохранена правильно
        assert_eq!(retrieved_config.name, "test_model");
        assert_eq!(retrieved_config.version, "1.0.0");
        assert_eq!(retrieved_config.dimensions, vec![384, 768]);
        assert_eq!(retrieved_config.default_threshold, 0.3);
        assert_eq!(retrieved_config.optimization_settings.enable_caching, true);
        assert_eq!(
            retrieved_config.topology_settings.enable_toroidal_topology,
            true
        );

        // Проверяем список моделей
        let models_list = mcp.list_models().await;
        assert!(models_list.contains(&"test_model".to_string()));

        println!("✅ Model configuration management test passed");
    }

    #[tokio::test]
    async fn test_configuration_validation() {
        let mcp = MCPHandler::new();

        // Создаем действительную конфигурацию
        let mut valid_config = SystemConfiguration::default();
        valid_config.server_port = 8444; // Допустимый порт

        // Создаем файлы сертификатов для теста
        std::fs::write("test_cert.pem", "fake cert").unwrap();
        std::fs::write("test_key.pem", "fake key").unwrap();

        valid_config.tls_cert_path = "test_cert.pem".to_string();
        valid_config.tls_key_path = "test_key.pem".to_string();

        mcp.update_system_config(valid_config).await.unwrap();

        // Проверяем валидацию
        let is_valid = mcp.validate_config().await.unwrap();
        assert!(is_valid);

        // Удаляем временные файлы
        std::fs::remove_file("test_cert.pem").ok();
        std::fs::remove_file("test_key.pem").ok();

        println!("✅ Configuration validation test passed");
    }

    #[tokio::test]
    async fn test_apply_configuration_changes() {
        let mcp = MCPHandler::new();

        // Проверяем применение изменений конфигурации
        let result = mcp.apply_config_changes().await;
        assert!(result.is_ok());

        println!("✅ Apply configuration changes test passed");
    }

    #[tokio::test]
    async fn test_model_configuration_updates() {
        let mcp = MCPHandler::new();

        // Создаем и устанавливаем модель
        let model_config = ModelConfiguration {
            name: "update_test_model".to_string(),
            version: "1.0.0".to_string(),
            dimensions: vec![384],
            default_threshold: 0.3,
            optimization_settings: OptimizationSettings {
                enable_caching: true,
                cache_strategy: "LRU".to_string(),
                enable_parallel_search: true,
                early_termination: true,
                batch_size: 100,
            },
            topology_settings: TopologySettings {
                enable_toroidal_topology: true,
                enable_ricci_flow: false,
                ricci_iterations: 50,
                homotopy_class_tracking: true,
            },
            performance_settings: PerformanceSettings {
                thread_pool_size: 4,
                memory_limit_mb: 1024,
                disk_cache_enabled: true,
                disk_cache_path: "./cache".to_string(),
            },
        };

        mcp.set_model_config(model_config).await.unwrap();

        // Обновляем только некоторые параметры
        use std::collections::HashMap;
        let mut updates = HashMap::new();
        updates.insert(
            "version".to_string(),
            serde_json::Value::String("2.0.0".to_string()),
        );
        updates.insert(
            "default_threshold".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(0.25).unwrap()),
        );

        mcp.update_model_config("update_test_model", updates)
            .await
            .unwrap();

        // Проверяем обновленные параметры
        let updated_model = mcp.get_model_config("update_test_model").await.unwrap();
        assert_eq!(updated_model.version, "2.0.0");
        assert_eq!(updated_model.default_threshold, 0.25);
        // Проверяем, что другие параметры остались без изменений
        assert_eq!(updated_model.dimensions, vec![384]);

        println!("✅ Model configuration update test passed");
    }

    #[tokio::test]
    async fn test_reset_to_defaults() {
        let mcp = MCPHandler::new();

        // Обновляем конфигурацию
        let mut new_config = SystemConfiguration::default();
        new_config.server_port = 9999;
        new_config.cache_size = 9999;
        mcp.update_system_config(new_config).await.unwrap();

        // Сбрасываем к значениям по умолчанию
        mcp.reset_to_defaults().await.unwrap();

        // Проверяем, что значения вернулись к умолчаниям
        let config = mcp.get_system_config().await;
        assert_eq!(config.server_port, 8443); // значение по умолчанию
        assert_eq!(config.cache_size, 1000); // значение по умолчанию

        println!("✅ Reset to defaults test passed");
    }

    #[tokio::test]
    async fn test_multiple_models() {
        let mcp = MCPHandler::new();

        // Создаем несколько моделей
        let model1 = ModelConfiguration {
            name: "model_a".to_string(),
            version: "1.0.0".to_string(),
            dimensions: vec![384],
            default_threshold: 0.3,
            optimization_settings: OptimizationSettings {
                enable_caching: true,
                cache_strategy: "LRU".to_string(),
                enable_parallel_search: true,
                early_termination: true,
                batch_size: 100,
            },
            topology_settings: TopologySettings {
                enable_toroidal_topology: true,
                enable_ricci_flow: false,
                ricci_iterations: 50,
                homotopy_class_tracking: true,
            },
            performance_settings: PerformanceSettings {
                thread_pool_size: 4,
                memory_limit_mb: 1024,
                disk_cache_enabled: true,
                disk_cache_path: "./cache".to_string(),
            },
        };

        let model2 = ModelConfiguration {
            name: "model_b".to_string(),
            version: "2.1.0".to_string(),
            dimensions: vec![768, 1536],
            default_threshold: 0.2,
            optimization_settings: OptimizationSettings {
                enable_caching: false,
                cache_strategy: "FIFO".to_string(),
                enable_parallel_search: true,
                early_termination: false,
                batch_size: 50,
            },
            topology_settings: TopologySettings {
                enable_toroidal_topology: false,
                enable_ricci_flow: true,
                ricci_iterations: 100,
                homotopy_class_tracking: false,
            },
            performance_settings: PerformanceSettings {
                thread_pool_size: 8,
                memory_limit_mb: 2048,
                disk_cache_enabled: false,
                disk_cache_path: "".to_string(),
            },
        };

        mcp.set_model_config(model1).await.unwrap();
        mcp.set_model_config(model2).await.unwrap();

        // Проверяем, что обе модели сохранены
        assert!(mcp.get_model_config("model_a").await.is_some());
        assert!(mcp.get_model_config("model_b").await.is_some());

        // Проверяем список моделей
        let models = mcp.list_models().await;
        assert!(models.contains(&"model_a".to_string()));
        assert!(models.contains(&"model_b".to_string()));
        assert_eq!(models.len(), 2);

        println!("✅ Multiple models test passed");
    }

    #[tokio::test]
    async fn test_invalid_port_validation() {
        let mcp = MCPHandler::new();

        // Создаем конфигурацию с недопустимым портом
        let mut invalid_config = SystemConfiguration::default();
        invalid_config.server_port = 100; // Недопустимый порт (ниже 1024)

        mcp.update_system_config(invalid_config).await.unwrap();

        // Проверяем, что валидация обнаруживает ошибку
        let validation_result = mcp.validate_config().await;
        assert!(validation_result.is_err());

        println!("✅ Invalid port validation test passed");
    }

    #[tokio::test]
    async fn test_cache_size_validation() {
        let mcp = MCPHandler::new();

        // Создаем конфигурацию с недопустимым размером кэша
        let mut invalid_config = SystemConfiguration::default();
        invalid_config.cache_size = 0; // Недопустимый размер кэша

        mcp.update_system_config(invalid_config).await.unwrap();

        // Проверяем, что валидация обнаруживает ошибку
        let validation_result = mcp.validate_config().await;
        assert!(validation_result.is_err());

        println!("✅ Cache size validation test passed");
    }
}
