//! TTL caches for dynamic policy resolution (PERFORMANCE_SPEC / CACHE_SPEC local overlay).

use crate::cors_policy::{CorsPolicyContext, DynamicCorsPolicySource};
use crate::error::WebFrameworkError;
use crate::rate_limit::ResolvedRateLimitPolicy;
use crate::rate_limit_policy::{DynamicRateLimitPolicySource, RateLimitPolicyContext};
use crate::security::CorsPolicy;
use crate::tenant_runtime::{
    DynamicTenantRuntimeProfileSource, TenantRuntimeProfile, TenantRuntimeProfileContext,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct CacheEntry<T> {
    value: T,
    expires_at: Instant,
}

/// In-process TTL cache for derived policy overlays (single-replica / dev SQLx profile).
#[derive(Debug)]
pub struct TtlCache<T: Clone> {
    ttl: Duration,
    entries: Mutex<HashMap<String, CacheEntry<T>>>,
}

impl<T: Clone> TtlCache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_valid(&self, key: &str) -> Option<T> {
        let mut guard = self.entries.lock().expect("policy cache lock");
        let now = Instant::now();
        let is_valid = guard
            .get(key)
            .map(|entry| entry.expires_at > now)
            .unwrap_or(false);
        if is_valid {
            return guard.get(key).map(|entry| entry.value.clone());
        }
        guard.remove(key);
        None
    }

    pub fn insert(&self, key: impl Into<String>, value: T) {
        let mut guard = self.entries.lock().expect("policy cache lock");
        guard.insert(
            key.into(),
            CacheEntry {
                value,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    pub fn invalidate_prefix(&self, prefix: &str) {
        let mut guard = self.entries.lock().expect("policy cache lock");
        guard.retain(|key, _| !key.starts_with(prefix));
    }
}

/// Shared invalidation handles for SQLx-backed dynamic policy overlays.
#[derive(Debug)]
pub struct DynamicPolicyCaches {
    cors: Arc<TtlCache<Option<CorsPolicy>>>,
    rate_limit: Arc<TtlCache<Option<ResolvedRateLimitPolicy>>>,
    tenant_profile: Arc<TtlCache<Option<TenantRuntimeProfile>>>,
}

impl DynamicPolicyCaches {
    pub fn new(ttl: Duration) -> Self {
        Self {
            cors: Arc::new(TtlCache::new(ttl)),
            rate_limit: Arc::new(TtlCache::new(ttl)),
            tenant_profile: Arc::new(TtlCache::new(ttl)),
        }
    }

    pub fn cors(&self) -> Arc<TtlCache<Option<CorsPolicy>>> {
        self.cors.clone()
    }

    pub fn rate_limit(&self) -> Arc<TtlCache<Option<ResolvedRateLimitPolicy>>> {
        self.rate_limit.clone()
    }

    pub fn tenant_profile(&self) -> Arc<TtlCache<Option<TenantRuntimeProfile>>> {
        self.tenant_profile.clone()
    }

    pub fn invalidate_tenant_environment(&self, tenant_id: &str, environment: &str) {
        let prefix = format!("{tenant_id}|{environment}|");
        self.cors.invalidate_prefix(&prefix);
        self.rate_limit.invalidate_prefix(&prefix);
        self.tenant_profile.invalidate_prefix(&prefix);
    }
}

pub struct CachingDynamicCorsPolicySource {
    inner: Arc<dyn DynamicCorsPolicySource>,
    cache: Arc<TtlCache<Option<CorsPolicy>>>,
}

impl CachingDynamicCorsPolicySource {
    pub fn new(
        inner: Arc<dyn DynamicCorsPolicySource>,
        cache: Arc<TtlCache<Option<CorsPolicy>>>,
    ) -> Self {
        Self { inner, cache }
    }
}

#[async_trait]
impl DynamicCorsPolicySource for CachingDynamicCorsPolicySource {
    async fn resolve(
        &self,
        ctx: &CorsPolicyContext,
    ) -> Result<Option<CorsPolicy>, WebFrameworkError> {
        let key = format!(
            "{}|{}|{:?}",
            ctx.tenant_scope(),
            ctx.environment_label(),
            ctx.api_surface
        );
        if let Some(cached) = self.cache.get_valid(&key) {
            return Ok(cached);
        }
        let resolved = self.inner.resolve(ctx).await?;
        self.cache.insert(key, resolved.clone());
        Ok(resolved)
    }
}

pub struct CachingDynamicRateLimitPolicySource {
    inner: Arc<dyn DynamicRateLimitPolicySource>,
    cache: Arc<TtlCache<Option<ResolvedRateLimitPolicy>>>,
}

impl CachingDynamicRateLimitPolicySource {
    pub fn new(
        inner: Arc<dyn DynamicRateLimitPolicySource>,
        cache: Arc<TtlCache<Option<ResolvedRateLimitPolicy>>>,
    ) -> Self {
        Self { inner, cache }
    }
}

#[async_trait]
impl DynamicRateLimitPolicySource for CachingDynamicRateLimitPolicySource {
    async fn resolve(
        &self,
        ctx: &RateLimitPolicyContext,
    ) -> Result<Option<ResolvedRateLimitPolicy>, WebFrameworkError> {
        let key = format!(
            "{}|{}|{}|{:?}",
            ctx.tenant_scope(),
            ctx.environment_label(),
            ctx.tier_key(),
            ctx.api_surface
        );
        if let Some(cached) = self.cache.get_valid(&key) {
            return Ok(cached);
        }
        let resolved = self.inner.resolve(ctx).await?;
        self.cache.insert(key, resolved);
        Ok(resolved)
    }
}

pub struct CachingDynamicTenantRuntimeProfileSource {
    inner: Arc<dyn DynamicTenantRuntimeProfileSource>,
    cache: Arc<TtlCache<Option<TenantRuntimeProfile>>>,
}

impl CachingDynamicTenantRuntimeProfileSource {
    pub fn new(
        inner: Arc<dyn DynamicTenantRuntimeProfileSource>,
        cache: Arc<TtlCache<Option<TenantRuntimeProfile>>>,
    ) -> Self {
        Self { inner, cache }
    }
}

#[async_trait]
impl DynamicTenantRuntimeProfileSource for CachingDynamicTenantRuntimeProfileSource {
    async fn resolve(
        &self,
        ctx: &TenantRuntimeProfileContext,
    ) -> Result<Option<TenantRuntimeProfile>, WebFrameworkError> {
        let key = format!(
            "{}|{}|{:?}",
            ctx.tenant_scope(),
            ctx.environment_label(),
            ctx.api_surface
        );
        if let Some(cached) = self.cache.get_valid(&key) {
            return Ok(cached);
        }
        let resolved = self.inner.resolve(ctx).await?;
        self.cache.insert(key, resolved.clone());
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_context::{WebApiSurface, WebEnvironment};

    #[test]
    fn ttl_cache_expires_entries() {
        let cache = TtlCache::new(Duration::from_millis(1));
        cache.insert("k", 42);
        assert_eq!(Some(42), cache.get_valid("k"));
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(None, cache.get_valid("k"));
    }

    #[test]
    fn dynamic_policy_caches_invalidate_tenant_environment_prefix() {
        let caches = DynamicPolicyCaches::new(Duration::from_secs(60));
        caches
            .cors()
            .insert("100001|prod|AppApi", Some(CorsPolicy::default()));
        caches.cors().insert("100002|prod|AppApi", None);
        caches.invalidate_tenant_environment("100001", "prod");
        assert_eq!(None, caches.cors().get_valid("100001|prod|AppApi"));
        assert!(caches.cors().get_valid("100002|prod|AppApi").is_some());
    }

    #[tokio::test]
    async fn caching_cors_source_hits_inner_once_per_key() {
        use crate::cors_policy::NoOpDynamicCorsPolicySource;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingSource {
            calls: AtomicUsize,
        }

        #[async_trait]
        impl DynamicCorsPolicySource for CountingSource {
            async fn resolve(
                &self,
                _ctx: &CorsPolicyContext,
            ) -> Result<Option<CorsPolicy>, WebFrameworkError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            }
        }

        let cache = Arc::new(TtlCache::new(Duration::from_secs(60)));
        let inner = Arc::new(CountingSource {
            calls: AtomicUsize::new(0),
        });
        let source = CachingDynamicCorsPolicySource::new(inner.clone(), cache);
        let ctx = CorsPolicyContext {
            tenant_id: Some("100001".to_owned()),
            environment: WebEnvironment::Prod,
            api_surface: WebApiSurface::AppApi,
            origin: None,
        };
        source.resolve(&ctx).await.expect("first");
        source.resolve(&ctx).await.expect("second");
        assert_eq!(1, inner.calls.load(Ordering::SeqCst));
        let _ = NoOpDynamicCorsPolicySource;
    }
}
