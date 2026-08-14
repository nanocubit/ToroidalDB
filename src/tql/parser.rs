use crate::tql::ast::*;
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while1},
    character::complete::{alpha1, alphanumeric1, char, digit1, multispace0, multispace1},
    combinator::{map, map_res, opt},
    multi::{many0, separated_list0},
    sequence::{delimited, pair, preceded, separated_pair, tuple},
    IResult,
};

fn parse_whitespace(input: &str) -> IResult<&str, ()> {
    let (input, _) = multispace0(input)?;
    Ok((input, ()))
}

fn parse_identifier(input: &str) -> IResult<&str, &str> {
    parse_whitespace(input)?;
    let (input, ident) = take_while1(|c: char| c.is_alphanumeric() || c == '_')(input)?;
    Ok((input, ident))
}

fn parse_label(input: &str) -> IResult<&str, &str> {
    parse_whitespace(input)?;
    let (input, _) = char(':')(input)?;
    let (input, label) = alpha1(input)?;
    Ok((input, label))
}

fn parse_number(input: &str) -> IResult<&str, f32> {
    parse_whitespace(input)?;
    let (input, negative) = opt(char('-'))(input)?;
    let (input, integer_part) = digit1(input)?;
    let (input, decimal_part) = opt(preceded(char('.'), digit1))(input)?;

    let int_val: f32 = integer_part.parse().unwrap_or(0.0);
    let dec_val: f32 = match decimal_part {
        Some(dec) => {
            let decimal_places = dec.len();
            let dec_num: f32 = dec.parse().unwrap_or(0.0);
            dec_num / 10_f32.powi(decimal_places as i32)
        }
        None => 0.0,
    };

    let result = if negative.is_some() {
        -(int_val + dec_val)
    } else {
        int_val + dec_val
    };
    Ok((input, result))
}

fn parse_string_literal(input: &str) -> IResult<&str, String> {
    parse_whitespace(input)?;
    let (input, _) = char('"')(input)?;
    let (input, content) = take_while1(|c: char| c != '"')(input)?;
    let (input, _) = char('"')(input)?;
    Ok((input, content.to_string()))
}

fn parse_property_value(input: &str) -> IResult<&str, PropertyValue> {
    parse_whitespace(input)?;
    alt((
        map(parse_string_literal, PropertyValue::String),
        map(parse_number, |n| PropertyValue::Number(n as f64)),
        map(
            alt((tag_no_case("true"), tag_no_case("false"))),
            |b: &str| PropertyValue::Boolean(b.eq_ignore_ascii_case("true")),
        ),
    ))(input)
}

/// Parse HINTS clause for TQL v2.2
/// Syntax: HINTS [USING GPU|AVX512|AVX2|SCALAR] [SCATTER N SHARDS] [PREFER LOCAL_SHARD]
fn parse_hints(input: &str) -> IResult<&str, QueryHints> {
    parse_whitespace(input)?;

    let (input, _) = tag_no_case("HINTS")(input)?;
    parse_whitespace(input)?;

    let mut hints = QueryHints::default();

    // Parse USING [GPU|AVX512|AVX2|SCALAR]
    let (input, using_backend) = opt(preceded(
        tuple((tag_no_case("USING"), multispace1)),
        alt((
            map(tag_no_case("GPU"), |_| BackendHint::Gpu),
            map(tag_no_case("AVX512"), |_| BackendHint::Avx512),
            map(tag_no_case("AVX2"), |_| BackendHint::Avx2),
            map(tag_no_case("SCALAR"), |_| BackendHint::Scalar),
            map(tag_no_case("AUTO"), |_| BackendHint::Auto),
        )),
    ))(input)?;

    if let Some(backend) = using_backend {
        hints.backend = Some(backend);
    }

    parse_whitespace(input)?;

    // Parse SCATTER N SHARDS
    let (input, scatter) = opt(preceded(
        tuple((tag_no_case("SCATTER"), multispace1)),
        map_res(digit1, |s: &str| s.parse::<u32>()),
    ))(input)?;

    if let Some(n) = scatter {
        hints.scatter_shards = Some(n);
    }

    parse_whitespace(input)?;

    // Parse PREFER LOCAL_SHARD
    let (input, prefer_local): (&str, Option<&str>) = opt(preceded(
        tuple((
            tag_no_case("PREFER"),
            multispace1,
            tag_no_case("LOCAL_SHARD"),
        )),
        tag_no_case("LOCAL_SHARD"),
    ))(input)?;

    if prefer_local.is_some() {
        hints.prefer_local_shard = true;
    }

    parse_whitespace(input)?;

    // Parse PREFETCH GRAPH_HOPS N
    let (input, prefetch) = opt(preceded(
        tuple((
            tag_no_case("PREFETCH"),
            multispace1,
            tag_no_case("GRAPH_HOPS"),
            multispace1,
        )),
        map_res(digit1, |s: &str| s.parse::<u32>()),
    ))(input)?;

    if let Some(n) = prefetch {
        hints.prefetch_hops = Some(n);
    }

    Ok((input, hints))
}

fn parse_node_pattern(input: &str) -> IResult<&str, NodePattern> {
    parse_whitespace(input)?;
    let (input, _) = char('(')(input)?;

    // Parse alias:Label {properties}
    let (input, alias) = parse_identifier(input)?;
    let (input, _) = char(':')(input)?;
    let (input, label) = alpha1(input)?;

    // Parse optional properties
    let (input, properties) = opt(|input| {
        let (input, _) = char('{')(input)?;
        let (input, prop_list) =
            separated_pair(parse_identifier, char(':'), parse_property_value)(input)?;

        let mut props = vec![(prop_list.0.to_string(), prop_list.1)];
        let (input, _) = many0(|input| {
            let (input, _) = char(',')(input)?;
            separated_pair(parse_identifier, char(':'), parse_property_value)(input)
        })(input)?;

        let (input, _) = char('}')(input)?;
        Ok((input, Some(props)))
    })(input)?;

    let (input, _) = char(')')(input)?;

    Ok((
        input,
        NodePattern {
            alias: alias.to_string(),
            label: label.to_string(),
            properties: properties.flatten(),
        },
    ))
}

fn parse_direction(input: &str) -> IResult<&str, Direction> {
    parse_whitespace(input)?;
    alt((
        map(
            tuple((
                char('<'),
                char('-'),
                char('['),
                char(':'),
                alpha1,
                char(']'),
                char('-'),
                char('>'),
            )),
            |_| Direction::Outgoing,
        ),
        map(
            tuple((
                char('-'),
                char('['),
                char(':'),
                alpha1,
                char(']'),
                char('-'),
                char('>'),
            )),
            |_| Direction::Outgoing,
        ),
        map(
            tuple((
                char('<'),
                char('-'),
                char('['),
                char(':'),
                alpha1,
                char(']'),
                char('-'),
            )),
            |_| Direction::Incoming,
        ),
    ))(input)
}

fn parse_relationship_pattern(input: &str) -> IResult<&str, RelationshipPattern> {
    parse_whitespace(input)?;
    let (input, _) = char('-')(input)?;
    let (input, _) = char('[')(input)?;
    let (input, _) = char(':')(input)?;
    let (input, rel_type) = alpha1(input)?;
    let (input, _) = char(']')(input)?;

    let (input, direction) = alt((
        map(char('>'), |_| Direction::Outgoing),
        map(char('<'), |_| Direction::Incoming),
        map(char('-'), |_| Direction::Both),
    ))(input)?;

    Ok((
        input,
        RelationshipPattern {
            type_: rel_type.to_string(),
            direction,
        },
    ))
}

fn parse_match_clause(input: &str) -> IResult<&str, MatchClause> {
    parse_whitespace(input)?;
    let (input, _) = tag_no_case("MATCH")(input)?;
    parse_whitespace(input)?;

    let (input, source) = parse_node_pattern(input)?;

    // Check if there's a relationship pattern
    let (input, relationship_and_target) = opt(|input| {
        parse_whitespace(input)?;
        let (input, relationship) = parse_relationship_pattern(input)?;
        parse_whitespace(input)?;
        let (input, target) = parse_node_pattern(input)?;
        Ok((input, (relationship, target)))
    })(input)?;

    let (relationship, target) = match relationship_and_target {
        Some((rel, tgt)) => (Some(rel), Some(tgt)),
        None => (None, None),
    };

    Ok((
        input,
        MatchClause {
            source,
            relationship,
            target,
        },
    ))
}

fn parse_toroidal_distance(input: &str) -> IResult<&str, WhereCondition> {
    parse_whitespace(input)?;
    let (input, _) = tag_no_case("TOROIDALDISTANCE")(input)?;
    let (input, _) = char('(')(input)?;
    parse_whitespace(input)?;

    let (input, field) = parse_identifier(input)?;
    parse_whitespace(input)?;
    let (input, _) = char(',')(input)?;
    parse_whitespace(input)?;
    let (input, threshold) = parse_number(input)?;

    let (input, _) = char(')')(input)?;

    Ok((
        input,
        WhereCondition::ToroidalDistance {
            field: field.to_string(),
            threshold,
        },
    ))
}

fn parse_where_condition(input: &str) -> IResult<&str, WhereCondition> {
    parse_toroidal_distance(input)
}

fn parse_return_clause(input: &str) -> IResult<&str, ReturnClause> {
    parse_whitespace(input)?;
    let (input, _) = tag_no_case("RETURN")(input)?;
    parse_whitespace(input)?;

    let (input, first_field) = parse_identifier(input)?;
    let (input, additional_fields) = many0(|input| {
        parse_whitespace(input)?;
        let (input, _) = char(',')(input)?;
        parse_whitespace(input)?;
        parse_identifier(input)
    })(input)?;

    let mut fields = vec![first_field.to_string()];
    fields.extend(additional_fields.iter().map(|s| s.to_string()));

    Ok((input, ReturnClause { fields }))
}

fn parse_limit_clause(input: &str) -> IResult<&str, LimitClause> {
    parse_whitespace(input)?;
    let (input, _) = tag_no_case("LIMIT")(input)?;
    parse_whitespace(input)?;

    let (input, count_str) = digit1(input)?;
    let count = count_str.parse().unwrap_or(10);

    Ok((input, LimitClause { count }))
}

fn parse_connected_clause(input: &str) -> IResult<&str, ConnectedClause> {
    parse_whitespace(input)?;
    let (input, _) = tag_no_case("CONNECTEDTO")(input)?;
    let (input, _) = char('(')(input)?;
    parse_whitespace(input)?;

    let (input, target_label) = parse_identifier(input)?;
    parse_whitespace(input)?;
    let (input, _) = char(',')(input)?;
    parse_whitespace(input)?;

    let (input, relationship_type) = parse_string_literal(input)?;
    parse_whitespace(input)?;

    // Optional property filter
    let (input, property_filter) = if input.starts_with(',') {
        parse_whitespace(input)?;
        let (input, _) = char(',')(input)?;
        parse_whitespace(input)?;
        let (input, prop_name) = parse_identifier(input)?;
        parse_whitespace(input)?;
        let (input, _) = char(',')(input)?;
        parse_whitespace(input)?;
        let (input, prop_value) = parse_property_value(input)?;
        parse_whitespace(input)?;
        let (input, _) = char(')')(input)?;
        (input, Some((prop_name.to_string(), prop_value)))
    } else {
        let (input, _) = char(')')(input)?;
        (input, None)
    };

    Ok((
        input,
        ConnectedClause {
            target_label: target_label.to_string(),
            relationship_type,
            property_filter,
        },
    ))
}

fn parse_aggregation_function(input: &str) -> IResult<&str, AggregationFunction> {
    parse_whitespace(input)?;

    let (input, func_name) = alt((
        tag_no_case("COUNT"),
        tag_no_case("SUM"),
        tag_no_case("AVG"),
        tag_no_case("MIN"),
        tag_no_case("MAX"),
    ))(input)?;

    let (input, _) = char('(')(input)?;
    parse_whitespace(input)?;

    let (input, field) = parse_identifier(input)?;
    parse_whitespace(input)?;

    let (input, _) = char(')')(input)?;

    let func = match func_name.to_uppercase().as_str() {
        "COUNT" => AggregationFunction::Count,
        "SUM" => AggregationFunction::Sum(field.to_string()),
        "AVG" => AggregationFunction::Avg(field.to_string()),
        "MIN" => AggregationFunction::Min(field.to_string()),
        "MAX" => AggregationFunction::Max(field.to_string()),
        _ => {
            return Err(nom::Err::Failure(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Alpha,
            )))
        }
    };

    Ok((input, func))
}

fn parse_aggregation_field(input: &str) -> IResult<&str, AggregationField> {
    parse_whitespace(input)?;

    // Проверяем, является ли это агрегацией
    if input.to_uppercase().starts_with("COUNT(")
        || input.to_uppercase().starts_with("SUM(")
        || input.to_uppercase().starts_with("AVG(")
        || input.to_uppercase().starts_with("MIN(")
        || input.to_uppercase().starts_with("MAX(")
    {
        let (input, function) = parse_aggregation_function(input)?;

        // Проверяем наличие AS alias
        let (input, alias) = if input.to_uppercase().starts_with("AS") {
            parse_whitespace(input)?;
            let (input, _) = tag_no_case("AS")(input)?;
            parse_whitespace(input)?;
            let (input, alias) = parse_identifier(input)?;
            (input, Some(alias.to_string()))
        } else {
            (input, None)
        };

        Ok((input, AggregationField { function, alias }))
    } else {
        // Это не агрегация, возвращаем ошибку
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Alpha,
        )))
    }
}

fn parse_return_clause_with_aggregations(
    input: &str,
) -> IResult<&str, (Vec<String>, Vec<AggregationField>)> {
    parse_whitespace(input)?;
    let (input, _) = tag_no_case("RETURN")(input)?;
    parse_whitespace(input)?;

    // Разбираем список полей, которые могут быть как обычными, так и агрегациями
    let mut regular_fields = Vec::new();
    let mut aggregation_fields = Vec::new();

    // Парсим первое поле
    let (mut remaining_input, _) = if input.to_uppercase().starts_with("COUNT(")
        || input.to_uppercase().starts_with("SUM(")
        || input.to_uppercase().starts_with("AVG(")
        || input.to_uppercase().starts_with("MIN(")
        || input.to_uppercase().starts_with("MAX(")
    {
        // Это агрегация
        let (input, agg_field) = parse_aggregation_field(input)?;
        aggregation_fields.push(agg_field);
        (input, ())
    } else {
        // Это обычное поле
        let (input, field) = parse_identifier(input)?;
        regular_fields.push(field.to_string());
        (input, ())
    };

    // Парсим остальные поля, разделенные запятыми
    loop {
        parse_whitespace(remaining_input)?;
        if remaining_input.starts_with(',') {
            let (input, _) = char(',')(remaining_input)?;
            parse_whitespace(input)?;

            let (next_input, _) = if input.to_uppercase().starts_with("COUNT(")
                || input.to_uppercase().starts_with("SUM(")
                || input.to_uppercase().starts_with("AVG(")
                || input.to_uppercase().starts_with("MIN(")
                || input.to_uppercase().starts_with("MAX(")
            {
                // Это агрегация
                let (input, agg_field) = parse_aggregation_field(input)?;
                aggregation_fields.push(agg_field);
                (input, ())
            } else {
                // Это обычное поле
                let (input, field) = parse_identifier(input)?;
                regular_fields.push(field.to_string());
                (input, ())
            };

            remaining_input = next_input;
        } else {
            break;
        }
    }

    Ok((remaining_input, (regular_fields, aggregation_fields)))
}

fn parse_order_by_clause(input: &str) -> IResult<&str, OrderByClause> {
    parse_whitespace(input)?;
    let (input, _) = tag_no_case("ORDER BY")(input)?;
    parse_whitespace(input)?;

    let (input, field) = parse_identifier(input)?;
    parse_whitespace(input)?;

    // Проверяем направление сортировки
    let (input, ascending) = if input.to_uppercase().starts_with("DESC") {
        let (input, _) = tag_no_case("DESC")(input)?;
        (input, false)
    } else if input.to_uppercase().starts_with("ASC") {
        let (input, _) = tag_no_case("ASC")(input)?;
        (input, true)
    } else {
        // По умолчанию сортировка по возрастанию
        (input, true)
    };

    Ok((
        input,
        OrderByClause {
            field: field.to_string(),
            ascending,
        },
    ))
}

fn parse_within_clause(input: &str) -> IResult<&str, WithinClause> {
    parse_whitespace(input)?;
    let (input, _) = tag_no_case("WITHIN")(input)?;
    parse_whitespace(input)?;

    let (input, min_hops_str) = digit1(input)?;
    let min_hops = min_hops_str.parse().unwrap_or(1);

    // Проверяем, есть ли диапазон (например, "2 TO 5 HOPS")
    let (input, max_hops) = if input.to_uppercase().starts_with("TO") {
        parse_whitespace(input)?;
        let (input, _) = tag_no_case("TO")(input)?;
        parse_whitespace(input)?;
        let (input, max_hops_str) = digit1(input)?;
        let max_hops = max_hops_str.parse().unwrap_or(min_hops);
        (input, max_hops)
    } else {
        (input, min_hops)
    };

    parse_whitespace(input)?;
    let (input, _) = tag_no_case("HOPS")(input)?;

    Ok((input, WithinClause { min_hops, max_hops }))
}

pub fn parse_query(input: &str) -> IResult<&str, Query> {
    parse_whitespace(input)?;

    // Parse optional DISTRIBUTED keyword
    let (input, distributed) = if input.to_uppercase().starts_with("DISTRIBUTED") {
        parse_whitespace(input)?;
        let (input, _) = tag_no_case("DISTRIBUTED")(input)?;
        parse_whitespace(input)?;
        (input, true)
    } else {
        (input, false)
    };

    // Parse optional HINTS clause (TQL v2.2)
    let (input, hints) = if input.to_uppercase().starts_with("HINTS") {
        let (input, parsed_hints) = parse_hints(input)?;
        (input, Some(parsed_hints))
    } else {
        (input, None)
    };

    // Parse the match clause
    let (input, match_clause) = parse_match_clause(input)?;
    parse_whitespace(input)?;

    // Parse optional WHERE clause
    let (input, where_clause) = if input.to_uppercase().starts_with("WHERE") {
        parse_whitespace(input)?;
        let (input, _) = tag_no_case("WHERE")(input)?;
        parse_whitespace(input)?;
        let (input, clause) = parse_where_condition(input)?;
        (input, Some(clause))
    } else {
        (input, None) // No WHERE clause
    };
    parse_whitespace(input)?;

    // Parse optional CONNECTEDTO clause
    let (input, connected_clause) = if input.to_uppercase().starts_with("CONNECTEDTO") {
        let (input, clause) = parse_connected_clause(input)?;
        (input, Some(clause))
    } else {
        (input, None) // No CONNECTEDTO clause
    };
    parse_whitespace(input)?;

    // Parse optional WITHIN HOPS clause
    let (input, within_clause) = if input.to_uppercase().starts_with("WITHIN") {
        let (input, clause) = parse_within_clause(input)?;
        (input, Some(clause))
    } else {
        (input, None) // No WITHIN HOPS clause
    };
    parse_whitespace(input)?;

    // Parse RETURN clause with aggregations
    let (input, (return_fields, aggregation_fields)) =
        parse_return_clause_with_aggregations(input)?;
    parse_whitespace(input)?;

    // Parse optional ORDER BY clause
    let (input, order_by) = if input.to_uppercase().starts_with("ORDER BY") {
        let (input, clause) = parse_order_by_clause(input)?;
        (input, Some(clause))
    } else {
        (input, None) // No ORDER BY clause
    };
    parse_whitespace(input)?;

    // Parse LIMIT clause
    let (input, limit_clause) = parse_limit_clause(input)?;

    Ok((
        input,
        Query {
            with_clause: None,
            match_clause: Some(match_clause),
            where_clause,
            connected_clause,
            within_clause,
            return_fields,
            aggregation_fields,
            window_functions: Vec::new(),
            order_by,
            subqueries: Vec::new(),
            transaction: None,
            limit: limit_clause.count,
            distributed,
            hints,
            query_hash: 0,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_query() {
        let query =
            "MATCH (node:Label) WHERE TOROIDALDISTANCE(node.vector, 0.3) RETURN node.id LIMIT 10";
        let result = parse_query(query);
        assert!(result.is_ok());

        let (remaining, parsed_query) = result.unwrap();
        assert_eq!(remaining, "");
        assert!(parsed_query.match_clause.is_some());
        assert_eq!(parsed_query.limit, 10);
        if let Some(ref match_clause) = parsed_query.match_clause {
            assert_eq!(match_clause.source.label, "Label");
        }
    }

    #[test]
    fn test_parse_different_threshold() {
        let query =
            "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.embedding, 0.7) RETURN doc.id LIMIT 5";
        let result = parse_query(query);
        assert!(result.is_ok());

        let (remaining, parsed_query) = result.unwrap();
        assert_eq!(remaining, "");
        assert!(parsed_query.match_clause.is_some());
        assert_eq!(parsed_query.limit, 5);
        if let Some(ref match_clause) = parsed_query.match_clause {
            assert_eq!(match_clause.source.label, "Document");
        }
    }

    #[test]
    fn test_parse_high_limit() {
        let query = "MATCH (user:User) WHERE TOROIDALDISTANCE(user.vec, 0.1) RETURN user.name, user.id LIMIT 100";
        let result = parse_query(query);
        assert!(result.is_ok());

        let (remaining, parsed_query) = result.unwrap();
        assert_eq!(remaining, "");
        assert!(parsed_query.match_clause.is_some());
        assert_eq!(parsed_query.limit, 100);
        if let Some(ref match_clause) = parsed_query.match_clause {
            assert_eq!(match_clause.source.label, "User");
        }
    }

    #[test]
    fn test_parse_connectedto_query() {
        let query = r#"MATCH (doc:Document) CONNECTEDTO(doc, "TAGGED_WITH", "quantum") WITHIN 2 HOPS RETURN doc.id LIMIT 20"#;
        let result = parse_query(query);
        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.connected_clause.is_some());
        assert!(parsed_query.within_clause.is_some());

        if let Some(ref connected) = parsed_query.connected_clause {
            assert_eq!(connected.target_label, "doc");
            assert_eq!(connected.relationship_type, "quantum");
        }

        if let Some(ref within) = parsed_query.within_clause {
            assert_eq!(within.min_hops, 2);
            assert_eq!(within.max_hops, 2);
        }
    }

    #[test]
    fn test_parse_within_range_query() {
        let query = r#"MATCH (doc:Document) CONNECTEDTO(doc, "TAGGED_WITH", "quantum") WITHIN 2 TO 5 HOPS RETURN doc.id LIMIT 20"#;
        let result = parse_query(query);
        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.connected_clause.is_some());
        assert!(parsed_query.within_clause.is_some());

        if let Some(ref within) = parsed_query.within_clause {
            assert_eq!(within.min_hops, 2);
            assert_eq!(within.max_hops, 5);
        }
    }

    #[test]
    fn test_node_pattern_parsing() {
        let input = "(node:Label)";
        let result = parse_node_pattern(input);
        assert!(result.is_ok());

        let (_, node_pattern) = result.unwrap();
        assert_eq!(node_pattern.alias, "node");
        assert_eq!(node_pattern.label, "Label");
        assert!(node_pattern.properties.is_none());
    }

    #[test]
    fn test_number_parsing() {
        let result = parse_number("0.3");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().1, 0.3);

        let result = parse_number("-0.75");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().1, -0.75);
    }

    #[test]
    fn test_match_clause_parsing() {
        let input = "MATCH (node:Label)";
        let result = parse_match_clause(input);
        assert!(result.is_ok());
    }
}
