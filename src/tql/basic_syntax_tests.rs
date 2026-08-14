#[cfg(test)]
mod basic_syntax_tests {
    use crate::tql::ast::*;
    use crate::tql::parser;

    #[test]
    fn test_basic_match_parsing() {
        let query = "MATCH (node:Label) RETURN node.id LIMIT 10";
        let result = parser::parse_query(query);
        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.match_clause.is_some());
        assert_eq!(parsed_query.limit, 10);
        if let Some(ref match_clause) = parsed_query.match_clause {
            assert_eq!(match_clause.source.label, "Label");
            assert_eq!(match_clause.source.alias, "node");
        }
    }

    #[test]
    fn test_match_where_return_limit_parsing() {
        let query =
            "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 5";
        let result = parser::parse_query(query);
        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.match_clause.is_some());
        assert!(parsed_query.where_clause.is_some());
        assert_eq!(parsed_query.limit, 5);

        if let Some(ref match_clause) = parsed_query.match_clause {
            assert_eq!(match_clause.source.label, "Document");
        }

        if let Some(ref where_clause) = parsed_query.where_clause {
            match where_clause {
                WhereCondition::ToroidalDistance { field, threshold } => {
                    assert_eq!(field, "doc.vector");
                    assert_eq!(*threshold, 0.3);
                }
                _ => panic!("Expected ToroidalDistance condition"),
            }
        }
    }

    #[test]
    fn test_multiple_return_fields() {
        let query = "MATCH (user:User) WHERE TOROIDALDISTANCE(user.profile, 0.4) RETURN user.id, user.name, user.email LIMIT 20";
        let result = parser::parse_query(query);
        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.match_clause.is_some());
        assert!(parsed_query.where_clause.is_some());
        assert_eq!(parsed_query.return_fields.len(), 3);
        assert_eq!(parsed_query.limit, 20);

        assert!(parsed_query.return_fields.contains(&"user.id".to_string()));
        assert!(parsed_query
            .return_fields
            .contains(&"user.name".to_string()));
        assert!(parsed_query
            .return_fields
            .contains(&"user.email".to_string()));
    }

    #[test]
    fn test_different_thresholds() {
        let query = "MATCH (item:Item) WHERE TOROIDALDISTANCE(item.embedding, 0.15) RETURN item.id LIMIT 100";
        let result = parser::parse_query(query);
        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.where_clause.is_some());
        assert_eq!(parsed_query.limit, 100);

        if let Some(ref where_clause) = parsed_query.where_clause {
            match where_clause {
                WhereCondition::ToroidalDistance { field, threshold } => {
                    assert_eq!(field, "item.embedding");
                    assert_eq!(*threshold, 0.15);
                }
                _ => panic!("Expected ToroidalDistance condition"),
            }
        }
    }

    #[test]
    fn test_various_labels() {
        let queries = vec![
            "MATCH (n:Node) RETURN n.id LIMIT 1",
            "MATCH (doc:Document) RETURN doc.id LIMIT 10",
            "MATCH (usr:User) WHERE TOROIDALDISTANCE(usr.vector, 0.5) RETURN usr.id LIMIT 5",
            "MATCH (prod:Product) WHERE TOROIDALDISTANCE(prod.features, 0.2) RETURN prod.id, prod.name LIMIT 25",
            "MATCH (post:Post) WHERE TOROIDALDISTANCE(post.content, 0.35) RETURN post.id, post.title, post.author LIMIT 15",
        ];

        for (i, query) in queries.iter().enumerate() {
            let result = parser::parse_query(query);
            assert!(result.is_ok(), "Query {} failed: {}", i + 1, query);

            let (_, parsed_query) = result.unwrap();
            assert!(
                parsed_query.match_clause.is_some(),
                "Query {} - match clause missing",
                i + 1
            );
            assert!(
                parsed_query.return_fields.len() >= 1,
                "Query {} - return fields missing",
                i + 1
            );
            assert!(
                parsed_query.limit > 0,
                "Query {} - limit should be positive",
                i + 1
            );
        }
    }
}
