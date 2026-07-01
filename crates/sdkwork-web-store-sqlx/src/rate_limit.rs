use crate::now_epoch_secs;
use crate::pool::WebStorePool;
use crate::purge::ThrottledPurge;
use async_trait::async_trait;
use sdkwork_web_core::{RateLimitStore, WebFrameworkError};
use std::sync::Arc;
use std::time::Duration;

/// SQLx-backed fixed-window rate limiter.
///
/// Supports both SQLite (`?` placeholders) and PostgreSQL (`$1` placeholders).
pub struct SqlxRateLimitStore {
    pool: WebStorePool,
    purge: Arc<ThrottledPurge>,
}

impl SqlxRateLimitStore {
    pub fn new_sqlite(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: WebStorePool::Sqlite(pool.clone()),
            purge: Arc::new(ThrottledPurge::rate_limit(WebStorePool::Sqlite(pool))),
        }
    }

    #[cfg(feature = "postgres")]
    pub fn new_postgres(pool: sqlx::PgPool) -> Self {
        Self {
            pool: WebStorePool::Postgres(pool.clone()),
            purge: Arc::new(ThrottledPurge::rate_limit(WebStorePool::Postgres(pool))),
        }
    }

    pub fn new(pool: WebStorePool) -> Self {
        let _is_sqlite = pool.is_sqlite();
        Self {
            pool: pool.clone(),
            purge: Arc::new(ThrottledPurge::rate_limit(pool)),
        }
    }
}

#[derive(sqlx::FromRow)]
struct RateLimitRow {
    request_count: i64,
    window_start: i64,
}

#[async_trait]
impl RateLimitStore for SqlxRateLimitStore {
    async fn check_and_record(
        &self,
        key: &str,
        max_requests: u32,
        window: Duration,
    ) -> Result<(), WebFrameworkError> {
        self.purge.maybe_run().await?;
        let window_secs = window.as_secs().max(1) as i64;
        let now = now_epoch_secs();
        let expires_at = ttl_epoch_secs(window);

        // Use backend-specific queries.
        match &self.pool {
            WebStorePool::Sqlite(pool) => {
                sqlite_check_and_record(pool, key, max_requests, window_secs, now, expires_at).await
            }
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => {
                pg_check_and_record(pool, key, max_requests, window_secs, now, expires_at).await
            }
        }
    }

    fn is_distributed_ha(&self) -> bool {
        self.pool.is_distributed_ha()
    }
}

/// SQLite implementation — uses `?` placeholders and `INSERT OR REPLACE` / `ON CONFLICT DO UPDATE`.
async fn sqlite_check_and_record(
    pool: &sqlx::SqlitePool,
    key: &str,
    max_requests: u32,
    window_secs: i64,
    now: i64,
    expires_at: i64,
) -> Result<(), WebFrameworkError> {
    let mut tx = pool.begin().await.map_err(sqlx_error)?;

    let row = sqlx::query_as::<_, RateLimitRow>(
        "SELECT request_count, window_start FROM web_rate_limit_bucket WHERE bucket_key = ?",
    )
    .bind(key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(sqlx_error)?;

    let (next_count, window_start) = match row {
        None => (1_i64, now),
        Some(row) if now.saturating_sub(row.window_start) >= window_secs => (1, now),
        Some(row) => {
            if row.request_count >= i64::from(max_requests) {
                tx.rollback().await.map_err(sqlx_error)?;
                return Err(WebFrameworkError::rate_limit_exceeded(
                    "rate limit exceeded",
                    window_secs as u64,
                ));
            }
            (row.request_count + 1, row.window_start)
        }
    };

    if next_count > i64::from(max_requests) {
        tx.rollback().await.map_err(sqlx_error)?;
        return Err(WebFrameworkError::rate_limit_exceeded(
            "rate limit exceeded",
            window_secs as u64,
        ));
    }

    sqlx::query(
        "INSERT INTO web_rate_limit_bucket (bucket_key, request_count, window_start, expires_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(bucket_key) DO UPDATE SET
           request_count = excluded.request_count,
           window_start = excluded.window_start,
           expires_at = excluded.expires_at",
    )
    .bind(key)
    .bind(next_count)
    .bind(window_start)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(sqlx_error)?;

    tx.commit().await.map_err(sqlx_error)?;
    Ok(())
}

/// PostgreSQL implementation — uses `$N` placeholders and `ON CONFLICT DO UPDATE`.
#[cfg(feature = "postgres")]
async fn pg_check_and_record(
    pool: &sqlx::PgPool,
    key: &str,
    max_requests: u32,
    window_secs: i64,
    now: i64,
    expires_at: i64,
) -> Result<(), WebFrameworkError> {
    let mut tx = pool.begin().await.map_err(sqlx_error)?;

    let row = sqlx::query_as::<_, RateLimitRow>(
        "SELECT request_count, window_start FROM web_rate_limit_bucket WHERE bucket_key = $1",
    )
    .bind(key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(sqlx_error)?;

    let (next_count, window_start) = match row {
        None => (1_i64, now),
        Some(row) if now.saturating_sub(row.window_start) >= window_secs => (1, now),
        Some(row) => {
            if row.request_count >= i64::from(max_requests) {
                tx.rollback().await.map_err(sqlx_error)?;
                return Err(WebFrameworkError::rate_limit_exceeded(
                    "rate limit exceeded",
                    window_secs as u64,
                ));
            }
            (row.request_count + 1, row.window_start)
        }
    };

    if next_count > i64::from(max_requests) {
        tx.rollback().await.map_err(sqlx_error)?;
        return Err(WebFrameworkError::rate_limit_exceeded(
            "rate limit exceeded",
            window_secs as u64,
        ));
    }

    sqlx::query(
        "INSERT INTO web_rate_limit_bucket (bucket_key, request_count, window_start, expires_at)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (bucket_key) DO UPDATE SET
           request_count = EXCLUDED.request_count,
           window_start = EXCLUDED.window_start,
           expires_at = EXCLUDED.expires_at",
    )
    .bind(key)
    .bind(next_count)
    .bind(window_start)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(sqlx_error)?;

    tx.commit().await.map_err(sqlx_error)?;
    Ok(())
}

use crate::purge::sqlx_error;
use crate::ttl_epoch_secs;
