use crate::error::WebFrameworkError;
use crate::jwt::{JwtVerifier, VerifyingAccessTokenParser, VerifyingAuthTokenParser};
use crate::jwt_claims::JwtProductionClaimPolicy;
use crate::parsers::{
    optional_claim, parse_deployment_mode, parse_environment, required_claim, split_claim,
    AccessTokenClaims, AccessTokenParser, ApiKeyCredential, ApiKeyParser, AuthTokenParser,
    DefaultAccessTokenParser, DefaultApiKeyParser, DefaultAuthTokenParser,
    DefaultOAuthBearerParser, OAuthBearerCredential, OAuthBearerParser,
};
use crate::request_context::{
    WebAuthLevel, WebDeploymentMode, WebEnvironment, WebRequestPrincipal, WebSubjectType,
};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Production JWT resolver profile reported by [`WebRequestContextResolver::resolver_production_profile`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResolverProductionProfile {
    #[default]
    Unspecified,
    /// Control-plane bootstrap: tenant-bound JWT without IAM session revocation adapter.
    TenantBoundBootstrap,
    /// Production SaaS: tenant-bound JWT with [`crate::jwt_tenant::JwtSessionRevocationChecker`].
    TenantBoundSaaS,
}

#[async_trait]
pub trait WebRequestContextResolver: Clone + Send + Sync + 'static {
    fn resolver_production_profile(&self) -> ResolverProductionProfile {
        ResolverProductionProfile::Unspecified
    }

    fn jwt_production_claim_policy(&self) -> Option<JwtProductionClaimPolicy> {
        None
    }

    /// Reports whether the resolver wires claim-embedded API key lookup (dev-only).
    fn uses_default_api_key_lookup(&self) -> bool {
        false
    }

    /// Reports whether the resolver wires claim-embedded OAuth lookup (dev-only).
    fn uses_default_oauth_token_lookup(&self) -> bool {
        false
    }

    async fn resolve_api_key(
        &self,
        raw_api_key: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError>;

    async fn resolve_dual_token(
        &self,
        raw_auth_token: &str,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError>;

    /// Access-token-only resolution for public / refresh-token routes on non-open-api surfaces.
    /// Establishes tenant/app isolation context without requiring an authenticated session.
    async fn resolve_access_token(
        &self,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError>;

    /// OAuth 2.0 bearer token for open-api. Default returns dependency-unavailable.
    async fn resolve_oauth_bearer(
        &self,
        raw_bearer_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let _ = raw_bearer_token;
        Err(WebFrameworkError::dependency_unavailable(
            "oauth bearer resolution is not configured; wire OAuthTokenLookupService or override resolve_oauth_bearer",
        ))
    }
}

#[async_trait]
pub trait ApiKeyLookupService: Clone + Send + Sync + 'static {
    async fn lookup_api_key(
        &self,
        credential: &ApiKeyCredential,
    ) -> Result<ApiKeyPrincipalRecord, WebFrameworkError>;
}

#[async_trait]
pub trait OAuthTokenLookupService: Clone + Send + Sync + 'static {
    async fn lookup_oauth_token(
        &self,
        credential: &OAuthBearerCredential,
    ) -> Result<OAuthPrincipalRecord, WebFrameworkError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiKeyPrincipalRecord {
    pub api_key_id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub user_id: String,
    pub app_id: String,
    pub environment: WebEnvironment,
    pub deployment_mode: WebDeploymentMode,
    pub data_scope: Vec<String>,
    pub permission_scope: Vec<String>,
    pub subject_type: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuthPrincipalRecord {
    pub token_id: String,
    pub client_id: Option<String>,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub user_id: String,
    pub app_id: String,
    pub environment: WebEnvironment,
    pub deployment_mode: WebDeploymentMode,
    pub data_scope: Vec<String>,
    pub permission_scope: Vec<String>,
    pub subject_type: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone)]
pub struct WebRequestParserResolver<AuthParser, AccessParser, KeyParser, KeyLookup> {
    auth_token_parser: AuthParser,
    access_token_parser: AccessParser,
    api_key_parser: KeyParser,
    api_key_lookup: KeyLookup,
}

impl<AuthParser, AccessParser, KeyParser, KeyLookup>
    WebRequestParserResolver<AuthParser, AccessParser, KeyParser, KeyLookup>
{
    pub fn new(
        auth_token_parser: AuthParser,
        access_token_parser: AccessParser,
        api_key_parser: KeyParser,
        api_key_lookup: KeyLookup,
    ) -> Self {
        Self {
            auth_token_parser,
            access_token_parser,
            api_key_parser,
            api_key_lookup,
        }
    }
}

#[async_trait]
impl<AuthParser, AccessParser, KeyParser, KeyLookup> WebRequestContextResolver
    for WebRequestParserResolver<AuthParser, AccessParser, KeyParser, KeyLookup>
where
    AuthParser: AuthTokenParser,
    AccessParser: AccessTokenParser,
    KeyParser: ApiKeyParser,
    KeyLookup: ApiKeyLookupService,
{
    fn uses_default_api_key_lookup(&self) -> bool {
        std::any::TypeId::of::<KeyLookup>() == std::any::TypeId::of::<DefaultApiKeyLookupService>()
    }

    async fn resolve_api_key(
        &self,
        raw_api_key: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let credential = self.api_key_parser.parse_api_key(raw_api_key).await?;
        let record = self.api_key_lookup.lookup_api_key(&credential).await?;
        Ok(WebRequestPrincipal::builder()
            .tenant_id(record.tenant_id)
            .organization_id(record.organization_id)
            .user_id(record.user_id)
            .app_id(record.app_id)
            .environment(record.environment)
            .deployment_mode(record.deployment_mode)
            .auth_level(WebAuthLevel::ApiKey)
            .data_scope(record.data_scope)
            .permission_scope(record.permission_scope)
            .api_key_id(Some(record.api_key_id))
            .subject_type(WebSubjectType::ApiKey)
            .build())
    }

    async fn resolve_dual_token(
        &self,
        raw_auth_token: &str,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let auth = self
            .auth_token_parser
            .parse_auth_token(raw_auth_token)
            .await?;
        let access = self
            .access_token_parser
            .parse_access_token(raw_access_token)
            .await?;

        require_optional_match(
            "tenant_id",
            auth.tenant_id.as_deref(),
            Some(access.tenant_id.as_str()),
        )?;
        require_optional_match(
            "organization_id",
            auth.organization_id.as_deref(),
            access.organization_id.as_deref(),
        )?;
        require_optional_match(
            "user_id",
            Some(auth.user_id.as_str()),
            access.user_id.as_deref(),
        )?;
        require_optional_match(
            "session_id",
            auth.session_id.as_deref(),
            access.session_id.as_deref(),
        )?;
        require_optional_match(
            "app_id",
            auth.app_id.as_deref(),
            Some(access.app_id.as_str()),
        )?;
        require_optional_match(
            "subject_type",
            auth.subject_type.as_deref(),
            access.subject_type.as_deref(),
        )?;
        if auth.login_scope != access.login_scope {
            return Err(WebFrameworkError::forbidden(
                "auth token and access token login_scope contexts do not match",
            ));
        }

        Ok(WebRequestPrincipal::builder()
            .tenant_id(access.tenant_id)
            .login_scope(access.login_scope)
            .organization_id(access.organization_id.or(auth.organization_id))
            .user_id(auth.user_id)
            .session_id(auth.session_id.or(access.session_id))
            .app_id(access.app_id)
            .environment(access.environment)
            .deployment_mode(access.deployment_mode)
            .auth_level(auth.auth_level)
            .data_scope(if access.data_scope.is_empty() {
                auth.data_scope
            } else {
                access.data_scope
            })
            .permission_scope(if access.permission_scope.is_empty() {
                auth.permission_scope
            } else {
                access.permission_scope
            })
            .subject_type(parse_subject_type(
                access
                    .subject_type
                    .as_deref()
                    .or(auth.subject_type.as_deref()),
            ))
            .build())
    }

    async fn resolve_access_token(
        &self,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let access = self
            .access_token_parser
            .parse_access_token(raw_access_token)
            .await?;
        Ok(principal_from_access_token_claims(access))
    }

    async fn resolve_oauth_bearer(
        &self,
        raw_bearer_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let _ = raw_bearer_token;
        Err(WebFrameworkError::dependency_unavailable(
            "oauth bearer resolution requires OAuthBearerParser + OAuthTokenLookupService wiring via OpenApiWebRequestParserResolver",
        ))
    }
}

/// Open-api capable resolver with OAuth bearer parser + lookup wiring.
#[derive(Clone)]
pub struct OpenApiWebRequestParserResolver<
    AuthParser,
    AccessParser,
    KeyParser,
    KeyLookup,
    OAuthParser,
    OAuthLookup,
> {
    inner: WebRequestParserResolver<AuthParser, AccessParser, KeyParser, KeyLookup>,
    oauth_bearer_parser: OAuthParser,
    oauth_token_lookup: OAuthLookup,
}

impl<AuthParser, AccessParser, KeyParser, KeyLookup, OAuthParser, OAuthLookup>
    OpenApiWebRequestParserResolver<
        AuthParser,
        AccessParser,
        KeyParser,
        KeyLookup,
        OAuthParser,
        OAuthLookup,
    >
{
    pub fn new(
        auth_token_parser: AuthParser,
        access_token_parser: AccessParser,
        api_key_parser: KeyParser,
        api_key_lookup: KeyLookup,
        oauth_bearer_parser: OAuthParser,
        oauth_token_lookup: OAuthLookup,
    ) -> Self {
        Self {
            inner: WebRequestParserResolver::new(
                auth_token_parser,
                access_token_parser,
                api_key_parser,
                api_key_lookup,
            ),
            oauth_bearer_parser,
            oauth_token_lookup,
        }
    }
}

#[async_trait]
impl<AuthParser, AccessParser, KeyParser, KeyLookup, OAuthParser, OAuthLookup>
    WebRequestContextResolver
    for OpenApiWebRequestParserResolver<
        AuthParser,
        AccessParser,
        KeyParser,
        KeyLookup,
        OAuthParser,
        OAuthLookup,
    >
where
    AuthParser: AuthTokenParser,
    AccessParser: AccessTokenParser,
    KeyParser: ApiKeyParser,
    KeyLookup: ApiKeyLookupService,
    OAuthParser: OAuthBearerParser,
    OAuthLookup: OAuthTokenLookupService,
{
    fn uses_default_api_key_lookup(&self) -> bool {
        self.inner.uses_default_api_key_lookup()
    }

    fn uses_default_oauth_token_lookup(&self) -> bool {
        std::any::TypeId::of::<OAuthLookup>()
            == std::any::TypeId::of::<DefaultOAuthTokenLookupService>()
    }

    async fn resolve_api_key(
        &self,
        raw_api_key: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner.resolve_api_key(raw_api_key).await
    }

    async fn resolve_dual_token(
        &self,
        raw_auth_token: &str,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner
            .resolve_dual_token(raw_auth_token, raw_access_token)
            .await
    }

    async fn resolve_access_token(
        &self,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner.resolve_access_token(raw_access_token).await
    }

    async fn resolve_oauth_bearer(
        &self,
        raw_bearer_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let credential = self
            .oauth_bearer_parser
            .parse_oauth_bearer(raw_bearer_token)
            .await?;
        let record = self
            .oauth_token_lookup
            .lookup_oauth_token(&credential)
            .await?;
        Ok(WebRequestPrincipal::builder()
            .tenant_id(record.tenant_id)
            .organization_id(record.organization_id)
            .user_id(record.user_id)
            .app_id(record.app_id)
            .environment(record.environment)
            .deployment_mode(record.deployment_mode)
            .auth_level(WebAuthLevel::Password)
            .data_scope(record.data_scope)
            .permission_scope(record.permission_scope)
            .subject_type(parse_subject_type(record.subject_type.as_deref()))
            .build())
    }
}

/// Rejects open-api API key authentication for profiles that do not expose open-api surfaces
/// (for example control-plane backend-api only). Prefer this over
/// [`DefaultApiKeyLookupService`] in production binaries that must not accept claim-embedded keys.
#[derive(Clone, Default)]
pub struct DisabledApiKeyLookupService;

#[async_trait]
impl ApiKeyLookupService for DisabledApiKeyLookupService {
    async fn lookup_api_key(
        &self,
        _credential: &ApiKeyCredential,
    ) -> Result<ApiKeyPrincipalRecord, WebFrameworkError> {
        Err(WebFrameworkError::invalid_credentials(
            "api key authentication is not enabled for this service profile",
        ))
    }
}

#[derive(Clone, Default)]
pub struct DefaultApiKeyLookupService;

#[async_trait]
impl ApiKeyLookupService for DefaultApiKeyLookupService {
    async fn lookup_api_key(
        &self,
        credential: &ApiKeyCredential,
    ) -> Result<ApiKeyPrincipalRecord, WebFrameworkError> {
        if credential.metadata.is_empty() {
            return Err(WebFrameworkError::invalid_credentials(
                "default api key resolver requires claim-form api keys",
            ));
        }

        let api_key_id = credential.api_key_id.clone().ok_or_else(|| {
            WebFrameworkError::invalid_credentials("api_key_id claim is required")
        })?;

        Ok(ApiKeyPrincipalRecord {
            api_key_id,
            tenant_id: required_claim(&credential.metadata, "tenant_id")?,
            organization_id: optional_claim(&credential.metadata, "organization_id"),
            user_id: required_claim(&credential.metadata, "user_id")?,
            app_id: required_claim(&credential.metadata, "app_id")?,
            environment: parse_environment(
                credential.metadata.get("environment").map(String::as_str),
            ),
            deployment_mode: parse_deployment_mode(
                credential
                    .metadata
                    .get("deployment_mode")
                    .map(String::as_str),
            ),
            data_scope: split_claim(credential.metadata.get("data_scope")),
            permission_scope: split_claim(credential.metadata.get("permission_scope")),
            subject_type: optional_claim(&credential.metadata, "subject_type")
                .or_else(|| Some("api_key".to_owned())),
            metadata: api_key_record_metadata(credential),
        })
    }
}

#[derive(Clone, Default)]
pub struct DefaultOAuthTokenLookupService;

#[async_trait]
impl OAuthTokenLookupService for DefaultOAuthTokenLookupService {
    async fn lookup_oauth_token(
        &self,
        credential: &OAuthBearerCredential,
    ) -> Result<OAuthPrincipalRecord, WebFrameworkError> {
        if credential.metadata.is_empty() {
            return Err(WebFrameworkError::invalid_credentials(
                "default oauth resolver requires claim-form bearer tokens",
            ));
        }

        let token_id = optional_claim(&credential.metadata, "token_id")
            .or_else(|| optional_claim(&credential.metadata, "jti"))
            .or_else(|| optional_claim(&credential.metadata, "sub"))
            .ok_or_else(|| {
                WebFrameworkError::invalid_credentials(
                    "token_id, jti, or sub claim is required for oauth bearer",
                )
            })?;

        Ok(OAuthPrincipalRecord {
            token_id,
            client_id: credential
                .client_id
                .clone()
                .or_else(|| optional_claim(&credential.metadata, "client_id")),
            tenant_id: required_claim(&credential.metadata, "tenant_id")?,
            organization_id: optional_claim(&credential.metadata, "organization_id"),
            user_id: required_claim(&credential.metadata, "user_id")
                .or_else(|_| required_claim(&credential.metadata, "sub"))?,
            app_id: required_claim(&credential.metadata, "app_id")?,
            environment: parse_environment(
                credential.metadata.get("environment").map(String::as_str),
            ),
            deployment_mode: parse_deployment_mode(
                credential
                    .metadata
                    .get("deployment_mode")
                    .map(String::as_str),
            ),
            data_scope: if credential.scopes.is_empty() {
                split_claim(credential.metadata.get("data_scope"))
            } else {
                credential.scopes.clone()
            },
            permission_scope: split_claim(credential.metadata.get("permission_scope")),
            subject_type: optional_claim(&credential.metadata, "subject_type")
                .or_else(|| Some("service".to_owned())),
            metadata: oauth_record_metadata(credential),
        })
    }
}

/// **Development / test only.** Parses semicolon claim strings and JWT payloads without
/// signature verification. Production SaaS must supply a verifying resolver implementation.
pub type DefaultWebRequestContextResolver = WebRequestParserResolver<
    DefaultAuthTokenParser,
    DefaultAccessTokenParser,
    DefaultApiKeyParser,
    DefaultApiKeyLookupService,
>;

/// Dev/test resolver with both API key and OAuth inline-claim support on open-api.
pub type DefaultOpenApiWebRequestContextResolver = OpenApiWebRequestParserResolver<
    DefaultAuthTokenParser,
    DefaultAccessTokenParser,
    DefaultApiKeyParser,
    DefaultApiKeyLookupService,
    DefaultOAuthBearerParser,
    DefaultOAuthTokenLookupService,
>;

impl Default for DefaultOpenApiWebRequestContextResolver {
    fn default() -> Self {
        Self::new(
            DefaultAuthTokenParser,
            DefaultAccessTokenParser,
            DefaultApiKeyParser,
            DefaultApiKeyLookupService,
            DefaultOAuthBearerParser,
            DefaultOAuthTokenLookupService,
        )
    }
}

impl Default for DefaultWebRequestContextResolver {
    fn default() -> Self {
        Self::new(
            DefaultAuthTokenParser,
            DefaultAccessTokenParser,
            DefaultApiKeyParser,
            DefaultApiKeyLookupService,
        )
    }
}

/// Production resolver wiring — JWT signatures verified via [`JwtVerifier`].
pub fn verifying_web_request_resolver<V, K>(
    verifier: Arc<V>,
    api_key_lookup: K,
) -> WebRequestParserResolver<
    VerifyingAuthTokenParser<V>,
    VerifyingAccessTokenParser<V>,
    DefaultApiKeyParser,
    K,
>
where
    V: JwtVerifier + 'static,
    K: ApiKeyLookupService,
{
    WebRequestParserResolver::new(
        VerifyingAuthTokenParser::new(verifier.clone()),
        VerifyingAccessTokenParser::new(verifier),
        DefaultApiKeyParser,
        api_key_lookup,
    )
}

/// Wraps a resolver built through tenant-bound production helpers for assembly inspection.
#[derive(Clone)]
pub struct TenantBoundProductionWebRequestResolver<R> {
    profile: ResolverProductionProfile,
    jwt_claim_policy: JwtProductionClaimPolicy,
    inner: R,
}

impl<R> TenantBoundProductionWebRequestResolver<R> {
    pub fn profile(&self) -> ResolverProductionProfile {
        self.profile
    }

    pub fn jwt_claim_policy(&self) -> &JwtProductionClaimPolicy {
        &self.jwt_claim_policy
    }

    pub fn into_inner(self) -> R {
        self.inner
    }
}

#[async_trait]
impl<R> WebRequestContextResolver for TenantBoundProductionWebRequestResolver<R>
where
    R: WebRequestContextResolver,
{
    fn resolver_production_profile(&self) -> ResolverProductionProfile {
        self.profile
    }

    fn jwt_production_claim_policy(&self) -> Option<JwtProductionClaimPolicy> {
        Some(self.jwt_claim_policy.clone())
    }

    fn uses_default_api_key_lookup(&self) -> bool {
        self.inner.uses_default_api_key_lookup()
    }

    fn uses_default_oauth_token_lookup(&self) -> bool {
        self.inner.uses_default_oauth_token_lookup()
    }

    async fn resolve_api_key(
        &self,
        raw_api_key: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner.resolve_api_key(raw_api_key).await
    }

    async fn resolve_dual_token(
        &self,
        raw_auth_token: &str,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner
            .resolve_dual_token(raw_auth_token, raw_access_token)
            .await
    }

    async fn resolve_access_token(
        &self,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner.resolve_access_token(raw_access_token).await
    }

    async fn resolve_oauth_bearer(
        &self,
        raw_bearer_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner.resolve_oauth_bearer(raw_bearer_token).await
    }
}

type TenantBoundBootstrapInnerResolver<L, K> = WebRequestParserResolver<
    VerifyingAuthTokenParser<crate::jwt_tenant::TenantBoundJwtVerifier<L>>,
    VerifyingAccessTokenParser<crate::jwt_tenant::TenantBoundJwtVerifier<L>>,
    DefaultApiKeyParser,
    K,
>;

type TenantBoundSaasInnerResolver<L, Rev, K> = WebRequestParserResolver<
    VerifyingAuthTokenParser<crate::jwt_tenant::TenantBoundJwtVerifier<L, Rev>>,
    VerifyingAccessTokenParser<crate::jwt_tenant::TenantBoundJwtVerifier<L, Rev>>,
    DefaultApiKeyParser,
    K,
>;

/// Production control-plane resolver — tenant-bound HS256 JWT via [`TenantSigningKeyLookup`] (IAM_SPEC / C14).
pub fn tenant_bound_verifying_web_request_resolver<L, K>(
    lookup: L,
    api_key_lookup: K,
) -> TenantBoundProductionWebRequestResolver<TenantBoundBootstrapInnerResolver<L, K>>
where
    L: crate::jwt_tenant::TenantSigningKeyLookup + 'static,
    K: ApiKeyLookupService,
{
    tenant_bound_verifying_web_request_resolver_with_claim_policy(
        lookup,
        api_key_lookup,
        JwtProductionClaimPolicy::production(),
    )
}

/// Control-plane resolver with explicit IAM issuer/audience claim policy.
pub fn tenant_bound_verifying_web_request_resolver_with_claim_policy<L, K>(
    lookup: L,
    api_key_lookup: K,
    claim_policy: JwtProductionClaimPolicy,
) -> TenantBoundProductionWebRequestResolver<TenantBoundBootstrapInnerResolver<L, K>>
where
    L: crate::jwt_tenant::TenantSigningKeyLookup + 'static,
    K: ApiKeyLookupService,
{
    let verifier = Arc::new(
        crate::jwt_tenant::TenantBoundJwtVerifier::new(lookup)
            .with_claim_policy(claim_policy.clone()),
    );
    TenantBoundProductionWebRequestResolver {
        profile: ResolverProductionProfile::TenantBoundBootstrap,
        jwt_claim_policy: claim_policy,
        inner: verifying_web_request_resolver(verifier, api_key_lookup),
    }
}

/// Production SaaS resolver — tenant-bound JWT plus IAM session revocation (EP-05e / IAM_SPEC).
pub fn tenant_bound_saas_verifying_web_request_resolver<L, Rev, K>(
    lookup: L,
    session_revocation: Rev,
    api_key_lookup: K,
) -> TenantBoundProductionWebRequestResolver<TenantBoundSaasInnerResolver<L, Rev, K>>
where
    L: crate::jwt_tenant::TenantSigningKeyLookup + 'static,
    Rev: crate::jwt_tenant::JwtSessionRevocationChecker + 'static,
    K: ApiKeyLookupService,
{
    tenant_bound_saas_verifying_web_request_resolver_with_claim_policy(
        lookup,
        session_revocation,
        api_key_lookup,
        JwtProductionClaimPolicy::production(),
    )
}

/// SaaS resolver with explicit IAM issuer/audience claim policy (SECURITY_SPEC / IAM_SPEC).
pub fn tenant_bound_saas_verifying_web_request_resolver_with_claim_policy<L, Rev, K>(
    lookup: L,
    session_revocation: Rev,
    api_key_lookup: K,
    claim_policy: JwtProductionClaimPolicy,
) -> TenantBoundProductionWebRequestResolver<TenantBoundSaasInnerResolver<L, Rev, K>>
where
    L: crate::jwt_tenant::TenantSigningKeyLookup + 'static,
    Rev: crate::jwt_tenant::JwtSessionRevocationChecker + 'static,
    K: ApiKeyLookupService,
{
    let verifier = Arc::new(
        crate::jwt_tenant::TenantBoundJwtVerifier::new(lookup)
            .with_claim_policy(claim_policy.clone())
            .with_session_revocation_checker(session_revocation),
    );
    TenantBoundProductionWebRequestResolver {
        profile: ResolverProductionProfile::TenantBoundSaaS,
        jwt_claim_policy: claim_policy,
        inner: verifying_web_request_resolver(verifier, api_key_lookup),
    }
}

/// Returns `true` when `resolver` reports a tenant-bound production profile.
pub fn is_tenant_bound_verifying_resolver<R>(resolver: &R) -> bool
where
    R: WebRequestContextResolver,
{
    matches!(
        resolver.resolver_production_profile(),
        ResolverProductionProfile::TenantBoundBootstrap
            | ResolverProductionProfile::TenantBoundSaaS
    )
}

/// Production open-api resolver — JWT signatures verified for dual-token and OAuth bearer.
pub fn verifying_open_api_web_request_resolver<V, K, O>(
    verifier: Arc<V>,
    api_key_lookup: K,
    oauth_token_lookup: O,
) -> OpenApiWebRequestParserResolver<
    VerifyingAuthTokenParser<V>,
    VerifyingAccessTokenParser<V>,
    DefaultApiKeyParser,
    K,
    VerifyingOAuthBearerParser<V>,
    O,
>
where
    V: JwtVerifier + 'static,
    K: ApiKeyLookupService,
    O: OAuthTokenLookupService,
{
    OpenApiWebRequestParserResolver::new(
        VerifyingAuthTokenParser::new(verifier.clone()),
        VerifyingAccessTokenParser::new(verifier.clone()),
        DefaultApiKeyParser,
        api_key_lookup,
        VerifyingOAuthBearerParser::new(verifier),
        oauth_token_lookup,
    )
}

fn require_optional_match(
    claim_name: &str,
    left: Option<&str>,
    right: Option<&str>,
) -> Result<(), WebFrameworkError> {
    match (left, right) {
        (Some(left), Some(right)) if left != right => Err(WebFrameworkError::forbidden(format!(
            "auth token and access token {claim_name} contexts do not match"
        ))),
        _ => Ok(()),
    }
}

fn principal_from_access_token_claims(access: AccessTokenClaims) -> WebRequestPrincipal {
    WebRequestPrincipal::builder()
        .tenant_id(access.tenant_id)
        .login_scope(access.login_scope)
        .organization_id(access.organization_id)
        .user_id(access.user_id.unwrap_or_default())
        .session_id(access.session_id)
        .app_id(access.app_id)
        .environment(access.environment)
        .deployment_mode(access.deployment_mode)
        .auth_level(WebAuthLevel::Anonymous)
        .data_scope(access.data_scope)
        .permission_scope(access.permission_scope)
        .subject_type(parse_subject_type(access.subject_type.as_deref()))
        .build()
}

fn api_key_record_metadata(credential: &ApiKeyCredential) -> BTreeMap<String, String> {
    let mut metadata = credential.metadata.clone();
    metadata
        .entry("source".to_owned())
        .or_insert_with(|| credential.source.clone());
    if let Some(key_prefix) = &credential.key_prefix {
        metadata
            .entry("key_prefix".to_owned())
            .or_insert_with(|| key_prefix.clone());
    }
    metadata
}

fn oauth_record_metadata(credential: &OAuthBearerCredential) -> BTreeMap<String, String> {
    let mut metadata = credential.metadata.clone();
    metadata
        .entry("source".to_owned())
        .or_insert_with(|| credential.source.clone());
    if let Some(client_id) = &credential.client_id {
        metadata
            .entry("client_id".to_owned())
            .or_insert_with(|| client_id.clone());
    }
    metadata
}

fn parse_subject_type(value: Option<&str>) -> WebSubjectType {
    match value.unwrap_or("user").trim().to_ascii_lowercase().as_str() {
        "service" => WebSubjectType::Service,
        "api_key" | "apikey" => WebSubjectType::ApiKey,
        "system" => WebSubjectType::System,
        _ => WebSubjectType::User,
    }
}

/// Production OAuth bearer parser — JWT signatures verified via [`JwtVerifier`].
#[derive(Clone)]
pub struct VerifyingOAuthBearerParser<V> {
    verifier: Arc<V>,
}

impl<V> VerifyingOAuthBearerParser<V> {
    pub fn new(verifier: Arc<V>) -> Self {
        Self { verifier }
    }
}

#[async_trait]
impl<V> OAuthBearerParser for VerifyingOAuthBearerParser<V>
where
    V: JwtVerifier + 'static,
{
    async fn parse_oauth_bearer(
        &self,
        raw: &str,
    ) -> Result<OAuthBearerCredential, WebFrameworkError> {
        let claims = self.verifier.verify_and_decode_claims(raw)?;
        let metadata = claims;
        Ok(OAuthBearerCredential {
            raw_token: raw.trim().to_owned(),
            client_id: optional_claim(&metadata, "client_id")
                .or_else(|| optional_claim(&metadata, "azp")),
            scopes: split_claim(metadata.get("scope"))
                .into_iter()
                .chain(split_claim(metadata.get("scopes")))
                .collect(),
            source: "verified-jwt".to_owned(),
            metadata,
        })
    }
}

#[cfg(test)]
mod lookup_service_tests {
    use super::*;
    use crate::error::WebFrameworkErrorKind;

    #[tokio::test]
    async fn disabled_api_key_lookup_rejects_all_credentials() {
        let lookup = DisabledApiKeyLookupService;
        let credential = ApiKeyCredential {
            raw_value: "sk_test".to_owned(),
            api_key_id: Some("key-1".to_owned()),
            key_prefix: None,
            source: "header".to_owned(),
            metadata: Default::default(),
        };
        let error = lookup
            .lookup_api_key(&credential)
            .await
            .expect_err("disabled lookup must reject");
        assert_eq!(WebFrameworkErrorKind::InvalidCredentials, error.kind);
    }
}
