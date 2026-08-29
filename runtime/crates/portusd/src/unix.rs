use crate::{
    DisabledHealthProbes, DisabledIndexSources, HealthProbeMode, IndexSourceMode, RuntimeConfig,
    RuntimeCore, RuntimeError, RuntimeResult,
};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use portus_client::{FrameError, read_json_line, write_json_line};
use portus_protocol::{
    EventObjectKind, Principal, RequestEnvelope, SemanticError, SemanticErrorCode, TaskEventPage,
    TaskEventStreamFrame, TaskId,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufReader, ErrorKind},
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const SOCKET_MODE: u32 = 0o660;

pub struct RuntimeServer {
    config: RuntimeConfig,
    core: Arc<RuntimeCore>,
    listener: UnixListener,
    active_connections: Arc<AtomicUsize>,
}

impl Drop for RuntimeServer {
    fn drop(&mut self) {
        if self.core.readiness() != crate::RuntimeReadiness::Stopping {
            self.core.mark_stopping();
        }
        if let Ok(metadata) = fs::symlink_metadata(&self.config.socket_path) {
            if metadata.file_type().is_socket() {
                let _ = fs::remove_file(&self.config.socket_path);
            }
        }
    }
}

impl RuntimeServer {
    /// Starts the narrow first-party managed-process backend. This has no
    /// JSONL method counterpart and is not a generic command-execution API.
    pub fn launch_managed_process_for_internal_use(
        &self,
        principal: Principal,
        spec: portus_task::ManagedProcessSpec,
    ) -> RuntimeResult<portus_protocol::TaskView> {
        self.core
            .launch_managed_process_for_internal_use(principal, spec)
    }

    /// Registers a deliberate filesystem artifact for a first-party producer.
    /// This has no JSONL mutation counterpart and never turns file existence into registration.
    pub fn register_filesystem_artifact_for_internal_use(
        &self,
        request: portus_artifact::FilesystemRegistrationRequest,
    ) -> RuntimeResult<portus_protocol::ArtifactView> {
        self.core
            .register_filesystem_artifact_for_internal_use(request)
    }

    /// Registers a provider-owned resource by its exact provider-generation reference.
    pub fn register_provider_artifact_for_internal_use(
        &self,
        request: portus_artifact::ProviderRegistrationRequest,
    ) -> RuntimeResult<portus_protocol::ArtifactView> {
        self.core
            .register_provider_artifact_for_internal_use(request)
    }

    pub fn bind(config: RuntimeConfig) -> RuntimeResult<Self> {
        validate_config(&config)?;
        prepare_socket_path(&config.socket_path)?;
        let index_sources: Arc<dyn portus_index::IndexSourceSet> = match config.index_source_mode {
            IndexSourceMode::NativeLinux => Arc::new(portus_index::linux::LinuxIndexSources),
            IndexSourceMode::Disabled => Arc::new(DisabledIndexSources),
        };
        let audit = Arc::new(portus_audit::FileAuditSink::open(&config.audit_path)?);
        let health_probes: Arc<dyn portus_health::HealthProbeSet> = match config.health_probe_mode {
            HealthProbeMode::NativeLinux => Arc::new(portus_health::LinuxHealthProbes::default()),
            HealthProbeMode::Disabled => Arc::new(DisabledHealthProbes),
        };
        let core = RuntimeCore::open_with_sources_and_audit(
            &config.state_path,
            index_sources,
            health_probes,
            audit,
        )?;
        core.reconcile_provider_manifests(
            &config.provider_manifest_dir,
            config.provider_manifest_trust,
        );
        core.reconcile_policy(&config.policy_paths, config.policy_trust);
        let listener = UnixListener::bind(&config.socket_path)?;
        if let Err(error) =
            fs::set_permissions(&config.socket_path, fs::Permissions::from_mode(SOCKET_MODE))
        {
            let _ = fs::remove_file(&config.socket_path);
            return Err(RuntimeError::Io(error));
        }
        if let Err(error) = listener.set_nonblocking(true) {
            let _ = fs::remove_file(&config.socket_path);
            return Err(RuntimeError::Io(error));
        }
        core.mark_ready();
        if config.index_source_mode == IndexSourceMode::NativeLinux {
            let warm_core = Arc::clone(&core);
            thread::spawn(move || warm_core.warm_index_initial());
        }
        Ok(Self {
            config,
            core,
            listener,
            active_connections: Arc::new(AtomicUsize::new(0)),
        })
    }

    #[must_use]
    pub fn core(&self) -> &Arc<RuntimeCore> {
        &self.core
    }

    pub fn run_until(self, shutdown: Arc<AtomicBool>) -> RuntimeResult<()> {
        let mut workers: Vec<JoinHandle<()>> = Vec::new();
        while !shutdown.load(Ordering::Acquire) {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(permit) = ConnectionPermit::acquire(
                        Arc::clone(&self.active_connections),
                        self.config.max_connections,
                    ) {
                        let core = Arc::clone(&self.core);
                        let config = self.config.clone();
                        workers.push(thread::spawn(move || {
                            let _permit = permit;
                            if let Err(error) = serve_connection(stream, &core, &config) {
                                eprintln!(
                                    "portusd connection closed: {}",
                                    safe_connection_error(&error)
                                );
                            }
                        }));
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(ACCEPT_POLL_INTERVAL);
                }
                Err(error) => return Err(RuntimeError::Io(error)),
            }
            reap_finished_workers(&mut workers);
        }

        self.core.mark_stopping();
        for worker in workers {
            let _ = worker.join();
        }
        Ok(())
    }
}

fn reap_finished_workers(workers: &mut Vec<JoinHandle<()>>) {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            let worker = workers.swap_remove(index);
            let _ = worker.join();
        } else {
            index += 1;
        }
    }
}

struct ConnectionPermit {
    count: Arc<AtomicUsize>,
}

impl ConnectionPermit {
    fn acquire(count: Arc<AtomicUsize>, limit: usize) -> Option<Self> {
        loop {
            let current = count.load(Ordering::Acquire);
            if current >= limit {
                return None;
            }
            if count
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(Self { count });
            }
        }
    }
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}

fn validate_config(config: &RuntimeConfig) -> RuntimeResult<()> {
    if config.max_frame_bytes == 0 {
        return Err(RuntimeError::InvalidConfiguration(
            "max_frame_bytes must be nonzero".into(),
        ));
    }
    if config.max_connections == 0 {
        return Err(RuntimeError::InvalidConfiguration(
            "max_connections must be nonzero".into(),
        ));
    }
    let parent = config
        .socket_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or(RuntimeError::RuntimeDirectoryMissing)?;
    if !parent.is_dir() {
        return Err(RuntimeError::RuntimeDirectoryMissing);
    }
    Ok(())
}

fn prepare_socket_path(path: &Path) -> RuntimeResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => match UnixStream::connect(path) {
            Ok(_) => Err(RuntimeError::SocketAlreadyActive),
            Err(error) if error.kind() == ErrorKind::ConnectionRefused => {
                fs::remove_file(path)?;
                Ok(())
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(RuntimeError::Io(error)),
        },
        Ok(_) => Err(RuntimeError::SocketPathOccupied),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

fn serve_connection(
    stream: UnixStream,
    core: &Arc<RuntimeCore>,
    config: &RuntimeConfig,
) -> Result<(), ConnectionError> {
    stream.set_read_timeout(Some(config.io_timeout))?;
    stream.set_write_timeout(Some(config.io_timeout))?;
    let principal = peer_principal(&stream)?;
    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);
    let mut writer = stream;

    loop {
        let request: RequestEnvelope<Value> =
            match read_json_line(&mut reader, config.max_frame_bytes) {
                Ok(Some(request)) => request,
                Ok(None) => return Ok(()),
                Err(error) => return Err(ConnectionError::Frame(error)),
            };
        if request.method == "task.events.follow" {
            serve_task_event_stream(&mut writer, core, config, principal, request)?;
            return Ok(());
        }
        let response = core.dispatch(principal, request);
        write_json_line(&mut writer, &response, config.max_frame_bytes)?;
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskEventFollowParams {
    task_id: TaskId,
    #[serde(default)]
    after_sequence: Option<u64>,
    limit: u16,
}

fn serve_task_event_stream(
    writer: &mut UnixStream,
    core: &Arc<RuntimeCore>,
    config: &RuntimeConfig,
    principal: Principal,
    request: RequestEnvelope<Value>,
) -> Result<(), ConnectionError> {
    if let Err(error) = request.validate() {
        write_json_line(
            writer,
            &TaskEventStreamFrame::error(
                request.request_id,
                SemanticError::new(error.semantic_code(), error.to_string()),
            ),
            config.max_frame_bytes,
        )?;
        return Ok(());
    }
    let params: TaskEventFollowParams = match serde_json::from_value(request.params) {
        Ok(params) => params,
        Err(_) => {
            write_json_line(
                writer,
                &TaskEventStreamFrame::error(
                    request.request_id,
                    SemanticError::new(
                        SemanticErrorCode::InvalidRequest,
                        "task event stream parameters do not match the expected schema",
                    ),
                ),
                config.max_frame_bytes,
            )?;
            return Ok(());
        }
    };
    let subscription = core
        .events()
        .subscribe_object(EventObjectKind::Task, params.task_id.to_string());
    let mut after = params.after_sequence.unwrap_or(0);

    loop {
        let page = match core.task_event_page_for_stream(
            principal,
            &params.task_id,
            Some(after),
            params.limit,
        ) {
            Ok(page) => page,
            Err(error) => {
                write_json_line(
                    writer,
                    &TaskEventStreamFrame::error(request.request_id, error),
                    config.max_frame_bytes,
                )?;
                return Ok(());
            }
        };
        if page.gap_before_page {
            let error = SemanticError::new(
                SemanticErrorCode::StaleResource,
                "requested task event sequence is older than retained history",
            )
            .with_detail("requested_after", json!(after))
            .with_detail("retained_from", json!(page.retained_from_sequence))
            .with_detail("latest_sequence", json!(page.latest_sequence));
            write_json_line(
                writer,
                &TaskEventStreamFrame::error(request.request_id, error),
                config.max_frame_bytes,
            )?;
            return Ok(());
        }
        emit_task_event_page(writer, config, request.request_id, &page, &mut after)?;
        if page.next_sequence.is_some() {
            continue;
        }
        if let Some(terminal) = page.terminal_state
            && after >= page.latest_sequence
        {
            write_json_line(
                writer,
                &TaskEventStreamFrame::end(request.request_id, terminal.as_str()),
                config.max_frame_bytes,
            )?;
            return Ok(());
        }

        match subscription.recv_timeout(config.io_timeout) {
            Ok(_) => {
                let _ = subscription.take_missed();
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let final_page = match core.task_event_page_for_stream(
                    principal,
                    &params.task_id,
                    Some(after),
                    params.limit,
                ) {
                    Ok(page) => page,
                    Err(error) => {
                        write_json_line(
                            writer,
                            &TaskEventStreamFrame::error(request.request_id, error),
                            config.max_frame_bytes,
                        )?;
                        return Ok(());
                    }
                };
                if final_page.gap_before_page {
                    write_json_line(
                        writer,
                        &TaskEventStreamFrame::error(
                            request.request_id,
                            SemanticError::new(
                                SemanticErrorCode::StaleResource,
                                "task event history was pruned while the stream was idle",
                            ),
                        ),
                        config.max_frame_bytes,
                    )?;
                    return Ok(());
                }
                emit_task_event_page(writer, config, request.request_id, &final_page, &mut after)?;
                if let Some(terminal) = final_page.terminal_state
                    && after >= final_page.latest_sequence
                {
                    write_json_line(
                        writer,
                        &TaskEventStreamFrame::end(request.request_id, terminal.as_str()),
                        config.max_frame_bytes,
                    )?;
                } else {
                    write_json_line(
                        writer,
                        &TaskEventStreamFrame::error(
                            request.request_id,
                            SemanticError::new(
                                SemanticErrorCode::Timeout,
                                "task event stream reached its idle timeout",
                            )
                            .retryable(true),
                        ),
                        config.max_frame_bytes,
                    )?;
                }
                return Ok(());
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                write_json_line(
                    writer,
                    &TaskEventStreamFrame::error(
                        request.request_id,
                        SemanticError::new(
                            SemanticErrorCode::Unavailable,
                            "runtime event wake-up channel disconnected",
                        )
                        .retryable(true),
                    ),
                    config.max_frame_bytes,
                )?;
                return Ok(());
            }
        }
    }
}

fn emit_task_event_page(
    writer: &mut UnixStream,
    config: &RuntimeConfig,
    request_id: portus_protocol::RequestId,
    page: &TaskEventPage,
    after: &mut u64,
) -> Result<(), ConnectionError> {
    for event in &page.events {
        if event.sequence <= *after {
            continue;
        }
        write_json_line(
            writer,
            &TaskEventStreamFrame::event(request_id, event.clone()),
            config.max_frame_bytes,
        )?;
        *after = event.sequence;
    }
    Ok(())
}

fn peer_principal(stream: &UnixStream) -> Result<Principal, ConnectionError> {
    let credentials =
        getsockopt(stream, PeerCredentials).map_err(|_| ConnectionError::PeerCredentials)?;
    Ok(Principal::new(credentials.uid(), credentials.gid()))
}

#[derive(Debug)]
enum ConnectionError {
    Io(std::io::Error),
    Frame(FrameError),
    PeerCredentials,
}

impl From<std::io::Error> for ConnectionError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<FrameError> for ConnectionError {
    fn from(value: FrameError) -> Self {
        Self::Frame(value)
    }
}

fn safe_connection_error(error: &ConnectionError) -> &'static str {
    match error {
        ConnectionError::Io(io)
            if matches!(io.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) =>
        {
            "I/O timeout"
        }
        ConnectionError::Io(_) => "I/O failure",
        ConnectionError::Frame(FrameError::FrameTooLarge { .. }) => "frame too large",
        ConnectionError::Frame(FrameError::TruncatedFrame) => "truncated frame",
        ConnectionError::Frame(FrameError::InvalidJson(_)) => "invalid JSON",
        ConnectionError::Frame(FrameError::Io(_)) => "framing I/O failure",
        ConnectionError::PeerCredentials => "peer authentication failure",
    }
}
