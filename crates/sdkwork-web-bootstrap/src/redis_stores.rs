//! Optional Redis store wiring when the `redis` feature is enabled.

pub use sdkwork_web_store_redis::{
    shared_concurrent_admission_store, shared_idempotency_store, shared_rate_limit_store,
    RedisConcurrentAdmissionStore, RedisIdempotencyStore, RedisRateLimitStore,
};
