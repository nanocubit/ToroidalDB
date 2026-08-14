pub mod schema;

pub use schema::*;

use async_graphql::Request as GqlRequest;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GraphQLState {
    pub schema: ToroidalSchema,
}

impl GraphQLState {
    pub fn new(schema: ToroidalSchema) -> Self {
        GraphQLState { schema }
    }
}

async fn graphql_handler(
    State(state): State<Arc<RwLock<GraphQLState>>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let state = state.read().await;
    state.schema.execute(req.into_inner()).await.into()
}

async fn graphql_playground() -> impl IntoResponse {
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>ToroidalDB GraphQL</title>
        <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/graphql-playground-react/build/static/css/index.css" />
        <script src="https://cdn.jsdelivr.net/npm/graphql-playground-react/build/static/js/middleware.js"></script>
    </head>
    <body>
        <div id="root"></div>
        <script>
            window.addEventListener('load', function() {
                GraphQLPlayground.init(document.getElementById('root'), {
                    endpoint: '/graphql',
                });
            });
        </script>
    </body>
    </html>
    "#;

    (StatusCode::OK, [("content-type", "text/html")], html)
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "toroidal-db-graphql"
    }))
}

pub fn create_routes(state: Arc<RwLock<GraphQLState>>) -> Router {
    Router::new()
        .route("/graphql", post(graphql_handler))
        .route("/graphql", get(graphql_playground))
        .route("/health", get(health_check))
        .with_state(state)
}

pub async fn start_graphql_server(
    schema: ToroidalSchema,
    host: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = Arc::new(RwLock::new(GraphQLState::new(schema)));
    let app = create_routes(state);

    let addr = format!("{}:{}", host, port);
    println!("GraphQL server starting on http://{}/graphql", addr);
    println!("Playground available at http://{}/graphql", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_schema() {
        let schema = create_schema();
        assert!(!schema.schema().as_str().is_empty());
    }

    #[test]
    fn test_health_check() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let response = health_check().await.into_response();
            assert_eq!(response.status(), StatusCode::OK);
        });
    }
}
