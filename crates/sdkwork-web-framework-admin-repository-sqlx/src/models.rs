use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsPolicyRecord {
    pub tenant_id: String,
    pub environment: String,
    pub allow_all_origins: bool,
    pub allowed_origins: Vec<String>,
    pub allow_credentials: bool,
    /// 乐观锁版本号，由 DB 维护，每次 upsert 自增（migration 014）。
    /// DATABASE_SPEC §6.2 — 检测并发覆盖写。
    pub version: i64,
}

#[derive(Debug, Clone)]
pub struct UpsertCorsPolicyRecord {
    pub tenant_id: String,
    pub environment: String,
    pub allow_all_origins: bool,
    pub allowed_origins: Vec<String>,
    pub allow_credentials: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitPolicyRecord {
    pub tenant_id: String,
    pub environment: String,
    pub tier_key: String,
    pub max_requests: u32,
    pub window_secs: u64,
    pub enabled: bool,
    /// 乐观锁版本号，由 DB 维护，每次 upsert 自增（migration 014）。
    pub version: i64,
}

#[derive(Debug, Clone)]
pub struct UpsertRateLimitPolicyRecord {
    pub tenant_id: String,
    pub environment: String,
    pub tier_key: String,
    pub max_requests: u32,
    pub window_secs: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantRuntimeProfileRecord {
    pub tenant_id: String,
    pub environment: String,
    pub rate_limit_enabled: Option<bool>,
    /// 请求体最大字节数（byte counter）。`i64` 与 SQL BIGINT 对齐，
    /// 便于 DTO 层透传 `serde_int64`（API_SPEC §13 byte counters 必须 string）。
    pub max_content_length: Option<i64>,
    pub max_concurrent_requests: Option<u32>,
    /// 乐观锁版本号，由 DB 维护，每次 upsert 自增（migration 014）。
    pub version: i64,
}

#[derive(Debug, Clone)]
pub struct UpsertTenantRuntimeProfileRecord {
    pub tenant_id: String,
    pub environment: String,
    pub rate_limit_enabled: Option<bool>,
    pub max_content_length: Option<i64>,
    pub max_concurrent_requests: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEventRecord {
    pub id: i64,
    pub kind: String,
    pub request_id: Option<String>,
    /// 租户隔离字段（migration 010）。未鉴权请求回退到 "0"。
    pub tenant_id: Option<String>,
    pub path: String,
    pub method: String,
    pub api_surface: String,
    pub origin: Option<String>,
    pub detail: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventRecord {
    pub id: i64,
    pub request_id: String,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub api_surface: String,
    pub path: String,
    pub method: String,
    pub operation_id: Option<String>,
    pub status_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlNodeRecord {
    pub node_id: String,
    pub region: String,
    pub base_url: String,
    pub environment: String,
    pub status: String,
    pub last_heartbeat_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct RegisterControlNodeRecord {
    pub node_id: String,
    pub region: String,
    pub base_url: String,
    pub environment: String,
}

#[derive(Debug, Clone)]
pub enum AuditEventListScope {
    Tenant(String),
    PlatformTenant(String),
    PlatformAll,
}

/// 安全事件列表作用域 — 镜像 `AuditEventListScope` 语义：
/// - `Tenant` 仅返回指定租户的事件（管理员视图）
/// - `PlatformAll` 跨租户全量视图（control-plane 权限）
#[derive(Debug, Clone)]
pub enum SecurityEventListScope {
    Tenant(String),
    PlatformAll,
}
