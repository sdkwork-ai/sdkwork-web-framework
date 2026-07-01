//! Control-plane use-case layer (`WEB_BACKEND_SPEC.md` §2).

use crate::dto::{
    AuditEventRecord, ControlNodeRecord, CorsPolicyRecord, OptionalFeaturesSnapshot,
    RateLimitPolicyRecord, RegisterControlNodeOutcome, RegisterControlNodeRequest,
    RuntimeDefaultsSnapshot, SecurityEventRecord, TenantRuntimeProfileRecord,
    UpsertCorsPolicyRequest, UpsertRateLimitPolicyRequest, UpsertTenantRuntimeProfileRequest,
};
use crate::persistence::map_repository_error;
use crate::response::ApiProblem;
use crate::services::validation::{
    validate_control_node_register, validate_cors_upsert, validate_rate_limit_upsert,
    validate_tenant_runtime_profile_upsert,
};
use crate::tenant_scope::{AuditEventListScope, SecurityEventListScope};
use sdkwork_web_core::{DynamicPolicyCaches, SecurityPolicy, WebFrameworkOptionalFeatures};
use sdkwork_web_framework_admin_repository_sqlx::{
    AuditEventListScope as RepoAuditScope, RegisterControlNodeRecord,
    SecurityEventListScope as RepoSecurityScope, UpsertCorsPolicyRecord,
    UpsertRateLimitPolicyRecord, UpsertTenantRuntimeProfileRecord, WebFrameworkAdminRepository,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct WebFrameworkAdminService {
    repository: Arc<dyn WebFrameworkAdminRepository>,
    policy_caches: Option<Arc<DynamicPolicyCaches>>,
}

impl WebFrameworkAdminService {
    pub fn new(repository: Arc<dyn WebFrameworkAdminRepository>) -> Self {
        Self {
            repository,
            policy_caches: None,
        }
    }

    pub fn with_policy_caches(mut self, caches: Arc<DynamicPolicyCaches>) -> Self {
        self.policy_caches = Some(caches);
        self
    }

    fn invalidate_policy_cache(&self, tenant_id: &str, environment: &str) {
        if let Some(caches) = &self.policy_caches {
            caches.invalidate_tenant_environment(tenant_id, environment);
        }
    }

    pub async fn list_cors_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<CorsPolicyRecord>, ApiProblem> {
        self.repository
            .list_cors_policies(tenant_id, environment, limit)
            .await
            .map_err(map_repository_error)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| CorsPolicyRecord {
                        tenant_id: row.tenant_id,
                        environment: row.environment,
                        allow_all_origins: row.allow_all_origins,
                        allowed_origins: row.allowed_origins,
                        allow_credentials: row.allow_credentials,
                        version: row.version,
                    })
                    .collect()
            })
    }

    pub async fn upsert_cors_policy(
        &self,
        body: UpsertCorsPolicyRequest,
    ) -> Result<CorsPolicyRecord, ApiProblem> {
        validate_cors_upsert(&body)?;
        let record = self
            .repository
            .upsert_cors_policy(UpsertCorsPolicyRecord {
                tenant_id: body.tenant_id.clone(),
                environment: body.environment.clone(),
                allow_all_origins: body.allow_all_origins,
                allowed_origins: body.allowed_origins.clone(),
                allow_credentials: body.allow_credentials,
            })
            .await
            .map_err(map_repository_error)?;
        self.invalidate_policy_cache(&body.tenant_id, &body.environment);
        Ok(CorsPolicyRecord {
            tenant_id: record.tenant_id,
            environment: record.environment,
            allow_all_origins: record.allow_all_origins,
            allowed_origins: record.allowed_origins,
            allow_credentials: record.allow_credentials,
            version: record.version,
        })
    }

    pub async fn list_rate_limit_policies(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<RateLimitPolicyRecord>, ApiProblem> {
        self.repository
            .list_rate_limit_policies(tenant_id, environment, limit)
            .await
            .map_err(map_repository_error)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| RateLimitPolicyRecord {
                        tenant_id: row.tenant_id,
                        environment: row.environment,
                        tier_key: row.tier_key,
                        max_requests: row.max_requests,
                        window_secs: row.window_secs,
                        enabled: row.enabled,
                        version: row.version,
                    })
                    .collect()
            })
    }

    pub async fn upsert_rate_limit_policy(
        &self,
        body: UpsertRateLimitPolicyRequest,
    ) -> Result<RateLimitPolicyRecord, ApiProblem> {
        validate_rate_limit_upsert(&body)?;
        let record = self
            .repository
            .upsert_rate_limit_policy(UpsertRateLimitPolicyRecord {
                tenant_id: body.tenant_id.clone(),
                environment: body.environment.clone(),
                tier_key: body.tier_key.clone(),
                max_requests: body.max_requests,
                window_secs: body.window_secs,
                enabled: body.enabled,
            })
            .await
            .map_err(map_repository_error)?;
        self.invalidate_policy_cache(&body.tenant_id, &body.environment);
        Ok(RateLimitPolicyRecord {
            tenant_id: record.tenant_id,
            environment: record.environment,
            tier_key: record.tier_key,
            max_requests: record.max_requests,
            window_secs: record.window_secs,
            enabled: record.enabled,
            version: record.version,
        })
    }

    pub async fn list_tenant_runtime_profiles(
        &self,
        tenant_id: &str,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<TenantRuntimeProfileRecord>, ApiProblem> {
        self.repository
            .list_tenant_runtime_profiles(tenant_id, environment, limit)
            .await
            .map_err(map_repository_error)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| TenantRuntimeProfileRecord {
                        tenant_id: row.tenant_id,
                        environment: row.environment,
                        rate_limit_enabled: row.rate_limit_enabled,
                        max_content_length: row.max_content_length,
                        max_concurrent_requests: row.max_concurrent_requests,
                        version: row.version,
                    })
                    .collect()
            })
    }

    pub async fn upsert_tenant_runtime_profile(
        &self,
        body: UpsertTenantRuntimeProfileRequest,
    ) -> Result<TenantRuntimeProfileRecord, ApiProblem> {
        validate_tenant_runtime_profile_upsert(&body)?;
        let record = self
            .repository
            .upsert_tenant_runtime_profile(UpsertTenantRuntimeProfileRecord {
                tenant_id: body.tenant_id.clone(),
                environment: body.environment.clone(),
                rate_limit_enabled: body.rate_limit_enabled,
                max_content_length: body.max_content_length,
                max_concurrent_requests: body.max_concurrent_requests,
            })
            .await
            .map_err(map_repository_error)?;
        self.invalidate_policy_cache(&body.tenant_id, &body.environment);
        Ok(TenantRuntimeProfileRecord {
            tenant_id: record.tenant_id,
            environment: record.environment,
            rate_limit_enabled: record.rate_limit_enabled,
            max_content_length: record.max_content_length,
            max_concurrent_requests: record.max_concurrent_requests,
            version: record.version,
        })
    }

    pub async fn list_security_events(
        &self,
        scope: SecurityEventListScope,
        limit: u32,
    ) -> Result<Vec<SecurityEventRecord>, ApiProblem> {
        let repo_scope = match scope {
            SecurityEventListScope::Tenant(tenant_id) => RepoSecurityScope::Tenant(tenant_id),
            SecurityEventListScope::PlatformAll => RepoSecurityScope::PlatformAll,
        };
        self.repository
            .list_security_events(repo_scope, limit)
            .await
            .map_err(map_repository_error)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| SecurityEventRecord {
                        id: row.id,
                        kind: row.kind,
                        request_id: row.request_id,
                        tenant_id: row.tenant_id,
                        path: row.path,
                        method: row.method,
                        api_surface: row.api_surface,
                        origin: row.origin,
                        detail: row.detail,
                        created_at: row.created_at,
                    })
                    .collect()
            })
    }

    pub async fn list_audit_events(
        &self,
        scope: AuditEventListScope,
        limit: u32,
    ) -> Result<Vec<AuditEventRecord>, ApiProblem> {
        let repo_scope = match scope {
            AuditEventListScope::Tenant(tenant_id) => RepoAuditScope::Tenant(tenant_id),
            AuditEventListScope::PlatformTenant(tenant_id) => {
                RepoAuditScope::PlatformTenant(tenant_id)
            }
            AuditEventListScope::PlatformAll => RepoAuditScope::PlatformAll,
        };
        self.repository
            .list_audit_events(repo_scope, limit)
            .await
            .map_err(map_repository_error)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| AuditEventRecord {
                        id: row.id,
                        request_id: row.request_id,
                        tenant_id: row.tenant_id,
                        user_id: row.user_id,
                        api_surface: row.api_surface,
                        path: row.path,
                        method: row.method,
                        operation_id: row.operation_id,
                        status_code: row.status_code,
                        duration_ms: row.duration_ms,
                        created_at: row.created_at,
                    })
                    .collect()
            })
    }

    pub async fn list_control_nodes(
        &self,
        environment: Option<String>,
        limit: u32,
    ) -> Result<Vec<ControlNodeRecord>, ApiProblem> {
        self.repository
            .list_control_nodes(environment, limit)
            .await
            .map_err(map_repository_error)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| ControlNodeRecord {
                        node_id: row.node_id,
                        region: row.region,
                        base_url: row.base_url,
                        environment: row.environment,
                        status: row.status,
                        last_heartbeat_at: row.last_heartbeat_at,
                        created_at: row.created_at,
                        updated_at: row.updated_at,
                    })
                    .collect()
            })
    }

    pub async fn register_control_node(
        &self,
        body: RegisterControlNodeRequest,
        now: i64,
    ) -> Result<RegisterControlNodeOutcome, ApiProblem> {
        validate_control_node_register(&body)?;
        let region = body.region.unwrap_or_else(|| "default".to_owned());
        // Repository returns `(record, created)` atomically — no TOCTOU pre-check.
        let (record, created) = self
            .repository
            .register_control_node(
                RegisterControlNodeRecord {
                    node_id: body.node_id.clone(),
                    region: region.clone(),
                    base_url: body.base_url.clone(),
                    environment: body.environment.clone(),
                },
                now,
            )
            .await
            .map_err(map_repository_error)?;
        Ok(RegisterControlNodeOutcome {
            record: ControlNodeRecord {
                node_id: record.node_id,
                region: record.region,
                base_url: record.base_url,
                environment: record.environment,
                status: record.status,
                last_heartbeat_at: record.last_heartbeat_at,
                created_at: record.created_at,
                updated_at: record.updated_at,
            },
            created,
        })
    }

    pub async fn heartbeat_control_node(
        &self,
        node_id: &str,
        now: i64,
    ) -> Result<ControlNodeRecord, ApiProblem> {
        let record = self
            .repository
            .heartbeat_control_node(node_id, now)
            .await
            .map_err(map_repository_error)?;
        Ok(ControlNodeRecord {
            node_id: record.node_id,
            region: record.region,
            base_url: record.base_url,
            environment: record.environment,
            status: record.status,
            last_heartbeat_at: record.last_heartbeat_at,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }

    pub async fn delete_control_node(&self, node_id: &str) -> Result<(), ApiProblem> {
        self.repository
            .delete_control_node(node_id)
            .await
            .map_err(map_repository_error)
    }

    pub fn runtime_defaults_snapshot() -> RuntimeDefaultsSnapshot {
        let production = SecurityPolicy::production();
        let default = SecurityPolicy::default();
        RuntimeDefaultsSnapshot {
            production_security_policy: serde_json::json!({
                "rateLimitEnabled": production.rate_limit.enabled,
                "rateLimitMaxRequests": production.rate_limit.max_requests_per_window,
                "jsonContentTypeEnabled": production.json_content_type.enabled,
                "corsAllowAllOrigins": production.cors.allow_all_origins,
                "hstsConfigured": production.header_security.strict_transport_security.is_some(),
            }),
            default_security_policy: serde_json::json!({
                "rateLimitEnabled": default.rate_limit.enabled,
                "rateLimitMaxRequests": default.rate_limit.max_requests_per_window,
                "jsonContentTypeEnabled": default.json_content_type.enabled,
                "corsAllowAllOrigins": default.cors.allow_all_origins,
            }),
            optional_features_production_sqlx: WebFrameworkOptionalFeatures::production_sqlx(),
        }
    }

    pub fn optional_features_snapshot() -> OptionalFeaturesSnapshot {
        OptionalFeaturesSnapshot {
            recommended_production_sqlx: WebFrameworkOptionalFeatures::production_sqlx(),
            development: WebFrameworkOptionalFeatures::development(),
        }
    }
}
