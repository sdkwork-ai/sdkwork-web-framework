//! Admin control-plane SQL pool — PostgreSQL (HA production; DATABASE_SPEC:
//! authoritative-server persistence is PostgreSQL-only).

#[derive(Clone)]
pub enum AdminStorePool {
    #[cfg(feature = "postgres")]
    Postgres(sqlx::PgPool),
}

impl AdminStorePool {
    pub fn is_sqlite(&self) -> bool {
        false
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

#[cfg(feature = "postgres")]
impl From<sqlx::PgPool> for AdminStorePool {
    fn from(pool: sqlx::PgPool) -> Self {
        Self::Postgres(pool)
    }
}
