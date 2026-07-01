//! Maps repository boundary errors to HTTP problem responses (`API_SPEC.md`).

use crate::response::ApiProblem;
use sdkwork_web_framework_admin_repository_sqlx::RepositoryError;

pub fn map_repository_error(error: RepositoryError) -> ApiProblem {
    match error {
        RepositoryError::Database(message) if message.contains("not found") => {
            ApiProblem::not_found(message)
        }
        RepositoryError::Database(message) => ApiProblem::dependency_unavailable(message),
        RepositoryError::StoredJson(message) => ApiProblem::dependency_unavailable(message),
    }
}
