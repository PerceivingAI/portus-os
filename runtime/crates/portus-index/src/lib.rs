//! PortusOS System Index source and correlation layer.
//!
//! This crate owns the finite first-ISO source vocabulary, bounded source
//! adapters/parsers, and source-neutral correlation rules. It does not own the
//! SQLite materialization, runtime RPC surface, policy, or native/provider
//! control operations.

mod correlation;
mod model;
mod parsers;

#[cfg(target_os = "linux")]
pub mod linux;

pub use correlation::{CORRELATION_SOURCE_ID, CorrelationOutput, correlate};
pub use model::{
    DisabledIndexSources, IndexRescanDomain, IndexSourceSet, SourceBatch, SourceCollection,
};
pub use parsers::{
    DesktopEntry, I3Display, I3TreePlacement, I3Workspace, OpenRcService, ProcessStat,
    WindowProperties, parse_desktop_entry, parse_i3_outputs, parse_i3_tree_placements,
    parse_i3_workspaces, parse_openrc_status, parse_proc_stat, parse_status_identity,
    parse_xprop_client_list, parse_xprop_window,
};
