use portus_protocol::{
    ControlPathKind, EvidenceStrength, Freshness, IndexObservation, IndexObservationInput,
    IndexRelationInput, IndexResourceType, IndexSourceKind,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const CORRELATION_SOURCE_ID: &str = "correlation";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CorrelationOutput {
    pub observations: Vec<IndexObservationInput>,
    pub relations: Vec<IndexRelationInput>,
}

pub fn correlate(observations: &[IndexObservation], observed_at_ms: i64) -> CorrelationOutput {
    let mut output = CorrelationOutput::default();
    let mut processes_by_ref = BTreeMap::new();
    let mut processes_by_pid = BTreeMap::new();
    let mut app_defs_by_exec: BTreeMap<String, Vec<&IndexObservation>> = BTreeMap::new();
    let mut windows_by_process: BTreeMap<String, Vec<&IndexObservation>> = BTreeMap::new();

    for observation in observations {
        if matches!(
            observation.freshness,
            Freshness::Historical | Freshness::Unavailable
        ) {
            continue;
        }
        match observation.resource_type {
            IndexResourceType::Process => {
                if let Some(reference) = observation.authoritative_ref.as_deref() {
                    processes_by_ref.insert(reference.to_string(), observation);
                }
                if let Some(pid) = metadata_u64(&observation.metadata, "pid") {
                    processes_by_pid.insert((observation.owner, pid), observation);
                }
            }
            IndexResourceType::ApplicationDefinition => {
                if let Some(exec) = metadata_string(&observation.metadata, "exec_basename") {
                    app_defs_by_exec.entry(exec).or_default().push(observation);
                }
            }
            IndexResourceType::Window => {
                if let Some(process_ref) = metadata_string(&observation.metadata, "process_ref") {
                    windows_by_process
                        .entry(process_ref)
                        .or_default()
                        .push(observation);
                }
            }
            _ => {}
        }
    }

    for process in processes_by_ref.values() {
        let Some(process_ref) = process.authoritative_ref.as_deref() else {
            continue;
        };
        if let Some(ppid) = metadata_u64(&process.metadata, "ppid") {
            if let Some(parent) = processes_by_pid.get(&(process.owner, ppid)) {
                if let Some(parent_ref) = parent.authoritative_ref.as_deref() {
                    output.relations.push(relation(
                        process_ref,
                        parent_ref,
                        "child_of",
                        EvidenceStrength::Authoritative,
                        "proc_parent_pid_same_owner",
                        observed_at_ms,
                    ));
                }
            }
        }

        let windows = windows_by_process
            .get(process_ref)
            .cloned()
            .unwrap_or_default();
        for window in &windows {
            if let Some(window_ref) = window.authoritative_ref.as_deref() {
                output.relations.push(relation(
                    window_ref,
                    process_ref,
                    "owned_by_process",
                    EvidenceStrength::Strong,
                    "x11_pid_matches_process_generation",
                    observed_at_ms,
                ));
            }
        }

        let exec = metadata_string(&process.metadata, "exe_basename");
        let matching_apps = exec
            .as_ref()
            .and_then(|exec| app_defs_by_exec.get(exec))
            .cloned()
            .unwrap_or_default();
        let unique_app = (matching_apps.len() == 1).then(|| matching_apps[0]);
        if windows.is_empty() && unique_app.is_none() {
            continue;
        }

        let instance_ref = format!("application-instance:{process_ref}");
        let mut control_paths = Vec::new();
        if !windows.is_empty() {
            control_paths.push(ControlPathKind::ProcessWindow);
        }
        if unique_app.is_some() {
            control_paths.push(ControlPathKind::StructuredCli);
        }
        control_paths.sort();
        control_paths.dedup();
        let metadata = json!({
            "leader_process_ref": process_ref,
            "application_ref": unique_app.and_then(|app| app.authoritative_ref.clone()),
            "application_name": unique_app.and_then(|app| metadata_string(&app.metadata, "name")),
            "window_count": windows.len(),
            "exe_basename": exec,
        });
        output.observations.push(IndexObservationInput {
            resource_type: IndexResourceType::ApplicationInstance,
            source_id: CORRELATION_SOURCE_ID.into(),
            source_kind: IndexSourceKind::Correlation,
            source_generation: "correlation-v1".into(),
            native_identity: process_ref.to_string(),
            authoritative_ref: Some(instance_ref.clone()),
            owner: process.owner,
            freshness: process.freshness,
            observed_at_ms,
            metadata,
            control_paths,
        });
        output.relations.push(relation(
            &instance_ref,
            process_ref,
            "instance_process",
            EvidenceStrength::Strong,
            "instance_leader_process_generation",
            observed_at_ms,
        ));
        if let Some(app) = unique_app {
            if let Some(app_ref) = app.authoritative_ref.as_deref() {
                output.relations.push(relation(
                    &instance_ref,
                    app_ref,
                    "instance_application",
                    EvidenceStrength::Strong,
                    "unique_executable_definition_match",
                    observed_at_ms,
                ));
            }
        }
        for window in windows {
            if let Some(window_ref) = window.authoritative_ref.as_deref() {
                output.relations.push(relation(
                    &instance_ref,
                    window_ref,
                    "instance_window",
                    EvidenceStrength::Strong,
                    "window_process_generation_match",
                    observed_at_ms,
                ));
            }
        }
    }

    dedupe_relations(&mut output.relations);
    output
}

fn relation(
    from: &str,
    to: &str,
    kind: &str,
    evidence_strength: EvidenceStrength,
    reason_code: &str,
    observed_at_ms: i64,
) -> IndexRelationInput {
    IndexRelationInput {
        from_authoritative_ref: from.to_string(),
        to_authoritative_ref: to.to_string(),
        relation_kind: kind.to_string(),
        evidence_strength,
        source_id: CORRELATION_SOURCE_ID.into(),
        source_kind: IndexSourceKind::Correlation,
        reason_code: reason_code.to_string(),
        observed_at_ms,
    }
}

fn metadata_u64(metadata: &Value, key: &str) -> Option<u64> {
    metadata.get(key).and_then(Value::as_u64)
}

fn metadata_string(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn dedupe_relations(relations: &mut Vec<IndexRelationInput>) {
    let mut seen = BTreeSet::new();
    relations.retain(|relation| {
        seen.insert((
            relation.from_authoritative_ref.clone(),
            relation.to_authoritative_ref.clone(),
            relation.relation_kind.clone(),
            relation.source_id.clone(),
        ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_protocol::{IndexHandle, Principal};

    fn observation(
        resource_type: IndexResourceType,
        reference: &str,
        metadata: Value,
    ) -> IndexObservation {
        IndexObservation {
            index_handle: IndexHandle::new(),
            resource_type,
            source_id: "fixture".into(),
            source_kind: IndexSourceKind::Proc,
            source_generation: "boot-a".into(),
            native_identity: reference.into(),
            authoritative_ref: Some(reference.into()),
            owner: Some(Principal::new(1000, 1000)),
            freshness: Freshness::Recent,
            observed_at_ms: 1,
            updated_at_ms: 1,
            metadata,
            control_paths: Vec::new(),
        }
    }

    #[test]
    fn exact_process_window_and_application_match_creates_one_instance() {
        let process = observation(
            IndexResourceType::Process,
            "process:boot-a:42:100",
            json!({"pid":42,"ppid":1,"exe_basename":"demo"}),
        );
        let mut app = observation(
            IndexResourceType::ApplicationDefinition,
            "application:demo.desktop",
            json!({"name":"Demo","exec_basename":"demo"}),
        );
        app.owner = None;
        let window = observation(
            IndexResourceType::Window,
            "window:graph-a:77",
            json!({"xid":77,"process_ref":"process:boot-a:42:100"}),
        );
        let output = correlate(&[process, app, window], 10);
        assert_eq!(output.observations.len(), 1);
        assert_eq!(
            output.observations[0].resource_type,
            IndexResourceType::ApplicationInstance
        );
        assert!(
            output
                .relations
                .iter()
                .any(|relation| relation.relation_kind == "instance_application")
        );
        assert!(
            output
                .relations
                .iter()
                .any(|relation| relation.relation_kind == "owned_by_process")
        );
    }

    #[test]
    fn ambiguous_application_executable_is_not_guessed() {
        let process = observation(
            IndexResourceType::Process,
            "process:boot-a:42:100",
            json!({"pid":42,"ppid":1,"exe_basename":"shared"}),
        );
        let mut app_a = observation(
            IndexResourceType::ApplicationDefinition,
            "application:a.desktop",
            json!({"name":"A","exec_basename":"shared"}),
        );
        app_a.owner = None;
        let mut app_b = observation(
            IndexResourceType::ApplicationDefinition,
            "application:b.desktop",
            json!({"name":"B","exec_basename":"shared"}),
        );
        app_b.owner = None;
        let output = correlate(&[process, app_a, app_b], 10);
        assert!(output.observations.is_empty());
        assert!(
            !output
                .relations
                .iter()
                .any(|relation| relation.relation_kind == "instance_application")
        );
    }
}
