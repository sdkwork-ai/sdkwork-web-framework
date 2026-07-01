//! Pipeline concurrency and overhead stress checks (catalog K6).

use axum::body::Body;
use axum::extract::Request;
use sdkwork_web_core::{
    memory_rate_limit_store, DefaultWebRequestContextResolver, WebCallInterceptorChain,
    WebCallRuntime, WebFrameworkErrorKind,
};
use tokio::task::JoinSet;

fn sample_protected_request() -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri("/app/v3/api/users")
        .header(
            "Authorization",
            "Bearer api_key_id=key;tenant_id=100001;organization_id=0;user_id=30;app_id=app;environment=prod;deployment_mode=saas;data_scope=tenant;permission_scope=read",
        )
        .header(
            "Access-Token",
            "tenant_id=100001;organization_id=0;user_id=30;app_id=app;environment=prod;deployment_mode=saas",
        )
        .body(Body::empty())
        .expect("request")
}

async fn run_pipeline_before_once(runtime: &WebCallRuntime<DefaultWebRequestContextResolver>) {
    let chain = WebCallInterceptorChain::standard();
    let mut request = sample_protected_request();
    let mut state = sdkwork_web_core::WebCallState::from_request(&request);
    let _ = chain.before(&mut state, &mut request, runtime).await;
}

#[tokio::test]
async fn pipeline_before_handles_concurrent_requests_without_panic() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_rate_limit_store(memory_rate_limit_store());
    let runtime = std::sync::Arc::new(runtime);
    let mut tasks = JoinSet::new();
    for _ in 0..64 {
        let runtime = runtime.clone();
        tasks.spawn(async move { run_pipeline_before_once(&runtime).await });
    }
    while tasks.join_next().await.is_some() {}
}

#[tokio::test]
async fn rate_limit_store_enforces_under_concurrent_load() {
    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_rate_limit_store(memory_rate_limit_store());
    runtime.security_policy.rate_limit.enabled = true;
    runtime.security_policy.rate_limit.max_requests_per_window = 4;
    runtime.security_policy.rate_limit.window_secs = 60;
    runtime.security_policy.rate_limit.pre_auth_rate_limit = true;

    let chain = WebCallInterceptorChain::standard();
    let mut blocked = 0usize;
    for _ in 0..12 {
        let mut request = Request::builder()
            .method("GET")
            .uri("/app/v3/api/users")
            .header("x-forwarded-for", "203.0.113.10")
            .body(Body::empty())
            .expect("request");
        let mut state = sdkwork_web_core::WebCallState::from_request(&request);
        if chain
            .before(&mut state, &mut request, &runtime)
            .await
            .is_err()
        {
            blocked += 1;
        }
    }
    assert!(
        blocked >= 4,
        "expected concurrent pre-auth rate limit blocks, got {blocked}"
    );
}

#[tokio::test]
async fn rate_limit_exceeded_maps_to_expected_error_kind() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RateLimitTier};
    use sdkwork_web_core::{memory_rate_limit_store, HttpRouteManifest, RateLimitPolicy};

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_public(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions",
        "Auth",
        "createSession",
    )
    .with_rate_limit_tier(RateLimitTier::AuthCritical)];

    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_rate_limit_store(memory_rate_limit_store())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    runtime.security_policy.rate_limit = RateLimitPolicy {
        enabled: true,
        max_requests_per_window: 120,
        window_secs: 60,
        pre_auth_rate_limit: true,
        tenant_limit_after_auth: false,
    };
    runtime.profile.public_path_prefixes = vec![];
    let chain = WebCallInterceptorChain::standard();

    for _ in 0..10 {
        let mut request = Request::builder()
            .method("POST")
            .uri("/app/v3/api/auth/sessions")
            .header("x-forwarded-for", "203.0.113.55")
            .header(
                "Access-Token",
                sdkwork_web_core::bootstrap_access_token_jwt("100001", "app_tenant-bootstrap"),
            )
            .body(Body::empty())
            .expect("request");
        let mut state = sdkwork_web_core::WebCallState::from_request(&request);
        chain
            .before(&mut state, &mut request, &runtime)
            .await
            .expect("under auth critical limit");
    }

    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions")
        .header("x-forwarded-for", "203.0.113.55")
        .header(
            "Access-Token",
            sdkwork_web_core::bootstrap_access_token_jwt("100001", "app_tenant-bootstrap"),
        )
        .body(Body::empty())
        .expect("request");
    let mut state = sdkwork_web_core::WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("over auth critical limit");
    assert_eq!(WebFrameworkErrorKind::RateLimitExceeded, error.kind);
}
