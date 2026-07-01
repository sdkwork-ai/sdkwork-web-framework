#[derive(Debug)]
pub enum RepositoryError {
    Database(String),
    StoredJson(String),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(message) => write!(f, "{message}"),
            Self::StoredJson(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for RepositoryError {}
