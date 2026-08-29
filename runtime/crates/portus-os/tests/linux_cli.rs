#![cfg(target_os = "linux")]

use portus_os::{DoctorContext, SystemRuntime, run_from};
use portusd::{RuntimeConfig, RuntimeServer};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

struct TestRuntime {
    dir: PathBuf,
    socket_path: PathBuf,
    state_path: PathBuf,
    artifact_id: portus_protocol::ArtifactId,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<portusd::RuntimeResult<()>>>,
}

impl TestRuntime {
    fn start() -> Self {
        Self::start_with_modes(
            portusd::IndexSourceMode::Disabled,
            portusd::HealthProbeMode::Disabled,
        )
    }

    fn start_with_index_mode(index_source_mode: portusd::IndexSourceMode) -> Self {
        Self::start_with_modes(index_source_mode, portusd::HealthProbeMode::Disabled)
    }

    fn start_with_health_mode(health_probe_mode: portusd::HealthProbeMode) -> Self {
        Self::start_with_modes(portusd::IndexSourceMode::Disabled, health_probe_mode)
    }

    fn start_with_modes(
        index_source_mode: portusd::IndexSourceMode,
        health_probe_mode: portusd::HealthProbeMode,
    ) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "portus-cli-linux-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let manifests = dir.join("manifests");
        fs::create_dir_all(&manifests).unwrap();
        fs::write(
            manifests.join("test-provider.toml"),
            r#"manifest_version = 1

[provider]
type = "test-provider"
label = "Test Provider"
scope_support = ["system"]
software_version = "1.0.0"

[[interfaces]]
id = "cli"
type = "executable"
contract_version = 1
executable = "/usr/bin/test-provider"
structured_output = true

[[capabilities]]
id = "test.control"
contract_version = 1
interfaces = ["cli"]

[lifecycle]
owner = "provider-owned"

[health]
kind = "structured-cli"
reference = "cli"

[policy]
domain_owner = "provider"
"#,
        )
        .unwrap();
        let socket_path = dir.join("portusd.sock");
        let state_path = dir.join("portus.db");
        let server = RuntimeServer::bind(RuntimeConfig {
            socket_path: socket_path.clone(),
            state_path: state_path.clone(),
            audit_path: dir.join("audit.jsonl"),
            max_frame_bytes: portus_client::DEFAULT_MAX_FRAME_BYTES,
            io_timeout: Duration::from_secs(2),
            max_connections: 8,
            provider_manifest_dir: manifests,
            provider_manifest_trust: portusd::ManifestTrust::PretrustedFixture,
            policy_paths: portusd::PolicyPaths {
                policy_path: dir.join("policy.toml"),
                subjects_dir: dir.join("subjects.d"),
                actions_path: dir.join("actions.toml"),
                bundles_dir: dir.join("bundles"),
            },
            policy_trust: portusd::PolicyTrust::PretrustedFixture,
            index_source_mode,
            health_probe_mode,
        })
        .unwrap();
        let artifact_path = dir.join("registered-artifact.txt");
        fs::write(&artifact_path, b"registered over real Unix runtime").unwrap();
        let mut artifact_request = portus_artifact::FilesystemRegistrationRequest::retained(
            portus_protocol::Principal::new(4242, 4242),
            &artifact_path,
            portus_protocol::ArtifactType::Report,
        );
        artifact_request.confidentiality = portus_protocol::ArtifactConfidentiality::Public;
        artifact_request.safe_display_name = Some("registered-artifact.txt".into());
        let artifact_id = server
            .register_filesystem_artifact_for_internal_use(artifact_request)
            .unwrap()
            .artifact
            .artifact_id;
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let handle = thread::spawn(move || server.run_until(thread_shutdown));
        Self {
            dir,
            socket_path,
            state_path,
            shutdown,
            artifact_id,
            handle: Some(handle),
        }
    }
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap().unwrap();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn full_cli_status_uses_actual_authenticated_unix_runtime_path() {
    let server = TestRuntime::start();
    let mut runtime = SystemRuntime::for_socket(&server.socket_path);
    let doctor = DoctorContext {
        socket_path: server.socket_path.clone(),
        state_path: server.state_path.clone(),
        capabilities_dir: server.dir.join("capabilities"),
    };
    let rendered = run_from(["portus-os", "status", "--json"], &mut runtime, &doctor);
    assert_eq!(rendered.exit_code, 0);
    let output: Value = serde_json::from_str(rendered.stdout.trim()).unwrap();
    assert_eq!(output["command"], "status");
    assert_eq!(output["data"]["runtime"]["readiness"], "ready");
    assert!(output["data"]["runtime"]["principal"]["uid"].is_number());
    assert!(output["meta"]["request_id"].is_string());
}

#[test]
fn capability_cli_reads_reconciled_registry_over_actual_unix_runtime() {
    let server = TestRuntime::start();
    let mut runtime = SystemRuntime::for_socket(&server.socket_path);
    let doctor = DoctorContext {
        socket_path: server.socket_path.clone(),
        state_path: server.state_path.clone(),
        capabilities_dir: server.dir.join("manifests"),
    };
    let listed = run_from(
        ["portus-os", "capability", "list", "--json"],
        &mut runtime,
        &doctor,
    );
    assert_eq!(listed.exit_code, 0);
    let list_output: Value = serde_json::from_str(listed.stdout.trim()).unwrap();
    assert_eq!(
        list_output["data"]["items"][0]["capability_id"],
        "test.control"
    );

    let providers = run_from(
        ["portus-os", "capability", "provider", "list", "--json"],
        &mut runtime,
        &doctor,
    );
    assert_eq!(providers.exit_code, 0);
    let provider_output: Value = serde_json::from_str(providers.stdout.trim()).unwrap();
    assert_eq!(
        provider_output["data"]["items"][0]["provider_type"],
        "test-provider"
    );
}

#[test]
fn native_linux_index_rescan_and_query_use_actual_read_only_sources() {
    let server = TestRuntime::start_with_index_mode(portusd::IndexSourceMode::NativeLinux);
    let mut runtime = SystemRuntime::for_socket(&server.socket_path);
    let doctor = DoctorContext {
        socket_path: server.socket_path.clone(),
        state_path: server.state_path.clone(),
        capabilities_dir: server.dir.join("manifests"),
    };
    let rescan = run_from(
        ["portus-os", "index", "rescan", "runtime", "--json"],
        &mut runtime,
        &doctor,
    );
    assert_eq!(rescan.exit_code, 0);
    let rescan_value: Value = serde_json::from_str(rescan.stdout.trim()).unwrap();
    assert!(matches!(
        rescan_value["data"]["state"].as_str(),
        Some("healthy" | "degraded")
    ));

    let query = run_from(
        [
            "portus-os",
            "index",
            "query",
            "--type",
            "process",
            "--source",
            "proc",
            "--limit",
            "10",
            "--json",
        ],
        &mut runtime,
        &doctor,
    );
    assert_eq!(query.exit_code, 0);
    let query_value: Value = serde_json::from_str(query.stdout.trim()).unwrap();
    assert!(!query_value["data"]["items"].as_array().unwrap().is_empty());
    assert_eq!(query_value["meta"]["degraded"], false);
}

#[test]
fn native_linux_health_probes_flow_through_real_cli_and_socket() {
    let server = TestRuntime::start_with_health_mode(portusd::HealthProbeMode::NativeLinux);
    let mut runtime = SystemRuntime::for_socket(&server.socket_path);
    let doctor = DoctorContext {
        socket_path: server.socket_path.clone(),
        state_path: server.state_path.clone(),
        capabilities_dir: server.dir.join("manifests"),
    };
    let rendered = run_from(["portus-os", "health", "--json"], &mut runtime, &doctor);
    assert_eq!(rendered.exit_code, 0);
    let output: Value = serde_json::from_str(rendered.stdout.trim()).unwrap();
    assert!(
        output["data"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["component_ref"] == "memory:system")
    );
}

#[test]
fn artifact_cli_reads_deliberately_registered_artifact_over_actual_unix_runtime() {
    let server = TestRuntime::start();
    let mut runtime = SystemRuntime::for_socket(&server.socket_path);
    let doctor = DoctorContext {
        socket_path: server.socket_path.clone(),
        state_path: server.state_path.clone(),
        capabilities_dir: server.dir.join("manifests"),
    };
    let listed = run_from(
        ["portus-os", "artifact", "list", "--json"],
        &mut runtime,
        &doctor,
    );
    assert_eq!(listed.exit_code, 0);
    let listed_value: Value = serde_json::from_str(listed.stdout.trim()).unwrap();
    assert!(
        listed_value["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["artifact_id"] == server.artifact_id.to_string())
    );

    let shown = run_from(
        [
            "portus-os",
            "artifact",
            "show",
            &server.artifact_id.to_string(),
            "--json",
        ],
        &mut runtime,
        &doctor,
    );
    assert_eq!(shown.exit_code, 0);
    let shown_value: Value = serde_json::from_str(shown.stdout.trim()).unwrap();
    assert_eq!(
        shown_value["data"]["artifact"]["artifact_id"],
        server.artifact_id.to_string()
    );
}

#[test]
fn doctor_does_not_depend_on_runtime_business_request() {
    let server = TestRuntime::start();
    let mut runtime = SystemRuntime::for_socket(server.dir.join("not-used.sock"));
    let doctor = DoctorContext {
        socket_path: server.dir.join("missing.sock"),
        state_path: server.state_path.clone(),
        capabilities_dir: server.dir.join("capabilities"),
    };
    let rendered = run_from(
        ["portus-os", "doctor", "runtime", "--json"],
        &mut runtime,
        &doctor,
    );
    assert_eq!(rendered.exit_code, 0);
    let output: Value = serde_json::from_str(rendered.stdout.trim()).unwrap();
    assert_eq!(output["data"]["checks"][0]["status"], "unavailable");
}
