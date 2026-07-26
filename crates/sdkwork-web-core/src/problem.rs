use crate::error::{WebFrameworkError, WebFrameworkErrorKind};
use crate::trace::resolve_problem_trace_id;
use axum::body::Body;
use axum::http::{header, HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::Json;
use sdkwork_utils_rust::{SdkWorkProblemDetail, SdkWorkProblemRouting, SdkWorkResultCode};

const PROBLEM_RESPONSE_ENRICHMENT_MAX_BYTES: usize = 64 * 1024;

fn is_problem_json(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value.starts_with("application/problem+json")
                || value.starts_with("application/problem+json;")
        })
        .unwrap_or(false)
}

fn map_result_code(code: i32) -> SdkWorkResultCode {
    match code {
        40001 => SdkWorkResultCode::ValidationError,
        40101 => SdkWorkResultCode::AuthenticationRequired,
        40102 => SdkWorkResultCode::TokenExpired,
        40103 => SdkWorkResultCode::InvalidToken,
        40104 => SdkWorkResultCode::SessionRevoked,
        40301 => SdkWorkResultCode::PermissionRequired,
        40401 => SdkWorkResultCode::NotFound,
        40501 => SdkWorkResultCode::MethodNotAllowed,
        40801 => SdkWorkResultCode::RequestTimeout,
        40901 => SdkWorkResultCode::Conflict,
        41301 => SdkWorkResultCode::PayloadTooLarge,
        42901 => SdkWorkResultCode::RateLimitExceeded,
        50301 => SdkWorkResultCode::ServiceUnavailable,
        _ => SdkWorkResultCode::InternalError,
    }
}

fn result_code_from_status(status: axum::http::StatusCode) -> SdkWorkResultCode {
    match status.as_u16() {
        400 => SdkWorkResultCode::MalformedRequest,
        401 => SdkWorkResultCode::AuthenticationRequired,
        403 => SdkWorkResultCode::PermissionRequired,
        404 => SdkWorkResultCode::NotFound,
        405 => SdkWorkResultCode::MethodNotAllowed,
        408 => SdkWorkResultCode::RequestTimeout,
        409 => SdkWorkResultCode::Conflict,
        410 => SdkWorkResultCode::Gone,
        412 => SdkWorkResultCode::PreconditionFailed,
        413 => SdkWorkResultCode::PayloadTooLarge,
        415 => SdkWorkResultCode::UnsupportedMediaType,
        422 => SdkWorkResultCode::UnprocessableEntity,
        423 => SdkWorkResultCode::Locked,
        428 => SdkWorkResultCode::PreconditionRequired,
        429 => SdkWorkResultCode::RateLimitExceeded,
        502 => SdkWorkResultCode::BadGateway,
        503 => SdkWorkResultCode::ServiceUnavailable,
        504 => SdkWorkResultCode::GatewayTimeout,
        code if (400..500).contains(&code) => SdkWorkResultCode::ValidationError,
        _ => SdkWorkResultCode::InternalError,
    }
}

/// Correlation and routing fields for RFC 9457 Problem+JSON (`API_SPEC.md` §15.2).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProblemCorrelation<'a> {
    pub request_id: Option<&'a str>,
    pub trace_id: Option<&'a str>,
    pub method: Option<&'a str>,
    pub route_template: Option<&'a str>,
    pub fallback_path: Option<&'a str>,
    pub operation_id: Option<&'a str>,
}

impl<'a> ProblemCorrelation<'a> {
    pub fn new(request_id: Option<&'a str>, trace_id: Option<&'a str>) -> Self {
        Self {
            request_id,
            trace_id,
            ..Self::default()
        }
    }

    pub fn with_routing(
        mut self,
        method: Option<&'a str>,
        route_template: Option<&'a str>,
        fallback_path: Option<&'a str>,
        operation_id: Option<&'a str>,
    ) -> Self {
        self.method = method;
        self.route_template = route_template;
        self.fallback_path = fallback_path;
        self.operation_id = operation_id;
        self
    }

    pub fn resolved_trace_id(&self) -> Option<String> {
        if let Some(trace_id) = self.trace_id.filter(|value| !value.is_empty()) {
            return Some(trace_id.to_owned());
        }
        self.request_id
            .map(|request_id| resolve_problem_trace_id(request_id, None))
    }

    pub fn routing(&self) -> SdkWorkProblemRouting {
        SdkWorkProblemRouting::from_parts(
            self.method,
            self.route_template,
            self.fallback_path,
            self.operation_id,
        )
    }
}

impl<'a> From<Option<&'a str>> for ProblemCorrelation<'a> {
    fn from(trace_id: Option<&'a str>) -> Self {
        Self {
            request_id: None,
            trace_id,
            ..Self::default()
        }
    }
}

/// Client-safe Problem `detail` — internal failures must not leak implementation details.
pub fn client_safe_problem_detail(error: &WebFrameworkError) -> &str {
    match error.kind {
        WebFrameworkErrorKind::InternalServerError | WebFrameworkErrorKind::ContextNotInjected => {
            "An internal error occurred"
        }
        WebFrameworkErrorKind::DependencyUnavailable => {
            "A required dependency is temporarily unavailable"
        }
        _ => &error.message,
    }
}

/// Adds RFC 9457 routing fields to Problem+json bodies when handlers omitted them.
pub fn enrich_problem_detail_value(
    payload: &mut serde_json::Value,
    routing: &SdkWorkProblemRouting,
) {
    fn field_missing(payload: &serde_json::Value, field: &str) -> bool {
        payload
            .get(field)
            .map(|value| value.is_null())
            .unwrap_or(true)
    }

    if field_missing(payload, "instance") {
        if let Some(instance) = routing.instance() {
            payload["instance"] = serde_json::Value::String(instance);
        }
    }
    if field_missing(payload, "operationId") {
        if let Some(operation_id) = routing.operation_id.as_deref() {
            payload["operationId"] = serde_json::Value::String(operation_id.to_owned());
        }
    }
    if field_missing(payload, "i18nKey") {
        // API_SPEC.md §15.2: SDKWork-owned problem details SHOULD include i18nKey
        // when a safe localized message exists. The standard key is errors.result.<code>.
        if let Some(code) = payload.get("code").and_then(|v| v.as_i64()) {
            payload["i18nKey"] = serde_json::Value::String(format!("errors.result.{}", code));
        }
    }
}

/// Enriches an HTTP response body when it is `application/problem+json`.
pub async fn enrich_problem_response(
    correlation: ProblemCorrelation<'_>,
    response: &mut Response,
) -> Result<(), WebFrameworkError> {
    if !is_problem_json(response) {
        return Ok(());
    }

    let (parts, body) = std::mem::replace(response, Response::new(Body::empty())).into_parts();
    let bytes = axum::body::to_bytes(body, PROBLEM_RESPONSE_ENRICHMENT_MAX_BYTES)
        .await
        .map_err(|_| {
            WebFrameworkError::internal_server_error("failed to read problem response body")
        })?;
    if bytes.is_empty() {
        *response = Response::from_parts(parts, Body::from(bytes));
        return Ok(());
    }

    let mut payload: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        WebFrameworkError::internal_server_error(format!("invalid problem response json: {error}"))
    })?;
    enrich_problem_detail_value(&mut payload, &correlation.routing());
    if let Some(trace_id) = correlation.resolved_trace_id() {
        payload["traceId"] = serde_json::Value::String(trace_id);
    }
    let encoded = serde_json::to_vec(&payload).map_err(|error| {
        WebFrameworkError::internal_server_error(format!(
            "failed to encode problem response: {error}"
        ))
    })?;
    *response = Response::from_parts(parts, Body::from(encoded));
    Ok(())
}

/// Normalizes every SDKWork-owned 4xx/5xx response to the standard Problem+json contract.
pub async fn normalize_problem_response(
    correlation: ProblemCorrelation<'_>,
    response: &mut Response,
) -> Result<(), WebFrameworkError> {
    let status = response.status();
    if !status.is_client_error() && !status.is_server_error() {
        return enrich_problem_response(correlation, response).await;
    }
    if is_problem_json(response) {
        if let Err(error) = enrich_problem_response(correlation, response).await {
            replace_with_standard_problem(status, correlation, response);
            return Err(error);
        }
        return Ok(());
    }
    replace_with_standard_problem(status, correlation, response);
    Ok(())
}

fn replace_with_standard_problem(
    status: axum::http::StatusCode,
    correlation: ProblemCorrelation<'_>,
    response: &mut Response,
) {
    let original_headers = response.headers().clone();
    let result_code = result_code_from_status(status);
    let trace_id = correlation
        .resolved_trace_id()
        .unwrap_or_else(|| "unknown".to_owned());
    let mut problem = SdkWorkProblemDetail::platform_enriched(
        result_code,
        result_code.title(),
        trace_id.clone(),
        correlation.routing(),
    );
    problem.status = status.as_u16();
    let mut normalized = (
        status,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(problem),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&trace_id) {
        normalized.headers_mut().insert(
            HeaderName::from_static(crate::constants::SDKWORK_TRACE_ID_HEADER_LOWER),
            value,
        );
    }
    for (name, value) in &original_headers {
        if name == header::CONTENT_TYPE
            || name == header::CONTENT_LENGTH
            || name.as_str() == crate::constants::SDKWORK_TRACE_ID_HEADER_LOWER
        {
            continue;
        }
        normalized.headers_mut().append(name.clone(), value.clone());
    }
    *response = normalized;
}

/// Builds RFC 9457 Problem+json with required correlation and resolved route identity.
pub fn problem_response(
    error: &WebFrameworkError,
    correlation: ProblemCorrelation<'_>,
) -> Response {
    let status = error.status();
    let trace_id = correlation
        .resolved_trace_id()
        .unwrap_or_else(|| "unknown".to_owned());
    let result_code = map_result_code(error.result_code());
    let mut problem = SdkWorkProblemDetail::platform_enriched(
        result_code,
        client_safe_problem_detail(error),
        trace_id.clone(),
        correlation.routing(),
    );
    problem.status = status.as_u16();
    let mut payload = serde_json::to_value(problem).expect("problem detail is serializable");
    if let Some(auth_profile) = error.auth_profile.as_deref() {
        payload["authProfile"] = serde_json::Value::String(auth_profile.to_owned());
    }
    if let Some(failed_stage) = error.failed_stage.as_deref() {
        payload["failedStage"] = serde_json::Value::String(failed_stage.to_owned());
    }
    if let Some(reason) = error.reason.as_deref() {
        payload["reason"] = serde_json::Value::String(reason.to_owned());
    }
    let mut response = (
        status,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(payload),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&trace_id) {
        response.headers_mut().insert(
            HeaderName::from_static(crate::constants::SDKWORK_TRACE_ID_HEADER_LOWER),
            value,
        );
    }
    if let Some(retry_after) = error.retry_after_seconds {
        if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
            response
                .headers_mut()
                .insert(HeaderName::from_static("retry-after"), value);
        }
    }
    response
}

/// Redact numeric/uuid-like path segments for logging (observability spec §10).
pub fn redact_path_template(path: &str) -> String {
    sdkwork_utils_rust::redact_http_path_segments(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::trace_id_from_traceparent;

    #[test]
    fn redacts_numeric_path_segments() {
        assert_eq!(
            "/app/v3/api/users/{id}/orders/{id}",
            redact_path_template("/app/v3/api/users/42/orders/99")
        );
    }

    #[test]
    fn problem_response_sanitizes_internal_errors() {
        let error = WebFrameworkError::internal_server_error("sqlx connection reset by peer");
        let response = problem_response(&error, None.into());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let bytes = rt
            .block_on(async { axum::body::to_bytes(response.into_body(), usize::MAX).await })
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            "An internal error occurred",
            payload["detail"].as_str().unwrap()
        );
        assert!(!payload["detail"].as_str().unwrap().contains("sqlx"));
    }

    #[test]
    fn problem_response_sets_problem_json_content_type() {
        let error = WebFrameworkError::missing_credentials("test");
        let response = problem_response(&error, Some("req-trace-1").into());
        assert_eq!(
            "application/problem+json",
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
        );
    }

    #[test]
    fn auth_problem_codes_and_diagnostics_are_stable_and_redacted() {
        let cases = [
            (WebFrameworkError::missing_credentials("missing"), 40101),
            (WebFrameworkError::expired_credentials("expired"), 40102),
            (WebFrameworkError::invalid_credentials("invalid"), 40103),
            (WebFrameworkError::revoked_credentials("revoked"), 40104),
        ];
        for (error, expected_code) in cases {
            let error = error
                .with_reason("credential-rejected")
                .with_auth_diagnostics(
                    Some("credential-entry-bootstrap"),
                    "request-context-resolution",
                );
            let response = problem_response(
                &error,
                ProblemCorrelation::new(Some("req-auth"), Some("trace-auth")),
            );
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            let bytes = rt
                .block_on(async { axum::body::to_bytes(response.into_body(), usize::MAX).await })
                .expect("body");
            let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("problem json");
            assert_eq!(expected_code, payload["code"].as_i64().unwrap());
            assert_eq!(
                "credential-entry-bootstrap",
                payload["authProfile"].as_str().unwrap()
            );
            assert_eq!(
                "request-context-resolution",
                payload["failedStage"].as_str().unwrap()
            );
            assert_eq!("credential-rejected", payload["reason"].as_str().unwrap());
            let encoded = String::from_utf8(bytes.to_vec()).expect("utf8");
            for secret in ["eyJ", "lookup_key", "upstream.internal"] {
                assert!(!encoded.contains(secret));
            }
        }
    }

    #[test]
    fn problem_response_includes_trace_id() {
        let traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let error = WebFrameworkError::forbidden("denied");
        let response = problem_response(
            &error,
            ProblemCorrelation::new(Some("req-trace"), trace_id_from_traceparent(traceparent)),
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let bytes = rt
            .block_on(async { axum::body::to_bytes(response.into_body(), usize::MAX).await })
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(payload.get("requestId").is_none());
        assert_eq!(40301, payload["code"].as_i64().unwrap());
        assert_eq!(
            "4bf92f3577b34da6a3ce929d0e0e4736",
            payload["traceId"].as_str().unwrap()
        );
    }

    #[test]
    fn problem_response_includes_instance_and_operation_id() {
        let error = WebFrameworkError::internal_server_error("db down");
        let response = problem_response(
            &error,
            ProblemCorrelation::new(Some("req-trace"), Some("trace-abc")).with_routing(
                Some("GET"),
                Some("/app/v3/api/wallet/transactions"),
                None,
                Some("wallet.transactions.list"),
            ),
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let bytes = rt
            .block_on(async { axum::body::to_bytes(response.into_body(), usize::MAX).await })
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            "GET /app/v3/api/wallet/transactions",
            payload["instance"].as_str().unwrap()
        );
        assert_eq!(
            "wallet.transactions.list",
            payload["operationId"].as_str().unwrap()
        );
        assert_eq!(
            "https://docs.sdkwork.com/problems/50001",
            payload["type"].as_str().unwrap()
        );
    }

    #[tokio::test]
    async fn enrich_problem_response_adds_missing_routing_fields() {
        let routing = SdkWorkProblemRouting::from_parts(
            Some("GET"),
            Some("/app/v3/api/wallet/transactions"),
            None,
            Some("wallet.transactions.list"),
        );
        let mut payload = serde_json::json!({
            "type": "https://docs.sdkwork.com/problems/50001",
            "title": "Internal server error",
            "status": 500,
            "code": 50001,
            "traceId": "handler-local-trace",
            "detail": "An internal error occurred"
        });
        enrich_problem_detail_value(&mut payload, &routing);
        assert_eq!(
            "GET /app/v3/api/wallet/transactions",
            payload["instance"].as_str().unwrap()
        );
        assert_eq!(
            "wallet.transactions.list",
            payload["operationId"].as_str().unwrap()
        );
        assert_eq!("handler-local-trace", payload["traceId"].as_str().unwrap());

        let correlation = ProblemCorrelation::new(Some("req-1"), Some("trace-abc")).with_routing(
            Some("GET"),
            Some("/app/v3/api/wallet/transactions"),
            None,
            Some("wallet.transactions.list"),
        );
        let mut response = (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "application/problem+json")],
            axum::Json(payload),
        )
            .into_response();
        enrich_problem_response(correlation, &mut response)
            .await
            .expect("enrich");
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let enriched: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            "wallet.transactions.list",
            enriched["operationId"].as_str().unwrap()
        );
        assert_eq!("trace-abc", enriched["traceId"].as_str().unwrap());
    }

    #[test]
    fn problem_type_uri_is_stable() {
        let error = WebFrameworkError::conflict("dup");
        let response = problem_response(&error, None.into());
        assert!(response.headers().get(header::CONTENT_TYPE).is_some());
    }
}
