use crate::constants::{
    ACCESS_TOKEN_HEADER, AGENT_TOKEN_HEADER, API_KEY_HEADER, AUTHORIZATION_HEADER,
};
use axum::http::HeaderMap;

pub fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    header_value(headers, crate::constants::IDEMPOTENCY_KEY_HEADER)
        .or_else(|| header_value(headers, crate::constants::X_IDEMPOTENCY_KEY_HEADER))
}

pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let raw = header_value(headers, AUTHORIZATION_HEADER)?;
    raw.strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn api_key(headers: &HeaderMap) -> Option<String> {
    header_value(headers, API_KEY_HEADER)
}

/// Extracts the backend agent bootstrap token (`X-SDKWork-Agent-Token`) for `RouteAuth::AgentToken` routes (C8-C9).
pub fn agent_token(headers: &HeaderMap) -> Option<String> {
    header_value(headers, AGENT_TOKEN_HEADER)
}

pub fn access_token(headers: &HeaderMap) -> Option<String> {
    header_value(headers, ACCESS_TOKEN_HEADER)
}

pub fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
