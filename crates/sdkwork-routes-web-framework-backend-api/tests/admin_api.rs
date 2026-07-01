use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use sdkwork_routes_web_framework_backend_api::build_admin_router;
use sdkwork_routes_web_framework_backend_api::paths;
use sdkwork_routes_web_framework_backend_api::ROUTES;
use sdkwork_web_axum::with_web_request_context;
use sdkwork_web_core::DefaultWebRequestContextResolver;
use sdkwork_web_core::{HttpRouteManifest, ManifestAuthorizationPolicy};
use sdkwork_web_store_sqlx::connect_sqlite;
use sdkwork_web_test_utils::{fixtures, TestRuntimeBuilder};
use std::sync::Arc;
use tower::ServiceExt;

async fn test_pool() -> sqlx::SqlitePool {
    connect_sqlite("sqlite::memory:", 1).await.expect("pool")
}

fn protected_app(pool: sqlx::SqlitePool) -> Router {
    let manifest = HttpRouteManifest::new(ROUTES);
    let layer = TestRuntimeBuilder::new(DefaultWebRequestContextResolver::default())
        .build_layer()
        .with_authorization_policy(Arc::new(ManifestAuthorizationPolicy::new(manifest)));
    with_web_request_context(build_admin_router(pool.clone()), layer)
}

fn dual_token_request(method: &str, uri: &str, body: Option<&str>) -> Request<Body> {
    dual_token_request_with_auth(method, uri, body, fixtures::auth_token())
}

fn dual_token_request_with_auth(
    method: &str,
    uri: &str,
    body: Option<&str>,
    auth_token: String,
) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", fixtures::access_token());
    if let Some(payload) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(payload.to_owned()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    }
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&body).expect("json")
}

fn control_node_by_id(node_id: &str) -> String {
    paths::control_nodes::BY_ID.replace("{nodeId}", node_id)
}

fn control_node_heartbeat(node_id: &str) -> String {
    paths::control_nodes::HEARTBEAT.replace("{nodeId}", node_id)
}

fn audit_events_query(limit: Option<u32>) -> String {
    match limit {
        Some(value) => format!("{}?limit={value}", paths::audit_events::PATH),
        None => paths::audit_events::PATH.to_owned(),
    }
}

#[tokio::test]
async fn admin_api_rejects_unauthenticated_requests() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri(paths::runtime_defaults::PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
}

#[tokio::test]
async fn admin_api_lists_runtime_defaults() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "GET",
            paths::runtime_defaults::PATH,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
}

#[tokio::test]
async fn admin_api_upserts_cors_policy() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(
            dual_token_request(
                "PUT",
                paths::cors::PATH,
                Some(
                    r#"{"tenantId":"100001","environment":"prod","allowAllOrigins":false,"allowedOrigins":["https://app.example"],"allowCredentials":true}"#,
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
}

#[tokio::test]
async fn admin_api_upserts_rate_limit_policy() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "PUT",
            paths::rate_limit::PATH,
            Some(
                r#"{"tenantId":"100001","environment":"prod","tierKey":"default","maxRequests":100,"windowSecs":60,"enabled":true}"#,
            ),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let payload = response_json(response).await;
    assert_eq!(100, payload["data"]["maxRequests"].as_u64().unwrap());
}

#[tokio::test]
async fn admin_api_upserts_tenant_runtime_profile() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "PUT",
            paths::tenant_runtime::PATH,
            Some(
                r#"{"tenantId":"100001","environment":"prod","rateLimitEnabled":true,"maxContentLength":"1048576","maxConcurrentRequests":32}"#,
            ),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let payload = response_json(response).await;
    assert_eq!(
        32,
        payload["data"]["maxConcurrentRequests"].as_u64().unwrap()
    );
}

#[tokio::test]
async fn admin_api_rejects_cross_tenant_cors_upsert() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(
            dual_token_request(
                "PUT",
                paths::cors::PATH,
                Some(
                    r#"{"tenantId":"other-tenant","environment":"prod","allowAllOrigins":false,"allowedOrigins":["https://evil.example"],"allowCredentials":false}"#,
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(StatusCode::FORBIDDEN, response.status());
}

#[tokio::test]
async fn admin_api_rejects_unsafe_prod_cors_policy() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(
            dual_token_request(
                "PUT",
                paths::cors::PATH,
                Some(
                    r#"{"tenantId":"100001","environment":"prod","allowAllOrigins":true,"allowedOrigins":[],"allowCredentials":true}"#,
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn admin_api_rejects_invalid_tenant_runtime_profile() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "PUT",
            paths::tenant_runtime::PATH,
            Some(r#"{"tenantId":"100001","environment":"prod","maxContentLength":"999999999999"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn admin_api_rejects_invalid_rate_limit_policy() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(
            dual_token_request(
                "PUT",
                paths::rate_limit::PATH,
                Some(
                    r#"{"tenantId":"100001","environment":"prod","tierKey":"default","maxRequests":0,"windowSecs":60,"enabled":true}"#,
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn admin_api_registers_control_node() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request_with_auth(
            "POST",
            paths::control_nodes::COLLECTION,
            Some(r#"{"nodeId":"node-1","baseUrl":"https://node.example","environment":"prod"}"#),
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::CREATED, response.status());
}

#[tokio::test]
async fn admin_api_reregister_control_node_returns_ok_and_preserves_created_at() {
    let pool = test_pool().await;
    let app = protected_app(pool.clone());
    let body =
        r#"{"nodeId":"node-reregister","baseUrl":"https://node.example","environment":"prod"}"#;
    let first = app
        .clone()
        .oneshot(dual_token_request_with_auth(
            "POST",
            paths::control_nodes::COLLECTION,
            Some(body),
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::CREATED, first.status());
    let first_json = response_json(first).await;
    // API_SPEC §13: int64 fields serialize as string
    let created_at_str = first_json["data"]["createdAt"].as_str().expect("createdAt");
    let created_at: i64 = created_at_str.parse().expect("createdAt parse");

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let second = app
        .oneshot(dual_token_request_with_auth(
            "POST",
            paths::control_nodes::COLLECTION,
            Some(
                r#"{"nodeId":"node-reregister","baseUrl":"https://node2.example","environment":"prod"}"#,
            ),
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, second.status());
    let second_json = response_json(second).await;
    let second_created_at: i64 = second_json["data"]["createdAt"]
        .as_str()
        .expect("createdAt")
        .parse()
        .unwrap();
    assert_eq!(created_at, second_created_at);
    let last_heartbeat: i64 = second_json["data"]["lastHeartbeatAt"]
        .as_str()
        .expect("lastHeartbeatAt")
        .parse()
        .unwrap();
    assert!(last_heartbeat >= created_at);
    assert_eq!(
        "https://node2.example",
        second_json["data"]["baseUrl"].as_str().unwrap()
    );
}

#[tokio::test]
async fn admin_api_heartbeats_and_deletes_control_node() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let register = app
        .clone()
        .oneshot(dual_token_request_with_auth(
            "POST",
            paths::control_nodes::COLLECTION,
            Some(r#"{"nodeId":"node-2","baseUrl":"https://node2.example","environment":"prod"}"#),
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::CREATED, register.status());

    let heartbeat = app
        .clone()
        .oneshot(dual_token_request_with_auth(
            "POST",
            &control_node_heartbeat("node-2"),
            None,
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, heartbeat.status());

    let delete = app
        .oneshot(dual_token_request_with_auth(
            "DELETE",
            &control_node_by_id("node-2"),
            None,
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::NO_CONTENT, delete.status());
}

#[tokio::test]
async fn admin_api_returns_not_found_for_missing_control_node() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request_with_auth(
            "POST",
            &control_node_heartbeat("missing-node"),
            None,
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    let payload = response_json(response).await;
    assert_eq!(
        "https://sdkwork.dev/problems/not-found",
        payload["type"].as_str().unwrap()
    );
}

#[tokio::test]
async fn admin_api_audit_events_exclude_null_tenant_rows_for_tenant_admin() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO web_audit_event (request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at) \
         VALUES ('req-global', NULL, NULL, 'backendApi', '/healthz', 'GET', NULL, 200, 1, 1)",
    )
    .execute(&pool)
    .await
    .expect("insert global audit row");
    sqlx::query(
        "INSERT INTO web_audit_event (request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at) \
         VALUES ('req-tenant', '100001', 'user-test', 'backendApi', ?1, 'GET', 'webFramework.runtimeDefaults.snapshot', 200, 2, 2)",
    )
    .bind(paths::runtime_defaults::PATH)
    .execute(&pool)
    .await
    .expect("insert tenant audit row");

    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "GET",
            &audit_events_query(Some(10)),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let payload = response_json(response).await;
    let rows = payload["data"].as_array().expect("audit rows");
    assert_eq!(1, rows.len());
    assert_eq!("100001", rows[0]["tenantId"].as_str().unwrap());
}

#[tokio::test]
async fn admin_api_platform_read_can_list_global_audit_rows() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO web_audit_event (request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at) \
         VALUES ('req-global', NULL, NULL, 'backendApi', '/healthz', 'GET', NULL, 200, 1, 1)",
    )
    .execute(&pool)
    .await
    .expect("insert global audit row");

    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request_with_auth(
            "GET",
            &audit_events_query(Some(10)),
            None,
            fixtures::auth_token_platform_read(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let payload = response_json(response).await;
    let rows = payload["data"].as_array().expect("audit rows");
    assert_eq!(1, rows.len());
    assert!(rows[0]["tenantId"].is_null());
}

#[tokio::test]
async fn admin_api_maps_database_errors_to_503_problem_json() {
    let pool = test_pool().await;
    let app = protected_app(pool.clone());
    pool.close().await;
    let response = app
        .oneshot(dual_token_request("GET", paths::cors::PATH, None))
        .await
        .unwrap();
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
    let payload = response_json(response).await;
    assert_eq!(503, payload["status"].as_u64().unwrap());
    assert_eq!(
        "https://sdkwork.dev/problems/dependency-unavailable",
        payload["type"].as_str().unwrap()
    );
}

#[tokio::test]
async fn admin_api_rejects_invalid_control_node_base_url() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request_with_auth(
            "POST",
            paths::control_nodes::COLLECTION,
            Some(r#"{"nodeId":"node-a","baseUrl":"not-a-url","environment":"prod"}"#),
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn admin_api_rejects_tenant_admin_on_control_plane_routes() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    for uri in [
        paths::security_events::PATH,
        paths::control_nodes::COLLECTION,
    ] {
        let response = app
            .clone()
            .oneshot(dual_token_request("GET", uri, None))
            .await
            .unwrap();
        assert_eq!(
            StatusCode::FORBIDDEN,
            response.status(),
            "tenant admin must not access {uri}"
        );
        let payload = response_json(response).await;
        assert_eq!(403, payload["status"].as_u64().unwrap());
        assert!(
            payload["traceId"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "handler Problem+json must include traceId"
        );
    }
}

#[tokio::test]
async fn admin_handler_problem_includes_trace_id_from_context() {
    use sdkwork_web_core::{REQUEST_ID_HEADER, TRACEPARENT_HEADER};

    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(paths::security_events::PATH)
                .header(
                    "Authorization",
                    format!("Bearer {}", fixtures::auth_token()),
                )
                .header("Access-Token", fixtures::access_token())
                .header(REQUEST_ID_HEADER, "admin-req-correlation-1")
                .header(
                    TRACEPARENT_HEADER,
                    "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(StatusCode::FORBIDDEN, response.status());
    let payload = response_json(response).await;
    assert!(
        payload["traceId"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "handler Problem+json must include traceId"
    );
    assert_eq!(
        "4bf92f3577b34da6a3ce929d0e0e4736",
        payload["traceId"].as_str().unwrap()
    );
}

#[tokio::test]
async fn admin_api_rejects_oversized_cors_origin_list() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let origins = (0..257)
        .map(|index| format!("https://origin-{index}.example"))
        .collect::<Vec<_>>();
    let body = serde_json::json!({
        "tenantId": "100001",
        "environment": "prod",
        "allowAllOrigins": false,
        "allowedOrigins": origins,
        "allowCredentials": false,
    });
    let response = app
        .oneshot(dual_token_request(
            "PUT",
            paths::cors::PATH,
            Some(&body.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn admin_api_rejects_empty_tenant_id_on_upsert() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "PUT",
            paths::cors::PATH,
            Some(
                r#"{"tenantId":"   ","environment":"prod","allowAllOrigins":false,"allowedOrigins":["https://app.example"],"allowCredentials":false}"#,
            ),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn admin_api_rejects_zero_list_limit() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "GET",
            &audit_events_query(Some(0)),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

async fn seed_tenant_audit_rows(pool: &sqlx::SqlitePool, count: usize) {
    for index in 0..count {
        sqlx::query(
            "INSERT INTO web_audit_event (request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at) \
             VALUES (?1, '100001', 'user-test', 'backendApi', ?2, 'GET', 'webFramework.runtimeDefaults.snapshot', 200, 1, ?3)",
        )
        .bind(format!("req-{index}"))
        .bind(paths::runtime_defaults::PATH)
        .bind((index + 1) as i64)
        .execute(pool)
        .await
        .expect("insert tenant audit row");
    }
}

#[tokio::test]
async fn admin_api_defaults_list_limit_to_fifty() {
    let pool = test_pool().await;
    seed_tenant_audit_rows(&pool, 60).await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request("GET", &audit_events_query(None), None))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let payload = response_json(response).await;
    let rows = payload["data"].as_array().expect("audit rows");
    assert_eq!(50, rows.len());
}

#[tokio::test]
async fn admin_api_caps_list_limit_at_two_hundred() {
    let pool = test_pool().await;
    seed_tenant_audit_rows(&pool, 210).await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "GET",
            &audit_events_query(Some(500)),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let payload = response_json(response).await;
    let rows = payload["data"].as_array().expect("audit rows");
    assert_eq!(200, rows.len());
}

#[tokio::test]
async fn admin_api_rejects_allow_all_origins_without_credentials_in_prod() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request_with_auth(
            "PUT",
            paths::cors::PATH,
            Some(
                r#"{"tenantId":"100002","environment":"prod","allowAllOrigins":true,"allowedOrigins":[],"allowCredentials":false}"#,
            ),
            fixtures::auth_token_platform_read(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn admin_api_platform_read_can_upsert_other_tenant_cors_policy() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request_with_auth(
            "PUT",
            paths::cors::PATH,
            Some(
                r#"{"tenantId":"100002","environment":"prod","allowAllOrigins":false,"allowedOrigins":["https://other.example"],"allowCredentials":false}"#,
            ),
            fixtures::auth_token_platform_read(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let payload = response_json(response).await;
    assert_eq!("100002", payload["data"]["tenantId"].as_str().unwrap());
}

#[tokio::test]
async fn admin_api_lists_cors_policies() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request("GET", paths::cors::PATH, None))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    assert!(response_json(response).await["data"].is_array());
}

#[tokio::test]
async fn admin_api_lists_rate_limit_policies() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request("GET", paths::rate_limit::PATH, None))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    assert!(response_json(response).await["data"].is_array());
}

#[tokio::test]
async fn admin_api_lists_tenant_runtime_profiles() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request("GET", paths::tenant_runtime::PATH, None))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    assert!(response_json(response).await["data"].is_array());
}

#[tokio::test]
async fn admin_api_lists_control_nodes() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request_with_auth(
            "GET",
            paths::control_nodes::COLLECTION,
            None,
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    assert!(response_json(response).await["data"].is_array());
}

#[tokio::test]
async fn admin_api_lists_optional_features_snapshot() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "GET",
            paths::optional_features::PATH,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
}

#[tokio::test]
async fn admin_api_control_plane_can_list_security_events() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request_with_auth(
            "GET",
            paths::security_events::PATH,
            None,
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let payload = response_json(response).await;
    assert!(payload["data"].is_array());
}

#[tokio::test]
async fn admin_api_security_event_scope_narrows_to_requested_tenant() {
    // SECURITY_SPEC §5.1 / DATABASE_SPEC §6.3：tenant_id 隔离字段生效，
    // control-plane 可下钻到指定租户视图。
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO web_security_event \
         (kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at, expires_at) \
         VALUES \
           ('cors_denied', 'req-a', '100001', '/app/v3/api/users', 'POST', 'AppApi', 'https://evil.example', 'denied', 1, 1), \
           ('cors_denied', 'req-b', '100002', '/app/v3/api/users', 'POST', 'AppApi', 'https://evil.example', 'denied', 2, 2), \
           ('rate_limit_exceeded', 'req-c', '0', '/app/v3/api/auth/login', 'POST', 'AppApi', NULL, 'denied', 3, 3)",
    )
    .execute(&pool)
    .await
    .expect("seed");
    let app = protected_app(pool);
    // Narrowed view: only tenant 100001 row.
    let uri = format!("{}?tenant_id=100001", paths::security_events::PATH);
    let response = app
        .clone()
        .oneshot(dual_token_request_with_auth(
            "GET",
            &uri,
            None,
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let payload = response_json(response).await;
    let rows = payload["data"].as_array().expect("array");
    assert_eq!(1, rows.len());
    assert_eq!("100001", rows[0]["tenantId"].as_str().expect("tenant_id"));

    // Platform-wide view: all three rows.
    let response = app
        .oneshot(dual_token_request_with_auth(
            "GET",
            paths::security_events::PATH,
            None,
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    let payload = response_json(response).await;
    let rows = payload["data"].as_array().expect("array");
    assert_eq!(3, rows.len());
}

#[tokio::test]
async fn admin_api_security_event_rejects_tenant_admin() {
    // SECURITY_SPEC §5.1：安全事件为平台级敏感数据，tenant_admin 不可访问。
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request(
            "GET",
            paths::security_events::PATH,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::FORBIDDEN, response.status());
}

#[tokio::test]
async fn admin_api_rejects_cross_tenant_audit_query_for_tenant_admin() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let uri = format!("{}?tenant_id=100002&limit=10", paths::audit_events::PATH);
    let response = app
        .oneshot(dual_token_request("GET", &uri, None))
        .await
        .unwrap();
    assert_eq!(StatusCode::FORBIDDEN, response.status());
}

#[tokio::test]
async fn admin_api_rejects_invalid_control_node_id_in_path() {
    let pool = test_pool().await;
    let app = protected_app(pool);
    let response = app
        .oneshot(dual_token_request_with_auth(
            "POST",
            &control_node_heartbeat("bad..node"),
            None,
            fixtures::auth_token_control_plane(),
        ))
        .await
        .unwrap();
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}
