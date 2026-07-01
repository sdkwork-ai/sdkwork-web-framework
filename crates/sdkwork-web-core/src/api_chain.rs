use crate::context_injection::DomainContextInjector;
use crate::cors_policy::{DynamicCorsPolicySource, NoOpDynamicCorsPolicySource};
use crate::error::WebFrameworkError;
use crate::extractors::{
    access_token, agent_token, api_key, bearer_token, header_value, idempotency_key,
};
use crate::idempotency::IdempotencyResponseRecord;
use crate::open_api_auth::{default_open_api_scheme_detector, DynOpenApiCredentialSchemeDetector};
use crate::policies::{
    AllowAllAuthorizationPolicy, AuditEmitter, AuthorizationPolicy, NoOpAuditEmitter,
    NoOpSecurityEventEmitter, PassThroughTenantIsolationPolicy, SecurityEventEmitter,
    TenantIsolationPolicy,
};
use crate::rate_limit::{
    DefaultRateLimitPolicyResolver, RateLimitPolicyResolver, ResolvedRateLimitPolicy,
};
use crate::rate_limit_policy::{DynamicRateLimitPolicySource, NoOpDynamicRateLimitPolicySource};
use crate::request_context::{
    WebApiSurface, WebAuthMode, WebRequestContext, WebRequestContextProfile, WebRequestPrincipal,
};
use crate::request_identity::ServerRequestId;
use crate::resolvers::WebRequestContextResolver;
use crate::route_manifest::HttpRouteManifest;
use crate::runtime_options::WebFrameworkOptionalFeatures;
use crate::security::{CorsPolicy, SecurityPolicy};
use crate::stores::{
    memory_concurrent_admission_store, memory_idempotency_store, memory_rate_limit_store,
    ConcurrentAdmissionStore, IdempotencyStore, RateLimitStore,
};
use crate::tenant_runtime::{
    DynamicTenantRuntimeProfileSource, NoOpDynamicTenantRuntimeProfileSource, TenantRuntimeProfile,
};
use async_trait::async_trait;
use axum::extract::Request;
use axum::response::Response;
use sdkwork_web_contract::{RateLimitTier, RouteAuth};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebCallStage {
    RequestIdentity,
    SurfaceClassification,
    Cors,
    MethodGuard,
    HeaderSecurity,
    CrossSiteRequest,
    SqlInjectionGuard,
    RequestSizeLimit,
    RateLimit,
    Idempotency,
    RequestContextResolution,
    Authentication,
    Authorization,
    TenantIsolation,
    ContextInjection,
    Logging,
    Audit,
    ResponseIdentity,
}

pub const STANDARD_STAGE_ORDER: [WebCallStage; 18] = [
    WebCallStage::RequestIdentity,
    WebCallStage::SurfaceClassification,
    WebCallStage::Cors,
    WebCallStage::MethodGuard,
    WebCallStage::CrossSiteRequest,
    WebCallStage::SqlInjectionGuard,
    WebCallStage::RequestSizeLimit,
    WebCallStage::RateLimit,
    WebCallStage::Idempotency,
    WebCallStage::RequestContextResolution,
    WebCallStage::Authentication,
    WebCallStage::Authorization,
    WebCallStage::TenantIsolation,
    WebCallStage::ContextInjection,
    WebCallStage::Logging,
    WebCallStage::Audit,
    WebCallStage::HeaderSecurity,
    WebCallStage::ResponseIdentity,
];

#[derive(Clone, Debug)]
pub struct WebCallCredentials {
    pub auth_token: Option<String>,
    pub access_token: Option<String>,
    pub api_key: Option<String>,
    /// OAuth bearer for open-api when `Authorization: Bearer` is present without `Access-Token`.
    pub oauth_bearer: Option<String>,
    /// Backend agent bootstrap token (`X-SDKWork-Agent-Token`) for `RouteAuth::AgentToken` routes (C8-C9).
    pub agent_token: Option<String>,
}

#[derive(Clone, Debug)]
pub struct WebCallState {
    pub request_id: Option<ServerRequestId>,
    pub api_surface: WebApiSurface,
    pub auth_mode: WebAuthMode,
    pub principal: Option<WebRequestPrincipal>,
    pub path: String,
    pub method: String,
    pub origin: Option<String>,
    pub public_path: bool,
    pub operation_id: Option<String>,
    /// Manifest route template (`{param}`) when method+path matches.
    pub route_template: Option<String>,
    pub client_kind: Option<crate::request_context::WebClientKind>,
    /// Manifest `RouteAuth` when method+path matches; guides open-api scheme constraints.
    pub route_auth: Option<RouteAuth>,
    pub credentials: WebCallCredentials,
    pub idempotency_key: Option<String>,
    pub idempotency_fingerprint: Option<String>,
    pub idempotency_leader: bool,
    pub idempotency_replay: Option<IdempotencyResponseRecord>,
    pub traceparent: Option<String>,
    pub tracestate: Option<String>,
    pub rate_limit_tier: Option<RateLimitTier>,
    pub manifest_idempotent: bool,
    /// Effective CORS overlay from [`DynamicCorsPolicySource`] when resolved at stage Cors.
    pub resolved_cors: Option<CorsPolicy>,
    /// Tenant runtime profile overlay (e.g. body limit, rate-limit switch).
    pub tenant_runtime_profile: Option<TenantRuntimeProfile>,
    /// Effective rate limit overlay from [`DynamicRateLimitPolicySource`].
    pub resolved_rate_limit: Option<ResolvedRateLimitPolicy>,
    /// Held tenant concurrent admission slot key; released after the handler completes.
    pub concurrent_admission_key: Option<String>,
    /// Set at stage 15 (Logging) for post-handler audit duration.
    pub accepted_at: Option<std::time::Instant>,
    /// Manifest flag: reject inbound credential/context headers before handler logic.
    pub forbid_credential_headers: bool,
    /// 记录 before 阶段失败时的错误，供 Audit.after 为失败请求留痕使用。
    /// SECURITY_SPEC §5.1：审计完整性要求失败请求也留痕。
    pub before_failure: Option<WebFrameworkError>,
}

/// Assembly surface for resolver, policies, stores, and injectors (spec §4 `WebFrameworkRuntime`).
#[derive(Clone)]
pub struct WebCallRuntime<R>
where
    R: WebRequestContextResolver + Clone,
{
    pub resolver: R,
    pub profile: WebRequestContextProfile,
    pub security_policy: SecurityPolicy,
    pub domain_injectors: Vec<Arc<dyn DomainContextInjector>>,
    pub authorization: Arc<dyn AuthorizationPolicy>,
    pub tenant_isolation: Arc<dyn TenantIsolationPolicy>,
    pub rate_limit_store: Arc<dyn RateLimitStore>,
    pub idempotency_store: Arc<dyn IdempotencyStore>,
    pub audit_emitter: Arc<dyn AuditEmitter>,
    pub security_event_emitter: Arc<dyn SecurityEventEmitter>,
    pub rate_limit_resolver: Arc<dyn RateLimitPolicyResolver>,
    pub dynamic_cors_policy_source: Arc<dyn DynamicCorsPolicySource>,
    pub dynamic_rate_limit_policy_source: Arc<dyn DynamicRateLimitPolicySource>,
    pub dynamic_tenant_runtime_profile_source: Arc<dyn DynamicTenantRuntimeProfileSource>,
    pub concurrent_admission_store: Arc<dyn ConcurrentAdmissionStore>,
    pub optional_features: WebFrameworkOptionalFeatures,
    pub route_manifest: Option<HttpRouteManifest>,
    pub metrics: Option<Arc<crate::metrics::HttpMetricsRegistry>>,
    pub open_api_scheme_detector: DynOpenApiCredentialSchemeDetector,
    pub request_timeout: Option<std::time::Duration>,
}

/// Spec alias — same assembly type as [`WebCallRuntime`].
pub type WebFrameworkRuntime<R> = WebCallRuntime<R>;

impl<R> WebCallRuntime<R>
where
    R: WebRequestContextResolver + Clone,
{
    /// Development/test defaults: in-memory stores, allow-all authorization, payload-only JWT resolver.
    ///
    /// Production services MUST call [`Self::with_security_policy`] (`SecurityPolicy::production()`),
    /// [`Self::with_authorization_policy`], and wire a signature-verifying resolver.
    pub fn new(resolver: R) -> Self {
        Self::development(resolver)
    }

    /// Explicit development/test runtime — same as [`Self::new`].
    pub fn development(resolver: R) -> Self {
        Self {
            resolver,
            profile: WebRequestContextProfile::default(),
            security_policy: SecurityPolicy::default(),
            domain_injectors: Vec::new(),
            authorization: Arc::new(AllowAllAuthorizationPolicy),
            tenant_isolation: Arc::new(PassThroughTenantIsolationPolicy),
            rate_limit_store: memory_rate_limit_store(),
            idempotency_store: memory_idempotency_store(),
            audit_emitter: Arc::new(NoOpAuditEmitter),
            security_event_emitter: Arc::new(NoOpSecurityEventEmitter),
            rate_limit_resolver: Arc::new(DefaultRateLimitPolicyResolver),
            dynamic_cors_policy_source: Arc::new(NoOpDynamicCorsPolicySource),
            dynamic_rate_limit_policy_source: Arc::new(NoOpDynamicRateLimitPolicySource),
            dynamic_tenant_runtime_profile_source: Arc::new(NoOpDynamicTenantRuntimeProfileSource),
            concurrent_admission_store: memory_concurrent_admission_store(),
            optional_features: WebFrameworkOptionalFeatures::default(),
            route_manifest: None,
            metrics: None,
            open_api_scheme_detector: default_open_api_scheme_detector(),
            request_timeout: None,
        }
    }

    /// Production-oriented runtime: secure policy + deny-all auth + principal tenant isolation.
    pub fn production(resolver: R) -> Self {
        let mut runtime = Self::development(resolver)
            .with_security_policy(SecurityPolicy::production())
            .with_authorization_policy(Arc::new(crate::policies::DenyAllAuthorizationPolicy))
            .with_tenant_isolation_policy(Arc::new(
                crate::policies::EnforcePrincipalTenantIsolationPolicy,
            ));
        runtime.optional_features.json_content_type_guard = true;
        runtime.optional_features.production_security_defaults = true;
        runtime
    }

    pub fn with_profile(mut self, profile: WebRequestContextProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn with_security_policy(mut self, security_policy: SecurityPolicy) -> Self {
        self.security_policy = security_policy;
        self
    }

    pub fn with_domain_injector(mut self, injector: Arc<dyn DomainContextInjector>) -> Self {
        self.domain_injectors.push(injector);
        self
    }

    pub fn with_authorization_policy(mut self, policy: Arc<dyn AuthorizationPolicy>) -> Self {
        self.authorization = policy;
        self
    }

    pub fn with_tenant_isolation_policy(mut self, policy: Arc<dyn TenantIsolationPolicy>) -> Self {
        self.tenant_isolation = policy;
        self
    }

    pub fn with_rate_limit_store(mut self, store: Arc<dyn RateLimitStore>) -> Self {
        self.rate_limit_store = store;
        self
    }

    pub fn with_idempotency_store(mut self, store: Arc<dyn IdempotencyStore>) -> Self {
        self.idempotency_store = store;
        self
    }

    pub fn with_audit_emitter(mut self, emitter: Arc<dyn AuditEmitter>) -> Self {
        self.audit_emitter = emitter;
        self
    }

    pub fn with_security_event_emitter(mut self, emitter: Arc<dyn SecurityEventEmitter>) -> Self {
        self.security_event_emitter = emitter;
        self
    }

    pub fn with_rate_limit_resolver(mut self, resolver: Arc<dyn RateLimitPolicyResolver>) -> Self {
        self.rate_limit_resolver = resolver;
        self
    }

    pub fn with_dynamic_cors_policy_source(
        mut self,
        source: Arc<dyn DynamicCorsPolicySource>,
    ) -> Self {
        self.dynamic_cors_policy_source = source;
        self
    }

    pub fn with_dynamic_rate_limit_policy_source(
        mut self,
        source: Arc<dyn DynamicRateLimitPolicySource>,
    ) -> Self {
        self.dynamic_rate_limit_policy_source = source;
        self
    }

    pub fn with_dynamic_tenant_runtime_profile_source(
        mut self,
        source: Arc<dyn DynamicTenantRuntimeProfileSource>,
    ) -> Self {
        self.dynamic_tenant_runtime_profile_source = source;
        self
    }

    pub fn with_concurrent_admission_store(
        mut self,
        store: Arc<dyn ConcurrentAdmissionStore>,
    ) -> Self {
        self.concurrent_admission_store = store;
        self
    }

    pub fn effective_cors<'a>(&'a self, state: &'a WebCallState) -> &'a CorsPolicy {
        state
            .resolved_cors
            .as_ref()
            .unwrap_or(&self.security_policy.cors)
    }

    pub fn effective_max_content_length(&self, state: &WebCallState) -> Option<u64> {
        state
            .tenant_runtime_profile
            .as_ref()
            .and_then(|profile| profile.max_content_length)
            .or(self.security_policy.request_size_limit.max_content_length)
    }

    pub fn rate_limit_globally_enabled(&self, state: &WebCallState) -> bool {
        if state
            .tenant_runtime_profile
            .as_ref()
            .and_then(|profile| profile.rate_limit_enabled)
            == Some(false)
        {
            return false;
        }
        self.security_policy.rate_limit.enabled
    }

    pub fn with_optional_features(
        mut self,
        optional_features: WebFrameworkOptionalFeatures,
    ) -> Self {
        self.optional_features = optional_features;
        self
    }

    pub fn with_route_manifest(mut self, manifest: HttpRouteManifest) -> Self {
        self.route_manifest = Some(manifest);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<crate::metrics::HttpMetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn with_request_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    pub fn request_timeout(&self) -> Option<std::time::Duration> {
        self.request_timeout
    }

    pub fn with_open_api_scheme_detector(
        mut self,
        detector: DynOpenApiCredentialSchemeDetector,
    ) -> Self {
        self.open_api_scheme_detector = detector;
        self
    }

    pub fn metrics(&self) -> Option<&Arc<crate::metrics::HttpMetricsRegistry>> {
        self.metrics.as_ref()
    }
}

#[async_trait]
pub trait WebCallInterceptor<R>: Send + Sync + 'static
where
    R: WebRequestContextResolver + Clone,
{
    fn name(&self) -> &'static str;

    fn stage(&self) -> WebCallStage;

    async fn before(
        &self,
        _state: &mut WebCallState,
        _request: &mut Request,
        _runtime: &WebCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        Ok(())
    }

    async fn after(
        &self,
        _state: &WebCallState,
        _response: &mut Response,
        _runtime: &WebCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct WebCallInterceptorChain<R>
where
    R: WebRequestContextResolver + Clone,
{
    interceptors: Vec<Arc<dyn WebCallInterceptor<R>>>,
}

impl WebCallState {
    pub fn from_request(request: &Request) -> Self {
        let headers = request.headers();
        let access_token = access_token(headers);
        let bearer = bearer_token(headers);
        let oauth_bearer = if access_token.is_some() {
            None
        } else {
            bearer.clone()
        };
        Self {
            request_id: None,
            api_surface: WebApiSurface::Unknown,
            auth_mode: WebAuthMode::Public,
            principal: None,
            path: request.uri().path().to_owned(),
            method: request.method().as_str().to_owned(),
            origin: request
                .headers()
                .get("origin")
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            public_path: false,
            operation_id: header_value(headers, crate::constants::OPERATION_ID_HEADER),
            route_template: None,
            client_kind: None,
            route_auth: None,
            credentials: WebCallCredentials {
                auth_token: bearer,
                access_token,
                api_key: api_key(headers),
                oauth_bearer,
                agent_token: agent_token(headers),
            },
            idempotency_key: idempotency_key(headers),
            idempotency_fingerprint: None,
            idempotency_leader: false,
            idempotency_replay: None,
            traceparent: None,
            tracestate: None,
            rate_limit_tier: None,
            manifest_idempotent: false,
            resolved_cors: None,
            tenant_runtime_profile: None,
            resolved_rate_limit: None,
            concurrent_admission_key: None,
            accepted_at: None,
            forbid_credential_headers: false,
            before_failure: None,
        }
    }

    pub fn request_id_value(&self) -> Option<&str> {
        self.request_id
            .as_ref()
            .map(|request_id| request_id.0.as_str())
    }

    pub fn problem_correlation(&self) -> crate::problem::ProblemCorrelation<'_> {
        crate::problem::ProblemCorrelation::new(
            self.request_id_value(),
            self.traceparent
                .as_deref()
                .and_then(crate::trace::trace_id_from_traceparent),
        )
        .with_routing(
            Some(self.method.as_str()),
            self.route_template.as_deref(),
            Some(self.path.as_str()),
            self.operation_id.as_deref(),
        )
    }

    pub fn to_context(&self) -> Result<WebRequestContext, WebFrameworkError> {
        let request_id = self
            .request_id
            .clone()
            .ok_or_else(|| WebFrameworkError::bad_request("request id interceptor has not run"))?;
        let operation = self
            .operation_id
            .as_ref()
            .zip(self.route_template.as_ref())
            .map(
                |(operation_id, route_template)| crate::request_context::WebOperationBinding {
                    operation_id: operation_id.clone(),
                    route_template: route_template.clone(),
                    rate_limit_tier: self.rate_limit_tier,
                    idempotent: self.manifest_idempotent,
                },
            );
        Ok(WebRequestContext {
            request_id,
            api_surface: self.api_surface.clone(),
            auth_mode: self.auth_mode.clone(),
            principal: self.principal.clone(),
            transport: crate::request_context::WebTransportFacts {
                path: self.path.clone(),
                method: self.method.clone(),
                auth_token_present: self.credentials.auth_token.is_some(),
                access_token_present: self.credentials.access_token.is_some(),
                api_key_present: self.credentials.api_key.is_some(),
                oauth_bearer_present: self.credentials.oauth_bearer.is_some(),
                agent_token_present: self.credentials.agent_token.is_some(),
            },
            locale: None,
            client_kind: self.client_kind.clone(),
            operation,
            trace_id: self
                .traceparent
                .as_deref()
                .and_then(crate::trace::trace_id_from_traceparent)
                .map(str::to_owned),
        })
    }

    pub fn rate_limit_key(&self) -> String {
        use crate::hashing::hash_key_material;
        let path_hash = hash_key_material(&self.path);
        let tier_suffix = self
            .rate_limit_tier
            .map(|tier| format!(":tier:{tier:?}"))
            .unwrap_or_default();
        if let Some(principal) = &self.principal {
            format!(
                "tenant:{}:path:{}{tier_suffix}",
                hash_key_material(principal.tenant_id()),
                path_hash,
            )
        } else if self.credentials_present() {
            format!(
                "cred:{}:path:{}{tier_suffix}",
                hash_key_material(&self.credentials_fingerprint()),
                path_hash,
            )
        } else {
            format!("anon:path:{path_hash}{tier_suffix}")
        }
    }

    pub fn credentials_present(&self) -> bool {
        self.credentials.auth_token.is_some()
            || self.credentials.access_token.is_some()
            || self.credentials.api_key.is_some()
            || self.credentials.oauth_bearer.is_some()
            || self.credentials.agent_token.is_some()
    }

    /// Namespaced idempotency store key — scopes by credentials/principal to prevent cross-tenant replay.
    pub fn scoped_idempotency_store_key(&self, client_key: &str) -> String {
        use crate::hashing::hash_key_material;
        let scope = if self.public_path {
            "public".to_owned()
        } else if let Some(principal) = &self.principal {
            format!(
                "tenant={}:app={}:user={}",
                hash_key_material(principal.tenant_id()),
                hash_key_material(principal.app_id()),
                hash_key_material(principal.user_id()),
            )
        } else if let Some(api_key) = &self.credentials.api_key {
            format!("api_key={}", hash_key_material(api_key))
        } else if let Some(agent_token) = &self.credentials.agent_token {
            format!("agent_token={}", hash_key_material(agent_token))
        } else if let Some(oauth) = &self.credentials.oauth_bearer {
            format!("oauth={}", hash_key_material(oauth))
        } else {
            format!(
                "tokens={}",
                hash_key_material(&format!(
                    "{}|{}",
                    self.credentials.auth_token.as_deref().unwrap_or(""),
                    self.credentials.access_token.as_deref().unwrap_or("")
                ))
            )
        };
        format!("{}:{}", hash_key_material(&scope), client_key)
    }

    pub fn concurrent_admission_scope_key(&self) -> Option<String> {
        use crate::hashing::hash_key_material;
        if let Some(principal) = &self.principal {
            return Some(format!(
                "tenant:{}:concurrent",
                hash_key_material(principal.tenant_id())
            ));
        }
        if self.public_path {
            return None;
        }
        if self.credentials_present() {
            return Some(format!(
                "cred:{}:concurrent",
                hash_key_material(&self.credentials_fingerprint())
            ));
        }
        Some("anon:concurrent".to_owned())
    }

    fn credentials_fingerprint(&self) -> String {
        format!(
            "a={}:t={}:k={}:o={}:g={}",
            self.credentials.auth_token.is_some(),
            self.credentials.access_token.is_some(),
            self.credentials.api_key.is_some(),
            self.credentials.oauth_bearer.is_some(),
            self.credentials.agent_token.is_some()
        )
    }
}

impl<R> WebCallInterceptorChain<R>
where
    R: WebRequestContextResolver + Clone,
{
    pub fn new() -> Self {
        Self {
            interceptors: Vec::new(),
        }
    }

    pub fn standard() -> Self {
        use crate::interceptors::{StandardWebCallInterceptor, StandardWebCallInterceptorKind};

        Self::new()
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::RequestIdentity,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::SurfaceClassification,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::Cors,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::MethodGuard,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::CrossSiteRequest,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::SqlInjectionGuard,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::RequestSizeLimit,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::RateLimit,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::Idempotency,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::RequestContextResolution,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::Authentication,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::Authorization,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::TenantIsolation,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::ContextInjection,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::Logging,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::Audit,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::HeaderSecurity,
            ))
            .with_interceptor(StandardWebCallInterceptor::new(
                StandardWebCallInterceptorKind::ResponseIdentity,
            ))
    }

    pub fn with_interceptor<I>(mut self, interceptor: I) -> Self
    where
        I: WebCallInterceptor<R>,
    {
        self.interceptors.push(Arc::new(interceptor));
        self
    }

    pub fn push<I>(&mut self, interceptor: I)
    where
        I: WebCallInterceptor<R>,
    {
        self.interceptors.push(Arc::new(interceptor));
    }

    pub fn interceptor_count(&self) -> usize {
        self.interceptors.len()
    }

    pub fn stage_order(&self) -> Vec<WebCallStage> {
        self.interceptors
            .iter()
            .map(|interceptor| interceptor.stage())
            .collect()
    }

    pub async fn before(
        &self,
        state: &mut WebCallState,
        request: &mut Request,
        runtime: &WebCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        for interceptor in &self.interceptors {
            let started = std::time::Instant::now();
            if let Err(error) = interceptor.before(state, request, runtime).await {
                // 记录失败阶段的 error，供 Audit.after 使用。
                state.before_failure = Some(error.clone());
                // 返回错误前，仍记录该阶段耗时。
                if let Some(metrics) = runtime.metrics() {
                    metrics.record_pipeline_stage_duration(interceptor.name(), started.elapsed());
                }
                return Err(error);
            }
            if let Some(metrics) = runtime.metrics() {
                metrics.record_pipeline_stage_duration(interceptor.name(), started.elapsed());
            }
        }
        Ok(())
    }

    pub async fn after(
        &self,
        state: &WebCallState,
        response: &mut Response,
        runtime: &WebCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        // 反向遍历执行 after。即使 before 失败也调用 after，
        // 确保 Audit/ResponseIdentity/HeaderSecurity 等阶段为失败请求留痕。
        // 各阶段的 after 须容忍 state.before_failure = Some 的场景。
        for interceptor in self.interceptors.iter().rev() {
            interceptor.after(state, response, runtime).await?;
        }
        Ok(())
    }
}

impl<R> Default for WebCallInterceptorChain<R>
where
    R: WebRequestContextResolver + Clone,
{
    fn default() -> Self {
        Self::standard()
    }
}
