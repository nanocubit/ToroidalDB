use crate::tql::ast::{
    DataType, DdlStatement, EdgeTypeDef, FieldConstraint, FieldDef, NodeTypeDef,
};
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case},
    character::complete::{char, digit1, multispace0, multispace1},
    combinator::{map, map_res, opt},
    multi::{many0, separated_list0},
    sequence::{delimited, preceded, tuple},
    IResult,
};

fn parse_whitespace(input: &str) -> IResult<&str, ()> {
    let (input, _) = multispace0(input)?;
    Ok((input, ()))
}

fn parse_identifier(input: &str) -> IResult<&str, &str> {
    let (input, _) = parse_whitespace(input)?;
    let (input, ident) =
        nom::bytes::complete::take_while1(|c: char| c.is_alphanumeric() || c == '_')(input)?;
    Ok((input, ident))
}

fn parse_data_type(input: &str) -> IResult<&str, DataType> {
    let (input, _) = parse_whitespace(input)?;

    alt((
        map(tag_no_case("INT"), |_| DataType::Int),
        map(tag_no_case("FLOAT"), |_| DataType::Float),
        map(tag_no_case("BOOL"), |_| DataType::Bool),
        map(tag_no_case("TEXT"), |_| DataType::Text),
        map(tag_no_case("TIMESTAMP"), |_| DataType::Timestamp),
        map(
            delimited(
                tuple((tag_no_case("VECTOR"), parse_whitespace, char('('))),
                map_res(digit1, |s: &str| s.parse::<u32>()),
                char(')'),
            ),
            DataType::Vector,
        ),
    ))(input)
}

fn parse_field_constraint(input: &str) -> IResult<&str, FieldConstraint> {
    let (input, _) = parse_whitespace(input)?;

    alt((
        map(tag_no_case("NOT NULL"), |_| FieldConstraint::NotNull),
        map(tag_no_case("UNIQUE"), |_| FieldConstraint::Unique),
        map(tag_no_case("INDEX"), |_| FieldConstraint::Index),
        map(
            tuple((
                tag_no_case("VECTOR_INDEX"),
                parse_whitespace,
                char('('),
                tag_no_case("phi"),
                parse_whitespace,
                char('='),
                parse_whitespace,
                nom::number::complete::float,
                char(')'),
            )),
            |(_, _, _, _, _, _, _, phi, _)| FieldConstraint::VectorIndex { phi },
        ),
    ))(input)
}

fn parse_field_def(input: &str) -> IResult<&str, FieldDef> {
    let (input, _) = parse_whitespace(input)?;

    let (input, name) = parse_identifier(input)?;
    let (input, _) = multispace1(input)?;
    let (input, data_type) = parse_data_type(input)?;

    // Parse PRIMARY KEY
    let (input, is_primary_key) = opt(preceded(multispace1, tag_no_case("PRIMARY KEY")))(input)?;
    let is_primary_key = is_primary_key.is_some();

    // Parse additional constraints
    let (input, constraints) = many0(preceded(parse_whitespace, parse_field_constraint))(input)?;

    Ok((
        input,
        FieldDef {
            name: name.to_string(),
            data_type,
            is_primary_key,
            constraints,
        },
    ))
}

fn parse_field_list(input: &str) -> IResult<&str, Vec<FieldDef>> {
    let (input, _) = parse_whitespace(input)?;
    let (input, _) = char('(')(input)?;
    let (input, fields) = separated_list0(
        tuple((parse_whitespace, char(','), parse_whitespace)),
        parse_field_def,
    )(input)?;
    // Допускаем пробелы/переводы строк перед закрывающей скобкой
    let (input, _) = parse_whitespace(input)?;
    let (input, _) = char(')')(input)?;
    Ok((input, fields))
}

fn parse_create_node_type(input: &str) -> IResult<&str, DdlStatement> {
    let (input, _) = parse_whitespace(input)?;

    let (input, _) = tag_no_case("CREATE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("NODE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("TYPE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, fields) = parse_field_list(input)?;

    Ok((
        input,
        DdlStatement::CreateNodeType(NodeTypeDef {
            name: name.to_string(),
            fields,
        }),
    ))
}

fn parse_create_edge_type(input: &str) -> IResult<&str, DdlStatement> {
    let (input, _) = parse_whitespace(input)?;

    let (input, _) = tag_no_case("CREATE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("EDGE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("TYPE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = parse_identifier(input)?;
    let (input, _) = parse_whitespace(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = parse_whitespace(input)?;

    // Parse FROM and TO
    let (input, _) = tag_no_case("from")(input)?;
    let (input, _) = parse_whitespace(input)?;
    let (input, from) = parse_identifier(input)?;
    let (input, _) = parse_whitespace(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = parse_whitespace(input)?;

    let (input, _) = tag_no_case("to")(input)?;
    let (input, _) = parse_whitespace(input)?;
    let (input, to) = parse_identifier(input)?;

    // Optional fields
    let (input, fields) = if input.starts_with(',') {
        let (input, _) = char(',')(input)?;
        let (input, _) = parse_whitespace(input)?;
        separated_list0(
            tuple((parse_whitespace, char(','), parse_whitespace)),
            parse_field_def,
        )(input)?
    } else {
        (input, vec![])
    };

    let (input, _) = parse_whitespace(input)?;
    let (input, _) = char(')')(input)?;

    Ok((
        input,
        DdlStatement::CreateEdgeType(EdgeTypeDef {
            name: name.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            fields,
        }),
    ))
}

fn parse_drop_node_type(input: &str) -> IResult<&str, DdlStatement> {
    let (input, _) = parse_whitespace(input)?;

    let (input, _) = tag_no_case("DROP")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("NODE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("TYPE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = parse_identifier(input)?;

    Ok((input, DdlStatement::DropNodeType(name.to_string())))
}

fn parse_drop_edge_type(input: &str) -> IResult<&str, DdlStatement> {
    let (input, _) = parse_whitespace(input)?;

    let (input, _) = tag_no_case("DROP")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("EDGE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("TYPE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, name) = parse_identifier(input)?;

    Ok((input, DdlStatement::DropEdgeType(name.to_string())))
}

fn parse_show_schema(input: &str) -> IResult<&str, DdlStatement> {
    let (input, _) = parse_whitespace(input)?;

    let (input, _) = tag_no_case("SHOW")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("SCHEMA")(input)?;

    Ok((input, DdlStatement::ShowSchema))
}

pub fn parse_ddl_statement(input: &str) -> IResult<&str, DdlStatement> {
    let (input, _) = parse_whitespace(input)?;

    alt((
        parse_create_node_type,
        parse_create_edge_type,
        parse_drop_node_type,
        parse_drop_edge_type,
        parse_show_schema,
    ))(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_node_type() {
        let input = r#"CREATE NODE TYPE Document (
            id INT PRIMARY KEY,
            title TEXT,
            content_t3 VECTOR(1536) VECTOR_INDEX(phi = 5.71)
        )"#;

        let result = parse_ddl_statement(input);
        assert!(result.is_ok());

        let (_, stmt) = result.unwrap();
        match stmt {
            DdlStatement::CreateNodeType(def) => {
                assert_eq!(def.name, "Document");
                assert_eq!(def.fields.len(), 3);

                // Check primary key
                assert!(def.fields[0].is_primary_key);
                assert!(!def.fields[1].is_primary_key);

                // Check vector field
                match &def.fields[2].data_type {
                    DataType::Vector(1536) => (),
                    _ => panic!("Expected VECTOR(1536)"),
                }
            }
            _ => panic!("Expected CreateNodeType"),
        }
    }

    #[test]
    fn test_parse_create_edge_type() {
        let input = r#"CREATE EDGE TYPE AUTHORED (
            from Author,
            to Document,
            weight FLOAT
        )"#;

        let result = parse_ddl_statement(input);
        assert!(result.is_ok());

        let (_, stmt) = result.unwrap();
        match stmt {
            DdlStatement::CreateEdgeType(def) => {
                assert_eq!(def.name, "AUTHORED");
                assert_eq!(def.from, "Author");
                assert_eq!(def.to, "Document");
                assert_eq!(def.fields.len(), 1);
            }
            _ => panic!("Expected CreateEdgeType"),
        }
    }

    #[test]
    fn test_parse_drop_node_type() {
        let input = "DROP NODE TYPE Document";

        let result = parse_ddl_statement(input);
        assert!(result.is_ok());

        let (_, stmt) = result.unwrap();
        match stmt {
            DdlStatement::DropNodeType(name) => {
                assert_eq!(name, "Document");
            }
            _ => panic!("Expected DropNodeType"),
        }
    }

    #[test]
    fn test_parse_show_schema() {
        let input = "SHOW SCHEMA";

        let result = parse_ddl_statement(input);
        assert!(result.is_ok());

        let (_, stmt) = result.unwrap();
        match stmt {
            DdlStatement::ShowSchema => (),
            _ => panic!("Expected ShowSchema"),
        }
    }
}
