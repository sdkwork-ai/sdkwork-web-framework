use axum::extract::Request;
use axum::response::Response;
use sdkwork_web_core::{
    new_request_id, problem_response, trace_id_from_traceparent, ProblemCorrelation,
    WebFrameworkError, WebRequestContext, REQUEST_ID_HEADER, TRACEPARENT_HEADER,
};

fn read_header(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedProblemCorrelation {
    pub request_id: String,
    pub trace_id: Option<String>,
    pub method: Option<String>,
    pub route_template: Option<String>,
    pub fallback_path: Option<String>,
    pub operation_id: Option<String>,
}

impl OwnedProblemCorrelation {
    pub fn from_request(request: &Request) -> Self {
        if let Some(context) = request.extensions().get::<WebRequestContext>() {
            let correlation = context.problem_correlation();
            return Self {
                request_id: context.request_id.0.clone(),
                trace_id: correlation.trace_id.map(str::to_owned),
                method: correlation.method.map(str::to_owned),
                route_template: correlation.route_template.map(str::to_owned),
                fallback_path: correlation.fallback_path.map(str::to_owned),
                operation_id: correlation.operation_id.map(str::to_owned),
            };
        }
        let request_id =
            read_header(request.headers(), REQUEST_ID_HEADER).unwrap_or_else(new_request_id);
        let trace_id = read_header(request.headers(), TRACEPARENT_HEADER)
            .and_then(|traceparent| trace_id_from_traceparent(&traceparent).map(str::to_owned));
        Self {
            request_id,
            trace_id,
            method: Some(request.method().to_string()),
            route_template: None,
            fallback_path: Some(request.uri().path().to_owned()),
            operation_id: None,
        }
    }

    pub fn as_correlation(&self) -> ProblemCorrelation<'_> {
        ProblemCorrelation::new(Some(self.request_id.as_str()), self.trace_id.as_deref())
            .with_routing(
                self.method.as_deref(),
                self.route_template.as_deref(),
                self.fallback_path.as_deref(),
                self.operation_id.as_deref(),
            )
    }
}

/// Build a Problem+json response using request-scoped correlation identifiers.
pub fn problem_response_for_request(error: &WebFrameworkError, request: &Request) -> Response {
    let correlation = OwnedProblemCorrelation::from_request(request);
    problem_response(error, correlation.as_correlation())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    #[test]
    fn includes_trace_id_from_traceparent_header() {
        let request = Request::builder()
            .header(
                TRACEPARENT_HEADER,
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            )
            .header(REQUEST_ID_HEADER, "req-timeout-1")
            .body(Body::empty())
            .expect("request");
        let response = problem_response_for_request(
            &WebFrameworkError::request_timeout("request timed out"),
            &request,
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
            "4bf92f3577b34da6a3ce929d0e0e4736",
            payload["traceId"].as_str().unwrap()
        );
        assert!(payload.get("requestId").is_none());
    }
}
