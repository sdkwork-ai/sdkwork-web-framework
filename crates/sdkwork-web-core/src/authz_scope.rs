//! Server-side authorization scope materialization.
//!
//! IAM_SPEC §5.2 (`data_scope`/`permission_scope` are never signed claims),
//! SECURITY_SPEC (authorization scope is a server-side fact), CACHE_SPEC §5–§8
//! (bounded TTL cache with explicit invalidation).
//!
//! Why this module exists: a verified JWT proves *who* the caller is. It must not
//! be the transport for *what the caller may do*, because an authorization scope
//! is variable-length and grows with every RBAC grant. Embedding it produced
//! `HTTP 431 Request Header Fields Too Large` at the entrypoint.
//!
//! Shape of the capability (all three layers are overridable):
//!
//! 1. [`DynamicAuthorizationScopeSource`] — the port. An owning domain (IAM)
//!    implements it against its authoritative store. No other layer knows the
//!    store, the SQL, or the table names.
//! 2. [`AuthorizationScopeProvider`] — the policy layer. Owns caching, negative
//!    caching, TTL bounds, the fail-closed fallback decision, and invalidation.
//! 3. [`ServerResolvedScopeResolver`] — the pipeline layer. Wraps any
//!    [`WebRequestContextResolver`], so every application that integrates the
//!    framework inherits server-side scope resolution without touching its own
//!    resolver.
//!
//! Applications that need different semantics replace layer 1 (different store),
//! layer 2 (different cache policy via [`AuthorizationScopeProvider::new`]), or
//! layer 3 (their own wrapper) — and never fork the framework.

use crate::error::WebFrameworkError;
use crate::policy_cache::TtlCache;
use crate::request_context::{
    WebApiSurface, WebDeploymentMode, WebEnvironment, WebRequestPrincipal,
};
use crate::resolvers::{CompatibilityCredential, WebRequestContextResolver};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// Per-header-line budget at the forwarding edge.
///
/// Mirrors the Nginx-compatible tuning surface
/// (`client_header_buffer_size 1k` + `large_client_header_buffers 4 8k`) that the
/// webserver materializes for the module import plane (`NGINX_SPEC.md`).
pub const SINGLE_TOKEN_HEADER_BUDGET_BYTES: usize = 8 * 1024;

/// Whole-request header-block budget of the developer-facing ingress.
///
/// Node's HTTP parser caps the complete header block (request line + every
/// header line) at 16 KiB by default, and answers `431` above it. The
/// dual-token contract puts two JWTs in that block at once.
pub const DUAL_TOKEN_HEADER_BLOCK_BUDGET_BYTES: usize = 16 * 1024;

/// Headroom reserved for every other header a real browser sends
/// (`Host`, `User-Agent`, `Accept*`, `Cookie`, `Origin`, `Sec-Fetch-*`, …).
///
/// A token is only safe when two of them plus this reserve still fit the
/// whole-block budget.
pub const ENTRYPOINT_HEADER_RESERVE_BYTES: usize = 4 * 1024;

/// Bytes consumed by the two credential header names themselves
/// (`Access-Token: ` + `Authorization: Bearer `), excluding the token values.
pub const DUAL_TOKEN_HEADER_NAME_BYTES: usize = 64;

/// Ceiling for one rendered token, derived from the edge budgets.
///
/// `(16 KiB whole block - 4 KiB other headers - 64 B header names) / 2 = 6112 B`.
pub const MAX_RENDERED_TOKEN_BYTES: usize = (DUAL_TOKEN_HEADER_BLOCK_BUDGET_BYTES
    - ENTRYPOINT_HEADER_RESERVE_BYTES
    - DUAL_TOKEN_HEADER_NAME_BYTES)
    / 2;

/// Rejects a rendered token that cannot survive the entrypoint header budget.
///
/// Issuers `MUST` call this on every rendered credential (IAM_SPEC §5.2). It is
/// the mechanical guard that keeps a future claim addition from silently
/// reintroducing `431`.
pub fn validate_rendered_token_bytes(rendered_len: usize) -> Result<(), WebFrameworkError> {
    if rendered_len > MAX_RENDERED_TOKEN_BYTES {
        return Err(WebFrameworkError::internal_server_error(format!(
            "rendered token is {rendered_len} bytes, above the {MAX_RENDERED_TOKEN_BYTES}-byte \
             entrypoint header budget ({DUAL_TOKEN_HEADER_BLOCK_BUDGET_BYTES}-byte whole-block \
             budget minus {ENTRYPOINT_HEADER_RESERVE_BYTES}-byte reserve, two tokens); \
             move variable-length authorization content to server-side scope resolution"
        )));
    }
    Ok(())
}

/// Rejects a dual-token pair whose combined header footprint overflows the block.
pub fn validate_dual_token_header_budget(
    auth_token_bytes: usize,
    access_token_bytes: usize,
    header_name_bytes: usize,
) -> Result<(), WebFrameworkError> {
    let total = auth_token_bytes + access_token_bytes + header_name_bytes;
    if total + ENTRYPOINT_HEADER_RESERVE_BYTES > DUAL_TOKEN_HEADER_BLOCK_BUDGET_BYTES {
        return Err(WebFrameworkError::internal_server_error(format!(
            "dual-token header footprint is {total} bytes; with a \
             {ENTRYPOINT_HEADER_RESERVE_BYTES}-byte reserve it overflows the \
             {DUAL_TOKEN_HEADER_BLOCK_BUDGET_BYTES}-byte entrypoint header block budget"
        )));
    }
    Ok(())
}

/// Identity of the subject whose authorization scope is being materialized.
///
/// Doubles as the cache key namespace: every field that can change the resolved
/// scope is part of the key, so a context switch (organization, session, app,
/// environment) can never read another context's cached scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationScopeSubject {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub user_id: String,
    pub app_id: String,
    pub session_id: Option<String>,
    pub environment: WebEnvironment,
    pub deployment_mode: WebDeploymentMode,
    pub api_surface: WebApiSurface,
}

impl AuthorizationScopeSubject {
    pub fn for_principal(
        principal: &WebRequestPrincipal,
        api_surface: WebApiSurface,
    ) -> Self {
        Self {
            tenant_id: principal.tenancy.tenant_id.clone(),
            organization_id: principal.tenancy.organization_id.clone(),
            user_id: principal.subject.user_id.clone(),
            app_id: principal.app.app_id.clone(),
            session_id: principal.subject.session_id.clone(),
            environment: principal.app.environment.clone(),
            deployment_mode: principal.app.deployment_mode.clone(),
            api_surface,
        }
    }

    /// Stable cache key. Field order is fixed; `None` is rendered as `-`.
    pub fn cache_key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{:?}|{:?}|{:?}",
            self.tenant_id,
            self.organization_id.as_deref().unwrap_or("-"),
            self.user_id,
            self.app_id,
            self.session_id.as_deref().unwrap_or("-"),
            self.environment,
            self.deployment_mode,
            self.api_surface,
        )
    }

    /// Tenant-wide invalidation prefix (RBAC change fans out to a whole tenant).
    pub fn tenant_prefix(&self) -> String {
        format!("{}|", self.tenant_id)
    }

    /// Per-subject invalidation prefix (context switch, single-user revocation).
    pub fn subject_prefix(&self) -> String {
        format!(
            "{}|{}|{}|",
            self.tenant_id,
            self.organization_id.as_deref().unwrap_or("-"),
            self.user_id,
        )
    }
}

/// The resolved authorization scope of one subject.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebAuthorizationScope {
    pub data_scope: Vec<String>,
    pub permission_scope: Vec<String>,
    /// Revision marker of the authoritative row (`iam_session.updated_at`, RBAC
    /// revision counter, …). Cached entries whose revision is superseded `MUST`
    /// be discarded before authorization is granted (IAM_SPEC §5.2).
    pub scope_revision: Option<String>,
}

impl WebAuthorizationScope {
    pub fn new(data_scope: Vec<String>, permission_scope: Vec<String>) -> Self {
        Self {
            data_scope,
            permission_scope,
            scope_revision: None,
        }
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.scope_revision = Some(revision.into());
        self
    }

    /// The scope currently carried by a verified principal (possibly token-derived).
    pub fn from_principal(principal: &WebRequestPrincipal) -> Self {
        Self {
            data_scope: principal.scopes.data_scope.clone(),
            permission_scope: principal.scopes.permission_scope.clone(),
            scope_revision: None,
        }
    }

    /// Overwrites the principal's scope in place, leaving identity untouched.
    pub fn apply_to(&self, principal: &mut WebRequestPrincipal) {
        principal.scopes.data_scope = self.data_scope.clone();
        principal.scopes.permission_scope = self.permission_scope.clone();
    }

    pub fn is_empty(&self) -> bool {
        self.data_scope.is_empty() && self.permission_scope.is_empty()
    }
}

/// Port: load the authoritative authorization scope for a subject.
///
/// Implemented by the domain that owns the scope (IAM). Returns `Ok(None)` when
/// no authoritative record exists, leaving the fallback policy to the provider.
/// Returning an error is treated as fail-closed: the request is rejected.
#[async_trait]
pub trait DynamicAuthorizationScopeSource: Send + Sync {
    async fn resolve(
        &self,
        subject: &AuthorizationScopeSubject,
    ) -> Result<Option<WebAuthorizationScope>, WebFrameworkError>;
}

/// Source used by profiles that resolve scope elsewhere (default wiring).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOpDynamicAuthorizationScopeSource;

#[async_trait]
impl DynamicAuthorizationScopeSource for NoOpDynamicAuthorizationScopeSource {
    async fn resolve(
        &self,
        _subject: &AuthorizationScopeSubject,
    ) -> Result<Option<WebAuthorizationScope>, WebFrameworkError> {
        Ok(None)
    }
}

/// What to do when no authoritative scope could be loaded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationScopeFallback {
    /// Reject the request. Requirement for production and production-like
    /// profiles (SECURITY_SPEC fail-closed).
    Deny,
    /// Accept the scope carried by the already-verified credential.
    ///
    /// Development/test only, and the compatibility path while a deployment
    /// migrates off claim-embedded scope.
    CredentialClaims,
}

/// Resolves, caches, and invalidates authorization scope for verified principals.
#[derive(Clone)]
pub struct AuthorizationScopeProvider {
    source: Option<Arc<dyn DynamicAuthorizationScopeSource>>,
    cache: Arc<TtlCache<Option<WebAuthorizationScope>>>,
    fallback: AuthorizationScopeFallback,
    ttl: Duration,
}

impl AuthorizationScopeProvider {
    /// Wires a source with an explicit cache TTL and fallback policy.
    pub fn new(
        source: Arc<dyn DynamicAuthorizationScopeSource>,
        ttl: Duration,
        fallback: AuthorizationScopeFallback,
    ) -> Self {
        Self {
            source: Some(source),
            cache: Arc::new(TtlCache::new(ttl)),
            fallback,
            ttl,
        }
    }

    /// Production wiring: authoritative source, fail-closed fallback.
    pub fn server_resolved(source: Arc<dyn DynamicAuthorizationScopeSource>) -> Self {
        Self::new(source, default_scope_cache_ttl(), AuthorizationScopeFallback::Deny)
    }

    /// Development wiring: authoritative source, credential-claim fallback.
    pub fn server_resolved_with_claim_fallback(
        source: Arc<dyn DynamicAuthorizationScopeSource>,
    ) -> Self {
        Self::new(
            source,
            default_scope_cache_ttl(),
            AuthorizationScopeFallback::CredentialClaims,
        )
    }

    /// No source; the credential's own scope is authoritative (legacy/dev).
    pub fn credential_claims_only() -> Self {
        Self::new(
            Arc::new(NoOpDynamicAuthorizationScopeSource),
            default_scope_cache_ttl(),
            AuthorizationScopeFallback::CredentialClaims,
        )
    }

    /// No source and fail-closed: every authorization decision resolves to deny.
    pub fn deny_all() -> Self {
        Self::new(
            Arc::new(NoOpDynamicAuthorizationScopeSource),
            default_scope_cache_ttl(),
            AuthorizationScopeFallback::Deny,
        )
    }

    pub fn fallback(&self) -> AuthorizationScopeFallback {
        self.fallback
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn cache(&self) -> Arc<TtlCache<Option<WebAuthorizationScope>>> {
        self.cache.clone()
    }

    /// Resolves the scope for `subject`.
    ///
    /// Order: positive cache hit → authoritative source → fallback policy.
    /// A `None` from the source is negative-cached for one TTL so a missing
    /// session row cannot turn every request into a database round trip.
    pub async fn materialize(
        &self,
        subject: &AuthorizationScopeSubject,
        credential_scope: WebAuthorizationScope,
    ) -> Result<WebAuthorizationScope, WebFrameworkError> {
        let key = subject.cache_key();
        if let Some(cached) = self.cache.get_valid(&key) {
            return match cached {
                Some(scope) => Ok(scope),
                None => self.apply_fallback(subject, credential_scope),
            };
        }

        let Some(source) = self.source.as_ref() else {
            return self.apply_fallback(subject, credential_scope);
        };

        match source.resolve(subject).await? {
            Some(scope) => {
                self.cache.insert(key, Some(scope.clone()));
                Ok(scope)
            }
            None => {
                self.cache.insert(key, None);
                self.apply_fallback(subject, credential_scope)
            }
        }
    }

    fn apply_fallback(
        &self,
        subject: &AuthorizationScopeSubject,
        credential_scope: WebAuthorizationScope,
    ) -> Result<WebAuthorizationScope, WebFrameworkError> {
        match self.fallback {
            AuthorizationScopeFallback::CredentialClaims => Ok(credential_scope),
            AuthorizationScopeFallback::Deny => {
                Err(WebFrameworkError::forbidden(format!(
                    "authorization scope is not resolvable server-side for tenant {} subject {} \
                     (session {}); refusing to trust credential-carried scope",
                    subject.tenant_id,
                    subject.user_id,
                    subject.session_id.as_deref().unwrap_or("-"),
                ))
                .with_reason("authorization-scope-unavailable"))
            }
        }
    }

    /// Drops the cached entry for one resolved context (context switch, RBAC
    /// change for a single subject, single-session revocation).
    pub fn invalidate_subject(&self, subject: &AuthorizationScopeSubject) {
        self.cache.invalidate_prefix(&subject.cache_key());
    }

    /// Drops every cached entry for a user across contexts (role re-assignment).
    pub fn invalidate_user(&self, subject: &AuthorizationScopeSubject) {
        self.cache.invalidate_prefix(&subject.subject_prefix());
    }

    /// Drops every cached entry for a tenant (tenant-wide RBAC or permission
    /// catalog change).
    pub fn invalidate_tenant(&self, tenant_id: &str) {
        self.cache
            .invalidate_prefix(&format!("{tenant_id}|"));
    }
}

/// Default scope-cache TTL.
///
/// Shorter than [`crate::constants::DYNAMIC_POLICY_CACHE_TTL_SECS`] because a
/// stale authorization scope is a security fact, not a routing preference: the
/// cache is a safety net behind explicit invalidation, not the consistency
/// mechanism (CACHE_SPEC §5).
pub const AUTHORIZATION_SCOPE_CACHE_TTL_SECS: u64 = 15;

pub fn default_scope_cache_ttl() -> Duration {
    Duration::from_secs(AUTHORIZATION_SCOPE_CACHE_TTL_SECS)
}

/// Pipeline wrapper: enriches any resolver's principal with server-side scope.
///
/// This is the reuse point. An application keeps its existing resolver and adds
/// one wrapper; every app-api / backend-api / open-api request handled by that
/// resolver now resolves authorization scope from the owning domain instead of
/// from the token.
///
/// API-key resolution is intentionally passed through unchanged: an API key
/// already resolves its scope from a server-side credential record, so applying
/// the *user* scope provider would be a downgrade.
#[derive(Clone)]
pub struct ServerResolvedScopeResolver<R> {
    inner: R,
    provider: Arc<AuthorizationScopeProvider>,
    api_surface: WebApiSurface,
}

impl<R> ServerResolvedScopeResolver<R> {
    pub fn new(
        inner: R,
        provider: Arc<AuthorizationScopeProvider>,
        api_surface: WebApiSurface,
    ) -> Self {
        Self {
            inner,
            provider,
            api_surface,
        }
    }

    pub fn provider(&self) -> &Arc<AuthorizationScopeProvider> {
        &self.provider
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    async fn enrich(
        &self,
        principal: WebRequestPrincipal,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let subject = AuthorizationScopeSubject::for_principal(&principal, self.api_surface.clone());
        let credential_scope = WebAuthorizationScope::from_principal(&principal);
        let resolved = self.provider.materialize(&subject, credential_scope).await?;
        let mut principal = principal;
        resolved.apply_to(&mut principal);
        Ok(principal)
    }
}

#[async_trait]
impl<R> WebRequestContextResolver for ServerResolvedScopeResolver<R>
where
    R: WebRequestContextResolver,
{
    fn resolver_production_profile(&self) -> crate::resolvers::ResolverProductionProfile {
        self.inner.resolver_production_profile()
    }

    fn jwt_production_claim_policy(&self) -> Option<crate::jwt_claims::JwtProductionClaimPolicy> {
        self.inner.jwt_production_claim_policy()
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
        // Server-record authoritative already: never overwrite with user scope.
        self.inner.resolve_api_key(raw_api_key).await
    }

    async fn resolve_dual_token(
        &self,
        raw_auth_token: &str,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let principal = self
            .inner
            .resolve_dual_token(raw_auth_token, raw_access_token)
            .await?;
        self.enrich(principal).await
    }

    async fn resolve_access_token(
        &self,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let principal = self.inner.resolve_access_token(raw_access_token).await?;
        self.enrich(principal).await
    }

    async fn resolve_oauth_bearer(
        &self,
        raw_bearer_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let principal = self.inner.resolve_oauth_bearer(raw_bearer_token).await?;
        self.enrich(principal).await
    }

    async fn resolve_bearer_auth_token(
        &self,
        raw_bearer_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let principal = self.inner.resolve_bearer_auth_token(raw_bearer_token).await?;
        self.enrich(principal).await
    }

    async fn resolve_compatibility(
        &self,
        external_protocol_id: &str,
        credentials: &[CompatibilityCredential],
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner
            .resolve_compatibility(external_protocol_id, credentials)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_context::{WebAuthLevel, WebLoginScope, WebSubjectType};

    fn principal() -> WebRequestPrincipal {
        WebRequestPrincipal::builder()
            .tenant_id("100001")
            .organization_id(Some("0".to_owned()))
            .login_scope(WebLoginScope::Tenant)
            .user_id("1")
            .session_id(Some("session-1".to_owned()))
            .app_id("sdkwork-webserver-pc")
            .environment(WebEnvironment::Dev)
            .deployment_mode(WebDeploymentMode::Saas)
            .auth_level(WebAuthLevel::Password)
            .subject_type(WebSubjectType::User)
            .data_scope(vec!["tenant:100001".to_owned()])
            .permission_scope(vec!["*".to_owned()])
            .build()
    }

    struct FixedSource {
        scope: Option<WebAuthorizationScope>,
    }

    #[async_trait]
    impl DynamicAuthorizationScopeSource for FixedSource {
        async fn resolve(
            &self,
            _subject: &AuthorizationScopeSubject,
        ) -> Result<Option<WebAuthorizationScope>, WebFrameworkError> {
            Ok(self.scope.clone())
        }
    }

    struct CountingSource {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl DynamicAuthorizationScopeSource for CountingSource {
        async fn resolve(
            &self,
            _subject: &AuthorizationScopeSubject,
        ) -> Result<Option<WebAuthorizationScope>, WebFrameworkError> {
            self.calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Some(
                WebAuthorizationScope::new(vec!["tenant:100001".to_owned()], vec!["iam:read".to_owned()])
                    .with_revision("rev-1"),
            ))
        }
    }

    fn subject() -> AuthorizationScopeSubject {
        AuthorizationScopeSubject::for_principal(&principal(), WebApiSurface::AppApi)
    }

    #[tokio::test]
    async fn authoritative_scope_replaces_credential_scope() {
        let source = Arc::new(FixedSource {
            scope: Some(WebAuthorizationScope::new(
                vec!["tenant:100001".to_owned(), "organization:9".to_owned()],
                vec!["app.orders.read".to_owned()],
            )),
        });
        let provider = AuthorizationScopeProvider::server_resolved(source);
        let resolved = provider
            .materialize(&subject(), WebAuthorizationScope::from_principal(&principal()))
            .await
            .expect("materialize");
        assert_eq!(resolved.permission_scope, vec!["app.orders.read".to_owned()]);
        assert_eq!(resolved.data_scope.len(), 2);
    }

    #[tokio::test]
    async fn deny_fallback_rejects_when_source_has_no_record() {
        let source = Arc::new(FixedSource { scope: None });
        let provider = AuthorizationScopeProvider::server_resolved(source);
        let error = provider
            .materialize(&subject(), WebAuthorizationScope::from_principal(&principal()))
            .await
            .expect_err("deny");
        assert_eq!(crate::error::WebFrameworkErrorKind::Forbidden, error.kind);
    }

    #[tokio::test]
    async fn claim_fallback_keeps_credential_scope_for_compatibility() {
        let source = Arc::new(FixedSource { scope: None });
        let provider = AuthorizationScopeProvider::server_resolved_with_claim_fallback(source);
        let resolved = provider
            .materialize(&subject(), WebAuthorizationScope::from_principal(&principal()))
            .await
            .expect("claim fallback");
        assert_eq!(resolved.permission_scope, vec!["*".to_owned()]);
    }

    #[tokio::test]
    async fn negative_result_is_cached_but_invalidation_reloads() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let provider = AuthorizationScopeProvider::server_resolved(Arc::new(CountingSource {
            calls: calls.clone(),
        }));
        let credential = WebAuthorizationScope::from_principal(&principal());

        provider.materialize(&subject(), credential.clone()).await.expect("first");
        provider.materialize(&subject(), credential.clone()).await.expect("cached");
        assert_eq!(1, calls.load(std::sync::atomic::Ordering::SeqCst), "second call must hit cache");

        provider.invalidate_subject(&subject());
        provider.materialize(&subject(), credential).await.expect("after invalidation");
        assert_eq!(2, calls.load(std::sync::atomic::Ordering::SeqCst), "invalidation must reload");
    }

    #[test]
    fn cache_key_separates_organization_and_session_contexts() {
        let base = subject();
        let mut switched = base.clone();
        switched.organization_id = Some("9".to_owned());
        let mut other_session = base.clone();
        other_session.session_id = Some("session-2".to_owned());
        assert_ne!(base.cache_key(), switched.cache_key());
        assert_ne!(base.cache_key(), other_session.cache_key());
        assert!(base.cache_key().starts_with(&base.tenant_prefix()));
    }

    #[test]
    fn rendered_token_budget_rejects_scope_inflated_tokens() {
        // The observed pre-fix tenant super-admin token.
        let error = validate_rendered_token_bytes(7882).expect_err("7882 must be rejected");
        assert!(error.message.contains("header budget"), "{}", error.message);
        // A minimal identity-only token has ample headroom.
        validate_rendered_token_bytes(420).expect("identity-only token fits");
    }

    #[test]
    fn dual_token_budget_rejects_two_max_tokens() {
        validate_dual_token_header_budget(MAX_RENDERED_TOKEN_BYTES, MAX_RENDERED_TOKEN_BYTES, 28)
            .expect("two max tokens plus header names must fit");
        let error = validate_dual_token_header_budget(7882, 7879, 28)
            .expect_err("observed pre-fix pair must be rejected");
        assert!(error.message.contains("overflows"), "{}", error.message);
    }
}
