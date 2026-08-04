//! Layered chain policy model and resolution.
//!
//! The effective policy for a request is resolved from three layers — built-in
//! defaults, a global policy, and a per-API-key policy — by the application
//! provided [`PolicyResolver`]. Field-level merge follows the industry
//! convention that the most specific layer wins (Kong/Envoy per-route
//! overriding global plugin config), and stage switches honor explicit
//! disablements over enablements for a safe default.

use crate::chain::ChainScopes;
use serde::{Deserialize, Serialize};

/// Concurrency scope a [`crate::concurrency::ConcurrencyStage`] enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConcurrencyScope {
    /// Platform-wide shared budget.
    Global,
    /// Budget per API key (`api_key_id`).
    ApiKey,
    /// Budget per tenant (`tenant_id`).
    Tenant,
}

impl ConcurrencyScope {
    /// Stable key fragment used for store keys and `max_inflight_per_scope`.
    pub fn name(self) -> &'static str {
        match self {
            ConcurrencyScope::Global => "global",
            ConcurrencyScope::ApiKey => "apiKey",
            ConcurrencyScope::Tenant => "tenant",
        }
    }
}

/// Concurrency (bulkhead) limits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConcurrencyPolicy {
    /// Maximum concurrent in-flight requests applied to every enabled scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_inflight: Option<u32>,
    /// Per-scope overrides keyed by [`ConcurrencyScope::name`]
    /// (e.g. `{"apiKey": 10}`), winning over `max_inflight`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_inflight_per_scope: Option<std::collections::HashMap<String, u32>>,
}

impl ConcurrencyPolicy {
    /// Effective limit for `scope`, honoring per-scope overrides.
    pub fn limit_for(&self, scope: ConcurrencyScope) -> Option<u32> {
        self.max_inflight_per_scope
            .as_ref()
            .and_then(|overrides| overrides.get(scope.name()).copied())
            .or(self.max_inflight)
    }
}

/// IP access mode: `Open` allows every client unless denied; `AllowlistOnly`
/// gates on the allowlist when it is non-empty.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IpAccessMode {
    #[default]
    Open,
    AllowlistOnly,
}

/// IP whitelist/blacklist policy. Entries are exact IPs or CIDR blocks
/// (IPv4 and IPv6). Denylist entries always win over allowlist matches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpAccessPolicy {
    #[serde(default)]
    pub mode: IpAccessMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowlist: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denylist: Vec<String>,
}

impl IpAccessPolicy {
    /// `true` when either list is non-empty or the mode gates access.
    pub fn is_restrictive(&self) -> bool {
        !self.allowlist.is_empty() || !self.denylist.is_empty()
    }
}

/// Per-stage enablement. `enabled_only` (when present) is a whitelist of
/// participating stages; `disabled` (when present) excludes stages. Both may
/// be combined: a stage runs when it is in `enabled_only` (or no
/// `enabled_only` is set) and not in `disabled`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageEnablement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_only: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<Vec<String>>,
}

impl StageEnablement {
    pub fn is_enabled(&self, name: &str) -> bool {
        if let Some(enabled_only) = &self.enabled_only {
            if !enabled_only.iter().any(|candidate| candidate == name) {
                return false;
            }
        }
        if let Some(disabled) = &self.disabled {
            if disabled.iter().any(|candidate| candidate == name) {
                return false;
            }
        }
        true
    }
}

/// One configuration layer of the chain: concurrency, IP access, stage switches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<ConcurrencyPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_access: Option<IpAccessPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stages: Option<StageEnablement>,
}

/// The merged, effective policy for one request plus an audit origin trace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedChainPolicy {
    pub concurrency: Option<ConcurrencyPolicy>,
    pub ip_access: Option<IpAccessPolicy>,
    pub stages: StageEnablement,
    /// Most specific layer that contributed (`default`/`global`/`api-key:{id}`).
    pub origin: String,
}

impl ResolvedChainPolicy {
    pub fn from(policy: &ChainPolicy) -> Self {
        Self {
            concurrency: policy.concurrency.clone(),
            ip_access: policy.ip_access.clone(),
            stages: policy.stages.clone().unwrap_or_default(),
            origin: "default".to_owned(),
        }
    }
}

/// Resolves the effective policy for a request's scopes.
///
/// Implemented by the consuming application (business repository), which owns
/// data access: e.g. global chain policy and per-API-key overrides from the
/// database, merged over built-in defaults. The framework never depends on
/// business data.
#[async_trait::async_trait]
pub trait PolicyResolver: Send + Sync {
    async fn resolve(&self, scopes: &ChainScopes) -> ResolvedChainPolicy;
}

fn pick<T: Clone>(per_key: &Option<T>, global: &Option<T>, defaults: &Option<T>) -> Option<T> {
    per_key
        .clone()
        .or_else(|| global.clone())
        .or_else(|| defaults.clone())
}

/// Merges three policy layers into the effective policy.
///
/// Field-level semantics: the most specific non-`None` value wins
/// (`per_key` > `global` > `defaults`). Stage switches merge field-wise the
/// same way; an explicit disablement at a more specific layer wins over an
/// enablement at a less specific layer (safe default).
pub fn merge_chain_policies(
    defaults: &ChainPolicy,
    global: &ChainPolicy,
    per_key: Option<&ChainPolicy>,
) -> ResolvedChainPolicy {
    let origin = if per_key.is_some() {
        "api-key"
    } else if global.is_restrictive() || global.concurrency.is_some() || global.stages.is_some() {
        "global"
    } else {
        "default"
    };
    ResolvedChainPolicy {
        concurrency: pick(
            &per_key.and_then(|policy| policy.concurrency.clone()),
            &global.concurrency,
            &defaults.concurrency,
        ),
        ip_access: pick(
            &per_key.and_then(|policy| policy.ip_access.clone()),
            &global.ip_access,
            &defaults.ip_access,
        ),
        stages: StageEnablement {
            enabled_only: pick(
                &per_key
                    .and_then(|policy| policy.stages.as_ref())
                    .and_then(|stages| stages.enabled_only.clone()),
                &global
                    .stages
                    .as_ref()
                    .and_then(|stages| stages.enabled_only.clone()),
                &defaults
                    .stages
                    .as_ref()
                    .and_then(|stages| stages.enabled_only.clone()),
            ),
            disabled: pick(
                &per_key
                    .and_then(|policy| policy.stages.as_ref())
                    .and_then(|stages| stages.disabled.clone()),
                &global
                    .stages
                    .as_ref()
                    .and_then(|stages| stages.disabled.clone()),
                &defaults
                    .stages
                    .as_ref()
                    .and_then(|stages| stages.disabled.clone()),
            ),
        },
        origin: origin.to_owned(),
    }
}

impl ChainPolicy {
    /// `true` when any layer is restrictive (enables or limits anything).
    fn is_restrictive(&self) -> bool {
        self.concurrency.is_some() || self.ip_access.is_some() || self.stages.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn global_ip() -> ChainPolicy {
        ChainPolicy {
            concurrency: None,
            ip_access: Some(IpAccessPolicy {
                mode: IpAccessMode::Open,
                allowlist: vec![],
                denylist: vec!["10.0.0.0/8".to_owned()],
            }),
            stages: None,
        }
    }

    #[test]
    fn per_key_overrides_global_and_defaults() {
        let defaults = ChainPolicy {
            concurrency: Some(ConcurrencyPolicy {
                max_inflight: Some(100),
                max_inflight_per_scope: None,
            }),
            ..ChainPolicy::default()
        };
        let global = global_ip();
        let per_key = ChainPolicy {
            concurrency: Some(ConcurrencyPolicy {
                max_inflight: Some(10),
                max_inflight_per_scope: None,
            }),
            ..ChainPolicy::default()
        };
        let resolved = merge_chain_policies(&defaults, &global, Some(&per_key));
        assert_eq!(
            resolved.concurrency.expect("concurrency").max_inflight,
            Some(10)
        );
        assert!(resolved.ip_access.is_some());
        assert_eq!(resolved.origin, "api-key");
    }

    #[test]
    fn per_scope_override_wins_over_max_inflight() {
        let policy = ConcurrencyPolicy {
            max_inflight: Some(50),
            max_inflight_per_scope: Some([("apiKey".to_owned(), 5_u32)].into_iter().collect()),
        };
        assert_eq!(policy.limit_for(ConcurrencyScope::ApiKey), Some(5));
        assert_eq!(policy.limit_for(ConcurrencyScope::Global), Some(50));
        assert_eq!(policy.limit_for(ConcurrencyScope::Tenant), Some(50));
    }

    #[test]
    fn stage_enablement_honors_whitelist_then_denylist() {
        let enablement = StageEnablement {
            enabled_only: Some(vec!["concurrency".to_owned()]),
            disabled: Some(vec!["ip_access".to_owned()]),
        };
        assert!(enablement.is_enabled("concurrency"));
        assert!(!enablement.is_enabled("ip_access"));
        assert!(!enablement.is_enabled("audit"));
        let open = StageEnablement::default();
        assert!(open.is_enabled("anything"));
        let only_disabled = StageEnablement {
            enabled_only: None,
            disabled: Some(vec!["ip_access".to_owned()]),
        };
        assert!(only_disabled.is_enabled("concurrency"));
        assert!(!only_disabled.is_enabled("ip_access"));
    }

    #[test]
    fn explicit_disable_at_specific_layer_wins() {
        let defaults = ChainPolicy {
            stages: Some(StageEnablement {
                enabled_only: None,
                disabled: None,
            }),
            ..ChainPolicy::default()
        };
        let global = ChainPolicy {
            stages: Some(StageEnablement {
                enabled_only: Some(vec!["concurrency".to_owned()]),
                disabled: None,
            }),
            ..ChainPolicy::default()
        };
        let per_key = ChainPolicy {
            stages: Some(StageEnablement {
                enabled_only: None,
                disabled: Some(vec!["concurrency".to_owned()]),
            }),
            ..ChainPolicy::default()
        };
        let resolved = merge_chain_policies(&defaults, &global, Some(&per_key));
        // per_key disables "concurrency"; merged whitelist comes from global.
        assert!(!resolved.stages.is_enabled("concurrency"));
        assert!(!resolved.stages.is_enabled("ip_access"));
    }

    #[test]
    fn ip_policy_serde_roundtrip_camel_case() {
        let policy = IpAccessPolicy {
            mode: IpAccessMode::AllowlistOnly,
            allowlist: vec!["1.2.3.0/24".to_owned()],
            denylist: vec!["1.2.3.4".to_owned()],
        };
        let json = serde_json::to_string(&policy).expect("serialize");
        assert!(json.contains("\"allowlistOnly\""));
        assert!(json.contains("\"allowlist\""));
        let decoded: IpAccessPolicy = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, policy);
    }

    #[test]
    fn stage_enablement_serde_roundtrip() {
        let enablement = StageEnablement {
            enabled_only: Some(vec!["concurrency".to_owned()]),
            disabled: Some(vec!["ipAccess".to_owned()]),
        };
        let json = serde_json::to_string(&enablement).expect("serialize");
        let decoded: StageEnablement = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, enablement);
    }

    #[test]
    fn resolved_policy_origin_falls_back_to_default() {
        let resolved = merge_chain_policies(&ChainPolicy::default(), &ChainPolicy::default(), None);
        assert_eq!(resolved.origin, "default");
    }
}
