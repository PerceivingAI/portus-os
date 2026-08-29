use portus_state::StateError;
use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum RuntimeError {
    Io(io::Error),
    State(StateError),
    Audit(portus_audit::AuditError),
    Artifact(portus_artifact::ArtifactError),
    ArtifactCleanupBlocked(portus_state::ArtifactCleanupEligibility),
    ArtifactCleanupUnsupported,
    Provider(portus_provider::ProviderError),
    Task(portus_task::TaskError),
    InvalidConfiguration(String),
    RuntimeDirectoryMissing,
    SocketPathOccupied,
    SocketAlreadyActive,
    SignalHandler(String),
    #[cfg(target_os = "linux")]
    PeerCredentials(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "runtime I/O error: {error}"),
            Self::State(error) => write!(f, "runtime state error: {error}"),
            Self::Audit(error) => write!(f, "runtime audit error: {error}"),
            Self::Artifact(error) => write!(f, "runtime artifact error: {error}"),
            Self::ArtifactCleanupBlocked(reason) => {
                write!(f, "artifact cleanup is not eligible: {reason:?}")
            }
            Self::ArtifactCleanupUnsupported => {
                f.write_str("artifact cleanup adapter is not implemented for this locator")
            }
            Self::Provider(error) => write!(f, "provider registry error: {error}"),
            Self::Task(error) => write!(f, "runtime task error: {error}"),
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid runtime configuration: {message}")
            }
            Self::RuntimeDirectoryMissing => {
                f.write_str("runtime socket parent directory is missing or invalid")
            }
            Self::SocketPathOccupied => {
                f.write_str("runtime socket path exists but is not a Unix socket")
            }
            Self::SocketAlreadyActive => {
                f.write_str("another portusd instance is already accepting connections")
            }
            Self::SignalHandler(message) => {
                write!(f, "failed to install shutdown signal handler: {message}")
            }
            #[cfg(target_os = "linux")]
            Self::PeerCredentials(message) => {
                write!(f, "failed to authenticate Unix peer credentials: {message}")
            }
        }
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::State(error) => Some(error),
            Self::Audit(error) => Some(error),
            Self::Artifact(error) => Some(error),
            Self::Provider(error) => Some(error),
            Self::Task(error) => Some(error),
            Self::InvalidConfiguration(_)
            | Self::ArtifactCleanupBlocked(_)
            | Self::ArtifactCleanupUnsupported
            | Self::RuntimeDirectoryMissing
            | Self::SocketPathOccupied
            | Self::SocketAlreadyActive
            | Self::SignalHandler(_) => None,
            #[cfg(target_os = "linux")]
            Self::PeerCredentials(_) => None,
        }
    }
}

impl From<io::Error> for RuntimeError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<portus_artifact::ArtifactError> for RuntimeError {
    fn from(value: portus_artifact::ArtifactError) -> Self {
        Self::Artifact(value)
    }
}

impl From<portus_audit::AuditError> for RuntimeError {
    fn from(value: portus_audit::AuditError) -> Self {
        Self::Audit(value)
    }
}

impl From<StateError> for RuntimeError {
    fn from(value: StateError) -> Self {
        Self::State(value)
    }
}

impl From<portus_provider::ProviderError> for RuntimeError {
    fn from(value: portus_provider::ProviderError) -> Self {
        Self::Provider(value)
    }
}

impl From<portus_task::TaskError> for RuntimeError {
    fn from(value: portus_task::TaskError) -> Self {
        Self::Task(value)
    }
}

pub type RuntimeResult<T> = Result<T, RuntimeError>;
