use clap::{Parser, Subcommand};
use std::ffi::OsString;

#[derive(Debug, Parser)]
#[command(
    name = "portus-api",
    version,
    about = "Protected API provider client",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true, default_value_t = 120_000, value_parser = clap::value_parser!(u64).range(100..=120_000))]
    pub timeout_ms: u64,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Credential {
        #[command(subcommand)]
        command: CredentialCommand,
    },
    Request {
        credential_ref: String,
        operation: String,
        #[arg(long, default_value = "-")]
        input: String,
    },
    Health,
}

#[derive(Debug, Subcommand)]
pub enum CredentialCommand {
    List,
    Show { credential_ref: String },
}

pub fn parse_from<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    Cli::try_parse_from(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_client_surface_parses_without_admin_surface() {
        assert!(parse_from(["portus-api", "credential", "list"]).is_ok());
        assert!(parse_from(["portus-api", "credential", "show", "openai/main"]).is_ok());
        assert!(
            parse_from([
                "portus-api",
                "request",
                "openai/main",
                "openai.responses.create"
            ])
            .is_ok()
        );
        assert!(parse_from(["portus-api", "health"]).is_ok());
        for command in ["export", "reveal", "provision", "rotate", "revoke"] {
            assert!(parse_from(["portus-api", command]).is_err());
        }
    }
}
