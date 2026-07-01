//! Bootstrap integration smoke tests.

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use axum::routing::get;
use axum::Router;
use sdkwork_web_axum::with_web_request_context;
use sdkwork_web_bootstrap::{
    assemble_multi_surface_router, build_openapi_document, contract_fallback_handler,
    service_router, ContractFallbackConfig, HttpMethod, HttpRoute, ReadinessCheck, ReadinessFuture,
    RouteAuth, ServiceRouterConfig, WebFramework, READINESS_DEPENDENCY_UNAVAILABLE,
};
use sdkwork_web_core::DefaultWebRequestContextResolver;
use sdkwork_web_core::{bootstrap_access_token_jwt, HttpRouteManifest, WebRequestContextProfile};
use sdkwork_web_test_utils::IsolatedDeploymentEnv;
use std::sync::Arc;
use tower::ServiceExt;

struct FailingReadiness;

impl ReadinessCheck for FailingReadiness {
    fn check(&self) -> ReadinessFuture<'_> {
        Box::pin(async { Err("sqlx: connection refused at 127.0.0.1:5432".into()) })
    }
}

const SAMPLE_ROUTES: &[HttpRoute] = &[HttpRoute::new(
    HttpMethod::Get,
    "/app/v3/api/users",
    "Users",
    "listUsers",
    RouteAuth::DualToken,
)];

#[test]
fn production_builder_sets_default_request_timeout() {
    use sdkwork_web_core::{
        tenant_bound_verifying_web_request_resolver, DefaultApiKeyLookupService,
        EnvBootstrapTenantSigningKeyLookup, WebFrameworkOptionalFeatures,
        PRODUCTION_DEFAULT_REQUEST_TIMEOUT_SECS,
    };
    use sdkwork_web_test_utils::{
        production_test_audit_emitter, production_test_security_event_emitter,
    };
    use std::time::Duration;

    let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let framework = WebFramework::builder(resolver)
        .production_defaults()
        .optional_features(
            WebFrameworkOptionalFeatures::production_sqlx().control_plane_standalone(),
        )
        .audit_emitter(production_test_audit_emitter())
        .security_event_emitter(production_test_security_event_emitter())
        .build();
    assert_eq!(
        Some(Duration::from_secs(PRODUCTION_DEFAULT_REQUEST_TIMEOUT_SECS)),
        framework.request_timeout()
    );
}

#[test]
fn production_builder_sets_default_shutdown_grace_period() {
    use sdkwork_web_core::{
        tenant_bound_verifying_web_request_resolver, DefaultApiKeyLookupService,
        EnvBootstrapTenantSigningKeyLookup, WebFrameworkOptionalFeatures,
        PRODUCTION_DEFAULT_SHUTDOWN_GRACE_SECS,
    };
    use sdkwork_web_test_utils::{
        production_test_audit_emitter, production_test_security_event_emitter,
    };
    use std::time::Duration;

    let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let framework = WebFramework::builder(resolver)
        .production_defaults()
        .optional_features(
            WebFrameworkOptionalFeatures::production_sqlx().control_plane_standalone(),
        )
        .audit_emitter(production_test_audit_emitter())
        .security_event_emitter(production_test_security_event_emitter())
        .build();
    assert_eq!(
        Some(Duration::from_secs(PRODUCTION_DEFAULT_SHUTDOWN_GRACE_SECS)),
        framework.shutdown_grace_period()
    );
}

#[test]
fn production_saas_builder_rejects_missing_readiness_probe() {
    use sdkwork_web_core::{
        tenant_bound_saas_verifying_web_request_resolver, DefaultApiKeyLookupService,
        EnvBootstrapTenantSigningKeyLookup, NoOpJwtSessionRevocationChecker,
    };
    use sdkwork_web_test_utils::{
        production_test_audit_emitter, production_test_security_event_emitter,
    };

    let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
    let resolver = tenant_bound_saas_verifying_web_request_resolver(
        lookup,
        NoOpJwtSessionRevocationChecker,
        DefaultApiKeyLookupService,
    );
    let result = std::panic::catch_unwind(|| {
        WebFramework::builder(resolver)
            .production_defaults()
            .audit_emitter(production_test_audit_emitter())
            .security_event_emitter(production_test_security_event_emitter())
            .build();
    });
    assert!(
        result.is_err(),
        "saas production must require readiness_check"
    );
}

#[test]
fn production_saas_builder_rejects_missing_iss_aud_claim_policy() {
    use async_trait::async_trait;
    use sdkwork_web_bootstrap::AlwaysReady;
    use sdkwork_web_core::{
        tenant_bound_saas_verifying_web_request_resolver, ConcurrentAdmissionStore,
        EnvBootstrapTenantSigningKeyLookup, IdempotencyBeginOutcome, IdempotencyResponseRecord,
        IdempotencyStore, NoOpJwtSessionRevocationChecker, RateLimitStore, WebFrameworkError,
    };
    use sdkwork_web_test_utils::{
        production_test_audit_emitter, production_test_security_event_emitter,
    };
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Clone)]
    struct StubApiKeyLookup;

    #[async_trait]
    impl sdkwork_web_core::ApiKeyLookupService for StubApiKeyLookup {
        async fn lookup_api_key(
            &self,
            _credential: &sdkwork_web_core::ApiKeyCredential,
        ) -> Result<sdkwork_web_core::ApiKeyPrincipalRecord, WebFrameworkError> {
            Err(WebFrameworkError::dependency_unavailable("stub"))
        }
    }

    #[derive(Clone)]
    struct TestRedisRateLimitStore;

    #[async_trait]
    impl RateLimitStore for TestRedisRateLimitStore {
        fn is_distributed_ha(&self) -> bool {
            true
        }

        async fn check_and_record(
            &self,
            _key: &str,
            _max_requests: u32,
            _window: Duration,
        ) -> Result<(), WebFrameworkError> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct TestRedisIdempotencyStore;

    #[async_trait]
    impl IdempotencyStore for TestRedisIdempotencyStore {
        fn is_distributed_ha(&self) -> bool {
            true
        }

        async fn begin(
            &self,
            _key: &str,
            _fingerprint: &str,
            _ttl: Duration,
        ) -> Result<IdempotencyBeginOutcome, WebFrameworkError> {
            Ok(IdempotencyBeginOutcome::Leader)
        }

        async fn complete(
            &self,
            _key: &str,
            _fingerprint: &str,
            _record: IdempotencyResponseRecord,
            _ttl: Duration,
        ) -> Result<(), WebFrameworkError> {
            Ok(())
        }

        async fn release(&self, _key: &str, _fingerprint: &str) -> Result<(), WebFrameworkError> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct TestRedisConcurrentAdmissionStore;

    #[async_trait]
    impl ConcurrentAdmissionStore for TestRedisConcurrentAdmissionStore {
        fn is_distributed_ha(&self) -> bool {
            true
        }

        async fn try_acquire(&self, _key: &str, _limit: u32) -> Result<(), WebFrameworkError> {
            Ok(())
        }

        async fn release(&self, _key: &str) -> Result<(), WebFrameworkError> {
            Ok(())
        }
    }

    let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
    let resolver = tenant_bound_saas_verifying_web_request_resolver(
        lookup,
        NoOpJwtSessionRevocationChecker,
        StubApiKeyLookup,
    );
    let result = std::panic::catch_unwind(|| {
        WebFramework::builder(resolver)
            .production_defaults()
            .audit_emitter(production_test_audit_emitter())
            .security_event_emitter(production_test_security_event_emitter())
            .readiness_check(Arc::new(AlwaysReady))
            .rate_limit_store(Arc::new(TestRedisRateLimitStore))
            .idempotency_store(Arc::new(TestRedisIdempotencyStore))
            .concurrent_admission_store(Arc::new(TestRedisConcurrentAdmissionStore))
            .build();
    });
    assert!(
        result.is_err(),
        "saas production must require iss/aud claim policy"
    );
}

#[tokio::test]
async fn web_framework_builder_produces_layer() {
    let _env = IsolatedDeploymentEnv::enter();
    let framework = WebFramework::builder(DefaultWebRequestContextResolver::default()).build();
    assert!(
        framework
            .layer()
            .runtime()
            .security_policy
            .rate_limit
            .window_secs
            > 0
    );
    let _ = framework.into_layer();
}

#[tokio::test]
async fn production_builder_rejects_dev_only_resolver() {
    let result = std::panic::catch_unwind(|| {
        WebFramework::builder(DefaultWebRequestContextResolver::default())
            .production_defaults()
            .build();
    });
    assert!(
        result.is_err(),
        "production build must reject dev-only resolver"
    );
}

#[test]
fn production_builder_accepts_tenant_bound_verifying_resolver() {
    use sdkwork_web_core::{
        tenant_bound_verifying_web_request_resolver, DefaultApiKeyLookupService,
        EnvBootstrapTenantSigningKeyLookup, WebFrameworkOptionalFeatures,
    };
    use sdkwork_web_test_utils::{
        production_test_audit_emitter, production_test_security_event_emitter,
    };

    let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let framework = WebFramework::builder(resolver)
        .production_defaults()
        .optional_features(
            WebFrameworkOptionalFeatures::production_sqlx().control_plane_standalone(),
        )
        .audit_emitter(production_test_audit_emitter())
        .security_event_emitter(production_test_security_event_emitter())
        .build();
    assert!(
        framework
            .layer()
            .runtime()
            .security_policy
            .rate_limit
            .enabled
    );
}

#[tokio::test]
async fn service_router_readyz_sanitizes_dependency_errors() {
    let app = service_router(
        Router::new(),
        ServiceRouterConfig::default().with_readiness_check(Arc::new(FailingReadiness)),
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
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        READINESS_DEPENDENCY_UNAVAILABLE,
        payload["detail"].as_str().unwrap()
    );
    assert!(!payload["detail"].as_str().unwrap().contains("sqlx"));
}

#[tokio::test]
async fn service_router_readyz_is_not_ready_without_probe() {
    let app = service_router(Router::new(), Default::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("readyz");
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
}

#[tokio::test]
async fn assemble_multi_surface_router_mounts_infra_once() {
    let surface_a = Router::new().route("/app/v3/api/example/a", get(|| async { "a" }));
    let surface_b = Router::new().route("/app/v3/api/example/b", get(|| async { "b" }));
    let app = assemble_multi_surface_router(
        [surface_a, surface_b],
        ServiceRouterConfig::default().with_always_ready(),
    );

    for path in ["/healthz", "/livez", "/readyz", "/metrics"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .expect(path);
        assert_eq!(
            StatusCode::OK,
            response.status(),
            "expected {path} to succeed"
        );
    }
}

#[tokio::test]
async fn service_router_exposes_health_and_metrics() {
    let app = service_router(Router::new(), Default::default());
    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("health");
    assert_eq!(StatusCode::OK, health.status());

    let metrics = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("metrics");
    assert_eq!(StatusCode::OK, metrics.status());
    let body = to_bytes(metrics.into_body(), usize::MAX)
        .await
        .expect("body");
    assert!(String::from_utf8_lossy(&body).contains("sdkwork_http_requests_total"));
}

#[tokio::test]
async fn contract_fallback_returns_501_for_manifest_only_route() {
    let config = ContractFallbackConfig::from_routes(SAMPLE_ROUTES);
    let request = Request::builder()
        .method(Method::GET)
        .uri("/app/v3/api/users")
        .body(Body::empty())
        .unwrap();
    let response = contract_fallback_handler(request, config).await;
    assert_eq!(StatusCode::NOT_IMPLEMENTED, response.status());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        "https://sdkwork.dev/problems/not-implemented",
        payload["type"].as_str().unwrap()
    );
    assert!(payload["traceId"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
}

#[tokio::test]
async fn service_router_contract_fallback_returns_501_for_unmounted_manifest_route() {
    let config = ServiceRouterConfig::default()
        .with_contract_fallback(ContractFallbackConfig::from_routes(SAMPLE_ROUTES));
    let app = service_router(Router::new(), config);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/app/v3/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(StatusCode::NOT_IMPLEMENTED, response.status());
}

#[tokio::test]
async fn service_router_contract_fallback_returns_404_for_unknown_route() {
    let config = ServiceRouterConfig::default()
        .with_contract_fallback(ContractFallbackConfig::from_routes(SAMPLE_ROUTES));
    let app = service_router(Router::new(), config);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/app/v3/api/unknown")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(StatusCode::NOT_FOUND, response.status());
}

#[test]
fn web_framework_builder_wires_contract_fallback_from_manifest() {
    let _env = IsolatedDeploymentEnv::enter();
    let framework = WebFramework::builder(DefaultWebRequestContextResolver::default())
        .route_manifest(HttpRouteManifest::new(SAMPLE_ROUTES))
        .build();
    assert!(
        framework
            .service_router_config()
            .contract_fallback
            .is_some(),
        "route_manifest must enable contract fallback on service router"
    );
}

#[tokio::test]
async fn mount_service_routes_contract_fallback_returns_501_for_unmounted_manifest_route() {
    let _env = IsolatedDeploymentEnv::enter();
    let framework = WebFramework::builder(DefaultWebRequestContextResolver::default())
        .route_manifest(HttpRouteManifest::new(SAMPLE_ROUTES))
        .build();
    let app = framework.mount_service_routes(Router::new());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/app/v3/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(StatusCode::NOT_IMPLEMENTED, response.status());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        "https://sdkwork.dev/problems/not-implemented",
        payload["type"].as_str().unwrap()
    );
}

#[tokio::test]
async fn web_framework_builder_shares_metrics_with_service_router() {
    let _env = IsolatedDeploymentEnv::enter();
    let framework = WebFramework::builder(DefaultWebRequestContextResolver::default()).build();
    let app = service_router(
        with_web_request_context(
            Router::new().route("/app/v3/api/ping", get(|| async { "pong" })),
            framework.layer().clone(),
        ),
        framework.service_router_config(),
    );

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/ping")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("ping");

    let metrics_response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("metrics");
    let body = to_bytes(metrics_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let rendered = String::from_utf8_lossy(&body);
    assert!(rendered.contains("sdkwork_http_requests_total 1"));
    assert!(rendered.contains("api_surface=\"app-api\""));
    assert!(rendered.contains("backend_layer=\"handler\""));
}

#[tokio::test]
async fn metrics_increment_when_layer_shares_registry() {
    use axum::routing::get;
    use sdkwork_web_axum::{with_web_request_context, WebFrameworkLayer};
    use sdkwork_web_core::{DefaultWebRequestContextResolver, HttpMetricsRegistry};

    let metrics = HttpMetricsRegistry::new();
    let layer = WebFrameworkLayer::new(DefaultWebRequestContextResolver::default())
        .with_metrics(metrics.clone());
    let app = service_router(
        with_web_request_context(
            Router::new().route("/app/v3/api/ping", get(|| async { "ok" })),
            layer,
        ),
        ServiceRouterConfig::default().with_metrics(metrics.clone()),
    );

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/ping")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("ping");

    let metrics_response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("metrics");
    let body = to_bytes(metrics_response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert!(String::from_utf8_lossy(&body).contains("sdkwork_http_requests_total 1"));
}

#[test]
fn openapi_document_includes_request_context_extension() {
    let doc = build_openapi_document("Test API", SAMPLE_ROUTES);
    let paths = doc["paths"].as_object().expect("paths");
    let users = paths["/app/v3/api/users"].as_object().expect("path");
    let get = users["get"].as_object().expect("get op");
    assert_eq!(
        "WebRequestContext",
        get["x-sdkwork-request-context"].as_str().unwrap()
    );
    assert_eq!("app-api", get["x-sdkwork-api-surface"].as_str().unwrap());
    assert_eq!("dual-token", get["x-sdkwork-route-auth"].as_str().unwrap());
    assert_eq!("dual-token", get["x-sdkwork-auth-mode"].as_str().unwrap());
}

#[test]
fn builder_rejects_public_prefix_covering_protected_manifest_route() {
    let _env = IsolatedDeploymentEnv::enter();
    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Get,
        "/app/v3/api/users/me",
        "Users",
        "users.me",
        RouteAuth::DualToken,
    )];
    let profile = WebRequestContextProfile {
        public_path_prefixes: vec!["/app/v3/api/users".to_owned()],
        ..Default::default()
    };
    let result = std::panic::catch_unwind(|| {
        WebFramework::builder(DefaultWebRequestContextResolver::default())
            .profile(profile)
            .route_manifest(HttpRouteManifest::new(ROUTES))
            .build();
    });
    assert!(result.is_err());
}

#[test]
fn builder_rejects_non_open_api_route_without_dual_token_auth() {
    let _env = IsolatedDeploymentEnv::enter();
    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Get,
        "/app/v3/api/users",
        "Users",
        "users.list",
        RouteAuth::ApiKey,
    )];
    let result = std::panic::catch_unwind(|| {
        WebFramework::builder(DefaultWebRequestContextResolver::default())
            .route_manifest(HttpRouteManifest::new(ROUTES))
            .build();
    });
    assert!(result.is_err());
}

#[tokio::test]
async fn manifest_public_route_reaches_handler_with_access_token_jwt() {
    let _env = IsolatedDeploymentEnv::enter();
    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_public(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions",
        "Auth",
        "sessions.create",
    )];
    let layer = WebFramework::builder(DefaultWebRequestContextResolver::default())
        .route_manifest(HttpRouteManifest::new(ROUTES))
        .build()
        .into_layer();
    let app = with_web_request_context(
        Router::new().route(
            "/app/v3/api/auth/sessions",
            axum::routing::post(|ctx: sdkwork_web_core::WebRequestContext| async move {
                assert_eq!(sdkwork_web_core::WebAuthMode::Public, ctx.auth_mode);
                assert_eq!(
                    "100001",
                    ctx.principal
                        .as_ref()
                        .expect("tenant isolation principal")
                        .tenant_id()
                );
                axum::Json(serde_json::json!({"ok": true}))
            }),
        ),
        layer,
    );
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/app/v3/api/auth/sessions")
                .header(
                    "Access-Token",
                    bootstrap_access_token_jwt("100001", "app_tenant-bootstrap"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(StatusCode::OK, response.status());
}
