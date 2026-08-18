use crate::{
    access_token_jwt, auth_token_jwt, bootstrap_access_token_jwt, memory_idempotency_store,
    AuditEmitter, AuditFact, AuthorizationPolicy, DefaultOpenApiWebRequestContextResolver,
    DefaultWebRequestContextResolver, DomainContextInjector, HttpRouteManifest, WebAuthMode,
    WebCallInterceptorChain, WebCallRuntime, WebCallState, WebFrameworkError,
    WebFrameworkErrorKind, WebRequestContext, WebRequestContextProfile, WebRequestContextResolver,
};
use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const COMPATIBILITY_SCHEMES: &[sdkwork_web_contract::CompatibilitySecurityScheme] = &[
    sdkwork_web_contract::CompatibilitySecurityScheme::api_key_header("OpenAIKey", "X-OpenAI-Key"),
];
const COMPATIBILITY_REQUIREMENT_NAMES: &[&str] = &["OpenAIKey"];
const COMPATIBILITY_REQUIREMENTS: &[sdkwork_web_contract::CompatibilitySecurityRequirement] = &[
    sdkwork_web_contract::CompatibilitySecurityRequirement::all_of(COMPATIBILITY_REQUIREMENT_NAMES),
];
const COMPATIBILITY_AUTH: sdkwork_web_contract::CompatibilityAuth =
    sdkwork_web_contract::CompatibilityAuth::new(
        "openai-v1",
        COMPATIBILITY_SCHEMES,
        COMPATIBILITY_REQUIREMENTS,
    );

#[derive(Clone, Default)]
struct CompatibilityTestResolver(DefaultWebRequestContextResolver);

#[async_trait::async_trait]
impl WebRequestContextResolver for CompatibilityTestResolver {
    async fn resolve_api_key(
        &self,
        raw_api_key: &str,
    ) -> Result<crate::WebRequestPrincipal, WebFrameworkError> {
        self.0.resolve_api_key(raw_api_key).await
    }

    async fn resolve_dual_token(
        &self,
        raw_auth_token: &str,
        raw_access_token: &str,
    ) -> Result<crate::WebRequestPrincipal, WebFrameworkError> {
        self.0
            .resolve_dual_token(raw_auth_token, raw_access_token)
            .await
    }

    async fn resolve_access_token(
        &self,
        raw_access_token: &str,
    ) -> Result<crate::WebRequestPrincipal, WebFrameworkError> {
        self.0.resolve_access_token(raw_access_token).await
    }

    async fn resolve_compatibility(
        &self,
        external_protocol_id: &str,
        credentials: &[crate::CompatibilityCredential],
    ) -> Result<crate::WebRequestPrincipal, WebFrameworkError> {
        assert_eq!("openai-v1", external_protocol_id);
        assert_eq!("OpenAIKey", credentials[0].scheme_name);
        assert_eq!("external-secret", credentials[0].value);
        self.0.resolve_api_key(fixture_ingress_token()).await
    }
}

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

fn fixture_ingress_token() -> &'static str {
    "api_key_id=internal-1;tenant_id=100001;user_id=service-drive;app_id=knowledgebase"
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
async fn manifest_idempotent_bounds_key_before_store_access() {
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

    let mut accepted_request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/orders")
        .header("content-length", "0")
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_access_header())
        .header("Idempotency-Key", "a".repeat(128))
        .body(Body::empty())
        .expect("request");
    let mut accepted_state = WebCallState::from_request(&accepted_request);
    chain
        .before(&mut accepted_state, &mut accepted_request, &runtime)
        .await
        .expect("128-byte key");

    let mut rejected_request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/orders")
        .header("content-length", "0")
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_access_header())
        .header("Idempotency-Key", "b".repeat(129))
        .body(Body::empty())
        .expect("request");
    let mut rejected_state = WebCallState::from_request(&rejected_request);
    let error = chain
        .before(&mut rejected_state, &mut rejected_request, &runtime)
        .await
        .expect_err("129-byte key");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert_eq!(Some("invalid-idempotency-key"), error.reason.as_deref());
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
async fn internal_api_accepts_only_canonical_ingress_token_header() {
    use crate::request_context::WebApiSurface;
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::ingress_token(
        HttpMethod::Get,
        "/internal/v3/api/drive/resources/example",
        "drive",
        "driveResources.retrieve",
    )];

    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/internal/v3/api/drive/resources/example")
        .header("X-SDKWork-Ingress-Token", fixture_ingress_token())
        .header("Access-Token", fixture_access_header())
        .body(Body::empty())
        .expect("internal-api request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("canonical ingress token should authenticate");
    assert_eq!(WebApiSurface::InternalApi, state.api_surface);
    assert_eq!(WebAuthMode::IngressToken, state.auth_mode);
    assert_eq!(
        Some("100001"),
        state
            .principal
            .as_ref()
            .map(|principal| principal.tenant_id())
    );

    for (header_name, header_value) in [
        ("X-API-Key", fixture_ingress_token().to_string()),
        (
            "Authorization",
            format!("Bearer {}", fixture_ingress_token()),
        ),
        (
            "X-SDKWork-Access-Token",
            fixture_ingress_token().to_string(),
        ),
    ] {
        let mut request = Request::builder()
            .method("GET")
            .uri("/internal/v3/api/drive/resources/example")
            .header(header_name, header_value)
            .body(Body::empty())
            .expect("internal-api request");
        let mut state = WebCallState::from_request(&request);
        let error = chain
            .before(&mut state, &mut request, &runtime)
            .await
            .unwrap_err();
        assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
        assert_eq!(
            Some("credential-profile-contamination"),
            error.reason.as_deref()
        );
    }
}

#[tokio::test]
async fn internal_api_rejects_missing_ingress_token() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::ingress_token(
        HttpMethod::Get,
        "/internal/v3/api/drive/resources/example",
        "drive",
        "driveResources.retrieve",
    )];
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/internal/v3/api/drive/resources/example")
        .body(Body::empty())
        .expect("internal-api request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("missing ingress token must fail closed");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
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
        RouteAuth::DualToken,
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
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_access_header())
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("gateway must strip projection headers downstream instead of rejecting");
    assert_eq!(WebApiSurface::GatewayApi, state.api_surface);
    assert!(!state.public_path);
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

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_bootstrap(
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
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_bootstrap(
        HttpMethod::Get,
        "/app/v3/api/public/ping",
        "system",
        "system.ping",
    )];
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES))
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

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_bootstrap(
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

    const ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_bootstrap(
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
async fn manifest_refresh_route_accepts_body_proof_without_credential_headers() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::refresh_token(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions/refresh",
        "Auth",
        "sessions.refresh",
    )];

    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/app/v3/api/auth/sessions/refresh")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"refreshToken":"rt-1"}"#))
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("refresh-token route delegates body proof validation to the handler");
    assert_eq!(WebAuthMode::RefreshToken, state.auth_mode);
    assert!(state.principal.is_none());
}

#[tokio::test]
async fn manifest_app_api_public_route_skips_credentials() {
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
        .expect("manifest-declared app-api public route skips credentials");
    assert_eq!(WebAuthMode::Public, state.auth_mode);
    assert!(state.principal.is_none());
    assert!(state.public_path);
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
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert_eq!(
        Some("credential-profile-contamination"),
        error.reason.as_deref()
    );
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
async fn bound_manifest_rejects_unregistered_app_api_route_auth_profile() {
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
        .uri("/app/v3/api/system/iam/runtime")
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("unregistered app-api route must not default to dual-token");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
    assert_eq!(
        Some("unregistered-route-auth-profile"),
        error.reason.as_deref()
    );
}

#[tokio::test]
async fn manifest_app_api_public_route_with_path_parameter_skips_credentials() {
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
        .body(Body::empty())
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("parameterized app-api public route skips credentials");
    assert!(state.public_path);
    assert_eq!(WebAuthMode::Public, state.auth_mode);
    assert!(state.principal.is_none());
}

#[tokio::test]
async fn manifest_backend_bootstrap_body_route_skips_header_credentials_without_being_public() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::bootstrap_body(
        HttpMethod::Post,
        "/backend/v3/api/iam/access_credentials",
        "iam",
        "accessCredentials.create",
    )];

    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/backend/v3/api/iam/access_credentials")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"username":"owner@example.test","password":"secret"}"#,
        ))
        .expect("request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("backend bootstrap-body route skips framework credential resolution");
    assert!(state.public_path);
    assert_eq!(WebAuthMode::BootstrapBody, state.auth_mode);
    assert!(state.principal.is_none());
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
async fn open_api_flexible_route_accepts_oauth_bearer_credentials() {
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
    let runtime = WebCallRuntime::new(DefaultOpenApiWebRequestContextResolver::default())
        .with_profile(profile)
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/im/v3/api/chat/inbox")
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
        .expect("open-api flexible route must accept OAuth bearer credentials");
    assert_eq!(WebAuthMode::OAuth, state.auth_mode);
    assert!(state.principal.is_some());
}

#[tokio::test]
async fn api_key_or_dual_token_route_accepts_each_complete_alternative() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::api_key_or_dual_token(
        HttpMethod::Get,
        "/im/v3/api/social/contacts",
        "social",
        "social.contacts.list",
    )];
    let profile = WebRequestContextProfile {
        open_api_prefixes: vec!["/im/v3/api".to_owned()],
        ..Default::default()
    };
    let runtime = WebCallRuntime::new(DefaultOpenApiWebRequestContextResolver::default())
        .with_profile(profile)
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();

    let mut api_key_request = Request::builder()
        .method("GET")
        .uri("/im/v3/api/social/contacts")
        .header(
            "X-API-Key",
            "api_key_id=key-1;tenant_id=100001;user_id=30;app_id=appbase",
        )
        .body(Body::empty())
        .expect("api key request");
    let mut api_key_state = WebCallState::from_request(&api_key_request);
    chain
        .before(&mut api_key_state, &mut api_key_request, &runtime)
        .await
        .expect("API key alternative");
    assert_eq!(WebAuthMode::ApiKey, api_key_state.auth_mode);

    let mut dual_token_request = Request::builder()
        .method("GET")
        .uri("/im/v3/api/social/contacts")
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_access_header())
        .body(Body::empty())
        .expect("dual token request");
    let mut dual_token_state = WebCallState::from_request(&dual_token_request);
    chain
        .before(&mut dual_token_state, &mut dual_token_request, &runtime)
        .await
        .expect("dual token alternative");
    assert_eq!(WebAuthMode::DualToken, dual_token_state.auth_mode);
}

#[tokio::test]
async fn api_key_or_dual_token_route_rejects_profile_mixing() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::api_key_or_dual_token(
        HttpMethod::Get,
        "/im/v3/api/social/contacts",
        "social",
        "social.contacts.list",
    )];
    let runtime = WebCallRuntime::new(DefaultOpenApiWebRequestContextResolver::default())
        .with_profile(WebRequestContextProfile {
            open_api_prefixes: vec!["/im/v3/api".to_owned()],
            ..Default::default()
        })
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/im/v3/api/social/contacts")
        .header("X-API-Key", "key-1")
        .header("Authorization", fixture_auth_header())
        .header("Access-Token", fixture_access_header())
        .body(Body::empty())
        .expect("mixed request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("mixed profiles must fail");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert_eq!(
        Some("credential-profile-contamination"),
        error.reason.as_deref()
    );
    assert_eq!(
        Some("surface-classification"),
        error.failed_stage.as_deref()
    );
}

#[tokio::test]
async fn manifest_refresh_token_route_rejects_automatic_credentials() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::refresh_token(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions/refresh",
        "auth",
        "sessions.refresh",
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
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("refresh-token route must reject inherited Access-Token");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert_eq!(
        Some("credential-profile-contamination"),
        error.reason.as_deref()
    );
}

#[tokio::test]
async fn compatibility_route_uses_declared_adapter_and_rejects_profile_contamination() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::compatibility(
        HttpMethod::Post,
        "/openai/v1/chat/completions",
        "OpenAI",
        "openai.chat.completions.create",
        COMPATIBILITY_AUTH,
        r#"{"operationId":"openai.chat.completions.create","responses":{"200":{"description":"ok"}}}"#,
    )];
    let profile = WebRequestContextProfile {
        open_api_prefixes: vec!["/openai/v1".to_owned()],
        ..Default::default()
    };
    let runtime = WebCallRuntime::new(CompatibilityTestResolver::default())
        .with_profile(profile)
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/openai/v1/chat/completions")
        .header("X-OpenAI-Key", "external-secret")
        .body(Body::empty())
        .expect("compatibility request");
    let mut state = WebCallState::from_request(&request);
    chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect("declared compatibility adapter");
    assert_eq!(WebAuthMode::Compatibility, state.auth_mode);
    assert!(state.principal.is_some());

    let mut contaminated = Request::builder()
        .method("POST")
        .uri("/openai/v1/chat/completions")
        .header("X-OpenAI-Key", "external-secret")
        .header("X-API-Key", "undeclared")
        .body(Body::empty())
        .expect("contaminated request");
    let mut state = WebCallState::from_request(&contaminated);
    let error = chain
        .before(&mut state, &mut contaminated, &runtime)
        .await
        .expect_err("undeclared standard credential must fail before adapter");
    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
    assert_eq!(Some("compatibility"), error.auth_profile.as_deref());
    assert_eq!(
        Some("surface-classification"),
        error.failed_stage.as_deref()
    );
    assert_eq!(
        Some("credential-profile-contamination"),
        error.reason.as_deref()
    );
}

#[tokio::test]
async fn compatibility_route_missing_credentials_is_diagnostic_401() {
    use sdkwork_web_contract::{HttpMethod, HttpRoute};

    const ROUTES: &[HttpRoute] = &[HttpRoute::compatibility(
        HttpMethod::Post,
        "/openai/v1/chat/completions",
        "OpenAI",
        "openai.chat.completions.create",
        COMPATIBILITY_AUTH,
        r#"{"operationId":"openai.chat.completions.create","responses":{"200":{"description":"ok"}}}"#,
    )];
    let runtime = WebCallRuntime::new(CompatibilityTestResolver::default())
        .with_profile(WebRequestContextProfile {
            open_api_prefixes: vec!["/openai/v1".to_owned()],
            ..Default::default()
        })
        .with_route_manifest(HttpRouteManifest::new(ROUTES));
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("POST")
        .uri("/openai/v1/chat/completions")
        .body(Body::empty())
        .expect("compatibility request");
    let mut state = WebCallState::from_request(&request);
    let error = chain
        .before(&mut state, &mut request, &runtime)
        .await
        .expect_err("missing compatibility credential");
    assert_eq!(WebFrameworkErrorKind::MissingCredentials, error.kind);
    assert_eq!(40101, error.result_code());
    assert_eq!(Some("compatibility"), error.auth_profile.as_deref());
    assert_eq!(
        Some("request-context-resolution"),
        error.failed_stage.as_deref()
    );
    assert_eq!(
        Some("missing-compatibility-credential"),
        error.reason.as_deref()
    );
}
