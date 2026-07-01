use crate::pool::WebStorePool;
use async_trait::async_trait;
use sdkwork_web_core::{
    rate_limit::ResolvedRateLimitPolicy,
    rate_limit_policy::{DynamicRateLimitPolicySource, RateLimitPolicyContext},
    WebFrameworkError,
};

/// Dynamic rate limit policy source backed by SQLx (SQLite or PostgreSQL).
pub struct SqlxRateLimitPolicySource {
    pool: WebStorePool,
}

impl SqlxRateLimitPolicySource {
    pub fn new_sqlite(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: WebStorePool::Sqlite(pool),
        }
    }

    #[cfg(feature = "postgres")]
    pub fn new_postgres(pool: sqlx::PgPool) -> Self {
        Self {
            pool: WebStorePool::Postgres(pool),
        }
    }

    pub fn new(pool: WebStorePool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct RateLimitPolicyRow {
    max_requests: i64,
    window_secs: i64,
    enabled: i64,
}

#[async_trait]
impl DynamicRateLimitPolicySource for SqlxRateLimitPolicySource {
    async fn resolve(
        &self,
        ctx: &RateLimitPolicyContext,
    ) -> Result<Option<ResolvedRateLimitPolicy>, WebFrameworkError> {
        let environment = ctx.environment_label();
        let tier_key = ctx.tier_key();
        let tenant = ctx.tenant_scope();

        // Fallback chain: tenant+tier -> tenant+default -> platform+tier -> platform+default
        let candidates: &[(&str, &str)] = &[
            (tenant, tier_key),
            (tenant, "default"),
            ("0", tier_key),
            ("0", "default"),
        ];

        match &self.pool {
            WebStorePool::Sqlite(pool) => {
                for (tenant_id, tier) in candidates {
                    let row = sqlx::query_as::<_, RateLimitPolicyRow>(
                        "SELECT max_requests, window_secs, enabled FROM web_rate_limit_policy \
                         WHERE tenant_id = ? AND environment = ? AND tier_key = ? LIMIT 1",
                    )
                    .bind(tenant_id)
                    .bind(environment)
                    .bind(tier)
                    .fetch_optional(pool)
                    .await
                    .map_err(sqlx_error)?;
                    if let Some(r) = row {
                        return policy_from_row(r);
                    }
                }
                Ok(None)
            }
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => {
                for (tenant_id, tier) in candidates {
                    let row = sqlx::query_as::<_, RateLimitPolicyRow>(
                        "SELECT max_requests, window_secs, enabled FROM web_rate_limit_policy \
                         WHERE tenant_id = $1 AND environment = $2 AND tier_key = $3 LIMIT 1",
                    )
                    .bind(tenant_id)
                    .bind(environment)
                    .bind(tier)
                    .fetch_optional(pool)
                    .await
                    .map_err(sqlx_error)?;
                    if let Some(r) = row {
                        return policy_from_row(r);
                    }
                }
                Ok(None)
            }
        }
    }
}

fn policy_from_row(
    row: RateLimitPolicyRow,
) -> Result<Option<ResolvedRateLimitPolicy>, WebFrameworkError> {
    if row.enabled == 0 {
        return Ok(Some(ResolvedRateLimitPolicy {
            max_requests: 0,
            window_secs: row.window_secs.max(1) as u64,
        }));
    }
    Ok(Some(ResolvedRateLimitPolicy {
        max_requests: row.max_requests.max(0) as u32,
        window_secs: row.window_secs.max(1) as u64,
    }))
}

fn sqlx_error(error: sqlx::Error) -> WebFrameworkError {
    WebFrameworkError::dependency_unavailable(format!("sqlx store error: {error}"))
}

#[cfg(test)]
mod tests {
    use sdkwork_web_core::{
        rate_limit_policy::{rate_limit_tier_key, RateLimitPolicyContext},
        RateLimitTier, WebApiSurface, WebEnvironment,
    };

    #[test]
    fn tier_key_matches_manifest_tiers() {
        assert_eq!(
            "auth_critical",
            rate_limit_tier_key(Some(RateLimitTier::AuthCritical))
        );
    }

    #[test]
    fn context_tier_key_defaults_to_default() {
        let ctx = RateLimitPolicyContext {
            tenant_id: None,
            environment: WebEnvironment::Prod,
            api_surface: WebApiSurface::AppApi,
            rate_limit_tier: None,
            operation_id: None,
        };
        assert_eq!("default", ctx.tier_key());
    }
}
