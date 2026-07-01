//! Live Redis integration tests (optional CI gate via `SDKWORK_REDIS_TEST_URL`).

use sdkwork_web_core::{IdempotencyBeginOutcome, IdempotencyResponseRecord, WebFrameworkErrorKind};
use sdkwork_web_store_redis::{shared_idempotency_store, shared_rate_limit_store};
use std::time::{Duration, SystemTime};

fn redis_test_url() -> Option<String> {
    std::env::var("SDKWORK_REDIS_TEST_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn unique_key(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    format!("{prefix}:{nanos}")
}

#[tokio::test]
async fn redis_rate_limit_store_enforces_max_requests() {
    let Some(redis_url) = redis_test_url() else {
        eprintln!("skip: SDKWORK_REDIS_TEST_URL not set; live Redis test bypassed");
        return;
    };
    let store = shared_rate_limit_store(&redis_url, "sdkwork:test:rl").expect("redis store");
    let key = unique_key("tenant");
    let window = Duration::from_secs(60);

    store
        .check_and_record(&key, 2, window)
        .await
        .expect("first request");
    store
        .check_and_record(&key, 2, window)
        .await
        .expect("second request");
    let error = store
        .check_and_record(&key, 2, window)
        .await
        .expect_err("third request must be rate limited");
    assert_eq!(WebFrameworkErrorKind::RateLimitExceeded, error.kind);
}

#[tokio::test]
async fn redis_idempotency_store_replays_completed_response() {
    let Some(redis_url) = redis_test_url() else {
        eprintln!("skip: SDKWORK_REDIS_TEST_URL not set; live Redis test bypassed");
        return;
    };
    let store = shared_idempotency_store(&redis_url, "sdkwork:test:idem").expect("redis store");
    let key = unique_key("order");
    let fingerprint = "fp-1";
    let ttl = Duration::from_secs(120);

    let begin = store.begin(&key, fingerprint, ttl).await.expect("begin");
    assert!(matches!(begin, IdempotencyBeginOutcome::Leader));

    let record = IdempotencyResponseRecord {
        status_code: 200,
        body: br#"{"ok":true}"#.to_vec(),
        content_type: Some("application/json".to_owned()),
    };
    store
        .complete(&key, fingerprint, record, ttl)
        .await
        .expect("complete");

    let replay = store
        .begin(&key, fingerprint, ttl)
        .await
        .expect("replay begin");
    match replay {
        IdempotencyBeginOutcome::Replay(response) => {
            assert_eq!(200, response.status_code);
            assert_eq!(br#"{"ok":true}"#.to_vec(), response.body);
        }
        other => panic!("expected replay, got {other:?}"),
    }
}
