use crate::problem::{problem_response, ProblemCorrelation};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebFrameworkErrorKind {
    MissingCredentials,
    InvalidCredentials,
    Forbidden,
    BadRequest,
    Conflict,
    PayloadTooLarge,
    RateLimitExceeded,
    DependencyUnavailable,
    RequestTimeout,
    MethodNotAllowed,
    NotFound,
    NotImplemented,
    InternalServerError,
    ContextNotInjected,
    WebSocketRejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebFrameworkError {
    pub kind: WebFrameworkErrorKind,
    pub message: String,
    pub retry_after_seconds: Option<u64>,
}

pub type AppRequestContextError = WebFrameworkError;
pub type AppRequestContextErrorKind = WebFrameworkErrorKind;

impl WebFrameworkError {
    fn new(kind: WebFrameworkErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retry_after_seconds: None,
        }
    }

    pub fn missing_credentials(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::MissingCredentials, message)
    }

    pub fn invalid_credentials(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::InvalidCredentials, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::Forbidden, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::BadRequest, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::Conflict, message)
    }

    pub fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::PayloadTooLarge, message)
    }

    pub fn rate_limit_exceeded(message: impl Into<String>, retry_after_seconds: u64) -> Self {
        Self {
            kind: WebFrameworkErrorKind::RateLimitExceeded,
            message: message.into(),
            retry_after_seconds: Some(retry_after_seconds),
        }
    }

    pub fn dependency_unavailable(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::DependencyUnavailable, message)
    }

    pub fn request_timeout(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::RequestTimeout, message)
    }

    pub fn method_not_allowed(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::MethodNotAllowed, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::NotFound, message)
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::NotImplemented, message)
    }

    pub fn internal_server_error(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::InternalServerError, message)
    }

    pub fn context_not_injected() -> Self {
        Self::new(
            WebFrameworkErrorKind::ContextNotInjected,
            "WebRequestContext was not injected by the framework pipeline",
        )
    }

    pub fn websocket_rejected(message: impl Into<String>) -> Self {
        Self::new(WebFrameworkErrorKind::WebSocketRejected, message)
    }

    pub fn status(&self) -> StatusCode {
        match self.kind {
            WebFrameworkErrorKind::MissingCredentials
            | WebFrameworkErrorKind::InvalidCredentials => StatusCode::UNAUTHORIZED,
            WebFrameworkErrorKind::Forbidden => StatusCode::FORBIDDEN,
            WebFrameworkErrorKind::BadRequest | WebFrameworkErrorKind::WebSocketRejected => {
                StatusCode::BAD_REQUEST
            }
            WebFrameworkErrorKind::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            WebFrameworkErrorKind::NotFound => StatusCode::NOT_FOUND,
            WebFrameworkErrorKind::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            WebFrameworkErrorKind::InternalServerError
            | WebFrameworkErrorKind::ContextNotInjected => StatusCode::INTERNAL_SERVER_ERROR,
            WebFrameworkErrorKind::Conflict => StatusCode::CONFLICT,
            WebFrameworkErrorKind::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            WebFrameworkErrorKind::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            WebFrameworkErrorKind::DependencyUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            WebFrameworkErrorKind::RequestTimeout => StatusCode::REQUEST_TIMEOUT,
        }
    }

    pub fn result_code(&self) -> i32 {
        use sdkwork_utils_rust::SdkWorkResultCode;
        match self.kind {
            WebFrameworkErrorKind::MissingCredentials => {
                SdkWorkResultCode::AuthenticationRequired.as_i32()
            }
            WebFrameworkErrorKind::InvalidCredentials => SdkWorkResultCode::InvalidToken.as_i32(),
            WebFrameworkErrorKind::Forbidden => SdkWorkResultCode::PermissionRequired.as_i32(),
            WebFrameworkErrorKind::BadRequest => SdkWorkResultCode::ValidationError.as_i32(),
            WebFrameworkErrorKind::Conflict => SdkWorkResultCode::Conflict.as_i32(),
            WebFrameworkErrorKind::PayloadTooLarge => SdkWorkResultCode::PayloadTooLarge.as_i32(),
            WebFrameworkErrorKind::RateLimitExceeded => {
                SdkWorkResultCode::RateLimitExceeded.as_i32()
            }
            WebFrameworkErrorKind::DependencyUnavailable => {
                SdkWorkResultCode::ServiceUnavailable.as_i32()
            }
            WebFrameworkErrorKind::RequestTimeout => SdkWorkResultCode::RequestTimeout.as_i32(),
            WebFrameworkErrorKind::MethodNotAllowed => SdkWorkResultCode::MethodNotAllowed.as_i32(),
            WebFrameworkErrorKind::NotFound => SdkWorkResultCode::NotFound.as_i32(),
            WebFrameworkErrorKind::NotImplemented
            | WebFrameworkErrorKind::InternalServerError
            | WebFrameworkErrorKind::ContextNotInjected => {
                SdkWorkResultCode::InternalError.as_i32()
            }
            WebFrameworkErrorKind::WebSocketRejected => SdkWorkResultCode::ValidationError.as_i32(),
        }
    }
}

impl fmt::Display for WebFrameworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl fmt::Display for WebFrameworkErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebFrameworkErrorKind::MissingCredentials => write!(f, "missing_credentials"),
            WebFrameworkErrorKind::InvalidCredentials => write!(f, "invalid_credentials"),
            WebFrameworkErrorKind::Forbidden => write!(f, "forbidden"),
            WebFrameworkErrorKind::BadRequest => write!(f, "bad_request"),
            WebFrameworkErrorKind::Conflict => write!(f, "conflict"),
            WebFrameworkErrorKind::PayloadTooLarge => write!(f, "payload_too_large"),
            WebFrameworkErrorKind::RateLimitExceeded => write!(f, "rate_limit_exceeded"),
            WebFrameworkErrorKind::DependencyUnavailable => write!(f, "dependency_unavailable"),
            WebFrameworkErrorKind::RequestTimeout => write!(f, "request_timeout"),
            WebFrameworkErrorKind::MethodNotAllowed => write!(f, "method_not_allowed"),
            WebFrameworkErrorKind::NotFound => write!(f, "not_found"),
            WebFrameworkErrorKind::NotImplemented => write!(f, "not_implemented"),
            WebFrameworkErrorKind::InternalServerError => write!(f, "internal_server_error"),
            WebFrameworkErrorKind::ContextNotInjected => write!(f, "context_not_injected"),
            WebFrameworkErrorKind::WebSocketRejected => write!(f, "websocket_rejected"),
        }
    }
}

use crate::request_context::WebRequestContext;
use crate::trace::trace_id_from_traceparent;
use crate::{new_request_id, REQUEST_ID_HEADER, TRACEPARENT_HEADER};
use axum::http::request::Parts;

fn read_header(headers: &http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn correlation_from_parts(parts: &Parts) -> (String, Option<String>) {
    if let Some(context) = parts.extensions.get::<WebRequestContext>() {
        return (context.request_id.0.clone(), context.trace_id.clone());
    }
    let request_id = read_header(&parts.headers, REQUEST_ID_HEADER).unwrap_or_else(new_request_id);
    let trace_id = read_header(&parts.headers, TRACEPARENT_HEADER)
        .and_then(|traceparent| trace_id_from_traceparent(&traceparent).map(str::to_owned));
    (request_id, trace_id)
}

/// Axum extractor rejection with request-scoped Problem correlation (`OBSERVABILITY_SPEC` §1).
#[derive(Debug)]
pub struct WebFrameworkRejection {
    pub error: WebFrameworkError,
    request_id: String,
    trace_id: Option<String>,
    method: String,
    path: String,
}

impl WebFrameworkRejection {
    pub fn new(error: WebFrameworkError, parts: &Parts) -> Self {
        let (request_id, trace_id) = correlation_from_parts(parts);
        Self {
            error,
            request_id,
            trace_id,
            method: parts.method.as_str().to_owned(),
            path: parts.uri.path().to_owned(),
        }
    }
}

impl IntoResponse for WebFrameworkRejection {
    fn into_response(self) -> Response {
        problem_response(
            &self.error,
            ProblemCorrelation::new(Some(&self.request_id), self.trace_id.as_deref()).with_routing(
                Some(self.method.as_str()),
                None,
                Some(self.path.as_str()),
                None,
            ),
        )
    }
}
