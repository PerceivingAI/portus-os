//! Root-oriented AUTH provisioning client for protected API credentials.

mod app;
mod args;
mod secret_input;
mod transport;

pub use app::{RenderedOutput, run_from, run_with};
pub use args::{Cli, Command, ProtectedApiCommand, parse_from};
pub use secret_input::{SecretReader, SystemSecretReader};
pub use transport::{AdminTransport, SystemAdminTransport, TransportError};
