use crate::pool::WebStorePool;
use crate::purge::ThrottledPurge;
use async_trait::async_trait;
use sdkwork_web_core::{SecurityEvent, SecurityEventEmitter, SecurityEventKind, WebFrameworkError};
use std::sync::Arc;

/// 90-day default TTL for security event rows. DATABASE_SPEC §6.3 / SECURITY_SPEC §5.1.
const SECURITY_EVENT_TTL_SECS: i64 = 90 * 24 * 60 * 60;

/// SQLx-backed security event emitter supporting SQLite and PostgreSQL.
///
/// NOTE: `emit()` failures are logged but NOT propagated to the caller.
/// This implements **fail-open** semantics: a transient DB error on the
/// security event table MUST NOT block the legitimate request from completing.
/// Security events are written asynchronously — loss is acceptable for
/// operational continuity. SECURITY_SPEC §5.1 (fail-open amendment).
pub struct SqlxSecurityEventEmitter {
    pool: WebStorePool,
    purge: Arc<ThrottledPurge>,
}

impl SqlxSecurityEventEmitter {
    pub fn new_sqlite(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: WebStorePool::Sqlite(pool.clone()),
            purge: Arc::new(ThrottledPurge::security_event(WebStorePool::Sqlite(pool))),
        }
    }

    #[cfg(feature = "postgres")]
    pub fn new_postgres(pool: sqlx::PgPool) -> Self {
        Self {
            pool: WebStorePool::Postgres(pool.clone()),
            purge: Arc::new(ThrottledPurge::security_event(WebStorePool::Postgres(pool))),
        }
    }

    pub fn new(pool: WebStorePool) -> Self {
        Self {
            pool: pool.clone(),
            purge: Arc::new(ThrottledPurge::security_event(pool)),
        }
    }
}

#[async_trait]
impl SecurityEventEmitter for SqlxSecurityEventEmitter {
    async fn emit(&self, event: SecurityEvent) -> Result<(), WebFrameworkError> {
        // Throttled purge — fails silently on error (best-effort).
        let _ = self.purge.maybe_run().await;
        let now = crate::now_epoch_secs();
        let expires_at = now + SECURITY_EVENT_TTL_SECS;
        let tenant_id = event.tenant_id.unwrap_or_else(|| "0".to_owned());

        match &self.pool {
            WebStorePool::Sqlite(pool) => {
                let result = sqlx::query(
                    "INSERT INTO web_security_event \
                     (kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at, expires_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(security_event_kind_label(&event.kind))
                .bind(event.request_id)
                .bind(&tenant_id)
                .bind(event.path)
                .bind(event.method)
                .bind(format!("{:?}", event.api_surface))
                .bind(event.origin)
                .bind(event.detail)
                .bind(now)
                .bind(expires_at)
                .execute(pool)
                .await;
                match result {
                    Ok(_) => Ok(()),
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            tenant_id = %tenant_id,
                            kind = ?event.kind,
                            "security event store write failed (fail-open)"
                        );
                        Ok(())
                    }
                }
            }
            #[cfg(feature = "postgres")]
            WebStorePool::Postgres(pool) => {
                let result = sqlx::query(
                    "INSERT INTO web_security_event \
                     (kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at, expires_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                )
                .bind(security_event_kind_label(&event.kind))
                .bind(event.request_id)
                .bind(&tenant_id)
                .bind(event.path)
                .bind(event.method)
                .bind(format!("{:?}", event.api_surface))
                .bind(event.origin)
                .bind(event.detail)
                .bind(now)
                .bind(expires_at)
                .execute(pool)
                .await;
                match result {
                    Ok(_) => Ok(()),
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            tenant_id = %tenant_id,
                            kind = ?event.kind,
                            "security event store write failed (fail-open)"
                        );
                        Ok(())
                    }
                }
            }
        }
    }
}

fn security_event_kind_label(kind: &SecurityEventKind) -> &'static str {
    match kind {
        SecurityEventKind::CorsDenied => "cors_denied",
        SecurityEventKind::RateLimitExceeded => "rate_limit_exceeded",
        SecurityEventKind::AuthenticationFailed => "authentication_failed",
        SecurityEventKind::AuthorizationDenied => "authorization_denied",
        SecurityEventKind::TenantIsolationDenied => "tenant_isolation_denied",
    }
}
