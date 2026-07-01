use crate::services::WebFrameworkAdminService;
use sdkwork_web_core::DynamicPolicyCaches;
use sdkwork_web_framework_admin_repository_sqlx::{
    SqlxWebFrameworkAdminRepository, WebFrameworkAdminRepository,
};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct WebFrameworkAdminState {
    pub service: WebFrameworkAdminService,
}

impl WebFrameworkAdminState {
    pub fn new(pool: SqlitePool) -> Self {
        Self::from_repository(Arc::new(SqlxWebFrameworkAdminRepository::new(pool)))
    }

    pub fn from_repository(repository: Arc<dyn WebFrameworkAdminRepository>) -> Self {
        Self {
            service: WebFrameworkAdminService::new(repository),
        }
    }

    pub fn with_policy_caches(mut self, caches: Arc<DynamicPolicyCaches>) -> Self {
        self.service = self.service.clone().with_policy_caches(caches);
        self
    }
}
