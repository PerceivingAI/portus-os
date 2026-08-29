use crate::{PortusState, StateError, StateResult};
use portus_protocol::{
    ControlPathKind, EvidenceStrength, Freshness, HealthState, IndexHandle, IndexHealthState,
    IndexObservation, IndexObservationInput, IndexPage, IndexRelation, IndexRelationInput,
    IndexResourceType, IndexSourceKind, IndexSourceStatus, Principal,
};
use rusqlite::{OptionalExtension, Transaction, params, params_from_iter};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    str::FromStr,
};

pub const MAX_INDEX_QUERY_SCAN: usize = 8_192;
const QUERY_CHUNK: usize = 512;
const MAX_SOURCE_OBSERVATIONS: usize = 65_536;
const MAX_SOURCE_RELATIONS: usize = 131_072;
const MAX_METADATA_BYTES: usize = 16 * 1024;
const MAX_NATIVE_ID_BYTES: usize = 512;
const MAX_AUTHORITATIVE_REF_BYTES: usize = 1024;
const MAX_SOURCE_ID_BYTES: usize = 128;
const MAX_RELATION_KIND_BYTES: usize = 96;
const MAX_REASON_CODE_BYTES: usize = 96;

#[derive(Clone, Debug, Default)]
pub struct IndexQueryFilter {
    pub resource_type: Option<IndexResourceType>,
    pub freshness: Option<Freshness>,
    pub source_kind: Option<IndexSourceKind>,
    pub application: Option<String>,
    pub provider: Option<String>,
    pub capability: Option<String>,
    pub workspace: Option<String>,
    pub display: Option<String>,
    pub evidence: Option<EvidenceStrength>,
    pub changed_since_ms: Option<i64>,
    pub control_path: Option<ControlPathKind>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IndexView {
    pub resource: IndexObservation,
    pub relations: Vec<IndexRelation>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IndexTopologyView {
    pub root: IndexObservation,
    pub resources: Vec<IndexObservation>,
    pub relations: Vec<IndexRelation>,
    pub depth: u8,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct IndexRuntimeStatus {
    pub generation: u64,
    pub state: IndexHealthState,
    pub reason_code: String,
    pub last_reconcile_at_ms: Option<i64>,
    pub sources: Vec<IndexSourceStatus>,
}

impl PortusState {
    pub fn prepare_index_restart(&self, updated_at_ms: i64) -> StateResult<()> {
        self.connection.execute(
            "UPDATE index_observations SET freshness='stale', updated_at_ms=?1 WHERE freshness IN ('live','recent')",
            params![updated_at_ms],
        )?;
        self.connection.execute(
            "UPDATE index_sources SET health_state='unknown', reason_code='runtime_restart', updated_at_ms=?1",
            params![updated_at_ms],
        )?;
        self.connection.execute(
            "UPDATE index_runtime_state SET state='initializing', reason_code='runtime_restart', updated_at_ms=?1 WHERE singleton=1",
            params![updated_at_ms],
        )?;
        Ok(())
    }

    pub fn reconcile_index_source(
        &mut self,
        status: &IndexSourceStatus,
        observations: &[IndexObservationInput],
        updated_at_ms: i64,
    ) -> StateResult<()> {
        validate_source_status(status)?;
        if observations.len() > MAX_SOURCE_OBSERVATIONS {
            return Err(StateError::InvalidIndexState(
                "source observation set exceeds bounded limit".into(),
            ));
        }
        for observation in observations {
            validate_observation(status, observation)?;
        }
        let transaction = self.connection.transaction()?;
        upsert_source_status(&transaction, status, updated_at_ms)?;

        let current_generation = status.source_generation.as_str();
        transaction.execute(
            "UPDATE index_observations SET freshness='historical', updated_at_ms=?2 WHERE source_id=?1 AND COALESCE(source_generation, '') <> ?3 AND freshness <> 'historical'",
            params![status.source_id, updated_at_ms, current_generation],
        )?;

        if status.health == HealthState::Unavailable {
            transaction.execute(
                "UPDATE index_observations SET freshness='unavailable', updated_at_ms=?2 WHERE source_id=?1 AND freshness <> 'historical'",
                params![status.source_id, updated_at_ms],
            )?;
            transaction.commit()?;
            return Ok(());
        }

        let mut existing = BTreeMap::new();
        {
            let mut statement = transaction.prepare(
                "SELECT native_identity, index_handle FROM index_observations WHERE source_id=?1 AND COALESCE(source_generation, '')=?2 AND freshness <> 'historical'",
            )?;
            for row in statement
                .query_map(params![status.source_id, current_generation], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
            {
                let (identity, raw_handle) = row?;
                let handle = raw_handle.parse::<IndexHandle>().map_err(|error| {
                    StateError::InvalidIndexState(format!(
                        "stored index handle is invalid: {error}"
                    ))
                })?;
                existing.insert(identity, handle);
            }
        }

        let unseen_freshness = if status.health == HealthState::Healthy {
            "historical"
        } else {
            "stale"
        };
        transaction.execute(
            "UPDATE index_observations SET freshness=?3, updated_at_ms=?2 WHERE source_id=?1 AND COALESCE(source_generation, '')=?4 AND freshness <> 'historical'",
            params![status.source_id, updated_at_ms, unseen_freshness, current_generation],
        )?;

        for observation in observations {
            let handle = existing
                .get(&observation.native_identity)
                .copied()
                .unwrap_or_else(IndexHandle::new);
            let metadata = serde_json::to_string(&observation.metadata).map_err(|_| {
                StateError::InvalidIndexState("index metadata encoding failed".into())
            })?;
            let control_paths =
                serde_json::to_string(&observation.control_paths).map_err(|_| {
                    StateError::InvalidIndexState("control-path encoding failed".into())
                })?;
            transaction.execute(
                "INSERT INTO index_observations(index_handle, owner_uid, owner_gid, resource_type, source_kind, source_generation, authoritative_ref, freshness, observed_at_ms, expires_at_ms, safe_metadata_json, source_id, native_identity, control_paths_json, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, ?12, ?13, ?14) ON CONFLICT(index_handle) DO UPDATE SET owner_uid=excluded.owner_uid, owner_gid=excluded.owner_gid, resource_type=excluded.resource_type, source_kind=excluded.source_kind, source_generation=excluded.source_generation, authoritative_ref=excluded.authoritative_ref, freshness=excluded.freshness, observed_at_ms=excluded.observed_at_ms, safe_metadata_json=excluded.safe_metadata_json, source_id=excluded.source_id, native_identity=excluded.native_identity, control_paths_json=excluded.control_paths_json, updated_at_ms=excluded.updated_at_ms",
                params![
                    handle.to_string(),
                    observation.owner.map(Principal::uid),
                    observation.owner.map(Principal::gid),
                    observation.resource_type.as_str(),
                    observation.source_kind.as_str(),
                    observation.source_generation,
                    observation.authoritative_ref,
                    observation.freshness.as_str(),
                    observation.observed_at_ms,
                    metadata,
                    observation.source_id,
                    observation.native_identity,
                    control_paths,
                    updated_at_ms,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn replace_index_relations_for_source(
        &mut self,
        source_id: &str,
        relations: &[IndexRelationInput],
    ) -> StateResult<usize> {
        validate_bounded_string(source_id, MAX_SOURCE_ID_BYTES, "relation source id")?;
        if relations.len() > MAX_SOURCE_RELATIONS {
            return Err(StateError::InvalidIndexState(
                "source relation set exceeds bounded limit".into(),
            ));
        }
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "DELETE FROM index_relations WHERE source_id=?1",
            params![source_id],
        )?;
        let mut inserted = 0;
        for relation in relations {
            if relation.source_id != source_id {
                return Err(StateError::InvalidIndexState(
                    "relation source id does not match reconciliation source".into(),
                ));
            }
            validate_relation(relation)?;
            let from_handle =
                resolve_current_authoritative_ref(&transaction, &relation.from_authoritative_ref)?;
            let to_handle =
                resolve_current_authoritative_ref(&transaction, &relation.to_authoritative_ref)?;
            let (Some(from_handle), Some(to_handle)) = (from_handle, to_handle) else {
                continue;
            };
            if from_handle == to_handle {
                continue;
            }
            transaction.execute(
                "INSERT INTO index_relations(from_handle, to_handle, relation_kind, evidence_strength, source_kind, observed_at_ms, source_id, reason_code) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(from_handle, to_handle, relation_kind, source_kind) DO UPDATE SET evidence_strength=excluded.evidence_strength, observed_at_ms=excluded.observed_at_ms, source_id=excluded.source_id, reason_code=excluded.reason_code",
                params![
                    from_handle.to_string(),
                    to_handle.to_string(),
                    relation.relation_kind,
                    relation.evidence_strength.as_str(),
                    relation.source_kind.as_str(),
                    relation.observed_at_ms,
                    relation.source_id,
                    relation.reason_code,
                ],
            )?;
            inserted += 1;
        }
        transaction.commit()?;
        Ok(inserted)
    }

    pub fn current_index_observations(&self) -> StateResult<Vec<IndexObservation>> {
        let mut statement = self.connection.prepare(
            "SELECT index_handle, resource_type, source_id, source_kind, COALESCE(source_generation,''), native_identity, authoritative_ref, owner_uid, owner_gid, freshness, observed_at_ms, updated_at_ms, safe_metadata_json, control_paths_json FROM index_observations WHERE freshness <> 'historical' ORDER BY index_handle",
        )?;
        statement
            .query_map([], observation_from_row)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(Ok)
            .collect()
    }

    pub fn query_index_visible(
        &self,
        principal: Principal,
        filter: &IndexQueryFilter,
        limit: u16,
        cursor: Option<&IndexHandle>,
    ) -> StateResult<IndexPage<IndexObservation>> {
        if limit == 0 || limit > 200 {
            return Err(StateError::InvalidIndexState(
                "index query limit must be between 1 and 200".into(),
            ));
        }
        let mut items = Vec::new();
        let mut scanned = 0_usize;
        let mut after = cursor.map(ToString::to_string);
        let mut exhausted = false;
        let mut last_scanned = None;
        while items.len() < usize::from(limit) && scanned < MAX_INDEX_QUERY_SCAN && !exhausted {
            let chunk = self.query_index_chunk(principal, filter, after.as_deref(), QUERY_CHUNK)?;
            if chunk.is_empty() {
                exhausted = true;
                break;
            }
            for observation in chunk {
                scanned += 1;
                last_scanned = Some(observation.index_handle.to_string());
                after = last_scanned.clone();
                if self.matches_index_query(&observation, filter, principal)? {
                    items.push(observation);
                    if items.len() == usize::from(limit) {
                        break;
                    }
                }
                if scanned >= MAX_INDEX_QUERY_SCAN {
                    break;
                }
            }
            if scanned % QUERY_CHUNK != 0 {
                exhausted = true;
            }
        }
        let bounded_scan_truncated = scanned >= MAX_INDEX_QUERY_SCAN && !exhausted;
        let next_cursor = if bounded_scan_truncated || items.len() == usize::from(limit) {
            last_scanned
        } else {
            None
        };
        let relevant_source = filter.source_kind.or_else(|| {
            filter
                .resource_type
                .map(|resource_type| match resource_type {
                    IndexResourceType::ApplicationDefinition => IndexSourceKind::Applications,
                    IndexResourceType::Process => IndexSourceKind::Proc,
                    IndexResourceType::OpenRcService => IndexSourceKind::OpenRc,
                    IndexResourceType::Window => IndexSourceKind::X11,
                    IndexResourceType::Workspace | IndexResourceType::Display => {
                        IndexSourceKind::I3
                    }
                    IndexResourceType::ProviderRegistration
                    | IndexResourceType::ProviderResource
                    | IndexResourceType::RegisteredCapability => IndexSourceKind::Providers,
                    IndexResourceType::ApplicationInstance => IndexSourceKind::Correlation,
                })
        });
        let partial =
            bounded_scan_truncated || self.index_sources_partial_for(principal, relevant_source)?;
        Ok(IndexPage {
            items,
            next_cursor,
            partial,
        })
    }

    pub fn index_view_visible(
        &self,
        principal: Principal,
        resource_ref: &str,
    ) -> StateResult<Option<IndexView>> {
        let resource = self.resolve_visible_resource(principal, resource_ref)?;
        let Some(resource) = resource else {
            return Ok(None);
        };
        let relations = self.relations_visible_for_handle(principal, &resource.index_handle)?;
        Ok(Some(IndexView {
            resource,
            relations,
        }))
    }

    pub fn index_topology_visible(
        &self,
        principal: Principal,
        resource_ref: &str,
        max_depth: u8,
        max_resources: u16,
    ) -> StateResult<Option<IndexTopologyView>> {
        if max_depth == 0 || max_depth > 6 || max_resources == 0 || max_resources > 200 {
            return Err(StateError::InvalidIndexState(
                "topology depth/limit exceeds bounded contract".into(),
            ));
        }
        let root = self.resolve_visible_resource(principal, resource_ref)?;
        let Some(root) = root else {
            return Ok(None);
        };
        let mut resources = Vec::new();
        let mut relations = Vec::new();
        let mut seen_handles = BTreeSet::new();
        let mut seen_relations = BTreeSet::new();
        let mut queue = VecDeque::new();
        seen_handles.insert(root.index_handle);
        queue.push_back((root.index_handle, 0_u8));
        let mut truncated = false;
        while let Some((handle, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            for relation in self.relations_visible_for_handle(principal, &handle)? {
                let key = (
                    relation.from_handle,
                    relation.to_handle,
                    relation.relation_kind.clone(),
                    relation.source_id.clone(),
                );
                if seen_relations.insert(key) {
                    relations.push(relation.clone());
                }
                let other = if relation.from_handle == handle {
                    relation.to_handle
                } else {
                    relation.from_handle
                };
                if seen_handles.contains(&other) {
                    continue;
                }
                if seen_handles.len() >= usize::from(max_resources) {
                    truncated = true;
                    continue;
                }
                if let Some(observation) =
                    self.resolve_visible_resource(principal, &other.to_string())?
                {
                    seen_handles.insert(other);
                    resources.push(observation);
                    queue.push_back((other, depth.saturating_add(1)));
                }
            }
        }
        Ok(Some(IndexTopologyView {
            root,
            resources,
            relations,
            depth: max_depth,
            truncated,
        }))
    }

    pub fn index_sources_visible(
        &self,
        principal: Principal,
    ) -> StateResult<Vec<IndexSourceStatus>> {
        let mut statement = self.connection.prepare(
            "SELECT source_id, source_kind, owner_uid, owner_gid, source_generation, health_state, reason_code, last_attempt_at_ms, last_success_at_ms FROM index_sources WHERE owner_uid IS NULL OR (owner_uid=?1 AND owner_gid=?2) ORDER BY source_id",
        )?;
        statement
            .query_map(
                params![principal.uid(), principal.gid()],
                source_status_from_row,
            )?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(Ok)
            .collect()
    }

    pub fn index_runtime_status(&self, principal: Principal) -> StateResult<IndexRuntimeStatus> {
        let (generation_raw, state, reason_code, last_reconcile_at_ms): (i64, String, String, Option<i64>) = self.connection.query_row(
            "SELECT generation, state, reason_code, last_reconcile_at_ms FROM index_runtime_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let generation = u64::try_from(generation_raw).map_err(|_| {
            StateError::InvalidIndexState("stored index generation is negative".into())
        })?;
        Ok(IndexRuntimeStatus {
            generation,
            state: IndexHealthState::from_str(&state)
                .map_err(|error| StateError::InvalidIndexState(error.to_string()))?,
            reason_code,
            last_reconcile_at_ms,
            sources: self.index_sources_visible(principal)?,
        })
    }

    pub fn set_index_runtime_state(
        &self,
        state: IndexHealthState,
        reason_code: &str,
        updated_at_ms: i64,
        reconciled: bool,
    ) -> StateResult<()> {
        validate_bounded_string(reason_code, MAX_REASON_CODE_BYTES, "index reason code")?;
        self.connection.execute(
            "UPDATE index_runtime_state SET state=?1, reason_code=?2, last_reconcile_at_ms=CASE WHEN ?3 THEN ?4 ELSE last_reconcile_at_ms END, updated_at_ms=?4 WHERE singleton=1",
            params![state.as_str(), reason_code, reconciled, updated_at_ms],
        )?;
        Ok(())
    }

    pub fn rebuild_index_derived(&mut self, updated_at_ms: i64) -> StateResult<u64> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM index_relations", [])?;
        transaction.execute("DELETE FROM index_observations", [])?;
        transaction.execute("DELETE FROM index_sources", [])?;
        transaction.execute(
            "UPDATE index_runtime_state SET generation=generation+1, state='rebuilding', reason_code='explicit_rebuild', last_reconcile_at_ms=NULL, updated_at_ms=?1 WHERE singleton=1",
            params![updated_at_ms],
        )?;
        let generation_raw: i64 = transaction.query_row(
            "SELECT generation FROM index_runtime_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let generation = u64::try_from(generation_raw).map_err(|_| {
            StateError::InvalidIndexState("stored index generation is negative".into())
        })?;
        transaction.commit()?;
        Ok(generation)
    }

    fn query_index_chunk(
        &self,
        principal: Principal,
        filter: &IndexQueryFilter,
        after: Option<&str>,
        limit: usize,
    ) -> StateResult<Vec<IndexObservation>> {
        let mut sql = String::from(
            "SELECT index_handle, resource_type, source_id, source_kind, COALESCE(source_generation,''), native_identity, authoritative_ref, owner_uid, owner_gid, freshness, observed_at_ms, updated_at_ms, safe_metadata_json, control_paths_json FROM index_observations WHERE (owner_uid IS NULL OR (owner_uid=? AND owner_gid=?)) AND index_handle > ?",
        );
        let mut values = vec![
            rusqlite::types::Value::Integer(i64::from(principal.uid())),
            rusqlite::types::Value::Integer(i64::from(principal.gid())),
            rusqlite::types::Value::Text(after.unwrap_or("").to_string()),
        ];
        if let Some(resource_type) = filter.resource_type {
            sql.push_str(" AND resource_type=?");
            values.push(rusqlite::types::Value::Text(
                resource_type.as_str().to_string(),
            ));
        }
        if let Some(freshness) = filter.freshness {
            sql.push_str(" AND freshness=?");
            values.push(rusqlite::types::Value::Text(freshness.as_str().to_string()));
        } else {
            sql.push_str(" AND freshness <> 'historical'");
        }
        if let Some(source_kind) = filter.source_kind {
            sql.push_str(" AND source_kind=?");
            values.push(rusqlite::types::Value::Text(
                source_kind.as_str().to_string(),
            ));
        }
        if let Some(changed_since_ms) = filter.changed_since_ms {
            sql.push_str(" AND updated_at_ms>=?");
            values.push(rusqlite::types::Value::Integer(changed_since_ms));
        }
        sql.push_str(" ORDER BY index_handle LIMIT ?");
        values.push(rusqlite::types::Value::Integer(limit as i64));
        let mut statement = self.connection.prepare(&sql)?;
        statement
            .query_map(params_from_iter(values), observation_from_row)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(Ok)
            .collect()
    }

    fn matches_index_query(
        &self,
        observation: &IndexObservation,
        filter: &IndexQueryFilter,
        principal: Principal,
    ) -> StateResult<bool> {
        if let Some(application) = filter.application.as_deref() {
            if !metadata_matches_any(
                &observation.metadata,
                &["desktop_id", "name", "application_ref", "application_name"],
                application,
            ) {
                return Ok(false);
            }
        }
        if let Some(provider) = filter.provider.as_deref() {
            if !metadata_matches_any(
                &observation.metadata,
                &["provider_id", "provider_type"],
                provider,
            ) {
                return Ok(false);
            }
        }
        if let Some(capability) = filter.capability.as_deref() {
            if !metadata_matches_any(&observation.metadata, &["capability_id"], capability) {
                return Ok(false);
            }
        }
        if let Some(control_path) = filter.control_path {
            if !observation.control_paths.contains(&control_path) {
                return Ok(false);
            }
        }
        if let Some(evidence) = filter.evidence {
            let exists: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM index_relations WHERE (from_handle=?1 OR to_handle=?1) AND evidence_strength=?2)",
                params![observation.index_handle.to_string(), evidence.as_str()],
                |row| row.get(0),
            )?;
            if !exists {
                return Ok(false);
            }
        }
        if let Some(workspace) = filter.workspace.as_deref() {
            if observation.resource_type == IndexResourceType::Workspace {
                if !metadata_matches_any(&observation.metadata, &["name"], workspace) {
                    return Ok(false);
                }
            } else if !self.resource_related_to_named(
                principal,
                &observation.index_handle,
                IndexResourceType::Workspace,
                workspace,
            )? {
                return Ok(false);
            }
        }
        if let Some(display) = filter.display.as_deref() {
            if observation.resource_type == IndexResourceType::Display {
                if !metadata_matches_any(&observation.metadata, &["name"], display) {
                    return Ok(false);
                }
            } else if !self.resource_related_to_named(
                principal,
                &observation.index_handle,
                IndexResourceType::Display,
                display,
            )? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn resource_related_to_named(
        &self,
        principal: Principal,
        handle: &IndexHandle,
        resource_type: IndexResourceType,
        name: &str,
    ) -> StateResult<bool> {
        let relations = self.relations_visible_for_handle(principal, handle)?;
        for relation in relations {
            let other = if relation.from_handle == *handle {
                relation.to_handle
            } else {
                relation.from_handle
            };
            if let Some(resource) = self.resolve_visible_resource(principal, &other.to_string())? {
                if resource.resource_type == resource_type
                    && metadata_matches_any(&resource.metadata, &["name"], name)
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn resolve_visible_resource(
        &self,
        principal: Principal,
        resource_ref: &str,
    ) -> StateResult<Option<IndexObservation>> {
        let (column, value) = if resource_ref.parse::<IndexHandle>().is_ok() {
            ("index_handle", resource_ref)
        } else {
            ("authoritative_ref", resource_ref)
        };
        let sql = format!(
            "SELECT index_handle, resource_type, source_id, source_kind, COALESCE(source_generation,''), native_identity, authoritative_ref, owner_uid, owner_gid, freshness, observed_at_ms, updated_at_ms, safe_metadata_json, control_paths_json FROM index_observations WHERE {column}=?1 AND (owner_uid IS NULL OR (owner_uid=?2 AND owner_gid=?3)) ORDER BY updated_at_ms DESC LIMIT 1"
        );
        self.connection
            .query_row(
                &sql,
                params![value, principal.uid(), principal.gid()],
                observation_from_row,
            )
            .optional()
            .map_err(StateError::from)
    }

    fn relations_visible_for_handle(
        &self,
        principal: Principal,
        handle: &IndexHandle,
    ) -> StateResult<Vec<IndexRelation>> {
        let mut statement = self.connection.prepare(
            "SELECT r.from_handle, r.to_handle, r.relation_kind, r.evidence_strength, r.source_id, r.source_kind, r.reason_code, r.observed_at_ms FROM index_relations r JOIN index_observations a ON a.index_handle=r.from_handle JOIN index_observations b ON b.index_handle=r.to_handle WHERE (r.from_handle=?1 OR r.to_handle=?1) AND (a.owner_uid IS NULL OR (a.owner_uid=?2 AND a.owner_gid=?3)) AND (b.owner_uid IS NULL OR (b.owner_uid=?2 AND b.owner_gid=?3)) ORDER BY r.relation_id LIMIT 512",
        )?;
        statement
            .query_map(
                params![handle.to_string(), principal.uid(), principal.gid()],
                relation_from_row,
            )?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(Ok)
            .collect()
    }

    fn index_sources_partial_for(
        &self,
        principal: Principal,
        source_kind: Option<IndexSourceKind>,
    ) -> StateResult<bool> {
        let mut sql = String::from(
            "SELECT EXISTS(SELECT 1 FROM index_sources WHERE (owner_uid IS NULL OR (owner_uid=?1 AND owner_gid=?2)) AND health_state IN ('degraded','unavailable','unknown')",
        );
        let partial = if let Some(source_kind) = source_kind {
            sql.push_str(" AND source_kind=?3)");
            self.connection.query_row(
                &sql,
                params![principal.uid(), principal.gid(), source_kind.as_str()],
                |row| row.get(0),
            )?
        } else {
            sql.push(')');
            self.connection
                .query_row(&sql, params![principal.uid(), principal.gid()], |row| {
                    row.get(0)
                })?
        };
        Ok(partial)
    }
}

fn validate_source_status(status: &IndexSourceStatus) -> StateResult<()> {
    validate_bounded_string(&status.source_id, MAX_SOURCE_ID_BYTES, "source id")?;
    validate_bounded_string(&status.source_generation, 512, "source generation")?;
    validate_bounded_string(
        &status.reason_code,
        MAX_REASON_CODE_BYTES,
        "source reason code",
    )?;
    Ok(())
}

fn validate_observation(
    status: &IndexSourceStatus,
    observation: &IndexObservationInput,
) -> StateResult<()> {
    if observation.source_id != status.source_id
        || observation.source_kind != status.source_kind
        || observation.source_generation != status.source_generation
    {
        return Err(StateError::InvalidIndexState(
            "observation source identity does not match source status".into(),
        ));
    }
    validate_bounded_string(
        &observation.native_identity,
        MAX_NATIVE_ID_BYTES,
        "native identity",
    )?;
    if let Some(reference) = observation.authoritative_ref.as_deref() {
        validate_bounded_string(
            reference,
            MAX_AUTHORITATIVE_REF_BYTES,
            "authoritative reference",
        )?;
    }
    let metadata = serde_json::to_vec(&observation.metadata)
        .map_err(|_| StateError::InvalidIndexState("index metadata encoding failed".into()))?;
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(StateError::InvalidIndexState(
            "index metadata exceeds bounded limit".into(),
        ));
    }
    if observation.control_paths.len() > 16 {
        return Err(StateError::InvalidIndexState(
            "too many control paths on index observation".into(),
        ));
    }
    Ok(())
}

fn validate_relation(relation: &IndexRelationInput) -> StateResult<()> {
    validate_bounded_string(
        &relation.from_authoritative_ref,
        MAX_AUTHORITATIVE_REF_BYTES,
        "relation source reference",
    )?;
    validate_bounded_string(
        &relation.to_authoritative_ref,
        MAX_AUTHORITATIVE_REF_BYTES,
        "relation target reference",
    )?;
    validate_bounded_string(
        &relation.relation_kind,
        MAX_RELATION_KIND_BYTES,
        "relation kind",
    )?;
    validate_bounded_string(
        &relation.reason_code,
        MAX_REASON_CODE_BYTES,
        "relation reason code",
    )?;
    Ok(())
}

fn validate_bounded_string(value: &str, max_bytes: usize, field: &str) -> StateResult<()> {
    if value.is_empty() || value.len() > max_bytes {
        return Err(StateError::InvalidIndexState(format!(
            "{field} is empty or exceeds {max_bytes} bytes"
        )));
    }
    Ok(())
}

fn upsert_source_status(
    transaction: &Transaction<'_>,
    status: &IndexSourceStatus,
    updated_at_ms: i64,
) -> StateResult<()> {
    transaction.execute(
        "INSERT INTO index_sources(source_id, source_kind, owner_uid, owner_gid, source_generation, health_state, reason_code, last_attempt_at_ms, last_success_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) ON CONFLICT(source_id) DO UPDATE SET source_kind=excluded.source_kind, owner_uid=excluded.owner_uid, owner_gid=excluded.owner_gid, source_generation=excluded.source_generation, health_state=excluded.health_state, reason_code=excluded.reason_code, last_attempt_at_ms=excluded.last_attempt_at_ms, last_success_at_ms=COALESCE(excluded.last_success_at_ms,index_sources.last_success_at_ms), updated_at_ms=excluded.updated_at_ms",
        params![
            status.source_id,
            status.source_kind.as_str(),
            status.owner.map(Principal::uid),
            status.owner.map(Principal::gid),
            status.source_generation,
            status.health.as_str(),
            status.reason_code,
            status.last_attempt_at_ms,
            status.last_success_at_ms,
            updated_at_ms,
        ],
    )?;
    Ok(())
}

fn resolve_current_authoritative_ref(
    transaction: &Transaction<'_>,
    reference: &str,
) -> StateResult<Option<IndexHandle>> {
    let raw = transaction
        .query_row(
            "SELECT index_handle FROM index_observations WHERE authoritative_ref=?1 AND freshness IN ('live','recent','stale') ORDER BY updated_at_ms DESC LIMIT 1",
            params![reference],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    raw.map(|value| {
        value
            .parse::<IndexHandle>()
            .map_err(|error| StateError::InvalidIndexState(error.to_string()))
    })
    .transpose()
}

fn observation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexObservation> {
    let handle_raw: String = row.get(0)?;
    let resource_type_raw: String = row.get(1)?;
    let source_kind_raw: String = row.get(3)?;
    let owner_uid: Option<u32> = row.get(7)?;
    let owner_gid: Option<u32> = row.get(8)?;
    let freshness_raw: String = row.get(9)?;
    let metadata_raw: String = row.get(12)?;
    let control_paths_raw: String = row.get(13)?;
    Ok(IndexObservation {
        index_handle: parse_index_handle_sql(&handle_raw, 0)?,
        resource_type: IndexResourceType::from_str(&resource_type_raw)
            .map_err(|error| sql_conversion(error, 1))?,
        source_id: row.get(2)?,
        source_kind: IndexSourceKind::from_str(&source_kind_raw)
            .map_err(|error| sql_conversion(error, 3))?,
        source_generation: row.get(4)?,
        native_identity: row.get(5)?,
        authoritative_ref: row.get(6)?,
        owner: owner_uid
            .zip(owner_gid)
            .map(|(uid, gid)| Principal::new(uid, gid)),
        freshness: parse_freshness(&freshness_raw).map_err(|error| sql_conversion(error, 9))?,
        observed_at_ms: row.get(10)?,
        updated_at_ms: row.get(11)?,
        metadata: serde_json::from_str(&metadata_raw).map_err(|error| sql_conversion(error, 12))?,
        control_paths: serde_json::from_str(&control_paths_raw)
            .map_err(|error| sql_conversion(error, 13))?,
    })
}

fn relation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexRelation> {
    let from_raw: String = row.get(0)?;
    let to_raw: String = row.get(1)?;
    let evidence_raw: String = row.get(3)?;
    let source_kind_raw: String = row.get(5)?;
    Ok(IndexRelation {
        from_handle: parse_index_handle_sql(&from_raw, 0)?,
        to_handle: parse_index_handle_sql(&to_raw, 1)?,
        relation_kind: row.get(2)?,
        evidence_strength: parse_evidence(&evidence_raw)
            .map_err(|error| sql_conversion(error, 3))?,
        source_id: row.get(4)?,
        source_kind: IndexSourceKind::from_str(&source_kind_raw)
            .map_err(|error| sql_conversion(error, 5))?,
        reason_code: row.get(6)?,
        observed_at_ms: row.get(7)?,
    })
}

fn source_status_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexSourceStatus> {
    let source_kind_raw: String = row.get(1)?;
    let owner_uid: Option<u32> = row.get(2)?;
    let owner_gid: Option<u32> = row.get(3)?;
    let health_raw: String = row.get(5)?;
    Ok(IndexSourceStatus {
        source_id: row.get(0)?,
        source_kind: IndexSourceKind::from_str(&source_kind_raw)
            .map_err(|error| sql_conversion(error, 1))?,
        owner: owner_uid
            .zip(owner_gid)
            .map(|(uid, gid)| Principal::new(uid, gid)),
        source_generation: row.get(4)?,
        health: parse_health(&health_raw).map_err(|error| sql_conversion(error, 5))?,
        reason_code: row.get(6)?,
        last_attempt_at_ms: row.get(7)?,
        last_success_at_ms: row.get(8)?,
    })
}

fn parse_index_handle_sql(value: &str, column: usize) -> rusqlite::Result<IndexHandle> {
    value
        .parse::<IndexHandle>()
        .map_err(|error| sql_conversion(error, column))
}

fn sql_conversion(
    error: impl std::error::Error + Send + Sync + 'static,
    column: usize,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(error))
}

fn parse_freshness(value: &str) -> Result<Freshness, SimpleParseError> {
    match value {
        "live" => Ok(Freshness::Live),
        "recent" => Ok(Freshness::Recent),
        "stale" => Ok(Freshness::Stale),
        "unavailable" => Ok(Freshness::Unavailable),
        "historical" => Ok(Freshness::Historical),
        _ => Err(SimpleParseError(value.to_string())),
    }
}

fn parse_evidence(value: &str) -> Result<EvidenceStrength, SimpleParseError> {
    match value {
        "authoritative" => Ok(EvidenceStrength::Authoritative),
        "strong" => Ok(EvidenceStrength::Strong),
        "heuristic" => Ok(EvidenceStrength::Heuristic),
        _ => Err(SimpleParseError(value.to_string())),
    }
}

fn parse_health(value: &str) -> Result<HealthState, SimpleParseError> {
    match value {
        "healthy" => Ok(HealthState::Healthy),
        "degraded" => Ok(HealthState::Degraded),
        "unavailable" => Ok(HealthState::Unavailable),
        "unknown" => Ok(HealthState::Unknown),
        _ => Err(SimpleParseError(value.to_string())),
    }
}

#[derive(Debug)]
struct SimpleParseError(String);

impl std::fmt::Display for SimpleParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid stored enum value '{}'", self.0)
    }
}

impl std::error::Error for SimpleParseError {}

fn metadata_matches_any(metadata: &Value, keys: &[&str], expected: &str) -> bool {
    keys.iter().any(|key| {
        metadata
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case(expected))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{fs, path::PathBuf};

    struct TestDb {
        dir: PathBuf,
        state: PortusState,
    }

    impl TestDb {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "portus-index-state-{name}-{}",
                portus_protocol::TaskId::new()
            ));
            fs::create_dir_all(&dir).unwrap();
            let state = PortusState::open(dir.join("portus.db")).unwrap();
            Self { dir, state }
        }
    }

    impl Drop for TestDb {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn status(health: HealthState, generation: &str, at: i64) -> IndexSourceStatus {
        IndexSourceStatus {
            source_id: "proc".into(),
            source_kind: IndexSourceKind::Proc,
            owner: None,
            source_generation: generation.into(),
            health,
            reason_code: if health == HealthState::Healthy {
                "ready"
            } else {
                "partial"
            }
            .into(),
            last_attempt_at_ms: at,
            last_success_at_ms: (health != HealthState::Unavailable).then_some(at),
        }
    }

    fn process(
        generation: &str,
        pid: u32,
        start: u64,
        owner: Principal,
        at: i64,
    ) -> IndexObservationInput {
        IndexObservationInput {
            resource_type: IndexResourceType::Process,
            source_id: "proc".into(),
            source_kind: IndexSourceKind::Proc,
            source_generation: generation.into(),
            native_identity: format!("{pid}:{start}"),
            authoritative_ref: Some(format!("process:{generation}:{pid}:{start}")),
            owner: Some(owner),
            freshness: Freshness::Recent,
            observed_at_ms: at,
            metadata: json!({"pid":pid,"ppid":1,"start_ticks":start,"comm":"demo","exe_basename":"demo"}),
            control_paths: vec![ControlPathKind::NativeSystem],
        }
    }

    #[test]
    fn exact_source_generation_identity_preserves_handle_but_pid_reuse_does_not() {
        let mut db = TestDb::new("generation");
        let owner = Principal::new(1000, 1000);
        db.state
            .reconcile_index_source(
                &status(HealthState::Healthy, "boot-a", 10),
                &[process("boot-a", 42, 100, owner, 10)],
                10,
            )
            .unwrap();
        let first = db
            .state
            .query_index_visible(owner, &IndexQueryFilter::default(), 10, None)
            .unwrap()
            .items[0]
            .clone();
        db.state
            .reconcile_index_source(
                &status(HealthState::Healthy, "boot-a", 20),
                &[process("boot-a", 42, 100, owner, 20)],
                20,
            )
            .unwrap();
        let second = db
            .state
            .query_index_visible(owner, &IndexQueryFilter::default(), 10, None)
            .unwrap()
            .items[0]
            .clone();
        assert_eq!(first.index_handle, second.index_handle);

        db.state
            .reconcile_index_source(
                &status(HealthState::Healthy, "boot-a", 30),
                &[process("boot-a", 42, 999, owner, 30)],
                30,
            )
            .unwrap();
        let current = db
            .state
            .query_index_visible(owner, &IndexQueryFilter::default(), 10, None)
            .unwrap()
            .items[0]
            .clone();
        assert_ne!(current.index_handle, first.index_handle);
        let old = db
            .state
            .index_view_visible(owner, &first.index_handle.to_string())
            .unwrap()
            .unwrap();
        assert_eq!(old.resource.freshness, Freshness::Historical);
    }

    #[test]
    fn degraded_and_unavailable_sources_do_not_invent_disappearance() {
        let mut db = TestDb::new("partial");
        let owner = Principal::new(1000, 1000);
        let first = process("boot-a", 1, 10, owner, 10);
        let second = process("boot-a", 2, 20, owner, 10);
        db.state
            .reconcile_index_source(
                &status(HealthState::Healthy, "boot-a", 10),
                &[first.clone(), second.clone()],
                10,
            )
            .unwrap();
        db.state
            .reconcile_index_source(&status(HealthState::Degraded, "boot-a", 20), &[first], 20)
            .unwrap();
        let stale = db
            .state
            .query_index_visible(
                owner,
                &IndexQueryFilter {
                    freshness: Some(Freshness::Stale),
                    ..IndexQueryFilter::default()
                },
                10,
                None,
            )
            .unwrap();
        assert_eq!(stale.items.len(), 1);
        assert_eq!(stale.items[0].metadata["pid"], 2);

        db.state
            .reconcile_index_source(&status(HealthState::Unavailable, "boot-a", 30), &[], 30)
            .unwrap();
        let unavailable = db
            .state
            .query_index_visible(
                owner,
                &IndexQueryFilter {
                    freshness: Some(Freshness::Unavailable),
                    ..IndexQueryFilter::default()
                },
                10,
                None,
            )
            .unwrap();
        assert_eq!(unavailable.items.len(), 2);
    }

    #[test]
    fn exact_resource_query_ignores_unrelated_source_degradation() {
        let mut db = TestDb::new("relevant-source");
        let owner = Principal::new(1000, 1000);
        db.state
            .reconcile_index_source(
                &status(HealthState::Healthy, "boot-a", 10),
                &[process("boot-a", 42, 100, owner, 10)],
                10,
            )
            .unwrap();
        let x11_status = IndexSourceStatus {
            source_id: "x11:1000".into(),
            source_kind: IndexSourceKind::X11,
            owner: Some(owner),
            source_generation: "graph-a".into(),
            health: HealthState::Unavailable,
            reason_code: "no_graphical_session".into(),
            last_attempt_at_ms: 10,
            last_success_at_ms: None,
        };
        db.state
            .reconcile_index_source(&x11_status, &[], 10)
            .unwrap();
        let page = db
            .state
            .query_index_visible(
                owner,
                &IndexQueryFilter {
                    resource_type: Some(IndexResourceType::Process),
                    ..IndexQueryFilter::default()
                },
                10,
                None,
            )
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(!page.partial);
    }

    #[test]
    fn principal_filtering_hides_other_users_live_resources() {
        let mut db = TestDb::new("privacy");
        let owner_a = Principal::new(1000, 1000);
        let owner_b = Principal::new(1001, 1001);
        db.state
            .reconcile_index_source(
                &status(HealthState::Healthy, "boot-a", 10),
                &[
                    process("boot-a", 10, 10, owner_a, 10),
                    process("boot-a", 11, 11, owner_b, 10),
                ],
                10,
            )
            .unwrap();
        let a = db
            .state
            .query_index_visible(owner_a, &IndexQueryFilter::default(), 10, None)
            .unwrap();
        let b = db
            .state
            .query_index_visible(owner_b, &IndexQueryFilter::default(), 10, None)
            .unwrap();
        assert_eq!(a.items.len(), 1);
        assert_eq!(b.items.len(), 1);
        assert_eq!(a.items[0].metadata["pid"], 10);
        assert_eq!(b.items[0].metadata["pid"], 11);
    }

    #[test]
    fn topology_uses_evidence_relations_and_remains_bounded() {
        let mut db = TestDb::new("topology");
        let owner = Principal::new(1000, 1000);
        let process = process("boot-a", 42, 100, owner, 10);
        let process_ref = process.authoritative_ref.clone().unwrap();
        db.state
            .reconcile_index_source(&status(HealthState::Healthy, "boot-a", 10), &[process], 10)
            .unwrap();
        let window_status = IndexSourceStatus {
            source_id: "x11:1000".into(),
            source_kind: IndexSourceKind::X11,
            owner: Some(owner),
            source_generation: "graph-a".into(),
            health: HealthState::Healthy,
            reason_code: "ready".into(),
            last_attempt_at_ms: 10,
            last_success_at_ms: Some(10),
        };
        let window_ref = "window:graph-a:77".to_string();
        let window = IndexObservationInput {
            resource_type: IndexResourceType::Window,
            source_id: "x11:1000".into(),
            source_kind: IndexSourceKind::X11,
            source_generation: "graph-a".into(),
            native_identity: "xid:77".into(),
            authoritative_ref: Some(window_ref.clone()),
            owner: Some(owner),
            freshness: Freshness::Recent,
            observed_at_ms: 10,
            metadata: json!({"xid":77,"process_ref":process_ref}),
            control_paths: vec![ControlPathKind::ProcessWindow],
        };
        db.state
            .reconcile_index_source(&window_status, &[window], 10)
            .unwrap();
        db.state
            .replace_index_relations_for_source(
                "correlation",
                &[IndexRelationInput {
                    from_authoritative_ref: window_ref.clone(),
                    to_authoritative_ref: process_ref.clone(),
                    relation_kind: "owned_by_process".into(),
                    evidence_strength: EvidenceStrength::Strong,
                    source_id: "correlation".into(),
                    source_kind: IndexSourceKind::Correlation,
                    reason_code: "fixture".into(),
                    observed_at_ms: 10,
                }],
            )
            .unwrap();
        let topology = db
            .state
            .index_topology_visible(owner, &window_ref, 2, 10)
            .unwrap()
            .unwrap();
        assert_eq!(topology.resources.len(), 1);
        assert_eq!(topology.relations.len(), 1);
        assert_eq!(
            topology.relations[0].evidence_strength,
            EvidenceStrength::Strong
        );
    }

    #[test]
    fn rebuild_drops_only_derived_index_state_and_preserves_annotations() {
        let mut db = TestDb::new("rebuild");
        let owner = Principal::new(1000, 1000);
        db.state
            .reconcile_index_source(
                &status(HealthState::Healthy, "boot-a", 10),
                &[process("boot-a", 42, 100, owner, 10)],
                10,
            )
            .unwrap();
        db.state
            .connection
            .execute(
                "INSERT INTO index_annotations(owner_uid, owner_gid, target_ref, annotation_kind, safe_value, created_at_ms, updated_at_ms) VALUES (1000,1000,'role:test','role','demo',1,1)",
                [],
            )
            .unwrap();
        let generation = db.state.rebuild_index_derived(20).unwrap();
        assert_eq!(generation, 2);
        let observations: i64 = db
            .state
            .connection
            .query_row("SELECT COUNT(*) FROM index_observations", [], |row| {
                row.get(0)
            })
            .unwrap();
        let annotations: i64 = db
            .state
            .connection
            .query_row("SELECT COUNT(*) FROM index_annotations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(observations, 0);
        assert_eq!(annotations, 1);
    }
}
