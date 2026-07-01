//! Contract fallback (F3) must be wired through bootstrap service router with Problem+json correlation.

use std::fs;
use std::path::PathBuf;

#[test]
fn service_router_mounts_contract_fallback_handler() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/sdkwork-web-bootstrap/src/router.rs");
    let source = fs::read_to_string(&path).expect("read router.rs");
    assert!(
        source.contains("contract_fallback"),
        "ServiceRouterConfig must expose contract_fallback"
    );
    assert!(
        source.contains(".fallback(") && source.contains("contract_fallback_handler"),
        "service_router must mount contract_fallback_handler as Axum fallback"
    );
}

#[test]
fn web_framework_builder_wires_contract_fallback_from_route_manifest() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/sdkwork-web-bootstrap/src/framework.rs");
    let source = fs::read_to_string(&path).expect("read framework.rs");
    assert!(
        source.contains("contract_fallback: Option<ContractFallbackConfig>"),
        "WebFramework must store contract_fallback config"
    );
    assert!(
        source.contains("ContractFallbackConfig::from_manifest"),
        "WebFrameworkBuilder must derive contract fallback from route_manifest"
    );
    assert!(
        source.contains("with_contract_fallback(fallback)"),
        "service_router_config must pass contract fallback to ServiceRouterConfig"
    );
}

#[test]
fn contract_fallback_handler_uses_problem_response_for_request() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/sdkwork-web-bootstrap/src/fallback.rs");
    let source = fs::read_to_string(&path).expect("read fallback.rs");
    assert!(
        source.contains("problem_response_for_request"),
        "contract fallback must use sdkwork-web-axum problem_response_for_request"
    );
    assert!(
        !source.contains("about:blank"),
        "contract fallback must not emit about:blank Problem types"
    );
    assert!(
        source.contains("WebFrameworkError::not_implemented"),
        "manifest-only routes must map to NotImplemented error kind"
    );
}

#[test]
fn enable_admin_api_auto_derives_route_manifest_for_contract_fallback() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/sdkwork-web-bootstrap/src/framework.rs");
    let source = fs::read_to_string(&path).expect("read framework.rs");
    assert!(
        source.contains("self.route_manifest.is_none() && self.admin_api_pool.is_some()"),
        "enable_admin_api must auto-derive route_manifest when none is set"
    );
    assert!(
        source.contains("sdkwork_routes_web_framework_backend_api::ROUTES"),
        "admin-api auto manifest must use backend-api ROUTES authority"
    );
}
