use toroidal_db::tql::parser::parse_query;

fn main() {
    let inputs = [
        "MATCH (a:User)-[:F]->(b:User) RETURN b.id",
        "MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.id, b.id",
        "MATCH (a:User)-[:F]- (b:User) RETURN b.id",
        "MATCH (a:User)-[:F]-(b:User) RETURN b.id",
    ];
    for input in inputs {
        match parse_query(input) {
            Ok((rest, q)) => println!(
                "OK   rest={:?} dir={:?} | {:?}",
                rest,
                q.match_clause
                    .as_ref()
                    .and_then(|m| m.relationship.as_ref())
                    .map(|r| r.direction.clone()),
                input
            ),
            Err(e) => println!("ERR  {:?} | {:?}", e, input),
        }
    }
}
