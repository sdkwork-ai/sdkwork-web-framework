use crate::pool::WebStorePool;
use crate::purge::ThrottledPurge;
use async_trait::async_trait;
use sdkwork_web_core::{AuditEmitter, AuditFact, WebFrameworkError};
use std::sync::Arc;

/// 90-day default TTL for audit rows. DATABASE_SPEC §6.3 / SECURITY_SPEC §5.1.
const AUDIT_TTL_SECS: i64 = 90 * 24 * 60 * 60;

/// SQLx-backed audit event emitter supporting SQLite and PostgreSQL.
pub struct SqlxAuditEmitter {
    pool: WebStorePool,
    purge: Arc<ThrottledPurge>,
}

impl SqlxAuditEmitter {
    pub fn new_sqlite(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: WebStorePool::Sqlite(pool.clone()),
            purge: Arc::new(ThrottledPurge::audit(WebStorePool::Sqlite(pool))),
        }
    }

    #[cfg(feature = "postgres")]
    pub fn new_postgres(pool: sqlx::PgPool) -> Self {
        Self {
            pool: WebStorePool::Postgres(pool.clone()),
            purge: Arc::new(ThrottledPurge::audit(WebStorePool::Postgres(pool))),
        }
    }

    pub fn new(pool: WebStorePool) -> Self {
        Self {
            pool: pool.clone(),
            purge: Arc::new(ThrottledPurge::audit(pool)),
        }
    }
}

#[async_trait]
impl AuditEmitter for SqlxAuditEmitter {
    async fn emit(&self, fact: AuditFact) -> Result<(), WebFrameworkError> {
        // Throttled purge fails silently (best-effort).
        let _ = self.purge.maybe_run().await;
        let now = crate::now_epoch_secs();
        let expires_at = now + AUDIT_TTL_SECS;

        match &self.pool {
            WebStorePool::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO web_audit_event \
                     (request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at, expires_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&fact.request_id)
                .bind(&fact.tenant_id)
                .bind(&fact.user_id)
                .bind(format!("{:?}", fact.api_surface))
                .bind(&fact.path)
                .bind(&fact.method)
                .bind(&fact.operation_id)
                .bind(fact.status_code.map(i64::from))
                .bind(fact.duration_ms.map(|v| v as i64))
                .bind(now)
                .bind(expires_at)
                .execute(pool)
                .await
                .map_err(|error| {
                    WebFrameworkError::dependency_unavailable(format!(
                        "sqlx audit store error: {error}"
                    ))
                })?;
            }
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO web_audit_event \
                     (request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at, expires_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                )
                .bind(&fact.request_id)
                .bind(&fact.tenant_id)
                .bind(&fact.user_id)
                .bind(format!("{:?}", fact.api_surface))
                .bind(&fact.path)
                .bind(&fact.method)
                .bind(&fact.operation_id)
                .bind(fact.status_code.map(i64::from))
                .bind(fact.duration_ms.map(|v| v as i64))
                .bind(now)
                .bind(expires_at)
                .execute(pool)
                .await
                .map_err(|error| {
                    WebFrameworkError::dependency_unavailable(format!(
                        "pg audit store error: {error}"
                    ))
                })?;
            }
        }
        Ok(())
    }
}
