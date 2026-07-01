use crate::correlation::OwnedProblemCorrelation;
use axum::extract::Request;
use axum::middleware::{from_fn, Next};
use axum::response::Response;
use axum::Router;
use sdkwork_web_core::{problem_response, WebFrameworkError};
use std::time::Duration;

/// Apply a request timeout to all routes (catalog A10).
///
/// Apply **inside** `with_web_request_context` (closer to handlers than the framework layer)
/// so idempotency reservations are finalized or released when a request times out.
pub fn with_request_timeout(router: Router, timeout: Duration) -> Router {
    router.layer(from_fn(move |request, next| {
        let timeout = timeout;
        async move { request_timeout_middleware(request, next, timeout).await }
    }))
}

async fn request_timeout_middleware(request: Request, next: Next, timeout: Duration) -> Response {
    let correlation = OwnedProblemCorrelation::from_request(&request);
    match tokio::time::timeout(timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => problem_response(
            &WebFrameworkError::request_timeout("request timed out"),
            correlation.as_correlation(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::routing::get;
    use sdkwork_web_core::{REQUEST_ID_HEADER, TRACEPARENT_HEADER};
    use std::time::Duration;
    use tower::ServiceExt;

    #[tokio::test]
    async fn times_out_slow_handlers() {
        let app = with_request_timeout(
            Router::new().route(
                "/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    "ok"
                }),
            ),
            Duration::from_millis(50),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/slow")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(StatusCode::REQUEST_TIMEOUT, response.status());
    }

    #[tokio::test]
    async fn timeout_problem_includes_trace_correlation() {
        let app = with_request_timeout(
            Router::new().route(
                "/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    "ok"
                }),
            ),
            Duration::from_millis(50),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/slow")
                    .header(
                        TRACEPARENT_HEADER,
                        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                    )
                    .header(REQUEST_ID_HEADER, "req-timeout-integration")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            "4bf92f3577b34da6a3ce929d0e0e4736",
            payload["traceId"].as_str().unwrap()
        );
        assert!(payload.get("requestId").is_none());
    }
}
