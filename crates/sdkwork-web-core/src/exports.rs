//! Crate-root re-exports (facade for the public API).
//!
//! Split from `lib.rs` per RUST_CODE_SPEC §1: lib.rs must stay under ~150 lines
//! and contain only module declarations, re-exports, and lightweight wiring.

pub use crate::api_chain::{
    WebCallCredentials, WebCallCredentials as ApiCallCredentials, WebCallInterceptor,
    WebCallInterceptor as ApiCallInterceptor, WebCallInterceptorChain,
    WebCallInterceptorChain as ApiCallInterceptorChain, WebCallRuntime,
    WebCallRuntime as ApiCallRuntime, WebCallStage, WebCallStage as ApiCallStage, WebCallState,
    WebCallState as ApiCallState, WebFrameworkRuntime, STANDARD_STAGE_ORDER,
};
pub use crate::axum_integration::{
    RequireAppApi, RequireDualToken, RequireOpenApi, RequirePrincipal, RequireTenantApp,
};
pub use crate::client_context_guard::{
    inspect_json_body_context_selectors, is_forbidden_context_selector_key,
    reject_client_context_selectors, reject_forbidden_ambient_context_path,
    reject_forbidden_context_body_json, reject_forbidden_context_query,
    requires_client_context_selector_guard,
};
pub use crate::constants::{
    ACCESS_TOKEN_HEADER, API_KEY_HEADER, APP_API_PREFIX, AUTHORIZATION_HEADER, BACKEND_API_PREFIX,
    CONTENT_SHA256_HEADER, DYNAMIC_POLICY_CACHE_TTL_SECS, GATEWAY_API_PREFIX,
    IDEMPOTENCY_FINGERPRINT_HEADER, IDEMPOTENCY_KEY_HEADER, OPEN_API_PREFIX, OPERATION_ID_HEADER,
    PRODUCTION_DEFAULT_REQUEST_TIMEOUT_SECS, PRODUCTION_DEFAULT_SHUTDOWN_GRACE_SECS,
    REQUEST_ID_HEADER, WS_APP_API_SUFFIX, X_IDEMPOTENCY_KEY_HEADER,
};
pub use crate::context_injection::{inject_web_request_context, DomainContextInjector};
pub use crate::cors_policy::{
    CorsPolicyContext, DynamicCorsPolicySource, NoOpDynamicCorsPolicySource,
};
pub use crate::error::{
    AppRequestContextError, AppRequestContextErrorKind, WebFrameworkError, WebFrameworkErrorKind,
    WebFrameworkRejection,
};
pub use crate::idempotency::{
    content_length_from_headers, idempotency_fingerprint, idempotency_replay_response,
    resolve_idempotency_fingerprint, IdempotencyBeginOutcome, IdempotencyResponseRecord,
};
pub use crate::interceptors::{
    StandardWebCallInterceptor, StandardWebCallInterceptor as StandardApiCallInterceptor,
    StandardWebCallInterceptorKind,
    StandardWebCallInterceptorKind as StandardApiCallInterceptorKind,
};
pub use crate::jwt::{
    is_hmac_sha256_jwt_verifier, is_payload_only_jwt_verifier, uses_global_shared_jwt_verifier,
    validate_production_jwt_verifier, HmacSha256JwtVerifier, JwtVerifier, PayloadOnlyJwtVerifier,
    StrictJwtVerifier, VerifyingAccessTokenParser, VerifyingAuthTokenParser,
};
pub use crate::jwt_claims::{validate_jwt_token_type_claim, JwtProductionClaimPolicy};
pub use crate::jwt_fixtures::{
    access_token_jwt, auth_token_jwt, auth_token_jwt_with_permissions, bootstrap_access_token_jwt,
    encode_hs256_test_jwt, encode_hs256_test_jwt_with_kid, encode_hs256_test_jwt_without_kid,
    encode_rs256_test_jwt_with_kid, encode_unsigned_test_jwt, generate_rs256_test_keypair,
};
pub use crate::jwt_tenant::{
    EnvBootstrapTenantSigningKeyLookup, JwtSessionRevocationChecker,
    NoOpJwtSessionRevocationChecker, StaticJwtSessionRevocationChecker,
    StaticTenantSigningKeyLookup, TenantBoundJwtVerifier, TenantSigningKeyAlgorithm,
    TenantSigningKeyLookup, TenantSigningKeyMaterial,
};
pub use crate::metrics::{
    environment_metric_label, http_request_labels_from_state, HttpMetricsDimensions,
    HttpMetricsRegistry, HttpRequestLabels,
};
pub use crate::open_api_auth::{
    allowed_open_api_schemes, default_open_api_scheme_detector, resolve_open_api_request_context,
    DefaultOpenApiCredentialSchemeDetector, DynOpenApiCredentialSchemeDetector, OpenApiAuthPolicy,
    OpenApiAuthScheme, OpenApiCredentialSchemeDetector,
};
pub use crate::parsers::{
    AccessTokenClaims, AccessTokenParser, ApiKeyCredential, ApiKeyParser, AuthTokenClaims,
    AuthTokenParser, DefaultAccessTokenParser, DefaultApiKeyParser, DefaultAuthTokenParser,
    DefaultOAuthBearerParser, OAuthBearerCredential, OAuthBearerParser,
};
pub use crate::path_resource_guard::{
    extract_path_resource_ids, verify_path_resource_ids_match_principal, PathResourceIds,
};
pub use crate::policies::{
    AllowAllAuthorizationPolicy, AuditEmitter, AuditFact, AuthorizationPolicy,
    DenyAllAuthorizationPolicy, EnforcePrincipalTenantIsolationPolicy, ManifestAuthorizationPolicy,
    NoOpAuditEmitter, NoOpSecurityEventEmitter, PassThroughTenantIsolationPolicy, SecurityEvent,
    SecurityEventEmitter, SecurityEventKind, TenantIsolationPolicy,
};
pub use crate::policy_cache::{
    CachingDynamicCorsPolicySource, CachingDynamicRateLimitPolicySource,
    CachingDynamicTenantRuntimeProfileSource, DynamicPolicyCaches, TtlCache,
};
pub use crate::problem::{
    enrich_problem_response, problem_response, redact_path_template, ProblemCorrelation,
};
pub use crate::production_assembly::{
    requires_production_assembly, validate_deployment_environment, validate_production_assembly,
    ProductionAssemblyInput,
};
pub use crate::rate_limit::{
    limits_for_tier, DefaultRateLimitPolicyResolver, RateLimitPolicyResolver,
    ResolvedRateLimitPolicy,
};
pub use crate::rate_limit_policy::{
    rate_limit_tier_key, DynamicRateLimitPolicySource, NoOpDynamicRateLimitPolicySource,
    RateLimitPolicyContext,
};
pub use crate::redact::{
    is_redacted_log_field, redact_sensitive_header, redact_sensitive_log_value,
};
pub use crate::request_context::{
    AppRequestApiSurface, AppRequestAuthLevel, AppRequestAuthMode, AppRequestContext,
    AppRequestContextProfile, AppRequestDeploymentMode, AppRequestEnvironment,
    AppRequestLoginScope, AppRequestPrincipal, WebApiSurface, WebAppContext, WebAuthContext,
    WebAuthLevel, WebAuthMode, WebClientKind, WebDeploymentMode, WebEnvironment, WebLoginScope,
    WebOperationBinding, WebRequestContext, WebRequestContextProfile, WebRequestPrincipal,
    WebRequestPrincipalBuilder, WebScopeContext, WebSubjectContext, WebSubjectType,
    WebTenancyContext, WebTransportFacts,
};
pub use crate::request_identity::{
    is_canonical_uuid, new_request_id, resolve_request_id, ServerRequestId,
};
pub use crate::resolvers::{
    is_tenant_bound_verifying_resolver, tenant_bound_saas_verifying_web_request_resolver,
    tenant_bound_saas_verifying_web_request_resolver_with_claim_policy,
    tenant_bound_verifying_web_request_resolver,
    tenant_bound_verifying_web_request_resolver_with_claim_policy,
    verifying_open_api_web_request_resolver, verifying_web_request_resolver, ApiKeyLookupService,
    ApiKeyPrincipalRecord, DefaultApiKeyLookupService, DefaultOAuthTokenLookupService,
    DefaultOpenApiWebRequestContextResolver, DefaultWebRequestContextResolver,
    DefaultWebRequestContextResolver as DefaultAppRequestContextResolver,
    DisabledApiKeyLookupService, OAuthPrincipalRecord, OAuthTokenLookupService,
    OpenApiWebRequestParserResolver, ResolverProductionProfile,
    TenantBoundProductionWebRequestResolver, VerifyingOAuthBearerParser, WebRequestContextResolver,
    WebRequestContextResolver as AppRequestContextResolver, WebRequestParserResolver,
    WebRequestParserResolver as AppRequestParserResolver,
};
pub use crate::route_manifest::{route_path_matches, HttpRouteManifest};
pub use crate::runtime_options::WebFrameworkOptionalFeatures;
pub use crate::security::{
    CorsPolicy, CrossSiteRequestPolicy, HeaderSecurityPolicy, IdempotencyPolicy,
    JsonContentTypePolicy, MethodGuardPolicy, RateLimitPolicy, RequestSecurityPolicy,
    RequestSizeLimitPolicy, SecurityPolicy, SqlInjectionGuardPolicy,
};
pub use crate::stores::{
    memory_concurrent_admission_store, memory_idempotency_store, memory_rate_limit_store,
    ConcurrentAdmissionStore, IdempotencyGuard, IdempotencyStore, MemoryConcurrentAdmissionStore,
    MemoryIdempotencyStore, MemoryRateLimitStore, RateLimitStore,
};
pub use crate::surface::{
    api_surface_contract_label, api_surface_metric_label, classify_api_surface, resolve_public_path,
};
pub use crate::tenant_app_context::TenantAppContext;
pub use crate::tenant_runtime::{
    DynamicTenantRuntimeProfileSource, NoOpDynamicTenantRuntimeProfileSource, TenantRuntimeProfile,
    TenantRuntimeProfileContext,
};
pub use crate::token_version::{
    extract_token_version_from_json, stamp_token_version, validate_token_version,
    validate_token_version_claims, validate_token_version_json, TokenVersionPolicy,
    SDKWORK_TOKEN_VERSION_CURRENT,
};
pub use crate::trace::{
    resolve_trace_context, trace_id_from_traceparent, TraceContext, TRACEPARENT_HEADER,
    TRACESTATE_HEADER,
};
pub use crate::websocket::{
    WebSocketCallInterceptor, WebSocketCallInterceptorChain, WebSocketCallRuntime,
    WebSocketCallStage, WebSocketCallState, WebSocketMessageFrame, WebSocketSession,
};
pub use crate::ws_interceptors::{
    StandardWebSocketCallInterceptor, StandardWebSocketCallInterceptorKind,
};
pub use sdkwork_web_contract::{
    ApiSurface, HttpMethod, HttpRoute, IamHttpRoute, RateLimitTier, RouteAuth,
};
