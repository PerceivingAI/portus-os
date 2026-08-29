use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum StateError {
    Sqlite(rusqlite::Error),
    Io(io::Error),
    InvalidPath(String),
    UnsupportedSchemaVersion { found: u32, latest: u32 },
    InvalidMigrationHistory { expected: u32, found: u32 },
    MigrationFailed { version: u32, message: String },
    IntegrityFailure(String),
    ReadOnlySchemaMissing,
    InvalidProviderState(String),
    InvalidIndexState(String),
    InvalidEventState(String),
    InvalidTaskState(String),
    InvalidHealthState(String),
    InvalidArtifactState(String),
    TaskPreconditionFailed { expected: String, found: String },
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "SQLite state error: {error}"),
            Self::Io(error) => write!(f, "state filesystem error: {error}"),
            Self::InvalidPath(message) => write!(f, "invalid state path: {message}"),
            Self::UnsupportedSchemaVersion { found, latest } => write!(
                f,
                "database schema version {found} is newer than supported version {latest}"
            ),
            Self::InvalidMigrationHistory { expected, found } => write!(
                f,
                "database migration history is not contiguous: expected version {expected}, found {found}"
            ),
            Self::MigrationFailed { version, message } => {
                write!(f, "migration to schema version {version} failed: {message}")
            }
            Self::IntegrityFailure(message) => {
                write!(f, "database integrity check failed: {message}")
            }
            Self::ReadOnlySchemaMissing => {
                f.write_str("read-only state database has no initialized Portus schema")
            }
            Self::InvalidProviderState(message) => write!(f, "invalid provider state: {message}"),
            Self::InvalidIndexState(message) => write!(f, "invalid index state: {message}"),
            Self::InvalidEventState(message) => write!(f, "invalid event state: {message}"),
            Self::InvalidTaskState(message) => write!(f, "invalid task state: {message}"),
            Self::InvalidHealthState(message) => write!(f, "invalid health state: {message}"),
            Self::InvalidArtifactState(message) => write!(f, "invalid artifact state: {message}"),
            Self::TaskPreconditionFailed { expected, found } => write!(
                f,
                "task state precondition failed: expected {expected}, found {found}"
            ),
        }
    }
}

impl Error for StateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sqlite(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StateError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<io::Error> for StateError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub type StateResult<T> = Result<T, StateError>;
