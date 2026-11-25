//! Non-destructive body observation contract (request **and** response streams).
//!
//! # The rule
//!
//! An HTTP body — a request body read by a middleware, or a streamed response
//! body read by a finalizer — can be polled to completion **exactly once**. Any
//! stage that observes a body therefore has exactly two safe shapes:
//!
//! 1. **Replay.** Buffer the whole body, then hand the *same bytes* to the next
//!    reader: `Request::from_parts(parts, Body::from(bytes))` /
//!    `Response::from_parts(parts, Body::from(bytes))`.
//! 2. **Pass through.** Do not poll the stream at all; leave the body exactly
//!    where it is.
//!
//! A reader that has already consumed part of a stream **cannot rebuild it**.
//! That is why the decision between those two shapes must be taken *before* the
//! first poll — which is what [`may_buffer_whole`] answers.
//!
//! # What is forbidden
//!
//! Substituting an empty body where the original one was:
//!
//! ```ignore
//! // FORBIDDEN — when the capture is `None` the payload silently disappears.
//! let captured: Option<Bytes> = body.collect().await.ok().map(|c| c.to_bytes());
//! let rebuilt = Request::from_parts(parts, captured.map(Body::from).unwrap_or_else(Body::empty));
//! ```
//!
//! This is not a logging or enrichment bug — it is a data-plane outage. The
//! dependent service receives zero bytes while the headers still advertise the
//! original `content-length`, so it reports the request as malformed and blames
//! the caller. The same shape applied to a response turns a real upstream answer
//! into an empty `200`/`5xx`.
//!
//! # Applying it
//!
//! ```ignore
//! use sdkwork_web_core::body_contract::may_buffer_whole;
//!
//! // Decide first.
//! if !may_buffer_whole(response.body(), BUDGET) {
//!     return; // Cannot replay it ⇒ observation is skipped, body untouched.
//! }
//! // Then buffer and replay the exact bytes.
//! let (parts, body) = std::mem::replace(response, Response::new(Body::empty())).into_parts();
//! let bytes = to_bytes(body, BUDGET).await?;
//! *response = Response::from_parts(parts, Body::from(bytes));
//! ```
//!
//! Observation-only stages (request logging, audit, tracing) do not need to wait
//! for a whole body at all: they should **tee** the stream — forward every frame
//! unchanged and retain a bounded copy for the record.

use axum::body::HttpBody;

/// Returns `true` when `body` is known to fit inside `budget` bytes, so it can
/// be buffered once and replayed without losing anything.
///
/// Returns `false` for a body whose length is not declared up front (a chunked
/// or streamed body): such a body cannot be proven small, and buffering it would
/// leave its reader unable to restore it if it turns out larger. Callers must
/// then leave the body untouched, or tee it.
#[must_use]
pub fn may_buffer_whole<B>(body: &B, budget: usize) -> bool
where
    B: HttpBody + ?Sized,
{
    body.size_hint()
        .upper()
        .is_some_and(|upper| upper <= budget as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    #[test]
    fn accepts_a_body_whose_declared_length_fits_the_budget() {
        assert!(may_buffer_whole(&Body::from(vec![0u8; 128]), 1024));
        // The boundary itself is inside the budget.
        assert!(may_buffer_whole(&Body::from(vec![0u8; 1024]), 1024));
        // An empty body is trivially replayable.
        assert!(may_buffer_whole(&Body::empty(), 1024));
    }

    #[test]
    fn rejects_a_body_larger_than_the_budget() {
        assert!(!may_buffer_whole(&Body::from(vec![0u8; 1025]), 1024));
    }

    // A streamed (chunked) body reports no upper bound, so `may_buffer_whole`
    // answers `false`; that branch is documented above and exercised end to end
    // by the request-log tee regression
    // (`megabyte_scale_bodies_are_forwarded_without_loss`).
}
