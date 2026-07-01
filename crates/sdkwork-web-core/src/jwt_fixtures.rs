//! Unsigned JWT builders for integration tests only.
//!
//! Production runtimes must verify signatures through [`JwtVerifier`] and tenant-bound signing keys.

use crate::token_version::stamp_token_version;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

const JWT_HEADER: &str = r#"{"alg":"none","typ":"JWT"}"#;

/// Stamps `exp` / `iat` on HS256 fixture payloads for production claim validation.
fn stamp_hs256_temporal_claims(payload: &mut Value) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    if let Some(object) = payload.as_object_mut() {
        object.entry("iat").or_insert(json!(now));
        object
            .entry("exp")
            .or_insert(json!(now.saturating_add(3600)));
    }
}

/// Builds a compact JWT string (`header.payload.signature`) for local tests.
pub fn encode_unsigned_test_jwt(payload: Value) -> String {
    let mut payload = payload;
    if let Some(object) = payload.as_object_mut() {
        object
            .entry("token_version")
            .or_insert(json!(stamp_token_version()));
    }
    format!(
        "{}.{}.test-signature",
        encode_base64url(JWT_HEADER.as_bytes()),
        encode_base64url(payload.to_string().as_bytes())
    )
}

/// Builds a compact HS256 JWT for tests that exercise [`crate::jwt::HmacSha256JwtVerifier`].
pub fn encode_hs256_test_jwt(secret: &str, payload: Value) -> String {
    encode_hs256_test_jwt_with_kid(secret, "bootstrap", payload)
}

/// Builds a compact HS256 JWT without header `kid` (negative tests for tenant-bound verification).
pub fn encode_hs256_test_jwt_without_kid(secret: &str, payload: Value) -> String {
    let mut payload = payload;
    if let Some(object) = payload.as_object_mut() {
        object
            .entry("token_version")
            .or_insert(json!(stamp_token_version()));
    }
    stamp_hs256_temporal_claims(&mut payload);
    let header = r#"{"alg":"HS256","typ":"JWT"}"#;
    let header_b64 = encode_base64url(header.as_bytes());
    let payload_b64 = encode_base64url(payload.to_string().as_bytes());
    let signing_input = format!("{header_b64}.{payload_b64}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(signing_input.as_bytes());
    let signature = mac.finalize().into_bytes();
    format!("{signing_input}.{}", encode_base64url(signature.as_ref()))
}

/// Builds a compact HS256 JWT with an explicit header `kid` for tenant-bound verifier tests.
pub fn encode_hs256_test_jwt_with_kid(secret: &str, key_id: &str, payload: Value) -> String {
    let mut payload = payload;
    if let Some(object) = payload.as_object_mut() {
        object
            .entry("token_version")
            .or_insert(json!(stamp_token_version()));
    }
    stamp_hs256_temporal_claims(&mut payload);
    let header = format!(r#"{{"alg":"HS256","typ":"JWT","kid":"{key_id}"}}"#);
    let header_b64 = encode_base64url(header.as_bytes());
    let payload_b64 = encode_base64url(payload.to_string().as_bytes());
    let signing_input = format!("{header_b64}.{payload_b64}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(signing_input.as_bytes());
    let signature = mac.finalize().into_bytes();
    format!("{signing_input}.{}", encode_base64url(signature.as_ref()))
}

/// Generates a 2048-bit RSA key pair for RS256 tenant-bound verifier tests.
pub fn generate_rs256_test_keypair() -> (rsa::RsaPrivateKey, Vec<u8>) {
    use rsa::pkcs8::EncodePublicKey;
    use rsa::rand_core::OsRng;
    let private_key = rsa::RsaPrivateKey::new(&mut OsRng, 2048).expect("rsa key");
    let spki = private_key
        .to_public_key()
        .to_public_key_der()
        .expect("spki der");
    (private_key, spki.to_vec())
}

/// Builds a compact RS256 JWT with header `kid` for tenant-bound verifier tests.
pub fn encode_rs256_test_jwt_with_kid(
    private_key: &rsa::RsaPrivateKey,
    key_id: &str,
    payload: Value,
) -> String {
    use rsa::pkcs1v15::SigningKey;
    use rsa::rand_core::OsRng;
    use rsa::signature::{RandomizedSigner, SignatureEncoding};

    let mut payload = payload;
    if let Some(object) = payload.as_object_mut() {
        object
            .entry("token_version")
            .or_insert(json!(stamp_token_version()));
    }
    stamp_hs256_temporal_claims(&mut payload);
    let header = format!(r#"{{"alg":"RS256","typ":"JWT","kid":"{key_id}"}}"#);
    let header_b64 = encode_base64url(header.as_bytes());
    let payload_b64 = encode_base64url(payload.to_string().as_bytes());
    let signing_input = format!("{header_b64}.{payload_b64}");
    let signing_key = SigningKey::<Sha256>::new(private_key.clone());
    let signature = signing_key.sign_with_rng(&mut OsRng, signing_input.as_bytes());
    format!(
        "{signing_input}.{}",
        encode_base64url(signature.to_bytes().as_ref())
    )
}

pub fn bootstrap_access_token_jwt(tenant_id: &str, app_id: &str) -> String {
    encode_unsigned_test_jwt(json!({
        "token_type": "access",
        "tenant_id": tenant_id,
        "user_id": "system",
        "app_id": app_id,
        "environment": "prod",
        "deployment_mode": "saas",
        "login_scope": "TENANT",
    }))
}

pub fn access_token_jwt(tenant_id: &str, user_id: &str, session_id: &str, app_id: &str) -> String {
    encode_unsigned_test_jwt(json!({
        "token_type": "access",
        "tenant_id": tenant_id,
        "user_id": user_id,
        "session_id": session_id,
        "app_id": app_id,
        "environment": "prod",
        "deployment_mode": "saas",
        "login_scope": "TENANT",
    }))
}

pub fn auth_token_jwt(tenant_id: &str, user_id: &str, session_id: &str, app_id: &str) -> String {
    auth_token_jwt_with_permissions(tenant_id, user_id, session_id, app_id, "")
}

pub fn auth_token_jwt_with_permissions(
    tenant_id: &str,
    user_id: &str,
    session_id: &str,
    app_id: &str,
    permission_scope: &str,
) -> String {
    let mut payload = json!({
        "token_type": "auth",
        "tenant_id": tenant_id,
        "user_id": user_id,
        "session_id": session_id,
        "app_id": app_id,
        "auth_level": "password",
        "login_scope": "TENANT",
    });
    if !permission_scope.trim().is_empty() {
        payload["permission_scope"] = json!(permission_scope);
    }
    encode_unsigned_test_jwt(payload)
}

fn encode_base64url(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    let mut buffer: u32 = 0;
    let mut bits: u8 = 0;

    for byte in input {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 6 {
            bits -= 6;
            let index = ((buffer >> bits) & 0x3f) as usize;
            output.push(TABLE[index] as char);
        }
    }

    if bits > 0 {
        buffer <<= 6 - bits;
        let index = (buffer & 0x3f) as usize;
        output.push(TABLE[index] as char);
    }

    output
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{AccessTokenParser, DefaultAccessTokenParser};

    #[tokio::test]
    async fn rejects_semicolon_claim_string_access_tokens() {
        let parser = DefaultAccessTokenParser;
        let error = parser
            .parse_access_token(
                "tenant_id=tenant-1;user_id=user-1;app_id=appbase;environment=prod;deployment_mode=saas",
            )
            .await
            .expect_err("claim string");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
    }

    #[tokio::test]
    async fn rejects_jwt_without_token_version() {
        let parser = DefaultAccessTokenParser;
        let token = encode_unsigned_test_jwt(json!({
            "token_type": "access",
            "tenant_id": "100001",
            "user_id": "user-1",
            "session_id": "session-1",
            "app_id": "appbase",
            "environment": "prod",
            "deployment_mode": "saas",
            "login_scope": "TENANT",
            "token_version": null,
        }));
        let error = parser
            .parse_access_token(&token)
            .await
            .expect_err("missing version");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
    }

    #[tokio::test]
    async fn accepts_unsigned_test_access_token_jwt() {
        let parser = DefaultAccessTokenParser;
        let token = access_token_jwt("100001", "user-1", "session-1", "appbase");
        let claims = parser.parse_access_token(&token).await.expect("jwt");
        assert_eq!("100001", claims.tenant_id);
    }
}
