use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    use portus_auth::{SystemAdminTransport, SystemSecretReader, run_with};
    use std::io::{self, Write};
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("portus-auth protected credential administration requires root");
        return ExitCode::from(5);
    }
    let mut transport = SystemAdminTransport::default();
    let mut secrets = SystemSecretReader;
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let code = run_with(
        std::env::args_os(),
        &mut transport,
        &mut secrets,
        &mut stdout,
        &mut stderr,
    );
    let _ = stdout.flush();
    let _ = stderr.flush();
    ExitCode::from(code)
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!("portus-auth protected credential provisioning requires Linux/root admin IPC");
    ExitCode::from(78)
}
