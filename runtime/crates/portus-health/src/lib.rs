//! Common PortusOS health/recovery policy and bounded resource observation logic.
//!
//! This crate does not own durable state or service execution. It classifies
//! observations, enforces common restart budgets, and supplies native read-only
//! probes behind an explicit source boundary.

mod probe;
mod recovery;
mod resource;

#[cfg(target_os = "linux")]
mod linux;

pub use probe::{DisabledHealthProbes, HealthProbeSet};
pub use recovery::{
    MAX_RESTART_ATTEMPTS, RESTART_BACKOFF_MS, RESTART_WINDOW_MS, RestartBudgetDecision,
    STABLE_RESET_MS, evaluate_restart_budget,
};
pub use resource::{MemorySample, StorageSample, classify_memory, classify_storage};

#[cfg(target_os = "linux")]
pub use linux::LinuxHealthProbes;
