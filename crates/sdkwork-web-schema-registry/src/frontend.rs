use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_yaml::{Mapping, Value};

use crate::error::SchemaRegistryError;

const METADATA_KEYS: &[&str] = &["schema", "version", "source", "rule"];
const MERGEABLE_LIST_SECTIONS: &[&str] = &["frontend_models", "frontend_operations", "routes"];
const MERGEABLE_MAPPING_SECTIONS: &[&str] = &["x_response_entities"];

pub struct FrontendContractComposer {
    app_root: PathBuf,
    index_path: PathBuf,
}

impl FrontendContractComposer {
    pub fn new(app_root: impl AsRef<Path>, index_path: impl AsRef<Path>) -> Self {
        Self {
            app_root: app_root.as_ref().to_path_buf(),
            index_path: index_path.as_ref().to_path_buf(),
        }
    }

    pub fn compose(&self) -> Result<Value, SchemaRegistryError> {
        compose_frontend_field_contract(&self.app_root, &self.index_path)
    }

    pub fn render_yaml(&self) -> Result<String, SchemaRegistryError> {
        serde_yaml::to_string(&self.compose()?)
            .map_err(|source| SchemaRegistryError::Frontend(source.to_string()))
    }
}

pub struct FrontendContractSnapshot {
    pub index_path: PathBuf,
    pub snapshot_path: PathBuf,
}

impl FrontendContractSnapshot {
    pub fn check_stale(&self) -> Result<(), SchemaRegistryError> {
        if !self.index_path.is_file() {
            return Ok(());
        }
        let compiled = FrontendContractComposer::new(
            self.index_path.parent().unwrap_or_else(|| Path::new(".")),
            &self.index_path,
        )
        .compose()?;
        let snapshot = read_yaml_mapping(&self.snapshot_path)?;
        if snapshot != compiled.as_mapping().cloned().unwrap_or_default() {
            return Err(SchemaRegistryError::StaleSnapshot(
                self.snapshot_path.display().to_string(),
            ));
        }
        Ok(())
    }
}

pub fn compose_frontend_field_contract(
    app_root: impl AsRef<Path>,
    index_path: impl AsRef<Path>,
) -> Result<Value, SchemaRegistryError> {
    let _app_root = app_root.as_ref();
    let index_path = index_path.as_ref();
    let index = read_yaml_mapping(index_path)?;
    if !is_fragment_index(&index) {
        return Ok(Value::Mapping(index));
    }

    let mut compiled = Mapping::new();
    for key in METADATA_KEYS {
        if let Some(value) = index.get(Value::from(*key)) {
            compiled.insert(Value::from(*key), value.clone());
        }
    }

    let fragments = index
        .get(Value::from("fragments"))
        .and_then(Value::as_sequence)
        .ok_or_else(|| {
            SchemaRegistryError::Frontend(
                "frontend field contract index fragments must be a non-empty list".to_string(),
            )
        })?;
    if fragments.is_empty() {
        return Err(SchemaRegistryError::Frontend(
            "frontend field contract index fragments must be a non-empty list".to_string(),
        ));
    }

    let mut seen = BTreeSet::new();
    for fragment_ref in fragments {
        let fragment_path = fragment_path(index_path, fragment_ref)?;
        if !seen.insert(fragment_path.clone()) {
            return Err(SchemaRegistryError::Frontend(format!(
                "duplicate frontend field contract fragment: {}",
                fragment_path.display()
            )));
        }
        let fragment = read_yaml_mapping(&fragment_path)?;
        merge_fragment(&mut compiled, &fragment, &fragment_path)?;
    }

    for section in MERGEABLE_LIST_SECTIONS {
        compiled
            .entry(Value::from(*section))
            .or_insert_with(|| Value::Sequence(Vec::new()));
    }
    for section in MERGEABLE_MAPPING_SECTIONS {
        compiled
            .entry(Value::from(*section))
            .or_insert_with(|| Value::Mapping(Mapping::new()));
    }
    Ok(Value::Mapping(compiled))
}

fn is_fragment_index(index: &Mapping) -> bool {
    index
        .get(Value::from("fragments"))
        .and_then(Value::as_sequence)
        .is_some()
}

fn fragment_path(index_path: &Path, fragment_ref: &Value) -> Result<PathBuf, SchemaRegistryError> {
    let raw_path = if let Some(path) = fragment_ref.as_str() {
        path.to_string()
    } else if let Some(mapping) = fragment_ref.as_mapping() {
        mapping
            .get(Value::from("path"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SchemaRegistryError::Frontend("fragment mappings must declare path".to_string())
            })?
            .to_string()
    } else {
        return Err(SchemaRegistryError::Frontend(
            "frontend field contract fragment entries must be strings or mappings with path"
                .to_string(),
        ));
    };
    let candidate = PathBuf::from(&raw_path);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| component.as_os_str() == "..")
    {
        return Err(SchemaRegistryError::Frontend(format!(
            "frontend field contract fragment path must stay inside the contract directory: {raw_path}"
        )));
    }
    Ok(index_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(candidate))
}

fn merge_fragment(
    compiled: &mut Mapping,
    fragment: &Mapping,
    fragment_path: &Path,
) -> Result<(), SchemaRegistryError> {
    for (key, value) in fragment {
        let key_name = key.as_str().unwrap_or("");
        if METADATA_KEYS.contains(&key_name) {
            continue;
        }
        if MERGEABLE_LIST_SECTIONS.contains(&key_name) {
            let target = compiled
                .entry(key.clone())
                .or_insert_with(|| Value::Sequence(Vec::new()));
            let Value::Sequence(items) = value else {
                return Err(SchemaRegistryError::Frontend(format!(
                    "{key_name} must be a list in {}",
                    fragment_path.display()
                )));
            };
            let Value::Sequence(target_items) = target else {
                return Err(SchemaRegistryError::Frontend(format!(
                    "compiled {key_name} must be a list"
                )));
            };
            target_items.extend(items.clone());
            continue;
        }
        if MERGEABLE_MAPPING_SECTIONS.contains(&key_name) {
            let target = compiled
                .entry(key.clone())
                .or_insert_with(|| Value::Mapping(Mapping::new()));
            let Value::Mapping(source) = value else {
                return Err(SchemaRegistryError::Frontend(format!(
                    "{key_name} must be a mapping in {}",
                    fragment_path.display()
                )));
            };
            let Value::Mapping(target_map) = target else {
                return Err(SchemaRegistryError::Frontend(format!(
                    "compiled {key_name} must be a mapping"
                )));
            };
            for (entry_key, entry_value) in source {
                if target_map.contains_key(entry_key) {
                    return Err(SchemaRegistryError::Frontend(format!(
                        "duplicate frontend contract key `{entry_key:?}` in {}",
                        fragment_path.display()
                    )));
                }
                target_map.insert(entry_key.clone(), entry_value.clone());
            }
            continue;
        }
        if compiled.contains_key(key) {
            return Err(SchemaRegistryError::Frontend(format!(
                "duplicate frontend contract section `{key_name}` in {}",
                fragment_path.display()
            )));
        }
        compiled.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn read_yaml_mapping(path: &Path) -> Result<Mapping, SchemaRegistryError> {
    if !path.is_file() {
        return Ok(Mapping::new());
    }
    let raw = std::fs::read_to_string(path).map_err(|source| SchemaRegistryError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let value: Value = serde_yaml::from_str(&raw).map_err(|source| SchemaRegistryError::Yaml {
        path: path.display().to_string(),
        source,
    })?;
    value
        .as_mapping()
        .cloned()
        .ok_or_else(|| SchemaRegistryError::InvalidShape {
            path: path.display().to_string(),
            message: "root must be a mapping".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn merges_frontend_fragments() {
        let dir = tempdir().unwrap();
        let contract_dir = dir
            .path()
            .join("docs/schema-registry/frontend-field-contracts");
        fs::create_dir_all(contract_dir.join("models")).unwrap();
        fs::write(
            contract_dir.join("index.yaml"),
            r#"
schema: demo
version: 0.1.0
fragments:
  - models/a.yaml
  - models/b.yaml
"#,
        )
        .unwrap();
        fs::write(
            contract_dir.join("models/a.yaml"),
            "frontend_models:\n  - id: a\n",
        )
        .unwrap();
        fs::write(
            contract_dir.join("models/b.yaml"),
            "frontend_models:\n  - id: b\n",
        )
        .unwrap();

        let composed =
            compose_frontend_field_contract(dir.path(), contract_dir.join("index.yaml")).unwrap();
        let models = composed
            .get("frontend_models")
            .and_then(Value::as_sequence)
            .unwrap();
        assert_eq!(models.len(), 2);
    }
}
