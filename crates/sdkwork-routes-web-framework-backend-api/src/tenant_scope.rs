//! Tenant scope enforcement for framework control-plane handlers (`WEB_BACKEND_SPEC.md`).

use crate::response::ApiProblem;
use sdkwork_web_core::WebRequestContext;

pub const PERM_TENANT_ADMIN: &str = "web-framework.tenant.admin";
pub const PERM_PLATFORM_READ: &str = "web-framework.platform.read";
pub const PERM_CONTROL_PLANE: &str = "web-framework.control-plane";

/// Audit list scope — tenant admins never receive cross-tenant or NULL tenant rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEventListScope {
    Tenant(String),
    PlatformTenant(String),
    PlatformAll,
}

/// Security event list scope — control-plane permission only.
/// SECURITY_SPEC §5.1：安全事件为平台级敏感数据，租户管理员不可越权访问。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityEventListScope {
    /// 控制台管理员下钻到指定租户的安全事件。
    Tenant(String),
    /// 跨租户全量视图（仅 `web-framework.control-plane` 权限）。
    PlatformAll,
}

pub fn require_control_plane(ctx: &WebRequestContext) -> Result<(), ApiProblem> {
    if ctx.has_permission(PERM_CONTROL_PLANE) {
        return Ok(());
    }
    Err(ApiProblem::forbidden(format!(
        "missing required permission: {PERM_CONTROL_PLANE}"
    )))
}

/// Resolves the tenant id for list queries — scoped to authenticated tenant unless platform read is granted.
pub fn resolve_list_tenant_id(
    ctx: &WebRequestContext,
    query_tenant_id: Option<&str>,
) -> Result<String, ApiProblem> {
    if ctx.has_permission(PERM_PLATFORM_READ) {
        if let Some(requested) = query_tenant_id.filter(|value| !value.is_empty()) {
            return Ok(requested.to_owned());
        }
        return ctx
            .require_tenant_id()
            .map(str::to_owned)
            .map_err(ApiProblem::from_web_framework);
    }

    let tenant = ctx
        .require_tenant_id()
        .map_err(ApiProblem::from_web_framework)?;
    if let Some(requested) = query_tenant_id.filter(|value| !value.is_empty()) {
        if requested != tenant {
            return Err(ApiProblem::forbidden(
                "tenant_id query parameter does not match authenticated tenant",
            ));
        }
    }
    Ok(tenant.to_owned())
}

pub fn require_upsert_tenant_id(
    ctx: &WebRequestContext,
    body_tenant_id: &str,
) -> Result<(), ApiProblem> {
    if body_tenant_id.trim().is_empty() {
        return Err(ApiProblem::bad_request("tenant_id must not be empty"));
    }
    if ctx.has_permission(PERM_PLATFORM_READ) {
        return Ok(());
    }
    let tenant = ctx
        .require_tenant_id()
        .map_err(ApiProblem::from_web_framework)?;
    if body_tenant_id != tenant {
        return Err(ApiProblem::forbidden(
            "request tenant_id does not match authenticated tenant",
        ));
    }
    Ok(())
}

pub fn require_tenant_admin(ctx: &WebRequestContext) -> Result<(), ApiProblem> {
    ctx.require_tenant_id()
        .map_err(ApiProblem::from_web_framework)?;
    ctx.require_app_id()
        .map_err(ApiProblem::from_web_framework)?;
    if ctx.has_permission(PERM_TENANT_ADMIN) {
        return Ok(());
    }
    Err(ApiProblem::forbidden(format!(
        "missing required permission: {PERM_TENANT_ADMIN}"
    )))
}

pub fn resolve_audit_event_list_scope(
    ctx: &WebRequestContext,
    query_tenant_id: Option<&str>,
) -> Result<AuditEventListScope, ApiProblem> {
    require_tenant_admin(ctx)?;

    if ctx.has_permission(PERM_PLATFORM_READ) {
        if let Some(requested) = query_tenant_id.filter(|value| !value.is_empty()) {
            return Ok(AuditEventListScope::PlatformTenant(requested.to_owned()));
        }
        return Ok(AuditEventListScope::PlatformAll);
    }

    let tenant = ctx
        .require_tenant_id()
        .map_err(ApiProblem::from_web_framework)?;
    if let Some(requested) = query_tenant_id.filter(|value| !value.is_empty()) {
        if requested != tenant {
            return Err(ApiProblem::forbidden(
                "tenant_id query parameter does not match authenticated tenant",
            ));
        }
    }
    Ok(AuditEventListScope::Tenant(tenant.to_owned()))
}

/// Resolves the security event list scope. Requires `web-framework.control-plane`
/// permission. Platform-wide read when no `tenant_id` query parameter is supplied;
/// otherwise narrowed to the requested tenant. SECURITY_SPEC §5.1.
pub fn resolve_security_event_list_scope(
    ctx: &WebRequestContext,
    query_tenant_id: Option<&str>,
) -> Result<SecurityEventListScope, ApiProblem> {
    require_control_plane(ctx)?;
    if let Some(requested) = query_tenant_id.filter(|value| !value.is_empty()) {
        return Ok(SecurityEventListScope::Tenant(requested.to_owned()));
    }
    Ok(SecurityEventListScope::PlatformAll)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;
    use sdkwork_web_core::{
        ServerRequestId, WebApiSurface, WebAuthLevel, WebAuthMode, WebDeploymentMode,
        WebEnvironment, WebLoginScope, WebRequestContext, WebRequestPrincipal, WebSubjectType,
        WebTransportFacts,
    };

    fn ctx_with_permissions(permissions: &[&str]) -> WebRequestContext {
        let principal = WebRequestPrincipal::builder()
            .tenant_id("100001")
            .organization_id(Some("0".to_owned()))
            .login_scope(WebLoginScope::Tenant)
            .user_id("user-test")
            .session_id(Some("session-test".to_owned()))
            .app_id("appbase")
            .environment(WebEnvironment::Prod)
            .deployment_mode(WebDeploymentMode::Saas)
            .auth_level(WebAuthLevel::Password)
            .subject_type(WebSubjectType::User)
            .permission_scope(
                permissions
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            )
            .build();
        WebRequestContext {
            request_id: ServerRequestId("req-test".to_owned()),
            api_surface: WebApiSurface::BackendApi,
            auth_mode: WebAuthMode::DualToken,
            principal: Some(principal),
            transport: WebTransportFacts {
                path: paths::audit_events::PATH.to_owned(),
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
        }
    }

    #[test]
    fn tenant_admin_scope_excludes_platform_all() {
        let ctx = ctx_with_permissions(&["web-framework.tenant.admin"]);
        let scope = resolve_audit_event_list_scope(&ctx, None).expect("scope");
        assert_eq!(AuditEventListScope::Tenant("100001".to_owned()), scope);
    }

    #[test]
    fn platform_read_scope_allows_all_rows() {
        let ctx =
            ctx_with_permissions(&["web-framework.tenant.admin", "web-framework.platform.read"]);
        let scope = resolve_audit_event_list_scope(&ctx, None).expect("scope");
        assert_eq!(AuditEventListScope::PlatformAll, scope);
    }

    #[test]
    fn platform_read_allows_cross_tenant_upsert() {
        let ctx =
            ctx_with_permissions(&["web-framework.tenant.admin", "web-framework.platform.read"]);
        require_upsert_tenant_id(&ctx, "100002").expect("platform upsert");
    }

    #[test]
    fn tenant_admin_rejects_cross_tenant_upsert() {
        let ctx = ctx_with_permissions(&["web-framework.tenant.admin"]);
        assert!(require_upsert_tenant_id(&ctx, "100002").is_err());
    }

    #[test]
    fn tenant_admin_rejects_cross_tenant_list_query() {
        let ctx = ctx_with_permissions(&["web-framework.tenant.admin"]);
        assert!(resolve_list_tenant_id(&ctx, Some("100002")).is_err());
    }

    #[test]
    fn platform_read_scopes_audit_list_to_requested_tenant() {
        let ctx =
            ctx_with_permissions(&["web-framework.tenant.admin", "web-framework.platform.read"]);
        let scope = resolve_audit_event_list_scope(&ctx, Some("100002")).expect("scope");
        assert_eq!(
            AuditEventListScope::PlatformTenant("100002".to_owned()),
            scope
        );
    }

    #[test]
    fn security_event_scope_requires_control_plane() {
        let ctx = ctx_with_permissions(&["web-framework.tenant.admin"]);
        assert!(resolve_security_event_list_scope(&ctx, None).is_err());
    }

    #[test]
    fn security_event_scope_platform_all_default() {
        let ctx = ctx_with_permissions(&["web-framework.control-plane"]);
        let scope = resolve_security_event_list_scope(&ctx, None).expect("scope");
        assert_eq!(SecurityEventListScope::PlatformAll, scope);
    }

    #[test]
    fn security_event_scope_narrows_to_requested_tenant() {
        let ctx = ctx_with_permissions(&["web-framework.control-plane"]);
        let scope = resolve_security_event_list_scope(&ctx, Some("100002")).expect("scope");
        assert_eq!(SecurityEventListScope::Tenant("100002".to_owned()), scope);
    }
}
