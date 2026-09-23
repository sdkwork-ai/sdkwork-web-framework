//! Registered SDKWork console host patterns for platform API edge CORS.
//!
//! Every module browser console is served on its own cloud sub-domain
//! (`<label><environment-suffix>.<base-domain>`) and calls the platform API edge
//! cross-origin. The edge matches browser `Origin` values exactly and has no
//! sub-domain wildcard, so a deployment either enumerates every console origin
//! (thousands of entries, past the point where a single environment variable is
//! still a sane carrier) or declares the *pattern* and lets the framework decide.
//!
//! This module implements the pattern form: the operator supplies the console
//! host **labels** (`im`, `shop`, `server`, ...) plus the environment suffix and
//! accepted schemes, and matching stays exact — a host is allowed only when it
//! equals `<label><suffix>.<registered base domain>` for a registered label and
//! one of the [`SDKWORK_REGISTERED_SERVICE_BASE_DOMAINS`]. The base-domain
//! registry is platform infrastructure (`APP_RUNTIME_TOPOLOGY_NAMING` §9.3 Base
//! Domain Registry), so it lives here; the label list is data supplied per
//! deployment, so a new module console never needs a framework release.
//!
//! Security properties (enforced by [`RegisteredConsoleHosts::validate`]):
//! - base domains are restricted to the registered platform registry, so a
//!   deployment can never widen the rule to an unrelated domain;
//! - a label is a single DNS label (no `.`, `/`, `:`, `@`, `*`), so the pattern
//!   cannot match a nested host such as `evil.sdkwork.com.attacker.example`;
//! - the scheme must be one of the configured schemes, so a production policy
//!   can stay HTTPS-only.

use std::env;

use axum::http::Uri;

/// Canonical registered console host **labels** environment key shared by every
/// service.
///
/// Comma-separated console host labels without the environment suffix (for
/// example `im,shop,server-app`). Setting this key enables the registered
/// console host pattern so a deployment never has to enumerate every module
/// console origin in `SDKWORK_CORS_ALLOWED_ORIGINS`.
pub const SHARED_CORS_CONSOLE_HOST_LABELS_ENV_KEY: &str = "SDKWORK_CORS_CONSOLE_HOST_LABELS";

/// Environment suffix appended to every registered console host label
/// (`-dev`, `-test`, `-staging`, `-demo`, or empty for production).
///
/// Set-but-empty is meaningful (production), so the key must be present
/// whenever [`SHARED_CORS_CONSOLE_HOST_LABELS_ENV_KEY`] is configured.
pub const SHARED_CORS_CONSOLE_HOST_SUFFIX_ENV_KEY: &str = "SDKWORK_CORS_CONSOLE_HOST_SUFFIX";

/// Accepted origin schemes for registered console hosts, comma-separated
/// (`http,https` outside production, `https` in production).
pub const SHARED_CORS_CONSOLE_HOST_SCHEMES_ENV_KEY: &str = "SDKWORK_CORS_CONSOLE_HOST_SCHEMES";

/// Optional override of the registered console host base domains. Absent means
/// the registered platform registry from [`registered_service_base_domains`].
pub const SHARED_CORS_CONSOLE_HOST_BASE_DOMAINS_ENV_KEY: &str =
    "SDKWORK_CORS_CONSOLE_HOST_BASE_DOMAINS";

/// Registered SDKWork public base-domain family (`APP_RUNTIME_TOPOLOGY_NAMING`
/// §9.3 Base Domain Registry). The edge serves
/// `api<environment-suffix>.<base-domain>` and every module console
/// `<label><environment-suffix>.<base-domain>` for each entry.
pub const SDKWORK_REGISTERED_SERVICE_BASE_DOMAINS: &[&str] = &[
    "sdkwork.com",
    "birdcoder.com",
    "dtupay.com",
    "noaper.com",
    "sdkwork.cn",
    "birdcoder.cn",
    "dtupay.cn",
    "noaper.cn",
    "skubc.com",
    "skubc.cn",
    "zowalk.com",
    "zowalk.cn",
    "offer86.com",
    "offer86.cn",
    "86offer.com",
    "86offer.cn",
];

/// Returns whether `domain` is a registered SDKWork public base domain.
pub fn is_registered_service_base_domain(domain: &str) -> bool {
    SDKWORK_REGISTERED_SERVICE_BASE_DOMAINS
        .iter()
        .any(|registered| registered.eq_ignore_ascii_case(domain))
}

/// Returns the registered platform base domains.
pub fn registered_service_base_domains() -> Vec<String> {
    SDKWORK_REGISTERED_SERVICE_BASE_DOMAINS
        .iter()
        .map(|domain| (*domain).to_owned())
        .collect()
}

const SUPPORTED_SCHEMES: &[&str] = &["http", "https"];

fn read_env_list<F>(lookup: &F, keys: &[&str]) -> Option<Vec<String>>
where
    F: Fn(&str) -> Option<String>,
{
    keys.iter().find_map(|key| {
        lookup(key).map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_owned)
                .collect()
        })
    })
}

/// Resolves the registered console host pattern from the process environment.
///
/// Returns `Ok(None)` when [`SHARED_CORS_CONSOLE_HOST_LABELS_ENV_KEY`] is unset,
/// which keeps the historical exact-origin-only behavior. A half-configured or
/// invalid pattern is a diagnosable error so a deployment fails loudly at
/// startup instead of silently serving a console the edge will reject with
/// `No 'Access-Control-Allow-Origin' header is present on the requested
/// resource`.
///
/// The environment suffix and the accepted schemes carry deployment intent that
/// cannot be recovered from the framework `WebEnvironment` (staging and demo
/// both collapse onto the strict production posture), so both keys are required
/// alongside the labels rather than derived.
pub fn registered_console_hosts_from_env() -> Result<Option<RegisteredConsoleHosts>, String> {
    registered_console_hosts_from_lookup(|key| env::var(key).ok())
}

/// Resolves the registered console host pattern from an injected environment.
///
/// Applications that thread their own resolved environment (loaded config plus
/// overrides, or a test fixture) rather than reading the process environment
/// directly use this form; `lookup` must return `Some("")` for a key that is
/// explicitly set to an empty value, because an empty suffix is meaningful
/// (production).
pub fn registered_console_hosts_from_lookup<F>(
    lookup: F,
) -> Result<Option<RegisteredConsoleHosts>, String>
where
    F: Fn(&str) -> Option<String>,
{
    let labels =
        read_env_list(&lookup, &[SHARED_CORS_CONSOLE_HOST_LABELS_ENV_KEY]).unwrap_or_default();
    let suffix =
        lookup(SHARED_CORS_CONSOLE_HOST_SUFFIX_ENV_KEY).map(|value| value.trim().to_owned());
    let schemes =
        read_env_list(&lookup, &[SHARED_CORS_CONSOLE_HOST_SCHEMES_ENV_KEY]).unwrap_or_default();
    let base_domains = read_env_list(&lookup, &[SHARED_CORS_CONSOLE_HOST_BASE_DOMAINS_ENV_KEY])
        .unwrap_or_else(registered_service_base_domains);

    if labels.is_empty() {
        if suffix.is_some() || !schemes.is_empty() {
            return Err(format!(
                "{SHARED_CORS_CONSOLE_HOST_LABELS_ENV_KEY} must be set when {} or {} is configured",
                SHARED_CORS_CONSOLE_HOST_SUFFIX_ENV_KEY, SHARED_CORS_CONSOLE_HOST_SCHEMES_ENV_KEY,
            ));
        }
        return Ok(None);
    }

    let suffix = suffix.ok_or_else(|| {
        format!(
            "{SHARED_CORS_CONSOLE_HOST_SUFFIX_ENV_KEY} must be set when {SHARED_CORS_CONSOLE_HOST_LABELS_ENV_KEY} is configured (use an empty value for production)"
        )
    })?;
    if schemes.is_empty() {
        return Err(format!(
            "{SHARED_CORS_CONSOLE_HOST_SCHEMES_ENV_KEY} must be set when {SHARED_CORS_CONSOLE_HOST_LABELS_ENV_KEY} is configured"
        ));
    }

    let hosts = RegisteredConsoleHosts::new(labels, suffix, schemes, base_domains);
    hosts.validate()?;
    Ok(Some(hosts))
}

/// Registered console host pattern: `<scheme>://<label><suffix>.<base-domain>`.
///
/// Matching is exact per component; the pattern never behaves like a wildcard
/// over arbitrary sub-domains because both `labels` and `base_domains` are
/// closed registries.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredConsoleHosts {
    /// Console host labels without the environment suffix (for example `im`,
    /// `router-admin`, `server-app`).
    pub labels: Vec<String>,
    /// Environment suffix appended to every label (`-dev`, `-test`,
    /// `-staging`, `-demo`, or empty for production).
    pub environment_suffix: String,
    /// Accepted origin schemes (`http`, `https`).
    pub schemes: Vec<String>,
    /// Registered base domains the labels may be combined with. Omitted in a
    /// configuration file means the platform registry
    /// ([`registered_service_base_domains`]); a deployment can therefore never
    /// widen the rule by accident, and only a deliberate override lists domains.
    #[serde(default = "registered_service_base_domains")]
    pub base_domains: Vec<String>,
}

impl RegisteredConsoleHosts {
    /// Builds a pattern from already-resolved components.
    pub fn new(
        labels: Vec<String>,
        environment_suffix: impl Into<String>,
        schemes: Vec<String>,
        base_domains: Vec<String>,
    ) -> Self {
        Self {
            labels,
            environment_suffix: environment_suffix.into(),
            schemes,
            base_domains,
        }
    }

    /// Returns whether this pattern matches a browser `Origin` value.
    pub fn matches(&self, origin: &str) -> bool {
        if self.labels.is_empty() || self.schemes.is_empty() || self.base_domains.is_empty() {
            return false;
        }
        let Ok(uri) = origin.parse::<Uri>() else {
            return false;
        };
        let Some(scheme) = uri.scheme_str() else {
            return false;
        };
        if !self
            .schemes
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(scheme))
        {
            return false;
        }
        let Some(authority) = uri.authority() else {
            return false;
        };
        // Browsers never emit userinfo; a value that carries one is not a plain
        // browser origin and must not be trusted.
        if authority.as_str().contains('@') {
            return false;
        }
        let path = uri.path();
        if !(path.is_empty() || path == "/") || uri.query().is_some() {
            return false;
        }
        // `CORS_SPEC.md` §3: a console host origin carries no port. Port-bearing
        // console origins (`http://server.sdkwork.com:18080`) are legacy drift and
        // stay rejected, matching `tools/cors/registry.mjs#isRegisteredHostOrigin`
        // — the pattern narrows an enumeration, it never widens into "any port on
        // a registered host".
        if uri.port().is_some() {
            return false;
        }

        let host = authority.host().to_ascii_lowercase();
        for base_domain in &self.base_domains {
            let Some(label_with_suffix) = host.strip_suffix(&format!(".{base_domain}")) else {
                continue;
            };
            if label_with_suffix.is_empty() || label_with_suffix.contains('.') {
                continue;
            }
            for label in &self.labels {
                if label_with_suffix == format!("{label}{}", self.environment_suffix) {
                    return true;
                }
            }
        }
        false
    }

    /// Validates the pattern components. Fail-closed: every ambiguity is an error.
    pub fn validate(&self) -> Result<(), String> {
        if self.labels.is_empty() {
            return Err("registered console hosts require at least one label".into());
        }
        if self.schemes.is_empty() {
            return Err("registered console hosts require at least one scheme".into());
        }
        if self.base_domains.is_empty() {
            return Err("registered console hosts require at least one base domain".into());
        }
        if let Some(scheme) = self.schemes.iter().find(|scheme| {
            !SUPPORTED_SCHEMES
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(scheme))
        }) {
            return Err(format!(
                "registered console host scheme `{scheme}` is not one of {}",
                SUPPORTED_SCHEMES.join(", ")
            ));
        }
        if !is_valid_suffix(&self.environment_suffix) {
            return Err(format!(
                "registered console host suffix `{}` must be empty or a `-`-prefixed DNS label",
                self.environment_suffix
            ));
        }
        if let Some(label) = self.labels.iter().find(|label| !is_valid_dns_label(label)) {
            return Err(format!(
                "registered console host label `{label}` must be a single lowercase DNS label"
            ));
        }
        if let Some(base_domain) = self
            .base_domains
            .iter()
            .find(|base_domain| !is_registered_service_base_domain(base_domain))
        {
            return Err(format!(
                "registered console host base domain `{base_domain}` is not a registered SDKWork service base domain"
            ));
        }
        Ok(())
    }
}

fn is_valid_dns_label(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn is_valid_suffix(value: &str) -> bool {
    value.is_empty() || (value.starts_with('-') && is_valid_dns_label(&value[1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(labels: &[&str], suffix: &str, schemes: &[&str]) -> RegisteredConsoleHosts {
        RegisteredConsoleHosts::new(
            labels.iter().map(|label| (*label).to_owned()).collect(),
            suffix,
            schemes.iter().map(|scheme| (*scheme).to_owned()).collect(),
            registered_service_base_domains(),
        )
    }

    #[test]
    fn matches_registered_console_hosts_for_the_environment() {
        let hosts = pattern(&["im", "server-app"], "-dev", &["http", "https"]);
        assert!(hosts.matches("http://im-dev.sdkwork.com"));
        assert!(hosts.matches("https://im-dev.sdkwork.com"));
        assert!(hosts.matches("https://server-app-dev.86offer.cn"));
    }

    #[test]
    fn production_pattern_has_no_suffix_and_stays_https_only() {
        let hosts = pattern(&["im", "admin"], "", &["https"]);
        assert!(hosts.matches("https://im.sdkwork.com"));
        assert!(hosts.matches("https://admin.zowalk.cn"));
        assert!(!hosts.matches("http://im.sdkwork.com"));
        assert!(!hosts.matches("https://im-dev.sdkwork.com"));
    }

    #[test]
    fn rejects_unregistered_labels_domains_and_nested_hosts() {
        let hosts = pattern(&["im", "server-app"], "-dev", &["http", "https"]);
        assert!(!hosts.matches("http://evil.example.com"));
        assert!(!hosts.matches("http://im-dev.birdcoder.com.attacker.example"));
        assert!(!hosts.matches("http://im-dev.sdkwork.com.attacker.example"));
        // `im-product-dev` strips to label `im-product`, which is not registered.
        assert!(!hosts.matches("http://im-product-dev.sdkwork.com"));
        assert!(!hosts.matches("http://im-dev.notregistered.com"));
        // userinfo and non-root paths are never browser console origins
        assert!(!hosts.matches("http://user@im-dev.sdkwork.com"));
        assert!(!hosts.matches("http://im-dev.sdkwork.com/console"));
        // CORS_SPEC.md §3: port-bearing console origins are legacy drift and stay
        // rejected; the pattern narrows an enumeration, it never widens to "any
        // port on a registered host".
        assert!(!hosts.matches("http://im-dev.sdkwork.com:13800"));
        assert!(!hosts.matches("https://im-dev.sdkwork.com:8443"));
        // scheme must be allowed
        assert!(!hosts.matches("ftp://im-dev.sdkwork.com"));
        // scheme-relative and opaque origins are not console origins
        assert!(!hosts.matches("null"));
        assert!(!hosts.matches("app://dsh"));
    }

    #[test]
    fn validate_is_fail_closed() {
        assert!(pattern(&["im"], "-dev", &["http", "https"])
            .validate()
            .is_ok());
        assert!(pattern(&["im"], "", &["https"]).validate().is_ok());

        let mut hosts = pattern(&["im"], "-dev", &["http", "https"]);
        hosts.labels.clear();
        assert!(hosts.validate().is_err());

        let mut hosts = pattern(&["im"], "-dev", &["http", "https"]);
        hosts.schemes = vec!["*".to_owned()];
        assert!(hosts.validate().is_err());

        let mut hosts = pattern(&["im"], "-dev", &["http", "https"]);
        hosts.base_domains = vec!["notregistered.com".to_owned()];
        assert!(hosts.validate().is_err());

        let mut hosts = pattern(&["im"], "-dev", &["http", "https"]);
        hosts.labels = vec!["*".to_owned()];
        assert!(hosts.validate().is_err());

        let mut hosts = pattern(&["im"], "-dev", &["http", "https"]);
        hosts.labels = vec!["evil.example".to_owned()];
        assert!(hosts.validate().is_err());

        let mut hosts = pattern(&["im"], "-dev", &["http", "https"]);
        hosts.environment_suffix = ".sdkwork.com".to_owned();
        assert!(hosts.validate().is_err());
    }
}
