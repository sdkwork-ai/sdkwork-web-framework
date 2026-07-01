use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Canonical infrastructure paths that bypass auth and interceptor chains.
pub fn infra_public_path_prefixes() -> Vec<String> {
    vec![
        "/healthz".to_owned(),
        "/readyz".to_owned(),
        "/livez".to_owned(),
        "/metrics".to_owned(),
    ]
}

pub type ReadinessFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

/// Optional readiness probe injected by the application.
pub trait ReadinessCheck: Send + Sync {
    fn check(&self) -> ReadinessFuture<'_>;
}

#[derive(Clone, Default)]
pub struct AlwaysReady;

impl ReadinessCheck for AlwaysReady {
    fn check(&self) -> ReadinessFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

/// Runs multiple readiness probes; fails fast on the first error (EP-15 composite).
#[derive(Clone, Default)]
pub struct CompositeReadinessCheck {
    checks: Vec<Arc<dyn ReadinessCheck>>,
}

impl CompositeReadinessCheck {
    pub fn new(checks: Vec<Arc<dyn ReadinessCheck>>) -> Self {
        Self { checks }
    }

    pub fn push(mut self, check: Arc<dyn ReadinessCheck>) -> Self {
        self.checks.push(check);
        self
    }
}

impl ReadinessCheck for CompositeReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let checks = self.checks.clone();
        Box::pin(async move {
            for check in checks {
                check.check().await?;
            }
            Ok(())
        })
    }
}

pub async fn healthz_handler() -> impl IntoResponse {
    (StatusCode::OK, axum::Json(json!({ "status": "ok" })))
}

/// Kubernetes-style liveness alias; delegates to [`healthz_handler`].
pub async fn livez_handler() -> impl IntoResponse {
    healthz_handler().await
}

/// Client-safe readiness failure message (`SECURITY_SPEC.md` §3 — no dependency internals).
pub const READINESS_DEPENDENCY_UNAVAILABLE: &str =
    "one or more dependencies are unavailable; see server logs for detail";

pub async fn readyz_handler(check: Option<Arc<dyn ReadinessCheck>>) -> Response {
    let Some(check) = check else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({
                "status": "not_ready",
                "detail": "readiness probe is not configured; wire WebFrameworkBuilder::readiness_check() or ServiceRouterConfig::with_always_ready()"
            })),
        )
            .into_response();
    };
    match check.check().await {
        Ok(()) => (StatusCode::OK, axum::Json(json!({ "status": "ready" }))).into_response(),
        Err(detail) => {
            tracing::error!(readiness_error = %detail, "readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(json!({
                    "status": "not_ready",
                    "detail": READINESS_DEPENDENCY_UNAVAILABLE
                })),
            )
                .into_response()
        }
    }
}
