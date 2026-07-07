use crate::error::RepositoryError;
use crate::models::{
    AuditEventListScope, AuditEventRecord, ControlNodeRecord, CorsPolicyRecord,
    RateLimitPolicyRecord, RegisterControlNodeRecord, SecurityEventListScope, SecurityEventRecord,
    TenantRuntimeProfileRecord, UpsertCorsPolicyRecord, UpsertRateLimitPolicyRecord,
    UpsertTenantRuntimeProfileRecord,
};
use crate::pagination::{RepoKeysetPage, RepoOffsetPage};
use crate::pool::AdminStorePool;
use async_trait::async_trait;
use sdkwork_utils_rust::OffsetListPageParams;

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    tracing::error!(%error, "database operation failed");
    RepositoryError::Database("database operation failed".to_owned())
}

fn map_stored_json_error(error: serde_json::Error) -> RepositoryError {
    tracing::error!(%error, "stored configuration payload is corrupt");
    RepositoryError::StoredJson("stored configuration payload is corrupt".to_owned())
}

fn parse_allowed_origins_json(json: &str) -> Result<Vec<String>, RepositoryError> {
    serde_json::from_str(json).map_err(map_stored_json_error)
}

#[async_trait]
pub trait WebFrameworkAdminRepository: Send + Sync {
    async fn list_cors_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        params: OffsetListPageParams,
    ) -> Result<RepoOffsetPage<CorsPolicyRecord>, RepositoryError>;

    async fn upsert_cors_policy(
        &self,
        body: UpsertCorsPolicyRecord,
    ) -> Result<CorsPolicyRecord, RepositoryError>;

    async fn list_rate_limit_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        params: OffsetListPageParams,
    ) -> Result<RepoOffsetPage<RateLimitPolicyRecord>, RepositoryError>;

    async fn upsert_rate_limit_policy(
        &self,
        body: UpsertRateLimitPolicyRecord,
    ) -> Result<RateLimitPolicyRecord, RepositoryError>;

    async fn list_tenant_runtime_profiles(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        params: OffsetListPageParams,
    ) -> Result<RepoOffsetPage<TenantRuntimeProfileRecord>, RepositoryError>;

    async fn upsert_tenant_runtime_profile(
        &self,
        body: UpsertTenantRuntimeProfileRecord,
    ) -> Result<TenantRuntimeProfileRecord, RepositoryError>;

    /// Lists security events with tenant scoping. `scope` mirrors
    /// [`AuditEventListScope`] semantics for tenant isolation (migration 010).
    async fn list_security_events(
        &self,
        scope: SecurityEventListScope,
        before_id: Option<i64>,
        page_size: u32,
    ) -> Result<RepoKeysetPage<SecurityEventRecord>, RepositoryError>;

    async fn list_audit_events(
        &self,
        scope: AuditEventListScope,
        before_id: Option<i64>,
        page_size: u32,
    ) -> Result<RepoKeysetPage<AuditEventRecord>, RepositoryError>;

    async fn list_control_nodes(
        &self,
        environment: Option<String>,
        params: OffsetListPageParams,
    ) -> Result<RepoOffsetPage<ControlNodeRecord>, RepositoryError>;

    async fn control_node_exists(&self, node_id: &str) -> Result<bool, RepositoryError>;

    /// Atomically registers or refreshes a control node, returning the record
    /// and a `created` flag (`true` on insert, `false` on conflict-update).
    /// Eliminates the TOCTOU window between `control_node_exists` and insert.
    async fn register_control_node(
        &self,
        body: RegisterControlNodeRecord,
        now: i64,
    ) -> Result<(ControlNodeRecord, bool), RepositoryError>;

    async fn get_control_node(
        &self,
        node_id: &str,
    ) -> Result<Option<ControlNodeRecord>, RepositoryError>;

    async fn heartbeat_control_node(
        &self,
        node_id: &str,
        now: i64,
    ) -> Result<ControlNodeRecord, RepositoryError>;

    async fn delete_control_node(&self, node_id: &str) -> Result<(), RepositoryError>;
}

#[derive(Clone)]
pub struct SqlxWebFrameworkAdminRepository {
    pool: AdminStorePool,
}

impl SqlxWebFrameworkAdminRepository {
    pub fn new(pool: AdminStorePool) -> Self {
        Self { pool }
    }

    pub fn from_sqlite(pool: sqlx::SqlitePool) -> Self {
        Self::new(AdminStorePool::Sqlite(pool))
    }

    #[cfg(feature = "postgres")]
    pub fn from_postgres(pool: sqlx::PgPool) -> Self {
        Self::new(AdminStorePool::Postgres(pool))
    }
}

#[async_trait]
impl WebFrameworkAdminRepository for SqlxWebFrameworkAdminRepository {
    async fn list_cors_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        params: OffsetListPageParams,
    ) -> Result<RepoOffsetPage<CorsPolicyRecord>, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => {
                list_cors_policies_sqlite(pool, tenant_id, environment, params).await
            }
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => {
                list_cors_policies_postgres(pool, tenant_id, environment, params).await
            }
        }
    }

    async fn upsert_cors_policy(
        &self,
        body: UpsertCorsPolicyRecord,
    ) -> Result<CorsPolicyRecord, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => upsert_cors_policy_sqlite(pool, body).await,
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => upsert_cors_policy_postgres(pool, body).await,
        }
    }

    async fn list_rate_limit_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        params: OffsetListPageParams,
    ) -> Result<RepoOffsetPage<RateLimitPolicyRecord>, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => {
                list_rate_limit_policies_sqlite(pool, tenant_id, environment, params).await
            }
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => {
                list_rate_limit_policies_postgres(pool, tenant_id, environment, params).await
            }
        }
    }

    async fn upsert_rate_limit_policy(
        &self,
        body: UpsertRateLimitPolicyRecord,
    ) -> Result<RateLimitPolicyRecord, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => upsert_rate_limit_policy_sqlite(pool, body).await,
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => upsert_rate_limit_policy_postgres(pool, body).await,
        }
    }

    async fn list_tenant_runtime_profiles(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        params: OffsetListPageParams,
    ) -> Result<RepoOffsetPage<TenantRuntimeProfileRecord>, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => {
                list_tenant_runtime_profiles_sqlite(pool, tenant_id, environment, params).await
            }
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => {
                list_tenant_runtime_profiles_postgres(pool, tenant_id, environment, params).await
            }
        }
    }

    async fn upsert_tenant_runtime_profile(
        &self,
        body: UpsertTenantRuntimeProfileRecord,
    ) -> Result<TenantRuntimeProfileRecord, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => upsert_tenant_runtime_profile_sqlite(pool, body).await,
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => {
                upsert_tenant_runtime_profile_postgres(pool, body).await
            }
        }
    }

    async fn list_security_events(
        &self,
        scope: SecurityEventListScope,
        before_id: Option<i64>,
        page_size: u32,
    ) -> Result<RepoKeysetPage<SecurityEventRecord>, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => {
                list_security_events_sqlite(pool, scope, before_id, page_size).await
            }
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => {
                list_security_events_postgres(pool, scope, before_id, page_size).await
            }
        }
    }

    async fn list_audit_events(
        &self,
        scope: AuditEventListScope,
        before_id: Option<i64>,
        page_size: u32,
    ) -> Result<RepoKeysetPage<AuditEventRecord>, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => {
                list_audit_events_sqlite(pool, scope, before_id, page_size).await
            }
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => {
                list_audit_events_postgres(pool, scope, before_id, page_size).await
            }
        }
    }

    async fn list_control_nodes(
        &self,
        environment: Option<String>,
        params: OffsetListPageParams,
    ) -> Result<RepoOffsetPage<ControlNodeRecord>, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => {
                list_control_nodes_sqlite(pool, environment, params).await
            }
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => {
                list_control_nodes_postgres(pool, environment, params).await
            }
        }
    }

    async fn control_node_exists(&self, node_id: &str) -> Result<bool, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => control_node_exists_sqlite(pool, node_id).await,
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => control_node_exists_postgres(pool, node_id).await,
        }
    }

    async fn register_control_node(
        &self,
        body: RegisterControlNodeRecord,
        now: i64,
    ) -> Result<(ControlNodeRecord, bool), RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => register_control_node_sqlite(pool, body, now).await,
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => register_control_node_postgres(pool, body, now).await,
        }
    }

    async fn get_control_node(
        &self,
        node_id: &str,
    ) -> Result<Option<ControlNodeRecord>, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => get_control_node_sqlite(pool, node_id).await,
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => get_control_node_postgres(pool, node_id).await,
        }
    }

    async fn heartbeat_control_node(
        &self,
        node_id: &str,
        now: i64,
    ) -> Result<ControlNodeRecord, RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => heartbeat_control_node_sqlite(pool, node_id, now).await,
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => {
                heartbeat_control_node_postgres(pool, node_id, now).await
            }
        }
    }

    async fn delete_control_node(&self, node_id: &str) -> Result<(), RepositoryError> {
        match &self.pool {
            AdminStorePool::Sqlite(pool) => delete_control_node_sqlite(pool, node_id).await,
            #[cfg(feature = "postgres")]
            AdminStorePool::Postgres(pool) => delete_control_node_postgres(pool, node_id).await,
        }
    }
}

// ---------------------------------------------------------------------------
// SQLite implementations
// ---------------------------------------------------------------------------

async fn list_cors_policies_sqlite(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    environment: Option<String>,
    params: OffsetListPageParams,
) -> Result<RepoOffsetPage<CorsPolicyRecord>, RepositoryError> {
    let total_items = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM web_cors_policy \
         WHERE (?1 IS NULL OR environment = ?1) AND tenant_id = ?2",
    )
    .bind(&environment)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    let rows = sqlx::query_as::<_, (String, String, i64, String, i64, i64)>(
        "SELECT tenant_id, environment, allow_all_origins, allowed_origins, allow_credentials, version \
         FROM web_cors_policy \
         WHERE (?1 IS NULL OR environment = ?1) AND tenant_id = ?2 \
         ORDER BY tenant_id, environment \
         LIMIT ?3 OFFSET ?4",
    )
    .bind(environment)
    .bind(tenant_id)
    .bind(params.page_size)
    .bind(params.offset)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_error)?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(CorsPolicyRecord {
            tenant_id: row.0,
            environment: row.1,
            allow_all_origins: row.2 != 0,
            allowed_origins: parse_allowed_origins_json(&row.3)?,
            allow_credentials: row.4 != 0,
            version: row.5,
        });
    }

    Ok(RepoOffsetPage { items, total_items })
}

async fn upsert_cors_policy_sqlite(
    pool: &sqlx::SqlitePool,
    body: UpsertCorsPolicyRecord,
) -> Result<CorsPolicyRecord, RepositoryError> {
    let origins_json = serde_json::to_string(&body.allowed_origins)
        .map_err(|_| RepositoryError::StoredJson("allowed_origins payload is invalid".into()))?;
    let row = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO web_cors_policy (tenant_id, environment, allow_all_origins, allowed_origins, allow_credentials, version) \
         VALUES (?, ?, ?, ?, ?, 1) \
         ON CONFLICT(tenant_id, environment) DO UPDATE SET \
           allow_all_origins = excluded.allow_all_origins, \
           allowed_origins = excluded.allowed_origins, \
           allow_credentials = excluded.allow_credentials, \
           version = web_cors_policy.version + 1 \
         RETURNING version",
    )
    .bind(&body.tenant_id)
    .bind(&body.environment)
    .bind(i64::from(body.allow_all_origins))
    .bind(&origins_json)
    .bind(i64::from(body.allow_credentials))
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(CorsPolicyRecord {
        tenant_id: body.tenant_id,
        environment: body.environment,
        allow_all_origins: body.allow_all_origins,
        allowed_origins: body.allowed_origins,
        allow_credentials: body.allow_credentials,
        version: row.0,
    })
}

async fn list_rate_limit_policies_sqlite(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    environment: Option<String>,
    params: OffsetListPageParams,
) -> Result<RepoOffsetPage<RateLimitPolicyRecord>, RepositoryError> {
    let total_items = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM web_rate_limit_policy \
         WHERE (?1 IS NULL OR environment = ?1) AND tenant_id = ?2",
    )
    .bind(&environment)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    let rows = sqlx::query_as::<_, (String, String, String, i64, i64, i64, i64)>(
        "SELECT tenant_id, environment, tier_key, max_requests, window_secs, enabled, version \
         FROM web_rate_limit_policy \
         WHERE (?1 IS NULL OR environment = ?1) AND tenant_id = ?2 \
         ORDER BY tenant_id, environment, tier_key \
         LIMIT ?3 OFFSET ?4",
    )
    .bind(environment)
    .bind(tenant_id)
    .bind(params.page_size)
    .bind(params.offset)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_error)?;

    let items = rows
        .into_iter()
        .map(|row| RateLimitPolicyRecord {
            tenant_id: row.0,
            environment: row.1,
            tier_key: row.2,
            max_requests: row.3.max(0) as u32,
            window_secs: row.4.max(1) as u64,
            enabled: row.5 != 0,
            version: row.6,
        })
        .collect();

    Ok(RepoOffsetPage { items, total_items })
}

async fn upsert_rate_limit_policy_sqlite(
    pool: &sqlx::SqlitePool,
    body: UpsertRateLimitPolicyRecord,
) -> Result<RateLimitPolicyRecord, RepositoryError> {
    let row = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO web_rate_limit_policy (tenant_id, environment, tier_key, max_requests, window_secs, enabled, version) \
         VALUES (?, ?, ?, ?, ?, ?, 1) \
         ON CONFLICT(tenant_id, environment, tier_key) DO UPDATE SET \
           max_requests = excluded.max_requests, \
           window_secs = excluded.window_secs, \
           enabled = excluded.enabled, \
           version = web_rate_limit_policy.version + 1 \
         RETURNING version",
    )
    .bind(&body.tenant_id)
    .bind(&body.environment)
    .bind(&body.tier_key)
    .bind(i64::from(body.max_requests))
    .bind(body.window_secs as i64)
    .bind(i64::from(body.enabled))
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(RateLimitPolicyRecord {
        tenant_id: body.tenant_id,
        environment: body.environment,
        tier_key: body.tier_key,
        max_requests: body.max_requests,
        window_secs: body.window_secs,
        enabled: body.enabled,
        version: row.0,
    })
}

async fn list_tenant_runtime_profiles_sqlite(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    environment: Option<String>,
    params: OffsetListPageParams,
) -> Result<RepoOffsetPage<TenantRuntimeProfileRecord>, RepositoryError> {
    let total_items = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM web_tenant_runtime_profile \
         WHERE (?1 IS NULL OR environment = ?1) AND tenant_id = ?2",
    )
    .bind(&environment)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    let rows =
        sqlx::query_as::<_, (String, String, Option<i64>, Option<i64>, Option<i64>, i64)>(
            "SELECT tenant_id, environment, rate_limit_enabled, max_content_length, max_concurrent_requests, version \
             FROM web_tenant_runtime_profile \
             WHERE (?1 IS NULL OR environment = ?1) AND tenant_id = ?2 \
             ORDER BY tenant_id, environment \
             LIMIT ?3 OFFSET ?4",
        )
        .bind(environment)
        .bind(tenant_id)
        .bind(params.page_size)
        .bind(params.offset)
        .fetch_all(pool)
        .await
        .map_err(map_sqlx_error)?;

    let items = rows
        .into_iter()
        .map(|row| TenantRuntimeProfileRecord {
            tenant_id: row.0,
            environment: row.1,
            rate_limit_enabled: row.2.map(|value| value != 0),
            max_content_length: row.3,
            max_concurrent_requests: row.4.and_then(|value| u32::try_from(value.max(0)).ok()),
            version: row.5,
        })
        .collect();

    Ok(RepoOffsetPage { items, total_items })
}

async fn upsert_tenant_runtime_profile_sqlite(
    pool: &sqlx::SqlitePool,
    body: UpsertTenantRuntimeProfileRecord,
) -> Result<TenantRuntimeProfileRecord, RepositoryError> {
    let rate_limit = body.rate_limit_enabled.map(i64::from);
    let max_concurrent = body.max_concurrent_requests.map(|value| value as i64);
    let row = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO web_tenant_runtime_profile (tenant_id, environment, rate_limit_enabled, max_content_length, max_concurrent_requests, version) \
         VALUES (?, ?, ?, ?, ?, 1) \
         ON CONFLICT(tenant_id, environment) DO UPDATE SET \
           rate_limit_enabled = excluded.rate_limit_enabled, \
           max_content_length = excluded.max_content_length, \
           max_concurrent_requests = excluded.max_concurrent_requests, \
           version = web_tenant_runtime_profile.version + 1 \
         RETURNING version",
    )
    .bind(&body.tenant_id)
    .bind(&body.environment)
    .bind(rate_limit)
    .bind(body.max_content_length)
    .bind(max_concurrent)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(TenantRuntimeProfileRecord {
        tenant_id: body.tenant_id,
        environment: body.environment,
        rate_limit_enabled: body.rate_limit_enabled,
        max_content_length: body.max_content_length,
        max_concurrent_requests: body.max_concurrent_requests,
        version: row.0,
    })
}

type SecurityEventRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    String,
    i64,
);

fn map_security_event_row(row: SecurityEventRow) -> SecurityEventRecord {
    SecurityEventRecord {
        id: row.0,
        kind: row.1,
        request_id: row.2,
        tenant_id: row.3,
        path: row.4,
        method: row.5,
        api_surface: row.6,
        origin: row.7,
        detail: row.8,
        created_at: row.9,
    }
}

async fn list_security_events_sqlite(
    pool: &sqlx::SqlitePool,
    scope: SecurityEventListScope,
    before_id: Option<i64>,
    page_size: u32,
) -> Result<RepoKeysetPage<SecurityEventRecord>, RepositoryError> {
    let fetch_limit = i64::from(page_size) + 1;
    let rows = match (scope, before_id) {
        (SecurityEventListScope::Tenant(tenant_id), Some(before)) => {
            sqlx::query_as::<_, SecurityEventRow>(
                "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                 FROM web_security_event \
                 WHERE tenant_id = ?1 AND id < ?2 \
                 ORDER BY id DESC LIMIT ?3",
            )
            .bind(&tenant_id)
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (SecurityEventListScope::Tenant(tenant_id), None) => {
            sqlx::query_as::<_, SecurityEventRow>(
                "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                 FROM web_security_event \
                 WHERE tenant_id = ?1 \
                 ORDER BY id DESC LIMIT ?2",
            )
            .bind(&tenant_id)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (SecurityEventListScope::PlatformAll, Some(before)) => {
            sqlx::query_as::<_, SecurityEventRow>(
                "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                 FROM web_security_event \
                 WHERE id < ?1 \
                 ORDER BY id DESC LIMIT ?2",
            )
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (SecurityEventListScope::PlatformAll, None) => {
            sqlx::query_as::<_, SecurityEventRow>(
                "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                 FROM web_security_event \
                 ORDER BY id DESC LIMIT ?1",
            )
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
    }
    .map_err(map_sqlx_error)?;

    let items = rows.into_iter().map(map_security_event_row).collect();
    Ok(RepoKeysetPage::from_limit_plus_one(
        items,
        page_size as usize,
    ))
}

type AuditEventRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<i64>,
    i64,
);

fn map_audit_event_row(row: AuditEventRow) -> AuditEventRecord {
    AuditEventRecord {
        id: row.0,
        request_id: row.1,
        tenant_id: row.2,
        user_id: row.3,
        api_surface: row.4,
        path: row.5,
        method: row.6,
        operation_id: row.7,
        status_code: row.8,
        duration_ms: row.9,
        created_at: row.10,
    }
}

async fn list_audit_events_sqlite(
    pool: &sqlx::SqlitePool,
    scope: AuditEventListScope,
    before_id: Option<i64>,
    page_size: u32,
) -> Result<RepoKeysetPage<AuditEventRecord>, RepositoryError> {
    let fetch_limit = i64::from(page_size) + 1;
    let rows = match (scope, before_id) {
        (AuditEventListScope::Tenant(tenant_id) | AuditEventListScope::PlatformTenant(tenant_id), Some(before)) => {
            sqlx::query_as::<_, AuditEventRow>(
                "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                 FROM web_audit_event \
                 WHERE tenant_id = ?1 AND id < ?2 \
                 ORDER BY id DESC LIMIT ?3",
            )
            .bind(&tenant_id)
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (AuditEventListScope::Tenant(tenant_id) | AuditEventListScope::PlatformTenant(tenant_id), None) => {
            sqlx::query_as::<_, AuditEventRow>(
                "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                 FROM web_audit_event \
                 WHERE tenant_id = ?1 \
                 ORDER BY id DESC LIMIT ?2",
            )
            .bind(&tenant_id)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (AuditEventListScope::PlatformAll, Some(before)) => {
            sqlx::query_as::<_, AuditEventRow>(
                "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                 FROM web_audit_event \
                 WHERE id < ?1 \
                 ORDER BY id DESC LIMIT ?2",
            )
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (AuditEventListScope::PlatformAll, None) => {
            sqlx::query_as::<_, AuditEventRow>(
                "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                 FROM web_audit_event \
                 ORDER BY id DESC LIMIT ?1",
            )
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
    }
    .map_err(map_sqlx_error)?;

    let items = rows.into_iter().map(map_audit_event_row).collect();
    Ok(RepoKeysetPage::from_limit_plus_one(
        items,
        page_size as usize,
    ))
}

type ControlNodeRow = (
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    i64,
    i64,
);

fn map_control_node_row(row: ControlNodeRow) -> ControlNodeRecord {
    ControlNodeRecord {
        node_id: row.0,
        region: row.1,
        base_url: row.2,
        environment: row.3,
        status: row.4,
        last_heartbeat_at: row.5,
        created_at: row.6,
        updated_at: row.7,
    }
}

async fn list_control_nodes_sqlite(
    pool: &sqlx::SqlitePool,
    environment: Option<String>,
    params: OffsetListPageParams,
) -> Result<RepoOffsetPage<ControlNodeRecord>, RepositoryError> {
    let total_items = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM web_control_node WHERE (?1 IS NULL OR environment = ?1)",
    )
    .bind(&environment)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    let rows = sqlx::query_as::<_, ControlNodeRow>(
        "SELECT node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at \
         FROM web_control_node \
         WHERE (?1 IS NULL OR environment = ?1) \
         ORDER BY region, node_id \
         LIMIT ?2 OFFSET ?3",
    )
    .bind(environment)
    .bind(params.page_size)
    .bind(params.offset)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_error)?;

    let items = rows.into_iter().map(map_control_node_row).collect();
    Ok(RepoOffsetPage { items, total_items })
}

async fn control_node_exists_sqlite(
    pool: &sqlx::SqlitePool,
    node_id: &str,
) -> Result<bool, RepositoryError> {
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM web_control_node WHERE node_id = ?")
            .bind(node_id)
            .fetch_one(pool)
            .await
            .map_err(map_sqlx_error)?;
    Ok(count > 0)
}

async fn register_control_node_sqlite(
    pool: &sqlx::SqlitePool,
    body: RegisterControlNodeRecord,
    now: i64,
) -> Result<(ControlNodeRecord, bool), RepositoryError> {
    let inserted = sqlx::query_as::<_, ControlNodeRow>(
        "INSERT INTO web_control_node (node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 'registered', ?, ?, ?) \
         ON CONFLICT(node_id) DO NOTHING \
         RETURNING node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at",
    )
    .bind(&body.node_id)
    .bind(&body.region)
    .bind(&body.base_url)
    .bind(&body.environment)
    .bind(now)
    .bind(now)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx_error)?;

    if let Some(row) = inserted {
        return Ok((map_control_node_row(row), true));
    }

    let row = sqlx::query_as::<_, ControlNodeRow>(
        "UPDATE web_control_node SET \
           region = ?, \
           base_url = ?, \
           environment = ?, \
           status = 'registered', \
           last_heartbeat_at = ?, \
           updated_at = ? \
         WHERE node_id = ? \
         RETURNING node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at",
    )
    .bind(&body.region)
    .bind(&body.base_url)
    .bind(&body.environment)
    .bind(now)
    .bind(now)
    .bind(&body.node_id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx_error)?
    .ok_or_else(|| {
        RepositoryError::Database("control node missing after register update".into())
    })?;

    Ok((map_control_node_row(row), false))
}

async fn get_control_node_sqlite(
    pool: &sqlx::SqlitePool,
    node_id: &str,
) -> Result<Option<ControlNodeRecord>, RepositoryError> {
    let row = sqlx::query_as::<_, ControlNodeRow>(
        "SELECT node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at \
         FROM web_control_node WHERE node_id = ?",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx_error)?;
    Ok(row.map(map_control_node_row))
}

async fn heartbeat_control_node_sqlite(
    pool: &sqlx::SqlitePool,
    node_id: &str,
    now: i64,
) -> Result<ControlNodeRecord, RepositoryError> {
    let updated = sqlx::query(
        "UPDATE web_control_node SET status = 'online', last_heartbeat_at = ?, updated_at = ? WHERE node_id = ?",
    )
    .bind(now)
    .bind(now)
    .bind(node_id)
    .execute(pool)
    .await
    .map_err(map_sqlx_error)?;

    if updated.rows_affected() == 0 {
        return Err(RepositoryError::Database(format!(
            "control node {node_id} not found"
        )));
    }

    get_control_node_sqlite(pool, node_id)
        .await?
        .ok_or_else(|| RepositoryError::Database(format!("control node {node_id} not found")))
}

async fn delete_control_node_sqlite(
    pool: &sqlx::SqlitePool,
    node_id: &str,
) -> Result<(), RepositoryError> {
    let result = sqlx::query("DELETE FROM web_control_node WHERE node_id = ?")
        .bind(node_id)
        .execute(pool)
        .await
        .map_err(map_sqlx_error)?;
    if result.rows_affected() == 0 {
        return Err(RepositoryError::Database(format!(
            "control node {node_id} not found"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// PostgreSQL implementations
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
async fn list_cors_policies_postgres(
    pool: &sqlx::PgPool,
    tenant_id: &str,
    environment: Option<String>,
    params: OffsetListPageParams,
) -> Result<RepoOffsetPage<CorsPolicyRecord>, RepositoryError> {
    let total_items = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM web_cors_policy \
         WHERE ($1::text IS NULL OR environment = $1) AND tenant_id = $2",
    )
    .bind(&environment)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    let rows = sqlx::query_as::<_, (String, String, i64, String, i64, i64)>(
        "SELECT tenant_id, environment, allow_all_origins, allowed_origins, allow_credentials, version \
         FROM web_cors_policy \
         WHERE ($1::text IS NULL OR environment = $1) AND tenant_id = $2 \
         ORDER BY tenant_id, environment \
         LIMIT $3 OFFSET $4",
    )
    .bind(environment)
    .bind(tenant_id)
    .bind(params.page_size)
    .bind(params.offset)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_error)?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(CorsPolicyRecord {
            tenant_id: row.0,
            environment: row.1,
            allow_all_origins: row.2 != 0,
            allowed_origins: parse_allowed_origins_json(&row.3)?,
            allow_credentials: row.4 != 0,
            version: row.5,
        });
    }

    Ok(RepoOffsetPage { items, total_items })
}

#[cfg(feature = "postgres")]
async fn upsert_cors_policy_postgres(
    pool: &sqlx::PgPool,
    body: UpsertCorsPolicyRecord,
) -> Result<CorsPolicyRecord, RepositoryError> {
    let origins_json = serde_json::to_string(&body.allowed_origins)
        .map_err(|_| RepositoryError::StoredJson("allowed_origins payload is invalid".into()))?;
    let row = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO web_cors_policy (tenant_id, environment, allow_all_origins, allowed_origins, allow_credentials, version) \
         VALUES ($1, $2, $3, $4, $5, 1) \
         ON CONFLICT(tenant_id, environment) DO UPDATE SET \
           allow_all_origins = excluded.allow_all_origins, \
           allowed_origins = excluded.allowed_origins, \
           allow_credentials = excluded.allow_credentials, \
           version = web_cors_policy.version + 1 \
         RETURNING version",
    )
    .bind(&body.tenant_id)
    .bind(&body.environment)
    .bind(i64::from(body.allow_all_origins))
    .bind(&origins_json)
    .bind(i64::from(body.allow_credentials))
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(CorsPolicyRecord {
        tenant_id: body.tenant_id,
        environment: body.environment,
        allow_all_origins: body.allow_all_origins,
        allowed_origins: body.allowed_origins,
        allow_credentials: body.allow_credentials,
        version: row.0,
    })
}

#[cfg(feature = "postgres")]
async fn list_rate_limit_policies_postgres(
    pool: &sqlx::PgPool,
    tenant_id: &str,
    environment: Option<String>,
    params: OffsetListPageParams,
) -> Result<RepoOffsetPage<RateLimitPolicyRecord>, RepositoryError> {
    let total_items = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM web_rate_limit_policy \
         WHERE ($1::text IS NULL OR environment = $1) AND tenant_id = $2",
    )
    .bind(&environment)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    let rows = sqlx::query_as::<_, (String, String, String, i64, i64, i64, i64)>(
        "SELECT tenant_id, environment, tier_key, max_requests, window_secs, enabled, version \
         FROM web_rate_limit_policy \
         WHERE ($1::text IS NULL OR environment = $1) AND tenant_id = $2 \
         ORDER BY tenant_id, environment, tier_key \
         LIMIT $3 OFFSET $4",
    )
    .bind(environment)
    .bind(tenant_id)
    .bind(params.page_size)
    .bind(params.offset)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_error)?;

    let items = rows
        .into_iter()
        .map(|row| RateLimitPolicyRecord {
            tenant_id: row.0,
            environment: row.1,
            tier_key: row.2,
            max_requests: row.3.max(0) as u32,
            window_secs: row.4.max(1) as u64,
            enabled: row.5 != 0,
            version: row.6,
        })
        .collect();

    Ok(RepoOffsetPage { items, total_items })
}

#[cfg(feature = "postgres")]
async fn upsert_rate_limit_policy_postgres(
    pool: &sqlx::PgPool,
    body: UpsertRateLimitPolicyRecord,
) -> Result<RateLimitPolicyRecord, RepositoryError> {
    let row = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO web_rate_limit_policy (tenant_id, environment, tier_key, max_requests, window_secs, enabled, version) \
         VALUES ($1, $2, $3, $4, $5, $6, 1) \
         ON CONFLICT(tenant_id, environment, tier_key) DO UPDATE SET \
           max_requests = excluded.max_requests, \
           window_secs = excluded.window_secs, \
           enabled = excluded.enabled, \
           version = web_rate_limit_policy.version + 1 \
         RETURNING version",
    )
    .bind(&body.tenant_id)
    .bind(&body.environment)
    .bind(&body.tier_key)
    .bind(i64::from(body.max_requests))
    .bind(body.window_secs as i64)
    .bind(i64::from(body.enabled))
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(RateLimitPolicyRecord {
        tenant_id: body.tenant_id,
        environment: body.environment,
        tier_key: body.tier_key,
        max_requests: body.max_requests,
        window_secs: body.window_secs,
        enabled: body.enabled,
        version: row.0,
    })
}

#[cfg(feature = "postgres")]
async fn list_tenant_runtime_profiles_postgres(
    pool: &sqlx::PgPool,
    tenant_id: &str,
    environment: Option<String>,
    params: OffsetListPageParams,
) -> Result<RepoOffsetPage<TenantRuntimeProfileRecord>, RepositoryError> {
    let total_items = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM web_tenant_runtime_profile \
         WHERE ($1::text IS NULL OR environment = $1) AND tenant_id = $2",
    )
    .bind(&environment)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    let rows =
        sqlx::query_as::<_, (String, String, Option<i64>, Option<i64>, Option<i64>, i64)>(
            "SELECT tenant_id, environment, rate_limit_enabled, max_content_length, max_concurrent_requests, version \
             FROM web_tenant_runtime_profile \
             WHERE ($1::text IS NULL OR environment = $1) AND tenant_id = $2 \
             ORDER BY tenant_id, environment \
             LIMIT $3 OFFSET $4",
        )
        .bind(environment)
        .bind(tenant_id)
        .bind(params.page_size)
        .bind(params.offset)
        .fetch_all(pool)
        .await
        .map_err(map_sqlx_error)?;

    let items = rows
        .into_iter()
        .map(|row| TenantRuntimeProfileRecord {
            tenant_id: row.0,
            environment: row.1,
            rate_limit_enabled: row.2.map(|value| value != 0),
            max_content_length: row.3,
            max_concurrent_requests: row.4.and_then(|value| u32::try_from(value.max(0)).ok()),
            version: row.5,
        })
        .collect();

    Ok(RepoOffsetPage { items, total_items })
}

#[cfg(feature = "postgres")]
async fn upsert_tenant_runtime_profile_postgres(
    pool: &sqlx::PgPool,
    body: UpsertTenantRuntimeProfileRecord,
) -> Result<TenantRuntimeProfileRecord, RepositoryError> {
    let rate_limit = body.rate_limit_enabled.map(i64::from);
    let max_concurrent = body.max_concurrent_requests.map(|value| value as i64);
    let row = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO web_tenant_runtime_profile (tenant_id, environment, rate_limit_enabled, max_content_length, max_concurrent_requests, version) \
         VALUES ($1, $2, $3, $4, $5, 1) \
         ON CONFLICT(tenant_id, environment) DO UPDATE SET \
           rate_limit_enabled = excluded.rate_limit_enabled, \
           max_content_length = excluded.max_content_length, \
           max_concurrent_requests = excluded.max_concurrent_requests, \
           version = web_tenant_runtime_profile.version + 1 \
         RETURNING version",
    )
    .bind(&body.tenant_id)
    .bind(&body.environment)
    .bind(rate_limit)
    .bind(body.max_content_length)
    .bind(max_concurrent)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(TenantRuntimeProfileRecord {
        tenant_id: body.tenant_id,
        environment: body.environment,
        rate_limit_enabled: body.rate_limit_enabled,
        max_content_length: body.max_content_length,
        max_concurrent_requests: body.max_concurrent_requests,
        version: row.0,
    })
}

#[cfg(feature = "postgres")]
async fn list_security_events_postgres(
    pool: &sqlx::PgPool,
    scope: SecurityEventListScope,
    before_id: Option<i64>,
    page_size: u32,
) -> Result<RepoKeysetPage<SecurityEventRecord>, RepositoryError> {
    let fetch_limit = i64::from(page_size) + 1;
    let rows = match (scope, before_id) {
        (SecurityEventListScope::Tenant(tenant_id), Some(before)) => {
            sqlx::query_as::<_, SecurityEventRow>(
                "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                 FROM web_security_event \
                 WHERE tenant_id = $1 AND id < $2 \
                 ORDER BY id DESC LIMIT $3",
            )
            .bind(&tenant_id)
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (SecurityEventListScope::Tenant(tenant_id), None) => {
            sqlx::query_as::<_, SecurityEventRow>(
                "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                 FROM web_security_event \
                 WHERE tenant_id = $1 \
                 ORDER BY id DESC LIMIT $2",
            )
            .bind(&tenant_id)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (SecurityEventListScope::PlatformAll, Some(before)) => {
            sqlx::query_as::<_, SecurityEventRow>(
                "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                 FROM web_security_event \
                 WHERE id < $1 \
                 ORDER BY id DESC LIMIT $2",
            )
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (SecurityEventListScope::PlatformAll, None) => {
            sqlx::query_as::<_, SecurityEventRow>(
                "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                 FROM web_security_event \
                 ORDER BY id DESC LIMIT $1",
            )
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
    }
    .map_err(map_sqlx_error)?;

    let items = rows.into_iter().map(map_security_event_row).collect();
    Ok(RepoKeysetPage::from_limit_plus_one(
        items,
        page_size as usize,
    ))
}

#[cfg(feature = "postgres")]
async fn list_audit_events_postgres(
    pool: &sqlx::PgPool,
    scope: AuditEventListScope,
    before_id: Option<i64>,
    page_size: u32,
) -> Result<RepoKeysetPage<AuditEventRecord>, RepositoryError> {
    let fetch_limit = i64::from(page_size) + 1;
    let rows = match (scope, before_id) {
        (AuditEventListScope::Tenant(tenant_id) | AuditEventListScope::PlatformTenant(tenant_id), Some(before)) => {
            sqlx::query_as::<_, AuditEventRow>(
                "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                 FROM web_audit_event \
                 WHERE tenant_id = $1 AND id < $2 \
                 ORDER BY id DESC LIMIT $3",
            )
            .bind(&tenant_id)
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (AuditEventListScope::Tenant(tenant_id) | AuditEventListScope::PlatformTenant(tenant_id), None) => {
            sqlx::query_as::<_, AuditEventRow>(
                "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                 FROM web_audit_event \
                 WHERE tenant_id = $1 \
                 ORDER BY id DESC LIMIT $2",
            )
            .bind(&tenant_id)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (AuditEventListScope::PlatformAll, Some(before)) => {
            sqlx::query_as::<_, AuditEventRow>(
                "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                 FROM web_audit_event \
                 WHERE id < $1 \
                 ORDER BY id DESC LIMIT $2",
            )
            .bind(before)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
        (AuditEventListScope::PlatformAll, None) => {
            sqlx::query_as::<_, AuditEventRow>(
                "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                 FROM web_audit_event \
                 ORDER BY id DESC LIMIT $1",
            )
            .bind(fetch_limit)
            .fetch_all(pool)
            .await
        }
    }
    .map_err(map_sqlx_error)?;

    let items = rows.into_iter().map(map_audit_event_row).collect();
    Ok(RepoKeysetPage::from_limit_plus_one(
        items,
        page_size as usize,
    ))
}

#[cfg(feature = "postgres")]
async fn list_control_nodes_postgres(
    pool: &sqlx::PgPool,
    environment: Option<String>,
    params: OffsetListPageParams,
) -> Result<RepoOffsetPage<ControlNodeRecord>, RepositoryError> {
    let total_items = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM web_control_node WHERE ($1::text IS NULL OR environment = $1)",
    )
    .bind(&environment)
    .fetch_one(pool)
    .await
    .map_err(map_sqlx_error)?;

    let rows = sqlx::query_as::<_, ControlNodeRow>(
        "SELECT node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at \
         FROM web_control_node \
         WHERE ($1::text IS NULL OR environment = $1) \
         ORDER BY region, node_id \
         LIMIT $2 OFFSET $3",
    )
    .bind(environment)
    .bind(params.page_size)
    .bind(params.offset)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_error)?;

    let items = rows.into_iter().map(map_control_node_row).collect();
    Ok(RepoOffsetPage { items, total_items })
}

#[cfg(feature = "postgres")]
async fn control_node_exists_postgres(
    pool: &sqlx::PgPool,
    node_id: &str,
) -> Result<bool, RepositoryError> {
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM web_control_node WHERE node_id = $1")
            .bind(node_id)
            .fetch_one(pool)
            .await
            .map_err(map_sqlx_error)?;
    Ok(count > 0)
}

#[cfg(feature = "postgres")]
async fn register_control_node_postgres(
    pool: &sqlx::PgPool,
    body: RegisterControlNodeRecord,
    now: i64,
) -> Result<(ControlNodeRecord, bool), RepositoryError> {
    let inserted = sqlx::query_as::<_, ControlNodeRow>(
        "INSERT INTO web_control_node (node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, 'registered', $5, $6, $7) \
         ON CONFLICT(node_id) DO NOTHING \
         RETURNING node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at",
    )
    .bind(&body.node_id)
    .bind(&body.region)
    .bind(&body.base_url)
    .bind(&body.environment)
    .bind(now)
    .bind(now)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx_error)?;

    if let Some(row) = inserted {
        return Ok((map_control_node_row(row), true));
    }

    let row = sqlx::query_as::<_, ControlNodeRow>(
        "UPDATE web_control_node SET \
           region = $1, \
           base_url = $2, \
           environment = $3, \
           status = 'registered', \
           last_heartbeat_at = $4, \
           updated_at = $5 \
         WHERE node_id = $6 \
         RETURNING node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at",
    )
    .bind(&body.region)
    .bind(&body.base_url)
    .bind(&body.environment)
    .bind(now)
    .bind(now)
    .bind(&body.node_id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx_error)?
    .ok_or_else(|| {
        RepositoryError::Database("control node missing after register update".into())
    })?;

    Ok((map_control_node_row(row), false))
}

#[cfg(feature = "postgres")]
async fn get_control_node_postgres(
    pool: &sqlx::PgPool,
    node_id: &str,
) -> Result<Option<ControlNodeRecord>, RepositoryError> {
    let row = sqlx::query_as::<_, ControlNodeRow>(
        "SELECT node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at \
         FROM web_control_node WHERE node_id = $1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx_error)?;
    Ok(row.map(map_control_node_row))
}

#[cfg(feature = "postgres")]
async fn heartbeat_control_node_postgres(
    pool: &sqlx::PgPool,
    node_id: &str,
    now: i64,
) -> Result<ControlNodeRecord, RepositoryError> {
    let updated = sqlx::query(
        "UPDATE web_control_node SET status = 'online', last_heartbeat_at = $1, updated_at = $2 WHERE node_id = $3",
    )
    .bind(now)
    .bind(now)
    .bind(node_id)
    .execute(pool)
    .await
    .map_err(map_sqlx_error)?;

    if updated.rows_affected() == 0 {
        return Err(RepositoryError::Database(format!(
            "control node {node_id} not found"
        )));
    }

    get_control_node_postgres(pool, node_id)
        .await?
        .ok_or_else(|| RepositoryError::Database(format!("control node {node_id} not found")))
}

#[cfg(feature = "postgres")]
async fn delete_control_node_postgres(
    pool: &sqlx::PgPool,
    node_id: &str,
) -> Result<(), RepositoryError> {
    let result = sqlx::query("DELETE FROM web_control_node WHERE node_id = $1")
        .bind(node_id)
        .execute(pool)
        .await
        .map_err(map_sqlx_error)?;
    if result.rows_affected() == 0 {
        return Err(RepositoryError::Database(format!(
            "control node {node_id} not found"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod repository_tests {
    use super::*;
    use sdkwork_web_store_sqlx::connect_sqlite;

    async fn test_repository() -> SqlxWebFrameworkAdminRepository {
        let pool = connect_sqlite("sqlite::memory:", 1)
            .await
            .expect("in-memory sqlite pool");
        SqlxWebFrameworkAdminRepository::from_sqlite(pool)
    }

    #[tokio::test]
    async fn list_cors_policies_returns_empty_for_unknown_tenant() {
        let repo = test_repository().await;
        let page = repo
            .list_cors_policies(
                "199999",
                None,
                OffsetListPageParams::parse(Some(1), Some(10)),
            )
            .await
            .expect("list");
        assert!(page.items.is_empty());
        assert_eq!(0, page.total_items);
    }

    #[tokio::test]
    async fn upsert_cors_policy_round_trips() {
        let repo = test_repository().await;
        let saved = repo
            .upsert_cors_policy(UpsertCorsPolicyRecord {
                tenant_id: "100001".to_owned(),
                environment: "prod".to_owned(),
                allow_all_origins: false,
                allowed_origins: vec!["https://console.example".to_owned()],
                allow_credentials: true,
            })
            .await
            .expect("upsert");
        assert_eq!("100001", saved.tenant_id);
        assert_eq!("prod", saved.environment);
        assert!(!saved.allow_all_origins);
        assert_eq!(
            vec!["https://console.example".to_owned()],
            saved.allowed_origins
        );
        assert_eq!(1, saved.version);

        let page = repo
            .list_cors_policies(
                "100001",
                Some("prod".to_owned()),
                OffsetListPageParams::parse(Some(1), Some(10)),
            )
            .await
            .expect("list");
        assert_eq!(1, page.items.len());
        assert_eq!(1, page.total_items);
        assert_eq!(saved.tenant_id, page.items[0].tenant_id);
        assert_eq!(saved.environment, page.items[0].environment);
        assert_eq!(saved.allow_all_origins, page.items[0].allow_all_origins);
        assert_eq!(saved.allowed_origins, page.items[0].allowed_origins);
        assert_eq!(saved.allow_credentials, page.items[0].allow_credentials);
        assert_eq!(saved.version, page.items[0].version);
    }

    #[tokio::test]
    async fn upsert_cors_policy_increments_version_on_update() {
        let repo = test_repository().await;
        let body = UpsertCorsPolicyRecord {
            tenant_id: "100001".to_owned(),
            environment: "prod".to_owned(),
            allow_all_origins: false,
            allowed_origins: vec!["https://console.example".to_owned()],
            allow_credentials: true,
        };
        let v1 = repo.upsert_cors_policy(body.clone()).await.expect("insert");
        assert_eq!(1, v1.version);
        let v2 = repo.upsert_cors_policy(body.clone()).await.expect("update");
        assert_eq!(2, v2.version);
        let v3 = repo.upsert_cors_policy(body).await.expect("update 2");
        assert_eq!(3, v3.version);

        let page = repo
            .list_cors_policies(
                "100001",
                None,
                OffsetListPageParams::parse(Some(1), Some(10)),
            )
            .await
            .expect("list");
        assert_eq!(1, page.items.len());
        assert_eq!(3, page.items[0].version);
    }

    #[tokio::test]
    async fn upsert_rate_limit_policy_increments_version_on_update() {
        let repo = test_repository().await;
        let body = UpsertRateLimitPolicyRecord {
            tenant_id: "100001".to_owned(),
            environment: "prod".to_owned(),
            tier_key: "default".to_owned(),
            max_requests: 100,
            window_secs: 60,
            enabled: true,
        };
        let v1 = repo
            .upsert_rate_limit_policy(body.clone())
            .await
            .expect("insert");
        assert_eq!(1, v1.version);
        let v2 = repo
            .upsert_rate_limit_policy(body.clone())
            .await
            .expect("update");
        assert_eq!(2, v2.version);
    }

    #[tokio::test]
    async fn upsert_tenant_runtime_profile_increments_version_on_update() {
        let repo = test_repository().await;
        let body = UpsertTenantRuntimeProfileRecord {
            tenant_id: "100001".to_owned(),
            environment: "prod".to_owned(),
            rate_limit_enabled: Some(true),
            max_content_length: Some(4096),
            max_concurrent_requests: Some(2),
        };
        let v1 = repo
            .upsert_tenant_runtime_profile(body.clone())
            .await
            .expect("insert");
        assert_eq!(1, v1.version);
        let v2 = repo
            .upsert_tenant_runtime_profile(body)
            .await
            .expect("update");
        assert_eq!(2, v2.version);
    }

    #[tokio::test]
    async fn control_node_register_and_heartbeat_round_trips() {
        let repo = test_repository().await;
        let now = 1_700_000_000_i64;
        let (registered, created) = repo
            .register_control_node(
                RegisterControlNodeRecord {
                    node_id: "node-a".to_owned(),
                    region: "us-east-1".to_owned(),
                    base_url: "https://node-a.internal".to_owned(),
                    environment: "prod".to_owned(),
                },
                now,
            )
            .await
            .expect("register");
        assert_eq!("node-a", registered.node_id);
        assert!(created);

        let (refreshed, created_again) = repo
            .register_control_node(
                RegisterControlNodeRecord {
                    node_id: "node-a".to_owned(),
                    region: "us-west-2".to_owned(),
                    base_url: "https://node-a-v2.internal".to_owned(),
                    environment: "prod".to_owned(),
                },
                now + 30,
            )
            .await
            .expect("re-register");
        assert!(!created_again);
        assert_eq!("us-west-2", refreshed.region);
        assert_eq!("https://node-a-v2.internal", refreshed.base_url);

        let heartbeat = repo
            .heartbeat_control_node("node-a", now + 60)
            .await
            .expect("heartbeat");
        assert!(heartbeat.last_heartbeat_at.unwrap_or(0) >= now);

        assert!(repo.control_node_exists("node-a").await.expect("exists"));
        repo.delete_control_node("node-a").await.expect("delete");
        assert!(!repo.control_node_exists("node-a").await.expect("exists"));
    }
}
