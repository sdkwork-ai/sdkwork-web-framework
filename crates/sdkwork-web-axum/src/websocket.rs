use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures_util::StreamExt;
use sdkwork_web_core::{
    WebRequestContext, WebSocketCallInterceptorChain, WebSocketCallRuntime, WebSocketCallState,
    WebSocketMessageFrame, WebSocketSession,
};
use std::future::Future;
use std::sync::Arc;

/// Config for WebSocket message pipeline. HTTP upgrade must pass `with_web_request_context` first.
#[derive(Clone)]
pub struct WebSocketUpgradeLayer<R>
where
    R: sdkwork_web_core::WebRequestContextResolver + Clone,
{
    pub ws_chain: Arc<WebSocketCallInterceptorChain<R>>,
    pub http_runtime: Arc<WebSocketCallRuntime<R>>,
}

/// Upgrade handler: `ctx` is auto-injected via `WebRequestContext` extractor on the upgrade route.
pub async fn run_websocket_session<R, H, Fut>(
    ws: WebSocketUpgrade,
    ctx: WebRequestContext,
    layer: WebSocketUpgradeLayer<R>,
    handler: H,
) -> Response
where
    R: sdkwork_web_core::WebRequestContextResolver + Clone,
    H: Fn(WebSocketSession, WebSocket) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let session = WebSocketSession::new(ctx);
    let ws_chain = layer.ws_chain.clone();
    let runtime = layer.http_runtime.clone();
    let handler = Arc::new(handler);

    ws.on_upgrade(move |socket| {
        let ws_chain = ws_chain.clone();
        let runtime = runtime.clone();
        let handler = handler.clone();
        let session = session.clone();
        async move {
            let mut state = WebSocketCallState {
                session: session.clone(),
                message_index: 0,
                message_rate_window_started: None,
                message_rate_window_count: 0,
            };
            if ws_chain.connect(&mut state, &runtime).await.is_err() {
                return;
            }
            handler(session, socket).await;
            let _ = ws_chain.close(&state, &runtime).await;
        }
    })
}

fn message_byte_len(message: &Message) -> usize {
    match message {
        Message::Text(text) => text.len(),
        Message::Binary(bytes) => bytes.len(),
        Message::Ping(bytes) | Message::Pong(bytes) => bytes.len(),
        Message::Close(_) => 0,
    }
}

/// Echo helper with per-message WebSocket interceptors.
pub async fn echo_with_interceptors<R>(
    mut socket: WebSocket,
    mut state: WebSocketCallState,
    ws_chain: Arc<WebSocketCallInterceptorChain<R>>,
    runtime: Arc<WebSocketCallRuntime<R>>,
) where
    R: sdkwork_web_core::WebRequestContextResolver + Clone,
{
    while let Some(Ok(message)) = socket.next().await {
        state.message_index += 1;
        let frame = WebSocketMessageFrame {
            index: state.message_index,
            byte_len: message_byte_len(&message),
        };
        if ws_chain.message(&mut state, frame, &runtime).await.is_err() {
            break;
        }
        if let Message::Close(_) = message {
            break;
        }
        if socket.send(message).await.is_err() {
            break;
        }
    }
    let _ = ws_chain.close(&state, &runtime).await;
}
