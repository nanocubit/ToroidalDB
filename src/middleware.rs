use axum::{extract::Request, middleware::Next, response::Response};
use tracing::info;

pub async fn log_requests(req: Request, next: Next) -> Response {
    let start = std::time::Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();

    let response = next.run(req).await;

    let duration = start.elapsed();
    info!("{} {} - {}ms", method, uri, duration.as_millis());

    response
}
