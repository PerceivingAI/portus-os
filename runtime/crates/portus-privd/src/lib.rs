//! Narrow root privilege boundary for PortusOS.
//!
//! The daemon accepts only typed policy operations. It intentionally exposes
//! no generic shell, exec, arbitrary-root-file, or caller-selected command API.

mod core;
mod repository;

#[cfg(target_os = "linux")]
mod unix;

pub use core::{
    OperationExecutor, PrivilegeCore, PrivilegeError, PrivilegeResult, UnavailableExecutor,
};
pub use repository::{FilesystemPolicyRepository, PolicyRepository};

#[cfg(target_os = "linux")]
pub use unix::{PrivilegeServer, PrivilegeServerConfig};

pub const CANONICAL_USE_SOCKET: &str = "/run/portus/priv/use.sock";
pub const CANONICAL_ADMIN_SOCKET: &str = "/run/portus/priv/admin.sock";
pub const CANONICAL_AUDIT_PATH: &str = "/var/log/portus/audit/portus-privd.jsonl";
