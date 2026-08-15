//! API assembly bootstrap for sdkwork-web-framework.
//! Authored bootstrap preserved by the assembly materializer.
//!
//! The assembly exports the indivisible `ApiAssemblyContribution` contract
//! (API_ASSEMBLY_SPEC.md §4): the executable business router, the route
//! manifest inventory, the derived OpenAPI document, the permission catalog,
//! domain context injectors, and the readiness check.

use std::sync::Arc;

use axum::Router;
use sdkwork_web_bootstrap::PgPoolReadinessCheck;
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
