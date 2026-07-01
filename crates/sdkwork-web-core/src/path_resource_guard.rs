//! Verifies path resource tenant/org identifiers match authenticated principal (API_SPEC §10).

use crate::error::WebFrameworkError;
use crate::request_context::{WebApiSurface, WebRequestContext};
use crate::route_manifest::route_path_matches;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PathResourceIds {
    pub tenant_id: Option<String>,
    pub organization_id: Option<String>,
}

fn normalize_param_name(name: &str) -> String {
    name.chars()
        .filter(|ch| *ch != '_' && *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn is_tenant_path_param(name: &str) -> bool {
    matches!(normalize_param_name(name).as_str(), "tenantid" | "tenant")
}

fn is_organization_path_param(name: &str) -> bool {
    matches!(
        normalize_param_name(name).as_str(),
        "organizationid" | "organization" | "orgid" | "org"
    )
}

fn path_segments(path: &str) -> Vec<String> {
    path.trim()
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Extracts tenant/org resource identifiers declared by OpenAPI-style `{param}` path templates.
pub fn extract_path_resource_ids(manifest_path: &str, request_path: &str) -> PathResourceIds {
    if !route_path_matches(manifest_path, request_path) {
        return PathResourceIds::default();
    }
    let template_segments = path_segments(manifest_path);
    let request_segments = path_segments(request_path);
    let mut ids = PathResourceIds::default();
    for (template, actual) in template_segments.iter().zip(request_segments.iter()) {
        let Some(param_name) = template
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
        else {
            continue;
        };
        if is_tenant_path_param(param_name) {
            ids.tenant_id = Some(actual.clone());
        }
        if is_organization_path_param(param_name) {
            ids.organization_id = Some(actual.clone());
        }
    }
    ids
}

/// Ensures path resource tenant/org identifiers match token context unless platform permission applies.
pub fn verify_path_resource_ids_match_principal(
    ctx: &WebRequestContext,
    manifest_path: &str,
    required_permission: Option<&str>,
) -> Result<(), WebFrameworkError> {
    let Some(principal) = ctx.principal.as_ref() else {
        return Ok(());
    };
    let extracted = extract_path_resource_ids(manifest_path, &ctx.transport.path);
    if extracted.tenant_id.is_none() && extracted.organization_id.is_none() {
        return Ok(());
    }

    if let Some(path_tenant) = extracted.tenant_id {
        if path_tenant != principal.tenant_id() {
            if allows_cross_tenant_resource_access(ctx, required_permission) {
                return Ok(());
            }
            return Err(WebFrameworkError::forbidden(format!(
                "path tenant resource id `{path_tenant}` does not match authenticated tenant `{}`",
                principal.tenant_id()
            )));
        }
    }

    if let Some(path_org) = extracted.organization_id {
        let principal_org = principal.organization_id().unwrap_or("0");
        if path_org != principal_org && path_org != "0" {
            if allows_cross_tenant_resource_access(ctx, required_permission) {
                return Ok(());
            }
            return Err(WebFrameworkError::forbidden(format!(
                "path organization resource id `{path_org}` does not match authenticated organization `{principal_org}`"
            )));
        }
    }

    Ok(())
}

fn allows_cross_tenant_resource_access(
    ctx: &WebRequestContext,
    required_permission: Option<&str>,
) -> bool {
    if ctx.api_surface != WebApiSurface::BackendApi {
        return false;
    }
    required_permission.is_some_and(|permission| ctx.has_permission(permission))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_context::{
        WebApiSurface, WebAuthMode, WebDeploymentMode, WebEnvironment, WebLoginScope,
        WebRequestContext, WebRequestPrincipal, WebTransportFacts,
    };
    use crate::request_identity::ServerRequestId;

    fn principal_context(
        tenant_id: &str,
        organization_id: Option<&str>,
        permissions: &[&str],
        path: &str,
    ) -> WebRequestContext {
        WebRequestContext {
            request_id: ServerRequestId("req-1".to_owned()),
            api_surface: WebApiSurface::AppApi,
            auth_mode: WebAuthMode::DualToken,
            transport: WebTransportFacts {
                path: path.to_owned(),
                method: "GET".to_owned(),
                auth_token_present: true,
                access_token_present: true,
                api_key_present: false,
                oauth_bearer_present: false,
                agent_token_present: false,
            },
            principal: Some(
                WebRequestPrincipal::builder()
                    .tenant_id(tenant_id)
                    .organization_id(organization_id.map(str::to_owned))
                    .login_scope(if organization_id.is_some_and(|org| org != "0") {
                        WebLoginScope::Organization
                    } else {
                        WebLoginScope::Tenant
                    })
                    .user_id("user-1")
                    .session_id(Some("session-1".to_owned()))
                    .app_id("appbase")
                    .environment(WebEnvironment::Prod)
                    .deployment_mode(WebDeploymentMode::Saas)
                    .permission_scope(
                        permissions
                            .iter()
                            .map(|value| (*value).to_owned())
                            .collect(),
                    )
                    .build(),
            ),
            locale: None,
            client_kind: None,
            operation: None,
            trace_id: None,
        }
    }

    #[test]
    fn extracts_tenant_and_org_path_params_from_manifest_template() {
        let ids = extract_path_resource_ids(
            "/backend/v3/api/tenants/{tenantId}/organizations/{organizationId}/settings",
            "/backend/v3/api/tenants/t-1/organizations/o-1/settings",
        );
        assert_eq!(Some("t-1".to_owned()), ids.tenant_id);
        assert_eq!(Some("o-1".to_owned()), ids.organization_id);
    }

    #[test]
    fn rejects_mismatched_tenant_path_resource_on_app_api() {
        let ctx = principal_context("100001", None, &[], "/app/v3/api/resources/100002/items");
        let error = verify_path_resource_ids_match_principal(
            &ctx,
            "/app/v3/api/resources/{tenantId}/items",
            None,
        )
        .expect_err("tenant mismatch");
        assert!(error.message.contains("100002"));
    }

    #[test]
    fn allows_backend_cross_tenant_resource_with_required_permission() {
        let mut ctx = principal_context(
            "100001",
            None,
            &["web-framework.cors-policies.write"],
            "/backend/v3/api/web-framework/tenants/100002/cors_policies",
        );
        ctx.api_surface = WebApiSurface::BackendApi;
        verify_path_resource_ids_match_principal(
            &ctx,
            "/backend/v3/api/web-framework/tenants/{tenantId}/cors_policies",
            Some("web-framework.cors-policies.write"),
        )
        .expect("platform permission authorizes target tenant");
    }
}
