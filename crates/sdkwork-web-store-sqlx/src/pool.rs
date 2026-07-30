/// SQLx connection pool backend abstraction.
///
/// Allows store adapters to share a single implementation path without
/// generic type parameter explosion. Each variant delegates to the
/// corresponding backend connection.
///
/// IMPORTANT: SQLite placeholders are `?`; PostgreSQL placeholders are `$1, $2, ...`.
/// This struct DOES NOT rewrite placeholders. PostgreSQL store implementations
/// MUST use `$N` syntax directly.
#[derive(Clone)]
pub enum WebStorePool {
    /// SQLite is available only to explicitly declared client-local consumers.
    #[cfg(feature = "sqlite")]
    Sqlite(sqlx::SqlitePool),
    /// PostgreSQL (multi-replica HA production capable).
    #[cfg(feature = "postgres")]
    Postgres(sqlx::PgPool),
}

impl WebStorePool {
    /// Returns `true` when backed by a single-replica SQLite pool.
    pub fn is_sqlite(&self) -> bool {
        #[cfg(feature = "sqlite")]
        {
            matches!(self, Self::Sqlite(_))
        }
        #[cfg(not(feature = "sqlite"))]
        {
            false
        }
    }

    /// Returns `true` when backed by a multi-replica-capable PostgreSQL pool.
    pub fn is_postgres(&self) -> bool {
        #[cfg(feature = "postgres")]
        {
            matches!(self, Self::Postgres(_))
        }
        #[cfg(not(feature = "postgres"))]
        {
            false
        }
    }

    /// Returns `true` if this pool supports distributed HA deployments.
    /// SQLite is single-node only; PostgreSQL can be HA with pooling/replication.
    pub fn is_distributed_ha(&self) -> bool {
        self.is_postgres()
    }
}

#[cfg(feature = "sqlite")]
impl From<sqlx::SqlitePool> for WebStorePool {
    fn from(pool: sqlx::SqlitePool) -> Self {
        Self::Sqlite(pool)
    }
}

#[cfg(feature = "postgres")]
impl From<sqlx::PgPool> for WebStorePool {
    fn from(pool: sqlx::PgPool) -> Self {
        Self::Postgres(pool)
    }
}
