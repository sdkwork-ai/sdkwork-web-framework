//! `SDKWORK_WEB_*` environment vocabulary (catalog H5).

use std::env;

use sdkwork_web_core::{CorsPolicy, SecurityPolicy, WebEnvironment};

/// Canonical browser CORS allow-list environment key shared by every service.
///
/// Deployment configurations set exactly this key; domain-specific legacy keys
/// (`SDKWORK_<DOMAIN>_CORS_ALLOWED_ORIGINS`, bare `*_CORS_ALLOW_ORIGINS`, ...)
/// still resolve as a compatibility fallback while emitting a deprecation
/// warning, and must not be relied on for new deployments.
pub const SHARED_CORS_ALLOWED_ORIGINS_ENV_KEY: &str = "SDKWORK_CORS_ALLOWED_ORIGINS";

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

/// Resolves the browser CORS allow-list from the environment.
///
/// The caller-provided keys are tried first for compatibility with legacy
/// deployments; the canonical [`SHARED_CORS_ALLOWED_ORIGINS_ENV_KEY`] always
/// wins when it is set and otherwise acts as the final fallback, so every
/// service converges on a single configuration key. Matching a legacy key
/// emits a deprecation warning.
pub fn cors_allowed_origins_from_env(keys: &[&str]) -> Vec<String> {
    if let Some(origins) = read_env_list(&[SHARED_CORS_ALLOWED_ORIGINS_ENV_KEY]) {
        return origins;
    }
    if let Some(legacy_key) = keys
        .iter()
        .find(|key| **key != SHARED_CORS_ALLOWED_ORIGINS_ENV_KEY && env::var(key).is_ok())
    {
        tracing::warn!(
            key = *legacy_key,
            "CORS origins configured through a legacy key; set {} instead",
            SHARED_CORS_ALLOWED_ORIGINS_ENV_KEY,
        );
        return read_env_list(&[*legacy_key]).unwrap_or_default();
    }
    Vec::new()
}

/// Resolves the CORS allow-list from the canonical shared key only.
pub fn cors_allowed_origins_from_process_env() -> Vec<String> {
    read_env_list(&[SHARED_CORS_ALLOWED_ORIGINS_ENV_KEY]).unwrap_or_default()
}

fn read_env_list(keys: &[&str]) -> Option<Vec<String>> {
    keys.iter().find_map(|key| {
        env::var(key).ok().map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_owned)
                .collect()
        })
    })
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
    policy.cors = policy.cors.with_registered_sdkwork_client_origins();
    policy
}

pub fn application_security_policy_from_env(
    environment_keys: &[&str],
    allowed_origin_keys: &[&str],
) -> (WebEnvironment, SecurityPolicy) {
    let environment = web_environment_from_env(environment_keys);
    let origins = cors_allowed_origins_from_env(allowed_origin_keys);
    let policy = security_policy_for_environment(&environment, origins);
    (environment, policy)
}

pub fn application_cors_layer_from_env(
    environment_keys: &[&str],
    allowed_origin_keys: &[&str],
) -> tower_http::cors::CorsLayer {
    let (_, policy) = application_security_policy_from_env(environment_keys, allowed_origin_keys);
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
    use super::{application_security_policy_from_env, security_policy_for_environment};
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
            .validate_origin_value("app://dsh")
            .expect("registered desktop origin");
        policy
            .cors
            .validate_origin_value("https://servicewechat.com")
            .expect("registered mini program origin");
        policy
            .cors
            .validate_origin_value("http://192.168.50.12:5173")
            .expect_err("private-network development origin must not leak into production");
    }

    #[test]
    fn application_bootstrap_resolves_environment_and_origins_from_env() {
        const ENVIRONMENT_KEY: &str = "SDKWORK_WEB_BOOTSTRAP_TEST_ENVIRONMENT";
        const ORIGINS_KEY: &str = "SDKWORK_WEB_BOOTSTRAP_TEST_CORS_ALLOWED_ORIGINS";
        std::env::set_var(ENVIRONMENT_KEY, "production");
        std::env::set_var(
            ORIGINS_KEY,
            "https://manager.sdkwork.com, https://admin.sdkwork.com",
        );

        let (environment, policy) =
            application_security_policy_from_env(&[ENVIRONMENT_KEY], &[ORIGINS_KEY]);

        std::env::remove_var(ENVIRONMENT_KEY);
        std::env::remove_var(ORIGINS_KEY);
        assert_eq!(WebEnvironment::Prod, environment);
        policy
            .cors
            .validate_origin_value("https://manager.sdkwork.com")
            .expect("configured production origin");
        policy
            .cors
            .validate_origin_value("https://admin.sdkwork.com")
            .expect("second configured production origin");
        policy
            .cors
            .validate_origin_value("https://evil.example.com")
            .expect_err("unconfigured production origin");
    }
}
