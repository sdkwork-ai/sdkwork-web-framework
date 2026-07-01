//! OpenAPI extension helpers for SDKWork route manifests (WEB_FRAMEWORK_STANDARD §3.3 I6).

use crate::{ApiSurface, HttpMethod, HttpRoute, RateLimitTier, RouteAuth};
use serde_json::{json, Map, Value};

pub const OPENAPI_REQUEST_CONTEXT_EXTENSION: &str = "x-sdkwork-request-context";
pub const OPENAPI_API_SURFACE_EXTENSION: &str = "x-sdkwork-api-surface";
pub const OPENAPI_ROUTE_AUTH_EXTENSION: &str = "x-sdkwork-route-auth";
pub const OPENAPI_AUTH_MODE_EXTENSION: &str = "x-sdkwork-auth-mode";
pub const OPENAPI_FORBID_CREDENTIAL_HEADERS_EXTENSION: &str = "x-sdkwork-forbid-credential-headers";
pub const OPENAPI_RATE_LIMIT_TIER_EXTENSION: &str = "x-sdkwork-rate-limit-tier";

pub const OPENAPI_PERMISSION_EXTENSION: &str = "x-sdkwork-permission";
pub const OPENAPI_ALTERNATE_PERMISSIONS_EXTENSION: &str = "x-sdkwork-alternate-permissions";
pub const OPENAPI_REQUIRED_SURFACE_EXTENSION: &str = "x-sdkwork-required-surface";

const APP_API_PREFIX: &str = "/app/v3/api";
const BACKEND_API_PREFIX: &str = "/backend/v3/api";
const OPEN_API_PREFIX: &str = "/open/v3/api";
const GATEWAY_API_PREFIX: &str = "/v1";

const FORBIDDEN_CONTEXT_SELECTOR_QUERY_KEYS: &[&str] = &[
    "tenant_id",
    "tenantid",
    "tenant",
    "tenant-id",
    "app_id",
    "appid",
    "app-id",
    "organization_id",
    "organizationid",
    "organization-id",
    "org_id",
    "orgid",
    "user_id",
    "userid",
    "user-id",
    "session_id",
    "sessionid",
    "session-id",
];

const FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS: &[&str] = &["/tenants/", "/organizations/"];

/// Infer contract surface from a manifest path prefix.
pub fn infer_api_surface_from_path(path: &str) -> ApiSurface {
    if path.starts_with(APP_API_PREFIX) {
        ApiSurface::AppApi
    } else if path.starts_with(BACKEND_API_PREFIX) {
        ApiSurface::BackendApi
    } else if path.starts_with(OPEN_API_PREFIX) {
        ApiSurface::OpenApi
    } else if path.starts_with(GATEWAY_API_PREFIX) {
        ApiSurface::GatewayApi
    } else {
        ApiSurface::Unknown
    }
}

pub fn openapi_extensions_for_route(route: &HttpRoute) -> Map<String, Value> {
    let surface = infer_api_surface_from_path(route.path);
    let mut extensions = Map::new();
    extensions.insert(
        OPENAPI_REQUEST_CONTEXT_EXTENSION.to_owned(),
        Value::String("WebRequestContext".to_owned()),
    );
    extensions.insert(
        OPENAPI_API_SURFACE_EXTENSION.to_owned(),
        Value::String(api_surface_label(surface).to_owned()),
    );
    extensions.insert(
        OPENAPI_ROUTE_AUTH_EXTENSION.to_owned(),
        Value::String(route_auth_label(route.auth).to_owned()),
    );
    extensions.insert(
        OPENAPI_AUTH_MODE_EXTENSION.to_owned(),
        Value::String(auth_mode_label(route.auth).to_owned()),
    );
    if route.forbid_credential_headers {
        extensions.insert(
            OPENAPI_FORBID_CREDENTIAL_HEADERS_EXTENSION.to_owned(),
            Value::Bool(true),
        );
    }
    if let Some(tier) = route.rate_limit_tier {
        extensions.insert(
            OPENAPI_RATE_LIMIT_TIER_EXTENSION.to_owned(),
            Value::String(rate_limit_tier_label(tier).to_owned()),
        );
    }
    if let Some(permission) = route.required_permission {
        extensions.insert(
            OPENAPI_PERMISSION_EXTENSION.to_owned(),
            Value::String(permission.to_owned()),
        );
    }
    if let Some(alternate) = route.alternate_permissions {
        extensions.insert(
            OPENAPI_ALTERNATE_PERMISSIONS_EXTENSION.to_owned(),
            json!(alternate),
        );
    }
    if surface == ApiSurface::BackendApi && !route.auth.skips_credential_resolution() {
        extensions.insert(
            OPENAPI_REQUIRED_SURFACE_EXTENSION.to_owned(),
            Value::String("organizationMember".to_owned()),
        );
    }
    extensions
}

pub fn build_openapi_operation(route: &HttpRoute) -> Value {
    let mut operation = Map::new();
    operation.insert(
        "operationId".to_owned(),
        Value::String(route.operation_id.to_owned()),
    );
    operation.insert("tags".to_owned(), json!([route.tag]));
    operation.insert(
        "summary".to_owned(),
        Value::String(route.operation_id.to_owned()),
    );
    operation.insert("responses".to_owned(), openapi_responses_for_route(route));
    if let Some(parameters) = openapi_parameters_for_route(route) {
        operation.insert("parameters".to_owned(), parameters);
    }
    if route.auth.skips_credential_resolution() {
        operation.insert("security".to_owned(), json!([{ "sdkworkAccessToken": [] }]));
    } else if route.auth.requires_dual_token_headers() {
        operation.insert("security".to_owned(), json!([{ "sdkworkDualToken": [] }]));
    } else if route.auth.is_agent_token_credential_mode() {
        operation.insert("security".to_owned(), json!([{ "sdkworkAgentToken": [] }]));
    }
    for (key, value) in openapi_extensions_for_route(route) {
        operation.insert(key, value);
    }
    Value::Object(operation)
}

pub fn build_openapi_path_item(routes: &[HttpRoute]) -> Value {
    let mut item = Map::new();
    for route in routes {
        let method = http_method_label(route.method).to_owned();
        item.insert(method, build_openapi_operation(route));
    }
    Value::Object(item)
}

pub fn build_openapi_document(title: &str, routes: &[HttpRoute]) -> Value {
    validate_openapi_routes_context_selectors(routes)
        .expect("route manifest violates client context selector rules");
    let mut paths = Map::new();
    for route in routes {
        paths
            .entry(route.path.to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("path object")
            .insert(
                http_method_label(route.method).to_owned(),
                build_openapi_operation(route),
            );
    }
    let document = json!({
        "openapi": "3.1.0",
        "info": {
            "title": title,
            "version": "0.1.0"
        },
        "components": {
            "securitySchemes": {
                "sdkworkAccessToken": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "Access-Token",
                    "description": "Signed JWT access_token (`header.payload.signature`). Required on all non-open-api routes, including public and refresh-token entrypoints, for tenant isolation."
                },
                "sdkworkDualToken": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "Access-Token",
                    "description": "Protected routes require Authorization: Bearer <auth_token JWT> and Access-Token: <access_token JWT>."
                },
                "sdkworkAgentToken": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-SDKWork-Agent-Token",
                    "description": "Backend agent bootstrap token for /backend/v3/api/agent/* routes. Resolves via api-key auth-mode without dual-token JWT."
                }
            },
            "schemas": openapi_envelope_component_schemas()
        },
        "paths": paths
    });
    validate_openapi_document_context_selectors(&document)
        .expect("materialized OpenAPI violates client context selector rules");
    document
}

fn requires_context_selector_guard(surface: ApiSurface) -> bool {
    matches!(
        surface,
        ApiSurface::AppApi | ApiSurface::OpenApi | ApiSurface::GatewayApi
    )
}

fn normalize_selector_key(key: &str) -> String {
    key.chars()
        .filter(|ch| *ch != '_' && *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn forbidden_context_selector_keys() -> &'static [String] {
    use std::sync::OnceLock;
    static KEYS: OnceLock<Vec<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        FORBIDDEN_CONTEXT_SELECTOR_QUERY_KEYS
            .iter()
            .map(|key| normalize_selector_key(key))
            .collect()
    })
}

fn is_forbidden_context_selector_param(name: &str) -> bool {
    let normalized = normalize_selector_key(name);
    forbidden_context_selector_keys()
        .iter()
        .any(|candidate| candidate == &normalized)
}

/// Validates route manifest paths before OpenAPI materialization (B8 / API_SPEC §10.0).
pub fn validate_openapi_routes_context_selectors(routes: &[HttpRoute]) -> Result<(), String> {
    for route in routes {
        let surface = infer_api_surface_from_path(route.path);
        if !requires_context_selector_guard(surface) {
            continue;
        }
        let normalized = route.path.to_ascii_lowercase();
        for marker in FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS {
            if normalized.contains(marker) {
                return Err(format!(
                    "route {} {} uses forbidden ambient context path marker `{marker}`",
                    http_method_label(route.method),
                    route.path
                ));
            }
        }
    }
    Ok(())
}

/// Validates materialized OpenAPI documents forbid client context selector params on SaaS surfaces.
pub fn validate_openapi_document_context_selectors(document: &Value) -> Result<(), String> {
    let paths = document
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| "OpenAPI document missing paths object".to_owned())?;

    for (path, path_item) in paths {
        let surface = infer_api_surface_from_path(path);
        if !requires_context_selector_guard(surface) {
            continue;
        }
        let normalized = path.to_ascii_lowercase();
        for marker in FORBIDDEN_AMBIENT_CONTEXT_PATH_MARKERS {
            if normalized.contains(marker) {
                return Err(format!(
                    "OpenAPI path `{path}` uses forbidden ambient context path marker `{marker}`"
                ));
            }
        }

        let Some(path_item) = path_item.as_object() else {
            continue;
        };
        if let Some(parameters) = path_item.get("parameters").and_then(Value::as_array) {
            validate_openapi_parameters(path, parameters)?;
        }
        for (method, operation) in path_item {
            if matches!(
                method.as_str(),
                "get" | "post" | "put" | "patch" | "delete" | "head" | "options" | "trace"
            ) {
                if let Some(operation) = operation.as_object() {
                    if let Some(parameters) = operation.get("parameters").and_then(Value::as_array)
                    {
                        validate_openapi_parameters(path, parameters)?;
                    }
                    validate_openapi_request_body(path, operation)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_openapi_request_body(
    path: &str,
    operation: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    let Some(request_body) = operation.get("requestBody") else {
        return Ok(());
    };
    let Some(content) = request_body.get("content").and_then(Value::as_object) else {
        return Ok(());
    };
    for (media_type, media_value) in content {
        if !media_type.starts_with("application/json") {
            continue;
        }
        if let Some(schema) = media_value.get("schema") {
            validate_openapi_schema_context_selectors(path, schema)?;
        }
    }
    Ok(())
}

fn validate_openapi_schema_context_selectors(path: &str, schema: &Value) -> Result<(), String> {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for key in properties.keys() {
            if is_forbidden_context_selector_param(key) {
                return Err(format!(
                    "OpenAPI path `{path}` request body declares forbidden context selector field `{key}`"
                ));
            }
        }
    }
    if let Some(items) = schema.get("items") {
        validate_openapi_schema_context_selectors(path, items)?;
    }
    for combinator in ["allOf", "anyOf", "oneOf"] {
        if let Some(parts) = schema.get(combinator).and_then(Value::as_array) {
            for part in parts {
                validate_openapi_schema_context_selectors(path, part)?;
            }
        }
    }
    Ok(())
}

fn validate_openapi_parameters(path: &str, parameters: &[Value]) -> Result<(), String> {
    for parameter in parameters {
        let name = parameter
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let location = parameter
            .get("in")
            .and_then(Value::as_str)
            .unwrap_or("query");
        if location == "query" && is_forbidden_context_selector_param(name) {
            return Err(format!(
                "OpenAPI path `{path}` declares forbidden context selector query parameter `{name}`"
            ));
        }
        if location == "path" && is_forbidden_context_selector_param(name) {
            return Err(format!(
                "OpenAPI path `{path}` declares forbidden context selector path parameter `{name}`"
            ));
        }
    }
    Ok(())
}

fn route_may_return_not_found(route: &HttpRoute) -> bool {
    route.path.contains('{')
        && matches!(
            route.method,
            HttpMethod::Delete | HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
        )
}

fn route_accepts_request_body(route: &HttpRoute) -> bool {
    matches!(
        route.method,
        HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
    )
}

fn route_supports_list_query(route: &HttpRoute) -> bool {
    route.method == HttpMethod::Get && route.operation_id.ends_with(".list")
}

fn openapi_parameters_for_route(route: &HttpRoute) -> Option<Value> {
    if !route_supports_list_query(route) {
        return None;
    }
    let mut parameters = Vec::new();
    if route.operation_id != "webFramework.securityEvents.list" {
        parameters.push(json!({
            "name": "environment",
            "in": "query",
            "required": false,
            "schema": { "type": "string" }
        }));
    }
    if route.alternate_permissions.is_some() {
        parameters.push(json!({
            "name": "tenant_id",
            "in": "query",
            "required": false,
            "schema": { "type": "string" }
        }));
    }
    parameters.push(json!({
        "name": "limit",
        "in": "query",
        "required": false,
        "schema": { "type": "integer", "minimum": 1, "maximum": 200 }
    }));
    Some(Value::Array(parameters))
}

fn route_post_collection_may_return_ok(route: &HttpRoute) -> bool {
    route.method == HttpMethod::Post
        && !route.path.contains('{')
        && route.operation_id == "webFramework.controlNodes.register"
}

fn route_creates_resource(route: &HttpRoute) -> bool {
    route.method == HttpMethod::Post && !route.path.contains('{')
}

fn route_deletes_resource(route: &HttpRoute) -> bool {
    route.method == HttpMethod::Delete && route.path.contains('{')
}

fn openapi_responses_for_route(route: &HttpRoute) -> Value {
    let mut responses = Map::new();
    if route_creates_resource(route) {
        responses.insert("201".to_owned(), openapi_success_response(route, "Created"));
        if route_post_collection_may_return_ok(route) {
            responses.insert("200".to_owned(), openapi_success_response(route, "Success"));
        }
    } else if route_deletes_resource(route) {
        responses.insert("204".to_owned(), json!({ "description": "No Content" }));
    } else {
        responses.insert("200".to_owned(), openapi_success_response(route, "Success"));
    }
    responses.insert("401".to_owned(), openapi_problem_response("Unauthorized"));
    responses.insert("403".to_owned(), openapi_problem_response("Forbidden"));
    responses.insert(
        "429".to_owned(),
        openapi_problem_response("Too Many Requests"),
    );
    if route_accepts_request_body(route) || route_supports_list_query(route) {
        responses.insert("400".to_owned(), openapi_problem_response("Bad Request"));
        if route_accepts_request_body(route) {
            responses.insert(
                "413".to_owned(),
                openapi_problem_response("Payload Too Large"),
            );
        }
    }
    if route_may_return_not_found(route) {
        responses.insert("404".to_owned(), openapi_problem_response("Not Found"));
    }
    responses.insert(
        "503".to_owned(),
        openapi_problem_response("Service Unavailable"),
    );
    responses.insert(
        "500".to_owned(),
        openapi_problem_response("Internal Server Error"),
    );
    Value::Object(responses)
}

fn openapi_success_response(route: &HttpRoute, description: &str) -> Value {
    json!({
        "description": description,
        "content": {
            "application/json": {
                "schema": success_envelope_ref_for_route(route)
            }
        }
    })
}

fn openapi_problem_response(description: &str) -> Value {
    json!({
        "description": description,
        "content": {
            "application/problem+json": {
                "schema": { "$ref": "#/components/schemas/ProblemDetail" }
            }
        }
    })
}

fn success_envelope_ref_for_route(route: &HttpRoute) -> Value {
    if route.operation_id.ends_with(".list") {
        return json!({ "$ref": "#/components/schemas/SdkWorkListResponse" });
    }
    if is_command_operation(route) {
        return json!({ "$ref": "#/components/schemas/SdkWorkCommandResponse" });
    }
    json!({ "$ref": "#/components/schemas/SdkWorkResourceResponse" })
}

fn is_command_operation(route: &HttpRoute) -> bool {
    if route_creates_resource(route) {
        return false;
    }
    if route.method != HttpMethod::Post {
        return false;
    }
    let action = route.operation_id.rsplit('.').next().unwrap_or_default();
    matches!(
        action,
        "revoke"
            | "enable"
            | "disable"
            | "delete"
            | "heartbeat"
            | "verify"
            | "refresh"
            | "logout"
            | "provision"
    ) && !route.operation_id.ends_with(".create")
}

fn openapi_envelope_component_schemas() -> Map<String, Value> {
    let mut schemas = Map::new();
    schemas.insert(
        "SdkWorkApiResponse".to_owned(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["code", "data", "traceId"],
            "properties": {
                "code": {
                    "type": "integer",
                    "format": "int32",
                    "enum": [0],
                    "default": 0,
                    "minimum": 0,
                    "maximum": 0
                },
                "data": {
                    "description": "Operation-specific payload typed per response schema."
                },
                "traceId": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Server-owned request correlation id."
                }
            }
        }),
    );
    schemas.insert(
        "SdkWorkResourceData".to_owned(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["item"],
            "properties": {
                "item": {
                    "type": "object",
                    "additionalProperties": true,
                    "description": "Typed domain resource for the operation."
                }
            }
        }),
    );
    schemas.insert(
        "SdkWorkPageData".to_owned(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["items", "pageInfo"],
            "properties": {
                "items": {
                    "type": "array",
                    "items": { "type": "object", "additionalProperties": true }
                },
                "pageInfo": { "$ref": "#/components/schemas/PageInfo" }
            }
        }),
    );
    schemas.insert(
        "SdkWorkCommandData".to_owned(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["accepted"],
            "properties": {
                "accepted": { "type": "boolean", "const": true },
                "resourceId": { "type": "string" },
                "status": { "type": "string" }
            }
        }),
    );
    schemas.insert(
        "PageInfo".to_owned(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["mode"],
            "properties": {
                "mode": { "type": "string", "enum": ["offset", "cursor"] },
                "page": { "type": "integer", "minimum": 1 },
                "pageSize": { "type": "integer", "minimum": 1, "maximum": 200 },
                "totalItems": { "type": "string", "pattern": "^[0-9]+$" },
                "totalPages": { "type": "integer", "minimum": 0 },
                "nextCursor": { "type": ["string", "null"] },
                "hasMore": { "type": "boolean" }
            }
        }),
    );
    schemas.insert(
        "SdkWorkPlatformErrorCode".to_owned(),
        json!({
            "type": "integer",
            "format": "int32",
            "minimum": 40001,
            "maximum": 79999,
            "description": "Platform or domain error code per API_SPEC.md §15.3."
        }),
    );
    schemas.insert(
        "ProblemDetail".to_owned(),
        json!({
            "type": "object",
            "additionalProperties": true,
            "required": ["type", "title", "status", "code", "traceId"],
            "properties": {
                "type": { "type": "string", "format": "uri-reference" },
                "title": { "type": "string" },
                "status": { "type": "integer", "minimum": 100, "maximum": 599 },
                "detail": { "type": "string" },
                "instance": { "type": "string" },
                "code": { "$ref": "#/components/schemas/SdkWorkPlatformErrorCode" },
                "traceId": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Server-owned request correlation id."
                },
                "errors": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/FieldError" }
                }
            }
        }),
    );
    schemas.insert(
        "FieldError".to_owned(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["field", "message"],
            "properties": {
                "field": { "type": "string" },
                "message": { "type": "string" },
                "code": {
                    "type": "integer",
                    "format": "int32",
                    "minimum": 40011,
                    "maximum": 40099
                }
            }
        }),
    );
    schemas.insert(
        "SdkWorkResourceResponse".to_owned(),
        json!({
            "allOf": [
                { "$ref": "#/components/schemas/SdkWorkApiResponse" },
                {
                    "type": "object",
                    "required": ["data"],
                    "properties": {
                        "data": { "$ref": "#/components/schemas/SdkWorkResourceData" }
                    }
                }
            ]
        }),
    );
    schemas.insert(
        "SdkWorkListResponse".to_owned(),
        json!({
            "allOf": [
                { "$ref": "#/components/schemas/SdkWorkApiResponse" },
                {
                    "type": "object",
                    "required": ["data"],
                    "properties": {
                        "data": { "$ref": "#/components/schemas/SdkWorkPageData" }
                    }
                }
            ]
        }),
    );
    schemas.insert(
        "SdkWorkCommandResponse".to_owned(),
        json!({
            "allOf": [
                { "$ref": "#/components/schemas/SdkWorkApiResponse" },
                {
                    "type": "object",
                    "required": ["data"],
                    "properties": {
                        "data": { "$ref": "#/components/schemas/SdkWorkCommandData" }
                    }
                }
            ]
        }),
    );
    schemas
}

fn api_surface_label(surface: ApiSurface) -> &'static str {
    match surface {
        ApiSurface::OpenApi => "open-api",
        ApiSurface::AppApi => "app-api",
        ApiSurface::BackendApi => "backend-api",
        ApiSurface::GatewayApi => "gateway-api",
        ApiSurface::Unknown => "unknown",
    }
}

fn route_auth_label(auth: RouteAuth) -> &'static str {
    match auth {
        RouteAuth::Public => "public",
        RouteAuth::RefreshToken => "refresh-token",
        RouteAuth::DualToken => "dual-token",
        RouteAuth::ApiKey => "api-key",
        RouteAuth::OAuth => "oauth",
        RouteAuth::OpenApiFlexible => "open-api-flexible",
        RouteAuth::AgentToken => "agent-token",
    }
}

fn auth_mode_label(auth: RouteAuth) -> &'static str {
    match auth {
        RouteAuth::Public => "anonymous",
        RouteAuth::RefreshToken => "refresh-token",
        RouteAuth::DualToken => "dual-token",
        RouteAuth::ApiKey => "api-key",
        RouteAuth::OAuth => "oauth",
        RouteAuth::OpenApiFlexible => "open-api-flexible",
        // AgentToken maps to canonical api-key auth-mode (API_SPEC §19).
        RouteAuth::AgentToken => "api-key",
    }
}

fn rate_limit_tier_label(tier: RateLimitTier) -> &'static str {
    match tier {
        RateLimitTier::AuthCritical => "auth-critical",
        RateLimitTier::OpenApiDefault => "open-api-default",
        RateLimitTier::Upload => "upload",
        RateLimitTier::Search => "search",
        RateLimitTier::Bulk => "bulk",
        RateLimitTier::Worker => "worker",
        RateLimitTier::Internal => "internal",
    }
}

fn http_method_label(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "get",
        HttpMethod::Post => "post",
        HttpMethod::Put => "put",
        HttpMethod::Patch => "patch",
        HttpMethod::Delete => "delete",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpMethod, HttpRoute, RateLimitTier, RouteAuth};
    use serde_json::Value;

    #[test]
    fn openapi_resource_mutation_declares_not_found_response() {
        let route = HttpRoute::dual_token(
            HttpMethod::Delete,
            "/backend/v3/api/web-framework/control_nodes/{nodeId}",
            "WebFramework",
            "webFramework.controlNodes.delete",
        )
        .with_required_permission("web-framework.control-plane");
        let operation = build_openapi_operation(&route);
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(responses.contains_key("404"));
        assert!(responses.contains_key("503"));
    }

    #[test]
    fn openapi_delete_declares_no_content_response() {
        let route = HttpRoute::dual_token(
            HttpMethod::Delete,
            "/backend/v3/api/web-framework/control_nodes/{nodeId}",
            "WebFramework",
            "webFramework.controlNodes.delete",
        )
        .with_required_permission("web-framework.control-plane");
        let operation = build_openapi_operation(&route);
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(responses.contains_key("204"));
        assert!(responses.contains_key("404"));
    }

    #[test]
    fn openapi_post_collection_declares_created_response() {
        let route = HttpRoute::dual_token(
            HttpMethod::Post,
            "/backend/v3/api/web-framework/control_nodes",
            "WebFramework",
            "webFramework.controlNodes.register",
        )
        .with_required_permission("web-framework.control-plane");
        let operation = build_openapi_operation(&route);
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(responses.contains_key("201"));
        assert!(responses.contains_key("429"));
    }

    #[test]
    fn openapi_mutation_declares_bad_request_and_dependency_unavailable() {
        let route = HttpRoute::dual_token(
            HttpMethod::Put,
            "/backend/v3/api/web-framework/cors_policies",
            "WebFramework",
            "webFramework.corsPolicies.upsert",
        )
        .with_required_permission("web-framework.tenant.admin");
        let operation = build_openapi_operation(&route);
        let responses = operation
            .get("responses")
            .and_then(Value::as_object)
            .expect("responses");
        assert!(responses.contains_key("400"));
        assert!(responses.contains_key("413"));
        assert!(responses.contains_key("503"));
    }

    #[test]
    fn openapi_includes_rate_limit_tier_extension() {
        let route = HttpRoute::new(
            HttpMethod::Post,
            "/app/v3/api/auth/sessions",
            "Auth",
            "createSession",
            RouteAuth::Public,
        )
        .with_rate_limit_tier(RateLimitTier::AuthCritical);
        let operation = build_openapi_operation(&route);
        let object = operation.as_object().expect("operation object");
        assert_eq!(
            "auth-critical",
            object
                .get(OPENAPI_RATE_LIMIT_TIER_EXTENSION)
                .and_then(Value::as_str)
                .expect("rate limit tier extension")
        );
    }

    #[test]
    fn openapi_includes_permission_extension_when_route_requires_permission() {
        let route = HttpRoute::dual_token(
            HttpMethod::Get,
            "/backend/v3/api/iam/users",
            "iam",
            "users.list",
        )
        .with_required_permission("iam.users.read");
        let operation = build_openapi_operation(&route);
        let object = operation.as_object().expect("operation object");
        assert_eq!(
            "iam.users.read",
            object
                .get(OPENAPI_PERMISSION_EXTENSION)
                .and_then(Value::as_str)
                .expect("permission extension")
        );
        assert_eq!(
            "organizationMember",
            object
                .get(OPENAPI_REQUIRED_SURFACE_EXTENSION)
                .and_then(Value::as_str)
                .expect("required surface extension")
        );
    }

    #[test]
    fn openapi_public_route_declares_anonymous_security() {
        let route = HttpRoute::credential_entry_public(
            HttpMethod::Post,
            "/app/v3/api/auth/sessions",
            "Auth",
            "createSession",
        );
        let operation = build_openapi_operation(&route);
        let object = operation.as_object().expect("operation object");
        assert_eq!(
            Some(&json!([{ "sdkworkAccessToken": [] }])),
            object.get("security")
        );
        assert_eq!(
            "anonymous",
            object
                .get(OPENAPI_AUTH_MODE_EXTENSION)
                .and_then(Value::as_str)
                .expect("auth mode extension")
        );
        assert_eq!(
            Some(&Value::Bool(true)),
            object.get(OPENAPI_FORBID_CREDENTIAL_HEADERS_EXTENSION)
        );
    }

    #[test]
    fn openapi_extensions_use_kebab_case_surface_labels() {
        let route =
            HttpRoute::dual_token(HttpMethod::Get, "/app/v3/api/users", "Users", "listUsers");
        let operation = build_openapi_operation(&route);
        let object = operation.as_object().expect("operation object");
        assert_eq!(
            "app-api",
            object
                .get(OPENAPI_API_SURFACE_EXTENSION)
                .and_then(Value::as_str)
                .expect("api surface extension")
        );
    }

    #[test]
    fn openapi_extensions_are_flat_on_operation() {
        let route =
            HttpRoute::dual_token(HttpMethod::Get, "/app/v3/api/users", "Users", "listUsers");
        let operation = build_openapi_operation(&route);
        let object = operation.as_object().expect("operation object");
        assert_eq!(
            "WebRequestContext",
            object
                .get(OPENAPI_REQUEST_CONTEXT_EXTENSION)
                .and_then(Value::as_str)
                .expect("request context extension at operation root")
        );
        assert!(!object.contains_key("x-sdkwork-extensions"));
        assert_eq!(
            Some(&json!([{ "sdkworkDualToken": [] }])),
            object.get("security")
        );
    }

    #[test]
    fn openapi_document_includes_dual_token_security_scheme() {
        let route =
            HttpRoute::dual_token(HttpMethod::Get, "/app/v3/api/users", "Users", "listUsers");
        let doc = build_openapi_document("Test", &[route]);
        let schemes = doc
            .pointer("/components/securitySchemes/sdkworkDualToken/name")
            .and_then(Value::as_str)
            .expect("dual token security scheme");
        assert_eq!("Access-Token", schemes);
    }

    #[test]
    fn openapi_rejects_request_body_tenant_selector_fields() {
        let document = json!({
            "paths": {
                "/app/v3/api/users": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "tenantId": { "type": "string" },
                                            "displayName": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let error = validate_openapi_document_context_selectors(&document)
            .expect_err("tenant selector body field");
        assert!(error.contains("tenantId"));
    }

    #[test]
    fn openapi_rejects_path_tenant_selector_parameters() {
        let document = json!({
            "paths": {
                "/app/v3/api/resources/{tenantId}/items": {
                    "get": {
                        "parameters": [{
                            "name": "tenantId",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        }]
                    }
                }
            }
        });
        let error =
            validate_openapi_document_context_selectors(&document).expect_err("tenant path param");
        assert!(error.contains("tenantId"));
    }
}
