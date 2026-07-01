use crate::error::WebFrameworkError;
use crate::request_context::WebRequestContext;
use crate::route_manifest::HttpRouteManifest;
use async_trait::async_trait;

/// Stage 12 — manifest-driven permission enforcement (wildcard-aware).
#[derive(Clone, Copy, Debug)]
pub struct ManifestAuthorizationPolicy {
    pub manifest: HttpRouteManifest,
}

impl ManifestAuthorizationPolicy {
    pub const fn new(manifest: HttpRouteManifest) -> Self {
        Self { manifest }
    }
}

impl AuthorizationPolicy for ManifestAuthorizationPolicy {
    fn authorize(
        &self,
        ctx: &WebRequestContext,
        _operation_id: Option<&str>,
    ) -> Result<(), WebFrameworkError> {
        let route = self
            .manifest
            .match_route(&ctx.transport.method, &ctx.transport.path);

        if route.is_some_and(|matched| matched.auth.skips_credential_resolution()) {
            return Ok(());
        }

        ctx.require_principal()?;

        if let Some(required) = route.and_then(|matched| matched.required_permission) {
            if !ctx.has_permission(required) {
                return Err(WebFrameworkError::forbidden(format!(
                    "missing required permission: {required}"
                )));
            }
        }

        Ok(())
    }
}

/// Stage 12 — business authorization decisions.
pub trait AuthorizationPolicy: Send + Sync {
    fn authorize(
        &self,
        ctx: &WebRequestContext,
        operation_id: Option<&str>,
    ) -> Result<(), WebFrameworkError>;
}

/// Stage 13 — tenant boundary enforcement.
pub trait TenantIsolationPolicy: Send + Sync {
    fn enforce(
        &self,
        ctx: &WebRequestContext,
        operation_id: Option<&str>,
    ) -> Result<(), WebFrameworkError>;
}

/// Stage 16 — audit emission hook.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditFact {
    pub request_id: String,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub api_surface: crate::request_context::WebApiSurface,
    pub path: String,
    pub method: String,
    pub operation_id: Option<String>,
    /// HTTP response status when audit runs after the handler (stage 16 `after`).
    pub status_code: Option<u16>,
    /// Wall-clock milliseconds from pipeline acceptance (stage 15) to audit emission.
    pub duration_ms: Option<u64>,
}

/// Stage 4 / 8 — security event emission (EP-13, catalog C12).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecurityEventKind {
    CorsDenied,
    RateLimitExceeded,
    AuthenticationFailed,
    AuthorizationDenied,
    TenantIsolationDenied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityEvent {
    pub kind: SecurityEventKind,
    pub request_id: Option<String>,
    /// Resolved tenant id when principal is authenticated; `None` for unauthenticated
    /// requests (DB column falls back to platform-shared "0" via migration 010).
    /// SECURITY_SPEC §5.1 / DATABASE_SPEC §6.3：多租户表必须携带 tenant_id。
    pub tenant_id: Option<String>,
    pub path: String,
    pub method: String,
    pub api_surface: crate::request_context::WebApiSurface,
    pub origin: Option<String>,
    pub detail: String,
}

#[async_trait]
pub trait AuditEmitter: Send + Sync {
    async fn emit(&self, fact: AuditFact) -> Result<(), WebFrameworkError>;
}

#[async_trait]
pub trait SecurityEventEmitter: Send + Sync {
    async fn emit(&self, event: SecurityEvent) -> Result<(), WebFrameworkError>;
}

/// Production-safe default: deny authorization until a real policy is wired (EP-06).
#[derive(Clone, Debug, Default)]
pub struct DenyAllAuthorizationPolicy;

impl AuthorizationPolicy for DenyAllAuthorizationPolicy {
    fn authorize(
        &self,
        _ctx: &WebRequestContext,
        _operation_id: Option<&str>,
    ) -> Result<(), WebFrameworkError> {
        Err(WebFrameworkError::forbidden(
            "authorization policy is not configured for this deployment",
        ))
    }
}

/// Test-only default: allow all authenticated requests.
#[derive(Clone, Debug, Default)]
pub struct AllowAllAuthorizationPolicy;

impl AuthorizationPolicy for AllowAllAuthorizationPolicy {
    fn authorize(
        &self,
        _ctx: &WebRequestContext,
        _operation_id: Option<&str>,
    ) -> Result<(), WebFrameworkError> {
        Ok(())
    }
}

/// Test-only default: pass-through tenant isolation.
#[derive(Clone, Debug, Default)]
pub struct PassThroughTenantIsolationPolicy;

impl TenantIsolationPolicy for PassThroughTenantIsolationPolicy {
    fn enforce(
        &self,
        _ctx: &WebRequestContext,
        _operation_id: Option<&str>,
    ) -> Result<(), WebFrameworkError> {
        Ok(())
    }
}

/// Production bootstrap default: require resolved principal and tenant id before handler logic.
#[derive(Clone, Debug, Default)]
pub struct EnforcePrincipalTenantIsolationPolicy;

impl TenantIsolationPolicy for EnforcePrincipalTenantIsolationPolicy {
    fn enforce(
        &self,
        ctx: &WebRequestContext,
        _operation_id: Option<&str>,
    ) -> Result<(), WebFrameworkError> {
        ctx.require_principal()?;
        ctx.require_tenant_id()?;
        ctx.require_app_id()?;
        if ctx.api_surface == crate::request_context::WebApiSurface::BackendApi
            && ctx.login_scope() == Some(crate::request_context::WebLoginScope::Tenant)
        {
            return Err(WebFrameworkError::forbidden(
                "backend API rejects personal sessions (login_scope TENANT)",
            ));
        }
        Ok(())
    }
}

/// Default no-op audit sink.
#[derive(Clone, Debug, Default)]
pub struct NoOpAuditEmitter;

#[async_trait]
impl AuditEmitter for NoOpAuditEmitter {
    async fn emit(&self, _fact: AuditFact) -> Result<(), WebFrameworkError> {
        Ok(())
    }
}

/// Default no-op security event sink.
#[derive(Clone, Debug, Default)]
pub struct NoOpSecurityEventEmitter;

#[async_trait]
impl SecurityEventEmitter for NoOpSecurityEventEmitter {
    async fn emit(&self, _event: SecurityEvent) -> Result<(), WebFrameworkError> {
        Ok(())
    }
}
