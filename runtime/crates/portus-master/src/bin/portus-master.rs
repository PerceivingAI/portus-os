use portus_master::{
    LaunchContext, MasterMode, SystemCommandRunner, parse_master_from, run_master,
};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = match parse_master_from(std::env::args_os()) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(64);
        }
    };
    let mode = cli.command.map(MasterMode::from).unwrap_or_default();
    let context = match LaunchContext::system() {
        Ok(context) => context,
        Err(error) => return report(error),
    };
    let mut runner = SystemCommandRunner;
    match run_master(&context, mode, &mut runner) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report(error),
    }
}

fn report(error: portus_master::MasterLaunchError) -> ExitCode {
    eprintln!("portus-master: {error}");
    ExitCode::from(error.exit_code())
}
