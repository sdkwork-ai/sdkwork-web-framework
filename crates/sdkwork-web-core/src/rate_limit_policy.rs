//! Dynamic rate-limit policy resolution (extension point EP-10).

use crate::error::WebFrameworkError;
use crate::rate_limit::ResolvedRateLimitPolicy;
use crate::request_context::{WebApiSurface, WebEnvironment};
use async_trait::async_trait;
use sdkwork_web_contract::RateLimitTier;

/// Inputs available when resolving tenant-scoped rate limits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitPolicyContext {
    pub tenant_id: Option<String>,
    pub environment: WebEnvironment,
    pub api_surface: WebApiSurface,
    pub rate_limit_tier: Option<RateLimitTier>,
    pub operation_id: Option<String>,
}

impl RateLimitPolicyContext {
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

    pub fn tier_key(&self) -> &'static str {
        rate_limit_tier_key(self.rate_limit_tier)
    }
}

pub fn rate_limit_tier_key(tier: Option<RateLimitTier>) -> &'static str {
    match tier {
        Some(RateLimitTier::AuthCritical) => "auth_critical",
        Some(RateLimitTier::OpenApiDefault) => "open_api_default",
        Some(RateLimitTier::Upload) => "upload",
        Some(RateLimitTier::Search) => "search",
        Some(RateLimitTier::Bulk) => "bulk",
        Some(RateLimitTier::Worker) => "worker",
        Some(RateLimitTier::Internal) => "internal",
        None => "default",
    }
}

/// Optional overlay on top of [`DefaultRateLimitPolicyResolver`] (EP-10).
#[async_trait]
pub trait DynamicRateLimitPolicySource: Send + Sync {
    async fn resolve(
        &self,
        ctx: &RateLimitPolicyContext,
    ) -> Result<Option<ResolvedRateLimitPolicy>, WebFrameworkError>;
}

/// No-op dynamic source — static resolver + manifest tiers only.
#[derive(Clone, Debug, Default)]
pub struct NoOpDynamicRateLimitPolicySource;

#[async_trait]
impl DynamicRateLimitPolicySource for NoOpDynamicRateLimitPolicySource {
    async fn resolve(
        &self,
        _ctx: &RateLimitPolicyContext,
    ) -> Result<Option<ResolvedRateLimitPolicy>, WebFrameworkError> {
        Ok(None)
    }
}
