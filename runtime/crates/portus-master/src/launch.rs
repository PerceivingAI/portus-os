use crate::{
    CommandRunner, CommandSpec, LaunchContext, MasterLaunchError, MasterLaunchResult,
    prepare_workspace,
};
use std::ffi::OsString;

pub const MASTER_TMUX_SESSION: &str = "MasterPortus";
pub const MASTER_TMUX_WINDOW: &str = "Master";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum MasterMode {
    New,
    #[default]
    Resume,
    ResumeLast,
    ResumeId(String),
    Shell,
    Doctor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapOutcome {
    AttachedExisting,
    CreatedAndAttached,
    AttachedAfterCreateRace,
}

pub fn run_bootstrap(
    context: &LaunchContext,
    runner: &mut dyn CommandRunner,
) -> MasterLaunchResult<BootstrapOutcome> {
    context.validate()?;
    let workspace = prepare_workspace(context)?;
    let has_session = run_command(runner, &tmux_has_session(context, &workspace.path))?;
    if has_session.success {
        require_success(runner, &tmux_attach(context, &workspace.path))?;
        return Ok(BootstrapOutcome::AttachedExisting);
    }
    if has_session.code != Some(1) {
        return Err(MasterLaunchError::ProcessFailed {
            program: context.layout.tmux_program.display().to_string(),
            code: has_session.code,
        });
    }

    let created = run_command(runner, &tmux_create(context, &workspace.path))?;
    if created.success {
        require_success(runner, &tmux_attach(context, &workspace.path))?;
        return Ok(BootstrapOutcome::CreatedAndAttached);
    }

    let after_race = run_command(runner, &tmux_has_session(context, &workspace.path))?;
    if after_race.success {
        require_success(runner, &tmux_attach(context, &workspace.path))?;
        return Ok(BootstrapOutcome::AttachedAfterCreateRace);
    }

    Err(MasterLaunchError::ProcessFailed {
        program: context.layout.tmux_program.display().to_string(),
        code: created.code,
    })
}

pub fn run_master(
    context: &LaunchContext,
    mode: MasterMode,
    runner: &mut dyn CommandRunner,
) -> MasterLaunchResult<()> {
    context.validate()?;
    let workspace = prepare_workspace(context)?;
    match mode {
        MasterMode::Doctor => run_doctor(context, runner, &workspace.path),
        other => {
            let command = master_command(context, other, &workspace.path)?;
            require_success(runner, &command)
        }
    }
}

fn run_doctor(
    context: &LaunchContext,
    runner: &mut dyn CommandRunner,
    workspace: &std::path::Path,
) -> MasterLaunchResult<()> {
    let codex = CommandSpec::new(context.layout.codex_program.as_os_str(), workspace)
        .with_args([OsString::from("doctor")]);
    let portus = CommandSpec::new(context.layout.portus_os_program.as_os_str(), workspace)
        .with_args([OsString::from("doctor"), OsString::from("codex")]);
    let codex_result = run_command(runner, &codex);
    let portus_result = run_command(runner, &portus);
    match codex_result {
        Err(error) => Err(error),
        Ok(outcome) if !outcome.success => Err(MasterLaunchError::ProcessFailed {
            program: codex.program_display(),
            code: outcome.code,
        }),
        Ok(_) => match portus_result {
            Err(error) => Err(error),
            Ok(outcome) if !outcome.success => Err(MasterLaunchError::ProcessFailed {
                program: portus.program_display(),
                code: outcome.code,
            }),
            Ok(_) => Ok(()),
        },
    }
}

fn master_command(
    context: &LaunchContext,
    mode: MasterMode,
    workspace: &std::path::Path,
) -> MasterLaunchResult<CommandSpec> {
    let mut spec = match mode {
        MasterMode::Shell => CommandSpec::new(context.identity.shell.as_os_str(), workspace),
        MasterMode::New => CommandSpec::new(context.layout.codex_program.as_os_str(), workspace),
        MasterMode::Resume => CommandSpec::new(context.layout.codex_program.as_os_str(), workspace)
            .with_args([OsString::from("resume")]),
        MasterMode::ResumeLast => {
            CommandSpec::new(context.layout.codex_program.as_os_str(), workspace)
                .with_args([OsString::from("resume"), OsString::from("--last")])
        }
        MasterMode::ResumeId(session) => {
            validate_resume_target(&session)?;
            CommandSpec::new(context.layout.codex_program.as_os_str(), workspace)
                .with_args([OsString::from("resume"), OsString::from(session)])
        }
        MasterMode::Doctor => unreachable!("doctor is handled separately"),
    };
    spec.current_dir = workspace.to_path_buf();
    Ok(spec)
}

fn validate_resume_target(session: &str) -> MasterLaunchResult<()> {
    if session.is_empty()
        || session.len() > 256
        || session.starts_with('-')
        || session.chars().any(char::is_control)
    {
        return Err(MasterLaunchError::InvalidResumeTarget(
            "session ID/name must be 1-256 printable characters and must not begin with '-'".into(),
        ));
    }
    Ok(())
}

fn tmux_has_session(context: &LaunchContext, workspace: &std::path::Path) -> CommandSpec {
    CommandSpec::new(context.layout.tmux_program.as_os_str(), workspace).with_args([
        OsString::from("has-session"),
        OsString::from("-t"),
        OsString::from(format!("={MASTER_TMUX_SESSION}")),
    ])
}

fn tmux_create(context: &LaunchContext, workspace: &std::path::Path) -> CommandSpec {
    CommandSpec::new(context.layout.tmux_program.as_os_str(), workspace).with_args([
        OsString::from("new-session"),
        OsString::from("-d"),
        OsString::from("-s"),
        OsString::from(MASTER_TMUX_SESSION),
        OsString::from("-n"),
        OsString::from(MASTER_TMUX_WINDOW),
        OsString::from("-c"),
        workspace.as_os_str().to_os_string(),
        context.layout.master_program.as_os_str().to_os_string(),
    ])
}

fn tmux_attach(context: &LaunchContext, workspace: &std::path::Path) -> CommandSpec {
    CommandSpec::new(context.layout.tmux_program.as_os_str(), workspace).with_args([
        OsString::from("attach-session"),
        OsString::from("-t"),
        OsString::from(format!("={MASTER_TMUX_SESSION}")),
    ])
}

fn run_command(
    runner: &mut dyn CommandRunner,
    spec: &CommandSpec,
) -> MasterLaunchResult<crate::CommandOutcome> {
    runner
        .run(spec)
        .map_err(|source| MasterLaunchError::DependencyUnavailable {
            program: spec.program_display(),
            source,
        })
}

fn require_success(runner: &mut dyn CommandRunner, spec: &CommandSpec) -> MasterLaunchResult<()> {
    let result = run_command(runner, spec)?;
    if result.success {
        Ok(())
    } else {
        Err(MasterLaunchError::ProcessFailed {
            program: spec.program_display(),
            code: result.code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandOutcome, LaunchIdentity, LaunchLayout};
    use std::{
        collections::VecDeque,
        fs, io,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    struct FakeRunner {
        outcomes: VecDeque<io::Result<CommandOutcome>>,
        specs: Vec<CommandSpec>,
    }

    impl FakeRunner {
        fn new(outcomes: impl IntoIterator<Item = CommandOutcome>) -> Self {
            Self {
                outcomes: outcomes.into_iter().map(Ok).collect(),
                specs: Vec::new(),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&mut self, spec: &CommandSpec) -> io::Result<CommandOutcome> {
            self.specs.push(spec.clone());
            self.outcomes.pop_front().unwrap_or_else(|| {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "missing fake outcome",
                ))
            })
        }
    }

    fn success() -> CommandOutcome {
        CommandOutcome {
            success: true,
            code: Some(0),
        }
    }

    fn failure(code: i32) -> CommandOutcome {
        CommandOutcome {
            success: false,
            code: Some(code),
        }
    }

    fn fixture() -> (LaunchContext, PathBuf) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "portus-launch-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let context = LaunchContext {
            identity: LaunchIdentity {
                username: "demo".into(),
                uid: None,
                gid: None,
                home: root.join("home"),
                shell: PathBuf::from("test-shell"),
            },
            layout: LaunchLayout {
                workspace_root: root.clone(),
                tmux_program: PathBuf::from("test-tmux"),
                master_program: PathBuf::from("test-portus-master"),
                codex_program: PathBuf::from("test-codex"),
                portus_os_program: PathBuf::from("test-portus-os"),
            },
        };
        (context, root)
    }

    #[test]
    fn bootstrap_attaches_existing_session_without_creating_duplicate() {
        let (context, root) = fixture();
        let mut runner = FakeRunner::new([success(), success()]);
        assert_eq!(
            run_bootstrap(&context, &mut runner).unwrap(),
            BootstrapOutcome::AttachedExisting
        );
        assert_eq!(runner.specs.len(), 2);
        assert_eq!(runner.specs[0].args[0], "has-session");
        assert_eq!(runner.specs[1].args[0], "attach-session");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bootstrap_targets_the_master_session_exactly() {
        let (context, root) = fixture();
        let mut runner = FakeRunner::new([success(), success()]);
        run_bootstrap(&context, &mut runner).unwrap();
        assert_eq!(runner.specs[0].args, ["has-session", "-t", "=MasterPortus"]);
        assert_eq!(
            runner.specs[1].args,
            ["attach-session", "-t", "=MasterPortus"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bootstrap_creates_exact_master_session_then_attaches() {
        let (context, root) = fixture();
        let mut runner = FakeRunner::new([failure(1), success(), success()]);
        assert_eq!(
            run_bootstrap(&context, &mut runner).unwrap(),
            BootstrapOutcome::CreatedAndAttached
        );
        let create = &runner.specs[1];
        assert_eq!(create.args[0], "new-session");
        assert!(create.args.iter().any(|arg| arg == MASTER_TMUX_SESSION));
        assert!(create.args.iter().any(|arg| arg == MASTER_TMUX_WINDOW));
        assert_eq!(create.args.last().unwrap(), "test-portus-master");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bootstrap_recovers_create_race_without_duplicate_session() {
        let (context, root) = fixture();
        let mut runner = FakeRunner::new([failure(1), failure(1), success(), success()]);
        assert_eq!(
            run_bootstrap(&context, &mut runner).unwrap(),
            BootstrapOutcome::AttachedAfterCreateRace
        );
        assert_eq!(runner.specs[2].args[0], "has-session");
        assert_eq!(runner.specs[3].args[0], "attach-session");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn master_modes_construct_argv_without_shell_strings_and_use_workspace() {
        let (context, root) = fixture();
        for (mode, expected) in [
            (MasterMode::New, Vec::<&str>::new()),
            (MasterMode::Resume, vec!["resume"]),
            (MasterMode::ResumeLast, vec!["resume", "--last"]),
            (
                MasterMode::ResumeId("thread-1".into()),
                vec!["resume", "thread-1"],
            ),
        ] {
            let mut runner = FakeRunner::new([success()]);
            run_master(&context, mode, &mut runner).unwrap();
            assert_eq!(runner.specs[0].program, "test-codex");
            assert_eq!(
                runner.specs[0]
                    .args
                    .iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(runner.specs[0].current_dir, context.master_workspace());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_mode_is_scoped_picker_not_automatic_resume_last() {
        let (context, root) = fixture();
        let mut runner = FakeRunner::new([success()]);
        run_master(&context, MasterMode::default(), &mut runner).unwrap();
        assert_eq!(runner.specs[0].args, ["resume"]);
        assert!(!runner.specs[0].args.iter().any(|arg| arg == "--all"));
        assert!(!runner.specs[0].args.iter().any(|arg| arg == "--last"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shell_mode_uses_configured_user_shell() {
        let (context, root) = fixture();
        let mut runner = FakeRunner::new([success()]);
        run_master(&context, MasterMode::Shell, &mut runner).unwrap();
        assert_eq!(runner.specs[0].program, "test-shell");
        assert!(runner.specs[0].args.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn doctor_runs_both_diagnostic_paths_even_when_codex_doctor_fails() {
        let (context, root) = fixture();
        let mut runner = FakeRunner::new([failure(3), success()]);
        assert!(matches!(
            run_master(&context, MasterMode::Doctor, &mut runner),
            Err(MasterLaunchError::ProcessFailed { ref program, code: Some(3) }) if program == "test-codex"
        ));
        assert_eq!(runner.specs.len(), 2);
        assert_eq!(runner.specs[1].program, "test-portus-os");
        assert_eq!(runner.specs[1].args, ["doctor", "codex"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn doctor_still_attempts_portus_diagnostics_when_codex_is_missing() {
        let (context, root) = fixture();
        let mut runner = FakeRunner {
            outcomes: VecDeque::from([
                Err(io::Error::new(io::ErrorKind::NotFound, "codex missing")),
                Ok(success()),
            ]),
            specs: Vec::new(),
        };
        assert!(matches!(
            run_master(&context, MasterMode::Doctor, &mut runner),
            Err(MasterLaunchError::DependencyUnavailable { ref program, .. }) if program == "test-codex"
        ));
        assert_eq!(runner.specs.len(), 2);
        assert_eq!(runner.specs[1].program, "test-portus-os");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn option_like_resume_target_is_rejected() {
        let (context, root) = fixture();
        let mut runner = FakeRunner::new([]);
        assert!(matches!(
            run_master(&context, MasterMode::ResumeId("--all".into()), &mut runner),
            Err(MasterLaunchError::InvalidResumeTarget(_))
        ));
        assert!(runner.specs.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsafe_resume_target_is_rejected_before_process_execution() {
        let (context, root) = fixture();
        let mut runner = FakeRunner::new([]);
        assert!(matches!(
            run_master(
                &context,
                MasterMode::ResumeId("bad\nthread".into()),
                &mut runner
            ),
            Err(MasterLaunchError::InvalidResumeTarget(_))
        ));
        assert!(runner.specs.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn root_execution_is_rejected_before_workspace_or_process_work() {
        let (mut context, root) = fixture();
        context.identity.uid = Some(0);
        let mut runner = FakeRunner::new([]);
        assert!(matches!(
            run_bootstrap(&context, &mut runner),
            Err(MasterLaunchError::RootExecution)
        ));
        assert!(runner.specs.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
