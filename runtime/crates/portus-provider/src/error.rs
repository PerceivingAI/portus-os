use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum ProviderError {
    Io(io::Error),
    Parse { file: String, message: String },
    InvalidManifest { file: String, message: String },
    UntrustedPath { path: String, message: String },
    TooManyManifests { limit: usize },
    ManifestTooLarge { file: String, limit: u64 },
    State(portus_state::StateError),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "provider registry I/O error: {error}"),
            Self::Parse { file, message } => {
                write!(f, "invalid provider manifest {file}: {message}")
            }
            Self::InvalidManifest { file, message } => {
                write!(f, "provider manifest {file} failed validation: {message}")
            }
            Self::UntrustedPath { path, message } => {
                write!(f, "untrusted provider manifest path {path}: {message}")
            }
            Self::TooManyManifests { limit } => {
                write!(f, "provider manifest count exceeds limit {limit}")
            }
            Self::ManifestTooLarge { file, limit } => {
                write!(f, "provider manifest {file} exceeds {limit} bytes")
            }
            Self::State(error) => write!(f, "provider registry state error: {error}"),
        }
    }
}

impl Error for ProviderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::State(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ProviderError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<portus_state::StateError> for ProviderError {
    fn from(value: portus_state::StateError) -> Self {
        Self::State(value)
    }
}

pub type ProviderResult<T> = Result<T, ProviderError>;
