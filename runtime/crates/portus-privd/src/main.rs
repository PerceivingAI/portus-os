use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    use portus_privd::{PrivilegeServer, PrivilegeServerConfig};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let shutdown = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&shutdown);
    if let Err(error) = ctrlc::set_handler(move || signal.store(true, Ordering::Release)) {
        eprintln!("portus-privd failed to install shutdown handler: {error}");
        return ExitCode::from(1);
    }
    let server = match PrivilegeServer::bind(PrivilegeServerConfig::canonical()) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("portus-privd startup failed: {error}");
            return ExitCode::from(1);
        }
    };
    match server.run_until(shutdown) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("portus-privd runtime failed: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!(
        "portus-privd requires Linux and exposes no substitute privilege boundary on this host"
    );
    ExitCode::from(78)
}
