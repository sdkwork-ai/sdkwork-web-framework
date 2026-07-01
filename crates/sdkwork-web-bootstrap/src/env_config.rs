//! `SDKWORK_WEB_*` environment vocabulary (catalog H5).

use std::env;

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
