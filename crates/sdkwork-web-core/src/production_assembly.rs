//! Production assembly validation for [`WebFrameworkBuilder`](crate::WebFrameworkRuntime) wiring.

use crate::jwt::{
    HmacSha256JwtVerifier, PayloadOnlyJwtVerifier, VerifyingAccessTokenParser,
    VerifyingAuthTokenParser,
};
use crate::parsers::DefaultApiKeyParser;
use crate::policies::{
    AllowAllAuthorizationPolicy, NoOpAuditEmitter, NoOpSecurityEventEmitter,
    PassThroughTenantIsolationPolicy,
};
use crate::request_context::WebEnvironment;
use crate::resolvers::{ResolverProductionProfile, WebRequestParserResolver};
use crate::security::SecurityPolicy;
use crate::stores::{
    ConcurrentAdmissionStore, IdempotencyStore, MemoryConcurrentAdmissionStore,
    MemoryIdempotencyStore, MemoryRateLimitStore, RateLimitStore,
};
use crate::DefaultApiKeyLookupService;
use crate::{
    AuditEmitter, AuthorizationPolicy, SecurityEventEmitter, TenantIsolationPolicy,
    WebRequestContextResolver,
};
use std::any::{Any, TypeId};
use std::sync::Arc;

/// Inputs collected before a production-facing framework layer is assembled.
pub struct ProductionAssemblyInput<'a, R>
where
    R: WebRequestContextResolver + Clone,
{
    pub environment: WebEnvironment,
    pub production_security_defaults: bool,
    pub security_policy: &'a SecurityPolicy,
    pub authorization: &'a Option<Arc<dyn AuthorizationPolicy>>,
    pub tenant_isolation: &'a Option<Arc<dyn TenantIsolationPolicy>>,
    pub resolver: &'a R,
    pub control_plane_standalone: bool,
    pub has_readiness_probe: bool,
    pub rate_limit_store: Option<&'a Arc<dyn RateLimitStore>>,
    pub idempotency_store: Option<&'a Arc<dyn IdempotencyStore>>,
    pub concurrent_admission_store: Option<&'a Arc<dyn ConcurrentAdmissionStore>>,
    pub audit_emitter: Option<&'a Arc<dyn AuditEmitter>>,
    pub security_event_emitter: Option<&'a Arc<dyn SecurityEventEmitter>>,
}

/// Returns `true` when the deployment profile requires production-safe wiring.
pub fn requires_production_assembly(
    environment: WebEnvironment,
    production_security_defaults: bool,
) -> bool {
    production_security_defaults || environment == WebEnvironment::Prod
}

/// Rejects deployments where process env declares production but builder wiring is still dev-safe.
pub fn validate_deployment_environment(
    environment: WebEnvironment,
    production_security_defaults: bool,
) -> Result<(), String> {
    let Some(raw) = std::env::var("SDKWORK_WEB_FRAMEWORK_ENV").ok() else {
        return Ok(());
    };
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized != "prod" && normalized != "production" {
        return Ok(());
    }
    if !requires_production_assembly(environment.clone(), production_security_defaults) {
        return Err(
            "SDKWORK_WEB_FRAMEWORK_ENV=prod requires WebEnvironment::Prod and production_defaults(); call WebFrameworkBuilder::production_defaults() before build()".into(),
        );
    }
    if environment != WebEnvironment::Prod {
        return Err(
            "SDKWORK_WEB_FRAMEWORK_ENV=prod requires WebRequestContextProfile.environment = Prod"
                .into(),
        );
    }
    Ok(())
}

pub fn validate_production_assembly<R>(input: ProductionAssemblyInput<'_, R>) -> Result<(), String>
where
    R: WebRequestContextResolver + Clone + Any,
{
    if !requires_production_assembly(input.environment, input.production_security_defaults) {
        return Ok(());
    }

    if !input.security_policy.rate_limit.enabled {
        return Err(
            "production assembly requires SecurityPolicy.rate_limit.enabled = true; call production_defaults() or configure rate limiting explicitly".into(),
        );
    }

    input.security_policy.cors.validate_for_production()?;

    if is_noop_audit_emitter(input.audit_emitter) {
        return Err(
            "production assembly requires an explicit AuditEmitter; wire SqlxAuditEmitter or a domain implementation through WebFrameworkBuilder::audit_emitter()".into(),
        );
    }

    if is_noop_security_event_emitter(input.security_event_emitter) {
        return Err(
            "production assembly requires an explicit SecurityEventEmitter; wire SqlxSecurityEventEmitter or a domain implementation through WebFrameworkBuilder::security_event_emitter()".into(),
        );
    }

    match input.authorization {
        Some(policy) if is_allow_all_authorization(policy) => {
            return Err(
                "production assembly must not use AllowAllAuthorizationPolicy; wire a real AuthorizationPolicy".into(),
            );
        }
        None => {
            return Err(
                "production assembly requires an explicit AuthorizationPolicy; production_defaults() sets DenyAllAuthorizationPolicy when none is provided".into(),
            );
        }
        Some(_) => {}
    }

    match input.tenant_isolation {
        Some(policy) if is_pass_through_tenant_isolation(policy) => {
            return Err(
                "production assembly must not use PassThroughTenantIsolationPolicy; wire TenantIsolationPolicy".into(),
            );
        }
        None => {
            return Err("production assembly requires an explicit TenantIsolationPolicy".into());
        }
        Some(_) => {}
    }

    if is_dev_only_resolver(input.resolver) {
        return Err(
            "production assembly must not use DefaultWebRequestContextResolver or DefaultOpenApiWebRequestContextResolver; wire verifying_web_request_resolver() with a signature-verifying JwtVerifier".into(),
        );
    }

    if uses_payload_only_verifying_resolver(input.resolver)
        || uses_hmac_sha256_verifying_resolver(input.resolver)
    {
        return Err(
            "production assembly must not use PayloadOnlyJwtVerifier or HmacSha256JwtVerifier; wire TenantBoundJwtVerifier with TenantSigningKeyLookup".into(),
        );
    }

    let profile = input.resolver.resolver_production_profile();
    if !matches!(
        profile,
        ResolverProductionProfile::TenantBoundBootstrap
            | ResolverProductionProfile::TenantBoundSaaS
    ) {
        return Err(
            "production assembly requires tenant_bound_verifying_web_request_resolver() or tenant_bound_saas_verifying_web_request_resolver() with TenantSigningKeyLookup".into(),
        );
    }

    if !input.control_plane_standalone {
        if profile != ResolverProductionProfile::TenantBoundSaaS {
            return Err(
                "production SaaS assembly requires tenant_bound_saas_verifying_web_request_resolver() with JwtSessionRevocationChecker".into(),
            );
        }
        if !input.has_readiness_probe {
            return Err(
                "production SaaS assembly requires an explicit ReadinessCheck wired through WebFrameworkBuilder::readiness_check()".into(),
            );
        }
    }

    if !input.control_plane_standalone {
        if uses_memory_rate_limit_store(input.rate_limit_store) {
            return Err(
                "production SaaS assembly must not use MemoryRateLimitStore; wire RedisRateLimitStore".into(),
            );
        }
        if !uses_redis_rate_limit_store(input.rate_limit_store) {
            return Err(
                "production SaaS assembly must wire RedisRateLimitStore; SQLx rate-limit stores are not multi-replica HA-safe".into(),
            );
        }
        if uses_memory_idempotency_store(input.idempotency_store) {
            return Err(
                "production SaaS assembly must not use MemoryIdempotencyStore; wire RedisIdempotencyStore".into(),
            );
        }
        if !uses_redis_idempotency_store(input.idempotency_store) {
            return Err(
                "production SaaS assembly must wire RedisIdempotencyStore; SQLx idempotency stores are not multi-replica HA-safe".into(),
            );
        }
        if uses_memory_concurrent_admission_store(input.concurrent_admission_store) {
            return Err(
                "production SaaS assembly must not use MemoryConcurrentAdmissionStore; wire RedisConcurrentAdmissionStore".into(),
            );
        }
        if !uses_redis_concurrent_admission_store(input.concurrent_admission_store) {
            return Err(
                "production SaaS assembly must wire RedisConcurrentAdmissionStore for distributed tenant concurrency limits".into(),
            );
        }
        if input.resolver.uses_default_api_key_lookup() {
            return Err(
                "production SaaS assembly must not use DefaultApiKeyLookupService; wire a server-side ApiKeyLookupService".into(),
            );
        }
        if input.resolver.uses_default_oauth_token_lookup() {
            return Err(
                "production SaaS assembly must not use DefaultOAuthTokenLookupService; wire a server-side OAuthTokenLookupService".into(),
            );
        }
        if profile == ResolverProductionProfile::TenantBoundSaaS {
            match input.resolver.jwt_production_claim_policy() {
                Some(claim_policy) if claim_policy.has_saas_issuer_audience_allowlist() => {}
                Some(_) => {
                    return Err(
                        "production SaaS assembly requires JwtProductionClaimPolicy::saas_production with non-empty issuers and audiences; use tenant_bound_saas_verifying_web_request_resolver_with_claim_policy".into(),
                    );
                }
                None => {
                    return Err(
                        "production SaaS assembly requires tenant_bound_saas_verifying_web_request_resolver_with_claim_policy for IAM issuer/audience validation".into(),
                    );
                }
            }
        }
    }

    Ok(())
}

fn is_noop_audit_emitter(emitter: Option<&Arc<dyn AuditEmitter>>) -> bool {
    match emitter {
        None => true,
        Some(emitter) => emitter.as_ref().type_id() == TypeId::of::<NoOpAuditEmitter>(),
    }
}

fn is_noop_security_event_emitter(emitter: Option<&Arc<dyn SecurityEventEmitter>>) -> bool {
    match emitter {
        None => true,
        Some(emitter) => emitter.as_ref().type_id() == TypeId::of::<NoOpSecurityEventEmitter>(),
    }
}

fn uses_memory_rate_limit_store(store: Option<&Arc<dyn RateLimitStore>>) -> bool {
    match store {
        None => true,
        Some(store) => (store.as_ref() as &dyn Any).is::<MemoryRateLimitStore>(),
    }
}

fn uses_memory_idempotency_store(store: Option<&Arc<dyn IdempotencyStore>>) -> bool {
    match store {
        None => true,
        Some(store) => (store.as_ref() as &dyn Any).is::<MemoryIdempotencyStore>(),
    }
}

fn uses_redis_rate_limit_store(store: Option<&Arc<dyn RateLimitStore>>) -> bool {
    match store {
        None => false,
        Some(store) => store.is_distributed_ha(),
    }
}

fn uses_redis_idempotency_store(store: Option<&Arc<dyn IdempotencyStore>>) -> bool {
    match store {
        None => false,
        Some(store) => store.is_distributed_ha(),
    }
}

fn uses_memory_concurrent_admission_store(
    store: Option<&Arc<dyn ConcurrentAdmissionStore>>,
) -> bool {
    match store {
        None => true,
        Some(store) => (store.as_ref() as &dyn Any).is::<MemoryConcurrentAdmissionStore>(),
    }
}

fn uses_redis_concurrent_admission_store(
    store: Option<&Arc<dyn ConcurrentAdmissionStore>>,
) -> bool {
    match store {
        None => false,
        Some(store) => store.is_distributed_ha(),
    }
}

fn is_allow_all_authorization(policy: &Arc<dyn AuthorizationPolicy>) -> bool {
    policy.as_ref().type_id() == TypeId::of::<AllowAllAuthorizationPolicy>()
}

type HmacSha256VerifyingResolver = WebRequestParserResolver<
    VerifyingAuthTokenParser<HmacSha256JwtVerifier>,
    VerifyingAccessTokenParser<HmacSha256JwtVerifier>,
    DefaultApiKeyParser,
    DefaultApiKeyLookupService,
>;

fn uses_hmac_sha256_verifying_resolver<R>(resolver: &R) -> bool
where
    R: Any,
{
    resolver.type_id() == TypeId::of::<HmacSha256VerifyingResolver>()
}

type PayloadOnlyVerifyingResolver = WebRequestParserResolver<
    VerifyingAuthTokenParser<PayloadOnlyJwtVerifier>,
    VerifyingAccessTokenParser<PayloadOnlyJwtVerifier>,
    DefaultApiKeyParser,
    DefaultApiKeyLookupService,
>;

fn uses_payload_only_verifying_resolver<R>(resolver: &R) -> bool
where
    R: Any,
{
    resolver.type_id() == TypeId::of::<PayloadOnlyVerifyingResolver>()
}

fn is_pass_through_tenant_isolation(policy: &Arc<dyn TenantIsolationPolicy>) -> bool {
    policy.as_ref().type_id() == TypeId::of::<PassThroughTenantIsolationPolicy>()
}

fn is_dev_only_resolver<R>(resolver: &R) -> bool
where
    R: Any,
{
    use crate::resolvers::{
        DefaultOpenApiWebRequestContextResolver, DefaultWebRequestContextResolver,
    };

    let type_id = resolver.type_id();
    type_id == TypeId::of::<DefaultWebRequestContextResolver>()
        || type_id == TypeId::of::<DefaultOpenApiWebRequestContextResolver>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::WebFrameworkError;
    use crate::policies::{AuditFact, DenyAllAuthorizationPolicy, SecurityEvent};
    use crate::request_context::WebRequestContext;
    use crate::resolvers::DefaultWebRequestContextResolver;
    use async_trait::async_trait;

    #[derive(Clone)]
    struct TestAuditEmitter;

    #[async_trait]
    impl AuditEmitter for TestAuditEmitter {
        async fn emit(&self, _fact: AuditFact) -> Result<(), WebFrameworkError> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct TestSecurityEventEmitter;

    #[async_trait]
    impl SecurityEventEmitter for TestSecurityEventEmitter {
        async fn emit(&self, _event: SecurityEvent) -> Result<(), WebFrameworkError> {
            Ok(())
        }
    }

    fn test_audit_emitter() -> Arc<dyn AuditEmitter> {
        Arc::new(TestAuditEmitter)
    }

    fn test_security_event_emitter() -> Arc<dyn SecurityEventEmitter> {
        Arc::new(TestSecurityEventEmitter)
    }

    fn production_emitters() -> (Arc<dyn AuditEmitter>, Arc<dyn SecurityEventEmitter>) {
        (test_audit_emitter(), test_security_event_emitter())
    }

    struct FixedTenantIsolation;

    impl TenantIsolationPolicy for FixedTenantIsolation {
        fn enforce(
            &self,
            _ctx: &WebRequestContext,
            _operation_id: Option<&str>,
        ) -> Result<(), WebFrameworkError> {
            Ok(())
        }
    }

    #[test]
    fn development_profile_skips_validation() {
        let resolver = DefaultWebRequestContextResolver::default();
        let security = SecurityPolicy::default();
        let authorization = None;
        let tenant_isolation = None;
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Dev,
            production_security_defaults: false,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: false,
            has_readiness_probe: false,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: None,
            security_event_emitter: None,
        };
        validate_production_assembly(input).expect("dev profile");
    }

    #[test]
    fn production_profile_rejects_dev_resolver() {
        let resolver = DefaultWebRequestContextResolver::default();
        let security = SecurityPolicy::production();
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: false,
            has_readiness_probe: false,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        let error = validate_production_assembly(input).expect_err("dev resolver");
        assert!(error.contains("DefaultWebRequestContextResolver"));
    }

    #[test]
    fn production_profile_rejects_payload_only_verifying_resolver() {
        use crate::jwt::PayloadOnlyJwtVerifier;
        use crate::verifying_web_request_resolver;

        let resolver = verifying_web_request_resolver(
            Arc::new(PayloadOnlyJwtVerifier),
            crate::DefaultApiKeyLookupService,
        );
        let security = SecurityPolicy::production();
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: false,
            has_readiness_probe: false,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        let error = validate_production_assembly(input).expect_err("payload-only verifier");
        assert!(error.contains("PayloadOnlyJwtVerifier"));
    }

    #[test]
    fn production_profile_rejects_hmac_sha256_verifying_resolver() {
        use crate::jwt::HmacSha256JwtVerifier;
        use crate::verifying_web_request_resolver;

        let resolver = verifying_web_request_resolver(
            Arc::new(HmacSha256JwtVerifier::new("secret")),
            crate::DefaultApiKeyLookupService,
        );
        let security = SecurityPolicy::production();
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: false,
            has_readiness_probe: false,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        let error = validate_production_assembly(input).expect_err("global hmac verifier");
        assert!(error.contains("HmacSha256JwtVerifier"));
    }

    #[test]
    fn production_profile_accepts_tenant_bound_verifying_resolver() {
        use crate::jwt_tenant::EnvBootstrapTenantSigningKeyLookup;
        use crate::tenant_bound_verifying_web_request_resolver;

        let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
        let resolver =
            tenant_bound_verifying_web_request_resolver(lookup, crate::DefaultApiKeyLookupService);
        let security = SecurityPolicy::production();
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: true,
            has_readiness_probe: false,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        validate_production_assembly(input).expect("tenant-bound verifier");
    }

    #[test]
    fn production_saas_profile_rejects_memory_stores() {
        use crate::jwt_claims::JwtProductionClaimPolicy;
        use crate::jwt_tenant::{
            EnvBootstrapTenantSigningKeyLookup, NoOpJwtSessionRevocationChecker,
        };
        use crate::resolvers::ApiKeyLookupService;
        use crate::tenant_bound_saas_verifying_web_request_resolver_with_claim_policy;
        use async_trait::async_trait;

        #[derive(Clone)]
        struct StubApiKeyLookup;

        #[async_trait]
        impl ApiKeyLookupService for StubApiKeyLookup {
            async fn lookup_api_key(
                &self,
                _credential: &crate::parsers::ApiKeyCredential,
            ) -> Result<crate::resolvers::ApiKeyPrincipalRecord, WebFrameworkError> {
                Err(WebFrameworkError::dependency_unavailable("stub"))
            }
        }

        let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
        let resolver = tenant_bound_saas_verifying_web_request_resolver_with_claim_policy(
            lookup,
            NoOpJwtSessionRevocationChecker,
            StubApiKeyLookup,
            JwtProductionClaimPolicy::saas_production(
                vec!["https://iam.sdkwork.dev".to_owned()],
                vec!["sdkwork-api".to_owned()],
            ),
        );
        let security = SecurityPolicy::production();
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let memory_rate_limit =
            Arc::new(MemoryRateLimitStore::default()) as Arc<dyn RateLimitStore>;
        let memory_idempotency =
            Arc::new(MemoryIdempotencyStore::default()) as Arc<dyn IdempotencyStore>;
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: false,
            has_readiness_probe: true,
            rate_limit_store: Some(&memory_rate_limit),
            idempotency_store: Some(&memory_idempotency),
            concurrent_admission_store: None,
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        let error = validate_production_assembly(input).expect_err("memory stores");
        assert!(error.contains("MemoryRateLimitStore"));
    }

    #[test]
    fn production_saas_profile_rejects_bootstrap_tenant_bound_resolver() {
        use crate::jwt_tenant::EnvBootstrapTenantSigningKeyLookup;
        use crate::tenant_bound_verifying_web_request_resolver;

        let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
        let resolver =
            tenant_bound_verifying_web_request_resolver(lookup, crate::DefaultApiKeyLookupService);
        let security = SecurityPolicy::production();
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: false,
            has_readiness_probe: true,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        let error =
            validate_production_assembly(input).expect_err("bootstrap resolver on saas profile");
        assert!(error.contains("tenant_bound_saas_verifying_web_request_resolver"));
    }

    #[test]
    fn production_saas_profile_rejects_empty_iss_aud_claim_policy() {
        use crate::jwt_tenant::{
            EnvBootstrapTenantSigningKeyLookup, NoOpJwtSessionRevocationChecker,
        };
        use crate::resolvers::ApiKeyLookupService;
        use crate::tenant_bound_saas_verifying_web_request_resolver;
        use async_trait::async_trait;

        #[derive(Clone)]
        struct StubApiKeyLookup;

        #[async_trait]
        impl ApiKeyLookupService for StubApiKeyLookup {
            async fn lookup_api_key(
                &self,
                _credential: &crate::parsers::ApiKeyCredential,
            ) -> Result<crate::resolvers::ApiKeyPrincipalRecord, WebFrameworkError> {
                Err(WebFrameworkError::dependency_unavailable("stub"))
            }
        }

        #[derive(Clone)]
        struct RedisRateLimitStoreStub;

        #[async_trait]
        impl RateLimitStore for RedisRateLimitStoreStub {
            fn is_distributed_ha(&self) -> bool {
                true
            }

            async fn check_and_record(
                &self,
                _key: &str,
                _max_requests: u32,
                _window: std::time::Duration,
            ) -> Result<(), WebFrameworkError> {
                Ok(())
            }
        }

        #[derive(Clone)]
        struct RedisIdempotencyStoreStub;

        #[async_trait]
        impl IdempotencyStore for RedisIdempotencyStoreStub {
            fn is_distributed_ha(&self) -> bool {
                true
            }

            async fn begin(
                &self,
                _key: &str,
                _fingerprint: &str,
                _ttl: std::time::Duration,
            ) -> Result<crate::idempotency::IdempotencyBeginOutcome, WebFrameworkError>
            {
                Ok(crate::idempotency::IdempotencyBeginOutcome::Leader)
            }

            async fn complete(
                &self,
                _key: &str,
                _fingerprint: &str,
                _record: crate::idempotency::IdempotencyResponseRecord,
                _ttl: std::time::Duration,
            ) -> Result<(), WebFrameworkError> {
                Ok(())
            }

            async fn release(
                &self,
                _key: &str,
                _fingerprint: &str,
            ) -> Result<(), WebFrameworkError> {
                Ok(())
            }
        }

        #[derive(Clone)]
        struct RedisConcurrentAdmissionStoreStub;

        #[async_trait]
        impl ConcurrentAdmissionStore for RedisConcurrentAdmissionStoreStub {
            fn is_distributed_ha(&self) -> bool {
                true
            }

            async fn try_acquire(&self, _key: &str, _limit: u32) -> Result<(), WebFrameworkError> {
                Ok(())
            }

            async fn release(&self, _key: &str) -> Result<(), WebFrameworkError> {
                Ok(())
            }
        }

        let rate_limit = Arc::new(RedisRateLimitStoreStub) as Arc<dyn RateLimitStore>;
        let idempotency = Arc::new(RedisIdempotencyStoreStub) as Arc<dyn IdempotencyStore>;
        let concurrent =
            Arc::new(RedisConcurrentAdmissionStoreStub) as Arc<dyn ConcurrentAdmissionStore>;

        let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
        let resolver = tenant_bound_saas_verifying_web_request_resolver(
            lookup,
            NoOpJwtSessionRevocationChecker,
            StubApiKeyLookup,
        );
        let security = SecurityPolicy::production();
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: false,
            has_readiness_probe: true,
            rate_limit_store: Some(&rate_limit),
            idempotency_store: Some(&idempotency),
            concurrent_admission_store: Some(&concurrent),
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        let error = validate_production_assembly(input).expect_err("empty iss/aud claim policy");
        assert!(
            error.contains("tenant_bound_saas_verifying_web_request_resolver_with_claim_policy")
                || error.contains("saas_production"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn production_saas_profile_rejects_sqlx_rate_limit_store() {
        use crate::jwt_claims::JwtProductionClaimPolicy;
        use crate::jwt_tenant::{
            EnvBootstrapTenantSigningKeyLookup, NoOpJwtSessionRevocationChecker,
        };
        use crate::resolvers::ApiKeyLookupService;
        use crate::tenant_bound_saas_verifying_web_request_resolver_with_claim_policy;
        use async_trait::async_trait;

        #[derive(Clone)]
        struct StubApiKeyLookup;

        #[async_trait]
        impl ApiKeyLookupService for StubApiKeyLookup {
            async fn lookup_api_key(
                &self,
                _credential: &crate::parsers::ApiKeyCredential,
            ) -> Result<crate::resolvers::ApiKeyPrincipalRecord, WebFrameworkError> {
                Err(WebFrameworkError::dependency_unavailable("stub"))
            }
        }

        #[derive(Clone)]
        struct SqlxRateLimitStoreStub;

        #[async_trait]
        impl RateLimitStore for SqlxRateLimitStoreStub {
            async fn check_and_record(
                &self,
                _key: &str,
                _max_requests: u32,
                _window: std::time::Duration,
            ) -> Result<(), WebFrameworkError> {
                Ok(())
            }
        }

        #[derive(Clone)]
        struct RedisIdempotencyStoreStub;

        #[async_trait]
        impl IdempotencyStore for RedisIdempotencyStoreStub {
            fn is_distributed_ha(&self) -> bool {
                true
            }

            async fn begin(
                &self,
                _key: &str,
                _fingerprint: &str,
                _ttl: std::time::Duration,
            ) -> Result<crate::idempotency::IdempotencyBeginOutcome, WebFrameworkError>
            {
                Ok(crate::idempotency::IdempotencyBeginOutcome::Leader)
            }

            async fn complete(
                &self,
                _key: &str,
                _fingerprint: &str,
                _record: crate::idempotency::IdempotencyResponseRecord,
                _ttl: std::time::Duration,
            ) -> Result<(), WebFrameworkError> {
                Ok(())
            }

            async fn release(
                &self,
                _key: &str,
                _fingerprint: &str,
            ) -> Result<(), WebFrameworkError> {
                Ok(())
            }
        }

        #[derive(Clone)]
        struct RedisConcurrentAdmissionStoreStub;

        #[async_trait]
        impl ConcurrentAdmissionStore for RedisConcurrentAdmissionStoreStub {
            fn is_distributed_ha(&self) -> bool {
                true
            }

            async fn try_acquire(&self, _key: &str, _limit: u32) -> Result<(), WebFrameworkError> {
                Ok(())
            }

            async fn release(&self, _key: &str) -> Result<(), WebFrameworkError> {
                Ok(())
            }
        }

        let rate_limit = Arc::new(SqlxRateLimitStoreStub) as Arc<dyn RateLimitStore>;
        let idempotency = Arc::new(RedisIdempotencyStoreStub) as Arc<dyn IdempotencyStore>;
        let concurrent =
            Arc::new(RedisConcurrentAdmissionStoreStub) as Arc<dyn ConcurrentAdmissionStore>;

        let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
        let resolver = tenant_bound_saas_verifying_web_request_resolver_with_claim_policy(
            lookup,
            NoOpJwtSessionRevocationChecker,
            StubApiKeyLookup,
            JwtProductionClaimPolicy::saas_production(
                vec!["https://iam.sdkwork.dev".to_owned()],
                vec!["sdkwork-api".to_owned()],
            ),
        );
        let security = SecurityPolicy::production();
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: false,
            has_readiness_probe: true,
            rate_limit_store: Some(&rate_limit),
            idempotency_store: Some(&idempotency),
            concurrent_admission_store: Some(&concurrent),
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        let error = validate_production_assembly(input).expect_err("sqlx rate limit");
        assert!(error.contains("RedisRateLimitStore"));
    }

    #[test]
    fn production_profile_rejects_missing_audit_emitter() {
        let resolver = DefaultWebRequestContextResolver::default();
        let security = SecurityPolicy::production();
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: false,
            has_readiness_probe: false,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: None,
            security_event_emitter: Some(&production_emitters().1),
        };
        let error = validate_production_assembly(input).expect_err("missing audit emitter");
        assert!(error.contains("AuditEmitter"));
    }

    #[test]
    fn production_profile_rejects_unsafe_cors_policy() {
        use crate::jwt_tenant::EnvBootstrapTenantSigningKeyLookup;
        use crate::tenant_bound_verifying_web_request_resolver;

        let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
        let resolver =
            tenant_bound_verifying_web_request_resolver(lookup, crate::DefaultApiKeyLookupService);
        let mut security = SecurityPolicy::production();
        security.cors.allow_all_origins = true;
        security.cors.allow_credentials = true;
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: true,
            has_readiness_probe: false,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        let error = validate_production_assembly(input).expect_err("unsafe cors");
        assert!(error.contains("allow_all_origins"));
    }

    #[test]
    fn production_profile_rejects_allow_all_origins_even_without_credentials() {
        use crate::jwt_tenant::EnvBootstrapTenantSigningKeyLookup;
        use crate::tenant_bound_verifying_web_request_resolver;

        let lookup = EnvBootstrapTenantSigningKeyLookup::new("100001", "kid-1", b"secret");
        let resolver =
            tenant_bound_verifying_web_request_resolver(lookup, crate::DefaultApiKeyLookupService);
        let mut security = SecurityPolicy::production();
        security.cors.allow_all_origins = true;
        security.cors.allow_credentials = false;
        let authorization =
            Some(Arc::new(DenyAllAuthorizationPolicy) as Arc<dyn AuthorizationPolicy>);
        let tenant_isolation =
            Some(Arc::new(FixedTenantIsolation) as Arc<dyn TenantIsolationPolicy>);
        let input = ProductionAssemblyInput {
            environment: WebEnvironment::Prod,
            production_security_defaults: true,
            security_policy: &security,
            authorization: &authorization,
            tenant_isolation: &tenant_isolation,
            resolver: &resolver,
            control_plane_standalone: true,
            has_readiness_probe: false,
            rate_limit_store: None,
            idempotency_store: None,
            concurrent_admission_store: None,
            audit_emitter: Some(&production_emitters().0),
            security_event_emitter: Some(&production_emitters().1),
        };
        let error = validate_production_assembly(input).expect_err("unsafe cors");
        assert!(error.contains("allow_all_origins"));
    }

    #[test]
    fn deployment_environment_prod_requires_production_profile() {
        let previous = std::env::var("SDKWORK_WEB_FRAMEWORK_ENV").ok();
        std::env::set_var("SDKWORK_WEB_FRAMEWORK_ENV", "prod");
        let error = validate_deployment_environment(WebEnvironment::Dev, false)
            .expect_err("dev profile with prod env");
        assert!(error.contains("production_defaults"));
        match previous {
            Some(value) => std::env::set_var("SDKWORK_WEB_FRAMEWORK_ENV", value),
            None => std::env::remove_var("SDKWORK_WEB_FRAMEWORK_ENV"),
        }
    }
}
