//! PortusOS command-line client foundation.
//!
//! P4 owns argument parsing, stable presentation, runtime-client error mapping,
//! metadata/help/version output, and bounded daemon-independent diagnostics.
//! Higher-level domain behavior is defined by the public subsystem authorities under docs/.

mod app;
mod args;
mod command;
mod doctor;
mod output;
mod privilege;
mod runtime;

pub use app::{run_from, run_from_with_privilege, run_to_writers, run_to_writers_with_privilege};
pub use args::{
    ArtifactCommand, CapabilityCommand, CapabilityProviderCommand, Cli, Command, DoctorDomain,
    HealthCommand, IndexCommand, IndexControlPathArg, IndexEvidenceArg, IndexFreshnessArg,
    IndexQueryArgs, IndexRescanArg, IndexResourceArg, IndexSourceArg, OutputMode, PaginationArgs,
    PolicyAdminCommand, PolicyBundleCommand, PolicyCommand, PolicyEffectArg, TaskCommand,
    TaskListArgs, TaskStateArg, parse_from,
};
pub use command::{ExecutionContext, execute};
pub use doctor::{DoctorContext, DoctorReport};
pub use output::{
    CliError, CliMeta, CliSuccess, RenderedOutput, meta_with_request, render_error, render_success,
};
pub use privilege::{PrivilegeTransport, SystemPrivilege, UnavailablePrivilege};
pub use runtime::{RuntimeReply, RuntimeTransport, SystemRuntime, TaskStreamEnd};

pub const CLI_OUTPUT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const MIN_TIMEOUT_MS: u64 = 100;
pub const MAX_TIMEOUT_MS: u64 = 300_000;
pub const DEFAULT_PAGE_LIMIT: u16 = 50;
pub const MAX_PAGE_LIMIT: u16 = 200;
