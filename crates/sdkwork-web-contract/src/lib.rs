//! Contract types for SDKWork HTTP route manifests.

mod openapi;

use serde::{Deserialize, Serialize};

pub use openapi::{
    build_openapi_document, build_openapi_operation, build_openapi_path_item,
    infer_api_surface_from_path, openapi_extensions_for_route,
    validate_openapi_document_context_selectors, validate_openapi_routes_context_selectors,
    OPENAPI_API_SURFACE_EXTENSION, OPENAPI_AUTH_MODE_EXTENSION,
    OPENAPI_FORBID_CREDENTIAL_HEADERS_EXTENSION, OPENAPI_PERMISSION_EXTENSION,
    OPENAPI_RATE_LIMIT_TIER_EXTENSION, OPENAPI_REQUEST_CONTEXT_EXTENSION,
    OPENAPI_REQUIRED_SURFACE_EXTENSION, OPENAPI_ROUTE_AUTH_EXTENSION,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApiSurface {
    OpenApi,
    AppApi,
    BackendApi,
    GatewayApi,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RateLimitTier {
    /// Highest priority — authentication and authorization endpoints.
    AuthCritical,
    /// Default tier for OpenAPI schema routes.
    OpenApiDefault,
    /// File upload / media ingestion — typically higher quotas than API calls.
    Upload,
    /// Search and query operations — can be expensive on database side.
    Search,
    /// Bulk operations — batch processing with moderate throughput needs.
    Bulk,
    /// Background jobs and async workers — long-running processes.
    Worker,
    /// Internal/platform service-to-service communication.
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RouteAuth {
    Public,
    DualToken,
    ApiKey,
    /// OAuth 2.0 bearer token (`Authorization: Bearer`) for open-api.
    OAuth,
    /// Header-driven open-api auth: API key or OAuth bearer (detector chooses).
    OpenApiFlexible,
    /// Refresh-token proof in request body; skips dual-token and open-api header auth.
    RefreshToken,
    /// Agent bootstrap token (`X-SDKWork-Agent-Token`) on backend-api agent routes.
    ///
    /// Maps to canonical OpenAPI `x-sdkwork-auth-mode: api-key` (API_SPEC §19) but resolves
    /// via [`WebRequestContextResolver::resolve_api_key`] using the agent token credential,
    /// without requiring `Access-Token` or `Authorization: Bearer` JWTs.
    AgentToken,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HttpMethod {
    Delete,
    Get,
    Patch,
    Post,
    Put,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HttpRoute {
    pub method: HttpMethod,
    pub path: &'static str,
    pub tag: &'static str,
    pub operation_id: &'static str,
    pub auth: RouteAuth,
    pub idempotent: bool,
    pub rate_limit_tier: Option<RateLimitTier>,
    pub required_permission: Option<&'static str>,
    /// Alternate permissions that also authorize the operation (e.g. platform read for cross-tenant list).
    pub alternate_permissions: Option<&'static [&'static str]>,
    /// Credential-entry routes (login/register/reset) reject inbound credential headers at runtime.
    pub forbid_credential_headers: bool,
}

impl HttpRoute {
    pub const fn new(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
        auth: RouteAuth,
    ) -> Self {
        Self {
            method,
            path,
            tag,
            operation_id,
            auth,
            idempotent: false,
            rate_limit_tier: None,
            required_permission: None,
            alternate_permissions: None,
            forbid_credential_headers: false,
        }
    }

    pub const fn with_required_permission(mut self, permission: &'static str) -> Self {
        self.required_permission = Some(permission);
        self
    }

    pub const fn with_alternate_permissions(
        mut self,
        permissions: &'static [&'static str],
    ) -> Self {
        self.alternate_permissions = Some(permissions);
        self
    }

    pub const fn with_idempotent(mut self, idempotent: bool) -> Self {
        self.idempotent = idempotent;
        self
    }

    pub const fn with_rate_limit_tier(mut self, tier: RateLimitTier) -> Self {
        self.rate_limit_tier = Some(tier);
        self
    }

    pub const fn with_forbid_credential_headers(mut self, forbid: bool) -> Self {
        self.forbid_credential_headers = forbid;
        self
    }

    /// Marks credential-entry anonymous routes (login/register/reset) per `WEB_FRAMEWORK_SPEC.md`.
    pub const fn credential_entry_public(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::public(method, path, tag, operation_id).with_forbid_credential_headers(true)
    }

    pub const fn public(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::new(method, path, tag, operation_id, RouteAuth::Public)
    }

    pub const fn dual_token(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::new(method, path, tag, operation_id, RouteAuth::DualToken)
    }

    pub const fn api_key(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::new(method, path, tag, operation_id, RouteAuth::ApiKey)
    }

    pub const fn oauth(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::new(method, path, tag, operation_id, RouteAuth::OAuth)
    }

    pub const fn open_api_flexible(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::new(method, path, tag, operation_id, RouteAuth::OpenApiFlexible)
    }

    pub const fn refresh_token(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::new(method, path, tag, operation_id, RouteAuth::RefreshToken)
    }

    /// Backend-api agent route authenticated via `X-SDKWork-Agent-Token` (C8-C9).
    pub const fn agent_token(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::new(method, path, tag, operation_id, RouteAuth::AgentToken)
    }
}

impl RouteAuth {
    /// Routes that skip session auth (`Authorization`) and full dual-token resolution.
    pub const fn skips_credential_resolution(self) -> bool {
        matches!(self, Self::Public | Self::RefreshToken)
    }

    /// Protected app-api / backend-api / gateway-api routes require both auth and access tokens.
    pub const fn requires_dual_token_headers(self) -> bool {
        matches!(self, Self::DualToken)
    }

    /// Open-api protected routes authenticate via API key and/or OAuth bearer headers.
    pub const fn is_open_api_credential_mode(self) -> bool {
        matches!(self, Self::ApiKey | Self::OAuth | Self::OpenApiFlexible)
    }

    /// Backend-api agent routes authenticate via `X-SDKWork-Agent-Token` (C8-C9).
    /// Resolves through `resolve_api_key` without dual-token or `Access-Token` JWT.
    pub const fn is_agent_token_credential_mode(self) -> bool {
        matches!(self, Self::AgentToken)
    }
}

/// Non-open-api HTTP surfaces always require `Access-Token` for tenant isolation.
pub const fn non_open_api_surface_requires_access_token(surface: ApiSurface) -> bool {
    matches!(
        surface,
        ApiSurface::AppApi | ApiSurface::BackendApi | ApiSurface::GatewayApi
    )
}

/// Legacy alias used by early IAM manifests during migration.
pub type IamHttpRoute = HttpRoute;
