//! Composable schema registry loading for SDKWork applications.

mod composer;
mod error;
mod frontend;
mod types;

pub use composer::{
    load_schema_registry, render_schema_registry, schema_registry_source_paths,
    SchemaRegistryComposer,
};
pub use error::SchemaRegistryError;
pub use frontend::{
    compose_frontend_field_contract, FrontendContractComposer, FrontendContractSnapshot,
};
pub use types::RegistryDependency;
