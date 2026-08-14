use axum::{
    http::{header, StatusCode},
    response::Response,
};
pub async fn admin_ui_handler() -> Response {
    std::fs::read("static/admin.html")
        .map(|bytes| {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(bytes.into())
                .unwrap()
        })
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("Admin UI not found".into())
                .unwrap()
        })
}
