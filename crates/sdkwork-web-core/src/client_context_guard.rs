//! Rejects client-supplied tenant/app/subject context selectors (catalog B12 / API_SPEC §10.0).

use crate::constants::{
    FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS, FORBIDDEN_CLIENT_CONTEXT_QUERY_KEYS,
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

fn allows_login_context_organization_selector(path: &str, key: &str) -> bool {
    // Organization login selection is bound to and checked against the one-time continuation token.
    let normalized_path = path.split('?').next().unwrap_or(path).to_ascii_lowercase();
    normalized_path == "/app/v3/api/auth/sessions/login_context_selection"
        && normalize_selector_key(key) == "organizationid"
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
        WebApiSurface::AppApi
            | WebApiSurface::OpenApi
            | WebApiSurface::InternalApi
            | WebApiSurface::GatewayApi
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
///
/// The guard judges payload; it must never consume it. Every accepted request
/// keeps the exact bytes it arrived with, and every rejection that happens before
/// the body is taken leaves the body in place, so a caller whose error handling
/// continues can never hand a down-stream service a request that merely *looks*
/// complete while its body is gone. A body that cannot be read within the guard's
/// budget is still a rejection, never a silently emptied request.
pub async fn inspect_json_body_context_selectors(
    request: &mut Request,
    max_bytes: u64,
    api_surface: WebApiSurface,
) -> Result<(), WebFrameworkError> {
    if !requires_client_context_selector_guard(api_surface) || !request_has_json_payload(request) {
        return Ok(());
    }
    let limit = max_bytes.max(1) as usize;
    // Reject a declared oversize before touching the stream: this is the one
    // rejection that can be answered without taking the body out of `request`.
    if let Some(declared) = content_length_from_headers(request.headers()) {
        if declared > limit as u64 {
            return Err(WebFrameworkError::payload_too_large(format!(
                "request body exceeds {limit} byte limit"
            )));
        }
    }
    let (parts, body) = std::mem::replace(request, Request::new(Body::empty())).into_parts();
    let bytes = match to_bytes(body, limit.saturating_add(1)).await {
        Ok(bytes) => bytes,
        // The stream is already consumed, so the request cannot continue as a
        // complete message; the rejection is the only honest outcome.
        Err(error) => {
            return Err(WebFrameworkError::bad_request(format!(
                "failed to read request body: {error}"
            )));
        }
    };
    let over_limit = bytes.len() > limit;
    let path = parts.uri.path();
    let verdict = if over_limit {
        Err(WebFrameworkError::payload_too_large(format!(
            "request body exceeds {limit} byte limit"
        )))
    } else if allows_login_context_organization_selector(path, "organizationId") {
        let value = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
        let forbidden = value
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .and_then(|object| {
                object.keys().find(|key| {
                    is_forbidden_context_selector_key(key)
                        && !allows_login_context_organization_selector(path, key)
                })
            })
            .map(|key| {
                WebFrameworkError::bad_request(format!(
                    "client must not supply context selector body field `{key}`"
                ))
            });
        forbidden.map_or(Ok(()), Err)
    } else {
        reject_forbidden_context_body_json(&bytes)
    };
    // Restore the body *before* reporting the verdict: whether this request is
    // accepted or rejected, it leaves the guard with the payload it arrived with.
    *request = Request::from_parts(parts, Body::from(bytes));
    verdict
}

/// Returns `true` when the path addresses canonical IAM tenant/org resources (API_SPEC §11.3).
pub use sdkwork_web_contract::is_canonical_iam_context_resource_path;

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

    #[tokio::test]
    async fn allows_organization_selector_for_login_context_selection_only() {
        let mut request = Request::builder()
            .method("POST")
            .uri("/app/v3/api/auth/sessions/login_context_selection")
            .header("content-type", "application/json")
            .header("content-length", "98")
            .body(Body::from(
                r#"{"continuationToken":"continuation","loginScope":"ORGANIZATION","organizationId":"200001"}"#,
            ))
            .expect("request");
        inspect_json_body_context_selectors(&mut request, 1024, WebApiSurface::AppApi)
            .await
            .expect("organization selection is validated by the IAM continuation");
    }

    #[tokio::test]
    async fn still_rejects_other_context_selectors_on_login_context_selection() {
        let mut request = Request::builder()
            .method("POST")
            .uri("/app/v3/api/auth/sessions/login_context_selection")
            .header("content-type", "application/json")
            .header("content-length", "94")
            .body(Body::from(
                r#"{"continuationToken":"continuation","loginScope":"ORGANIZATION","tenantId":"100001"}"#,
            ))
            .expect("request");
        let error = inspect_json_body_context_selectors(&mut request, 1024, WebApiSurface::AppApi)
            .await
            .expect_err("tenant selector");
        assert!(error.message.contains("tenantId"));
    }

    /// A prompt-sized payload must come out of the guard byte for byte.
    ///
    /// Regression: the guard used to hand a dependent service an empty body for
    /// payloads beyond the buffer it read, so the service reported a missing
    /// request field (`model is required`) for a body the caller had sent.
    #[tokio::test]
    async fn keeps_a_prompt_sized_body_intact_when_it_inspects_it() {
        let pad = "x".repeat(300 * 1024);
        let payload = format!(r#"{{"model":"deepseek/deepseek-flash","pad":"{pad}"}}"#);
        let mut request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("content-length", payload.len().to_string())
            .body(Body::from(payload.clone()))
            .expect("request");

        inspect_json_body_context_selectors(&mut request, 16 * 1024 * 1024, WebApiSurface::OpenApi)
            .await
            .expect("a prompt body with no context selector is accepted");

        let body = to_bytes(request.into_body(), 16 * 1024 * 1024)
            .await
            .expect("body");
        assert_eq!(body.len(), payload.len());
        assert_eq!(body.as_ref(), payload.as_bytes());
    }

    /// A rejected request must keep its body: the guard judges payload, never
    /// consumes it, so error handling downstream cannot see an empty message.
    #[tokio::test]
    async fn restores_the_body_of_a_rejected_request() {
        let payload = r#"{"tenantId":"100001","name":"x"}"#;
        let mut request = Request::builder()
            .method("POST")
            .uri("/app/v3/api/orders")
            .header("content-type", "application/json")
            .header("content-length", payload.len().to_string())
            .body(Body::from(payload))
            .expect("request");

        let error = inspect_json_body_context_selectors(&mut request, 1024, WebApiSurface::AppApi)
            .await
            .expect_err("tenant selector");
        assert!(error.message.contains("tenantId"));

        let body = to_bytes(request.into_body(), 1024).await.expect("body");
        assert_eq!(body.as_ref(), payload.as_bytes());
    }

    /// An oversized declaration is rejected without touching the stream.
    #[tokio::test]
    async fn leaves_the_body_in_place_when_the_declared_size_exceeds_the_budget() {
        let payload = format!(r#"{{"pad":"{}"}}"#, "x".repeat(4096));
        let mut request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("content-length", payload.len().to_string())
            .body(Body::from(payload.clone()))
            .expect("request");

        let error = inspect_json_body_context_selectors(&mut request, 1024, WebApiSurface::OpenApi)
            .await
            .expect_err("oversized body");
        assert_eq!(error.status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);

        let body = to_bytes(request.into_body(), 8192).await.expect("body");
        assert_eq!(body.as_ref(), payload.as_bytes());
    }
}
