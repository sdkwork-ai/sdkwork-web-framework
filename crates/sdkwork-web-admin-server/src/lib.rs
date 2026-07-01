//! Control-plane admin server assembly (`control_plane_standalone` profile).

use sdkwork_routes_web_framework_backend_api::ROUTES;
use sdkwork_web_bootstrap::{
    connect_sqlite, CompositeReadinessCheck, RedisReadinessCheck, SqliteReadinessCheck,
    WebFramework, WebFrameworkEnv,
};
use sdkwork_web_core::{
    tenant_bound_verifying_web_request_resolver, DisabledApiKeyLookupService,
    EnvBootstrapTenantSigningKeyLookup, HttpRouteManifest, ManifestAuthorizationPolicy,
    WebFrameworkOptionalFeatures, WebRequestContextResolver,
};
use sdkwork_web_store_sqlx::{
    shared_audit_emitter, shared_dynamic_policy_bundle,
    shared_idempotency_store as sqlx_idempotency_store,
    shared_rate_limit_store as sqlx_rate_limit_store, shared_security_event_emitter,
};
use sqlx::SqlitePool;
use std::any::Any;
use std::sync::Arc;

/// Built control-plane stack: framework runtime + SQLx pool backing admin tables.
pub struct ControlPlaneAssembly<R>
where
    R: WebRequestContextResolver + Clone + Any,
{
    pub framework: WebFramework<R>,
    pub pool: SqlitePool,
}

/// Assembles the standalone admin/control-plane `WebFramework` profile.
///
/// Uses `control_plane_standalone()` production features: bootstrap JWT, SQLx stores
/// when Redis is absent, `DisabledApiKeyLookupService`, and wired readiness probes.
pub async fn assemble_control_plane(
    env: &WebFrameworkEnv,
) -> Result<
    ControlPlaneAssembly<impl WebRequestContextResolver + Any + 'static>,
    Box<dyn std::error::Error>,
> {
    let database_url = env
        .store_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("sqlite:./data/web-framework.db?mode=rwc");
    let jwt_secret = env
        .jwt_hs256_secret
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(
            "SDKWORK_WEB_FRAMEWORK_JWT_HS256_SECRET is required for production admin-server assembly",
        )?;
    let bootstrap_tenant_id = env
        .jwt_bootstrap_tenant_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("bootstrap");
    let bootstrap_key_id = env
        .jwt_bootstrap_key_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("bootstrap");

    let pool_size = env.store_pool_size.unwrap_or(8);
    let pool = connect_sqlite(database_url, pool_size).await?;
    let policy_bundle = shared_dynamic_policy_bundle(pool.clone());
    let manifest = HttpRouteManifest::new(ROUTES);
    let lookup =
        EnvBootstrapTenantSigningKeyLookup::new(bootstrap_tenant_id, bootstrap_key_id, jwt_secret);
    let resolver = tenant_bound_verifying_web_request_resolver(lookup, DisabledApiKeyLookupService);

    let (rate_limit_store, idempotency_store, concurrent_admission_store) = if let Some(redis_url) =
        env.redis_url
            .as_ref()
            .filter(|value| !value.trim().is_empty())
    {
        use sdkwork_web_store_redis::{
            shared_concurrent_admission_store as redis_concurrent_admission_store,
            shared_idempotency_store as redis_idempotency_store,
            shared_rate_limit_store as redis_rate_limit_store,
        };
        (
            redis_rate_limit_store(redis_url, "sdkwork:admin")?,
            redis_idempotency_store(redis_url, "sdkwork:admin")?,
            Some(redis_concurrent_admission_store(
                redis_url,
                "sdkwork:admin",
            )?),
        )
    } else {
        (
            sqlx_rate_limit_store(pool.clone()),
            sqlx_idempotency_store(pool.clone()),
            None,
        )
    };

    let sqlite_readiness = Arc::new(SqliteReadinessCheck::new(pool.clone()))
        as Arc<dyn sdkwork_web_bootstrap::ReadinessCheck>;
    let readiness: Arc<dyn sdkwork_web_bootstrap::ReadinessCheck> = match env
        .redis_url
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(redis_url) => {
            let redis_readiness = Arc::new(RedisReadinessCheck::new(redis_url)?)
                as Arc<dyn sdkwork_web_bootstrap::ReadinessCheck>;
            Arc::new(CompositeReadinessCheck::new(vec![
                sqlite_readiness,
                redis_readiness,
            ]))
        }
        None => sqlite_readiness,
    };

    let mut builder = WebFramework::builder(resolver)
        .production_defaults()
        .optional_features(
            WebFrameworkOptionalFeatures::production_sqlx().control_plane_standalone(),
        )
        .authorization_policy(Arc::new(ManifestAuthorizationPolicy::new(manifest)))
        .rate_limit_store(rate_limit_store)
        .idempotency_store(idempotency_store);
    if let Some(store) = concurrent_admission_store {
        builder = builder.concurrent_admission_store(store);
    }
    let framework = builder
        .audit_emitter(shared_audit_emitter(pool.clone()))
        .security_event_emitter(shared_security_event_emitter(pool.clone()))
        .dynamic_cors_policy_source(policy_bundle.cors_policy_source)
        .dynamic_rate_limit_policy_source(policy_bundle.rate_limit_policy_source)
        .dynamic_tenant_runtime_profile_source(policy_bundle.tenant_runtime_profile_source)
        .readiness_check(readiness)
        .enable_admin_api(pool.clone())
        .admin_policy_caches(policy_bundle.caches)
        .build();

    Ok(ControlPlaneAssembly { framework, pool })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn assembles_sqlite_control_plane_without_redis() {
        let env = WebFrameworkEnv {
            store_url: Some("sqlite::memory:".to_owned()),
            store_pool_size: Some(1),
            jwt_hs256_secret: Some("test-bootstrap-secret-with-sufficient-length".to_owned()),
            ..WebFrameworkEnv::default()
        };
        let assembly = assemble_control_plane(&env)
            .await
            .expect("control plane assembly");
        let router = assembly.framework.mount_admin_routes(axum::Router::new());
        assert!(!format!("{router:?}").is_empty());
    }

    #[tokio::test]
    async fn control_plane_mount_serves_health_and_readyz() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use axum::Router;
        use tower::ServiceExt;

        let env = WebFrameworkEnv {
            store_url: Some("sqlite::memory:".to_owned()),
            store_pool_size: Some(1),
            jwt_hs256_secret: Some("test-bootstrap-secret-with-sufficient-length".to_owned()),
            ..WebFrameworkEnv::default()
        };
        let assembly = assemble_control_plane(&env)
            .await
            .expect("control plane assembly");
        let app = assembly
            .framework
            .mount_service_routes(assembly.framework.mount_admin_routes(Router::new()));

        let health = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("healthz");
        assert_eq!(StatusCode::OK, health.status());

        let ready = app
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("readyz");
        assert_eq!(StatusCode::OK, ready.status());
    }

    #[tokio::test]
    async fn rejects_missing_jwt_secret() {
        let env = WebFrameworkEnv {
            store_url: Some("sqlite::memory:".to_owned()),
            ..WebFrameworkEnv::default()
        };
        let result = assemble_control_plane(&env).await;
        assert!(result.is_err());
        assert!(result
            .err()
            .expect("error")
            .to_string()
            .contains("JWT_HS256_SECRET"));
    }
}
