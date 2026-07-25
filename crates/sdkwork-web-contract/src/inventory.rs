use crate::openapi::{
    infer_api_surface_from_path, OPENAPI_API_SURFACE_EXTENSION, OPENAPI_AUTH_MODE_EXTENSION,
};
use crate::{ApiSurface, HttpMethod, HttpRoute};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Canonical comparison row shared by executable routers, manifests, served OpenAPI, and SDK
/// authorities.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiRouteInventoryEntry {
    pub surface: String,
    pub method: String,
    pub normalized_path: String,
    pub operation_id: String,
    pub auth_profile: String,
}

impl ApiRouteInventoryEntry {
    pub fn from_route(route: &HttpRoute) -> Self {
        Self {
            surface: api_surface_label(infer_api_surface_from_path(route.path)).to_owned(),
            method: http_method_label(route.method).to_owned(),
            normalized_path: normalize_route_path(route.path),
            operation_id: route.operation_id.to_owned(),
            auth_profile: route.auth.auth_profile_label().to_owned(),
        }
    }
}

pub fn route_inventory_from_routes(routes: &[HttpRoute]) -> Vec<ApiRouteInventoryEntry> {
    let mut inventory = routes
        .iter()
        .map(ApiRouteInventoryEntry::from_route)
        .collect::<Vec<_>>();
    inventory.sort();
    inventory
}

pub fn route_inventory_from_openapi(
    document: &Value,
) -> Result<Vec<ApiRouteInventoryEntry>, String> {
    let paths = document
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| "OpenAPI document must contain a paths object".to_owned())?;
    let mut inventory = Vec::new();
    for (path, path_item) in paths {
        let path_item = path_item
            .as_object()
            .ok_or_else(|| format!("OpenAPI path {path} must be an object"))?;
        for (method, operation) in path_item {
            let method = method.to_ascii_uppercase();
            if !matches!(method.as_str(), "DELETE" | "GET" | "PATCH" | "POST" | "PUT") {
                continue;
            }
            let operation = operation
                .as_object()
                .ok_or_else(|| format!("OpenAPI operation {method} {path} must be an object"))?;
            let operation_id = operation
                .get("operationId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| format!("OpenAPI operation {method} {path} lacks operationId"))?;
            let surface = operation
                .get(OPENAPI_API_SURFACE_EXTENSION)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "OpenAPI operation {method} {path} lacks {OPENAPI_API_SURFACE_EXTENSION}"
                    )
                })?;
            let auth_profile = operation
                .get(OPENAPI_AUTH_MODE_EXTENSION)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!("OpenAPI operation {method} {path} lacks {OPENAPI_AUTH_MODE_EXTENSION}")
                })?;
            inventory.push(ApiRouteInventoryEntry {
                surface: surface.to_owned(),
                method,
                normalized_path: normalize_route_path(path),
                operation_id: operation_id.to_owned(),
                auth_profile: auth_profile.to_owned(),
            });
        }
    }
    inventory.sort();
    Ok(inventory)
}

/// Normalizes route syntax without erasing path-parameter names needed by generated SDK methods.
pub fn normalize_route_path(path: &str) -> String {
    let normalized = format!("/{}", path.trim().trim_matches('/'));
    let segments = normalized
        .split('/')
        .map(|segment| {
            if let Some(parameter) = segment.strip_prefix(':') {
                format!("{{{parameter}}}")
            } else {
                segment.to_owned()
            }
        })
        .collect::<Vec<_>>();
    let normalized = segments.join("/");
    if normalized.is_empty() {
        "/".to_owned()
    } else {
        normalized
    }
}

fn api_surface_label(surface: ApiSurface) -> &'static str {
    match surface {
        ApiSurface::OpenApi => "open-api",
        ApiSurface::AppApi => "app-api",
        ApiSurface::BackendApi => "backend-api",
        ApiSurface::InternalApi => "internal-api",
        ApiSurface::GatewayApi => "gateway-api",
        ApiSurface::Unknown => "unknown",
    }
}

fn http_method_label(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Delete => "DELETE",
        HttpMethod::Get => "GET",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_openapi_document, RouteAuth};

    #[test]
    fn manifest_and_openapi_inventories_match() {
        let routes = [
            HttpRoute::credential_entry_bootstrap(
                HttpMethod::Post,
                "/app/v3/api/auth/sessions",
                "Auth",
                "sessions.create",
            ),
            HttpRoute::new(
                HttpMethod::Get,
                "/app/v3/api/users/:userId",
                "Users",
                "users.retrieve",
                RouteAuth::DualToken,
            ),
        ];
        let document = build_openapi_document("inventory", &routes);
        assert_eq!(
            route_inventory_from_routes(&routes),
            route_inventory_from_openapi(&document).expect("OpenAPI inventory")
        );
    }

    #[test]
    fn path_normalization_preserves_parameter_name() {
        assert_eq!(
            "/app/v3/api/users/{userId}",
            normalize_route_path("app/v3/api/users/:userId/")
        );
    }
}
