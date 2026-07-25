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
            "https://docs.sdkwork.com/problems/40101",
        ),
        (
            WebFrameworkError::invalid_credentials("invalid"),
            401,
            "https://docs.sdkwork.com/problems/40103",
        ),
        (
            WebFrameworkError::expired_credentials("expired"),
            401,
            "https://docs.sdkwork.com/problems/40102",
        ),
        (
            WebFrameworkError::revoked_credentials("revoked"),
            401,
            "https://docs.sdkwork.com/problems/40104",
        ),
        (
            WebFrameworkError::forbidden("forbidden"),
            403,
            "https://docs.sdkwork.com/problems/40301",
        ),
        (
            WebFrameworkError::bad_request("bad"),
            400,
            "https://docs.sdkwork.com/problems/40001",
        ),
        (
            WebFrameworkError::conflict("conflict"),
            409,
            "https://docs.sdkwork.com/problems/40901",
        ),
        (
            WebFrameworkError::payload_too_large("large"),
            413,
            "https://docs.sdkwork.com/problems/41301",
        ),
        (
            WebFrameworkError::rate_limit_exceeded("slow down", 30),
            429,
            "https://docs.sdkwork.com/problems/42901",
        ),
        (
            WebFrameworkError::dependency_unavailable("down"),
            503,
            "https://docs.sdkwork.com/problems/50301",
        ),
        (
            WebFrameworkError::request_timeout("timeout"),
            408,
            "https://docs.sdkwork.com/problems/40801",
        ),
        (
            WebFrameworkError::method_not_allowed("method"),
            405,
            "https://docs.sdkwork.com/problems/40501",
        ),
        (
            WebFrameworkError::not_found("missing"),
            404,
            "https://docs.sdkwork.com/problems/40401",
        ),
        (
            WebFrameworkError::not_implemented("unmounted"),
            501,
            "https://docs.sdkwork.com/problems/50001",
        ),
        (
            WebFrameworkError::internal_server_error("internal"),
            500,
            "https://docs.sdkwork.com/problems/50001",
        ),
        (
            WebFrameworkError::context_not_injected(),
            500,
            "https://docs.sdkwork.com/problems/50001",
        ),
        (
            WebFrameworkError::websocket_rejected("rejected"),
            400,
            "https://docs.sdkwork.com/problems/40001",
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
            expected_type.starts_with("https://docs.sdkwork.com/problems/"),
            "problem type must use docs.sdkwork.com numeric URI namespace"
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
