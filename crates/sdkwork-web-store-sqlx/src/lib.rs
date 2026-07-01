//! SQLx store adapters for idempotency, rate limiting, audit, and security events (`web_*` only).
//!
//! Supports both SQLite (default, single-replica) and PostgreSQL (multi-replica HA).
//! Use feature flags `sqlite` (default) or `postgres` to select the backend.

mod audit;
mod bootstrap;
mod cors_policy;
mod idempotency;
pub mod pool;
mod purge;
mod rate_limit;
mod rate_limit_policy;
mod security_events;
mod tenant_runtime;

pub use audit::SqlxAuditEmitter;
pub use cors_policy::SqlxCorsPolicySource;
pub use idempotency::SqlxIdempotencyStore;
pub use pool::WebStorePool;
pub use rate_limit::SqlxRateLimitStore;
pub use rate_limit_policy::SqlxRateLimitPolicySource;
pub use security_events::SqlxSecurityEventEmitter;
pub use tenant_runtime::SqlxTenantRuntimeProfileSource;

pub use bootstrap::{
    bootstrap_webstore_database, bootstrap_webstore_database_from_env,
    connect_and_bootstrap_webstore_database_from_env, connect_webstore_database_pool_from_env,
    WebStoreDatabaseHost, WebStoreDatabasePool,
};

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine, DeploymentMode};
use sdkwork_database_sqlx::PoolBuilder;
use sdkwork_web_core::{
    CachingDynamicCorsPolicySource, CachingDynamicRateLimitPolicySource,
    CachingDynamicTenantRuntimeProfileSource, DynamicPolicyCaches, DYNAMIC_POLICY_CACHE_TTL_SECS,
};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;

/// Open a SQLite pool through `sdkwork-database-sqlx`, run embedded migrations, and return it.
pub async fn connect_sqlite(
    database_url: &str,
    max_connections: u32,
) -> Result<SqlitePool, sqlx::Error> {
    let mut config = DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: database_url.to_string(),
        mode: DeploymentMode::Standalone,
        max_connections: max_connections.max(1),
        ..DatabaseConfig::default()
    };
    config.sqlite.create_if_missing = true;

    let db_pool = PoolBuilder::new(config)
        .build()
        .await
        .map_err(|error| sqlx::Error::Configuration(error.to_string().into()))?;

    let pool = db_pool
        .as_sqlite()
        .cloned()
        .ok_or_else(|| sqlx::Error::Configuration("expected sqlite pool".into()))?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

// ---- Shared factory functions (SQLite) ----

pub fn shared_idempotency_store(
    pool: SqlitePool,
) -> std::sync::Arc<dyn sdkwork_web_core::IdempotencyStore> {
    std::sync::Arc::new(SqlxIdempotencyStore::new_sqlite(pool))
}

pub fn shared_rate_limit_store(
    pool: SqlitePool,
) -> std::sync::Arc<dyn sdkwork_web_core::RateLimitStore> {
    std::sync::Arc::new(SqlxRateLimitStore::new_sqlite(pool))
}

pub fn shared_security_event_emitter(
    pool: SqlitePool,
) -> std::sync::Arc<dyn sdkwork_web_core::SecurityEventEmitter> {
    std::sync::Arc::new(SqlxSecurityEventEmitter::new_sqlite(pool))
}

pub fn shared_audit_emitter(
    pool: SqlitePool,
) -> std::sync::Arc<dyn sdkwork_web_core::AuditEmitter> {
    std::sync::Arc::new(SqlxAuditEmitter::new_sqlite(pool))
}

pub fn shared_cors_policy_source(
    pool: SqlitePool,
) -> std::sync::Arc<dyn sdkwork_web_core::DynamicCorsPolicySource> {
    std::sync::Arc::new(SqlxCorsPolicySource::new_sqlite(pool))
}

pub fn shared_rate_limit_policy_source(
    pool: SqlitePool,
) -> std::sync::Arc<dyn sdkwork_web_core::DynamicRateLimitPolicySource> {
    std::sync::Arc::new(SqlxRateLimitPolicySource::new_sqlite(pool))
}

pub fn shared_tenant_runtime_profile_source(
    pool: SqlitePool,
) -> std::sync::Arc<dyn sdkwork_web_core::DynamicTenantRuntimeProfileSource> {
    std::sync::Arc::new(SqlxTenantRuntimeProfileSource::new_sqlite(pool))
}

// ---- PostgreSQL shared factories (feature-gated) ----

#[cfg(feature = "postgres")]
pub fn shared_idempotency_store_pg(
    pool: sqlx::PgPool,
) -> std::sync::Arc<dyn sdkwork_web_core::IdempotencyStore> {
    std::sync::Arc::new(SqlxIdempotencyStore::new_postgres(pool))
}

#[cfg(feature = "postgres")]
pub fn shared_rate_limit_store_pg(
    pool: sqlx::PgPool,
) -> std::sync::Arc<dyn sdkwork_web_core::RateLimitStore> {
    std::sync::Arc::new(SqlxRateLimitStore::new_postgres(pool))
}

#[cfg(feature = "postgres")]
pub fn shared_security_event_emitter_pg(
    pool: sqlx::PgPool,
) -> std::sync::Arc<dyn sdkwork_web_core::SecurityEventEmitter> {
    std::sync::Arc::new(SqlxSecurityEventEmitter::new_postgres(pool))
}

#[cfg(feature = "postgres")]
pub fn shared_audit_emitter_pg(
    pool: sqlx::PgPool,
) -> std::sync::Arc<dyn sdkwork_web_core::AuditEmitter> {
    std::sync::Arc::new(SqlxAuditEmitter::new_postgres(pool))
}

#[cfg(feature = "postgres")]
pub fn shared_cors_policy_source_pg(
    pool: sqlx::PgPool,
) -> std::sync::Arc<dyn sdkwork_web_core::DynamicCorsPolicySource> {
    std::sync::Arc::new(SqlxCorsPolicySource::new_postgres(pool))
}

#[cfg(feature = "postgres")]
pub fn shared_rate_limit_policy_source_pg(
    pool: sqlx::PgPool,
) -> std::sync::Arc<dyn sdkwork_web_core::DynamicRateLimitPolicySource> {
    std::sync::Arc::new(SqlxRateLimitPolicySource::new_postgres(pool))
}

#[cfg(feature = "postgres")]
pub fn shared_tenant_runtime_profile_source_pg(
    pool: sqlx::PgPool,
) -> std::sync::Arc<dyn sdkwork_web_core::DynamicTenantRuntimeProfileSource> {
    std::sync::Arc::new(SqlxTenantRuntimeProfileSource::new_postgres(pool))
}

/// Cached SQLx dynamic policy sources plus shared invalidation handles for admin upserts.
pub struct SqlxDynamicPolicyBundle {
    pub caches: Arc<DynamicPolicyCaches>,
    pub cors_policy_source: Arc<dyn sdkwork_web_core::DynamicCorsPolicySource>,
    pub rate_limit_policy_source: Arc<dyn sdkwork_web_core::DynamicRateLimitPolicySource>,
    pub tenant_runtime_profile_source: Arc<dyn sdkwork_web_core::DynamicTenantRuntimeProfileSource>,
}

pub fn shared_dynamic_policy_bundle(pool: SqlitePool) -> SqlxDynamicPolicyBundle {
    let caches = Arc::new(DynamicPolicyCaches::new(Duration::from_secs(
        DYNAMIC_POLICY_CACHE_TTL_SECS,
    )));
    let cors_policy_source = Arc::new(CachingDynamicCorsPolicySource::new(
        Arc::new(SqlxCorsPolicySource::new_sqlite(pool.clone())),
        caches.cors(),
    ));
    let rate_limit_policy_source = Arc::new(CachingDynamicRateLimitPolicySource::new(
        Arc::new(SqlxRateLimitPolicySource::new_sqlite(pool.clone())),
        caches.rate_limit(),
    ));
    let tenant_runtime_profile_source = Arc::new(CachingDynamicTenantRuntimeProfileSource::new(
        Arc::new(SqlxTenantRuntimeProfileSource::new_sqlite(pool)),
        caches.tenant_profile(),
    ));
    SqlxDynamicPolicyBundle {
        caches,
        cors_policy_source,
        rate_limit_policy_source,
        tenant_runtime_profile_source,
    }
}

#[cfg(feature = "postgres")]
pub fn shared_dynamic_policy_bundle_pg(pool: sqlx::PgPool) -> SqlxDynamicPolicyBundle {
    let caches = Arc::new(DynamicPolicyCaches::new(Duration::from_secs(
        DYNAMIC_POLICY_CACHE_TTL_SECS,
    )));
    let cors_policy_source = Arc::new(CachingDynamicCorsPolicySource::new(
        Arc::new(SqlxCorsPolicySource::new_postgres(pool.clone())),
        caches.cors(),
    ));
    let rate_limit_policy_source = Arc::new(CachingDynamicRateLimitPolicySource::new(
        Arc::new(SqlxRateLimitPolicySource::new_postgres(pool.clone())),
        caches.rate_limit(),
    ));
    let tenant_runtime_profile_source = Arc::new(CachingDynamicTenantRuntimeProfileSource::new(
        Arc::new(SqlxTenantRuntimeProfileSource::new_postgres(pool)),
        caches.tenant_profile(),
    ));
    SqlxDynamicPolicyBundle {
        caches,
        cors_policy_source,
        rate_limit_policy_source,
        tenant_runtime_profile_source,
    }
}

pub(crate) fn ttl_epoch_secs(ttl: Duration) -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_secs() as i64;
    now + ttl.as_secs().max(1) as i64
}

pub(crate) fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_web_core::{
        AuditEmitter, AuditFact, DynamicCorsPolicySource, DynamicRateLimitPolicySource,
        DynamicTenantRuntimeProfileSource, IdempotencyBeginOutcome, IdempotencyResponseRecord,
        IdempotencyStore, RateLimitStore, RateLimitTier, SecurityEvent, SecurityEventEmitter,
        SecurityEventKind, WebApiSurface,
    };

    async fn test_pool() -> SqlitePool {
        connect_sqlite("sqlite::memory:", 1)
            .await
            .expect("sqlite pool")
    }

    #[tokio::test]
    async fn sqlx_idempotency_replays_completed_response() {
        let pool = test_pool().await;
        let store = SqlxIdempotencyStore::new_sqlite(pool);
        let ttl = Duration::from_secs(60);
        assert!(matches!(
            store.begin("k1", "fp", ttl).await.expect("leader"),
            IdempotencyBeginOutcome::Leader
        ));
        store
            .complete(
                "k1",
                "fp",
                IdempotencyResponseRecord {
                    status_code: 200,
                    body: br#"{"ok":true}"#.to_vec(),
                    content_type: Some("application/json".to_owned()),
                },
                ttl,
            )
            .await
            .expect("complete");
        assert!(matches!(
            store.begin("k1", "fp", ttl).await.expect("replay"),
            IdempotencyBeginOutcome::Replay(_)
        ));
    }

    #[tokio::test]
    async fn sqlx_idempotency_release_allows_retry() {
        let pool = test_pool().await;
        let store = SqlxIdempotencyStore::new_sqlite(pool);
        let ttl = Duration::from_secs(60);
        store.begin("k2", "fp", ttl).await.expect("leader");
        store.release("k2", "fp").await.expect("release");
        assert!(matches!(
            store.begin("k2", "fp", ttl).await.expect("leader again"),
            IdempotencyBeginOutcome::Leader
        ));
    }

    #[tokio::test]
    async fn sqlx_security_event_emitter_persists_rows() {
        let pool = test_pool().await;
        let emitter = SqlxSecurityEventEmitter::new_sqlite(pool.clone());
        emitter
            .emit(SecurityEvent {
                kind: SecurityEventKind::CorsDenied,
                request_id: Some("req-1".to_owned()),
                tenant_id: Some("100001".to_owned()),
                path: "/app/v3/api/users".to_owned(),
                method: "POST".to_owned(),
                api_surface: WebApiSurface::AppApi,
                origin: Some("https://evil.example".to_owned()),
                detail: "denied".to_owned(),
            })
            .await
            .expect("emit");
        let row: (i64, Option<String>) =
            sqlx::query_as("SELECT COUNT(*), MAX(tenant_id) FROM web_security_event")
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(1, row.0);
        assert_eq!(Some("100001".to_owned()), row.1);
    }

    #[tokio::test]
    async fn sqlx_rate_limit_store_enforces_window() {
        let pool = test_pool().await;
        let store = SqlxRateLimitStore::new_sqlite(pool);
        let window = Duration::from_secs(60);
        store
            .check_and_record("tenant:1", 2, window)
            .await
            .expect("first");
        store
            .check_and_record("tenant:1", 2, window)
            .await
            .expect("second");
        let error = store
            .check_and_record("tenant:1", 2, window)
            .await
            .expect_err("third exceeds limit");
        assert_eq!(
            sdkwork_web_core::WebFrameworkErrorKind::RateLimitExceeded,
            error.kind
        );
    }

    #[tokio::test]
    async fn sqlx_audit_emitter_persists_rows() {
        let pool = test_pool().await;
        let emitter = SqlxAuditEmitter::new_sqlite(pool.clone());
        emitter
            .emit(AuditFact {
                request_id: "req-audit-1".to_owned(),
                tenant_id: Some("100001".to_owned()),
                user_id: Some("user-1".to_owned()),
                api_surface: WebApiSurface::AppApi,
                path: "/app/v3/api/users/:id".to_owned(),
                method: "GET".to_owned(),
                operation_id: Some("listUsers".to_owned()),
                status_code: Some(200),
                duration_ms: Some(12),
            })
            .await
            .expect("emit");
        let row: (String, Option<String>, Option<String>) =
            sqlx::query_as("SELECT request_id, tenant_id, user_id FROM web_audit_event LIMIT 1")
                .fetch_one(&pool)
                .await
                .expect("row");
        assert_eq!("req-audit-1", row.0);
        assert_eq!(Some("100001".to_owned()), row.1);
        assert_eq!(Some("user-1".to_owned()), row.2);
    }

    #[tokio::test]
    async fn sqlx_cors_policy_source_resolves_tenant_overlay() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO web_cors_policy (tenant_id, environment, allow_all_origins, allowed_origins, allow_credentials) \
             VALUES ('100001', 'prod', 0, '[\"https://app.example\"]', 1)",
        )
        .execute(&pool)
        .await
        .expect("seed cors");
        let source = SqlxCorsPolicySource::new_sqlite(pool);
        let policy = source
            .resolve(&sdkwork_web_core::CorsPolicyContext {
                tenant_id: Some("100001".to_owned()),
                environment: sdkwork_web_core::WebEnvironment::Prod,
                api_surface: WebApiSurface::AppApi,
                origin: Some("https://app.example".to_owned()),
            })
            .await
            .expect("resolve")
            .expect("overlay");
        assert!(policy
            .allowed_origins
            .contains(&"https://app.example".to_owned()));
    }

    #[tokio::test]
    async fn sqlx_rate_limit_policy_source_resolves_tenant_tier() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO web_rate_limit_policy (tenant_id, environment, tier_key, max_requests, window_secs, enabled) \
             VALUES ('100001', 'prod', 'auth_critical', 3, 60, 1)",
        )
        .execute(&pool)
        .await
        .expect("seed rate limit policy");
        let source = SqlxRateLimitPolicySource::new_sqlite(pool);
        let policy = source
            .resolve(&sdkwork_web_core::RateLimitPolicyContext {
                tenant_id: Some("100001".to_owned()),
                environment: sdkwork_web_core::WebEnvironment::Prod,
                api_surface: WebApiSurface::AppApi,
                rate_limit_tier: Some(RateLimitTier::AuthCritical),
                operation_id: None,
            })
            .await
            .expect("resolve")
            .expect("overlay");
        assert_eq!(3, policy.max_requests);
    }

    #[tokio::test]
    async fn sqlx_tenant_runtime_profile_source_resolves_overrides() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO web_tenant_runtime_profile (tenant_id, environment, rate_limit_enabled, max_content_length, max_concurrent_requests) \
             VALUES ('100001', 'prod', 0, 4096, 2)",
        )
        .execute(&pool)
        .await
        .expect("seed tenant profile");
        let source = SqlxTenantRuntimeProfileSource::new_sqlite(pool);
        let profile = source
            .resolve(&sdkwork_web_core::TenantRuntimeProfileContext {
                tenant_id: Some("100001".to_owned()),
                environment: sdkwork_web_core::WebEnvironment::Prod,
                api_surface: WebApiSurface::AppApi,
            })
            .await
            .expect("resolve")
            .expect("profile");
        assert_eq!(Some(false), profile.rate_limit_enabled);
        assert_eq!(Some(4096), profile.max_content_length);
        assert_eq!(Some(2), profile.max_concurrent_requests);
    }
}
