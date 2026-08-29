use clap::{Parser, Subcommand};
use std::ffi::OsString;

#[derive(Debug, Parser)]
#[command(
    name = "portus-auth",
    version,
    about = "Administrator protected-credential provisioning",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true, default_value_t = 30_000, value_parser = clap::value_parser!(u64).range(100..=120_000))]
    pub timeout_ms: u64,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    ProtectedApi {
        #[command(subcommand)]
        command: ProtectedApiCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProtectedApiCommand {
    Provision {
        credential_ref: String,
        #[arg(long)]
        provider: String,
        #[arg(long)]
        label: Option<String>,
    },
    Rotate {
        credential_ref: String,
    },
    Revoke {
        credential_ref: String,
    },
    Delete {
        credential_ref: String,
    },
    Show {
        credential_ref: String,
    },
    List,
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
    fn protected_api_admin_surface_has_no_value_or_export_argument() {
        assert!(
            parse_from([
                "portus-auth",
                "protected-api",
                "provision",
                "openai/main",
                "--provider",
                "openai"
            ])
            .is_ok()
        );
        assert!(parse_from(["portus-auth", "protected-api", "rotate", "openai/main"]).is_ok());
        assert!(parse_from(["portus-auth", "protected-api", "revoke", "openai/main"]).is_ok());
        assert!(parse_from(["portus-auth", "protected-api", "delete", "openai/main"]).is_ok());
        assert!(parse_from(["portus-auth", "protected-api", "show", "openai/main"]).is_ok());
        assert!(parse_from(["portus-auth", "protected-api", "list"]).is_ok());
        assert!(parse_from(["portus-auth", "protected-api", "export", "openai/main"]).is_err());
    }
}
