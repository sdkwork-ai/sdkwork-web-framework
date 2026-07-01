//! Optional runtime feature switches — all default off until explicitly enabled.

use serde::{Deserialize, Serialize};

/// Governs which dynamic policy overlays and guards are active at runtime.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct WebFrameworkOptionalFeatures {
    pub dynamic_cors_policy: bool,
    pub dynamic_rate_limit_policy: bool,
    pub dynamic_tenant_runtime_profile: bool,
    pub json_content_type_guard: bool,
    /// When true, [`SecurityPolicy::production`] static defaults apply unless overridden.
    pub production_security_defaults: bool,
    /// Control-plane single-node profile: allows in-memory stores and bootstrap signing lookup.
    pub control_plane_standalone: bool,
}

impl WebFrameworkOptionalFeatures {
    pub fn development() -> Self {
        Self::default()
    }

    pub fn production_sqlx() -> Self {
        Self {
            dynamic_cors_policy: true,
            dynamic_rate_limit_policy: true,
            dynamic_tenant_runtime_profile: true,
            json_content_type_guard: true,
            production_security_defaults: true,
            control_plane_standalone: false,
        }
    }

    pub fn control_plane_standalone(mut self) -> Self {
        self.control_plane_standalone = true;
        self
    }
}
