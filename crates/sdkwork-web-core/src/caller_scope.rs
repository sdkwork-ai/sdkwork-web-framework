//! Request-scoped caller-credential accessor for backend adapters.
//!
//! `APP_SDK_INTEGRATION_SPEC.md` §5.2.1 requires a user-scoped backend call to
//! carry the caller's dual tokens to the destination. The framework verifies
//! them at the authentication stage and stores the typed carrier in the request
//! extensions ([`crate::context_injection`]). An outbound adapter reached
//! through many service layers (route handler → domain service → repository
//! port → HTTP adapter) would otherwise have to thread the carrier through every
//! intermediate signature, which is how per-site drift and process-credential
//! shortcuts appear.
//!
//! This module exposes the *same* verified carrier as a task-local scope that
//! the request pipeline installs around the handler. A deeper adapter reads it
//! with [`current_caller_credentials`] and stays inside the plain `&self` seam.
//!
//! Rules for consumers:
//!
//! - The value is a transport convenience, never an authorization decision: the
//!   principal was already verified by the pipeline and is carried separately in
//!   [`crate::WebRequestContext`].
//! - The scope never crosses a task boundary. Work moved to `tokio::spawn` or
//!   `spawn_blocking` sees an empty carrier by design — a background/worker path
//!   must use an explicitly declared service identity, and a user path that
//!   needs to cross such a boundary must carry the typed context itself.
//! - Outside a request task (tests, workers) the accessor yields an empty
//!   carrier, so an adapter fails closed rather than inventing an identity.

use std::future::Future;

use sdkwork_utils_rust::caller_identity::CallerCredentials;

tokio::task_local! {
    static CALLER_CREDENTIALS: CallerCredentials;
}

/// Runs `future` with the verified caller credentials installed for the whole
/// request task.
pub async fn with_caller_credentials<F>(credentials: CallerCredentials, future: F) -> F::Output
where
    F: Future,
{
    CALLER_CREDENTIALS.scope(credentials, future).await
}

/// The caller credentials of the request currently executing on this task.
///
/// Returns an empty carrier outside a request task or in a spawned task; see the
/// module documentation for why that is deliberate.
#[must_use]
pub fn current_caller_credentials() -> CallerCredentials {
    CALLER_CREDENTIALS
        .try_with(Clone::clone)
        .unwrap_or_default()
}

/// Whether a request-scoped caller carrier is installed on the current task.
///
/// Diagnostics only: distinguish "this path runs inside a request but carried no
/// credential" from "this path is not a request task at all".
#[must_use]
pub fn has_current_caller_credentials() -> bool {
    CALLER_CREDENTIALS.try_with(|_| ()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_request_task_sees_its_caller_credentials() {
        let credentials = CallerCredentials::new(
            Some("auth-token".to_owned()),
            Some("access-token".to_owned()),
        );
        let seen = with_caller_credentials(credentials.clone(), async {
            assert!(has_current_caller_credentials());
            current_caller_credentials()
        })
        .await;
        assert_eq!(seen, credentials);
        assert!(
            !has_current_caller_credentials(),
            "the scope ends with the request task"
        );
        assert!(current_caller_credentials().is_empty());
    }

    #[tokio::test]
    async fn a_spawned_task_does_not_inherit_the_caller() {
        let credentials = CallerCredentials::new(
            Some("auth-token".to_owned()),
            Some("access-token".to_owned()),
        );
        let inherited = with_caller_credentials(credentials, async {
            tokio::spawn(async { current_caller_credentials() })
                .await
                .expect("spawned task")
        })
        .await;
        assert!(
            inherited.is_empty(),
            "background work must carry an explicitly declared identity"
        );
    }
}
