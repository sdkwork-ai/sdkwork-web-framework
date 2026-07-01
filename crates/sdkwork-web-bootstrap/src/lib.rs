//! Service bootstrap: health, readiness, metrics, contract fallback, and `WebFramework` builder.

#[cfg(feature = "admin-api")]
mod admin_api;
mod env_config;
mod fallback;
mod framework;
mod health;
mod lifecycle;
mod observability;
mod openapi;
#[cfg(feature = "otel")]
mod otel;
#[cfg(feature = "redis")]
mod redis_readiness;
#[cfg(feature = "redis")]
mod redis_stores;
mod router;
mod serve;
#[cfg(feature = "sqlx")]
mod sqlx_readiness;
#[cfg(feature = "sqlx")]
mod sqlx_stores;
mod tracing_init;

#[cfg(feature = "admin-api")]
pub use admin_api::{mount_web_framework_admin_api, WebFrameworkAdminMount};
pub use env_config::WebFrameworkEnv;
pub use fallback::{contract_fallback_handler, ContractFallbackConfig};
pub use framework::{WebFramework, WebFrameworkBuilder};
pub use health::{
    healthz_handler, infra_public_path_prefixes, livez_handler, readyz_handler, AlwaysReady,
    CompositeReadinessCheck, ReadinessCheck, ReadinessFuture, READINESS_DEPENDENCY_UNAVAILABLE,
};
pub use lifecycle::{
    CompositeWebFrameworkLifecycle, NoOpWebFrameworkLifecycle, WebFrameworkLifecycle,
};
pub use observability::{metrics_handler, HttpMetricsRegistry};
pub use openapi::{mount_openapi_json, OpenApiMount};
#[cfg(feature = "otel")]
pub use otel::init_otel_tracing;
#[cfg(feature = "redis")]
pub use redis_readiness::RedisReadinessCheck;
#[cfg(feature = "redis")]
pub use redis_stores::{
    shared_concurrent_admission_store, shared_idempotency_store, shared_rate_limit_store,
    RedisConcurrentAdmissionStore, RedisIdempotencyStore, RedisRateLimitStore,
};
pub use router::{
    assemble_multi_surface_router, mount_infra_routes, service_router, ServiceRouterConfig,
};
pub use serve::{serve, serve_with_lifecycle};
#[cfg(feature = "sqlx")]
pub use sqlx_readiness::{PgPoolReadinessCheck, SqliteReadinessCheck};
#[cfg(feature = "sqlx")]
pub use sqlx_stores::{
    connect_sqlite, shared_audit_emitter as shared_sqlx_audit_emitter,
    shared_idempotency_store as shared_sqlx_idempotency_store,
    shared_rate_limit_store as shared_sqlx_rate_limit_store, shared_security_event_emitter,
    SqlxAuditEmitter, SqlxIdempotencyStore, SqlxRateLimitStore, SqlxSecurityEventEmitter,
};
pub use tracing_init::{init_tracing, init_tracing_from_env};

pub use sdkwork_web_axum::{
    run_websocket_session, with_request_timeout, with_server_request_identity,
    with_web_request_context, AppRequestContextLayer, RequirePrincipal, WebFrameworkLayer,
    WebRequestContextExtractor, WebSocketUpgradeLayer,
};
pub use sdkwork_web_contract::{
    build_openapi_document, build_openapi_operation, build_openapi_path_item,
    infer_api_surface_from_path, openapi_extensions_for_route, ApiSurface, HttpMethod, HttpRoute,
    IamHttpRoute, RouteAuth, OPENAPI_API_SURFACE_EXTENSION, OPENAPI_RATE_LIMIT_TIER_EXTENSION,
    OPENAPI_REQUEST_CONTEXT_EXTENSION, OPENAPI_ROUTE_AUTH_EXTENSION,
};
pub use sdkwork_web_core::*;
