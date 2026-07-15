//! `SDKWORK_WEB_*` environment vocabulary (catalog H5).

use std::env;

use sdkwork_web_core::{CorsPolicy, SecurityPolicy, WebEnvironment};

pub fn web_environment_from_env(keys: &[&str]) -> WebEnvironment {
    let value = keys
        .iter()
        .find_map(|key| env::var(key).ok())
        .unwrap_or_else(|| "development".to_owned());
    match value.trim().to_ascii_lowercase().as_str() {
        "development" | "dev" | "local" => WebEnvironment::Dev,
        "test" | "testing" => WebEnvironment::Test,
        _ => WebEnvironment::Prod,
    }
}

pub fn cors_allowed_origins_from_env(keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| env::var(key).ok())
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn security_policy_for_environment(
    environment: &WebEnvironment,
    configured_origins: impl IntoIterator<Item = String>,
) -> SecurityPolicy {
    let mut policy = SecurityPolicy::default();
    if matches!(environment, WebEnvironment::Dev | WebEnvironment::Test) {
        policy.cors = CorsPolicy::development_private_network();
    }
    for origin in configured_origins {
        if !policy.cors.allowed_origins.contains(&origin) {
            policy.cors.allowed_origins.push(origin);
        }
    }
    policy
}

pub fn application_cors_layer_from_env(
    environment_keys: &[&str],
    allowed_origin_keys: &[&str],
) -> tower_http::cors::CorsLayer {
    let environment = web_environment_from_env(environment_keys);
    let origins = cors_allowed_origins_from_env(allowed_origin_keys);
    let policy = security_policy_for_environment(&environment, origins);
    sdkwork_web_axum::cors_layer_from_policy(policy.cors)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebFrameworkEnv {
    pub store_url: Option<String>,
    pub store_pool_size: Option<u32>,
    pub admin_bind: Option<String>,
    pub redis_url: Option<String>,
    pub jwt_hs256_secret: Option<String>,
    pub jwt_bootstrap_tenant_id: Option<String>,
    pub jwt_bootstrap_key_id: Option<String>,
    pub otel_service_name: Option<String>,
    pub otel_exporter_endpoint: Option<String>,
    pub deployment_environment: Option<String>,
}

impl WebFrameworkEnv {
    pub fn from_process_env() -> Self {
        Self {
            store_url: env::var("SDKWORK_WEB_FRAMEWORK_STORE_URL").ok(),
            store_pool_size: env::var("SDKWORK_WEB_FRAMEWORK_STORE_POOL_SIZE")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|size| *size > 0),
            admin_bind: env::var("SDKWORK_WEB_FRAMEWORK_ADMIN_BIND").ok(),
            redis_url: env::var("SDKWORK_WEB_FRAMEWORK_REDIS_URL").ok(),
            jwt_hs256_secret: env::var("SDKWORK_WEB_FRAMEWORK_JWT_HS256_SECRET").ok(),
            jwt_bootstrap_tenant_id: env::var("SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_TENANT_ID").ok(),
            jwt_bootstrap_key_id: env::var("SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_KEY_ID").ok(),
            otel_service_name: env::var("OTEL_SERVICE_NAME").ok(),
            otel_exporter_endpoint: env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            deployment_environment: env::var("SDKWORK_WEB_FRAMEWORK_ENV").ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::security_policy_for_environment;
    use sdkwork_web_core::WebEnvironment;

    #[test]
    fn development_bootstrap_uses_shared_private_network_cors_policy() {
        let policy = security_policy_for_environment(&WebEnvironment::Dev, Vec::new());
        policy
            .cors
            .validate_origin_value("http://192.168.50.12:5173")
            .expect("private-network development origin");
        policy
            .cors
            .validate_origin_value("https://evil.example.com")
            .expect_err("public hostname must remain rejected");
    }

    #[test]
    fn production_bootstrap_keeps_exact_origin_allowlist() {
        let policy = security_policy_for_environment(
            &WebEnvironment::Prod,
            ["https://console.example.com".to_owned()],
        );
        policy
            .cors
            .validate_origin_value("https://console.example.com")
            .expect("configured production origin");
        policy
            .cors
            .validate_origin_value("http://192.168.50.12:5173")
            .expect_err("private-network development origin must not leak into production");
    }
}
