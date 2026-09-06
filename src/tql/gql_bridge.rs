//! GQL (ISO/IEC 39075) to TQL bridge — nom-based парсер.
//!
//! Транслирует GQL-запросы в TQL AST. Покрывает подмножество GQL,
//! достаточное для совместимости с Neo4j-инструментарием.

use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::engine::{TqlEngine, TqlResult};
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while1},
    character::complete::{char, multispace0},
    combinator::{map, opt},
    multi::separated_list0,
    sequence::{preceded, tuple},
    IResult,
};
use std::sync::Arc;

fn ws(input: &str) -> IResult<&str, ()> {
    let (input, _) = multispace0(input)?;
    Ok((input, ()))
}

fn identifier(input: &str) -> IResult<&str, &str> {
    ws(input)?;
    take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '.')(input)
}

fn number(input: &str) -> IResult<&str, f32> {
    ws(input)?;
    let (input, neg) = opt(char('-'))(input)?;
    let (input, int_part) = nom::character::complete::digit1(input)?;
    let (input, dec_part) = opt(preceded(char('.'), nom::character::complete::digit1))(input)?;
    let int_val: f32 = int_part.parse().unwrap_or(0.0);
    let dec_val: f32 = match dec_part {
        Some(d) => format!("0.{d}").parse().unwrap_or(0.0),
        None => 0.0,
    };
    let val = int_val + dec_val;
    Ok((input, if neg.is_some() { -val } else { val }))
}

fn string_literal(input: &str) -> IResult<&str, String> {
    ws(input)?;
    let (input, _) = char('\'')(input)?;
    let (input, content) = take_while1(|c: char| c != '\'')(input)?;
    let (input, _) = char('\'')(input)?;
    Ok((input, content.to_string()))
}

fn parse_gql_match(input: &str) -> IResult<&str, (String, String)> {
    ws(input)?;
    let (input, _) = char('(')(input)?;
    ws(input)?;
    let (input, alias) = identifier(input)?;
    ws(input)?;
    let (input, _) = char(':')(input)?;
    let (input, label) = identifier(input)?;
    ws(input)?;
    let (input, _) = char(')')(input)?;
    Ok((input, (alias.to_string(), label.to_string())))
}

fn parse_gql_return(input: &str) -> IResult<&str, Vec<String>> {
    ws(input)?;
    let (input, _) = tag_no_case("RETURN")(input)?;
    ws(input)?;
    separated_list0(
        tuple((ws, char(','), ws)),
        map(identifier, std::string::ToString::to_string),
    )(input)
}

fn parse_gql_limit(input: &str) -> IResult<&str, u32> {
    ws(input)?;
    let (input, _) = tag_no_case("LIMIT")(input)?;
    ws(input)?;
    map(nom::character::complete::digit1, |s: &str| {
        s.parse::<u32>().unwrap_or(10)
    })(input)
}

fn parse_gql_where(input: &str) -> IResult<&str, (String, f32)> {
    ws(input)?;
    let (input, _) = tag_no_case("WHERE")(input)?;
    ws(input)?;
    // Try cosine_similarity(field, $query) > threshold
    let (input, _) = tag_no_case("cosine_similarity")(input)?;
    ws(input)?;
    let (input, _) = char('(')(input)?;
    ws(input)?;
    let (input, field) = identifier(input)?;
    ws(input)?;
    let (input, _) = char(',')(input)?;
    ws(input)?;
    let (input, _) = alt((tag("$query"), tag("$q"), tag("query_vector")))(input)?;
    ws(input)?;
    let (input, _) = char(')')(input)?;
    ws(input)?;
    let (input, _) = char('>')(input)?;
    ws(input)?;
    let (input, threshold) = number(input)?;
    Ok((input, (field.to_string(), 1.0 - threshold)))
}

fn parse_gql_order_by(input: &str) -> IResult<&str, (String, bool)> {
    ws(input)?;
    let (input, _) = tag_no_case("ORDER BY")(input)?;
    ws(input)?;
    let (input, field) = identifier(input)?;
    ws(input)?;
    let (input, asc) = opt(tag_no_case("DESC"))(input)?;
    Ok((input, (field.to_string(), asc.is_none())))
}

/// GQL → TQL bridge with nom parser.
pub struct GqlBridge {
    engine: TqlEngine,
}

impl GqlBridge {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        Self {
            engine: TqlEngine::with_store(store),
        }
    }

    pub async fn execute(&self, gql: &str) -> Result<GqlResult, GqlError> {
        let tql = self.translate(gql)?;
        let result = self
            .engine
            .execute(&tql)
            .await
            .map_err(|e| GqlError::Execution(format!("{e}")))?;
        Ok(match result {
            TqlResult::Query(results) => GqlResult::Rows(
                results
                    .into_iter()
                    .map(|r| GqlRow {
                        id: r.id,
                        score: r.score,
                        properties: r.properties,
                    })
                    .collect(),
            ),
            TqlResult::Explain(plan) => GqlResult::Plan(plan),
            TqlResult::DdlSuccess(msg) => GqlResult::Message(msg),
            TqlResult::QueryReady(_) => GqlResult::Message("Query ready".to_string()),
        })
    }

    pub fn translate(&self, gql: &str) -> Result<String, GqlError> {
        let trimmed = gql.trim();

        if trimmed.to_uppercase().starts_with("MATCH") {
            return self.translate_match(trimmed);
        }
        if trimmed.to_uppercase().starts_with("INSERT") {
            return self.translate_insert(trimmed);
        }
        if trimmed.to_uppercase().contains("CREATE VECTOR INDEX") {
            return self.translate_vector_index(trimmed);
        }

        Err(GqlError::Unsupported(
            "Unsupported GQL statement. Supported: MATCH, INSERT, CREATE VECTOR INDEX".to_string(),
        ))
    }

    fn translate_match(&self, gql: &str) -> Result<String, GqlError> {
        let mut remaining = gql;

        // Parse MATCH clause
        let (rest, match_clause) = parse_gql_match(remaining)
            .map_err(|_| GqlError::Parse("Failed to parse MATCH clause".into()))?;
        remaining = rest;

        // Parse optional WHERE
        let (rest, where_info) = match opt(parse_gql_where)(remaining) {
            Ok((r, w)) => (r, w),
            Err(e) => return Err(GqlError::Parse(format!("Failed to parse WHERE: {e:?}"))),
        };
        remaining = rest;

        // Parse optional ORDER BY
        let (rest, order_info) = match opt(parse_gql_order_by)(remaining) {
            Ok((r, o)) => (r, o),
            Err(e) => return Err(GqlError::Parse(format!("Failed to parse ORDER BY: {e:?}"))),
        };
        remaining = rest;

        // Parse optional LIMIT
        let (rest, limit) = match opt(parse_gql_limit)(remaining) {
            Ok((r, l)) => (r, l.unwrap_or(10)),
            Err(e) => return Err(GqlError::Parse(format!("Failed to parse LIMIT: {e:?}"))),
        };
        remaining = rest;

        // Parse RETURN
        let (_rest, return_fields) = parse_gql_return(remaining)
            .map_err(|_| GqlError::Parse("Failed to parse RETURN".into()))?;

        // Build TQL string
        let mut tql = String::new();
        tql.push_str(&format!("MATCH ({}:{})", match_clause.0, match_clause.1));

        if let Some((field, threshold)) = where_info {
            tql.push_str(&format!(" WHERE TOROIDALDISTANCE({field}, {threshold})"));
        }

        if !return_fields.is_empty() {
            tql.push_str(&format!(" RETURN {}", return_fields.join(", ")));
        }

        if let Some((field, ascending)) = order_info {
            let dir = if ascending { "" } else { " DESC" };
            tql.push_str(&format!(" ORDER BY {field}{dir}"));
        }

        tql.push_str(&format!(" LIMIT {limit}"));

        Ok(tql)
    }

    fn translate_insert(&self, gql: &str) -> Result<String, GqlError> {
        let label = gql
            .split(':')
            .nth(1)
            .and_then(|s| s.split([' ', ')', '{']).next())
            .ok_or_else(|| GqlError::Parse("Could not extract label".into()))?;

        Ok(format!(
            "CREATE NODE TYPE {label} (id INT PRIMARY KEY);\nMATCH (n:{label}) RETURN n.id LIMIT 1"
        ))
    }

    fn translate_vector_index(&self, gql: &str) -> Result<String, GqlError> {
        let label = gql
            .split(':')
            .nth(1)
            .and_then(|s| s.split([' ', ')']).next())
            .unwrap_or("Document");

        let dim = gql
            .split("DIMENSION")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1536);

        Ok(format!(
            "ALTER NODE TYPE {label} ADD content_t3 VECTOR({dim}) VECTOR_INDEX(phi = 5.71);"
        ))
    }
}

#[derive(Debug, Clone)]
pub enum GqlResult {
    Rows(Vec<GqlRow>),
    Plan(String),
    Message(String),
}

#[derive(Debug, Clone)]
pub struct GqlRow {
    pub id: u64,
    pub score: f32,
    pub properties: serde_json::Value,
}

#[derive(Debug)]
pub enum GqlError {
    Parse(String),
    Execution(String),
    Unsupported(String),
}

impl std::fmt::Display for GqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GqlError::Parse(msg) => write!(f, "GQL parse error: {msg}"),
            GqlError::Execution(msg) => write!(f, "GQL execution error: {msg}"),
            GqlError::Unsupported(msg) => write!(f, "GQL unsupported: {msg}"),
        }
    }
}

impl std::error::Error for GqlError {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_store() -> Arc<HybridPersistentStore> {
        let dir = TempDir::new().unwrap();
        Arc::new(HybridPersistentStore::open(dir.path().join("test")).unwrap())
    }

    #[test]
    fn test_translate_match_simple() {
        let bridge = GqlBridge::new(create_store());
        let tql = bridge
            .translate("MATCH (n:Person) RETURN n.name, n.age LIMIT 10")
            .unwrap();
        assert!(tql.contains("MATCH (n:Person)"));
        assert!(tql.contains("RETURN n.name, n.age"));
        assert!(tql.contains("LIMIT 10"));
    }

    #[test]
    fn test_translate_cosine_similarity() {
        let bridge = GqlBridge::new(create_store());
        let tql = bridge.translate(
            "MATCH (d:Document) WHERE cosine_similarity(d.embedding, $query) > 0.9 RETURN d.title LIMIT 10"
        ).unwrap();
        assert!(tql.contains("TOROIDALDISTANCE"));
        assert!(tql.contains("0.1")); // 1 - 0.9
    }

    #[test]
    fn test_translate_order_by() {
        let bridge = GqlBridge::new(create_store());
        let tql = bridge
            .translate("MATCH (n:Person) RETURN n.name ORDER BY n.name DESC LIMIT 5")
            .unwrap();
        assert!(tql.contains("ORDER BY"));
        assert!(tql.contains("DESC"));
    }

    #[test]
    fn test_translate_insert() {
        let bridge = GqlBridge::new(create_store());
        let tql = bridge
            .translate("INSERT (:Person {name: 'Alice'})")
            .unwrap();
        assert!(tql.contains("CREATE NODE TYPE Person"));
    }

    #[test]
    fn test_unsupported() {
        let bridge = GqlBridge::new(create_store());
        let result = bridge.translate("CALL db.schema()");
        assert!(result.is_err());
    }
}
