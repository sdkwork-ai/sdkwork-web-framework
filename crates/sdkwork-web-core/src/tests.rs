//! Core crate integration tests (moved from lib.rs per RUST_CODE_SPEC §1).

use crate::*;
use async_trait::async_trait;
use std::collections::BTreeMap;

#[test]
fn classifies_standard_api_surfaces() {
    let profile = WebRequestContextProfile::default();
    assert_eq!(
        WebApiSurface::AppApi,
        classify_api_surface("/app/v3/api/auth/sessions", &profile)
    );
    assert_eq!(
        WebApiSurface::BackendApi,
        classify_api_surface("/backend/v3/api/iam/users", &profile)
    );
    assert_eq!(
        WebApiSurface::OpenApi,
        classify_api_surface("/open/v3/api/messages", &profile)
    );
}

#[test]
fn chain_order_matches_api_spec() {
    let chain = WebCallInterceptorChain::<DefaultWebRequestContextResolver>::standard();
    assert_eq!(18, chain.interceptor_count());
    assert_eq!(
        STANDARD_STAGE_ORDER.as_slice(),
        chain.stage_order().as_slice()
    );
}

#[test]
fn web_request_context_json_matches_schema_vocabulary() {
    use crate::request_context::{WebClientKind, WebOperationBinding};
    use sdkwork_web_contract::RateLimitTier;

    let ctx = WebRequestContext {
        request_id: ServerRequestId("550e8400-e29b-41d4-a716-446655440000".to_owned()),
        api_surface: WebApiSurface::BackendApi,
        auth_mode: WebAuthMode::DualToken,
        principal: None,
        transport: WebTransportFacts {
            path: "/backend/v3/api/web-framework/cors_policies".to_owned(),
            method: "GET".to_owned(),
            auth_token_present: true,
            access_token_present: true,
            api_key_present: false,
            oauth_bearer_present: false,
            agent_token_present: false,
        },
        locale: Some("en-US".to_owned()),
        client_kind: Some(WebClientKind::Browser),
        operation: Some(WebOperationBinding {
            operation_id: "webFramework.corsPolicies.list".to_owned(),
            route_template: "/backend/v3/api/web-framework/cors_policies".to_owned(),
            rate_limit_tier: Some(RateLimitTier::OpenApiDefault),
            idempotent: false,
        }),
        trace_id: None,
    };
    let json = serde_json::to_value(&ctx).expect("serialize context");
    assert!(json.get("requestId").is_some());
    assert!(json.get("apiSurface").is_some());
    assert!(json.get("authMode").is_some());
    assert!(json.get("transport").is_some());
    let transport = json["transport"].as_object().expect("transport object");
    assert!(transport.contains_key("authTokenPresent"));
    assert!(transport.contains_key("accessTokenPresent"));
    assert!(transport.contains_key("apiKeyPresent"));
    assert!(transport.contains_key("oauthBearerPresent"));
    assert!(json.get("clientKind").is_some());
    assert!(json.get("operation").is_some());
}

#[test]
fn api_surface_bridge_matches_contract() {
    assert_eq!(
        WebApiSurface::AppApi,
        WebApiSurface::from(ApiSurface::AppApi)
    );
    assert_eq!(
        ApiSurface::OpenApi,
        ApiSurface::from(WebApiSurface::OpenApi)
    );
}

#[test]
fn workspace_manifest_has_no_business_dependencies() {
    let manifest = include_str!("../Cargo.toml");
    for forbidden in ["sdkwork-appbase", "sdkwork-clawrouter", "openchat"] {
        assert!(
            !manifest.contains(forbidden),
            "core crate must not depend on {forbidden}"
        );
    }
}

#[test]
fn standard_call_chain_declares_common_interceptors() {
    let chain = WebCallInterceptorChain::<DefaultWebRequestContextResolver>::standard();
    assert_eq!(18, chain.interceptor_count());
}

#[test]
fn production_runtime_enables_rate_limit_and_deny_all_auth() {
    let runtime = WebCallRuntime::production(DefaultWebRequestContextResolver::default());
    assert!(runtime.security_policy.rate_limit.enabled);
    let ctx = WebRequestContext {
        request_id: ServerRequestId("req-1".to_owned()),
        api_surface: WebApiSurface::AppApi,
        auth_mode: WebAuthMode::DualToken,
        principal: Some(
            WebRequestPrincipal::builder()
                .tenant_id("100001")
                .user_id("user-1")
                .app_id("appbase")
                .build(),
        ),
        transport: WebTransportFacts {
            path: "/app/v3/api/users".to_owned(),
            method: "GET".to_owned(),
            auth_token_present: true,
            access_token_present: true,
            api_key_present: false,
            oauth_bearer_present: false,
            agent_token_present: false,
        },
        locale: None,
        client_kind: None,
        operation: None,
        trace_id: None,
    };
    let error = runtime
        .authorization
        .authorize(&ctx, Some("listUsers"))
        .expect_err("deny all");
    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
}

#[tokio::test]
async fn verifying_resolver_rejects_inline_claim_strings() {
    use crate::jwt::PayloadOnlyJwtVerifier;
    use crate::verifying_web_request_resolver;
    use std::sync::Arc;

    let resolver = verifying_web_request_resolver(
        Arc::new(PayloadOnlyJwtVerifier),
        DefaultApiKeyLookupService,
    );
    let error = resolver
        .resolve_dual_token(
            "tenant_id=100001;user_id=30;session_id=session-1;app_id=appbase;auth_level=password",
            "tenant_id=100001;user_id=30;session_id=session-1;app_id=appbase;environment=prod;deployment_mode=saas",
        )
        .await
        .expect_err("inline claims rejected");
    assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
}

#[tokio::test]
async fn default_api_key_resolver_maps_claims_to_principal() {
    let resolver = DefaultWebRequestContextResolver::default();
    let principal = resolver
        .resolve_api_key(
            "api_key_id=key-1;tenant_id=100001;organization_id=0;user_id=30;app_id=appbase;environment=prod;deployment_mode=saas;data_scope=tenant;permission_scope=iam.read,settings.read",
        )
        .await
        .expect("principal");

    assert_eq!("100001", principal.tenant_id());
    assert_eq!(Some("0"), principal.organization_id());
    assert_eq!("30", principal.user_id());
    assert_eq!(Some("key-1"), principal.api_key_id());
    assert_eq!(WebAuthLevel::ApiKey, principal.auth_level());
}

#[tokio::test]
async fn default_access_token_resolver_establishes_tenant_isolation() {
    let resolver = DefaultWebRequestContextResolver::default();
    let principal = resolver
        .resolve_access_token(&bootstrap_access_token_jwt(
            "100001",
            "app_tenant-bootstrap",
        ))
        .await
        .expect("access token principal");

    assert_eq!("100001", principal.tenant_id());
    assert_eq!("app_tenant-bootstrap", principal.app_id());
    assert_eq!(WebAuthLevel::Anonymous, principal.auth_level());
}

#[tokio::test]
async fn default_dual_token_resolver_rejects_mismatched_tenant() {
    let resolver = DefaultWebRequestContextResolver::default();
    let error = resolver
        .resolve_dual_token(
            &auth_token_jwt("100001", "1", "session-1", "appbase"),
            &access_token_jwt("100002", "1", "session-1", "appbase"),
        )
        .await
        .expect_err("mismatch");

    assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
}

#[tokio::test]
async fn custom_api_key_lookup_service_can_resolve_raw_key() {
    #[derive(Clone)]
    struct StaticApiKeyLookupService;

    #[async_trait]
    impl ApiKeyLookupService for StaticApiKeyLookupService {
        async fn lookup_api_key(
            &self,
            credential: &ApiKeyCredential,
        ) -> Result<ApiKeyPrincipalRecord, WebFrameworkError> {
            assert_eq!("plain-secret-key", credential.raw_value);
            Ok(ApiKeyPrincipalRecord {
                api_key_id: "custom-key-1".to_owned(),
                tenant_id: "100001".to_owned(),
                organization_id: Some("0".to_owned()),
                user_id: "user-custom".to_owned(),
                app_id: "appbase".to_owned(),
                environment: WebEnvironment::Prod,
                deployment_mode: WebDeploymentMode::Saas,
                data_scope: vec!["tenant".to_owned()],
                permission_scope: vec!["custom.read".to_owned()],
                subject_type: Some("api_key".to_owned()),
                metadata: BTreeMap::new(),
            })
        }
    }

    let resolver = WebRequestParserResolver::new(
        DefaultAuthTokenParser,
        DefaultAccessTokenParser,
        DefaultApiKeyParser,
        StaticApiKeyLookupService,
    );

    let principal = resolver
        .resolve_api_key("plain-secret-key")
        .await
        .expect("principal");

    assert_eq!("100001", principal.tenant_id());
    assert_eq!(Some("custom-key-1"), principal.api_key_id());
}

#[tokio::test]
async fn default_open_api_resolver_accepts_oauth_bearer_claims() {
    let resolver = DefaultOpenApiWebRequestContextResolver::default();
    let principal = resolver
        .resolve_oauth_bearer(
            "token_id=tok-1;tenant_id=100001;user_id=30;app_id=appbase;environment=prod",
        )
        .await
        .expect("oauth principal");

    assert_eq!("100001", principal.tenant_id());
    assert_eq!("30", principal.user_id());
    assert_eq!(WebSubjectType::Service, principal.subject.subject_type);
}

#[tokio::test]
async fn open_api_flexible_detector_selects_oauth_when_only_bearer_present() {
    use axum::http::{HeaderMap, HeaderValue};

    let detector = DefaultOpenApiCredentialSchemeDetector::default();
    let credentials = WebCallCredentials {
        auth_token: Some("oauth-token".to_owned()),
        access_token: None,
        api_key: None,
        oauth_bearer: Some("oauth-token".to_owned()),
        agent_token: None,
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION_HEADER,
        HeaderValue::from_static("Bearer oauth-token"),
    );
    let scheme = detector
        .detect(&credentials, &headers, Some(RouteAuth::OpenApiFlexible))
        .expect("detect")
        .expect("scheme");
    assert_eq!(OpenApiAuthScheme::OAuthBearer, scheme);
}
