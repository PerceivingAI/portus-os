use crate::{
    NewSignificantEvent, PortusState, StateError, StateResult,
    event::{insert_significant_event_tx, prune_object_tx},
    safety::{secret_like_key, secret_like_text},
};
use portus_protocol::{
    ArtifactAvailabilityState, ArtifactCleanupAuthority, ArtifactConfidentiality, ArtifactHold,
    ArtifactHoldKind, ArtifactId, ArtifactIntegrityKind, ArtifactLocator, ArtifactPage,
    ArtifactRecord, ArtifactRegistrationSpec, ArtifactRetentionKind, ArtifactSummary,
    ArtifactTaskRelationship, ArtifactView, EventObjectKind, Principal, ProviderResourceId,
    ProviderResourceRef, ResourceType,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use std::{collections::BTreeMap, str::FromStr};

pub const MAX_ARTIFACT_LIST_PAGE: u16 = 200;
pub const MAX_ARTIFACT_METADATA_FIELDS: usize = 16;
pub const MAX_ARTIFACT_METADATA_JSON_BYTES: usize = 8192;
pub const MAX_ARTIFACT_SHARED_PRINCIPALS: usize = 64;
const MAX_ARTIFACT_TEXT_BYTES: usize = 4096;
const MAX_ARTIFACT_DISPLAY_BYTES: usize = 256;
const MAX_ARTIFACT_MEDIA_BYTES: usize = 128;
const MAX_ARTIFACT_HOLDER_REF_BYTES: usize = 512;

type LocatorColumns<'a> = (
    Option<&'a str>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<&'a str>,
);

type ArtifactSummaryRaw = (
    String,
    u32,
    u32,
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    Option<String>,
    i64,
);

struct ArtifactEventSpec<'a> {
    event_kind: &'a str,
    reason_code: &'a str,
    safe_summary: Option<&'a str>,
    safe_data: Value,
    occurred_at_ms: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactCleanupEligibility {
    Eligible,
    NoCleanupAuthority,
    Retained,
    NotExpired,
    Held,
    ActiveTaskRelationship,
    ContentNotAvailable,
}

impl PortusState {
    pub fn register_artifact(
        &mut self,
        spec: &ArtifactRegistrationSpec,
    ) -> StateResult<ArtifactView> {
        validate_registration_spec(spec)?;
        let tx = self.connection.transaction()?;
        ensure_artifact_identity_unused(&tx, &spec.artifact_id)?;
        validate_task_relationship(&tx, spec)?;
        validate_provider_relationship(&tx, spec)?;
        let metadata = serde_json::to_string(&spec.safe_metadata).map_err(|_| {
            StateError::InvalidArtifactState("safe artifact metadata is not serializable".into())
        })?;
        let (filesystem_path, provider_id, resource_type, resource_id, generation) =
            locator_columns(&spec.locator);
        tx.execute(
            "INSERT INTO artifacts(artifact_id, owner_uid, owner_gid, artifact_type, confidentiality, retention_kind, expires_at_ms, availability_state, locator_kind, filesystem_path, provider_id, provider_resource_type, provider_resource_id, provider_generation, integrity_kind, sha256, size_bytes, media_type, created_at_ms, registered_at_ms, project_ref, safe_display_name, safe_metadata_json, last_verified_at_ms, removed_at_ms, cleanup_authority, cleanup_ref) VALUES (?1,?2,?3,?4,?5,?6,?7,'available',?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,NULL,?24,?25)",
            params![
                spec.artifact_id.to_string(),
                spec.owner.uid(),
                spec.owner.gid(),
                spec.artifact_type.as_str(),
                spec.confidentiality.as_str(),
                spec.retention_kind.as_str(),
                spec.expires_at_ms,
                spec.locator.kind(),
                filesystem_path,
                provider_id,
                resource_type,
                resource_id,
                generation,
                spec.integrity_kind.as_str(),
                spec.sha256,
                spec.size_bytes.and_then(|value| i64::try_from(value).ok()),
                spec.media_type,
                spec.created_at_ms,
                spec.registered_at_ms,
                spec.project_ref,
                spec.safe_display_name,
                metadata,
                (spec.integrity_kind == ArtifactIntegrityKind::Verified)
                    .then_some(spec.registered_at_ms),
                spec.cleanup_authority.as_str(),
                spec.cleanup_ref,
            ],
        )?;

        if let Some(task_id) = spec.source_task_id {
            tx.execute(
                "INSERT INTO artifact_task_relationships(artifact_id, task_id, relationship_kind, created_at_ms) VALUES (?1,?2,'produced_by',?3)",
                params![spec.artifact_id.to_string(), task_id.to_string(), spec.registered_at_ms],
            )?;
        }
        if let ArtifactLocator::ProviderResource { reference } = &spec.locator {
            tx.execute(
                "INSERT INTO artifact_provider_relationships(artifact_id, provider_id, resource_type, resource_id, generation, created_at_ms) VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    spec.artifact_id.to_string(),
                    reference.provider_registration_id.to_string(),
                    reference.resource_type.to_string(),
                    reference.resource_id.to_string(),
                    reference.generation.as_deref().unwrap_or(""),
                    spec.registered_at_ms,
                ],
            )?;
        }
        for principal in &spec.shared_with {
            tx.execute(
                "INSERT INTO artifact_grants(artifact_id, principal_uid, principal_gid, created_at_ms) VALUES (?1,?2,?3,?4)",
                params![
                    spec.artifact_id.to_string(),
                    principal.uid(),
                    principal.gid(),
                    spec.registered_at_ms,
                ],
            )?;
        }
        insert_artifact_event(
            &tx,
            spec.artifact_id,
            spec.owner,
            ArtifactEventSpec {
                event_kind: "artifact.registered",
                reason_code: "registered",
                safe_summary: Some("artifact registration created"),
                safe_data: json!({"artifact_type":spec.artifact_type.as_str(),"locator_kind":spec.locator.kind()}),
                occurred_at_ms: spec.registered_at_ms,
            },
        )?;
        tx.commit()?;
        self.artifact_view_visible(&spec.artifact_id, spec.owner)?
            .ok_or_else(|| {
                StateError::InvalidArtifactState("registered artifact is not readable".into())
            })
    }

    pub fn list_artifacts_visible(
        &self,
        principal: Principal,
        limit: u16,
        after: Option<&ArtifactId>,
    ) -> StateResult<ArtifactPage> {
        validate_list_limit(limit)?;
        let after = after.map(ToString::to_string);
        let mut statement = self.connection.prepare(
            "SELECT artifact_id, owner_uid, owner_gid, artifact_type, confidentiality, retention_kind, availability_state, integrity_kind, size_bytes, safe_display_name, registered_at_ms FROM artifacts a WHERE (?1=0 OR (a.owner_uid=?1 AND a.owner_gid=?2) OR a.confidentiality='public' OR (a.confidentiality='shared' AND EXISTS(SELECT 1 FROM artifact_grants g WHERE g.artifact_id=a.artifact_id AND g.principal_uid=?1 AND g.principal_gid=?2))) AND (?3 IS NULL OR a.artifact_id>?3) ORDER BY a.artifact_id LIMIT ?4",
        )?;
        let mut raw = statement
            .query_map(
                params![
                    principal.uid(),
                    principal.gid(),
                    after,
                    i64::from(limit) + 1
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, u32>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = raw.len() > usize::from(limit);
        raw.truncate(usize::from(limit));
        let mut items = Vec::with_capacity(raw.len());
        for row in raw {
            items.push(summary_from_raw(row)?);
        }
        let next_cursor = has_more
            .then(|| items.last().map(|item| item.artifact_id.to_string()))
            .flatten();
        Ok(ArtifactPage { items, next_cursor })
    }

    pub fn artifact_view_visible(
        &self,
        artifact_id: &ArtifactId,
        principal: Principal,
    ) -> StateResult<Option<ArtifactView>> {
        let Some(artifact) = artifact_record_visible(&self.connection, artifact_id, principal)?
        else {
            return Ok(None);
        };
        let task_relationships = artifact_task_relationships(&self.connection, artifact_id)?;
        let provider_resource = artifact_provider_resource(&self.connection, artifact_id)?;
        let shared_with = artifact_grants(&self.connection, artifact_id)?;
        let holds = artifact_holds(&self.connection, artifact_id)?;
        Ok(Some(ArtifactView {
            artifact,
            task_relationships,
            provider_resource,
            shared_with,
            holds,
        }))
    }

    pub fn update_artifact_observation(
        &mut self,
        artifact_id: &ArtifactId,
        availability: ArtifactAvailabilityState,
        integrity: ArtifactIntegrityKind,
        observed_at_ms: i64,
    ) -> StateResult<()> {
        if observed_at_ms < 0 {
            return Err(StateError::InvalidArtifactState(
                "artifact observation timestamp must not be negative".into(),
            ));
        }
        let tx = self.connection.transaction()?;
        let previous: Option<(String, String, u32, u32)> = tx
            .query_row(
                "SELECT availability_state, integrity_kind, owner_uid, owner_gid FROM artifacts WHERE artifact_id=?1",
                params![artifact_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((old_availability, old_integrity, uid, gid)) = previous else {
            return Err(StateError::InvalidArtifactState(
                "artifact does not exist".into(),
            ));
        };
        tx.execute(
            "UPDATE artifacts SET availability_state=?2, integrity_kind=?3, last_verified_at_ms=?4 WHERE artifact_id=?1",
            params![artifact_id.to_string(), availability.as_str(), integrity.as_str(), observed_at_ms],
        )?;
        if old_availability != availability.as_str() || old_integrity != integrity.as_str() {
            insert_artifact_event(
                &tx,
                *artifact_id,
                Principal::new(uid, gid),
                ArtifactEventSpec {
                    event_kind: "artifact.observed",
                    reason_code: "state_changed",
                    safe_summary: Some("artifact availability or integrity changed"),
                    safe_data: json!({"availability":availability.as_str(),"integrity":integrity.as_str()}),
                    occurred_at_ms: observed_at_ms,
                },
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn add_artifact_hold(
        &self,
        artifact_id: &ArtifactId,
        owner_or_root: Principal,
        hold: &ArtifactHold,
    ) -> StateResult<()> {
        authorize_owner_or_root(&self.connection, artifact_id, owner_or_root)?;
        if hold.holder_ref.is_empty()
            || hold.holder_ref.len() > MAX_ARTIFACT_HOLDER_REF_BYTES
            || hold.holder_ref.contains(['\r', '\n'])
        {
            return Err(StateError::InvalidArtifactState(
                "artifact hold reference is invalid".into(),
            ));
        }
        self.connection.execute(
            "INSERT INTO artifact_holds(artifact_id, hold_kind, holder_ref, created_at_ms, expires_at_ms) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(artifact_id, hold_kind, holder_ref) DO UPDATE SET expires_at_ms=excluded.expires_at_ms",
            params![artifact_id.to_string(), hold.kind.as_str(), hold.holder_ref, hold.created_at_ms, hold.expires_at_ms],
        )?;
        Ok(())
    }

    pub fn remove_artifact_hold(
        &self,
        artifact_id: &ArtifactId,
        owner_or_root: Principal,
        kind: ArtifactHoldKind,
        holder_ref: &str,
    ) -> StateResult<bool> {
        authorize_owner_or_root(&self.connection, artifact_id, owner_or_root)?;
        let deleted = self.connection.execute(
            "DELETE FROM artifact_holds WHERE artifact_id=?1 AND hold_kind=?2 AND holder_ref=?3",
            params![artifact_id.to_string(), kind.as_str(), holder_ref],
        )?;
        Ok(deleted > 0)
    }

    pub fn add_artifact_task_relationship(
        &self,
        artifact_id: &ArtifactId,
        owner_or_root: Principal,
        relationship: &ArtifactTaskRelationship,
        created_at_ms: i64,
    ) -> StateResult<()> {
        authorize_owner_or_root(&self.connection, artifact_id, owner_or_root)?;
        let artifact_owner: (u32, u32) = self.connection.query_row(
            "SELECT owner_uid, owner_gid FROM artifacts WHERE artifact_id=?1",
            params![artifact_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let task_owner: Option<(u32, u32)> = self
            .connection
            .query_row(
                "SELECT owner_uid, owner_gid FROM tasks WHERE task_id=?1",
                params![relationship.task_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if task_owner != Some(artifact_owner) {
            return invalid("task relationship must remain within the artifact owner principal");
        }
        self.connection.execute(
            "INSERT INTO artifact_task_relationships(artifact_id, task_id, relationship_kind, created_at_ms) VALUES (?1,?2,?3,?4) ON CONFLICT(artifact_id, task_id, relationship_kind) DO NOTHING",
            params![artifact_id.to_string(), relationship.task_id.to_string(), relationship.kind.as_str(), created_at_ms],
        )?;
        Ok(())
    }

    pub fn artifact_cleanup_eligibility(
        &self,
        artifact_id: &ArtifactId,
        now_ms: i64,
    ) -> StateResult<ArtifactCleanupEligibility> {
        let row: Option<(String, Option<i64>, String, String)> = self
            .connection
            .query_row(
                "SELECT retention_kind, expires_at_ms, cleanup_authority, availability_state FROM artifacts WHERE artifact_id=?1",
                params![artifact_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((retention, expires_at, cleanup_authority, availability)) = row else {
            return Err(StateError::InvalidArtifactState(
                "artifact does not exist".into(),
            ));
        };
        if cleanup_authority == "none" {
            return Ok(ArtifactCleanupEligibility::NoCleanupAuthority);
        }
        match retention.as_str() {
            "retained" => return Ok(ArtifactCleanupEligibility::Retained),
            "until" if expires_at.is_none_or(|expiry| expiry > now_ms) => {
                return Ok(ArtifactCleanupEligibility::NotExpired);
            }
            "temporary" | "until" => {}
            _ => {
                return Err(StateError::InvalidArtifactState(
                    "stored artifact retention is invalid".into(),
                ));
            }
        }
        if availability != "available" {
            return Ok(ArtifactCleanupEligibility::ContentNotAvailable);
        }
        let held: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM artifact_holds WHERE artifact_id=?1 AND (expires_at_ms IS NULL OR expires_at_ms>?2))",
            params![artifact_id.to_string(), now_ms],
            |row| row.get(0),
        )?;
        if held {
            return Ok(ArtifactCleanupEligibility::Held);
        }
        let active_task: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM artifact_task_relationships r JOIN tasks t ON t.task_id=r.task_id WHERE r.artifact_id=?1 AND t.state NOT IN ('succeeded','failed','cancelled','interrupted'))",
            params![artifact_id.to_string()],
            |row| row.get(0),
        )?;
        if active_task {
            return Ok(ArtifactCleanupEligibility::ActiveTaskRelationship);
        }
        Ok(ArtifactCleanupEligibility::Eligible)
    }

    pub fn mark_artifact_removed(
        &mut self,
        artifact_id: &ArtifactId,
        removed_at_ms: i64,
    ) -> StateResult<()> {
        let tx = self.connection.transaction()?;
        let owner: Option<(u32, u32)> = tx
            .query_row(
                "SELECT owner_uid, owner_gid FROM artifacts WHERE artifact_id=?1",
                params![artifact_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((uid, gid)) = owner else {
            return Err(StateError::InvalidArtifactState(
                "artifact does not exist".into(),
            ));
        };
        tx.execute(
            "UPDATE artifacts SET availability_state='removed', removed_at_ms=?2 WHERE artifact_id=?1",
            params![artifact_id.to_string(), removed_at_ms],
        )?;
        insert_artifact_event(
            &tx,
            *artifact_id,
            Principal::new(uid, gid),
            ArtifactEventSpec {
                event_kind: "artifact.content_removed",
                reason_code: "removed",
                safe_summary: Some("artifact content was intentionally removed"),
                safe_data: Value::Object(Default::default()),
                occurred_at_ms: removed_at_ms,
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn forget_artifact_metadata(
        &mut self,
        artifact_id: &ArtifactId,
        owner_or_root: Principal,
        forgotten_at_ms: i64,
    ) -> StateResult<()> {
        let tx = self.connection.transaction()?;
        let row: Option<(u32, u32, String, String, i64)> = tx
            .query_row(
                "SELECT owner_uid, owner_gid, artifact_type, confidentiality, registered_at_ms FROM artifacts WHERE artifact_id=?1",
                params![artifact_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?;
        let Some((uid, gid, artifact_type, confidentiality, registered_at_ms)) = row else {
            return Err(StateError::InvalidArtifactState(
                "artifact does not exist".into(),
            ));
        };
        if owner_or_root.uid() != 0 && (owner_or_root.uid(), owner_or_root.gid()) != (uid, gid) {
            return Err(StateError::InvalidArtifactState(
                "caller does not own artifact metadata".into(),
            ));
        }
        tx.execute(
            "INSERT INTO artifact_tombstones(artifact_id, owner_uid, owner_gid, artifact_type, confidentiality, registered_at_ms, tombstoned_at_ms, reason_code) VALUES (?1,?2,?3,?4,?5,?6,?7,'forgotten')",
            params![artifact_id.to_string(), uid, gid, artifact_type, confidentiality, registered_at_ms, forgotten_at_ms],
        )?;
        insert_artifact_event(
            &tx,
            *artifact_id,
            Principal::new(uid, gid),
            ArtifactEventSpec {
                event_kind: "artifact.forgotten",
                reason_code: "forgotten",
                safe_summary: Some(
                    "artifact registration metadata was forgotten without deleting content",
                ),
                safe_data: Value::Object(Default::default()),
                occurred_at_ms: forgotten_at_ms,
            },
        )?;
        tx.execute(
            "DELETE FROM artifacts WHERE artifact_id=?1",
            params![artifact_id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn artifact_tombstone_exists(&self, artifact_id: &ArtifactId) -> StateResult<bool> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM artifact_tombstones WHERE artifact_id=?1)",
                params![artifact_id.to_string()],
                |row| row.get(0),
            )
            .map_err(StateError::from)
    }
}

fn validate_registration_spec(spec: &ArtifactRegistrationSpec) -> StateResult<()> {
    if spec.created_at_ms < 0 || spec.registered_at_ms < 0 {
        return invalid("artifact timestamps must not be negative");
    }
    match spec.retention_kind {
        ArtifactRetentionKind::Until if spec.expires_at_ms.is_none() => {
            return invalid("until retention requires expires_at_ms");
        }
        ArtifactRetentionKind::Temporary | ArtifactRetentionKind::Retained
            if spec.expires_at_ms.is_some() =>
        {
            return invalid("only until retention may have expires_at_ms");
        }
        _ => {}
    }
    if spec.confidentiality != ArtifactConfidentiality::Shared && !spec.shared_with.is_empty() {
        return invalid("artifact grants require shared confidentiality");
    }
    if spec.shared_with.len() > MAX_ARTIFACT_SHARED_PRINCIPALS {
        return invalid("artifact share list exceeds bound");
    }
    for (index, principal) in spec.shared_with.iter().enumerate() {
        if spec.shared_with[index + 1..].contains(principal) {
            return invalid("artifact share list contains duplicate principal");
        }
    }
    let metadata = serde_json::to_string(&spec.safe_metadata).map_err(|_| {
        StateError::InvalidArtifactState("artifact metadata is not serializable".into())
    })?;
    if spec.safe_metadata.len() > MAX_ARTIFACT_METADATA_FIELDS
        || metadata.len() > MAX_ARTIFACT_METADATA_JSON_BYTES
    {
        return invalid("artifact metadata exceeds bounds");
    }
    for (key, value) in &spec.safe_metadata {
        if !safe_metadata_key(key)
            || value.len() > 512
            || value.contains(['\r', '\n'])
            || secret_like_text(value)
        {
            return invalid("artifact metadata contains unsafe, secret-like, or unbounded field");
        }
    }
    for value in [spec.project_ref.as_deref(), spec.cleanup_ref.as_deref()]
        .into_iter()
        .flatten()
    {
        if value.is_empty()
            || value.len() > MAX_ARTIFACT_TEXT_BYTES
            || value.contains(['\r', '\n'])
            || secret_like_text(value)
        {
            return invalid("artifact reference field is invalid or secret-like");
        }
    }
    if let Some(value) = spec.safe_display_name.as_deref() {
        if value.is_empty()
            || value.len() > MAX_ARTIFACT_DISPLAY_BYTES
            || value.contains(['\r', '\n'])
            || secret_like_text(value)
        {
            return invalid("artifact display name is invalid or secret-like");
        }
    }
    if let Some(value) = spec.media_type.as_deref() {
        if value.is_empty()
            || value.len() > MAX_ARTIFACT_MEDIA_BYTES
            || value.contains(['\r', '\n'])
        {
            return invalid("artifact media type is invalid");
        }
    }
    if let Some(sha256) = spec.sha256.as_deref() {
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return invalid("artifact SHA-256 is invalid");
        }
    }
    match (&spec.locator, spec.cleanup_authority) {
        (ArtifactLocator::Filesystem { .. }, ArtifactCleanupAuthority::Provider) => {
            return invalid("provider cleanup authority is invalid for filesystem content");
        }
        (
            ArtifactLocator::ProviderResource { .. },
            ArtifactCleanupAuthority::Portus | ArtifactCleanupAuthority::Task,
        ) => {
            return invalid("provider resources may only use provider cleanup authority");
        }
        _ => {}
    }
    if matches!(spec.locator, ArtifactLocator::Filesystem { .. })
        && (spec.integrity_kind != ArtifactIntegrityKind::Verified
            || spec.sha256.is_none()
            || spec.size_bytes.is_none())
    {
        return invalid("filesystem artifact requires verified digest and size at registration");
    }
    Ok(())
}

fn ensure_artifact_identity_unused(
    tx: &Transaction<'_>,
    artifact_id: &ArtifactId,
) -> StateResult<()> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM artifacts WHERE artifact_id=?1 UNION SELECT 1 FROM artifact_tombstones WHERE artifact_id=?1)",
        params![artifact_id.to_string()],
        |row| row.get(0),
    )?;
    if exists {
        invalid("artifact identity is already registered or tombstoned")
    } else {
        Ok(())
    }
}

fn validate_task_relationship(
    tx: &Transaction<'_>,
    spec: &ArtifactRegistrationSpec,
) -> StateResult<()> {
    let Some(task_id) = spec.source_task_id else {
        return Ok(());
    };
    let owner: Option<(u32, u32)> = tx
        .query_row(
            "SELECT owner_uid, owner_gid FROM tasks WHERE task_id=?1",
            params![task_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match owner {
        Some((uid, gid)) if (uid, gid) == (spec.owner.uid(), spec.owner.gid()) => Ok(()),
        Some(_) => invalid("source task owner does not match artifact owner"),
        None => invalid("source task does not exist"),
    }
}

fn validate_provider_relationship(
    tx: &Transaction<'_>,
    spec: &ArtifactRegistrationSpec,
) -> StateResult<()> {
    let ArtifactLocator::ProviderResource { reference } = &spec.locator else {
        return Ok(());
    };
    let active: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM provider_registrations WHERE provider_id=?1 AND removed_at_ms IS NULL)",
        params![reference.provider_registration_id.to_string()],
        |row| row.get(0),
    )?;
    if !active {
        return invalid("provider artifact requires an active provider registration");
    }
    let row: Option<(Option<u32>, Option<u32>, String)> = tx
        .query_row(
            "SELECT owner_uid, owner_gid, availability_state FROM provider_resource_refs WHERE provider_id=?1 AND resource_type=?2 AND resource_id=?3 AND generation=?4",
            params![
                reference.provider_registration_id.to_string(),
                reference.resource_type.to_string(),
                reference.resource_id.to_string(),
                reference.generation.as_deref().unwrap_or(""),
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    match row {
        Some((None, None, state)) if state != "stale" && state != "removed" => Ok(()),
        Some((Some(uid), Some(gid), state))
            if (uid, gid) == (spec.owner.uid(), spec.owner.gid())
                && state != "stale"
                && state != "removed" =>
        {
            Ok(())
        }
        Some(_) => invalid("provider resource is stale, removed, or owned by another principal"),
        None => invalid("provider resource reference is not registered"),
    }
}

fn locator_columns(locator: &ArtifactLocator) -> LocatorColumns<'_> {
    match locator {
        ArtifactLocator::Filesystem { path } => (Some(path), None, None, None, None),
        ArtifactLocator::ProviderResource { reference } => (
            None,
            Some(reference.provider_registration_id.to_string()),
            Some(reference.resource_type.to_string()),
            Some(reference.resource_id.to_string()),
            reference.generation.as_deref(),
        ),
    }
}

fn artifact_record_visible(
    connection: &rusqlite::Connection,
    artifact_id: &ArtifactId,
    principal: Principal,
) -> StateResult<Option<ArtifactRecord>> {
    let raw = connection
        .query_row(
            "SELECT artifact_id, owner_uid, owner_gid, artifact_type, confidentiality, retention_kind, expires_at_ms, availability_state, locator_kind, filesystem_path, provider_id, provider_resource_type, provider_resource_id, provider_generation, integrity_kind, sha256, size_bytes, media_type, created_at_ms, registered_at_ms, project_ref, safe_display_name, safe_metadata_json, last_verified_at_ms, removed_at_ms, cleanup_authority, cleanup_ref FROM artifacts a WHERE a.artifact_id=?1 AND (?2=0 OR (a.owner_uid=?2 AND a.owner_gid=?3) OR a.confidentiality='public' OR (a.confidentiality='shared' AND EXISTS(SELECT 1 FROM artifact_grants g WHERE g.artifact_id=a.artifact_id AND g.principal_uid=?2 AND g.principal_gid=?3)))",
            params![artifact_id.to_string(), principal.uid(), principal.gid()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?, row.get::<_, u32>(1)?, row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?,
                    row.get::<_, Option<i64>>(6)?, row.get::<_, String>(7)?, row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?, row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?, row.get::<_, Option<String>>(12)?,
                    row.get::<_, Option<String>>(13)?, row.get::<_, String>(14)?,
                    row.get::<_, Option<String>>(15)?, row.get::<_, Option<i64>>(16)?,
                    row.get::<_, Option<String>>(17)?, row.get::<_, i64>(18)?, row.get::<_, i64>(19)?,
                    row.get::<_, Option<String>>(20)?, row.get::<_, Option<String>>(21)?,
                    row.get::<_, String>(22)?, row.get::<_, Option<i64>>(23)?,
                    row.get::<_, Option<i64>>(24)?, row.get::<_, String>(25)?,
                    row.get::<_, Option<String>>(26)?,
                ))
            },
        )
        .optional()?;
    raw.map(record_from_raw).transpose()
}

type ArtifactRaw = (
    String,
    u32,
    u32,
    String,
    String,
    String,
    Option<i64>,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
    i64,
    i64,
    Option<String>,
    Option<String>,
    String,
    Option<i64>,
    Option<i64>,
    String,
    Option<String>,
);

fn record_from_raw(raw: ArtifactRaw) -> StateResult<ArtifactRecord> {
    let (
        artifact_id,
        uid,
        gid,
        artifact_type,
        confidentiality,
        retention_kind,
        expires_at_ms,
        availability_state,
        locator_kind,
        filesystem_path,
        provider_id,
        provider_resource_type,
        provider_resource_id,
        provider_generation,
        integrity_kind,
        sha256,
        size_bytes,
        media_type,
        created_at_ms,
        registered_at_ms,
        project_ref,
        safe_display_name,
        safe_metadata_json,
        last_verified_at_ms,
        removed_at_ms,
        cleanup_authority,
        cleanup_ref,
    ) = raw;
    let locator = match locator_kind.as_str() {
        "filesystem" => ArtifactLocator::Filesystem {
            path: filesystem_path.ok_or_else(|| {
                StateError::InvalidArtifactState("filesystem artifact has no path".into())
            })?,
        },
        "provider_resource" => ArtifactLocator::ProviderResource {
            reference: ProviderResourceRef {
                provider_registration_id: parse_id(
                    &provider_id.ok_or_else(|| {
                        StateError::InvalidArtifactState(
                            "provider artifact has no provider id".into(),
                        )
                    })?,
                    "provider id",
                )?,
                resource_type: ResourceType::new(provider_resource_type.ok_or_else(|| {
                    StateError::InvalidArtifactState(
                        "provider artifact has no resource type".into(),
                    )
                })?)
                .map_err(|_| StateError::InvalidArtifactState("invalid resource type".into()))?,
                resource_id: ProviderResourceId::new(provider_resource_id.ok_or_else(|| {
                    StateError::InvalidArtifactState("provider artifact has no resource id".into())
                })?)
                .map_err(|_| StateError::InvalidArtifactState("invalid resource id".into()))?,
                generation: provider_generation.filter(|value| !value.is_empty()),
            },
        },
        _ => return invalid("stored artifact locator kind is invalid"),
    };
    let safe_metadata: BTreeMap<String, String> = serde_json::from_str(&safe_metadata_json)
        .map_err(|_| {
            StateError::InvalidArtifactState("stored artifact metadata is invalid".into())
        })?;
    Ok(ArtifactRecord {
        artifact_id: parse_id(&artifact_id, "artifact id")?,
        owner: Principal::new(uid, gid),
        artifact_type: parse_enum(&artifact_type, "artifact type")?,
        confidentiality: parse_enum(&confidentiality, "confidentiality")?,
        retention_kind: parse_enum(&retention_kind, "retention")?,
        expires_at_ms,
        availability_state: parse_enum(&availability_state, "availability")?,
        locator,
        integrity_kind: parse_enum(&integrity_kind, "integrity")?,
        sha256,
        size_bytes: size_bytes.and_then(|value| u64::try_from(value).ok()),
        media_type,
        created_at_ms,
        registered_at_ms,
        project_ref,
        safe_display_name,
        safe_metadata,
        last_verified_at_ms,
        removed_at_ms,
        cleanup_authority: parse_enum(&cleanup_authority, "cleanup authority")?,
        cleanup_ref,
    })
}

fn summary_from_raw(raw: ArtifactSummaryRaw) -> StateResult<ArtifactSummary> {
    Ok(ArtifactSummary {
        artifact_id: parse_id(&raw.0, "artifact id")?,
        owner: Principal::new(raw.1, raw.2),
        artifact_type: parse_enum(&raw.3, "artifact type")?,
        confidentiality: parse_enum(&raw.4, "confidentiality")?,
        retention_kind: parse_enum(&raw.5, "retention")?,
        availability_state: parse_enum(&raw.6, "availability")?,
        integrity_kind: parse_enum(&raw.7, "integrity")?,
        size_bytes: raw.8.and_then(|value| u64::try_from(value).ok()),
        safe_display_name: raw.9,
        registered_at_ms: raw.10,
    })
}

fn artifact_task_relationships(
    connection: &rusqlite::Connection,
    artifact_id: &ArtifactId,
) -> StateResult<Vec<ArtifactTaskRelationship>> {
    let mut statement = connection.prepare(
        "SELECT task_id, relationship_kind FROM artifact_task_relationships WHERE artifact_id=?1 ORDER BY task_id, relationship_kind",
    )?;
    let raw = statement
        .query_map(params![artifact_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    raw.into_iter()
        .map(|(task_id, kind)| {
            Ok(ArtifactTaskRelationship {
                task_id: parse_id(&task_id, "task id")?,
                kind: parse_enum(&kind, "task relationship")?,
            })
        })
        .collect()
}

fn artifact_provider_resource(
    connection: &rusqlite::Connection,
    artifact_id: &ArtifactId,
) -> StateResult<Option<ProviderResourceRef>> {
    let raw = connection
        .query_row(
            "SELECT provider_id, resource_type, resource_id, generation FROM artifact_provider_relationships WHERE artifact_id=?1",
            params![artifact_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
        )
        .optional()?;
    raw.map(|(provider_id, resource_type, resource_id, generation)| {
        Ok(ProviderResourceRef {
            provider_registration_id: parse_id(&provider_id, "provider id")?,
            resource_type: ResourceType::new(resource_type)
                .map_err(|_| StateError::InvalidArtifactState("invalid resource type".into()))?,
            resource_id: ProviderResourceId::new(resource_id)
                .map_err(|_| StateError::InvalidArtifactState("invalid resource id".into()))?,
            generation: (!generation.is_empty()).then_some(generation),
        })
    })
    .transpose()
}

fn artifact_grants(
    connection: &rusqlite::Connection,
    artifact_id: &ArtifactId,
) -> StateResult<Vec<Principal>> {
    let mut statement = connection.prepare(
        "SELECT principal_uid, principal_gid FROM artifact_grants WHERE artifact_id=?1 ORDER BY principal_uid, principal_gid",
    )?;
    statement
        .query_map(params![artifact_id.to_string()], |row| {
            Ok(Principal::new(row.get(0)?, row.get(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StateError::from)
}

fn artifact_holds(
    connection: &rusqlite::Connection,
    artifact_id: &ArtifactId,
) -> StateResult<Vec<ArtifactHold>> {
    let mut statement = connection.prepare(
        "SELECT hold_kind, holder_ref, created_at_ms, expires_at_ms FROM artifact_holds WHERE artifact_id=?1 ORDER BY hold_kind, holder_ref",
    )?;
    let raw = statement
        .query_map(params![artifact_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    raw.into_iter()
        .map(|(kind, holder_ref, created_at_ms, expires_at_ms)| {
            Ok(ArtifactHold {
                kind: parse_enum(&kind, "hold kind")?,
                holder_ref,
                created_at_ms,
                expires_at_ms,
            })
        })
        .collect()
}

fn authorize_owner_or_root(
    connection: &rusqlite::Connection,
    artifact_id: &ArtifactId,
    principal: Principal,
) -> StateResult<()> {
    let owner: Option<(u32, u32)> = connection
        .query_row(
            "SELECT owner_uid, owner_gid FROM artifacts WHERE artifact_id=?1",
            params![artifact_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match owner {
        Some(_) if principal.uid() == 0 => Ok(()),
        Some((uid, gid)) if (uid, gid) == (principal.uid(), principal.gid()) => Ok(()),
        Some(_) => invalid("caller does not own artifact"),
        None => invalid("artifact does not exist"),
    }
}

fn insert_artifact_event(
    tx: &Transaction<'_>,
    artifact_id: ArtifactId,
    principal: Principal,
    event: ArtifactEventSpec<'_>,
) -> StateResult<()> {
    let object_ref = artifact_id.to_string();
    let next: i64 = tx.query_row(
        "SELECT COALESCE(MAX(object_sequence),0)+1 FROM significant_events WHERE object_kind='artifact' AND object_ref=?1",
        params![object_ref],
        |row| row.get(0),
    )?;
    let sequence = u64::try_from(next)
        .map_err(|_| StateError::InvalidArtifactState("artifact event sequence overflow".into()))?;
    insert_significant_event_tx(
        tx,
        &NewSignificantEvent {
            object_kind: EventObjectKind::Artifact,
            object_ref: artifact_id.to_string(),
            principal: Some(principal),
            event_kind: event.event_kind.into(),
            reason_code: Some(event.reason_code.into()),
            source_ref: Some("portus-state-artifact".into()),
            safe_summary: event.safe_summary.map(ToOwned::to_owned),
            safe_data: event.safe_data,
            occurred_at_ms: event.occurred_at_ms,
        },
        sequence,
    )?;
    prune_object_tx(
        tx,
        EventObjectKind::Artifact,
        &artifact_id.to_string(),
        sequence,
    )
}

fn validate_list_limit(limit: u16) -> StateResult<()> {
    if limit == 0 || limit > MAX_ARTIFACT_LIST_PAGE {
        invalid("artifact list limit must be between 1 and 200")
    } else {
        Ok(())
    }
}

fn parse_enum<T: FromStr>(value: &str, field: &str) -> StateResult<T> {
    value
        .parse()
        .map_err(|_| StateError::InvalidArtifactState(format!("stored {field} is invalid")))
}

fn parse_id<T: FromStr>(value: &str, field: &str) -> StateResult<T> {
    value
        .parse()
        .map_err(|_| StateError::InvalidArtifactState(format!("stored {field} is invalid")))
}

fn safe_metadata_key(key: &str) -> bool {
    if key.is_empty() || key.len() > 64 || key.contains(['\r', '\n']) {
        return false;
    }
    !secret_like_key(key)
}

fn invalid<T>(message: &str) -> StateResult<T> {
    Err(StateError::InvalidArtifactState(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_protocol::{ArtifactType, TaskId};
    use std::{fs, path::PathBuf};

    struct TestDb {
        dir: PathBuf,
        path: PathBuf,
    }

    impl TestDb {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "portus-state-artifact-{label}-{}",
                ArtifactId::new()
            ));
            fs::create_dir_all(&dir).unwrap();
            Self {
                path: dir.join("portus.db"),
                dir,
            }
        }
    }

    impl Drop for TestDb {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn filesystem_spec(owner: Principal, path: &str) -> ArtifactRegistrationSpec {
        ArtifactRegistrationSpec {
            artifact_id: ArtifactId::new(),
            owner,
            artifact_type: ArtifactType::Report,
            confidentiality: ArtifactConfidentiality::Private,
            retention_kind: ArtifactRetentionKind::Retained,
            expires_at_ms: None,
            locator: ArtifactLocator::Filesystem { path: path.into() },
            integrity_kind: ArtifactIntegrityKind::Verified,
            sha256: Some("0".repeat(64)),
            size_bytes: Some(10),
            media_type: Some("application/pdf".into()),
            created_at_ms: 10,
            registered_at_ms: 20,
            project_ref: None,
            safe_display_name: Some("report.pdf".into()),
            safe_metadata: BTreeMap::new(),
            source_task_id: None,
            shared_with: Vec::new(),
            cleanup_authority: ArtifactCleanupAuthority::None,
            cleanup_ref: None,
        }
    }

    #[test]
    fn private_artifact_is_principal_filtered_and_id_is_immutable() {
        let test = TestDb::new("private");
        let mut state = PortusState::open(&test.path).unwrap();
        let owner = Principal::new(1000, 1000);
        let spec = filesystem_spec(owner, "/tmp/report.pdf");
        let artifact_id = spec.artifact_id;
        state.register_artifact(&spec).unwrap();
        assert!(
            state
                .artifact_view_visible(&artifact_id, owner)
                .unwrap()
                .is_some()
        );
        assert!(
            state
                .artifact_view_visible(&artifact_id, Principal::new(1001, 1001))
                .unwrap()
                .is_none()
        );
        assert!(
            state
                .artifact_view_visible(&artifact_id, Principal::new(0, 0))
                .unwrap()
                .is_some()
        );
        assert!(state.register_artifact(&spec).is_err());
    }

    #[test]
    fn shared_requires_explicit_grant_while_public_is_visible() {
        let test = TestDb::new("sharing");
        let mut state = PortusState::open(&test.path).unwrap();
        let owner = Principal::new(1000, 1000);
        let other = Principal::new(1001, 1001);
        let mut shared = filesystem_spec(owner, "/tmp/shared.pdf");
        shared.confidentiality = ArtifactConfidentiality::Shared;
        shared.shared_with = vec![other];
        let shared_id = shared.artifact_id;
        state.register_artifact(&shared).unwrap();
        assert!(
            state
                .artifact_view_visible(&shared_id, other)
                .unwrap()
                .is_some()
        );
        assert!(
            state
                .artifact_view_visible(&shared_id, Principal::new(1002, 1002))
                .unwrap()
                .is_none()
        );

        let mut public = filesystem_spec(owner, "/tmp/public.pdf");
        public.confidentiality = ArtifactConfidentiality::Public;
        let public_id = public.artifact_id;
        state.register_artifact(&public).unwrap();
        assert!(
            state
                .artifact_view_visible(&public_id, Principal::new(1002, 1002))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn secret_like_metadata_is_rejected_at_state_boundary() {
        let test = TestDb::new("metadata");
        let mut state = PortusState::open(&test.path).unwrap();
        let mut spec = filesystem_spec(Principal::new(1000, 1000), "/tmp/report.pdf");
        spec.safe_metadata
            .insert("access_token".into(), "secret".into());
        assert!(matches!(
            state.register_artifact(&spec),
            Err(StateError::InvalidArtifactState(_))
        ));
        spec.safe_metadata.clear();
        spec.safe_metadata
            .insert("note".into(), "Authorization: Bearer do-not-store".into());
        assert!(matches!(
            state.register_artifact(&spec),
            Err(StateError::InvalidArtifactState(_))
        ));
    }

    #[test]
    fn retention_holds_and_active_task_relationships_block_cleanup() {
        let test = TestDb::new("cleanup");
        let mut state = PortusState::open(&test.path).unwrap();
        let owner = Principal::new(1000, 1000);
        let task_id = TaskId::new();
        state
            .insert_task_fixture(&task_id, owner, "produce report", "running", 1)
            .unwrap();
        let mut spec = filesystem_spec(owner, "/tmp/temporary.pdf");
        spec.retention_kind = ArtifactRetentionKind::Temporary;
        spec.cleanup_authority = ArtifactCleanupAuthority::Task;
        spec.cleanup_ref = Some(task_id.to_string());
        spec.source_task_id = Some(task_id);
        let id = spec.artifact_id;
        state.register_artifact(&spec).unwrap();
        assert_eq!(
            state.artifact_cleanup_eligibility(&id, 20).unwrap(),
            ArtifactCleanupEligibility::ActiveTaskRelationship
        );

        state
            .connection
            .execute(
                "UPDATE tasks SET state='succeeded' WHERE task_id=?1",
                params![task_id.to_string()],
            )
            .unwrap();
        state
            .add_artifact_hold(
                &id,
                owner,
                &ArtifactHold {
                    kind: ArtifactHoldKind::Explicit,
                    holder_ref: "owner-hold".into(),
                    created_at_ms: 21,
                    expires_at_ms: None,
                },
            )
            .unwrap();
        assert_eq!(
            state.artifact_cleanup_eligibility(&id, 22).unwrap(),
            ArtifactCleanupEligibility::Held
        );
        assert!(
            state
                .remove_artifact_hold(&id, owner, ArtifactHoldKind::Explicit, "owner-hold")
                .unwrap()
        );
        assert_eq!(
            state.artifact_cleanup_eligibility(&id, 23).unwrap(),
            ArtifactCleanupEligibility::Eligible
        );

        let consumer_task = TaskId::new();
        state
            .insert_task_fixture(&consumer_task, owner, "consume report", "waiting", 24)
            .unwrap();
        state
            .add_artifact_task_relationship(
                &id,
                owner,
                &ArtifactTaskRelationship {
                    task_id: consumer_task,
                    kind: portus_protocol::ArtifactTaskRelationshipKind::RequiredBy,
                },
                24,
            )
            .unwrap();
        let view = state.artifact_view_visible(&id, owner).unwrap().unwrap();
        assert!(view.task_relationships.iter().any(|relationship| {
            relationship.task_id == consumer_task
                && relationship.kind == portus_protocol::ArtifactTaskRelationshipKind::RequiredBy
        }));
        assert_eq!(
            state.artifact_cleanup_eligibility(&id, 25).unwrap(),
            ArtifactCleanupEligibility::ActiveTaskRelationship
        );
    }

    #[test]
    fn forgetting_metadata_tombstones_without_touching_native_file() {
        let test = TestDb::new("forget");
        let file = test.dir.join("keep.txt");
        fs::write(&file, b"keep me").unwrap();
        let mut state = PortusState::open(&test.path).unwrap();
        let owner = Principal::new(1000, 1000);
        let spec = filesystem_spec(owner, file.to_str().unwrap());
        let id = spec.artifact_id;
        state.register_artifact(&spec).unwrap();
        state.forget_artifact_metadata(&id, owner, 30).unwrap();
        assert!(file.exists());
        assert!(state.artifact_view_visible(&id, owner).unwrap().is_none());
        assert!(state.artifact_tombstone_exists(&id).unwrap());
    }
}
