use crate::request_context::{WebApiSurface, WebRequestContextProfile};
use crate::route_manifest::HttpRouteManifest;

pub fn classify_api_surface(path: &str, profile: &WebRequestContextProfile) -> WebApiSurface {
    let normalized = normalize_path(path);
    if matches_prefix(&normalized, &profile.app_api_prefix) {
        return WebApiSurface::AppApi;
    }
    if matches_prefix(&normalized, &profile.backend_api_prefix) {
        return WebApiSurface::BackendApi;
    }
    if profile
        .gateway_api_prefixes
        .iter()
        .any(|prefix| matches_prefix(&normalized, prefix))
    {
        return WebApiSurface::GatewayApi;
    }
    if profile
        .open_api_prefixes
        .iter()
        .any(|prefix| matches_prefix(&normalized, prefix))
    {
        return WebApiSurface::OpenApi;
    }
    WebApiSurface::Unknown
}

pub(crate) fn is_public_path(path: &str, profile: &WebRequestContextProfile) -> bool {
    let normalized = normalize_path(path);
    profile
        .public_path_prefixes
        .iter()
        .any(|prefix| matches_prefix(&normalized, prefix))
}

/// Resolves whether a request skips credential resolution and auth stages.
///
/// Manifest entries take precedence: an exact `method + path` match uses `RouteAuth`.
/// Unmatched paths fall back to infra [`WebRequestContextProfile::public_path_prefixes`].
pub fn resolve_public_path(
    method: &str,
    path: &str,
    profile: &WebRequestContextProfile,
    manifest: Option<HttpRouteManifest>,
) -> bool {
    if let Some(manifest) = manifest {
        if let Some(route) = manifest.match_route(method, path) {
            return route.auth.skips_credential_resolution();
        }
    }
    is_public_path(path, profile)
}

pub fn matches_prefix(path: &str, prefix: &str) -> bool {
    let normalized_prefix = normalize_path(prefix);
    path == normalized_prefix || path.starts_with(&format!("{normalized_prefix}/"))
}

pub(crate) fn normalize_path(path: &str) -> String {
    let value = path.trim();
    if value.is_empty() {
        return "/".to_owned();
    }
    format!("/{}", value.trim_matches('/'))
}

/// Prometheus label for [`WebApiSurface`] (catalog E4 legacy camelCase).
pub fn api_surface_metric_label(surface: &crate::request_context::WebApiSurface) -> &'static str {
    use crate::request_context::WebApiSurface;
    match surface {
        WebApiSurface::OpenApi => "openApi",
        WebApiSurface::AppApi => "appApi",
        WebApiSurface::BackendApi => "backendApi",
        WebApiSurface::GatewayApi => "gatewayApi",
        WebApiSurface::Unknown => "unknown",
    }
}

/// Contract kebab-case API surface label (`OBSERVABILITY_SPEC.md` §3).
pub fn api_surface_contract_label(surface: &crate::request_context::WebApiSurface) -> &'static str {
    use crate::request_context::WebApiSurface;
    match surface {
        WebApiSurface::OpenApi => "open-api",
        WebApiSurface::AppApi => "app-api",
        WebApiSurface::BackendApi => "backend-api",
        WebApiSurface::GatewayApi => "gateway-api",
        WebApiSurface::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_context::WebRequestContextProfile;
    use crate::route_manifest::HttpRouteManifest;
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    #[test]
    fn resolve_public_path_prefers_manifest_over_missing_prefix() {
        const ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Post,
            "/app/v3/api/auth/sessions",
            "Auth",
            "sessions.create",
            RouteAuth::Public,
        )];
        let profile = WebRequestContextProfile {
            public_path_prefixes: vec![],
            ..Default::default()
        };
        assert!(resolve_public_path(
            "POST",
            "/app/v3/api/auth/sessions",
            &profile,
            Some(HttpRouteManifest::new(ROUTES)),
        ));
    }

    #[test]
    fn resolve_public_path_treats_refresh_token_routes_as_credential_optional() {
        const ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Post,
            "/app/v3/api/auth/sessions/refresh",
            "auth",
            "sessions.refresh",
            RouteAuth::RefreshToken,
        )];
        let profile = WebRequestContextProfile::default();
        assert!(resolve_public_path(
            "POST",
            "/app/v3/api/auth/sessions/refresh",
            &profile,
            Some(HttpRouteManifest::new(ROUTES)),
        ));
    }

    #[test]
    fn resolve_public_path_manifest_protected_overrides_broad_prefix() {
        const ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/app/v3/api/users/me",
            "Users",
            "users.me",
            RouteAuth::DualToken,
        )];
        let profile = WebRequestContextProfile {
            public_path_prefixes: vec!["/app/v3/api".to_owned()],
            ..Default::default()
        };
        assert!(!resolve_public_path(
            "GET",
            "/app/v3/api/users/me",
            &profile,
            Some(HttpRouteManifest::new(ROUTES)),
        ));
    }
}
