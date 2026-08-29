use crate::{
    DEFAULT_PAGE_LIMIT, DEFAULT_TIMEOUT_MS, MAX_PAGE_LIMIT, MAX_TIMEOUT_MS, MIN_TIMEOUT_MS,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use portus_protocol::{ArtifactId, ProviderRegistrationId, TaskId};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    Human,
    Json,
    Jsonl,
}

#[derive(Debug, Parser)]
#[command(name = "portus-os", version, disable_help_subcommand = true)]
#[command(about = "PortusOS local control-plane CLI")]
pub struct Cli {
    /// Emit the stable JSON response envelope.
    #[arg(long, global = true, conflicts_with = "jsonl")]
    pub json: bool,

    /// Emit JSONL for commands that explicitly support streaming.
    #[arg(long, global = true, conflicts_with = "json")]
    pub jsonl: bool,

    /// Runtime request I/O timeout in milliseconds.
    #[arg(
        long,
        global = true,
        default_value_t = DEFAULT_TIMEOUT_MS,
        value_parser = clap::value_parser!(u64).range(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS)
    )]
    pub timeout_ms: u64,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    #[must_use]
    pub const fn output_mode(&self) -> OutputMode {
        if self.json {
            OutputMode::Json
        } else if self.jsonl {
            OutputMode::Jsonl
        } else {
            OutputMode::Human
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Cheap daemon-backed operational summary.
    Status,
    /// Bounded daemon-independent diagnostics.
    Doctor {
        #[arg(value_enum)]
        domain: Option<DoctorDomain>,
        /// Write the allowlisted diagnostic report to a new JSON evidence file.
        #[arg(long)]
        bundle: Option<PathBuf>,
    },
    /// Freshness-aware System Index operations.
    Index {
        #[command(subcommand)]
        command: IndexCommand,
    },
    /// Durable task lifecycle inspection and cancellation.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Capability/provider discovery.
    Capability {
        #[command(subcommand)]
        command: CapabilityCommand,
    },
    /// Policy inspection/preflight and root-only administration.
    Policy {
        #[command(subcommand)]
        command: PolicyCommand,
    },
    /// Deliberately registered artifact inspection.
    Artifact {
        #[command(subcommand)]
        command: ArtifactCommand,
    },
    /// Runtime-owned health view.
    Health {
        #[command(subcommand)]
        command: Option<HealthCommand>,
    },
    /// Machine-readable installed command contract.
    Help,
    /// CLI/output/runtime protocol version information.
    Version,
}

impl Command {
    #[must_use]
    pub const fn command_id(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Doctor { .. } => "doctor",
            Self::Index { command } => command.command_id(),
            Self::Task { command } => command.command_id(),
            Self::Capability {
                command: CapabilityCommand::List(_),
            } => "capability.list",
            Self::Capability {
                command: CapabilityCommand::Show { .. },
            } => "capability.show",
            Self::Capability {
                command:
                    CapabilityCommand::Provider {
                        command: CapabilityProviderCommand::List(_),
                    },
            } => "capability.provider.list",
            Self::Capability {
                command:
                    CapabilityCommand::Provider {
                        command: CapabilityProviderCommand::Show { .. },
                    },
            } => "capability.provider.show",
            Self::Policy { command } => command.command_id(),
            Self::Artifact { command } => command.command_id(),
            Self::Health { command: None } => "health",
            Self::Health {
                command: Some(HealthCommand::Show { .. }),
            } => "health.show",
            Self::Health {
                command: Some(HealthCommand::Degraded),
            } => "health.degraded",
            Self::Help => "help",
            Self::Version => "version",
        }
    }

    #[must_use]
    pub const fn supports_jsonl(&self) -> bool {
        matches!(
            self,
            Self::Task {
                command: TaskCommand::Events { follow: true, .. }
            }
        )
    }
}

#[derive(Debug, Subcommand)]
pub enum PolicyCommand {
    Effective,
    Check {
        action: String,
        #[arg(long)]
        resource: Option<String>,
    },
    Admin {
        #[command(subcommand)]
        command: PolicyAdminCommand,
    },
}

impl PolicyCommand {
    #[must_use]
    pub const fn command_id(&self) -> &'static str {
        match self {
            Self::Effective => "policy.effective",
            Self::Check { .. } => "policy.check",
            Self::Admin { command } => command.command_id(),
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum PolicyAdminCommand {
    Show {
        uid: u32,
    },
    Grant {
        uid: u32,
        action: String,
        #[arg(long, value_enum)]
        effect: PolicyEffectArg,
        #[arg(long)]
        resource: Option<String>,
        #[arg(long)]
        ack_root_equivalent: bool,
    },
    Revoke {
        uid: u32,
        action: String,
        #[arg(long)]
        resource: Option<String>,
    },
    Bundle {
        #[command(subcommand)]
        command: PolicyBundleCommand,
    },
}

impl PolicyAdminCommand {
    #[must_use]
    pub const fn command_id(&self) -> &'static str {
        match self {
            Self::Show { .. } => "policy.admin.show",
            Self::Grant { .. } => "policy.admin.grant",
            Self::Revoke { .. } => "policy.admin.revoke",
            Self::Bundle { command } => command.command_id(),
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum PolicyBundleCommand {
    Set {
        uid: u32,
        bundle_id: String,
        #[arg(
            long,
            conflicts_with = "disabled",
            required_unless_present = "disabled"
        )]
        enabled: bool,
        #[arg(long, conflicts_with = "enabled", required_unless_present = "enabled")]
        disabled: bool,
    },
}

impl PolicyBundleCommand {
    #[must_use]
    pub const fn command_id(&self) -> &'static str {
        match self {
            Self::Set { .. } => "policy.admin.bundle.set",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum PolicyEffectArg {
    Allow,
    Prompt,
    Reject,
}
impl PolicyEffectArg {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Prompt => "prompt",
            Self::Reject => "reject",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum DoctorDomain {
    Runtime,
    State,
    Index,
    Providers,
    Codex,
}

#[derive(Debug, Subcommand)]
pub enum IndexCommand {
    Apps(PaginationArgs),
    Windows(PaginationArgs),
    Workspaces(PaginationArgs),
    Displays(PaginationArgs),
    Providers(PaginationArgs),
    Stale(PaginationArgs),
    Query(IndexQueryArgs),
    Show {
        resource_ref: String,
    },
    Topology {
        resource_ref: String,
        #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=6))]
        depth: u8,
        #[arg(long, default_value_t = 100, value_parser = parse_page_limit)]
        limit: u16,
    },
    Refresh {
        resource_ref: String,
    },
    Rescan {
        #[arg(value_enum)]
        domain: IndexRescanArg,
    },
    Reconcile,
    Rebuild,
    Status,
}

impl IndexCommand {
    #[must_use]
    pub const fn command_id(&self) -> &'static str {
        match self {
            Self::Apps(_) => "index.apps",
            Self::Windows(_) => "index.windows",
            Self::Workspaces(_) => "index.workspaces",
            Self::Displays(_) => "index.displays",
            Self::Providers(_) => "index.providers",
            Self::Stale(_) => "index.stale",
            Self::Query(_) => "index.query",
            Self::Show { .. } => "index.show",
            Self::Topology { .. } => "index.topology",
            Self::Refresh { .. } => "index.refresh",
            Self::Rescan { .. } => "index.rescan",
            Self::Reconcile => "index.reconcile",
            Self::Rebuild => "index.rebuild",
            Self::Status => "index.status",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum IndexRescanArg {
    Applications,
    Runtime,
    Providers,
    Services,
}

impl IndexRescanArg {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Applications => "applications",
            Self::Runtime => "runtime",
            Self::Providers => "providers",
            Self::Services => "services",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum IndexResourceArg {
    ApplicationDefinition,
    ApplicationInstance,
    Process,
    OpenrcService,
    Window,
    Workspace,
    Display,
    ProviderRegistration,
    ProviderResource,
    RegisteredCapability,
}

impl IndexResourceArg {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::ApplicationDefinition => "application_definition",
            Self::ApplicationInstance => "application_instance",
            Self::Process => "process",
            Self::OpenrcService => "openrc_service",
            Self::Window => "window",
            Self::Workspace => "workspace",
            Self::Display => "display",
            Self::ProviderRegistration => "provider_registration",
            Self::ProviderResource => "provider_resource",
            Self::RegisteredCapability => "registered_capability",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum IndexFreshnessArg {
    Live,
    Recent,
    Stale,
    Unavailable,
    Historical,
}

impl IndexFreshnessArg {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Recent => "recent",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
            Self::Historical => "historical",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum IndexSourceArg {
    Applications,
    Proc,
    Openrc,
    X11,
    I3,
    Providers,
    Correlation,
}

impl IndexSourceArg {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Applications => "applications",
            Self::Proc => "proc",
            Self::Openrc => "openrc",
            Self::X11 => "x11",
            Self::I3 => "i3",
            Self::Providers => "providers",
            Self::Correlation => "correlation",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum IndexEvidenceArg {
    Authoritative,
    Strong,
    Heuristic,
}

impl IndexEvidenceArg {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Authoritative => "authoritative",
            Self::Strong => "strong",
            Self::Heuristic => "heuristic",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum IndexControlPathArg {
    RegisteredProvider,
    StructuredApi,
    StructuredCli,
    ApplicationAdapter,
    NativeSystem,
    Accessibility,
    ProcessWindow,
    VisualFallback,
}

impl IndexControlPathArg {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::RegisteredProvider => "registered_provider",
            Self::StructuredApi => "structured_api",
            Self::StructuredCli => "structured_cli",
            Self::ApplicationAdapter => "application_adapter",
            Self::NativeSystem => "native_system",
            Self::Accessibility => "accessibility",
            Self::ProcessWindow => "process_window",
            Self::VisualFallback => "visual_fallback",
        }
    }
}

#[derive(Clone, Debug, Args)]
pub struct IndexQueryArgs {
    #[arg(long = "type", value_enum)]
    pub resource_type: Option<IndexResourceArg>,
    #[arg(long, value_enum)]
    pub freshness: Option<IndexFreshnessArg>,
    #[arg(long = "source", value_enum)]
    pub source_kind: Option<IndexSourceArg>,
    #[arg(long)]
    pub application: Option<String>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub capability: Option<String>,
    #[arg(long)]
    pub workspace: Option<String>,
    #[arg(long)]
    pub display: Option<String>,
    #[arg(long, value_enum)]
    pub evidence: Option<IndexEvidenceArg>,
    #[arg(long)]
    pub changed_since_ms: Option<i64>,
    #[arg(long, value_enum)]
    pub control_path: Option<IndexControlPathArg>,
    #[command(flatten)]
    pub pagination: PaginationArgs,
}

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    List(TaskListArgs),
    Show {
        task_id: TaskId,
    },
    Events {
        task_id: TaskId,
        #[arg(long)]
        after: Option<u64>,
        #[arg(long, default_value_t = 100, value_parser = parse_page_limit)]
        limit: u16,
        #[arg(long)]
        follow: bool,
    },
    Cancel {
        task_id: TaskId,
        #[arg(long = "if-state", value_enum)]
        if_state: Option<TaskStateArg>,
    },
}

impl TaskCommand {
    #[must_use]
    pub const fn command_id(&self) -> &'static str {
        match self {
            Self::List(_) => "task.list",
            Self::Show { .. } => "task.show",
            Self::Events { .. } => "task.events",
            Self::Cancel { .. } => "task.cancel",
        }
    }
}

#[derive(Clone, Debug, Args)]
pub struct TaskListArgs {
    #[arg(long, value_enum)]
    pub state: Option<TaskStateArg>,
    #[arg(long)]
    pub project: Option<String>,
    #[command(flatten)]
    pub pagination: PaginationArgs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum TaskStateArg {
    Created,
    Queued,
    Starting,
    Running,
    Waiting,
    Paused,
    Reconciling,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl TaskStateArg {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Queued => "queued",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Paused => "paused",
            Self::Reconciling => "reconciling",
            Self::Cancelling => "cancelling",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum CapabilityCommand {
    List(PaginationArgs),
    Show {
        capability_id: String,
    },
    Provider {
        #[command(subcommand)]
        command: CapabilityProviderCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum CapabilityProviderCommand {
    List(PaginationArgs),
    Show { provider_id: ProviderRegistrationId },
}

#[derive(Debug, Subcommand)]
pub enum ArtifactCommand {
    List(PaginationArgs),
    Show { artifact_id: ArtifactId },
}

impl ArtifactCommand {
    #[must_use]
    pub const fn command_id(&self) -> &'static str {
        match self {
            Self::List(_) => "artifact.list",
            Self::Show { .. } => "artifact.show",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum HealthCommand {
    Show { component_ref: String },
    Degraded,
}

#[derive(Clone, Debug, Args)]
pub struct PaginationArgs {
    #[arg(long, default_value_t = DEFAULT_PAGE_LIMIT, value_parser = parse_page_limit)]
    pub limit: u16,
    #[arg(long)]
    pub cursor: Option<String>,
}

fn parse_page_limit(value: &str) -> Result<u16, String> {
    let parsed = value
        .parse::<u16>()
        .map_err(|_| "limit must be an integer".to_string())?;
    if parsed == 0 || parsed > MAX_PAGE_LIMIT {
        return Err(format!("limit must be between 1 and {MAX_PAGE_LIMIT}"));
    }
    Ok(parsed)
}

pub fn parse_from<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Cli::try_parse_from(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn json_and_jsonl_are_mutually_exclusive() {
        let error = parse_from(["portus-os", "status", "--json", "--jsonl"]).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn timeout_is_bounded() {
        assert!(parse_from(["portus-os", "status", "--timeout-ms", "99"]).is_err());
        assert!(parse_from(["portus-os", "status", "--timeout-ms", "300000"]).is_ok());
        assert!(parse_from(["portus-os", "status", "--timeout-ms", "300001"]).is_err());
    }

    #[test]
    fn pagination_primitive_is_bounded() {
        assert_eq!(parse_page_limit("1").unwrap(), 1);
        assert_eq!(parse_page_limit("200").unwrap(), MAX_PAGE_LIMIT);
        assert!(parse_page_limit("0").is_err());
        assert!(parse_page_limit("201").is_err());
    }

    #[test]
    fn locked_top_level_commands_are_reserved() {
        let clap = Cli::command();
        let names = clap
            .get_subcommands()
            .map(|subcommand| subcommand.get_name())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "status",
                "doctor",
                "index",
                "task",
                "capability",
                "policy",
                "artifact",
                "health",
                "help",
                "version"
            ]
        );
    }

    #[test]
    fn p12_artifact_read_surface_parses_without_mutation_commands() {
        let artifact_id = ArtifactId::new().to_string();
        assert!(parse_from(["portus-os", "artifact", "list"]).is_ok());
        assert!(parse_from(["portus-os", "artifact", "list", "--limit", "200",]).is_ok());
        assert!(parse_from(["portus-os", "artifact", "show", &artifact_id]).is_ok());
        assert!(parse_from(["portus-os", "artifact", "register", "/tmp/file"]).is_err());
        assert!(parse_from(["portus-os", "artifact", "forget", &artifact_id]).is_err());
        assert!(parse_from(["portus-os", "artifact", "delete", &artifact_id]).is_err());
        assert!(parse_from(["portus-os", "artifact", "cat", &artifact_id]).is_err());
    }

    #[test]
    fn p11_health_and_doctor_bundle_forms_parse() {
        assert!(parse_from(["portus-os", "health"]).is_ok());
        assert!(parse_from(["portus-os", "health", "show", "runtime:portusd"]).is_ok());
        assert!(parse_from(["portus-os", "health", "degraded"]).is_ok());
        assert!(parse_from(["portus-os", "doctor", "--bundle", "doctor.json"]).is_ok());
        assert!(parse_from(["portus-os", "doctor", "state", "--bundle", "doctor.json"]).is_ok());
    }

    #[test]
    fn p7_task_command_tree_parses_without_create_pause_or_retry() {
        let task_id = TaskId::new().to_string();
        assert!(parse_from(["portus-os", "task", "list"]).is_ok());
        assert!(
            parse_from([
                "portus-os",
                "task",
                "list",
                "--state",
                "running",
                "--project",
                "project:demo",
                "--limit",
                "200",
            ])
            .is_ok()
        );
        assert!(parse_from(["portus-os", "task", "show", &task_id]).is_ok());
        assert!(
            parse_from([
                "portus-os",
                "task",
                "events",
                &task_id,
                "--after",
                "4",
                "--limit",
                "100",
            ])
            .is_ok()
        );
        assert!(parse_from(["portus-os", "task", "events", &task_id, "--follow"]).is_ok());
        assert!(
            parse_from([
                "portus-os",
                "task",
                "cancel",
                &task_id,
                "--if-state",
                "running",
            ])
            .is_ok()
        );
        assert!(parse_from(["portus-os", "task", "create"]).is_err());
        assert!(parse_from(["portus-os", "task", "pause", &task_id]).is_err());
        assert!(parse_from(["portus-os", "task", "retry", &task_id]).is_err());
    }

    #[test]
    fn p9_policy_command_tree_parses_without_generic_privilege_surface() {
        assert!(parse_from(["portus-os", "policy", "effective"]).is_ok());
        assert!(
            parse_from([
                "portus-os",
                "policy",
                "check",
                "service.restart",
                "--resource",
                "portusd",
            ])
            .is_ok()
        );
        assert!(parse_from(["portus-os", "policy", "admin", "show", "1000"]).is_ok());
        assert!(
            parse_from([
                "portus-os",
                "policy",
                "admin",
                "grant",
                "1000",
                "service.restart",
                "--effect",
                "allow",
                "--resource",
                "portusd",
            ])
            .is_ok()
        );
        assert!(
            parse_from([
                "portus-os",
                "policy",
                "admin",
                "grant",
                "1000",
                "root.shell",
                "--effect",
                "allow",
                "--ack-root-equivalent",
            ])
            .is_ok()
        );
        assert!(
            parse_from([
                "portus-os",
                "policy",
                "admin",
                "revoke",
                "1000",
                "service.restart",
                "--resource",
                "portusd",
            ])
            .is_ok()
        );
        assert!(
            parse_from([
                "portus-os",
                "policy",
                "admin",
                "bundle",
                "set",
                "1000",
                "system-administration",
                "--enabled",
            ])
            .is_ok()
        );
        assert!(parse_from(["portus-os", "policy", "exec"]).is_err());
        assert!(parse_from(["portus-os", "policy", "shell"]).is_err());
        assert!(parse_from(["portus-os", "policy", "admin", "exec"]).is_err());
    }

    #[test]
    fn capability_locked_forms_parse_without_invoke_surface() {
        assert!(parse_from(["portus-os", "capability", "list"]).is_ok());
        assert!(parse_from(["portus-os", "capability", "show", "browser.control"]).is_ok());
        assert!(parse_from(["portus-os", "capability", "provider", "list"]).is_ok());
        let provider_id = ProviderRegistrationId::new().to_string();
        assert!(parse_from(["portus-os", "capability", "provider", "show", &provider_id]).is_ok());
        assert!(parse_from(["portus-os", "capability", "invoke"]).is_err());
    }

    #[test]
    fn p6_index_command_tree_and_bounds_parse() {
        assert!(parse_from(["portus-os", "index", "apps"]).is_ok());
        assert!(parse_from(["portus-os", "index", "windows", "--limit", "200"]).is_ok());
        assert!(
            parse_from([
                "portus-os",
                "index",
                "query",
                "--type",
                "process",
                "--freshness",
                "recent",
                "--source",
                "proc",
                "--control-path",
                "native-system",
            ])
            .is_ok()
        );
        assert!(
            parse_from(["portus-os", "index", "query", "--type", "provider-resource",]).is_ok()
        );
        assert!(parse_from(["portus-os", "index", "topology", "idx_bad", "--depth", "7"]).is_err());
        assert!(parse_from(["portus-os", "index", "rescan", "filesystem"]).is_err());
        assert!(parse_from(["portus-os", "index"]).is_err());
    }

    #[test]
    fn health_locked_forms_parse() {
        assert!(parse_from(["portus-os", "health"]).is_ok());
        assert!(parse_from(["portus-os", "health", "degraded"]).is_ok());
        assert!(parse_from(["portus-os", "health", "show", "portusd"]).is_ok());
    }
}
