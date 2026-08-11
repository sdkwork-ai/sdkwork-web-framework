//! Open-api multi-scheme authentication: header-driven credential detection and resolution.
//!
//! Supported schemes: API key (`X-Api-Key`), OAuth 2.0 bearer (`Authorization: Bearer`),
//! and SDKWork dual token (`Authorization: Bearer` plus `Access-Token`).
//! Applications extend via custom [`OpenApiCredentialSchemeDetector`], [`WebRequestContextResolver`]
//! method overrides, or a custom [`WebCallInterceptor`] at `RequestContextResolution`.

use crate::api_chain::WebCallCredentials;
use crate::error::WebFrameworkError;
use crate::extractors::{api_key, bearer_token};
use crate::request_context::{WebAuthMode, WebRequestPrincipal};
use crate::resolvers::WebRequestContextResolver;
use axum::http::HeaderMap;
use sdkwork_web_contract::RouteAuth;
use std::sync::Arc;

/// Credential scheme detected from open-api request headers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenApiAuthScheme {
    ApiKey,
    OAuthBearer,
    DualToken,
}

/// Kind of a single open-api bearer credential after classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenApiCredentialKind {
    /// A gateway/platform API key (conventionally `sk-`/`sp-` prefixed).
    ApiKey,
    /// An SDKWork login auth token.
    AuthToken,
}

/// Classifies a single `Authorization: Bearer` credential for
/// [`RouteAuth::OpenApiBearerFlexible`] routes.
///
/// Implementations are pluggable: applications override the default
/// prefix-based classifier to add vendor-specific key formats.
pub trait OpenApiBearerCredentialClassifier: Send + Sync + 'static {
    fn classify(&self, raw_credential: &str) -> OpenApiCredentialKind;
}

/// Default classifier: credentials starting with an API key prefix
/// (`sk-` or `sp-`) are treated as API keys; everything else is an auth token.
#[derive(Clone, Debug)]
pub struct PrefixedOpenApiBearerCredentialClassifier {
    pub api_key_prefixes: Vec<String>,
}

impl Default for PrefixedOpenApiBearerCredentialClassifier {
    fn default() -> Self {
        Self {
            api_key_prefixes: vec!["sk-".to_string(), "sp-".to_string()],
        }
    }
}

impl OpenApiBearerCredentialClassifier for PrefixedOpenApiBearerCredentialClassifier {
    fn classify(&self, raw_credential: &str) -> OpenApiCredentialKind {
        if self
            .api_key_prefixes
            .iter()
            .any(|prefix| raw_credential.starts_with(prefix.as_str()))
        {
            OpenApiCredentialKind::ApiKey
        } else {
            OpenApiCredentialKind::AuthToken
        }
    }
}

/// Type-erased classifier for runtime wiring.
pub type DynOpenApiBearerCredentialClassifier = Arc<dyn OpenApiBearerCredentialClassifier>;

pub fn default_open_api_bearer_classifier() -> DynOpenApiBearerCredentialClassifier {
    Arc::new(PrefixedOpenApiBearerCredentialClassifier::default())
}

/// Resolves a single bearer credential for [`RouteAuth::OpenApiBearerFlexible`]
/// routes: the classifier picks the credential kind, then the matching resolver
/// channel authenticates it.
///
/// The bearer credential is read from `Authorization: Bearer`
/// (`credentials.oauth_bearer`); `X-Api-Key` is accepted as a fallback so the
/// same flexible route serves both header styles.
pub async fn resolve_open_api_bearer_flexible<R>(
    credentials: &WebCallCredentials,
    classifier: &dyn OpenApiBearerCredentialClassifier,
    inner: &R,
) -> Result<(WebAuthMode, WebRequestPrincipal), WebFrameworkError>
where
    R: WebRequestContextResolver,
{
    let raw = credentials
        .oauth_bearer
        .as_deref()
        .or(credentials.api_key.as_deref())
        .ok_or_else(|| {
            WebFrameworkError::missing_credentials(
                "open-api bearer-flexible routes require Authorization: Bearer or X-Api-Key",
            )
        })?;
    match classifier.classify(raw) {
        OpenApiCredentialKind::ApiKey => {
            let principal = inner.resolve_api_key(raw).await?;
            Ok((WebAuthMode::ApiKey, principal))
        }
        OpenApiCredentialKind::AuthToken => {
            let principal = inner.resolve_bearer_auth_token(raw).await?;
            Ok((WebAuthMode::OAuth, principal))
        }
    }
}

/// Header-driven detection of which open-api auth scheme a client is using.
pub trait OpenApiCredentialSchemeDetector: Send + Sync + 'static {
    /// Returns the detected scheme, or `None` when no supported credentials are present.
    fn detect(
        &self,
        credentials: &WebCallCredentials,
        headers: &HeaderMap,
        route_auth: Option<RouteAuth>,
    ) -> Result<Option<OpenApiAuthScheme>, WebFrameworkError>;
}

/// Policy for open-api multi-scheme resolution.
#[derive(Clone, Debug)]
pub struct OpenApiAuthPolicy {
    /// Preference order when multiple credential headers are present.
    pub scheme_preference: Vec<OpenApiAuthScheme>,
}

impl Default for OpenApiAuthPolicy {
    fn default() -> Self {
        Self {
            scheme_preference: vec![OpenApiAuthScheme::ApiKey, OpenApiAuthScheme::OAuthBearer],
        }
    }
}

/// Default header-driven detector for open-api protected routes.
#[derive(Clone, Debug, Default)]
pub struct DefaultOpenApiCredentialSchemeDetector {
    pub policy: OpenApiAuthPolicy,
}

impl DefaultOpenApiCredentialSchemeDetector {
    pub fn new(policy: OpenApiAuthPolicy) -> Self {
        Self { policy }
    }
}

impl OpenApiCredentialSchemeDetector for DefaultOpenApiCredentialSchemeDetector {
    fn detect(
        &self,
        credentials: &WebCallCredentials,
        headers: &HeaderMap,
        route_auth: Option<RouteAuth>,
    ) -> Result<Option<OpenApiAuthScheme>, WebFrameworkError> {
        let api_key_present = credentials.api_key.is_some() || api_key(headers).is_some();
        let auth_token_present =
            credentials.auth_token.is_some() || bearer_token(headers).is_some();
        let access_token_present = credentials.access_token.is_some();
        let oauth_present =
            credentials.oauth_bearer.is_some() || (auth_token_present && !access_token_present);

        match route_auth {
            Some(RouteAuth::ApiKey) => {
                if !api_key_present {
                    return Ok(None);
                }
                if oauth_present {
                    return Err(WebFrameworkError::invalid_credentials(
                        "route requires API key authentication; OAuth bearer is not accepted",
                    ));
                }
                return Ok(Some(OpenApiAuthScheme::ApiKey));
            }
            Some(RouteAuth::OAuth) => {
                if !oauth_present {
                    return Ok(None);
                }
                if api_key_present {
                    return Err(WebFrameworkError::invalid_credentials(
                        "route requires OAuth bearer authentication; API key is not accepted",
                    ));
                }
                return Ok(Some(OpenApiAuthScheme::OAuthBearer));
            }
            Some(RouteAuth::ApiKeyOrDualToken) => {
                if api_key_present && (auth_token_present || access_token_present) {
                    return Err(WebFrameworkError::invalid_credentials(
                        "api-key-or-dual-token routes do not accept mixed credential profiles",
                    )
                    .with_reason("credential-profile-contamination"));
                }
                if auth_token_present != access_token_present {
                    return Err(WebFrameworkError::missing_credentials(
                        "the dual-token branch requires both Authorization and Access-Token",
                    )
                    .with_reason("incomplete-credential-profile"));
                }
                if api_key_present {
                    return Ok(Some(OpenApiAuthScheme::ApiKey));
                }
                if auth_token_present {
                    return Ok(Some(OpenApiAuthScheme::DualToken));
                }
                return Ok(None);
            }
            Some(RouteAuth::DualToken) | Some(RouteAuth::DualTokenOrAnonymous) => {
                if auth_token_present && access_token_present {
                    return Ok(Some(OpenApiAuthScheme::DualToken));
                }
                return Ok(None);
            }
            Some(RouteAuth::OpenApiBearerFlexible) => {
                if access_token_present {
                    return Err(WebFrameworkError::invalid_credentials(
                        "open-api bearer-flexible routes accept a single bearer credential; Access-Token is not allowed",
                    )
                    .with_reason("credential-profile-contamination"));
                }
                if api_key_present && auth_token_present {
                    return Err(WebFrameworkError::invalid_credentials(
                        "open-api bearer-flexible routes do not accept mixed credential headers",
                    )
                    .with_reason("credential-profile-contamination"));
                }
                if api_key_present || oauth_present {
                    return Ok(Some(OpenApiAuthScheme::OAuthBearer));
                }
                return Ok(None);
            }
            Some(RouteAuth::OpenApiFlexible) | None => {}
            Some(
                RouteAuth::Public
                | RouteAuth::BootstrapBody
                | RouteAuth::CredentialEntryBootstrap
                | RouteAuth::RefreshToken
                | RouteAuth::IngressToken
                | RouteAuth::AgentToken
                | RouteAuth::Compatibility,
            ) => {}
        }

        let mut detected = Vec::new();
        if api_key_present {
            detected.push(OpenApiAuthScheme::ApiKey);
        }
        if oauth_present {
            detected.push(OpenApiAuthScheme::OAuthBearer);
        }
        if detected.is_empty() {
            return Ok(None);
        }
        if detected.len() == 1 {
            return Ok(Some(detected[0]));
        }

        for preferred in &self.policy.scheme_preference {
            if detected.contains(preferred) {
                return Ok(Some(*preferred));
            }
        }
        Ok(Some(detected[0]))
    }
}

/// Full open-api resolution pipeline: detect scheme from headers, then resolve principal.
pub async fn resolve_open_api_request_context<R>(
    credentials: &WebCallCredentials,
    headers: &HeaderMap,
    route_auth: Option<RouteAuth>,
    detector: &dyn OpenApiCredentialSchemeDetector,
    inner: &R,
) -> Result<(WebAuthMode, WebRequestPrincipal), WebFrameworkError>
where
    R: WebRequestContextResolver,
{
    let scheme = detector
        .detect(credentials, headers, route_auth)?
        .ok_or_else(|| {
            WebFrameworkError::missing_credentials(
                "open-api requests require API key (X-Api-Key) or OAuth bearer (Authorization: Bearer)",
            )
        })?;

    match scheme {
        OpenApiAuthScheme::ApiKey => {
            let raw = credentials.api_key.as_deref().ok_or_else(|| {
                WebFrameworkError::missing_credentials("open-api API key is required")
            })?;
            let principal = inner.resolve_api_key(raw).await?;
            Ok((WebAuthMode::ApiKey, principal))
        }
        OpenApiAuthScheme::OAuthBearer => {
            let raw = credentials.oauth_bearer.as_deref().ok_or_else(|| {
                WebFrameworkError::missing_credentials("open-api OAuth bearer token is required")
            })?;
            let principal = inner.resolve_oauth_bearer(raw).await?;
            Ok((WebAuthMode::OAuth, principal))
        }
        OpenApiAuthScheme::DualToken => {
            let auth_token = credentials.auth_token.as_deref().ok_or_else(|| {
                WebFrameworkError::missing_credentials(
                    "open-api dual-token authentication requires Authorization",
                )
            })?;
            let access_token = credentials.access_token.as_deref().ok_or_else(|| {
                WebFrameworkError::missing_credentials(
                    "open-api dual-token authentication requires Access-Token",
                )
            })?;
            let principal = inner.resolve_dual_token(auth_token, access_token).await?;
            Ok((WebAuthMode::DualToken, principal))
        }
    }
}

/// Maps route manifest auth to allowed open-api schemes (when not using flexible mode).
pub fn allowed_open_api_schemes(route_auth: RouteAuth) -> &'static [OpenApiAuthScheme] {
    match route_auth {
        RouteAuth::ApiKey => &[OpenApiAuthScheme::ApiKey],
        RouteAuth::OAuth => &[OpenApiAuthScheme::OAuthBearer],
        RouteAuth::OpenApiFlexible => &[OpenApiAuthScheme::ApiKey, OpenApiAuthScheme::OAuthBearer],
        RouteAuth::OpenApiBearerFlexible => &[OpenApiAuthScheme::OAuthBearer],
        RouteAuth::ApiKeyOrDualToken => &[OpenApiAuthScheme::ApiKey, OpenApiAuthScheme::DualToken],
        RouteAuth::DualTokenOrAnonymous => &[OpenApiAuthScheme::DualToken],
        RouteAuth::Public
        | RouteAuth::BootstrapBody
        | RouteAuth::CredentialEntryBootstrap
        | RouteAuth::RefreshToken
        | RouteAuth::DualToken
        | RouteAuth::IngressToken
        | RouteAuth::AgentToken
        | RouteAuth::Compatibility => &[],
    }
}

/// Type-erased detector for runtime wiring.
pub type DynOpenApiCredentialSchemeDetector = Arc<dyn OpenApiCredentialSchemeDetector>;

pub fn default_open_api_scheme_detector() -> DynOpenApiCredentialSchemeDetector {
    Arc::new(DefaultOpenApiCredentialSchemeDetector::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolvers::{
        DefaultOpenApiWebRequestContextResolver, DefaultWebRequestContextResolver,
    };

    fn credentials(
        api_key: Option<&str>,
        auth_token: Option<&str>,
        access_token: Option<&str>,
    ) -> WebCallCredentials {
        WebCallCredentials {
            auth_token: auth_token.map(str::to_owned),
            access_token: access_token.map(str::to_owned),
            api_key: api_key.map(str::to_owned),
            ingress_token: None,
            oauth_bearer: auth_token
                .filter(|_| access_token.is_none())
                .map(str::to_owned),
            agent_token: None,
        }
    }

    #[test]
    fn detector_prefers_api_key_by_default_when_both_present() {
        let detector = DefaultOpenApiCredentialSchemeDetector::default();
        let credentials = WebCallCredentials {
            auth_token: None,
            access_token: None,
            api_key: Some("key-abc".to_owned()),
            ingress_token: None,
            oauth_bearer: Some("oauth-token".to_owned()),
            agent_token: None,
        };
        let headers = HeaderMap::new();
        let scheme = detector
            .detect(&credentials, &headers, Some(RouteAuth::OpenApiFlexible))
            .expect("detect")
            .expect("scheme");
        assert_eq!(OpenApiAuthScheme::ApiKey, scheme);
    }

    #[test]
    fn detector_enforces_route_api_key_only() {
        let detector = DefaultOpenApiCredentialSchemeDetector::default();
        let credentials = WebCallCredentials {
            auth_token: None,
            access_token: None,
            api_key: None,
            ingress_token: None,
            oauth_bearer: Some("oauth-only".to_owned()),
            agent_token: None,
        };
        let headers = HeaderMap::new();
        let scheme = detector
            .detect(&credentials, &headers, Some(RouteAuth::ApiKey))
            .expect("detect");
        assert!(scheme.is_none());
    }

    #[test]
    fn detector_rejects_mixed_credentials_on_oauth_route() {
        let detector = DefaultOpenApiCredentialSchemeDetector::default();
        let credentials = WebCallCredentials {
            auth_token: None,
            access_token: None,
            api_key: Some("key-abc".to_owned()),
            ingress_token: None,
            oauth_bearer: Some("oauth-token".to_owned()),
            agent_token: None,
        };
        let headers = HeaderMap::new();
        let error = detector
            .detect(&credentials, &headers, Some(RouteAuth::OAuth))
            .expect_err("mixed");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::InvalidCredentials,
            error.kind
        );
    }

    #[test]
    fn api_key_or_dual_token_detector_accepts_each_complete_alternative() {
        let detector = DefaultOpenApiCredentialSchemeDetector::default();
        let headers = HeaderMap::new();

        let api_key_scheme = detector
            .detect(
                &credentials(Some("key-abc"), None, None),
                &headers,
                Some(RouteAuth::ApiKeyOrDualToken),
            )
            .expect("api key detection");
        assert_eq!(Some(OpenApiAuthScheme::ApiKey), api_key_scheme);

        let dual_token_scheme = detector
            .detect(
                &credentials(None, Some("auth-token"), Some("access-token")),
                &headers,
                Some(RouteAuth::ApiKeyOrDualToken),
            )
            .expect("dual token detection");
        assert_eq!(Some(OpenApiAuthScheme::DualToken), dual_token_scheme);
    }

    #[test]
    fn api_key_or_dual_token_detector_rejects_mixed_and_partial_credentials() {
        let detector = DefaultOpenApiCredentialSchemeDetector::default();
        let headers = HeaderMap::new();
        let cases = [
            credentials(Some("key-abc"), Some("auth-token"), None),
            credentials(Some("key-abc"), None, Some("access-token")),
            credentials(Some("key-abc"), Some("auth-token"), Some("access-token")),
            credentials(None, Some("auth-token"), None),
            credentials(None, None, Some("access-token")),
        ];

        for credentials in cases {
            detector
                .detect(&credentials, &headers, Some(RouteAuth::ApiKeyOrDualToken))
                .expect_err("mixed or partial credentials must fail");
        }
    }

    #[tokio::test]
    async fn resolve_open_api_with_api_key_claims() {
        let resolver = DefaultWebRequestContextResolver::default();
        let credentials = WebCallCredentials {
            auth_token: None,
            access_token: None,
            api_key: Some("api_key_id=key-1;tenant_id=100001;user_id=30;app_id=appbase".to_owned()),
            ingress_token: None,
            oauth_bearer: None,
            agent_token: None,
        };
        let (auth_mode, principal) = resolve_open_api_request_context(
            &credentials,
            &HeaderMap::new(),
            Some(RouteAuth::OpenApiFlexible),
            &DefaultOpenApiCredentialSchemeDetector::default(),
            &resolver,
        )
        .await
        .expect("resolved");
        assert_eq!(WebAuthMode::ApiKey, auth_mode);
        assert_eq!("100001", principal.tenant_id());
    }

    #[test]
    fn prefixed_classifier_distinguishes_api_keys_from_auth_tokens() {
        let classifier = PrefixedOpenApiBearerCredentialClassifier::default();
        assert_eq!(
            OpenApiCredentialKind::ApiKey,
            classifier.classify("sk-0123456789abcdef")
        );
        assert_eq!(
            OpenApiCredentialKind::ApiKey,
            classifier.classify("sp-0123456789abcdef")
        );
        assert_eq!(
            OpenApiCredentialKind::AuthToken,
            classifier.classify("eyJhbGciOiJIUzI1NiJ9.eyJ0ZW5hbnQiOiIxIn0.signature")
        );
        assert_eq!(
            OpenApiCredentialKind::AuthToken,
            classifier.classify("random-non-prefixed-value")
        );
    }

    #[test]
    fn classifier_prefixes_are_extensible_and_overridable() {
        let classifier = PrefixedOpenApiBearerCredentialClassifier {
            api_key_prefixes: vec!["ak-".to_string()],
        };
        assert_eq!(
            OpenApiCredentialKind::ApiKey,
            classifier.classify("ak-custom-prefixed-key")
        );
        assert_eq!(
            OpenApiCredentialKind::AuthToken,
            classifier.classify("sk-not-an-api-key-with-this-policy")
        );
    }

    #[tokio::test]
    async fn bearer_flexible_routes_api_key_prefix_through_api_key_channel() {
        let resolver = DefaultWebRequestContextResolver::default();
        let credentials = WebCallCredentials {
            auth_token: None,
            access_token: None,
            api_key: None,
            ingress_token: None,
            oauth_bearer: Some(
                "sk-claims;api_key_id=key-1;tenant_id=100001;user_id=30;app_id=appbase".to_owned(),
            ),
            agent_token: None,
        };
        let (auth_mode, principal) = resolve_open_api_bearer_flexible(
            &credentials,
            &PrefixedOpenApiBearerCredentialClassifier::default(),
            &resolver,
        )
        .await
        .expect("api key channel resolves");
        assert_eq!(WebAuthMode::ApiKey, auth_mode);
        assert_eq!("100001", principal.tenant_id());
    }

    #[tokio::test]
    async fn bearer_flexible_routes_non_prefixed_credential_through_auth_token_channel() {
        let resolver = DefaultOpenApiWebRequestContextResolver::default();
        let credentials = WebCallCredentials {
            auth_token: Some("auth-token".to_owned()),
            access_token: None,
            api_key: None,
            ingress_token: None,
            oauth_bearer: Some(
                "token_id=tok-1;tenant_id=100001;user_id=user-oauth;app_id=appbase".to_owned(),
            ),
            agent_token: None,
        };
        let (auth_mode, principal) = resolve_open_api_bearer_flexible(
            &credentials,
            &PrefixedOpenApiBearerCredentialClassifier::default(),
            &resolver,
        )
        .await
        .expect("auth token channel resolves");
        assert_eq!(WebAuthMode::OAuth, auth_mode);
        assert_eq!("100001", principal.tenant_id());
    }

    #[tokio::test]
    async fn bearer_flexible_missing_credentials_fails_closed() {
        let resolver = DefaultOpenApiWebRequestContextResolver::default();
        let credentials = WebCallCredentials {
            auth_token: None,
            access_token: None,
            api_key: None,
            ingress_token: None,
            oauth_bearer: None,
            agent_token: None,
        };
        let error = resolve_open_api_bearer_flexible(
            &credentials,
            &PrefixedOpenApiBearerCredentialClassifier::default(),
            &resolver,
        )
        .await
        .expect_err("missing credentials must fail");
        assert_eq!(
            crate::error::WebFrameworkErrorKind::MissingCredentials,
            error.kind
        );
    }
}
