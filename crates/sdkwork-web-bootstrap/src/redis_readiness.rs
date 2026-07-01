//! EP-15: Redis PING readiness probe for distributed store wiring.

use crate::health::{ReadinessCheck, ReadinessFuture};

/// Verifies Redis is reachable before `/readyz` reports ready.
#[derive(Clone)]
pub struct RedisReadinessCheck {
    client: redis::Client,
}

impl RedisReadinessCheck {
    pub fn new(redis_url: impl AsRef<str>) -> Result<Self, redis::RedisError> {
        Ok(Self {
            client: redis::Client::open(redis_url.as_ref())?,
        })
    }

    pub fn from_client(client: redis::Client) -> Self {
        Self { client }
    }
}

impl ReadinessCheck for RedisReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let client = self.client.clone();
        Box::pin(async move {
            let mut conn = client
                .get_multiplexed_async_connection()
                .await
                .map_err(|error| error.to_string())?;
            let pong: String = redis::cmd("PING")
                .query_async(&mut conn)
                .await
                .map_err(|error| error.to_string())?;
            if pong.eq_ignore_ascii_case("PONG") {
                Ok(())
            } else {
                Err(format!("redis ping returned unexpected payload: {pong}"))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_from_redis_url() {
        let check = RedisReadinessCheck::new("redis://127.0.0.1/").expect("client");
        let _ = check.client;
    }
}
