//! HTTP 边界 DTO（`API_SPEC.md §13`）。
//!
//! - JSON 字段名使用 `lowerCamelCase`（`#[serde(rename_all = "camelCase")]`）。
//! - `int64` 字段（IDs、versions、timestamps、byte counters、status_code、duration_ms）
//!   在 JSON 边界序列化为 string，遵循 `API_SPEC.md §13` int64-as-string 规则，
//!   使用 `sdkwork_utils_rust::serde_int64` helper 避免 JavaScript 精度丢失。
//! - Rust 内部领域模型保留原生 `i64`；序列化层透明转换。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorsPolicyRecord {
    pub tenant_id: String,
    pub environment: String,
    pub allow_all_origins: bool,
    pub allowed_origins: Vec<String>,
    pub allow_credentials: bool,
    /// 乐观锁版本号（migration 014）。每次 upsert 自增。
    /// API 边界 string（API_SPEC §13 int64-as-string）。
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub version: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertCorsPolicyRequest {
    pub tenant_id: String,
    pub environment: String,
    pub allow_all_origins: bool,
    pub allowed_origins: Vec<String>,
    pub allow_credentials: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitPolicyRecord {
    pub tenant_id: String,
    pub environment: String,
    pub tier_key: String,
    /// 限流配额。Rust 内部 `u32`，JSON 边界保留 number（实际值 < 2^31，JS 安全）。
    pub max_requests: u32,
    /// 限流窗口秒数。Rust 内部 `u64`，JSON 边界保留 number（实际值 < 86400，JS 安全）。
    pub window_secs: u64,
    pub enabled: bool,
    /// 乐观锁版本号（migration 014）。API 边界 string。
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub version: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertRateLimitPolicyRequest {
    pub tenant_id: String,
    pub environment: String,
    pub tier_key: String,
    pub max_requests: u32,
    pub window_secs: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TenantRuntimeProfileRecord {
    pub tenant_id: String,
    pub environment: String,
    pub rate_limit_enabled: Option<bool>,
    /// 请求体最大字节数（byte counter）。
    /// API_SPEC §13：byte counters 必须 string。
    #[serde(with = "sdkwork_utils_rust::serde_int64::option", default)]
    pub max_content_length: Option<i64>,
    /// 最大并发请求数。Rust 内部 `Option<u32>`，JSON 边界保留 number（实际值小）。
    pub max_concurrent_requests: Option<u32>,
    /// 乐观锁版本号（migration 014）。API 边界 string。
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub version: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertTenantRuntimeProfileRequest {
    pub tenant_id: String,
    pub environment: String,
    pub rate_limit_enabled: Option<bool>,
    /// 请求体最大字节数（byte counter）。API 边界 string。
    #[serde(with = "sdkwork_utils_rust::serde_int64::option", default)]
    pub max_content_length: Option<i64>,
    pub max_concurrent_requests: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityEventRecord {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
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
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventRecord {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub id: i64,
    pub request_id: String,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub api_surface: String,
    pub path: String,
    pub method: String,
    pub operation_id: Option<String>,
    #[serde(with = "sdkwork_utils_rust::serde_int64::option", default)]
    pub status_code: Option<i64>,
    #[serde(with = "sdkwork_utils_rust::serde_int64::option", default)]
    pub duration_ms: Option<i64>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlNodeRecord {
    pub node_id: String,
    pub region: String,
    pub base_url: String,
    pub environment: String,
    pub status: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64::option", default)]
    pub last_heartbeat_at: Option<i64>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub created_at: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterControlNodeOutcome {
    pub record: ControlNodeRecord,
    pub created: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterControlNodeRequest {
    pub node_id: String,
    pub region: Option<String>,
    pub base_url: String,
    pub environment: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDefaultsSnapshot {
    pub production_security_policy: serde_json::Value,
    pub default_security_policy: serde_json::Value,
    pub optional_features_production_sqlx: sdkwork_web_core::WebFrameworkOptionalFeatures,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalFeaturesSnapshot {
    pub recommended_production_sqlx: sdkwork_web_core::WebFrameworkOptionalFeatures,
    pub development: sdkwork_web_core::WebFrameworkOptionalFeatures,
}

/// 列表查询参数。`API_SPEC.md §11.2`：多词 query 参数名使用 `lower_snake_case`。
///
/// 注意：query 参数命名规则与 JSON body 字段不同。JSON body 用 `lowerCamelCase`，
/// query 参数用 `lower_snake_case`。因此此结构体不使用 `rename_all`。
#[derive(Debug, Clone, Deserialize)]
pub struct ListQuery {
    pub environment: Option<String>,
    pub tenant_id: Option<String>,
    pub limit: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_policy_record_serializes_int64_as_string() {
        let record = CorsPolicyRecord {
            tenant_id: "100001".to_owned(),
            environment: "prod".to_owned(),
            allow_all_origins: true,
            allowed_origins: vec!["https://app.example".to_owned()],
            allow_credentials: true,
            version: 42,
        };
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!("42", json["version"].as_str().unwrap());
        assert_eq!("100001", json["tenantId"].as_str().unwrap());
        assert_eq!("prod", json["environment"].as_str().unwrap());
        assert_eq!(true, json["allowAllOrigins"].as_bool().unwrap());
    }

    #[test]
    fn security_event_record_serializes_id_and_created_at_as_string() {
        let record = SecurityEventRecord {
            id: 9_223_372_036_854_775_807,
            kind: "auth.failure".to_owned(),
            request_id: Some("req-1".to_owned()),
            tenant_id: Some("100001".to_owned()),
            path: "/health".to_owned(),
            method: "GET".to_owned(),
            api_surface: "backend-api".to_owned(),
            origin: None,
            detail: "auth failed".to_owned(),
            created_at: 1_700_000_000,
        };
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!("9223372036854775807", json["id"].as_str().unwrap());
        assert_eq!("1700000000", json["createdAt"].as_str().unwrap());
        assert_eq!("req-1", json["requestId"].as_str().unwrap());
        assert_eq!("100001", json["tenantId"].as_str().unwrap());
    }

    #[test]
    fn audit_event_record_serializes_optional_int64_as_string_or_null() {
        let record = AuditEventRecord {
            id: 1,
            request_id: "req-1".to_owned(),
            tenant_id: None,
            user_id: None,
            api_surface: "backend-api".to_owned(),
            path: "/x".to_owned(),
            method: "GET".to_owned(),
            operation_id: None,
            status_code: Some(200),
            duration_ms: None,
            created_at: 100,
        };
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!("1", json["id"].as_str().unwrap());
        assert_eq!("200", json["statusCode"].as_str().unwrap());
        assert!(json["durationMs"].is_null());
    }

    #[test]
    fn control_node_record_round_trips_int64_string() {
        let record = ControlNodeRecord {
            node_id: "node-1".to_owned(),
            region: "default".to_owned(),
            base_url: "https://node.example".to_owned(),
            environment: "prod".to_owned(),
            status: "active".to_owned(),
            last_heartbeat_at: Some(1_700_000_000),
            created_at: 1_699_999_999,
            updated_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: ControlNodeRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record.last_heartbeat_at, parsed.last_heartbeat_at);
        assert_eq!(record.created_at, parsed.created_at);
        assert_eq!(record.updated_at, parsed.updated_at);
    }

    #[test]
    fn upsert_tenant_runtime_profile_deserializes_max_content_length_string() {
        let json = r#"{"tenantId":"100001","environment":"prod","maxContentLength":"1048576"}"#;
        let body: UpsertTenantRuntimeProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(Some(1_048_576), body.max_content_length);
    }

    #[test]
    fn upsert_rate_limit_request_uses_camel_case_fields() {
        let json = r#"{"tenantId":"100001","environment":"prod","tierKey":"default","maxRequests":100,"windowSecs":60,"enabled":true}"#;
        let body: UpsertRateLimitPolicyRequest = serde_json::from_str(json).unwrap();
        assert_eq!("100001", body.tenant_id);
        assert_eq!("default", body.tier_key);
        assert_eq!(100, body.max_requests);
        assert_eq!(60, body.window_secs);
        assert!(body.enabled);
    }
}
