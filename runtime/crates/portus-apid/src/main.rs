use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    use portus_apid::{ProtectedApiServer, ProtectedApiServerConfig};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let shutdown = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&shutdown);
    if let Err(error) = ctrlc::set_handler(move || signal.store(true, Ordering::Release)) {
        eprintln!("portus-apid failed to install shutdown handler: {error}");
        return ExitCode::from(1);
    }
    let server = match ProtectedApiServer::bind(ProtectedApiServerConfig::canonical()) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("portus-apid startup failed: {error}");
            return ExitCode::from(1);
        }
    };
    match server.run_until(shutdown) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("portus-apid runtime failed: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!(
        "portus-apid requires Linux and exposes no substitute protected-credential boundary on this host"
    );
    ExitCode::from(78)
}
