use clap::{Parser, Subcommand};
use portus_build::{
    BuildError, BuildMetadataInput, CandidateInitInput, EXIT_UNRESOLVED, RedactionLedger,
    ValidationCandidate, ValidationCommandRecord, ValidationReportInput, ValidationResult,
    aggregate_validation_report, append_validation_command, build_metadata_preview, build_plan,
    default_portus_stage_root, initialize_candidate, materialize_validation_harness,
    native_iso_build_gate, record_redaction_ledger, record_validation_result, render_target_config,
    stage_portus_to, validate_w6, validation_harness_check, validation_plan, validation_vm_gate,
    verify_candidate_bundle, verify_validation_harness,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode},
};

#[derive(Debug, Parser)]
#[command(
    name = "portus-build",
    version,
    about = "PortusOS W6 host-safe build skeleton"
)]
struct Cli {
    #[arg(long, default_value = ".", global = true)]
    repo_root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate,
    Plan {
        #[arg(long)]
        disk_size_mib: u64,
    },
    StagePortus {
        #[arg(long)]
        binary_dir: PathBuf,
        #[arg(long)]
        target_root: Option<PathBuf>,
    },
    Render {
        #[arg(long)]
        identifiers_json: PathBuf,
    },
    ValidationPlan {
        #[arg(long)]
        candidate_id: String,
        #[arg(long)]
        iso_sha256: String,
    },
    ValidationHarnessCheck,
    ValidationMaterialize {
        #[arg(long)]
        candidate_json: PathBuf,
        #[arg(long)]
        output_root: PathBuf,
    },
    ValidationAction {
        #[arg(long)]
        candidate_root: PathBuf,
        #[arg(long)]
        test_id: String,
        #[arg(long)]
        record_json: PathBuf,
    },
    ValidationRecord {
        #[arg(long)]
        candidate_root: PathBuf,
        #[arg(long)]
        result_json: PathBuf,
    },
    ValidationRedactions {
        #[arg(long)]
        candidate_root: PathBuf,
        #[arg(long)]
        input_json: PathBuf,
    },
    ValidationReport {
        #[arg(long)]
        candidate_root: PathBuf,
        #[arg(long)]
        input_json: PathBuf,
    },
    ValidationVerify {
        #[arg(long)]
        candidate_root: PathBuf,
    },
    ValidationVmRun,
    CandidateInit {
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        input_json: PathBuf,
    },
    CandidateVerify {
        #[arg(long)]
        candidate_root: PathBuf,
    },
    Metadata {
        #[arg(long)]
        input_json: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
    },
    BuildIso {
        #[arg(long)]
        release_candidate: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match execute(cli) {
        Ok(output) => {
            if let Some(output) = output {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(BuildError::Unresolved(message)) => {
            eprintln!("{message}");
            ExitCode::from(EXIT_UNRESOLVED)
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: Cli) -> Result<Option<String>, BuildError> {
    let repo = cli.repo_root;
    match cli.command {
        Command::Validate => {
            let report = validate_w6(&repo)?;
            Ok(Some(to_json(&report)?))
        }
        Command::Plan { disk_size_mib } => Ok(Some(to_json(&build_plan(&repo, disk_size_mib)?)?)),
        Command::StagePortus {
            binary_dir,
            target_root,
        } => {
            let target = match target_root {
                Some(target) => target,
                None => default_portus_stage_root(&repo)?,
            };
            let report = stage_portus_to(&repo, &binary_dir, &target)?;
            Ok(Some(to_json(&serde_json::json!({
                "staging_root": target,
                "created": report.created,
                "replaced": report.replaced,
                "unchanged": report.unchanged,
                "preserved_modified": report.preserved_modified,
                "unresolved_linux_items": report.unresolved_linux_items
            }))?))
        }
        Command::Render { identifiers_json } => {
            let input: portus_build::TargetIdentifiers =
                serde_json::from_str(&fs::read_to_string(identifiers_json)?).map_err(|error| {
                    BuildError::Invalid(format!("invalid target identifiers JSON: {error}"))
                })?;
            Ok(Some(to_json(&render_target_config(&repo, &input)?)?))
        }
        Command::ValidationPlan {
            candidate_id,
            iso_sha256,
        } => Ok(Some(to_json(&validation_plan(
            &repo,
            &candidate_id,
            &iso_sha256,
        )?)?)),
        Command::ValidationHarnessCheck => Ok(Some(to_json(&validation_harness_check(&repo)?)?)),
        Command::ValidationMaterialize {
            candidate_json,
            output_root,
        } => {
            let candidate: ValidationCandidate = read_json_input(&candidate_json, "candidate")?;
            Ok(Some(to_json(&materialize_validation_harness(
                &repo,
                &output_root,
                &candidate,
            )?)?))
        }
        Command::ValidationAction {
            candidate_root,
            test_id,
            record_json,
        } => {
            let record: ValidationCommandRecord = read_json_input(&record_json, "command record")?;
            append_validation_command(&repo, &candidate_root, &test_id, &record)?;
            Ok(None)
        }
        Command::ValidationRecord {
            candidate_root,
            result_json,
        } => {
            let result: ValidationResult = read_json_input(&result_json, "validation result")?;
            record_validation_result(&repo, &candidate_root, &result)?;
            Ok(None)
        }
        Command::ValidationRedactions {
            candidate_root,
            input_json,
        } => {
            let ledger: RedactionLedger = read_json_input(&input_json, "redaction ledger")?;
            record_redaction_ledger(&candidate_root, &ledger)?;
            Ok(None)
        }
        Command::ValidationReport {
            candidate_root,
            input_json,
        } => {
            let input: ValidationReportInput =
                read_json_input(&input_json, "validation report input")?;
            Ok(Some(to_json(&aggregate_validation_report(
                &repo,
                &candidate_root,
                &input,
            )?)?))
        }
        Command::ValidationVerify { candidate_root } => Ok(Some(to_json(
            &verify_validation_harness(&repo, &candidate_root)?,
        )?)),
        Command::ValidationVmRun => {
            validation_vm_gate()?;
            Ok(None)
        }
        Command::CandidateInit {
            artifact,
            input_json,
        } => {
            let input: CandidateInitInput = read_json_input(&input_json, "candidate init input")?;
            Ok(Some(to_json(&initialize_candidate(
                &repo, &artifact, &input,
            )?)?))
        }
        Command::CandidateVerify { candidate_root } => Ok(Some(to_json(
            &verify_candidate_bundle(&repo, &candidate_root)?,
        )?)),
        Command::Metadata {
            input_json,
            artifact,
        } => {
            let input: BuildMetadataInput = read_json_input(&input_json, "build metadata input")?;
            Ok(Some(to_json(&build_metadata_preview(
                &repo, &artifact, &input,
            )?)?))
        }
        Command::BuildIso { release_candidate } => {
            native_iso_build_gate(&repo, release_candidate)?;
            execute_native_iso(&repo)?;
            Ok(None)
        }
    }
}

fn execute_native_iso(repo: &Path) -> Result<(), BuildError> {
    let manifest = env::var_os("PORTUS_BUILD_STAGING_MANIFEST")
        .map(PathBuf::from)
        .ok_or_else(|| {
            BuildError::Unresolved(
                "native ISO construction requires the run-owned PORTUS_BUILD_STAGING_MANIFEST"
                    .to_string(),
            )
        })?;
    if !manifest.is_file() {
        return Err(BuildError::Unresolved(format!(
            "native ISO staging manifest is missing: {}",
            manifest.display()
        )));
    }

    let sudo_check = ProcessCommand::new("sudo").args(["-n", "-v"]).status();
    match sudo_check {
        Ok(status) if status.success() => {}
        Ok(_) => {
            return Err(BuildError::Unresolved(
                "native ISO construction needs an owner-authorized sudo ticket; run `sudo -v` in the VM terminal, then rerun the same canonical build command"
                    .to_string(),
            ));
        }
        Err(error) => {
            return Err(BuildError::Unresolved(format!(
                "native ISO construction cannot invoke sudo: {error}"
            )));
        }
    }

    let status = ProcessCommand::new("sudo")
        .arg("-n")
        .arg("python")
        .arg("-B")
        .arg(repo.join("scripts/artix/context.py"))
        .arg("build-iso")
        .arg("--manifest")
        .arg(&manifest)
        .current_dir(repo)
        .status()
        .map_err(|error| {
            BuildError::Invalid(format!(
                "failed to start private Artix native helper: {error}"
            ))
        })?;
    if !status.success() {
        return Err(BuildError::Invalid(format!(
            "private Artix native helper failed with exit {}",
            status.code().unwrap_or(1)
        )));
    }
    Ok(())
}

fn read_json_input<T: serde::de::DeserializeOwned>(
    path: &PathBuf,
    label: &str,
) -> Result<T, BuildError> {
    let contents = fs::read_to_string(path)?;
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(&contents);
    serde_json::from_str(contents)
        .map_err(|error| BuildError::Invalid(format!("invalid {label} JSON: {error}")))
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String, BuildError> {
    serde_json::to_string_pretty(value)
        .map_err(|error| BuildError::Invalid(format!("cannot serialize W6 output: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn json_input_accepts_optional_utf8_bom_without_relaxing_shape() {
        let path = std::env::temp_dir().join(format!(
            "portus-build-json-bom-{}-{}.json",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, "\u{feff}{\"schema_version\":1}").unwrap();
        let value: serde_json::Value = read_json_input(&path, "fixture").unwrap();
        assert_eq!(value["schema_version"], 1);
        fs::remove_file(path).unwrap();
    }
}
