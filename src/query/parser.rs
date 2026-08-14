use crate::math::MatryoshkaDim;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{alphanumeric1, char, digit1, space0},
    combinator::{map, opt, recognize},
    multi::separated_list0,
    number::complete::float,
    sequence::{delimited, preceded, tuple},
    IResult,
}; // ← ЕДИНЫЙ ТИП ИЗ MATH

#[derive(Debug, Clone, PartialEq)]
pub enum TQLQuery {
    ToroidalSearch {
        collection: String,
        vector: Vec<f32>,
        threshold: f32,
        dimension: Option<MatryoshkaDim>, // ← Используем единый тип
    },
    AddEdge {
        from_id: u64,
        to_id: u64,
        relation_type: String,
        weight: f32,
    },
    GetNeighbors {
        node_id: u64,
    },
    GraphSearch {
        start_id: u64,
        max_depth: usize,
    },
}

fn parse_u64(input: &str) -> IResult<&str, u64> {
    map(recognize(digit1), |s: &str| s.parse::<u64>().unwrap())(input)
}

fn parse_float(input: &str) -> IResult<&str, f32> {
    let (input, sign) = opt(char('-'))(input)?;
    let (input, num) = float(input)?;
    Ok((input, if sign.is_some() { -num } else { num }))
}

fn parse_vector(input: &str) -> IResult<&str, Vec<f32>> {
    delimited(
        char('['),
        preceded(
            space0,
            separated_list0(tuple((space0, char(','), space0)), parse_float),
        ),
        preceded(space0, char(']')),
    )(input)
}

fn parse_string_literal(input: &str) -> IResult<&str, String> {
    delimited(
        char('"'),
        map(
            nom::bytes::complete::take_while(|c: char| c != '"'),
            |s: &str| s.to_string(),
        ),
        char('"'),
    )(input)
}

fn parse_dimension(input: &str) -> IResult<&str, Option<MatryoshkaDim>> {
    if input.trim_start().starts_with(',') {
        let (input, _) = tuple((space0, char(','), space0))(input)?;
        let (input, dim_str) = alphanumeric1(input)?;
        let dim = match dim_str.to_lowercase().as_str() {
            "d384" => Some(MatryoshkaDim::D384),
            "d768" => Some(MatryoshkaDim::D768),
            "d1024" => Some(MatryoshkaDim::D1024),
            "d1536" => Some(MatryoshkaDim::D1536),
            _ => None,
        };
        Ok((input, dim))
    } else {
        Ok((input, None))
    }
}

pub fn parse_tql(input: &str) -> IResult<&str, TQLQuery> {
    let (input, _) = space0(input)?;

    let (input, command) = alt((
        tag("toroidal_search"),
        tag("TOROIDAL_SEARCH"),
        tag("add_edge"),
        tag("ADD_EDGE"),
        tag("get_neighbors"),
        tag("GET_NEIGHBORS"),
        tag("graph_search"),
        tag("GRAPH_SEARCH"),
    ))(input)?;

    let (input, _) = space0(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = space0(input)?;

    let (input, query) = match command.to_lowercase().as_str() {
        "toroidal_search" => {
            let (input, collection) = parse_string_literal(input)?;
            let (input, _) = tuple((space0, char(','), space0))(input)?;
            let (input, vector) = parse_vector(input)?;
            let (input, threshold) = if input.trim_start().starts_with(',') {
                let (input, _) = tuple((space0, char(','), space0))(input)?;
                let (input, thresh) = parse_float(input)?;
                (input, thresh)
            } else {
                (input, 0.3)
            };
            let (input, dimension) = parse_dimension(input)?;
            (
                input,
                TQLQuery::ToroidalSearch {
                    collection,
                    vector,
                    threshold,
                    dimension,
                },
            )
        }
        "add_edge" => {
            let (input, from_id) = parse_u64(input)?;
            let (input, _) = tuple((space0, char(','), space0))(input)?;
            let (input, to_id) = parse_u64(input)?;
            let (input, _) = tuple((space0, char(','), space0))(input)?;
            let (input, relation_type) = parse_string_literal(input)?;
            let (input, _) = tuple((space0, char(','), space0))(input)?;
            let (input, weight) = parse_float(input)?;
            (
                input,
                TQLQuery::AddEdge {
                    from_id,
                    to_id,
                    relation_type,
                    weight,
                },
            )
        }
        "get_neighbors" => {
            let (input, node_id) = parse_u64(input)?;
            (input, TQLQuery::GetNeighbors { node_id })
        }
        "graph_search" => {
            let (input, start_id) = parse_u64(input)?;
            let (input, _) = tuple((space0, char(','), space0))(input)?;
            let (input, max_depth) = map(digit1, |s: &str| s.parse::<usize>().unwrap())(input)?;
            (
                input,
                TQLQuery::GraphSearch {
                    start_id,
                    max_depth,
                },
            )
        }
        _ => {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    };

    let (input, _) = space0(input)?;
    let (input, _) = char(')')(input)?;
    let (input, _) = space0(input)?;

    Ok((input, query))
}
