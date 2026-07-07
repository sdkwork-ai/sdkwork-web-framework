//! Admin control-plane SQL pool — SQLite (dev/standalone) and PostgreSQL (HA production).

#[derive(Clone)]
pub enum AdminStorePool {
    Sqlite(sqlx::SqlitePool),
    #[cfg(feature = "postgres")]
    Postgres(sqlx::PgPool),
}

impl AdminStorePool {
    pub fn is_sqlite(&self) -> bool {
        matches!(self, Self::Sqlite(_))
    }

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

    pub fn is_distributed_ha(&self) -> bool {
        self.is_postgres()
    }
}

impl From<sqlx::SqlitePool> for AdminStorePool {
    fn from(pool: sqlx::SqlitePool) -> Self {
        Self::Sqlite(pool)
    }
}

#[cfg(feature = "postgres")]
impl From<sqlx::PgPool> for AdminStorePool {
    fn from(pool: sqlx::PgPool) -> Self {
        Self::Postgres(pool)
    }
}
