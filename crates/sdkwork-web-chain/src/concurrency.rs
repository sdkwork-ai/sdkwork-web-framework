//! Concurrency guard stage (bulkhead pattern).
//!
//! [`ConcurrencyStage`] limits the number of in-flight requests per scope
//! (global, per API key, per tenant). It delegates counting to a
//! [`ConcurrentAdmissionStore`] so the same stage works single-node (memory
//! store) and multi-replica (distributed store with leases/TTL).
//!
//! Lifecycle: `before` acquires a lease for every enabled scope; `after` /
//! `on_error` release them in reverse order. When any scope rejects, the
//! already-acquired leases are released before returning the rejection, so
//! the chain never leaks a slot. Rejection carries HTTP 429 semantics with
//! `Retry-After`.

use crate::chain::{ChainContext, ChainStage, ChainVerdict, RejectReason};
use crate::policy::ConcurrencyScope;
use async_trait::async_trait;
use sdkwork_web_core::WebFrameworkError;
use std::sync::Arc;

/// Leases acquired by [`ConcurrencyStage`], stored in the chain context so
/// `after` / `on_error` can release exactly what `before` acquired.
#[derive(Debug, Clone, Default)]
pub struct ConcurrencyLeaseState {
    /// Store keys of acquired leases, in acquisition order.
    pub scope_keys: Vec<String>,
}

/// Bulkhead stage enforcing per-scope in-flight limits from the resolved
/// [`crate::policy::ConcurrencyPolicy`].
pub struct ConcurrencyStage {
    store: Arc<dyn sdkwork_web_core::ConcurrentAdmissionStore>,
    scopes: Vec<ConcurrencyScope>,
    /// Default `Retry-After` (seconds) when the store error carries none.
    retry_after_secs: u64,
}

impl ConcurrencyStage {
    /// Default scope set: platform global budget plus per-API-key budget.
    pub const DEFAULT_SCOPES: [ConcurrencyScope; 2] =
        [ConcurrencyScope::Global, ConcurrencyScope::ApiKey];

    pub fn new(store: Arc<dyn sdkwork_web_core::ConcurrentAdmissionStore>) -> Self {
        Self {
            store,
            scopes: Self::DEFAULT_SCOPES.to_vec(),
            retry_after_secs: 1,
        }
    }

    /// Replaces the enforced scope set (free composition of budgets).
    pub fn with_scopes(mut self, scopes: impl IntoIterator<Item = ConcurrencyScope>) -> Self {
        self.scopes = scopes.into_iter().collect();
        self
    }

    pub fn with_retry_after_secs(mut self, retry_after_secs: u64) -> Self {
        self.retry_after_secs = retry_after_secs;
        self
    }

    /// Stable store key for a scope, matching the web-core admission key
    /// style (`tenant:{id}:concurrent`, `cred:{id}:concurrent`).
    pub fn scope_key(&self, scope: ConcurrencyScope, ctx: &ChainContext) -> Option<String> {
        match scope {
            ConcurrencyScope::Global => Some("global:concurrent".to_owned()),
            ConcurrencyScope::ApiKey => ctx
                .scopes
                .api_key_id
                .map(|id| format!("api-key:{id}:concurrent")),
            ConcurrencyScope::Tenant => ctx
                .scopes
                .tenant_id
                .map(|id| format!("tenant:{id}:concurrent")),
        }
    }

    async fn release_keys(&self, keys: &[String]) {
        for key in keys.iter().rev() {
            if let Err(error) = self.store.release(key).await {
                tracing::warn!(
                    scope_key = %key,
                    error = ?error,
                    "failed to release concurrency lease"
                );
            }
        }
    }
}

#[async_trait]
impl ChainStage for ConcurrencyStage {
    fn name(&self) -> &'static str {
        "concurrency"
    }

    fn stage_order(&self) -> u32 {
        200
    }

    async fn before(&self, ctx: &mut ChainContext) -> Result<ChainVerdict, WebFrameworkError> {
        let Some(policy) = ctx.policy.concurrency.clone() else {
            // No concurrency policy configured: stage passes and holds nothing.
            ctx.insert_state(ConcurrencyLeaseState::default());
            return Ok(ChainVerdict::Pass);
        };
        let mut acquired = Vec::new();
        for scope in &self.scopes {
            let Some(limit) = policy.limit_for(*scope) else {
                continue;
            };
            let Some(key) = self.scope_key(*scope, ctx) else {
                continue;
            };
            if let Err(error) = self.store.try_acquire(&key, limit).await {
                if error.kind == sdkwork_web_core::WebFrameworkErrorKind::RateLimitExceeded {
                    // Genuine limit rejection: release partially acquired
                    // slots and reject with 429 semantics.
                    self.release_keys(&acquired).await;
                    return Ok(ChainVerdict::Reject(RejectReason::concurrency_exceeded(
                        format!(
                            "concurrency limit exceeded for scope {}: {}",
                            scope.name(),
                            error.message
                        ),
                        error.retry_after_seconds.unwrap_or(self.retry_after_secs),
                    )));
                }
                // Store failure (e.g. Redis unavailable): degrade open with a
                // warning instead of turning an infrastructure blip into
                // client rejections, matching the gateway's local-fallback
                // rate-limit philosophy. The slot is simply not acquired.
                tracing::warn!(
                    scope = scope.name(),
                    scope_key = %key,
                    error = ?error,
                    "concurrency store unavailable; bypassing scope for this request"
                );
                continue;
            }
            acquired.push(key);
        }
        ctx.insert_state(ConcurrencyLeaseState {
            scope_keys: acquired,
        });
        Ok(ChainVerdict::Pass)
    }

    async fn after(&self, ctx: &mut ChainContext) -> Result<(), WebFrameworkError> {
        if let Some(leases) = ctx.take_state::<ConcurrencyLeaseState>() {
            self.release_keys(&leases.scope_keys).await;
        }
        Ok(())
    }

    async fn on_error(
        &self,
        ctx: &mut ChainContext,
        _error: &WebFrameworkError,
    ) -> Result<(), WebFrameworkError> {
        self.after(ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{CallChain, CallChainBuilder, ChainOutcome, ChainScopes};
    use crate::policy::{ChainPolicy, ConcurrencyPolicy, PolicyResolver, ResolvedChainPolicy};
    use sdkwork_web_core::{memory_concurrent_admission_store, ConcurrentAdmissionStore};

    struct ResolvePolicy(ChainPolicy);

    #[async_trait]
    impl PolicyResolver for ResolvePolicy {
        async fn resolve(&self, _scopes: &ChainScopes) -> ResolvedChainPolicy {
            ResolvedChainPolicy::from(&self.0)
        }
    }

    fn concurrency_policy(max: u32) -> ChainPolicy {
        ChainPolicy {
            concurrency: Some(ConcurrencyPolicy {
                max_inflight: Some(max),
                max_inflight_per_scope: None,
            }),
            ..ChainPolicy::default()
        }
    }

    fn scopes_with_key(api_key_id: Option<i64>) -> ChainScopes {
        ChainScopes {
            tenant_id: Some(1),
            organization_id: Some(2),
            api_key_id,
        }
    }

    /// Builds a chain whose stage shares `store`, so multiple chains observe
    /// the same budgets like multiple gateway nodes would.
    async fn chain_with(
        policy: ChainPolicy,
        store: Arc<dyn ConcurrentAdmissionStore>,
        scopes: Option<Vec<ConcurrencyScope>>,
    ) -> CallChain {
        let mut stage = ConcurrencyStage::new(store);
        if let Some(scopes) = scopes {
            stage = stage.with_scopes(scopes);
        }
        CallChainBuilder::new()
            .with_stage(Arc::new(stage))
            .with_policy_resolver(Arc::new(ResolvePolicy(policy)))
            .build()
            .expect("chain")
    }

    #[tokio::test]
    async fn enforces_global_limit_and_releases_on_after() {
        let store = memory_concurrent_admission_store();
        let chain_a = chain_with(concurrency_policy(1), store.clone(), None).await;
        let ctx_a = chain_a
            .before(None, &scopes_with_key(Some(42)))
            .await
            .expect("first request acquired");
        // Second request (any key) hits the global budget of 1.
        let chain_b = chain_with(concurrency_policy(1), store.clone(), None).await;
        let outcome = chain_b
            .before(None, &scopes_with_key(Some(43)))
            .await
            .expect_err("second request rejected");
        match outcome {
            ChainOutcome::Rejected(reason) => {
                assert_eq!(reason.http_status, 429);
                assert!(reason.retry_after_secs.is_some());
            }
            ChainOutcome::Failed(_) => panic!("expected rejection"),
        }
        // After release the slot is free again.
        let mut ctx_a = ctx_a;
        chain_a.after(&mut ctx_a).await.expect("release");
        let chain_c = chain_with(concurrency_policy(1), store.clone(), None).await;
        let ctx_c = chain_c
            .before(None, &scopes_with_key(Some(44)))
            .await
            .expect("slot freed after release");
        let mut ctx_c = ctx_c;
        chain_c.after(&mut ctx_c).await.expect("release");
    }

    #[tokio::test]
    async fn per_api_key_budget_is_independent_of_global() {
        let store = memory_concurrent_admission_store();
        let policy = ChainPolicy {
            concurrency: Some(ConcurrencyPolicy {
                max_inflight: Some(10),
                max_inflight_per_scope: Some([("apiKey".to_owned(), 1_u32)].into_iter().collect()),
            }),
            ..ChainPolicy::default()
        };
        let chain_a = chain_with(policy.clone(), store.clone(), None).await;
        chain_a
            .before(None, &scopes_with_key(Some(1)))
            .await
            .expect("key 1 acquired");
        // A different key is unaffected by key 1's per-key budget.
        let chain_b = chain_with(policy.clone(), store.clone(), None).await;
        let ctx_b = chain_b
            .before(None, &scopes_with_key(Some(2)))
            .await
            .expect("key 2 acquired");
        // The same key again exceeds its per-key budget of 1.
        let chain_c = chain_with(policy.clone(), store.clone(), None).await;
        let outcome = chain_c
            .before(None, &scopes_with_key(Some(1)))
            .await
            .expect_err("key 1 budget full");
        assert!(matches!(outcome, ChainOutcome::Rejected(_)));
        let mut ctx_b = ctx_b;
        chain_b.after(&mut ctx_b).await.expect("release");
    }

    #[tokio::test]
    async fn rejection_releases_partially_acquired_scopes() {
        let store = memory_concurrent_admission_store();
        let policy = ChainPolicy {
            concurrency: Some(ConcurrencyPolicy {
                max_inflight: Some(10),
                max_inflight_per_scope: Some([("apiKey".to_owned(), 1_u32)].into_iter().collect()),
            }),
            ..ChainPolicy::default()
        };
        let chain_a = chain_with(policy.clone(), store.clone(), None).await;
        let ctx_a = chain_a
            .before(None, &scopes_with_key(Some(1)))
            .await
            .expect("key 1 acquired (global + api-key:1)");
        // Global still has capacity but the per-key budget is full: the global
        // slot acquired by this request must be released on rejection.
        let chain_b = chain_with(policy.clone(), store.clone(), None).await;
        let outcome = chain_b
            .before(None, &scopes_with_key(Some(1)))
            .await
            .expect_err("per-key budget full");
        assert!(matches!(outcome, ChainOutcome::Rejected(_)));
        let mut ctx_a = ctx_a;
        chain_a.after(&mut ctx_a).await.expect("release");
        // After both releases the global budget is back and a new key passes.
        let chain_c = chain_with(policy.clone(), store.clone(), None).await;
        let ctx_c = chain_c
            .before(None, &scopes_with_key(Some(3)))
            .await
            .expect("global slot was released by the rejected request");
        let mut ctx_c = ctx_c;
        chain_c.after(&mut ctx_c).await.expect("release");
    }

    #[tokio::test]
    async fn no_policy_means_no_restriction() {
        let store = memory_concurrent_admission_store();
        let chain = chain_with(ChainPolicy::default(), store, None).await;
        let ctx = chain
            .before(None, &scopes_with_key(Some(1)))
            .await
            .expect("passes without policy");
        assert!(ctx.get_state::<ConcurrencyLeaseState>().is_some());
        let mut ctx = ctx;
        chain.after(&mut ctx).await.expect("after");
    }

    #[tokio::test]
    async fn store_infrastructure_failure_degrades_open() {
        use sdkwork_web_core::WebFrameworkError;

        struct BrokenStore;

        #[async_trait]
        impl ConcurrentAdmissionStore for BrokenStore {
            async fn try_acquire(&self, _key: &str, _limit: u32) -> Result<(), WebFrameworkError> {
                Err(WebFrameworkError::internal_server_error("redis down"))
            }
            async fn release(&self, _key: &str) -> Result<(), WebFrameworkError> {
                Ok(())
            }
        }

        // Infrastructure failure (Redis unavailable) must not turn into a
        // client-facing 429: the scope is bypassed with a warning instead,
        // matching the gateway's local-fallback rate-limit philosophy.
        let store: Arc<dyn ConcurrentAdmissionStore> = Arc::new(BrokenStore);
        let chain = chain_with(concurrency_policy(1), store.clone(), None).await;
        let ctx = chain
            .before(None, &scopes_with_key(Some(1)))
            .await
            .expect("store failure must not reject clients");
        let mut ctx = ctx;
        chain.after(&mut ctx).await.expect("release");
    }

    #[tokio::test]
    async fn genuine_limit_rejection_is_preserved() {
        use sdkwork_web_core::WebFrameworkError;

        struct AlwaysFullStore;

        #[async_trait]
        impl ConcurrentAdmissionStore for AlwaysFullStore {
            async fn try_acquire(&self, _key: &str, _limit: u32) -> Result<(), WebFrameworkError> {
                Err(WebFrameworkError::rate_limit_exceeded("full", 5))
            }
            async fn release(&self, _key: &str) -> Result<(), WebFrameworkError> {
                Ok(())
            }
        }

        let store: Arc<dyn ConcurrentAdmissionStore> = Arc::new(AlwaysFullStore);
        let chain = chain_with(concurrency_policy(1), store, None).await;
        let outcome = chain
            .before(None, &scopes_with_key(Some(1)))
            .await
            .expect_err("limit rejection preserved");
        match outcome {
            ChainOutcome::Rejected(reason) => {
                assert_eq!(reason.http_status, 429);
                assert_eq!(reason.retry_after_secs, Some(5));
            }
            ChainOutcome::Failed(_) => panic!("expected rejection"),
        }
    }

    #[tokio::test]
    async fn custom_scope_set_is_respected() {
        let store = memory_concurrent_admission_store();
        let chain = chain_with(
            concurrency_policy(1),
            store,
            Some(vec![ConcurrencyScope::Tenant]),
        )
        .await;
        // Only the tenant scope is enforced: two keys of the same tenant
        // compete, but the per-key scope never runs.
        let ctx_a = chain
            .before(None, &scopes_with_key(Some(1)))
            .await
            .expect("first tenant request");
        let outcome = chain
            .before(None, &scopes_with_key(Some(2)))
            .await
            .expect_err("tenant budget full");
        assert!(matches!(outcome, ChainOutcome::Rejected(_)));
        let mut ctx_a = ctx_a;
        chain.after(&mut ctx_a).await.expect("release");
    }
}
