use crate::request_context::WebRequestContext;
use crate::request_identity::ServerRequestId;
use axum::extract::Request;
use std::sync::Arc;

/// Business-owned hook: map `WebRequestContext` into domain types in Extensions.
pub trait DomainContextInjector: Send + Sync {
    fn inject(&self, request: &mut Request, context: &WebRequestContext);
}

pub fn inject_web_request_context(
    request: &mut Request,
    request_id: ServerRequestId,
    context: WebRequestContext,
    domain_injectors: &[Arc<dyn DomainContextInjector>],
) {
    request.extensions_mut().insert(request_id);
    request.extensions_mut().insert(context.clone());
    for injector in domain_injectors {
        injector.inject(request, &context);
    }
}
