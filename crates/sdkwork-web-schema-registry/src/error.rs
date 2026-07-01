use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaRegistryError {
    #[error("schema registry not found: {0}")]
    RegistryNotFound(String),
    #[error("dependency registry not found: module={module_id}, path={path}")]
    DependencyRegistryNotFound { module_id: String, path: String },
    #[error("invalid registry shape at {path}: {message}")]
    InvalidShape { path: String, message: String },
    #[error("duplicate table `{table}` from {origin}")]
    DuplicateTable { table: String, origin: String },
    #[error(
        "table `{table}` conflicts with dependency-owned table `{dependency}` without source_refs"
    )]
    MissingSourceRefs { table: String, dependency: String },
    #[error("table fragment must stay under schema registry directory: {0}")]
    FragmentEscape(String),
    #[error("table fragment not found: {0}")]
    FragmentNotFound(String),
    #[error("failed to read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse YAML at {path}: {source}")]
    Yaml {
        path: String,
        source: serde_yaml::Error,
    },
    #[error("failed to parse JSON at {path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
    #[error("frontend contract error: {0}")]
    Frontend(String),
    #[error("generated snapshot is stale: {0}")]
    StaleSnapshot(String),
}
