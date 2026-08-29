use portus_master::{LaunchContext, SystemCommandRunner, parse_bootstrap_from, run_bootstrap};
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(error) = parse_bootstrap_from(std::env::args_os()) {
        let _ = error.print();
        return ExitCode::from(64);
    }
    let context = match LaunchContext::system() {
        Ok(context) => context,
        Err(error) => return report(error),
    };
    let mut runner = SystemCommandRunner;
    match run_bootstrap(&context, &mut runner) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => report(error),
    }
}

fn report(error: portus_master::MasterLaunchError) -> ExitCode {
    eprintln!("portus-bootstrap: {error}");
    ExitCode::from(error.exit_code())
}
