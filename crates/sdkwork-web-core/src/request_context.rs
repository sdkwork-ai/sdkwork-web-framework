use crate::request_identity::ServerRequestId;
use sdkwork_web_contract::RateLimitTier;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebApiSurface {
    OpenApi,
    AppApi,
    BackendApi,
    GatewayApi,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebAuthMode {
    Public,
    ApiKey,
    OAuth,
    DualToken,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebLoginScope {
    Tenant,
    Organization,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebEnvironment {
    Dev,
    Test,
    Prod,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebDeploymentMode {
    Saas,
    Private,
    Local,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebAuthLevel {
    Anonymous,
    Password,
    Mfa,
    System,
    #[serde(rename = "apiKey")]
    ApiKey,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSubjectType {
    User,
    Service,
    #[serde(rename = "apiKey")]
    ApiKey,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebTransportFacts {
    pub path: String,
    pub method: String,
    pub auth_token_present: bool,
    pub access_token_present: bool,
    pub api_key_present: bool,
    pub oauth_bearer_present: bool,
    /// Backend agent bootstrap token presence (C8-C9).
    pub agent_token_present: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebClientKind {
    Browser,
    Mobile,
    Desktop,
    Server,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebOperationBinding {
    pub operation_id: String,
    pub route_template: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_tier: Option<RateLimitTier>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub idempotent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebTenancyContext {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub login_scope: WebLoginScope,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebAppContext {
    pub app_id: String,
    pub environment: WebEnvironment,
    pub deployment_mode: WebDeploymentMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composition_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSubjectContext {
    pub user_id: String,
    pub session_id: Option<String>,
    pub subject_type: WebSubjectType,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebAuthContext {
    pub auth_level: WebAuthLevel,
    pub api_key_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebScopeContext {
    pub data_scope: Vec<String>,
    pub permission_scope: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebRequestPrincipal {
    pub tenancy: WebTenancyContext,
    pub app: WebAppContext,
    pub subject: WebSubjectContext,
    pub auth: WebAuthContext,
    pub scopes: WebScopeContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebRequestContext {
    #[serde(rename = "requestId")]
    pub request_id: ServerRequestId,
    pub api_surface: WebApiSurface,
    pub auth_mode: WebAuthMode,
    pub transport: WebTransportFacts,
    pub principal: Option<WebRequestPrincipal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(rename = "clientKind", skip_serializing_if = "Option::is_none")]
    pub client_kind: Option<WebClientKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<WebOperationBinding>,
    /// W3C trace id extracted from `traceparent` at the framework boundary (OBSERVABILITY_SPEC §1).
    #[serde(rename = "traceId", skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebRequestContextProfile {
    pub app_api_prefix: String,
    pub backend_api_prefix: String,
    pub open_api_prefixes: Vec<String>,
    pub public_path_prefixes: Vec<String>,
    pub gateway_api_prefixes: Vec<String>,
    /// Deployment environment for dynamic policy lookups (e.g. EP-16 CORS).
    pub environment: WebEnvironment,
}

impl Default for WebRequestContextProfile {
    fn default() -> Self {
        Self {
            app_api_prefix: crate::constants::APP_API_PREFIX.to_owned(),
            backend_api_prefix: crate::constants::BACKEND_API_PREFIX.to_owned(),
            open_api_prefixes: vec![crate::constants::OPEN_API_PREFIX.to_owned()],
            public_path_prefixes: vec![
                "/healthz".to_owned(),
                "/readyz".to_owned(),
                "/livez".to_owned(),
                "/metrics".to_owned(),
            ],
            gateway_api_prefixes: vec![crate::constants::GATEWAY_API_PREFIX.to_owned()],
            environment: WebEnvironment::Dev,
        }
    }
}

impl WebRequestPrincipal {
    pub fn builder() -> WebRequestPrincipalBuilder {
        WebRequestPrincipalBuilder::default()
    }

    pub fn tenant_id(&self) -> &str {
        &self.tenancy.tenant_id
    }

    pub fn organization_id(&self) -> Option<&str> {
        self.tenancy.organization_id.as_deref()
    }

    pub fn login_scope(&self) -> WebLoginScope {
        self.tenancy.login_scope.clone()
    }

    pub fn app_id(&self) -> &str {
        &self.app.app_id
    }

    pub fn user_id(&self) -> &str {
        &self.subject.user_id
    }

    pub fn session_id(&self) -> Option<&str> {
        self.subject.session_id.as_deref()
    }

    pub fn auth_level(&self) -> WebAuthLevel {
        self.auth.auth_level.clone()
    }

    pub fn api_key_id(&self) -> Option<&str> {
        self.auth.api_key_id.as_deref()
    }
}

#[derive(Default)]
pub struct WebRequestPrincipalBuilder {
    tenant_id: Option<String>,
    organization_id: Option<String>,
    login_scope: Option<WebLoginScope>,
    user_id: Option<String>,
    session_id: Option<String>,
    app_id: Option<String>,
    environment: Option<WebEnvironment>,
    deployment_mode: Option<WebDeploymentMode>,
    auth_level: Option<WebAuthLevel>,
    data_scope: Vec<String>,
    permission_scope: Vec<String>,
    api_key_id: Option<String>,
    subject_type: Option<WebSubjectType>,
    workspace_id: Option<String>,
    composition_id: Option<String>,
}

impl WebRequestPrincipalBuilder {
    pub fn tenant_id(mut self, value: impl Into<String>) -> Self {
        self.tenant_id = Some(value.into());
        self
    }

    pub fn organization_id(mut self, value: Option<String>) -> Self {
        self.organization_id = value;
        self
    }

    pub fn login_scope(mut self, value: WebLoginScope) -> Self {
        self.login_scope = Some(value);
        self
    }

    pub fn user_id(mut self, value: impl Into<String>) -> Self {
        self.user_id = Some(value.into());
        self
    }

    pub fn session_id(mut self, value: Option<String>) -> Self {
        self.session_id = value;
        self
    }

    pub fn app_id(mut self, value: impl Into<String>) -> Self {
        self.app_id = Some(value.into());
        self
    }

    pub fn environment(mut self, value: WebEnvironment) -> Self {
        self.environment = Some(value);
        self
    }

    pub fn deployment_mode(mut self, value: WebDeploymentMode) -> Self {
        self.deployment_mode = Some(value);
        self
    }

    pub fn auth_level(mut self, value: WebAuthLevel) -> Self {
        self.auth_level = Some(value);
        self
    }

    pub fn data_scope(mut self, value: Vec<String>) -> Self {
        self.data_scope = value;
        self
    }

    pub fn permission_scope(mut self, value: Vec<String>) -> Self {
        self.permission_scope = value;
        self
    }

    pub fn api_key_id(mut self, value: Option<String>) -> Self {
        self.api_key_id = value;
        self
    }

    pub fn subject_type(mut self, value: WebSubjectType) -> Self {
        self.subject_type = Some(value);
        self
    }

    pub fn build(self) -> WebRequestPrincipal {
        let organization_id = self.organization_id;
        let login_scope = self
            .login_scope
            .unwrap_or_else(|| WebLoginScope::from_organization_id(organization_id.as_deref()));
        WebRequestPrincipal {
            tenancy: WebTenancyContext {
                tenant_id: self.tenant_id.unwrap_or_default(),
                organization_id,
                login_scope,
            },
            app: WebAppContext {
                app_id: self.app_id.unwrap_or_default(),
                environment: self.environment.unwrap_or(WebEnvironment::Dev),
                deployment_mode: self.deployment_mode.unwrap_or(WebDeploymentMode::Local),
                workspace_id: self.workspace_id,
                composition_id: self.composition_id,
            },
            subject: WebSubjectContext {
                user_id: self.user_id.unwrap_or_default(),
                session_id: self.session_id,
                subject_type: self.subject_type.unwrap_or(WebSubjectType::User),
            },
            auth: WebAuthContext {
                auth_level: self.auth_level.unwrap_or(WebAuthLevel::Anonymous),
                api_key_id: self.api_key_id,
            },
            scopes: WebScopeContext {
                data_scope: self.data_scope,
                permission_scope: self.permission_scope,
            },
        }
    }
}

impl WebRequestContext {
    pub fn tenant_id(&self) -> Option<&str> {
        self.principal.as_ref().map(WebRequestPrincipal::tenant_id)
    }

    pub fn organization_id(&self) -> Option<&str> {
        self.principal
            .as_ref()
            .and_then(WebRequestPrincipal::organization_id)
    }

    pub fn login_scope(&self) -> Option<WebLoginScope> {
        self.principal
            .as_ref()
            .map(WebRequestPrincipal::login_scope)
    }

    pub fn app_id(&self) -> Option<&str> {
        self.principal.as_ref().map(WebRequestPrincipal::app_id)
    }

    pub fn user_id(&self) -> Option<&str> {
        self.principal.as_ref().map(WebRequestPrincipal::user_id)
    }

    pub fn principal(&self) -> Option<&WebRequestPrincipal> {
        self.principal.as_ref()
    }

    pub fn is_public(&self) -> bool {
        matches!(self.auth_mode, WebAuthMode::Public)
    }

    pub fn require_principal(
        &self,
    ) -> Result<&WebRequestPrincipal, crate::error::WebFrameworkError> {
        self.principal.as_ref().ok_or_else(|| {
            crate::error::WebFrameworkError::missing_credentials(
                "authenticated principal is required",
            )
        })
    }

    pub fn require_tenant_id(&self) -> Result<&str, crate::error::WebFrameworkError> {
        Ok(self.require_principal()?.tenant_id())
    }

    pub fn require_app_id(&self) -> Result<&str, crate::error::WebFrameworkError> {
        Ok(self.require_principal()?.app_id())
    }

    pub fn has_permission(&self, grant: &str) -> bool {
        self.principal.as_ref().is_some_and(|principal| {
            permission_scope_matches_any(&principal.scopes.permission_scope, grant)
        })
    }

    pub fn problem_correlation(&self) -> crate::problem::ProblemCorrelation<'_> {
        crate::problem::ProblemCorrelation::new(
            Some(self.request_id.0.as_str()),
            self.trace_id.as_deref(),
        )
        .with_routing(
            Some(self.transport.method.as_str()),
            self.operation
                .as_ref()
                .map(|operation| operation.route_template.as_str()),
            Some(self.transport.path.as_str()),
            self.operation
                .as_ref()
                .map(|operation| operation.operation_id.as_str()),
        )
    }

    pub fn problem_routing(&self) -> sdkwork_utils_rust::SdkWorkProblemRouting {
        self.problem_correlation().routing()
    }

    /// Server-owned correlation id for success envelopes and Problem+json.
    pub fn resolved_trace_id(&self) -> String {
        self.problem_correlation()
            .resolved_trace_id()
            .unwrap_or_else(|| self.request_id.0.clone())
    }
}

/// Returns true when any granted code authorizes `required` (wildcard-aware).
pub fn permission_scope_matches_any(granted_codes: &[String], required: &str) -> bool {
    granted_codes
        .iter()
        .any(|granted| permission_code_matches(granted, required))
}

/// Returns true when a single granted permission code authorizes `required`.
pub fn permission_code_matches(granted: &str, required: &str) -> bool {
    let granted = granted.trim().to_ascii_lowercase();
    let required = required.trim().to_ascii_lowercase();
    if granted.is_empty() || required.is_empty() {
        return false;
    }
    if granted == "*" || granted == required {
        return true;
    }
    if granted.ends_with(".*") {
        let prefix = granted[..granted.len() - 2].trim_end_matches('.');
        return required == prefix || required.starts_with(&format!("{prefix}."));
    }
    if let Some(action) = granted.strip_prefix("*.") {
        let action = action.trim_start_matches('.');
        return required.ends_with(action) && required.split('.').next_back() == Some(action);
    }
    false
}

impl WebLoginScope {
    pub fn from_organization_id(organization_id: Option<&str>) -> Self {
        match organization_id.map(str::trim) {
            Some(value) if !value.is_empty() && value != "0" => Self::Organization,
            _ => Self::Tenant,
        }
    }
}

// Migration aliases (API_SPEC §10 transitional vocabulary)
pub type AppRequestContext = WebRequestContext;
pub type AppRequestPrincipal = WebRequestPrincipal;
pub type AppRequestApiSurface = WebApiSurface;
pub type AppRequestAuthMode = WebAuthMode;
pub type AppRequestEnvironment = WebEnvironment;
pub type AppRequestDeploymentMode = WebDeploymentMode;
pub type AppRequestAuthLevel = WebAuthLevel;
pub type AppRequestLoginScope = WebLoginScope;
pub type AppRequestContextProfile = WebRequestContextProfile;
