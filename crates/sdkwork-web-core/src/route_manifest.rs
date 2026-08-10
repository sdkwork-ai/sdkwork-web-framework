use std::sync::Arc;

use sdkwork_web_contract::{HttpMethod, HttpRoute, RateLimitTier, RouteAuth};

use crate::client_context_guard::is_canonical_iam_context_resource_path;
use crate::client_context_guard::requires_client_context_selector_guard;
use crate::constants::FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS;
use crate::request_context::{WebApiSurface, WebRequestContextProfile};
use crate::surface::{classify_api_surface, matches_prefix};

/// Owned route manifest for operationId / rate-limit tier resolution (EP-17 lite).
///
/// Static route crates and runtime-composed assemblies share the same contract.
/// Cloning a manifest only clones the reference-counted route inventory.
#[derive(Clone, Debug)]
pub struct HttpRouteManifest {
    routes: Arc<[HttpRoute]>,
}

impl HttpRouteManifest {
    pub fn new(routes: &[HttpRoute]) -> Self {
        Self {
            routes: Arc::from(routes),
        }
    }

    pub fn from_owned_routes(routes: Vec<HttpRoute>) -> Self {
        Self {
            routes: Arc::from(routes),
        }
    }

    pub fn routes(&self) -> &[HttpRoute] {
        &self.routes
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
            .is_some_and(|route| route.auth.is_anonymous())
    }

    pub fn public_routes(&self) -> impl Iterator<Item = &HttpRoute> {
        self.routes.iter().filter(|route| route.auth.is_anonymous())
    }

    /// Ensures infra [`public_path_prefixes`](crate::request_context::WebRequestContextProfile::public_path_prefixes)
    /// do not cover protected manifest routes.
    pub fn validate_public_path_prefixes(&self, prefixes: &[String]) -> Result<(), String> {
        for route in self.routes.iter() {
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
    /// App API routes may be public or use an Access-Token-bearing profile. Gateway routes use
    /// dual-token or access-token-only entry profiles; backend additionally permits body-credential
    /// bootstrap and agent-token plus `Access-Token`; internal routes use ingress-token plus
    /// `Access-Token`.
    pub fn validate_route_auth_for_surfaces(
        &self,
        profile: &WebRequestContextProfile,
    ) -> Result<(), String> {
        for route in self.routes.iter() {
            route.validate_compatibility_contract()?;
            route.validate_log_retention()?;
            let surface = classify_api_surface(route.path, profile);
            match surface {
                WebApiSurface::AppApi => {
                    if route.auth.is_anonymous() || route.auth == RouteAuth::RefreshToken {
                        continue;
                    }
                    if !route.auth.requires_dual_token_headers()
                        && !route.auth.requires_access_token_only()
                    {
                        return Err(format!(
                            "app-api route {} {} must declare RouteAuth::Public, RouteAuth::RefreshToken, or an Access-Token-bearing auth profile: RouteAuth::DualToken or RouteAuth::CredentialEntryBootstrap (found {})",
                            http_method_label(route.method),
                            route.path,
                            route_auth_label(route.auth),
                        ));
                    }
                }
                WebApiSurface::GatewayApi => {
                    if !route.auth.requires_dual_token_headers()
                        && !route.auth.requires_access_token_only()
                    {
                        return Err(format!(
                            "gateway-api route {} {} must declare an Access-Token-bearing auth profile (found {})",
                            http_method_label(route.method),
                            route.path,
                            route_auth_label(route.auth),
                        ));
                    }
                }
                WebApiSurface::BackendApi => {
                    // Backend-api permits body/access-token entry profiles and agent bootstrap.
                    // Explicit RouteAuth::Public is allowed for provider webhook
                    // callbacks whose handler owns signature verification; the
                    // manifest author declares it deliberately.
                    if !route.auth.is_anonymous()
                        && !route.auth.requires_dual_token_headers()
                        && !route.auth.requires_access_token_only()
                        && !route.auth.is_bootstrap_body_credential_mode()
                        && !route.auth.is_agent_token_credential_mode()
                    {
                        return Err(format!(
                            "backend-api route {} {} must declare a protected, bootstrap-body, or explicitly public auth profile (found {})",
                            http_method_label(route.method),
                            route.path,
                            route_auth_label(route.auth),
                        ));
                    }
                }
                WebApiSurface::InternalApi => {
                    if !route.auth.is_ingress_token_credential_mode() {
                        return Err(format!(
                            "internal-api route {} {} must declare RouteAuth::IngressToken (found {})",
                            http_method_label(route.method),
                            route.path,
                            route_auth_label(route.auth),
                        ));
                    }
                }
                WebApiSurface::OpenApi => {
                    if route.auth.is_anonymous() {
                        continue;
                    }
                    if route.auth == RouteAuth::Compatibility {
                        continue;
                    }
                    if route.auth.requires_dual_token_headers()
                        || route.auth == RouteAuth::RefreshToken
                    {
                        return Err(format!(
                            "open-api route {} {} must not use {} auth; declare api-key, oauth, open-api-flexible, or api-key-or-dual-token",
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
        for route in self.routes.iter() {
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
        RouteAuth::BootstrapBody => "bootstrapBody",
        RouteAuth::CredentialEntryBootstrap => "credentialEntryBootstrap",
        RouteAuth::RefreshToken => "refresh-token",
        RouteAuth::DualToken => "dualToken",
        RouteAuth::ApiKey => "apiKey",
        RouteAuth::IngressToken => "ingressToken",
        RouteAuth::OAuth => "oauth",
        RouteAuth::OpenApiFlexible => "openApiFlexible",
        RouteAuth::OpenApiBearerFlexible => "openApiBearerFlexible",
        RouteAuth::ApiKeyOrDualToken => "apiKeyOrDualToken",
        RouteAuth::AgentToken => "agentToken",
        RouteAuth::Compatibility => "compatibility",
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
    fn owns_runtime_composed_route_inventory() {
        let manifest = HttpRouteManifest::from_owned_routes(ROUTES.to_vec());
        let shared = manifest.clone();

        assert_eq!(shared.routes(), ROUTES);
        assert_eq!(
            shared
                .match_route("POST", "/app/v3/api/auth/sessions")
                .map(|route| route.operation_id),
            Some("createSession")
        );
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
    fn accepts_app_api_public_and_non_open_api_access_token_profiles() {
        use crate::request_context::WebRequestContextProfile;

        const ROUTES: &[HttpRoute] = &[
            HttpRoute::public(
                HttpMethod::Get,
                "/app/v3/api/system/runtime",
                "system",
                "runtime.retrieve",
            ),
            HttpRoute::credential_entry_bootstrap(
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
            .expect("app-api public, access-token-only, and dual-token routes are valid");
    }

    #[test]
    fn accepts_explicit_backend_api_public_webhook_routes() {
        use crate::request_context::WebRequestContextProfile;

        // Provider webhook callbacks are deliberately public; the handler owns
        // signature verification (see validate_route_auth_for_surfaces).
        const ROUTES: &[HttpRoute] = &[HttpRoute::public(
            HttpMethod::Post,
            "/backend/v3/api/rtc/provider_webhooks/{provider}/events",
            "rtc",
            "rtc.providerWebhooks.receive",
        )];
        HttpRouteManifest::new(ROUTES)
            .validate_route_auth_for_surfaces(&WebRequestContextProfile::default())
            .expect("explicitly public backend-api webhook routes are valid");
    }

    #[test]
    fn accepts_backend_api_bootstrap_body_routes() {
        use crate::request_context::WebRequestContextProfile;

        const ROUTES: &[HttpRoute] = &[HttpRoute::bootstrap_body(
            HttpMethod::Post,
            "/backend/v3/api/iam/access_credentials",
            "iam",
            "accessCredentials.create",
        )];
        let manifest = HttpRouteManifest::new(ROUTES);
        manifest
            .validate_route_auth_for_surfaces(&WebRequestContextProfile::default())
            .expect("backend-api bootstrap-body route should be valid");
        assert!(!manifest.is_public_route("POST", "/backend/v3/api/iam/access_credentials"));
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
    fn internal_api_requires_ingress_token_auth() {
        use crate::request_context::WebRequestContextProfile;

        const VALID: &[HttpRoute] = &[HttpRoute::ingress_token(
            HttpMethod::Get,
            "/internal/v3/api/drive/resources/{resourceId}",
            "drive",
            "driveResources.retrieve",
        )];
        HttpRouteManifest::new(VALID)
            .validate_route_auth_for_surfaces(&WebRequestContextProfile::default())
            .expect("internal-api ingress-token route should be valid");

        const INVALID: &[HttpRoute] = &[HttpRoute::api_key(
            HttpMethod::Get,
            "/internal/v3/api/drive/resources/{resourceId}",
            "drive",
            "driveResources.retrieve",
        )];
        let error = HttpRouteManifest::new(INVALID)
            .validate_route_auth_for_surfaces(&WebRequestContextProfile::default())
            .expect_err("internal-api api-key route must not bypass ingress-token semantics");
        assert!(error.contains("RouteAuth::IngressToken"));
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
