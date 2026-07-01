use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_yaml::{Mapping, Value};

use crate::error::SchemaRegistryError;
use crate::types::{
    parse_registry_dependencies, resolve_registry_dependencies, RegistryDependency,
};

pub struct SchemaRegistryComposer {
    app_root: PathBuf,
    registry_path: PathBuf,
    require_dependency_registries: bool,
}

impl SchemaRegistryComposer {
    pub fn new(app_root: impl AsRef<Path>, registry_path: impl AsRef<Path>) -> Self {
        Self {
            app_root: app_root.as_ref().to_path_buf(),
            registry_path: registry_path.as_ref().to_path_buf(),
            require_dependency_registries: false,
        }
    }

    pub fn require_dependency_registries(mut self, require: bool) -> Self {
        self.require_dependency_registries = require;
        self
    }

    pub fn compose(&self) -> Result<Value, SchemaRegistryError> {
        compose_schema_registry(
            &self.app_root,
            &self.registry_path,
            self.require_dependency_registries,
        )
    }

    pub fn render_yaml(&self) -> Result<String, SchemaRegistryError> {
        serde_yaml::to_string(&self.compose()?).map_err(|source| {
            SchemaRegistryError::InvalidShape {
                path: self.registry_path.display().to_string(),
                message: source.to_string(),
            }
        })
    }

    pub fn source_paths(&self) -> Result<Vec<PathBuf>, SchemaRegistryError> {
        schema_registry_source_paths(
            &self.app_root,
            &self.registry_path,
            self.require_dependency_registries,
        )
    }
}

pub fn load_schema_registry(
    app_root: impl AsRef<Path>,
    registry_path: impl AsRef<Path>,
) -> Result<Value, SchemaRegistryError> {
    SchemaRegistryComposer::new(app_root, registry_path).compose()
}

pub fn render_schema_registry(
    app_root: impl AsRef<Path>,
    registry_path: impl AsRef<Path>,
) -> Result<String, SchemaRegistryError> {
    SchemaRegistryComposer::new(app_root, registry_path).render_yaml()
}

pub fn schema_registry_source_paths(
    app_root: impl AsRef<Path>,
    registry_path: impl AsRef<Path>,
    require_dependency_registries: bool,
) -> Result<Vec<PathBuf>, SchemaRegistryError> {
    let app_root = app_root.as_ref();
    let registry_path = registry_path.as_ref();
    let entry = read_yaml_mapping(registry_path)?;
    let mut paths = vec![registry_path.to_path_buf()];
    paths.extend(local_fragment_paths(registry_path, &entry)?);

    let deps = load_dependencies(app_root, registry_path, &entry)?;
    for dependency in deps {
        let dependency_registry = dependency.resolve_registry_file(app_root);
        if dependency_registry.is_file() {
            let dependency_entry = read_yaml_mapping(&dependency_registry)?;
            paths.push(dependency_registry.clone());
            paths.extend(local_fragment_paths(
                &dependency_registry,
                &dependency_entry,
            )?);
        } else if require_dependency_registries {
            return Err(SchemaRegistryError::DependencyRegistryNotFound {
                module_id: dependency.module_id,
                path: dependency_registry.display().to_string(),
            });
        }
    }
    Ok(paths)
}

fn compose_schema_registry(
    app_root: &Path,
    registry_path: &Path,
    require_dependency_registries: bool,
) -> Result<Value, SchemaRegistryError> {
    let entry = read_yaml_mapping(registry_path)?;
    let mut merged = entry.clone();

    let dependency_tables = load_dependency_tables(
        app_root,
        registry_path,
        &entry,
        require_dependency_registries,
    )?;
    let assembly_tables = collect_local_tables(registry_path, &entry)?;

    validate_assembly_tables(&dependency_tables, &assembly_tables)?;

    let mut effective_tables = dependency_tables;
    for table in assembly_tables {
        let table_name = table_name(&table)?;
        if let Some(existing) = effective_tables.get(&table_name) {
            if !table_matches(existing, &table) {
                return Err(SchemaRegistryError::DuplicateTable {
                    table: table_name,
                    origin: "assembly registry conflicts with dependency table definition"
                        .to_string(),
                });
            }
            continue;
        }
        effective_tables.insert(table_name, annotate_table(table, false, None));
    }

    let mut tables = effective_tables.into_values().collect::<Vec<_>>();
    sort_tables(&mut tables);
    merged.insert(
        Value::from("tables"),
        Value::Sequence(tables.into_iter().collect()),
    );
    merged.remove(Value::from("table_fragments"));
    merged.remove(Value::from("registry_dependencies"));
    Ok(Value::Mapping(merged))
}

fn load_dependencies(
    app_root: &Path,
    _registry_path: &Path,
    entry: &Mapping,
) -> Result<Vec<RegistryDependency>, SchemaRegistryError> {
    let explicit = entry
        .get(Value::from("registry_dependencies"))
        .map(parse_registry_dependencies)
        .transpose()?;
    resolve_registry_dependencies(app_root, explicit.as_deref())
}

fn load_dependency_tables(
    app_root: &Path,
    registry_path: &Path,
    entry: &Mapping,
    require_dependency_registries: bool,
) -> Result<BTreeMap<String, Value>, SchemaRegistryError> {
    let mut tables = BTreeMap::new();
    for dependency in load_dependencies(app_root, registry_path, entry)? {
        let dependency_registry = dependency.resolve_registry_file(app_root);
        if !dependency_registry.is_file() {
            if require_dependency_registries {
                return Err(SchemaRegistryError::DependencyRegistryNotFound {
                    module_id: dependency.module_id.clone(),
                    path: dependency_registry.display().to_string(),
                });
            }
            continue;
        }
        let dependency_entry = read_yaml_mapping(&dependency_registry)?;
        for table in collect_local_tables(&dependency_registry, &dependency_entry)? {
            let name = table_name(&table)?;
            if tables.contains_key(&name) {
                return Err(SchemaRegistryError::DuplicateTable {
                    table: name,
                    origin: "dependency modules declare the same table".to_string(),
                });
            }
            tables.insert(
                name,
                annotate_table(table, true, Some(dependency.module_id.clone())),
            );
        }
    }
    Ok(tables)
}

fn validate_assembly_tables(
    dependency_tables: &BTreeMap<String, Value>,
    assembly_tables: &[Value],
) -> Result<(), SchemaRegistryError> {
    for table in assembly_tables {
        let name = table_name(table)?;
        if !dependency_tables.contains_key(&name) {
            continue;
        }
        let imported = table
            .get("imported")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let generated = table
            .get("generated_by_this_project")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let system_of_record = table
            .get("system_of_record")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let has_source_refs = table
            .get("source_refs")
            .and_then(Value::as_sequence)
            .is_some_and(|items| !items.is_empty());
        if imported || (!generated && !system_of_record) || has_source_refs {
            continue;
        }
        return Err(SchemaRegistryError::MissingSourceRefs {
            table: name.clone(),
            dependency: name,
        });
    }
    Ok(())
}

fn collect_local_tables(
    registry_path: &Path,
    entry: &Mapping,
) -> Result<Vec<Value>, SchemaRegistryError> {
    let mut tables = inline_tables(entry)?;
    for fragment_path in fragment_paths(registry_path, entry)? {
        let fragment = read_yaml_mapping(&fragment_path)?;
        tables.extend(inline_tables(&fragment)?);
    }
    Ok(tables)
}

fn inline_tables(entry: &Mapping) -> Result<Vec<Value>, SchemaRegistryError> {
    let Some(value) = entry.get(Value::from("tables")) else {
        return Ok(Vec::new());
    };
    let Some(sequence) = value.as_sequence() else {
        return Err(SchemaRegistryError::InvalidShape {
            path: "tables".to_string(),
            message: "must be a list".to_string(),
        });
    };
    Ok(sequence.clone())
}

fn fragment_paths(
    registry_path: &Path,
    entry: &Mapping,
) -> Result<Vec<PathBuf>, SchemaRegistryError> {
    let Some(value) = entry.get(Value::from("table_fragments")) else {
        return Ok(Vec::new());
    };
    let Some(sequence) = value.as_sequence() else {
        return Err(SchemaRegistryError::InvalidShape {
            path: "table_fragments".to_string(),
            message: "must be a list".to_string(),
        });
    };
    let registry_dir = registry_path
        .parent()
        .ok_or_else(|| SchemaRegistryError::InvalidShape {
            path: registry_path.display().to_string(),
            message: "registry path must have a parent directory".to_string(),
        })?;
    let mut paths = Vec::new();
    for item in sequence {
        let fragment = item
            .as_str()
            .ok_or_else(|| SchemaRegistryError::InvalidShape {
                path: "table_fragments".to_string(),
                message: "entries must be non-empty strings".to_string(),
            })?;
        if fragment.is_empty() {
            return Err(SchemaRegistryError::InvalidShape {
                path: "table_fragments".to_string(),
                message: "entries must be non-empty strings".to_string(),
            });
        }
        let fragment_path = registry_dir.join(fragment).canonicalize().map_err(|_| {
            SchemaRegistryError::FragmentNotFound(registry_dir.join(fragment).display().to_string())
        })?;
        if !fragment_path.starts_with(
            registry_dir
                .canonicalize()
                .unwrap_or_else(|_| registry_dir.to_path_buf()),
        ) {
            return Err(SchemaRegistryError::FragmentEscape(fragment.to_string()));
        }
        paths.push(fragment_path);
    }
    Ok(paths)
}

fn local_fragment_paths(
    registry_path: &Path,
    entry: &Mapping,
) -> Result<Vec<PathBuf>, SchemaRegistryError> {
    fragment_paths(registry_path, entry)
}

fn table_name(table: &Value) -> Result<String, SchemaRegistryError> {
    table
        .get("table")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| SchemaRegistryError::InvalidShape {
            path: "tables".to_string(),
            message: "each table entry must declare table".to_string(),
        })
}

fn annotate_table(table: Value, imported: bool, dependency_module: Option<String>) -> Value {
    let Value::Mapping(mut mapping) = table else {
        return table;
    };
    mapping.insert(Value::from("imported"), Value::Bool(imported));
    if imported {
        mapping.insert(Value::from("generated_by_this_project"), Value::Bool(false));
        if let Some(module_id) = dependency_module {
            mapping.insert(
                Value::from("imported_from_module"),
                Value::String(module_id),
            );
        }
    }
    Value::Mapping(mapping)
}

fn table_matches(left: &Value, right: &Value) -> bool {
    normalize_table(left) == normalize_table(right)
}

fn normalize_table(table: &Value) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    if let Some(mapping) = table.as_mapping() {
        for key in mapping.keys() {
            if key.as_str() == Some("imported") || key.as_str() == Some("imported_from_module") {
                continue;
            }
            keys.insert(format!("{key:?}"));
        }
    }
    keys
}

fn sort_tables(tables: &mut [Value]) {
    tables.sort_by(|left, right| {
        table_name(left)
            .unwrap_or_default()
            .cmp(&table_name(right).unwrap_or_default())
    });
}

fn read_yaml_mapping(path: &Path) -> Result<Mapping, SchemaRegistryError> {
    if !path.is_file() {
        return Err(SchemaRegistryError::RegistryNotFound(
            path.display().to_string(),
        ));
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
    fn merges_local_fragments() {
        let dir = tempdir().unwrap();
        let registry_dir = dir.path().join("docs/schema-registry");
        fs::create_dir_all(registry_dir.join("tables")).unwrap();
        fs::write(
            registry_dir.join("app.tables.yaml"),
            r#"
tables:
  - table: ai_runtime
    domain: ai
table_fragments:
  - tables/010-ai.yaml
"#,
        )
        .unwrap();
        fs::write(
            registry_dir.join("tables/010-ai.yaml"),
            r#"
tables:
  - table: ai_channel
    domain: ai
"#,
        )
        .unwrap();

        let composed =
            compose_schema_registry(dir.path(), &registry_dir.join("app.tables.yaml"), false)
                .unwrap();
        let tables = composed.get("tables").unwrap().as_sequence().unwrap();
        assert_eq!(tables.len(), 2);
        assert!(tables
            .iter()
            .any(|table| table.get("table").and_then(Value::as_str) == Some("ai_runtime")));
        assert!(tables
            .iter()
            .any(|table| table.get("table").and_then(Value::as_str) == Some("ai_channel")));
    }

    #[test]
    fn rejects_conflicting_dependency_projection_without_source_refs() {
        let dir = tempdir().unwrap();
        let registry_dir = dir.path().join("docs/schema-registry");
        let dependency_dir = dir.path().join("dependency");
        fs::create_dir_all(&registry_dir).unwrap();
        fs::create_dir_all(dependency_dir.join("docs/schema-registry")).unwrap();
        fs::create_dir_all(dir.path().join("database")).unwrap();
        fs::write(
            dependency_dir.join("docs/schema-registry/commerce-core.tables.yaml"),
            r#"
tables:
  - table: commerce_wallet
    domain: commerce
    system_of_record: true
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("database/database.manifest.json"),
            r#"{"modules":[{"moduleId":"commerce-core","locator":"dependency","order":10}]}"#,
        )
        .unwrap();
        fs::write(
            registry_dir.join("app.tables.yaml"),
            r#"
tables:
  - table: commerce_wallet
    domain: commerce
    system_of_record: true
    generated_by_this_project: true
"#,
        )
        .unwrap();

        let error =
            compose_schema_registry(dir.path(), &registry_dir.join("app.tables.yaml"), false)
                .unwrap_err();
        assert!(matches!(
            error,
            SchemaRegistryError::MissingSourceRefs { .. }
        ));
    }
}
