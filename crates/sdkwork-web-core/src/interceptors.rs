use crate::api_chain::{WebCallInterceptor, WebCallRuntime, WebCallStage, WebCallState};
use crate::constants::IDEMPOTENCY_KEY_HEADER;
use crate::context_injection::inject_web_request_context;
use crate::cors_policy::CorsPolicyContext;
use crate::error::{WebFrameworkError, WebFrameworkErrorKind};
use crate::extractors::header_value;
use crate::idempotency::{resolve_idempotency_fingerprint, IdempotencyBeginOutcome};
use crate::open_api_auth::resolve_open_api_request_context;
use crate::policies::{AuditFact, SecurityEvent, SecurityEventKind};
use crate::problem::redact_path_template;
use crate::rate_limit_policy::RateLimitPolicyContext;
use crate::request_context::{WebApiSurface, WebAuthMode};
use crate::request_identity::{new_request_id, ServerRequestId};
use crate::resolvers::WebRequestContextResolver;
use crate::security::SecurityPolicy;
use crate::surface::{classify_api_surface, resolve_public_path};
use crate::tenant_runtime::TenantRuntimeProfileContext;
use crate::trace::resolve_trace_context;
use async_trait::async_trait;
use axum::extract::Request;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use axum::response::Response;
use sdkwork_web_contract::RouteAuth;
use std::time::Duration;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StandardWebCallInterceptorKind {
    RequestIdentity,
    SurfaceClassification,
    Cors,
    MethodGuard,
    HeaderSecurity,
    CrossSiteRequest,
    SqlInjectionGuard,
    RequestSizeLimit,
    RateLimit,
    Idempotency,
    RequestContextResolution,
    Authentication,
    Authorization,
    TenantIsolation,
    ContextInjection,
    Logging,
    Audit,
    ResponseIdentity,
}

#[derive(Clone, Debug)]
pub struct StandardWebCallInterceptor {
    pub kind: StandardWebCallInterceptorKind,
}

impl StandardWebCallInterceptor {
    pub fn new(kind: StandardWebCallInterceptorKind) -> Self {
        Self { kind }
    }

    fn stage_for(kind: &StandardWebCallInterceptorKind) -> WebCallStage {
        match kind {
            StandardWebCallInterceptorKind::RequestIdentity => WebCallStage::RequestIdentity,
            StandardWebCallInterceptorKind::SurfaceClassification => {
                WebCallStage::SurfaceClassification
            }
            StandardWebCallInterceptorKind::Cors => WebCallStage::Cors,
            StandardWebCallInterceptorKind::MethodGuard => WebCallStage::MethodGuard,
            StandardWebCallInterceptorKind::HeaderSecurity => WebCallStage::HeaderSecurity,
            StandardWebCallInterceptorKind::CrossSiteRequest => WebCallStage::CrossSiteRequest,
            StandardWebCallInterceptorKind::SqlInjectionGuard => WebCallStage::SqlInjectionGuard,
            StandardWebCallInterceptorKind::RequestSizeLimit => WebCallStage::RequestSizeLimit,
            StandardWebCallInterceptorKind::RateLimit => WebCallStage::RateLimit,
            StandardWebCallInterceptorKind::Idempotency => WebCallStage::Idempotency,
            StandardWebCallInterceptorKind::RequestContextResolution => {
                WebCallStage::RequestContextResolution
            }
            StandardWebCallInterceptorKind::Authentication => WebCallStage::Authentication,
            StandardWebCallInterceptorKind::Authorization => WebCallStage::Authorization,
            StandardWebCallInterceptorKind::TenantIsolation => WebCallStage::TenantIsolation,
            StandardWebCallInterceptorKind::ContextInjection => WebCallStage::ContextInjection,
            StandardWebCallInterceptorKind::Logging => WebCallStage::Logging,
            StandardWebCallInterceptorKind::Audit => WebCallStage::Audit,
            StandardWebCallInterceptorKind::ResponseIdentity => WebCallStage::ResponseIdentity,
        }
    }

    fn name_for(kind: &StandardWebCallInterceptorKind) -> &'static str {
        match kind {
            StandardWebCallInterceptorKind::RequestIdentity => "request_identity",
            StandardWebCallInterceptorKind::SurfaceClassification => "surface_classification",
            StandardWebCallInterceptorKind::Cors => "cors",
            StandardWebCallInterceptorKind::MethodGuard => "method_guard",
            StandardWebCallInterceptorKind::HeaderSecurity => "header_security",
            StandardWebCallInterceptorKind::CrossSiteRequest => "cross_site_request",
            StandardWebCallInterceptorKind::SqlInjectionGuard => "sql_injection_guard",
            StandardWebCallInterceptorKind::RequestSizeLimit => "request_size_limit",
            StandardWebCallInterceptorKind::RateLimit => "rate_limit",
            StandardWebCallInterceptorKind::Idempotency => "idempotency",
            StandardWebCallInterceptorKind::RequestContextResolution => {
                "request_context_resolution"
            }
            StandardWebCallInterceptorKind::Authentication => "authentication",
            StandardWebCallInterceptorKind::Authorization => "authorization",
            StandardWebCallInterceptorKind::TenantIsolation => "tenant_isolation",
            StandardWebCallInterceptorKind::ContextInjection => "context_injection",
            StandardWebCallInterceptorKind::Logging => "logging",
            StandardWebCallInterceptorKind::Audit => "audit",
            StandardWebCallInterceptorKind::ResponseIdentity => "response_identity",
        }
    }
}

#[async_trait]
impl<R> WebCallInterceptor<R> for StandardWebCallInterceptor
where
    R: WebRequestContextResolver + Clone,
{
    fn name(&self) -> &'static str {
        Self::name_for(&self.kind)
    }

    fn stage(&self) -> WebCallStage {
        Self::stage_for(&self.kind)
    }

    async fn before(
        &self,
        state: &mut WebCallState,
        request: &mut Request,
        runtime: &WebCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        match self.kind {
            StandardWebCallInterceptorKind::RequestIdentity => {
                let request_id = new_request_id();
                request.headers_mut().insert(
                    crate::constants::REQUEST_ID_HEADER,
                    axum::http::HeaderValue::from_str(&request_id)
                        .expect("generated request ids are valid header values"),
                );
                state.request_id = Some(ServerRequestId(request_id.clone()));
                let trace = resolve_trace_context(request.headers(), &request_id);
                state.traceparent = Some(trace.traceparent);
                state.tracestate = trace.tracestate;
            }
            StandardWebCallInterceptorKind::SurfaceClassification => {
                state.api_surface = classify_api_surface(&state.path, &runtime.profile);
                state.public_path = resolve_public_path(
                    &state.method,
                    &state.path,
                    &runtime.profile,
                    runtime.route_manifest,
                );
                if let Some(manifest) = runtime.route_manifest {
                    if let Some(route) = manifest.match_route(&state.method, &state.path) {
                        if state.operation_id.is_none() {
                            state.operation_id = Some(route.operation_id.to_owned());
                        }
                        state.route_template = Some(route.path.to_owned());
                        state.rate_limit_tier = route.rate_limit_tier;
                        state.manifest_idempotent = route.idempotent;
                        state.route_auth = Some(route.auth);
                        state.forbid_credential_headers = route.forbid_credential_headers;
                    }
                }
                if state.forbid_credential_headers {
                    SecurityPolicy::reject_credential_entry_headers(request.headers())?;
                }
                if !state.forbid_credential_headers
                    && !matches!(
                        state.api_surface,
                        WebApiSurface::Unknown | WebApiSurface::GatewayApi
                    )
                {
                    runtime
                        .security_policy
                        .reject_client_identity_projection(request.headers())?;
                    crate::client_context_guard::reject_client_context_selectors(
                        &state.path,
                        request.uri().query(),
                        state.api_surface.clone(),
                    )?;
                }
                if runtime.optional_features.dynamic_tenant_runtime_profile {
                    refresh_tenant_runtime_profile(state, runtime).await?;
                }
                let user_agent = request
                    .headers()
                    .get("user-agent")
                    .and_then(|value| value.to_str().ok());
                let explicit_client = request
                    .headers()
                    .get("x-sdkwork-client-kind")
                    .and_then(|value| value.to_str().ok());
                state.client_kind = Some(crate::client_kind::infer_client_kind(
                    user_agent,
                    explicit_client,
                ));
            }
            StandardWebCallInterceptorKind::Cors => {
                if runtime.optional_features.dynamic_cors_policy {
                    let ctx = CorsPolicyContext {
                        tenant_id: state
                            .principal
                            .as_ref()
                            .map(|principal| principal.tenant_id().to_owned()),
                        environment: runtime.profile.environment.clone(),
                        api_surface: state.api_surface.clone(),
                        origin: state.origin.clone(),
                    };
                    if let Some(overlay) = runtime.dynamic_cors_policy_source.resolve(&ctx).await? {
                        state.resolved_cors = Some(overlay);
                    }
                }
                let cors = runtime.effective_cors(state);
                if !state.public_path && !is_cors_preflight(state) {
                    if let Err(error) = SecurityPolicy::validate_cors_policy(cors, request) {
                        emit_security_event(
                            runtime,
                            state,
                            SecurityEventKind::CorsDenied,
                            error.message.clone(),
                        )
                        .await?;
                        return Err(error);
                    }
                }
            }
            StandardWebCallInterceptorKind::MethodGuard => {
                runtime.security_policy.validate_method(request)?;
            }
            StandardWebCallInterceptorKind::CrossSiteRequest => {
                if is_cors_preflight(state) {
                    return Ok(());
                }
                let cors = runtime.effective_cors(state);
                if let Err(error) = SecurityPolicy::validate_cross_site_request_with_cors(
                    &runtime.security_policy.cross_site,
                    cors,
                    request,
                    state.public_path,
                ) {
                    emit_security_event(
                        runtime,
                        state,
                        SecurityEventKind::CorsDenied,
                        error.message.clone(),
                    )
                    .await?;
                    return Err(error);
                }
            }
            StandardWebCallInterceptorKind::SqlInjectionGuard => {
                runtime.security_policy.validate_sql_injection(request)?;
            }
            StandardWebCallInterceptorKind::RequestSizeLimit => {
                runtime.security_policy.validate_content_length_with_limit(
                    request,
                    runtime.effective_max_content_length(state),
                )?;
                if runtime.optional_features.json_content_type_guard {
                    runtime
                        .security_policy
                        .validate_json_content_type(request)?;
                }
                let json_inspect_limit = runtime
                    .effective_max_content_length(state)
                    .or(runtime
                        .security_policy
                        .request_size_limit
                        .max_content_length)
                    .unwrap_or(16 * 1024 * 1024);
                crate::client_context_guard::inspect_json_body_context_selectors(
                    request,
                    json_inspect_limit,
                    state.api_surface.clone(),
                )
                .await?;
            }
            StandardWebCallInterceptorKind::RateLimit => {
                // Skip rate limiting for health check and metrics endpoints.
                // These paths are critical for monitoring and orchestration systems
                // and must not be throttled. SECURITY_SPEC §4.3 / WEB_FRAMEWORK_STANDARD §6.
                let is_health_or_metrics = matches!(
                    state.path.as_str(),
                    "/healthz" | "/readyz" | "/livez" | "/metrics" | "/favicon.ico"
                );
                if !is_health_or_metrics
                    && runtime.rate_limit_globally_enabled(state)
                    && runtime.security_policy.rate_limit.pre_auth_rate_limit
                {
                    resolve_dynamic_rate_limit(state, runtime).await?;
                    apply_rate_limit(state, runtime).await?;
                }
            }
            StandardWebCallInterceptorKind::Idempotency => {
                let idempotency_required = runtime
                    .security_policy
                    .idempotency
                    .require_for_retryable_commands
                    || state.manifest_idempotent;
                if idempotency_required
                    && is_idempotent_command(&state.method)
                    && !is_cors_preflight(state)
                {
                    // Do not reserve keys for unauthenticated protected requests (auth fails later).
                    if !state.public_path && !state.credentials_present() {
                        return Ok(());
                    }
                    let Some(client_key) = state
                        .idempotency_key
                        .clone()
                        .or_else(|| header_value(request.headers(), IDEMPOTENCY_KEY_HEADER))
                    else {
                        return Err(WebFrameworkError::bad_request(
                            "Idempotency-Key header is required for this command",
                        ));
                    };
                    let store_key = state.scoped_idempotency_store_key(&client_key);
                    let fingerprint = resolve_idempotency_fingerprint(
                        &state.method,
                        &state.path,
                        state.operation_id.as_deref(),
                        request.headers(),
                        runtime
                            .security_policy
                            .idempotency
                            .require_body_hash_for_payload,
                    )?;
                    let ttl = Duration::from_secs(
                        runtime.security_policy.idempotency.retention_secs.max(1),
                    );
                    match runtime
                        .idempotency_store
                        .begin(&store_key, &fingerprint, ttl)
                        .await?
                    {
                        IdempotencyBeginOutcome::Leader => {
                            state.idempotency_key = Some(store_key);
                            state.idempotency_fingerprint = Some(fingerprint);
                            state.idempotency_leader = true;
                        }
                        IdempotencyBeginOutcome::Replay(record) => {
                            state.idempotency_replay = Some(record);
                        }
                    }
                }
            }
            StandardWebCallInterceptorKind::RequestContextResolution => {
                let headers = request.headers().clone();
                resolve_request_context(state, &headers, runtime).await?;
            }
            StandardWebCallInterceptorKind::Authentication => {
                if let Err(error) = require_authenticated_context(state) {
                    emit_security_event(
                        runtime,
                        state,
                        SecurityEventKind::AuthenticationFailed,
                        error.message.clone(),
                    )
                    .await?;
                    return Err(error);
                }
                if runtime.optional_features.dynamic_tenant_runtime_profile {
                    refresh_tenant_runtime_profile(state, runtime).await?;
                    try_acquire_tenant_concurrency(state, runtime).await?;
                }
            }
            StandardWebCallInterceptorKind::Authorization => {
                if !state.public_path && !is_cors_preflight(state) {
                    let ctx = state.to_context()?;
                    if let Err(error) = runtime
                        .authorization
                        .authorize(&ctx, state.operation_id.as_deref())
                    {
                        emit_security_event(
                            runtime,
                            state,
                            SecurityEventKind::AuthorizationDenied,
                            error.message.clone(),
                        )
                        .await?;
                        return Err(error);
                    }
                    if runtime.rate_limit_globally_enabled(state)
                        && runtime.security_policy.rate_limit.tenant_limit_after_auth
                    {
                        resolve_dynamic_rate_limit(state, runtime).await?;
                        apply_rate_limit(state, runtime).await?;
                    }
                }
            }
            StandardWebCallInterceptorKind::TenantIsolation => {
                if !state.public_path && !is_cors_preflight(state) {
                    let ctx = state.to_context()?;
                    if let Some(manifest) = runtime.route_manifest {
                        if let Some(route) = manifest.match_route(&state.method, &state.path) {
                            if let Err(error) =
                                crate::path_resource_guard::verify_path_resource_ids_match_principal(
                                    &ctx,
                                    route.path,
                                    route.required_permission,
                                )
                            {
                                emit_security_event(
                                    runtime,
                                    state,
                                    SecurityEventKind::TenantIsolationDenied,
                                    error.message.clone(),
                                )
                                .await?;
                                return Err(error);
                            }
                        }
                    }
                    if let Err(error) = runtime
                        .tenant_isolation
                        .enforce(&ctx, state.operation_id.as_deref())
                    {
                        emit_security_event(
                            runtime,
                            state,
                            SecurityEventKind::TenantIsolationDenied,
                            error.message.clone(),
                        )
                        .await?;
                        return Err(error);
                    }
                }
            }
            StandardWebCallInterceptorKind::ContextInjection => {
                let context = state.to_context()?;
                let request_id = context.request_id.clone();
                inject_web_request_context(request, request_id, context, &runtime.domain_injectors);
            }
            StandardWebCallInterceptorKind::Logging => {
                state.accepted_at = Some(std::time::Instant::now());
                let route_template = redact_path_template(&state.path);
                tracing::info!(
                    request_id = ?state.request_id_value(),
                    api_surface = ?state.api_surface,
                    method = %state.method,
                    route_template = %route_template,
                    operation_id = ?state.operation_id,
                    public_path = state.public_path,
                    "web request accepted"
                );
            }
            StandardWebCallInterceptorKind::Audit => {}
            StandardWebCallInterceptorKind::HeaderSecurity
            | StandardWebCallInterceptorKind::ResponseIdentity => {}
        }
        Ok(())
    }

    async fn after(
        &self,
        state: &WebCallState,
        response: &mut Response,
        runtime: &WebCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        match self.kind {
            StandardWebCallInterceptorKind::Idempotency => {
                if state.idempotency_leader {
                    if let (Some(key), Some(fingerprint)) =
                        (&state.idempotency_key, &state.idempotency_fingerprint)
                    {
                        let ttl = Duration::from_secs(
                            runtime.security_policy.idempotency.retention_secs.max(1),
                        );
                        let max_bytes = runtime
                            .security_policy
                            .idempotency
                            .max_cached_response_bytes;
                        let (parts, body) =
                            std::mem::replace(response, Response::new(axum::body::Body::empty()))
                                .into_parts();
                        let bytes = axum::body::to_bytes(body, max_bytes as usize)
                            .await
                            .map_err(|_| {
                                WebFrameworkError::payload_too_large(
                                    "idempotency response exceeds cache limit",
                                )
                            })?;
                        let content_type = parts
                            .headers
                            .get(axum::http::header::CONTENT_TYPE)
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_owned);
                        let status_code = parts.status.as_u16();
                        let store_result =
                            if status_code >= 500 || !(200..300).contains(&status_code) {
                                runtime.idempotency_store.release(key, fingerprint).await
                            } else {
                                runtime
                                    .idempotency_store
                                    .complete(
                                        key,
                                        fingerprint,
                                        crate::idempotency::IdempotencyResponseRecord {
                                            status_code,
                                            body: bytes.to_vec(),
                                            content_type,
                                        },
                                        ttl,
                                    )
                                    .await
                            };
                        if let Err(error) = store_result {
                            tracing::warn!(
                                request_id = ?state.request_id_value(),
                                status_code,
                                error = ?error,
                                "idempotency store update failed after handler completed"
                            );
                        }
                        *response = Response::from_parts(parts, axum::body::Body::from(bytes));
                    }
                }
            }
            StandardWebCallInterceptorKind::ResponseIdentity => {
                let trace_id = state
                    .traceparent
                    .as_deref()
                    .and_then(crate::trace::trace_id_from_traceparent)
                    .map(str::to_owned)
                    .or_else(|| state.request_id_value().map(str::to_owned));
                if let Some(trace_id) = trace_id {
                    if let Ok(value) = axum::http::HeaderValue::from_str(&trace_id) {
                        response.headers_mut().insert(
                            HeaderName::from_static(
                                crate::constants::SDKWORK_TRACE_ID_HEADER_LOWER,
                            ),
                            value,
                        );
                    }
                }
                if let Some(traceparent) = &state.traceparent {
                    if let Ok(value) = axum::http::HeaderValue::from_str(traceparent) {
                        response
                            .headers_mut()
                            .insert(crate::trace::TRACEPARENT_HEADER, value);
                    }
                }
                if let Some(tracestate) = &state.tracestate {
                    if let Ok(value) = axum::http::HeaderValue::from_str(tracestate) {
                        response
                            .headers_mut()
                            .insert(crate::trace::TRACESTATE_HEADER, value);
                    }
                }
                if let Err(error) =
                    crate::problem::enrich_problem_response(state.problem_correlation(), response)
                        .await
                {
                    tracing::warn!(
                        request_id = ?state.request_id_value(),
                        operation_id = ?state.operation_id,
                        error = %error,
                        "problem response enrichment failed"
                    );
                }
            }
            StandardWebCallInterceptorKind::HeaderSecurity => {
                runtime.security_policy.apply_response_headers(response);
            }
            StandardWebCallInterceptorKind::Cors => {
                SecurityPolicy::apply_cors_policy_headers_from_origin(
                    runtime.effective_cors(state),
                    state.origin.as_deref(),
                    response,
                );
            }
            StandardWebCallInterceptorKind::Audit => {
                let fact = AuditFact {
                    request_id: state.request_id_value().unwrap_or("unknown").to_owned(),
                    tenant_id: state
                        .principal
                        .as_ref()
                        .map(|principal| principal.tenant_id().to_owned()),
                    user_id: state
                        .principal
                        .as_ref()
                        .map(|principal| principal.user_id().to_owned()),
                    api_surface: state.api_surface.clone(),
                    path: redact_path_template(&state.path),
                    method: state.method.clone(),
                    operation_id: state.operation_id.clone(),
                    status_code: Some(response.status().as_u16()),
                    duration_ms: state
                        .accepted_at
                        .map(|started| started.elapsed().as_millis() as u64),
                };
                runtime.audit_emitter.emit(fact).await?;
            }
            _ => {}
        }
        Ok(())
    }
}

async fn resolve_request_context<R>(
    state: &mut WebCallState,
    headers: &HeaderMap,
    runtime: &WebCallRuntime<R>,
) -> Result<(), WebFrameworkError>
where
    R: WebRequestContextResolver + Clone,
{
    if is_cors_preflight(state) {
        state.auth_mode = WebAuthMode::Public;
        state.principal = None;
        return Ok(());
    }

    if matches!(state.api_surface, WebApiSurface::Unknown) && state.public_path {
        state.auth_mode = WebAuthMode::Public;
        state.principal = None;
        return Ok(());
    }

    match state.api_surface {
        WebApiSurface::OpenApi => {
            if state.public_path
                && skips_access_token_for_tenant_isolation(
                    state.route_auth,
                    state.forbid_credential_headers,
                )
            {
                return finish_optional_public_access_context(state, runtime).await;
            }
            if !state.public_path {
                if let (Some(auth_token), Some(access_token)) = (
                    state.credentials.auth_token.as_deref(),
                    state.credentials.access_token.as_deref(),
                ) {
                    state.principal = Some(
                        runtime
                            .resolver
                            .resolve_dual_token(auth_token, access_token)
                            .await?,
                    );
                    state.auth_mode = WebAuthMode::DualToken;
                    return Ok(());
                }
            }
            let route_auth = state
                .route_auth
                .or(Some(sdkwork_web_contract::RouteAuth::OpenApiFlexible));
            let (auth_mode, principal) = resolve_open_api_request_context(
                &state.credentials,
                headers,
                route_auth,
                runtime.open_api_scheme_detector.as_ref(),
                &runtime.resolver,
            )
            .await?;
            state.auth_mode = auth_mode;
            state.principal = Some(principal);
        }
        WebApiSurface::AppApi | WebApiSurface::BackendApi | WebApiSurface::GatewayApi => {
            if state.public_path
                && skips_access_token_for_tenant_isolation(
                    state.route_auth,
                    state.forbid_credential_headers,
                )
            {
                return finish_optional_public_access_context(state, runtime).await;
            }
            // AgentToken routes resolve via api-key path using X-SDKWork-Agent-Token (C8-C9).
            // The agent token provides both authentication and tenant isolation without
            // requiring Access-Token or Authorization: Bearer JWTs.
            if state.route_auth == Some(RouteAuth::AgentToken) {
                let agent_token = state.credentials.agent_token.as_deref().ok_or_else(|| {
                    WebFrameworkError::missing_credentials(
                        "agent routes require X-SDKWork-Agent-Token header",
                    )
                })?;
                state.principal = Some(runtime.resolver.resolve_api_key(agent_token).await?);
                state.auth_mode = WebAuthMode::ApiKey;
                return Ok(());
            }
            let access_token = state.credentials.access_token.as_deref().ok_or_else(|| {
                WebFrameworkError::missing_credentials(
                    "non-open-api requests require Access-Token JWT for tenant isolation",
                )
            })?;
            if state.public_path {
                state.auth_mode = WebAuthMode::Public;
                state.principal = Some(runtime.resolver.resolve_access_token(access_token).await?);
            } else {
                let auth_token = state.credentials.auth_token.as_deref().ok_or_else(|| {
                    WebFrameworkError::missing_credentials(
                        "app-api and backend-api requests require Authorization: Bearer <auth_token>",
                    )
                })?;
                state.principal = Some(
                    runtime
                        .resolver
                        .resolve_dual_token(auth_token, access_token)
                        .await?,
                );
                state.auth_mode = WebAuthMode::DualToken;
            }
        }
        WebApiSurface::Unknown => {
            return Err(WebFrameworkError::missing_credentials(
                "requests on unclassified API surfaces require explicit public_path registration",
            ));
        }
    }
    Ok(())
}

fn is_cors_preflight(state: &WebCallState) -> bool {
    state.method.eq_ignore_ascii_case("OPTIONS")
}

fn skips_access_token_for_tenant_isolation(
    route_auth: Option<RouteAuth>,
    forbid_credential_headers: bool,
) -> bool {
    if forbid_credential_headers || route_auth == Some(RouteAuth::RefreshToken) {
        return false;
    }
    route_auth.is_none_or(|auth| auth.skips_credential_resolution())
}

/// Public / optional Access-Token routes: missing token is allowed; malformed tokens are rejected.
async fn finish_optional_public_access_context<R>(
    state: &mut WebCallState,
    runtime: &WebCallRuntime<R>,
) -> Result<(), WebFrameworkError>
where
    R: WebRequestContextResolver + Clone,
{
    state.auth_mode = WebAuthMode::Public;
    if let Some(access_token) = state.credentials.access_token.as_deref() {
        state.principal = Some(runtime.resolver.resolve_access_token(access_token).await?);
    } else {
        state.principal = None;
    }
    Ok(())
}

fn is_idempotent_command(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
}

fn require_authenticated_context(state: &WebCallState) -> Result<(), WebFrameworkError> {
    if state.public_path || is_cors_preflight(state) {
        return Ok(());
    }
    if state.principal.is_none() {
        return Err(WebFrameworkError::missing_credentials(
            "protected routes require authenticated credentials",
        ));
    }
    Ok(())
}

async fn resolve_dynamic_rate_limit<R>(
    state: &mut WebCallState,
    runtime: &WebCallRuntime<R>,
) -> Result<(), WebFrameworkError>
where
    R: WebRequestContextResolver + Clone,
{
    if !runtime.optional_features.dynamic_rate_limit_policy {
        return Ok(());
    }
    let ctx = RateLimitPolicyContext {
        tenant_id: state
            .principal
            .as_ref()
            .map(|principal| principal.tenant_id().to_owned()),
        environment: runtime.profile.environment.clone(),
        api_surface: state.api_surface.clone(),
        rate_limit_tier: state.rate_limit_tier,
        operation_id: state.operation_id.clone(),
    };
    if let Some(overlay) = runtime
        .dynamic_rate_limit_policy_source
        .resolve(&ctx)
        .await?
    {
        state.resolved_rate_limit = Some(overlay);
    }
    Ok(())
}

async fn apply_rate_limit<R>(
    state: &WebCallState,
    runtime: &WebCallRuntime<R>,
) -> Result<(), WebFrameworkError>
where
    R: WebRequestContextResolver + Clone,
{
    if !runtime.rate_limit_globally_enabled(state) {
        return Ok(());
    }
    let resolved = state.resolved_rate_limit.unwrap_or_else(|| {
        runtime
            .rate_limit_resolver
            .resolve(state, &runtime.security_policy.rate_limit)
    });
    if resolved.max_requests == 0 {
        return Ok(());
    }
    if let Err(error) = runtime
        .rate_limit_store
        .check_and_record(
            &state.rate_limit_key(),
            resolved.max_requests,
            Duration::from_secs(resolved.window_secs.max(1)),
        )
        .await
    {
        if error.kind == WebFrameworkErrorKind::RateLimitExceeded {
            emit_security_event(
                runtime,
                state,
                SecurityEventKind::RateLimitExceeded,
                error.message.clone(),
            )
            .await?;
        }
        return Err(error);
    }
    Ok(())
}

async fn refresh_tenant_runtime_profile<R>(
    state: &mut WebCallState,
    runtime: &WebCallRuntime<R>,
) -> Result<(), WebFrameworkError>
where
    R: WebRequestContextResolver + Clone,
{
    let profile_ctx = TenantRuntimeProfileContext {
        tenant_id: state
            .principal
            .as_ref()
            .map(|principal| principal.tenant_id().to_owned()),
        environment: runtime.profile.environment.clone(),
        api_surface: state.api_surface.clone(),
    };
    if let Some(profile) = runtime
        .dynamic_tenant_runtime_profile_source
        .resolve(&profile_ctx)
        .await?
    {
        state.tenant_runtime_profile = Some(profile);
    }
    Ok(())
}

async fn try_acquire_tenant_concurrency<R>(
    state: &mut WebCallState,
    runtime: &WebCallRuntime<R>,
) -> Result<(), WebFrameworkError>
where
    R: WebRequestContextResolver + Clone,
{
    let Some(limit) = state
        .tenant_runtime_profile
        .as_ref()
        .and_then(|profile| profile.max_concurrent_requests)
    else {
        return Ok(());
    };
    let Some(key) = state.concurrent_admission_scope_key() else {
        return Ok(());
    };
    runtime
        .concurrent_admission_store
        .try_acquire(&key, limit)
        .await?;
    state.concurrent_admission_key = Some(key);
    Ok(())
}

async fn emit_security_event<R>(
    runtime: &WebCallRuntime<R>,
    state: &WebCallState,
    kind: SecurityEventKind,
    detail: String,
) -> Result<(), WebFrameworkError>
where
    R: WebRequestContextResolver + Clone,
{
    let event = SecurityEvent {
        kind,
        request_id: state.request_id_value().map(str::to_owned),
        tenant_id: state
            .principal
            .as_ref()
            .map(|principal| principal.tenant_id().to_owned()),
        path: redact_path_template(&state.path),
        method: state.method.clone(),
        api_surface: state.api_surface.clone(),
        origin: state.origin.clone(),
        detail,
    };
    // fail-closed：安全事件 emit 失败时返回 503 DependencyUnavailable，
    // 而非静默丢弃。SECURITY_SPEC §5.1 / WEB_FRAMEWORK_STANDARD §9。
    runtime.security_event_emitter.emit(event).await
}
