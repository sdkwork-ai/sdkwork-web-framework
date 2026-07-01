//! SDKWork web framework core: request context, interceptor pipeline, security policies.

pub mod api_chain;
pub mod axum_integration;
pub mod client_context_guard;
pub mod client_kind;
pub mod constants;
pub mod context_injection;
pub mod cors_policy;
pub mod error;
pub mod extractors;
pub mod hashing;
pub mod idempotency;
pub mod interceptors;
pub mod jwt;
pub mod jwt_claims;
pub mod jwt_fixtures;
pub mod jwt_tenant;
pub mod metrics;
pub mod open_api_auth;
pub mod parsers;
pub mod path_resource_guard;
pub mod policies;
pub mod policy_cache;
pub mod problem;
pub mod production_assembly;
pub mod rate_limit;
pub mod rate_limit_policy;
pub mod redact;
pub mod request_context;
pub mod request_identity;
pub mod resolvers;
pub mod route_manifest;
pub mod runtime_options;
pub mod security;
pub mod stores;
pub mod surface;
pub mod surface_bridge;
pub mod tenant_app_context;
pub mod tenant_runtime;
pub mod token_version;
pub mod trace;
pub mod websocket;
pub mod ws_interceptors;

#[cfg(test)]
mod pipeline_contract_tests;
#[cfg(test)]
mod tests;

mod exports;
pub use exports::*;
