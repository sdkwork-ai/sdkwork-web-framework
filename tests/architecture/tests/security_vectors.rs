//! Security vector tests (catalog K4).

use axum::body::Body;
use axum::extract::Request;
use sdkwork_web_core::{
    AllowAllAuthorizationPolicy, DefaultOpenApiWebRequestContextResolver,
    DefaultWebRequestContextResolver, HttpRouteManifest, SecurityPolicy, WebAuthMode,
    WebCallInterceptorChain, WebCallRuntime, WebCallState, WebFrameworkErrorKind,
};
use std::sync::Arc;
#[tokio::test]
async fn production_header_security_applies_response_headers() {
    use axum::http::Response;
    use sdkwork_web_core::SecurityPolicy;

    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.security_policy = SecurityPolicy::production();
    let chain = WebCallInterceptorChain::standard();
    let request = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .expect("request");
    let state = WebCallState::from_request(&request);
    let mut response = Response::new(Body::empty());
    chain
        .after(&state, &mut response, &runtime)
        .await
        .expect("after pipeline");
    assert_eq!(
        Some("nosniff"),
        response
            .headers()
            .get("x-content-type-options")
            .and_then(|value| value.to_str().ok())
    );
    assert_eq!(
        Some("DENY"),
        response
            .headers()
            .get("x-frame-options")
            .and_then(|value| value.to_str().ok())
    );
    assert!(response
        .headers()
        .get("strict-transport-security")
        .is_some());
}

#[tokio::test]
async fn cors_rejects_unknown_origin_on_state_changing_request() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/users")
        .header("origin", "https://evil.example")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("cors");
    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
}

#[tokio::test]
async fn cors_preflight_options_skips_authentication() {
    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.security_policy.cors.allowed_origins = vec!["https://app.example".to_owned()];
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("OPTIONS")
        .uri("/app/v3/api/users")
        .header("origin", "https://app.example")
        .header("access-control-request-method", "POST")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("preflight should not require auth tokens");
    assert!(state.principal.is_none());
}

#[tokio::test]
async fn sql_injection_guard_blocks_api_key_header_payload() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/open/v3/api/messages")
        .header("x-api-key", "' OR 1=1 --")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("sql guard");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
}

#[tokio::test]
async fn oversized_content_length_returns_413() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/health")
        .header("content-length", "999999999")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("size");
    assert_eq!(WebFrameworkErrorKind::PayloadTooLarge, error.kind);
}

#[tokio::test]
async fn cors_denial_emits_security_event() {
    use async_trait::async_trait;
    use sdkwork_web_core::{
        SecurityEvent, SecurityEventEmitter, SecurityEventKind, WebFrameworkError,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct RecordingSecurityEvents(Arc<Mutex<Vec<SecurityEvent>>>);

    #[async_trait]
    impl SecurityEventEmitter for RecordingSecurityEvents {
        async fn emit(&self, event: SecurityEvent) -> Result<(), WebFrameworkError> {
            self.0.lock().expect("mutex").push(event);
            Ok(())
        }
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_security_event_emitter(Arc::new(RecordingSecurityEvents(events.clone())));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/users")
        .header("origin", "https://evil.example")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let _ = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("cors");
    let recorded = events.lock().expect("mutex");
    assert_eq!(1, recorded.len());
    assert_eq!(SecurityEventKind::CorsDenied, recorded[0].kind);
}

#[tokio::test]
async fn auth_critical_tier_limits_requests() {
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
            .header("Access-Token", bootstrap_access_header())
            .body(Body::empty())
            .expect("request");
        let mut state = WebCallState::from_request(&request);
        chain
            .before(&mut state, &mut request, &runtime)
            .await
            .expect("under auth critical limit");
    }

    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions")
        .header("Access-Token", bootstrap_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("over auth critical limit");
    assert_eq!(WebFrameworkErrorKind::RateLimitExceeded, error.kind);
}

#[tokio::test]
async fn csrf_cookie_without_origin_is_rejected() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/users")
        .header("cookie", "session=abc")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("csrf");
    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
}

#[tokio::test]
async fn csrf_cookie_with_untrusted_referer_is_rejected() {
    let mut security = SecurityPolicy::default();
    security
        .cors
        .allowed_origins
        .push("https://trusted.example".to_owned());
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_security_policy(security);
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/users")
        .header("cookie", "session=abc")
        .header("referer", "https://attacker.example/evil")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("csrf referer");
    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
}

#[tokio::test]
async fn backend_api_rejects_personal_login_scope_session() {
    use sdkwork_web_core::EnforcePrincipalTenantIsolationPolicy;

    let (auth, access) = dual_token_fixture_headers();
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_tenant_isolation_policy(Arc::new(EnforcePrincipalTenantIsolationPolicy));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/backend/v3/api/iam/users")
        .header("Authorization", auth)
        .header("Access-Token", access)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("personal session on backend");
    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
    assert!(error.message.contains("personal sessions"));
}

#[tokio::test]
async fn deny_all_authorization_policy_blocks_protected_routes() {
    use sdkwork_web_core::DenyAllAuthorizationPolicy;

    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_authorization_policy(Arc::new(DenyAllAuthorizationPolicy));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                sdkwork_web_core::auth_token_jwt("100001", "30", "s-1", "appbase")
            ),
        )
        .header(
            "Access-Token",
            sdkwork_web_core::access_token_jwt("100001", "30", "s-1", "appbase"),
        )
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("deny all");
    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
}

#[tokio::test]
async fn disallowed_method_returns_405() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("TRACE")
        .uri("/health")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("method");
    assert_eq!(WebFrameworkErrorKind::MethodNotAllowed, error.kind);
}

#[tokio::test]
async fn production_json_content_type_rejects_non_json_post() {
    let runtime = WebCallRuntime::production(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/health")
        .header("content-length", "12")
        .header("content-type", "text/plain")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("json content type");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
}

#[tokio::test]
async fn json_content_type_guard_is_optional() {
    let mut runtime = WebCallRuntime::production(DefaultWebRequestContextResolver::default());
    runtime.optional_features.json_content_type_guard = false;
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/healthz")
        .header("content-length", "12")
        .header("content-type", "text/plain")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("json guard disabled");
}

#[tokio::test]
async fn dynamic_cors_overlay_allows_tenant_specific_origin() {
    use sdkwork_web_core::{
        CorsPolicy, CorsPolicyContext, DynamicCorsPolicySource, WebEnvironment,
        WebRequestContextProfile,
    };

    struct StaticCorsOverlay;

    #[async_trait::async_trait]
    impl DynamicCorsPolicySource for StaticCorsOverlay {
        async fn resolve(
            &self,
            _ctx: &CorsPolicyContext,
        ) -> Result<Option<CorsPolicy>, sdkwork_web_core::WebFrameworkError> {
            Ok(Some(CorsPolicy {
                allowed_origins: vec!["https://tenant.example".to_owned()],
                ..CorsPolicy::default()
            }))
        }
    }

    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.profile = WebRequestContextProfile {
        environment: WebEnvironment::Prod,
        ..WebRequestContextProfile::default()
    };
    runtime.optional_features.dynamic_cors_policy = true;
    runtime = runtime.with_dynamic_cors_policy_source(Arc::new(StaticCorsOverlay));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/healthz")
        .header("origin", "https://tenant.example")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("dynamic cors allows tenant origin");
    assert!(state.resolved_cors.is_some());
}

#[tokio::test]
async fn tenant_runtime_profile_tightens_body_limit() {
    use sdkwork_web_core::{
        DynamicTenantRuntimeProfileSource, TenantRuntimeProfile, TenantRuntimeProfileContext,
        WebEnvironment, WebRequestContextProfile,
    };

    struct StaticTenantProfile;

    #[async_trait::async_trait]
    impl DynamicTenantRuntimeProfileSource for StaticTenantProfile {
        async fn resolve(
            &self,
            _ctx: &TenantRuntimeProfileContext,
        ) -> Result<Option<TenantRuntimeProfile>, sdkwork_web_core::WebFrameworkError> {
            Ok(Some(TenantRuntimeProfile {
                max_content_length: Some(1024),
                ..TenantRuntimeProfile::default()
            }))
        }
    }

    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.profile = WebRequestContextProfile {
        environment: WebEnvironment::Prod,
        ..WebRequestContextProfile::default()
    };
    runtime.optional_features.dynamic_tenant_runtime_profile = true;
    runtime = runtime.with_dynamic_tenant_runtime_profile_source(Arc::new(StaticTenantProfile));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/health")
        .header("content-length", "2048")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("tenant body limit");
    assert_eq!(WebFrameworkErrorKind::PayloadTooLarge, error.kind);
}

#[tokio::test]
async fn tenant_concurrent_limit_blocks_extra_inflight_requests() {
    use sdkwork_web_core::{
        DefaultWebRequestContextResolver, DynamicTenantRuntimeProfileSource, TenantRuntimeProfile,
        TenantRuntimeProfileContext, WebEnvironment, WebRequestContextProfile,
    };

    struct ConcurrentTenantProfile;

    #[async_trait::async_trait]
    impl DynamicTenantRuntimeProfileSource for ConcurrentTenantProfile {
        async fn resolve(
            &self,
            _ctx: &TenantRuntimeProfileContext,
        ) -> Result<Option<TenantRuntimeProfile>, sdkwork_web_core::WebFrameworkError> {
            Ok(Some(TenantRuntimeProfile {
                max_concurrent_requests: Some(1),
                ..TenantRuntimeProfile::default()
            }))
        }
    }

    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.profile = WebRequestContextProfile {
        environment: WebEnvironment::Prod,
        ..WebRequestContextProfile::default()
    };
    runtime.optional_features.dynamic_tenant_runtime_profile = true;
    runtime = runtime.with_dynamic_tenant_runtime_profile_source(Arc::new(ConcurrentTenantProfile));
    let chain = WebCallInterceptorChain::standard();

    let mut first_request = Request::builder()
        .uri("/app/v3/api/orders")
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                sdkwork_web_core::auth_token_jwt("100001", "30", "s-1", "appbase")
            ),
        )
        .header(
            "Access-Token",
            sdkwork_web_core::access_token_jwt("100001", "30", "s-1", "appbase"),
        )
        .body(Body::empty())
        .expect("request");
    let mut first_state = WebCallState::from_request(&first_request);
    chain
        .before(&mut first_state, &mut first_request, &runtime)
        .await
        .expect("first acquire");

    let mut second_request = Request::builder()
        .uri("/app/v3/api/orders")
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                sdkwork_web_core::auth_token_jwt("100001", "30", "s-1", "appbase")
            ),
        )
        .header(
            "Access-Token",
            sdkwork_web_core::access_token_jwt("100001", "30", "s-1", "appbase"),
        )
        .body(Body::empty())
        .expect("request");
    let mut second_state = WebCallState::from_request(&second_request);
    let error = chain
        .before(&mut second_state, &mut second_request, &runtime)
        .await
        .expect_err("second concurrent");
    assert_eq!(WebFrameworkErrorKind::RateLimitExceeded, error.kind);

    runtime
        .concurrent_admission_store
        .release(first_state.concurrent_admission_key.as_ref().expect("key"))
        .await
        .expect("release");
}

#[tokio::test]
async fn open_api_api_key_resolves_authenticated_principal() {
    let mut runtime = WebCallRuntime::new(DefaultOpenApiWebRequestContextResolver::default());
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/open/v3/api/messages")
        .header(
            "x-api-key",
            "api_key_id=key-1;tenant_id=100001;user_id=30;app_id=appbase",
        )
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("open-api api key auth");
    assert_eq!(WebAuthMode::ApiKey, state.auth_mode);
    assert_eq!(
        "100001",
        state.principal.as_ref().expect("principal").tenant_id()
    );
}

#[tokio::test]
async fn open_api_oauth_bearer_resolves_authenticated_principal() {
    let mut runtime = WebCallRuntime::new(DefaultOpenApiWebRequestContextResolver::default());
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/open/v3/api/messages")
        .header(
            "Authorization",
            "Bearer token_id=tok-1;tenant_id=100001;user_id=user-oauth;app_id=appbase",
        )
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("open-api oauth auth");
    assert_eq!(WebAuthMode::OAuth, state.auth_mode);
    assert_eq!(
        "100001",
        state.principal.as_ref().expect("principal").tenant_id()
    );
}

#[tokio::test]
async fn open_api_without_credentials_is_rejected() {
    let runtime = WebCallRuntime::new(DefaultOpenApiWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/open/v3/api/messages")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("missing credentials");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
}

#[tokio::test]
async fn public_credential_entry_route_rejects_missing_access_token() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_public(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions",
        "Auth",
        "sessions.create",
    )];
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("credential-entry route without bootstrap access token");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
}

#[tokio::test]
async fn public_app_api_rejects_semicolon_claim_string_access_token() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions/refresh",
        "Auth",
        "sessions.refresh",
        RouteAuth::RefreshToken,
    )];
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions/refresh")
        .header(
            "Access-Token",
            "tenant_id=100001;app_id=appbase;environment=prod;deployment_mode=saas",
        )
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("claim string access token");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
}

#[tokio::test]
async fn app_api_rejects_tenant_id_query_selector() {
    let (auth, access) = dual_token_fixture_headers();
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/users?tenant_id=100001")
        .header("Authorization", auth)
        .header("Access-Token", access)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("tenant_id query selector");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert!(error.message.contains("tenant_id"));
}

#[tokio::test]
async fn app_api_rejects_tenant_id_body_selector() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let body = r#"{"tenantId":"100001","displayName":"Acme"}"#;
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/users")
        .header("content-type", "application/json")
        .header("content-length", body.len().to_string())
        .body(Body::from(body))
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("tenantId body selector");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert!(error.message.contains("tenantId"));
}

#[tokio::test]
async fn app_api_rejects_ambient_tenant_path_segment() {
    let (auth, access) = dual_token_fixture_headers();
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/tenants/t1/orders")
        .header("Authorization", auth)
        .header("Access-Token", access)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("ambient tenant path");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert!(error.message.contains("/tenants/"));
}

#[tokio::test]
async fn protected_route_rejects_mismatched_tenant_path_resource_id() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};
    use sdkwork_web_core::EnforcePrincipalTenantIsolationPolicy;

    const ROUTES: &[HttpRoute] = &[HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/web-framework/tenants/{tenantId}/runtime_defaults",
        "web-framework",
        "runtimeDefaults.byTenant",
    )
    .with_required_permission("web-framework.runtime-defaults.read")];

    let (auth, access) = dual_token_fixture_headers();
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES))
        .with_tenant_isolation_policy(Arc::new(EnforcePrincipalTenantIsolationPolicy));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/backend/v3/api/web-framework/tenants/100002/runtime_defaults")
        .header("Authorization", auth)
        .header("Access-Token", access)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("path tenant mismatch");
    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
    assert!(error.message.contains("100002"));
}

fn dual_token_fixture_headers() -> (String, String) {
    (
        format!(
            "Bearer {}",
            sdkwork_web_core::auth_token_jwt("100001", "30", "s-1", "appbase")
        ),
        sdkwork_web_core::access_token_jwt("100001", "30", "s-1", "appbase"),
    )
}

fn bootstrap_access_header() -> String {
    sdkwork_web_core::bootstrap_access_token_jwt("100001", "app_tenant-bootstrap")
}

#[tokio::test]
async fn app_api_without_access_token_is_rejected() {
    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let (auth_value, _access_value) = dual_token_fixture_headers();
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", auth_value)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("missing access token");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
    assert!(
        error.message.contains("Access-Token"),
        "expected Access-Token requirement, got: {}",
        error.message
    );
}

#[tokio::test]
async fn app_api_without_auth_token_is_rejected() {
    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let access_value = dual_token_fixture_headers().1;
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Access-Token", access_value)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("missing auth token");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
    assert!(
        error.message.contains("Authorization"),
        "expected Authorization requirement, got: {}",
        error.message
    );
}

#[tokio::test]
async fn backend_api_without_access_token_is_rejected() {
    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_value = dual_token_fixture_headers().0;
    let mut request = Request::builder()
        .uri("/backend/v3/api/iam/users")
        .header("Authorization", auth_value)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("missing access token");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
    assert!(error.message.contains("Access-Token"));
}

#[tokio::test]
async fn gateway_api_without_access_token_is_rejected() {
    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_value = dual_token_fixture_headers().0;
    let mut request = Request::builder()
        .uri("/v1/proxy/resources")
        .header("Authorization", auth_value)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("missing access token");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
    assert!(error.message.contains("Access-Token"));
}

#[tokio::test]
async fn non_open_api_with_dual_tokens_resolves_principal() {
    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let (auth_value, access_value) = dual_token_fixture_headers();
    let mut request = Request::builder()
        .uri("/backend/v3/api/iam/users")
        .header("Authorization", auth_value)
        .header("Access-Token", access_value)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("dual-token backend-api request");
    assert_eq!(WebAuthMode::DualToken, state.auth_mode);
    assert!(state.credentials.access_token.is_some());
    assert_eq!(
        "100001",
        state.principal.as_ref().expect("principal").tenant_id()
    );
}

#[tokio::test]
async fn credential_entry_route_rejects_authorization_header() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_public(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions",
        "Auth",
        "sessions.create",
    )];
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions")
        .header("Authorization", dual_token_fixture_headers().0)
        .header("Access-Token", bootstrap_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("credential entry with auth token");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
}

#[tokio::test]
async fn credential_entry_route_accepts_bootstrap_access_token_jwt() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_public(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions",
        "Auth",
        "sessions.create",
    )];
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions")
        .header("Access-Token", bootstrap_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("bootstrap access token jwt accepted");
    assert_eq!(
        "100001",
        state.principal.as_ref().expect("principal").tenant_id()
    );
}

#[tokio::test]
async fn csrf_cookie_without_origin_is_rejected_on_public_auth_route() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_public(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions",
        "Auth",
        "sessions.create",
    )];
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions")
        .header("cookie", "session=abc")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("csrf on public auth route");
    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
}

#[tokio::test]
async fn csrf_cookie_with_untrusted_origin_is_rejected_on_public_auth_route() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_public(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions",
        "Auth",
        "sessions.create",
    )];
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions")
        .header("cookie", "session=abc")
        .header("origin", "https://evil.example")
        .header("Access-Token", bootstrap_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("untrusted origin with cookie on public auth route");
    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
}

#[tokio::test]
async fn tenant_bound_verifier_rejects_mismatched_tenant_claim_in_pipeline() {
    use sdkwork_web_core::{
        encode_hs256_test_jwt_with_kid, tenant_bound_verifying_web_request_resolver,
        DefaultApiKeyLookupService, StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-1".to_owned(),
        TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
    )]));
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100002",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
        }),
    );
    let access_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "access",
            "tenant_id": "100002",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("tenant-bound jwt tenant mismatch");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    assert!(error.message.contains("100002"));
}

#[tokio::test]
async fn tenant_bound_verifier_accepts_valid_dual_token_in_pipeline() {
    use sdkwork_web_core::{
        encode_hs256_test_jwt_with_kid, tenant_bound_verifying_web_request_resolver,
        DefaultApiKeyLookupService, StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-1".to_owned(),
        TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
    )]));
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
        }),
    );
    let access_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "access",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("valid tenant-bound dual token");
    let principal = state.principal.as_ref().expect("principal");
    assert_eq!("100001", principal.tenancy.tenant_id);
}

#[tokio::test]
async fn tenant_bound_verifier_rejects_jwt_without_kid_in_pipeline() {
    use sdkwork_web_core::{
        encode_hs256_test_jwt_with_kid, encode_hs256_test_jwt_without_kid,
        tenant_bound_verifying_web_request_resolver, DefaultApiKeyLookupService,
        StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-1".to_owned(),
        TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
    )]));
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
        }),
    );
    let access_token = encode_hs256_test_jwt_without_kid(
        "secret-1",
        json!({
            "token_type": "access",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("missing kid");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    assert!(error.message.contains("kid"));
}

#[tokio::test]
async fn tenant_bound_verifier_rejects_unknown_kid_in_pipeline() {
    use sdkwork_web_core::{
        encode_hs256_test_jwt_with_kid, tenant_bound_verifying_web_request_resolver,
        DefaultApiKeyLookupService, StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-1".to_owned(),
        TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
    )]));
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
        }),
    );
    let access_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-revoked",
        json!({
            "token_type": "access",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("unknown kid");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
}

#[tokio::test]
async fn tenant_bound_verifier_rejects_expired_token_in_pipeline() {
    use sdkwork_web_core::{
        encode_hs256_test_jwt_with_kid, tenant_bound_verifying_web_request_resolver,
        DefaultApiKeyLookupService, StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-1".to_owned(),
        TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
    )]));
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
            "exp": 1,
            "iat": 1,
        }),
    );
    let access_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "access",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
            "exp": 1,
            "iat": 1,
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("expired jwt");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    assert!(error.message.contains("exp"));
}

#[tokio::test]
async fn tenant_bound_verifier_rejects_wrong_token_type_in_pipeline() {
    use sdkwork_web_core::{
        encode_hs256_test_jwt_with_kid, tenant_bound_verifying_web_request_resolver,
        DefaultApiKeyLookupService, StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-1".to_owned(),
        TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
    )]));
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
        }),
    );
    let access_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("wrong access token type");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    assert!(error.message.contains("token_type"));
}

#[tokio::test]
async fn tenant_bound_verifier_rejects_wrong_issuer_in_pipeline() {
    use sdkwork_web_core::{
        encode_hs256_test_jwt_with_kid,
        tenant_bound_verifying_web_request_resolver_with_claim_policy, DefaultApiKeyLookupService,
        JwtProductionClaimPolicy, StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-1".to_owned(),
        TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
    )]));
    let resolver = tenant_bound_verifying_web_request_resolver_with_claim_policy(
        lookup,
        DefaultApiKeyLookupService,
        JwtProductionClaimPolicy::saas_production(
            vec!["https://iam.example".to_owned()],
            vec!["appbase".to_owned()],
        ),
    );
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
            "iss": "https://evil.example",
            "aud": "appbase",
        }),
    );
    let access_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "access",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
            "iss": "https://evil.example",
            "aud": "appbase",
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("wrong iss");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    assert!(error.message.contains("iss"));
}

#[tokio::test]
async fn tenant_bound_verifier_rejects_wrong_audience_in_pipeline() {
    use sdkwork_web_core::{
        encode_hs256_test_jwt_with_kid,
        tenant_bound_verifying_web_request_resolver_with_claim_policy, DefaultApiKeyLookupService,
        JwtProductionClaimPolicy, StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-1".to_owned(),
        TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
    )]));
    let resolver = tenant_bound_verifying_web_request_resolver_with_claim_policy(
        lookup,
        DefaultApiKeyLookupService,
        JwtProductionClaimPolicy::saas_production(
            vec!["https://iam.example".to_owned()],
            vec!["appbase".to_owned()],
        ),
    );
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
            "iss": "https://iam.example",
            "aud": "wrong-app",
        }),
    );
    let access_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "access",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
            "iss": "https://iam.example",
            "aud": "wrong-app",
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("wrong aud");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    assert!(error.message.contains("aud"));
}

#[tokio::test]
async fn tenant_bound_saas_verifier_rejects_revoked_session_in_pipeline() {
    use sdkwork_web_core::{
        encode_hs256_test_jwt_with_kid, tenant_bound_saas_verifying_web_request_resolver,
        DefaultApiKeyLookupService, StaticJwtSessionRevocationChecker,
        StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-1".to_owned(),
        TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
    )]));
    let resolver = tenant_bound_saas_verifying_web_request_resolver(
        lookup,
        StaticJwtSessionRevocationChecker::with_revoked(["session-revoked"]),
        DefaultApiKeyLookupService,
    );
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "session-revoked",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
        }),
    );
    let access_token = encode_hs256_test_jwt_with_kid(
        "secret-1",
        "kid-1",
        json!({
            "token_type": "access",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "session-revoked",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("revoked session");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    assert!(error.message.contains("revoked"));
}

#[tokio::test]
async fn tenant_bound_verifier_accepts_rs256_dual_token_in_pipeline() {
    use sdkwork_web_core::{
        encode_rs256_test_jwt_with_kid, generate_rs256_test_keypair,
        tenant_bound_verifying_web_request_resolver, DefaultApiKeyLookupService,
        StaticTenantSigningKeyLookup, TenantSigningKeyMaterial,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    let (private_key, spki_der) = generate_rs256_test_keypair();
    let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
        "kid-rs256".to_owned(),
        TenantSigningKeyMaterial::rs256_spki("100001", "kid-rs256", spki_der),
    )]));
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DefaultApiKeyLookupService);
    let mut runtime = WebCallRuntime::new(resolver);
    runtime.authorization = Arc::new(AllowAllAuthorizationPolicy);
    let chain = WebCallInterceptorChain::standard();
    let auth_token = encode_rs256_test_jwt_with_kid(
        &private_key,
        "kid-rs256",
        json!({
            "token_type": "auth",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "auth_level": "password",
            "login_scope": "TENANT",
        }),
    );
    let access_token = encode_rs256_test_jwt_with_kid(
        &private_key,
        "kid-rs256",
        json!({
            "token_type": "access",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "s-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
        }),
    );
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", format!("Bearer {auth_token}"))
        .header("Access-Token", access_token)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("rs256 dual token");
    assert!(state.principal.is_some());
}
