//! OpenAPI authority contract for the framework control-plane backend-api.

use sdkwork_routes_web_framework_backend_api::paths;
use sdkwork_routes_web_framework_backend_api::ROUTES;
use sdkwork_web_contract::{
    build_openapi_document, validate_openapi_document_context_selectors,
    validate_openapi_routes_context_selectors, HttpMethod, OPENAPI_API_SURFACE_EXTENSION,
    OPENAPI_AUTH_MODE_EXTENSION, OPENAPI_PERMISSION_EXTENSION, OPENAPI_REQUEST_CONTEXT_EXTENSION,
};
use serde_json::Value;

#[test]
fn openapi_authority_matches_manifest_contract() {
    let doc = build_openapi_document("SDKWork Web Framework Control Plane", ROUTES);
    let paths = doc["paths"].as_object().expect("paths object");
    assert_eq!(ROUTES.len(), count_operations(paths));

    let sample = paths[paths::runtime_defaults::PATH]
        .as_object()
        .expect("runtime defaults path")["get"]
        .as_object()
        .expect("get operation");
    assert_eq!(
        "WebRequestContext",
        sample[OPENAPI_REQUEST_CONTEXT_EXTENSION].as_str().unwrap()
    );
    assert_eq!(
        "backend-api",
        sample[OPENAPI_API_SURFACE_EXTENSION].as_str().unwrap()
    );
    assert_eq!(
        "dual-token",
        sample[OPENAPI_AUTH_MODE_EXTENSION].as_str().unwrap()
    );
    validate_openapi_routes_context_selectors(ROUTES).expect("manifest paths");
    validate_openapi_document_context_selectors(&doc).expect("materialized openapi");
    validate_openapi_document_context_selectors(&read_json(authority_dir().join("openapi.json")))
        .expect("committed openapi authority");
}

#[test]
fn committed_openapi_authority_matches_runtime_contract() {
    let expected = build_openapi_document("SDKWork Web Framework Control Plane", ROUTES);
    let authority_dir = authority_dir();
    let committed = read_json(authority_dir.join("openapi.json"));
    assert_eq!(
        expected, committed,
        "apis/backend-api/web-framework/openapi.json is stale; run \
         cargo test -p sdkwork-routes-web-framework-backend-api materialize_openapi_authority_file -- --ignored"
    );
}

#[test]
fn committed_route_manifest_matches_runtime_contract() {
    let expected = manifest_rows();
    let authority_dir = authority_dir();
    let committed = read_json(authority_dir.join("routes.manifest.json"));
    let committed: Vec<Value> = committed
        .as_array()
        .cloned()
        .expect("routes.manifest.json must be a JSON array");
    assert_eq!(
        expected, committed,
        "apis/backend-api/web-framework/routes.manifest.json is stale; run \
         cargo test -p sdkwork-routes-web-framework-backend-api materialize_openapi_authority_file -- --ignored"
    );
}

#[test]
#[ignore = "run manually to refresh apis/backend-api/web-framework/openapi.json"]
fn materialize_openapi_authority_file() {
    let doc = build_openapi_document("SDKWork Web Framework Control Plane", ROUTES);
    let rendered = serde_json::to_string_pretty(&doc).expect("serialize openapi");
    let manifest = serde_json::to_string_pretty(&manifest_rows()).expect("serialize manifest");
    let root = authority_dir();
    std::fs::create_dir_all(&root).expect("create authority dir");
    std::fs::write(root.join("openapi.json"), rendered).expect("write openapi.json");
    std::fs::write(root.join("routes.manifest.json"), manifest).expect("write manifest");
}

#[test]
fn committed_openapi_declares_payload_too_large_on_body_routes() {
    let committed = read_json(authority_dir().join("openapi.json"));
    let paths = committed["paths"]
        .as_object()
        .expect("openapi paths must be an object");
    for route in ROUTES {
        if !matches!(
            route.method,
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
        ) {
            continue;
        }
        let path_entry = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("openapi missing path {}", route.path));
        let method = method_label(route.method);
        let operation = path_entry
            .get(method)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("openapi missing {method} on {}", route.path));
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(
            responses.contains_key("413"),
            "{} {} must declare 413 Payload Too Large",
            method,
            route.path
        );
    }
}

#[test]
fn committed_openapi_declares_unauthorized_on_all_routes() {
    assert_openapi_responses_on_all_routes("401", "Unauthorized");
}

#[test]
fn committed_openapi_declares_forbidden_on_all_routes() {
    assert_openapi_responses_on_all_routes("403", "Forbidden");
}

#[test]
fn committed_openapi_declares_bad_request_on_body_routes() {
    let committed = read_json(authority_dir().join("openapi.json"));
    let paths = committed["paths"]
        .as_object()
        .expect("openapi paths must be an object");
    for route in ROUTES {
        if !matches!(
            route.method,
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
        ) {
            continue;
        }
        let path_entry = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("openapi missing path {}", route.path));
        let method = method_label(route.method);
        let operation = path_entry
            .get(method)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("openapi missing {method} on {}", route.path));
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(
            responses.contains_key("400"),
            "{} {} must declare 400 Bad Request",
            method,
            route.path
        );
    }
}

#[test]
fn committed_openapi_post_with_path_param_declares_ok_not_created() {
    let committed = read_json(authority_dir().join("openapi.json"));
    let paths = committed["paths"]
        .as_object()
        .expect("openapi paths must be an object");
    for route in ROUTES {
        if route.method != HttpMethod::Post || !route.path.contains('{') {
            continue;
        }
        let path_entry = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("openapi missing path {}", route.path));
        let operation = path_entry
            .get("post")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("openapi missing post on {}", route.path));
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(
            responses.contains_key("200"),
            "post {} must declare 200 Success",
            route.path
        );
        assert!(
            !responses.contains_key("201"),
            "post {} must not declare 201 Created",
            route.path
        );
    }
}

#[test]
fn committed_openapi_declares_success_on_get_routes() {
    assert_openapi_responses_on_matching_routes(HttpMethod::Get, "200", "Success");
}

#[test]
fn committed_openapi_declares_success_on_put_routes() {
    assert_openapi_responses_on_matching_routes(HttpMethod::Put, "200", "Success");
}

#[test]
fn committed_openapi_declares_created_on_post_collection_routes() {
    let committed = read_json(authority_dir().join("openapi.json"));
    let paths = committed["paths"]
        .as_object()
        .expect("openapi paths must be an object");
    for route in ROUTES {
        if route.method != HttpMethod::Post || route.path.contains('{') {
            continue;
        }
        let path_entry = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("openapi missing path {}", route.path));
        let operation = path_entry
            .get("post")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("openapi missing post on {}", route.path));
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(
            responses.contains_key("201"),
            "post {} must declare 201 Created",
            route.path
        );
    }
}

#[test]
fn committed_openapi_declares_no_content_on_delete_routes() {
    let committed = read_json(authority_dir().join("openapi.json"));
    let paths = committed["paths"]
        .as_object()
        .expect("openapi paths must be an object");
    for route in ROUTES {
        if route.method != HttpMethod::Delete || !route.path.contains('{') {
            continue;
        }
        let path_entry = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("openapi missing path {}", route.path));
        let operation = path_entry
            .get("delete")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("openapi missing delete on {}", route.path));
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(
            responses.contains_key("204"),
            "delete {} must declare 204 No Content",
            route.path
        );
    }
}

#[test]
fn committed_openapi_declares_rate_limit_on_all_routes() {
    assert_openapi_responses_on_all_routes("429", "Too Many Requests");
}

#[test]
fn committed_openapi_declares_internal_error_on_all_routes() {
    assert_openapi_responses_on_all_routes("500", "Internal Server Error");
}

#[test]
fn committed_openapi_declares_dependency_unavailable_on_all_routes() {
    assert_openapi_responses_on_all_routes("503", "Service Unavailable");
}

#[test]
fn committed_openapi_declares_not_found_on_resource_mutation_routes() {
    let committed = read_json(authority_dir().join("openapi.json"));
    let paths = committed["paths"]
        .as_object()
        .expect("openapi paths must be an object");
    for route in ROUTES {
        if !route_may_return_not_found(route) {
            continue;
        }
        let path_entry = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("openapi missing path {}", route.path));
        let method = method_label(route.method);
        let operation = path_entry
            .get(method)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("openapi missing {method} on {}", route.path));
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(
            responses.contains_key("404"),
            "{} {} must declare 404 Not Found",
            method,
            route.path
        );
    }
}

#[test]
fn committed_openapi_declares_permission_extensions() {
    let committed = read_json(authority_dir().join("openapi.json"));
    let paths = committed["paths"]
        .as_object()
        .expect("openapi paths must be an object");
    for route in ROUTES {
        let Some(permission) = route.required_permission else {
            continue;
        };
        let path_entry = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("openapi missing path {}", route.path));
        let method = method_label(route.method);
        let operation = path_entry
            .get(method)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("openapi missing {method} on {}", route.path));
        assert_eq!(
            permission,
            operation
                .get(OPENAPI_PERMISSION_EXTENSION)
                .and_then(Value::as_str)
                .unwrap_or_else(|| {
                    panic!(
                        "{method} {} must declare {OPENAPI_PERMISSION_EXTENSION}",
                        route.path
                    )
                })
        );
    }
}

#[test]
fn committed_manifest_declares_required_permissions() {
    let committed = read_json(authority_dir().join("routes.manifest.json"));
    let rows = committed.as_array().expect("manifest array");
    for row in rows {
        let operation_id = row["operationId"].as_str().expect("operationId");
        let permission = row["requiredPermission"]
            .as_str()
            .unwrap_or_else(|| panic!("{operation_id} must declare requiredPermission"));
        assert!(
            permission.starts_with("web-framework."),
            "{operation_id} permission must be framework-scoped"
        );
    }
}

fn authority_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apis/backend-api/web-framework")
}

fn read_json(path: std::path::PathBuf) -> Value {
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn count_operations(paths: &serde_json::Map<String, Value>) -> usize {
    paths
        .values()
        .filter_map(Value::as_object)
        .map(|methods| methods.len())
        .sum()
}

fn manifest_rows() -> Vec<serde_json::Value> {
    ROUTES
        .iter()
        .map(|route| {
            let mut row = serde_json::json!({
                "method": method_label(route.method),
                "path": route.path,
                "operationId": route.operation_id,
                "auth": auth_mode_label(route.auth),
                "apiSurface": "backend-api",
                "requestContext": "WebRequestContext",
                "forbidCredentialHeaders": route.forbid_credential_headers,
                "requiredPermission": route.required_permission,
            });
            if let Some(alternate) = route.alternate_permissions {
                row["alternatePermissions"] = serde_json::json!(alternate);
            }
            row
        })
        .collect()
}

fn auth_mode_label(auth: sdkwork_web_contract::RouteAuth) -> &'static str {
    use sdkwork_web_contract::RouteAuth;
    match auth {
        RouteAuth::Public => "anonymous",
        RouteAuth::DualToken => "dual-token",
        RouteAuth::ApiKey => "api-key",
        RouteAuth::OAuth => "oauth",
        RouteAuth::OpenApiFlexible => "open-api-flexible",
        RouteAuth::RefreshToken => "refresh-token",
        // AgentToken maps to canonical api-key auth-mode (API_SPEC §19).
        RouteAuth::AgentToken => "api-key",
    }
}

fn assert_openapi_responses_on_matching_routes(method: HttpMethod, status_code: &str, label: &str) {
    let committed = read_json(authority_dir().join("openapi.json"));
    let paths = committed["paths"]
        .as_object()
        .expect("openapi paths must be an object");
    for route in ROUTES {
        if route.method != method {
            continue;
        }
        let path_entry = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("openapi missing path {}", route.path));
        let method_label = method_label(route.method);
        let operation = path_entry
            .get(method_label)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("openapi missing {method_label} on {}", route.path));
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(
            responses.contains_key(status_code),
            "{method_label} {} must declare {status_code} {label}",
            route.path
        );
    }
}

fn assert_openapi_responses_on_all_routes(status_code: &str, label: &str) {
    let committed = read_json(authority_dir().join("openapi.json"));
    let paths = committed["paths"]
        .as_object()
        .expect("openapi paths must be an object");
    for route in ROUTES {
        let path_entry = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("openapi missing path {}", route.path));
        let method = method_label(route.method);
        let operation = path_entry
            .get(method)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("openapi missing {method} on {}", route.path));
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(
            responses.contains_key(status_code),
            "{} {} must declare {status_code} {label}",
            method,
            route.path
        );
    }
}

fn route_may_return_not_found(route: &sdkwork_web_contract::HttpRoute) -> bool {
    route.path.contains('{')
        && matches!(
            route.method,
            HttpMethod::Delete | HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
        )
}

fn method_label(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "get",
        HttpMethod::Post => "post",
        HttpMethod::Put => "put",
        HttpMethod::Patch => "patch",
        HttpMethod::Delete => "delete",
    }
}
