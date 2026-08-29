//! Thin Master/Codex client for the protected API provider.

mod app;
mod args;
mod transport;

pub use app::{RenderedOutput, run_from, run_with_io};
pub use args::{Cli, Command, CredentialCommand, parse_from};
pub use transport::{ApiTransport, SystemApiTransport, TransportError};
