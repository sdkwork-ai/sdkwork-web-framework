//! Multi-level wildcard browser-origin patterns for the CORS allowlist.
//!
//! The exact-origin allowlist cannot express a first-party fleet whose browser
//! surfaces live on operator-chosen names at arbitrary depth. The registered
//! console-host pattern covers one registered label per host, and
//! [`crate::registered_subdomain_hosts::RegisteredSubdomainHosts`] covers a whole
//! registered base domain, but neither lets a deployment state a single
//! wildcard entry in the ordinary allowlist.
//!
//! This module adds exactly two host-wildcard forms to `allowed_origins`:
//!
//! - `https://*.example.com` matches **one** label before the base domain
//!   (`mall.example.com`) and nothing deeper;
//! - `https://**.example.com` matches **one or more** labels
//!   (`mall.example.com`, `a.b.example.com`, `region.console.eu.example.com`).
//!
//! Safety rules, all fail-closed:
//!
//! - the wildcard must be the leftmost label and the only wildcard;
//! - the scheme must be `http` or `https` and must match exactly;
//! - the base domain must carry at least two DNS labels, so `*.com` and
//!   `*.example` cannot be declared as a whole-registry trust statement;
//! - matching is suffix-anchored on whole DNS labels, so `evil-example.com`,
//!   `example.com.attacker.test` and `notexample.com` never match;
//! - patterns are lowercase and carry no port, userinfo, path, query or
//!   fragment, and a port-bearing or non-browser origin never matches;
//! - the apex (`example.com`) is not matched by either wildcard form; declare
//!   it as an exact origin when it is trusted.

use axum::http::Uri;

const SUPPORTED_WILDCARD_SCHEMES: &[&str] = &["http", "https"];

/// Prefix of a single-label host wildcard (`*.example.com`).
pub const SINGLE_LABEL_HOST_WILDCARD: &str = "*.";

/// Prefix of a multi-label host wildcard (`**.example.com`).
pub const MULTI_LABEL_HOST_WILDCARD: &str = "**.";

/// A parsed host-wildcard origin pattern borrowed from the configured entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HostWildcardPattern<'a> {
    scheme: &'a str,
    /// Lower bound of DNS labels this wildcard may replace (always one).
    minimum_replaced_labels: usize,
    /// Upper bound of replaced labels; `None` means unbounded depth.
    maximum_replaced_labels: Option<usize>,
    base_domain: &'a str,
}

/// Returns whether `pattern` is a syntactically valid host-wildcard origin
/// pattern (`scheme://*.base` or `scheme://**.base`).
pub fn is_host_wildcard_origin_pattern(pattern: &str) -> bool {
    parse_host_wildcard_pattern(pattern).is_ok()
}

/// Validates a host-wildcard origin pattern, returning a diagnosable reason.
pub fn validate_host_wildcard_origin_pattern(pattern: &str) -> Result<(), String> {
    parse_host_wildcard_pattern(pattern).map(|_| ())
}

/// Matches a browser `Origin` value against a host-wildcard origin pattern.
///
/// Returns `false` for any malformed pattern or origin so a bad configuration
/// entry can never widen the allowlist. The request path is allocation-free.
pub fn host_wildcard_origin_matches(pattern: &str, origin: &str) -> bool {
    let Ok(pattern) = parse_host_wildcard_pattern(pattern) else {
        return false;
    };
    if origin.trim() != origin {
        return false;
    }
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    let Some(scheme) = uri.scheme_str() else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case(pattern.scheme) {
        return false;
    }
    let Some(authority) = uri.authority() else {
        return false;
    };
    // A browser origin carries no userinfo, and a first-party cloud host never
    // carries a port; this matcher never trusts either form.
    if authority.as_str().contains('@') || uri.port().is_some() {
        return false;
    }
    let path = uri.path();
    if !(path.is_empty() || path == "/") || uri.query().is_some() {
        return false;
    }
    let Some(host) = uri.host() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let Some(prefix) = host.strip_suffix(pattern.base_domain) else {
        return false;
    };
    let Some(prefix) = prefix.strip_suffix('.') else {
        return false;
    };
    if prefix.is_empty() {
        return false;
    }
    let mut replaced_labels = 0usize;
    for label in prefix.split('.') {
        if !is_valid_dns_label(label) {
            return false;
        }
        replaced_labels += 1;
    }
    if replaced_labels < pattern.minimum_replaced_labels {
        return false;
    }
    match pattern.maximum_replaced_labels {
        Some(maximum) if replaced_labels > maximum => false,
        _ => true,
    }
}

fn parse_host_wildcard_pattern(pattern: &str) -> Result<HostWildcardPattern<'_>, String> {
    if pattern.is_empty() || pattern.trim() != pattern {
        return Err("host wildcard origin pattern must be non-empty and trimmed".to_owned());
    }
    let Some((scheme, authority)) = pattern.split_once("://") else {
        return Err(format!(
            "host wildcard origin pattern `{pattern}` must start with an explicit http(s) scheme"
        ));
    };
    if !SUPPORTED_WILDCARD_SCHEMES
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(scheme))
    {
        return Err(format!(
            "host wildcard origin pattern `{pattern}` scheme must be one of {}",
            SUPPORTED_WILDCARD_SCHEMES.join(", ")
        ));
    }
    if authority.contains(['@', '?', '#', '/', ':']) {
        return Err(format!(
            "host wildcard origin pattern `{pattern}` must not carry userinfo, a port, a path, a query or a fragment"
        ));
    }
    let (minimum_replaced_labels, maximum_replaced_labels, base_domain) =
        if let Some(base_domain) = authority.strip_prefix(MULTI_LABEL_HOST_WILDCARD) {
            (1usize, None, base_domain)
        } else if let Some(base_domain) = authority.strip_prefix(SINGLE_LABEL_HOST_WILDCARD) {
            (1usize, Some(1usize), base_domain)
        } else {
            return Err(format!(
                "host wildcard origin pattern `{pattern}` must start with `*.` or `**.`"
            ));
        };
    if base_domain.contains('*') {
        return Err(format!(
            "host wildcard origin pattern `{pattern}` may contain exactly one wildcard label"
        ));
    }
    if base_domain != base_domain.to_ascii_lowercase() {
        return Err(format!(
            "host wildcard origin pattern `{pattern}` must be lowercase"
        ));
    }
    let mut base_labels = 0usize;
    for label in base_domain.split('.') {
        if !is_valid_dns_label(label) {
            return Err(format!(
                "host wildcard origin pattern `{pattern}` base domain must be a valid DNS domain"
            ));
        }
        base_labels += 1;
    }
    if base_labels < 2 {
        return Err(format!(
            "host wildcard origin pattern `{pattern}` base domain must carry at least two DNS labels"
        ));
    }
    Ok(HostWildcardPattern {
        scheme,
        minimum_replaced_labels,
        maximum_replaced_labels,
        base_domain,
    })
}

fn is_valid_dns_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_label_wildcard_matches_exactly_one_label() {
        assert!(host_wildcard_origin_matches(
            "https://*.sdkwork.com",
            "https://mall.sdkwork.com"
        ));
        assert!(host_wildcard_origin_matches(
            "https://*.sdkwork.com",
            "https://MALL.SDKWORK.COM"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://*.sdkwork.com",
            "https://a.b.sdkwork.com"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://*.sdkwork.com",
            "https://sdkwork.com"
        ));
    }

    #[test]
    fn multi_label_wildcard_matches_second_third_and_fourth_levels() {
        let pattern = "https://**.birdcoder.com";
        assert!(host_wildcard_origin_matches(
            pattern,
            "https://platform.birdcoder.com"
        ));
        assert!(host_wildcard_origin_matches(
            pattern,
            "https://a.b.birdcoder.com"
        ));
        assert!(host_wildcard_origin_matches(
            pattern,
            "https://x.y.z.birdcoder.com"
        ));
        assert!(host_wildcard_origin_matches(
            pattern,
            "https://region.console.eu.birdcoder.com"
        ));
        assert!(!host_wildcard_origin_matches(
            pattern,
            "https://birdcoder.com"
        ));
    }

    #[test]
    fn wildcard_never_crosses_a_lookalike_suffix_or_scheme() {
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "https://evil-sdkwork.com"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "https://notsdkwork.com"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "https://sdkwork.com.attacker.test"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "https://a.b.sdkwork.com.attacker.test"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://*.sdkwork.com",
            "http://mall.sdkwork.com"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://*.sdkwork.com",
            "https://mall.sdkwork.com.cn"
        ));
    }

    #[test]
    fn wildcard_rejects_non_browser_origin_shapes() {
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "https://user@mall.sdkwork.com"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "https://mall.sdkwork.com:8443"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "https://mall.sdkwork.com/console"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "https://mall.sdkwork.com?x=1"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            " https://mall.sdkwork.com"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "app://dsh"
        ));
        assert!(!host_wildcard_origin_matches(
            "https://**.sdkwork.com",
            "null"
        ));
    }

    #[test]
    fn invalid_patterns_are_rejected_before_matching() {
        assert!(validate_host_wildcard_origin_pattern("https://**.sdkwork.com").is_ok());
        assert!(validate_host_wildcard_origin_pattern("http://*.sdkwork.com").is_ok());
        // No scheme, single-label base domain, embedded wildcard, port, path.
        assert!(validate_host_wildcard_origin_pattern("**.sdkwork.com").is_err());
        assert!(validate_host_wildcard_origin_pattern("https://**.com").is_err());
        assert!(validate_host_wildcard_origin_pattern("https://a.*.sdkwork.com").is_err());
        assert!(validate_host_wildcard_origin_pattern("https://**.sdkwork.com:443").is_err());
        assert!(validate_host_wildcard_origin_pattern("https://**.sdkwork.com/app").is_err());
        assert!(validate_host_wildcard_origin_pattern("https://**.sdk_work.com").is_err());
        assert!(validate_host_wildcard_origin_pattern("https://**.SDKWORK.com").is_err());
        assert!(validate_host_wildcard_origin_pattern("ftp://**.sdkwork.com").is_err());
        assert!(validate_host_wildcard_origin_pattern("https://example.com").is_err());
        // A wildcard match against an invalid pattern can never succeed.
        assert!(!host_wildcard_origin_matches(
            "https://**.com",
            "https://a.com"
        ));
    }

    #[test]
    fn wildcard_detection_is_distinct_from_other_allowlist_forms() {
        assert!(is_host_wildcard_origin_pattern("https://*.sdkwork.com"));
        assert!(is_host_wildcard_origin_pattern("https://**.sdkwork.com"));
        assert!(!is_host_wildcard_origin_pattern("https://sdkwork.com"));
        assert!(!is_host_wildcard_origin_pattern("app://*"));
        assert!(!is_host_wildcard_origin_pattern("http://localhost:*"));
        assert!(!is_host_wildcard_origin_pattern("http://private-network:*"));
    }
}
