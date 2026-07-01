use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryDependency {
    pub module_id: String,
    pub locator: PathBuf,
    pub registry_path: PathBuf,
    pub order: i32,
    #[serde(default = "default_ownership")]
    pub ownership: String,
}

fn default_ownership() -> String {
    "read_only".to_string()
}

impl RegistryDependency {
    pub fn resolve_registry_file(&self, app_root: &Path) -> PathBuf {
        app_root.join(&self.locator).join(&self.registry_path)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DatabaseManifest {
    modules: Option<Vec<DatabaseManifestModule>>,
}

#[derive(Debug, Clone, Deserialize)]
struct DatabaseManifestModule {
    #[serde(rename = "moduleId")]
    module_id: String,
    locator: PathBuf,
    order: Option<i32>,
}

pub fn resolve_registry_dependencies(
    app_root: &Path,
    explicit: Option<&[RegistryDependency]>,
) -> Result<Vec<RegistryDependency>, crate::SchemaRegistryError> {
    if let Some(deps) = explicit {
        if !deps.is_empty() {
            return Ok(deps.to_vec());
        }
    }

    let manifest_path = app_root.join("database").join("database.manifest.json");
    if !manifest_path.is_file() {
        return Ok(Vec::new());
    }

    let raw = std::fs::read_to_string(&manifest_path).map_err(|source| {
        crate::SchemaRegistryError::Io {
            path: manifest_path.display().to_string(),
            source,
        }
    })?;
    let manifest: DatabaseManifest =
        serde_json::from_str(&raw).map_err(|source| crate::SchemaRegistryError::Json {
            path: manifest_path.display().to_string(),
            source,
        })?;

    let mut deps = manifest
        .modules
        .unwrap_or_default()
        .into_iter()
        .map(|module| RegistryDependency {
            module_id: module.module_id.clone(),
            locator: module.locator,
            registry_path: PathBuf::from(format!(
                "docs/schema-registry/{}.tables.yaml",
                module.module_id
            )),
            order: module.order.unwrap_or(0),
            ownership: "read_only".to_string(),
        })
        .collect::<Vec<_>>();
    deps.sort_by(|left, right| {
        left.order
            .cmp(&right.order)
            .then_with(|| left.module_id.cmp(&right.module_id))
    });
    Ok(deps)
}

pub fn parse_registry_dependencies(
    value: &serde_yaml::Value,
) -> Result<Vec<RegistryDependency>, crate::SchemaRegistryError> {
    let Some(items) = value.as_sequence() else {
        return Err(crate::SchemaRegistryError::InvalidShape {
            path: "registry_dependencies".to_string(),
            message: "must be a list".to_string(),
        });
    };

    let mut deps = Vec::new();
    for item in items {
        let mapping =
            item.as_mapping()
                .ok_or_else(|| crate::SchemaRegistryError::InvalidShape {
                    path: "registry_dependencies".to_string(),
                    message: "entries must be mappings".to_string(),
                })?;
        let module_id = mapping
            .get(serde_yaml::Value::from("module_id"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| crate::SchemaRegistryError::InvalidShape {
                path: "registry_dependencies".to_string(),
                message: "module_id is required".to_string(),
            })?
            .to_string();
        let locator = mapping
            .get(serde_yaml::Value::from("locator"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| crate::SchemaRegistryError::InvalidShape {
                path: "registry_dependencies".to_string(),
                message: "locator is required".to_string(),
            })?;
        let registry_path = mapping
            .get(serde_yaml::Value::from("registry_path"))
            .and_then(|value| value.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(format!("docs/schema-registry/{module_id}.tables.yaml"))
            });
        let order = mapping
            .get(serde_yaml::Value::from("order"))
            .and_then(|value| value.as_i64())
            .unwrap_or(0) as i32;
        let ownership = mapping
            .get(serde_yaml::Value::from("ownership"))
            .and_then(|value| value.as_str())
            .unwrap_or("read_only")
            .to_string();
        deps.push(RegistryDependency {
            module_id,
            locator: PathBuf::from(locator),
            registry_path,
            order,
            ownership,
        });
    }
    deps.sort_by(|left, right| {
        left.order
            .cmp(&right.order)
            .then_with(|| left.module_id.cmp(&right.module_id))
    });
    Ok(deps)
}
