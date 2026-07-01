use crate::handlers;
use crate::paths;
use crate::state::WebFrameworkAdminState;
use axum::routing::{delete, get, post};
use axum::Router;
use sqlx::SqlitePool;

pub fn build_admin_router(pool: SqlitePool) -> Router {
    build_admin_router_with_options(pool, None)
}

pub fn build_admin_router_with_options(
    pool: SqlitePool,
    policy_caches: Option<std::sync::Arc<sdkwork_web_core::DynamicPolicyCaches>>,
) -> Router {
    let mut state = WebFrameworkAdminState::new(pool);
    if let Some(caches) = policy_caches {
        state = state.with_policy_caches(caches);
    }
    Router::new()
        .route(
            paths::cors::PATH,
            get(handlers::list_cors_policies).put(handlers::upsert_cors_policy),
        )
        .route(
            paths::rate_limit::PATH,
            get(handlers::list_rate_limit_policies).put(handlers::upsert_rate_limit_policy),
        )
        .route(
            paths::tenant_runtime::PATH,
            get(handlers::list_tenant_runtime_profiles)
                .put(handlers::upsert_tenant_runtime_profile),
        )
        .route(
            paths::security_events::PATH,
            get(handlers::list_security_events),
        )
        .route(paths::audit_events::PATH, get(handlers::list_audit_events))
        .route(
            paths::control_nodes::COLLECTION,
            get(handlers::list_control_nodes).post(handlers::register_control_node),
        )
        .route(
            paths::control_nodes::HEARTBEAT,
            post(handlers::heartbeat_control_node),
        )
        .route(
            paths::control_nodes::BY_ID,
            delete(handlers::delete_control_node),
        )
        .route(
            paths::runtime_defaults::PATH,
            get(handlers::runtime_defaults_snapshot),
        )
        .route(
            paths::optional_features::PATH,
            get(handlers::optional_features_snapshot),
        )
        .with_state(state)
}
