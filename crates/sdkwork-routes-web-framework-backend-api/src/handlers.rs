//! Thin HTTP adapters for framework control-plane backend-api (`WEB_BACKEND_SPEC.md` §2).

use crate::dto::{
    ListQuery, RegisterControlNodeRequest, UpsertCorsPolicyRequest, UpsertRateLimitPolicyRequest,
    UpsertTenantRuntimeProfileRequest,
};
use crate::response::{
    created_json, finish_api_json, finish_api_response, no_content, ok_json, success_json,
};
use crate::services::WebFrameworkAdminService;
use crate::state::WebFrameworkAdminState;
use crate::tenant_scope::{
    require_control_plane, require_tenant_admin, require_upsert_tenant_id,
    resolve_audit_event_list_scope, resolve_list_tenant_id, resolve_security_event_list_scope,
};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::Json;
use sdkwork_web_core::WebRequestContext;

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_secs() as i64
}

pub async fn list_cors_policies(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Query(query): Query<ListQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_tenant_admin(&ctx)?;
            let tenant_id = resolve_list_tenant_id(&ctx, query.tenant_id.as_deref())?;
            let limit = crate::services::validation::validate_list_limit(query.limit)?;
            ok_json(
                state
                    .service
                    .list_cors_policies(&tenant_id, query.environment, limit)
                    .await?,
            )
        }
        .await,
    )
}

pub async fn upsert_cors_policy(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Json(body): Json<UpsertCorsPolicyRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_tenant_admin(&ctx)?;
            require_upsert_tenant_id(&ctx, &body.tenant_id)?;
            ok_json(state.service.upsert_cors_policy(body).await?)
        }
        .await,
    )
}

pub async fn list_rate_limit_policies(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Query(query): Query<ListQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_tenant_admin(&ctx)?;
            let tenant_id = resolve_list_tenant_id(&ctx, query.tenant_id.as_deref())?;
            let limit = crate::services::validation::validate_list_limit(query.limit)?;
            ok_json(
                state
                    .service
                    .list_rate_limit_policies(&tenant_id, query.environment, limit)
                    .await?,
            )
        }
        .await,
    )
}

pub async fn upsert_rate_limit_policy(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Json(body): Json<UpsertRateLimitPolicyRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_tenant_admin(&ctx)?;
            require_upsert_tenant_id(&ctx, &body.tenant_id)?;
            ok_json(state.service.upsert_rate_limit_policy(body).await?)
        }
        .await,
    )
}

pub async fn list_tenant_runtime_profiles(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Query(query): Query<ListQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_tenant_admin(&ctx)?;
            let tenant_id = resolve_list_tenant_id(&ctx, query.tenant_id.as_deref())?;
            let limit = crate::services::validation::validate_list_limit(query.limit)?;
            ok_json(
                state
                    .service
                    .list_tenant_runtime_profiles(&tenant_id, query.environment, limit)
                    .await?,
            )
        }
        .await,
    )
}

pub async fn upsert_tenant_runtime_profile(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Json(body): Json<UpsertTenantRuntimeProfileRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_tenant_admin(&ctx)?;
            require_upsert_tenant_id(&ctx, &body.tenant_id)?;
            ok_json(state.service.upsert_tenant_runtime_profile(body).await?)
        }
        .await,
    )
}

pub async fn list_security_events(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Query(query): Query<ListQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let scope = resolve_security_event_list_scope(&ctx, query.tenant_id.as_deref())?;
            let limit = crate::services::validation::validate_list_limit(query.limit)?;
            ok_json(state.service.list_security_events(scope, limit).await?)
        }
        .await,
    )
}

pub async fn list_audit_events(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Query(query): Query<ListQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let scope = resolve_audit_event_list_scope(&ctx, query.tenant_id.as_deref())?;
            let limit = crate::services::validation::validate_list_limit(query.limit)?;
            ok_json(state.service.list_audit_events(scope, limit).await?)
        }
        .await,
    )
}

pub async fn list_control_nodes(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Query(query): Query<ListQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_control_plane(&ctx)?;
            let limit = crate::services::validation::validate_list_limit(query.limit)?;
            ok_json(
                state
                    .service
                    .list_control_nodes(query.environment, limit)
                    .await?,
            )
        }
        .await,
    )
}

pub async fn register_control_node(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Json(body): Json<RegisterControlNodeRequest>,
) -> Response {
    finish_api_response(
        &ctx,
        async {
            require_control_plane(&ctx)?;
            let outcome = state
                .service
                .register_control_node(body, now_epoch())
                .await?;
            if outcome.created {
                created_json(&ctx, outcome.record)
            } else {
                success_json(&ctx, outcome.record)
            }
        }
        .await,
    )
}

pub async fn heartbeat_control_node(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Path(node_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_control_plane(&ctx)?;
            crate::services::validation::validate_control_node_id(&node_id)?;
            ok_json(
                state
                    .service
                    .heartbeat_control_node(&node_id, now_epoch())
                    .await?,
            )
        }
        .await,
    )
}

pub async fn delete_control_node(
    ctx: WebRequestContext,
    State(state): State<WebFrameworkAdminState>,
    Path(node_id): Path<String>,
) -> Response {
    finish_api_response(
        &ctx,
        async {
            require_control_plane(&ctx)?;
            crate::services::validation::validate_control_node_id(&node_id)?;
            state.service.delete_control_node(&node_id).await?;
            no_content(&ctx)
        }
        .await,
    )
}

pub async fn runtime_defaults_snapshot(ctx: WebRequestContext) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_tenant_admin(&ctx)?;
            ok_json(WebFrameworkAdminService::runtime_defaults_snapshot())
        }
        .await,
    )
}

pub async fn optional_features_snapshot(ctx: WebRequestContext) -> Response {
    finish_api_json(
        &ctx,
        async {
            require_tenant_admin(&ctx)?;
            ok_json(WebFrameworkAdminService::optional_features_snapshot())
        }
        .await,
    )
}
