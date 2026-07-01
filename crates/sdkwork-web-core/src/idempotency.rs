//! Idempotency fingerprinting and replay response helpers (catalog D5–D7).

use crate::error::WebFrameworkError;
use crate::extractors::header_value;
use crate::hashing::hash_key_material;
use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use serde::{Deserialize, Serialize};

/// Cached HTTP response for idempotent command replay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IdempotencyResponseRecord {
    pub status_code: u16,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
}

/// Result of reserving an idempotency key before handler execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdempotencyBeginOutcome {
    /// First request with this key — execute handler and cache the response.
    Leader,
    /// Prior completed request — replay cached response without running the handler.
    Replay(IdempotencyResponseRecord),
}

pub fn content_length_from_headers(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

/// Resolve idempotency fingerprint — prefers explicit body hash headers (D6).
///
/// 安全契约（API_SPEC §17 / SECURITY_SPEC §5.1）：
/// - 有 body 时必须携带 `X-Content-SHA256` 或 `X-Idempotency-Fingerprint`，否则拒绝。
/// - 无 body 时用 method+path+operation_id 稳定指纹。
/// - 禁止用 `content_length` 做指纹（同长度不同 body 会碰撞，导致复用错误响应）。
pub fn resolve_idempotency_fingerprint(
    method: &str,
    path: &str,
    operation_id: Option<&str>,
    headers: &HeaderMap,
    require_body_hash_when_payload: bool,
) -> Result<String, WebFrameworkError> {
    if let Some(explicit) = header_value(headers, crate::constants::IDEMPOTENCY_FINGERPRINT_HEADER)
        .or_else(|| header_value(headers, crate::constants::CONTENT_SHA256_HEADER))
    {
        return Ok(hash_key_material(&format!(
            "{method}:{path}:body:{explicit}"
        )));
    }

    let content_length = content_length_from_headers(headers);
    let has_chunked_body = headers
        .get(header::TRANSFER_ENCODING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"));
    let has_payload = content_length.unwrap_or(0) > 0 || has_chunked_body;

    if require_body_hash_when_payload && has_payload {
        return Err(WebFrameworkError::bad_request(
            "requests with a body must include X-Content-SHA256 or X-Idempotency-Fingerprint for idempotent commands",
        ));
    }

    // 无 body 或未强制要求 body hash 时，用 method+path+operation_id 稳定指纹。
    // 不再使用 content_length —— 同长度不同 body 会碰撞导致复用错误响应。
    Ok(idempotency_fingerprint(method, path, operation_id))
}

/// Stable fingerprint for method + path + operation (no content_length — avoids collision).
pub fn idempotency_fingerprint(method: &str, path: &str, operation_id: Option<&str>) -> String {
    hash_key_material(&format!(
        "{method}:{path}:op={}",
        operation_id.unwrap_or("")
    ))
}

/// Build a replay response from a cached idempotency record.
pub fn idempotency_replay_response(
    record: &IdempotencyResponseRecord,
    request_id: Option<&str>,
) -> Result<Response, WebFrameworkError> {
    let status = StatusCode::from_u16(record.status_code).map_err(|_| {
        WebFrameworkError::dependency_unavailable("cached idempotency response has invalid status")
    })?;
    let mut response = Response::new(Body::from(record.body.clone()));
    *response.status_mut() = status;
    if let Some(content_type) = &record.content_type {
        if let Ok(value) = HeaderValue::from_str(content_type) {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
    }
    if let Some(request_id) = request_id {
        if let Ok(value) = HeaderValue::from_str(request_id) {
            response
                .headers_mut()
                .insert(crate::constants::REQUEST_ID_HEADER, value);
        }
    }

    // Idempotent-Replayed header (Stripe-compatible signal).
    response.headers_mut().insert(
        axum::http::HeaderName::from_static("idempotent-replayed"),
        HeaderValue::from_static("true"),
    );

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn fingerprint_stable_without_content_length() {
        // 不再使用 content_length —— 同 method+path+op 产生相同指纹。
        let a = idempotency_fingerprint("POST", "/app/v3/api/orders", None);
        let b = idempotency_fingerprint("POST", "/app/v3/api/orders", None);
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_changes_with_operation_id() {
        let a = idempotency_fingerprint("POST", "/app/v3/api/orders", Some("orders.create"));
        let b = idempotency_fingerprint("POST", "/app/v3/api/orders", Some("orders.update"));
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_changes_with_body_hash() {
        // 有 body hash 时指纹区分不同 body。
        let mut headers_a = HeaderMap::new();
        headers_a.insert(
            crate::constants::CONTENT_SHA256_HEADER,
            "hash_a".parse().unwrap(),
        );
        let a =
            resolve_idempotency_fingerprint("POST", "/app/v3/api/orders", None, &headers_a, true)
                .expect("fingerprint a");
        let mut headers_b = HeaderMap::new();
        headers_b.insert(
            crate::constants::CONTENT_SHA256_HEADER,
            "hash_b".parse().unwrap(),
        );
        let b =
            resolve_idempotency_fingerprint("POST", "/app/v3/api/orders", None, &headers_b, true)
                .expect("fingerprint b");
        assert_ne!(a, b);
    }

    #[test]
    fn requires_body_hash_when_payload_present() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, "128".parse().unwrap());
        let error =
            resolve_idempotency_fingerprint("POST", "/app/v3/api/orders", None, &headers, true)
                .expect_err("missing body hash");
        assert_eq!(crate::error::WebFrameworkErrorKind::BadRequest, error.kind);
    }

    #[test]
    fn explicit_body_hash_is_used() {
        let mut headers = HeaderMap::new();
        headers.insert(
            crate::constants::CONTENT_SHA256_HEADER,
            "abc123".parse().unwrap(),
        );
        let fp =
            resolve_idempotency_fingerprint("POST", "/app/v3/api/orders", None, &headers, true)
                .expect("fingerprint");
        assert!(fp.len() >= 16);
    }

    #[test]
    fn requires_body_hash_for_chunked_transfer_encoding() {
        let mut headers = HeaderMap::new();
        headers.insert(header::TRANSFER_ENCODING, "chunked".parse().unwrap());
        let error =
            resolve_idempotency_fingerprint("POST", "/app/v3/api/orders", None, &headers, true)
                .expect_err("missing body hash");
        assert_eq!(crate::error::WebFrameworkErrorKind::BadRequest, error.kind);
    }
}
