//! Core `portusd` runtime coordination service.
//!
//! P3 implements only the authenticated local runtime foundation: bounded
//! JSONL dispatch, SQLite readiness, in-memory subscriber plumbing, and the
//! real Unix-domain transport on the Linux/Artix target. Higher-level task/index/provider
//! behavior belongs to later phases.

mod core;
mod error;
mod events;

#[cfg(target_os = "linux")]
mod unix;

pub use core::{RuntimeCore, RuntimeReadiness};
pub use error::{RuntimeError, RuntimeResult};
pub use events::{EventFilter, EventHub, EventSubscription, PublishOutcome, RuntimeEvent};
pub use portus_health::{DisabledHealthProbes, HealthProbeSet};
pub use portus_index::{DisabledIndexSources, IndexRescanDomain, IndexSourceSet};
pub use portus_policy::{PolicyPaths, PolicyTrust};
pub use portus_provider::ManifestTrust;

#[cfg(target_os = "linux")]
pub use unix::RuntimeServer;

use portus_client::{DEFAULT_IO_TIMEOUT, DEFAULT_MAX_FRAME_BYTES};
use portus_provider::CANONICAL_MANIFEST_DIR;
use std::{path::PathBuf, time::Duration};

pub const CANONICAL_SOCKET_PATH: &str = "/run/portus/portusd.sock";
pub const DEFAULT_MAX_CONNECTIONS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexSourceMode {
    NativeLinux,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthProbeMode {
    NativeLinux,
    Disabled,
}

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub socket_path: PathBuf,
    pub state_path: PathBuf,
    pub audit_path: PathBuf,
    pub max_frame_bytes: usize,
    pub io_timeout: Duration,
    pub max_connections: usize,
    pub provider_manifest_dir: PathBuf,
    pub provider_manifest_trust: ManifestTrust,
    pub policy_paths: PolicyPaths,
    pub policy_trust: PolicyTrust,
    pub index_source_mode: IndexSourceMode,
    pub health_probe_mode: HealthProbeMode,
}

impl RuntimeConfig {
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            socket_path: CANONICAL_SOCKET_PATH.into(),
            state_path: portus_state::CANONICAL_DATABASE_PATH.into(),
            audit_path: portus_audit::CANONICAL_AUDIT_PATH.into(),
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            io_timeout: DEFAULT_IO_TIMEOUT,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            provider_manifest_dir: CANONICAL_MANIFEST_DIR.into(),
            provider_manifest_trust: ManifestTrust::RootOwnedSystem,
            policy_paths: PolicyPaths::canonical(),
            policy_trust: PolicyTrust::RootOwnedSystem,
            index_source_mode: IndexSourceMode::NativeLinux,
            health_probe_mode: HealthProbeMode::NativeLinux,
        }
    }
}
