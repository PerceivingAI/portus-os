#![cfg(target_os = "linux")]

use nix::sys::socket::UnixCredentials;
use portus_client::UnixRuntimeClient;
use portus_protocol::{
    CURRENT_PROTOCOL_VERSION, Principal, ProtocolVersion, RequestEnvelope, ResponseEnvelope,
    SemanticErrorCode, TaskEventPage, TaskEventStreamFrame, TaskEventStreamFrameKind, TaskId,
};
use portus_state::PortusState;
use portusd::{RuntimeConfig, RuntimeError, RuntimeResult, RuntimeServer};
use serde_json::{Value, json};
use std::{
    fs,
    io::{ErrorKind, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

fn unique_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", TaskId::new()))
}

fn config_for(dir: &Path, max_frame_bytes: usize) -> RuntimeConfig {
    RuntimeConfig {
        socket_path: dir.join("portusd.sock"),
        state_path: dir.join("portus.db"),
        audit_path: dir.join("audit.jsonl"),
        max_frame_bytes,
        io_timeout: Duration::from_secs(2),
        max_connections: 8,
        provider_manifest_dir: dir.join("manifests"),
        provider_manifest_trust: portus_provider::ManifestTrust::PretrustedFixture,
        policy_paths: portus_policy::PolicyPaths {
            policy_path: dir.join("policy.toml"),
            subjects_dir: dir.join("subjects.d"),
            actions_path: dir.join("actions.toml"),
            bundles_dir: dir.join("bundles"),
        },
        policy_trust: portus_policy::PolicyTrust::PretrustedFixture,
        index_source_mode: portusd::IndexSourceMode::Disabled,
        health_probe_mode: portusd::HealthProbeMode::Disabled,
    }
}

struct TestServer {
    dir: PathBuf,
    socket_path: PathBuf,
    state_path: PathBuf,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<RuntimeResult<()>>>,
}

impl TestServer {
    fn start(max_frame_bytes: usize) -> Self {
        let dir = unique_dir("portusd-linux");
        fs::create_dir_all(&dir).unwrap();
        let config = config_for(&dir, max_frame_bytes);
        let socket_path = config.socket_path.clone();
        let state_path = config.state_path.clone();
        let server = RuntimeServer::bind(config).unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let handle = thread::spawn(move || server.run_until(thread_shutdown));
        Self {
            dir,
            socket_path,
            state_path,
            shutdown,
            handle: Some(handle),
        }
    }

    fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap().unwrap();
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn assert_connection_closed(stream: &mut UnixStream) {
    let mut output = [0_u8; 1];
    match stream.read(&mut output) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::ConnectionReset | ErrorKind::BrokenPipe | ErrorKind::NotConnected
            ) => {}
        other => panic!("expected closed connection, got {other:?}"),
    }
}

#[test]
fn actual_socket_round_trip_uses_kernel_peer_credentials() {
    let server = TestServer::start(4096);
    let mut client =
        UnixRuntimeClient::connect_with_limits(&server.socket_path, 4096, Duration::from_secs(2))
            .unwrap();
    let request = RequestEnvelope::new("runtime.status", json!({}));
    let response: ResponseEnvelope<Value> = client.request(&request).unwrap();
    let current = UnixCredentials::new();
    let result = response.result.unwrap();
    assert_eq!(
        result["principal"],
        json!({"uid": current.uid(), "gid": current.gid()})
    );
}

#[test]
fn payload_cannot_spoof_peer_identity() {
    let server = TestServer::start(4096);
    let mut client =
        UnixRuntimeClient::connect_with_limits(&server.socket_path, 4096, Duration::from_secs(2))
            .unwrap();
    let request = RequestEnvelope::new("runtime.status", json!({"uid": 0, "gid": 0}));
    let response: ResponseEnvelope<Value> = client.request(&request).unwrap();
    assert_eq!(
        response.error.unwrap().code,
        SemanticErrorCode::InvalidRequest
    );
}

#[test]
fn incompatible_protocol_returns_structured_error() {
    let server = TestServer::start(4096);
    let mut client =
        UnixRuntimeClient::connect_with_limits(&server.socket_path, 4096, Duration::from_secs(2))
            .unwrap();
    let mut request = RequestEnvelope::new("runtime.ping", json!({}));
    request.version = ProtocolVersion::new(CURRENT_PROTOCOL_VERSION.get() + 1);
    let response: ResponseEnvelope<Value> = client.request(&request).unwrap();
    assert_eq!(
        response.error.unwrap().code,
        SemanticErrorCode::IncompatibleProtocol
    );
}

#[test]
fn malformed_json_is_rejected_and_connection_closed() {
    let server = TestServer::start(4096);
    let mut stream = UnixStream::connect(&server.socket_path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(b"{bad}\n").unwrap();
    assert_connection_closed(&mut stream);
}

#[test]
fn oversized_frame_is_rejected_and_connection_closed() {
    let server = TestServer::start(64);
    let mut stream = UnixStream::connect(&server.socket_path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(&[b'x'; 65]).unwrap();
    stream.write_all(b"\n").unwrap();
    assert_connection_closed(&mut stream);
}

#[test]
fn disconnect_after_partial_frame_does_not_corrupt_state() {
    let server = TestServer::start(4096);
    {
        let mut stream = UnixStream::connect(&server.socket_path).unwrap();
        stream.write_all(b"{\"version\":1").unwrap();
    }
    let state = PortusState::open_read_only(&server.state_path).unwrap();
    state.integrity_check().unwrap();
}

#[test]
fn socket_mode_is_owner_group_only() {
    let dir = unique_dir("portusd-mode");
    fs::create_dir_all(&dir).unwrap();
    let config = config_for(&dir, 4096);
    let server = RuntimeServer::bind(config.clone()).unwrap();
    let mode = fs::metadata(&config.socket_path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o660);
    drop(server);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn active_socket_is_not_unlinked_by_second_daemon() {
    let dir = unique_dir("portusd-active");
    fs::create_dir_all(&dir).unwrap();
    let config = config_for(&dir, 4096);
    let first = RuntimeServer::bind(config.clone()).unwrap();
    let error = match RuntimeServer::bind(config.clone()) {
        Ok(_) => panic!("second daemon unexpectedly replaced active socket"),
        Err(error) => error,
    };
    assert!(matches!(error, RuntimeError::SocketAlreadyActive));
    assert!(config.socket_path.exists());
    drop(first);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn stale_socket_path_is_replaced_safely() {
    let dir = unique_dir("portusd-stale");
    fs::create_dir_all(&dir).unwrap();
    let config = config_for(&dir, 4096);
    let stale = UnixListener::bind(&config.socket_path).unwrap();
    drop(stale);
    assert!(config.socket_path.exists());
    let server = RuntimeServer::bind(config).unwrap();
    drop(server);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn managed_task_show_and_cancel_use_authenticated_socket_path() {
    let dir = unique_dir("portusd-task-linux");
    fs::create_dir_all(&dir).unwrap();
    let config = config_for(&dir, 16 * 1024);
    let server = RuntimeServer::bind(config.clone()).unwrap();
    let credentials = UnixCredentials::new();
    let principal = Principal::new(credentials.uid(), credentials.gid());
    let mut spec = portus_task::ManagedProcessSpec::new(
        "sleep",
        "fixture.linux.sleep",
        "prove Linux managed task cancellation",
    );
    spec.args = vec!["5".into()];
    spec.requester_surface = "linux-integration".into();
    let task = server
        .launch_managed_process_for_internal_use(principal, spec)
        .unwrap();
    let task_id = task.task.task_id;

    let shutdown = Arc::new(AtomicBool::new(false));
    let thread_shutdown = Arc::clone(&shutdown);
    let handle = thread::spawn(move || server.run_until(thread_shutdown));

    let mut client = UnixRuntimeClient::connect_with_limits(
        &config.socket_path,
        16 * 1024,
        Duration::from_secs(2),
    )
    .unwrap();
    let shown: ResponseEnvelope<Value> = client
        .request(&RequestEnvelope::new(
            "task.show",
            json!({"task_id":task_id}),
        ))
        .unwrap();
    assert_eq!(shown.result.unwrap()["state"], "running");

    let cancelled: ResponseEnvelope<Value> = client
        .request(&RequestEnvelope::new(
            "task.cancel",
            json!({"task_id":task_id,"if_state":"running"}),
        ))
        .unwrap();
    assert_eq!(cancelled.result.unwrap()["state"], "cancelled");

    shutdown.store(true, Ordering::Release);
    handle.join().unwrap().unwrap();
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn task_event_stream_recovers_from_disconnect_using_durable_sequence() {
    let dir = unique_dir("portusd-task-stream-linux");
    fs::create_dir_all(&dir).unwrap();
    let config = config_for(&dir, 16 * 1024);
    let server = RuntimeServer::bind(config.clone()).unwrap();
    let credentials = UnixCredentials::new();
    let principal = Principal::new(credentials.uid(), credentials.gid());
    let mut spec = portus_task::ManagedProcessSpec::new(
        "sleep",
        "fixture.linux.stream",
        "prove durable Linux task-event stream recovery",
    );
    spec.args = vec!["5".into()];
    spec.requester_surface = "linux-integration".into();
    let task = server
        .launch_managed_process_for_internal_use(principal, spec)
        .unwrap();
    let task_id = task.task.task_id;

    let shutdown = Arc::new(AtomicBool::new(false));
    let thread_shutdown = Arc::clone(&shutdown);
    let handle = thread::spawn(move || server.run_until(thread_shutdown));

    let mut control = UnixRuntimeClient::connect_with_limits(
        &config.socket_path,
        16 * 1024,
        Duration::from_secs(2),
    )
    .unwrap();
    let initial: ResponseEnvelope<Value> = control
        .request(&RequestEnvelope::new(
            "task.events",
            json!({"task_id":task_id,"after_sequence":0,"limit":50}),
        ))
        .unwrap();
    let initial_page: TaskEventPage = serde_json::from_value(initial.result.unwrap()).unwrap();
    assert!(initial_page.latest_sequence > 0);

    let mut abandoned = UnixRuntimeClient::connect_with_limits(
        &config.socket_path,
        16 * 1024,
        Duration::from_secs(2),
    )
    .unwrap();
    abandoned
        .send(&RequestEnvelope::new(
            "task.events.follow",
            json!({
                "task_id":task_id,
                "after_sequence":initial_page.latest_sequence,
                "limit":50
            }),
        ))
        .unwrap();
    drop(abandoned);

    let cancelled: ResponseEnvelope<Value> = control
        .request(&RequestEnvelope::new(
            "task.cancel",
            json!({"task_id":task_id,"if_state":"running"}),
        ))
        .unwrap();
    assert_eq!(cancelled.result.unwrap()["state"], "cancelled");

    let mut resumed = UnixRuntimeClient::connect_with_limits(
        &config.socket_path,
        16 * 1024,
        Duration::from_secs(2),
    )
    .unwrap();
    let request = RequestEnvelope::new(
        "task.events.follow",
        json!({
            "task_id":task_id,
            "after_sequence":initial_page.latest_sequence,
            "limit":50
        }),
    );
    let request_id = request.request_id;
    resumed.send(&request).unwrap();

    let mut sequences = Vec::new();
    loop {
        let frame: TaskEventStreamFrame = resumed.read().unwrap().unwrap();
        frame.validate().unwrap();
        assert_eq!(frame.request_id, request_id);
        match frame.frame {
            TaskEventStreamFrameKind::Event => {
                let event = frame.event.unwrap();
                assert_eq!(event.task_id, task_id);
                assert!(event.sequence > initial_page.latest_sequence);
                sequences.push(event.sequence);
            }
            TaskEventStreamFrameKind::End => {
                assert_eq!(frame.terminal_state.as_deref(), Some("cancelled"));
                break;
            }
            TaskEventStreamFrameKind::Error => {
                panic!(
                    "resumed task-event stream returned an error: {:?}",
                    frame.error
                )
            }
        }
    }
    assert!(!sequences.is_empty());
    assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));

    shutdown.store(true, Ordering::Release);
    handle.join().unwrap().unwrap();
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn daemon_restart_preserves_durable_state() {
    let dir = unique_dir("portusd-restart");
    fs::create_dir_all(&dir).unwrap();
    let config = config_for(&dir, 4096);
    let task_id = TaskId::new();
    let principal = Principal::new(2200, 2200);

    {
        let first = RuntimeServer::bind(config.clone()).unwrap();
        let state = PortusState::open(&config.state_path).unwrap();
        state
            .insert_task_fixture(
                &task_id,
                principal,
                "persist across daemon restart",
                "running",
                1,
            )
            .unwrap();
        drop(state);
        drop(first);
    }

    {
        let second = RuntimeServer::bind(config.clone()).unwrap();
        let state = PortusState::open_read_only(&config.state_path).unwrap();
        assert!(
            state
                .task_for_principal(&task_id, principal)
                .unwrap()
                .is_some()
        );
        drop(state);
        drop(second);
    }

    let _ = fs::remove_dir_all(dir);
}
