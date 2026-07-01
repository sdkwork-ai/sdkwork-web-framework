//! Mounted admin router paths must match the route manifest contract.

use sdkwork_routes_web_framework_backend_api::paths;
use sdkwork_routes_web_framework_backend_api::ROUTES;
use sdkwork_web_contract::HttpMethod;
use std::collections::HashMap;

#[test]
fn route_manifest_paths_use_framework_api_prefix() {
    for route in ROUTES {
        assert!(
            route.path.starts_with(paths::API_PREFIX),
            "route {} must use {}",
            route.operation_id,
            paths::API_PREFIX
        );
    }
}

#[test]
fn control_node_paths_use_shared_path_constants() {
    let delete = ROUTES
        .iter()
        .find(|route| route.operation_id == "webFramework.controlNodes.delete")
        .expect("delete route");
    let heartbeat_route = ROUTES
        .iter()
        .find(|route| route.operation_id == "webFramework.controlNodes.heartbeat")
        .expect("heartbeat route");

    assert_eq!(paths::control_nodes::BY_ID, delete.path);
    assert_eq!(HttpMethod::Delete, delete.method);
    assert_eq!(paths::control_nodes::HEARTBEAT, heartbeat_route.path);
    assert_eq!(HttpMethod::Post, heartbeat_route.method);
}

#[test]
fn route_manifest_declares_fourteen_control_plane_operations() {
    assert_eq!(14, ROUTES.len());
}

#[test]
fn route_manifest_api_prefix_matches_paths_module() {
    assert!(
        paths::cors::PATH.starts_with(paths::API_PREFIX),
        "paths module constants must share API_PREFIX"
    );
    for route in ROUTES {
        assert!(
            route.path.starts_with(paths::API_PREFIX),
            "route {} must use paths::API_PREFIX",
            route.operation_id
        );
    }
}

#[test]
fn route_manifest_paths_match_paths_module_constants() {
    let expected = HashMap::from([
        ("webFramework.corsPolicies.list", paths::cors::PATH),
        ("webFramework.corsPolicies.upsert", paths::cors::PATH),
        (
            "webFramework.rateLimitPolicies.list",
            paths::rate_limit::PATH,
        ),
        (
            "webFramework.rateLimitPolicies.upsert",
            paths::rate_limit::PATH,
        ),
        (
            "webFramework.tenantRuntimeProfiles.list",
            paths::tenant_runtime::PATH,
        ),
        (
            "webFramework.tenantRuntimeProfiles.upsert",
            paths::tenant_runtime::PATH,
        ),
        (
            "webFramework.securityEvents.list",
            paths::security_events::PATH,
        ),
        ("webFramework.auditEvents.list", paths::audit_events::PATH),
        (
            "webFramework.controlNodes.list",
            paths::control_nodes::COLLECTION,
        ),
        (
            "webFramework.controlNodes.register",
            paths::control_nodes::COLLECTION,
        ),
        (
            "webFramework.controlNodes.heartbeat",
            paths::control_nodes::HEARTBEAT,
        ),
        (
            "webFramework.controlNodes.delete",
            paths::control_nodes::BY_ID,
        ),
        (
            "webFramework.runtimeDefaults.snapshot",
            paths::runtime_defaults::PATH,
        ),
        (
            "webFramework.optionalFeatures.snapshot",
            paths::optional_features::PATH,
        ),
    ]);

    for route in ROUTES {
        assert_eq!(
            expected.get(route.operation_id).copied(),
            Some(route.path),
            "route {} must use paths module constant",
            route.operation_id
        );
    }
}
