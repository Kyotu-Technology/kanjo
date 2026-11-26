#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("config error: {0}")]
    Config(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

/// SQLite reports "busy" or "locked" when another process held the write lock
/// for longer than the busy timeout. That is not damage, so it is not reported as a
/// database error (whose advice is to restore a backup).
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Error {
        match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
                Error::Other("the database is busy because another Kanjo process is writing to it; try again in a moment".into())
            }
            _ => Error::Db(e),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
