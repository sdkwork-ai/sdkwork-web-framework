//! SDKWork Web Store database pool bootstrap via `sdkwork-database`.

use sdkwork_database_config::DatabaseConfig;
use sdkwork_database_sqlx::{create_pool_from_config, DatabasePool, PoolError};

pub use sdkwork_webstore_database_host::{
    bootstrap_webstore_database, bootstrap_webstore_database_from_env, WebStoreDatabaseHost,
};

pub type WebStoreDatabasePool = DatabasePool;

pub async fn connect_webstore_database_pool_from_env() -> Result<WebStoreDatabasePool, PoolError> {
    let config = DatabaseConfig::from_env("WEB_STORE")?;
    create_pool_from_config(config).await
}

pub async fn connect_and_bootstrap_webstore_database_from_env(
) -> Result<WebStoreDatabaseHost, String> {
    let pool = connect_webstore_database_pool_from_env()
        .await
        .map_err(|error| error.to_string())?;
    bootstrap_webstore_database(pool).await
}
