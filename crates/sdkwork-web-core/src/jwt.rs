//! Production JWT verification hooks — business supplies crypto; framework maps claims.

use crate::error::WebFrameworkError;
use crate::jwt_claims::validate_jwt_token_type_claim;
use crate::parsers::{
    optional_claim, parse_auth_level, parse_claims, parse_deployment_mode, parse_environment,
    parse_login_scope, require_sdkwork_jwt, required_claim, split_claim, AccessTokenClaims,
    AccessTokenParser, AuthTokenClaims, AuthTokenParser,
};
use crate::token_version::{validate_token_version_claims, TokenVersionPolicy};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::sync::Arc;

type HmacSha256 = Hmac<Sha256>;

pub(crate) mod codec {
    use super::*;
    use crate::error::WebFrameworkError;
    use crate::parsers::require_sdkwork_jwt;

    pub fn split_jwt_compact(jwt: &str) -> Result<(String, String, String), WebFrameworkError> {
        require_sdkwork_jwt(jwt, "token")?;
        let mut parts = jwt.split('.');
        let header_b64 = parts.next().ok_or_else(|| {
            WebFrameworkError::invalid_credentials("JWT compact serialization is incomplete")
        })?;
        let payload_b64 = parts.next().ok_or_else(|| {
            WebFrameworkError::invalid_credentials("JWT compact serialization is incomplete")
        })?;
        let signature_b64 = parts.next().ok_or_else(|| {
            WebFrameworkError::invalid_credentials("JWT compact serialization is incomplete")
        })?;
        if parts.next().is_some() {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT compact serialization has unexpected segments",
            ));
        }
        Ok((
            header_b64.to_owned(),
            payload_b64.to_owned(),
            signature_b64.to_owned(),
        ))
    }

    pub fn verify_hs256_signature(
        secret: &[u8],
        header_b64: &str,
        payload_b64: &str,
        signature_b64: &str,
    ) -> Result<(), WebFrameworkError> {
        let header = decode_base64url_json(header_b64).map_err(|error| {
            WebFrameworkError::invalid_credentials(format!("invalid JWT header: {error}"))
        })?;
        let algorithm = header
            .get("alg")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if algorithm != "HS256" {
            return Err(WebFrameworkError::invalid_credentials(format!(
                "unsupported JWT algorithm `{algorithm}`; expected HS256"
            )));
        }
        let signing_input = format!("{header_b64}.{payload_b64}");
        let mut mac = HmacSha256::new_from_slice(secret).map_err(|error| {
            WebFrameworkError::dependency_unavailable(format!("JWT HMAC key error: {error}"))
        })?;
        mac.update(signing_input.as_bytes());
        let expected = mac.finalize().into_bytes();
        let actual = decode_base64url(signature_b64).map_err(|error| {
            WebFrameworkError::invalid_credentials(format!(
                "invalid JWT signature encoding: {error}"
            ))
        })?;
        if actual.len() != expected.len()
            || !subtle_constant_time_eq(actual.as_slice(), expected.as_slice())
        {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT signature verification failed",
            ));
        }
        Ok(())
    }

    pub fn verify_rs256_signature(
        public_key_spki_der: &[u8],
        header_b64: &str,
        payload_b64: &str,
        signature_b64: &str,
    ) -> Result<(), WebFrameworkError> {
        use rsa::pkcs1v15::VerifyingKey;
        use rsa::pkcs8::DecodePublicKey;
        use rsa::signature::Verifier;
        use sha2::Sha256;

        let public_key =
            rsa::RsaPublicKey::from_public_key_der(public_key_spki_der).map_err(|error| {
                WebFrameworkError::dependency_unavailable(format!(
                    "invalid RS256 tenant public key: {error}"
                ))
            })?;
        let verifying_key = VerifyingKey::<Sha256>::new(public_key);
        let signing_input = format!("{header_b64}.{payload_b64}");
        let signature = decode_base64url(signature_b64).map_err(|error| {
            WebFrameworkError::invalid_credentials(format!(
                "invalid JWT signature encoding: {error}"
            ))
        })?;
        let signature = rsa::pkcs1v15::Signature::try_from(signature.as_slice()).map_err(|_| {
            WebFrameworkError::invalid_credentials("JWT RS256 signature has invalid length")
        })?;
        verifying_key
            .verify(signing_input.as_bytes(), &signature)
            .map_err(|_| {
                WebFrameworkError::invalid_credentials("JWT RS256 signature verification failed")
            })
    }

    pub fn decode_base64url_json(input: &str) -> Result<serde_json::Value, String> {
        let bytes = decode_base64url(input)?;
        let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
        serde_json::from_str(text).map_err(|error| error.to_string())
    }

    pub fn decode_base64url(input: &str) -> Result<Vec<u8>, String> {
        let mut output = Vec::with_capacity(input.len() * 3 / 4);
        let mut buffer: u32 = 0;
        let mut bits: u8 = 0;

        for byte in input.bytes() {
            if byte == b'=' {
                break;
            }
            let value = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'-' | b'+' => 62,
                b'_' | b'/' => 63,
                _ => return Err(format!("invalid base64url character `{byte}`")),
            };
            buffer = (buffer << 6) | u32::from(value);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                output.push((buffer >> bits) as u8);
            }
        }
        Ok(output)
    }

    pub fn subtle_constant_time_eq(left: &[u8], right: &[u8]) -> bool {
        if left.len() != right.len() {
            return false;
        }
        let mut diff = 0u8;
        for (left, right) in left.iter().zip(right.iter()) {
            diff |= left ^ right;
        }
        diff == 0
    }
}

/// Verify JWT signature and return SDKWork-normalized claim strings.
pub trait JwtVerifier: Send + Sync + Clone {
    fn verify_and_decode_claims(
        &self,
        jwt: &str,
    ) -> Result<BTreeMap<String, String>, WebFrameworkError>;
}

/// Production auth/access parsers that require signature verification.
#[derive(Clone)]
pub struct VerifyingAuthTokenParser<V> {
    verifier: Arc<V>,
}

#[derive(Clone)]
pub struct VerifyingAccessTokenParser<V> {
    verifier: Arc<V>,
}

impl<V> VerifyingAuthTokenParser<V> {
    pub fn new(verifier: Arc<V>) -> Self {
        Self { verifier }
    }
}

impl<V> VerifyingAccessTokenParser<V> {
    pub fn new(verifier: Arc<V>) -> Self {
        Self { verifier }
    }
}

#[async_trait]
impl<V> AuthTokenParser for VerifyingAuthTokenParser<V>
where
    V: JwtVerifier + 'static,
{
    async fn parse_auth_token(&self, raw: &str) -> Result<AuthTokenClaims, WebFrameworkError> {
        let claims = self.verifier.verify_and_decode_claims(raw)?;
        map_auth_token_claims(claims)
    }
}

#[async_trait]
impl<V> AccessTokenParser for VerifyingAccessTokenParser<V>
where
    V: JwtVerifier + 'static,
{
    async fn parse_access_token(&self, raw: &str) -> Result<AccessTokenClaims, WebFrameworkError> {
        let claims = self.verifier.verify_and_decode_claims(raw)?;
        map_access_token_claims(claims)
    }
}

/// Decodes JWT payload segments only — **does not verify signatures**.
/// Production apps must supply a business [`JwtVerifier`] with real crypto.
#[derive(Clone)]
pub struct PayloadOnlyJwtVerifier;

/// Backward-compatible alias — prefer [`PayloadOnlyJwtVerifier`] in new code.
pub type StrictJwtVerifier = PayloadOnlyJwtVerifier;

impl JwtVerifier for PayloadOnlyJwtVerifier {
    fn verify_and_decode_claims(
        &self,
        jwt: &str,
    ) -> Result<BTreeMap<String, String>, WebFrameworkError> {
        require_sdkwork_jwt(jwt, "token")?;
        let claims = parse_claims(jwt)?;
        validate_token_version_claims(&claims, &TokenVersionPolicy::standard())?;
        if claims.is_empty() {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT verification produced no claims",
            ));
        }
        Ok(claims)
    }
}

/// HS256 JWT verifier for standalone/dev deployments using a shared bootstrap secret.
///
/// Production SaaS should prefer tenant-bound signing keys from IAM; this verifier satisfies
/// production assembly when `SDKWORK_WEB_FRAMEWORK_JWT_HS256_SECRET` is configured.
#[derive(Clone)]
pub struct HmacSha256JwtVerifier {
    secret: Arc<Vec<u8>>,
}

impl HmacSha256JwtVerifier {
    pub fn new(secret: impl AsRef<[u8]>) -> Self {
        Self {
            secret: Arc::new(secret.as_ref().to_vec()),
        }
    }
}

impl JwtVerifier for HmacSha256JwtVerifier {
    fn verify_and_decode_claims(
        &self,
        jwt: &str,
    ) -> Result<BTreeMap<String, String>, WebFrameworkError> {
        let (header_b64, payload_b64, signature_b64) = codec::split_jwt_compact(jwt)?;
        codec::verify_hs256_signature(
            self.secret.as_ref(),
            &header_b64,
            &payload_b64,
            &signature_b64,
        )?;
        let claims = parse_claims(jwt)?;
        validate_token_version_claims(&claims, &TokenVersionPolicy::standard())?;
        if claims.is_empty() {
            return Err(WebFrameworkError::invalid_credentials(
                "JWT verification produced no claims",
            ));
        }
        Ok(claims)
    }
}

pub fn is_hmac_sha256_jwt_verifier<V>(verifier: &V) -> bool
where
    V: JwtVerifier + Any,
{
    verifier.type_id() == TypeId::of::<HmacSha256JwtVerifier>()
}

pub fn uses_global_shared_jwt_verifier<V>(verifier: &V) -> bool
where
    V: JwtVerifier + Any,
{
    is_payload_only_jwt_verifier(verifier) || is_hmac_sha256_jwt_verifier(verifier)
}

pub fn is_payload_only_jwt_verifier<V>(verifier: &V) -> bool
where
    V: JwtVerifier + Any,
{
    verifier.type_id() == TypeId::of::<PayloadOnlyJwtVerifier>()
}

pub fn validate_production_jwt_verifier<V>(verifier: &V) -> Result<(), String>
where
    V: JwtVerifier + Any,
{
    if uses_global_shared_jwt_verifier(verifier) {
        return Err(
            "production assembly must not use PayloadOnlyJwtVerifier or HmacSha256JwtVerifier; wire TenantBoundJwtVerifier with TenantSigningKeyLookup".into(),
        );
    }
    Ok(())
}

fn map_auth_token_claims(
    claims: BTreeMap<String, String>,
) -> Result<AuthTokenClaims, WebFrameworkError> {
    validate_token_version_claims(&claims, &TokenVersionPolicy::standard())?;
    validate_jwt_token_type_claim(&claims, "auth")?;
    let organization_id = optional_claim(&claims, "organization_id");
    Ok(AuthTokenClaims {
        tenant_id: optional_claim(&claims, "tenant_id"),
        login_scope: parse_login_scope(
            claims.get("login_scope").map(String::as_str),
            organization_id.as_deref(),
        )?,
        organization_id,
        user_id: required_claim(&claims, "user_id")?,
        session_id: optional_claim(&claims, "session_id"),
        app_id: optional_claim(&claims, "app_id"),
        auth_level: parse_auth_level(claims.get("auth_level").map(String::as_str)),
        data_scope: split_claim(claims.get("data_scope")),
        permission_scope: split_claim(claims.get("permission_scope")),
        subject_type: optional_claim(&claims, "subject_type"),
        metadata: claims,
    })
}

fn map_access_token_claims(
    claims: BTreeMap<String, String>,
) -> Result<AccessTokenClaims, WebFrameworkError> {
    validate_token_version_claims(&claims, &TokenVersionPolicy::standard())?;
    validate_jwt_token_type_claim(&claims, "access")?;
    let organization_id = optional_claim(&claims, "organization_id");
    Ok(AccessTokenClaims {
        tenant_id: required_claim(&claims, "tenant_id")?,
        login_scope: parse_login_scope(
            claims.get("login_scope").map(String::as_str),
            organization_id.as_deref(),
        )?,
        organization_id,
        user_id: optional_claim(&claims, "user_id"),
        session_id: optional_claim(&claims, "session_id"),
        app_id: required_claim(&claims, "app_id")?,
        environment: parse_environment(claims.get("environment").map(String::as_str)),
        deployment_mode: parse_deployment_mode(claims.get("deployment_mode").map(String::as_str)),
        data_scope: split_claim(claims.get("data_scope")),
        permission_scope: split_claim(claims.get("permission_scope")),
        subject_type: optional_claim(&claims, "subject_type"),
        metadata: claims,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::WebFrameworkErrorKind;
    use serde_json::json;

    #[test]
    fn strict_verifier_rejects_inline_claim_strings() {
        let verifier = PayloadOnlyJwtVerifier;
        let error = verifier
            .verify_and_decode_claims("tenant_id=t1;user_id=u1")
            .expect_err("inline");
        assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    }

    #[test]
    fn hmac_verifier_accepts_valid_hs256_token() {
        use crate::jwt_fixtures::encode_hs256_test_jwt;
        let token = encode_hs256_test_jwt(
            "test-secret",
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
        let verifier = HmacSha256JwtVerifier::new("test-secret");
        let claims = verifier
            .verify_and_decode_claims(&token)
            .expect("valid hs256 token");
        assert_eq!(
            "100001",
            claims.get("tenant_id").map(String::as_str).unwrap()
        );
    }

    #[test]
    fn hmac_verifier_rejects_invalid_signature() {
        use crate::jwt_fixtures::encode_hs256_test_jwt;
        let token = encode_hs256_test_jwt(
            "test-secret",
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
        let verifier = HmacSha256JwtVerifier::new("other-secret");
        let error = verifier
            .verify_and_decode_claims(&token)
            .expect_err("bad signature");
        assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    }
}
