use serde_json::Value;

pub struct QueryValidator {
    config: ValidatorConfig,
}

#[derive(Clone)]
pub struct ValidatorConfig {
    pub max_query_depth: usize,
    pub max_result_size: usize,
    pub max_complexity: f64,
    pub enable_recursion_limit: bool,
    pub max_recursion_depth: usize,
    pub max_memory_mb: usize,
    pub timeout_seconds: u64,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        ValidatorConfig {
            max_query_depth: 10,
            max_result_size: 10000,
            max_complexity: 1000.0,
            enable_recursion_limit: true,
            max_recursion_depth: 100,
            max_memory_mb: 1024,
            timeout_seconds: 30,
        }
    }
}

impl QueryValidator {
    pub fn new(config: ValidatorConfig) -> Self {
        QueryValidator { config }
    }

    pub fn validate(&self, query: &str) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if let Err(e) = self.check_syntax(query) {
            errors.push(e);
        }

        if let Err(e) = self.check_depth_attacks(query) {
            errors.push(e);
        }

        if let Err(e) = self.check_recursion_limits(query) {
            errors.push(e);
        }

        if let Err(e) = self.check_resource_exhaustion(query) {
            warnings.push(e);
        }

        if warnings.len() > 0 && errors.len() == 0 {
            ValidationResult::Warning(warnings)
        } else if errors.len() > 0 {
            ValidationResult::Invalid(errors)
        } else {
            ValidationResult::Valid
        }
    }

    fn check_syntax(&self, query: &str) -> Result<(), ValidationError> {
        if query.trim().is_empty() {
            return Err(ValidationError::EmptyQuery);
        }

        if query.len() > 1_000_000 {
            return Err(ValidationError::QueryTooLarge {
                size: query.len(),
                max: 1_000_000,
            });
        }

        Ok(())
    }

    fn check_depth_attacks(&self, query: &str) -> Result<(), ValidationError> {
        let depth = self.count_nested_parens(query);

        if depth > self.config.max_query_depth {
            return Err(ValidationError::DepthAttack {
                depth,
                max: self.config.max_query_depth,
            });
        }

        let subquery_count = query.to_lowercase().matches("select").count();
        if subquery_count > 10 {
            return Err(ValidationError::TooManySubqueries(subquery_count));
        }

        Ok(())
    }

    fn check_recursion_limits(&self, query: &str) -> Result<(), ValidationError> {
        if !self.config.enable_recursion_limit {
            return Ok(());
        }

        let query_lower = query.to_lowercase();

        if query_lower.contains("with recursive") || query_lower.contains("with_recursive") {
            let depth = self.estimate_recursion_depth(query);

            if depth > self.config.max_recursion_depth {
                return Err(ValidationError::RecursionTooDeep {
                    depth,
                    max: self.config.max_recursion_depth,
                });
            }
        }

        Ok(())
    }

    fn check_resource_exhaustion(&self, query: &str) -> Result<(), ValidationError> {
        let complexity = self.estimate_complexity(query);

        if complexity > self.config.max_complexity {
            return Err(ValidationError::QueryTooComplex {
                complexity,
                max: self.config.max_complexity,
            });
        }

        if query.to_lowercase().contains("limit 1000000")
            || query.to_lowercase().contains("limit 999999")
        {
            return Err(ValidationError::ResultSizeTooLarge);
        }

        Ok(())
    }

    fn count_nested_parens(&self, query: &str) -> usize {
        let mut max_depth = 0;
        let mut current_depth = 0;

        for c in query.chars() {
            match c {
                '(' => {
                    current_depth += 1;
                    max_depth = max_depth.max(current_depth);
                }
                ')' => {
                    current_depth = current_depth.saturating_sub(1);
                }
                _ => {}
            }
        }

        max_depth
    }

    fn estimate_recursion_depth(&self, query: &str) -> usize {
        let mut depth = 10;

        if let Some(limit_pos) = query.to_lowercase().find("limit ") {
            let after_limit = &query[limit_pos..];
            let number: String = after_limit
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();

            if let Ok(n) = number.parse::<usize>() {
                depth = n;
            }
        }

        depth
    }

    fn estimate_complexity(&self, query: &str) -> f64 {
        let mut complexity = 0.0;

        complexity += query.to_lowercase().matches("join").count() as f64 * 10.0;
        complexity += query.to_lowercase().matches("substr").count() as f64 * 5.0;
        complexity += query.to_lowercase().matches("like").count() as f64 * 3.0;
        complexity += query.to_lowercase().matches("or").count() as f64 * 2.0;
        complexity += query.to_lowercase().matches("and").count() as f64 * 1.0;
        complexity += query.to_lowercase().matches("order by").count() as f64 * 5.0;
        complexity += query.to_lowercase().matches("group by").count() as f64 * 8.0;
        complexity += query.to_lowercase().matches("having").count() as f64 * 8.0;

        complexity
    }

    pub fn sanitize_query(&self, query: &str) -> String {
        let mut sanitized = query.to_string();

        let dangerous = ["--", "/*", "*/", ";", "xp_", "sp_", "exec", "execute"];
        for pattern in dangerous {
            sanitized = sanitized.replace(pattern, "");
        }

        sanitized
    }
}

#[derive(Clone, Debug)]
pub enum ValidationResult {
    Valid,
    Warning(Vec<String>),
    Invalid(Vec<ValidationError>),
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        matches!(self, ValidationResult::Valid)
    }

    pub fn is_invalid(&self) -> bool {
        matches!(self, ValidationResult::Invalid(_))
    }

    pub fn errors(&self) -> Vec<&ValidationError> {
        match self {
            ValidationResult::Invalid(errors) => errors.iter().collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ValidationError {
    EmptyQuery,
    QueryTooLarge { size: usize, max: usize },
    DepthAttack { depth: usize, max: usize },
    TooManySubqueries(usize),
    RecursionTooDeep { depth: usize, max: usize },
    QueryTooComplex { complexity: f64, max: f64 },
    ResultSizeTooLarge,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::EmptyQuery => write!(f, "Query is empty"),
            ValidationError::QueryTooLarge { size, max } => {
                write!(f, "Query size {} exceeds maximum {}", size, max)
            }
            ValidationError::DepthAttack { depth, max } => {
                write!(f, "Query depth {} exceeds maximum {}", depth, max)
            }
            ValidationError::TooManySubqueries(count) => {
                write!(f, "Too many subqueries: {}", count)
            }
            ValidationError::RecursionTooDeep { depth, max } => {
                write!(f, "Recursion depth {} exceeds maximum {}", depth, max)
            }
            ValidationError::QueryTooComplex { complexity, max } => {
                write!(f, "Query complexity {} exceeds maximum {}", complexity, max)
            }
            ValidationError::ResultSizeTooLarge => write!(f, "Result size too large"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query() {
        let validator = QueryValidator::new(ValidatorConfig::default());
        let result = validator.validate("");
        assert!(result.is_invalid());
    }

    #[test]
    fn test_depth_attack() {
        let config = ValidatorConfig {
            max_query_depth: 5,
            ..Default::default()
        };
        let validator = QueryValidator::new(config);

        let query =
            "SELECT * FROM (SELECT * FROM (SELECT * FROM (SELECT * FROM (SELECT * FROM users))))";
        let result = validator.validate(query);
        assert!(result.is_invalid());
    }

    #[test]
    fn test_complex_query() {
        let config = ValidatorConfig {
            max_complexity: 10.0,
            ..Default::default()
        };
        let validator = QueryValidator::new(config);

        let query =
            "SELECT * FROM a JOIN b ON a.id = b.id JOIN c ON b.id = c.id JOIN d ON c.id = d.id";
        let result = validator.validate(query);
        assert!(result.is_invalid());
    }

    #[test]
    fn test_sanitize() {
        let validator = QueryValidator::new(ValidatorConfig::default());
        let query = "SELECT * FROM users; DROP TABLE users; --";
        let sanitized = validator.sanitize_query(query);
        assert!(!sanitized.contains("DROP TABLE"));
    }

    #[test]
    fn test_valid_query() {
        let validator = QueryValidator::new(ValidatorConfig::default());
        let query = "SELECT * FROM users WHERE id = 1";
        let result = validator.validate(query);
        assert!(result.is_valid());
    }
}
