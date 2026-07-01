//! Canonical path constants for framework control-plane backend-api (`WEB_BACKEND_SPEC.md`).
//!
//! 路径命名遵循 `API_SPEC.md §5.1`：静态段使用 `lower_snake_case`，路径参数使用
//! `lowerCamelCase`（如 `{nodeId}`）。禁止 kebab-case 静态段或 snake_case 路径参数。

macro_rules! framework_path {
    ($suffix:literal) => {
        concat!("/backend/v3/api/web-framework", $suffix)
    };
}

pub const API_PREFIX: &str = "/backend/v3/api/web-framework";

pub mod cors {
    pub const PATH: &str = framework_path!("/cors_policies");
}

pub mod rate_limit {
    pub const PATH: &str = framework_path!("/rate_limit_policies");
}

pub mod tenant_runtime {
    pub const PATH: &str = framework_path!("/tenant_runtime_profiles");
}

pub mod security_events {
    pub const PATH: &str = framework_path!("/security_events");
}

pub mod audit_events {
    pub const PATH: &str = framework_path!("/audit_events");
}

pub mod control_nodes {
    pub const COLLECTION: &str = framework_path!("/control_nodes");
    pub const BY_ID: &str = framework_path!("/control_nodes/{nodeId}");
    pub const HEARTBEAT: &str = framework_path!("/control_nodes/{nodeId}/heartbeat");
}

pub mod runtime_defaults {
    pub const PATH: &str = framework_path!("/runtime_defaults");
}

pub mod optional_features {
    pub const PATH: &str = framework_path!("/optional_features");
}
