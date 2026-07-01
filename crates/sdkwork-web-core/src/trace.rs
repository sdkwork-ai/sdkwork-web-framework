//! W3C trace context propagation (catalog E8 lite — no full OTel dependency).

use crate::extractors::header_value;
use crate::hashing::hash_key_material;
use axum::http::HeaderMap;

pub const TRACEPARENT_HEADER: &str = "traceparent";
pub const TRACESTATE_HEADER: &str = "tracestate";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceContext {
    pub traceparent: String,
    pub tracestate: Option<String>,
}

/// Parse incoming `traceparent` or generate a new root trace id for outbound correlation.
pub fn resolve_trace_context(headers: &HeaderMap, request_id: &str) -> TraceContext {
    if let Some(traceparent) = header_value(headers, TRACEPARENT_HEADER) {
        if is_valid_traceparent(&traceparent) {
            return TraceContext {
                traceparent,
                tracestate: header_value(headers, TRACESTATE_HEADER),
            };
        }
    }
    TraceContext {
        traceparent: synthetic_traceparent(request_id),
        tracestate: None,
    }
}

/// W3C trace id (32 hex chars) from a `traceparent` header value.
pub fn trace_id_from_traceparent(traceparent: &str) -> Option<&str> {
    let mut parts = traceparent.split('-');
    let version = parts.next()?;
    if version != "00" {
        return None;
    }
    let trace_id = parts.next()?;
    if trace_id.len() == 32 && trace_id.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(trace_id)
    } else {
        None
    }
}

fn is_valid_traceparent(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    parts.len() == 4
        && parts[0].len() == 2
        && parts[1].len() == 32
        && parts[2].len() == 16
        && parts[3].len() == 2
        && parts[1].chars().all(|c| c.is_ascii_hexdigit())
        && parts[2].chars().all(|c| c.is_ascii_hexdigit())
}

/// Resolve the trace id exposed on Problem+json responses.
pub fn resolve_problem_trace_id(request_id: &str, trace_id: Option<&str>) -> String {
    if let Some(trace_id) = trace_id.map(str::trim).filter(|value| !value.is_empty()) {
        return trace_id.to_owned();
    }
    trace_id_from_traceparent(&synthetic_traceparent(request_id))
        .map(str::to_owned)
        .unwrap_or_else(|| {
            request_id
                .chars()
                .filter(|c| c.is_ascii_hexdigit())
                .collect()
        })
}

fn synthetic_traceparent(request_id: &str) -> String {
    let trace_id = pad_hex(
        &request_id
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect::<String>(),
        32,
    );
    let span_id = pad_hex(&hash_key_material(&format!("span:{request_id}")), 16);
    format!("00-{trace_id}-{span_id}-01")
}

fn pad_hex(value: &str, len: usize) -> String {
    let mut out: String = value.chars().take(len).collect();
    while out.len() < len {
        out.push('0');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn accepts_valid_traceparent() {
        let mut headers = HeaderMap::new();
        headers.insert(
            TRACEPARENT_HEADER,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
                .parse()
                .unwrap(),
        );
        let ctx = resolve_trace_context(&headers, "req-1");
        assert_eq!(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            ctx.traceparent
        );
    }

    #[test]
    fn synthesizes_traceparent_from_request_id() {
        let ctx = resolve_trace_context(&HeaderMap::new(), "req-abc");
        assert!(ctx.traceparent.starts_with("00-"));
    }

    #[test]
    fn extracts_trace_id_from_traceparent() {
        assert_eq!(
            Some("4bf92f3577b34da6a3ce929d0e0e4736"),
            trace_id_from_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
        );
    }
}
