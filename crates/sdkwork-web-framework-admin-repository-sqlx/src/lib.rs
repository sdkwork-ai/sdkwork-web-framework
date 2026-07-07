//! Persistence layer for framework control-plane admin tables (`WEB_BACKEND_SPEC.md` §2).

mod error;
mod models;
mod pagination;
mod pool;
mod repository;

pub use error::RepositoryError;
pub use models::*;
pub use pagination::{KeysetId, RepoKeysetPage, RepoOffsetPage};
pub use pool::AdminStorePool;
pub use repository::{SqlxWebFrameworkAdminRepository, WebFrameworkAdminRepository};
