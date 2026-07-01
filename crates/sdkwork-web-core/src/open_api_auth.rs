//! Open-api multi-scheme authentication: header-driven credential detection and resolution.
//!
//! Supported schemes: API key (`X-Api-Key`), OAuth 2.0 bearer (`Authorization: Bearer`).
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
        let oauth_present = credentials.oauth_bearer.is_some()
            || (bearer_token(headers).is_some() && credentials.access_token.is_none());

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
            Some(RouteAuth::OpenApiFlexible) | None => {}
            Some(
                RouteAuth::Public
                | RouteAuth::RefreshToken
                | RouteAuth::DualToken
                | RouteAuth::AgentToken,
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
    }
}

/// Maps route manifest auth to allowed open-api schemes (when not using flexible mode).
pub fn allowed_open_api_schemes(route_auth: RouteAuth) -> &'static [OpenApiAuthScheme] {
    match route_auth {
        RouteAuth::ApiKey | RouteAuth::AgentToken => &[OpenApiAuthScheme::ApiKey],
        RouteAuth::OAuth => &[OpenApiAuthScheme::OAuthBearer],
        RouteAuth::OpenApiFlexible => &[OpenApiAuthScheme::ApiKey, OpenApiAuthScheme::OAuthBearer],
        RouteAuth::Public | RouteAuth::RefreshToken | RouteAuth::DualToken => &[],
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

    #[test]
    fn detector_prefers_api_key_by_default_when_both_present() {
        let detector = DefaultOpenApiCredentialSchemeDetector::default();
        let credentials = WebCallCredentials {
            auth_token: None,
            access_token: None,
            api_key: Some("key-abc".to_owned()),
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

    #[tokio::test]
    async fn resolve_open_api_with_api_key_claims() {
        let resolver = DefaultWebRequestContextResolver::default();
        let credentials = WebCallCredentials {
            auth_token: None,
            access_token: None,
            api_key: Some("api_key_id=key-1;tenant_id=100001;user_id=30;app_id=appbase".to_owned()),
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

    #[tokio::test]
    async fn resolve_open_api_with_oauth_bearer_claims() {
        let resolver = DefaultOpenApiWebRequestContextResolver::default();
        let credentials = WebCallCredentials {
            auth_token: Some("oauth-token".to_owned()),
            access_token: None,
            api_key: None,
            oauth_bearer: Some(
                "token_id=tok-1;tenant_id=100001;user_id=user-oauth;app_id=appbase".to_owned(),
            ),
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
        assert_eq!(WebAuthMode::OAuth, auth_mode);
        assert_eq!("100001", principal.tenant_id());
    }
}
