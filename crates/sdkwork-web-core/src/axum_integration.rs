use crate::error::{WebFrameworkError, WebFrameworkRejection};
use crate::request_context::{WebApiSurface, WebAuthMode, WebRequestContext, WebRequestPrincipal};
use crate::tenant_app_context::TenantAppContext;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

impl<S> FromRequestParts<S> for WebRequestContext
where
    S: Send + Sync,
{
    type Rejection = WebFrameworkRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<WebRequestContext>()
            .cloned()
            .ok_or_else(|| {
                WebFrameworkRejection::new(WebFrameworkError::context_not_injected(), parts)
            })
    }
}

/// Protected handlers: principal must be present after the HTTP pipeline.
pub struct RequirePrincipal(pub WebRequestPrincipal);

/// Protected handlers: tenant + app scoped service view.
pub struct RequireTenantApp(pub TenantAppContext);

/// Requires dual-token auth mode on protected routes.
pub struct RequireDualToken(pub WebRequestContext);

/// Requires app-api surface classification.
pub struct RequireAppApi(pub WebRequestContext);

/// Requires open-api surface with API key or OAuth bearer authentication.
pub struct RequireOpenApi(pub WebRequestContext);

impl<S> FromRequestParts<S> for RequirePrincipal
where
    S: Send + Sync,
{
    type Rejection = WebFrameworkRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ctx = WebRequestContext::from_request_parts(parts, state).await?;
        let principal = ctx
            .require_principal()
            .map_err(|error| WebFrameworkRejection::new(error, parts))?
            .clone();
        Ok(Self(principal))
    }
}

impl<S> FromRequestParts<S> for RequireTenantApp
where
    S: Send + Sync,
{
    type Rejection = WebFrameworkRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ctx = WebRequestContext::from_request_parts(parts, state).await?;
        let tenant_app = TenantAppContext::try_from_request_context(&ctx)
            .map_err(|error| WebFrameworkRejection::new(error, parts))?;
        Ok(Self(tenant_app))
    }
}

impl<S> FromRequestParts<S> for RequireDualToken
where
    S: Send + Sync,
{
    type Rejection = WebFrameworkRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ctx = WebRequestContext::from_request_parts(parts, state).await?;
        if !matches!(ctx.auth_mode, WebAuthMode::DualToken) {
            return Err(WebFrameworkRejection::new(
                WebFrameworkError::missing_credentials(
                    "handler requires dual-token authenticated context",
                ),
                parts,
            ));
        }
        ctx.require_principal()
            .map_err(|error| WebFrameworkRejection::new(error, parts))?;
        Ok(Self(ctx))
    }
}

impl<S> FromRequestParts<S> for RequireAppApi
where
    S: Send + Sync,
{
    type Rejection = WebFrameworkRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ctx = WebRequestContext::from_request_parts(parts, state).await?;
        if !matches!(ctx.api_surface, WebApiSurface::AppApi) {
            return Err(WebFrameworkRejection::new(
                WebFrameworkError::forbidden("handler requires app-api surface"),
                parts,
            ));
        }
        Ok(Self(ctx))
    }
}

impl<S> FromRequestParts<S> for RequireOpenApi
where
    S: Send + Sync,
{
    type Rejection = WebFrameworkRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ctx = WebRequestContext::from_request_parts(parts, state).await?;
        if !matches!(ctx.api_surface, WebApiSurface::OpenApi) {
            return Err(WebFrameworkRejection::new(
                WebFrameworkError::forbidden("handler requires open-api surface"),
                parts,
            ));
        }
        if !matches!(ctx.auth_mode, WebAuthMode::ApiKey | WebAuthMode::OAuth) {
            return Err(WebFrameworkRejection::new(
                WebFrameworkError::missing_credentials(
                    "handler requires open-api API key or OAuth bearer authentication",
                ),
                parts,
            ));
        }
        ctx.require_principal()
            .map_err(|error| WebFrameworkRejection::new(error, parts))?;
        Ok(Self(ctx))
    }
}
