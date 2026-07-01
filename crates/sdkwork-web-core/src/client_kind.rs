//! Client kind inference from transport headers (API_SPEC §10 / web-request-context schema).

use crate::request_context::WebClientKind;

/// Infers client kind from `User-Agent` and optional `X-SDKWork-Client-Kind` override.
pub fn infer_client_kind(user_agent: Option<&str>, explicit_kind: Option<&str>) -> WebClientKind {
    if let Some(kind) = explicit_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return parse_explicit_client_kind(kind);
    }
    let ua = user_agent.unwrap_or("").to_ascii_lowercase();
    if ua.is_empty() {
        return WebClientKind::Unknown;
    }
    if ua.contains("curl/")
        || ua.contains("httpie")
        || ua.contains("postman")
        || ua.contains("insomnia")
        || ua.contains("okhttp")
        || ua.contains("reqwest")
        || ua.contains("python-requests")
        || ua.contains("go-http-client")
        || ua.contains("axios/")
        || ua.contains("node-fetch")
    {
        return WebClientKind::Server;
    }
    if ua.contains("mobile")
        || ua.contains("android")
        || ua.contains("iphone")
        || ua.contains("ipad")
        || ua.contains("harmonyos")
    {
        return WebClientKind::Mobile;
    }
    if ua.contains("electron") || ua.contains("windows") || ua.contains("macintosh") {
        return WebClientKind::Desktop;
    }
    if ua.contains("mozilla/")
        || ua.contains("chrome/")
        || ua.contains("safari/")
        || ua.contains("firefox/")
        || ua.contains("edg/")
    {
        return WebClientKind::Browser;
    }
    WebClientKind::Unknown
}

fn parse_explicit_client_kind(kind: &str) -> WebClientKind {
    match kind.to_ascii_lowercase().as_str() {
        "browser" => WebClientKind::Browser,
        "mobile" => WebClientKind::Mobile,
        "desktop" => WebClientKind::Desktop,
        "server" => WebClientKind::Server,
        _ => WebClientKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_browser_from_chrome_ua() {
        assert_eq!(
            WebClientKind::Browser,
            infer_client_kind(Some("Mozilla/5.0 Chrome/120.0"), None)
        );
    }

    #[test]
    fn infers_server_from_curl() {
        assert_eq!(
            WebClientKind::Server,
            infer_client_kind(Some("curl/8.0.0"), None)
        );
    }

    #[test]
    fn explicit_kind_overrides_user_agent() {
        assert_eq!(
            WebClientKind::Mobile,
            infer_client_kind(Some("curl/8.0.0"), Some("mobile"))
        );
    }
}
