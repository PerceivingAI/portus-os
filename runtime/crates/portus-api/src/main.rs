use portus_api::{SystemApiTransport, run_with_io};
use std::{
    io::{self, Write},
    process::ExitCode,
};

fn main() -> ExitCode {
    let mut transport = SystemApiTransport::default();
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let code = run_with_io(
        std::env::args_os(),
        &mut transport,
        &mut stdin,
        &mut stdout,
        &mut stderr,
    );
    let _ = stdout.flush();
    let _ = stderr.flush();
    ExitCode::from(code)
}
