//! Rejects client-supplied tenant/app/subject context selectors (catalog B12 / API_SPEC §10.0).

use crate::constants::{
    FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS, FORBIDDEN_CLIENT_CONTEXT_QUERY_KEYS,
    IAM_CANONICAL_CONTEXT_RESOURCE_PREFIXES,
};
use crate::error::WebFrameworkError;
use crate::idempotency::content_length_from_headers;
use crate::request_context::WebApiSurface;
use axum::body::{to_bytes, Body};
use axum::extract::Request;
use axum::http::Method;

fn normalize_selector_key(key: &str) -> String {
    key.chars()
        .filter(|ch| *ch != '_' && *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn forbidden_query_keys() -> &'static [String] {
    use std::sync::OnceLock;
    static KEYS: OnceLock<Vec<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        FORBIDDEN_CLIENT_CONTEXT_QUERY_KEYS
            .iter()
            .map(|key| normalize_selector_key(key))
            .collect()
    })
}

/// Returns `true` when a path/query/body key selects ambient tenant/app/subject context.
pub fn is_forbidden_context_selector_key(key: &str) -> bool {
    let normalized = normalize_selector_key(key);
    forbidden_query_keys()
        .iter()
        .any(|candidate| candidate == &normalized)
}

/// Returns `true` when the API surface participates in SaaS tenant-context rules.
pub fn requires_client_context_selector_guard(api_surface: WebApiSurface) -> bool {
    matches!(
        api_surface,
        WebApiSurface::AppApi | WebApiSurface::OpenApi | WebApiSurface::GatewayApi
    )
}

pub fn reject_forbidden_context_query(query: Option<&str>) -> Result<(), WebFrameworkError> {
    let Some(query) = query else {
        return Ok(());
    };
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let key = pair.split_once('=').map(|(key, _)| key).unwrap_or(pair);
        if is_forbidden_context_selector_key(key.trim()) {
            return Err(WebFrameworkError::bad_request(format!(
                "client must not supply context selector query parameter `{key}`"
            )));
        }
    }
    Ok(())
}

/// Rejects top-level JSON object keys that select ambient tenant/app/subject context (API_SPEC §14).
pub fn reject_forbidden_context_body_json(body: &[u8]) -> Result<(), WebFrameworkError> {
    if body.is_empty() || body.iter().all(u8::is_ascii_whitespace) {
        return Ok(());
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Ok(());
    };
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    for key in object.keys() {
        if is_forbidden_context_selector_key(key) {
            return Err(WebFrameworkError::bad_request(format!(
                "client must not supply context selector body field `{key}`"
            )));
        }
    }
    Ok(())
}

fn request_has_json_payload(request: &Request) -> bool {
    if !matches!(
        request.method(),
        &Method::POST | &Method::PUT | &Method::PATCH
    ) {
        return false;
    }
    let has_body = content_length_from_headers(request.headers()).unwrap_or(0) > 0
        || request.headers().contains_key("transfer-encoding");
    if !has_body {
        return false;
    }
    let content_type = request
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    mime == "application/json"
}

/// Buffers JSON request bodies on guarded surfaces and rejects client context selector fields.
pub async fn inspect_json_body_context_selectors(
    request: &mut Request,
    max_bytes: u64,
    api_surface: WebApiSurface,
) -> Result<(), WebFrameworkError> {
    if !requires_client_context_selector_guard(api_surface) || !request_has_json_payload(request) {
        return Ok(());
    }
    let limit = max_bytes.max(1) as usize;
    let (parts, body) = std::mem::replace(request, Request::new(Body::empty())).into_parts();
    let bytes = to_bytes(body, limit.saturating_add(1))
        .await
        .map_err(|error| {
            WebFrameworkError::bad_request(format!("failed to read request body: {error}"))
        })?;
    if bytes.len() > limit {
        return Err(WebFrameworkError::payload_too_large(format!(
            "request body exceeds {limit} byte limit"
        )));
    }
    reject_forbidden_context_body_json(&bytes)?;
    *request = Request::from_parts(parts, Body::from(bytes));
    Ok(())
}

/// Returns `true` when the path addresses canonical IAM tenant/org resources (API_SPEC §11.3).
pub fn is_canonical_iam_context_resource_path(path: &str) -> bool {
    let lowered = path.split('?').next().unwrap_or(path).to_ascii_lowercase();
    IAM_CANONICAL_CONTEXT_RESOURCE_PREFIXES
        .iter()
        .any(|prefix| lowered.contains(prefix))
}

pub fn reject_forbidden_ambient_context_path(path: &str) -> Result<(), WebFrameworkError> {
    if is_canonical_iam_context_resource_path(path) {
        return Ok(());
    }
    let lowered = path.to_ascii_lowercase();
    for marker in FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS {
        if lowered.contains(marker) {
            return Err(WebFrameworkError::bad_request(format!(
                "API paths must not use ambient tenant/org scoping marker `{marker}`"
            )));
        }
    }
    Ok(())
}

pub fn reject_client_context_selectors(
    path: &str,
    query: Option<&str>,
    api_surface: WebApiSurface,
) -> Result<(), WebFrameworkError> {
    if !requires_client_context_selector_guard(api_surface) {
        return Ok(());
    }
    reject_forbidden_ambient_context_path(path)?;
    reject_forbidden_context_query(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_tenant_id_query_on_app_api_surface() {
        let error = reject_client_context_selectors(
            "/app/v3/api/orders",
            Some("tenant_id=100001"),
            WebApiSurface::AppApi,
        )
        .expect_err("tenant selector");
        assert!(error.message.contains("tenant_id"));
    }

    #[test]
    fn allows_tenant_id_query_on_backend_api_surface() {
        reject_client_context_selectors(
            "/backend/v3/api/web-framework/cors_policies",
            Some("tenant_id=100001"),
            WebApiSurface::BackendApi,
        )
        .expect("platform admin filter");
    }

    #[test]
    fn rejects_ambient_tenant_path_on_open_api_surface() {
        let error = reject_client_context_selectors(
            "/im/v3/api/tenants/t1/orders",
            None,
            WebApiSurface::OpenApi,
        )
        .expect_err("ambient tenant path");
        assert!(error.message.contains("/tenants/"));
    }

    #[test]
    fn allows_canonical_iam_organization_tree_on_app_api_surface() {
        reject_client_context_selectors(
            "/app/v3/api/iam/organizations/tree",
            None,
            WebApiSurface::AppApi,
        )
        .expect("canonical IAM organization tree path");
    }

    #[test]
    fn rejects_tenant_id_body_field_on_app_api_surface() {
        let error = reject_forbidden_context_body_json(br#"{"tenantId":"100001","name":"x"}"#)
            .expect_err("tenant body selector");
        assert!(error.message.contains("tenantId"));
    }

    #[tokio::test]
    async fn skips_body_inspection_on_backend_api_surface() {
        let mut request = Request::builder()
            .method("POST")
            .uri("/backend/v3/api/web-framework/cors_policies")
            .header("content-type", "application/json")
            .header("content-length", "25")
            .body(Body::from(r#"{"tenantId":"100001"}"#))
            .expect("request");
        inspect_json_body_context_selectors(&mut request, 1024, WebApiSurface::BackendApi)
            .await
            .expect("backend platform routes may target tenant resources");
    }
}
