//! Re-exports Axum extractors implemented in `sdkwork-web-core`.

pub use sdkwork_web_core::{RequireOpenApi, RequirePrincipal, RequireTenantApp, WebRequestContext};

/// Alias documenting automatic handler injection (see `WebRequestContext`).
pub type WebRequestContextExtractor = WebRequestContext;
