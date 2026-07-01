use crate::pool::WebStorePool;
use async_trait::async_trait;
use sdkwork_web_core::{
    tenant_runtime::{
        DynamicTenantRuntimeProfileSource, TenantRuntimeProfile, TenantRuntimeProfileContext,
    },
    WebFrameworkError,
};

/// Dynamic tenant runtime profile source backed by SQLx (SQLite or PostgreSQL).
pub struct SqlxTenantRuntimeProfileSource {
    pool: WebStorePool,
}

impl SqlxTenantRuntimeProfileSource {
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
struct TenantRuntimeProfileRow {
    rate_limit_enabled: Option<i64>,
    max_content_length: Option<i64>,
    max_concurrent_requests: Option<i64>,
}

#[async_trait]
impl DynamicTenantRuntimeProfileSource for SqlxTenantRuntimeProfileSource {
    async fn resolve(
        &self,
        ctx: &TenantRuntimeProfileContext,
    ) -> Result<Option<TenantRuntimeProfile>, WebFrameworkError> {
        match &self.pool {
            WebStorePool::Sqlite(pool) => {
                let row = sqlx::query_as::<_, TenantRuntimeProfileRow>(
                    "SELECT rate_limit_enabled, max_content_length, max_concurrent_requests \
                     FROM web_tenant_runtime_profile WHERE tenant_id = ? AND environment = ?",
                )
                .bind(ctx.tenant_scope())
                .bind(ctx.environment_label())
                .fetch_optional(pool)
                .await
                .map_err(sqlx_error)?;
                Ok(row_to_profile(row))
            }
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => {
                let row = sqlx::query_as::<_, TenantRuntimeProfileRow>(
                    "SELECT rate_limit_enabled, max_content_length, max_concurrent_requests \
                     FROM web_tenant_runtime_profile WHERE tenant_id = $1 AND environment = $2",
                )
                .bind(ctx.tenant_scope())
                .bind(ctx.environment_label())
                .fetch_optional(pool)
                .await
                .map_err(sqlx_error)?;
                Ok(row_to_profile(row))
            }
        }
    }
}

fn row_to_profile(row: Option<TenantRuntimeProfileRow>) -> Option<TenantRuntimeProfile> {
    let row = row?;
    Some(TenantRuntimeProfile {
        rate_limit_enabled: row.rate_limit_enabled.map(|v| v != 0),
        max_content_length: row
            .max_content_length
            .and_then(|v| u64::try_from(v.max(0)).ok()),
        max_concurrent_requests: row
            .max_concurrent_requests
            .and_then(|v| u32::try_from(v.max(0)).ok()),
    })
}

fn sqlx_error(error: sqlx::Error) -> WebFrameworkError {
    WebFrameworkError::dependency_unavailable(format!("sqlx store error: {error}"))
}
