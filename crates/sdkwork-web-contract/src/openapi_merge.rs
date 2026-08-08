use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

const HTTP_METHODS: &[&str] = &[
    "delete", "get", "head", "options", "patch", "post", "put", "trace",
];
const PATH_ITEM_FIELDS: &[&str] = &["$ref", "description", "parameters", "servers", "summary"];
const COMPONENT_NAMESPACES: &[&str] = &[
    "callbacks",
    "examples",
    "headers",
    "links",
    "parameters",
    "pathItems",
    "requestBodies",
    "responses",
    "schemas",
    "securitySchemes",
];

/// One owner-provided OpenAPI document participating in a combined authority document.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenApiDocumentContribution {
    pub owner: String,
    pub document: Value,
}

impl OpenApiDocumentContribution {
    pub fn new(owner: impl Into<String>, document: Value) -> Self {
        Self {
            owner: owner.into(),
            document,
        }
    }
}

impl<O> From<(O, Value)> for OpenApiDocumentContribution
where
    O: Into<String>,
{
    fn from((owner, document): (O, Value)) -> Self {
        Self::new(owner, document)
    }
}

impl<O> From<(O, &Value)> for OpenApiDocumentContribution
where
    O: Into<String>,
{
    fn from((owner, document): (O, &Value)) -> Self {
        Self::new(owner, document.clone())
    }
}

/// A structural or ownership conflict encountered while combining OpenAPI authorities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpenApiMergeError {
    EmptyCombinedTitle,
    NoDocuments,
    EmptyOwner {
        index: usize,
    },
    InvalidShape {
        owner: String,
        location: String,
        expected: &'static str,
    },
    OpenApiVersionConflict {
        expected_owner: String,
        expected: String,
        conflicting_owner: String,
        actual: String,
    },
    PathOperationConflict {
        path: String,
        method: String,
        first_owner: String,
        second_owner: String,
    },
    PathItemConflict {
        path: String,
        field: String,
        first_owner: String,
        second_owner: String,
    },
    ComponentConflict {
        namespace: String,
        name: String,
        first_owner: String,
        second_owner: String,
    },
    TagConflict {
        name: String,
        first_owner: String,
        second_owner: String,
    },
    ServerConflict {
        url: String,
        first_owner: String,
        second_owner: String,
    },
    TopLevelConflict {
        field: String,
        first_owner: String,
        second_owner: String,
    },
}

impl Display for OpenApiMergeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCombinedTitle => {
                write!(formatter, "combined OpenAPI title must not be empty")
            }
            Self::NoDocuments => write!(formatter, "at least one OpenAPI document is required"),
            Self::EmptyOwner { index } => {
                write!(
                    formatter,
                    "OpenAPI contribution at index {index} has an empty owner"
                )
            }
            Self::InvalidShape {
                owner,
                location,
                expected,
            } => write!(
                formatter,
                "OpenAPI contribution `{owner}` has invalid `{location}`; expected {expected}"
            ),
            Self::OpenApiVersionConflict {
                expected_owner,
                expected,
                conflicting_owner,
                actual,
            } => write!(
                formatter,
                "OpenAPI version conflict: `{expected_owner}` declares `{expected}` but \
                 `{conflicting_owner}` declares `{actual}`"
            ),
            Self::PathOperationConflict {
                path,
                method,
                first_owner,
                second_owner,
            } => write!(
                formatter,
                "OpenAPI operation conflict at `{method} {path}` between `{first_owner}` and \
                 `{second_owner}`"
            ),
            Self::PathItemConflict {
                path,
                field,
                first_owner,
                second_owner,
            } => write!(
                formatter,
                "OpenAPI path item conflict at `{path}.{field}` between `{first_owner}` and \
                 `{second_owner}`"
            ),
            Self::ComponentConflict {
                namespace,
                name,
                first_owner,
                second_owner,
            } => write!(
                formatter,
                "OpenAPI component conflict at `components.{namespace}.{name}` between \
                 `{first_owner}` and `{second_owner}`"
            ),
            Self::TagConflict {
                name,
                first_owner,
                second_owner,
            } => write!(
                formatter,
                "OpenAPI tag `{name}` conflicts between `{first_owner}` and `{second_owner}`"
            ),
            Self::ServerConflict {
                url,
                first_owner,
                second_owner,
            } => write!(
                formatter,
                "OpenAPI server `{url}` conflicts between `{first_owner}` and `{second_owner}`"
            ),
            Self::TopLevelConflict {
                field,
                first_owner,
                second_owner,
            } => write!(
                formatter,
                "OpenAPI field `{field}` conflicts between `{first_owner}` and `{second_owner}`"
            ),
        }
    }
}

impl Error for OpenApiMergeError {}

#[derive(Clone)]
struct OwnedValue {
    owner: String,
    value: Value,
}

#[derive(Default)]
struct PathState {
    fields: BTreeMap<String, OwnedValue>,
    operations: BTreeMap<String, OwnedValue>,
}

#[derive(Default)]
struct MergeState {
    openapi: Option<OwnedValue>,
    info: BTreeMap<String, OwnedValue>,
    paths: BTreeMap<String, PathState>,
    components: BTreeMap<String, BTreeMap<String, OwnedValue>>,
    component_extensions: BTreeMap<String, OwnedValue>,
    tags: BTreeMap<String, OwnedValue>,
    servers: BTreeMap<String, OwnedValue>,
    security: BTreeMap<String, Value>,
    webhooks: BTreeMap<String, OwnedValue>,
    top_level: BTreeMap<String, OwnedValue>,
    saw_components: bool,
    saw_tags: bool,
    saw_servers: bool,
    saw_security: bool,
    saw_webhooks: bool,
}

/// Combines complete owner OpenAPI documents without rebuilding operations from route metadata.
///
/// Duplicate component definitions are accepted only when structurally identical. A duplicate
/// `(path, method)` always fails because an executable operation must have exactly one owner.
pub fn merge_openapi_documents<I, C>(
    combined_title: &str,
    contributions: I,
) -> Result<Value, OpenApiMergeError>
where
    I: IntoIterator<Item = C>,
    C: Into<OpenApiDocumentContribution>,
{
    if combined_title.trim().is_empty() {
        return Err(OpenApiMergeError::EmptyCombinedTitle);
    }

    let contributions = contributions
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
    if contributions.is_empty() {
        return Err(OpenApiMergeError::NoDocuments);
    }

    let mut state = MergeState::default();
    for (index, contribution) in contributions.into_iter().enumerate() {
        let owner = contribution.owner.trim();
        if owner.is_empty() {
            return Err(OpenApiMergeError::EmptyOwner { index });
        }
        merge_document(&mut state, owner, canonicalize(contribution.document))?;
    }

    materialize(state, combined_title)
}

fn merge_document(
    state: &mut MergeState,
    owner: &str,
    document: Value,
) -> Result<(), OpenApiMergeError> {
    let document = expect_object(owner, "$", &document)?;
    let openapi = expect_string_field(owner, document, "openapi", "OpenAPI version string")?;
    merge_openapi_version(state, owner, openapi)?;

    let info = document
        .get("info")
        .ok_or_else(|| invalid_shape(owner, "info", "object with title and version"))?;
    merge_info(state, owner, expect_object(owner, "info", info)?)?;

    let paths = document
        .get("paths")
        .ok_or_else(|| invalid_shape(owner, "paths", "object"))?;
    merge_paths(state, owner, expect_object(owner, "paths", paths)?)?;

    for (field, value) in document {
        match field.as_str() {
            "openapi" | "info" | "paths" => {}
            "components" => merge_components(state, owner, value)?,
            "tags" => merge_tags(state, owner, value)?,
            "servers" => merge_servers(state, owner, value)?,
            "security" => merge_security(state, owner, value)?,
            "webhooks" => merge_webhooks(state, owner, value)?,
            "jsonSchemaDialect" | "externalDocs" => {
                merge_top_level_value(state, owner, field, value)?;
            }
            extension if extension.starts_with("x-") => {
                // Document-scoped x-* metadata (owner, domain, ...) is
                // first-wins: each owner documents its own identity and the
                // combined document keeps the first contributor's value.
                if !state.top_level.contains_key(extension) {
                    state.top_level.insert(
                        extension.to_owned(),
                        OwnedValue {
                            owner: owner.to_owned(),
                            value: value.clone(),
                        },
                    );
                }
            }
            _ => {
                return Err(invalid_shape(
                    owner,
                    field,
                    "an OpenAPI 3.1 top-level field or x-* extension",
                ));
            }
        }
    }
    Ok(())
}

fn merge_openapi_version(
    state: &mut MergeState,
    owner: &str,
    version: &str,
) -> Result<(), OpenApiMergeError> {
    match &state.openapi {
        Some(existing) if existing.value.as_str() != Some(version) => {
            Err(OpenApiMergeError::OpenApiVersionConflict {
                expected_owner: existing.owner.clone(),
                expected: existing.value.as_str().unwrap_or_default().to_owned(),
                conflicting_owner: owner.to_owned(),
                actual: version.to_owned(),
            })
        }
        Some(_) => Ok(()),
        None => {
            state.openapi = Some(OwnedValue {
                owner: owner.to_owned(),
                value: Value::String(version.to_owned()),
            });
            Ok(())
        }
    }
}

fn merge_info(
    state: &mut MergeState,
    owner: &str,
    info: &Map<String, Value>,
) -> Result<(), OpenApiMergeError> {
    expect_string_field(owner, info, "title", "non-empty string")?;
    expect_string_field(owner, info, "version", "non-empty string")?;
    for (field, value) in info {
        // The combined title is supplied by the composer; the combined
        // version and description follow the first contributing document
        // (each owner documents its own package version and description, so
        // first-wins is the only meaningful combination).
        if field == "title" {
            continue;
        }
        if field == "version"
            || field == "description"
            || (field.starts_with("x-") && state.info.contains_key(field))
        {
            if !state.info.contains_key(field) {
                state.info.insert(
                    field.to_owned(),
                    OwnedValue {
                        owner: owner.to_owned(),
                        value: value.clone(),
                    },
                );
            }
            continue;
        }
        merge_compatible_value(
            &mut state.info,
            owner,
            field,
            value,
            format!("info.{field}"),
        )?;
    }
    Ok(())
}

fn merge_paths(
    state: &mut MergeState,
    owner: &str,
    paths: &Map<String, Value>,
) -> Result<(), OpenApiMergeError> {
    for (path, path_item) in paths {
        if !path.starts_with('/') {
            return Err(invalid_shape(
                owner,
                format!("paths.{path}"),
                "path beginning with /",
            ));
        }
        let path_item = expect_object(owner, format!("paths.{path}"), path_item)?;
        let target = state.paths.entry(path.clone()).or_default();
        for (field, value) in path_item {
            if HTTP_METHODS.contains(&field.as_str()) {
                if !value.is_object() {
                    return Err(invalid_shape(
                        owner,
                        format!("paths.{path}.{field}"),
                        "operation object",
                    ));
                }
                if let Some(existing) = target.operations.get(field) {
                    return Err(OpenApiMergeError::PathOperationConflict {
                        path: path.clone(),
                        method: field.clone(),
                        first_owner: existing.owner.clone(),
                        second_owner: owner.to_owned(),
                    });
                }
                target.operations.insert(
                    field.clone(),
                    OwnedValue {
                        owner: owner.to_owned(),
                        value: value.clone(),
                    },
                );
            } else if PATH_ITEM_FIELDS.contains(&field.as_str()) || field.starts_with("x-") {
                match target.fields.get(field) {
                    Some(existing) if existing.value != *value => {
                        return Err(OpenApiMergeError::PathItemConflict {
                            path: path.clone(),
                            field: field.clone(),
                            first_owner: existing.owner.clone(),
                            second_owner: owner.to_owned(),
                        });
                    }
                    Some(_) => {}
                    None => {
                        target.fields.insert(
                            field.clone(),
                            OwnedValue {
                                owner: owner.to_owned(),
                                value: value.clone(),
                            },
                        );
                    }
                }
            } else {
                return Err(invalid_shape(
                    owner,
                    format!("paths.{path}.{field}"),
                    "HTTP method, path-item field, or x-* extension",
                ));
            }
        }
    }
    Ok(())
}

fn merge_components(
    state: &mut MergeState,
    owner: &str,
    components: &Value,
) -> Result<(), OpenApiMergeError> {
    state.saw_components = true;
    let components = expect_object(owner, "components", components)?;
    for (namespace, entries) in components {
        if namespace.starts_with("x-") {
            merge_compatible_value(
                &mut state.component_extensions,
                owner,
                namespace,
                entries,
                format!("components.{namespace}"),
            )?;
            continue;
        }
        if !COMPONENT_NAMESPACES.contains(&namespace.as_str()) {
            return Err(invalid_shape(
                owner,
                format!("components.{namespace}"),
                "OpenAPI component namespace or x-* extension",
            ));
        }
        let entries = expect_object(owner, format!("components.{namespace}"), entries)?;
        let target = state.components.entry(namespace.clone()).or_default();
        for (name, definition) in entries {
            match target.get(name) {
                // A component with the same name across owners keeps the
                // first definition (first-wins): the combined document cannot
                // represent two different shapes under one name, and the
                // per-owner authored documents remain authoritative for SDK
                // generation. Same-shaped duplicates pass through unchanged.
                // Runtime authentication is driven by route auth metadata,
                // not by the combined securitySchemes document.
                Some(_) => {}
                None => {
                    target.insert(
                        name.clone(),
                        OwnedValue {
                            owner: owner.to_owned(),
                            value: definition.clone(),
                        },
                    );
                }
            }
        }
    }
    Ok(())
}

fn merge_tags(state: &mut MergeState, owner: &str, tags: &Value) -> Result<(), OpenApiMergeError> {
    state.saw_tags = true;
    let tags = expect_array(owner, "tags", tags)?;
    for (index, tag) in tags.iter().enumerate() {
        let location = format!("tags[{index}]");
        let tag_object = expect_object(owner, &location, tag)?;
        let name = expect_string_field(owner, tag_object, "name", "non-empty string")?;
        match state.tags.get(name) {
            Some(existing) if existing.value != *tag => {
                return Err(OpenApiMergeError::TagConflict {
                    name: name.to_owned(),
                    first_owner: existing.owner.clone(),
                    second_owner: owner.to_owned(),
                });
            }
            Some(_) => {}
            None => {
                state.tags.insert(
                    name.to_owned(),
                    OwnedValue {
                        owner: owner.to_owned(),
                        value: tag.clone(),
                    },
                );
            }
        }
    }
    Ok(())
}

fn merge_servers(
    state: &mut MergeState,
    owner: &str,
    servers: &Value,
) -> Result<(), OpenApiMergeError> {
    state.saw_servers = true;
    let servers = expect_array(owner, "servers", servers)?;
    for (index, server) in servers.iter().enumerate() {
        let location = format!("servers[{index}]");
        let server_object = expect_object(owner, &location, server)?;
        let url = expect_string_field(owner, server_object, "url", "non-empty string")?;
        match state.servers.get(url) {
            Some(existing) if existing.value != *server => {
                return Err(OpenApiMergeError::ServerConflict {
                    url: url.to_owned(),
                    first_owner: existing.owner.clone(),
                    second_owner: owner.to_owned(),
                });
            }
            Some(_) => {}
            None => {
                state.servers.insert(
                    url.to_owned(),
                    OwnedValue {
                        owner: owner.to_owned(),
                        value: server.clone(),
                    },
                );
            }
        }
    }
    Ok(())
}

fn merge_security(
    state: &mut MergeState,
    owner: &str,
    security: &Value,
) -> Result<(), OpenApiMergeError> {
    state.saw_security = true;
    let requirements = expect_array(owner, "security", security)?;
    for (index, requirement) in requirements.iter().enumerate() {
        let object = expect_object(owner, format!("security[{index}]"), requirement)?;
        for (scheme, scopes) in object {
            let scopes = expect_array(owner, format!("security[{index}].{scheme}"), scopes)?;
            if scopes.iter().any(|scope| !scope.is_string()) {
                return Err(invalid_shape(
                    owner,
                    format!("security[{index}].{scheme}"),
                    "array of scope strings",
                ));
            }
        }
        let key = serde_json::to_string(requirement).expect("JSON values always serialize");
        state
            .security
            .entry(key)
            .or_insert_with(|| requirement.clone());
    }
    Ok(())
}

fn merge_webhooks(
    state: &mut MergeState,
    owner: &str,
    webhooks: &Value,
) -> Result<(), OpenApiMergeError> {
    state.saw_webhooks = true;
    let webhooks = expect_object(owner, "webhooks", webhooks)?;
    for (name, webhook) in webhooks {
        if !webhook.is_object() {
            return Err(invalid_shape(
                owner,
                format!("webhooks.{name}"),
                "path-item object or reference",
            ));
        }
        merge_compatible_value(
            &mut state.webhooks,
            owner,
            name,
            webhook,
            format!("webhooks.{name}"),
        )?;
    }
    Ok(())
}

fn merge_top_level_value(
    state: &mut MergeState,
    owner: &str,
    field: &str,
    value: &Value,
) -> Result<(), OpenApiMergeError> {
    merge_compatible_value(&mut state.top_level, owner, field, value, field.to_owned())
}

fn merge_compatible_value(
    target: &mut BTreeMap<String, OwnedValue>,
    owner: &str,
    key: &str,
    value: &Value,
    location: String,
) -> Result<(), OpenApiMergeError> {
    match target.get(key) {
        Some(existing) if existing.value != *value => Err(OpenApiMergeError::TopLevelConflict {
            field: location,
            first_owner: existing.owner.clone(),
            second_owner: owner.to_owned(),
        }),
        Some(_) => Ok(()),
        None => {
            target.insert(
                key.to_owned(),
                OwnedValue {
                    owner: owner.to_owned(),
                    value: value.clone(),
                },
            );
            Ok(())
        }
    }
}

fn materialize(state: MergeState, combined_title: &str) -> Result<Value, OpenApiMergeError> {
    let openapi = state.openapi.ok_or(OpenApiMergeError::NoDocuments)?.value;
    let mut document = Map::new();
    document.insert("openapi".to_owned(), openapi);

    let mut info = Map::new();
    info.insert(
        "title".to_owned(),
        Value::String(combined_title.trim().to_owned()),
    );
    for (field, value) in state.info {
        info.insert(field, value.value);
    }
    document.insert("info".to_owned(), Value::Object(info));
    for (field, value) in state.top_level {
        document.insert(field, value.value);
    }

    if state.saw_servers {
        document.insert(
            "servers".to_owned(),
            Value::Array(
                state
                    .servers
                    .into_values()
                    .map(|entry| entry.value)
                    .collect(),
            ),
        );
    }

    let mut paths = Map::new();
    for (path, path_state) in state.paths {
        let mut path_item = Map::new();
        for (field, value) in path_state.fields {
            path_item.insert(field, value.value);
        }
        for (method, value) in path_state.operations {
            path_item.insert(method, value.value);
        }
        paths.insert(path, Value::Object(path_item));
    }
    document.insert("paths".to_owned(), Value::Object(paths));

    if state.saw_webhooks {
        document.insert(
            "webhooks".to_owned(),
            Value::Object(map_owned_values(state.webhooks)),
        );
    }

    if state.saw_components {
        let mut components = Map::new();
        for (namespace, entries) in state.components {
            components.insert(namespace, Value::Object(map_owned_values(entries)));
        }
        for (field, value) in state.component_extensions {
            components.insert(field, value.value);
        }
        document.insert("components".to_owned(), Value::Object(components));
    }

    if state.saw_security {
        document.insert(
            "security".to_owned(),
            Value::Array(state.security.into_values().collect()),
        );
    }
    if state.saw_tags {
        document.insert(
            "tags".to_owned(),
            Value::Array(state.tags.into_values().map(|entry| entry.value).collect()),
        );
    }

    Ok(Value::Object(document))
}

fn map_owned_values(entries: BTreeMap<String, OwnedValue>) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(key, entry)| (key, entry.value))
        .collect()
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (key, canonicalize(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        scalar => scalar,
    }
}

fn expect_object<'a>(
    owner: &str,
    location: impl Into<String>,
    value: &'a Value,
) -> Result<&'a Map<String, Value>, OpenApiMergeError> {
    value
        .as_object()
        .ok_or_else(|| invalid_shape(owner, location, "object"))
}

fn expect_array<'a>(
    owner: &str,
    location: impl Into<String>,
    value: &'a Value,
) -> Result<&'a [Value], OpenApiMergeError> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| invalid_shape(owner, location, "array"))
}

fn expect_string_field<'a>(
    owner: &str,
    object: &'a Map<String, Value>,
    field: &str,
    expected: &'static str,
) -> Result<&'a str, OpenApiMergeError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid_shape(owner, field, expected))
}

fn invalid_shape(
    owner: &str,
    location: impl Into<String>,
    expected: &'static str,
) -> OpenApiMergeError {
    OpenApiMergeError::InvalidShape {
        owner: owner.to_owned(),
        location: location.into(),
        expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document(title: &str, path: &str, method: &str, operation: Value) -> Value {
        json!({
            "openapi": "3.1.2",
            "info": { "title": title, "version": "1.0.0" },
            "paths": { path: { method: operation } }
        })
    }

    #[test]
    fn preserves_complete_owner_documents_while_merging_collection_fields() {
        let web = json!({
            "openapi": "3.1.2",
            "info": {
                "title": "Web API",
                "version": "1.0.0",
                "description": "Combined application surface",
                "x-info-owner": "web"
            },
            "jsonSchemaDialect": "https://json-schema.org/draft/2020-12/schema",
            "servers": [{ "url": "/", "description": "Same-origin ingress" }],
            "paths": {
                "/app/v3/api/sites": {
                    "description": "Sites resource",
                    "get": {
                        "operationId": "listSites",
                        "tags": ["sites"],
                        "parameters": [{ "name": "cursor", "in": "query" }],
                        "responses": { "200": { "description": "ok" } },
                        "security": [{ "AuthToken": [], "AccessToken": [] }],
                        "x-sdkwork-request-context": "WebRequestContext"
                    }
                }
            },
            "components": {
                "schemas": { "Site": { "type": "object" } },
                "parameters": { "Cursor": { "name": "cursor", "in": "query" } },
                "securitySchemes": {
                    "AuthToken": { "type": "http", "scheme": "bearer" }
                },
                "x-component-owner": "framework"
            },
            "security": [{ "AuthToken": [], "AccessToken": [] }],
            "tags": [{ "name": "sites", "description": "Site operations" }],
            "x-sdkwork-profile": "standalone"
        });
        let iam_operation = json!({
            "operationId": "currentSession",
            "summary": "Current session",
            "tags": ["iam"],
            "responses": {
                "200": {
                    "description": "ok",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/Session" }
                        }
                    }
                }
            },
            "security": [{ "AuthToken": [], "AccessToken": [] }],
            "x-sdkwork-api-surface": "app-api"
        });
        let iam = json!({
            "openapi": "3.1.2",
            "info": { "title": "IAM API", "version": "1.0.0" },
            "servers": [{ "url": "/iam-preview" }],
            "paths": {
                "/app/v3/api/auth/sessions/current": { "get": iam_operation.clone() }
            },
            "components": {
                "schemas": { "Session": { "type": "object", "required": ["id"] } },
                "responses": { "Unauthorized": { "description": "unauthorized" } }
            },
            "security": [{ "ApiKey": [] }],
            "tags": [{ "name": "iam" }],
            "x-sdkwork-profile": "standalone"
        });

        let merged = merge_openapi_documents("SDKWork Web Server", [("web", web), ("iam", iam)])
            .expect("documents should merge");

        assert_eq!(merged["info"]["title"], "SDKWork Web Server");
        assert_eq!(
            merged["info"]["description"],
            "Combined application surface"
        );
        assert_eq!(merged["x-sdkwork-profile"], "standalone");
        assert_eq!(
            merged["paths"]["/app/v3/api/auth/sessions/current"]["get"],
            iam_operation
        );
        assert_eq!(merged["components"]["schemas"]["Site"]["type"], "object");
        assert_eq!(merged["components"]["schemas"]["Session"]["type"], "object");
        assert!(merged["components"]["parameters"]["Cursor"].is_object());
        assert!(merged["components"]["responses"]["Unauthorized"].is_object());
        assert_eq!(merged["servers"].as_array().unwrap().len(), 2);
        assert_eq!(merged["security"].as_array().unwrap().len(), 2);
        assert_eq!(merged["tags"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn accepts_identical_shared_components_in_every_namespace() {
        let shared_components = json!({
            "schemas": { "Problem": { "type": "object" } },
            "responses": { "Problem": { "description": "problem" } },
            "parameters": { "Trace": { "name": "trace", "in": "header" } },
            "examples": { "Problem": { "value": { "code": 40001 } } },
            "requestBodies": { "Command": { "content": {} } },
            "headers": { "Trace": { "schema": { "type": "string" } } },
            "securitySchemes": { "ApiKey": { "type": "apiKey", "in": "header", "name": "X-API-Key" } },
            "links": { "Next": { "operationId": "next" } },
            "callbacks": { "Done": {} },
            "pathItems": { "Health": { "get": { "responses": {} } } }
        });
        let mut first = document("First", "/first", "get", json!({}));
        first["components"] = shared_components.clone();
        let mut second = document("Second", "/second", "post", json!({}));
        second["components"] = shared_components.clone();

        let merged = merge_openapi_documents("Combined", [("first", first), ("second", second)])
            .expect("identical components should be shared");

        assert_eq!(merged["components"], shared_components);
    }

    #[test]
    fn rejects_openapi_version_conflicts() {
        let first = document("First", "/first", "get", json!({}));
        let mut second = document("Second", "/second", "get", json!({}));
        second["openapi"] = json!("3.0.3");

        let error = merge_openapi_documents("Combined", [("first", first), ("second", second)])
            .expect_err("versions must agree");
        assert!(matches!(
            error,
            OpenApiMergeError::OpenApiVersionConflict { .. }
        ));
    }

    #[test]
    fn rejects_duplicate_path_method_even_when_operations_are_identical() {
        let first = document(
            "First",
            "/shared",
            "get",
            json!({ "operationId": "shared" }),
        );
        let second = document(
            "Second",
            "/shared",
            "get",
            json!({ "operationId": "shared" }),
        );

        let error = merge_openapi_documents("Combined", [("first", first), ("second", second)])
            .expect_err("an operation must have one owner");
        assert_eq!(
            error,
            OpenApiMergeError::PathOperationConflict {
                path: "/shared".to_owned(),
                method: "get".to_owned(),
                first_owner: "first".to_owned(),
                second_owner: "second".to_owned(),
            }
        );
    }

    #[test]
    fn merges_distinct_operations_owned_under_the_same_path() {
        let mut first = document(
            "First",
            "/shared",
            "get",
            json!({ "operationId": "getShared" }),
        );
        first["paths"]["/shared"]["description"] = json!("Shared resource");
        let second = document(
            "Second",
            "/shared",
            "post",
            json!({ "operationId": "createShared" }),
        );

        let merged = merge_openapi_documents("Combined", [("first", first), ("second", second)])
            .expect("distinct methods have distinct owners");

        let path = &merged["paths"]["/shared"];
        assert_eq!(path["description"], "Shared resource");
        assert_eq!(path["get"]["operationId"], "getShared");
        assert_eq!(path["post"]["operationId"], "createShared");
    }

    #[test]
    fn component_duplicates_are_first_wins() {
        // All component namespaces (schemas, securitySchemes, ...) are
        // first-wins in the combined document: the per-owner authored
        // documents remain authoritative, and runtime authentication is
        // driven by route auth metadata, not the combined securitySchemes.
        for (namespace, name) in [("schemas", "Thing"), ("securitySchemes", "ApiKey")] {
            let mut first = document("First", "/first", "get", json!({}));
            first["components"] = json!({ namespace: { name: { "type": "string" } } });
            let mut second = document("Second", "/second", "get", json!({}));
            second["components"] = json!({ namespace: { name: { "type": "integer" } } });

            let merged = merge_openapi_documents("Combined", [("first", first), ("second", second)])
                .expect("component duplicates are first-wins");
            assert_eq!(
                merged["components"][namespace][name],
                json!({ "type": "string" }),
                "combined document keeps the first definition"
            );
        }
    }

    #[test]
    fn rejects_invalid_document_shapes() {
        let invalid_cases = [
            ("root", json!([])),
            (
                "paths",
                json!({
                    "openapi": "3.1.2",
                    "info": { "title": "Invalid", "version": "1.0.0" },
                    "paths": []
                }),
            ),
            (
                "components",
                json!({
                    "openapi": "3.1.2",
                    "info": { "title": "Invalid", "version": "1.0.0" },
                    "paths": {},
                    "components": { "schemas": [] }
                }),
            ),
            (
                "security",
                json!({
                    "openapi": "3.1.2",
                    "info": { "title": "Invalid", "version": "1.0.0" },
                    "paths": {},
                    "security": [{ "ApiKey": "all" }]
                }),
            ),
        ];

        for (owner, invalid) in invalid_cases {
            let error = merge_openapi_documents("Combined", [(owner, invalid)])
                .expect_err("invalid shape must fail");
            assert!(matches!(error, OpenApiMergeError::InvalidShape { .. }));
        }
    }

    #[test]
    fn incompatible_top_level_extensions_are_first_wins() {
        // Document-scoped x-* metadata is first-wins: each owner documents
        // its own identity and the combined document keeps the first
        // contributor's value.
        let mut first = document("First", "/first", "get", json!({}));
        first["x-sdkwork-profile"] = json!("standalone");
        let mut second = document("Second", "/second", "get", json!({}));
        second["x-sdkwork-profile"] = json!("cloud");

        let merged = merge_openapi_documents("Combined", [("first", first), ("second", second)])
            .expect("x-* extensions are first-wins");
        assert_eq!(merged["x-sdkwork-profile"], json!("standalone"));
    }

    #[test]
    fn output_is_deterministic_across_contribution_and_object_key_order() {
        let first = json!({
            "paths": { "/z": { "post": { "responses": {}, "operationId": "z" } } },
            "info": { "version": "1.0.0", "title": "First" },
            "openapi": "3.1.2",
            "servers": [{ "url": "/z" }],
            "security": [{ "ZuluToken": [] }],
            "tags": [{ "description": "Zulu", "name": "z" }],
            "components": { "schemas": { "Zulu": { "required": ["id"], "type": "object" } } }
        });
        let second = json!({
            "openapi": "3.1.2",
            "info": { "title": "Second", "version": "1.0.0" },
            "components": { "schemas": { "Alpha": { "type": "string" } } },
            "security": [{ "AlphaToken": [] }],
            "servers": [{ "url": "/a" }],
            "tags": [{ "name": "a", "description": "Alpha" }],
            "paths": { "/a": { "get": { "operationId": "a", "responses": {} } } }
        });

        let forward = merge_openapi_documents(
            "Combined",
            [("first", first.clone()), ("second", second.clone())],
        )
        .expect("forward merge");
        let reverse = merge_openapi_documents("Combined", [("second", second), ("first", first)])
            .expect("reverse merge");

        assert_eq!(forward, reverse);
        assert_eq!(
            serde_json::to_string(&forward).unwrap(),
            serde_json::to_string(&reverse).unwrap()
        );
    }
}
