//! Integration tests for HTTP context injection and WebSocket route wiring.

use axum::body::{to_bytes, Body};
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::Query;
use axum::http::Request;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use sdkwork_web_axum::{
    run_websocket_session, with_web_request_context, WebFrameworkLayer, WebSocketUpgradeLayer,
};
use sdkwork_web_core::{
    DefaultWebRequestContextResolver, WebRequestContext, WebRequestContextProfile,
    WebSocketCallInterceptorChain, WebSocketCallRuntime, WebSocketSession, SDKWORK_TRACE_ID_HEADER,
};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn handler_receives_injected_web_request_context() {
    let layer = WebFrameworkLayer::new(DefaultWebRequestContextResolver::default());
    let app = with_web_request_context(
        Router::new().route(
            "/healthz",
            get(|ctx: WebRequestContext| async move { ctx.request_id.0.clone() }),
        ),
        layer,
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let request_id = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(!request_id.is_empty());
}

#[derive(serde::Deserialize)]
struct PingQuery {
    echo: Option<String>,
}

#[tokio::test]
async fn handler_supports_web_request_context_with_other_extractors() {
    let layer = WebFrameworkLayer::new(DefaultWebRequestContextResolver::default());
    let app = with_web_request_context(
        Router::new().route(
            "/healthz",
            get(
                |Query(query): Query<PingQuery>, ctx: WebRequestContext| async move {
                    format!("{}:{}", ctx.request_id.0, query.echo.unwrap_or_default())
                },
            ),
        ),
        layer,
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz?echo=ok")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let text = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(text.ends_with(":ok"));
}

/// `tower::ServiceExt::oneshot` cannot complete a WebSocket handshake; this test verifies
/// the WS route is registered and the HTTP pipeline runs (`X-SdkWork-Trace-Id` header present).
#[tokio::test]
async fn websocket_route_runs_http_pipeline_before_upgrade_extractor() {
    let resolver = DefaultWebRequestContextResolver::default();
    let profile = WebRequestContextProfile {
        public_path_prefixes: vec!["/ws".to_owned()],
        ..WebRequestContextProfile::default()
    };
    let http_layer = WebFrameworkLayer::new(resolver.clone()).with_profile(profile);
    let ws_layer = WebSocketUpgradeLayer {
        ws_chain: Arc::new(WebSocketCallInterceptorChain::new()),
        http_runtime: Arc::new(WebSocketCallRuntime::new(resolver)),
    };

    let ws_layer_for_route = ws_layer.clone();
    let app = with_web_request_context(
        Router::new().route(
            "/ws",
            get(move |ws: WebSocketUpgrade, ctx: WebRequestContext| {
                let ws_layer = ws_layer_for_route.clone();
                async move { ws_upgrade(ws, ctx, ws_layer).await }
            }),
        ),
        http_layer,
    );

    let response = app
        .oneshot(Request::builder().uri("/ws").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert!(
        response.status() == axum::http::StatusCode::UPGRADE_REQUIRED
            || response.status() == axum::http::StatusCode::BAD_REQUEST,
        "unexpected status: {}",
        response.status()
    );
    assert!(
        response.headers().get(SDKWORK_TRACE_ID_HEADER).is_some(),
        "HTTP pipeline must run before the upgrade handler"
    );
}

#[tokio::test]
async fn pipeline_problem_response_includes_trace_id_from_traceparent() {
    use axum::routing::post;
    use sdkwork_web_core::TRACEPARENT_HEADER;

    let layer = WebFrameworkLayer::new(DefaultWebRequestContextResolver::default());
    let app = with_web_request_context(
        Router::new().route("/app/v3/api/users", post(|| async { "ok" })),
        layer,
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/users")
                .header("origin", "https://evil.example")
                .header(
                    TRACEPARENT_HEADER,
                    "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(axum::http::StatusCode::FORBIDDEN, response.status());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        "4bf92f3577b34da6a3ce929d0e0e4736",
        payload["traceId"].as_str().unwrap()
    );
    assert!(payload.get("requestId").is_none());
    assert!(payload["traceId"].as_str().is_some());
}

#[tokio::test]
async fn extractor_rejection_includes_trace_id_without_pipeline_context() {
    use axum::routing::get;
    use sdkwork_web_core::TRACEPARENT_HEADER;

    let app = Router::new().route(
        "/no-pipeline",
        get(|_ctx: WebRequestContext| async { "ok" }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/no-pipeline")
                .header(
                    TRACEPARENT_HEADER,
                    "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        response.status()
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        "https://sdkwork.dev/problems/context-not-injected",
        payload["type"].as_str().unwrap()
    );
    assert_eq!(
        "4bf92f3577b34da6a3ce929d0e0e4736",
        payload["traceId"].as_str().unwrap()
    );
    assert!(payload.get("requestId").is_none());
    assert!(payload["traceId"].as_str().is_some());
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    ctx: WebRequestContext,
    ws_layer: WebSocketUpgradeLayer<DefaultWebRequestContextResolver>,
) -> Response {
    run_websocket_session(
        ws,
        ctx,
        ws_layer,
        |_session: WebSocketSession, socket| async move {
            let _ = socket;
        },
    )
    .await
}
