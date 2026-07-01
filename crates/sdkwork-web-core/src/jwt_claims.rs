//! Production JWT claim validation (API_SPEC §10.821–825 / SECURITY_SPEC §1).

use crate::error::WebFrameworkError;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Policy for temporal and audience claims on production JWTs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JwtProductionClaimPolicy {
    pub require_exp: bool,
    pub clock_skew_secs: u64,
    pub expected_issuers: Vec<String>,
    pub expected_audiences: Vec<String>,
}

impl JwtProductionClaimPolicy {
    pub fn production() -> Self {
        Self {
            require_exp: true,
            clock_skew_secs: 60,
            expected_issuers: Vec::new(),
            expected_audiences: Vec::new(),
        }
    }

    pub fn with_issuer_audience(mut self, issuers: Vec<String>, audiences: Vec<String>) -> Self {
        self.expected_issuers = issuers;
        self.expected_audiences = audiences;
        self
    }

    /// Production SaaS policy with IAM issuer/audience allow-lists (IAM_SPEC / SECURITY_SPEC).
    pub fn saas_production(issuers: Vec<String>, audiences: Vec<String>) -> Self {
        Self::production().with_issuer_audience(issuers, audiences)
    }

    /// Returns `true` when IAM issuer and audience allow-lists are configured for SaaS production.
    pub fn has_saas_issuer_audience_allowlist(&self) -> bool {
        !self.expected_issuers.is_empty() && !self.expected_audiences.is_empty()
    }
}

impl Default for JwtProductionClaimPolicy {
    fn default() -> Self {
        Self::production()
    }
}

pub fn validate_jwt_token_type_claim(
    claims: &BTreeMap<String, String>,
    expected: &str,
) -> Result<(), WebFrameworkError> {
    let actual = claims
        .get("token_type")
        .map(String::as_str)
        .unwrap_or_default();
    if actual.trim().is_empty() {
        return Err(WebFrameworkError::invalid_credentials(
            "JWT token_type claim is required",
        ));
    }
    if actual != expected {
        return Err(WebFrameworkError::invalid_credentials(format!(
            "JWT token_type `{actual}` does not match expected `{expected}`"
        )));
    }
    Ok(())
}

pub fn validate_jwt_temporal_claims(
    claims: &BTreeMap<String, String>,
    policy: &JwtProductionClaimPolicy,
    now: SystemTime,
) -> Result<(), WebFrameworkError> {
    let now_secs = system_time_secs(now)?;

    if policy.require_exp {
        let exp = parse_required_numeric_claim(claims, "exp")?;
        if now_secs.saturating_add(policy.clock_skew_secs) >= exp {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT exp claim is expired",
            ));
        }
    }

    if let Some(nbf) = parse_optional_numeric_claim(claims, "nbf")? {
        if now_secs.saturating_sub(policy.clock_skew_secs) < nbf {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT nbf claim is not yet valid",
            ));
        }
    }

    validate_jwt_issuer_audience_claims(claims, policy)
}

fn validate_jwt_issuer_audience_claims(
    claims: &BTreeMap<String, String>,
    policy: &JwtProductionClaimPolicy,
) -> Result<(), WebFrameworkError> {
    if !policy.expected_issuers.is_empty() {
        let iss = claims.get("iss").map(String::as_str).unwrap_or_default();
        if iss.trim().is_empty() || !policy.expected_issuers.iter().any(|v| v == iss) {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT iss claim is missing or not allowed",
            ));
        }
    }
    if !policy.expected_audiences.is_empty() {
        let aud = claims.get("aud").map(String::as_str).unwrap_or_default();
        if aud.trim().is_empty() || !policy.expected_audiences.iter().any(|v| v == aud) {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT aud claim is missing or not allowed",
            ));
        }
    }
    Ok(())
}

fn parse_required_numeric_claim(
    claims: &BTreeMap<String, String>,
    name: &str,
) -> Result<u64, WebFrameworkError> {
    parse_optional_numeric_claim(claims, name)?.ok_or_else(|| {
        WebFrameworkError::invalid_credentials(format!("JWT {name} claim is required"))
    })
}

fn parse_optional_numeric_claim(
    claims: &BTreeMap<String, String>,
    name: &str,
) -> Result<Option<u64>, WebFrameworkError> {
    let Some(raw) = claims.get(name) else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let value = raw.parse::<u64>().map_err(|_| {
        WebFrameworkError::invalid_credentials(format!("JWT {name} claim must be a numeric epoch"))
    })?;
    Ok(Some(value))
}

fn system_time_secs(time: SystemTime) -> Result<u64, WebFrameworkError> {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| {
            WebFrameworkError::dependency_unavailable(format!(
                "system clock is before unix epoch: {error}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn rejects_wrong_token_type() {
        let claims = claims(&[("token_type", "access")]);
        let error = validate_jwt_token_type_claim(&claims, "auth").expect_err("type");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
    }

    #[test]
    fn rejects_expired_token() {
        let claims = claims(&[("exp", "1")]);
        let policy = JwtProductionClaimPolicy::production();
        let now = UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        let error = validate_jwt_temporal_claims(&claims, &policy, now).expect_err("expired");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
    }

    #[test]
    fn accepts_valid_exp() {
        let claims = claims(&[("exp", "20000")]);
        let policy = JwtProductionClaimPolicy::production();
        let now = UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        validate_jwt_temporal_claims(&claims, &policy, now).expect("valid exp");
    }

    #[test]
    fn saas_production_accepts_matching_iss_and_aud() {
        let claims = claims(&[
            ("exp", "20000"),
            ("iss", "https://iam.example"),
            ("aud", "appbase"),
        ]);
        let policy = JwtProductionClaimPolicy::saas_production(
            vec!["https://iam.example".to_owned()],
            vec!["appbase".to_owned()],
        );
        let now = UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        validate_jwt_temporal_claims(&claims, &policy, now).expect("valid iss/aud");
    }

    #[test]
    fn saas_production_allowlist_helper_detects_configured_policy() {
        let production = JwtProductionClaimPolicy::production();
        assert!(!production.has_saas_issuer_audience_allowlist());
        let saas = JwtProductionClaimPolicy::saas_production(
            vec!["https://iam.example".to_owned()],
            vec!["appbase".to_owned()],
        );
        assert!(saas.has_saas_issuer_audience_allowlist());
    }

    #[test]
    fn saas_production_rejects_wrong_audience() {
        let claims = claims(&[
            ("exp", "20000"),
            ("iss", "https://iam.example"),
            ("aud", "other-app"),
        ]);
        let policy = JwtProductionClaimPolicy::saas_production(
            vec!["https://iam.example".to_owned()],
            vec!["appbase".to_owned()],
        );
        let now = UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        let error = validate_jwt_temporal_claims(&claims, &policy, now).expect_err("aud");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
        assert!(error.message.contains("aud"));
    }
}
