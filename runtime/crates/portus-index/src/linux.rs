use crate::{
    IndexRescanDomain, IndexSourceSet, SourceBatch, SourceCollection, parse_desktop_entry,
    parse_i3_outputs, parse_i3_tree_placements, parse_i3_workspaces, parse_openrc_status,
    parse_proc_stat, parse_status_identity, parse_xprop_client_list, parse_xprop_window,
};
use portus_protocol::{
    ControlPathKind, EvidenceStrength, Freshness, HealthState, IndexObservationInput,
    IndexRelationInput, IndexResourceType, IndexSourceKind, IndexSourceStatus, Principal,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::OsStr,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const MAX_PROCESSES: usize = 65_536;
const MAX_DESKTOP_FILES: usize = 4_096;
const MAX_PACKAGE_RECORDS: usize = 20_000;
const MAX_WINDOWS: usize = 256;
const MAX_TEXT_FILE_BYTES: u64 = 256 * 1024;
const MAX_PROC_ENV_BYTES: u64 = 128 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_COMMAND_OUTPUT: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub struct LinuxIndexSources;

impl IndexSourceSet for LinuxIndexSources {
    fn collect(
        &self,
        domain: IndexRescanDomain,
        principal: Principal,
        observed_at_ms: i64,
    ) -> SourceCollection {
        let mut batches = Vec::new();
        if matches!(
            domain,
            IndexRescanDomain::Applications | IndexRescanDomain::All
        ) {
            batches.push(collect_applications(observed_at_ms));
        }
        if matches!(domain, IndexRescanDomain::Runtime | IndexRescanDomain::All) {
            let proc_batch = collect_processes(observed_at_ms);
            let boot_generation = proc_batch.status.source_generation.clone();
            batches.push(proc_batch);
            if principal.uid() != 0 {
                let (i3, x11) = collect_graphical(principal, &boot_generation, observed_at_ms);
                batches.push(i3);
                batches.push(x11);
            }
        }
        if matches!(domain, IndexRescanDomain::Services | IndexRescanDomain::All) {
            batches.push(collect_openrc(observed_at_ms));
        }
        SourceCollection { batches }
    }
}

fn collect_processes(observed_at_ms: i64) -> SourceBatch {
    let boot_id = match bounded_read_string(Path::new("/proc/sys/kernel/random/boot_id"), 4096) {
        Ok(value) => value.trim().to_string(),
        Err(_) => {
            return SourceBatch::unavailable(
                "proc",
                IndexSourceKind::Proc,
                None,
                "unknown",
                "boot_generation_unavailable",
                observed_at_ms,
            );
        }
    };
    let mut entries = match fs::read_dir("/proc") {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .and_then(|value| value.parse::<u32>().ok())
                    .map(|pid| (pid, entry.path()))
            })
            .collect::<Vec<_>>(),
        Err(_) => {
            return SourceBatch::unavailable(
                "proc",
                IndexSourceKind::Proc,
                None,
                boot_id,
                "proc_unavailable",
                observed_at_ms,
            );
        }
    };
    entries.sort_by_key(|(pid, _)| *pid);
    let truncated = entries.len() > MAX_PROCESSES;
    entries.truncate(MAX_PROCESSES);
    let mut observations = Vec::new();
    for (pid, path) in entries {
        let Ok(stat_text) = bounded_read_string(&path.join("stat"), 32 * 1024) else {
            continue;
        };
        let Some(stat) = parse_proc_stat(&stat_text) else {
            continue;
        };
        if stat.pid != pid {
            continue;
        }
        let Ok(status_text) = bounded_read_string(&path.join("status"), 64 * 1024) else {
            continue;
        };
        let Some((uid, gid)) = parse_status_identity(&status_text) else {
            continue;
        };
        let exe_basename = fs::read_link(path.join("exe"))
            .ok()
            .and_then(|path| path.file_name().map(OsStr::to_owned))
            .and_then(|name| name.to_str().map(ToOwned::to_owned));
        let reference = process_ref(&boot_id, stat.pid, stat.start_ticks);
        observations.push(IndexObservationInput {
            resource_type: IndexResourceType::Process,
            source_id: "proc".into(),
            source_kind: IndexSourceKind::Proc,
            source_generation: boot_id.clone(),
            native_identity: format!("{}:{}", stat.pid, stat.start_ticks),
            authoritative_ref: Some(reference),
            owner: Some(Principal::new(uid, gid)),
            freshness: Freshness::Recent,
            observed_at_ms,
            metadata: json!({
                "pid": stat.pid,
                "ppid": stat.ppid,
                "start_ticks": stat.start_ticks,
                "comm": stat.comm,
                "exe_basename": exe_basename,
            }),
            control_paths: vec![ControlPathKind::NativeSystem],
        });
    }
    healthy_batch(
        SuccessfulSourceBatchMeta::new(
            "proc",
            IndexSourceKind::Proc,
            None,
            boot_id,
            if truncated { "scan_truncated" } else { "ready" },
            if truncated {
                HealthState::Degraded
            } else {
                HealthState::Healthy
            },
            observed_at_ms,
        ),
        observations,
        Vec::new(),
    )
}

fn collect_applications(observed_at_ms: i64) -> SourceBatch {
    const APPLICATION_SOURCE_GENERATION: &str = "applications-v1";
    let (package_map, package_partial, package_available) = pacman_desktop_owners();
    // The later root wins for duplicate desktop IDs, giving /usr/local the
    // normal local-admin override over /usr while keeping one stable resource.
    let roots = [
        Path::new("/usr/share/applications"),
        Path::new("/usr/local/share/applications"),
    ];
    let mut files_by_id = BTreeMap::new();
    let mut discovered = 0_usize;
    let mut readable_roots = 0_usize;
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        readable_roots = readable_roots.saturating_add(1);
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(OsStr::to_str) != Some("desktop") {
                continue;
            }
            discovered = discovered.saturating_add(1);
            let Some(desktop_id) = path.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            files_by_id.insert(desktop_id.to_string(), path);
        }
    }
    if readable_roots == 0 {
        return SourceBatch::unavailable(
            "applications",
            IndexSourceKind::Applications,
            None,
            APPLICATION_SOURCE_GENERATION,
            "application_roots_unavailable",
            observed_at_ms,
        );
    }

    let files_truncated = discovered > MAX_DESKTOP_FILES;
    let mut observations = Vec::new();
    let mut read_partial = false;
    for (desktop_id, path) in files_by_id.into_iter().take(MAX_DESKTOP_FILES) {
        let Ok(contents) = bounded_read_string(&path, MAX_TEXT_FILE_BYTES) else {
            read_partial = true;
            continue;
        };
        let Some(entry) = parse_desktop_entry(&contents) else {
            continue;
        };
        let package = package_map
            .get(&path.to_string_lossy().to_string())
            .cloned();
        observations.push(IndexObservationInput {
            resource_type: IndexResourceType::ApplicationDefinition,
            source_id: "applications".into(),
            source_kind: IndexSourceKind::Applications,
            source_generation: APPLICATION_SOURCE_GENERATION.into(),
            native_identity: desktop_id.clone(),
            authoritative_ref: Some(format!("application:{desktop_id}")),
            owner: None,
            freshness: Freshness::Recent,
            observed_at_ms,
            metadata: json!({
                "desktop_id": desktop_id,
                "name": entry.name,
                "exec_basename": entry.executable_basename,
                "package": package,
                "terminal": entry.terminal,
            }),
            control_paths: vec![ControlPathKind::StructuredCli],
        });
    }

    let partial = package_partial || !package_available || files_truncated || read_partial;
    healthy_batch(
        SuccessfulSourceBatchMeta::new(
            "applications",
            IndexSourceKind::Applications,
            None,
            APPLICATION_SOURCE_GENERATION,
            if partial { "source_partial" } else { "ready" },
            if partial {
                HealthState::Degraded
            } else {
                HealthState::Healthy
            },
            observed_at_ms,
        ),
        observations,
        Vec::new(),
    )
}

fn pacman_desktop_owners() -> (HashMap<String, String>, bool, bool) {
    let root = Path::new("/var/lib/pacman/local");
    let Ok(entries) = fs::read_dir(root) else {
        return (HashMap::new(), false, false);
    };
    let mut packages = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    packages.sort();
    let truncated = packages.len() > MAX_PACKAGE_RECORDS;
    packages.truncate(MAX_PACKAGE_RECORDS);
    let mut map = HashMap::new();
    let mut partial = truncated;
    for package in packages {
        let Ok(desc) = bounded_read_string(&package.join("desc"), MAX_TEXT_FILE_BYTES) else {
            partial = true;
            continue;
        };
        let Some(name) = pacman_section_first(&desc, "%NAME%") else {
            partial = true;
            continue;
        };
        let Ok(files) = bounded_read_string(&package.join("files"), MAX_TEXT_FILE_BYTES) else {
            partial = true;
            continue;
        };
        for file in pacman_section_values(&files, "%FILES%") {
            if !file.ends_with(".desktop") {
                continue;
            }
            let absolute = format!("/{file}");
            if absolute.starts_with("/usr/share/applications/")
                || absolute.starts_with("/usr/local/share/applications/")
            {
                map.insert(absolute, name.clone());
            }
        }
    }
    (map, partial, true)
}

fn pacman_section_first(input: &str, section: &str) -> Option<String> {
    pacman_section_values(input, section).into_iter().next()
}

fn pacman_section_values(input: &str, section: &str) -> Vec<String> {
    let mut active = false;
    let mut values = Vec::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.starts_with('%') && line.ends_with('%') {
            active = line == section;
            continue;
        }
        if active && !line.is_empty() {
            values.push(line.to_string());
        }
    }
    values
}

fn collect_openrc(observed_at_ms: i64) -> SourceBatch {
    let boot_id = bounded_read_string(Path::new("/proc/sys/kernel/random/boot_id"), 4096)
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let output = match run_bounded(
        "rc-status",
        &["--all"],
        &[],
        COMMAND_TIMEOUT,
        MAX_COMMAND_OUTPUT,
    ) {
        Ok(output) => output,
        Err(reason) => {
            return SourceBatch::unavailable(
                "openrc",
                IndexSourceKind::OpenRc,
                None,
                boot_id,
                reason,
                observed_at_ms,
            );
        }
    };
    let mut services = BTreeMap::<String, (String, BTreeSet<String>)>::new();
    for service in parse_openrc_status(&output) {
        let entry = services
            .entry(service.name)
            .or_insert_with(|| (service.state.clone(), BTreeSet::new()));
        if entry.0 != service.state {
            entry.0 = "mixed".into();
        }
        if let Some(runlevel) = service.runlevel {
            entry.1.insert(runlevel);
        }
    }
    let observations = services
        .into_iter()
        .map(|(name, (state, runlevels))| IndexObservationInput {
            resource_type: IndexResourceType::OpenRcService,
            source_id: "openrc".into(),
            source_kind: IndexSourceKind::OpenRc,
            source_generation: boot_id.clone(),
            native_identity: name.clone(),
            authoritative_ref: Some(format!("openrc-service:{name}")),
            owner: None,
            freshness: Freshness::Recent,
            observed_at_ms,
            metadata: json!({
                "service": name,
                "state": state,
                "runlevels": runlevels,
            }),
            control_paths: vec![ControlPathKind::NativeSystem],
        })
        .collect();
    healthy_batch(
        SuccessfulSourceBatchMeta::new(
            "openrc",
            IndexSourceKind::OpenRc,
            None,
            boot_id,
            "ready",
            HealthState::Healthy,
            observed_at_ms,
        ),
        observations,
        Vec::new(),
    )
}

#[derive(Clone, Debug)]
struct GraphicalContext {
    generation: String,
    display: String,
    xauthority: Option<String>,
    i3_socket: String,
    boot_id: String,
}

fn collect_graphical(
    principal: Principal,
    boot_generation: &str,
    observed_at_ms: i64,
) -> (SourceBatch, SourceBatch) {
    let source_i3 = format!("i3:{}", principal.uid());
    let source_x11 = format!("x11:{}", principal.uid());
    let context = match discover_graphical_context(principal, boot_generation) {
        Ok(context) => context,
        Err(reason) => {
            return (
                SourceBatch::unavailable(
                    source_i3,
                    IndexSourceKind::I3,
                    Some(principal),
                    "none",
                    &reason,
                    observed_at_ms,
                ),
                SourceBatch::unavailable(
                    source_x11,
                    IndexSourceKind::X11,
                    Some(principal),
                    "none",
                    reason,
                    observed_at_ms,
                ),
            );
        }
    };
    let i3 = collect_i3(principal, &context, observed_at_ms);
    let x11 = collect_x11(principal, &context, observed_at_ms);
    (i3, x11)
}

fn discover_graphical_context(
    principal: Principal,
    boot_generation: &str,
) -> Result<GraphicalContext, String> {
    let entries = fs::read_dir("/proc").map_err(|_| "proc_unavailable".to_string())?;
    let mut candidates = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let path = entry.path();
        let Ok(stat_text) = bounded_read_string(&path.join("stat"), 32 * 1024) else {
            continue;
        };
        let Some(stat) = parse_proc_stat(&stat_text) else {
            continue;
        };
        if stat.comm != "i3" {
            continue;
        }
        let Ok(status_text) = bounded_read_string(&path.join("status"), 64 * 1024) else {
            continue;
        };
        let Some((uid, gid)) = parse_status_identity(&status_text) else {
            continue;
        };
        if uid != principal.uid() || gid != principal.gid() {
            continue;
        }
        let Ok(environment) = read_allowlisted_environment(&path.join("environ")) else {
            continue;
        };
        let Some(display) = environment.get("DISPLAY").cloned() else {
            continue;
        };
        let Some(i3_socket) = environment.get("I3SOCK").cloned() else {
            continue;
        };
        candidates.push((
            pid,
            stat.start_ticks,
            display,
            environment.get("XAUTHORITY").cloned(),
            i3_socket,
        ));
    }
    if candidates.is_empty() {
        return Err("no_graphical_session".into());
    }
    if candidates.len() > 1 {
        return Err("ambiguous_graphical_session".into());
    }
    let (pid, start_ticks, display, xauthority, i3_socket) = candidates.remove(0);
    Ok(GraphicalContext {
        generation: format!("{}:i3:{}:{}", boot_generation, pid, start_ticks),
        display,
        xauthority,
        i3_socket,
        boot_id: boot_generation.to_string(),
    })
}

fn read_allowlisted_environment(path: &Path) -> Result<BTreeMap<String, String>, ()> {
    let bytes = bounded_read_bytes(path, MAX_PROC_ENV_BYTES).map_err(|_| ())?;
    let mut output = BTreeMap::new();
    for raw in bytes.split(|byte| *byte == 0) {
        let Ok(value) = std::str::from_utf8(raw) else {
            continue;
        };
        let Some((key, value)) = value.split_once('=') else {
            continue;
        };
        if matches!(key, "DISPLAY" | "XAUTHORITY" | "I3SOCK") && value.len() <= 4096 {
            output.insert(key.to_string(), value.to_string());
        }
    }
    Ok(output)
}

fn collect_i3(
    principal: Principal,
    context: &GraphicalContext,
    observed_at_ms: i64,
) -> SourceBatch {
    let source_id = format!("i3:{}", principal.uid());
    let socket = context.i3_socket.as_str();
    let workspaces = match run_bounded(
        "i3-msg",
        &["-s", socket, "-t", "get_workspaces"],
        &[],
        COMMAND_TIMEOUT,
        MAX_COMMAND_OUTPUT,
    )
    .ok()
    .and_then(|output| parse_i3_workspaces(&output))
    {
        Some(value) => value,
        None => {
            return SourceBatch::unavailable(
                source_id,
                IndexSourceKind::I3,
                Some(principal),
                context.generation.clone(),
                "i3_query_failed",
                observed_at_ms,
            );
        }
    };
    let outputs = match run_bounded(
        "i3-msg",
        &["-s", socket, "-t", "get_outputs"],
        &[],
        COMMAND_TIMEOUT,
        MAX_COMMAND_OUTPUT,
    )
    .ok()
    .and_then(|output| parse_i3_outputs(&output))
    {
        Some(value) => value,
        None => {
            return SourceBatch::unavailable(
                source_id,
                IndexSourceKind::I3,
                Some(principal),
                context.generation.clone(),
                "i3_query_failed",
                observed_at_ms,
            );
        }
    };
    let placements = run_bounded(
        "i3-msg",
        &["-s", socket, "-t", "get_tree"],
        &[],
        COMMAND_TIMEOUT,
        MAX_COMMAND_OUTPUT,
    )
    .ok()
    .and_then(|output| parse_i3_tree_placements(&output))
    .unwrap_or_default();

    let mut observations = Vec::new();
    let mut relations = Vec::new();
    for display in outputs {
        let display_ref = format!("display:{}:{}", context.generation, display.name);
        observations.push(IndexObservationInput {
            resource_type: IndexResourceType::Display,
            source_id: source_id.clone(),
            source_kind: IndexSourceKind::I3,
            source_generation: context.generation.clone(),
            native_identity: format!("display:{}", display.name),
            authoritative_ref: Some(display_ref),
            owner: Some(principal),
            freshness: Freshness::Recent,
            observed_at_ms,
            metadata: json!({
                "name": display.name,
                "active": display.active,
                "primary": display.primary,
                "current_workspace": display.current_workspace,
            }),
            control_paths: vec![ControlPathKind::NativeSystem],
        });
    }
    for workspace in workspaces {
        let workspace_ref = format!("workspace:{}:{}", context.generation, workspace.name);
        let display_ref = format!("display:{}:{}", context.generation, workspace.output);
        observations.push(IndexObservationInput {
            resource_type: IndexResourceType::Workspace,
            source_id: source_id.clone(),
            source_kind: IndexSourceKind::I3,
            source_generation: context.generation.clone(),
            native_identity: format!("workspace:{}", workspace.name),
            authoritative_ref: Some(workspace_ref.clone()),
            owner: Some(principal),
            freshness: Freshness::Recent,
            observed_at_ms,
            metadata: json!({
                "num": workspace.num,
                "name": workspace.name,
                "visible": workspace.visible,
                "focused": workspace.focused,
                "urgent": workspace.urgent,
                "output": workspace.output,
            }),
            control_paths: vec![ControlPathKind::NativeSystem],
        });
        relations.push(IndexRelationInput {
            from_authoritative_ref: workspace_ref,
            to_authoritative_ref: display_ref,
            relation_kind: "workspace_display".into(),
            evidence_strength: EvidenceStrength::Authoritative,
            source_id: source_id.clone(),
            source_kind: IndexSourceKind::I3,
            reason_code: "i3_workspace_output".into(),
            observed_at_ms,
        });
    }
    for placement in placements {
        relations.push(IndexRelationInput {
            from_authoritative_ref: window_ref(&context.generation, placement.xid),
            to_authoritative_ref: format!(
                "workspace:{}:{}",
                context.generation, placement.workspace
            ),
            relation_kind: "window_workspace".into(),
            evidence_strength: EvidenceStrength::Authoritative,
            source_id: source_id.clone(),
            source_kind: IndexSourceKind::I3,
            reason_code: "i3_tree_placement".into(),
            observed_at_ms,
        });
    }
    healthy_batch(
        SuccessfulSourceBatchMeta::new(
            source_id,
            IndexSourceKind::I3,
            Some(principal),
            context.generation.clone(),
            "ready",
            HealthState::Healthy,
            observed_at_ms,
        ),
        observations,
        relations,
    )
}

fn collect_x11(
    principal: Principal,
    context: &GraphicalContext,
    observed_at_ms: i64,
) -> SourceBatch {
    let source_id = format!("x11:{}", principal.uid());
    let mut env = vec![("DISPLAY", context.display.as_str())];
    if let Some(xauthority) = context.xauthority.as_deref() {
        env.push(("XAUTHORITY", xauthority));
    }
    let client_output = match run_bounded(
        "xprop",
        &["-root", "_NET_CLIENT_LIST"],
        &env,
        COMMAND_TIMEOUT,
        MAX_COMMAND_OUTPUT,
    ) {
        Ok(output) => output,
        Err(reason) => {
            return SourceBatch::unavailable(
                source_id,
                IndexSourceKind::X11,
                Some(principal),
                context.generation.clone(),
                reason,
                observed_at_ms,
            );
        }
    };
    let mut xids = parse_xprop_client_list(&client_output);
    let truncated = xids.len() > MAX_WINDOWS;
    xids.truncate(MAX_WINDOWS);
    let mut observations = Vec::new();
    for xid in xids {
        let id = format!("0x{xid:x}");
        let Ok(output) = run_bounded(
            "xprop",
            &[
                "-id",
                &id,
                "_NET_WM_PID",
                "WM_CLASS",
                "_NET_WM_NAME",
                "WM_NAME",
                "_NET_WM_STATE",
            ],
            &env,
            COMMAND_TIMEOUT,
            256 * 1024,
        ) else {
            continue;
        };
        let properties = parse_xprop_window(&output);
        let process_ref = properties
            .pid
            .and_then(|pid| process_reference_if_owned(pid, principal, &context.boot_id));
        observations.push(IndexObservationInput {
            resource_type: IndexResourceType::Window,
            source_id: source_id.clone(),
            source_kind: IndexSourceKind::X11,
            source_generation: context.generation.clone(),
            native_identity: format!("xid:{xid}"),
            authoritative_ref: Some(window_ref(&context.generation, xid)),
            owner: Some(principal),
            freshness: Freshness::Recent,
            observed_at_ms,
            metadata: json!({
                "xid": xid,
                "pid": properties.pid,
                "process_ref": process_ref,
                "class": truncate_string(properties.class, 128),
                "instance": truncate_string(properties.instance, 128),
                "title": truncate_string(properties.title, 256),
                "hidden": properties.hidden,
            }),
            control_paths: vec![
                ControlPathKind::ProcessWindow,
                ControlPathKind::VisualFallback,
            ],
        });
    }
    healthy_batch(
        SuccessfulSourceBatchMeta::new(
            source_id,
            IndexSourceKind::X11,
            Some(principal),
            context.generation.clone(),
            if truncated { "scan_truncated" } else { "ready" },
            if truncated {
                HealthState::Degraded
            } else {
                HealthState::Healthy
            },
            observed_at_ms,
        ),
        observations,
        Vec::new(),
    )
}

fn process_reference_if_owned(pid: u32, principal: Principal, boot_id: &str) -> Option<String> {
    let root = PathBuf::from(format!("/proc/{pid}"));
    let stat_text = bounded_read_string(&root.join("stat"), 32 * 1024).ok()?;
    let stat = parse_proc_stat(&stat_text)?;
    let status_text = bounded_read_string(&root.join("status"), 64 * 1024).ok()?;
    let (uid, gid) = parse_status_identity(&status_text)?;
    (uid == principal.uid() && gid == principal.gid())
        .then(|| process_ref(boot_id, pid, stat.start_ticks))
}

fn process_ref(boot_id: &str, pid: u32, start_ticks: u64) -> String {
    format!("process:{boot_id}:{pid}:{start_ticks}")
}

fn window_ref(graph_generation: &str, xid: u64) -> String {
    format!("window:{graph_generation}:{xid}")
}

struct SuccessfulSourceBatchMeta {
    source_id: String,
    source_kind: IndexSourceKind,
    owner: Option<Principal>,
    source_generation: String,
    reason_code: String,
    health: HealthState,
    observed_at_ms: i64,
}

impl SuccessfulSourceBatchMeta {
    fn new(
        source_id: impl Into<String>,
        source_kind: IndexSourceKind,
        owner: Option<Principal>,
        generation: impl Into<String>,
        reason_code: impl Into<String>,
        health: HealthState,
        observed_at_ms: i64,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            source_kind,
            owner,
            source_generation: generation.into(),
            reason_code: reason_code.into(),
            health,
            observed_at_ms,
        }
    }
}

fn healthy_batch(
    meta: SuccessfulSourceBatchMeta,
    observations: Vec<IndexObservationInput>,
    relations: Vec<IndexRelationInput>,
) -> SourceBatch {
    SourceBatch {
        status: IndexSourceStatus {
            source_id: meta.source_id,
            source_kind: meta.source_kind,
            owner: meta.owner,
            source_generation: meta.source_generation,
            health: meta.health,
            reason_code: meta.reason_code,
            last_attempt_at_ms: meta.observed_at_ms,
            last_success_at_ms: Some(meta.observed_at_ms),
        },
        observations,
        relations,
    }
}

fn bounded_read_string(path: &Path, max_bytes: u64) -> Result<String, ()> {
    let bytes = bounded_read_bytes(path, max_bytes)?;
    String::from_utf8(bytes).map_err(|_| ())
}

fn bounded_read_bytes(path: &Path, max_bytes: u64) -> Result<Vec<u8>, ()> {
    let mut file = File::open(path).map_err(|_| ())?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > max_bytes {
        return Err(());
    }
    Ok(bytes)
}

fn run_bounded(
    program: &str,
    args: &[&str],
    environment: &[(&str, &str)],
    timeout: Duration,
    max_output: usize,
) -> Result<String, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, value) in environment {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "command_unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "command_io_failed".to_string())?;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut reader = stdout;
        let mut retained = Vec::new();
        let mut buffer = [0_u8; 8192];
        let mut too_large = false;
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if retained.len() < max_output.saturating_add(1) {
                        let remaining = max_output.saturating_add(1) - retained.len();
                        retained.extend_from_slice(&buffer[..count.min(remaining)]);
                    }
                    if retained.len() > max_output {
                        too_large = true;
                    }
                }
                Err(_) => {
                    let _ = sender.send(Err("command_io_failed".to_string()));
                    return;
                }
            }
        }
        if too_large {
            let _ = sender.send(Err("command_output_too_large".to_string()));
        } else {
            let _ = sender.send(
                String::from_utf8(retained).map_err(|_| "command_output_invalid".to_string()),
            );
        }
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("command_timeout".into());
            }
            Err(_) => return Err("command_io_failed".into()),
        }
    };
    if !status.success() {
        return Err("command_failed".into());
    }
    let output = receiver
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| "command_io_failed".to_string())??;
    Ok(output)
}

fn truncate_string(value: Option<String>, max_chars: usize) -> Option<String> {
    value.map(|value| value.chars().take(max_chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacman_section_parser_is_bounded_to_requested_section() {
        let input = "%NAME%\ndemo\n\n%FILES%\nusr/share/applications/demo.desktop\nusr/bin/demo\n\n%BACKUP%\nfoo\n";
        assert_eq!(
            pacman_section_first(input, "%NAME%").as_deref(),
            Some("demo")
        );
        assert_eq!(
            pacman_section_values(input, "%FILES%"),
            vec!["usr/share/applications/demo.desktop", "usr/bin/demo"]
        );
    }

    #[test]
    fn truncation_never_expands_sensitive_string() {
        assert_eq!(
            truncate_string(Some("abcdef".into()), 3).as_deref(),
            Some("abc")
        );
    }
}
