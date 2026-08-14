//! # AI Assistant для GuiTor
//! 
//! Интеграция с bgpt/Ollama для AI помощи с SQL

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::Command;

/// AI Assistant Manager
pub struct AiAssistant {
    client: Client,
    bgpt_path: String,
    model: String,
    ollama_url: String,
}

/// AI запрос
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub query: String,
    pub context: Option<String>,
    pub sql: Option<String>,
    pub schema: Option<String>,
}

/// AI ответ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub text: String,
    pub sql: Option<String>,
    pub explanation: Option<String>,
}

impl AiAssistant {
    pub fn new(bgpt_path: &str, model: &str, ollama_url: &str) -> Self {
        Self {
            client: Client::new(),
            bgpt_path: bgpt_path.to_string(),
            model: model.to_string(),
            ollama_url: ollama_url.to_string(),
        }
    }

    /// Запрос к bgpt (локальная LLM)
    pub async fn query_bgpt(&self, request: &AiRequest) -> Result<AiResponse> {
        let prompt = self.build_prompt(request);

        let output = Command::new(&self.bgpt_path)
            .arg("--model")
            .arg(&self.model)
            .arg("--stream")
            .arg("--quiet")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()?
            .wait_with_output()?;

        let text = String::from_utf8_lossy(&output.stdout).to_string();

        // Пытаемся извлечь SQL из ответа
        let sql = self.extract_sql(&text);

        Ok(AiResponse {
            text,
            sql,
            explanation: None,
        })
    }

    /// Запрос к Ollama API
    pub async fn query_ollama(&self, request: &AiRequest) -> Result<AiResponse> {
        let prompt = self.build_prompt(request);

        let response = self.client
            .post(&format!("{}/api/generate", self.ollama_url))
            .json(&serde_json::json!({
                "model": &self.model,
                "prompt": prompt,
                "stream": false,
            }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let text = response["response"].as_str().unwrap_or("").to_string();
        let sql = self.extract_sql(&text);

        Ok(AiResponse {
            text,
            sql,
            explanation: None,
        })
    }

    /// Строит промпт для AI
    fn build_prompt(&self, request: &AiRequest) -> String {
        let mut prompt = String::from("You are an expert SQL database administrator. ");

        if let Some(schema) = &request.schema {
            prompt.push_str(&format!("Current database schema:\n{}\n\n", schema));
        }

        if let Some(sql) = &request.sql {
            prompt.push_str(&format!("Current SQL query:\n{}\n\n", sql));
        }

        if let Some(context) = &request.context {
            prompt.push_str(&format!("Context: {}\n\n", context));
        }

        prompt.push_str(&request.query);
        prompt.push_str("\n\nProvide a clear, concise answer. If generating SQL, format it properly.");

        prompt
    }

    /// Извлекает SQL из ответа
    fn extract_sql(&self, text: &str) -> Option<String> {
        // Ищем SQL в markdown code blocks
        if let Some(start) = text.find("```sql") {
            let start = start + 6;
            if let Some(end) = text[start..].find("```") {
                return Some(text[start..start + end].trim().to_string());
            }
        }
        
        // Или просто code blocks
        if let Some(start) = text.find("```") {
            let start = start + 3;
            if let Some(end) = text[start..].find("```") {
                return Some(text[start..start + end].trim().to_string());
            }
        }

        None
    }

    /// Объясняет SQL запрос
    pub async fn explain_sql(&self, sql: &str, schema: &str) -> Result<AiResponse> {
        self.query_bgpt(&AiRequest {
            query: "Explain what this SQL query does step by step".to_string(),
            context: None,
            sql: Some(sql.to_string()),
            schema: Some(schema.to_string()),
        }).await
    }

    /// Оптимизирует SQL запрос
    pub async fn optimize_sql(&self, sql: &str, schema: &str) -> Result<AiResponse> {
        self.query_bgpt(&AiRequest {
            query: "Optimize this SQL query for better performance. Explain the changes.".to_string(),
            context: None,
            sql: Some(sql.to_string()),
            schema: Some(schema.to_string()),
        }).await
    }

    /// Генерирует SQL по описанию
    pub async fn generate_sql(&self, description: &str, schema: &str) -> Result<AiResponse> {
        self.query_bgpt(&AiRequest {
            query: format!("Generate SQL query for: {}", description),
            context: None,
            sql: None,
            schema: Some(schema.to_string()),
        }).await
    }

    /// Создаёт миграцию
    pub async fn create_migration(&self, description: &str, current_schema: &str) -> Result<AiResponse> {
        self.query_bgpt(&AiRequest {
            query: format!("Create a database migration for: {}. Current schema:\n{}", description, current_schema),
            context: None,
            sql: None,
            schema: None,
        }).await
    }

    /// Проверяет SQL на ошибки
    pub async fn validate_sql(&self, sql: &str, dialect: &str) -> Result<AiResponse> {
        self.query_bgpt(&AiRequest {
            query: format!("Validate this {} SQL for syntax errors and suggest fixes:", dialect),
            context: None,
            sql: Some(sql.to_string()),
            schema: None,
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_extract_sql() {
        let assistant = AiAssistant::new("", "", "");
        
        let text = "Here's the query:\n```sql\nSELECT * FROM users;\n```";
        let sql = assistant.extract_sql(text);
        
        assert_eq!(sql, Some("SELECT * FROM users;".to_string()));
    }

    #[test]
    fn test_build_prompt() {
        let assistant = AiAssistant::new("", "", "");
        
        let request = AiRequest {
            query: "Explain this".to_string(),
            context: Some("test".to_string()),
            sql: Some("SELECT 1".to_string()),
            schema: None,
        };

        let prompt = assistant.build_prompt(&request);
        assert!(prompt.contains("Explain this"));
        assert!(prompt.contains("SELECT 1"));
    }
}
