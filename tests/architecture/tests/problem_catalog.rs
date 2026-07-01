//! RFC 7807 problem type URI catalog must cover every framework error kind (catalog G2).

use sdkwork_web_core::{problem_response, ProblemCorrelation, WebFrameworkError};

fn problem_type(error: WebFrameworkError) -> String {
    let response = problem_response(&error, ProblemCorrelation::default());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let bytes = rt
        .block_on(async { axum::body::to_bytes(response.into_body(), usize::MAX).await })
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    payload["type"].as_str().expect("type").to_owned()
}

#[test]
fn problem_type_uris_cover_all_framework_error_kinds() {
    let cases = [
        (
            WebFrameworkError::missing_credentials("missing"),
            401_u16,
            "https://sdkwork.dev/problems/missing-credentials",
        ),
        (
            WebFrameworkError::invalid_credentials("invalid"),
            401,
            "https://sdkwork.dev/problems/invalid-credentials",
        ),
        (
            WebFrameworkError::forbidden("forbidden"),
            403,
            "https://sdkwork.dev/problems/forbidden",
        ),
        (
            WebFrameworkError::bad_request("bad"),
            400,
            "https://sdkwork.dev/problems/bad-request",
        ),
        (
            WebFrameworkError::conflict("conflict"),
            409,
            "https://sdkwork.dev/problems/conflict",
        ),
        (
            WebFrameworkError::payload_too_large("large"),
            413,
            "https://sdkwork.dev/problems/payload-too-large",
        ),
        (
            WebFrameworkError::rate_limit_exceeded("slow down", 30),
            429,
            "https://sdkwork.dev/problems/rate-limit-exceeded",
        ),
        (
            WebFrameworkError::dependency_unavailable("down"),
            503,
            "https://sdkwork.dev/problems/dependency-unavailable",
        ),
        (
            WebFrameworkError::request_timeout("timeout"),
            408,
            "https://sdkwork.dev/problems/request-timeout",
        ),
        (
            WebFrameworkError::method_not_allowed("method"),
            405,
            "https://sdkwork.dev/problems/method-not-allowed",
        ),
        (
            WebFrameworkError::not_found("missing"),
            404,
            "https://sdkwork.dev/problems/not-found",
        ),
        (
            WebFrameworkError::not_implemented("unmounted"),
            501,
            "https://sdkwork.dev/problems/not-implemented",
        ),
        (
            WebFrameworkError::internal_server_error("internal"),
            500,
            "https://sdkwork.dev/problems/internal-server-error",
        ),
        (
            WebFrameworkError::context_not_injected(),
            500,
            "https://sdkwork.dev/problems/context-not-injected",
        ),
        (
            WebFrameworkError::websocket_rejected("rejected"),
            400,
            "https://sdkwork.dev/problems/websocket-rejected",
        ),
    ];

    for (error, status, expected_type) in cases {
        let ty = problem_type(error.clone());
        let response = problem_response(&error, ProblemCorrelation::default());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let bytes = rt
            .block_on(async { axum::body::to_bytes(response.into_body(), usize::MAX).await })
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            u64::from(status),
            payload["status"].as_u64().unwrap(),
            "{expected_type}"
        );
        assert_eq!(expected_type, ty);
        assert!(
            expected_type.starts_with("https://sdkwork.dev/problems/"),
            "problem type must use sdkwork.dev URI namespace"
        );
    }
}

#[test]
fn rate_limit_problem_includes_retry_after_header() {
    let error = WebFrameworkError::rate_limit_exceeded("slow down", 42);
    let response = problem_response(&error, ProblemCorrelation::default());
    assert_eq!(
        Some("42"),
        response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
    );
}
