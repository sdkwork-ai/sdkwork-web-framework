use axum::{
    http::{HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use sdkwork_utils_rust::SdkWorkApiResponse;
use sdkwork_web_core::{
    problem_response, WebFrameworkError, WebFrameworkErrorKind, WebRequestContext,
};
use serde::Serialize;

pub type ApiResult<T> = Result<T, ApiProblem>;

pub fn ok_json<T>(data: T) -> ApiResult<T> {
    Ok(data)
}

pub fn created_json<T: Serialize>(
    ctx: &WebRequestContext,
    data: T,
) -> Result<Response, ApiProblem> {
    success_response(ctx, StatusCode::CREATED, data)
}

pub fn success_json<T: Serialize>(
    ctx: &WebRequestContext,
    data: T,
) -> Result<Response, ApiProblem> {
    success_response(ctx, StatusCode::OK, data)
}

pub fn no_content(ctx: &WebRequestContext) -> Result<Response, ApiProblem> {
    let mut response = StatusCode::NO_CONTENT.into_response();
    attach_trace_header(&mut response, &ctx.resolved_trace_id());
    Ok(response)
}

fn success_response<T: Serialize>(
    ctx: &WebRequestContext,
    status: StatusCode,
    data: T,
) -> Result<Response, ApiProblem> {
    let trace_id = ctx.resolved_trace_id();
    let envelope = SdkWorkApiResponse::success(data, trace_id.clone());
    let mut response = (status, Json(envelope)).into_response();
    attach_trace_header(&mut response, &trace_id);
    Ok(response)
}

fn attach_trace_header(response: &mut Response, trace_id: &str) {
    if let Ok(value) = HeaderValue::from_str(trace_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-sdkwork-trace-id"), value);
    }
}

#[derive(Debug)]
pub struct ApiProblem {
    pub message: String,
    status: StatusCode,
}

impl ApiProblem {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::BAD_REQUEST,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::FORBIDDEN,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::NOT_FOUND,
        }
    }

    pub fn dependency_unavailable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    pub fn internal_server_error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn from_web_framework(error: sdkwork_web_core::WebFrameworkError) -> Self {
        let status = error.status();
        Self {
            message: error.message,
            status,
        }
    }

    fn framework_error(&self) -> WebFrameworkError {
        let kind = match self.status {
            StatusCode::BAD_REQUEST => WebFrameworkErrorKind::BadRequest,
            StatusCode::FORBIDDEN => WebFrameworkErrorKind::Forbidden,
            StatusCode::NOT_FOUND => WebFrameworkErrorKind::NotFound,
            StatusCode::CONFLICT => WebFrameworkErrorKind::Conflict,
            StatusCode::PAYLOAD_TOO_LARGE => WebFrameworkErrorKind::PayloadTooLarge,
            StatusCode::TOO_MANY_REQUESTS => WebFrameworkErrorKind::RateLimitExceeded,
            StatusCode::SERVICE_UNAVAILABLE => WebFrameworkErrorKind::DependencyUnavailable,
            StatusCode::REQUEST_TIMEOUT => WebFrameworkErrorKind::RequestTimeout,
            StatusCode::METHOD_NOT_ALLOWED => WebFrameworkErrorKind::MethodNotAllowed,
            StatusCode::UNAUTHORIZED => WebFrameworkErrorKind::MissingCredentials,
            StatusCode::INTERNAL_SERVER_ERROR => WebFrameworkErrorKind::InternalServerError,
            _ => WebFrameworkErrorKind::InternalServerError,
        };
        WebFrameworkError {
            kind,
            message: self.message.clone(),
            retry_after_seconds: None,
        }
    }

    pub fn into_response_for(&self, ctx: &WebRequestContext) -> Response {
        problem_response(&self.framework_error(), ctx.problem_correlation())
    }
}

/// Finish a JSON handler `Result` with request-scoped Problem correlation.
pub fn finish_api_json<T: Serialize>(ctx: &WebRequestContext, result: ApiResult<T>) -> Response {
    match result {
        Ok(data) => {
            success_json(ctx, data).unwrap_or_else(|problem| problem.into_response_for(ctx))
        }
        Err(problem) => problem.into_response_for(ctx),
    }
}

/// Finish a raw `Response` handler `Result` with request-scoped Problem correlation.
pub fn finish_api_response(
    ctx: &WebRequestContext,
    result: Result<Response, ApiProblem>,
) -> Response {
    match result {
        Ok(response) => response,
        Err(problem) => problem.into_response_for(ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use sdkwork_web_core::{ServerRequestId, WebApiSurface, WebAuthMode, WebTransportFacts};

    fn test_context() -> WebRequestContext {
        WebRequestContext {
            request_id: ServerRequestId("test-req".to_owned()),
            api_surface: WebApiSurface::BackendApi,
            auth_mode: WebAuthMode::DualToken,
            principal: None,
            transport: WebTransportFacts {
                path: "/backend/v3/api/web-framework/cors_policies".to_owned(),
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
            trace_id: Some("trace-from-context-abc".to_owned()),
        }
    }

    #[tokio::test]
    async fn success_json_uses_sdkwork_api_response_envelope() {
        let response =
            success_json(&test_context(), serde_json::json!({ "item": 1 })).expect("response");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(0, payload["code"].as_i64().unwrap());
        assert_eq!(
            "trace-from-context-abc",
            payload["traceId"].as_str().unwrap()
        );
        assert_eq!(1, payload["data"]["item"].as_i64().unwrap());
    }

    #[tokio::test]
    async fn api_problem_uses_problem_json_content_type() {
        let response =
            ApiProblem::forbidden("missing permission").into_response_for(&test_context());
        assert_eq!(
            "application/problem+json",
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(403, payload["status"].as_u64().unwrap());
        assert_eq!(40301, payload["code"].as_i64().unwrap());
        assert!(payload["detail"]
            .as_str()
            .unwrap()
            .contains("missing permission"));
        assert!(!payload.to_string().contains("backtrace"));
    }

    #[tokio::test]
    async fn api_problem_into_response_for_includes_request_correlation() {
        let ctx = test_context();
        let response = ApiProblem::forbidden("missing permission").into_response_for(&ctx);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            "trace-from-context-abc",
            payload["traceId"].as_str().unwrap()
        );
        assert_eq!(
            "GET /backend/v3/api/web-framework/cors_policies",
            payload["instance"].as_str().unwrap()
        );
        assert!(payload.get("requestId").is_none());
    }

    #[tokio::test]
    async fn api_problem_not_found_returns_404_problem_json() {
        let response =
            ApiProblem::not_found("control node missing").into_response_for(&test_context());
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(404, payload["status"].as_u64().unwrap());
        assert_eq!(40401, payload["code"].as_i64().unwrap());
        assert_eq!(
            "https://docs.sdkwork.com/problems/40401",
            payload["type"].as_str().unwrap()
        );
    }

    #[tokio::test]
    async fn no_content_response_has_no_body() {
        let response = no_content(&test_context()).expect("response");
        assert_eq!(StatusCode::NO_CONTENT, response.status());
        assert!(response.headers().get("x-sdkwork-trace-id").is_some());
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn api_problem_dependency_unavailable_returns_503_problem_json() {
        let response = ApiProblem::dependency_unavailable("database operation failed")
            .into_response_for(&test_context());
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(503, payload["status"].as_u64().unwrap());
        assert_eq!(50301, payload["code"].as_i64().unwrap());
        assert_eq!(
            "https://docs.sdkwork.com/problems/50301",
            payload["type"].as_str().unwrap()
        );
    }
}
