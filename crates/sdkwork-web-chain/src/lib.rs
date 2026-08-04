//! SDKWork web framework call chain: composable guard stages with layered
//! per-scope policy.
//!
//! [`CallChain`] runs an ordered set of [`ChainStage`] guards around a
//! protected operation (e.g. an open-API invocation). Stages are independent
//! building blocks — concurrency control ([`ConcurrencyStage`]) and IP
//! whitelist/blacklist ([`IpAccessStage`]) ship built-in, and applications
//! compose any number of custom stages through [`ChainStage`] plus a
//! [`PolicyResolver`] that computes the effective policy from built-in
//! defaults, global config, and per-API-key overrides.
//!
//! This is a business-domain guard chain complementary to the standard
//! `WebCallInterceptorChain` HTTP request chain in `sdkwork-web-core`; it
//! never redefines the standard 18-stage HTTP chain semantics.

pub mod chain;
pub mod concurrency;
pub mod ip_access;
pub mod policy;

pub use chain::{
    CallChain, CallChainBuilder, ChainContext, ChainOutcome, ChainScopes, ChainStage, ChainVerdict,
    RejectKind, RejectReason,
};
pub use concurrency::{ConcurrencyLeaseState, ConcurrencyStage};
pub use ip_access::{ip_matches_entry, ContextClientIpExtractor, IpAccessStage, IpExtractor};
pub use policy::{
    merge_chain_policies, ChainPolicy, ConcurrencyPolicy, ConcurrencyScope, IpAccessMode,
    IpAccessPolicy, PolicyResolver, ResolvedChainPolicy, StageEnablement,
};
