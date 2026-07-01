use crate::api_chain::WebCallState;
use crate::security::RateLimitPolicy;
use sdkwork_web_contract::RateLimitTier;

/// Resolved per-request rate limit window (EP-10).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedRateLimitPolicy {
    pub max_requests: u32,
    pub window_secs: u64,
}

/// Maps manifest tiers to concrete windows (catalog D8).
pub fn limits_for_tier(tier: RateLimitTier) -> ResolvedRateLimitPolicy {
    match tier {
        RateLimitTier::AuthCritical => ResolvedRateLimitPolicy {
            max_requests: 10,
            window_secs: 60,
        },
        RateLimitTier::OpenApiDefault => ResolvedRateLimitPolicy {
            max_requests: 120,
            window_secs: 60,
        },
        RateLimitTier::Upload => ResolvedRateLimitPolicy {
            max_requests: 30,
            window_secs: 60,
        },
        RateLimitTier::Search => ResolvedRateLimitPolicy {
            max_requests: 60,
            window_secs: 60,
        },
        RateLimitTier::Bulk => ResolvedRateLimitPolicy {
            max_requests: 20,
            window_secs: 60,
        },
        RateLimitTier::Worker => ResolvedRateLimitPolicy {
            max_requests: 100,
            window_secs: 60,
        },
        RateLimitTier::Internal => ResolvedRateLimitPolicy {
            max_requests: 500,
            window_secs: 60,
        },
    }
}

/// Resolves effective rate limit policy for a request.
pub trait RateLimitPolicyResolver: Send + Sync {
    fn resolve(&self, state: &WebCallState, global: &RateLimitPolicy) -> ResolvedRateLimitPolicy;
}

/// Default resolver: manifest tier overrides global policy limits.
#[derive(Clone, Debug, Default)]
pub struct DefaultRateLimitPolicyResolver;

impl RateLimitPolicyResolver for DefaultRateLimitPolicyResolver {
    fn resolve(&self, state: &WebCallState, global: &RateLimitPolicy) -> ResolvedRateLimitPolicy {
        if let Some(tier) = state.rate_limit_tier {
            return limits_for_tier(tier);
        }
        ResolvedRateLimitPolicy {
            max_requests: global.max_requests_per_window,
            window_secs: global.window_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_chain::{WebCallCredentials, WebCallState};
    use crate::request_context::{WebApiSurface, WebAuthMode};

    fn base_state() -> WebCallState {
        WebCallState {
            request_id: None,
            api_surface: WebApiSurface::AppApi,
            auth_mode: WebAuthMode::Public,
            principal: None,
            path: "/app/v3/api/auth/login".to_owned(),
            method: "POST".to_owned(),
            origin: None,
            public_path: false,
            operation_id: None,
            route_template: None,
            client_kind: None,
            route_auth: None,
            credentials: WebCallCredentials {
                auth_token: None,
                access_token: None,
                api_key: None,
                oauth_bearer: None,
                agent_token: None,
            },
            idempotency_key: None,
            idempotency_fingerprint: None,
            idempotency_leader: false,
            idempotency_replay: None,
            traceparent: None,
            tracestate: None,
            rate_limit_tier: None,
            manifest_idempotent: false,
            resolved_cors: None,
            tenant_runtime_profile: None,
            resolved_rate_limit: None,
            concurrent_admission_key: None,
            accepted_at: None,
            forbid_credential_headers: false,
            before_failure: None,
        }
    }

    #[test]
    fn auth_critical_tier_is_stricter_than_global_default() {
        let mut state = base_state();
        state.rate_limit_tier = Some(RateLimitTier::AuthCritical);
        let resolver = DefaultRateLimitPolicyResolver;
        let global = RateLimitPolicy {
            enabled: true,
            max_requests_per_window: 120,
            window_secs: 60,
            pre_auth_rate_limit: true,
            tenant_limit_after_auth: false,
        };
        let resolved = resolver.resolve(&state, &global);
        assert_eq!(10, resolved.max_requests);
    }
}
