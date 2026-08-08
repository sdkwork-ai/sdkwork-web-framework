use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::Router;
use sdkwork_web_axum::with_web_request_context;
use sdkwork_web_contract::{
    build_owned_openapi_document, enrich_owned_openapi_document, merge_openapi_documents,
    normalize_route_path, route_inventory_from_openapi, route_inventory_from_routes, HttpRoute,
    OPENAPI_API_AUTHORITY_EXTENSION, OPENAPI_OWNER_EXTENSION,
};
use sdkwork_web_core::{DomainContextInjector, HttpRouteManifest, WebRequestContextResolver};
use serde_json::Value;

use crate::{
    mount_openapi_json, CompositeReadinessCheck, OpenApiMount, ReadinessCheck, WebFrameworkBuilder,
};

const INFRASTRUCTURE_PATHS: &[&str] = &["/healthz", "/livez", "/metrics", "/readyz"];

/// Indivisible host-neutral contribution exported by one application API assembly.
pub struct ApiAssemblyContribution {
    pub owner: &'static str,
    pub router: Router,
    pub route_manifest: HttpRouteManifest,
    pub openapi: Value,
    pub permission_catalog: Vec<&'static str>,
    pub domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
    pub readiness_check: Arc<dyn ReadinessCheck>,
}

impl ApiAssemblyContribution {
    pub fn from_manifest(
        owner: &'static str,
        title: &str,
        router: Router,
        route_manifest: HttpRouteManifest,
        domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
        readiness_check: Arc<dyn ReadinessCheck>,
    ) -> Result<Self, String> {
        let openapi = build_owned_openapi_document(title, owner, route_manifest.routes())?;
        let permission_catalog = permission_catalog(route_manifest.routes());
        Self::try_new(
            owner,
            router,
            route_manifest,
            openapi,
            permission_catalog,
            domain_context_injectors,
            readiness_check,
        )
    }

    pub fn from_openapi_documents(
        owner: &'static str,
        title: &str,
        router: Router,
        route_manifest: HttpRouteManifest,
        documents: Vec<Value>,
        domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
        readiness_check: Arc<dyn ReadinessCheck>,
    ) -> Result<Self, String> {
        if documents.is_empty() {
            return Err(format!(
                "{owner} must provide at least one authored OpenAPI document"
            ));
        }
        let mut owned_documents = Vec::with_capacity(documents.len());
        for (index, document) in documents.into_iter().enumerate() {
            let mut document =
                enrich_owned_openapi_document(document, owner, route_manifest.routes())?;
            remove_document_scoped_ownership(&mut document);
            owned_documents.push((format!("{owner}-{index}"), document));
        }
        let openapi = merge_openapi_documents(
            title,
            owned_documents
                .iter()
                .map(|(name, document)| (name.as_str(), document.clone()))
                .collect::<Vec<_>>(),
        )
        .map_err(|error| format!("{owner} authored OpenAPI merge failed: {error}"))?;
        let permission_catalog = permission_catalog(route_manifest.routes());
        Self::try_new(
            owner,
            router,
            route_manifest,
            openapi,
            permission_catalog,
            domain_context_injectors,
            readiness_check,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        owner: &'static str,
        router: Router,
        route_manifest: HttpRouteManifest,
        openapi: Value,
        permission_catalog: Vec<&'static str>,
        domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
        readiness_check: Arc<dyn ReadinessCheck>,
    ) -> Result<Self, String> {
        // Stamp per-operation ownership/authority extensions so every
        // contribution (authored files and dynamically built documents alike)
        // satisfies ownership validation. Idempotent for documents that were
        // already enriched by from_openapi_documents. Document-scoped
        // ownership stays removed, matching from_openapi_documents semantics.
        let mut openapi = enrich_owned_openapi_document(openapi, owner, route_manifest.routes())
            .map_err(|error| format!("{owner} OpenAPI enrichment failed: {error}"))?;
        remove_document_scoped_ownership(&mut openapi);
        let contribution = Self {
            owner,
            router,
            route_manifest,
            openapi,
            permission_catalog,
            domain_context_injectors,
            readiness_check,
        };
        contribution.validate()?;
        Ok(contribution)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_owner(self.owner)?;
        validate_manifest(self.owner, &self.route_manifest)?;

        let manifest_inventory = route_inventory_from_routes(self.route_manifest.routes());
        let openapi_inventory = route_inventory_from_openapi(&self.openapi)
            .map_err(|error| format!("{} OpenAPI inventory is invalid: {error}", self.owner))?;
        if manifest_inventory != openapi_inventory {
            return Err(format!(
                "{} route manifest and OpenAPI inventories differ",
                self.owner
            ));
        }

        validate_openapi_ownership(self.owner, &self.openapi)?;
        let expected_permissions = permission_catalog(self.route_manifest.routes());
        if self.permission_catalog != expected_permissions {
            return Err(format!(
                "{} permission catalog differs from its route manifest",
                self.owner
            ));
        }
        Ok(())
    }
}

fn remove_document_scoped_ownership(document: &mut Value) {
    let Some(document) = document.as_object_mut() else {
        return;
    };
    document.remove(OPENAPI_OWNER_EXTENSION);
    document.remove(OPENAPI_API_AUTHORITY_EXTENSION);
    if let Some(info) = document.get_mut("info").and_then(Value::as_object_mut) {
        info.remove(OPENAPI_OWNER_EXTENSION);
        info.remove(OPENAPI_API_AUTHORITY_EXTENSION);
    }
}

/// One selected gateway profile after all owner contributions have been validated and merged.
pub struct ComposedApiAssembly {
    pub owners: Vec<&'static str>,
    pub router: Router,
    pub route_manifest: HttpRouteManifest,
    pub openapi: Value,
    pub permission_catalog: Vec<&'static str>,
    pub domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
    pub readiness_check: Arc<dyn ReadinessCheck>,
}

/// A selected API profile after its complete contribution has been bound to one HTTP host.
pub struct HostedApiAssembly {
    pub owners: Vec<&'static str>,
    pub router: Router,
    pub route_manifest: HttpRouteManifest,
    pub openapi: Value,
    pub permission_catalog: Vec<&'static str>,
    pub domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
    pub readiness_check: Arc<dyn ReadinessCheck>,
}

impl ComposedApiAssembly {
    pub fn try_compose(
        title: &str,
        contributions: Vec<ApiAssemblyContribution>,
    ) -> Result<Self, String> {
        if contributions.is_empty() {
            return Err("at least one API assembly contribution must be selected".to_owned());
        }

        let mut owners = Vec::with_capacity(contributions.len());
        let mut owner_set = BTreeSet::new();
        let mut router = Router::new();
        let mut routes = Vec::new();
        let mut documents = Vec::with_capacity(contributions.len());
        let mut permissions = BTreeSet::new();
        let mut domain_context_injectors = Vec::new();
        let mut readiness_checks = Vec::with_capacity(contributions.len());

        for contribution in contributions {
            contribution.validate()?;
            if !owner_set.insert(contribution.owner) {
                return Err(format!(
                    "API assembly owner {} was selected more than once",
                    contribution.owner
                ));
            }
            owners.push(contribution.owner);
            router = router.merge(contribution.router);
            routes.extend_from_slice(contribution.route_manifest.routes());
            documents.push((contribution.owner, contribution.openapi));
            permissions.extend(contribution.permission_catalog);
            domain_context_injectors.extend(contribution.domain_context_injectors);
            readiness_checks.push(contribution.readiness_check);
        }

        let route_manifest = HttpRouteManifest::from_owned_routes(routes);
        validate_manifest("combined API profile", &route_manifest)?;
        let openapi = merge_openapi_documents(title, documents)
            .map_err(|error| format!("combined OpenAPI is invalid: {error}"))?;
        let combined_inventory = route_inventory_from_routes(route_manifest.routes());
        let openapi_inventory = route_inventory_from_openapi(&openapi)
            .map_err(|error| format!("combined OpenAPI inventory is invalid: {error}"))?;
        if combined_inventory != openapi_inventory {
            return Err("combined route manifest and OpenAPI inventories differ".to_owned());
        }

        Ok(Self {
            owners,
            router,
            route_manifest,
            openapi,
            permission_catalog: permissions.into_iter().collect(),
            domain_context_injectors,
            readiness_check: Arc::new(CompositeReadinessCheck::new(readiness_checks)),
        })
    }

    /// Consumes the complete selected profile and applies process HTTP infrastructure once.
    pub fn into_hosted<R>(self, mut framework: WebFrameworkBuilder<R>) -> HostedApiAssembly
    where
        R: WebRequestContextResolver + Clone + Any,
    {
        let Self {
            owners,
            router,
            route_manifest,
            openapi,
            permission_catalog,
            domain_context_injectors,
            readiness_check,
        } = self;

        framework = framework
            .route_manifest(route_manifest.clone())
            .readiness_check(readiness_check.clone());
        for injector in &domain_context_injectors {
            framework = framework.domain_injector(injector.clone());
        }

        let framework = framework.build();
        let router = framework.mount_contract_fallback(router);
        let router = with_web_request_context(router, framework.layer().clone());
        let router = mount_openapi_json(
            router,
            &[OpenApiMount {
                path: "/openapi.json",
                document: Arc::new(openapi.clone()),
            }],
        );
        let router = framework.mount_process_routes(router);

        HostedApiAssembly {
            owners,
            router,
            route_manifest,
            openapi,
            permission_catalog,
            domain_context_injectors,
            readiness_check,
        }
    }
}

pub fn permission_catalog(routes: &[HttpRoute]) -> Vec<&'static str> {
    let mut permissions = BTreeSet::new();
    for route in routes {
        if let Some(permission) = route.required_permission {
            permissions.insert(permission);
        }
        if let Some(alternate_permissions) = route.alternate_permissions {
            permissions.extend(alternate_permissions.iter().copied());
        }
    }
    permissions.into_iter().collect()
}

fn validate_owner(owner: &str) -> Result<(), String> {
    if owner.trim().is_empty() {
        return Err("API assembly owner must not be empty".to_owned());
    }
    if !owner.starts_with("sdkwork-") {
        return Err(format!(
            "API assembly owner {owner:?} must use the sdkwork- lower-kebab identity"
        ));
    }
    Ok(())
}

fn validate_manifest(owner: &str, manifest: &HttpRouteManifest) -> Result<(), String> {
    let mut collision_owners = BTreeMap::new();
    for route in manifest.routes() {
        route
            .validate_compatibility_contract()
            .map_err(|error| format!("{owner} route manifest is invalid: {error}"))?;
        let normalized = normalize_route_path(route.path);
        if INFRASTRUCTURE_PATHS.contains(&normalized.as_str()) {
            return Err(format!(
                "{owner} API assembly must not own infrastructure route {normalized}"
            ));
        }
        let key = collision_key(route);
        if let Some(first_operation) = collision_owners.insert(key.clone(), route.operation_id) {
            return Err(format!(
                "{owner} route collision for {} {} between {} and {}",
                key.0, key.1, first_operation, route.operation_id
            ));
        }
    }
    Ok(())
}

fn collision_key(route: &HttpRoute) -> (String, String) {
    let method = format!("{:?}", route.method).to_ascii_uppercase();
    let path = normalize_route_path(route.path)
        .split('/')
        .map(|segment| {
            if (segment.starts_with('{') && segment.ends_with('}'))
                || segment.starts_with(':')
                || (segment.starts_with('<') && segment.ends_with('>'))
            {
                "{param}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    (method, path)
}

fn validate_openapi_ownership(owner: &str, document: &Value) -> Result<(), String> {
    let paths = document
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{owner} OpenAPI document must contain a paths object"))?;
    for (path, path_item) in paths {
        let path_item = path_item
            .as_object()
            .ok_or_else(|| format!("{owner} OpenAPI path {path} must be an object"))?;
        for (method, operation) in path_item {
            if !matches!(method.as_str(), "delete" | "get" | "patch" | "post" | "put") {
                continue;
            }
            let operation = operation.as_object().ok_or_else(|| {
                format!(
                    "{owner} OpenAPI operation {} {path} must be an object",
                    method.to_uppercase()
                )
            })?;
            let operation_owner = operation
                .get(OPENAPI_OWNER_EXTENSION)
                .and_then(Value::as_str);
            if operation_owner != Some(owner) {
                return Err(format!(
                    "{owner} OpenAPI operation {} {path} must declare {OPENAPI_OWNER_EXTENSION}: {owner}",
                    method.to_uppercase()
                ));
            }
            let authority = operation
                .get(OPENAPI_API_AUTHORITY_EXTENSION)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "{owner} OpenAPI operation {} {path} lacks {OPENAPI_API_AUTHORITY_EXTENSION}",
                        method.to_uppercase()
                    )
                })?;
            if !authority.starts_with("sdkwork-") {
                return Err(format!(
                    "{owner} OpenAPI operation {} {path} has invalid API authority {authority:?}",
                    method.to_uppercase()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use sdkwork_web_contract::{HttpMethod, RouteAuth};
    use sdkwork_web_core::DefaultWebRequestContextResolver;
    use tower::ServiceExt;

    const ROUTES: &[HttpRoute] = &[HttpRoute::new(
        HttpMethod::Get,
        "/app/v3/api/widgets/{widgetId}",
        "Widgets",
        "widgets.retrieve",
        RouteAuth::DualToken,
    )
    .with_required_permission("widgets.read")];

    #[test]
    fn contribution_derives_aligned_contract_views() {
        let contribution = ApiAssemblyContribution::from_manifest(
            "sdkwork-widgets",
            "SDKWork Widgets API",
            Router::new(),
            HttpRouteManifest::new(ROUTES),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid contribution");

        assert_eq!(contribution.permission_catalog, vec!["widgets.read"]);
        assert_eq!(
            contribution.openapi["paths"]["/app/v3/api/widgets/{widgetId}"]["get"]
                [OPENAPI_OWNER_EXTENSION],
            "sdkwork-widgets"
        );
    }

    #[test]
    fn authored_openapi_contribution_preserves_complete_wire_contract() {
        const AUTHORED_ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Post,
            "/app/v3/api/widgets",
            "Widgets",
            "widgets.create",
            RouteAuth::DualToken,
        )
        .with_required_permission("widgets.write")];
        let authored = serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "Authored Widgets API", "version": "3.0.0" },
            "paths": {
                "/app/v3/api/widgets": {
                    "post": {
                        "operationId": "widgets.create",
                        "tags": ["Widgets"],
                        "requestBody": {
                            "required": true,
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/CreateWidgetRequest" },
                                    "example": { "name": "contract-first" }
                                }
                            }
                        },
                        "responses": {
                            "201": {
                                "description": "Created",
                                "content": {
                                    "application/json": {
                                        "schema": { "$ref": "#/components/schemas/WidgetEnvelope" },
                                        "example": { "code": 0, "data": { "id": "widget-1" } }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "components": {
                "schemas": {
                    "CreateWidgetRequest": {
                        "type": "object",
                        "required": ["name"],
                        "properties": { "name": { "type": "string", "minLength": 1 } }
                    },
                    "WidgetEnvelope": {
                        "type": "object",
                        "required": ["code", "data"],
                        "properties": {
                            "code": { "type": "integer", "format": "int32" },
                            "data": { "type": "object" }
                        }
                    }
                }
            }
        });

        let contribution = ApiAssemblyContribution::from_openapi_documents(
            "sdkwork-widgets",
            "SDKWork Widgets API",
            Router::new(),
            HttpRouteManifest::new(AUTHORED_ROUTES),
            vec![authored.clone()],
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid authored contribution");

        assert_eq!(
            contribution
                .openapi
                .pointer("/components/schemas/CreateWidgetRequest"),
            authored.pointer("/components/schemas/CreateWidgetRequest")
        );
        assert_eq!(
            contribution
                .openapi
                .pointer("/paths/~1app~1v3~1api~1widgets/post/requestBody"),
            authored.pointer("/paths/~1app~1v3~1api~1widgets/post/requestBody")
        );
        assert_eq!(
            contribution
                .openapi
                .pointer("/paths/~1app~1v3~1api~1widgets/post/responses"),
            authored.pointer("/paths/~1app~1v3~1api~1widgets/post/responses")
        );
        assert_eq!(
            contribution.openapi["paths"]["/app/v3/api/widgets"]["post"][OPENAPI_OWNER_EXTENSION],
            "sdkwork-widgets"
        );
        assert_eq!(
            contribution.openapi["paths"]["/app/v3/api/widgets"]["post"]
                [OPENAPI_API_AUTHORITY_EXTENSION],
            "sdkwork-widgets-app-api"
        );
    }

    #[test]
    fn authored_openapi_contribution_merges_distinct_surface_authorities() {
        const MULTI_SURFACE_ROUTES: &[HttpRoute] = &[
            HttpRoute::new(
                HttpMethod::Get,
                "/app/v3/api/widgets",
                "Widgets",
                "widgets.list",
                RouteAuth::DualToken,
            ),
            HttpRoute::new(
                HttpMethod::Get,
                "/backend/v3/api/widgets",
                "Widgets",
                "widgets.management.list",
                RouteAuth::DualToken,
            ),
        ];
        let app_api = serde_json::json!({
            "openapi": "3.1.2",
            "x-sdkwork-owner": "sdkwork-widgets",
            "x-sdkwork-api-authority": "sdkwork-widgets-app-api",
            "info": {
                "title": "Widgets App API",
                "version": "1.0.0",
                "x-sdkwork-owner": "sdkwork-widgets",
                "x-sdkwork-api-authority": "sdkwork-widgets-app-api"
            },
            "paths": {
                "/app/v3/api/widgets": {
                    "get": {
                        "operationId": "widgets.list",
                        "tags": ["Widgets"],
                        "responses": { "200": { "description": "OK" } },
                        "security": [{ "AuthToken": [], "AccessToken": [] }]
                    }
                }
            }
        });
        let backend_api = serde_json::json!({
            "openapi": "3.1.2",
            "x-sdkwork-owner": "sdkwork-widgets",
            "x-sdkwork-api-authority": "sdkwork-widgets-backend-api",
            "info": {
                "title": "Widgets Backend API",
                "version": "1.0.0",
                "x-sdkwork-owner": "sdkwork-widgets",
                "x-sdkwork-api-authority": "sdkwork-widgets-backend-api"
            },
            "paths": {
                "/backend/v3/api/widgets": {
                    "get": {
                        "operationId": "widgets.management.list",
                        "tags": ["Widgets"],
                        "responses": { "200": { "description": "OK" } },
                        "security": [{ "AuthToken": [], "AccessToken": [] }]
                    }
                }
            }
        });

        let contribution = ApiAssemblyContribution::from_openapi_documents(
            "sdkwork-widgets",
            "SDKWork Widgets API",
            Router::new(),
            HttpRouteManifest::new(MULTI_SURFACE_ROUTES),
            vec![app_api, backend_api],
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid multi-surface authored contribution");

        assert!(contribution.openapi["info"]
            .get(OPENAPI_API_AUTHORITY_EXTENSION)
            .is_none());
        assert!(contribution.openapi["info"]
            .get(OPENAPI_OWNER_EXTENSION)
            .is_none());
        assert!(contribution
            .openapi
            .get(OPENAPI_API_AUTHORITY_EXTENSION)
            .is_none());
        assert!(contribution.openapi.get(OPENAPI_OWNER_EXTENSION).is_none());
        assert_eq!(
            contribution.openapi["paths"]["/app/v3/api/widgets"]["get"]
                [OPENAPI_API_AUTHORITY_EXTENSION],
            "sdkwork-widgets-app-api"
        );
        assert_eq!(
            contribution.openapi["paths"]["/backend/v3/api/widgets"]["get"]
                [OPENAPI_API_AUTHORITY_EXTENSION],
            "sdkwork-widgets-backend-api"
        );
    }

    #[test]
    fn composition_rejects_cross_owner_route_collisions() {
        let build = |owner| {
            ApiAssemblyContribution::from_manifest(
                owner,
                "Widgets",
                Router::new(),
                HttpRouteManifest::new(ROUTES),
                Vec::new(),
                Arc::new(crate::AlwaysReady),
            )
            .expect("valid contribution")
        };

        let error = ComposedApiAssembly::try_compose(
            "Combined",
            vec![build("sdkwork-first"), build("sdkwork-second")],
        )
        .err()
        .expect("collision");
        assert!(error.contains("route collision"), "{error}");
    }

    #[tokio::test]
    async fn hosted_profile_binds_openapi_manifest_and_infrastructure_once() {
        const HOSTED_ROUTES: &[HttpRoute] = &[
            HttpRoute::new(
                HttpMethod::Get,
                "/app/v3/api/widgets",
                "Widgets",
                "widgets.list",
                RouteAuth::Public,
            ),
            HttpRoute::new(
                HttpMethod::Get,
                "/app/v3/api/widgets/{widgetId}",
                "Widgets",
                "widgets.retrieve",
                RouteAuth::DualToken,
            ),
        ];
        let contribution = ApiAssemblyContribution::from_manifest(
            "sdkwork-widgets",
            "SDKWork Widgets API",
            Router::new().route("/app/v3/api/widgets", get(|| async { "ok" })),
            HttpRouteManifest::new(HOSTED_ROUTES),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid contribution");
        let composed = ComposedApiAssembly::try_compose("SDKWork API", vec![contribution])
            .expect("valid composition");
        let hosted = composed.into_hosted(WebFrameworkBuilder::new(
            DefaultWebRequestContextResolver::default(),
        ));

        let response = hosted
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let document: Value = serde_json::from_slice(&body).expect("OpenAPI JSON");
        assert_eq!(
            document["paths"]["/app/v3/api/widgets"]["get"][OPENAPI_OWNER_EXTENSION],
            "sdkwork-widgets"
        );

        for path in ["/healthz", "/livez", "/readyz", "/metrics"] {
            let response = hosted
                .router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .expect("request"),
                )
                .await
                .expect("response");
            assert_eq!(response.status(), StatusCode::OK, "{path}");
        }

        let response = hosted
            .router
            .oneshot(
                Request::builder()
                    .uri("/app/v3/api/widgets/widget-1")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let problem: Value = serde_json::from_slice(&body).expect("problem JSON");
        assert_eq!(problem["operationId"], "widgets.retrieve");
        assert_eq!(problem["instance"], "GET /app/v3/api/widgets/{widgetId}");
    }
}
