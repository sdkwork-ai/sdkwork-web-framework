//! Admin-api builder auto-wires SQLx readiness for `/readyz`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use sdkwork_web_bootstrap::WebFramework;
use sdkwork_web_core::DefaultWebRequestContextResolver;
use sdkwork_web_store_sqlx::connect_sqlite;
use sdkwork_web_test_utils::IsolatedDeploymentEnv;
use tower::ServiceExt;

#[tokio::test]
async fn enable_admin_api_wires_sqlite_readiness_probe() {
    let _env = IsolatedDeploymentEnv::enter();
    let pool = connect_sqlite("sqlite::memory:", 1).await.expect("pool");
    let framework = WebFramework::builder(DefaultWebRequestContextResolver::default())
        .enable_admin_api(pool)
        .build();
    let app = framework.mount_service_routes(Router::new());

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

#[tokio::test]
async fn enable_admin_api_auto_manifest_contract_fallback_returns_501() {
    use axum::body::to_bytes;
    use axum::http::Method;
    use sdkwork_routes_web_framework_backend_api::paths;

    let _env = IsolatedDeploymentEnv::enter();
    let pool = connect_sqlite("sqlite::memory:", 1).await.expect("pool");
    let framework = WebFramework::builder(DefaultWebRequestContextResolver::default())
        .enable_admin_api(pool)
        .build();
    assert!(
        framework
            .service_router_config()
            .contract_fallback
            .is_some(),
        "enable_admin_api must auto-wire contract fallback from ROUTES"
    );

    let app = framework.mount_service_routes(Router::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(paths::cors::PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("manifest route");
    assert_eq!(StatusCode::NOT_IMPLEMENTED, response.status());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        "https://sdkwork.dev/problems/not-implemented",
        payload["type"].as_str().expect("type")
    );
    assert!(payload.get("requestId").is_none());
    assert!(payload.get("traceId").is_some());
}
