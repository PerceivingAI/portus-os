use crate::MasterMode;
use clap::{Parser, Subcommand};
use std::ffi::OsString;

#[derive(Debug, Parser)]
#[command(name = "portus-bootstrap", disable_version_flag = true)]
pub struct BootstrapCli {}

#[derive(Debug, Parser)]
#[command(name = "portus-master", disable_version_flag = true)]
pub struct MasterCli {
    #[command(subcommand)]
    pub command: Option<MasterCommand>,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum MasterCommand {
    /// Start a new interactive Master Codex session.
    New,
    /// Open Codex's session picker scoped to the Master workspace.
    Resume,
    /// Explicitly resume the latest Codex session in the Master workspace.
    ResumeLast,
    /// Explicitly resume a known Codex session ID or name.
    ResumeId { session: String },
    /// Stay in an interactive user shell without starting Codex.
    Shell,
    /// Run Codex and PortusOS Codex diagnostics.
    Doctor,
}

impl From<MasterCommand> for MasterMode {
    fn from(value: MasterCommand) -> Self {
        match value {
            MasterCommand::New => Self::New,
            MasterCommand::Resume => Self::Resume,
            MasterCommand::ResumeLast => Self::ResumeLast,
            MasterCommand::ResumeId { session } => Self::ResumeId(session),
            MasterCommand::Shell => Self::Shell,
            MasterCommand::Doctor => Self::Doctor,
        }
    }
}

pub fn parse_bootstrap_from<I, T>(args: I) -> Result<BootstrapCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    BootstrapCli::try_parse_from(args)
}

pub fn parse_master_from<I, T>(args: I) -> Result<MasterCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    MasterCli::try_parse_from(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_rejects_undeclared_arguments() {
        assert!(parse_bootstrap_from(["portus-bootstrap"]).is_ok());
        assert!(parse_bootstrap_from(["portus-bootstrap", "resume-last"]).is_err());
    }

    #[test]
    fn master_modes_parse_and_default_is_left_to_launcher() {
        let cli = parse_master_from(["portus-master"]).unwrap();
        assert!(cli.command.is_none());
        assert_eq!(
            parse_master_from(["portus-master", "resume-last"])
                .unwrap()
                .command,
            Some(MasterCommand::ResumeLast)
        );
        assert_eq!(
            parse_master_from(["portus-master", "resume-id", "thread-123"])
                .unwrap()
                .command,
            Some(MasterCommand::ResumeId {
                session: "thread-123".into()
            })
        );
    }
}
