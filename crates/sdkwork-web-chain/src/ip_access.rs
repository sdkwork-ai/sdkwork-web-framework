//! IP whitelist/blacklist guard stage.
//!
//! [`IpAccessStage`] enforces the resolved [`crate::policy::IpAccessPolicy`]
//! with industry-standard semantics: denylist entries always win; when the
//! policy mode is `AllowlistOnly` and the allowlist is non-empty, only
//! listed clients pass. Matching supports exact IPs and CIDR blocks for
//! IPv4 and IPv6 via the `ipnet` crate.
//!
//! The client IP is obtained from the chain context unless a custom
//! [`IpExtractor`] is injected (callers may implement spoof-proof extraction,
//! e.g. honoring `trust_forwarded_headers` only for trusted ingress).

use crate::chain::{ChainContext, ChainStage, ChainVerdict, RejectReason};
use crate::policy::IpAccessPolicy;
use async_trait::async_trait;
use ipnet::IpNet;
use sdkwork_web_core::WebFrameworkError;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

/// Pluggable client-IP extraction. Callers inject their trust policy here.
#[async_trait]
pub trait IpExtractor: Send + Sync {
    async fn extract_client_ip(&self, ctx: &mut ChainContext) -> Option<IpAddr>;
}

/// Default extractor: uses the IP the caller placed on the context.
pub struct ContextClientIpExtractor;

#[async_trait]
impl IpExtractor for ContextClientIpExtractor {
    async fn extract_client_ip(&self, ctx: &mut ChainContext) -> Option<IpAddr> {
        ctx.client_ip
    }
}

/// IP access stage enforcing allowlist/denylist from the resolved policy.
pub struct IpAccessStage {
    extractor: Arc<dyn IpExtractor>,
}

impl IpAccessStage {
    pub fn new() -> Self {
        Self {
            extractor: Arc::new(ContextClientIpExtractor),
        }
    }

    /// Replaces the client-IP extraction strategy.
    pub fn with_extractor(mut self, extractor: Arc<dyn IpExtractor>) -> Self {
        self.extractor = extractor;
        self
    }

    /// Evaluates `ip` against `policy`. `None` IPs are denied only when a
    /// non-empty allowlist gates access; denylists never match a missing IP.
    pub fn evaluate(ip: Option<IpAddr>, policy: &IpAccessPolicy) -> ChainVerdict {
        if let Some(ip) = ip {
            if policy
                .denylist
                .iter()
                .any(|entry| ip_matches_entry(ip, entry))
            {
                return ChainVerdict::Reject(RejectReason::ip_forbidden(
                    "client IP is denied by the call chain access policy",
                ));
            }
        }
        if policy.mode == crate::policy::IpAccessMode::AllowlistOnly && !policy.allowlist.is_empty()
        {
            let allowed = ip.is_some_and(|ip| {
                policy
                    .allowlist
                    .iter()
                    .any(|entry| ip_matches_entry(ip, entry))
            });
            if !allowed {
                return ChainVerdict::Reject(RejectReason::ip_forbidden(
                    "client IP is not allowed by the call chain access policy",
                ));
            }
        }
        ChainVerdict::Pass
    }
}

impl Default for IpAccessStage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChainStage for IpAccessStage {
    fn name(&self) -> &'static str {
        "ip_access"
    }

    fn stage_order(&self) -> u32 {
        100
    }

    async fn before(&self, ctx: &mut ChainContext) -> Result<ChainVerdict, WebFrameworkError> {
        let Some(policy) = ctx.policy.ip_access.clone() else {
            return Ok(ChainVerdict::Pass);
        };
        let ip = self.extractor.extract_client_ip(ctx).await;
        Ok(Self::evaluate(ip, &policy))
    }
}

/// `true` when `ip` matches `entry`, which is an exact address or a CIDR
/// block (IPv4 or IPv6). Invalid entries never match.
///
/// IPv4-mapped IPv6 addresses (`::ffff:a.b.c.d`) are normalized to their
/// IPv4 form before matching so denylist/allowlist entries written in IPv4
/// cannot be bypassed (or falsely denied) via dual-stack connections.
pub fn ip_matches_entry(ip: IpAddr, entry: &str) -> bool {
    let ip = canonical_ip(ip);
    let entry = entry.trim();
    if entry.is_empty() {
        return false;
    }
    if entry.contains('/') {
        // CIDR form: parse as a network and test containment.
        return IpNet::from_str(entry).is_ok_and(|network| network.contains(&ip));
    }
    // Exact form: parse as a single address (IPv4 or IPv6).
    match entry.parse::<IpAddr>() {
        Ok(candidate) => canonical_ip(candidate) == ip,
        Err(_) => {
            tracing::debug!(entry = %entry, "invalid IP list entry");
            false
        }
    }
}

/// Normalizes IPv4-mapped IPv6 addresses to their canonical IPv4 form.
fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        ipv4 => ipv4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{IpAccessMode, IpAccessPolicy};
    use std::net::IpAddr;

    fn ipv4(value: &str) -> IpAddr {
        value.parse().expect("ipv4")
    }

    #[test]
    fn exact_ip_and_cidr_match() {
        assert!(ip_matches_entry(ipv4("192.168.1.1"), "192.168.1.1"));
        assert!(ip_matches_entry(ipv4("192.168.1.1"), "192.168.1.0/24"));
        assert!(!ip_matches_entry(ipv4("192.168.2.1"), "192.168.1.0/24"));
        assert!(!ip_matches_entry(ipv4("8.8.8.8"), "192.168.1.0/24"));
    }

    #[test]
    fn ipv6_and_mapped_entries_match() {
        let ipv6: IpAddr = "2001:db8::1".parse().expect("ipv6");
        assert!(ip_matches_entry(ipv6, "2001:db8::1"));
        assert!(ip_matches_entry(ipv6, "2001:db8::/32"));
        assert!(!ip_matches_entry(ipv6, "2001:db9::/32"));
        assert!(!ip_matches_entry(ipv6, "192.168.1.1"));
    }

    #[test]
    fn ipv4_mapped_ipv6_is_normalized_before_matching() {
        let mapped: IpAddr = "::ffff:192.168.1.1".parse().expect("mapped");
        // Denylist entries written in IPv4 must not be bypassable via the
        // mapped representation (and must not falsely deny it either).
        assert!(ip_matches_entry(mapped, "192.168.1.1"));
        assert!(ip_matches_entry(mapped, "192.168.1.0/24"));
        assert!(!ip_matches_entry(mapped, "192.168.2.1"));
        // True IPv6 addresses keep their own family semantics.
        let ipv6: IpAddr = "2001:db8::1".parse().expect("ipv6");
        assert!(!ip_matches_entry(ipv6, "192.168.1.1"));
    }

    #[test]
    fn invalid_entries_never_match() {
        assert!(!ip_matches_entry(ipv4("1.2.3.4"), "not-an-ip"));
        assert!(!ip_matches_entry(ipv4("1.2.3.4"), "10.0.0.0/999"));
        assert!(!ip_matches_entry(ipv4("1.2.3.4"), ""));
        assert!(!ip_matches_entry(ipv4("1.2.3.4"), "  "));
    }

    #[test]
    fn denylist_always_wins_over_allowlist() {
        let policy = IpAccessPolicy {
            mode: IpAccessMode::AllowlistOnly,
            allowlist: vec!["1.2.3.4".to_owned()],
            denylist: vec!["1.2.3.4".to_owned()],
        };
        assert!(matches!(
            IpAccessStage::evaluate(Some(ipv4("1.2.3.4")), &policy),
            ChainVerdict::Reject(_)
        ));
    }

    #[test]
    fn allowlist_only_gates_when_non_empty() {
        let policy = IpAccessPolicy {
            mode: IpAccessMode::AllowlistOnly,
            allowlist: vec!["10.0.0.0/8".to_owned()],
            denylist: vec![],
        };
        assert!(matches!(
            IpAccessStage::evaluate(Some(ipv4("10.1.2.3")), &policy),
            ChainVerdict::Pass
        ));
        assert!(matches!(
            IpAccessStage::evaluate(Some(ipv4("8.8.8.8")), &policy),
            ChainVerdict::Reject(_)
        ));
        // Missing client IP cannot satisfy the allowlist.
        assert!(matches!(
            IpAccessStage::evaluate(None, &policy),
            ChainVerdict::Reject(_)
        ));
    }

    #[test]
    fn open_mode_with_empty_lists_allows_everything() {
        let policy = IpAccessPolicy::default();
        assert!(matches!(
            IpAccessStage::evaluate(Some(ipv4("8.8.8.8")), &policy),
            ChainVerdict::Pass
        ));
        assert!(matches!(
            IpAccessStage::evaluate(None, &policy),
            ChainVerdict::Pass
        ));
    }

    #[tokio::test]
    async fn stage_rejects_denied_ip_via_chain() {
        use crate::chain::ChainScopes;
        use crate::policy::{
            merge_chain_policies, ChainPolicy, PolicyResolver, ResolvedChainPolicy,
        };

        struct ResolvePolicy(IpAccessPolicy);

        #[async_trait]
        impl PolicyResolver for ResolvePolicy {
            async fn resolve(&self, _scopes: &ChainScopes) -> ResolvedChainPolicy {
                let global = ChainPolicy {
                    ip_access: Some(self.0.clone()),
                    ..ChainPolicy::default()
                };
                merge_chain_policies(&ChainPolicy::default(), &global, None)
            }
        }

        let policy = IpAccessPolicy {
            mode: IpAccessMode::Open,
            allowlist: vec![],
            denylist: vec!["10.0.0.0/8".to_owned()],
        };
        let stage = Arc::new(IpAccessStage::new());
        let chain = crate::chain::CallChainBuilder::new()
            .with_stage(stage)
            .with_policy_resolver(Arc::new(ResolvePolicy(policy)))
            .build()
            .expect("chain");
        let scopes = ChainScopes {
            tenant_id: Some(1),
            organization_id: Some(1),
            api_key_id: Some(1),
        };
        let outcome = chain
            .before(Some(ipv4("10.1.1.1")), &scopes)
            .await
            .expect_err("denied");
        assert!(matches!(
            outcome,
            crate::chain::ChainOutcome::Rejected(reason)
                if reason.http_status == 403
        ));
        let allowed = chain
            .before(Some(ipv4("8.8.8.8")), &scopes)
            .await
            .expect("allowed");
        drop(allowed);
    }
}
