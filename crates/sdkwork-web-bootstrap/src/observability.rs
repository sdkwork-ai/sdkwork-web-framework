pub use sdkwork_web_core::HttpMetricsRegistry;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::sync::Arc;

pub async fn metrics_handler(registry: Arc<HttpMetricsRegistry>) -> impl IntoResponse {
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        registry.render_prometheus(),
    )
}
