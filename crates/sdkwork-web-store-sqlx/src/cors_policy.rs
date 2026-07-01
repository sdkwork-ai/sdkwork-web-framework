use crate::pool::WebStorePool;
use async_trait::async_trait;
use sdkwork_web_core::{
    cors_policy::{CorsPolicyContext, DynamicCorsPolicySource},
    CorsPolicy, WebFrameworkError,
};

/// Dynamic CORS policy source backed by SQLx (SQLite or PostgreSQL).
pub struct SqlxCorsPolicySource {
    pool: WebStorePool,
}

impl SqlxCorsPolicySource {
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
struct CorsPolicyRow {
    allow_all_origins: i64,
    allowed_origins: String,
    allow_credentials: i64,
}

#[async_trait]
impl DynamicCorsPolicySource for SqlxCorsPolicySource {
    async fn resolve(
        &self,
        ctx: &CorsPolicyContext,
    ) -> Result<Option<CorsPolicy>, WebFrameworkError> {
        match &self.pool {
            WebStorePool::Sqlite(pool) => {
                let row = sqlx::query_as::<_, CorsPolicyRow>(
                    "SELECT allow_all_origins, allowed_origins, allow_credentials \
                     FROM web_cors_policy WHERE tenant_id = ? AND environment = ?",
                )
                .bind(ctx.tenant_scope())
                .bind(ctx.environment_label())
                .fetch_optional(pool)
                .await
                .map_err(sqlx_error)?;
                row_to_policy(row)
            }
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => {
                let row = sqlx::query_as::<_, CorsPolicyRow>(
                    "SELECT allow_all_origins, allowed_origins, allow_credentials \
                     FROM web_cors_policy WHERE tenant_id = $1 AND environment = $2",
                )
                .bind(ctx.tenant_scope())
                .bind(ctx.environment_label())
                .fetch_optional(pool)
                .await
                .map_err(sqlx_error)?;
                row_to_policy(row)
            }
        }
    }
}

fn row_to_policy(row: Option<CorsPolicyRow>) -> Result<Option<CorsPolicy>, WebFrameworkError> {
    let Some(row) = row else {
        return Ok(None);
    };

    let allowed_origins: Vec<String> =
        serde_json::from_str(&row.allowed_origins).map_err(|error| {
            WebFrameworkError::dependency_unavailable(format!(
                "sqlx cors policy decode error: {error}"
            ))
        })?;

    Ok(Some(CorsPolicy {
        allow_all_origins: row.allow_all_origins != 0,
        allowed_origins,
        allowed_methods: CorsPolicy::default().allowed_methods,
        allowed_headers: CorsPolicy::default().allowed_headers,
        allow_credentials: row.allow_credentials != 0,
    }))
}

fn sqlx_error(error: sqlx::Error) -> WebFrameworkError {
    WebFrameworkError::dependency_unavailable(format!("sqlx store error: {error}"))
}
