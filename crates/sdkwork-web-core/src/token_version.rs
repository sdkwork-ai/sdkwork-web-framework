//! SDKWork session JWT version policy — shared by issuers and validators.

use crate::error::WebFrameworkError;
use serde_json::Value;
use std::collections::BTreeMap;

/// Current `token_version` claim value for newly issued auth/access JWTs.
pub const SDKWORK_TOKEN_VERSION_CURRENT: u32 = 1;

/// Accept/reject policy for the `token_version` claim during validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenVersionPolicy {
    pub current: u32,
    pub minimum_accepted: u32,
    pub maximum_accepted: u32,
}

impl Default for TokenVersionPolicy {
    fn default() -> Self {
        Self::standard()
    }
}

impl TokenVersionPolicy {
    pub fn standard() -> Self {
        Self {
            current: SDKWORK_TOKEN_VERSION_CURRENT,
            minimum_accepted: SDKWORK_TOKEN_VERSION_CURRENT,
            maximum_accepted: SDKWORK_TOKEN_VERSION_CURRENT,
        }
    }

    /// Accept both the current version and a newer version during coordinated rollout.
    pub fn with_upgrade_window(next_version: u32) -> Self {
        Self {
            current: SDKWORK_TOKEN_VERSION_CURRENT,
            minimum_accepted: SDKWORK_TOKEN_VERSION_CURRENT,
            maximum_accepted: next_version.max(SDKWORK_TOKEN_VERSION_CURRENT),
        }
    }
}

pub fn stamp_token_version() -> u32 {
    SDKWORK_TOKEN_VERSION_CURRENT
}

pub fn parse_token_version(raw: &str) -> Result<u32, WebFrameworkError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(WebFrameworkError::invalid_credentials(
            "token_version claim is required",
        ));
    }
    let version = raw.parse::<u32>().map_err(|_| {
        WebFrameworkError::invalid_credentials(format!(
            "token_version claim must be a non-negative integer, got {raw}"
        ))
    })?;
    Ok(version)
}

pub fn validate_token_version(
    version: u32,
    policy: &TokenVersionPolicy,
) -> Result<(), WebFrameworkError> {
    if version < policy.minimum_accepted {
        return Err(WebFrameworkError::invalid_credentials(format!(
            "token_version {version} is below the accepted minimum {}",
            policy.minimum_accepted
        )));
    }
    if version > policy.maximum_accepted {
        return Err(WebFrameworkError::invalid_credentials(format!(
            "token_version {version} exceeds the accepted maximum {}",
            policy.maximum_accepted
        )));
    }
    Ok(())
}

pub fn validate_token_version_claims(
    claims: &BTreeMap<String, String>,
    policy: &TokenVersionPolicy,
) -> Result<(), WebFrameworkError> {
    let raw = claims
        .get("token_version")
        .map(String::as_str)
        .ok_or_else(|| WebFrameworkError::invalid_credentials("token_version claim is required"))?;
    let version = parse_token_version(raw)?;
    validate_token_version(version, policy)
}

pub fn extract_token_version_from_json(value: &Value) -> Result<u32, WebFrameworkError> {
    let claim = value
        .get("token_version")
        .ok_or_else(|| WebFrameworkError::invalid_credentials("token_version claim is required"))?;
    match claim {
        Value::Number(number) => number
            .as_u64()
            .or_else(|| {
                number
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .map(|value| value as u64)
            })
            .map(|value| value as u32)
            .ok_or_else(|| {
                WebFrameworkError::invalid_credentials(
                    "token_version claim must be a non-negative integer",
                )
            }),
        Value::String(raw) => parse_token_version(raw),
        _ => Err(WebFrameworkError::invalid_credentials(
            "token_version claim must be a non-negative integer",
        )),
    }
}

pub fn validate_token_version_json(
    payload: &Value,
    policy: &TokenVersionPolicy,
) -> Result<(), WebFrameworkError> {
    let version = extract_token_version_from_json(payload)?;
    validate_token_version(version, policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::WebFrameworkErrorKind;

    #[test]
    fn standard_policy_accepts_current_version() {
        let policy = TokenVersionPolicy::standard();
        assert!(validate_token_version(SDKWORK_TOKEN_VERSION_CURRENT, &policy).is_ok());
    }

    #[test]
    fn standard_policy_rejects_missing_claim() {
        let claims = BTreeMap::new();
        let error = validate_token_version_claims(&claims, &TokenVersionPolicy::standard())
            .expect_err("missing");
        assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    }

    #[test]
    fn standard_policy_rejects_future_version() {
        let error = validate_token_version(
            SDKWORK_TOKEN_VERSION_CURRENT + 1,
            &TokenVersionPolicy::standard(),
        )
        .expect_err("future");
        assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    }

    #[test]
    fn upgrade_window_accepts_next_version() {
        let policy = TokenVersionPolicy::with_upgrade_window(SDKWORK_TOKEN_VERSION_CURRENT + 1);
        assert!(validate_token_version(SDKWORK_TOKEN_VERSION_CURRENT + 1, &policy).is_ok());
    }

    #[test]
    fn json_payload_validation_matches_claim_map_rules() {
        let payload = serde_json::json!({ "token_version": SDKWORK_TOKEN_VERSION_CURRENT });
        assert!(validate_token_version_json(&payload, &TokenVersionPolicy::standard()).is_ok());
    }
}
