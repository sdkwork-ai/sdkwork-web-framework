//! Tenant-bound JWT verification (IAM_SPEC / API_SPEC §10.738).

use crate::error::WebFrameworkError;
use crate::jwt::codec::{decode_base64url_json, split_jwt_compact};
use crate::jwt::JwtVerifier;
use crate::jwt_claims::{validate_jwt_temporal_claims, JwtProductionClaimPolicy};
use crate::parsers::{parse_claims, required_claim};
use crate::token_version::{validate_token_version_claims, TokenVersionPolicy};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::SystemTime;

/// EP-05e: IAM session revocation lookup for production SaaS JWT verification.
pub trait JwtSessionRevocationChecker: Send + Sync + Clone {
    fn is_session_revoked(&self, session_id: &str) -> Result<bool, WebFrameworkError>;
}

/// Default no-op checker for control-plane bootstrap and local tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOpJwtSessionRevocationChecker;

impl JwtSessionRevocationChecker for NoOpJwtSessionRevocationChecker {
    fn is_session_revoked(&self, _: &str) -> Result<bool, WebFrameworkError> {
        Ok(false)
    }
}

/// In-memory revocation set for tests and local wiring.
#[derive(Clone, Debug, Default)]
pub struct StaticJwtSessionRevocationChecker {
    revoked_sessions: Arc<BTreeSet<String>>,
}

impl StaticJwtSessionRevocationChecker {
    pub fn with_revoked(sessions: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            revoked_sessions: Arc::new(
                sessions
                    .into_iter()
                    .map(Into::into)
                    .collect::<BTreeSet<String>>(),
            ),
        }
    }
}

impl JwtSessionRevocationChecker for StaticJwtSessionRevocationChecker {
    fn is_session_revoked(&self, session_id: &str) -> Result<bool, WebFrameworkError> {
        Ok(self.revoked_sessions.contains(session_id))
    }
}

fn session_id_from_claims(claims: &BTreeMap<String, String>) -> Option<String> {
    claims
        .get("session_id")
        .or_else(|| claims.get("sid"))
        .filter(|value| !value.trim().is_empty())
        .cloned()
}

fn validate_jwt_session_revocation<R: JwtSessionRevocationChecker>(
    claims: &BTreeMap<String, String>,
    checker: &R,
    enforce_session_claim: bool,
) -> Result<(), WebFrameworkError> {
    let Some(session_id) = session_id_from_claims(claims) else {
        if enforce_session_claim {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT session_id claim is required for session revocation check",
            ));
        }
        return Ok(());
    };
    if checker.is_session_revoked(&session_id)? {
        return Err(WebFrameworkError::invalid_credentials(
            "JWT session has been revoked",
        ));
    }
    Ok(())
}

/// Tenant signing key algorithm (IAM `iam_tenant_signing_key`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TenantSigningKeyAlgorithm {
    Hs256,
    Rs256,
}

/// Tenant signing key material resolved from `kid` (IAM `iam_tenant_signing_key`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TenantSigningKeyMaterial {
    pub tenant_id: String,
    pub key_id: String,
    pub algorithm: TenantSigningKeyAlgorithm,
    pub symmetric_secret: Option<Arc<Vec<u8>>>,
    pub rsa_public_key_spki: Option<Arc<Vec<u8>>>,
}

impl TenantSigningKeyMaterial {
    pub fn hs256(
        tenant_id: impl Into<String>,
        key_id: impl Into<String>,
        secret: impl AsRef<[u8]>,
    ) -> Self {
        let key_id = key_id.into();
        Self {
            tenant_id: tenant_id.into(),
            key_id: key_id.clone(),
            algorithm: TenantSigningKeyAlgorithm::Hs256,
            symmetric_secret: Some(Arc::new(secret.as_ref().to_vec())),
            rsa_public_key_spki: None,
        }
    }

    pub fn rs256_spki(
        tenant_id: impl Into<String>,
        key_id: impl Into<String>,
        public_key_spki_der: impl Into<Vec<u8>>,
    ) -> Self {
        let key_id = key_id.into();
        Self {
            tenant_id: tenant_id.into(),
            key_id: key_id.clone(),
            algorithm: TenantSigningKeyAlgorithm::Rs256,
            symmetric_secret: None,
            rsa_public_key_spki: Some(Arc::new(public_key_spki_der.into())),
        }
    }
}

/// EP-05d extension: resolve tenant-bound signing keys by JWT `kid` and `alg`.
pub trait TenantSigningKeyLookup: Send + Sync + Clone {
    fn resolve_hs256_key(
        &self,
        key_id: &str,
    ) -> Result<TenantSigningKeyMaterial, WebFrameworkError> {
        let _ = key_id;
        Err(WebFrameworkError::invalid_credentials(
            "HS256 tenant signing key is not configured for this lookup",
        ))
    }

    fn resolve_rs256_key(
        &self,
        key_id: &str,
    ) -> Result<TenantSigningKeyMaterial, WebFrameworkError> {
        let _ = key_id;
        Err(WebFrameworkError::invalid_credentials(
            "RS256 tenant signing key is not configured for this lookup",
        ))
    }

    fn resolve_signing_key(
        &self,
        key_id: &str,
        algorithm: &str,
    ) -> Result<TenantSigningKeyMaterial, WebFrameworkError> {
        match algorithm {
            "HS256" => self.resolve_hs256_key(key_id),
            "RS256" => self.resolve_rs256_key(key_id),
            other => Err(WebFrameworkError::invalid_credentials(format!(
                "unsupported JWT algorithm `{other}` for tenant-bound verification"
            ))),
        }
    }
}

/// In-memory lookup table for tests and local bootstrap wiring.
#[derive(Clone, Default)]
pub struct StaticTenantSigningKeyLookup {
    keys: Arc<BTreeMap<String, TenantSigningKeyMaterial>>,
}

impl StaticTenantSigningKeyLookup {
    pub fn new(keys: BTreeMap<String, TenantSigningKeyMaterial>) -> Self {
        Self {
            keys: Arc::new(keys),
        }
    }
}

impl TenantSigningKeyLookup for StaticTenantSigningKeyLookup {
    fn resolve_signing_key(
        &self,
        key_id: &str,
        algorithm: &str,
    ) -> Result<TenantSigningKeyMaterial, WebFrameworkError> {
        let material = self.keys.get(key_id).cloned().ok_or_else(|| {
            WebFrameworkError::invalid_credentials(format!(
                "unknown tenant signing key id `{key_id}`"
            ))
        })?;
        let expected = match material.algorithm {
            TenantSigningKeyAlgorithm::Hs256 => "HS256",
            TenantSigningKeyAlgorithm::Rs256 => "RS256",
        };
        if expected != algorithm {
            return Err(WebFrameworkError::invalid_credentials(format!(
                "JWT algorithm `{algorithm}` does not match signing key algorithm `{expected}`"
            )));
        }
        Ok(material)
    }
}

/// Standalone control-plane bootstrap lookup backed by env-provided secret + tenant id.
#[derive(Clone, Debug)]
pub struct EnvBootstrapTenantSigningKeyLookup {
    material: TenantSigningKeyMaterial,
}

impl EnvBootstrapTenantSigningKeyLookup {
    pub fn new(
        tenant_id: impl Into<String>,
        key_id: impl Into<String>,
        secret: impl AsRef<[u8]>,
    ) -> Self {
        let key_id = key_id.into();
        Self {
            material: TenantSigningKeyMaterial::hs256(tenant_id, key_id, secret),
        }
    }
}

impl TenantSigningKeyLookup for EnvBootstrapTenantSigningKeyLookup {
    fn resolve_hs256_key(
        &self,
        key_id: &str,
    ) -> Result<TenantSigningKeyMaterial, WebFrameworkError> {
        if key_id == self.material.key_id {
            Ok(self.material.clone())
        } else {
            tracing::warn!(
                requested_kid = key_id,
                expected_kid = %self.material.key_id,
                "bootstrap signing key lookup rejected unknown kid"
            );
            Err(WebFrameworkError::invalid_credentials(
                "bootstrap signing key lookup rejected unknown key id",
            ))
        }
    }
}

/// Production JWT verifier: HS256 signature + `kid` tenant key binding + claim tenant match.
#[derive(Clone)]
pub struct TenantBoundJwtVerifier<L, Rev = NoOpJwtSessionRevocationChecker> {
    lookup: L,
    claim_policy: JwtProductionClaimPolicy,
    revocation_checker: Rev,
    enforce_session_revocation: bool,
}

impl<L> TenantBoundJwtVerifier<L, NoOpJwtSessionRevocationChecker> {
    pub fn new(lookup: L) -> Self {
        Self {
            lookup,
            claim_policy: JwtProductionClaimPolicy::production(),
            revocation_checker: NoOpJwtSessionRevocationChecker,
            enforce_session_revocation: false,
        }
    }
}

impl<L, Rev> TenantBoundJwtVerifier<L, Rev> {
    pub fn with_claim_policy(mut self, claim_policy: JwtProductionClaimPolicy) -> Self {
        self.claim_policy = claim_policy;
        self
    }

    pub fn with_session_revocation_checker<Rev2>(
        self,
        revocation_checker: Rev2,
    ) -> TenantBoundJwtVerifier<L, Rev2>
    where
        Rev2: JwtSessionRevocationChecker,
    {
        TenantBoundJwtVerifier {
            lookup: self.lookup,
            claim_policy: self.claim_policy,
            revocation_checker,
            enforce_session_revocation: true,
        }
    }
}

impl<L, Rev> JwtVerifier for TenantBoundJwtVerifier<L, Rev>
where
    L: TenantSigningKeyLookup,
    Rev: JwtSessionRevocationChecker,
{
    fn verify_and_decode_claims(
        &self,
        jwt: &str,
    ) -> Result<BTreeMap<String, String>, WebFrameworkError> {
        let (header_b64, payload_b64, signature_b64) = split_jwt_compact(jwt)?;
        let header = decode_base64url_json(&header_b64).map_err(|error| {
            WebFrameworkError::invalid_credentials(format!("invalid JWT header: {error}"))
        })?;
        let algorithm = header
            .get("alg")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if algorithm != "HS256" && algorithm != "RS256" {
            return Err(WebFrameworkError::invalid_credentials(format!(
                "unsupported JWT algorithm `{algorithm}`; expected HS256 or RS256"
            )));
        }
        let key_id = header
            .get("kid")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                WebFrameworkError::invalid_credentials(
                    "tenant-bound JWT verification requires header kid",
                )
            })?;
        let material = self.lookup.resolve_signing_key(key_id, algorithm)?;
        match material.algorithm {
            TenantSigningKeyAlgorithm::Hs256 => {
                let secret = material.symmetric_secret.as_ref().ok_or_else(|| {
                    WebFrameworkError::dependency_unavailable(
                        "HS256 signing key material is missing symmetric secret",
                    )
                })?;
                crate::jwt::codec::verify_hs256_signature(
                    secret.as_ref(),
                    &header_b64,
                    &payload_b64,
                    &signature_b64,
                )?;
            }
            TenantSigningKeyAlgorithm::Rs256 => {
                let public_key = material.rsa_public_key_spki.as_ref().ok_or_else(|| {
                    WebFrameworkError::dependency_unavailable(
                        "RS256 signing key material is missing SPKI public key",
                    )
                })?;
                crate::jwt::codec::verify_rs256_signature(
                    public_key.as_ref(),
                    &header_b64,
                    &payload_b64,
                    &signature_b64,
                )?;
            }
        }
        let claims = parse_claims(jwt)?;
        validate_token_version_claims(&claims, &TokenVersionPolicy::standard())?;
        validate_jwt_temporal_claims(&claims, &self.claim_policy, SystemTime::now())?;
        validate_jwt_session_revocation(
            &claims,
            &self.revocation_checker,
            self.enforce_session_revocation,
        )?;
        let claim_tenant = required_claim(&claims, "tenant_id")?;
        if claim_tenant != material.tenant_id {
            return Err(WebFrameworkError::invalid_credentials(format!(
                "JWT tenant_id `{claim_tenant}` does not match signing key tenant `{}`",
                material.tenant_id
            )));
        }
        if claims.is_empty() {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT verification produced no claims",
            ));
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jwt_claims::JwtProductionClaimPolicy;
    use crate::jwt_fixtures::encode_hs256_test_jwt_with_kid;
    use serde_json::json;

    #[test]
    fn tenant_bound_verifier_accepts_matching_kid_and_tenant() {
        let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
            "kid-1".to_owned(),
            TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
        )]));
        let token = encode_hs256_test_jwt_with_kid(
            "secret-1",
            "kid-1",
            json!({
                "token_type": "access",
                "tenant_id": "100001",
                "user_id": "user-1",
                "app_id": "appbase",
                "environment": "prod",
                "deployment_mode": "saas",
                "login_scope": "TENANT",
            }),
        );
        let verifier = TenantBoundJwtVerifier::new(lookup);
        let claims = verifier
            .verify_and_decode_claims(&token)
            .expect("valid tenant-bound token");
        assert_eq!(
            "100001",
            claims.get("tenant_id").map(String::as_str).unwrap()
        );
    }

    #[test]
    fn tenant_bound_verifier_rejects_mismatched_tenant_claim() {
        let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
            "kid-1".to_owned(),
            TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
        )]));
        let token = encode_hs256_test_jwt_with_kid(
            "secret-1",
            "kid-1",
            json!({
                "token_type": "access",
                "tenant_id": "100002",
                "user_id": "user-1",
                "app_id": "appbase",
                "environment": "prod",
                "deployment_mode": "saas",
                "login_scope": "TENANT",
            }),
        );
        let verifier = TenantBoundJwtVerifier::new(lookup);
        let error = verifier
            .verify_and_decode_claims(&token)
            .expect_err("tenant mismatch");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
    }

    #[test]
    fn tenant_bound_verifier_supports_key_rotation_overlap() {
        let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([
            (
                "kid-old".to_owned(),
                TenantSigningKeyMaterial::hs256("100001", "kid-old", b"secret-old"),
            ),
            (
                "kid-new".to_owned(),
                TenantSigningKeyMaterial::hs256("100001", "kid-new", b"secret-new"),
            ),
        ]));
        let verifier = TenantBoundJwtVerifier::new(lookup);
        let old_token = encode_hs256_test_jwt_with_kid(
            "secret-old",
            "kid-old",
            json!({
                "token_type": "access",
                "tenant_id": "100001",
                "user_id": "user-1",
                "app_id": "appbase",
                "environment": "prod",
                "deployment_mode": "saas",
                "login_scope": "TENANT",
            }),
        );
        let new_token = encode_hs256_test_jwt_with_kid(
            "secret-new",
            "kid-new",
            json!({
                "token_type": "access",
                "tenant_id": "100001",
                "user_id": "user-1",
                "app_id": "appbase",
                "environment": "prod",
                "deployment_mode": "saas",
                "login_scope": "TENANT",
            }),
        );
        verifier
            .verify_and_decode_claims(&old_token)
            .expect("old kid still valid during overlap");
        verifier
            .verify_and_decode_claims(&new_token)
            .expect("new kid valid");
    }

    #[test]
    fn tenant_bound_verifier_rejects_revoked_session() {
        let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
            "kid-1".to_owned(),
            TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
        )]));
        let token = encode_hs256_test_jwt_with_kid(
            "secret-1",
            "kid-1",
            json!({
                "token_type": "access",
                "tenant_id": "100001",
                "user_id": "user-1",
                "session_id": "session-revoked",
                "app_id": "appbase",
                "environment": "prod",
                "deployment_mode": "saas",
                "login_scope": "TENANT",
            }),
        );
        let verifier = TenantBoundJwtVerifier::new(lookup).with_session_revocation_checker(
            StaticJwtSessionRevocationChecker::with_revoked(["session-revoked"]),
        );
        let error = verifier
            .verify_and_decode_claims(&token)
            .expect_err("revoked session");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
        assert!(error.message.contains("revoked"));
    }

    #[test]
    fn tenant_bound_verifier_rejects_wrong_issuer() {
        let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
            "kid-1".to_owned(),
            TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
        )]));
        let token = encode_hs256_test_jwt_with_kid(
            "secret-1",
            "kid-1",
            json!({
                "token_type": "access",
                "tenant_id": "100001",
                "user_id": "user-1",
                "app_id": "appbase",
                "environment": "prod",
                "deployment_mode": "saas",
                "login_scope": "TENANT",
                "iss": "https://evil.example",
                "aud": "appbase",
            }),
        );
        let verifier = TenantBoundJwtVerifier::new(lookup).with_claim_policy(
            JwtProductionClaimPolicy::saas_production(
                vec!["https://iam.example".to_owned()],
                vec!["appbase".to_owned()],
            ),
        );
        let error = verifier
            .verify_and_decode_claims(&token)
            .expect_err("wrong iss");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
        assert!(error.message.contains("iss"));
    }

    #[test]
    fn tenant_bound_verifier_rejects_wrong_audience() {
        let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
            "kid-1".to_owned(),
            TenantSigningKeyMaterial::hs256("100001", "kid-1", b"secret-1"),
        )]));
        let token = encode_hs256_test_jwt_with_kid(
            "secret-1",
            "kid-1",
            json!({
                "token_type": "access",
                "tenant_id": "100001",
                "user_id": "user-1",
                "app_id": "appbase",
                "environment": "prod",
                "deployment_mode": "saas",
                "login_scope": "TENANT",
                "iss": "https://iam.example",
                "aud": "wrong-app",
            }),
        );
        let verifier = TenantBoundJwtVerifier::new(lookup).with_claim_policy(
            JwtProductionClaimPolicy::saas_production(
                vec!["https://iam.example".to_owned()],
                vec!["appbase".to_owned()],
            ),
        );
        let error = verifier
            .verify_and_decode_claims(&token)
            .expect_err("wrong aud");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
        assert!(error.message.contains("aud"));
    }

    #[test]
    fn tenant_bound_verifier_accepts_rs256_spki_key() {
        use crate::jwt_fixtures::{encode_rs256_test_jwt_with_kid, generate_rs256_test_keypair};

        let (private_key, spki_der) = generate_rs256_test_keypair();
        let lookup = StaticTenantSigningKeyLookup::new(BTreeMap::from([(
            "kid-rs256".to_owned(),
            TenantSigningKeyMaterial::rs256_spki("100001", "kid-rs256", spki_der),
        )]));
        let token = encode_rs256_test_jwt_with_kid(
            &private_key,
            "kid-rs256",
            json!({
                "token_type": "access",
                "tenant_id": "100001",
                "user_id": "user-1",
                "app_id": "appbase",
                "environment": "prod",
                "deployment_mode": "saas",
                "login_scope": "TENANT",
            }),
        );
        let verifier = TenantBoundJwtVerifier::new(lookup);
        verifier
            .verify_and_decode_claims(&token)
            .expect("valid rs256 tenant-bound token");
    }
}
