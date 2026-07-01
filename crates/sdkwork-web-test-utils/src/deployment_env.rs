//! Process env isolation for dev-builder integration tests.

use std::sync::Mutex;

static DEPLOYMENT_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Clears `SDKWORK_WEB_FRAMEWORK_ENV=prod` for dev-builder tests that must not
/// require `production_defaults()` wiring.
pub struct IsolatedDeploymentEnv {
    previous: Option<String>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl IsolatedDeploymentEnv {
    pub fn enter() -> Self {
        let _lock = DEPLOYMENT_ENV_LOCK
            .lock()
            .expect("deployment env isolation lock");
        let previous = std::env::var("SDKWORK_WEB_FRAMEWORK_ENV").ok();
        std::env::remove_var("SDKWORK_WEB_FRAMEWORK_ENV");
        Self { previous, _lock }
    }
}

impl Drop for IsolatedDeploymentEnv {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("SDKWORK_WEB_FRAMEWORK_ENV", value),
            None => std::env::remove_var("SDKWORK_WEB_FRAMEWORK_ENV"),
        }
    }
}
