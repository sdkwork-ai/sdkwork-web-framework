use crate::error::WebFrameworkError;
use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, Method};
use axum::response::Response;
use percent_encoding::percent_decode_str;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorsPolicy {
    pub allow_all_origins: bool,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<Method>,
    pub allowed_headers: Vec<String>,
    pub allow_credentials: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossSiteRequestPolicy {
    pub reject_untrusted_state_changing_origins: bool,
    /// Reject state-changing requests that carry session cookies without Origin/Referer (CSRF, catalog C3).
    pub reject_cookie_auth_without_origin: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeaderSecurityPolicy {
    pub content_type_options: bool,
    pub frame_options_deny: bool,
    pub referrer_policy: Option<String>,
    pub permissions_policy: Option<String>,
    pub strict_transport_security: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodGuardPolicy {
    pub allowed_methods: Vec<Method>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestSizeLimitPolicy {
    pub max_content_length: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitPolicy {
    pub enabled: bool,
    pub max_requests_per_window: u32,
    pub window_secs: u64,
    /// Stage 8 — anonymous/credential fingerprint limit before auth resolution.
    pub pre_auth_rate_limit: bool,
    /// After authentication, apply an additional tenant-scoped limit (stage 12).
    pub tenant_limit_after_auth: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyPolicy {
    pub require_for_retryable_commands: bool,
    pub retention_secs: u64,
    pub max_cached_response_bytes: u64,
    /// When true, POST/PUT/PATCH with Content-Length > 0 require X-Content-SHA256 (D6).
    pub require_body_hash_for_payload: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqlInjectionGuardPolicy {
    pub enabled: bool,
    pub inspected_headers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct JsonContentTypePolicy {
    /// When true, state-changing requests with a body must declare `application/json`.
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebSocketPolicy {
    pub max_message_bytes: Option<u64>,
    pub message_rate_limit_enabled: bool,
    pub max_messages_per_window: u32,
    pub message_window_secs: u64,
}

impl Default for WebSocketPolicy {
    fn default() -> Self {
        Self {
            max_message_bytes: Some(1024 * 1024),
            message_rate_limit_enabled: true,
            max_messages_per_window: 120,
            message_window_secs: 60,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SecurityPolicy {
    pub cors: CorsPolicy,
    pub cross_site: CrossSiteRequestPolicy,
    pub header_security: HeaderSecurityPolicy,
    pub method_guard: MethodGuardPolicy,
    pub request_size_limit: RequestSizeLimitPolicy,
    pub rate_limit: RateLimitPolicy,
    pub idempotency: IdempotencyPolicy,
    pub sql_injection_guard: SqlInjectionGuardPolicy,
    pub json_content_type: JsonContentTypePolicy,
    pub websocket: WebSocketPolicy,
}

pub trait RequestSecurityPolicy {
    fn validate_request(&self, request: &Request) -> Result<(), WebFrameworkError>;
}

impl Default for CorsPolicy {
    fn default() -> Self {
        Self {
            allow_all_origins: false,
            allowed_origins: Vec::new(),
            allowed_methods: vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ],
            allowed_headers: vec![
                "authorization".to_owned(),
                "access-token".to_owned(),
                "content-type".to_owned(),
                "idempotency-key".to_owned(),
                "x-api-key".to_owned(),
            ],
            allow_credentials: true,
        }
    }
}

impl Default for CrossSiteRequestPolicy {
    fn default() -> Self {
        Self {
            reject_untrusted_state_changing_origins: true,
            reject_cookie_auth_without_origin: true,
        }
    }
}

impl Default for HeaderSecurityPolicy {
    fn default() -> Self {
        Self {
            content_type_options: true,
            frame_options_deny: true,
            referrer_policy: Some("no-referrer".to_owned()),
            permissions_policy: Some("geolocation=(), microphone=(), camera=()".to_owned()),
            strict_transport_security: None,
        }
    }
}

impl Default for MethodGuardPolicy {
    fn default() -> Self {
        Self {
            allowed_methods: vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ],
        }
    }
}

impl Default for RequestSizeLimitPolicy {
    fn default() -> Self {
        Self {
            max_content_length: Some(16 * 1024 * 1024),
        }
    }
}

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            max_requests_per_window: 120,
            window_secs: 60,
            pre_auth_rate_limit: true,
            tenant_limit_after_auth: true,
        }
    }
}

impl Default for IdempotencyPolicy {
    fn default() -> Self {
        Self {
            require_for_retryable_commands: false,
            retention_secs: 86_400,
            max_cached_response_bytes: 1024 * 1024,
            require_body_hash_for_payload: true,
        }
    }
}

impl Default for SqlInjectionGuardPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            // 扫描所有 inbound 凭证/上下文头，避免攻击者经未扫描头注入 SQL。
            // SECURITY_SPEC §5.1 / OWASP API8。
            inspected_headers: vec![
                "x-api-key".to_owned(),
                "authorization".to_owned(),
                "access-token".to_owned(),
                "idempotency-key".to_owned(),
                "x-idempotency-fingerprint".to_owned(),
                "x-content-sha256".to_owned(),
                "x-sdkwork-agent-token".to_owned(),
                "cookie".to_owned(),
                "referer".to_owned(),
                "x-forwarded-for".to_owned(),
                "x-real-ip".to_owned(),
                "user-agent".to_owned(),
            ],
        }
    }
}

impl CorsPolicy {
    /// Rejects permissive CORS combinations unsafe for production (catalog C1 / maturity §3.2).
    pub fn validate_for_production(&self) -> Result<(), String> {
        if self.allow_all_origins {
            return Err(
                "production CORS policy must not set allow_all_origins; configure an explicit origin allowlist"
                    .into(),
            );
        }
        Ok(())
    }

    pub fn validate_origin(&self, request: &Request) -> Result<(), WebFrameworkError> {
        let Some(origin) = request
            .headers()
            .get("origin")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        self.validate_origin_value(origin)
    }

    /// Validates a browser origin value against this policy (CORS allowlist).
    pub fn validate_origin_value(&self, origin: &str) -> Result<(), WebFrameworkError> {
        if self.allow_all_origins || self.allowed_origins.iter().any(|allowed| allowed == origin) {
            return Ok(());
        }
        Err(WebFrameworkError::forbidden(
            "CORS origin is not allowed by API policy",
        ))
    }

    pub fn apply_headers_from_origin(&self, origin: Option<&str>, response: &mut Response) {
        let Some(origin) = origin.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };
        if !(self.allow_all_origins || self.allowed_origins.iter().any(|allowed| allowed == origin))
        {
            return;
        }
        if let Ok(value) = HeaderValue::from_str(origin) {
            response.headers_mut().insert(
                HeaderName::from_static("access-control-allow-origin"),
                value,
            );
        }
        if self.allow_credentials {
            response.headers_mut().insert(
                HeaderName::from_static("access-control-allow-credentials"),
                HeaderValue::from_static("true"),
            );
        }
        if let Ok(value) = HeaderValue::from_str(
            &self
                .allowed_headers
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        ) {
            response.headers_mut().insert(
                HeaderName::from_static("access-control-allow-headers"),
                value,
            );
        }
        if let Ok(value) = HeaderValue::from_str(
            &self
                .allowed_methods
                .iter()
                .map(|method| method.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ) {
            response.headers_mut().insert(
                HeaderName::from_static("access-control-allow-methods"),
                value,
            );
        }
    }
}

impl RequestSecurityPolicy for SecurityPolicy {
    fn validate_request(&self, request: &Request) -> Result<(), WebFrameworkError> {
        self.validate_method(request)?;
        self.validate_content_length(request)?;
        self.validate_cors(request)?;
        self.validate_cross_site_request(request)?;
        self.validate_sql_injection(request)
    }
}

impl SecurityPolicy {
    pub fn reject_client_identity_projection(
        &self,
        headers: &axum::http::HeaderMap,
    ) -> Result<(), WebFrameworkError> {
        for name in crate::constants::FORBIDDEN_CLIENT_IDENTITY_HEADERS {
            if headers.contains_key(*name) {
                return Err(WebFrameworkError::bad_request(format!(
                    "client must not send identity projection header {name}"
                )));
            }
        }
        Ok(())
    }

    /// Rejects inbound credential and projection headers on credential-entry routes.
    pub fn reject_credential_entry_headers(
        headers: &axum::http::HeaderMap,
    ) -> Result<(), WebFrameworkError> {
        for name in crate::constants::FORBIDDEN_CREDENTIAL_ENTRY_HEADERS {
            if headers.contains_key(*name) {
                return Err(WebFrameworkError::bad_request(format!(
                    "credential-entry routes must not receive inbound header {name}"
                )));
            }
        }
        for name in crate::constants::FORBIDDEN_CLIENT_IDENTITY_HEADERS {
            if headers.contains_key(*name) {
                return Err(WebFrameworkError::bad_request(format!(
                    "credential-entry routes must not receive identity projection header {name}"
                )));
            }
        }
        Ok(())
    }

    pub fn validate_method(&self, request: &Request) -> Result<(), WebFrameworkError> {
        if self.method_guard.allowed_methods.contains(request.method()) {
            return Ok(());
        }
        Err(WebFrameworkError::method_not_allowed(format!(
            "HTTP method {} is not allowed",
            request.method()
        )))
    }

    pub fn validate_content_length(&self, request: &Request) -> Result<(), WebFrameworkError> {
        self.validate_content_length_with_limit(request, self.request_size_limit.max_content_length)
    }

    pub fn validate_content_length_with_limit(
        &self,
        request: &Request,
        limit: Option<u64>,
    ) -> Result<(), WebFrameworkError> {
        let Some(limit) = limit else {
            return Ok(());
        };
        let Some(value) = request
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
        else {
            if (request.method() == axum::http::Method::POST
                || request.method() == axum::http::Method::PUT
                || request.method() == axum::http::Method::PATCH)
                && request.headers().contains_key("transfer-encoding")
            {
                return Err(WebFrameworkError::bad_request(
                    "requests with transfer-encoding must include content-length for API size policy enforcement",
                ));
            }
            return Ok(());
        };
        if value <= limit {
            return Ok(());
        }
        Err(WebFrameworkError::payload_too_large(
            "request content length exceeds API policy",
        ))
    }

    pub fn validate_cors(&self, request: &Request) -> Result<(), WebFrameworkError> {
        self.cors.validate_origin(request)
    }

    pub fn validate_cors_policy(
        cors: &CorsPolicy,
        request: &Request,
    ) -> Result<(), WebFrameworkError> {
        cors.validate_origin(request)
    }

    pub fn validate_cross_site_request(&self, request: &Request) -> Result<(), WebFrameworkError> {
        Self::validate_cross_site_request_with_cors(&self.cross_site, &self.cors, request, false)
    }

    pub fn validate_cross_site_request_with_cors(
        cross_site: &CrossSiteRequestPolicy,
        cors: &CorsPolicy,
        request: &Request,
        skip_origin_rejection: bool,
    ) -> Result<(), WebFrameworkError> {
        if !cross_site.reject_untrusted_state_changing_origins
            && !cross_site.reject_cookie_auth_without_origin
        {
            return Ok(());
        }
        if !is_state_changing_method(request.method()) {
            return Ok(());
        }
        let has_cookie = request.headers().contains_key(axum::http::header::COOKIE);
        // Cookie-authenticated browser flows always require CORS origin validation (P10/P3).
        let enforce_cors_origin = cross_site.reject_untrusted_state_changing_origins
            && (!skip_origin_rejection || has_cookie);
        if enforce_cors_origin {
            cors.validate_origin(request)?;
        }
        if cross_site.reject_cookie_auth_without_origin && has_cookie {
            validate_cookie_authenticated_source(cors, request)?;
        }
        Ok(())
    }

    pub fn validate_json_content_type(&self, request: &Request) -> Result<(), WebFrameworkError> {
        if !self.json_content_type.enabled {
            return Ok(());
        }
        if !is_state_changing_method(request.method()) {
            return Ok(());
        }
        let has_body = request
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|length| length > 0)
            || request.headers().contains_key("transfer-encoding");
        if !has_body {
            return Ok(());
        }
        let content_type = request
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        let mime = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if mime == "application/json" {
            return Ok(());
        }
        Err(WebFrameworkError::bad_request(
            "state-changing JSON API requests require Content-Type: application/json",
        ))
    }

    pub fn validate_sql_injection(&self, request: &Request) -> Result<(), WebFrameworkError> {
        if !self.sql_injection_guard.enabled {
            return Ok(());
        }
        // URL-decode path 与 query 后再匹配，避免 `%27%20or%20` 绕过。
        // SECURITY_SPEC §5.1 / OWASP API8。
        let raw_path = request.uri().path();
        let decoded_path = percent_decode_str(raw_path)
            .decode_utf8_lossy()
            .into_owned();
        let raw_query = request.uri().query().unwrap_or_default();
        let decoded_query = percent_decode_str(raw_query)
            .decode_utf8_lossy()
            .into_owned();
        let mut inspected = vec![decoded_path, decoded_query];
        for header in &self.sql_injection_guard.inspected_headers {
            if let Some(value) = request
                .headers()
                .get(header.as_str())
                .and_then(|value| value.to_str().ok())
            {
                inspected.push(value.to_owned());
            }
        }
        if inspected
            .iter()
            .any(|value| contains_sql_injection_signal(value))
        {
            return Err(WebFrameworkError::bad_request(
                "request contains blocked SQL injection pattern",
            ));
        }
        Ok(())
    }

    pub fn apply_response_headers(&self, response: &mut Response) {
        if self.header_security.content_type_options {
            response.headers_mut().insert(
                HeaderName::from_static("x-content-type-options"),
                HeaderValue::from_static("nosniff"),
            );
        }
        if self.header_security.frame_options_deny {
            response.headers_mut().insert(
                HeaderName::from_static("x-frame-options"),
                HeaderValue::from_static("DENY"),
            );
        }
        if let Some(value) = &self.header_security.referrer_policy {
            if let Ok(value) = HeaderValue::from_str(value) {
                response
                    .headers_mut()
                    .insert(HeaderName::from_static("referrer-policy"), value);
            }
        }
        if let Some(value) = &self.header_security.permissions_policy {
            if let Ok(value) = HeaderValue::from_str(value) {
                response
                    .headers_mut()
                    .insert(HeaderName::from_static("permissions-policy"), value);
            }
        }
        if let Some(value) = &self.header_security.strict_transport_security {
            if let Ok(value) = HeaderValue::from_str(value) {
                response
                    .headers_mut()
                    .insert(HeaderName::from_static("strict-transport-security"), value);
            }
        }
    }

    pub fn apply_cors_headers(&self, request: &Request, response: &mut Response) {
        let origin = request
            .headers()
            .get("origin")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        self.cors.apply_headers_from_origin(origin, response);
        self.insert_cors_allow_methods(response);
    }

    pub fn apply_cors_headers_from_origin(&self, origin: Option<&str>, response: &mut Response) {
        self.cors.apply_headers_from_origin(origin, response);
        self.insert_cors_allow_methods(response);
    }

    pub fn apply_cors_policy_headers_from_origin(
        cors: &CorsPolicy,
        origin: Option<&str>,
        response: &mut Response,
    ) {
        cors.apply_headers_from_origin(origin, response);
        if let Ok(value) = HeaderValue::from_str(
            &cors
                .allowed_methods
                .iter()
                .map(|method| method.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ) {
            response.headers_mut().insert(
                HeaderName::from_static("access-control-allow-methods"),
                value,
            );
        }
    }

    /// Stricter defaults for production SaaS deployments.
    pub fn production() -> Self {
        Self {
            cors: CorsPolicy {
                allow_all_origins: false,
                ..CorsPolicy::default()
            },
            cross_site: CrossSiteRequestPolicy::default(),
            header_security: HeaderSecurityPolicy {
                strict_transport_security: Some("max-age=31536000; includeSubDomains".to_owned()),
                ..HeaderSecurityPolicy::default()
            },
            rate_limit: RateLimitPolicy {
                enabled: true,
                max_requests_per_window: 120,
                window_secs: 60,
                pre_auth_rate_limit: true,
                tenant_limit_after_auth: true,
            },
            json_content_type: JsonContentTypePolicy { enabled: true },
            ..Self::default()
        }
    }

    fn insert_cors_allow_methods(&self, response: &mut Response) {
        if let Ok(value) = HeaderValue::from_str(
            &self
                .cors
                .allowed_methods
                .iter()
                .map(|method| method.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ) {
            response.headers_mut().insert(
                HeaderName::from_static("access-control-allow-methods"),
                value,
            );
        }
    }
}

fn validate_cookie_authenticated_source(
    cors: &CorsPolicy,
    request: &Request,
) -> Result<(), WebFrameworkError> {
    let origin = request
        .headers()
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            request
                .headers()
                .get("referer")
                .and_then(|value| value.to_str().ok())
                .and_then(extract_origin_from_referer)
        });
    let Some(origin) = origin else {
        return Err(WebFrameworkError::forbidden(
            "state-changing cookie-authenticated requests require Origin or Referer",
        ));
    };
    cors.validate_origin_value(&origin)
}

fn extract_origin_from_referer(referer: &str) -> Option<String> {
    let trimmed = referer.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let scheme_end = without_fragment.find("://")?;
    let scheme = &without_fragment[..scheme_end];
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority_start = scheme_end + 3;
    let authority = without_fragment[authority_start..]
        .split(['/', '?'])
        .next()
        .filter(|value| !value.is_empty())?;
    Some(format!("{scheme}://{authority}"))
}

fn is_state_changing_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn contains_sql_injection_signal(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    [
        "' or ",
        "\" or ",
        " union select ",
        " union all select ",
        " drop table ",
        " truncate table ",
        " information_schema",
        " sleep(",
        " benchmark(",
        "--",
        "/*",
        "*/",
        " xp_",
    ]
    .iter()
    .any(|pattern| lowered.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::WebFrameworkErrorKind;
    use axum::body::Body;

    #[test]
    fn extract_origin_from_referer_parses_https_authority() {
        assert_eq!(
            Some("https://trusted.example".to_owned()),
            extract_origin_from_referer("https://trusted.example/path?x=1#frag")
        );
    }

    #[test]
    fn validate_for_production_rejects_allow_all_origins() {
        let policy = CorsPolicy {
            allow_all_origins: true,
            ..CorsPolicy::default()
        };
        let error = policy
            .validate_for_production()
            .expect_err("allow_all_origins must be rejected in production");
        assert!(error.contains("allow_all_origins"));
    }

    #[test]
    fn validate_for_production_accepts_explicit_allowlist() {
        let policy = CorsPolicy {
            allow_all_origins: false,
            allowed_origins: vec!["https://app.example".to_owned()],
            ..CorsPolicy::default()
        };
        policy
            .validate_for_production()
            .expect("explicit allowlist is production-safe");
    }

    #[test]
    fn validate_cookie_authenticated_source_rejects_untrusted_referer_origin() {
        let cors = CorsPolicy {
            allowed_origins: vec!["https://trusted.example".to_owned()],
            ..CorsPolicy::default()
        };
        let request = Request::builder()
            .method("POST")
            .uri("/app/v3/api/users")
            .header("cookie", "session=abc")
            .header("referer", "https://attacker.example/evil")
            .body(Body::empty())
            .expect("request");
        let error = validate_cookie_authenticated_source(&cors, &request).expect_err("referer");
        assert_eq!(WebFrameworkErrorKind::Forbidden, error.kind);
    }
}
