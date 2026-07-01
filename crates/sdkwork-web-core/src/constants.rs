pub const PRODUCTION_DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
/// Upper bound aligned with typical Kubernetes `terminationGracePeriodSeconds` (catalog H4).
pub const PRODUCTION_DEFAULT_SHUTDOWN_GRACE_SECS: u64 = 30;
/// TTL for SQLx-backed dynamic CORS/rate-limit/tenant-runtime overlays (local_cache namespace).
pub const DYNAMIC_POLICY_CACHE_TTL_SECS: u64 = 30;

pub const REQUEST_ID_HEADER: &str = "X-Request-Id";
/// Server-owned request correlation header (`API_SPEC.md` §15).
pub const SDKWORK_TRACE_ID_HEADER: &str = sdkwork_utils_rust::SDKWORK_TRACE_ID_HEADER;
/// Lowercase HTTP header name for `HeaderName::from_static`.
pub const SDKWORK_TRACE_ID_HEADER_LOWER: &str = "x-sdkwork-trace-id";
pub const AUTHORIZATION_HEADER: &str = "Authorization";
pub const ACCESS_TOKEN_HEADER: &str = "Access-Token";
pub const API_KEY_HEADER: &str = "X-Api-Key";
/// Backend agent bootstrap token header (C8-C9). Maps to `RouteAuth::AgentToken`.
pub const AGENT_TOKEN_HEADER: &str = "X-SDKWork-Agent-Token";

pub const APP_API_PREFIX: &str = "/app/v3/api";
pub const BACKEND_API_PREFIX: &str = "/backend/v3/api";
pub const OPEN_API_PREFIX: &str = "/open/v3/api";
pub const GATEWAY_API_PREFIX: &str = "/v1";

/// WebSocket upgrade paths commonly live under app-api.
pub const WS_APP_API_SUFFIX: &str = "/ws";

pub const OPERATION_ID_HEADER: &str = "X-Sdkwork-Operation-Id";
pub const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";
pub const X_IDEMPOTENCY_KEY_HEADER: &str = "X-Idempotency-Key";
/// Client-supplied SHA-256 hex digest of the request body (catalog D6 body fingerprint).
pub const CONTENT_SHA256_HEADER: &str = "X-Content-SHA256";
pub const IDEMPOTENCY_FINGERPRINT_HEADER: &str = "X-Idempotency-Fingerprint";

/// Headers clients must not send to project tenancy/subject into the request context (spec B9 / API_SPEC §10.2).
/// Also includes `x-sdkwork-operation-id` — the framework MUST derive operation_id from the route
/// manifest, not from client-supplied headers (SECURITY_SPEC §5.1 / API_SPEC §10.2).
pub const FORBIDDEN_CLIENT_IDENTITY_HEADERS: &[&str] = &[
    "x-sdkwork-tenant-id",
    "x-sdkwork-app-id",
    "x-sdkwork-user-id",
    "x-sdkwork-organization-id",
    "x-sdkwork-actor-id",
    "x-sdkwork-actor-kind",
    "x-sdkwork-session-id",
    "x-sdkwork-environment",
    "x-sdkwork-deployment-profile",
    "x-sdkwork-deployment-mode",
    "x-sdkwork-runtime-target",
    "x-sdkwork-auth-level",
    "x-sdkwork-data-scope",
    "x-sdkwork-permission-scope",
    "x-sdkwork-device-id",
    "x-sdkwork-context-signature",
    "x-sdkwork-operation-id",
    "x-tenant-id",
    "x-app-id",
    "x-user-id",
    "x-organization-id",
];

/// Query keys that must not select current tenant/app/subject context (B12 / API_SPEC §10.0).
pub const FORBIDDEN_CLIENT_CONTEXT_QUERY_KEYS: &[&str] = &[
    "tenant_id",
    "tenantid",
    "tenant",
    "tenant-id",
    "app_id",
    "appid",
    "app-id",
    "organization_id",
    "organizationid",
    "organization-id",
    "org_id",
    "orgid",
    "user_id",
    "userid",
    "user-id",
    "session_id",
    "sessionid",
    "session-id",
];

/// Path segments that must not scope ambient tenant/org context on SaaS surfaces (B8).
pub const FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS: &[&str] = &["/tenants/", "/organizations/"];

/// Canonical IAM resource roots (API_SPEC §11.3) — not ambient tenant/org path scoping.
pub const IAM_CANONICAL_CONTEXT_RESOURCE_PREFIXES: &[&str] =
    &["/iam/organizations", "/iam/tenants"];

/// Session and API-key headers rejected on credential-entry routes (`forbidCredentialHeaders`).
/// Bootstrap `Access-Token` JWT is EXCLUDED because it remains required for tenant isolation
/// on credential-entry routes — reject it would prevent tenant-context establishment
/// for login/register flows (see WEB_FRAMEWORK_SPEC §7 / API_SPEC §10.2).
pub const FORBIDDEN_CREDENTIAL_ENTRY_HEADERS: &[&str] =
    &["authorization", "x-api-key", "x-sdkwork-agent-token"];
