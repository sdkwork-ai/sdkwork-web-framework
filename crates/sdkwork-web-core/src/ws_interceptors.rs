//! Standard WebSocket interceptors (connect / message / close).

use crate::error::WebFrameworkError;
use crate::problem::redact_path_template;
use crate::resolvers::WebRequestContextResolver;
use crate::websocket::{
    WebSocketCallInterceptor, WebSocketCallRuntime, WebSocketCallStage, WebSocketCallState,
    WebSocketMessageFrame,
};
use async_trait::async_trait;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StandardWebSocketCallInterceptorKind {
    ConnectLogging,
    MessageLogging,
    PrincipalRequired,
    MessageSizeLimit,
    MessageRateLimit,
}

#[derive(Clone, Debug)]
pub struct StandardWebSocketCallInterceptor {
    pub kind: StandardWebSocketCallInterceptorKind,
}

impl StandardWebSocketCallInterceptor {
    pub fn new(kind: StandardWebSocketCallInterceptorKind) -> Self {
        Self { kind }
    }
}

#[async_trait]
impl<R> WebSocketCallInterceptor<R> for StandardWebSocketCallInterceptor
where
    R: WebRequestContextResolver + Clone,
{
    fn name(&self) -> &'static str {
        match self.kind {
            StandardWebSocketCallInterceptorKind::ConnectLogging => "ws_connect_logging",
            StandardWebSocketCallInterceptorKind::MessageLogging => "ws_message_logging",
            StandardWebSocketCallInterceptorKind::PrincipalRequired => "ws_principal_required",
            StandardWebSocketCallInterceptorKind::MessageSizeLimit => "ws_message_size_limit",
            StandardWebSocketCallInterceptorKind::MessageRateLimit => "ws_message_rate_limit",
        }
    }

    fn stage(&self) -> WebSocketCallStage {
        match self.kind {
            StandardWebSocketCallInterceptorKind::ConnectLogging
            | StandardWebSocketCallInterceptorKind::PrincipalRequired => {
                WebSocketCallStage::Connect
            }
            StandardWebSocketCallInterceptorKind::MessageLogging
            | StandardWebSocketCallInterceptorKind::MessageSizeLimit
            | StandardWebSocketCallInterceptorKind::MessageRateLimit => WebSocketCallStage::Message,
        }
    }

    async fn on_connect(
        &self,
        state: &mut WebSocketCallState,
        _runtime: &WebSocketCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        match self.kind {
            StandardWebSocketCallInterceptorKind::ConnectLogging => {
                let ctx = state.session.ctx();
                tracing::info!(
                    connection_id = %state.session.connection_id.0,
                    request_id = %ctx.request_id.0,
                    api_surface = ?ctx.api_surface,
                    route_template = %redact_path_template(&ctx.transport.path),
                    "websocket connection accepted"
                );
            }
            StandardWebSocketCallInterceptorKind::PrincipalRequired => {
                if state.session.ctx().principal.is_none() {
                    return Err(WebFrameworkError::websocket_rejected(
                        "websocket connection requires authenticated principal",
                    ));
                }
            }
            StandardWebSocketCallInterceptorKind::MessageLogging
            | StandardWebSocketCallInterceptorKind::MessageSizeLimit
            | StandardWebSocketCallInterceptorKind::MessageRateLimit => {}
        }
        Ok(())
    }

    async fn on_message(
        &self,
        state: &mut WebSocketCallState,
        frame: WebSocketMessageFrame,
        runtime: &WebSocketCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        match self.kind {
            StandardWebSocketCallInterceptorKind::MessageSizeLimit => {
                if let Some(max_bytes) = runtime.security_policy.websocket.max_message_bytes {
                    if frame.byte_len as u64 > max_bytes {
                        return Err(WebFrameworkError::payload_too_large(format!(
                            "websocket message exceeds limit of {max_bytes} bytes"
                        )));
                    }
                }
            }
            StandardWebSocketCallInterceptorKind::MessageRateLimit => {
                let policy = &runtime.security_policy.websocket;
                if !policy.message_rate_limit_enabled {
                    return Ok(());
                }
                let now = Instant::now();
                let window = Duration::from_secs(policy.message_window_secs);
                let window_expired = state
                    .message_rate_window_started
                    .is_none_or(|started| now.duration_since(started) > window);
                if window_expired {
                    state.message_rate_window_started = Some(now);
                    state.message_rate_window_count = 1;
                } else {
                    state.message_rate_window_count += 1;
                    if state.message_rate_window_count > policy.max_messages_per_window {
                        return Err(WebFrameworkError::rate_limit_exceeded(
                            "websocket message rate limit exceeded",
                            policy.message_window_secs,
                        ));
                    }
                }
            }
            StandardWebSocketCallInterceptorKind::MessageLogging => {
                tracing::debug!(
                    connection_id = %state.session.connection_id.0,
                    message_index = frame.index,
                    byte_len = frame.byte_len,
                    "websocket message received"
                );
            }
            StandardWebSocketCallInterceptorKind::ConnectLogging
            | StandardWebSocketCallInterceptorKind::PrincipalRequired => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_context::{
        WebApiSurface, WebAuthMode, WebRequestContext, WebTransportFacts,
    };
    use crate::request_identity::ServerRequestId;
    use crate::websocket::WebSocketCallInterceptorChain;
    use crate::{DefaultWebRequestContextResolver, WebCallRuntime, WebRequestPrincipal};

    fn sample_state() -> WebSocketCallState {
        let ctx = WebRequestContext {
            request_id: ServerRequestId("req-ws-1".to_owned()),
            api_surface: WebApiSurface::AppApi,
            auth_mode: WebAuthMode::DualToken,
            principal: Some(
                WebRequestPrincipal::builder()
                    .tenant_id("100001")
                    .user_id("user-1")
                    .app_id("app-1")
                    .build(),
            ),
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
            session: crate::websocket::WebSocketSession::new(ctx),
            message_index: 1,
            message_rate_window_started: None,
            message_rate_window_count: 0,
        }
    }

    #[tokio::test]
    async fn message_size_limit_rejects_oversized_frames() {
        let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
        let chain = WebSocketCallInterceptorChain::standard();
        let mut state = sample_state();
        let frame = WebSocketMessageFrame {
            index: 1,
            byte_len: 2 * 1024 * 1024,
        };
        let error = chain
            .message(&mut state, frame, &runtime)
            .await
            .expect_err("oversized websocket frame");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::PayloadTooLarge,
            error.kind
        );
        assert!(error.message.contains("websocket message exceeds limit"));
    }

    #[tokio::test]
    async fn message_rate_limit_blocks_burst_traffic() {
        let mut runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
        runtime.security_policy.websocket.max_messages_per_window = 2;
        runtime.security_policy.websocket.message_window_secs = 60;
        let chain = WebSocketCallInterceptorChain::standard();
        let mut state = sample_state();
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
            .expect_err("third message should be rate limited");
        assert!(error.message.contains("websocket message rate limit"));
    }
}
