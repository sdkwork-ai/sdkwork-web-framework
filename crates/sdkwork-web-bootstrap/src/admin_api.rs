use axum::Router;
use sdkwork_routes_web_framework_backend_api::build_admin_router_with_options;
use sdkwork_web_axum::{with_web_request_context, WebFrameworkLayer};
use sdkwork_web_core::{DynamicPolicyCaches, WebRequestContextResolver};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct WebFrameworkAdminMount {
    pool: SqlitePool,
    policy_caches: Option<Arc<DynamicPolicyCaches>>,
}

impl WebFrameworkAdminMount {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            policy_caches: None,
        }
    }

    pub fn with_policy_caches(mut self, caches: Arc<DynamicPolicyCaches>) -> Self {
        self.policy_caches = Some(caches);
        self
    }

    pub fn merge_protected<R>(self, router: Router, layer: WebFrameworkLayer<R>) -> Router
    where
        R: WebRequestContextResolver + Clone,
    {
        router.merge(with_web_request_context(
            build_admin_router_with_options(self.pool, self.policy_caches),
            layer,
        ))
    }
}

pub fn mount_web_framework_admin_api<R>(
    router: Router,
    pool: SqlitePool,
    layer: WebFrameworkLayer<R>,
    policy_caches: Option<Arc<DynamicPolicyCaches>>,
) -> Router
where
    R: WebRequestContextResolver + Clone,
{
    let mount = WebFrameworkAdminMount::new(pool);
    let mount = if let Some(caches) = policy_caches {
        mount.with_policy_caches(caches)
    } else {
        mount
    };
    mount.merge_protected(router, layer)
}
