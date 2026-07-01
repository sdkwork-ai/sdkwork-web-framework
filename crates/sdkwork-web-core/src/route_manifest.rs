use sdkwork_web_contract::{HttpMethod, HttpRoute, RateLimitTier, RouteAuth};

use crate::client_context_guard::is_canonical_iam_context_resource_path;
use crate::client_context_guard::requires_client_context_selector_guard;
use crate::constants::FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS;
use crate::request_context::{WebApiSurface, WebRequestContextProfile};
use crate::surface::{classify_api_surface, matches_prefix};

/// Static route manifest for operationId / rate-limit tier resolution (EP-17 lite).
#[derive(Clone, Copy, Debug)]
pub struct HttpRouteManifest {
    routes: &'static [HttpRoute],
}

impl HttpRouteManifest {
    pub const fn new(routes: &'static [HttpRoute]) -> Self {
        Self { routes }
    }

    pub fn routes(&self) -> &'static [HttpRoute] {
        self.routes
    }

    pub fn match_route(&self, method: &str, path: &str) -> Option<&HttpRoute> {
        let normalized = normalize_path(path);
        self.routes.iter().find(|route| {
            http_method_matches(route.method, method) && route_path_matches(route.path, &normalized)
        })
    }

    pub fn rate_limit_tier_for(&self, method: &str, path: &str) -> Option<RateLimitTier> {
        self.match_route(method, path)
            .and_then(|route| route.rate_limit_tier)
    }

    pub fn is_public_route(&self, method: &str, path: &str) -> bool {
        self.match_route(method, path)
            .is_some_and(|route| route.auth.skips_credential_resolution())
    }

    pub fn public_routes(&self) -> impl Iterator<Item = &'static HttpRoute> {
        self.routes
            .iter()
            .filter(|route| route.auth.skips_credential_resolution())
    }

    /// Ensures infra [`public_path_prefixes`](crate::request_context::WebRequestContextProfile::public_path_prefixes)
    /// do not cover protected manifest routes.
    pub fn validate_public_path_prefixes(&self, prefixes: &[String]) -> Result<(), String> {
        for route in self.routes {
            if route.auth.skips_credential_resolution() {
                continue;
            }
            let normalized = normalize_path(route.path);
            for prefix in prefixes {
                if matches_prefix(&normalized, prefix) {
                    return Err(format!(
                        "protected manifest route {} {} is covered by public_path_prefix {prefix:?}",
                        http_method_label(route.method),
                        route.path
                    ));
                }
            }
        }
        Ok(())
    }

    /// Ensures manifest `RouteAuth` matches the API surface inferred from each route path.
    ///
    /// Non-open-api routes (app-api, backend-api, gateway-api) that are not public or
    /// refresh-token entrypoints must declare `RouteAuth::DualToken` so materialized OpenAPI
    /// and runtime credential rules stay aligned. Backend-api additionally permits
    /// `RouteAuth::AgentToken` for `/backend/v3/api/agent/*` routes (C8-C9).
    pub fn validate_route_auth_for_surfaces(
        &self,
        profile: &WebRequestContextProfile,
    ) -> Result<(), String> {
        for route in self.routes {
            let surface = classify_api_surface(route.path, profile);
            match surface {
                WebApiSurface::AppApi | WebApiSurface::GatewayApi => {
                    if route.auth.skips_credential_resolution() {
                        continue;
                    }
                    if !route.auth.requires_dual_token_headers() {
                        return Err(format!(
                            "non-open-api route {} {} must declare RouteAuth::DualToken (found {})",
                            http_method_label(route.method),
                            route.path,
                            route_auth_label(route.auth),
                        ));
                    }
                }
                WebApiSurface::BackendApi => {
                    if route.auth.skips_credential_resolution() {
                        continue;
                    }
                    // Backend-api permits DualToken (standard) or AgentToken (C8-C9 agent routes).
                    if !route.auth.requires_dual_token_headers()
                        && !route.auth.is_agent_token_credential_mode()
                    {
                        return Err(format!(
                            "backend-api route {} {} must declare RouteAuth::DualToken or RouteAuth::AgentToken (found {})",
                            http_method_label(route.method),
                            route.path,
                            route_auth_label(route.auth),
                        ));
                    }
                }
                WebApiSurface::OpenApi => {
                    if route.auth.skips_credential_resolution() {
                        continue;
                    }
                    if route.auth.requires_dual_token_headers()
                        || route.auth == RouteAuth::RefreshToken
                    {
                        return Err(format!(
                            "open-api route {} {} must not use {} auth; declare api-key, oauth, or open-api-flexible",
                            http_method_label(route.method),
                            route.path,
                            route_auth_label(route.auth),
                        ));
                    }
                    if !route.auth.is_open_api_credential_mode() {
                        return Err(format!(
                            "open-api protected route {} {} must declare an open-api credential mode (found {})",
                            http_method_label(route.method),
                            route.path,
                            route_auth_label(route.auth),
                        ));
                    }
                }
                WebApiSurface::Unknown => {}
            }
        }
        Ok(())
    }

    /// Ensures manifest paths on SaaS surfaces do not embed ambient tenant/org scoping (B8).
    pub fn validate_no_ambient_context_path_markers(
        &self,
        profile: &WebRequestContextProfile,
    ) -> Result<(), String> {
        for route in self.routes {
            let surface = classify_api_surface(route.path, profile);
            if !requires_client_context_selector_guard(surface) {
                continue;
            }
            let normalized = normalize_path(route.path).to_ascii_lowercase();
            if is_canonical_iam_context_resource_path(&normalized) {
                continue;
            }
            for marker in FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS {
                if normalized.contains(marker) {
                    return Err(format!(
                        "manifest route {} {} uses forbidden ambient context path marker `{marker}` (B8)",
                        http_method_label(route.method),
                        route.path,
                    ));
                }
            }
        }
        Ok(())
    }
}

fn route_auth_label(auth: RouteAuth) -> &'static str {
    match auth {
        RouteAuth::Public => "public",
        RouteAuth::RefreshToken => "refresh-token",
        RouteAuth::DualToken => "dualToken",
        RouteAuth::ApiKey => "apiKey",
        RouteAuth::OAuth => "oauth",
        RouteAuth::OpenApiFlexible => "openApiFlexible",
        RouteAuth::AgentToken => "agentToken",
    }
}

fn normalize_path(path: &str) -> String {
    let value = path.trim();
    if value.is_empty() {
        return "/".to_owned();
    }
    format!("/{}", value.trim_matches('/'))
}

fn path_segments(path: &str) -> Vec<String> {
    normalize_path(path)
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Matches OpenAPI-style manifest paths (including `{param}` segments) to request paths.
pub fn route_path_matches(manifest_path: &str, request_path: &str) -> bool {
    let template_segments = path_segments(manifest_path);
    let request_segments = path_segments(request_path);
    if template_segments.len() != request_segments.len() {
        return false;
    }
    template_segments
        .iter()
        .zip(request_segments.iter())
        .all(|(template, actual)| {
            if template.starts_with('{') && template.ends_with('}') {
                !actual.is_empty()
            } else {
                template == actual
            }
        })
}

fn http_method_label(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
    }
}

fn http_method_matches(route_method: HttpMethod, method: &str) -> bool {
    let upper = method.to_ascii_uppercase();
    matches!(
        (route_method, upper.as_str()),
        (HttpMethod::Get, "GET")
            | (HttpMethod::Post, "POST")
            | (HttpMethod::Put, "PUT")
            | (HttpMethod::Patch, "PATCH")
            | (HttpMethod::Delete, "DELETE")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_web_contract::{HttpMethod, RouteAuth};

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Post,
        "/app/v3/api/auth/sessions",
        "Auth",
        "createSession",
        RouteAuth::Public,
    )
    .with_rate_limit_tier(RateLimitTier::AuthCritical)];

    #[test]
    fn matches_manifest_route_and_tier() {
        let manifest = HttpRouteManifest::new(ROUTES);
        let route = manifest
            .match_route("POST", "/app/v3/api/auth/sessions")
            .expect("route");
        assert_eq!("createSession", route.operation_id);
        assert_eq!(
            Some(RateLimitTier::AuthCritical),
            manifest.rate_limit_tier_for("POST", "/app/v3/api/auth/sessions")
        );
        assert!(manifest.is_public_route("POST", "/app/v3/api/auth/sessions"));
        assert!(!manifest.is_public_route("GET", "/app/v3/api/auth/sessions"));
    }

    #[test]
    fn rejects_public_prefix_covering_protected_route() {
        const PROTECTED: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/app/v3/api/users/me",
            "Users",
            "users.me",
            RouteAuth::DualToken,
        )];
        let manifest = HttpRouteManifest::new(PROTECTED);
        let error = manifest
            .validate_public_path_prefixes(&["/app/v3/api/users".to_owned()])
            .expect_err("prefix must not cover protected route");
        assert!(error.contains("/app/v3/api/users/me"));
    }

    #[test]
    fn matches_manifest_route_with_path_parameter() {
        const ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/app/v3/api/oauth/device_authorizations/{deviceAuthorizationId}",
            "oauth",
            "oauth.deviceAuthorizations.retrieve",
            RouteAuth::Public,
        )];
        let manifest = HttpRouteManifest::new(ROUTES);
        assert!(manifest.is_public_route(
            "GET",
            "/app/v3/api/oauth/device_authorizations/qr_session_key_123"
        ));
        assert!(!manifest.is_public_route(
            "GET",
            "/app/v3/api/oauth/device_authorizations/qr_session_key_123/scans"
        ));
    }

    #[test]
    fn route_path_matches_supports_openapi_templates() {
        assert!(route_path_matches(
            "/app/v3/api/oauth/callbacks/{providerCode}",
            "/app/v3/api/oauth/callbacks/github"
        ));
        assert!(!route_path_matches(
            "/app/v3/api/oauth/callbacks/{providerCode}",
            "/app/v3/api/oauth/callbacks/github/extra"
        ));
    }

    #[test]
    fn rejects_non_open_api_route_without_dual_token_auth() {
        use crate::request_context::WebRequestContextProfile;

        const ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/app/v3/api/users",
            "Users",
            "users.list",
            RouteAuth::ApiKey,
        )];
        let manifest = HttpRouteManifest::new(ROUTES);
        let error = manifest
            .validate_route_auth_for_surfaces(&WebRequestContextProfile::default())
            .expect_err("app-api protected route must require dual token");
        assert!(error.contains("RouteAuth::DualToken"));
    }

    #[test]
    fn accepts_non_open_api_public_and_dual_token_routes() {
        use crate::request_context::WebRequestContextProfile;

        const ROUTES: &[HttpRoute] = &[
            HttpRoute::public(
                HttpMethod::Post,
                "/app/v3/api/auth/sessions",
                "Auth",
                "sessions.create",
            ),
            HttpRoute::dual_token(
                HttpMethod::Get,
                "/backend/v3/api/iam/users",
                "iam",
                "users.list",
            ),
        ];
        let manifest = HttpRouteManifest::new(ROUTES);
        manifest
            .validate_route_auth_for_surfaces(&WebRequestContextProfile::default())
            .expect("public and dual-token routes are valid");
    }

    #[test]
    fn rejects_open_api_route_with_dual_token_auth() {
        use crate::request_context::WebRequestContextProfile;

        const ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/open/v3/api/messages",
            "Messages",
            "messages.list",
            RouteAuth::DualToken,
        )];
        let manifest = HttpRouteManifest::new(ROUTES);
        let error = manifest
            .validate_route_auth_for_surfaces(&WebRequestContextProfile::default())
            .expect_err("open-api must not use dual token");
        assert!(error.contains("open-api route"));
    }

    #[test]
    fn rejects_ambient_tenant_path_on_app_api_surface() {
        use crate::request_context::WebRequestContextProfile;

        const ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/app/v3/api/tenants/{tenantId}/orders",
            "Orders",
            "orders.list",
            RouteAuth::DualToken,
        )];
        let manifest = HttpRouteManifest::new(ROUTES);
        let error = manifest
            .validate_no_ambient_context_path_markers(&WebRequestContextProfile::default())
            .expect_err("ambient tenant path");
        assert!(error.contains("/tenants/"));
    }
}
