pub mod dto;
pub mod handlers;
pub mod manifest;
pub mod paths;
pub mod persistence;
pub mod response;
pub mod routes;
pub mod services;
pub mod state;
pub mod tenant_scope;

pub use manifest::ROUTES;
pub use paths::API_PREFIX;
pub use routes::{build_admin_router, build_admin_router_with_options};
pub use state::WebFrameworkAdminState;

use axum::Router;
use sqlx::SqlitePool;

pub fn gateway_mount(pool: SqlitePool) -> Router {
    build_admin_router(pool)
}
