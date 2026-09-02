//! API assembly for sdkwork-web-framework.
//! Application bootstrap lives in `bootstrap.rs`; route inventory is in `assembly-manifest.json`.
// SDKWORK-ASSEMBLY-LIB-CUSTOM

mod bootstrap;
mod generated;

pub use bootstrap::{
    assemble_api_router, web_module, web_module_with_pool, ApiAssembly, ApiAssemblyContribution,
};

pub fn assembly_route_count() -> usize {
    generated::ROUTE_CRATE_COUNT
}
