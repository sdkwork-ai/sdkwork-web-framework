use crate::now_epoch_secs;
use crate::pool::WebStorePool;
use sdkwork_web_core::WebFrameworkError;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

const DEFAULT_PURGE_INTERVAL_SECS: i64 = 60;

/// Generic throttled purge that deletes expired rows from a single table.
///
/// `table_sql` must be a parameterised DELETE with a single `?` (SQLite) / `$1` (PostgreSQL)
/// placeholder for the expiry timestamp.
#[derive(Clone)]
pub struct ThrottledPurge {
    pool: WebStorePool,
    table_sql: Arc<str>,
    last_purge_secs: Arc<AtomicI64>,
    interval_secs: i64,
}

impl ThrottledPurge {
    pub(crate) fn idempotency(pool: WebStorePool) -> Self {
        let pool_clone = pool.clone();
        Self::new(
            pool_clone,
            match &pool {
                WebStorePool::Sqlite(_) => {
                    "DELETE FROM web_idempotency_record WHERE expires_at <= ?"
                }
                #[cfg(feature = "postgres")]
                WebStorePool::Postgres(_) => {
                    "DELETE FROM web_idempotency_record WHERE expires_at <= $1"
                }
            },
        )
    }

    pub(crate) fn rate_limit(pool: WebStorePool) -> Self {
        let pool_clone = pool.clone();
        Self::new(
            pool_clone,
            match &pool {
                WebStorePool::Sqlite(_) => {
                    "DELETE FROM web_rate_limit_bucket WHERE expires_at <= ?"
                }
                #[cfg(feature = "postgres")]
                WebStorePool::Postgres(_) => {
                    "DELETE FROM web_rate_limit_bucket WHERE expires_at <= $1"
                }
            },
        )
    }

    pub(crate) fn audit(pool: WebStorePool) -> Self {
        let pool_clone = pool.clone();
        Self::new(
            pool_clone,
            match &pool {
                WebStorePool::Sqlite(_) => {
                    "DELETE FROM web_audit_event WHERE expires_at IS NOT NULL AND expires_at <= ?"
                }
                #[cfg(feature = "postgres")]
                WebStorePool::Postgres(_) => {
                    "DELETE FROM web_audit_event WHERE expires_at IS NOT NULL AND expires_at <= $1"
                }
            },
        )
    }

    pub(crate) fn security_event(pool: WebStorePool) -> Self {
        let pool_clone = pool.clone();
        Self::new(
            pool_clone,
            match &pool {
                WebStorePool::Sqlite(_) => {
                    "DELETE FROM web_security_event WHERE expires_at IS NOT NULL AND expires_at <= ?"
                }
                #[cfg(feature = "postgres")]
                WebStorePool::Postgres(_) => {
                    "DELETE FROM web_security_event WHERE expires_at IS NOT NULL AND expires_at <= $1"
                }
            },
        )
    }

    fn new(pool: WebStorePool, table_sql: &'static str) -> Self {
        Self {
            pool,
            table_sql: Arc::from(table_sql),
            last_purge_secs: Arc::new(AtomicI64::new(0)),
            interval_secs: DEFAULT_PURGE_INTERVAL_SECS,
        }
    }

    pub(crate) async fn maybe_run(&self) -> Result<(), WebFrameworkError> {
        let now = now_epoch_secs();
        let last = self.last_purge_secs.load(Ordering::Relaxed);
        if now.saturating_sub(last) < self.interval_secs {
            return Ok(());
        }
        if self
            .last_purge_secs
            .compare_exchange(last, now, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }

        match &self.pool {
            WebStorePool::Sqlite(pool) => {
                sqlx::query(self.table_sql.as_ref())
                    .bind(now)
                    .execute(pool)
                    .await
                    .map_err(sqlx_error)?;
            }
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => {
                sqlx::query(self.table_sql.as_ref())
                    .bind(now)
                    .execute(pool)
                    .await
                    .map_err(sqlx_error)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn sqlx_error(error: sqlx::Error) -> WebFrameworkError {
    WebFrameworkError::dependency_unavailable(format!("sqlx store error: {error}"))
}
