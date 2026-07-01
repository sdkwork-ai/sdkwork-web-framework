//! SQLx-backed bootstrap integration tests.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use sdkwork_web_bootstrap::{service_router, ServiceRouterConfig};
use sdkwork_web_store_sqlx::connect_sqlite;
use tower::ServiceExt;

#[tokio::test]
async fn sqlite_readiness_reports_ready_when_store_is_healthy() {
    let pool = connect_sqlite("sqlite::memory:", 1).await.expect("pool");
    let app = service_router(
        Router::new(),
        ServiceRouterConfig::default().with_sqlite_readiness(pool),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("readyz");
    assert_eq!(StatusCode::OK, response.status());
}
