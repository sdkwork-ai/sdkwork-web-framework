//! Contract types for SDKWork HTTP route manifests.

mod inventory;
mod openapi;

use serde::{Deserialize, Serialize};

pub use inventory::{
    normalize_route_path, route_inventory_from_openapi, route_inventory_from_routes,
    ApiRouteInventoryEntry,
};
pub use openapi::{
    build_openapi_document, build_openapi_operation, build_openapi_path_item,
    infer_api_surface_from_path, is_canonical_iam_context_resource_path,
    openapi_extensions_for_route, validate_openapi_document_context_selectors,
    validate_openapi_routes_context_selectors, IAM_CANONICAL_CONTEXT_RESOURCE_PREFIXES,
    OPENAPI_API_SURFACE_EXTENSION, OPENAPI_AUTH_MODE_EXTENSION,
    OPENAPI_EXTERNAL_PROTOCOL_ID_EXTENSION, OPENAPI_FORBID_CREDENTIAL_HEADERS_EXTENSION,
    OPENAPI_PERMISSION_EXTENSION, OPENAPI_RATE_LIMIT_TIER_EXTENSION,
    OPENAPI_REQUEST_CONTEXT_EXTENSION, OPENAPI_REQUIRED_SURFACE_EXTENSION,
    OPENAPI_ROUTE_AUTH_EXTENSION, OPENAPI_WIRE_PROTOCOL_EXTENSION,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApiSurface {
    OpenApi,
    AppApi,
    BackendApi,
    InternalApi,
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
#[serde(rename_all = "kebab-case")]
pub enum RouteAuth {
    Public,
    CredentialEntryBootstrap,
    DualToken,
    ApiKey,
    /// Application-ingress token for protected internal-api routes.
    IngressToken,
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
    Compatibility,
}

/// OpenAPI security scheme used by an explicitly governed vendor-compatibility route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilitySecuritySchemeKind {
    ApiKeyHeader { header_name: &'static str },
    HttpBearer { bearer_format: Option<&'static str> },
}

/// Named compatibility security scheme. `name` is the OpenAPI component/security key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilitySecurityScheme {
    pub name: &'static str,
    pub kind: CompatibilitySecuritySchemeKind,
}

impl CompatibilitySecurityScheme {
    pub const fn api_key_header(name: &'static str, header_name: &'static str) -> Self {
        Self {
            name,
            kind: CompatibilitySecuritySchemeKind::ApiKeyHeader { header_name },
        }
    }

    pub const fn http_bearer(name: &'static str, bearer_format: Option<&'static str>) -> Self {
        Self {
            name,
            kind: CompatibilitySecuritySchemeKind::HttpBearer { bearer_format },
        }
    }
}

/// One OpenAPI security requirement object. Multiple rows are alternatives (OR); scheme names
/// within a row are required together (AND).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilitySecurityRequirement {
    pub scheme_names: &'static [&'static str],
}

impl CompatibilitySecurityRequirement {
    pub const fn all_of(scheme_names: &'static [&'static str]) -> Self {
        Self { scheme_names }
    }
}

/// Explicit external authentication contract for `RouteAuth::Compatibility`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityAuth {
    pub external_protocol_id: &'static str,
    pub schemes: &'static [CompatibilitySecurityScheme],
    pub requirements: &'static [CompatibilitySecurityRequirement],
}

impl CompatibilityAuth {
    pub const fn new(
        external_protocol_id: &'static str,
        schemes: &'static [CompatibilitySecurityScheme],
        requirements: &'static [CompatibilitySecurityRequirement],
    ) -> Self {
        Self {
            external_protocol_id,
            schemes,
            requirements,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.external_protocol_id.is_empty()
            || !self
                .external_protocol_id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err("external protocol id must be non-empty lowercase kebab-case".to_owned());
        }
        if self.schemes.is_empty() || self.requirements.is_empty() {
            return Err(
                "compatibility auth must declare schemes and security requirements".to_owned(),
            );
        }
        for scheme in self.schemes {
            if scheme.name.trim().is_empty() {
                return Err("compatibility security scheme name must not be empty".to_owned());
            }
            match scheme.kind {
                CompatibilitySecuritySchemeKind::ApiKeyHeader { header_name }
                    if header_name.trim().is_empty() =>
                {
                    return Err("compatibility API-key header name must not be empty".to_owned());
                }
                _ => {}
            }
        }
        for requirement in self.requirements {
            if requirement.scheme_names.is_empty() {
                return Err("compatibility security requirement must not be empty".to_owned());
            }
            for name in requirement.scheme_names {
                if !self.schemes.iter().any(|scheme| scheme.name == *name) {
                    return Err(format!(
                        "compatibility security requirement references unknown scheme {name}"
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn scheme(&self, name: &str) -> Option<&CompatibilitySecurityScheme> {
        self.schemes.iter().find(|scheme| scheme.name == name)
    }
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
    /// Required external authentication metadata for `RouteAuth::Compatibility`.
    pub compatibility_auth: Option<CompatibilityAuth>,
    /// Exact upstream-compatible OpenAPI operation JSON for compatibility routes.
    pub compatibility_openapi_operation: Option<&'static str>,
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
            compatibility_auth: None,
            compatibility_openapi_operation: None,
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

    pub fn validate_compatibility_contract(&self) -> Result<(), String> {
        if self.auth != RouteAuth::Compatibility {
            if self.compatibility_auth.is_some() || self.compatibility_openapi_operation.is_some() {
                return Err(format!(
                    "non-compatibility route {} must not declare compatibility metadata",
                    self.operation_id
                ));
            }
            return Ok(());
        }
        let auth = self.compatibility_auth.as_ref().ok_or_else(|| {
            format!(
                "compatibility route {} must declare external authentication metadata",
                self.operation_id
            )
        })?;
        auth.validate()?;
        let source = self.compatibility_openapi_operation.ok_or_else(|| {
            format!(
                "compatibility route {} must provide exact upstream OpenAPI operation JSON",
                self.operation_id
            )
        })?;
        let operation: serde_json::Value = serde_json::from_str(source).map_err(|error| {
            format!(
                "compatibility route {} has invalid OpenAPI operation JSON: {error}",
                self.operation_id
            )
        })?;
        let object = operation.as_object().ok_or_else(|| {
            format!(
                "compatibility route {} OpenAPI operation must be a JSON object",
                self.operation_id
            )
        })?;
        if object
            .get("operationId")
            .and_then(serde_json::Value::as_str)
            != Some(self.operation_id)
        {
            return Err(format!(
                "compatibility route {} operationId must match its upstream OpenAPI operation",
                self.operation_id
            ));
        }
        if !object
            .get("responses")
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(format!(
                "compatibility route {} must preserve upstream response definitions",
                self.operation_id
            ));
        }
        Ok(())
    }

    /// First-class credential-entry route (login/register/OAuth/reset).
    pub const fn credential_entry_bootstrap(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::new(
            method,
            path,
            tag,
            operation_id,
            RouteAuth::CredentialEntryBootstrap,
        )
        .with_forbid_credential_headers(true)
    }

    /// Migration alias for [`Self::credential_entry_bootstrap`].
    #[deprecated(
        since = "0.1.0",
        note = "use HttpRoute::credential_entry_bootstrap; credential entry is not anonymous"
    )]
    pub const fn credential_entry_public(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::credential_entry_bootstrap(method, path, tag, operation_id)
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

    pub const fn ingress_token(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
    ) -> Self {
        Self::new(method, path, tag, operation_id, RouteAuth::IngressToken)
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

    /// Vendor-compatibility operation with adapter-defined authentication semantics.
    pub const fn compatibility(
        method: HttpMethod,
        path: &'static str,
        tag: &'static str,
        operation_id: &'static str,
        auth: CompatibilityAuth,
        openapi_operation_json: &'static str,
    ) -> Self {
        let mut route = Self::new(method, path, tag, operation_id, RouteAuth::Compatibility)
            .with_forbid_credential_headers(true);
        route.compatibility_auth = Some(auth);
        route.compatibility_openapi_operation = Some(openapi_operation_json);
        route
    }
}

impl RouteAuth {
    /// Routes that bypass session authorization and dual-token resolution.
    ///
    /// Credential-entry still resolves its bootstrap access JWT. Refresh-token handlers validate
    /// their declared body proof. The existing name remains for source compatibility.
    pub const fn skips_credential_resolution(self) -> bool {
        matches!(
            self,
            Self::Public | Self::CredentialEntryBootstrap | Self::RefreshToken
        )
    }

    pub const fn is_anonymous(self) -> bool {
        matches!(self, Self::Public)
    }

    pub const fn requires_bootstrap_access_token(self) -> bool {
        matches!(self, Self::CredentialEntryBootstrap)
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

    /// Internal-api routes authenticate with an application ingress token.
    pub const fn is_ingress_token_credential_mode(self) -> bool {
        matches!(self, Self::IngressToken)
    }

    /// Canonical contract label used by route manifests, OpenAPI, diagnostics, and SDK policy.
    pub const fn auth_profile_label(self) -> &'static str {
        match self {
            Self::Public => "anonymous",
            Self::CredentialEntryBootstrap => "credential-entry-bootstrap",
            Self::RefreshToken => "refresh-token",
            Self::DualToken => "dual-token",
            Self::ApiKey => "api-key",
            Self::IngressToken => "ingress-token",
            Self::OAuth => "oauth",
            Self::OpenApiFlexible => "open-api-flexible",
            Self::AgentToken => "agent-token",
            Self::Compatibility => "compatibility",
        }
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
