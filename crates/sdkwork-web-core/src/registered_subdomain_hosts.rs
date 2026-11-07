//! Registered SDKWork sub-domain origin policy for the platform API edge.
//!
//! [`RegisteredConsoleHosts`](crate::registered_console_hosts::RegisteredConsoleHosts)
//! matches `<label><suffix>.<base domain>` exactly and deliberately has no
//! sub-domain wildcard: the label set is a closed registry, so a new console is a
//! data edit rather than a framework release.
//!
//! Some first-party surfaces are nevertheless served on names the label registry
//! cannot express — an operator-chosen brand host (`platform.birdcoder.com`), an
//! apex (`sdkwork.com`), or a deeper regional/staged name
//! (`region.console.sdkwork.com`). This module implements the opt-in rule a
//! deployment uses when it decides that **every** host under a registered base
//! domain is a trusted first-party origin.
//!
//! Trust boundary (enforced by [`RegisteredSubdomainHosts::validate`]):
//! - base domains are restricted to the registered platform registry, so a
//!   deployment can never widen the rule to an unrelated domain;
//! - the match is suffix-anchored on whole DNS labels, so
//!   `evil-sdkwork.com`, `sdkwork.com.attacker.example` and
//!   `notsdkwork.com` stay rejected;
//! - every label of the sub-domain part must be a valid DNS label;
//! - a browser origin carries no userinfo, port, path, query or fragment;
//! - the scheme must be one of the configured schemes.
//!
//! The rule is a deliberate deviation from the exact-console-host pattern; the
//! standard records it as `CORS_SPEC.md` §5.1 and it `MUST` stay opt-in.

use std::env;

use axum::http::Uri;

use crate::registered_console_hosts::is_registered_service_base_domain;

/// Comma-separated registered base domains whose whole sub-domain space is a
/// trusted browser origin, for example `sdkwork.com,birdcoder.com`.
pub const SHARED_CORS_SUBDOMAIN_BASE_DOMAINS_ENV_KEY: &str = "SDKWORK_CORS_SUBDOMAIN_BASE_DOMAINS";

/// Comma-separated schemes accepted for the sub-domain rule (`http,https`).
///
/// Required whenever [`SHARED_CORS_SUBDOMAIN_BASE_DOMAINS_ENV_KEY`] is
/// configured: an absent scheme list must fail closed rather than default to the
/// widest posture.
pub const SHARED_CORS_SUBDOMAIN_SCHEMES_ENV_KEY: &str = "SDKWORK_CORS_SUBDOMAIN_SCHEMES";

const SUPPORTED_SCHEMES: &[&str] = &["http", "https"];

fn read_env_list<F>(lookup: &F, key: &str) -> Option<Vec<String>>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key).map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

/// Resolves the sub-domain origin policy from the process environment.
///
/// Returns `Ok(None)` when the base-domain key is unset or empty, which keeps the
/// historical exact-origin + console-pattern behavior. A half-configured or
/// invalid policy is a diagnosable error so a deployment fails loudly at startup
/// instead of silently rejecting first-party preflights.
pub fn registered_subdomain_hosts_from_env() -> Result<Option<RegisteredSubdomainHosts>, String> {
    registered_subdomain_hosts_from_lookup(|key| env::var(key).ok())
}

/// Resolves the sub-domain origin policy from an injected environment.
///
/// `lookup` must return `Some("")` for a key that is explicitly set to an empty
/// value; an empty base-domain list means "disabled".
pub fn registered_subdomain_hosts_from_lookup<F>(
    lookup: F,
) -> Result<Option<RegisteredSubdomainHosts>, String>
where
    F: Fn(&str) -> Option<String>,
{
    let base_domains =
        read_env_list(&lookup, SHARED_CORS_SUBDOMAIN_BASE_DOMAINS_ENV_KEY).unwrap_or_default();
    let schemes = read_env_list(&lookup, SHARED_CORS_SUBDOMAIN_SCHEMES_ENV_KEY).unwrap_or_default();

    if base_domains.is_empty() {
        if !schemes.is_empty() {
            return Err(format!(
                "{SHARED_CORS_SUBDOMAIN_SCHEMES_ENV_KEY} is set but {SHARED_CORS_SUBDOMAIN_BASE_DOMAINS_ENV_KEY} is empty"
            ));
        }
        return Ok(None);
    }
    if schemes.is_empty() {
        return Err(format!(
            "{SHARED_CORS_SUBDOMAIN_SCHEMES_ENV_KEY} must be set when {SHARED_CORS_SUBDOMAIN_BASE_DOMAINS_ENV_KEY} is configured"
        ));
    }

    let hosts = RegisteredSubdomainHosts::new(base_domains, schemes);
    hosts.validate()?;
    Ok(Some(hosts))
}

/// Opt-in rule: every host under a registered base domain is a trusted origin.
///
/// Matching is suffix-anchored on DNS labels (never a raw string suffix), so the
/// rule cannot be smuggled onto an attacker-controlled domain that merely
/// contains a registered domain.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredSubdomainHosts {
    /// Registered base domains whose apex and every sub-domain may act as an
    /// origin (`sdkwork.com`, `birdcoder.com`, ...).
    pub base_domains: Vec<String>,
    /// Accepted origin schemes (`http`, `https`).
    pub schemes: Vec<String>,
}

impl RegisteredSubdomainHosts {
    /// Builds a policy from already-resolved components.
    pub fn new(base_domains: Vec<String>, schemes: Vec<String>) -> Self {
        Self {
            base_domains,
            schemes,
        }
    }

    /// Returns whether this rule matches a browser `Origin` value.
    pub fn matches(&self, origin: &str) -> bool {
        if self.base_domains.is_empty() || self.schemes.is_empty() {
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
        // A browser origin may carry a port, but a first-party cloud host never
        // does; CORS_SPEC.md §3 keeps port-bearing console origins out.
        if uri.port().is_some() {
            return false;
        }

        let host = authority.host().to_ascii_lowercase();
        for base_domain in &self.base_domains {
            let base_domain = base_domain.to_ascii_lowercase();
            if host == base_domain {
                return true;
            }
            let Some(prefix) = host.strip_suffix(&format!(".{base_domain}")) else {
                continue;
            };
            // `strip_suffix` already guarantees a non-empty remainder here, but
            // an empty prefix would mean the host equals the base domain and is
            // handled above.
            if !prefix.is_empty() && prefix.split('.').all(is_valid_dns_label) {
                return true;
            }
        }
        false
    }

    /// Validates the rule components. Fail-closed: every ambiguity is an error.
    pub fn validate(&self) -> Result<(), String> {
        if self.base_domains.is_empty() {
            return Err("registered sub-domain hosts require at least one base domain".into());
        }
        if self.schemes.is_empty() {
            return Err("registered sub-domain hosts require at least one scheme".into());
        }
        if let Some(scheme) = self.schemes.iter().find(|scheme| {
            !SUPPORTED_SCHEMES
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(scheme))
        }) {
            return Err(format!(
                "registered sub-domain host scheme `{scheme}` is not one of {}",
                SUPPORTED_SCHEMES.join(", ")
            ));
        }
        if let Some(base_domain) = self
            .base_domains
            .iter()
            .find(|base_domain| !is_registered_service_base_domain(base_domain))
        {
            return Err(format!(
                "registered sub-domain host base domain `{base_domain}` is not a registered SDKWork service base domain"
            ));
        }
        for (index, base_domain) in self.base_domains.iter().enumerate() {
            if self.base_domains[index + 1..]
                .iter()
                .any(|other| other.eq_ignore_ascii_case(base_domain))
            {
                return Err(format!(
                    "registered sub-domain host base domain `{base_domain}` is declared more than once"
                ));
            }
        }
        Ok(())
    }
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

    fn policy(base_domains: &[&str], schemes: &[&str]) -> RegisteredSubdomainHosts {
        RegisteredSubdomainHosts::new(
            base_domains
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            schemes.iter().map(|value| (*value).to_owned()).collect(),
        )
    }

    fn canonical() -> RegisteredSubdomainHosts {
        policy(
            &["sdkwork.com", "birdcoder.com", "sdkwork.cn", "birdcoder.cn"],
            &["http", "https"],
        )
    }

    #[test]
    fn matches_apex_and_every_subdomain_depth() {
        let hosts = canonical();
        assert!(hosts.matches("https://sdkwork.com"));
        assert!(hosts.matches("http://birdcoder.cn"));
        // Second, third and fourth level (the depth the standard names).
        assert!(hosts.matches("https://platform.birdcoder.com"));
        assert!(hosts.matches("https://a.b.sdkwork.com"));
        assert!(hosts.matches("https://x.y.z.birdcoder.com"));
        assert!(hosts.matches("https://region.console.eu.sdkwork.cn"));
        assert!(hosts.matches("https://anything.birdcoder.cn"));
    }

    #[test]
    fn rejects_unregistered_domains_and_lookalike_suffixes() {
        let hosts = canonical();
        assert!(!hosts.matches("https://evil.example.com"));
        assert!(!hosts.matches("https://evil-sdkwork.com"));
        assert!(!hosts.matches("https://notsdkwork.com"));
        assert!(!hosts.matches("https://sdkwork.com.attacker.example"));
        assert!(!hosts.matches("https://a.b.sdkwork.com.attacker.example"));
        assert!(!hosts.matches("https://dtupay.com"));
    }

    #[test]
    fn rejects_non_browser_origin_shapes() {
        let hosts = canonical();
        assert!(!hosts.matches("http://user@sdkwork.com"));
        assert!(!hosts.matches("http://sdkwork.com/console"));
        assert!(!hosts.matches("http://sdkwork.com?x=1"));
        assert!(!hosts.matches("http://sdkwork.com:8443"));
        assert!(!hosts.matches("ftp://sdkwork.com"));
        assert!(!hosts.matches("app://dsh"));
        assert!(!hosts.matches("null"));
    }

    #[test]
    fn scheme_axis_is_enforced() {
        let https_only = policy(&["sdkwork.com"], &["https"]);
        assert!(https_only.matches("https://a.sdkwork.com"));
        assert!(!https_only.matches("http://a.sdkwork.com"));
    }

    #[test]
    fn validate_is_fail_closed() {
        assert!(canonical().validate().is_ok());

        let mut hosts = canonical();
        hosts.base_domains.clear();
        assert!(hosts.validate().is_err());

        let mut hosts = canonical();
        hosts.schemes = vec!["*".to_owned()];
        assert!(hosts.validate().is_err());

        let mut hosts = canonical();
        hosts.base_domains = vec!["attacker.example".to_owned()];
        assert!(hosts.validate().is_err());

        let mut hosts = canonical();
        hosts.base_domains = vec!["SDKWORK.COM".to_owned()];
        assert!(
            hosts.validate().is_ok(),
            "case-insensitive registry membership stays valid"
        );

        let mut hosts = canonical();
        hosts.base_domains = vec!["sdkwork.com".to_owned(), "sdkwork.com".to_owned()];
        assert!(hosts.validate().is_err());
    }

    #[test]
    fn env_resolution_requires_both_keys_and_fails_closed() {
        let resolve = |entries: &[(&str, &str)]| {
            let owned = entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect::<Vec<_>>();
            registered_subdomain_hosts_from_lookup(move |key| {
                owned
                    .iter()
                    .find(|(candidate, _)| candidate == key)
                    .map(|(_, value)| value.clone())
            })
        };

        assert_eq!(resolve(&[]).expect("unset"), None);
        assert_eq!(
            resolve(&[(SHARED_CORS_SUBDOMAIN_BASE_DOMAINS_ENV_KEY, "")]).expect("empty"),
            None
        );
        assert!(resolve(&[(SHARED_CORS_SUBDOMAIN_SCHEMES_ENV_KEY, "https")]).is_err());
        assert!(resolve(&[(SHARED_CORS_SUBDOMAIN_BASE_DOMAINS_ENV_KEY, "sdkwork.com")]).is_err());

        let configured = resolve(&[
            (
                SHARED_CORS_SUBDOMAIN_BASE_DOMAINS_ENV_KEY,
                "sdkwork.com, birdcoder.com",
            ),
            (SHARED_CORS_SUBDOMAIN_SCHEMES_ENV_KEY, "http,https"),
        ])
        .expect("valid")
        .expect("configured");
        assert!(configured.matches("https://platform.birdcoder.com"));
    }
}
