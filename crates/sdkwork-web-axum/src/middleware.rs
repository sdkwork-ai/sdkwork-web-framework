use crate::correlation::OwnedProblemCorrelation;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::HeaderValue;
use axum::middleware::{from_fn, from_fn_with_state, Next};
use axum::response::Response;
use axum::Router;
use futures_util::FutureExt;
use sdkwork_web_core::{
    http_request_labels_from_state, idempotency_replay_response, inject_web_request_context,
    new_request_id, problem_response, resolve_trace_context, trace_id_from_traceparent,
    AuditEmitter, AuthorizationPolicy, DomainContextInjector, HttpMetricsRegistry,
    HttpRouteManifest, IdempotencyStore, RateLimitPolicyResolver, RateLimitStore,
    SecurityEventEmitter, SecurityPolicy, ServerRequestId, TenantIsolationPolicy, WebApiSurface,
    WebAuthMode, WebCallInterceptorChain, WebCallRuntime, WebCallState, WebFrameworkError,
    WebRequestContext, WebRequestContextProfile, WebRequestContextResolver, WebTransportFacts,
    REQUEST_ID_HEADER,
};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct WebFrameworkLayer<R>
where
    R: WebRequestContextResolver + Clone,
{
    runtime: WebCallRuntime<R>,
    call_chain: WebCallInterceptorChain<R>,
}

impl<R> WebFrameworkLayer<R>
where
    R: WebRequestContextResolver + Clone,
{
    pub fn new(resolver: R) -> Self {
        Self {
            runtime: WebCallRuntime::new(resolver),
            call_chain: WebCallInterceptorChain::standard(),
        }
    }

    pub fn with_profile(mut self, profile: WebRequestContextProfile) -> Self {
        self.runtime = self.runtime.with_profile(profile);
        self
    }

    pub fn with_security_policy(mut self, security_policy: SecurityPolicy) -> Self {
        self.runtime = self.runtime.with_security_policy(security_policy);
        self
    }

    pub fn with_call_chain(mut self, call_chain: WebCallInterceptorChain<R>) -> Self {
        self.call_chain = call_chain;
        self
    }

    pub fn with_domain_injector(mut self, injector: Arc<dyn DomainContextInjector>) -> Self {
        self.runtime = self.runtime.with_domain_injector(injector);
        self
    }

    pub fn with_authorization_policy(mut self, policy: Arc<dyn AuthorizationPolicy>) -> Self {
        self.runtime = self.runtime.with_authorization_policy(policy);
        self
    }

    pub fn with_tenant_isolation_policy(mut self, policy: Arc<dyn TenantIsolationPolicy>) -> Self {
        self.runtime = self.runtime.with_tenant_isolation_policy(policy);
        self
    }

    pub fn with_rate_limit_store(mut self, store: Arc<dyn RateLimitStore>) -> Self {
        self.runtime = self.runtime.with_rate_limit_store(store);
        self
    }

    pub fn with_idempotency_store(mut self, store: Arc<dyn IdempotencyStore>) -> Self {
        self.runtime = self.runtime.with_idempotency_store(store);
        self
    }

    pub fn with_concurrent_admission_store(
        mut self,
        store: Arc<dyn sdkwork_web_core::ConcurrentAdmissionStore>,
    ) -> Self {
        self.runtime = self.runtime.with_concurrent_admission_store(store);
        self
    }

    pub fn with_audit_emitter(mut self, emitter: Arc<dyn AuditEmitter>) -> Self {
        self.runtime = self.runtime.with_audit_emitter(emitter);
        self
    }

    pub fn with_security_event_emitter(mut self, emitter: Arc<dyn SecurityEventEmitter>) -> Self {
        self.runtime = self.runtime.with_security_event_emitter(emitter);
        self
    }

    pub fn with_rate_limit_resolver(mut self, resolver: Arc<dyn RateLimitPolicyResolver>) -> Self {
        self.runtime = self.runtime.with_rate_limit_resolver(resolver);
        self
    }

    pub fn with_dynamic_cors_policy_source(
        mut self,
        source: Arc<dyn sdkwork_web_core::DynamicCorsPolicySource>,
    ) -> Self {
        self.runtime = self.runtime.with_dynamic_cors_policy_source(source);
        self
    }

    pub fn with_dynamic_rate_limit_policy_source(
        mut self,
        source: Arc<dyn sdkwork_web_core::DynamicRateLimitPolicySource>,
    ) -> Self {
        self.runtime = self.runtime.with_dynamic_rate_limit_policy_source(source);
        self
    }

    pub fn with_dynamic_tenant_runtime_profile_source(
        mut self,
        source: Arc<dyn sdkwork_web_core::DynamicTenantRuntimeProfileSource>,
    ) -> Self {
        self.runtime = self
            .runtime
            .with_dynamic_tenant_runtime_profile_source(source);
        self
    }

    pub fn with_optional_features(
        mut self,
        features: sdkwork_web_core::WebFrameworkOptionalFeatures,
    ) -> Self {
        self.runtime = self.runtime.with_optional_features(features);
        self
    }

    pub fn with_route_manifest(mut self, manifest: HttpRouteManifest) -> Self {
        self.runtime = self.runtime.with_route_manifest(manifest);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<HttpMetricsRegistry>) -> Self {
        self.runtime = self.runtime.with_metrics(metrics);
        self
    }

    pub fn with_request_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.runtime = self.runtime.with_request_timeout(timeout);
        self
    }

    pub fn with_open_api_scheme_detector(
        mut self,
        detector: sdkwork_web_core::DynOpenApiCredentialSchemeDetector,
    ) -> Self {
        self.runtime = self.runtime.with_open_api_scheme_detector(detector);
        self
    }

    pub fn runtime(&self) -> &WebCallRuntime<R> {
        &self.runtime
    }
}

pub type AppRequestContextLayer<R> = WebFrameworkLayer<R>;

/// Lightweight request-id injection for routers **without** the full 18-stage pipeline.
/// Do not stack this **outside** `with_web_request_context` — the full pipeline already
/// assigns request identity at stage 1.
pub fn with_server_request_identity(router: Router) -> Router {
    router.layer(from_fn(server_request_identity_middleware))
}

pub fn with_web_request_context<R>(router: Router, layer: WebFrameworkLayer<R>) -> Router
where
    R: WebRequestContextResolver + Clone,
{
    let max_body = layer
        .runtime
        .security_policy
        .request_size_limit
        .max_content_length
        .unwrap_or(16 * 1024 * 1024);
    router
        .layer(DefaultBodyLimit::max(max_body as usize))
        .layer(from_fn_with_state(
            layer,
            web_request_context_middleware::<R>,
        ))
}

fn anonymous_context_from_request(request: &Request, request_id: &str) -> WebRequestContext {
    let user_agent = request
        .headers()
        .get("user-agent")
        .and_then(|value| value.to_str().ok());
    let explicit_client = request
        .headers()
        .get("x-sdkwork-client-kind")
        .and_then(|value| value.to_str().ok());
    let trace = resolve_trace_context(request.headers(), request_id);
    WebRequestContext {
        request_id: ServerRequestId(request_id.to_owned()),
        api_surface: WebApiSurface::Unknown,
        auth_mode: WebAuthMode::Public,
        principal: None,
        transport: WebTransportFacts {
            path: request.uri().path().to_owned(),
            method: request.method().as_str().to_owned(),
            auth_token_present: false,
            access_token_present: false,
            api_key_present: false,
            oauth_bearer_present: false,
            agent_token_present: false,
        },
        locale: None,
        client_kind: Some(sdkwork_web_core::client_kind::infer_client_kind(
            user_agent,
            explicit_client,
        )),
        operation: None,
        trace_id: trace_id_from_traceparent(&trace.traceparent).map(str::to_string),
    }
}

async fn run_handler_with_timeout<R>(
    runtime: &WebCallRuntime<R>,
    request: Request,
    next: Next,
) -> Response
where
    R: WebRequestContextResolver + Clone,
{
    let Some(timeout) = runtime.request_timeout() else {
        return next.run(request).await;
    };
    let correlation = OwnedProblemCorrelation::from_request(&request);
    match tokio::time::timeout(timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => problem_response(
            &WebFrameworkError::request_timeout("request timed out"),
            correlation.as_correlation(),
        ),
    }
}

async fn server_request_identity_middleware(mut request: Request, next: Next) -> Response {
    if request.extensions().get::<WebRequestContext>().is_some() {
        return next.run(request).await;
    }

    let request_id = new_request_id();
    request.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&request_id).expect("valid request id"),
    );
    let context = anonymous_context_from_request(&request, &request_id);
    inject_web_request_context(
        &mut request,
        ServerRequestId(request_id.clone()),
        context,
        &[],
    );
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&request_id).expect("valid request id"),
    );
    response
}

async fn release_idempotency_leader<R>(runtime: &WebCallRuntime<R>, state: &WebCallState)
where
    R: WebRequestContextResolver + Clone,
{
    if !state.idempotency_leader {
        return;
    }
    if let (Some(key), Some(fingerprint)) = (&state.idempotency_key, &state.idempotency_fingerprint)
    {
        let _ = runtime.idempotency_store.release(key, fingerprint).await;
    }
}

async fn release_concurrent_admission<R>(runtime: &WebCallRuntime<R>, state: &WebCallState)
where
    R: WebRequestContextResolver + Clone,
{
    if let Some(key) = &state.concurrent_admission_key {
        let _ = runtime.concurrent_admission_store.release(key).await;
    }
}

struct IdempotencyReleaseGuard {
    store: Arc<dyn IdempotencyStore>,
    key: String,
    fingerprint: String,
    released: Arc<AtomicBool>,
}

impl IdempotencyReleaseGuard {
    fn new(store: Arc<dyn IdempotencyStore>, key: String, fingerprint: String) -> Self {
        Self {
            store,
            key,
            fingerprint,
            released: Arc::new(AtomicBool::new(false)),
        }
    }

    fn mark_released(&self) {
        self.released.store(true, Ordering::SeqCst);
    }
}

impl Drop for IdempotencyReleaseGuard {
    fn drop(&mut self) {
        if self.released.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let store = self.store.clone();
            let key = self.key.clone();
            let fingerprint = self.fingerprint.clone();
            handle.spawn(async move {
                let _ = store.release(&key, &fingerprint).await;
            });
        }
    }
}

async fn finalize_response<R>(
    layer: &WebFrameworkLayer<R>,
    state: &WebCallState,
    mut response: Response,
) -> Response
where
    R: WebRequestContextResolver + Clone,
{
    if let Err(error) = layer
        .call_chain
        .after(state, &mut response, &layer.runtime)
        .await
    {
        return problem_response(&error, state.problem_correlation());
    }
    if let Some(metrics) = layer.runtime.metrics() {
        if HttpMetricsRegistry::should_record_path(&state.path) {
            let labels = http_request_labels_from_state(
                state,
                &metrics.dimensions(),
                response.status().as_u16(),
            );
            metrics.record_request(&labels);
        }
    }
    release_concurrent_admission(&layer.runtime, state).await;
    response
}

async fn web_request_context_middleware<R>(
    State(layer): State<WebFrameworkLayer<R>>,
    mut request: Request,
    next: Next,
) -> Response
where
    R: WebRequestContextResolver + Clone,
{
    let mut state = WebCallState::from_request(&request);
    if let Err(error) = layer
        .call_chain
        .before(&mut state, &mut request, &layer.runtime)
        .await
    {
        release_concurrent_admission(&layer.runtime, &state).await;
        release_idempotency_leader(&layer.runtime, &state).await;
        let mut response = problem_response(&error, state.problem_correlation());
        // 即使 before 失败也执行 after，确保 Audit/ResponseIdentity/HeaderSecurity
        // 为失败请求留痕。SECURITY_SPEC §5.1 审计完整性。
        let _ = layer
            .call_chain
            .after(&state, &mut response, &layer.runtime)
            .await;
        return finalize_response(&layer, &state, response).await;
    }

    if let Some(replay) = state.idempotency_replay.clone() {
        let request_id = state
            .request_id_value()
            .map(str::to_owned)
            .unwrap_or_else(new_request_id);
        let response = idempotency_replay_response(&replay, Some(&request_id))
            .unwrap_or_else(|error| problem_response(&error, state.problem_correlation()));
        return finalize_response(&layer, &state, response).await;
    }

    let idem_key = state.idempotency_key.clone();
    let idem_fp = state.idempotency_fingerprint.clone();
    let idem_leader = state.idempotency_leader;
    let idem_store = layer.runtime.idempotency_store.clone();
    let idem_guard = idem_leader.then(|| {
        IdempotencyReleaseGuard::new(
            idem_store.clone(),
            idem_key.clone().expect("leader has store key"),
            idem_fp.clone().expect("leader has fingerprint"),
        )
    });

    let handler_result = AssertUnwindSafe(run_handler_with_timeout(&layer.runtime, request, next))
        .catch_unwind()
        .await;
    let response = match handler_result {
        Ok(response) => response,
        Err(_) => {
            if let Some(guard) = &idem_guard {
                guard.mark_released();
            }
            if idem_leader {
                if let (Some(key), Some(fingerprint)) = (&idem_key, &idem_fp) {
                    let _ = idem_store.release(key, fingerprint).await;
                }
            }
            return finalize_response(
                &layer,
                &state,
                problem_response(
                    &WebFrameworkError::dependency_unavailable("request handler panicked"),
                    state.problem_correlation(),
                ),
            )
            .await;
        }
    };
    if let Some(guard) = &idem_guard {
        guard.mark_released();
    }
    finalize_response(&layer, &state, response).await
}
