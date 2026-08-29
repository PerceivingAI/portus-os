use portus_build_contract::validate_repository;
use std::{env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let mut repo_root = PathBuf::from(".");
    let mut repo_root_set = false;
    let mut require_release_resolved = false;

    for argument in env::args().skip(1) {
        if argument == "--release-resolved" {
            require_release_resolved = true;
        } else if !repo_root_set {
            repo_root = PathBuf::from(argument);
            repo_root_set = true;
        } else {
            eprintln!("usage: portus-build-contract [repo-root] [--release-resolved]");
            return ExitCode::from(64);
        }
    }

    match validate_repository(&repo_root) {
        Ok(report) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("report serialization must succeed")
            );
            if require_release_resolved && !report.release_resolved {
                ExitCode::from(78)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
