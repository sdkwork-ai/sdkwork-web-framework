//! RFC 7807 Problem+json golden snapshots (catalog K5 / G6).

use sdkwork_web_core::{problem_response, ProblemCorrelation, WebFrameworkError};

fn render_problem(error: WebFrameworkError, request_id: Option<&str>) -> serde_json::Value {
    let correlation = ProblemCorrelation::new(request_id, None);
    let response = problem_response(&error, correlation);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let bytes = rt
        .block_on(async { axum::body::to_bytes(response.into_body(), usize::MAX).await })
        .expect("body");
    serde_json::from_slice(&bytes).expect("json")
}

#[test]
fn problem_json_snapshot_missing_credentials() {
    let error = WebFrameworkError::missing_credentials("Access-Token header is required");
    let payload = render_problem(error, Some("req-snapshot-1"));
    assert_eq!(
        "https://sdkwork.dev/problems/missing-credentials",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(401, payload["status"].as_u64().unwrap());
    assert!(payload.get("requestId").is_none());
    assert!(payload["traceId"].as_str().is_some());
    let rendered = payload.to_string();
    assert!(!rendered.contains("backtrace"));
    assert!(!rendered.contains("stack trace"));
}

#[test]
fn problem_json_snapshot_forbidden() {
    let error = WebFrameworkError::forbidden("missing required permission");
    let payload = render_problem(error, None);
    assert_eq!(403, payload["status"].as_u64().unwrap());
    assert!(payload["detail"]
        .as_str()
        .unwrap()
        .contains("missing required permission"));
}

#[test]
fn problem_json_snapshot_not_found() {
    let error = WebFrameworkError::not_found("control node missing");
    let payload = render_problem(error, Some("req-snapshot-404"));
    assert_eq!(
        "https://sdkwork.dev/problems/not-found",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(404, payload["status"].as_u64().unwrap());
    assert!(payload.get("requestId").is_none());
    assert!(payload["traceId"].as_str().is_some());
}

#[test]
fn problem_json_snapshot_not_implemented() {
    let error = WebFrameworkError::not_implemented("handler is not mounted");
    let payload = render_problem(error, Some("req-snapshot-501"));
    assert_eq!(
        "https://sdkwork.dev/problems/not-implemented",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(501, payload["status"].as_u64().unwrap());
    assert!(payload.get("requestId").is_none());
    assert!(payload["traceId"].as_str().is_some());
}

#[test]
fn problem_json_snapshot_internal_server_error() {
    let error = WebFrameworkError::internal_server_error("unexpected failure");
    let payload = render_problem(error, None);
    assert_eq!(
        "https://sdkwork.dev/problems/internal-server-error",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(500, payload["status"].as_u64().unwrap());
}

#[test]
fn problem_json_snapshot_dependency_unavailable() {
    let error = WebFrameworkError::dependency_unavailable("database operation failed");
    let payload = render_problem(error, Some("req-snapshot-503"));
    assert_eq!(
        "https://sdkwork.dev/problems/dependency-unavailable",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(503, payload["status"].as_u64().unwrap());
}

#[test]
fn problem_json_snapshot_conflict() {
    let error = WebFrameworkError::conflict("idempotency fingerprint mismatch");
    let payload = render_problem(error, Some("req-snapshot-409"));
    assert_eq!(
        "https://sdkwork.dev/problems/conflict",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(409, payload["status"].as_u64().unwrap());
}

#[test]
fn problem_json_snapshot_payload_too_large() {
    let error = WebFrameworkError::payload_too_large("request body exceeds limit");
    let payload = render_problem(error, Some("req-snapshot-413"));
    assert_eq!(
        "https://sdkwork.dev/problems/payload-too-large",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(413, payload["status"].as_u64().unwrap());
    assert!(payload.get("requestId").is_none());
    assert!(payload["traceId"].as_str().is_some());
}

#[test]
fn problem_json_snapshot_invalid_credentials() {
    let error = WebFrameworkError::invalid_credentials("token signature invalid");
    let payload = render_problem(error, None);
    assert_eq!(
        "https://sdkwork.dev/problems/invalid-credentials",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(401, payload["status"].as_u64().unwrap());
}

#[test]
fn problem_json_snapshot_bad_request() {
    let error = WebFrameworkError::bad_request("environment must not be empty");
    let payload = render_problem(error, None);
    assert_eq!(
        "https://sdkwork.dev/problems/bad-request",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(400, payload["status"].as_u64().unwrap());
}

#[test]
fn problem_json_snapshot_method_not_allowed() {
    let error = WebFrameworkError::method_not_allowed("PATCH is not supported");
    let payload = render_problem(error, None);
    assert_eq!(
        "https://sdkwork.dev/problems/method-not-allowed",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(405, payload["status"].as_u64().unwrap());
}

#[test]
fn problem_json_snapshot_request_timeout() {
    let error = WebFrameworkError::request_timeout("request exceeded deadline");
    let payload = render_problem(error, Some("req-snapshot-timeout"));
    assert_eq!(
        "https://sdkwork.dev/problems/request-timeout",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(408, payload["status"].as_u64().unwrap());
    assert_eq!(40801, payload["code"].as_i64().unwrap());
}

#[test]
fn problem_json_snapshot_context_not_injected() {
    let error = WebFrameworkError::context_not_injected();
    let payload = render_problem(error, None);
    assert_eq!(
        "https://sdkwork.dev/problems/context-not-injected",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(500, payload["status"].as_u64().unwrap());
}

#[test]
fn problem_json_snapshot_includes_trace_id_when_correlated() {
    let error = WebFrameworkError::forbidden("tenant isolation mismatch");
    let payload = render_problem(error, Some("req-snapshot-trace"));
    let response = problem_response(
        &WebFrameworkError::forbidden("tenant isolation mismatch"),
        ProblemCorrelation::new(
            Some("req-snapshot-trace"),
            Some("4bf92f3577b34da6a3ce929d0e0e4736"),
        ),
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let bytes = rt
        .block_on(async { axum::body::to_bytes(response.into_body(), usize::MAX).await })
        .expect("body");
    let correlated: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        "4bf92f3577b34da6a3ce929d0e0e4736",
        correlated["traceId"].as_str().unwrap()
    );
    assert!(payload.get("requestId").is_none());
    assert!(payload["traceId"].as_str().is_some());
}

#[test]
fn problem_json_snapshot_websocket_rejected() {
    let error = WebFrameworkError::websocket_rejected("upgrade denied");
    let payload = render_problem(error, None);
    assert_eq!(
        "https://sdkwork.dev/problems/websocket-rejected",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(400, payload["status"].as_u64().unwrap());
}

#[test]
fn problem_json_snapshot_rate_limit_includes_retry_after() {
    let error = WebFrameworkError::rate_limit_exceeded("too many requests", 60);
    let response = problem_response(
        &error,
        ProblemCorrelation::new(Some("req-snapshot-429"), None),
    );
    assert_eq!(
        Some("60"),
        response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
    );
    let payload = render_problem(
        WebFrameworkError::rate_limit_exceeded("too many requests", 60),
        Some("req-snapshot-429"),
    );
    assert_eq!(
        "https://sdkwork.dev/problems/rate-limit-exceeded",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(429, payload["status"].as_u64().unwrap());
}
