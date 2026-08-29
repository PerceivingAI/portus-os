use crate::{PortusState, StateError, StateResult, safety::secret_like_text};
use portus_protocol::{
    Principal, ProviderRegistrationId, ProviderResourceId, ProviderResourceRef, ResourceType,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderInterfaceSpec {
    pub interface_id: String,
    pub interface_type: String,
    pub contract_version: u32,
    pub target: String,
    pub structured_output: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCapabilitySpec {
    pub capability_id: String,
    pub contract_version: u32,
    pub interface_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderResourceTypeSpec {
    pub resource_type: String,
    pub authority: String,
    pub lifetime: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRegistrationSpec {
    pub provider_type: String,
    pub display_label: String,
    pub scope: String,
    pub owner: Option<Principal>,
    pub manifest_id: String,
    pub manifest_version: u32,
    pub software_version: String,
    pub lifecycle_ownership: String,
    pub compatibility_state: String,
    pub health_state: String,
    pub health_reason: Option<String>,
    pub policy_domain_owner: String,
    pub interfaces: Vec<ProviderInterfaceSpec>,
    pub capabilities: Vec<ProviderCapabilitySpec>,
    pub resources: Vec<ProviderResourceTypeSpec>,
    pub skills: Vec<String>,
    pub health_integration_kind: String,
    pub health_reference: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderRegistrationRecord {
    pub provider_id: ProviderRegistrationId,
    pub provider_type: String,
    pub display_label: String,
    pub scope: String,
    pub owner: Option<Principal>,
    pub manifest_id: String,
    pub manifest_version: u32,
    pub software_version: String,
    pub lifecycle_ownership: String,
    pub compatibility_state: String,
    pub health_state: String,
    pub health_reason: Option<String>,
    pub policy_domain_owner: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub removed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderInterfaceView {
    pub interface_id: String,
    pub interface_type: String,
    pub contract_version: u32,
    pub target: String,
    pub structured_output: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderCapabilityView {
    pub capability_id: String,
    pub contract_version: u32,
    pub availability_state: String,
    pub reason_code: Option<String>,
    pub interfaces: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderResourceTypeView {
    pub resource_type: String,
    pub authority: String,
    pub lifetime: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderTombstoneView {
    pub removed_at_ms: i64,
    pub safe_reason: Option<String>,
    pub software_version: Option<String>,
    pub interface_version: Option<String>,
    pub successor_provider_id: Option<ProviderRegistrationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderView {
    #[serde(flatten)]
    pub registration: ProviderRegistrationRecord,
    pub interfaces: Vec<ProviderInterfaceView>,
    pub capabilities: Vec<ProviderCapabilityView>,
    pub resource_types: Vec<ProviderResourceTypeView>,
    pub skills: Vec<String>,
    pub health_integration_kind: String,
    pub health_reference: Option<String>,
    pub tombstone: Option<ProviderTombstoneView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityProviderView {
    pub provider_id: ProviderRegistrationId,
    pub provider_type: String,
    pub display_label: String,
    pub contract_version: u32,
    pub availability_state: String,
    pub reason_code: Option<String>,
    pub compatibility_state: String,
    pub health_state: String,
    pub interfaces: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityView {
    pub capability_id: String,
    pub providers: Vec<CapabilityProviderView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderReconcileResult {
    pub provider_id: ProviderRegistrationId,
    pub created: bool,
}

/// Bounded dynamic status for one registered provider capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCapabilityRuntimeSpec {
    pub capability_id: String,
    pub availability_state: String,
    pub reason_code: Option<String>,
}

/// Dynamic provider state produced by a typed integration adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRuntimeStatusSpec {
    pub compatibility_state: String,
    pub health_state: String,
    pub health_reason: Option<String>,
    pub capabilities: Vec<ProviderCapabilityRuntimeSpec>,
}

/// One currently observed provider-owned resource. Missing resources are made
/// stale by reconciliation rather than silently rebound to a replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderResourceRuntimeSpec {
    pub reference: ProviderResourceRef,
    pub availability_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderResourceView {
    pub reference: ProviderResourceRef,
    pub provider_type: String,
    pub provider_scope: String,
    pub owner: Option<Principal>,
    pub availability_state: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub const MAX_PROVIDER_RUNTIME_REASON_BYTES: usize = 256;
pub const MAX_PROVIDER_RESOURCE_VIEWS: usize = 1_024;

impl PortusState {
    pub fn reconcile_provider_registration(
        &mut self,
        spec: &ProviderRegistrationSpec,
        now_ms: i64,
    ) -> StateResult<ProviderReconcileResult> {
        validate_scope_owner(spec)?;
        let transaction = self.connection.transaction()?;
        let existing = active_provider_id(&transaction, spec)?;
        let (provider_id, created) = match existing {
            Some(id) => (id, false),
            None => (ProviderRegistrationId::new(), true),
        };

        if created {
            let interface_summary =
                version_summary(spec.interfaces.iter().map(|item| item.contract_version));
            let capability_summary =
                version_summary(spec.capabilities.iter().map(|item| item.contract_version));
            transaction.execute(
                "INSERT INTO provider_registrations(provider_id, owner_uid, owner_gid, provider_type, manifest_id, software_version, interface_version, contract_version, lifecycle_ownership, created_at_ms, display_label, scope, manifest_version, compatibility_state, health_state, health_reason, policy_domain_owner, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    provider_id.to_string(),
                    spec.owner.map(Principal::uid),
                    spec.owner.map(Principal::gid),
                    spec.provider_type,
                    spec.manifest_id,
                    spec.software_version,
                    interface_summary,
                    capability_summary,
                    spec.lifecycle_ownership,
                    now_ms,
                    spec.display_label,
                    spec.scope,
                    spec.manifest_version,
                    spec.compatibility_state,
                    spec.health_state,
                    spec.health_reason,
                    spec.policy_domain_owner,
                    now_ms,
                ],
            )?;
            transaction.execute(
                "UPDATE provider_tombstones SET successor_provider_id = ?1 WHERE provider_id = (SELECT provider_id FROM provider_tombstones WHERE provider_type = ?2 AND successor_provider_id IS NULL ORDER BY removed_at_ms DESC LIMIT 1)",
                params![provider_id.to_string(), spec.provider_type],
            )?;
        } else {
            let interface_summary =
                version_summary(spec.interfaces.iter().map(|item| item.contract_version));
            let capability_summary =
                version_summary(spec.capabilities.iter().map(|item| item.contract_version));
            transaction.execute(
                "UPDATE provider_registrations SET manifest_id=?2, software_version=?3, interface_version=?4, contract_version=?5, lifecycle_ownership=?6, display_label=?7, manifest_version=?8, compatibility_state=?9, health_state=?10, health_reason=?11, policy_domain_owner=?12, updated_at_ms=?13 WHERE provider_id=?1 AND removed_at_ms IS NULL",
                params![provider_id.to_string(), spec.manifest_id, spec.software_version, interface_summary, capability_summary, spec.lifecycle_ownership, spec.display_label, spec.manifest_version, spec.compatibility_state, spec.health_state, spec.health_reason, spec.policy_domain_owner, now_ms],
            )?;
        }

        replace_provider_details(&transaction, &provider_id, spec)?;
        transaction.commit()?;
        Ok(ProviderReconcileResult {
            provider_id,
            created,
        })
    }

    pub fn tombstone_missing_system_providers(
        &mut self,
        seen_manifest_ids: &[String],
        now_ms: i64,
    ) -> StateResult<Vec<ProviderRegistrationId>> {
        let transaction = self.connection.transaction()?;
        let mut statement = transaction.prepare(
            "SELECT provider_id, provider_type, manifest_id, software_version, interface_version FROM provider_registrations WHERE scope='system' AND removed_at_ms IS NULL ORDER BY provider_id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        let mut removed = Vec::new();
        for (provider_id_raw, provider_type, manifest_id, software_version, interface_version) in
            rows
        {
            if seen_manifest_ids.iter().any(|seen| seen == &manifest_id) {
                continue;
            }
            let provider_id = parse_provider_id(&provider_id_raw)?;
            transaction.execute(
                "UPDATE provider_registrations SET removed_at_ms=?2, health_state='unavailable', health_reason='manifest_removed', updated_at_ms=?2 WHERE provider_id=?1",
                params![provider_id_raw, now_ms],
            )?;
            transaction.execute(
                "INSERT INTO provider_tombstones(provider_id, provider_type, manifest_id, removed_at_ms, safe_reason, software_version, interface_version) VALUES (?1, ?2, ?3, ?4, 'manifest_removed', ?5, ?6) ON CONFLICT(provider_id) DO UPDATE SET removed_at_ms=excluded.removed_at_ms, safe_reason=excluded.safe_reason, software_version=excluded.software_version, interface_version=excluded.interface_version",
                params![provider_id.to_string(), provider_type, manifest_id, now_ms, software_version, interface_version],
            )?;
            transaction.execute(
                "UPDATE provider_resource_refs SET availability_state='stale', updated_at_ms=?2 WHERE provider_id=?1 AND availability_state != 'removed'",
                params![provider_id.to_string(), now_ms],
            )?;
            removed.push(provider_id);
        }
        transaction.commit()?;
        Ok(removed)
    }

    pub fn active_system_provider_count(&self) -> StateResult<usize> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM provider_registrations WHERE scope='system' AND removed_at_ms IS NULL",
            [],
            |row| row.get(0),
        )?;
        usize::try_from(count).map_err(|_| {
            StateError::InvalidProviderState("active provider count is out of range".into())
        })
    }

    pub fn provider_visible_by_id(
        &self,
        provider_id: &ProviderRegistrationId,
        principal: Principal,
    ) -> StateResult<Option<ProviderView>> {
        let registration = registration_visible(&self.connection, provider_id, principal)?;
        registration
            .map(|registration| provider_view(&self.connection, registration))
            .transpose()
    }

    pub fn list_providers_visible(
        &self,
        principal: Principal,
        limit: u16,
        after: Option<&ProviderRegistrationId>,
    ) -> StateResult<ProviderPage<ProviderRegistrationRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT provider_id, provider_type, display_label, scope, owner_uid, owner_gid, manifest_id, manifest_version, software_version, lifecycle_ownership, compatibility_state, health_state, health_reason, policy_domain_owner, created_at_ms, updated_at_ms, removed_at_ms FROM provider_registrations WHERE removed_at_ms IS NULL AND (scope='system' OR (owner_uid=?1 AND owner_gid=?2)) AND (?3 IS NULL OR provider_id > ?3) ORDER BY provider_id LIMIT ?4",
        )?;
        let after = after.map(ToString::to_string);
        let fetch_limit = i64::from(limit) + 1;
        let mut items = statement
            .query_map(
                params![principal.uid(), principal.gid(), after, fetch_limit],
                registration_from_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if items.len() > usize::from(limit) {
            items.truncate(usize::from(limit));
            items.last().map(|item| item.provider_id.to_string())
        } else {
            None
        };
        Ok(ProviderPage { items, next_cursor })
    }

    pub fn capability_visible_by_id(
        &self,
        capability_id: &str,
        principal: Principal,
    ) -> StateResult<Option<CapabilityView>> {
        let providers = capability_providers(&self.connection, capability_id, principal)?;
        if providers.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CapabilityView {
                capability_id: capability_id.to_string(),
                providers,
            }))
        }
    }

    pub fn list_capabilities_visible(
        &self,
        principal: Principal,
        limit: u16,
        after: Option<&str>,
    ) -> StateResult<ProviderPage<CapabilityView>> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT pc.capability_id FROM provider_capabilities pc JOIN provider_registrations pr ON pr.provider_id=pc.provider_id WHERE pr.removed_at_ms IS NULL AND (pr.scope='system' OR (pr.owner_uid=?1 AND pr.owner_gid=?2)) AND (?3 IS NULL OR pc.capability_id > ?3) ORDER BY pc.capability_id LIMIT ?4",
        )?;
        let fetch_limit = i64::from(limit) + 1;
        let ids = statement
            .query_map(
                params![principal.uid(), principal.gid(), after, fetch_limit],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = ids.len() > usize::from(limit);
        let ids = ids.into_iter().take(usize::from(limit)).collect::<Vec<_>>();
        let mut items = Vec::with_capacity(ids.len());
        for capability_id in ids {
            items.push(CapabilityView {
                providers: capability_providers(&self.connection, &capability_id, principal)?,
                capability_id,
            });
        }
        let next_cursor = has_more
            .then(|| items.last().map(|item| item.capability_id.clone()))
            .flatten();
        Ok(ProviderPage { items, next_cursor })
    }

    pub fn update_provider_runtime_status(
        &mut self,
        provider_id: &ProviderRegistrationId,
        spec: &ProviderRuntimeStatusSpec,
        now_ms: i64,
    ) -> StateResult<()> {
        validate_provider_runtime_status(spec)?;
        let tx = self.connection.transaction()?;
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM provider_registrations WHERE provider_id=?1 AND removed_at_ms IS NULL)",
            params![provider_id.to_string()],
            |row| row.get(0),
        )?;
        if !active {
            return Err(StateError::InvalidProviderState(
                "provider runtime status requires an active registration".into(),
            ));
        }
        tx.execute(
            "UPDATE provider_registrations SET compatibility_state=?2, health_state=?3, health_reason=?4, updated_at_ms=?5 WHERE provider_id=?1 AND removed_at_ms IS NULL",
            params![
                provider_id.to_string(),
                spec.compatibility_state,
                spec.health_state,
                spec.health_reason,
                now_ms,
            ],
        )?;
        let mut seen = HashSet::new();
        for capability in &spec.capabilities {
            if !seen.insert(capability.capability_id.as_str()) {
                return Err(StateError::InvalidProviderState(
                    "provider runtime status repeats a capability".into(),
                ));
            }
            let changed = tx.execute(
                "UPDATE provider_capabilities SET availability_state=?3, reason_code=?4 WHERE provider_id=?1 AND capability_id=?2",
                params![
                    provider_id.to_string(),
                    capability.capability_id,
                    capability.availability_state,
                    capability.reason_code,
                ],
            )?;
            if changed != 1 {
                return Err(StateError::InvalidProviderState(
                    "provider runtime status references an undeclared capability".into(),
                ));
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn reconcile_provider_resource_refs(
        &mut self,
        provider_id: &ProviderRegistrationId,
        resource_type: &str,
        owner: Option<Principal>,
        resources: &[ProviderResourceRuntimeSpec],
        now_ms: i64,
    ) -> StateResult<()> {
        if resource_type.is_empty() || resource_type.len() > 96 {
            return Err(StateError::InvalidProviderState(
                "provider resource type is invalid".into(),
            ));
        }
        let tx = self.connection.transaction()?;
        let declared: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM provider_resource_types prt JOIN provider_registrations pr ON pr.provider_id=prt.provider_id WHERE prt.provider_id=?1 AND prt.resource_type=?2 AND pr.removed_at_ms IS NULL)",
            params![provider_id.to_string(), resource_type],
            |row| row.get(0),
        )?;
        if !declared {
            return Err(StateError::InvalidProviderState(
                "provider resource reconciliation requires a declared active resource type".into(),
            ));
        }

        match owner {
            Some(owner) => {
                tx.execute(
                    "UPDATE provider_resource_refs SET availability_state='stale', updated_at_ms=?5 WHERE provider_id=?1 AND resource_type=?2 AND owner_uid=?3 AND owner_gid=?4 AND availability_state != 'removed'",
                    params![provider_id.to_string(), resource_type, owner.uid(), owner.gid(), now_ms],
                )?;
            }
            None => {
                tx.execute(
                    "UPDATE provider_resource_refs SET availability_state='stale', updated_at_ms=?3 WHERE provider_id=?1 AND resource_type=?2 AND owner_uid IS NULL AND owner_gid IS NULL AND availability_state != 'removed'",
                    params![provider_id.to_string(), resource_type, now_ms],
                )?;
            }
        }

        let mut seen = HashSet::new();
        for resource in resources {
            validate_runtime_resource(provider_id, resource_type, resource)?;
            let key = (
                resource.reference.resource_id.to_string(),
                resource.reference.generation.clone().unwrap_or_default(),
            );
            if !seen.insert(key) {
                return Err(StateError::InvalidProviderState(
                    "provider runtime resource set contains a duplicate identity".into(),
                ));
            }
            tx.execute(
                "INSERT INTO provider_resource_refs(provider_id, owner_uid, owner_gid, resource_type, resource_id, generation, availability_state, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) ON CONFLICT(provider_id, resource_type, resource_id, generation) DO UPDATE SET owner_uid=excluded.owner_uid, owner_gid=excluded.owner_gid, availability_state=excluded.availability_state, updated_at_ms=excluded.updated_at_ms",
                params![
                    provider_id.to_string(),
                    owner.map(Principal::uid),
                    owner.map(Principal::gid),
                    resource_type,
                    resource.reference.resource_id.to_string(),
                    resource.reference.generation.as_deref().unwrap_or(""),
                    resource.availability_state,
                    now_ms,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn record_provider_resource_ref(
        &self,
        reference: &ProviderResourceRef,
        owner: Option<Principal>,
        availability_state: &str,
        now_ms: i64,
    ) -> StateResult<()> {
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM provider_registrations WHERE provider_id=?1 AND removed_at_ms IS NULL)",
            params![reference.provider_registration_id.to_string()],
            |row| row.get(0),
        )?;
        if !active {
            return Err(StateError::InvalidProviderState(
                "provider registration is not active".into(),
            ));
        }
        self.connection.execute(
            "INSERT INTO provider_resource_refs(provider_id, owner_uid, owner_gid, resource_type, resource_id, generation, availability_state, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) ON CONFLICT(provider_id, resource_type, resource_id, generation) DO UPDATE SET availability_state=excluded.availability_state, updated_at_ms=excluded.updated_at_ms",
            params![
                reference.provider_registration_id.to_string(),
                owner.map(Principal::uid),
                owner.map(Principal::gid),
                reference.resource_type.to_string(),
                reference.resource_id.to_string(),
                reference.generation.as_deref().unwrap_or(""),
                availability_state,
                now_ms,
            ],
        )?;
        Ok(())
    }

    pub fn provider_resources_visible(
        &self,
        principal: Principal,
    ) -> StateResult<Vec<ProviderResourceView>> {
        let mut statement = self.connection.prepare(
            "SELECT rr.provider_id, pr.provider_type, pr.scope, rr.owner_uid, rr.owner_gid, rr.resource_type, rr.resource_id, rr.generation, rr.availability_state, rr.created_at_ms, rr.updated_at_ms FROM provider_resource_refs rr JOIN provider_registrations pr ON pr.provider_id=rr.provider_id WHERE pr.removed_at_ms IS NULL AND rr.availability_state != 'removed' AND (rr.owner_uid IS NULL OR (rr.owner_uid=?1 AND rr.owner_gid=?2) OR ?1=0) ORDER BY rr.provider_id, rr.resource_type, rr.resource_id, rr.generation LIMIT ?3",
        )?;
        let rows = statement
            .query_map(
                params![
                    principal.uid(),
                    principal.gid(),
                    (MAX_PROVIDER_RESOURCE_VIEWS + 1) as i64
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<u32>>(3)?,
                        row.get::<_, Option<u32>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() > MAX_PROVIDER_RESOURCE_VIEWS {
            return Err(StateError::InvalidProviderState(
                "provider resource view exceeds bounded limit".into(),
            ));
        }
        rows.into_iter()
            .map(
                |(
                    provider_id,
                    provider_type,
                    provider_scope,
                    uid,
                    gid,
                    resource_type,
                    resource_id,
                    generation,
                    availability_state,
                    created_at_ms,
                    updated_at_ms,
                )| {
                    let owner = match (uid, gid) {
                        (Some(uid), Some(gid)) => Some(Principal::new(uid, gid)),
                        (None, None) => None,
                        _ => {
                            return Err(StateError::InvalidProviderState(
                                "provider resource owner identity is inconsistent".into(),
                            ));
                        }
                    };
                    let provider_registration_id = parse_provider_id(&provider_id)?;
                    let resource_type = ResourceType::new(resource_type).map_err(|_| {
                        StateError::InvalidProviderState(
                            "stored provider resource type is invalid".into(),
                        )
                    })?;
                    let resource_id = ProviderResourceId::new(resource_id).map_err(|_| {
                        StateError::InvalidProviderState(
                            "stored provider resource id is invalid".into(),
                        )
                    })?;
                    let mut reference = ProviderResourceRef::new(
                        provider_registration_id,
                        resource_type,
                        resource_id,
                    );
                    if !generation.is_empty() {
                        reference = reference.with_generation(generation);
                    }
                    Ok(ProviderResourceView {
                        reference,
                        provider_type,
                        provider_scope,
                        owner,
                        availability_state,
                        created_at_ms,
                        updated_at_ms,
                    })
                },
            )
            .collect()
    }

    pub fn provider_resource_availability(
        &self,
        reference: &ProviderResourceRef,
    ) -> StateResult<Option<String>> {
        self.connection
            .query_row(
                "SELECT availability_state FROM provider_resource_refs WHERE provider_id=?1 AND resource_type=?2 AND resource_id=?3 AND generation=?4",
                params![reference.provider_registration_id.to_string(), reference.resource_type.to_string(), reference.resource_id.to_string(), reference.generation.as_deref().unwrap_or("")],
                |row| row.get(0),
            )
            .optional()
            .map_err(StateError::from)
    }
}

fn validate_provider_runtime_status(spec: &ProviderRuntimeStatusSpec) -> StateResult<()> {
    if !matches!(
        spec.compatibility_state.as_str(),
        "compatible" | "incompatible" | "unknown"
    ) {
        return Err(StateError::InvalidProviderState(
            "provider compatibility state is invalid".into(),
        ));
    }
    if !matches!(
        spec.health_state.as_str(),
        "healthy" | "degraded" | "unavailable" | "unknown"
    ) {
        return Err(StateError::InvalidProviderState(
            "provider health state is invalid".into(),
        ));
    }
    validate_runtime_reason(spec.health_reason.as_deref())?;
    for capability in &spec.capabilities {
        if capability.capability_id.is_empty() || capability.capability_id.len() > 128 {
            return Err(StateError::InvalidProviderState(
                "provider runtime capability id is invalid".into(),
            ));
        }
        if !matches!(
            capability.availability_state.as_str(),
            "available" | "degraded" | "unavailable" | "unknown"
        ) {
            return Err(StateError::InvalidProviderState(
                "provider capability availability is invalid".into(),
            ));
        }
        validate_runtime_reason(capability.reason_code.as_deref())?;
    }
    Ok(())
}

fn validate_runtime_resource(
    provider_id: &ProviderRegistrationId,
    resource_type: &str,
    resource: &ProviderResourceRuntimeSpec,
) -> StateResult<()> {
    if &resource.reference.provider_registration_id != provider_id
        || resource.reference.resource_type.as_str() != resource_type
        || !matches!(
            resource.availability_state.as_str(),
            "available" | "unavailable"
        )
    {
        return Err(StateError::InvalidProviderState(
            "provider runtime resource does not match reconciliation scope".into(),
        ));
    }
    Ok(())
}

fn validate_runtime_reason(value: Option<&str>) -> StateResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.is_empty()
        || value.len() > MAX_PROVIDER_RUNTIME_REASON_BYTES
        || value.contains(['\0', '\n', '\r'])
    {
        return Err(StateError::InvalidProviderState(
            "provider runtime reason is invalid".into(),
        ));
    }
    if secret_like_text(value) {
        return Err(StateError::InvalidProviderState(
            "provider runtime reason contains secret-like material".into(),
        ));
    }
    Ok(())
}

fn validate_scope_owner(spec: &ProviderRegistrationSpec) -> StateResult<()> {
    match (spec.scope.as_str(), spec.owner) {
        ("system", None) | ("user", Some(_)) => Ok(()),
        ("system", Some(_)) => Err(StateError::InvalidProviderState(
            "system provider cannot have an owner principal".into(),
        )),
        ("user", None) => Err(StateError::InvalidProviderState(
            "user provider requires an owner principal".into(),
        )),
        _ => Err(StateError::InvalidProviderState(
            "provider scope must be system or user".into(),
        )),
    }
}

fn active_provider_id(
    transaction: &Transaction<'_>,
    spec: &ProviderRegistrationSpec,
) -> StateResult<Option<ProviderRegistrationId>> {
    let raw = match spec.owner {
        None => transaction
            .query_row(
                "SELECT provider_id FROM provider_registrations WHERE provider_type=?1 AND scope='system' AND removed_at_ms IS NULL",
                params![spec.provider_type],
                |row| row.get::<_, String>(0),
            )
            .optional()?,
        Some(owner) => transaction
            .query_row(
                "SELECT provider_id FROM provider_registrations WHERE provider_type=?1 AND scope='user' AND owner_uid=?2 AND owner_gid=?3 AND removed_at_ms IS NULL",
                params![spec.provider_type, owner.uid(), owner.gid()],
                |row| row.get::<_, String>(0),
            )
            .optional()?,
    };
    raw.map(|value| parse_provider_id(&value)).transpose()
}

fn replace_provider_details(
    transaction: &Transaction<'_>,
    provider_id: &ProviderRegistrationId,
    spec: &ProviderRegistrationSpec,
) -> StateResult<()> {
    let id = provider_id.to_string();
    transaction.execute(
        "DELETE FROM provider_capability_interfaces WHERE provider_id=?1",
        params![id],
    )?;
    transaction.execute(
        "DELETE FROM provider_capabilities WHERE provider_id=?1",
        params![id],
    )?;
    transaction.execute(
        "DELETE FROM provider_interfaces WHERE provider_id=?1",
        params![id],
    )?;
    transaction.execute(
        "DELETE FROM provider_resource_types WHERE provider_id=?1",
        params![id],
    )?;
    transaction.execute(
        "DELETE FROM provider_skills WHERE provider_id=?1",
        params![id],
    )?;
    transaction.execute(
        "DELETE FROM provider_health_contracts WHERE provider_id=?1",
        params![id],
    )?;

    for interface in &spec.interfaces {
        transaction.execute(
            "INSERT INTO provider_interfaces(provider_id, interface_id, interface_type, contract_version, target, structured_output) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, interface.interface_id, interface.interface_type, interface.contract_version, interface.target, i64::from(interface.structured_output)],
        )?;
    }
    for capability in &spec.capabilities {
        transaction.execute(
            "INSERT INTO provider_capabilities(provider_id, capability_id, contract_version, availability_state) VALUES (?1, ?2, ?3, 'unknown')",
            params![id, capability.capability_id, capability.contract_version],
        )?;
        for interface_id in &capability.interface_ids {
            transaction.execute(
                "INSERT INTO provider_capability_interfaces(provider_id, capability_id, interface_id) VALUES (?1, ?2, ?3)",
                params![id, capability.capability_id, interface_id],
            )?;
        }
    }
    for resource in &spec.resources {
        transaction.execute(
            "INSERT INTO provider_resource_types(provider_id, resource_type, authority, lifetime) VALUES (?1, ?2, ?3, ?4)",
            params![id, resource.resource_type, resource.authority, resource.lifetime],
        )?;
    }
    for skill in &spec.skills {
        transaction.execute(
            "INSERT INTO provider_skills(provider_id, skill_id) VALUES (?1, ?2)",
            params![id, skill],
        )?;
    }
    transaction.execute(
        "INSERT INTO provider_health_contracts(provider_id, integration_kind, reference_id) VALUES (?1, ?2, ?3)",
        params![id, spec.health_integration_kind, spec.health_reference],
    )?;
    Ok(())
}

fn registration_visible(
    connection: &rusqlite::Connection,
    provider_id: &ProviderRegistrationId,
    principal: Principal,
) -> StateResult<Option<ProviderRegistrationRecord>> {
    connection
        .query_row(
            "SELECT provider_id, provider_type, display_label, scope, owner_uid, owner_gid, manifest_id, manifest_version, software_version, lifecycle_ownership, compatibility_state, health_state, health_reason, policy_domain_owner, created_at_ms, updated_at_ms, removed_at_ms FROM provider_registrations WHERE provider_id=?1 AND (scope='system' OR (owner_uid=?2 AND owner_gid=?3))",
            params![provider_id.to_string(), principal.uid(), principal.gid()],
            registration_from_row,
        )
        .optional()
        .map_err(StateError::from)
}

fn registration_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderRegistrationRecord> {
    let provider_id_raw: String = row.get(0)?;
    let provider_id = provider_id_raw
        .parse::<ProviderRegistrationId>()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let owner_uid: Option<u32> = row.get(4)?;
    let owner_gid: Option<u32> = row.get(5)?;
    Ok(ProviderRegistrationRecord {
        provider_id,
        provider_type: row.get(1)?,
        display_label: row.get(2)?,
        scope: row.get(3)?,
        owner: owner_uid
            .zip(owner_gid)
            .map(|(uid, gid)| Principal::new(uid, gid)),
        manifest_id: row.get(6)?,
        manifest_version: row.get(7)?,
        software_version: row.get(8)?,
        lifecycle_ownership: row.get(9)?,
        compatibility_state: row.get(10)?,
        health_state: row.get(11)?,
        health_reason: row.get(12)?,
        policy_domain_owner: row.get(13)?,
        created_at_ms: row.get(14)?,
        updated_at_ms: row.get(15)?,
        removed_at_ms: row.get(16)?,
    })
}

fn provider_view(
    connection: &rusqlite::Connection,
    registration: ProviderRegistrationRecord,
) -> StateResult<ProviderView> {
    let provider_id = registration.provider_id.to_string();
    let interfaces = query_interfaces(connection, &provider_id)?;
    let capabilities = query_capabilities(connection, &provider_id)?;
    let resource_types = query_resource_types(connection, &provider_id)?;
    let skills = query_strings(
        connection,
        "SELECT skill_id FROM provider_skills WHERE provider_id=?1 ORDER BY skill_id",
        &provider_id,
    )?;
    let (health_integration_kind, health_reference) = connection.query_row(
        "SELECT integration_kind, reference_id FROM provider_health_contracts WHERE provider_id=?1",
        params![provider_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let tombstone_raw = connection
        .query_row(
            "SELECT removed_at_ms, safe_reason, software_version, interface_version, successor_provider_id FROM provider_tombstones WHERE provider_id=?1",
            params![provider_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()?;
    let tombstone = tombstone_raw
        .map(
            |(removed_at_ms, safe_reason, software_version, interface_version, successor_raw)| {
                let successor_provider_id = successor_raw
                    .map(|value| parse_provider_id(&value))
                    .transpose()?;
                Ok::<ProviderTombstoneView, StateError>(ProviderTombstoneView {
                    removed_at_ms,
                    safe_reason,
                    software_version,
                    interface_version,
                    successor_provider_id,
                })
            },
        )
        .transpose()?;
    Ok(ProviderView {
        registration,
        interfaces,
        capabilities,
        resource_types,
        skills,
        health_integration_kind,
        health_reference,
        tombstone,
    })
}

fn query_interfaces(
    connection: &rusqlite::Connection,
    provider_id: &str,
) -> StateResult<Vec<ProviderInterfaceView>> {
    let mut statement = connection.prepare("SELECT interface_id, interface_type, contract_version, target, structured_output FROM provider_interfaces WHERE provider_id=?1 ORDER BY interface_id")?;
    statement
        .query_map(params![provider_id], |row| {
            Ok(ProviderInterfaceView {
                interface_id: row.get(0)?,
                interface_type: row.get(1)?,
                contract_version: row.get(2)?,
                target: row.get(3)?,
                structured_output: row.get::<_, i64>(4)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StateError::from)
}

fn query_capabilities(
    connection: &rusqlite::Connection,
    provider_id: &str,
) -> StateResult<Vec<ProviderCapabilityView>> {
    let mut statement = connection.prepare("SELECT capability_id, contract_version, availability_state, reason_code FROM provider_capabilities WHERE provider_id=?1 ORDER BY capability_id")?;
    let base = statement
        .query_map(params![provider_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    base.into_iter()
        .map(|(capability_id, contract_version, availability_state, reason_code)| {
            let interfaces = query_strings_two(
                connection,
                "SELECT interface_id FROM provider_capability_interfaces WHERE provider_id=?1 AND capability_id=?2 ORDER BY interface_id",
                provider_id,
                &capability_id,
            )?;
            Ok(ProviderCapabilityView {
                capability_id,
                contract_version,
                availability_state,
                reason_code,
                interfaces,
            })
        })
        .collect()
}

fn query_resource_types(
    connection: &rusqlite::Connection,
    provider_id: &str,
) -> StateResult<Vec<ProviderResourceTypeView>> {
    let mut statement = connection.prepare("SELECT resource_type, authority, lifetime FROM provider_resource_types WHERE provider_id=?1 ORDER BY resource_type")?;
    statement
        .query_map(params![provider_id], |row| {
            Ok(ProviderResourceTypeView {
                resource_type: row.get(0)?,
                authority: row.get(1)?,
                lifetime: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StateError::from)
}

fn capability_providers(
    connection: &rusqlite::Connection,
    capability_id: &str,
    principal: Principal,
) -> StateResult<Vec<CapabilityProviderView>> {
    let mut statement = connection.prepare(
        "SELECT pr.provider_id, pr.provider_type, pr.display_label, pc.contract_version, pc.availability_state, pc.reason_code, pr.compatibility_state, pr.health_state FROM provider_capabilities pc JOIN provider_registrations pr ON pr.provider_id=pc.provider_id WHERE pc.capability_id=?1 AND pr.removed_at_ms IS NULL AND (pr.scope='system' OR (pr.owner_uid=?2 AND pr.owner_gid=?3)) ORDER BY pr.provider_id",
    )?;
    let base = statement
        .query_map(
            params![capability_id, principal.uid(), principal.gid()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    base.into_iter()
        .map(|(provider_id_raw, provider_type, display_label, contract_version, availability_state, reason_code, compatibility_state, health_state)| {
            let provider_id = parse_provider_id(&provider_id_raw)?;
            let interfaces = query_strings_two(
                connection,
                "SELECT interface_id FROM provider_capability_interfaces WHERE provider_id=?1 AND capability_id=?2 ORDER BY interface_id",
                &provider_id_raw,
                capability_id,
            )?;
            Ok(CapabilityProviderView {
                provider_id,
                provider_type,
                display_label,
                contract_version,
                availability_state,
                reason_code,
                compatibility_state,
                health_state,
                interfaces,
            })
        })
        .collect()
}

fn query_strings(
    connection: &rusqlite::Connection,
    sql: &str,
    first: &str,
) -> StateResult<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    statement
        .query_map(params![first], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StateError::from)
}

fn query_strings_two(
    connection: &rusqlite::Connection,
    sql: &str,
    first: &str,
    second: &str,
) -> StateResult<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    statement
        .query_map(params![first, second], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StateError::from)
}

fn version_summary(versions: impl Iterator<Item = u32>) -> String {
    let mut versions = versions.collect::<Vec<_>>();
    versions.sort_unstable();
    versions.dedup();
    match versions.as_slice() {
        [] => "none".into(),
        [single] => single.to_string(),
        _ => "mixed".into(),
    }
}

fn parse_provider_id(value: &str) -> StateResult<ProviderRegistrationId> {
    value.parse().map_err(|error| {
        StateError::InvalidProviderState(format!(
            "invalid provider registration id in state: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_protocol::{ProviderResourceId, ResourceType, TaskId};
    use std::{fs, path::PathBuf};

    struct TestDb {
        dir: PathBuf,
        path: PathBuf,
    }

    impl TestDb {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("portus-provider-runtime-{name}-{}", TaskId::new()));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("portus.db");
            Self { dir, path }
        }
    }

    impl Drop for TestDb {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn provider_spec() -> ProviderRegistrationSpec {
        ProviderRegistrationSpec {
            provider_type: "runtime-fixture".into(),
            display_label: "Runtime Fixture".into(),
            scope: "system".into(),
            owner: None,
            manifest_id: "runtime-fixture.toml".into(),
            manifest_version: 1,
            software_version: "1.0.0".into(),
            lifecycle_ownership: "provider-owned".into(),
            compatibility_state: "unknown".into(),
            health_state: "unknown".into(),
            health_reason: Some("not_probed".into()),
            policy_domain_owner: "provider".into(),
            interfaces: vec![ProviderInterfaceSpec {
                interface_id: "cli".into(),
                interface_type: "executable".into(),
                contract_version: 1,
                target: "/usr/bin/runtime-fixture".into(),
                structured_output: true,
            }],
            capabilities: vec![ProviderCapabilitySpec {
                capability_id: "fixture.control".into(),
                contract_version: 1,
                interface_ids: vec!["cli".into()],
            }],
            resources: vec![ProviderResourceTypeSpec {
                resource_type: "fixture-session".into(),
                authority: "provider".into(),
                lifetime: "session".into(),
            }],
            skills: Vec::new(),
            health_integration_kind: "structured-cli".into(),
            health_reference: Some("cli".into()),
        }
    }

    fn resource(
        provider_id: ProviderRegistrationId,
        id: &str,
        generation: &str,
    ) -> ProviderResourceRuntimeSpec {
        ProviderResourceRuntimeSpec {
            reference: ProviderResourceRef::new(
                provider_id,
                ResourceType::new("fixture-session").unwrap(),
                ProviderResourceId::new(id).unwrap(),
            )
            .with_generation(generation),
            availability_state: "available".into(),
        }
    }

    #[test]
    fn runtime_status_rejects_undeclared_capability_atomically() {
        let test = TestDb::new("capability");
        let mut state = PortusState::open(&test.path).unwrap();
        let registration = state
            .reconcile_provider_registration(&provider_spec(), 1)
            .unwrap();
        let result = state.update_provider_runtime_status(
            &registration.provider_id,
            &ProviderRuntimeStatusSpec {
                compatibility_state: "compatible".into(),
                health_state: "healthy".into(),
                health_reason: Some("ready".into()),
                capabilities: vec![ProviderCapabilityRuntimeSpec {
                    capability_id: "fixture.undeclared".into(),
                    availability_state: "available".into(),
                    reason_code: None,
                }],
            },
            10,
        );
        assert!(result.is_err());
        let view = state
            .provider_visible_by_id(&registration.provider_id, Principal::new(1000, 1000))
            .unwrap()
            .unwrap();
        assert_eq!(view.registration.compatibility_state, "unknown");
        assert_eq!(view.registration.health_state, "unknown");
        assert_eq!(view.capabilities[0].availability_state, "unknown");
    }

    #[test]
    fn runtime_status_rejects_secret_like_reason_without_mutation() {
        let test = TestDb::new("reason");
        let mut state = PortusState::open(&test.path).unwrap();
        let registration = state
            .reconcile_provider_registration(&provider_spec(), 1)
            .unwrap();
        assert!(
            state
                .update_provider_runtime_status(
                    &registration.provider_id,
                    &ProviderRuntimeStatusSpec {
                        compatibility_state: "compatible".into(),
                        health_state: "degraded".into(),
                        health_reason: Some("token=do-not-store".into()),
                        capabilities: Vec::new(),
                    },
                    10,
                )
                .is_err()
        );
        let view = state
            .provider_visible_by_id(&registration.provider_id, Principal::new(1000, 1000))
            .unwrap()
            .unwrap();
        assert_eq!(view.registration.health_state, "unknown");
        assert_eq!(
            view.registration.health_reason.as_deref(),
            Some("not_probed")
        );
    }

    #[test]
    fn resource_reconcile_stales_disappeared_generation_without_rebinding() {
        let test = TestDb::new("resources");
        let mut state = PortusState::open(&test.path).unwrap();
        let registration = state
            .reconcile_provider_registration(&provider_spec(), 1)
            .unwrap();
        let owner = Principal::new(1000, 1000);
        let first = resource(registration.provider_id, "session-a", "generation-a");
        state
            .reconcile_provider_resource_refs(
                &registration.provider_id,
                "fixture-session",
                Some(owner),
                std::slice::from_ref(&first),
                10,
            )
            .unwrap();
        assert_eq!(
            state
                .provider_resource_availability(&first.reference)
                .unwrap()
                .as_deref(),
            Some("available")
        );

        let replacement = resource(registration.provider_id, "session-a", "generation-b");
        state
            .reconcile_provider_resource_refs(
                &registration.provider_id,
                "fixture-session",
                Some(owner),
                std::slice::from_ref(&replacement),
                20,
            )
            .unwrap();
        assert_eq!(
            state
                .provider_resource_availability(&first.reference)
                .unwrap()
                .as_deref(),
            Some("stale")
        );
        assert_eq!(
            state
                .provider_resource_availability(&replacement.reference)
                .unwrap()
                .as_deref(),
            Some("available")
        );
    }
}
