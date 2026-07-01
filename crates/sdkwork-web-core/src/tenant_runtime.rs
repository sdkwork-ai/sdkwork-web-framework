//! Per-tenant runtime profile overlays (admin table `web_tenant_runtime_profile`).

use crate::error::WebFrameworkError;
use crate::request_context::{WebApiSurface, WebEnvironment};
use async_trait::async_trait;

/// Tenant-scoped overrides for framework runtime knobs.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct TenantRuntimeProfile {
    pub rate_limit_enabled: Option<bool>,
    pub max_content_length: Option<u64>,
    /// Per-tenant in-flight request cap (catalog D9). `None` = no overlay.
    pub max_concurrent_requests: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TenantRuntimeProfileContext {
    pub tenant_id: Option<String>,
    pub environment: WebEnvironment,
    pub api_surface: WebApiSurface,
}

impl TenantRuntimeProfileContext {
    pub fn tenant_scope(&self) -> &str {
        self.tenant_id
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("0")
    }

    pub fn environment_label(&self) -> &'static str {
        match self.environment {
            WebEnvironment::Prod => "prod",
            WebEnvironment::Test => "test",
            WebEnvironment::Dev => "dev",
        }
    }
}

#[async_trait]
pub trait DynamicTenantRuntimeProfileSource: Send + Sync {
    async fn resolve(
        &self,
        ctx: &TenantRuntimeProfileContext,
    ) -> Result<Option<TenantRuntimeProfile>, WebFrameworkError>;
}

#[derive(Clone, Debug, Default)]
pub struct NoOpDynamicTenantRuntimeProfileSource;

#[async_trait]
impl DynamicTenantRuntimeProfileSource for NoOpDynamicTenantRuntimeProfileSource {
    async fn resolve(
        &self,
        _ctx: &TenantRuntimeProfileContext,
    ) -> Result<Option<TenantRuntimeProfile>, WebFrameworkError> {
        Ok(None)
    }
}
