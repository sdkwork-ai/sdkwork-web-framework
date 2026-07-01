use crate::error::WebFrameworkError;
use crate::request_context::{WebAuthLevel, WebDeploymentMode, WebEnvironment, WebLoginScope};
use crate::token_version::{validate_token_version_claims, TokenVersionPolicy};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthTokenClaims {
    pub tenant_id: Option<String>,
    pub organization_id: Option<String>,
    pub login_scope: WebLoginScope,
    pub user_id: String,
    pub session_id: Option<String>,
    pub app_id: Option<String>,
    pub auth_level: WebAuthLevel,
    pub data_scope: Vec<String>,
    pub permission_scope: Vec<String>,
    pub subject_type: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessTokenClaims {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub login_scope: WebLoginScope,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub app_id: String,
    pub environment: WebEnvironment,
    pub deployment_mode: WebDeploymentMode,
    pub data_scope: Vec<String>,
    pub permission_scope: Vec<String>,
    pub subject_type: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiKeyCredential {
    pub raw_value: String,
    pub api_key_id: Option<String>,
    pub key_prefix: Option<String>,
    pub source: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuthBearerCredential {
    pub raw_token: String,
    pub client_id: Option<String>,
    pub scopes: Vec<String>,
    pub source: String,
    pub metadata: BTreeMap<String, String>,
}

#[async_trait]
pub trait AuthTokenParser: Clone + Send + Sync + 'static {
    async fn parse_auth_token(&self, raw: &str) -> Result<AuthTokenClaims, WebFrameworkError>;
}

#[async_trait]
pub trait AccessTokenParser: Clone + Send + Sync + 'static {
    async fn parse_access_token(&self, raw: &str) -> Result<AccessTokenClaims, WebFrameworkError>;
}

#[async_trait]
pub trait ApiKeyParser: Clone + Send + Sync + 'static {
    async fn parse_api_key(&self, raw: &str) -> Result<ApiKeyCredential, WebFrameworkError>;
}

#[async_trait]
pub trait OAuthBearerParser: Clone + Send + Sync + 'static {
    async fn parse_oauth_bearer(
        &self,
        raw: &str,
    ) -> Result<OAuthBearerCredential, WebFrameworkError>;
}

#[derive(Clone, Default)]
pub struct DefaultAuthTokenParser;

#[derive(Clone, Default)]
pub struct DefaultAccessTokenParser;

#[derive(Clone, Default)]
pub struct DefaultApiKeyParser;

#[derive(Clone, Default)]
pub struct DefaultOAuthBearerParser;

#[async_trait]
impl AuthTokenParser for DefaultAuthTokenParser {
    async fn parse_auth_token(&self, raw: &str) -> Result<AuthTokenClaims, WebFrameworkError> {
        require_sdkwork_jwt(raw, "auth_token")?;
        let claims = parse_claims(raw)?;
        validate_token_version_claims(&claims, &TokenVersionPolicy::standard())?;
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
            subject_type: optional_claim(&claims, "subject_type")
                .or_else(|| optional_claim(&claims, "actor_kind")),
            metadata: claims,
        })
    }
}

#[async_trait]
impl AccessTokenParser for DefaultAccessTokenParser {
    async fn parse_access_token(&self, raw: &str) -> Result<AccessTokenClaims, WebFrameworkError> {
        require_sdkwork_jwt(raw, "access_token")?;
        let claims = parse_claims(raw)?;
        validate_token_version_claims(&claims, &TokenVersionPolicy::standard())?;
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
            deployment_mode: parse_deployment_mode(
                claims.get("deployment_mode").map(String::as_str),
            ),
            data_scope: split_claim(claims.get("data_scope")),
            permission_scope: split_claim(claims.get("permission_scope")),
            subject_type: optional_claim(&claims, "subject_type")
                .or_else(|| optional_claim(&claims, "actor_kind")),
            metadata: claims,
        })
    }
}

#[async_trait]
impl OAuthBearerParser for DefaultOAuthBearerParser {
    async fn parse_oauth_bearer(
        &self,
        raw: &str,
    ) -> Result<OAuthBearerCredential, WebFrameworkError> {
        let raw_token = raw.trim();
        if raw_token.is_empty() {
            return Err(WebFrameworkError::missing_credentials(
                "oauth bearer token is required",
            ));
        }
        let metadata = parse_claims(raw_token)?;
        Ok(OAuthBearerCredential {
            raw_token: raw_token.to_owned(),
            client_id: optional_claim(&metadata, "client_id")
                .or_else(|| optional_claim(&metadata, "azp")),
            scopes: split_claim(metadata.get("scope"))
                .into_iter()
                .chain(split_claim(metadata.get("scopes")))
                .collect(),
            source: claim_or_default(
                &metadata,
                "source",
                if metadata.is_empty() {
                    "raw-oauth-bearer"
                } else {
                    "inline-claims"
                },
            ),
            metadata,
        })
    }
}

#[async_trait]
impl ApiKeyParser for DefaultApiKeyParser {
    async fn parse_api_key(&self, raw: &str) -> Result<ApiKeyCredential, WebFrameworkError> {
        let raw_value = raw.trim();
        if raw_value.is_empty() {
            return Err(WebFrameworkError::missing_credentials(
                "api key is required",
            ));
        }
        let metadata = parse_claims(raw_value)?;
        Ok(ApiKeyCredential {
            raw_value: raw_value.to_owned(),
            api_key_id: optional_claim(&metadata, "api_key_id")
                .or_else(|| optional_claim(&metadata, "kid")),
            key_prefix: optional_claim(&metadata, "key_prefix")
                .or_else(|| derive_key_prefix(raw_value)),
            source: claim_or_default(
                &metadata,
                "source",
                if metadata.is_empty() {
                    "raw-api-key"
                } else {
                    "inline-claims"
                },
            ),
            metadata,
        })
    }
}

pub(crate) fn require_sdkwork_jwt(raw: &str, token_label: &str) -> Result<(), WebFrameworkError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(WebFrameworkError::missing_credentials(format!(
            "{token_label} is required"
        )));
    }
    if raw.starts_with('{') {
        return Err(WebFrameworkError::invalid_credentials(format!(
            "{token_label} must be a JWT compact serialization, not a JSON object"
        )));
    }
    if raw.contains('=') && !raw.contains('.') {
        return Err(WebFrameworkError::invalid_credentials(format!(
            "{token_label} must be a signed JWT; semicolon claim-string tokens are not accepted"
        )));
    }
    let parts = raw.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(WebFrameworkError::invalid_credentials(format!(
            "{token_label} must be a JWT compact serialization (header.payload.signature)"
        )));
    }
    Ok(())
}

pub(crate) fn parse_claims(raw: &str) -> Result<BTreeMap<String, String>, WebFrameworkError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(BTreeMap::new());
    }
    if raw.starts_with('{') {
        return parse_json_claims(raw);
    }
    if let Some(claims) = parse_jwt_payload_claims(raw) {
        return Ok(claims);
    }
    Ok(parse_key_value_claims(raw))
}

fn parse_key_value_claims(raw: &str) -> BTreeMap<String, String> {
    raw.split(';')
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() || value.is_empty() {
                return None;
            }
            Some((key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn parse_json_claims(raw: &str) -> Result<BTreeMap<String, String>, WebFrameworkError> {
    let value = serde_json::from_str::<Value>(raw).map_err(|error| {
        WebFrameworkError::invalid_credentials(format!("invalid JSON claims: {error}"))
    })?;
    let object = value.as_object().ok_or_else(|| {
        WebFrameworkError::invalid_credentials("token claims must be a JSON object")
    })?;
    Ok(object
        .iter()
        .filter_map(|(key, value)| claim_value_to_string(value).map(|value| (key.clone(), value)))
        .collect())
}

fn parse_jwt_payload_claims(raw: &str) -> Option<BTreeMap<String, String>> {
    let mut parts = raw.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let _signature = parts.next()?;
    let decoded = decode_base64url(payload)?;
    let decoded = std::str::from_utf8(&decoded).ok()?;
    parse_json_claims(decoded).ok()
}

fn claim_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value.trim().to_owned()).filter(|value| !value.is_empty()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Array(items) => {
            let values = items
                .iter()
                .filter_map(claim_value_to_string)
                .collect::<Vec<_>>();
            if values.is_empty() {
                None
            } else {
                Some(values.join(","))
            }
        }
        Value::Object(_) => serde_json::to_string(value).ok(),
    }
}

fn decode_base64url(input: &str) -> Option<Vec<u8>> {
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
            _ => return None,
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    Some(output)
}

pub(crate) fn parse_environment(value: Option<&str>) -> WebEnvironment {
    match value.unwrap_or("prod").trim().to_ascii_lowercase().as_str() {
        "dev" | "development" => WebEnvironment::Dev,
        "test" | "testing" => WebEnvironment::Test,
        _ => WebEnvironment::Prod,
    }
}

pub(crate) fn parse_deployment_mode(value: Option<&str>) -> WebDeploymentMode {
    match value.unwrap_or("saas").trim().to_ascii_lowercase().as_str() {
        "local" => WebDeploymentMode::Local,
        "private" | "private_cloud" => WebDeploymentMode::Private,
        _ => WebDeploymentMode::Saas,
    }
}

pub(crate) fn parse_auth_level(value: Option<&str>) -> WebAuthLevel {
    match value
        .unwrap_or("password")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "anonymous" => WebAuthLevel::Anonymous,
        "mfa" => WebAuthLevel::Mfa,
        "system" => WebAuthLevel::System,
        "api_key" | "apikey" => WebAuthLevel::ApiKey,
        _ => WebAuthLevel::Password,
    }
}

pub(crate) fn parse_login_scope(
    value: Option<&str>,
    organization_id: Option<&str>,
) -> Result<WebLoginScope, WebFrameworkError> {
    let login_scope = match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.eq_ignore_ascii_case("TENANT") => WebLoginScope::Tenant,
        Some(value) if value.eq_ignore_ascii_case("ORGANIZATION") => WebLoginScope::Organization,
        Some(value) => {
            return Err(WebFrameworkError::invalid_credentials(format!(
                "unsupported login_scope claim: {value}",
            )));
        }
        None => WebLoginScope::from_organization_id(organization_id),
    };

    match (&login_scope, organization_id.map(str::trim)) {
        (WebLoginScope::Tenant, Some(value)) if !value.is_empty() && value != "0" => {
            Err(WebFrameworkError::invalid_credentials(
                "login_scope TENANT requires organization_id to be absent or 0",
            ))
        }
        (WebLoginScope::Organization, Some(value)) if !value.is_empty() && value != "0" => {
            Ok(login_scope)
        }
        (WebLoginScope::Organization, _) => Err(WebFrameworkError::invalid_credentials(
            "login_scope ORGANIZATION requires a non-zero organization_id",
        )),
        _ => Ok(login_scope),
    }
}

pub(crate) fn split_claim(value: Option<&String>) -> Vec<String> {
    value
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn required_claim(
    claims: &BTreeMap<String, String>,
    key: &str,
) -> Result<String, WebFrameworkError> {
    claims
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| WebFrameworkError::invalid_credentials(format!("{key} claim is required")))
}

pub(crate) fn optional_claim(claims: &BTreeMap<String, String>, key: &str) -> Option<String> {
    claims
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn claim_or_default(
    claims: &BTreeMap<String, String>,
    key: &str,
    default: &str,
) -> String {
    optional_claim(claims, key).unwrap_or_else(|| default.to_owned())
}

fn derive_key_prefix(raw: &str) -> Option<String> {
    let prefix = raw.chars().take(12).collect::<String>();
    if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    }
}
