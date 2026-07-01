use sdkwork_web_axum::WebFrameworkLayer;
use sdkwork_web_core::{
    validate_deployment_environment, validate_production_assembly, AuditEmitter,
    AuthorizationPolicy, ConcurrentAdmissionStore, DenyAllAuthorizationPolicy,
    DomainContextInjector, DynamicCorsPolicySource, DynamicRateLimitPolicySource,
    DynamicTenantRuntimeProfileSource, EnforcePrincipalTenantIsolationPolicy,
    HttpMetricsDimensions, HttpMetricsRegistry, HttpRouteManifest, IdempotencyStore,
    ProductionAssemblyInput, RateLimitPolicyResolver, RateLimitStore, SecurityEventEmitter,
    SecurityPolicy, TenantIsolationPolicy, WebCallInterceptorChain, WebEnvironment,
    WebFrameworkOptionalFeatures, WebRequestContextProfile, WebRequestContextResolver,
    PRODUCTION_DEFAULT_REQUEST_TIMEOUT_SECS, PRODUCTION_DEFAULT_SHUTDOWN_GRACE_SECS,
};
use std::any::Any;
use std::sync::Arc;

#[cfg(feature = "admin-api")]
use sdkwork_web_core::DynamicPolicyCaches;
#[cfg(feature = "admin-api")]
use sqlx::SqlitePool;

use crate::health::{CompositeReadinessCheck, ReadinessCheck};
use crate::lifecycle::{NoOpWebFrameworkLifecycle, WebFrameworkLifecycle};
use crate::router::ServiceRouterConfig;
use crate::ContractFallbackConfig;
use std::net::SocketAddr;
use std::time::Duration;
/// Fluent builder for [`WebFrameworkLayer`] — North Star three-step integration entry.
pub struct WebFrameworkBuilder<R>
where
    R: WebRequestContextResolver + Clone + Any,
{
    resolver: R,
    profile: WebRequestContextProfile,
    security_policy: SecurityPolicy,
    call_chain: Option<WebCallInterceptorChain<R>>,
    domain_injectors: Vec<Arc<dyn DomainContextInjector>>,
    authorization: Option<Arc<dyn AuthorizationPolicy>>,
    tenant_isolation: Option<Arc<dyn TenantIsolationPolicy>>,
    rate_limit_store: Option<Arc<dyn RateLimitStore>>,
    idempotency_store: Option<Arc<dyn IdempotencyStore>>,
    concurrent_admission_store: Option<Arc<dyn ConcurrentAdmissionStore>>,
    audit_emitter: Option<Arc<dyn AuditEmitter>>,
    security_event_emitter: Option<Arc<dyn SecurityEventEmitter>>,
    rate_limit_resolver: Option<Arc<dyn RateLimitPolicyResolver>>,
    dynamic_cors_policy_source: Option<Arc<dyn DynamicCorsPolicySource>>,
    dynamic_rate_limit_policy_source: Option<Arc<dyn DynamicRateLimitPolicySource>>,
    dynamic_tenant_runtime_profile_source: Option<Arc<dyn DynamicTenantRuntimeProfileSource>>,
    optional_features: WebFrameworkOptionalFeatures,
    #[cfg(feature = "admin-api")]
    admin_api_pool: Option<SqlitePool>,
    #[cfg(feature = "admin-api")]
    admin_policy_caches: Option<Arc<DynamicPolicyCaches>>,
    route_manifest: Option<HttpRouteManifest>,
    metrics: Option<Arc<HttpMetricsRegistry>>,
    readiness_check: Option<Arc<dyn ReadinessCheck>>,
    request_timeout: Option<Duration>,
    shutdown_grace_period: Option<Duration>,
    lifecycle: Option<Arc<dyn WebFrameworkLifecycle>>,
    open_api_scheme_detector: Option<sdkwork_web_core::DynOpenApiCredentialSchemeDetector>,
}

pub struct WebFramework<R>
where
    R: WebRequestContextResolver + Clone + Any,
{
    pub layer: WebFrameworkLayer<R>,
    pub metrics: Arc<HttpMetricsRegistry>,
    readiness_check: Option<Arc<dyn ReadinessCheck>>,
    contract_fallback: Option<ContractFallbackConfig>,
    request_timeout: Option<Duration>,
    shutdown_grace_period: Option<Duration>,
    lifecycle: Arc<dyn WebFrameworkLifecycle>,
    #[cfg(feature = "admin-api")]
    pub admin_api_pool: Option<SqlitePool>,
    #[cfg(feature = "admin-api")]
    admin_policy_caches: Option<Arc<DynamicPolicyCaches>>,
}

impl<R> WebFramework<R>
where
    R: WebRequestContextResolver + Clone + Any,
{
    pub fn builder(resolver: R) -> WebFrameworkBuilder<R> {
        WebFrameworkBuilder::new(resolver)
    }

    pub fn layer(&self) -> &WebFrameworkLayer<R> {
        &self.layer
    }

    pub fn metrics(&self) -> &Arc<HttpMetricsRegistry> {
        &self.metrics
    }

    pub fn request_timeout(&self) -> Option<Duration> {
        self.request_timeout
    }

    pub fn shutdown_grace_period(&self) -> Option<Duration> {
        self.shutdown_grace_period
    }

    pub fn service_router_config(&self) -> ServiceRouterConfig {
        let mut config = ServiceRouterConfig::default().with_metrics(self.metrics.clone());
        if let Some(readiness) = self.readiness_check.clone() {
            config = config.with_readiness_check(readiness);
        }
        if let Some(fallback) = self.contract_fallback.clone() {
            config = config.with_contract_fallback(fallback);
        }
        config
    }

    /// Mount `/healthz`, `/readyz`, and `/metrics`. Request timeout applies inside
    /// [`with_web_request_context`] so idempotency reservations finalize correctly.
    pub fn mount_service_routes(&self, router: axum::Router) -> axum::Router {
        crate::router::service_router(router, self.service_router_config())
    }

    /// Mount service routes and run until shutdown (EP-20 lifecycle hooks included).
    pub async fn run(self, addr: SocketAddr, router: axum::Router) -> std::io::Result<()> {
        let router = self.mount_service_routes(router);
        crate::serve::serve_with_lifecycle(router, addr, self.lifecycle, self.shutdown_grace_period)
            .await
    }

    pub fn into_layer(self) -> WebFrameworkLayer<R> {
        self.layer
    }

    /// Mount optional admin-api routes when built with [`WebFrameworkBuilder::enable_admin_api`].
    pub fn mount_admin_routes(&self, router: axum::Router) -> axum::Router {
        #[cfg(feature = "admin-api")]
        {
            if let Some(pool) = self.admin_api_pool.clone() {
                return crate::admin_api::mount_web_framework_admin_api(
                    router,
                    pool,
                    self.layer.clone(),
                    self.admin_policy_caches.clone(),
                );
            }
        }
        let _ = self;
        router
    }
}

impl<R> WebFrameworkBuilder<R>
where
    R: WebRequestContextResolver + Clone + Any,
{
    pub fn new(resolver: R) -> Self {
        Self {
            resolver,
            profile: WebRequestContextProfile::default(),
            security_policy: SecurityPolicy::default(),
            call_chain: None,
            domain_injectors: Vec::new(),
            authorization: None,
            tenant_isolation: None,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: None,
            security_event_emitter: None,
            rate_limit_resolver: None,
            dynamic_cors_policy_source: None,
            dynamic_rate_limit_policy_source: None,
            dynamic_tenant_runtime_profile_source: None,
            optional_features: WebFrameworkOptionalFeatures::default(),
            #[cfg(feature = "admin-api")]
            admin_api_pool: None,
            #[cfg(feature = "admin-api")]
            admin_policy_caches: None,
            route_manifest: None,
            metrics: None,
            readiness_check: None,
            request_timeout: None,
            shutdown_grace_period: None,
            lifecycle: None,
            open_api_scheme_detector: None,
        }
    }

    pub fn profile(mut self, profile: WebRequestContextProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn security_policy(mut self, security_policy: SecurityPolicy) -> Self {
        self.security_policy = security_policy;
        self
    }

    /// Production SaaS defaults: rate limiting enabled, HSTS, strict CORS, deny-all authorization until configured.
    pub fn production_defaults(mut self) -> Self {
        self.optional_features = WebFrameworkOptionalFeatures::production_sqlx();
        if self.optional_features.production_security_defaults {
            self.security_policy = SecurityPolicy::production();
            self.profile.environment = WebEnvironment::Prod;
            if self.authorization.is_none() {
                self.authorization = Some(Arc::new(DenyAllAuthorizationPolicy));
            }
            if self.tenant_isolation.is_none() {
                self.tenant_isolation = Some(Arc::new(EnforcePrincipalTenantIsolationPolicy));
            }
            if self.request_timeout.is_none() {
                self.request_timeout =
                    Some(Duration::from_secs(PRODUCTION_DEFAULT_REQUEST_TIMEOUT_SECS));
            }
            if self.shutdown_grace_period.is_none() {
                self.shutdown_grace_period =
                    Some(Duration::from_secs(PRODUCTION_DEFAULT_SHUTDOWN_GRACE_SECS));
            }
        }
        if !self.optional_features.json_content_type_guard {
            self.security_policy.json_content_type.enabled = false;
        }
        self
    }

    pub fn optional_features(mut self, features: WebFrameworkOptionalFeatures) -> Self {
        self.optional_features = features;
        self
    }

    #[cfg(feature = "admin-api")]
    pub fn enable_admin_api(mut self, pool: SqlitePool) -> Self {
        if self.readiness_check.is_none() {
            self.readiness_check = Some(Arc::new(
                crate::sqlx_readiness::SqliteReadinessCheck::new(pool.clone()),
            ));
        }
        self.admin_api_pool = Some(pool);
        self
    }

    #[cfg(feature = "admin-api")]
    pub fn admin_policy_caches(mut self, caches: Arc<DynamicPolicyCaches>) -> Self {
        self.admin_policy_caches = Some(caches);
        self
    }

    pub fn call_chain(mut self, call_chain: WebCallInterceptorChain<R>) -> Self {
        self.call_chain = Some(call_chain);
        self
    }

    pub fn domain_injector(mut self, injector: Arc<dyn DomainContextInjector>) -> Self {
        self.domain_injectors.push(injector);
        self
    }

    pub fn authorization_policy(mut self, policy: Arc<dyn AuthorizationPolicy>) -> Self {
        self.authorization = Some(policy);
        self
    }

    pub fn tenant_isolation_policy(mut self, policy: Arc<dyn TenantIsolationPolicy>) -> Self {
        self.tenant_isolation = Some(policy);
        self
    }

    pub fn rate_limit_store(mut self, store: Arc<dyn RateLimitStore>) -> Self {
        self.rate_limit_store = Some(store);
        self
    }

    pub fn idempotency_store(mut self, store: Arc<dyn IdempotencyStore>) -> Self {
        self.idempotency_store = Some(store);
        self
    }

    pub fn concurrent_admission_store(mut self, store: Arc<dyn ConcurrentAdmissionStore>) -> Self {
        self.concurrent_admission_store = Some(store);
        self
    }

    pub fn audit_emitter(mut self, emitter: Arc<dyn AuditEmitter>) -> Self {
        self.audit_emitter = Some(emitter);
        self
    }

    pub fn security_event_emitter(mut self, emitter: Arc<dyn SecurityEventEmitter>) -> Self {
        self.security_event_emitter = Some(emitter);
        self
    }

    pub fn rate_limit_resolver(mut self, resolver: Arc<dyn RateLimitPolicyResolver>) -> Self {
        self.rate_limit_resolver = Some(resolver);
        self
    }

    pub fn dynamic_cors_policy_source(mut self, source: Arc<dyn DynamicCorsPolicySource>) -> Self {
        self.dynamic_cors_policy_source = Some(source);
        self
    }

    pub fn dynamic_rate_limit_policy_source(
        mut self,
        source: Arc<dyn DynamicRateLimitPolicySource>,
    ) -> Self {
        self.dynamic_rate_limit_policy_source = Some(source);
        self
    }

    pub fn dynamic_tenant_runtime_profile_source(
        mut self,
        source: Arc<dyn DynamicTenantRuntimeProfileSource>,
    ) -> Self {
        self.dynamic_tenant_runtime_profile_source = Some(source);
        self
    }

    pub fn route_manifest(mut self, manifest: HttpRouteManifest) -> Self {
        self.route_manifest = Some(manifest);
        self
    }

    pub fn metrics_registry(mut self, metrics: Arc<HttpMetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn readiness_check(mut self, check: Arc<dyn ReadinessCheck>) -> Self {
        self.readiness_check = Some(check);
        self
    }

    pub fn composite_readiness_check(mut self, checks: Vec<Arc<dyn ReadinessCheck>>) -> Self {
        self.readiness_check = Some(Arc::new(CompositeReadinessCheck::new(checks)));
        self
    }

    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    pub fn shutdown_grace_period(mut self, period: Duration) -> Self {
        self.shutdown_grace_period = Some(period);
        self
    }

    pub fn lifecycle(mut self, lifecycle: Arc<dyn WebFrameworkLifecycle>) -> Self {
        self.lifecycle = Some(lifecycle);
        self
    }

    pub fn open_api_scheme_detector(
        mut self,
        detector: sdkwork_web_core::DynOpenApiCredentialSchemeDetector,
    ) -> Self {
        self.open_api_scheme_detector = Some(detector);
        self
    }

    pub fn build(self) -> WebFramework<R> {
        #[cfg(feature = "admin-api")]
        let route_manifest = if self.route_manifest.is_none() && self.admin_api_pool.is_some() {
            Some(HttpRouteManifest::new(
                sdkwork_routes_web_framework_backend_api::ROUTES,
            ))
        } else {
            self.route_manifest
        };
        #[cfg(not(feature = "admin-api"))]
        let route_manifest = self.route_manifest;

        if let Some(manifest) = route_manifest {
            if let Err(message) =
                manifest.validate_public_path_prefixes(&self.profile.public_path_prefixes)
            {
                panic!(
                    "WebFrameworkBuilder route manifest is inconsistent with profile: {message}"
                );
            }
            if let Err(message) = manifest.validate_route_auth_for_surfaces(&self.profile) {
                panic!(
                    "WebFrameworkBuilder route manifest auth is inconsistent with API surfaces: {message}"
                );
            }
            if let Err(message) = manifest.validate_no_ambient_context_path_markers(&self.profile) {
                panic!(
                    "WebFrameworkBuilder route manifest uses forbidden ambient context path markers: {message}"
                );
            }
        }

        if let Err(message) = validate_deployment_environment(
            self.profile.environment.clone(),
            self.optional_features.production_security_defaults,
        ) {
            panic!("WebFrameworkBuilder deployment environment is unsafe: {message}");
        }

        if let Err(message) = validate_production_assembly(ProductionAssemblyInput {
            environment: self.profile.environment.clone(),
            production_security_defaults: self.optional_features.production_security_defaults,
            security_policy: &self.security_policy,
            authorization: &self.authorization,
            tenant_isolation: &self.tenant_isolation,
            resolver: &self.resolver,
            control_plane_standalone: self.optional_features.control_plane_standalone,
            has_readiness_probe: self.readiness_check.is_some(),
            rate_limit_store: self.rate_limit_store.as_ref(),
            idempotency_store: self.idempotency_store.as_ref(),
            concurrent_admission_store: self.concurrent_admission_store.as_ref(),
            audit_emitter: self.audit_emitter.as_ref(),
            security_event_emitter: self.security_event_emitter.as_ref(),
        }) {
            panic!("WebFrameworkBuilder production assembly is unsafe: {message}");
        }

        let metrics = match self.metrics {
            Some(metrics) => {
                metrics.set_dimensions(HttpMetricsDimensions::from_profile_environment(
                    self.profile.environment.clone(),
                ));
                metrics
            }
            None => HttpMetricsRegistry::with_dimensions(
                HttpMetricsDimensions::from_profile_environment(self.profile.environment.clone()),
            ),
        };
        let mut layer = WebFrameworkLayer::new(self.resolver)
            .with_profile(self.profile)
            .with_security_policy(self.security_policy)
            .with_metrics(metrics.clone());
        if let Some(chain) = self.call_chain {
            layer = layer.with_call_chain(chain);
        }
        for injector in self.domain_injectors {
            layer = layer.with_domain_injector(injector);
        }
        if let Some(policy) = self.authorization {
            layer = layer.with_authorization_policy(policy);
        }
        if let Some(policy) = self.tenant_isolation {
            layer = layer.with_tenant_isolation_policy(policy);
        }
        if let Some(store) = self.rate_limit_store {
            layer = layer.with_rate_limit_store(store);
        }
        if let Some(store) = self.idempotency_store {
            layer = layer.with_idempotency_store(store);
        }
        if let Some(store) = self.concurrent_admission_store {
            layer = layer.with_concurrent_admission_store(store);
        }
        if let Some(emitter) = self.audit_emitter {
            layer = layer.with_audit_emitter(emitter);
        }
        if let Some(emitter) = self.security_event_emitter {
            layer = layer.with_security_event_emitter(emitter);
        }
        if let Some(resolver) = self.rate_limit_resolver {
            layer = layer.with_rate_limit_resolver(resolver);
        }
        if let Some(source) = self.dynamic_cors_policy_source {
            layer = layer.with_dynamic_cors_policy_source(source);
        }
        if let Some(source) = self.dynamic_rate_limit_policy_source {
            layer = layer.with_dynamic_rate_limit_policy_source(source);
        }
        if let Some(source) = self.dynamic_tenant_runtime_profile_source {
            layer = layer.with_dynamic_tenant_runtime_profile_source(source);
        }
        if let Some(detector) = self.open_api_scheme_detector {
            layer = layer.with_open_api_scheme_detector(detector);
        }
        layer = layer.with_optional_features(self.optional_features);
        let contract_fallback = route_manifest
            .as_ref()
            .map(ContractFallbackConfig::from_manifest);
        if let Some(manifest) = route_manifest {
            layer = layer.with_route_manifest(manifest);
        }
        if let Some(timeout) = self.request_timeout {
            layer = layer.with_request_timeout(timeout);
        }
        WebFramework {
            layer,
            metrics,
            readiness_check: self.readiness_check,
            contract_fallback,
            request_timeout: self.request_timeout,
            shutdown_grace_period: self.shutdown_grace_period,
            lifecycle: self
                .lifecycle
                .unwrap_or_else(|| Arc::new(NoOpWebFrameworkLifecycle)),
            #[cfg(feature = "admin-api")]
            admin_api_pool: self.admin_api_pool,
            #[cfg(feature = "admin-api")]
            admin_policy_caches: self.admin_policy_caches,
        }
    }
}
