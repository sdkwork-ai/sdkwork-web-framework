use crate::health::{ReadinessCheck, ReadinessFuture};
use sdkwork_database_sqlx::DatabasePool;
use sqlx::PgPool;
#[cfg(feature = "sqlx-sqlite")]
use sqlx::SqlitePool;

/// Verifies the canonical SDKWork database pool without exposing its engine to API assemblies.
#[derive(Clone)]
pub struct DatabasePoolReadinessCheck {
    pool: DatabasePool,
}

impl DatabasePoolReadinessCheck {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ReadinessCheck for DatabasePoolReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let pool = self.pool.clone();
        Box::pin(async move {
            match pool.test_connection().await {
                Ok(true) => Ok(()),
                Ok(false) => Err("database readiness query returned no row".to_owned()),
                Err(error) => Err(format!("database readiness check failed: {error}")),
            }
        })
    }
}

/// EP-15: verifies the shared SQLx store is reachable before `/readyz` reports ready.
#[cfg(feature = "sqlx-sqlite")]
#[derive(Clone)]
pub struct SqliteReadinessCheck {
    pool: SqlitePool,
}

#[cfg(feature = "sqlx-sqlite")]
impl SqliteReadinessCheck {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "sqlx-sqlite")]
impl ReadinessCheck for SqliteReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let pool = self.pool.clone();
        Box::pin(async move {
            sqlx::query("SELECT 1")
                .execute(&pool)
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }
}

/// Verifies a PostgreSQL pool is reachable before `/readyz` reports ready.
#[derive(Clone)]
pub struct PgPoolReadinessCheck {
    pool: PgPool,
}

impl PgPoolReadinessCheck {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl ReadinessCheck for PgPoolReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let pool = self.pool.clone();
        Box::pin(async move {
            sqlx::query("SELECT 1")
                .execute(&pool)
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }
}
