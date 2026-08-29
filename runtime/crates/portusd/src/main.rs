use portusd::RuntimeConfig;
use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    use portusd::RuntimeServer;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let shutdown = Arc::new(AtomicBool::new(false));
    let signal_shutdown = Arc::clone(&shutdown);
    if let Err(error) = ctrlc::set_handler(move || {
        signal_shutdown.store(true, Ordering::Release);
    }) {
        eprintln!("portusd startup failed: unable to install shutdown handler: {error}");
        return ExitCode::FAILURE;
    }

    let server = match RuntimeServer::bind(RuntimeConfig::canonical()) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("portusd startup failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    match server.run_until(shutdown) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("portusd stopped with error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    let _ = RuntimeConfig::canonical();
    eprintln!("portusd requires a Unix target with Unix-domain socket peer credentials");
    ExitCode::from(78)
}
