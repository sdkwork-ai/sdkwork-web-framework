//! WebSocket pipeline aligned with HTTP `WebCallInterceptor` semantics.

use crate::api_chain::WebCallRuntime;
use crate::error::WebFrameworkError;
use crate::request_context::WebRequestContext;
use crate::request_identity::ServerRequestId;
use crate::resolvers::WebRequestContextResolver;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

/// Established session after HTTP upgrade pipeline has run.
#[derive(Clone, Debug)]
pub struct WebSocketSession {
    pub connection_id: ServerRequestId,
    pub request_context: WebRequestContext,
}

impl WebSocketSession {
    pub fn new(request_context: WebRequestContext) -> Self {
        Self {
            connection_id: request_context.request_id.clone(),
            request_context,
        }
    }

    pub fn ctx(&self) -> &WebRequestContext {
        &self.request_context
    }

    pub fn tenant_id(&self) -> Option<&str> {
        self.request_context.tenant_id()
    }

    pub fn app_id(&self) -> Option<&str> {
        self.request_context.app_id()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebSocketCallStage {
    Connect,
    Message,
    Close,
}

/// Per-message metadata passed through the WebSocket interceptor chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebSocketMessageFrame {
    pub index: u64,
    pub byte_len: usize,
}

#[derive(Clone, Debug)]
pub struct WebSocketCallState {
    pub session: WebSocketSession,
    pub message_index: u64,
    pub message_rate_window_started: Option<Instant>,
    pub message_rate_window_count: u32,
}

pub type WebSocketCallRuntime<R> = WebCallRuntime<R>;

#[async_trait]
pub trait WebSocketCallInterceptor<R>: Send + Sync + 'static
where
    R: WebRequestContextResolver + Clone,
{
    fn name(&self) -> &'static str;
    fn stage(&self) -> WebSocketCallStage;

    async fn on_connect(
        &self,
        _state: &mut WebSocketCallState,
        _runtime: &WebSocketCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        Ok(())
    }

    async fn on_message(
        &self,
        _state: &mut WebSocketCallState,
        _frame: WebSocketMessageFrame,
        _runtime: &WebSocketCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        Ok(())
    }

    async fn on_close(
        &self,
        _state: &WebSocketCallState,
        _runtime: &WebSocketCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct WebSocketCallInterceptorChain<R>
where
    R: WebRequestContextResolver + Clone,
{
    interceptors: Vec<Arc<dyn WebSocketCallInterceptor<R>>>,
}

impl<R> WebSocketCallInterceptorChain<R>
where
    R: WebRequestContextResolver + Clone,
{
    pub fn new() -> Self {
        Self {
            interceptors: Vec::new(),
        }
    }

    pub fn standard() -> Self {
        use crate::ws_interceptors::{
            StandardWebSocketCallInterceptor, StandardWebSocketCallInterceptorKind,
        };

        Self::new()
            .with_interceptor(StandardWebSocketCallInterceptor::new(
                StandardWebSocketCallInterceptorKind::PrincipalRequired,
            ))
            .with_interceptor(StandardWebSocketCallInterceptor::new(
                StandardWebSocketCallInterceptorKind::ConnectLogging,
            ))
            .with_interceptor(StandardWebSocketCallInterceptor::new(
                StandardWebSocketCallInterceptorKind::MessageSizeLimit,
            ))
            .with_interceptor(StandardWebSocketCallInterceptor::new(
                StandardWebSocketCallInterceptorKind::MessageRateLimit,
            ))
            .with_interceptor(StandardWebSocketCallInterceptor::new(
                StandardWebSocketCallInterceptorKind::MessageLogging,
            ))
    }

    pub fn with_interceptor<I>(mut self, interceptor: I) -> Self
    where
        I: WebSocketCallInterceptor<R>,
    {
        self.interceptors.push(Arc::new(interceptor));
        self
    }

    pub async fn connect(
        &self,
        state: &mut WebSocketCallState,
        runtime: &WebSocketCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        for interceptor in &self.interceptors {
            interceptor.on_connect(state, runtime).await?;
        }
        Ok(())
    }

    pub async fn message(
        &self,
        state: &mut WebSocketCallState,
        frame: WebSocketMessageFrame,
        runtime: &WebSocketCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        for interceptor in &self.interceptors {
            interceptor.on_message(state, frame, runtime).await?;
        }
        Ok(())
    }

    pub async fn close(
        &self,
        state: &WebSocketCallState,
        runtime: &WebSocketCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        for interceptor in self.interceptors.iter().rev() {
            interceptor.on_close(state, runtime).await?;
        }
        Ok(())
    }
}
