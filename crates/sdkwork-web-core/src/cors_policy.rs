//! Dynamic CORS policy resolution (extension point EP-16).

use crate::error::WebFrameworkError;
use crate::request_context::{WebApiSurface, WebEnvironment};
use crate::security::CorsPolicy;
use async_trait::async_trait;

/// Inputs available at the Cors pipeline stage (before authentication).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorsPolicyContext {
    pub tenant_id: Option<String>,
    pub environment: WebEnvironment,
    pub api_surface: WebApiSurface,
    pub origin: Option<String>,
}

impl CorsPolicyContext {
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

/// Optional overlay on top of [`SecurityPolicy::cors`] (EP-16).
#[async_trait]
pub trait DynamicCorsPolicySource: Send + Sync {
    async fn resolve(
        &self,
        ctx: &CorsPolicyContext,
    ) -> Result<Option<CorsPolicy>, WebFrameworkError>;
}

/// No-op dynamic source — static [`SecurityPolicy::cors`] only.
#[derive(Clone, Debug, Default)]
pub struct NoOpDynamicCorsPolicySource;

#[async_trait]
impl DynamicCorsPolicySource for NoOpDynamicCorsPolicySource {
    async fn resolve(
        &self,
        _ctx: &CorsPolicyContext,
    ) -> Result<Option<CorsPolicy>, WebFrameworkError> {
        Ok(None)
    }
}
