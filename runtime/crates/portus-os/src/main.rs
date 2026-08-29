use portus_os::{DoctorContext, SystemPrivilege, SystemRuntime, run_to_writers_with_privilege};
use std::{
    io::{self, Write},
    process::ExitCode,
};

fn main() -> ExitCode {
    let mut runtime = SystemRuntime::default();
    let mut privilege = SystemPrivilege::default();
    let doctor = DoctorContext::default();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let exit_code = run_to_writers_with_privilege(
        std::env::args_os(),
        &mut runtime,
        &mut privilege,
        &doctor,
        &mut stdout,
        &mut stderr,
    );
    let _ = stdout.flush();
    let _ = stderr.flush();
    ExitCode::from(exit_code)
}
