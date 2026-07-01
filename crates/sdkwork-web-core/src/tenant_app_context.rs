use crate::error::WebFrameworkError;
use crate::request_context::{WebEnvironment, WebLoginScope, WebRequestContext};

/// Service-layer view of tenant + app + subject identifiers (spec §4).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TenantAppContext {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub app_id: String,
    pub user_id: String,
    pub session_id: Option<String>,
    pub environment: WebEnvironment,
    pub login_scope: WebLoginScope,
}

impl TenantAppContext {
    pub fn try_from_request_context(ctx: &WebRequestContext) -> Result<Self, WebFrameworkError> {
        let principal = ctx.require_principal()?;
        Ok(Self {
            tenant_id: principal.tenant_id().to_owned(),
            organization_id: principal.organization_id().map(str::to_owned),
            app_id: principal.app_id().to_owned(),
            user_id: principal.user_id().to_owned(),
            session_id: principal.subject.session_id.clone(),
            environment: principal.app.environment.clone(),
            login_scope: principal.tenancy.login_scope.clone(),
        })
    }
}

impl TryFrom<&WebRequestContext> for TenantAppContext {
    type Error = WebFrameworkError;

    fn try_from(ctx: &WebRequestContext) -> Result<Self, Self::Error> {
        Self::try_from_request_context(ctx)
    }
}
