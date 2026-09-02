//! API assembly bootstrap for sdkwork-web-framework.
//! Authored bootstrap preserved by the assembly materializer.
//!
//! The assembly exports the indivisible `ApiAssemblyContribution` contract
//! (API_ASSEMBLY_SPEC.md §4): the executable business router, the route
//! manifest inventory, the derived OpenAPI document, the permission catalog,
//! domain context injectors, and the readiness check.

use std::sync::Arc;

use axum::Router;
use sdkwork_web_bootstrap::{PgPoolReadinessCheck, WebModule};
use sdkwork_web_core::HttpRouteManifest;
use sdkwork_web_framework_admin_repository_sqlx::AdminStorePool;

pub use sdkwork_web_bootstrap::ApiAssemblyContribution;

pub type ApiAssembly = ApiAssemblyContribution;

pub fn assemble_api_router(pool: AdminStorePool) -> Result<ApiAssembly, String> {
    let router = Router::new().merge(sdkwork_routes_web_framework_backend_api::gateway_mount(
        pool.clone(),
    ));
    let readiness = match pool {
        AdminStorePool::Postgres(postgres_pool) => Arc::new(PgPoolReadinessCheck::new(
            postgres_pool,
        )),
    };
    ApiAssemblyContribution::from_manifest(
        "sdkwork-web-framework",
        "SDKWork Web Framework Admin API",
        router,
        HttpRouteManifest::from_owned_routes(
            sdkwork_routes_web_framework_backend_api::ROUTES.to_vec(),
        ),
        Vec::new(),
        readiness,
    )
}

/// Installs the Web Framework admin surface as a Web Module on a
/// caller-supplied admin store pool (API_ASSEMBLY_SPEC §4.1.1).
pub fn web_module_with_pool(pool: AdminStorePool) -> Result<WebModule, String> {
    Ok(WebModule::from_contribution(assemble_api_router(pool)?))
}

/// Canonical Web Module definition for this application
/// (API_ASSEMBLY_SPEC §4.1.1): the complete HTTP surface — every route,
/// manifest, and OpenAPI document of this owner — as one installable module.
///
/// The admin store pool is bootstrapped from the process environment through
/// the Web Store database host, so the module owns its own bootstrap instead of
/// depending on a running admin listener.
pub async fn web_module() -> Result<WebModule, String> {
    let host = sdkwork_webstore_database_host::bootstrap_webstore_database_from_env().await?;
    let postgres = host
        .pool()
        .as_postgres()
        .ok_or_else(|| "web framework admin API requires a PostgreSQL database profile".to_owned())?
        .clone();
    web_module_with_pool(AdminStorePool::Postgres(postgres))
}
