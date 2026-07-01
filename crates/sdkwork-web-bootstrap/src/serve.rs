use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

use crate::lifecycle::{NoOpWebFrameworkLifecycle, WebFrameworkLifecycle};

/// Run the router until Ctrl+C / SIGTERM (Tokio signal on supported platforms).
pub async fn serve(router: Router, addr: SocketAddr) -> std::io::Result<()> {
    serve_with_lifecycle(router, addr, Arc::new(NoOpWebFrameworkLifecycle), None).await
}

/// Run the router with EP-20 startup/shutdown hooks.
pub async fn serve_with_lifecycle(
    router: Router,
    addr: SocketAddr,
    lifecycle: Arc<dyn WebFrameworkLifecycle>,
    shutdown_grace_period: Option<Duration>,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    if let Err(detail) = lifecycle.on_startup().await {
        return Err(std::io::Error::other(format!(
            "WebFrameworkLifecycle startup hook failed: {detail}"
        )));
    }
    let lifecycle_for_shutdown = lifecycle.clone();
    let serve_future = axum::serve(listener, router)
        .with_graceful_shutdown(graceful_shutdown_trigger(lifecycle_for_shutdown));

    if let Some(grace) = shutdown_grace_period {
        match tokio::time::timeout(grace, serve_future).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(
                    grace_secs = grace.as_secs(),
                    "shutdown grace period elapsed while draining connections"
                );
                Ok(())
            }
        }
    } else {
        serve_future.await
    }
}

/// Waits for OS shutdown, starts lifecycle cleanup during connection drain, then completes
/// so Axum can stop accepting and drain inflight requests (`docs/architecture/tech/TECH-21-operations-runbook.md` §6).
async fn graceful_shutdown_trigger(lifecycle: Arc<dyn WebFrameworkLifecycle>) {
    wait_for_os_shutdown_signal().await;
    tracing::info!("shutdown signal received; beginning graceful connection drain");
    let lifecycle = lifecycle.clone();
    tokio::spawn(async move {
        if let Err(detail) = lifecycle.on_shutdown().await {
            tracing::error!(detail, "WebFrameworkLifecycle shutdown hook failed");
        }
    });
}

async fn wait_for_os_shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
