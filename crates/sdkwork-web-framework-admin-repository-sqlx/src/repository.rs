use crate::error::RepositoryError;
use crate::models::{
    AuditEventListScope, AuditEventRecord, ControlNodeRecord, CorsPolicyRecord,
    RateLimitPolicyRecord, RegisterControlNodeRecord, SecurityEventListScope, SecurityEventRecord,
    TenantRuntimeProfileRecord, UpsertCorsPolicyRecord, UpsertRateLimitPolicyRecord,
    UpsertTenantRuntimeProfileRecord,
};
use async_trait::async_trait;
use sqlx::SqlitePool;

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    tracing::error!(%error, "database operation failed");
    RepositoryError::Database("database operation failed".to_owned())
}

fn map_stored_json_error(error: serde_json::Error) -> RepositoryError {
    tracing::error!(%error, "stored configuration payload is corrupt");
    RepositoryError::StoredJson("stored configuration payload is corrupt".to_owned())
}

#[async_trait]
pub trait WebFrameworkAdminRepository: Send + Sync {
    async fn list_cors_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<CorsPolicyRecord>, RepositoryError>;

    async fn upsert_cors_policy(
        &self,
        body: UpsertCorsPolicyRecord,
    ) -> Result<CorsPolicyRecord, RepositoryError>;

    async fn list_rate_limit_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<RateLimitPolicyRecord>, RepositoryError>;

    async fn upsert_rate_limit_policy(
        &self,
        body: UpsertRateLimitPolicyRecord,
    ) -> Result<RateLimitPolicyRecord, RepositoryError>;

    async fn list_tenant_runtime_profiles(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<TenantRuntimeProfileRecord>, RepositoryError>;

    async fn upsert_tenant_runtime_profile(
        &self,
        body: UpsertTenantRuntimeProfileRecord,
    ) -> Result<TenantRuntimeProfileRecord, RepositoryError>;

    /// Lists security events with tenant scoping. `scope` mirrors
    /// [`AuditEventListScope`] semantics for tenant isolation (migration 010).
    async fn list_security_events(
        &self,
        scope: SecurityEventListScope,
        limit: u32,
    ) -> Result<Vec<SecurityEventRecord>, RepositoryError>;

    async fn list_audit_events(
        &self,
        scope: AuditEventListScope,
        limit: u32,
    ) -> Result<Vec<AuditEventRecord>, RepositoryError>;

    async fn list_control_nodes(
        &self,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<ControlNodeRecord>, RepositoryError>;

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
    pool: SqlitePool,
}

impl SqlxWebFrameworkAdminRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WebFrameworkAdminRepository for SqlxWebFrameworkAdminRepository {
    async fn list_cors_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<CorsPolicyRecord>, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, String, i64, String, i64, i64)>(
            "SELECT tenant_id, environment, allow_all_origins, allowed_origins, allow_credentials, version \
             FROM web_cors_policy \
             WHERE (?1 IS NULL OR environment = ?1) AND tenant_id = ?2 \
             ORDER BY tenant_id, environment \
             LIMIT ?3",
        )
        .bind(environment)
        .bind(tenant_id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let origins: Vec<String> =
                serde_json::from_str(&row.3).map_err(map_stored_json_error)?;
            items.push(CorsPolicyRecord {
                tenant_id: row.0,
                environment: row.1,
                allow_all_origins: row.2 != 0,
                allowed_origins: origins,
                allow_credentials: row.4 != 0,
                version: row.5,
            });
        }
        Ok(items)
    }

    async fn upsert_cors_policy(
        &self,
        body: UpsertCorsPolicyRecord,
    ) -> Result<CorsPolicyRecord, RepositoryError> {
        let origins_json = serde_json::to_string(&body.allowed_origins).map_err(|_| {
            RepositoryError::StoredJson("allowed_origins payload is invalid".into())
        })?;
        // UPSERT 自增 version（migration 014 乐观锁）。新插入行 version=1。
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
        .fetch_one(&self.pool)
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

    async fn list_rate_limit_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<RateLimitPolicyRecord>, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, String, String, i64, i64, i64, i64)>(
            "SELECT tenant_id, environment, tier_key, max_requests, window_secs, enabled, version \
             FROM web_rate_limit_policy \
             WHERE (?1 IS NULL OR environment = ?1) AND tenant_id = ?2 \
             ORDER BY tenant_id, environment, tier_key \
             LIMIT ?3",
        )
        .bind(environment)
        .bind(tenant_id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows
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
            .collect())
    }

    async fn upsert_rate_limit_policy(
        &self,
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
        .fetch_one(&self.pool)
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

    async fn list_tenant_runtime_profiles(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<TenantRuntimeProfileRecord>, RepositoryError> {
        let rows =
            sqlx::query_as::<_, (String, String, Option<i64>, Option<i64>, Option<i64>, i64)>(
                "SELECT tenant_id, environment, rate_limit_enabled, max_content_length, max_concurrent_requests, version \
                 FROM web_tenant_runtime_profile \
                 WHERE (?1 IS NULL OR environment = ?1) AND tenant_id = ?2 \
                 ORDER BY tenant_id, environment \
                 LIMIT ?3",
            )
            .bind(environment)
            .bind(tenant_id)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|row| TenantRuntimeProfileRecord {
                tenant_id: row.0,
                environment: row.1,
                rate_limit_enabled: row.2.map(|value| value != 0),
                max_content_length: row.3,
                max_concurrent_requests: row.4.and_then(|value| u32::try_from(value.max(0)).ok()),
                version: row.5,
            })
            .collect())
    }

    async fn upsert_tenant_runtime_profile(
        &self,
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
        .fetch_one(&self.pool)
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

    async fn list_security_events(
        &self,
        scope: SecurityEventListScope,
        limit: u32,
    ) -> Result<Vec<SecurityEventRecord>, RepositoryError> {
        let limit = i64::from(limit);
        let rows = match scope {
            SecurityEventListScope::Tenant(tenant_id) => {
                sqlx::query_as::<
                    _,
                    (
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
                    ),
                >(
                    "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                     FROM web_security_event \
                     WHERE tenant_id = ?1 \
                     ORDER BY id DESC LIMIT ?2",
                )
                .bind(&tenant_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            SecurityEventListScope::PlatformAll => {
                sqlx::query_as::<
                    _,
                    (
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
                    ),
                >(
                    "SELECT id, kind, request_id, tenant_id, path, method, api_surface, origin, detail, created_at \
                     FROM web_security_event \
                     ORDER BY id DESC LIMIT ?1",
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(map_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|row| SecurityEventRecord {
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
            })
            .collect())
    }

    async fn list_audit_events(
        &self,
        scope: AuditEventListScope,
        limit: u32,
    ) -> Result<Vec<AuditEventRecord>, RepositoryError> {
        let limit = i64::from(limit);
        let rows = match scope {
            AuditEventListScope::Tenant(tenant_id)
            | AuditEventListScope::PlatformTenant(tenant_id) => {
                sqlx::query_as::<_, (i64, String, Option<String>, Option<String>, String, String, String, Option<String>, Option<i64>, Option<i64>, i64)>(
                    "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                     FROM web_audit_event \
                     WHERE tenant_id = ?1 \
                     ORDER BY id DESC LIMIT ?2",
                )
                .bind(&tenant_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            AuditEventListScope::PlatformAll => {
                sqlx::query_as::<_, (i64, String, Option<String>, Option<String>, String, String, String, Option<String>, Option<i64>, Option<i64>, i64)>(
                    "SELECT id, request_id, tenant_id, user_id, api_surface, path, method, operation_id, status_code, duration_ms, created_at \
                     FROM web_audit_event \
                     ORDER BY id DESC LIMIT ?1",
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(map_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|row| AuditEventRecord {
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
            })
            .collect())
    }

    async fn list_control_nodes(
        &self,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<ControlNodeRecord>, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, Option<i64>, i64, i64)>(
            "SELECT node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at \
             FROM web_control_node \
             WHERE (?1 IS NULL OR environment = ?1) \
             ORDER BY region, node_id \
             LIMIT ?2",
        )
        .bind(environment)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|row| ControlNodeRecord {
                node_id: row.0,
                region: row.1,
                base_url: row.2,
                environment: row.3,
                status: row.4,
                last_heartbeat_at: row.5,
                created_at: row.6,
                updated_at: row.7,
            })
            .collect())
    }

    async fn control_node_exists(&self, node_id: &str) -> Result<bool, RepositoryError> {
        let count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM web_control_node WHERE node_id = ?")
                .bind(node_id)
                .fetch_one(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        Ok(count > 0)
    }

    async fn register_control_node(
        &self,
        body: RegisterControlNodeRecord,
        now: i64,
    ) -> Result<(ControlNodeRecord, bool), RepositoryError> {
        // Atomic insert-or-nothing. RETURNING yields the row only on insert,
        // so `created=true` with no follow-up. On conflict (no row returned),
        // we fall through to an UPDATE that refreshes the existing record.
        let inserted = sqlx::query_as::<_, (String, String, String, String, String, Option<i64>, i64, i64)>(
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
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if let Some(row) = inserted {
            return Ok((
                ControlNodeRecord {
                    node_id: row.0,
                    region: row.1,
                    base_url: row.2,
                    environment: row.3,
                    status: row.4,
                    last_heartbeat_at: row.5,
                    created_at: row.6,
                    updated_at: row.7,
                },
                true,
            ));
        }

        // Conflict: refresh the existing record via atomic UPDATE ... RETURNING.
        let row = sqlx::query_as::<_, (String, String, String, String, String, Option<i64>, i64, i64)>(
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
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?
        .ok_or_else(|| {
            RepositoryError::Database("control node missing after register update".into())
        })?;

        Ok((
            ControlNodeRecord {
                node_id: row.0,
                region: row.1,
                base_url: row.2,
                environment: row.3,
                status: row.4,
                last_heartbeat_at: row.5,
                created_at: row.6,
                updated_at: row.7,
            },
            false,
        ))
    }

    async fn get_control_node(
        &self,
        node_id: &str,
    ) -> Result<Option<ControlNodeRecord>, RepositoryError> {
        let row = sqlx::query_as::<_, (String, String, String, String, String, Option<i64>, i64, i64)>(
            "SELECT node_id, region, base_url, environment, status, last_heartbeat_at, created_at, updated_at \
             FROM web_control_node WHERE node_id = ?",
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.map(|row| ControlNodeRecord {
            node_id: row.0,
            region: row.1,
            base_url: row.2,
            environment: row.3,
            status: row.4,
            last_heartbeat_at: row.5,
            created_at: row.6,
            updated_at: row.7,
        }))
    }

    async fn heartbeat_control_node(
        &self,
        node_id: &str,
        now: i64,
    ) -> Result<ControlNodeRecord, RepositoryError> {
        let updated = sqlx::query(
            "UPDATE web_control_node SET status = 'online', last_heartbeat_at = ?, updated_at = ? WHERE node_id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(node_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if updated.rows_affected() == 0 {
            return Err(RepositoryError::Database(format!(
                "control node {node_id} not found"
            )));
        }

        self.get_control_node(node_id)
            .await?
            .ok_or_else(|| RepositoryError::Database(format!("control node {node_id} not found")))
    }

    async fn delete_control_node(&self, node_id: &str) -> Result<(), RepositoryError> {
        let result = sqlx::query("DELETE FROM web_control_node WHERE node_id = ?")
            .bind(node_id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Database(format!(
                "control node {node_id} not found"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod repository_tests {
    use super::*;
    use sdkwork_web_store_sqlx::connect_sqlite;

    async fn test_repository() -> SqlxWebFrameworkAdminRepository {
        let pool = connect_sqlite("sqlite::memory:", 1)
            .await
            .expect("in-memory sqlite pool");
        SqlxWebFrameworkAdminRepository::new(pool)
    }

    #[tokio::test]
    async fn list_cors_policies_returns_empty_for_unknown_tenant() {
        let repo = test_repository().await;
        let rows = repo
            .list_cors_policies("199999", None, 10)
            .await
            .expect("list");
        assert!(rows.is_empty());
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
        // migration 014：新插入行 version=1，每次 upsert 自增。
        assert_eq!(1, saved.version);

        let listed = repo
            .list_cors_policies("100001", Some("prod".to_owned()), 10)
            .await
            .expect("list");
        assert_eq!(1, listed.len());
        assert_eq!(saved.tenant_id, listed[0].tenant_id);
        assert_eq!(saved.environment, listed[0].environment);
        assert_eq!(saved.allow_all_origins, listed[0].allow_all_origins);
        assert_eq!(saved.allowed_origins, listed[0].allowed_origins);
        assert_eq!(saved.allow_credentials, listed[0].allow_credentials);
        assert_eq!(saved.version, listed[0].version);
    }

    #[tokio::test]
    async fn upsert_cors_policy_increments_version_on_update() {
        // migration 014：乐观锁版本号在每次 upsert 冲突更新时自增。
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

        let listed = repo
            .list_cors_policies("100001", None, 10)
            .await
            .expect("list");
        assert_eq!(1, listed.len());
        assert_eq!(3, listed[0].version);
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

        // Re-registering the same node_id must report `created=false` (refresh).
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
