use crate::error::WebFrameworkError;
use crate::idempotency::{IdempotencyBeginOutcome, IdempotencyResponseRecord};
use async_trait::async_trait;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Stage 8 — rate limit backing store.
#[async_trait]
pub trait RateLimitStore: Send + Sync + Any {
    /// `true` when the store is safe for multi-replica SaaS production (e.g. Redis).
    fn is_distributed_ha(&self) -> bool {
        false
    }

    async fn check_and_record(
        &self,
        key: &str,
        max_requests: u32,
        window: Duration,
    ) -> Result<(), WebFrameworkError>;
}

/// Stage 9 — idempotency backing store (begin / complete / replay).
#[async_trait]
pub trait IdempotencyStore: Send + Sync + Any {
    /// `true` when the store is safe for multi-replica SaaS production (e.g. Redis).
    fn is_distributed_ha(&self) -> bool {
        false
    }

    async fn begin(
        &self,
        key: &str,
        fingerprint: &str,
        ttl: Duration,
    ) -> Result<IdempotencyBeginOutcome, WebFrameworkError>;

    async fn complete(
        &self,
        key: &str,
        fingerprint: &str,
        record: IdempotencyResponseRecord,
        ttl: Duration,
    ) -> Result<(), WebFrameworkError>;

    /// Drop an in-progress reservation (5xx / panic) so clients may retry with the same key.
    async fn release(&self, key: &str, fingerprint: &str) -> Result<(), WebFrameworkError>;
}

/// Optional per-tenant in-flight request admission (catalog D9).
#[async_trait]
pub trait ConcurrentAdmissionStore: Send + Sync + Any {
    /// `true` when admission counters are shared across replicas (e.g. Redis).
    fn is_distributed_ha(&self) -> bool {
        false
    }

    async fn try_acquire(&self, key: &str, limit: u32) -> Result<(), WebFrameworkError>;
    async fn release(&self, key: &str) -> Result<(), WebFrameworkError>;
}

#[derive(Default)]
pub struct MemoryConcurrentAdmissionStore {
    active: Mutex<HashMap<String, u32>>,
}

#[async_trait]
impl ConcurrentAdmissionStore for MemoryConcurrentAdmissionStore {
    async fn try_acquire(&self, key: &str, limit: u32) -> Result<(), WebFrameworkError> {
        if limit == 0 {
            return Ok(());
        }
        let mut active = self.active.lock().await;
        let count = active.entry(key.to_owned()).or_insert(0);
        if *count >= limit {
            return Err(WebFrameworkError::rate_limit_exceeded(
                "tenant concurrent request limit exceeded",
                1,
            ));
        }
        *count += 1;
        Ok(())
    }

    async fn release(&self, key: &str) -> Result<(), WebFrameworkError> {
        let mut active = self.active.lock().await;
        if let Some(count) = active.get_mut(key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                active.remove(key);
            }
        }
        Ok(())
    }
}

pub fn memory_concurrent_admission_store() -> Arc<dyn ConcurrentAdmissionStore> {
    Arc::new(MemoryConcurrentAdmissionStore::default())
}

struct MemoryRateLimitBucket {
    count: u32,
    window_start: Instant,
}

#[derive(Default)]
pub struct MemoryRateLimitStore {
    buckets: Mutex<HashMap<String, MemoryRateLimitBucket>>,
}

#[async_trait]
impl RateLimitStore for MemoryRateLimitStore {
    async fn check_and_record(
        &self,
        key: &str,
        max_requests: u32,
        window: Duration,
    ) -> Result<(), WebFrameworkError> {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();
        let bucket = buckets
            .entry(key.to_owned())
            .or_insert_with(|| MemoryRateLimitBucket {
                count: 0,
                window_start: now,
            });
        if now.duration_since(bucket.window_start) >= window {
            bucket.count = 0;
            bucket.window_start = now;
        }
        if bucket.count >= max_requests {
            return Err(WebFrameworkError::rate_limit_exceeded(
                "rate limit exceeded",
                window.as_secs().max(1),
            ));
        }
        bucket.count += 1;
        Ok(())
    }
}

struct MemoryIdempotencyEntry {
    fingerprint: String,
    response: Option<IdempotencyResponseRecord>,
    created: Instant,
}

const MEMORY_IDEMPOTENCY_MAX_ENTRIES: usize = 10_000;

#[derive(Default)]
pub struct MemoryIdempotencyStore {
    entries: Mutex<HashMap<String, MemoryIdempotencyEntry>>,
}

impl MemoryIdempotencyStore {
    fn purge_expired(entries: &mut HashMap<String, MemoryIdempotencyEntry>, ttl: Duration) {
        let now = Instant::now();
        entries.retain(|_, entry| now.duration_since(entry.created) < ttl);
        if entries.len() > MEMORY_IDEMPOTENCY_MAX_ENTRIES {
            let excess = entries.len() - MEMORY_IDEMPOTENCY_MAX_ENTRIES;
            let mut oldest: Vec<_> = entries
                .iter()
                .map(|(key, entry)| (key.clone(), entry.created))
                .collect();
            oldest.sort_by_key(|(_, created)| *created);
            for (key, _) in oldest.into_iter().take(excess) {
                entries.remove(&key);
            }
        }
    }
}

#[async_trait]
impl IdempotencyStore for MemoryIdempotencyStore {
    async fn begin(
        &self,
        key: &str,
        fingerprint: &str,
        ttl: Duration,
    ) -> Result<IdempotencyBeginOutcome, WebFrameworkError> {
        let mut entries = self.entries.lock().await;
        Self::purge_expired(&mut entries, ttl);
        if let Some(entry) = entries.get(key) {
            if entry.fingerprint != fingerprint {
                return Err(WebFrameworkError::conflict(
                    "idempotency key was already used with a different request fingerprint",
                ));
            }
            if let Some(response) = entry.response.clone() {
                return Ok(IdempotencyBeginOutcome::Replay(response));
            }
            return Err(WebFrameworkError::conflict(
                "idempotency key is already in progress",
            ));
        }
        entries.insert(
            key.to_owned(),
            MemoryIdempotencyEntry {
                fingerprint: fingerprint.to_owned(),
                response: None,
                created: Instant::now(),
            },
        );
        Ok(IdempotencyBeginOutcome::Leader)
    }

    async fn complete(
        &self,
        key: &str,
        fingerprint: &str,
        record: IdempotencyResponseRecord,
        ttl: Duration,
    ) -> Result<(), WebFrameworkError> {
        let mut entries = self.entries.lock().await;
        Self::purge_expired(&mut entries, ttl);
        let entry = entries
            .get_mut(key)
            .ok_or_else(|| WebFrameworkError::bad_request("idempotency key was not reserved"))?;
        if entry.fingerprint != fingerprint {
            return Err(WebFrameworkError::conflict(
                "idempotency key fingerprint mismatch while completing response",
            ));
        }
        entry.response = Some(record);
        Ok(())
    }

    async fn release(&self, key: &str, fingerprint: &str) -> Result<(), WebFrameworkError> {
        let mut entries = self.entries.lock().await;
        let Some(entry) = entries.get(key) else {
            return Ok(());
        };
        if entry.fingerprint != fingerprint {
            return Ok(());
        }
        if entry.response.is_none() {
            entries.remove(key);
        }
        Ok(())
    }
}

pub fn memory_rate_limit_store() -> Arc<dyn RateLimitStore> {
    Arc::new(MemoryRateLimitStore::default())
}

pub fn memory_idempotency_store() -> Arc<dyn IdempotencyStore> {
    Arc::new(MemoryIdempotencyStore::default())
}

/// Guard that automatically releases an idempotency key on drop.
/// Ensures keys are released even if the handler panics, times out, or returns an error.
/// SECURITY_SPEC §5.1 / WEB_FRAMEWORK_STANDARD §8.
pub struct IdempotencyGuard {
    store: Arc<dyn IdempotencyStore>,
    key: String,
    fingerprint: String,
    released: bool,
}

impl IdempotencyGuard {
    pub fn new(store: Arc<dyn IdempotencyStore>, key: String, fingerprint: String) -> Self {
        Self {
            store,
            key,
            fingerprint,
            released: false,
        }
    }

    /// Mark the guard as completed (prevents release on drop).
    pub fn mark_completed(&mut self) {
        self.released = true;
    }
}

impl Drop for IdempotencyGuard {
    fn drop(&mut self) {
        if !self.released {
            let store = self.store.clone();
            let key = self.key.clone();
            let fingerprint = self.fingerprint.clone();
            // Spawn async task to release the key.
            // This is fire-and-forget: if release fails, the key will expire via TTL.
            tokio::spawn(async move {
                if let Err(e) = store.release(&key, &fingerprint).await {
                    tracing::warn!(
                        idempotency_key = %key,
                        error = ?e,
                        "failed to release idempotency key on drop"
                    );
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idempotency::IdempotencyBeginOutcome;
    use crate::WebCallState;

    #[tokio::test]
    async fn release_clears_in_progress_reservation() {
        let store = MemoryIdempotencyStore::default();
        let ttl = Duration::from_secs(60);
        store.begin("key-1", "fp-a", ttl).await.expect("leader");
        store.release("key-1", "fp-a").await.expect("release");
        assert!(matches!(
            store
                .begin("key-1", "fp-a", ttl)
                .await
                .expect("leader again"),
            IdempotencyBeginOutcome::Leader
        ));
    }

    #[tokio::test]
    async fn replays_completed_response_for_same_fingerprint() {
        let store = MemoryIdempotencyStore::default();
        let ttl = Duration::from_secs(60);
        assert!(matches!(
            store.begin("key-1", "fp-a", ttl).await.expect("leader"),
            IdempotencyBeginOutcome::Leader
        ));
        store
            .complete(
                "key-1",
                "fp-a",
                IdempotencyResponseRecord {
                    status_code: 201,
                    body: br#"{"ok":true}"#.to_vec(),
                    content_type: Some("application/json".to_owned()),
                },
                ttl,
            )
            .await
            .expect("complete");
        let replay = store.begin("key-1", "fp-a", ttl).await.expect("replay");
        assert!(matches!(replay, IdempotencyBeginOutcome::Replay(_)));
    }

    #[tokio::test]
    async fn scoped_idempotency_keys_isolate_callers() {
        let store = MemoryIdempotencyStore::default();
        let ttl = Duration::from_secs(60);
        let fingerprint = "fp-shared".to_owned();

        let tenant_a = WebCallState::from_request(
            &axum::http::Request::builder()
                .method("POST")
                .uri("/app/v3/api/orders")
                .header("Authorization", "Bearer token-a")
                .header("Access-Token", "access-a")
                .body(axum::body::Body::empty())
                .expect("request"),
        );
        let tenant_b = WebCallState::from_request(
            &axum::http::Request::builder()
                .method("POST")
                .uri("/app/v3/api/orders")
                .header("Authorization", "Bearer token-b")
                .header("Access-Token", "access-b")
                .body(axum::body::Body::empty())
                .expect("request"),
        );
        let key_a = tenant_a.scoped_idempotency_store_key("shared-client-key");
        let key_b = tenant_b.scoped_idempotency_store_key("shared-client-key");
        assert_ne!(key_a, key_b);

        store
            .begin(&key_a, &fingerprint, ttl)
            .await
            .expect("leader");
        store
            .complete(
                &key_a,
                &fingerprint,
                IdempotencyResponseRecord {
                    status_code: 201,
                    body: b"100001".to_vec(),
                    content_type: None,
                },
                ttl,
            )
            .await
            .expect("complete");

        assert!(matches!(
            store
                .begin(&key_b, &fingerprint, ttl)
                .await
                .expect("leader for b"),
            IdempotencyBeginOutcome::Leader
        ));
    }

    #[tokio::test]
    async fn different_fingerprint_returns_conflict() {
        let store = MemoryIdempotencyStore::default();
        let ttl = Duration::from_secs(60);
        store.begin("key-1", "fp-a", ttl).await.expect("leader");
        let error = store
            .begin("key-1", "fp-b", ttl)
            .await
            .expect_err("conflict");
        assert_eq!(crate::WebFrameworkErrorKind::Conflict, error.kind);
    }

    #[tokio::test]
    async fn four_xx_responses_are_not_cached() {
        let store = MemoryIdempotencyStore::default();
        let ttl = Duration::from_secs(60);
        store.begin("key-1", "fp-a", ttl).await.expect("leader");
        store.release("key-1", "fp-a").await.expect("release");
        assert!(matches!(
            store
                .begin("key-1", "fp-a", ttl)
                .await
                .expect("retry allowed"),
            IdempotencyBeginOutcome::Leader
        ));
    }

    #[tokio::test]
    async fn concurrent_admission_enforces_limit() {
        let store = MemoryConcurrentAdmissionStore::default();
        store.try_acquire("100001", 1).await.expect("first");
        let error = store
            .try_acquire("100001", 1)
            .await
            .expect_err("second exceeds limit");
        assert_eq!(crate::WebFrameworkErrorKind::RateLimitExceeded, error.kind);
        store.release("100001").await.expect("release");
        store.try_acquire("100001", 1).await.expect("after release");
    }
}
