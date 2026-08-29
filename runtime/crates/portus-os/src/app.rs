use crate::{
    CLI_OUTPUT_SCHEMA_VERSION, CliError, DoctorContext, ExecutionContext, OutputMode,
    PrivilegeTransport, RenderedOutput, RuntimeTransport, TaskCommand, UnavailablePrivilege,
    execute, render_error, render_success,
};
use portus_protocol::{SemanticErrorCode, TaskEvent};
use serde_json::json;
use std::{
    ffi::{OsStr, OsString},
    io::Write,
    time::Duration,
};
pub fn run_from<I, T>(
    args: I,
    runtime: &mut dyn RuntimeTransport,
    doctor: &DoctorContext,
) -> RenderedOutput
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut privilege = UnavailablePrivilege;
    let exit_code = run_to_writers_with_privilege(
        args,
        runtime,
        &mut privilege,
        doctor,
        &mut stdout,
        &mut stderr,
    );
    RenderedOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

pub fn run_from_with_privilege<I, T>(
    args: I,
    runtime: &mut dyn RuntimeTransport,
    privilege: &mut dyn PrivilegeTransport,
    doctor: &DoctorContext,
) -> RenderedOutput
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code =
        run_to_writers_with_privilege(args, runtime, privilege, doctor, &mut stdout, &mut stderr);
    RenderedOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

pub fn run_to_writers<I, T>(
    args: I,
    runtime: &mut dyn RuntimeTransport,
    doctor: &DoctorContext,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut privilege = UnavailablePrivilege;
    run_to_writers_with_privilege(args, runtime, &mut privilege, doctor, stdout, stderr)
}

pub fn run_to_writers_with_privilege<I, T>(
    args: I,
    runtime: &mut dyn RuntimeTransport,
    privilege: &mut dyn PrivilegeTransport,
    doctor: &DoctorContext,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let raw_args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let structured_requested = raw_args.iter().any(|arg| {
        matches!(
            arg.as_os_str(),
            value if value == OsStr::new("--json") || value == OsStr::new("--jsonl")
        )
    });
    let cli = match crate::parse_from(raw_args) {
        Ok(cli) => cli,
        Err(error) => {
            let rendered = render_parse_error(error, structured_requested);
            write_rendered(&rendered, stdout, stderr);
            return rendered.exit_code;
        }
    };

    if let crate::Command::Task {
        command:
            TaskCommand::Events {
                task_id,
                after,
                limit,
                follow: true,
            },
    } = &cli.command
    {
        return run_task_event_follow(
            runtime,
            TaskFollowOptions {
                task_id,
                after: *after,
                limit: *limit,
                output_mode: cli.output_mode(),
                timeout: Duration::from_millis(cli.timeout_ms),
            },
            stdout,
            stderr,
        );
    }

    let json_mode = cli.output_mode() == OutputMode::Json;
    let mut context = ExecutionContext {
        runtime,
        privilege,
        doctor,
    };
    let rendered = match execute(&cli, &mut context) {
        Ok(success) => render_success(&success, json_mode),
        Err(error) => render_error(&error, json_mode),
    };
    write_rendered(&rendered, stdout, stderr);
    rendered.exit_code
}

struct TaskFollowOptions<'a> {
    task_id: &'a portus_protocol::TaskId,
    after: Option<u64>,
    limit: u16,
    output_mode: OutputMode,
    timeout: Duration,
}

fn run_task_event_follow(
    runtime: &mut dyn RuntimeTransport,
    options: TaskFollowOptions<'_>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    if options.output_mode == OutputMode::Json {
        let rendered = render_error(
            &CliError::new(
                "task.events",
                SemanticErrorCode::UnsupportedOutputMode,
                "task event following supports human output or --jsonl, not buffered --json",
            ),
            true,
        );
        write_rendered(&rendered, stdout, stderr);
        return rendered.exit_code;
    }

    let jsonl = options.output_mode == OutputMode::Jsonl;
    let mut emit = |event: TaskEvent| -> Result<(), CliError> {
        let line = if jsonl {
            serde_json::to_string(&json!({
                "schema_version": CLI_OUTPUT_SCHEMA_VERSION,
                "command": "task.events",
                "event_type": event.event_kind,
                "task_id": event.task_id,
                "sequence": event.sequence,
                "timestamp": event.occurred_at_ms,
                "data": {
                    "source_ref": event.source_ref,
                    "safe_summary": event.safe_summary,
                    "safe_data": event.safe_data,
                }
            }))
            .map_err(|_| {
                CliError::new(
                    "task.events",
                    SemanticErrorCode::Internal,
                    "failed to encode task event stream output",
                )
            })?
        } else {
            format!(
                "{}  {}  {}",
                event.sequence,
                event.event_kind,
                event.safe_summary.as_deref().unwrap_or("-")
            )
        };
        writeln!(stdout, "{line}").map_err(|_| {
            CliError::new(
                "task.events",
                SemanticErrorCode::Internal,
                "failed to write task event stream output",
            )
        })?;
        stdout.flush().map_err(|_| {
            CliError::new(
                "task.events",
                SemanticErrorCode::Internal,
                "failed to flush task event stream output",
            )
        })?;
        Ok(())
    };

    match runtime.follow_task_events(
        options.task_id,
        options.after,
        options.limit,
        options.timeout,
        &mut emit,
    ) {
        Ok(_) => 0,
        Err(error) => {
            let rendered = render_error(&error, jsonl);
            write_rendered(&rendered, stdout, stderr);
            rendered.exit_code
        }
    }
}

fn write_rendered(rendered: &RenderedOutput, stdout: &mut dyn Write, stderr: &mut dyn Write) {
    if !rendered.stdout.is_empty() {
        let _ = stdout.write_all(rendered.stdout.as_bytes());
        let _ = stdout.flush();
    }
    if !rendered.stderr.is_empty() {
        let _ = stderr.write_all(rendered.stderr.as_bytes());
        let _ = stderr.flush();
    }
}

fn render_parse_error(error: clap::Error, json_requested: bool) -> RenderedOutput {
    if matches!(
        error.kind(),
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
    ) {
        return RenderedOutput {
            stdout: error.to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
    }
    if json_requested {
        return render_error(
            &CliError::new(
                "cli",
                SemanticErrorCode::InvalidArgument,
                "command-line arguments are invalid",
            ),
            true,
        );
    }
    RenderedOutput {
        stdout: String::new(),
        stderr: error.to_string(),
        exit_code: 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CliMeta, RuntimeReply};
    use serde_json::{Value, json};
    use std::{collections::VecDeque, time::Duration};

    struct FakeRuntime {
        replies: VecDeque<Result<RuntimeReply, CliError>>,
    }

    impl FakeRuntime {
        fn new(replies: impl IntoIterator<Item = Result<RuntimeReply, CliError>>) -> Self {
            Self {
                replies: replies.into_iter().collect(),
            }
        }
    }

    impl RuntimeTransport for FakeRuntime {
        fn request(
            &mut self,
            _method: &str,
            _params: Value,
            _timeout: Duration,
        ) -> Result<RuntimeReply, CliError> {
            self.replies.pop_front().expect("fake reply")
        }
    }

    struct StreamRuntime {
        events: Vec<TaskEvent>,
    }

    impl RuntimeTransport for StreamRuntime {
        fn request(
            &mut self,
            _method: &str,
            _params: Value,
            _timeout: Duration,
        ) -> Result<RuntimeReply, CliError> {
            panic!("stream fixture must not use request/response RPC")
        }

        fn follow_task_events(
            &mut self,
            _task_id: &portus_protocol::TaskId,
            _after_sequence: Option<u64>,
            _limit: u16,
            _timeout: Duration,
            on_event: &mut dyn FnMut(TaskEvent) -> Result<(), CliError>,
        ) -> Result<crate::TaskStreamEnd, CliError> {
            for event in self.events.clone() {
                on_event(event)?;
            }
            Ok(crate::TaskStreamEnd {
                terminal_state: "succeeded".into(),
            })
        }
    }

    fn stream_event(task_id: portus_protocol::TaskId, sequence: u64, kind: &str) -> TaskEvent {
        TaskEvent {
            task_id,
            sequence,
            event_kind: kind.into(),
            source_ref: Some("fixture".into()),
            safe_summary: Some(kind.into()),
            safe_data: json!({"phase":sequence}),
            occurred_at_ms: sequence as i64,
        }
    }

    #[test]
    fn task_follow_jsonl_emits_one_complete_event_per_line() {
        let task_id = portus_protocol::TaskId::new();
        let mut runtime = StreamRuntime {
            events: vec![
                stream_event(task_id, 1, "task.created"),
                stream_event(task_id, 2, "task.succeeded"),
            ],
        };
        let rendered = run_from(
            [
                "portus-os".to_string(),
                "task".into(),
                "events".into(),
                task_id.to_string(),
                "--follow".into(),
                "--jsonl".into(),
            ],
            &mut runtime,
            &DoctorContext::default(),
        );
        assert_eq!(rendered.exit_code, 0);
        assert!(rendered.stderr.is_empty());
        let lines = rendered.stdout.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        let first: Value = serde_json::from_str(lines[0]).unwrap();
        let second: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(first["command"], "task.events");
        assert_eq!(first["task_id"], task_id.to_string());
        assert_eq!(first["sequence"], 1);
        assert_eq!(second["event_type"], "task.succeeded");
        assert_eq!(second["sequence"], 2);
    }

    #[test]
    fn task_follow_human_stream_stays_bounded_and_readable() {
        let task_id = portus_protocol::TaskId::new();
        let mut runtime = StreamRuntime {
            events: vec![stream_event(task_id, 7, "task.running")],
        };
        let rendered = run_from(
            [
                "portus-os".to_string(),
                "task".into(),
                "events".into(),
                task_id.to_string(),
                "--follow".into(),
            ],
            &mut runtime,
            &DoctorContext::default(),
        );
        assert_eq!(rendered.exit_code, 0);
        assert_eq!(rendered.stdout, "7  task.running  task.running\n");
        assert!(rendered.stderr.is_empty());
    }

    #[test]
    fn task_follow_rejects_buffered_json_mode() {
        let task_id = portus_protocol::TaskId::new();
        let mut runtime = StreamRuntime { events: Vec::new() };
        let rendered = run_from(
            [
                "portus-os".to_string(),
                "task".into(),
                "events".into(),
                task_id.to_string(),
                "--follow".into(),
                "--json".into(),
            ],
            &mut runtime,
            &DoctorContext::default(),
        );
        assert_eq!(rendered.exit_code, 2);
        let value: Value = serde_json::from_str(rendered.stdout.trim()).unwrap();
        assert_eq!(value["error"]["code"], "unsupported_output_mode");
    }

    #[test]
    fn full_json_status_path_renders_stable_envelope() {
        let mut runtime = FakeRuntime::new([Ok(RuntimeReply {
            data: json!({"readiness":"ready","health":"healthy","schema_version":2}),
            meta: CliMeta::default(),
        })]);
        let rendered = run_from(
            ["portus-os", "status", "--json"],
            &mut runtime,
            &DoctorContext::default(),
        );
        assert_eq!(rendered.exit_code, 0);
        let output: Value = serde_json::from_str(rendered.stdout.trim()).unwrap();
        assert_eq!(output["command"], "status");
        assert_eq!(output["ok"], true);
        assert_eq!(output["data"]["runtime"]["health"], "healthy");
    }

    #[test]
    fn structured_parse_error_is_not_clap_prose() {
        let mut runtime = FakeRuntime::new([]);
        let rendered = run_from(
            ["portus-os", "status", "--json", "--jsonl"],
            &mut runtime,
            &DoctorContext::default(),
        );
        assert_eq!(rendered.exit_code, 2);
        assert!(rendered.stderr.is_empty());
        let output: Value = serde_json::from_str(rendered.stdout.trim()).unwrap();
        assert_eq!(output["error"]["code"], "invalid_argument");
    }

    #[test]
    fn doctor_missing_daemon_renders_successful_diagnostic_report() {
        let dir = std::env::temp_dir().join(format!(
            "portus-cli-doctor-{}",
            portus_protocol::TaskId::new()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let doctor = DoctorContext {
            socket_path: dir.join("missing.sock"),
            state_path: dir.join("missing.db"),
            capabilities_dir: dir.join("missing-capabilities"),
        };
        let mut runtime = FakeRuntime::new([]);
        let rendered = run_from(
            ["portus-os", "doctor", "runtime", "--json"],
            &mut runtime,
            &doctor,
        );
        assert_eq!(rendered.exit_code, 0);
        let output: Value = serde_json::from_str(rendered.stdout.trim()).unwrap();
        assert_eq!(output["data"]["checks"][0]["status"], "unavailable");
        let _ = std::fs::remove_dir_all(dir);
    }
}
