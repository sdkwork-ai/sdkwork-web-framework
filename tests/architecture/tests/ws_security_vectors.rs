//! WebSocket security vectors (catalog K4 / OWASP API WS hardening).

use sdkwork_web_core::request_context::{
    WebApiSurface, WebAuthMode, WebRequestContext, WebTransportFacts,
};
use sdkwork_web_core::request_identity::ServerRequestId;
use sdkwork_web_core::websocket::{
    WebSocketCallInterceptorChain, WebSocketCallState, WebSocketMessageFrame,
};
use sdkwork_web_core::{
    DefaultWebRequestContextResolver, WebCallRuntime, WebFrameworkErrorKind, WebRequestPrincipal,
};

fn ws_state_with_principal(principal: Option<WebRequestPrincipal>) -> WebSocketCallState {
    let ctx = WebRequestContext {
        request_id: ServerRequestId("req-ws-arch".to_owned()),
        api_surface: WebApiSurface::AppApi,
        auth_mode: WebAuthMode::DualToken,
        principal,
        transport: WebTransportFacts {
            path: "/app/v3/api/ws".to_owned(),
            method: "GET".to_owned(),
            auth_token_present: true,
            access_token_present: true,
            api_key_present: false,
            oauth_bearer_present: false,
            agent_token_present: false,
        },
        locale: None,
        client_kind: None,
        operation: None,
        trace_id: None,
    };
    WebSocketCallState {
        session: sdkwork_web_core::websocket::WebSocketSession::new(ctx),
        message_index: 1,
        message_rate_window_started: None,
        message_rate_window_count: 0,
    }
}

#[tokio::test]
async fn websocket_connect_without_principal_is_rejected() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebSocketCallInterceptorChain::standard();
    let mut state = ws_state_with_principal(None);
    let error = chain
        .connect(&mut state, &runtime)
        .await
        .expect_err("unauthenticated websocket connect must be rejected");
    assert_eq!(WebFrameworkErrorKind::WebSocketRejected, error.kind);
}

#[tokio::test]
async fn websocket_oversized_message_is_rejected() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebSocketCallInterceptorChain::standard();
    let mut state = ws_state_with_principal(Some(
        WebRequestPrincipal::builder()
            .tenant_id("100001")
            .user_id("user-1")
            .app_id("app-1")
            .build(),
    ));
    let max_bytes = runtime
        .security_policy
        .websocket
        .max_message_bytes
        .unwrap_or(1024 * 1024) as usize;
    let frame = WebSocketMessageFrame {
        index: 1,
        byte_len: max_bytes.saturating_add(1),
    };
    let error = chain
        .message(&mut state, frame, &runtime)
        .await
        .expect_err("oversized websocket message");
    assert_eq!(WebFrameworkErrorKind::PayloadTooLarge, error.kind);
}

#[tokio::test]
async fn websocket_message_rate_limit_blocks_burst_traffic() {
    let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    runtime.security_policy.websocket.max_messages_per_window = 2;
    runtime.security_policy.websocket.message_window_secs = 60;
    let chain = WebSocketCallInterceptorChain::standard();
    let mut state = ws_state_with_principal(Some(
        WebRequestPrincipal::builder()
            .tenant_id("100001")
            .user_id("user-1")
            .app_id("app-1")
            .build(),
    ));
    let frame = WebSocketMessageFrame {
        index: 1,
        byte_len: 16,
    };
    chain
        .message(&mut state, frame, &runtime)
        .await
        .expect("first message");
    chain
        .message(&mut state, frame, &runtime)
        .await
        .expect("second message");
    let error = chain
        .message(&mut state, frame, &runtime)
        .await
        .expect_err("third message exceeds rate limit");
    assert_eq!(WebFrameworkErrorKind::RateLimitExceeded, error.kind);
}
