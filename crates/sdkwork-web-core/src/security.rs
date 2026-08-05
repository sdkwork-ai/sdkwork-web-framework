use crate::error::WebFrameworkError;
use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, Method, Uri};
use axum::response::Response;
use percent_encoding::percent_decode_str;
use sdkwork_web_contract::RouteAuth;
use std::net::IpAddr;

const DEVELOPMENT_PRIVATE_NETWORK_HTTP_ORIGIN: &str = "http://private-network:*";
const DEVELOPMENT_PRIVATE_NETWORK_HTTPS_ORIGIN: &str = "https://private-network:*";

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
                "x-content-sha256".to_owned(),
                "x-idempotency-fingerprint".to_owned(),
                "x-api-key".to_owned(),
                "x-sdkwork-access-token".to_owned(),
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
                "x-sdkwork-access-token".to_owned(),
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
        if self
            .allowed_origins
            .iter()
            .any(|origin| origin.ends_with(":*"))
        {
            return Err(
                "production CORS policy must not use port wildcard origins; configure exact origins"
                    .into(),
            );
        }
        if self.allowed_headers.iter().any(|header| header == "*") {
            return Err(
                "production CORS policy must not allow all preflight request headers; configure an explicit header allowlist"
                    .into(),
            );
        }
        Ok(())
    }

    /// Development policy for browser apps running on arbitrary local dev-server ports.
    ///
    /// The wildcard syntax is intentionally restricted to loopback hosts and is rejected
    /// by `validate_for_production`.
    pub fn development_loopback() -> Self {
        Self {
            allowed_origins: vec![
                "http://localhost:*".to_owned(),
                "http://127.0.0.1:*".to_owned(),
                "http://[::1]:*".to_owned(),
                "https://localhost:*".to_owned(),
                "https://127.0.0.1:*".to_owned(),
                "https://[::1]:*".to_owned(),
            ],
            ..Self::default()
        }
    }

    /// Development policy for browser apps served from loopback or dynamically assigned
    /// private-network IP addresses on arbitrary numeric dev-server ports.
    ///
    /// The private-network markers are framework directives, never response origins, and are
    /// rejected by `validate_for_production` through the production wildcard guard.
    pub fn development_private_network() -> Self {
        let mut policy = Self::development_loopback();
        policy.allowed_origins.extend([
            DEVELOPMENT_PRIVATE_NETWORK_HTTP_ORIGIN.to_owned(),
            DEVELOPMENT_PRIVATE_NETWORK_HTTPS_ORIGIN.to_owned(),
        ]);
        policy
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

    /// Validates the method and headers requested by a browser CORS preflight.
    pub fn validate_preflight(&self, request: &Request) -> Result<(), WebFrameworkError> {
        let requested_method = request
            .headers()
            .get("access-control-request-method")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<Method>().ok())
            .ok_or_else(|| {
                WebFrameworkError::forbidden("CORS preflight request method is invalid or missing")
            })?;
        if !self.allowed_methods.contains(&requested_method) {
            return Err(WebFrameworkError::forbidden(
                "CORS preflight request method is not allowed by API policy",
            ));
        }

        // `*` in the header allowlist relaxes the per-header gate entirely.
        // Development/loopback policies use it so local browser surfaces never
        // fail preflight because the SDK grows a new request header; production
        // policies are rejected by `validate_for_production`.
        if self.allowed_headers.iter().any(|allowed| allowed == "*") {
            return Ok(());
        }

        let Some(requested_headers) = request.headers().get("access-control-request-headers")
        else {
            return Ok(());
        };
        let requested_headers = requested_headers.to_str().map_err(|_| {
            WebFrameworkError::forbidden("CORS preflight request headers are invalid")
        })?;
        for requested in requested_headers
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !self
                .allowed_headers
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(requested))
            {
                return Err(WebFrameworkError::forbidden(
                    "CORS preflight request header is not allowed by API policy",
                ));
            }
        }
        Ok(())
    }

    /// Validates a browser origin value against this policy (CORS allowlist).
    pub fn validate_origin_value(&self, origin: &str) -> Result<(), WebFrameworkError> {
        if self.allows_origin(origin) {
            return Ok(());
        }
        Err(WebFrameworkError::forbidden(
            "CORS origin is not allowed by API policy",
        ))
    }

    pub fn allows_origin_value(&self, origin: &str) -> bool {
        self.allows_origin(origin)
    }

    pub fn apply_headers_from_origin(&self, origin: Option<&str>, response: &mut Response) {
        let Some(origin) = origin.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };
        if !self.allows_origin(origin) {
            return;
        }
        if let Ok(value) = HeaderValue::from_str(origin) {
            response.headers_mut().insert(
                HeaderName::from_static("access-control-allow-origin"),
                value,
            );
            merge_vary_origin(response);
        }
        if self.allow_credentials {
            response.headers_mut().insert(
                HeaderName::from_static("access-control-allow-credentials"),
                HeaderValue::from_static("true"),
            );
        }
        let allow_headers_value = if self.allowed_headers.iter().any(|allowed| allowed == "*") {
            // The wildcard is honored by browsers for requests whose
            // credentials mode is not `include`; the SDKWork clients use the
            // default (same-origin) credentials mode and send tokens via
            // headers, so `*` matches their preflights. Production policies
            // are rejected by `validate_for_production`.
            "*".to_owned()
        } else {
            self.allowed_headers
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        };
        if let Ok(value) = HeaderValue::from_str(&allow_headers_value) {
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

    fn allows_origin(&self, origin: &str) -> bool {
        self.allow_all_origins
            || self
                .allowed_origins
                .iter()
                .any(|allowed| origin_matches_allowed(allowed, origin))
    }
}

fn origin_matches_allowed(allowed: &str, origin: &str) -> bool {
    if allowed == origin {
        return true;
    }

    if matches!(
        allowed,
        DEVELOPMENT_PRIVATE_NETWORK_HTTP_ORIGIN | DEVELOPMENT_PRIVATE_NETWORK_HTTPS_ORIGIN
    ) {
        let required_scheme = if allowed == DEVELOPMENT_PRIVATE_NETWORK_HTTP_ORIGIN {
            "http"
        } else {
            "https"
        };
        return is_development_private_network_origin_with_scheme(origin, required_scheme);
    }

    let Some(base) = allowed.strip_suffix(":*") else {
        return false;
    };
    if !matches!(
        base,
        "http://localhost"
            | "http://127.0.0.1"
            | "http://[::1]"
            | "https://localhost"
            | "https://127.0.0.1"
            | "https://[::1]"
    ) {
        return false;
    }

    let Some(port) = origin
        .strip_prefix(base)
        .and_then(|suffix| suffix.strip_prefix(':'))
    else {
        return origin == base;
    };
    !port.is_empty() && port.bytes().all(|value| value.is_ascii_digit())
}

/// Returns whether an Origin is an HTTP(S) loopback/private-network IP origin.
/// Hostnames and public IP addresses are intentionally excluded.
pub fn is_development_private_network_origin(origin: &str) -> bool {
    is_development_private_network_origin_with_scheme(origin, "http")
        || is_development_private_network_origin_with_scheme(origin, "https")
}

fn is_development_private_network_origin_with_scheme(origin: &str, scheme: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    if uri.scheme_str() != Some(scheme)
        || uri.authority().is_none()
        || uri.path() != "/"
        || uri.query().is_some()
    {
        return false;
    }
    let Some(host) = uri.host() else {
        return false;
    };
    let normalized_host = host.trim_start_matches('[').trim_end_matches(']');
    let Ok(ip) = normalized_host.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(address) => address.is_private() || address.is_loopback(),
        IpAddr::V6(address) => address.is_unique_local() || address.is_loopback(),
    }
}

fn merge_vary_origin(response: &mut Response) {
    let headers = response.headers_mut();
    let Some(existing) = headers.get("vary") else {
        headers.insert(
            HeaderName::from_static("vary"),
            HeaderValue::from_static("Origin"),
        );
        return;
    };
    let Ok(existing_text) = existing.to_str() else {
        headers.insert(
            HeaderName::from_static("vary"),
            HeaderValue::from_static("Origin"),
        );
        return;
    };
    if existing_text
        .split(',')
        .any(|value| value.trim().eq_ignore_ascii_case("origin"))
    {
        return;
    }
    if let Ok(value) = HeaderValue::from_str(&format!("{existing_text}, Origin")) {
        headers.insert(HeaderName::from_static("vary"), value);
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
    /// Enforces the credential-header allowlist selected by the matched route profile.
    pub fn validate_route_auth_credentials(
        route: &sdkwork_web_contract::HttpRoute,
        headers: &axum::http::HeaderMap,
    ) -> Result<(), WebFrameworkError> {
        let route_auth = route.auth;
        if route_auth == RouteAuth::Compatibility {
            let compatibility = route.compatibility_auth.as_ref().ok_or_else(|| {
                WebFrameworkError::bad_request(
                    "compatibility route is missing external authentication metadata",
                )
                .with_reason("invalid-compatibility-auth-contract")
            })?;
            compatibility.validate().map_err(|message| {
                WebFrameworkError::bad_request(message)
                    .with_reason("invalid-compatibility-auth-contract")
            })?;
            for name in crate::constants::STANDARD_CREDENTIAL_HEADERS {
                if headers.contains_key(*name)
                    && !compatibility
                        .schemes
                        .iter()
                        .any(|scheme| {
                            match scheme.kind {
                        sdkwork_web_contract::CompatibilitySecuritySchemeKind::ApiKeyHeader {
                            header_name,
                        } => header_name.eq_ignore_ascii_case(name),
                        sdkwork_web_contract::CompatibilitySecuritySchemeKind::HttpBearer {
                            ..
                        } => *name == "authorization",
                    }
                        })
                {
                    return Err(WebFrameworkError::bad_request(format!(
                        "compatibility routes must not receive undeclared credential header {name}"
                    ))
                    .with_reason("credential-profile-contamination"));
                }
            }
            return Ok(());
        }
        Self::validate_standard_route_auth_credentials(route_auth, headers)
    }

    fn validate_standard_route_auth_credentials(
        route_auth: RouteAuth,
        headers: &axum::http::HeaderMap,
    ) -> Result<(), WebFrameworkError> {
        let allowed: &[&str] = match route_auth {
            RouteAuth::Public | RouteAuth::BootstrapBody | RouteAuth::RefreshToken => &[],
            RouteAuth::CredentialEntryBootstrap => &["access-token"],
            RouteAuth::DualToken => &["authorization", "access-token"],
            RouteAuth::ApiKey => &["x-api-key"],
            RouteAuth::OAuth => &["authorization"],
            RouteAuth::OpenApiFlexible => &["authorization", "x-api-key"],
            RouteAuth::OpenApiBearerFlexible => &["authorization", "x-api-key"],
            RouteAuth::ApiKeyOrDualToken => &["authorization", "access-token", "x-api-key"],
            RouteAuth::IngressToken => &["x-sdkwork-ingress-token", "access-token"],
            RouteAuth::AgentToken => &["x-sdkwork-agent-token", "access-token"],
            RouteAuth::Compatibility => unreachable!("handled by route metadata validation"),
        };
        for name in crate::constants::STANDARD_CREDENTIAL_HEADERS {
            if headers.contains_key(*name) && !allowed.contains(name) {
                return Err(WebFrameworkError::bad_request(format!(
                    "{} routes must not receive credential header {name}",
                    route_auth.auth_profile_label(),
                ))
                .with_reason("credential-profile-contamination"));
            }
        }
        if route_auth == RouteAuth::ApiKeyOrDualToken {
            let has_api_key = headers.contains_key("x-api-key");
            let has_auth_token = headers.contains_key("authorization");
            let has_access_token = headers.contains_key("access-token");
            if has_api_key && (has_auth_token || has_access_token) {
                return Err(WebFrameworkError::bad_request(
                    "api-key-or-dual-token routes require either X-API-Key or the dual-token pair, never both",
                )
                .with_reason("credential-profile-contamination"));
            }
            if has_auth_token != has_access_token {
                return Err(WebFrameworkError::bad_request(
                    "the dual-token branch requires both Authorization and Access-Token",
                )
                .with_reason("incomplete-credential-profile"));
            }
        }
        Ok(())
    }

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
        Self::validate_standard_route_auth_credentials(
            RouteAuth::CredentialEntryBootstrap,
            headers,
        )?;
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
    const CORE_PATTERNS: &[&str] = &[
        "' or ",
        "\" or ",
        " union select ",
        " union all select ",
        " drop table ",
        " truncate table ",
        " information_schema",
        " sleep(",
        " benchmark(",
        " xp_",
    ];
    if CORE_PATTERNS
        .iter()
        .any(|pattern| lowered.contains(pattern))
    {
        return true;
    }
    contains_sql_comment_signal(&lowered)
}

/// Match SQL line/block comment introducers with surrounding syntax context.
///
/// Bare `--` is intentionally excluded: base64url JWT segments and cursor tokens
/// legitimately contain consecutive hyphens.
fn contains_sql_comment_signal(lowered: &str) -> bool {
    ["'--", "\"--", " --", ";--", "#--", "/*", "*/"]
        .iter()
        .any(|pattern| lowered.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::WebFrameworkErrorKind;
    use axum::body::Body;
    use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};

    fn route(auth: RouteAuth) -> HttpRoute {
        HttpRoute::new(HttpMethod::Get, "/test", "test", "test.get", auth)
    }

    #[test]
    fn route_auth_profile_allowlist_rejects_cross_profile_credentials() {
        let cases: &[(RouteAuth, &[&str])] = &[
            (RouteAuth::Public, &[]),
            (RouteAuth::BootstrapBody, &[]),
            (RouteAuth::RefreshToken, &[]),
            (RouteAuth::CredentialEntryBootstrap, &["access-token"]),
            (RouteAuth::DualToken, &["authorization", "access-token"]),
            (RouteAuth::ApiKey, &["x-api-key"]),
            (RouteAuth::OAuth, &["authorization"]),
            (RouteAuth::OpenApiFlexible, &["authorization", "x-api-key"]),
            (
                RouteAuth::IngressToken,
                &["x-sdkwork-ingress-token", "access-token"],
            ),
            (
                RouteAuth::AgentToken,
                &["x-sdkwork-agent-token", "access-token"],
            ),
        ];

        for (auth, allowed_headers) in cases {
            for header in crate::constants::STANDARD_CREDENTIAL_HEADERS {
                let mut headers = axum::http::HeaderMap::new();
                headers.insert(
                    axum::http::HeaderName::from_bytes(header.as_bytes()).expect("header"),
                    axum::http::HeaderValue::from_static("credential"),
                );
                let result =
                    SecurityPolicy::validate_route_auth_credentials(&route(*auth), &headers);
                if allowed_headers.contains(header) {
                    result.unwrap_or_else(|error| {
                        panic!(
                            "{} should allow {header}: {error}",
                            auth.auth_profile_label()
                        )
                    });
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(WebFrameworkErrorKind::BadRequest, error.kind);
                    assert_eq!(
                        Some("credential-profile-contamination"),
                        error.reason.as_deref()
                    );
                }
            }
        }
    }

    #[test]
    fn api_key_or_dual_token_profile_enforces_exact_credential_alternatives() {
        let route = route(RouteAuth::ApiKeyOrDualToken);
        let cases = [
            (&[("x-api-key", "key")][..], true, None),
            (
                &[("authorization", "Bearer auth"), ("access-token", "access")][..],
                true,
                None,
            ),
            (
                &[("x-api-key", "key"), ("authorization", "Bearer auth")][..],
                false,
                Some("credential-profile-contamination"),
            ),
            (
                &[("x-api-key", "key"), ("access-token", "access")][..],
                false,
                Some("credential-profile-contamination"),
            ),
            (
                &[
                    ("x-api-key", "key"),
                    ("authorization", "Bearer auth"),
                    ("access-token", "access"),
                ][..],
                false,
                Some("credential-profile-contamination"),
            ),
            (
                &[("authorization", "Bearer auth")][..],
                false,
                Some("incomplete-credential-profile"),
            ),
            (
                &[("access-token", "access")][..],
                false,
                Some("incomplete-credential-profile"),
            ),
        ];

        for (entries, accepted, expected_reason) in cases {
            let mut headers = axum::http::HeaderMap::new();
            for (name, value) in entries {
                headers.insert(
                    axum::http::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
                    axum::http::HeaderValue::from_str(value).expect("header value"),
                );
            }
            let result = SecurityPolicy::validate_route_auth_credentials(&route, &headers);
            if accepted {
                result.expect("credential alternative must be accepted");
            } else {
                let error = result.expect_err("invalid credential combination must fail");
                assert_eq!(expected_reason, error.reason.as_deref());
            }
        }
    }

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
    fn validate_for_production_rejects_wildcard_headers() {
        let policy = CorsPolicy {
            allow_all_origins: false,
            allowed_origins: vec!["https://app.example".to_owned()],
            allowed_headers: vec!["*".to_owned()],
            ..CorsPolicy::default()
        };
        let error = policy
            .validate_for_production()
            .expect_err("wildcard headers must be rejected in production");
        assert!(error.contains("header allowlist"));
    }

    #[test]
    fn preflight_wildcard_headers_allow_any_request_header() {
        let policy = CorsPolicy {
            allowed_headers: vec!["*".to_owned()],
            ..CorsPolicy::default()
        };
        let request = Request::builder()
            .header("origin", "http://127.0.0.1:1520")
            .header("access-control-request-method", "GET")
            .header(
                "access-control-request-headers",
                "x-request-id, x-sdkwork-agent-token, x-device-id",
            )
            .body(Body::empty())
            .expect("build wildcard header preflight request");
        policy
            .validate_preflight(&request)
            .expect("wildcard header policy must accept any requested header");
    }

    #[test]
    fn wildcard_headers_emit_asterisk_allow_headers_response() {
        let policy = CorsPolicy {
            allow_all_origins: true,
            allowed_headers: vec!["*".to_owned()],
            ..CorsPolicy::default()
        };
        let mut response = Response::new(Body::empty());
        policy.apply_headers_from_origin(Some("http://127.0.0.1:1520"), &mut response);
        assert_eq!(
            Some("*"),
            response
                .headers()
                .get("access-control-allow-headers")
                .and_then(|value| value.to_str().ok()),
        );
    }

    #[test]
    fn development_loopback_accepts_any_numeric_port() {
        let policy = CorsPolicy::development_loopback();
        for origin in [
            "http://localhost:3000",
            "http://127.0.0.1:3901",
            "http://[::1]:5173",
            "https://localhost:8443",
        ] {
            policy
                .validate_origin_value(origin)
                .expect("loopback origin should be accepted");
        }
    }

    #[test]
    fn development_loopback_rejects_lookalike_and_non_numeric_ports() {
        let policy = CorsPolicy::development_loopback();
        for origin in [
            "http://localhost.example:3000",
            "http://127.0.0.2:3901",
            "http://localhost:dev",
            "http://localhost:3000/path",
        ] {
            policy
                .validate_origin_value(origin)
                .expect_err("untrusted origin should be rejected");
        }
    }

    #[test]
    fn development_private_network_accepts_dynamic_private_ip_origins() {
        let policy = CorsPolicy::development_private_network();
        for origin in [
            "http://10.20.30.40:5173",
            "https://172.16.20.5:8443",
            "http://192.168.50.12:3901",
            "https://[fd12:3456:789a::12]:3901",
            "http://127.0.0.2:3901",
        ] {
            policy
                .validate_origin_value(origin)
                .expect("private-network origin should be accepted");
        }
    }

    #[test]
    fn development_private_network_rejects_public_ip_and_host_origins() {
        let policy = CorsPolicy::development_private_network();
        for origin in [
            "http://203.0.113.10:3901",
            "https://evil.example.com:3901",
            "http://192.168.50.12:3901/path",
            "http://172.32.0.1:3901",
        ] {
            policy
                .validate_origin_value(origin)
                .expect_err("untrusted origin should be rejected");
        }
    }

    #[test]
    fn cors_response_headers_vary_by_origin() {
        let policy = CorsPolicy::development_private_network();
        let mut response = Response::new(Body::empty());
        policy.apply_headers_from_origin(Some("http://192.168.50.12:3901"), &mut response);
        assert_eq!(
            Some("http://192.168.50.12:3901"),
            response
                .headers()
                .get("access-control-allow-origin")
                .and_then(|value| value.to_str().ok()),
        );
        assert_eq!(
            Some("Origin"),
            response
                .headers()
                .get("vary")
                .and_then(|value| value.to_str().ok()),
        );
    }

    #[test]
    fn production_rejects_loopback_port_wildcards() {
        let error = CorsPolicy::development_private_network()
            .validate_for_production()
            .expect_err("loopback and private-network directives must be development-only");
        assert!(error.contains("port wildcard"));
    }

    #[test]
    fn sql_injection_guard_allows_base64url_jwt_segments_with_double_hyphen() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ0b2tlbl90eXBlIjogImFjY2VzcyIsInRlbmFudF9pZCI6IjEwMDAwMSJ9.ab--cd";
        assert!(!contains_sql_injection_signal(token));
    }

    #[test]
    fn sql_injection_guard_blocks_classic_comment_payloads() {
        assert!(contains_sql_injection_signal("' OR 1=1--"));
        assert!(contains_sql_injection_signal("\" OR 1=1--"));
        assert!(contains_sql_injection_signal("admin'--"));
        assert!(contains_sql_injection_signal("1;--"));
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
