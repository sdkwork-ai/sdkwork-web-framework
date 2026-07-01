//! Input validation for framework control-plane mutations (`WEB_BACKEND_SPEC.md`).

use crate::dto::{
    RegisterControlNodeRequest, UpsertCorsPolicyRequest, UpsertRateLimitPolicyRequest,
    UpsertTenantRuntimeProfileRequest,
};
use crate::response::ApiProblem;
use sdkwork_web_core::CorsPolicy;

const MAX_RATE_LIMIT_WINDOW_SECS: u64 = 86_400;
const MAX_RATE_LIMIT_REQUESTS: u32 = 1_000_000;
/// 请求体最大字节数上限（256 MiB）。`i64` 与 DTO/repo 一致，
/// 便于 `max_content_length` 字段在 JSON 边界以 string 透传（API_SPEC §13 byte counters）。
const MAX_CONTENT_LENGTH_BYTES: i64 = 256 * 1024 * 1024;
const MAX_CONCURRENT_REQUESTS: u32 = 10_000;
const MAX_CORS_ALLOWED_ORIGINS: usize = 256;
const MAX_CORS_ORIGIN_LENGTH: usize = 2048;

pub fn validate_cors_upsert(body: &UpsertCorsPolicyRequest) -> Result<(), ApiProblem> {
    if body.environment.trim().is_empty() {
        return Err(ApiProblem::bad_request("environment must not be empty"));
    }
    if body.tenant_id.trim().is_empty() {
        return Err(ApiProblem::bad_request("tenant_id must not be empty"));
    }
    if !body.allow_all_origins {
        if body.allowed_origins.len() > MAX_CORS_ALLOWED_ORIGINS {
            return Err(ApiProblem::bad_request(
                "allowed_origins exceeds supported maximum of 256 entries",
            ));
        }
        for origin in &body.allowed_origins {
            let trimmed = origin.trim();
            if trimmed.is_empty() {
                return Err(ApiProblem::bad_request(
                    "allowed_origins must not contain empty entries",
                ));
            }
            if trimmed.len() > MAX_CORS_ORIGIN_LENGTH {
                return Err(ApiProblem::bad_request(
                    "allowed_origins entry exceeds supported maximum length of 2048 characters",
                ));
            }
        }
    }
    if body.environment.eq_ignore_ascii_case("prod")
        || body.environment.eq_ignore_ascii_case("staging")
    {
        CorsPolicy {
            allow_all_origins: body.allow_all_origins,
            allowed_origins: body.allowed_origins.clone(),
            allowed_methods: vec![],
            allowed_headers: vec![],
            allow_credentials: body.allow_credentials,
        }
        .validate_for_production()
        .map_err(ApiProblem::bad_request)?;
    }
    Ok(())
}

pub fn validate_rate_limit_upsert(body: &UpsertRateLimitPolicyRequest) -> Result<(), ApiProblem> {
    if body.tenant_id.trim().is_empty() {
        return Err(ApiProblem::bad_request("tenant_id must not be empty"));
    }
    if body.tier_key.trim().is_empty() {
        return Err(ApiProblem::bad_request("tier_key must not be empty"));
    }
    if body.environment.trim().is_empty() {
        return Err(ApiProblem::bad_request("environment must not be empty"));
    }
    if body.window_secs == 0 {
        return Err(ApiProblem::bad_request(
            "window_secs must be greater than zero",
        ));
    }
    if body.window_secs > MAX_RATE_LIMIT_WINDOW_SECS {
        return Err(ApiProblem::bad_request(
            "window_secs exceeds supported maximum of 86400 seconds",
        ));
    }
    if body.max_requests > MAX_RATE_LIMIT_REQUESTS {
        return Err(ApiProblem::bad_request(
            "max_requests exceeds supported maximum of 1000000",
        ));
    }
    if body.enabled && body.max_requests == 0 {
        return Err(ApiProblem::bad_request(
            "max_requests must be greater than zero when rate limiting is enabled",
        ));
    }
    Ok(())
}

pub fn validate_tenant_runtime_profile_upsert(
    body: &UpsertTenantRuntimeProfileRequest,
) -> Result<(), ApiProblem> {
    if body.tenant_id.trim().is_empty() {
        return Err(ApiProblem::bad_request("tenant_id must not be empty"));
    }
    if body.environment.trim().is_empty() {
        return Err(ApiProblem::bad_request("environment must not be empty"));
    }
    if let Some(max_len) = body.max_content_length {
        if max_len == 0 {
            return Err(ApiProblem::bad_request(
                "max_content_length must be greater than zero when set",
            ));
        }
        if max_len > MAX_CONTENT_LENGTH_BYTES {
            return Err(ApiProblem::bad_request(
                "max_content_length exceeds supported maximum of 268435456 bytes",
            ));
        }
    }
    if let Some(max_concurrent) = body.max_concurrent_requests {
        if max_concurrent == 0 {
            return Err(ApiProblem::bad_request(
                "max_concurrent_requests must be greater than zero when set",
            ));
        }
        if max_concurrent > MAX_CONCURRENT_REQUESTS {
            return Err(ApiProblem::bad_request(
                "max_concurrent_requests exceeds supported maximum of 10000",
            ));
        }
    }
    Ok(())
}

pub fn validate_control_node_id(node_id: &str) -> Result<(), ApiProblem> {
    let trimmed = node_id.trim();
    if trimmed.is_empty() {
        return Err(ApiProblem::bad_request("node_id must not be empty"));
    }
    if trimmed.len() > 128 {
        return Err(ApiProblem::bad_request(
            "node_id exceeds supported maximum length of 128 characters",
        ));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(ApiProblem::bad_request(
            "node_id contains invalid path characters",
        ));
    }
    Ok(())
}

pub fn validate_control_node_register(body: &RegisterControlNodeRequest) -> Result<(), ApiProblem> {
    validate_control_node_id(&body.node_id)?;
    if body.base_url.trim().is_empty() {
        return Err(ApiProblem::bad_request("base_url must not be empty"));
    }
    if body.environment.trim().is_empty() {
        return Err(ApiProblem::bad_request("environment must not be empty"));
    }
    let base_url = body.base_url.trim();
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Err(ApiProblem::bad_request(
            "base_url must be an absolute http or https URL",
        ));
    }
    Ok(())
}

pub fn validate_list_limit(limit: Option<u32>) -> Result<u32, ApiProblem> {
    match limit {
        None => Ok(50),
        Some(0) => Err(ApiProblem::bad_request("limit must be greater than zero")),
        Some(value) if value > 200 => Ok(200),
        Some(value) => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_enabled_rate_limit_with_zero_max_requests() {
        let body = UpsertRateLimitPolicyRequest {
            tenant_id: "100001".to_owned(),
            environment: "prod".to_owned(),
            tier_key: "default".to_owned(),
            max_requests: 0,
            window_secs: 60,
            enabled: true,
        };
        assert!(validate_rate_limit_upsert(&body).is_err());
    }

    #[test]
    fn rejects_oversized_tenant_runtime_profile() {
        let body = UpsertTenantRuntimeProfileRequest {
            tenant_id: "100001".to_owned(),
            environment: "prod".to_owned(),
            rate_limit_enabled: None,
            max_content_length: Some(MAX_CONTENT_LENGTH_BYTES + 1),
            max_concurrent_requests: None,
        };
        assert!(validate_tenant_runtime_profile_upsert(&body).is_err());
    }

    #[test]
    fn accepts_string_max_content_length_round_trip() {
        // API_SPEC §13：byte counter 在 JSON 边界为 string。反序列化后转为 i64。
        let body = UpsertTenantRuntimeProfileRequest {
            tenant_id: "100001".to_owned(),
            environment: "prod".to_owned(),
            rate_limit_enabled: None,
            max_content_length: Some(1_048_576), // 1 MiB
            max_concurrent_requests: None,
        };
        assert!(validate_tenant_runtime_profile_upsert(&body).is_ok());
    }

    #[test]
    fn rejects_invalid_control_node_base_url() {
        let body = RegisterControlNodeRequest {
            node_id: "node-a".to_owned(),
            base_url: "not-a-url".to_owned(),
            environment: "prod".to_owned(),
            region: None,
        };
        assert!(validate_control_node_register(&body).is_err());
    }

    #[test]
    fn rejects_invalid_control_node_id_characters() {
        assert!(validate_control_node_id("node/with-slash").is_err());
        assert!(validate_control_node_id("node..traversal").is_err());
    }

    #[test]
    fn rejects_allow_all_origins_in_prod() {
        let body = UpsertCorsPolicyRequest {
            tenant_id: "100001".to_owned(),
            environment: "prod".to_owned(),
            allow_all_origins: true,
            allowed_origins: vec![],
            allow_credentials: false,
        };
        assert!(validate_cors_upsert(&body).is_err());
    }

    #[test]
    fn rejects_oversized_cors_origin_list() {
        let body = UpsertCorsPolicyRequest {
            tenant_id: "100001".to_owned(),
            environment: "prod".to_owned(),
            allow_all_origins: false,
            allowed_origins: (0..257)
                .map(|index| format!("https://origin-{index}.example"))
                .collect(),
            allow_credentials: false,
        };
        assert!(validate_cors_upsert(&body).is_err());
    }

    #[test]
    fn rejects_zero_list_limit() {
        assert!(validate_list_limit(Some(0)).is_err());
    }

    #[test]
    fn caps_list_limit_at_two_hundred() {
        assert_eq!(200, validate_list_limit(Some(500)).expect("limit"));
    }
}
