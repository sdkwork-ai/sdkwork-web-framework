use crate::pool::WebStorePool;
use crate::purge::ThrottledPurge;
use crate::{now_epoch_secs, ttl_epoch_secs};
use async_trait::async_trait;
use sdkwork_web_core::{
    IdempotencyBeginOutcome, IdempotencyResponseRecord, IdempotencyStore, WebFrameworkError,
};
use std::sync::Arc;
use std::time::Duration;

/// SQLx-backed idempotency store supporting SQLite and PostgreSQL.
pub struct SqlxIdempotencyStore {
    pool: WebStorePool,
    purge: Arc<ThrottledPurge>,
}

impl SqlxIdempotencyStore {
    pub fn new_sqlite(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: WebStorePool::Sqlite(pool.clone()),
            purge: Arc::new(ThrottledPurge::idempotency(WebStorePool::Sqlite(pool))),
        }
    }

    #[cfg(feature = "postgres")]
    pub fn new_postgres(pool: sqlx::PgPool) -> Self {
        Self {
            pool: WebStorePool::Postgres(pool.clone()),
            purge: Arc::new(ThrottledPurge::idempotency(WebStorePool::Postgres(pool))),
        }
    }

    pub fn new(pool: WebStorePool) -> Self {
        Self {
            pool: pool.clone(),
            purge: Arc::new(ThrottledPurge::idempotency(pool)),
        }
    }
}

#[derive(sqlx::FromRow)]
struct IdempotencyRow {
    fingerprint: String,
    response_status: Option<i64>,
    response_body: Option<Vec<u8>>,
    content_type: Option<String>,
}

#[async_trait]
impl IdempotencyStore for SqlxIdempotencyStore {
    async fn begin(
        &self,
        key: &str,
        fingerprint: &str,
        ttl: Duration,
    ) -> Result<IdempotencyBeginOutcome, WebFrameworkError> {
        self.purge.maybe_run().await?;

        match &self.pool {
            WebStorePool::Sqlite(pool) => sqlite_begin(pool, key, fingerprint, ttl).await,
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => pg_begin(pool, key, fingerprint, ttl).await,
        }
    }

    async fn complete(
        &self,
        key: &str,
        fingerprint: &str,
        record: IdempotencyResponseRecord,
        ttl: Duration,
    ) -> Result<(), WebFrameworkError> {
        match &self.pool {
            WebStorePool::Sqlite(pool) => {
                sqlite_complete(pool, key, fingerprint, record, ttl).await
            }
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => pg_complete(pool, key, fingerprint, record, ttl).await,
        }
    }

    async fn release(&self, key: &str, fingerprint: &str) -> Result<(), WebFrameworkError> {
        match &self.pool {
            WebStorePool::Sqlite(pool) => sqlite_release(pool, key, fingerprint).await,
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => pg_release(pool, key, fingerprint).await,
        }
    }

    fn is_distributed_ha(&self) -> bool {
        self.pool.is_distributed_ha()
    }
}

/// ---- SQLite implementations ----
fn sqlite_begin<'a>(
    pool: &'a sqlx::SqlitePool,
    key: &'a str,
    fingerprint: &'a str,
    ttl: Duration,
) -> std::pin::Pin<
    Box<
        dyn std::future::Future<Output = Result<IdempotencyBeginOutcome, WebFrameworkError>>
            + Send
            + 'a,
    >,
> {
    Box::pin(async move {
        let row = sqlx::query_as::<_, IdempotencyRow>(
            "SELECT fingerprint, response_status, response_body, content_type \
             FROM web_idempotency_record WHERE idempotency_key = ?",
        )
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(sqlx_error)?;

        if let Some(row) = row {
            if row.fingerprint != fingerprint {
                return Err(WebFrameworkError::conflict(
                    "idempotency key was already used with a different request fingerprint",
                ));
            }
            if let Some(status_code) = row.response_status {
                return Ok(IdempotencyBeginOutcome::Replay(IdempotencyResponseRecord {
                    status_code: status_code as u16,
                    body: row.response_body.unwrap_or_default(),
                    content_type: row.content_type,
                }));
            }
            return Err(WebFrameworkError::conflict(
                "idempotency key is already in progress",
            ));
        }

        let now = now_epoch_secs();
        let expires_at = ttl_epoch_secs(ttl);
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO web_idempotency_record \
             (idempotency_key, fingerprint, response_status, response_body, content_type, created_at, expires_at) \
             VALUES (?, ?, NULL, NULL, NULL, ?, ?)",
        )
        .bind(key)
        .bind(fingerprint)
        .bind(now)
        .bind(expires_at)
        .execute(pool)
        .await
        .map_err(sqlx_error)?;

        if inserted.rows_affected() == 0 {
            // Race condition: another request inserted the key concurrently.
            // Return conflict to signal the caller to retry or fail.
            return Err(WebFrameworkError::conflict(
                "idempotency key race condition - please retry",
            ));
        }
        Ok(IdempotencyBeginOutcome::Leader)
    })
}

async fn sqlite_complete(
    pool: &sqlx::SqlitePool,
    key: &str,
    fingerprint: &str,
    record: IdempotencyResponseRecord,
    ttl: Duration,
) -> Result<(), WebFrameworkError> {
    let expires_at = ttl_epoch_secs(ttl);
    let updated = sqlx::query(
        "UPDATE web_idempotency_record \
         SET response_status = ?, response_body = ?, content_type = ?, expires_at = ? \
         WHERE idempotency_key = ? AND fingerprint = ? AND response_status IS NULL",
    )
    .bind(i64::from(record.status_code))
    .bind(record.body)
    .bind(record.content_type)
    .bind(expires_at)
    .bind(key)
    .bind(fingerprint)
    .execute(pool)
    .await
    .map_err(sqlx_error)?;

    if updated.rows_affected() == 0 {
        let row = sqlx::query_as::<_, IdempotencyRow>(
            "SELECT fingerprint, response_status, response_body, content_type \
             FROM web_idempotency_record WHERE idempotency_key = ?",
        )
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(sqlx_error)?;
        let Some(row) = row else {
            return Err(WebFrameworkError::bad_request(
                "idempotency key was not reserved",
            ));
        };
        if row.fingerprint != fingerprint {
            return Err(WebFrameworkError::conflict(
                "idempotency key fingerprint mismatch while completing response",
            ));
        }
    }
    Ok(())
}

async fn sqlite_release(
    pool: &sqlx::SqlitePool,
    key: &str,
    fingerprint: &str,
) -> Result<(), WebFrameworkError> {
    sqlx::query(
        "DELETE FROM web_idempotency_record \
         WHERE idempotency_key = ? AND fingerprint = ? AND response_status IS NULL",
    )
    .bind(key)
    .bind(fingerprint)
    .execute(pool)
    .await
    .map_err(sqlx_error)?;
    Ok(())
}

/// ---- PostgreSQL implementations ----

#[cfg(feature = "postgres")]
fn pg_begin<'a>(
    pool: &'a sqlx::PgPool,
    key: &'a str,
    fingerprint: &'a str,
    ttl: Duration,
) -> std::pin::Pin<
    Box<
        dyn std::future::Future<Output = Result<IdempotencyBeginOutcome, WebFrameworkError>>
            + Send
            + 'a,
    >,
> {
    Box::pin(async move {
        let row = sqlx::query_as::<_, IdempotencyRow>(
            "SELECT fingerprint, response_status, response_body, content_type \
             FROM web_idempotency_record WHERE idempotency_key = $1",
        )
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(sqlx_error)?;

        if let Some(row) = row {
            if row.fingerprint != fingerprint {
                return Err(WebFrameworkError::conflict(
                    "idempotency key was already used with a different request fingerprint",
                ));
            }
            if let Some(status_code) = row.response_status {
                return Ok(IdempotencyBeginOutcome::Replay(IdempotencyResponseRecord {
                    status_code: status_code as u16,
                    body: row.response_body.unwrap_or_default(),
                    content_type: row.content_type,
                }));
            }
            return Err(WebFrameworkError::conflict(
                "idempotency key is already in progress",
            ));
        }

        let now = now_epoch_secs();
        let expires_at = ttl_epoch_secs(ttl);
        // On PostgreSQL, use INSERT ... ON CONFLICT DO NOTHING
        let inserted = sqlx::query(
            "INSERT INTO web_idempotency_record \
             (idempotency_key, fingerprint, response_status, response_body, content_type, created_at, expires_at) \
             VALUES ($1, $2, NULL, NULL, NULL, $3, $4) \
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(key)
        .bind(fingerprint)
        .bind(now)
        .bind(expires_at)
        .execute(pool)
        .await
        .map_err(sqlx_error)?;

        if inserted.rows_affected() == 0 {
            // Race condition: another request inserted the key concurrently.
            // Return conflict to signal the caller to retry or fail.
            return Err(WebFrameworkError::conflict(
                "idempotency key race condition - please retry",
            ));
        }
        Ok(IdempotencyBeginOutcome::Leader)
    })
}

#[cfg(feature = "postgres")]
async fn pg_complete(
    pool: &sqlx::PgPool,
    key: &str,
    fingerprint: &str,
    record: IdempotencyResponseRecord,
    ttl: Duration,
) -> Result<(), WebFrameworkError> {
    let expires_at = ttl_epoch_secs(ttl);
    let updated = sqlx::query(
        "UPDATE web_idempotency_record \
         SET response_status = $1, response_body = $2, content_type = $3, expires_at = $4 \
         WHERE idempotency_key = $5 AND fingerprint = $6 AND response_status IS NULL",
    )
    .bind(i64::from(record.status_code))
    .bind(record.body)
    .bind(record.content_type)
    .bind(expires_at)
    .bind(key)
    .bind(fingerprint)
    .execute(pool)
    .await
    .map_err(sqlx_error)?;

    if updated.rows_affected() == 0 {
        let row = sqlx::query_as::<_, IdempotencyRow>(
            "SELECT fingerprint, response_status, response_body, content_type \
             FROM web_idempotency_record WHERE idempotency_key = $1",
        )
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(sqlx_error)?;
        let Some(row) = row else {
            return Err(WebFrameworkError::bad_request(
                "idempotency key was not reserved",
            ));
        };
        if row.fingerprint != fingerprint {
            return Err(WebFrameworkError::conflict(
                "idempotency key fingerprint mismatch while completing response",
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "postgres")]
async fn pg_release(
    pool: &sqlx::PgPool,
    key: &str,
    fingerprint: &str,
) -> Result<(), WebFrameworkError> {
    sqlx::query(
        "DELETE FROM web_idempotency_record \
         WHERE idempotency_key = $1 AND fingerprint = $2 AND response_status IS NULL",
    )
    .bind(key)
    .bind(fingerprint)
    .execute(pool)
    .await
    .map_err(sqlx_error)?;
    Ok(())
}

use crate::purge::sqlx_error;
