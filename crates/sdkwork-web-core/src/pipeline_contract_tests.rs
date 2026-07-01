use crate::{
    access_token_jwt, auth_token_jwt, bootstrap_access_token_jwt, memory_idempotency_store,
    AuditEmitter, AuditFact, AuthorizationPolicy, DefaultWebRequestContextResolver,
    DomainContextInjector, HttpRouteManifest, WebAuthMode, WebCallInterceptorChain, WebCallRuntime,
    WebCallState, WebFrameworkError, WebFrameworkErrorKind, WebRequestContext,
    WebRequestContextProfile,
};
use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn fixture_auth_header() -> String {
    format!(
        "Bearer {}",
        auth_token_jwt("100001", "30", "s-1", "appbase")
    )
}

fn fixture_access_header() -> String {
    access_token_jwt("100001", "30", "s-1", "appbase")
}

fn fixture_bootstrap_access_header() -> String {
    bootstrap_access_token_jwt("100001", "app_tenant-bootstrap")
}

fn security_with_idempotency(
    idempotency: crate::security::IdempotencyPolicy,
) -> crate::security::SecurityPolicy {
    crate::security::SecurityPolicy {
        idempotency,
        ..crate::security::SecurityPolicy::default()
    }
}

struct CountingAuthorizationPolicy {
    calls: Arc<AtomicUsize>,
}

impl AuthorizationPolicy for CountingAuthorizationPolicy {
    fn authorize(
        &self,
        _ctx: &WebRequestContext,
        _operation_id: Option<&str>,
    ) -> Result<(), WebFrameworkError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Clone)]
struct InjectorMarker;

struct MarkerDomainInjector;

impl DomainContextInjector for MarkerDomainInjector {
    fn inject(&self, request: &mut Request, _context: &WebRequestContext) {
        request.extensions_mut().insert(InjectorMarker);
    }
}

#[tokio::test]
async fn manifest_idempotent_requires_key_without_global_policy() {
    use crate::security::IdempotencyPolicy;
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Post,
        "/app/v3/api/orders",
        "Orders",
        "createOrder",
        RouteAuth::DualToken,
    )
    .with_idempotent(true)];

    let security = security_with_idempotency(IdempotencyPolicy {
        require_for_retryable_commands: false,
        retention_secs: 60,
        max_cached_response_bytes: 1024,
        require_body_hash_for_payload: true,
    });
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES))
        .with_security_policy(security);
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/orders")
        .header("content-length", "0")
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("missing idempotency key");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert_eq!(Some("createOrder"), state.operation_id.as_deref());
}

#[tokio::test]
async fn authorization_policy_is_invoked_for_protected_routes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_authorization_policy(Arc::new(CountingAuthorizationPolicy {
            calls: calls.clone(),
        }));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("pipeline");
    assert_eq!(1, calls.load(Ordering::SeqCst));
}

#[tokio::test]
async fn idempotency_replays_cached_response_without_duplicate_handler() {
    use crate::idempotency::{
        idempotency_fingerprint, IdempotencyBeginOutcome, IdempotencyResponseRecord,
    };
    use crate::security::IdempotencyPolicy;

    let store = memory_idempotency_store();
    let ttl = std::time::Duration::from_secs(60);
    let fingerprint = idempotency_fingerprint("POST", "/app/v3/api/orders", None);
    let seed_state = WebCallState::from_request(
        &Request::builder()
            .method("POST")
            .uri("/app/v3/api/orders")
            .header("Authorization", fixture_auth_header())
            .header("Access-Token", fixture_access_header())
            .body(Body::empty())
            .expect("request"),
    );
    let store_key = seed_state.scoped_idempotency_store_key("order-1");
    store
        .begin(&store_key, &fingerprint, ttl)
        .await
        .expect("leader");
    store
        .complete(
            &store_key,
            &fingerprint,
            IdempotencyResponseRecord {
                status_code: 201,
                body: br#"{"id":"1"}"#.to_vec(),
                content_type: Some("application/json".to_owned()),
            },
            ttl,
        )
        .await
        .expect("complete");
    let replay = store
        .begin(&store_key, &fingerprint, ttl)
        .await
        .expect("replay");
    assert!(matches!(replay, IdempotencyBeginOutcome::Replay(_)));

    let security = security_with_idempotency(IdempotencyPolicy {
        require_for_retryable_commands: true,
        retention_secs: 60,
        max_cached_response_bytes: 1024,
        require_body_hash_for_payload: true,
    });
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_idempotency_store(store)
        .with_security_policy(security);
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/orders")
        .header("Idempotency-Key", "order-1")
        .header("content-length", "0")
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("pipeline");
    assert!(state.idempotency_replay.is_some());
}

#[tokio::test]
async fn rejects_client_identity_projection_headers_on_protected_paths() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("x-sdkwork-tenant-id", "evil-tenant")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("forbidden header");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
}

#[tokio::test]
async fn gateway_api_surface_skips_client_identity_projection_rejection_for_strip_at_forward() {
    use crate::request_context::WebApiSurface;
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Get,
        "/im/v3/api/realtime/ws",
        "realtime",
        "realtime.websocket.upgrade",
        RouteAuth::Public,
    )];

    let profile = WebRequestContextProfile {
        gateway_api_prefixes: vec!["/im/v3/api".to_owned()],
        ..Default::default()
    };
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_profile(profile)
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/im/v3/api/realtime/ws")
        .header("x-sdkwork-tenant-id", "evil-tenant")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("gateway must strip projection headers downstream instead of rejecting");
    assert_eq!(WebApiSurface::GatewayApi, state.api_surface);
    assert!(state.public_path);
}

#[tokio::test]
async fn audit_fact_includes_tenant_and_user_from_principal() {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct CapturingAuditEmitter {
        facts: Arc<Mutex<Vec<AuditFact>>>,
    }

    #[async_trait::async_trait]
    impl AuditEmitter for CapturingAuditEmitter {
        async fn emit(&self, fact: AuditFact) -> Result<(), WebFrameworkError> {
            self.facts.lock().expect("mutex").push(fact);
            Ok(())
        }
    }

    let facts = Arc::new(Mutex::new(Vec::new()));
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_audit_emitter(Arc::new(CapturingAuditEmitter {
            facts: facts.clone(),
        }));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("pipeline");
    let mut response = Response::new(Body::empty());
    chain
        .after(&state, &mut response, &runtime)
        .await
        .expect("audit after");
    let captured = facts.lock().expect("mutex");
    assert_eq!(1, captured.len());
    assert_eq!(Some("100001".to_owned()), captured[0].tenant_id);
    assert_eq!(Some("30".to_owned()), captured[0].user_id);
    assert_eq!("/app/v3/api/users", captured[0].path.as_str());
    assert_eq!(Some(200), captured[0].status_code);
    assert!(captured[0].duration_ms.is_some());
}

#[tokio::test]
async fn public_route_emits_audit_fact() {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct CapturingAuditEmitter {
        facts: Arc<Mutex<Vec<AuditFact>>>,
    }

    #[async_trait::async_trait]
    impl AuditEmitter for CapturingAuditEmitter {
        async fn emit(&self, fact: AuditFact) -> Result<(), WebFrameworkError> {
            self.facts.lock().expect("mutex").push(fact);
            Ok(())
        }
    }

    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_public(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions",
        "Auth",
        "sessions.create",
    )];

    let facts = Arc::new(Mutex::new(Vec::new()));
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES))
        .with_audit_emitter(Arc::new(CapturingAuditEmitter {
            facts: facts.clone(),
        }));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions")
        .header("Access-Token", fixture_bootstrap_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("public credential-entry pipeline");
    let mut response = axum::response::Response::new(Body::empty());
    chain
        .after(&state, &mut response, &runtime)
        .await
        .expect("audit after");
    let captured = facts.lock().expect("mutex");
    assert_eq!(1, captured.len(), "public routes must emit audit facts");
    assert_eq!(
        "sessions.create",
        captured[0].operation_id.as_deref().unwrap()
    );
    assert_eq!(Some(200), captured[0].status_code);
}

#[tokio::test]
async fn domain_injector_runs_at_context_injection() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_profile(WebRequestContextProfile {
            public_path_prefixes: vec!["/app/v3/api/public".to_owned()],
            ..Default::default()
        })
        .with_domain_injector(Arc::new(MarkerDomainInjector));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/public/ping")
        .header("Access-Token", fixture_bootstrap_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("pipeline");
    assert!(request.extensions().get::<InjectorMarker>().is_some());
    assert!(request.extensions().get::<WebRequestContext>().is_some());
}

#[tokio::test]
async fn manifest_credential_entry_route_requires_access_token_jwt() {
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
        .header("Access-Token", fixture_bootstrap_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("credential-entry route accepts bootstrap access token jwt");
    assert_eq!(
        "100001",
        state
            .principal
            .as_ref()
            .expect("tenant isolation")
            .tenant_id()
    );
}

#[tokio::test]
async fn manifest_credential_entry_route_rejects_authorization_header() {
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
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_bootstrap_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("credential-entry route with auth token");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert!(error.message.contains("authorization"));
}

#[tokio::test]
async fn manifest_public_route_rejects_missing_access_token() {
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
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("public route without access token");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
    assert!(error.message.contains("Access-Token"));
}

#[tokio::test]
async fn manifest_public_infra_route_allows_missing_access_token() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/system/health",
        "system",
        "health.retrieve",
    )];

    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/system/health")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("infra public route accepts unauthenticated probes");
    assert_eq!(WebAuthMode::Public, state.auth_mode);
    assert!(state.principal.is_none());
}

#[tokio::test]
async fn manifest_public_route_rejects_malformed_optional_access_token() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/system/health",
        "system",
        "health.retrieve",
    )];

    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/system/health")
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
        .expect_err("semicolon claim-string access token");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
}

#[tokio::test]
async fn manifest_protected_route_still_requires_credentials() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Get,
        "/app/v3/api/users",
        "Users",
        "users.list",
        RouteAuth::DualToken,
    )];

    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .uri("/app/v3/api/users")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("protected route without tokens");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
}

#[tokio::test]
async fn manifest_public_route_with_path_parameter_skips_auth() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Get,
        "/app/v3/api/oauth/device_authorizations/{deviceAuthorizationId}",
        "oauth",
        "oauth.deviceAuthorizations.retrieve",
        RouteAuth::Public,
    )];

    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/app/v3/api/oauth/device_authorizations/qr_session_abc")
        .header("origin", "https://chat.example.test")
        .header("Access-Token", fixture_bootstrap_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("parameterized public route must skip auth and cors rejection");
    assert!(state.public_path);
}

#[tokio::test]
async fn manifest_open_api_public_route_skips_open_api_credentials() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Get,
        "/im/v3/api/realtime/ws",
        "realtime",
        "realtime.websocket.upgrade",
        RouteAuth::Public,
    )];

    let profile = WebRequestContextProfile {
        open_api_prefixes: vec!["/im/v3/api".to_owned()],
        ..Default::default()
    };
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_profile(profile)
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/im/v3/api/realtime/ws")
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("open-api public websocket upgrade must skip open-api credentials");
    assert_eq!(WebAuthMode::Public, state.auth_mode);
    assert!(state.public_path);
    assert!(state.principal.is_none());
}

#[tokio::test]
async fn open_api_protected_route_accepts_dual_token_credentials() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Get,
        "/im/v3/api/chat/inbox",
        "conversations",
        "inbox.list",
        RouteAuth::OpenApiFlexible,
    )];

    let profile = WebRequestContextProfile {
        open_api_prefixes: vec!["/im/v3/api".to_owned()],
        ..Default::default()
    };
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_profile(profile)
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let auth_value = fixture_auth_header();
    let access_value = fixture_access_header();
    let mut request = Request::builder()
        .method("GET")
        .uri("/im/v3/api/chat/inbox")
        .header("Authorization", auth_value)
        .header("Access-Token", access_value)
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("open-api protected route must accept dual-token app credentials");
    assert_eq!(WebAuthMode::DualToken, state.auth_mode);
    assert!(state.principal.is_some());
}

#[tokio::test]
async fn manifest_refresh_token_route_skips_dual_token_requirement() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions/refresh",
        "auth",
        "sessions.refresh",
        RouteAuth::RefreshToken,
    )];

    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions/refresh")
        .header("content-type", "application/json")
        .header("Access-Token", fixture_bootstrap_access_header())
        .body(Body::from(r#"{"refreshToken":"rt-1"}"#))
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("refresh-token route must skip header credential requirements");
    assert!(state.public_path);
}
