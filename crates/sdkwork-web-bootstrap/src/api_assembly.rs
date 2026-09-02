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
use sdkwork_web_core::{
    DomainContextInjector, HttpRouteManifest, WebRequestContextProfile, WebRequestContextResolver,
};
pub use sdkwork_web_core::RouteManifestMount;
use serde_json::Value;

use crate::{
    mount_openapi_json, CompositeReadinessCheck, OpenApiMount, ReadinessCheck, WebFrameworkBuilder,
};

const INFRASTRUCTURE_PATHS: &[&str] = &["/healthz", "/livez", "/metrics", "/readyz"];

/// Merges a host-owned manifest with dependency manifests using the shared Web Framework contract.
pub fn merge_route_manifest_mounts(
    owner: &str,
    base: HttpRouteManifest,
    mounts: &[RouteManifestMount],
) -> Result<HttpRouteManifest, String> {
    let composed = HttpRouteManifest::try_merge_mounts(owner, base, mounts)?;
    composed.validate_includes_dependency_manifests(mounts)?;
    Ok(composed)
}

/// Merges dependency manifests and validates the result before binding it to a Web Framework layer.
///
/// Host gateways that mount same-origin dependency routers `MUST` use this helper (or
/// [`merge_route_manifest_mounts`]) so mounted routes keep their declared `RouteAuth`
/// instead of falling through to dual-token defaults on unmatched app-api paths.
pub fn prepare_host_route_manifest(
    owner: &str,
    base: HttpRouteManifest,
    mounts: &[RouteManifestMount],
    profile: &WebRequestContextProfile,
    public_path_prefixes: &[String],
) -> Result<HttpRouteManifest, String> {
    let composed = merge_route_manifest_mounts(owner, base, mounts)?;
    finalize_host_route_manifest(owner, composed, mounts, profile, public_path_prefixes)
}

/// Validates an already-composed host manifest before binding it to a Web Framework layer.
///
/// Use this when the host already merged dependency manifests through
/// [`HttpRouteManifest::try_merge_mounts`] and must not merge the same mounts twice.
pub fn finalize_host_route_manifest(
    owner: &str,
    composed: HttpRouteManifest,
    mounts: &[RouteManifestMount],
    profile: &WebRequestContextProfile,
    public_path_prefixes: &[String],
) -> Result<HttpRouteManifest, String> {
    composed.validate_includes_dependency_manifests(mounts)?;
    composed.validate_public_path_prefixes(public_path_prefixes)?;
    composed
        .validate_route_auth_for_surfaces(profile)
        .map_err(|error| {
            format!("{owner} composed route manifest failed surface auth validation: {error}")
        })?;
    Ok(composed)
}

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

    /// Atomically merges a same-origin dependency contribution into this host contribution.
    ///
    /// Router and route manifest stay paired so mounted dependency routes cannot lose their
    /// declared `RouteAuth` when the host binds a Web Framework layer
    /// (`API_ASSEMBLY_SPEC` §4/§6.1).
    pub fn merge_dependency_contribution(
        mut self,
        dependency: &ApiAssemblyContribution,
    ) -> Result<Self, String> {
        self.route_manifest = merge_route_manifest_mounts(
            self.owner,
            self.route_manifest,
            &[RouteManifestMount {
                owner: dependency.owner,
                manifest: dependency.route_manifest.clone(),
            }],
        )?;
        self.router = self.router.merge(dependency.router.clone());
        self.permission_catalog = permission_catalog(self.route_manifest.routes());
        self.domain_context_injectors
            .extend(dependency.domain_context_injectors.clone());
        self.readiness_check = Arc::new(CompositeReadinessCheck::new(vec![
            self.readiness_check,
            dependency.readiness_check.clone(),
        ]));
        validate_manifest(self.owner, &self.route_manifest)?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_owner(self.owner)?;
        validate_manifest(self.owner, &self.route_manifest)?;

        let manifest_inventory = route_inventory_from_routes(self.route_manifest.routes());
        let openapi_inventory = route_inventory_from_openapi(&self.openapi)
            .map_err(|error| format!("{} OpenAPI inventory is invalid: {error}", self.owner))?;
        if manifest_inventory != openapi_inventory {
            let manifest_set = manifest_inventory.iter().collect::<BTreeSet<_>>();
            let openapi_set = openapi_inventory.iter().collect::<BTreeSet<_>>();
            let manifest_only = manifest_set
                .difference(&openapi_set)
                .take(10)
                .map(|entry| format!("{} {} {} ({}, {})", entry.surface, entry.method, entry.normalized_path, entry.operation_id, entry.auth_profile))
                .collect::<Vec<_>>()
                .join("; ");
            let openapi_only = openapi_set
                .difference(&manifest_set)
                .take(10)
                .map(|entry| format!("{} {} {} ({}, {})", entry.surface, entry.method, entry.normalized_path, entry.operation_id, entry.auth_profile))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!(
                "{} route manifest and OpenAPI inventories differ (manifest={}, OpenAPI={}); manifest-only: [{}]; OpenAPI-only: [{}]",
                self.owner,
                manifest_inventory.len(),
                openapi_inventory.len(),
                manifest_only,
                openapi_only,
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

/// One independently installable Web module: a single owner application's
/// complete HTTP surface set.
///
/// A module is the SDKWork equivalent of a FastAPI `APIRouter` bundle or a
/// NestJS module: it owns its app-api, backend-api, open-api (and any other
/// declared surface) contributions behind one identity and is installed as a
/// whole. Hosts never assemble a module's routes surface by surface; they
/// register the module once with [`ApiModuleRegistry::add_module`].
///
/// Surfaces are contributed as [`ApiAssemblyContribution`] values. A module
/// with exactly one served owner usually holds one contribution per selected
/// surface owner (for example `sdkwork-community` plus `sdkwork-community-open`).
pub struct WebModule {
    owner: &'static str,
    title: String,
    contributions: Vec<ApiAssemblyContribution>,
}

impl WebModule {
    pub fn new(owner: &'static str, title: impl Into<String>) -> Self {
        Self {
            owner,
            title: title.into(),
            contributions: Vec::new(),
        }
    }

    /// Builds a module from a single prepared contribution (the common case).
    pub fn from_contribution(contribution: ApiAssemblyContribution) -> Self {
        let owner = contribution.owner;
        Self::new(owner, owner).with_surface(contribution)
    }

    /// Adds one surface contribution to the module.
    pub fn with_surface(mut self, contribution: ApiAssemblyContribution) -> Self {
        self.contributions.push(contribution);
        self
    }

    /// Adds every surface contribution in order.
    pub fn with_surfaces<I>(mut self, contributions: I) -> Self
    where
        I: IntoIterator<Item = ApiAssemblyContribution>,
    {
        self.contributions.extend(contributions);
        self
    }

    pub fn owner(&self) -> &'static str {
        self.owner
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn contributions(&self) -> &[ApiAssemblyContribution] {
        &self.contributions
    }

    /// Installs this module into a registry.
    pub fn install_into(self, registry: &mut ApiModuleRegistry) -> &mut ApiModuleRegistry {
        registry.add_module(self)
    }
}

impl From<ApiAssemblyContribution> for WebModule {
    fn from(contribution: ApiAssemblyContribution) -> Self {
        Self::from_contribution(contribution)
    }
}

/// Ordered registry of API assembly modules (API_ASSEMBLY_SPEC §4.1.1).
///
/// Hosts (standalone gateways and cloud gateways alike) assemble their HTTP
/// surface by registering each module's [`ApiAssemblyContribution`] through
/// [`ApiModuleRegistry::add_module`] / [`ApiModuleRegistry::add_modules`].
/// Registering the same module owner more than once is tolerated: the first
/// registration wins, later duplicates are ignored with a warning and recorded
/// in [`ApiModuleRegistry::ignored_duplicates`] so free composition of route
/// modules never fails on repeated integration of the same module.
pub struct ApiModuleRegistry {
    modules: Vec<WebModule>,
    registered_owners: BTreeSet<&'static str>,
    ignored_duplicates: Vec<&'static str>,
}

impl Default for ApiModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            registered_owners: BTreeSet::new(),
            ignored_duplicates: Vec::new(),
        }
    }

    /// Registers one module (or a bare contribution). Duplicate module owners
    /// are ignored (first registration wins) instead of failing composition.
    pub fn add_module<M>(&mut self, module: M) -> &mut Self
    where
        M: Into<WebModule>,
    {
        let module = module.into();
        if !self.registered_owners.insert(module.owner) {
            tracing::warn!(
                owner = module.owner,
                "duplicate add_module ignored; first registration wins"
            );
            self.ignored_duplicates.push(module.owner);
            return self;
        }
        self.modules.push(module);
        self
    }

    /// Registers every module in order.
    pub fn add_modules<I, M>(&mut self, modules: I) -> &mut Self
    where
        I: IntoIterator<Item = M>,
        M: Into<WebModule>,
    {
        for module in modules {
            self.add_module(module);
        }
        self
    }

    /// Consuming builder form of [`ApiModuleRegistry::add_module`] for inline
    /// host composition:
    /// `ApiModuleRegistry::with_modules(modules).try_compose(title)?`.
    pub fn with_module<M>(mut self, module: M) -> Self
    where
        M: Into<WebModule>,
    {
        self.add_module(module);
        self
    }

    /// Consuming builder form of [`ApiModuleRegistry::add_modules`].
    pub fn with_modules<I, M>(mut self, modules: I) -> Self
    where
        I: IntoIterator<Item = M>,
        M: Into<WebModule>,
    {
        self.add_modules(modules);
        self
    }

    /// True when `owner` already has a registration in this registry.
    pub fn is_registered(&self, owner: &str) -> bool {
        self.registered_owners.contains(owner)
    }

    pub fn modules(&self) -> &[WebModule] {
        &self.modules
    }

    pub fn owners(&self) -> Vec<&'static str> {
        self.modules.iter().map(|module| module.owner).collect()
    }

    pub fn ignored_duplicates(&self) -> &[&'static str] {
        &self.ignored_duplicates
    }

    /// Validates and merges every registered module's surfaces into one
    /// composed profile.
    ///
    /// Contributions are flattened in registration order. A contribution the
    /// same module intentionally exposes twice (for example an app surface and
    /// a backend surface owned by one application) is always mounted; the same
    /// contribution owner installed by a *later* module is ignored with a
    /// warning, matching the duplicate-`add_module` guarantee. Cross-owner
    /// route collisions still fail closed inside
    /// [`ComposedApiAssembly::try_compose`].
    pub fn try_compose(self, title: &str) -> Result<ComposedApiAssembly, String> {
        let mut contributions = Vec::new();
        let mut mounted_owners = BTreeSet::new();
        for module in self.modules {
            let mut module_owners = BTreeSet::new();
            for contribution in module.contributions {
                let duplicate_from_earlier_module = mounted_owners.contains(&contribution.owner)
                    && !module_owners.contains(&contribution.owner);
                if duplicate_from_earlier_module {
                    tracing::warn!(
                        owner = contribution.owner,
                        module = module.owner,
                        "duplicate surface contribution ignored; first contribution wins"
                    );
                    continue;
                }
                module_owners.insert(contribution.owner);
                mounted_owners.insert(contribution.owner);
                contributions.push(contribution);
            }
        }
        ComposedApiAssembly::try_compose(title, contributions)
    }

    /// Composes the registry and binds the profile to one HTTP host in one step.
    pub fn into_hosted<R>(
        self,
        title: &str,
        framework: WebFrameworkBuilder<R>,
    ) -> Result<HostedApiAssembly, String>
    where
        R: WebRequestContextResolver + Clone + Any,
    {
        Ok(self.try_compose(title)?.into_hosted(framework))
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
        route
            .validate_log_retention()
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
    use sdkwork_web_core::{DefaultWebRequestContextResolver, WebRequestContextProfile};
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
    fn add_module_ignores_duplicate_owners_with_first_registration_winning() {
        let first = ApiAssemblyContribution::from_manifest(
            "sdkwork-widgets",
            "SDKWork Widgets API",
            Router::new(),
            HttpRouteManifest::new(ROUTES),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid contribution");
        let duplicate = ApiAssemblyContribution::from_manifest(
            "sdkwork-widgets",
            "SDKWork Widgets API (duplicate)",
            Router::new(),
            HttpRouteManifest::new(ROUTES),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid contribution");
        const GADGET_ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/app/v3/api/gadgets",
            "Gadgets",
            "gadgets.list",
            RouteAuth::DualToken,
        )];
        let other = ApiAssemblyContribution::from_manifest(
            "sdkwork-gadgets",
            "SDKWork Gadgets API",
            Router::new(),
            HttpRouteManifest::new(GADGET_ROUTES),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid contribution");

        let mut registry = ApiModuleRegistry::new();
        registry
            .add_module(first)
            .add_module(duplicate)
            .add_modules([other]);

        assert_eq!(registry.owners(), vec!["sdkwork-widgets", "sdkwork-gadgets"]);
        assert_eq!(registry.ignored_duplicates(), &["sdkwork-widgets"]);
        assert!(registry.is_registered("sdkwork-widgets"));
        assert!(!registry.is_registered("sdkwork-other"));

        let composed = registry
            .try_compose("Combined")
            .expect("duplicate registration must not fail composition");
        assert_eq!(composed.owners, vec!["sdkwork-widgets", "sdkwork-gadgets"]);
    }

    #[test]
    fn web_module_bundles_every_surface_of_one_owner() {
        const BUSINESS_ROUTES: &[HttpRoute] = &[
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
        const OPEN_ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/open/v3/api/widgets",
            "Widgets",
            "widgets.public.list",
            RouteAuth::Public,
        )];
        let contribution = |owner, title, routes| {
            ApiAssemblyContribution::from_manifest(
                owner,
                title,
                Router::new(),
                HttpRouteManifest::new(routes),
                Vec::new(),
                Arc::new(crate::AlwaysReady),
            )
            .expect("valid contribution")
        };

        // One contribution per served owner: the app and backend surfaces
        // belong to the same owner contribution, the open surface is served
        // under its own owner (`sdkwork-*-open`).
        let module = WebModule::new("sdkwork-widgets", "SDKWork Widgets")
            .with_surface(contribution(
                "sdkwork-widgets",
                "SDKWork Widgets API",
                BUSINESS_ROUTES,
            ))
            .with_surfaces([contribution(
                "sdkwork-widgets-open",
                "SDKWork Widgets Open API",
                OPEN_ROUTES,
            )]);

        assert_eq!(module.owner(), "sdkwork-widgets");
        assert_eq!(module.title(), "SDKWork Widgets");
        assert_eq!(module.contributions().len(), 2);

        let mut registry = ApiModuleRegistry::new();
        registry.add_module(module);
        let composed = registry
            .try_compose("Combined")
            .expect("valid module composition");
        assert_eq!(composed.route_manifest.routes().len(), 3);
        assert_eq!(
            composed.owners,
            vec!["sdkwork-widgets", "sdkwork-widgets-open"]
        );
    }

    #[test]
    fn registry_ignores_duplicate_surface_contributions_across_modules() {
        const ROUTES: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/open/v3/api/widgets",
            "Widgets",
            "widgets.public.list",
            RouteAuth::Public,
        )];
        let shared = |owner| {
            ApiAssemblyContribution::from_manifest(
                owner,
                "Widgets Open API",
                Router::new(),
                HttpRouteManifest::new(ROUTES),
                Vec::new(),
                Arc::new(crate::AlwaysReady),
            )
            .expect("valid contribution")
        };

        // Two modules both install the same open-surface owner: the shared
        // contribution must be mounted once, not rejected.
        let mut registry = ApiModuleRegistry::new();
        registry
            .add_module(WebModule::from_contribution(shared("sdkwork-widgets-open")))
            .add_module(
                WebModule::new("sdkwork-aggregate", "Aggregate")
                    .with_surface(shared("sdkwork-widgets-open")),
            );
        let composed = registry
            .try_compose("Combined")
            .expect("duplicate surface contribution must be ignored");
        assert_eq!(composed.route_manifest.routes().len(), 1);
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

    #[test]
    fn finalize_host_route_manifest_accepts_already_composed_manifest() {
        const HOST_ROUTES: &[HttpRoute] = &[HttpRoute::dual_token(
            HttpMethod::Get,
            "/app/v3/api/dashboard/overview",
            "dashboard",
            "dashboard.overview.retrieve",
        )];
        const DEP_ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_bootstrap(
            HttpMethod::Get,
            "/app/v3/api/system/iam/runtime",
            "system",
            "iam.runtime.retrieve",
        )];
        let mounts = [RouteManifestMount {
            owner: "sdkwork-iam",
            manifest: HttpRouteManifest::new(DEP_ROUTES),
        }];
        let composed = HttpRouteManifest::try_merge_mounts(
            "sdkwork-cloudrouter",
            HttpRouteManifest::new(HOST_ROUTES),
            &mounts,
        )
        .expect("compose once");
        let prepared = finalize_host_route_manifest(
            "sdkwork-cloudrouter",
            composed,
            &mounts,
            &WebRequestContextProfile::default(),
            &["/healthz".to_owned()],
        )
        .expect("finalize composed manifest");
        let route = prepared
            .match_route("GET", "/app/v3/api/system/iam/runtime")
            .expect("dependency route must stay registered");
        assert_eq!(RouteAuth::CredentialEntryBootstrap, route.auth);
    }

    #[test]
    fn merge_dependency_contribution_pairs_router_and_public_auth_manifest() {
        const HOST_ROUTES: &[HttpRoute] = &[HttpRoute::dual_token(
            HttpMethod::Get,
            "/app/v3/api/dashboard/overview",
            "dashboard",
            "dashboard.overview.retrieve",
        )];
        const DEP_ROUTES: &[HttpRoute] = &[HttpRoute::credential_entry_bootstrap(
            HttpMethod::Get,
            "/app/v3/api/system/iam/runtime",
            "system",
            "iam.runtime.retrieve",
        )];

        let host = ApiAssemblyContribution::from_manifest(
            "sdkwork-cloudrouter",
            "Cloud Router App API",
            Router::new(),
            HttpRouteManifest::new(HOST_ROUTES),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid host contribution");
        let dependency = ApiAssemblyContribution::from_manifest(
            "sdkwork-iam",
            "IAM App API",
            Router::new(),
            HttpRouteManifest::new(DEP_ROUTES),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid dependency contribution");

        let merged = host
            .merge_dependency_contribution(&dependency)
            .expect("dependency merge");
        let route = merged
            .route_manifest
            .match_route("GET", "/app/v3/api/system/iam/runtime")
            .expect("dependency route must stay registered");
        assert_eq!(RouteAuth::CredentialEntryBootstrap, route.auth);
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

    fn widgets_contribution() -> ApiAssemblyContribution {
        ApiAssemblyContribution::from_manifest(
            "sdkwork-widgets",
            "SDKWork Widgets API",
            Router::new(),
            HttpRouteManifest::new(ROUTES),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid contribution")
    }

    fn gadgets_contribution() -> ApiAssemblyContribution {
        const GADGETS: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/app/v3/api/gadgets/{gadgetId}",
            "Gadgets",
            "gadgets.retrieve",
            RouteAuth::DualToken,
        )];
        ApiAssemblyContribution::from_manifest(
            "sdkwork-gadgets",
            "SDKWork Gadgets API",
            Router::new(),
            HttpRouteManifest::new(GADGETS),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid contribution")
    }

    /// Building-block composition: registering the same module twice must be
    /// ignored (first registration wins) instead of failing the composition.

    #[test]
    fn add_modules_ignores_duplicates_inside_a_single_batch() {
        let mut registry = ApiModuleRegistry::new();
        registry.add_modules(vec![
            WebModule::from_contribution(widgets_contribution()),
            WebModule::from_contribution(gadgets_contribution()),
            WebModule::from_contribution(widgets_contribution()),
        ]);

        assert_eq!(registry.owners(), vec!["sdkwork-widgets", "sdkwork-gadgets"]);
        assert_eq!(registry.ignored_duplicates(), &["sdkwork-widgets"]);
    }

    #[test]
    fn consuming_builder_form_ignores_duplicates() {
        let registry = ApiModuleRegistry::new()
            .with_module(WebModule::from_contribution(widgets_contribution()))
            .with_modules(vec![WebModule::from_contribution(widgets_contribution())]);

        assert_eq!(registry.owners(), vec!["sdkwork-widgets"]);
        assert_eq!(registry.ignored_duplicates(), &["sdkwork-widgets"]);
    }

    /// One module may intentionally own several surfaces (app-api + backend-api
    /// + open-api). Those are the module's own definition and must all mount.

    /// A later module must not re-mount a surface an earlier module already
    /// mounted, even when the duplicate is contributed by a different module.
    #[test]
    fn later_module_cannot_re_mount_an_earlier_surface() {
        let widgets = WebModule::from_contribution(widgets_contribution());
        let bundle = WebModule::new("sdkwork-bundle", "SDKWork Bundle")
            .with_surface(widgets_contribution())
            .with_surface(gadgets_contribution());

        let mut registry = ApiModuleRegistry::new();
        registry.add_modules(vec![widgets, bundle]);

        let composed = registry
            .try_compose("SDKWork Platform API")
            .expect("duplicate surface across modules must not fail");
        assert_eq!(composed.owners, vec!["sdkwork-widgets", "sdkwork-gadgets"]);
    }

    #[test]
    fn cross_owner_route_collisions_still_fail_closed() {
        const CONFLICT: &[HttpRoute] = &[HttpRoute::new(
            HttpMethod::Get,
            "/app/v3/api/widgets/{widgetId}",
            "Gadgets",
            "gadgets.retrieve",
            RouteAuth::DualToken,
        )];
        let conflicting = ApiAssemblyContribution::from_manifest(
            "sdkwork-gadgets",
            "SDKWork Gadgets API",
            Router::new(),
            HttpRouteManifest::new(CONFLICT),
            Vec::new(),
            Arc::new(crate::AlwaysReady),
        )
        .expect("valid contribution");

        let mut registry = ApiModuleRegistry::new();
        registry.add_modules(vec![
            WebModule::from_contribution(widgets_contribution()),
            WebModule::from_contribution(conflicting),
        ]);

        let error = registry
            .try_compose("SDKWork Platform API")
            .err()
            .expect("route collisions must still fail");
        assert!(error.contains("route collision"), "{error}");
        assert!(error.contains("GET /app/v3/api/widgets/{param}"), "{error}");
    }
}
