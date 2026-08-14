//! # SQL Autocomplete Service для GuiTor
//! 
//! Интеллектуальное автодополнение SQL с использованием sqlparser

use sqlparser::ast::*;
use sqlparser::dialect::{Dialect, GenericDialect, PostgreSqlDialect, MySqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, TokenType};
use serde::{Deserialize, Serialize};

/// Предложение для автодополнения
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub sort_text: Option<String>,
    pub insert_text: Option<String>,
}

/// Тип предложения
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CompletionKind {
    Keyword,
    Schema,
    Table,
    View,
    Column,
    Function,
    Alias,
    Value,
    Operator,
}

/// Контекст для автодополнения
pub struct CompletionContext {
    pub sql: String,
    pub position: usize,
    pub schemas: Vec<String>,
    pub tables: Vec<TableInfo>,
    pub columns: Vec<ColumnInfo>,
    pub functions: Vec<FunctionInfo>,
    pub aliases: Vec<String>,
}

/// Информация о таблице
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub schema: String,
    pub columns: Vec<String>,
}

/// Информация о столбце
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub table: String,
    pub column: String,
    pub data_type: String,
}

/// Информация о функции
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub args: Vec<String>,
    pub return_type: String,
}

/// SQL Autocomplete Service
pub struct SqlAutocompleteService {
    dialect: Box<dyn Dialect>,
}

impl SqlAutocompleteService {
    pub fn new(dialect_name: &str) -> Self {
        let dialect: Box<dyn Dialect> = match dialect_name.to_lowercase().as_str() {
            "postgresql" | "postgres" => Box::new(PostgreSqlDialect {}),
            "mysql" => Box::new(MySqlDialect {}),
            "sqlite" => Box::new(SQLiteDialect {}),
            _ => Box::new(GenericDialect {}),
        };

        Self { dialect }
    }

    /// Получает предложения для автодополнения
    pub fn get_completions(&self, context: &CompletionContext) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        // 1. SQL Keywords
        items.extend(self.get_keyword_completions());

        // 2. Определяем контекст запроса
        if let Ok(statements) = Parser::parse_sql(&*self.dialect, &context.sql) {
            if let Some(stmt) = statements.first() {
                let query_context = self.analyze_query_context(stmt, context.position);
                
                // Добавляем предложения на основе контекста
                match query_context {
                    QueryContext::Select { after_from, after_join, .. } => {
                        if after_from {
                            items.extend(self.get_table_completions(&context.tables));
                        }
                        if after_join {
                            items.extend(self.get_table_completions(&context.tables));
                        }
                    }
                    QueryContext::Select { columns_available, .. } => {
                        items.extend(self.get_column_completions(&columns_available));
                    }
                    _ => {}
                }
            }
        }

        // 3. Schemas
        items.extend(self.get_schema_completions(&context.schemas));

        // 4. Tables
        items.extend(self.get_table_completions(&context.tables));

        // 5. Functions
        items.extend(self.get_function_completions(&context.functions));

        // 6. Aliases
        items.extend(self.get_alias_completions(&context.aliases));

        items
    }

    /// Анализирует контекст запроса
    fn analyze_query_context(&self, stmt: &Statement, position: usize) -> QueryContext {
        match stmt {
            Statement::Query(query) => {
                self.analyze_select_context(query, position)
            }
            Statement::Insert { .. } => QueryContext::Insert,
            Statement::Update { .. } => QueryContext::Update,
            Statement::Delete { .. } => QueryContext::Delete,
            Statement::Create { .. } => QueryContext::Create,
            _ => QueryContext::Unknown,
        }
    }

    /// Анализирует SELECT контекст
    fn analyze_select_context(&self, query: &Query, position: usize) -> QueryContext {
        let mut context = QueryContext::Select {
            columns_available: vec![],
            after_from: false,
            after_join: false,
            after_where: false,
            after_group_by: false,
            after_order_by: false,
        };

        // Упрощенный анализ - в production нужно парсить позицию
        if let Some(from) = &query.body.as_select() {
            if let Some(from_clause) = &from.from {
                // Собираем доступные таблицы и колонки
                for table_factor in &from_clause.relation {
                    if let TableFactor::Table { name, alias, .. } = table_factor {
                        let table_name = name.to_string();
                        let alias_name = alias.as_ref().map(|a| a.name.to_string());
                        
                        // Добавляем колонки таблицы в контекст
                        // (в реальной реализации нужно загружать из metadata)
                    }
                }
            }
        }

        context
    }

    /// Предложения ключевых слов SQL
    fn get_keyword_completions(&self) -> Vec<CompletionItem> {
        let keywords = vec![
            ("SELECT", "Select columns from table"),
            ("FROM", "Specify source table"),
            ("WHERE", "Filter rows"),
            ("JOIN", "Join tables"),
            ("LEFT JOIN", "Left outer join"),
            ("RIGHT JOIN", "Right outer join"),
            ("INNER JOIN", "Inner join"),
            ("ON", "Join condition"),
            ("GROUP BY", "Group results"),
            ("HAVING", "Filter groups"),
            ("ORDER BY", "Sort results"),
            ("LIMIT", "Limit rows"),
            ("OFFSET", "Skip rows"),
            ("INSERT INTO", "Insert data"),
            ("VALUES", "Insert values"),
            ("UPDATE", "Update data"),
            ("SET", "Set column values"),
            ("DELETE FROM", "Delete rows"),
            ("CREATE TABLE", "Create new table"),
            ("ALTER TABLE", "Modify table"),
            ("DROP TABLE", "Delete table"),
            ("CREATE INDEX", "Create index"),
            ("CREATE VIEW", "Create view"),
            ("WITH", "Common table expression"),
            ("UNION", "Combine results"),
            ("INTERSECT", "Intersection"),
            ("EXCEPT", "Exclusion"),
            ("DISTINCT", "Unique values"),
            ("AS", "Alias"),
            ("CASE", "Conditional expression"),
            ("WHEN", "Case condition"),
            ("THEN", "Case result"),
            ("ELSE", "Default case"),
            ("END", "End case"),
            ("CAST", "Convert type"),
            ("CONVERT", "Convert type"),
        ];

        keywords.iter().map(|(kw, detail)| CompletionItem {
            label: kw.to_string(),
            kind: CompletionKind::Keyword,
            detail: Some(detail.to_string()),
            documentation: None,
            sort_text: Some(format!("A_{}", kw)),
            insert_text: Some(kw.to_string()),
        }).collect()
    }

    /// Предложения схем
    fn get_schema_completions(&self, schemas: &[String]) -> Vec<CompletionItem> {
        schemas.iter().map(|schema| CompletionItem {
            label: schema.clone(),
            kind: CompletionKind::Schema,
            detail: Some("Schema".to_string()),
            documentation: None,
            sort_text: Some(format!("B_{}", schema)),
            insert_text: Some(schema.clone()),
        }).collect()
    }

    /// Предложения таблиц
    fn get_table_completions(&self, tables: &[TableInfo]) -> Vec<CompletionItem> {
        tables.iter().map(|table| CompletionItem {
            label: table.name.clone(),
            kind: CompletionKind::Table,
            detail: Some(format!("{}.{}", table.schema, table.name)),
            documentation: Some(format!("{} columns", table.columns.len())),
            sort_text: Some(format!("C_{}", table.name)),
            insert_text: Some(table.name.clone()),
        }).collect()
    }

    /// Предложения столбцов
    fn get_column_completions(&self, columns: &[ColumnInfo]) -> Vec<CompletionItem> {
        columns.iter().map(|col| CompletionItem {
            label: col.column.clone(),
            kind: CompletionKind::Column,
            detail: Some(format!("{}.{}", col.table, col.data_type)),
            documentation: None,
            sort_text: Some(format!("D_{}", col.column)),
            insert_text: Some(col.column.clone()),
        }).collect()
    }

    /// Предложения функций
    fn get_function_completions(&self, functions: &[FunctionInfo]) -> Vec<CompletionItem> {
        functions.iter().map(|func| CompletionItem {
            label: format!("{}()", func.name),
            kind: CompletionKind::Function,
            detail: Some(func.return_type.clone()),
            documentation: Some(format!("Args: {}", func.args.join(", "))),
            sort_text: Some(format!("E_{}", func.name)),
            insert_text: Some(format!("{}($0)", func.name)),
        }).collect()
    }

    /// Предложения алиасов
    fn get_alias_completions(&self, aliases: &[String]) -> Vec<CompletionItem> {
        aliases.iter().map(|alias| CompletionItem {
            label: alias.clone(),
            kind: CompletionKind::Alias,
            detail: Some("Table alias".to_string()),
            documentation: None,
            sort_text: Some(format!("F_{}", alias)),
            insert_text: Some(alias.clone()),
        }).collect()
    }

    /// Форматирует SQL запрос
    pub fn format_sql(&self, sql: &str) -> Result<String, String> {
        let statements = Parser::parse_sql(&*self.dialect, sql)
            .map_err(|e| format!("Parse error: {}", e))?;

        let formatted = statements.iter()
            .map(|stmt| format_statement(stmt))
            .collect::<Vec<_>>()
            .join(";\n\n");

        Ok(formatted)
    }

    /// Проверяет синтаксис SQL
    pub fn validate_sql(&self, sql: &str) -> Result<(), String> {
        Parser::parse_sql(&*self.dialect, sql)
            .map_err(|e| format!("Syntax error: {}", e))?;
        Ok(())
    }

    /// Извлекает таблицы из SQL запроса
    pub fn extract_tables(&self, sql: &str) -> Vec<String> {
        let mut tables = Vec::new();

        if let Ok(statements) = Parser::parse_sql(&*self.dialect, sql) {
            for stmt in statements {
                self.extract_tables_from_statement(&stmt, &mut tables);
            }
        }

        tables
    }

    fn extract_tables_from_statement(&self, stmt: &Statement, tables: &mut Vec<String>) {
        match stmt {
            Statement::Query(query) => {
                self.extract_tables_from_query(query, tables);
            }
            _ => {}
        }
    }

    fn extract_tables_from_query(&self, query: &Query, tables: &mut Vec<String>) {
        if let Some(select) = query.body.as_select() {
            if let Some(from) = &select.from {
                for table_factor in &from.relation {
                    if let TableFactor::Table { name, .. } = table_factor {
                        tables.push(name.to_string());
                    }
                }
            }
        }
    }
}

/// Контекст запроса
#[derive(Debug, Clone)]
pub enum QueryContext {
    Select {
        columns_available: Vec<ColumnInfo>,
        after_from: bool,
        after_join: bool,
        after_where: bool,
        after_group_by: bool,
        after_order_by: bool,
    },
    Insert,
    Update,
    Delete,
    Create,
    Unknown,
}

/// Форматирует SQL statement
fn format_statement(stmt: &Statement) -> String {
    // Упрощенная реализация - в production использовать sqlformat crate
    format!("{}", stmt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_completions() {
        let service = SqlAutocompleteService::new("postgresql");
        let context = CompletionContext {
            sql: String::new(),
            position: 0,
            schemas: vec![],
            tables: vec![],
            columns: vec![],
            functions: vec![],
            aliases: vec![],
        };

        let completions = service.get_completions(&context);
        assert!(completions.iter().any(|c| c.kind == CompletionKind::Keyword));
    }

    #[test]
    fn test_validate_sql() {
        let service = SqlAutocompleteService::new("postgresql");
        
        assert!(service.validate_sql("SELECT * FROM users").is_ok());
        assert!(service.validate_sql("INVALID SQL").is_err());
    }

    #[test]
    fn test_extract_tables() {
        let service = SqlAutocompleteService::new("postgresql");
        let sql = "SELECT * FROM users u JOIN orders o ON u.id = o.user_id";
        
        let tables = service.extract_tables(sql);
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
    }
}
