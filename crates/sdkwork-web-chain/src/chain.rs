//! Composable call chain engine.
//!
//! A [`CallChain`] is an ordered set of [`ChainStage`] guards that a caller
//! runs around a protected operation (typically an open-API invocation).
//! Stages are free building blocks: any implementation of [`ChainStage`] can
//! be combined in any order, and each stage can be independently enabled or
//! disabled by the effective policy resolved per request.
//!
//! Execution semantics (mirroring the invocation pipeline used by gateway
//! hosts):
//!
//! - `before` runs stages in `(stage_order, name)` order; the first
//!   [`ChainVerdict::Reject`] short-circuits the chain.
//! - When a stage rejects, stages that already passed run `after` in reverse
//!   order so acquired resources (e.g. concurrency leases) are released.
//! - When a stage fails with an error, all started stages run `on_error` in
//!   reverse order and the failure is surfaced as [`ChainOutcome::Failed`].
//! - `after` (success path, e.g. stream EOF) runs enabled stages in reverse
//!   order; `on_error` (error path) likewise, and both are idempotent.

use crate::policy::{PolicyResolver, ResolvedChainPolicy, StageEnablement};
use sdkwork_web_core::WebFrameworkError;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Request identity and scope facts available to every stage.
#[derive(Debug, Clone, Default)]
pub struct ChainScopes {
    pub tenant_id: Option<i64>,
    pub organization_id: Option<i64>,
    pub api_key_id: Option<i64>,
}

/// Per-request context carried through the chain.
///
/// `stage_state` holds private stage state (e.g. acquired concurrency lease
/// keys) keyed by type; stages store and retrieve it without leaking state
/// across stages.
#[derive(Debug)]
pub struct ChainContext {
    pub client_ip: Option<std::net::IpAddr>,
    pub scopes: ChainScopes,
    pub policy: Arc<ResolvedChainPolicy>,
    stage_state: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl ChainContext {
    pub fn new(
        client_ip: Option<std::net::IpAddr>,
        scopes: ChainScopes,
        policy: Arc<ResolvedChainPolicy>,
    ) -> Self {
        Self {
            client_ip,
            scopes,
            policy,
            stage_state: HashMap::new(),
        }
    }

    /// Stores stage-private state, replacing any previous value of the same type.
    pub fn insert_state<T: Any + Send + Sync>(&mut self, value: T) {
        self.stage_state.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get_state<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.stage_state
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
    }

    /// Takes stage-private state out of the context (removes it).
    pub fn take_state<T: Any + Send + Sync>(&mut self) -> Option<T> {
        self.stage_state
            .remove(&TypeId::of::<T>())
            .and_then(|value| value.downcast::<T>().ok())
            .map(|value| *value)
    }
}

/// Classification of a chain rejection, mapped by callers to HTTP semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectKind {
    /// Client IP is denied by the access policy (HTTP 403).
    IpForbidden,
    /// Concurrency limit exceeded (HTTP 429 with `Retry-After`).
    ConcurrencyExceeded,
    /// Application-defined rejection with an explicit HTTP status.
    Custom,
}

/// Structured rejection produced by a stage.
#[derive(Debug, Clone)]
pub struct RejectReason {
    pub kind: RejectKind,
    pub message: String,
    pub http_status: u16,
    pub retry_after_secs: Option<u64>,
}

impl RejectReason {
    pub fn ip_forbidden(message: impl Into<String>) -> Self {
        Self {
            kind: RejectKind::IpForbidden,
            message: message.into(),
            http_status: 403,
            retry_after_secs: None,
        }
    }

    pub fn concurrency_exceeded(message: impl Into<String>, retry_after_secs: u64) -> Self {
        Self {
            kind: RejectKind::ConcurrencyExceeded,
            message: message.into(),
            http_status: 429,
            retry_after_secs: Some(retry_after_secs),
        }
    }

    pub fn custom(http_status: u16, message: impl Into<String>) -> Self {
        Self {
            kind: RejectKind::Custom,
            message: message.into(),
            http_status,
            retry_after_secs: None,
        }
    }
}

/// Verdict a stage returns from `before`.
#[derive(Debug, Clone)]
pub enum ChainVerdict {
    Pass,
    Reject(RejectReason),
}

/// Terminal outcome of a rejected or failed chain run.
#[derive(Debug)]
pub enum ChainOutcome {
    /// A stage rejected the call (business denial, not an error).
    Rejected(RejectReason),
    /// A stage failed to evaluate (operational error).
    Failed(WebFrameworkError),
}

impl ChainOutcome {
    /// The rejection reason when the chain rejected the call.
    pub fn as_reject_reason(&self) -> Option<&RejectReason> {
        match self {
            ChainOutcome::Rejected(reason) => Some(reason),
            ChainOutcome::Failed(_) => None,
        }
    }
}

/// A composable guard unit of the call chain.
///
/// Implementations are free building blocks: [`CallChainBuilder::with_stage`]
/// composes them, and [`ChainStage::enabled`] lets the effective per-scope
/// policy switch each stage independently.
#[async_trait::async_trait]
pub trait ChainStage: Send + Sync + 'static {
    /// Stable snake_case name used by [`StageEnablement`] to switch the stage.
    fn name(&self) -> &'static str;

    /// Execution order; lower runs first. Stages with equal order are
    /// additionally ordered by name for determinism.
    fn stage_order(&self) -> u32 {
        100
    }

    /// Whether the stage participates in this run. The default honors the
    /// effective [`StageEnablement`] (`enabled_only` whitelist, then
    /// `disabled` denylist; unlisted stages are enabled).
    fn enabled(&self, enablement: &StageEnablement) -> bool {
        enablement.is_enabled(self.name())
    }

    /// Guard check before the protected operation. Return
    /// [`ChainVerdict::Pass`] to continue or [`ChainVerdict::Reject`] to
    /// short-circuit with a structured reason. Errors indicate an operational
    /// failure of the stage itself.
    async fn before(&self, _ctx: &mut ChainContext) -> Result<ChainVerdict, WebFrameworkError> {
        Ok(ChainVerdict::Pass)
    }

    /// Release hook run after the protected operation completes successfully
    /// (for streaming callers: after EOF). Must be idempotent.
    async fn after(&self, _ctx: &mut ChainContext) -> Result<(), WebFrameworkError> {
        Ok(())
    }

    /// Release/observe hook run when the protected operation (or an earlier
    /// stage) failed. Must be idempotent.
    async fn on_error(
        &self,
        _ctx: &mut ChainContext,
        _error: &WebFrameworkError,
    ) -> Result<(), WebFrameworkError> {
        Ok(())
    }
}

/// Immutable, ordered call chain assembled by [`CallChainBuilder`].
pub struct CallChain {
    stages: Vec<Arc<dyn ChainStage>>,
    resolver: Arc<dyn PolicyResolver>,
}

impl std::fmt::Debug for CallChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stage_names: Vec<&str> = self.stages.iter().map(|stage| stage.name()).collect();
        f.debug_struct("CallChain")
            .field("stages", &stage_names)
            .finish_non_exhaustive()
    }
}

impl CallChain {
    /// Resolves the effective policy for `scopes` and evaluates every enabled
    /// stage's `before` hook in order.
    ///
    /// On rejection the stages that already passed run `after` in reverse
    /// order (resource release) and [`ChainOutcome::Rejected`] is returned.
    /// On stage failure the started stages run `on_error` in reverse order and
    /// [`ChainOutcome::Failed`] is returned.
    pub async fn before(
        &self,
        client_ip: Option<std::net::IpAddr>,
        scopes: &ChainScopes,
    ) -> Result<ChainContext, ChainOutcome> {
        let policy = Arc::new(self.resolver.resolve(scopes).await);
        let mut ctx = ChainContext::new(client_ip, scopes.clone(), policy);
        let mut started: Vec<Arc<dyn ChainStage>> = Vec::new();
        for stage in &self.stages {
            if !stage.enabled(&ctx.policy.stages) {
                continue;
            }
            match stage.before(&mut ctx).await {
                Ok(ChainVerdict::Pass) => started.push(stage.clone()),
                Ok(ChainVerdict::Reject(reason)) => {
                    Self::release_after(&started, &mut ctx).await;
                    return Err(ChainOutcome::Rejected(reason));
                }
                Err(error) => {
                    Self::run_on_error(&started, &mut ctx, &error).await;
                    return Err(ChainOutcome::Failed(error));
                }
            }
        }
        Ok(ctx)
    }

    /// Releases every enabled stage's `after` hook in reverse order. Safe to
    /// call without a preceding `before` (hooks must be idempotent).
    pub async fn after(&self, ctx: &mut ChainContext) -> Result<(), WebFrameworkError> {
        for stage in self.stages.iter().rev() {
            if stage.enabled(&ctx.policy.stages) {
                stage.after(ctx).await?;
            }
        }
        Ok(())
    }

    /// Runs every enabled stage's `on_error` hook in reverse order. Errors
    /// from hooks are logged and swallowed so the original error surfaces.
    pub async fn on_error(&self, ctx: &mut ChainContext, error: &WebFrameworkError) {
        for stage in self.stages.iter().rev() {
            if !stage.enabled(&ctx.policy.stages) {
                continue;
            }
            if let Err(hook_error) = stage.on_error(ctx, error).await {
                tracing::warn!(
                    stage = stage.name(),
                    error = ?hook_error,
                    "call chain on_error hook failed"
                );
            }
        }
    }

    async fn release_after(started: &[Arc<dyn ChainStage>], ctx: &mut ChainContext) {
        for stage in started.iter().rev() {
            if let Err(error) = stage.after(ctx).await {
                tracing::warn!(
                    stage = stage.name(),
                    error = ?error,
                    "call chain release hook failed after rejection"
                );
            }
        }
    }

    async fn run_on_error(
        started: &[Arc<dyn ChainStage>],
        ctx: &mut ChainContext,
        error: &WebFrameworkError,
    ) {
        for stage in started.iter().rev() {
            if let Err(hook_error) = stage.on_error(ctx, error).await {
                tracing::warn!(
                    stage = stage.name(),
                    error = ?hook_error,
                    "call chain on_error hook failed after stage failure"
                );
            }
        }
    }

    pub fn stages(&self) -> &[Arc<dyn ChainStage>] {
        &self.stages
    }
}

/// Builds a [`CallChain`] from composable stages and a policy resolver.
#[derive(Default)]
pub struct CallChainBuilder {
    stages: Vec<Arc<dyn ChainStage>>,
    resolver: Option<Arc<dyn PolicyResolver>>,
}

impl CallChainBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a stage. Duplicate stage names are rejected at build time.
    pub fn with_stage(mut self, stage: Arc<dyn ChainStage>) -> Self {
        self.stages.push(stage);
        self
    }

    /// Sets the resolver that computes the effective per-scope policy.
    pub fn with_policy_resolver(mut self, resolver: Arc<dyn PolicyResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    pub fn build(self) -> Result<CallChain, WebFrameworkError> {
        let resolver = self.resolver.ok_or_else(|| {
            WebFrameworkError::internal_server_error("call chain requires a policy resolver")
        })?;
        let mut seen = std::collections::HashSet::new();
        for stage in &self.stages {
            if !seen.insert(stage.name()) {
                return Err(WebFrameworkError::internal_server_error(format!(
                    "duplicate call chain stage name: {}",
                    stage.name()
                )));
            }
        }
        let mut stages = self.stages;
        stages.sort_by(|a, b| (a.stage_order(), a.name()).cmp(&(b.stage_order(), b.name())));
        Ok(CallChain { stages, resolver })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{ChainPolicy, PolicyResolver, ResolvedChainPolicy};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct ResolvePolicy(ChainPolicy);

    #[async_trait::async_trait]
    impl PolicyResolver for ResolvePolicy {
        async fn resolve(&self, _scopes: &ChainScopes) -> ResolvedChainPolicy {
            ResolvedChainPolicy::from(&self.0)
        }
    }

    struct TraceStage {
        name: &'static str,
        order: u32,
        reject: Option<RejectReason>,
        fail: bool,
        events: Arc<std::sync::Mutex<Vec<String>>>,
        enabled: bool,
    }

    #[async_trait::async_trait]
    impl ChainStage for TraceStage {
        fn name(&self) -> &'static str {
            self.name
        }
        fn stage_order(&self) -> u32 {
            self.order
        }
        fn enabled(&self, _enablement: &StageEnablement) -> bool {
            self.enabled
        }
        async fn before(&self, _ctx: &mut ChainContext) -> Result<ChainVerdict, WebFrameworkError> {
            self.events
                .lock()
                .expect("lock")
                .push(format!("before:{}", self.name));
            if let Some(reason) = &self.reject {
                return Ok(ChainVerdict::Reject(reason.clone()));
            }
            if self.fail {
                return Err(WebFrameworkError::internal_server_error("boom"));
            }
            Ok(ChainVerdict::Pass)
        }
        async fn after(&self, _ctx: &mut ChainContext) -> Result<(), WebFrameworkError> {
            self.events
                .lock()
                .expect("lock")
                .push(format!("after:{}", self.name));
            Ok(())
        }
        async fn on_error(
            &self,
            _ctx: &mut ChainContext,
            _error: &WebFrameworkError,
        ) -> Result<(), WebFrameworkError> {
            self.events
                .lock()
                .expect("lock")
                .push(format!("on_error:{}", self.name));
            Ok(())
        }
    }

    fn stage(
        name: &'static str,
        order: u32,
        events: &Arc<std::sync::Mutex<Vec<String>>>,
    ) -> Arc<dyn ChainStage> {
        Arc::new(TraceStage {
            name,
            order,
            reject: None,
            fail: false,
            events: events.clone(),
            enabled: true,
        })
    }

    fn scopes() -> ChainScopes {
        ChainScopes {
            tenant_id: Some(1),
            organization_id: Some(2),
            api_key_id: Some(3),
        }
    }

    #[tokio::test]
    async fn before_runs_stages_in_order_then_after_in_reverse() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let chain = CallChainBuilder::new()
            .with_stage(stage("zeta", 200, &events))
            .with_stage(stage("alpha", 100, &events))
            .with_policy_resolver(Arc::new(ResolvePolicy(ChainPolicy::default())))
            .build()
            .expect("chain");
        let mut ctx = chain.before(None, &scopes()).await.expect("passes");
        chain.after(&mut ctx).await.expect("after");
        let events = events.lock().expect("lock");
        let order: Vec<_> = events.iter().map(String::as_str).collect();
        assert_eq!(
            order,
            vec!["before:alpha", "before:zeta", "after:zeta", "after:alpha"]
        );
    }

    #[tokio::test]
    async fn rejection_short_circuits_and_releases_started_stages() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let rejecting = Arc::new(TraceStage {
            name: "guard",
            order: 100,
            reject: Some(RejectReason::ip_forbidden("denied")),
            fail: false,
            events: events.clone(),
            enabled: true,
        });
        let chain = CallChainBuilder::new()
            .with_stage(stage("first", 50, &events))
            .with_stage(rejecting)
            .with_stage(stage("later", 150, &events))
            .with_policy_resolver(Arc::new(ResolvePolicy(ChainPolicy::default())))
            .build()
            .expect("chain");
        let outcome = chain.before(None, &scopes()).await.expect_err("reject");
        match outcome {
            ChainOutcome::Rejected(reason) => {
                assert_eq!(reason.kind, RejectKind::IpForbidden);
                assert_eq!(reason.http_status, 403);
            }
            ChainOutcome::Failed(_) => panic!("expected rejection"),
        }
        let events = events.lock().expect("lock");
        let order: Vec<_> = events.iter().map(String::as_str).collect();
        // "later" never runs; "first" is released via reverse after.
        assert_eq!(order, vec!["before:first", "before:guard", "after:first"]);
    }

    #[tokio::test]
    async fn stage_failure_runs_on_error_in_reverse() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let failing = Arc::new(TraceStage {
            name: "fragile",
            order: 100,
            reject: None,
            fail: true,
            events: events.clone(),
            enabled: true,
        });
        let chain = CallChainBuilder::new()
            .with_stage(stage("first", 50, &events))
            .with_stage(failing)
            .with_policy_resolver(Arc::new(ResolvePolicy(ChainPolicy::default())))
            .build()
            .expect("chain");
        let outcome = chain.before(None, &scopes()).await.expect_err("fail");
        assert!(matches!(outcome, ChainOutcome::Failed(_)));
        let events = events.lock().expect("lock");
        let order: Vec<_> = events.iter().map(String::as_str).collect();
        assert_eq!(
            order,
            vec!["before:first", "before:fragile", "on_error:first"]
        );
    }

    #[tokio::test]
    async fn disabled_stages_are_skipped() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let skipped = Arc::new(TraceStage {
            name: "off",
            order: 100,
            reject: Some(RejectReason::ip_forbidden("must not run")),
            fail: false,
            events: events.clone(),
            enabled: false,
        });
        let chain = CallChainBuilder::new()
            .with_stage(skipped)
            .with_stage(stage("on", 150, &events))
            .with_policy_resolver(Arc::new(ResolvePolicy(ChainPolicy::default())))
            .build()
            .expect("chain");
        let mut ctx = chain.before(None, &scopes()).await.expect("passes");
        chain.after(&mut ctx).await.expect("after");
        let events = events.lock().expect("lock");
        assert_eq!(*events, vec!["before:on".to_owned(), "after:on".to_owned()]);
    }

    #[tokio::test]
    async fn build_rejects_duplicate_stage_names() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let error = CallChainBuilder::new()
            .with_stage(stage("dup", 100, &events))
            .with_stage(stage("dup", 200, &events))
            .with_policy_resolver(Arc::new(ResolvePolicy(ChainPolicy::default())))
            .build()
            .expect_err("duplicate");
        assert_eq!(
            error.kind,
            sdkwork_web_core::WebFrameworkErrorKind::InternalServerError
        );
    }

    #[tokio::test]
    async fn context_state_roundtrips_per_type() {
        let mut ctx = ChainContext::new(None, scopes(), Arc::new(ResolvedChainPolicy::default()));
        assert!(ctx.get_state::<u32>().is_none());
        ctx.insert_state(42_u32);
        assert_eq!(*ctx.get_state::<u32>().expect("value"), 42);
        assert!(ctx.get_state::<String>().is_none());
        assert_eq!(ctx.take_state::<u32>().expect("taken"), 42);
        assert!(ctx.get_state::<u32>().is_none());
    }

    #[test]
    fn stage_state_default_noop() {
        struct Probe;
        impl ChainStage for Probe {
            fn name(&self) -> &'static str {
                "probe"
            }
        }
        let stage = Arc::new(Probe);
        assert_eq!(stage.stage_order(), 100);
        assert!(stage.enabled(&StageEnablement::default()));
        assert!(!stage.enabled(&StageEnablement {
            disabled: Some(vec!["probe".to_owned()]),
            enabled_only: None,
        }));
    }

    #[test]
    fn noop_calls_are_available() {
        struct Probe;
        impl ChainStage for Probe {
            fn name(&self) -> &'static str {
                "probe"
            }
        }
        let _ = ChainVerdict::Pass;
        let _ = RejectReason::concurrency_exceeded("busy", 1);
        let _counter = AtomicUsize::new(0);
    }
}
