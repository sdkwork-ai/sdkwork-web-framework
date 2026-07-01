//! Redis store adapters for rate limiting, idempotency, and concurrent admission (EP-09 / EP-11 / D9).

use async_trait::async_trait;
use redis::AsyncCommands;
use sdkwork_web_core::{
    ConcurrentAdmissionStore, IdempotencyBeginOutcome, IdempotencyResponseRecord, IdempotencyStore,
    RateLimitStore, WebFrameworkError,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

const CONCURRENT_ADMISSION_KEY_TTL_SECS: u64 = 7_200;

const CONCURRENT_ADMISSION_ACQUIRE_SCRIPT: &str = r#"
local current = redis.call('GET', KEYS[1])
if current and tonumber(current) >= tonumber(ARGV[1]) then
  return -1
end
local count = redis.call('INCR', KEYS[1])
if count == 1 then
  redis.call('EXPIRE', KEYS[1], tonumber(ARGV[2]))
end
return count
"#;

const CONCURRENT_ADMISSION_RELEASE_SCRIPT: &str = r#"
local count = redis.call('DECR', KEYS[1])
if count <= 0 then
  redis.call('DEL', KEYS[1])
  return 0
end
return count
"#;

/// Sliding window rate limiter (sorted set based).
///
/// Uses Redis sorted set with per-request timestamps as scores.
/// Counts entries within the current window (now - window_secs, now].
/// Purges expired entries atomically via `ZREMRANGEBYSCORE`.
///
/// Advantages over the fixed-window counter (`INCR`/`EXPIRE`):
/// - No burst at window boundaries (e.g. 59s + 61s can't do 2x quota)
/// - More uniform rate enforcement
///
/// KEYS[1] = rate limit key
/// ARGV[1] = max_requests (integer)
/// ARGV[2] = window_secs (integer) — TTL for the key
/// ARGV[3] = now_millis (integer) — current epoch in milliseconds
/// Returns: >= 0 = current count (allowed); -1 = rate limited
const RATE_LIMIT_SCRIPT: &str = r#"
	local key = KEYS[1]
	local max_requests = tonumber(ARGV[1])
	local window_secs = tonumber(ARGV[2])
	local now_ms = tonumber(ARGV[3])
	local window_start = now_ms - (window_secs * 1000)

	-- Remove entries outside the window
	redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)

	-- Count entries in the current window
	local current_count = redis.call('ZCARD', key)
	if current_count >= max_requests then
		return -1
	end

	-- Add current request timestamp
	redis.call('ZADD', key, now_ms, now_ms)
	redis.call('EXPIRE', key, window_secs)
	return current_count + 1
	"#;

const IDEMPOTENCY_BEGIN_SCRIPT: &str = r#"
local inserted = redis.call('SET', KEYS[1], ARGV[1], 'NX', 'EX', tonumber(ARGV[2]))
if inserted then
  return 1
end
local raw = redis.call('GET', KEYS[1])
if not raw then
  return 0
end
local ok, payload = pcall(cjson.decode, raw)
if not ok then
  return 0
end
if payload['fingerprint'] ~= ARGV[3] then
  return -1
end
if payload['response'] ~= nil and payload['response'] ~= cjson.null then
  return 2
end
return 0
"#;

const IDEMPOTENCY_RELEASE_SCRIPT: &str = r#"
local raw = redis.call('GET', KEYS[1])
if not raw then
  return 0
end
local ok, payload = pcall(cjson.decode, raw)
if not ok or payload['fingerprint'] ~= ARGV[1] then
  return 0
end
if payload['response'] == nil or payload['response'] == cjson.null then
  redis.call('DEL', KEYS[1])
end
return 1
"#;

const IDEMPOTENCY_COMPLETE_SCRIPT: &str = r#"
local raw = redis.call('GET', KEYS[1])
if not raw then
  return 0
end
local ok, payload = pcall(cjson.decode, raw)
if not ok or payload['fingerprint'] ~= ARGV[1] then
  return -1
end
if payload['response'] ~= nil and payload['response'] ~= cjson.null then
  return 1
end
redis.call('SET', KEYS[1], ARGV[2], 'EX', tonumber(ARGV[3]))
return 1
"#;

/// Redis-backed distributed rate limiter (fixed window counter).
pub struct RedisRateLimitStore {
    client: redis::Client,
    key_prefix: String,
    rate_limit_script: redis::Script,
}

impl RedisRateLimitStore {
    pub fn new(
        redis_url: impl AsRef<str>,
        key_prefix: impl Into<String>,
    ) -> Result<Self, redis::RedisError> {
        Ok(Self {
            client: redis::Client::open(redis_url.as_ref())?,
            key_prefix: key_prefix.into(),
            rate_limit_script: redis::Script::new(RATE_LIMIT_SCRIPT),
        })
    }

    fn key(&self, logical_key: &str) -> String {
        format!("{}:rl:{}", self.key_prefix, logical_key)
    }
}

#[async_trait]
impl RateLimitStore for RedisRateLimitStore {
    fn is_distributed_ha(&self) -> bool {
        true
    }

    async fn check_and_record(
        &self,
        key: &str,
        max_requests: u32,
        window: Duration,
    ) -> Result<(), WebFrameworkError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(redis_error)?;
        let redis_key = self.key(key);
        let result: i64 = self
            .rate_limit_script
            .key(redis_key)
            .arg(max_requests)
            .arg(window.as_secs().max(1) as i64)
            .arg(now_epoch_ms())
            .invoke_async(&mut conn)
            .await
            .map_err(redis_error)?;
        if result < 0 {
            return Err(WebFrameworkError::rate_limit_exceeded(
                "rate limit exceeded",
                window.as_secs().max(1),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RedisIdempotencyPayload {
    fingerprint: String,
    response: Option<IdempotencyResponseRecord>,
}

/// Redis-backed idempotency key registry with response replay.
pub struct RedisIdempotencyStore {
    client: redis::Client,
    key_prefix: String,
    begin_script: redis::Script,
    complete_script: redis::Script,
    release_script: redis::Script,
}

impl RedisIdempotencyStore {
    pub fn new(
        redis_url: impl AsRef<str>,
        key_prefix: impl Into<String>,
    ) -> Result<Self, redis::RedisError> {
        Ok(Self {
            client: redis::Client::open(redis_url.as_ref())?,
            key_prefix: key_prefix.into(),
            begin_script: redis::Script::new(IDEMPOTENCY_BEGIN_SCRIPT),
            complete_script: redis::Script::new(IDEMPOTENCY_COMPLETE_SCRIPT),
            release_script: redis::Script::new(IDEMPOTENCY_RELEASE_SCRIPT),
        })
    }

    fn key(&self, idempotency_key: &str) -> String {
        format!("{}:idem:{}", self.key_prefix, idempotency_key)
    }
}

#[async_trait]
impl IdempotencyStore for RedisIdempotencyStore {
    fn is_distributed_ha(&self) -> bool {
        true
    }

    async fn begin(
        &self,
        key: &str,
        fingerprint: &str,
        ttl: Duration,
    ) -> Result<IdempotencyBeginOutcome, WebFrameworkError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(redis_error)?;
        let redis_key = self.key(key);
        let payload = RedisIdempotencyPayload {
            fingerprint: fingerprint.to_owned(),
            response: None,
        };
        let serialized = serde_json::to_string(&payload).map_err(encode_error)?;
        let ttl_secs = ttl.as_secs().max(1) as i64;
        let result: i64 = self
            .begin_script
            .key(redis_key.clone())
            .arg(serialized)
            .arg(ttl_secs)
            .arg(fingerprint)
            .invoke_async(&mut conn)
            .await
            .map_err(redis_error)?;
        match result {
            1 => Ok(IdempotencyBeginOutcome::Leader),
            2 => {
                let raw: String = conn.get(redis_key).await.map_err(redis_error)?;
                let stored: RedisIdempotencyPayload =
                    serde_json::from_str(&raw).map_err(decode_error)?;
                if let Some(response) = stored.response {
                    Ok(IdempotencyBeginOutcome::Replay(response))
                } else {
                    Err(WebFrameworkError::conflict(
                        "idempotency key is already in progress",
                    ))
                }
            }
            -1 => Err(WebFrameworkError::conflict(
                "idempotency key was already used with a different request fingerprint",
            )),
            _ => Err(WebFrameworkError::conflict(
                "idempotency key is already in progress",
            )),
        }
    }

    async fn complete(
        &self,
        key: &str,
        fingerprint: &str,
        record: IdempotencyResponseRecord,
        ttl: Duration,
    ) -> Result<(), WebFrameworkError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(redis_error)?;
        let redis_key = self.key(key);
        let payload = RedisIdempotencyPayload {
            fingerprint: fingerprint.to_owned(),
            response: Some(record),
        };
        let serialized = serde_json::to_string(&payload).map_err(encode_error)?;
        let result: i64 = self
            .complete_script
            .key(redis_key)
            .arg(fingerprint)
            .arg(serialized)
            .arg(ttl.as_secs().max(1) as i64)
            .invoke_async(&mut conn)
            .await
            .map_err(redis_error)?;
        match result {
            0 => Err(WebFrameworkError::bad_request(
                "idempotency key was not reserved",
            )),
            -1 => Err(WebFrameworkError::conflict(
                "idempotency key fingerprint mismatch while completing response",
            )),
            _ => Ok(()),
        }
    }

    async fn release(&self, key: &str, fingerprint: &str) -> Result<(), WebFrameworkError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(redis_error)?;
        let redis_key = self.key(key);
        let _: i64 = self
            .release_script
            .key(redis_key)
            .arg(fingerprint)
            .invoke_async(&mut conn)
            .await
            .map_err(redis_error)?;
        Ok(())
    }
}

pub fn shared_rate_limit_store(
    redis_url: impl AsRef<str>,
    key_prefix: impl Into<String>,
) -> Result<Arc<dyn RateLimitStore>, redis::RedisError> {
    Ok(Arc::new(RedisRateLimitStore::new(redis_url, key_prefix)?))
}

pub fn shared_idempotency_store(
    redis_url: impl AsRef<str>,
    key_prefix: impl Into<String>,
) -> Result<Arc<dyn IdempotencyStore>, redis::RedisError> {
    Ok(Arc::new(RedisIdempotencyStore::new(redis_url, key_prefix)?))
}

/// Redis-backed distributed per-tenant concurrent request admission.
pub struct RedisConcurrentAdmissionStore {
    client: redis::Client,
    key_prefix: String,
    acquire_script: redis::Script,
    release_script: redis::Script,
}

impl RedisConcurrentAdmissionStore {
    pub fn new(
        redis_url: impl AsRef<str>,
        key_prefix: impl Into<String>,
    ) -> Result<Self, redis::RedisError> {
        Ok(Self {
            client: redis::Client::open(redis_url.as_ref())?,
            key_prefix: key_prefix.into(),
            acquire_script: redis::Script::new(CONCURRENT_ADMISSION_ACQUIRE_SCRIPT),
            release_script: redis::Script::new(CONCURRENT_ADMISSION_RELEASE_SCRIPT),
        })
    }

    fn key(&self, logical_key: &str) -> String {
        format!("{}:conc:{}", self.key_prefix, logical_key)
    }
}

#[async_trait]
impl ConcurrentAdmissionStore for RedisConcurrentAdmissionStore {
    fn is_distributed_ha(&self) -> bool {
        true
    }

    async fn try_acquire(&self, key: &str, limit: u32) -> Result<(), WebFrameworkError> {
        if limit == 0 {
            return Ok(());
        }
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(redis_error)?;
        let redis_key = self.key(key);
        let result: i64 = self
            .acquire_script
            .key(redis_key)
            .arg(limit)
            .arg(CONCURRENT_ADMISSION_KEY_TTL_SECS)
            .invoke_async(&mut conn)
            .await
            .map_err(redis_error)?;
        if result < 0 {
            return Err(WebFrameworkError::rate_limit_exceeded(
                "tenant concurrent request limit exceeded",
                1,
            ));
        }
        Ok(())
    }

    async fn release(&self, key: &str) -> Result<(), WebFrameworkError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(redis_error)?;
        let redis_key = self.key(key);
        let _: i64 = self
            .release_script
            .key(redis_key)
            .invoke_async(&mut conn)
            .await
            .map_err(redis_error)?;
        Ok(())
    }
}

pub fn shared_concurrent_admission_store(
    redis_url: impl AsRef<str>,
    key_prefix: impl Into<String>,
) -> Result<Arc<dyn ConcurrentAdmissionStore>, redis::RedisError> {
    Ok(Arc::new(RedisConcurrentAdmissionStore::new(
        redis_url, key_prefix,
    )?))
}

fn redis_error(error: redis::RedisError) -> WebFrameworkError {
    WebFrameworkError::dependency_unavailable(format!("redis store error: {error}"))
}

fn encode_error(error: serde_json::Error) -> WebFrameworkError {
    WebFrameworkError::dependency_unavailable(format!("redis idempotency encode error: {error}"))
}

fn decode_error(error: serde_json::Error) -> WebFrameworkError {
    WebFrameworkError::dependency_unavailable(format!("redis idempotency decode error: {error}"))
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_keys_are_namespaced() {
        let store = RedisRateLimitStore {
            client: redis::Client::open("redis://127.0.0.1/").expect("client"),
            key_prefix: "sdkwork".to_owned(),
            rate_limit_script: redis::Script::new(RATE_LIMIT_SCRIPT),
        };
        assert_eq!("sdkwork:rl:tenant:1", store.key("tenant:1"));
    }

    #[test]
    fn concurrent_admission_keys_are_namespaced() {
        let store = RedisConcurrentAdmissionStore {
            client: redis::Client::open("redis://127.0.0.1/").expect("client"),
            key_prefix: "sdkwork".to_owned(),
            acquire_script: redis::Script::new(CONCURRENT_ADMISSION_ACQUIRE_SCRIPT),
            release_script: redis::Script::new(CONCURRENT_ADMISSION_RELEASE_SCRIPT),
        };
        assert_eq!("sdkwork:conc:tenant:1", store.key("tenant:1"));
    }

    #[test]
    fn redis_stores_report_distributed_ha() {
        let rate_limit = RedisRateLimitStore {
            client: redis::Client::open("redis://127.0.0.1/").expect("client"),
            key_prefix: "sdkwork".to_owned(),
            rate_limit_script: redis::Script::new(RATE_LIMIT_SCRIPT),
        };
        let idempotency =
            RedisIdempotencyStore::new("redis://127.0.0.1/", "sdkwork").expect("idempotency store");
        let admission = RedisConcurrentAdmissionStore {
            client: redis::Client::open("redis://127.0.0.1/").expect("client"),
            key_prefix: "sdkwork".to_owned(),
            acquire_script: redis::Script::new(CONCURRENT_ADMISSION_ACQUIRE_SCRIPT),
            release_script: redis::Script::new(CONCURRENT_ADMISSION_RELEASE_SCRIPT),
        };
        assert!(rate_limit.is_distributed_ha());
        assert!(idempotency.is_distributed_ha());
        assert!(admission.is_distributed_ha());
    }

    #[test]
    fn idempotency_keys_are_namespaced() {
        let store =
            RedisIdempotencyStore::new("redis://127.0.0.1/", "sdkwork").expect("idempotency store");
        assert_eq!("sdkwork:idem:order-42", store.key("order-42"));
    }
}
