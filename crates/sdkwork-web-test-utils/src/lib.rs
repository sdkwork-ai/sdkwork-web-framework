//! In-memory test runtime and principal fixtures (catalog K2 / deployment profile `test`).

pub mod deployment_env;
pub mod jwt;

pub use deployment_env::IsolatedDeploymentEnv;

use async_trait::async_trait;
use sdkwork_web_axum::WebFrameworkLayer;
use sdkwork_web_core::{
    memory_idempotency_store, memory_rate_limit_store, AuditEmitter, AuditFact,
    DefaultWebRequestContextResolver, SecurityEvent, SecurityEventEmitter, WebCallRuntime,
    WebFrameworkError, WebRequestContextProfile, WebRequestContextResolver,
};
use std::sync::Arc;

/// Fluent builder for an in-memory [`WebCallRuntime`] used in integration tests.
pub struct TestRuntimeBuilder<R>
where
    R: WebRequestContextResolver + Clone,
{
    resolver: R,
    profile: WebRequestContextProfile,
}

impl<R> TestRuntimeBuilder<R>
where
    R: WebRequestContextResolver + Clone,
{
    pub fn new(resolver: R) -> Self {
        Self {
            resolver,
            profile: WebRequestContextProfile::default(),
        }
    }

    pub fn profile(mut self, profile: WebRequestContextProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn with_public_paths(mut self, prefixes: Vec<String>) -> Self {
        self.profile.public_path_prefixes = prefixes;
        self
    }

    pub fn build_runtime(self) -> WebCallRuntime<R> {
        WebCallRuntime::new(self.resolver)
            .with_profile(self.profile)
            .with_rate_limit_store(memory_rate_limit_store())
            .with_idempotency_store(memory_idempotency_store())
    }

    pub fn build_layer(self) -> WebFrameworkLayer<R> {
        let runtime = self.build_runtime();
        WebFrameworkLayer::new(runtime.resolver.clone())
            .with_profile(runtime.profile.clone())
            .with_security_policy(runtime.security_policy.clone())
            .with_rate_limit_store(runtime.rate_limit_store.clone())
            .with_idempotency_store(runtime.idempotency_store.clone())
    }
}

/// Default dev resolver with memory stores — suitable for handler/pipeline tests.
pub fn test_runtime() -> WebCallRuntime<DefaultWebRequestContextResolver> {
    TestRuntimeBuilder::new(DefaultWebRequestContextResolver::default()).build_runtime()
}

/// Default [`WebFrameworkLayer`] for axum integration tests.
pub fn test_layer() -> WebFrameworkLayer<DefaultWebRequestContextResolver> {
    TestRuntimeBuilder::new(DefaultWebRequestContextResolver::default()).build_layer()
}

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

/// Non-NoOp audit emitter for production assembly tests.
pub fn production_test_audit_emitter() -> Arc<dyn AuditEmitter> {
    Arc::new(TestAuditEmitter)
}

/// Non-NoOp security-event emitter for production assembly tests.
pub fn production_test_security_event_emitter() -> Arc<dyn SecurityEventEmitter> {
    Arc::new(TestSecurityEventEmitter)
}

/// JWT fixture tokens for [`DefaultWebRequestContextResolver`] integration tests.
pub mod fixtures {
    use crate::jwt::{
        access_token_jwt, auth_token_jwt_with_permissions, bootstrap_access_token_jwt,
    };

    pub fn auth_token_tenant_admin() -> String {
        auth_token_jwt_with_permissions(
            "100001",
            "user-test",
            "session-test",
            "appbase",
            "web-framework.tenant.admin",
        )
    }

    pub fn auth_token_control_plane() -> String {
        auth_token_jwt_with_permissions(
            "100001",
            "user-test",
            "session-test",
            "appbase",
            "web-framework.control-plane",
        )
    }

    pub fn auth_token_platform_read() -> String {
        auth_token_jwt_with_permissions(
            "100001",
            "user-test",
            "session-test",
            "appbase",
            "web-framework.tenant.admin,web-framework.platform.read",
        )
    }

    /// Default tenant-admin token for protected tenant-scoped admin routes.
    pub fn auth_token() -> String {
        auth_token_tenant_admin()
    }

    pub fn access_token() -> String {
        access_token_jwt("100001", "user-test", "session-test", "appbase")
    }

    pub fn bootstrap_access_token() -> String {
        bootstrap_access_token_jwt("100001", "app_tenant-bootstrap")
    }

    pub fn api_key() -> &'static str {
        "api_key_id=key-test;tenant_id=100001;organization_id=0;user_id=user-test;app_id=appbase;environment=prod;deployment_mode=saas;data_scope=tenant;permission_scope=iam.read"
    }

    pub fn app_api_path() -> &'static str {
        "/app/v3/api/users"
    }

    pub fn expected_environment() -> sdkwork_web_core::WebEnvironment {
        sdkwork_web_core::WebEnvironment::Prod
    }

    pub fn expected_deployment() -> sdkwork_web_core::WebDeploymentMode {
        sdkwork_web_core::WebDeploymentMode::Saas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_uses_memory_stores() {
        let runtime = test_runtime();
        assert!(!runtime.security_policy.rate_limit.enabled);
        assert_eq!(
            sdkwork_web_core::WebEnvironment::Prod,
            fixtures::expected_environment()
        );
    }

    #[test]
    fn test_layer_builds_without_panic() {
        let _layer = test_layer();
    }
}
