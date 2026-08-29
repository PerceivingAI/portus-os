//! Master Portus bootstrap and Codex/tmux launcher semantics.
//!
//! P13 keeps Master startup independent from `portusd`: tmux and Codex remain
//! directly usable even when the Portus runtime is degraded. This crate owns
//! only the user-scoped Master launcher contract, not graphical login, OpenRC,
//! Codex session storage, or Portus task/session truth.

mod args;
mod launch;
mod process;
mod system;
mod workspace;

pub use args::{BootstrapCli, MasterCli, MasterCommand, parse_bootstrap_from, parse_master_from};
pub use launch::{
    BootstrapOutcome, MASTER_TMUX_SESSION, MASTER_TMUX_WINDOW, MasterMode, run_bootstrap,
    run_master,
};
pub use process::{CommandOutcome, CommandRunner, CommandSpec, SystemCommandRunner};
pub use system::{LaunchContext, LaunchIdentity, LaunchLayout};
pub use workspace::{MASTER_AGENTS_CONTENT, PreparedWorkspace, prepare_workspace};

use std::{fmt, io, path::PathBuf};

#[derive(Debug)]
pub enum MasterLaunchError {
    RootExecution,
    InvalidIdentity(String),
    InvalidResumeTarget(String),
    WorkspaceRootUnavailable(PathBuf),
    UnsafeSymlink(PathBuf),
    UnsafeFileType(PathBuf),
    OwnershipMismatch {
        path: PathBuf,
        expected_uid: u32,
        found_uid: u32,
    },
    DependencyUnavailable {
        program: String,
        source: io::Error,
    },
    ProcessFailed {
        program: String,
        code: Option<i32>,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    IdentityDiscovery(String),
}

impl MasterLaunchError {
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::RootExecution | Self::OwnershipMismatch { .. } => 77,
            Self::DependencyUnavailable { .. } | Self::WorkspaceRootUnavailable(_) => 69,
            Self::InvalidIdentity(_)
            | Self::InvalidResumeTarget(_)
            | Self::UnsafeSymlink(_)
            | Self::UnsafeFileType(_) => 65,
            Self::ProcessFailed { .. } | Self::Io { .. } | Self::IdentityDiscovery(_) => 70,
        }
    }
}

impl fmt::Display for MasterLaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootExecution => f.write_str("Master Portus must not run as UID 0/root"),
            Self::InvalidIdentity(reason) => write!(f, "invalid Master identity: {reason}"),
            Self::InvalidResumeTarget(reason) => write!(f, "invalid Codex resume target: {reason}"),
            Self::WorkspaceRootUnavailable(path) => {
                write!(
                    f,
                    "Master workspace root is unavailable: {}",
                    path.display()
                )
            }
            Self::UnsafeSymlink(path) => {
                write!(
                    f,
                    "refusing symlink in Master workspace path: {}",
                    path.display()
                )
            }
            Self::UnsafeFileType(path) => {
                write!(
                    f,
                    "unexpected Master workspace file type: {}",
                    path.display()
                )
            }
            Self::OwnershipMismatch {
                path,
                expected_uid,
                found_uid,
            } => write!(
                f,
                "Master workspace ownership mismatch at {}: expected UID {expected_uid}, found {found_uid}",
                path.display()
            ),
            Self::DependencyUnavailable { program, source } => {
                write!(
                    f,
                    "required launcher dependency '{program}' is unavailable: {source}"
                )
            }
            Self::ProcessFailed { program, code } => match code {
                Some(code) => write!(f, "launcher command '{program}' exited with status {code}"),
                None => write!(
                    f,
                    "launcher command '{program}' terminated without an exit status"
                ),
            },
            Self::Io {
                operation,
                path,
                source,
            } => write!(f, "{operation} failed for {}: {source}", path.display()),
            Self::IdentityDiscovery(reason) => {
                write!(
                    f,
                    "unable to determine the current Master identity: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for MasterLaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DependencyUnavailable { source, .. } | Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type MasterLaunchResult<T> = Result<T, MasterLaunchError>;
