use crate::request_context::WebRequestContext;
use crate::request_identity::ServerRequestId;
use axum::extract::Request;
use sdkwork_utils_rust::caller_identity::CallerCredentials;
use std::sync::Arc;

/// Business-owned hook: map `WebRequestContext` into domain types in Extensions.
pub trait DomainContextInjector: Send + Sync {
    fn inject(&self, request: &mut Request, context: &WebRequestContext);
}

/// Injects identity without caller credentials (anonymous/bootstrap paths).
pub fn inject_web_request_context(
    request: &mut Request,
    request_id: ServerRequestId,
    context: WebRequestContext,
    domain_injectors: &[Arc<dyn DomainContextInjector>],
) {
    inject_web_request_context_with_credentials(
        request,
        request_id,
        context,
        CallerCredentials::default(),
        domain_injectors,
    );
}

/// Injects identity plus the caller's dual credentials.
///
/// `APP_SDK_INTEGRATION_SPEC.md` §5.2.1: a backend module that calls another
/// SDKWork surface on behalf of the end user must carry that user's identity.
/// Handlers receive the verified principal through [`WebRequestContext`], which
/// deliberately carries presence booleans instead of raw token material, so the
/// credentials travel in a separate request extension:
///
/// ```ignore
/// fn outbound(credentials: Extension<CallerCredentials>) {
///     credentials.apply_typed(|kind, token| match kind { /* set on the SDK client */ });
/// }
/// ```
///
/// The extension is always inserted, even when the request carried no
/// credential, so an outbound adapter can fail closed with
/// `sdkwork_utils_rust::caller_identity::missing_caller_identity` instead of
/// guessing. The carrier is never serialized, audited, or copied into the
/// request context.
pub fn inject_web_request_context_with_credentials(
    request: &mut Request,
    request_id: ServerRequestId,
    context: WebRequestContext,
    credentials: CallerCredentials,
    domain_injectors: &[Arc<dyn DomainContextInjector>],
) {
    request.extensions_mut().insert(request_id);
    request.extensions_mut().insert(context.clone());
    request.extensions_mut().insert(credentials);
    for injector in domain_injectors {
        injector.inject(request, &context);
    }
}
