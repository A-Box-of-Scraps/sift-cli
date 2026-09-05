use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot resolve data directory: provide an absolute XDG_DATA_HOME or HOME")]
    MissingDataDirectory,
    #[error("invalid snapshot handle: {0}")]
    InvalidHandle(PathBuf),
    #[error("invalid snapshot metadata: {0}")]
    InvalidMetadata(String),
    #[error("unsupported snapshot format version: {0}")]
    UnsupportedFormat(u32),
    #[error("unsupported snapshot backend: {0}")]
    UnsupportedBackend(String),
    #[error("snapshot destination already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite operation failed: {0}")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(Box::new(error))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
