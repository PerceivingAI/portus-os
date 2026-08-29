use crate::{
    ArtifactCommand, CLI_OUTPUT_SCHEMA_VERSION, CapabilityCommand, CapabilityProviderCommand, Cli,
    CliError, CliMeta, CliSuccess, Command, DoctorContext, HealthCommand, IndexCommand,
    IndexQueryArgs, OutputMode, PaginationArgs, PolicyAdminCommand, PolicyBundleCommand,
    PolicyCommand, PrivilegeTransport, RuntimeTransport, TaskCommand,
};
use portus_protocol::{CURRENT_PROTOCOL_VERSION, SemanticErrorCode};
use serde_json::{Value, json};
use std::time::Duration;

pub struct ExecutionContext<'a> {
    pub runtime: &'a mut dyn RuntimeTransport,
    pub privilege: &'a mut dyn PrivilegeTransport,
    pub doctor: &'a DoctorContext,
}

pub fn execute(cli: &Cli, context: &mut ExecutionContext<'_>) -> Result<CliSuccess, CliError> {
    if cli.output_mode() == OutputMode::Jsonl && !cli.command.supports_jsonl() {
        return Err(CliError::new(
            cli.command.command_id(),
            SemanticErrorCode::UnsupportedOutputMode,
            "this command does not support JSONL output",
        ));
    }
    let timeout = Duration::from_millis(cli.timeout_ms);
    match &cli.command {
        Command::Status => status(context.runtime, timeout),
        Command::Doctor { domain, bundle } => doctor(context.doctor, *domain, bundle.as_deref()),
        Command::Index { command } => index(command, context.runtime, timeout),
        Command::Task { command } => task(command, context.runtime, timeout),
        Command::Capability { command } => capability(command, context.runtime, timeout),
        Command::Policy { command } => policy(command, context.runtime, context.privilege, timeout),
        Command::Artifact { command } => artifact(command, context.runtime, timeout),
        Command::Health { command: None } => health_list(context.runtime, timeout),
        Command::Health {
            command: Some(HealthCommand::Show { component_ref }),
        } => health_show(context.runtime, component_ref, timeout),
        Command::Health {
            command: Some(HealthCommand::Degraded),
        } => health_degraded(context.runtime, timeout),
        Command::Help => help_contract(),
        Command::Version => version(),
    }
}

fn status(runtime: &mut dyn RuntimeTransport, timeout: Duration) -> Result<CliSuccess, CliError> {
    let reply = runtime
        .request("runtime.status", json!({}), timeout)
        .map_err(|mut error| {
            error.command = "status".into();
            if error.semantic.code == SemanticErrorCode::DaemonUnavailable {
                error.human_hint =
                    Some("Run `portus-os doctor runtime` for daemon-independent diagnosis.".into());
            }
            error
        })?;
    let readiness = reply
        .data
        .get("readiness")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let health = reply
        .data
        .get("health")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let schema = reply
        .data
        .get("schema_version")
        .and_then(Value::as_u64)
        .map_or_else(|| "-".into(), |value| value.to_string());
    let provider_health = reply
        .data
        .get("provider_registry")
        .and_then(|value| value.get("health"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let provider_count = reply
        .data
        .get("provider_registry")
        .and_then(|value| value.get("active_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let index_state = reply
        .data
        .get("index")
        .and_then(|value| value.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let index_reason = reply
        .data
        .get("index")
        .and_then(|value| value.get("reason_code"))
        .and_then(Value::as_str)
        .unwrap_or("status_unavailable");
    let task_state = reply
        .data
        .get("tasks")
        .and_then(|value| value.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let task_active = reply
        .data
        .get("tasks")
        .and_then(|value| value.get("active"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let task_terminal = reply
        .data
        .get("tasks")
        .and_then(|value| value.get("terminal"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let policy_health = reply
        .data
        .get("policy")
        .and_then(|value| value.get("health"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let policy_reason = reply
        .data
        .get("policy")
        .and_then(|value| value.get("reason_code"))
        .and_then(Value::as_str)
        .unwrap_or("not_loaded");
    Ok(CliSuccess {
        command: "status",
        data: json!({
            "runtime": reply.data,
            "implemented_domains": ["runtime", "state", "capability-provider-registry", "system-index", "tasks", "policy", "health-recovery", "artifact-registry"],
        }),
        meta: reply.meta,
        human: vec![
            format!("portusd   {health} ({readiness})"),
            format!("state     schema {schema}"),
            format!("index     {index_state} ({index_reason})"),
            format!("providers {provider_health} ({provider_count} registered)"),
            format!("tasks     {task_state} ({task_active} active, {task_terminal} terminal)"),
            format!("policy    {policy_health} ({policy_reason})"),
        ],
    })
}

fn health_list(
    runtime: &mut dyn RuntimeTransport,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    let reply = runtime
        .request("health.list", json!({}), timeout)
        .map_err(|mut error| {
            error.command = "health".into();
            error
        })?;
    let degraded = reply
        .data
        .get("degraded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let human = health_component_lines(
        reply.data.get("components").and_then(Value::as_array),
        "No visible health components.",
    );
    let mut meta = reply.meta;
    meta.degraded = Some(degraded);
    Ok(CliSuccess {
        command: "health",
        data: reply.data,
        meta,
        human,
    })
}

fn health_show(
    runtime: &mut dyn RuntimeTransport,
    component_ref: &str,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    let reply = runtime
        .request(
            "health.show",
            json!({"component_ref":component_ref}),
            timeout,
        )
        .map_err(|mut error| {
            error.command = "health.show".into();
            error
        })?;
    let human = vec![format_health_component(&reply.data)];
    Ok(CliSuccess {
        command: "health.show",
        data: reply.data,
        meta: reply.meta,
        human,
    })
}

fn health_degraded(
    runtime: &mut dyn RuntimeTransport,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    let reply = runtime
        .request("health.degraded", json!({}), timeout)
        .map_err(|mut error| {
            error.command = "health.degraded".into();
            error
        })?;
    let human = health_component_lines(
        reply.data.get("components").and_then(Value::as_array),
        "No degraded or unavailable components.",
    );
    let mut meta = reply.meta;
    meta.degraded = Some(reply.data.get("count").and_then(Value::as_u64).unwrap_or(0) > 0);
    Ok(CliSuccess {
        command: "health.degraded",
        data: reply.data,
        meta,
        human,
    })
}

fn health_component_lines(components: Option<&Vec<Value>>, empty: &str) -> Vec<String> {
    match components {
        Some(components) if !components.is_empty() => {
            components.iter().map(format_health_component).collect()
        }
        _ => vec![empty.into()],
    }
}

fn format_health_component(item: &Value) -> String {
    format!(
        "{}  {}  {}  recovery={}",
        item.get("component_ref")
            .and_then(Value::as_str)
            .unwrap_or("-"),
        item.get("health_state")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        item.get("reason_code")
            .and_then(Value::as_str)
            .unwrap_or("status_unavailable"),
        item.get("recovery_disposition")
            .and_then(Value::as_str)
            .unwrap_or("observe")
    )
}

fn artifact(
    command: &ArtifactCommand,
    runtime: &mut dyn RuntimeTransport,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    match command {
        ArtifactCommand::List(args) => {
            let reply = runtime
                .request(
                    "artifact.list",
                    json!({"limit":args.limit,"cursor":args.cursor}),
                    timeout,
                )
                .map_err(|mut error| {
                    error.command = "artifact.list".into();
                    error
                })?;
            paged_success(
                "artifact.list",
                reply,
                |item| {
                    let artifact_id = item
                        .get("artifact_id")
                        .and_then(Value::as_str)
                        .unwrap_or("-");
                    let artifact_type = item
                        .get("artifact_type")
                        .and_then(Value::as_str)
                        .unwrap_or("other");
                    let availability = item
                        .get("availability_state")
                        .and_then(Value::as_str)
                        .unwrap_or("unavailable");
                    let integrity = item
                        .get("integrity_kind")
                        .and_then(Value::as_str)
                        .unwrap_or("unverified");
                    let label = item
                        .get("safe_display_name")
                        .and_then(Value::as_str)
                        .unwrap_or("-");
                    format!("{artifact_id}  {artifact_type}  {availability}/{integrity}  {label}")
                },
                "No visible registered artifacts.",
            )
        }
        ArtifactCommand::Show { artifact_id } => {
            let reply = runtime
                .request("artifact.show", json!({"artifact_id":artifact_id}), timeout)
                .map_err(|mut error| {
                    error.command = "artifact.show".into();
                    error
                })?;
            let artifact = reply.data.get("artifact").unwrap_or(&reply.data);
            let id = artifact
                .get("artifact_id")
                .and_then(Value::as_str)
                .unwrap_or("-")
                .to_string();
            let artifact_type = artifact
                .get("artifact_type")
                .and_then(Value::as_str)
                .unwrap_or("other")
                .to_string();
            let availability = artifact
                .get("availability_state")
                .and_then(Value::as_str)
                .unwrap_or("unavailable")
                .to_string();
            let integrity = artifact
                .get("integrity_kind")
                .and_then(Value::as_str)
                .unwrap_or("unverified")
                .to_string();
            let confidentiality = artifact
                .get("confidentiality")
                .and_then(Value::as_str)
                .unwrap_or("private")
                .to_string();
            let retention = artifact
                .get("retention_kind")
                .and_then(Value::as_str)
                .unwrap_or("retained")
                .to_string();
            let locator = artifact
                .get("locator")
                .map(artifact_locator_human)
                .unwrap_or_else(|| "locator -".into());
            Ok(CliSuccess {
                command: "artifact.show",
                data: reply.data,
                meta: reply.meta,
                human: vec![
                    format!("artifact       {id}"),
                    format!("type           {artifact_type}"),
                    format!("state          {availability}/{integrity}"),
                    format!("confidential   {confidentiality}"),
                    format!("retention      {retention}"),
                    locator,
                ],
            })
        }
    }
}

fn artifact_locator_human(locator: &Value) -> String {
    match locator.get("kind").and_then(Value::as_str) {
        Some("filesystem") => format!(
            "filesystem     {}",
            locator.get("path").and_then(Value::as_str).unwrap_or("-")
        ),
        Some("provider_resource") => {
            let reference = locator.get("reference").unwrap_or(&Value::Null);
            format!(
                "provider       {}  {}",
                reference
                    .get("provider_registration_id")
                    .and_then(Value::as_str)
                    .unwrap_or("-"),
                reference
                    .get("resource_type")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
            )
        }
        _ => "locator        -".into(),
    }
}

fn task(
    command: &TaskCommand,
    runtime: &mut dyn RuntimeTransport,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    match command {
        TaskCommand::List(args) => {
            let state = args.state.map(|state| state.as_wire());
            let reply = runtime
                .request(
                    "task.list",
                    json!({
                        "limit": args.pagination.limit,
                        "cursor": args.pagination.cursor,
                        "state": state,
                        "project_ref": args.project,
                    }),
                    timeout,
                )
                .map_err(|mut error| {
                    error.command = "task.list".into();
                    error
                })?;
            paged_success(
                "task.list",
                reply,
                |item| {
                    let task_id = item.get("task_id").and_then(Value::as_str).unwrap_or("-");
                    let state = item
                        .get("state")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let label = item
                        .get("title")
                        .and_then(Value::as_str)
                        .or_else(|| item.get("objective_summary").and_then(Value::as_str))
                        .unwrap_or("-");
                    format!("{task_id}  {state}  {label}")
                },
                "No visible tasks.",
            )
        }
        TaskCommand::Show { task_id } => {
            let reply = runtime
                .request("task.show", json!({"task_id":task_id}), timeout)
                .map_err(|mut error| {
                    error.command = "task.show".into();
                    error
                })?;
            let state = reply
                .data
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let objective = reply
                .data
                .get("objective_summary")
                .and_then(Value::as_str)
                .unwrap_or("-")
                .to_string();
            let reason = reply
                .data
                .get("state_reason")
                .and_then(Value::as_str)
                .unwrap_or("-")
                .to_string();
            let relationships = reply
                .data
                .get("relationships")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let result = reply
                .data
                .get("result_summary")
                .and_then(Value::as_str)
                .unwrap_or("-")
                .to_string();
            Ok(CliSuccess {
                command: "task.show",
                data: reply.data,
                meta: reply.meta,
                human: vec![
                    format!("task           {task_id}"),
                    format!("state          {state}"),
                    format!("objective      {objective}"),
                    format!("reason         {reason}"),
                    format!("relationships  {relationships}"),
                    format!("result         {result}"),
                ],
            })
        }
        TaskCommand::Events {
            task_id,
            after,
            limit,
            follow,
        } => {
            if *follow {
                return unsupported(
                    "task.events",
                    "live task event following must use the streaming application path",
                );
            }
            let mut reply = runtime
                .request(
                    "task.events",
                    json!({
                        "task_id":task_id,
                        "after_sequence":after,
                        "limit":limit,
                    }),
                    timeout,
                )
                .map_err(|mut error| {
                    error.command = "task.events".into();
                    error
                })?;
            let next_sequence = reply.data.get("next_sequence").and_then(Value::as_u64);
            let events = reply
                .data
                .get("events")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if let Some(object) = reply.data.as_object_mut() {
                object.remove("next_sequence");
            }
            if let Some(next_sequence) = next_sequence {
                reply
                    .meta
                    .extra
                    .insert("next_sequence".into(), json!(next_sequence));
            }
            let human = if events.is_empty() {
                vec!["No retained task events after the requested sequence.".into()]
            } else {
                events
                    .iter()
                    .map(|event| {
                        let sequence = event.get("sequence").and_then(Value::as_u64).unwrap_or(0);
                        let kind = event
                            .get("event_kind")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown");
                        let summary = event
                            .get("safe_summary")
                            .and_then(Value::as_str)
                            .unwrap_or("-");
                        format!("{sequence}  {kind}  {summary}")
                    })
                    .collect()
            };
            Ok(CliSuccess {
                command: "task.events",
                data: reply.data,
                meta: reply.meta,
                human,
            })
        }
        TaskCommand::Cancel { task_id, if_state } => {
            let if_state = if_state.map(|state| state.as_wire());
            let reply = runtime
                .request(
                    "task.cancel",
                    json!({"task_id":task_id,"if_state":if_state}),
                    timeout,
                )
                .map_err(|mut error| {
                    error.command = "task.cancel".into();
                    error
                })?;
            let state = reply
                .data
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let reason = reply
                .data
                .get("state_reason")
                .and_then(Value::as_str)
                .unwrap_or("-")
                .to_string();
            Ok(CliSuccess {
                command: "task.cancel",
                data: reply.data,
                meta: reply.meta,
                human: vec![
                    format!("task   {task_id}"),
                    format!("state  {state}"),
                    format!("reason {reason}"),
                ],
            })
        }
    }
}

fn index(
    command: &IndexCommand,
    runtime: &mut dyn RuntimeTransport,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    match command {
        IndexCommand::Apps(pagination) => index_quick(
            runtime,
            pagination,
            "index.apps",
            Some("application_instance"),
            Some("recent"),
            timeout,
        ),
        IndexCommand::Windows(pagination) => index_quick(
            runtime,
            pagination,
            "index.windows",
            Some("window"),
            Some("recent"),
            timeout,
        ),
        IndexCommand::Workspaces(pagination) => index_quick(
            runtime,
            pagination,
            "index.workspaces",
            Some("workspace"),
            Some("recent"),
            timeout,
        ),
        IndexCommand::Displays(pagination) => index_quick(
            runtime,
            pagination,
            "index.displays",
            Some("display"),
            Some("recent"),
            timeout,
        ),
        IndexCommand::Providers(pagination) => index_quick(
            runtime,
            pagination,
            "index.providers",
            Some("provider_registration"),
            Some("recent"),
            timeout,
        ),
        IndexCommand::Stale(pagination) => index_quick(
            runtime,
            pagination,
            "index.stale",
            None,
            Some("stale"),
            timeout,
        ),
        IndexCommand::Query(args) => index_query(runtime, args, timeout),
        IndexCommand::Show { resource_ref } => {
            let reply = runtime
                .request("index.show", json!({"resource_ref":resource_ref}), timeout)
                .map_err(|mut error| {
                    error.command = "index.show".into();
                    error
                })?;
            index_resource_success("index.show", reply)
        }
        IndexCommand::Topology {
            resource_ref,
            depth,
            limit,
        } => {
            let reply = runtime
                .request(
                    "index.topology",
                    json!({"resource_ref":resource_ref,"depth":depth,"limit":limit}),
                    timeout,
                )
                .map_err(|mut error| {
                    error.command = "index.topology".into();
                    error
                })?;
            let root = reply
                .data
                .get("root")
                .map(index_resource_line)
                .unwrap_or_else(|| "topology root unavailable".into());
            let resource_count = reply
                .data
                .get("resources")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let relation_count = reply
                .data
                .get("relations")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let truncated = reply
                .data
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(CliSuccess {
                command: "index.topology",
                data: reply.data,
                meta: reply.meta,
                human: vec![
                    root,
                    format!("related resources  {resource_count}"),
                    format!("relations          {relation_count}"),
                    format!("truncated          {truncated}"),
                ],
            })
        }
        IndexCommand::Refresh { resource_ref } => {
            let reply = runtime
                .request(
                    "index.refresh",
                    json!({"resource_ref":resource_ref}),
                    timeout,
                )
                .map_err(|mut error| {
                    error.command = "index.refresh".into();
                    error
                })?;
            index_resource_success("index.refresh", reply)
        }
        IndexCommand::Rescan { domain } => index_control_request(
            runtime,
            "index.rescan",
            json!({"domain":domain.as_wire()}),
            timeout,
        ),
        IndexCommand::Reconcile => {
            index_control_request(runtime, "index.reconcile", json!({}), timeout)
        }
        IndexCommand::Rebuild => {
            index_control_request(runtime, "index.rebuild", json!({}), timeout)
        }
        IndexCommand::Status => index_control_request(runtime, "index.status", json!({}), timeout),
    }
}

fn index_quick(
    runtime: &mut dyn RuntimeTransport,
    pagination: &PaginationArgs,
    command: &'static str,
    resource_type: Option<&'static str>,
    freshness: Option<&'static str>,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    let reply = runtime
        .request(
            "index.query",
            json!({
                "limit":pagination.limit,
                "cursor":pagination.cursor,
                "resource_type":resource_type,
                "freshness":freshness
            }),
            timeout,
        )
        .map_err(|mut error| {
            error.command = command.into();
            error
        })?;
    index_page_success(command, reply)
}

fn index_query(
    runtime: &mut dyn RuntimeTransport,
    args: &IndexQueryArgs,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    let reply = runtime
        .request(
            "index.query",
            json!({
                "limit":args.pagination.limit,
                "cursor":args.pagination.cursor,
                "resource_type":args.resource_type.map(crate::IndexResourceArg::as_wire),
                "freshness":args.freshness.map(crate::IndexFreshnessArg::as_wire),
                "source_kind":args.source_kind.map(crate::IndexSourceArg::as_wire),
                "application":args.application,
                "provider":args.provider,
                "capability":args.capability,
                "workspace":args.workspace,
                "display":args.display,
                "evidence":args.evidence.map(crate::IndexEvidenceArg::as_wire),
                "changed_since_ms":args.changed_since_ms,
                "control_path":args.control_path.map(crate::IndexControlPathArg::as_wire),
            }),
            timeout,
        )
        .map_err(|mut error| {
            error.command = "index.query".into();
            error
        })?;
    index_page_success("index.query", reply)
}

fn index_page_success(
    command: &'static str,
    mut reply: crate::RuntimeReply,
) -> Result<CliSuccess, CliError> {
    let next_cursor = reply
        .data
        .get("next_cursor")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let partial = reply
        .data
        .get("partial")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let items = reply
        .data
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(object) = reply.data.as_object_mut() {
        object.remove("next_cursor");
    }
    reply.meta.next_cursor = next_cursor;
    reply.meta.degraded = Some(partial);
    let human = if items.is_empty() {
        vec![if partial {
            "No matching resources in the currently available index sources.".into()
        } else {
            "No matching indexed resources.".into()
        }]
    } else {
        items.iter().map(index_resource_line).collect()
    };
    Ok(CliSuccess {
        command,
        data: reply.data,
        meta: reply.meta,
        human,
    })
}

fn index_resource_success(
    command: &'static str,
    reply: crate::RuntimeReply,
) -> Result<CliSuccess, CliError> {
    let resource = reply.data.get("resource").unwrap_or(&reply.data);
    let mut human = vec![index_resource_line(resource)];
    if let Some(relations) = reply.data.get("relations").and_then(Value::as_array) {
        human.push(format!("relations  {}", relations.len()));
    }
    Ok(CliSuccess {
        command,
        data: reply.data,
        meta: reply.meta,
        human,
    })
}

fn index_control_request(
    runtime: &mut dyn RuntimeTransport,
    command: &'static str,
    params: Value,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    let reply = runtime
        .request(command, params, timeout)
        .map_err(|mut error| {
            error.command = command.into();
            error
        })?;
    let state = reply
        .data
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let reason = reply
        .data
        .get("reason_code")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let sources = reply
        .data
        .get("sources")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut meta = reply.meta;
    meta.degraded = Some(matches!(state.as_str(), "degraded" | "unavailable"));
    Ok(CliSuccess {
        command,
        data: reply.data,
        meta,
        human: vec![
            format!("index    {state}"),
            format!("reason   {reason}"),
            format!("sources  {sources}"),
        ],
    })
}

fn index_resource_line(value: &Value) -> String {
    let handle = value
        .get("index_handle")
        .and_then(Value::as_str)
        .unwrap_or("idx_-");
    let resource_type = value
        .get("resource_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let freshness = value
        .get("freshness")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let label = value
        .get("metadata")
        .and_then(|metadata| {
            [
                "name",
                "application_name",
                "provider_type",
                "capability_id",
                "comm",
            ]
            .into_iter()
            .find_map(|key| metadata.get(key).and_then(Value::as_str))
        })
        .or_else(|| value.get("authoritative_ref").and_then(Value::as_str))
        .unwrap_or("-");
    format!("{handle}  {resource_type}  {freshness}  {label}")
}

fn capability(
    command: &CapabilityCommand,
    runtime: &mut dyn RuntimeTransport,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    match command {
        CapabilityCommand::List(pagination) => capability_list(runtime, pagination, timeout),
        CapabilityCommand::Show { capability_id } => {
            let reply = runtime
                .request(
                    "capability.show",
                    json!({"capability_id": capability_id}),
                    timeout,
                )
                .map_err(|mut error| {
                    error.command = "capability.show".into();
                    error
                })?;
            let mut human = vec![format!("capability  {capability_id}")];
            if let Some(providers) = reply.data.get("providers").and_then(Value::as_array) {
                for provider in providers {
                    human.push(format!(
                        "provider    {}  {}  health={} compatibility={}",
                        provider
                            .get("provider_id")
                            .and_then(Value::as_str)
                            .unwrap_or("-"),
                        provider
                            .get("provider_type")
                            .and_then(Value::as_str)
                            .unwrap_or("-"),
                        provider
                            .get("health_state")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown"),
                        provider
                            .get("compatibility_state")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                    ));
                }
            }
            Ok(CliSuccess {
                command: "capability.show",
                data: reply.data,
                meta: reply.meta,
                human,
            })
        }
        CapabilityCommand::Provider { command } => match command {
            CapabilityProviderCommand::List(pagination) => {
                capability_provider_list(runtime, pagination, timeout)
            }
            CapabilityProviderCommand::Show { provider_id } => {
                let reply = runtime
                    .request(
                        "capability.provider.show",
                        json!({"provider_id": provider_id}),
                        timeout,
                    )
                    .map_err(|mut error| {
                        error.command = "capability.provider.show".into();
                        error
                    })?;
                let provider_type = reply
                    .data
                    .get("provider_type")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string();
                let label = reply
                    .data
                    .get("display_label")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string();
                let health = reply
                    .data
                    .get("health_state")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let compatibility = reply
                    .data
                    .get("compatibility_state")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let registration_state = if reply
                    .data
                    .get("tombstone")
                    .is_some_and(|value| !value.is_null())
                {
                    "removed"
                } else {
                    "active"
                };
                Ok(CliSuccess {
                    command: "capability.provider.show",
                    data: reply.data,
                    meta: reply.meta,
                    human: vec![
                        format!("provider       {provider_id}"),
                        format!("type           {provider_type}"),
                        format!("label          {label}"),
                        format!("state          {registration_state}"),
                        format!("health         {health}"),
                        format!("compatibility  {compatibility}"),
                    ],
                })
            }
        },
    }
}

fn policy(
    command: &PolicyCommand,
    runtime: &mut dyn RuntimeTransport,
    privilege: &mut dyn PrivilegeTransport,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    match command {
        PolicyCommand::Effective => {
            let reply = runtime
                .request("policy.effective", json!({}), timeout)
                .map_err(|mut error| {
                    error.command = "policy.effective".into();
                    error
                })?;
            let uid = reply
                .data
                .get("principal")
                .and_then(|value| value.get("uid"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let gid = reply
                .data
                .get("principal")
                .and_then(|value| value.get("gid"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let bundles = reply
                .data
                .get("bundles")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let grants = reply
                .data
                .get("grants")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let root_equivalent = reply
                .data
                .get("has_root_equivalent_authority")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(CliSuccess {
                command: "policy.effective",
                data: reply.data,
                meta: reply.meta,
                human: vec![
                    format!("principal       {uid}:{gid}"),
                    format!("bundles         {bundles}"),
                    format!("effective grants {grants}"),
                    format!("root-equivalent {root_equivalent}"),
                ],
            })
        }
        PolicyCommand::Check { action, resource } => {
            let reply = runtime
                .request(
                    "policy.check",
                    json!({"action":action,"resource":resource}),
                    timeout,
                )
                .map_err(|mut error| {
                    error.command = "policy.check".into();
                    error
                })?;
            let effect = reply
                .data
                .get("effect")
                .and_then(Value::as_str)
                .unwrap_or("reject")
                .to_string();
            let reason = reply
                .data
                .get("reason_code")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            Ok(CliSuccess {
                command: "policy.check",
                data: reply.data,
                meta: reply.meta,
                human: vec![
                    format!("action    {action}"),
                    format!("resource  {}", resource.as_deref().unwrap_or("-")),
                    format!("effect    {effect}"),
                    format!("reason    {reason}"),
                ],
            })
        }
        PolicyCommand::Admin { command } => policy_admin(command, privilege, timeout),
    }
}

fn policy_admin(
    command: &PolicyAdminCommand,
    privilege: &mut dyn PrivilegeTransport,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    let (method, params) = match command {
        PolicyAdminCommand::Show { uid } => ("policy.admin.show", json!({"uid":uid})),
        PolicyAdminCommand::Grant {
            uid,
            action,
            effect,
            resource,
            ack_root_equivalent,
        } => (
            "policy.admin.grant",
            json!({"uid":uid,"action":action,"effect":effect.as_wire(),"resource":resource,"ack_root_equivalent":ack_root_equivalent}),
        ),
        PolicyAdminCommand::Revoke {
            uid,
            action,
            resource,
        } => (
            "policy.admin.revoke",
            json!({"uid":uid,"action":action,"resource":resource}),
        ),
        PolicyAdminCommand::Bundle {
            command:
                PolicyBundleCommand::Set {
                    uid,
                    bundle_id,
                    enabled,
                    disabled,
                },
        } => (
            "policy.admin.bundle.set",
            json!({"uid":uid,"bundle":bundle_id,"enabled": if *enabled { true } else { !*disabled }}),
        ),
    };
    let reply = privilege
        .admin_request(method, params, timeout)
        .map_err(|mut error| {
            error.command = method.into();
            error
        })?;
    let uid = reply.data.get("uid").and_then(Value::as_u64).unwrap_or(0);
    let bundles = reply
        .data
        .get("bundles")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let grants = reply
        .data
        .get("grants")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    Ok(CliSuccess {
        command: method,
        data: reply.data,
        meta: reply.meta,
        human: vec![
            format!("subject uid  {uid}"),
            format!("bundles      {bundles}"),
            format!("grants       {grants}"),
        ],
    })
}

fn capability_list(
    runtime: &mut dyn RuntimeTransport,
    pagination: &PaginationArgs,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    let reply = runtime
        .request(
            "capability.list",
            json!({"limit": pagination.limit, "cursor": pagination.cursor}),
            timeout,
        )
        .map_err(|mut error| {
            error.command = "capability.list".into();
            error
        })?;
    paged_success(
        "capability.list",
        reply,
        |item| {
            let id = item
                .get("capability_id")
                .and_then(Value::as_str)
                .unwrap_or("-");
            let count = item
                .get("providers")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            format!("{id}  providers={count}")
        },
        "No registered capabilities.",
    )
}

fn capability_provider_list(
    runtime: &mut dyn RuntimeTransport,
    pagination: &PaginationArgs,
    timeout: Duration,
) -> Result<CliSuccess, CliError> {
    let reply = runtime
        .request(
            "capability.provider.list",
            json!({"limit": pagination.limit, "cursor": pagination.cursor}),
            timeout,
        )
        .map_err(|mut error| {
            error.command = "capability.provider.list".into();
            error
        })?;
    paged_success(
        "capability.provider.list",
        reply,
        |item| {
            format!(
                "{}  {}  health={} compatibility={}",
                item.get("provider_id")
                    .and_then(Value::as_str)
                    .unwrap_or("-"),
                item.get("provider_type")
                    .and_then(Value::as_str)
                    .unwrap_or("-"),
                item.get("health_state")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                item.get("compatibility_state")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            )
        },
        "No registered providers.",
    )
}

fn paged_success(
    command: &'static str,
    mut reply: crate::RuntimeReply,
    format_item: impl Fn(&Value) -> String,
    empty_message: &'static str,
) -> Result<CliSuccess, CliError> {
    let next_cursor = reply
        .data
        .get("next_cursor")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let items = reply
        .data
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(object) = reply.data.as_object_mut() {
        object.remove("next_cursor");
    }
    reply.meta.next_cursor = next_cursor;
    let human = if items.is_empty() {
        vec![empty_message.into()]
    } else {
        items.iter().map(format_item).collect()
    };
    Ok(CliSuccess {
        command,
        data: reply.data,
        meta: reply.meta,
        human,
    })
}

fn doctor(
    context: &DoctorContext,
    domain: Option<crate::DoctorDomain>,
    bundle: Option<&std::path::Path>,
) -> Result<CliSuccess, CliError> {
    let mut report = context.run(domain);
    if let Some(path) = bundle {
        context.write_bundle(&report, path).map_err(|error| {
            CliError::new(
                "doctor",
                SemanticErrorCode::InvalidRequest,
                format!("failed to write diagnostic bundle: {error}"),
            )
        })?;
        report.bundle_path = Some(path.display().to_string());
    }
    let data = serde_json::to_value(&report).map_err(|_| {
        CliError::new(
            "doctor",
            SemanticErrorCode::Internal,
            "failed to encode doctor report",
        )
    })?;
    Ok(CliSuccess {
        command: "doctor",
        data,
        meta: CliMeta::default(),
        human: report.human_lines(),
    })
}

fn version() -> Result<CliSuccess, CliError> {
    let cli_version = env!("CARGO_PKG_VERSION");
    let build_id = option_env!("PORTUS_BUILD_ID").unwrap_or("development");
    Ok(CliSuccess {
        command: "version",
        data: json!({
            "portus_os_cli_version": cli_version,
            "schema_version": CLI_OUTPUT_SCHEMA_VERSION,
            "runtime_protocol_version": CURRENT_PROTOCOL_VERSION.get(),
            "build_id": build_id,
            "target_arch": std::env::consts::ARCH,
            "target_os": std::env::consts::OS,
        }),
        meta: CliMeta::default(),
        human: vec![format!("portus-os {cli_version}")],
    })
}

fn help_contract() -> Result<CliSuccess, CliError> {
    Ok(CliSuccess {
        command: "help",
        data: json!({
            "cli_version": env!("CARGO_PKG_VERSION"),
            "output_schema_version": CLI_OUTPUT_SCHEMA_VERSION,
            "runtime_protocol_version": CURRENT_PROTOCOL_VERSION.get(),
            "global_options": [
                {"name":"--json", "type":"boolean"},
                {"name":"--jsonl", "type":"boolean", "mutually_exclusive_with":"--json"},
                {"name":"--timeout-ms", "type":"integer", "minimum":crate::MIN_TIMEOUT_MS, "maximum":crate::MAX_TIMEOUT_MS, "default":crate::DEFAULT_TIMEOUT_MS}
            ],
            "common_pagination": {
                "limit_default": crate::DEFAULT_PAGE_LIMIT,
                "limit_max": crate::MAX_PAGE_LIMIT,
                "cursor":"opaque"
            },
            "commands": [
                {"name":"status", "implemented":true, "structured_output":["human","json"], "streaming":false, "pagination":false, "dry_run":false, "preconditions":false},
                {"name":"doctor", "implemented":true, "structured_output":["human","json"], "streaming":false, "pagination":false, "dry_run":false, "preconditions":false, "argument":{"name":"domain","required":false,"enum":["runtime","state","index","providers","codex"]}, "option":{"name":"--bundle","type":"path","overwrite":false,"contents":"allowlisted_json_evidence"}},
                {"name":"index", "implemented":true, "structured_output":["human","json"], "streaming":false, "pagination":true, "dry_run":false, "preconditions":false, "subcommands":[{"name":"apps","implemented":true},{"name":"windows","implemented":true},{"name":"workspaces","implemented":true},{"name":"displays","implemented":true},{"name":"providers","implemented":true},{"name":"stale","implemented":true},{"name":"query","implemented":true},{"name":"show","implemented":true},{"name":"topology","implemented":true},{"name":"refresh","implemented":true},{"name":"rescan","implemented":true,"domains":["applications","runtime","providers","services"]},{"name":"reconcile","implemented":true},{"name":"rebuild","implemented":true,"authority":"authenticated_uid_0_current"},{"name":"status","implemented":true}]},
                {"name":"task", "implemented":true, "structured_output":["human","json","jsonl"], "streaming":true, "pagination":true, "dry_run":false, "preconditions":true, "subcommands":[{"name":"list","implemented":true},{"name":"show","implemented":true},{"name":"events","implemented":true,"follow_implemented":true,"follow_output":["human","jsonl"]},{"name":"cancel","implemented":true,"precondition":"--if-state"}]},
                {"name":"capability", "implemented":true, "structured_output":["human","json"], "streaming":false, "pagination":true, "dry_run":false, "preconditions":false, "subcommands":[{"name":"list","implemented":true},{"name":"show","implemented":true},{"name":"provider","implemented":true,"subcommands":[{"name":"list","implemented":true},{"name":"show","implemented":true}]}]},
                {"name":"policy", "implemented":true, "structured_output":["human","json"], "streaming":false, "pagination":false, "dry_run":false, "preconditions":true, "subcommands":[{"name":"effective","implemented":true,"transport":"portusd"},{"name":"check","implemented":true,"transport":"portusd","side_effect_free":true},{"name":"admin","implemented":true,"transport":"portus-privd-admin","authority":"authenticated_uid_0","subcommands":[{"name":"show","implemented":true},{"name":"grant","implemented":true,"root_equivalent_ack":"--ack-root-equivalent"},{"name":"revoke","implemented":true},{"name":"bundle","implemented":true,"subcommands":[{"name":"set","implemented":true}]}]}]},
                {"name":"artifact", "implemented":true, "structured_output":["human","json"], "streaming":false, "pagination":true, "dry_run":false, "preconditions":false, "subcommands":[{"name":"list","implemented":true},{"name":"show","implemented":true}], "mutation_surface":"internal_typed_only"},
                {"name":"health", "implemented":true, "bare_form_implemented":true, "structured_output":["human","json"], "streaming":false, "pagination":false, "dry_run":false, "preconditions":false, "subcommands":[{"name":"show","implemented":true},{"name":"degraded","implemented":true}]},
                {"name":"help", "implemented":true, "structured_output":["human","json"], "streaming":false, "pagination":false, "dry_run":false, "preconditions":false},
                {"name":"version", "implemented":true, "structured_output":["human","json"], "streaming":false, "pagination":false, "dry_run":false, "preconditions":false}
            ]
        }),
        meta: CliMeta::default(),
        human: vec![
            "PortusOS local control-plane CLI".into(),
            "Use `portus-os --help` for syntax or `portus-os help --json` for the machine contract.".into(),
        ],
    })
}

fn unsupported(command: &'static str, message: &'static str) -> Result<CliSuccess, CliError> {
    Err(CliError::new(
        command,
        SemanticErrorCode::Unsupported,
        message,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DoctorContext, RuntimeReply, UnavailablePrivilege, meta_with_request, parse_from};
    use portus_protocol::{RequestId, SemanticError};
    use std::collections::VecDeque;

    struct FakeRuntime {
        replies: VecDeque<Result<RuntimeReply, CliError>>,
        methods: Vec<String>,
    }

    impl FakeRuntime {
        fn new(replies: impl IntoIterator<Item = Result<RuntimeReply, CliError>>) -> Self {
            Self {
                replies: replies.into_iter().collect(),
                methods: Vec::new(),
            }
        }
    }

    struct FakePrivilege {
        replies: VecDeque<Result<RuntimeReply, CliError>>,
        methods: Vec<String>,
        params: Vec<Value>,
    }

    impl FakePrivilege {
        fn new(replies: impl IntoIterator<Item = Result<RuntimeReply, CliError>>) -> Self {
            Self {
                replies: replies.into_iter().collect(),
                methods: Vec::new(),
                params: Vec::new(),
            }
        }
    }

    impl PrivilegeTransport for FakePrivilege {
        fn admin_request(
            &mut self,
            method: &str,
            params: Value,
            _timeout: Duration,
        ) -> Result<RuntimeReply, CliError> {
            self.methods.push(method.to_string());
            self.params.push(params);
            self.replies.pop_front().expect("fake privilege reply")
        }
    }

    impl RuntimeTransport for FakeRuntime {
        fn request(
            &mut self,
            method: &str,
            _params: Value,
            _timeout: Duration,
        ) -> Result<RuntimeReply, CliError> {
            self.methods.push(method.to_string());
            self.replies.pop_front().expect("fake reply")
        }
    }

    fn reply(data: Value) -> Result<RuntimeReply, CliError> {
        Ok(RuntimeReply {
            data,
            meta: meta_with_request(RequestId::new()),
        })
    }

    fn execute_with_privilege(
        cli: Cli,
        runtime: &mut dyn RuntimeTransport,
        privilege: &mut dyn PrivilegeTransport,
    ) -> Result<CliSuccess, CliError> {
        let doctor = DoctorContext::default();
        execute(
            &cli,
            &mut ExecutionContext {
                runtime,
                privilege,
                doctor: &doctor,
            },
        )
    }

    fn execute_with(cli: Cli, runtime: &mut dyn RuntimeTransport) -> Result<CliSuccess, CliError> {
        let doctor = DoctorContext::default();
        let mut privilege = UnavailablePrivilege;
        execute(
            &cli,
            &mut ExecutionContext {
                runtime,
                privilege: &mut privilege,
                doctor: &doctor,
            },
        )
    }

    #[test]
    fn status_is_daemon_backed_and_preserves_request_meta() {
        let cli = parse_from(["portus-os", "status", "--json"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "readiness":"ready",
            "health":"healthy",
            "schema_version":2
        }))]);
        let success = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["runtime.status"]);
        assert!(success.meta.request_id.is_some());
        assert_eq!(success.data["runtime"]["health"], "healthy");
    }

    #[test]
    fn health_uses_typed_p11_runtime_surface() {
        let cli = parse_from(["portus-os", "health"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "components":[{
                "component_ref":"runtime:portusd",
                "health_state":"healthy",
                "reason_code":"ready",
                "recovery_disposition":"observe"
            }],
            "degraded":false,
            "observed_at_ms":10
        }))]);
        let success = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["health.list"]);
        assert_eq!(success.meta.degraded, Some(false));
        assert_eq!(
            success.data["components"][0]["component_ref"],
            "runtime:portusd"
        );

        let cli = parse_from(["portus-os", "health", "show", "runtime:portusd"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "component_ref":"runtime:portusd",
            "health_state":"healthy",
            "reason_code":"ready",
            "recovery_disposition":"observe"
        }))]);
        let shown = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["health.show"]);
        assert_eq!(shown.command, "health.show");

        let cli = parse_from(["portus-os", "health", "degraded"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "components":[{
                "component_ref":"storage:/workspace",
                "health_state":"degraded",
                "reason_code":"resource_low",
                "recovery_disposition":"observe"
            }],
            "count":1,
            "observed_at_ms":10
        }))]);
        let degraded = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["health.degraded"]);
        assert_eq!(degraded.meta.degraded, Some(true));
    }

    #[test]
    fn artifact_list_and_show_use_typed_p12_runtime_methods() {
        let artifact_id = portus_protocol::ArtifactId::new();
        let cli = parse_from(["portus-os", "artifact", "list", "--limit", "1", "--json"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "items":[{
                "artifact_id":artifact_id,
                "owner":{"uid":1000,"gid":1000},
                "artifact_type":"report",
                "confidentiality":"private",
                "retention_kind":"retained",
                "availability_state":"available",
                "integrity_kind":"verified",
                "size_bytes":12,
                "safe_display_name":"report.pdf",
                "registered_at_ms":10
            }],
            "next_cursor":artifact_id.to_string()
        }))]);
        let listed = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["artifact.list"]);
        assert_eq!(listed.command, "artifact.list");
        assert_eq!(
            listed.meta.next_cursor.as_deref(),
            Some(artifact_id.to_string().as_str())
        );
        assert!(listed.data.get("next_cursor").is_none());

        let cli = parse_from([
            "portus-os",
            "artifact",
            "show",
            &artifact_id.to_string(),
            "--json",
        ])
        .unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "artifact":{
                "artifact_id":artifact_id,
                "owner":{"uid":1000,"gid":1000},
                "artifact_type":"report",
                "confidentiality":"private",
                "retention_kind":"retained",
                "availability_state":"available",
                "locator":{"kind":"filesystem","path":"/workspace/report.pdf"},
                "integrity_kind":"verified",
                "registered_at_ms":10,
                "created_at_ms":10,
                "cleanup_authority":"none",
                "safe_metadata":{}
            },
            "task_relationships":[],
            "shared_with":[],
            "holds":[]
        }))]);
        let shown = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["artifact.show"]);
        assert_eq!(shown.command, "artifact.show");
        assert!(
            shown
                .human
                .iter()
                .any(|line| line.contains("/workspace/report.pdf"))
        );
    }

    #[test]
    fn capability_list_moves_runtime_cursor_into_cli_meta() {
        let cli =
            parse_from(["portus-os", "capability", "list", "--limit", "1", "--json"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "items":[{"capability_id":"browser.control","providers":[]}],
            "next_cursor":"browser.control"
        }))]);
        let success = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["capability.list"]);
        assert_eq!(success.meta.next_cursor.as_deref(), Some("browser.control"));
        assert!(success.data.get("next_cursor").is_none());
        assert_eq!(success.command, "capability.list");
    }

    #[test]
    fn index_quick_view_uses_query_and_moves_cursor_to_meta() {
        let cli = parse_from(["portus-os", "index", "windows", "--limit", "1", "--json"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "items":[{"index_handle":"idx_019c0000-0000-7000-8000-000000000001","resource_type":"window","freshness":"recent","metadata":{"class":"Demo"}}],
            "next_cursor":"idx_019c0000-0000-7000-8000-000000000002",
            "partial":false
        }))]);
        let success = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["index.query"]);
        assert_eq!(success.command, "index.windows");
        assert_eq!(
            success.meta.next_cursor.as_deref(),
            Some("idx_019c0000-0000-7000-8000-000000000002")
        );
        assert_eq!(success.meta.degraded, Some(false));
    }

    #[test]
    fn index_rescan_and_status_use_typed_runtime_methods() {
        let rescan_cli = parse_from(["portus-os", "index", "rescan", "runtime"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "generation":1,"state":"degraded","reason_code":"source_degraded","sources":[]
        }))]);
        let success = execute_with(rescan_cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["index.rescan"]);
        assert_eq!(success.meta.degraded, Some(true));

        let status_cli = parse_from(["portus-os", "index", "status"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "generation":1,"state":"healthy","reason_code":"ready","sources":[]
        }))]);
        let success = execute_with(status_cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["index.status"]);
        assert_eq!(success.meta.degraded, Some(false));
    }

    #[test]
    fn task_list_moves_cursor_and_uses_typed_filters() {
        let task_id = portus_protocol::TaskId::new();
        let cli = parse_from([
            "portus-os",
            "task",
            "list",
            "--state",
            "running",
            "--project",
            "project:demo",
            "--limit",
            "1",
            "--json",
        ])
        .unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "items":[{
                "task_id":task_id,
                "owner":{"uid":1000,"gid":1000},
                "objective_summary":"bounded job",
                "state":"running",
                "requester_surface":"test",
                "retry_safety":"never",
                "created_at_ms":1,
                "updated_at_ms":2,
                "last_event_sequence":3,
                "attempt_count":1,
                "managed_relationships":1,
                "associated_relationships":0
            }],
            "next_cursor":task_id
        }))]);
        let success = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["task.list"]);
        assert_eq!(success.command, "task.list");
        assert_eq!(
            success.meta.next_cursor.as_deref(),
            Some(task_id.to_string().as_str())
        );
        assert!(success.data.get("next_cursor").is_none());
    }

    #[test]
    fn task_show_events_and_cancel_use_locked_runtime_methods() {
        let task_id = portus_protocol::TaskId::new();
        let show_cli = parse_from(["portus-os", "task", "show", &task_id.to_string()]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "task_id":task_id,
            "objective_summary":"demo",
            "state":"running",
            "state_reason":"working",
            "relationships":[]
        }))]);
        let shown = execute_with(show_cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["task.show"]);
        assert_eq!(shown.command, "task.show");

        let events_cli = parse_from([
            "portus-os",
            "task",
            "events",
            &task_id.to_string(),
            "--after",
            "2",
        ])
        .unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "events":[{"task_id":task_id,"sequence":3,"event_kind":"task.running","safe_summary":"running","safe_data":{},"occurred_at_ms":3}],
            "next_sequence":3
        }))]);
        let events = execute_with(events_cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["task.events"]);
        assert_eq!(events.meta.extra["next_sequence"], 3);

        let cancel_cli = parse_from([
            "portus-os",
            "task",
            "cancel",
            &task_id.to_string(),
            "--if-state",
            "running",
        ])
        .unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "task_id":task_id,
            "objective_summary":"demo",
            "state":"cancelled",
            "state_reason":"cancellation_confirmed",
            "relationships":[]
        }))]);
        let cancelled = execute_with(cancel_cli, &mut runtime).unwrap();
        assert_eq!(runtime.methods, ["task.cancel"]);
        assert_eq!(cancelled.data["state"], "cancelled");
    }

    #[test]
    fn direct_execute_rejects_follow_because_app_stream_path_owns_it() {
        let task_id = portus_protocol::TaskId::new().to_string();
        let cli = parse_from(["portus-os", "task", "events", &task_id, "--follow"]).unwrap();
        let mut runtime = FakeRuntime::new([]);
        let error = execute_with(cli, &mut runtime).unwrap_err();
        assert_eq!(error.semantic.code, SemanticErrorCode::Unsupported);
        assert!(
            error
                .semantic
                .message
                .contains("streaming application path")
        );
    }

    #[test]
    fn p9_policy_effective_and_check_use_runtime_without_admin_transport() {
        let mut runtime = FakeRuntime::new([
            reply(json!({
                "principal":{"uid":1000,"gid":1000},
                "policy_version":1,
                "bundles":[],
                "grants":[],
                "has_root_equivalent_authority":false
            })),
            reply(json!({
                "principal":{"uid":1000,"gid":1000},
                "action":"service.restart",
                "resource":"portusd",
                "effect":"prompt",
                "reason_code":"explicit_grant",
                "enforcement_class":"privileged_typed_operation",
                "root_equivalent":false
            })),
        ]);
        let mut privilege = FakePrivilege::new([]);
        let effective = parse_from(["portus-os", "policy", "effective"]).unwrap();
        let success = execute_with_privilege(effective, &mut runtime, &mut privilege).unwrap();
        assert_eq!(success.command, "policy.effective");
        let check = parse_from([
            "portus-os",
            "policy",
            "check",
            "service.restart",
            "--resource",
            "portusd",
        ])
        .unwrap();
        let success = execute_with_privilege(check, &mut runtime, &mut privilege).unwrap();
        assert_eq!(success.data["effect"], "prompt");
        assert_eq!(runtime.methods, ["policy.effective", "policy.check"]);
        assert!(privilege.methods.is_empty());
    }

    #[test]
    fn p9_policy_admin_routes_only_to_privilege_admin_transport() {
        let mut runtime = FakeRuntime::new([]);
        let mut privilege = FakePrivilege::new([reply(json!({
            "uid":1000,
            "label":null,
            "bundles":[],
            "grants":[]
        }))]);
        let cli = parse_from([
            "portus-os",
            "policy",
            "admin",
            "grant",
            "1000",
            "root.shell",
            "--effect",
            "allow",
            "--ack-root-equivalent",
        ])
        .unwrap();
        let success = execute_with_privilege(cli, &mut runtime, &mut privilege).unwrap();
        assert_eq!(success.command, "policy.admin.grant");
        assert!(runtime.methods.is_empty());
        assert_eq!(privilege.methods, ["policy.admin.grant"]);
        assert_eq!(privilege.params[0]["uid"], 1000);
        assert_eq!(privilege.params[0]["action"], "root.shell");
        assert_eq!(privilege.params[0]["effect"], "allow");
        assert_eq!(privilege.params[0]["ack_root_equivalent"], true);
    }

    #[test]
    fn daemon_unavailable_keeps_exit_family_and_status_hint() {
        let cli = parse_from(["portus-os", "status"]).unwrap();
        let mut runtime = FakeRuntime::new([Err(CliError::new(
            "runtime.status",
            SemanticErrorCode::DaemonUnavailable,
            "portusd is unavailable",
        ))]);
        let error = execute_with(cli, &mut runtime).unwrap_err();
        assert_eq!(error.exit_code(), 3);
        assert!(error.human_hint.unwrap().contains("doctor runtime"));
    }

    #[test]
    fn semantic_protocol_mismatch_preserves_code() {
        let cli = parse_from(["portus-os", "status"]).unwrap();
        let mut runtime = FakeRuntime::new([Err(CliError {
            command: "runtime.status".into(),
            semantic: Box::new(SemanticError::new(
                SemanticErrorCode::IncompatibleProtocol,
                "mismatch",
            )),
            meta: Box::new(CliMeta::default()),
            human_hint: None,
        })]);
        let error = execute_with(cli, &mut runtime).unwrap_err();
        assert_eq!(error.semantic.code, SemanticErrorCode::IncompatibleProtocol);
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn help_contract_matches_locked_top_level_commands() {
        let cli = parse_from(["portus-os", "help", "--json"]).unwrap();
        let mut runtime = FakeRuntime::new([]);
        let success = execute_with(cli, &mut runtime).unwrap();
        let names = success.data["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        let index = success.data["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["name"] == "index")
            .unwrap();
        assert_eq!(index["implemented"], true);
        assert_eq!(index["pagination"], true);
        let task = success.data["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["name"] == "task")
            .unwrap();
        assert_eq!(task["implemented"], true);
        assert_eq!(task["pagination"], true);
        assert_eq!(task["preconditions"], true);
        let capability = success.data["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["name"] == "capability")
            .unwrap();
        assert_eq!(capability["implemented"], true);
        assert_eq!(capability["pagination"], true);
        let policy = success.data["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["name"] == "policy")
            .unwrap();
        assert_eq!(policy["implemented"], true);
        assert_eq!(policy["preconditions"], true);
        assert_eq!(policy["subcommands"][0]["name"], "effective");
        assert_eq!(policy["subcommands"][1]["name"], "check");
        assert_eq!(policy["subcommands"][2]["name"], "admin");
        assert_eq!(
            names,
            vec![
                "status",
                "doctor",
                "index",
                "task",
                "capability",
                "policy",
                "artifact",
                "health",
                "help",
                "version"
            ]
        );
    }

    #[test]
    fn status_human_output_is_stable() {
        let cli = parse_from(["portus-os", "status"]).unwrap();
        let mut runtime = FakeRuntime::new([reply(json!({
            "readiness":"ready",
            "health":"healthy",
            "schema_version":5,
            "index":{"state":"healthy","reason_code":"ready"},
            "tasks":{"state":"ready","active":2,"terminal":3}
        }))]);
        let success = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(
            success.human,
            vec![
                "portusd   healthy (ready)",
                "state     schema 5",
                "index     healthy (ready)",
                "providers unknown (0 registered)",
                "tasks     ready (2 active, 3 terminal)",
                "policy    unknown (not_loaded)"
            ]
        );
    }

    #[test]
    fn version_contract_contains_required_machine_fields() {
        let cli = parse_from(["portus-os", "version", "--json"]).unwrap();
        let mut runtime = FakeRuntime::new([]);
        let success = execute_with(cli, &mut runtime).unwrap();
        assert_eq!(
            success.human,
            vec![format!("portus-os {}", env!("CARGO_PKG_VERSION"))]
        );
        for field in [
            "portus_os_cli_version",
            "schema_version",
            "runtime_protocol_version",
            "build_id",
            "target_arch",
            "target_os",
        ] {
            assert!(
                success.data.get(field).is_some(),
                "missing version field: {field}"
            );
        }
    }

    #[test]
    fn jsonl_is_rejected_for_non_streaming_p4_commands() {
        let cli = parse_from(["portus-os", "status", "--jsonl"]).unwrap();
        let mut runtime = FakeRuntime::new([]);
        let error = execute_with(cli, &mut runtime).unwrap_err();
        assert_eq!(
            error.semantic.code,
            SemanticErrorCode::UnsupportedOutputMode
        );
    }

    #[test]
    fn artifact_bare_form_requires_a_read_subcommand() {
        assert!(parse_from(["portus-os", "artifact"]).is_err());
    }
}
