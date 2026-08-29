pub(crate) const MIGRATION_1: &str = r#"
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY CHECK (version > 0),
    applied_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE runtime_metadata (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE projects (
    project_ref TEXT PRIMARY KEY,
    owner_uid INTEGER NOT NULL CHECK (owner_uid >= 0),
    owner_gid INTEGER NOT NULL CHECK (owner_gid >= 0),
    workspace_path TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_projects_owner ON projects(owner_uid, owner_gid);

CREATE TABLE session_refs (
    session_ref TEXT PRIMARY KEY,
    owner_uid INTEGER NOT NULL CHECK (owner_uid >= 0),
    owner_gid INTEGER NOT NULL CHECK (owner_gid >= 0),
    project_ref TEXT REFERENCES projects(project_ref) ON DELETE SET NULL,
    session_kind TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_session_refs_owner ON session_refs(owner_uid, owner_gid);

CREATE TABLE tasks (
    task_id TEXT PRIMARY KEY CHECK (task_id GLOB 'task_*'),
    owner_uid INTEGER NOT NULL CHECK (owner_uid >= 0),
    owner_gid INTEGER NOT NULL CHECK (owner_gid >= 0),
    objective_summary TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('created', 'queued', 'starting', 'running', 'waiting', 'paused', 'reconciling', 'cancelling', 'succeeded', 'failed', 'cancelled', 'interrupted')),
    project_ref TEXT REFERENCES projects(project_ref) ON DELETE SET NULL,
    session_ref TEXT REFERENCES session_refs(session_ref) ON DELETE SET NULL,
    parent_task_id TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
    retry_of_task_id TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
    created_at_ms INTEGER NOT NULL,
    started_at_ms INTEGER,
    finished_at_ms INTEGER
) STRICT;
CREATE INDEX idx_tasks_owner_state ON tasks(owner_uid, owner_gid, state);
CREATE INDEX idx_tasks_project ON tasks(project_ref);

CREATE TABLE task_attempts (
    attempt_id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    backend_kind TEXT NOT NULL,
    backend_ref TEXT,
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    outcome TEXT,
    UNIQUE(task_id, attempt_number)
) STRICT;

CREATE TABLE task_events (
    event_id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    event_kind TEXT NOT NULL,
    safe_summary TEXT,
    occurred_at_ms INTEGER NOT NULL,
    UNIQUE(task_id, sequence)
) STRICT;
CREATE INDEX idx_task_events_task_sequence ON task_events(task_id, sequence);

CREATE TABLE provider_registrations (
    provider_id TEXT PRIMARY KEY CHECK (provider_id GLOB 'provider_*'),
    owner_uid INTEGER,
    owner_gid INTEGER,
    provider_type TEXT NOT NULL,
    manifest_id TEXT NOT NULL,
    software_version TEXT NOT NULL,
    interface_version TEXT NOT NULL,
    contract_version TEXT NOT NULL,
    lifecycle_ownership TEXT NOT NULL CHECK (lifecycle_ownership IN ('portus-supervised', 'provider-owned', 'user-owned', 'external')),
    created_at_ms INTEGER NOT NULL,
    removed_at_ms INTEGER,
    CHECK ((owner_uid IS NULL AND owner_gid IS NULL) OR (owner_uid >= 0 AND owner_gid >= 0))
) STRICT;
CREATE INDEX idx_provider_registrations_owner ON provider_registrations(owner_uid, owner_gid);

CREATE TABLE provider_tombstones (
    provider_id TEXT PRIMARY KEY,
    provider_type TEXT NOT NULL,
    manifest_id TEXT NOT NULL,
    removed_at_ms INTEGER NOT NULL,
    safe_reason TEXT
) STRICT;

CREATE TABLE artifacts (
    artifact_id TEXT PRIMARY KEY CHECK (artifact_id GLOB 'artifact_*'),
    owner_uid INTEGER NOT NULL CHECK (owner_uid >= 0),
    owner_gid INTEGER NOT NULL CHECK (owner_gid >= 0),
    task_id TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
    artifact_type TEXT NOT NULL CHECK (artifact_type IN ('file', 'report', 'release', 'diagnostic_bundle', 'screenshot', 'archive', 'other')),
    confidentiality TEXT NOT NULL CHECK (confidentiality IN ('private', 'shared', 'public')),
    retention_kind TEXT NOT NULL CHECK (retention_kind IN ('temporary', 'retained', 'until')),
    expires_at_ms INTEGER,
    availability_state TEXT NOT NULL CHECK (availability_state IN ('available', 'missing', 'unavailable', 'removed')),
    locator_kind TEXT NOT NULL CHECK (locator_kind IN ('filesystem', 'provider_resource')),
    locator TEXT NOT NULL,
    integrity_kind TEXT NOT NULL CHECK (integrity_kind IN ('verified', 'mismatch', 'provider_authoritative', 'unverified', 'not_applicable')),
    media_type TEXT,
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    sha256 TEXT,
    created_at_ms INTEGER NOT NULL,
    CHECK (retention_kind != 'until' OR expires_at_ms IS NOT NULL)
) STRICT;
CREATE INDEX idx_artifacts_owner ON artifacts(owner_uid, owner_gid);
CREATE INDEX idx_artifacts_task ON artifacts(task_id);

CREATE TABLE approvals (
    approval_id TEXT PRIMARY KEY,
    owner_uid INTEGER NOT NULL CHECK (owner_uid >= 0),
    owner_gid INTEGER NOT NULL CHECK (owner_gid >= 0),
    task_id TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
    action_id TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    resolved_at_ms INTEGER
) STRICT;
CREATE INDEX idx_approvals_owner_state ON approvals(owner_uid, owner_gid, state);

CREATE TABLE policy_relationships (
    relationship_id INTEGER PRIMARY KEY,
    owner_uid INTEGER NOT NULL CHECK (owner_uid >= 0),
    owner_gid INTEGER NOT NULL CHECK (owner_gid >= 0),
    subject_ref TEXT NOT NULL,
    action_id TEXT NOT NULL,
    policy_ref TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_policy_relationships_owner ON policy_relationships(owner_uid, owner_gid);

CREATE TABLE index_observations (
    index_handle TEXT PRIMARY KEY CHECK (index_handle GLOB 'idx_*'),
    owner_uid INTEGER,
    owner_gid INTEGER,
    resource_type TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_generation TEXT,
    authoritative_ref TEXT,
    freshness TEXT NOT NULL CHECK (freshness IN ('live', 'recent', 'stale', 'unavailable', 'historical')),
    observed_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER,
    safe_metadata_json TEXT NOT NULL DEFAULT '{}',
    CHECK ((owner_uid IS NULL AND owner_gid IS NULL) OR (owner_uid >= 0 AND owner_gid >= 0))
) STRICT;
CREATE INDEX idx_index_observations_type_freshness ON index_observations(resource_type, freshness);
CREATE INDEX idx_index_observations_owner ON index_observations(owner_uid, owner_gid);

CREATE TABLE index_relations (
    relation_id INTEGER PRIMARY KEY,
    from_handle TEXT NOT NULL REFERENCES index_observations(index_handle) ON DELETE CASCADE,
    to_handle TEXT NOT NULL REFERENCES index_observations(index_handle) ON DELETE CASCADE,
    relation_kind TEXT NOT NULL,
    evidence_strength TEXT NOT NULL CHECK (evidence_strength IN ('authoritative', 'strong', 'heuristic')),
    source_kind TEXT NOT NULL,
    observed_at_ms INTEGER NOT NULL,
    UNIQUE(from_handle, to_handle, relation_kind, source_kind)
) STRICT;

CREATE TABLE index_annotations (
    annotation_id INTEGER PRIMARY KEY,
    owner_uid INTEGER NOT NULL CHECK (owner_uid >= 0),
    owner_gid INTEGER NOT NULL CHECK (owner_gid >= 0),
    target_ref TEXT NOT NULL,
    annotation_kind TEXT NOT NULL,
    safe_value TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_index_annotations_owner ON index_annotations(owner_uid, owner_gid);

CREATE TABLE health_observations (
    observation_id INTEGER PRIMARY KEY,
    component_ref TEXT NOT NULL,
    component_type TEXT NOT NULL,
    health_state TEXT NOT NULL CHECK (health_state IN ('healthy', 'degraded', 'unavailable', 'unknown')),
    reason_code TEXT NOT NULL,
    source TEXT NOT NULL,
    observed_at_ms INTEGER NOT NULL,
    recovery_disposition TEXT NOT NULL CHECK (recovery_disposition IN ('observe', 'reconcile', 'restart', 'repair', 'administrator_required', 'terminal')),
    safe_summary TEXT NOT NULL
) STRICT;
CREATE INDEX idx_health_component_time ON health_observations(component_ref, observed_at_ms DESC);

CREATE TABLE recovery_attempts (
    recovery_id INTEGER PRIMARY KEY,
    component_ref TEXT NOT NULL,
    action_kind TEXT NOT NULL,
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    outcome TEXT,
    safe_summary TEXT
) STRICT;
CREATE INDEX idx_recovery_component_time ON recovery_attempts(component_ref, started_at_ms DESC);

CREATE TABLE audit_event_refs (
    audit_ref TEXT PRIMARY KEY,
    owner_uid INTEGER,
    owner_gid INTEGER,
    event_kind TEXT NOT NULL,
    occurred_at_ms INTEGER NOT NULL,
    external_log_ref TEXT,
    safe_summary TEXT,
    CHECK ((owner_uid IS NULL AND owner_gid IS NULL) OR (owner_uid >= 0 AND owner_gid >= 0))
) STRICT;
CREATE INDEX idx_audit_event_refs_owner ON audit_event_refs(owner_uid, owner_gid);
"#;

pub(crate) const MIGRATION_2: &str = r#"
CREATE TABLE state_cleanup_watermarks (
    domain TEXT PRIMARY KEY,
    last_completed_at_ms INTEGER NOT NULL,
    last_cutoff_at_ms INTEGER,
    rows_removed INTEGER NOT NULL DEFAULT 0 CHECK (rows_removed >= 0)
) STRICT;
"#;

pub(crate) const MIGRATION_3: &str = r#"
ALTER TABLE provider_registrations ADD COLUMN display_label TEXT NOT NULL DEFAULT '';
ALTER TABLE provider_registrations ADD COLUMN scope TEXT NOT NULL DEFAULT 'system' CHECK (scope IN ('system', 'user'));
ALTER TABLE provider_registrations ADD COLUMN manifest_version INTEGER NOT NULL DEFAULT 1 CHECK (manifest_version > 0);
ALTER TABLE provider_registrations ADD COLUMN compatibility_state TEXT NOT NULL DEFAULT 'unknown' CHECK (compatibility_state IN ('compatible', 'incompatible', 'unknown'));
ALTER TABLE provider_registrations ADD COLUMN health_state TEXT NOT NULL DEFAULT 'unknown' CHECK (health_state IN ('healthy', 'degraded', 'unavailable', 'unknown'));
ALTER TABLE provider_registrations ADD COLUMN health_reason TEXT;
ALTER TABLE provider_registrations ADD COLUMN policy_domain_owner TEXT NOT NULL DEFAULT 'provider';
ALTER TABLE provider_registrations ADD COLUMN updated_at_ms INTEGER NOT NULL DEFAULT 0;
UPDATE provider_registrations SET display_label = provider_type, updated_at_ms = created_at_ms WHERE display_label = '';

CREATE UNIQUE INDEX idx_provider_active_system_type
ON provider_registrations(provider_type)
WHERE removed_at_ms IS NULL AND scope = 'system';

CREATE UNIQUE INDEX idx_provider_active_user_type_owner
ON provider_registrations(provider_type, owner_uid, owner_gid)
WHERE removed_at_ms IS NULL AND scope = 'user';

ALTER TABLE provider_tombstones ADD COLUMN software_version TEXT;
ALTER TABLE provider_tombstones ADD COLUMN interface_version TEXT;
ALTER TABLE provider_tombstones ADD COLUMN successor_provider_id TEXT;

CREATE TABLE provider_interfaces (
    provider_id TEXT NOT NULL REFERENCES provider_registrations(provider_id) ON DELETE CASCADE,
    interface_id TEXT NOT NULL,
    interface_type TEXT NOT NULL CHECK (interface_type IN ('executable', 'unix-socket', 'local-proxy', 'adapter')),
    contract_version INTEGER NOT NULL CHECK (contract_version > 0),
    target TEXT NOT NULL,
    structured_output INTEGER NOT NULL DEFAULT 0 CHECK (structured_output IN (0, 1)),
    PRIMARY KEY(provider_id, interface_id)
) STRICT;

CREATE TABLE provider_capabilities (
    provider_id TEXT NOT NULL REFERENCES provider_registrations(provider_id) ON DELETE CASCADE,
    capability_id TEXT NOT NULL,
    contract_version INTEGER NOT NULL CHECK (contract_version > 0),
    availability_state TEXT NOT NULL DEFAULT 'unknown' CHECK (availability_state IN ('available', 'degraded', 'unavailable', 'unknown')),
    reason_code TEXT,
    PRIMARY KEY(provider_id, capability_id)
) STRICT;
CREATE INDEX idx_provider_capabilities_id ON provider_capabilities(capability_id);

CREATE TABLE provider_capability_interfaces (
    provider_id TEXT NOT NULL,
    capability_id TEXT NOT NULL,
    interface_id TEXT NOT NULL,
    PRIMARY KEY(provider_id, capability_id, interface_id),
    FOREIGN KEY(provider_id, capability_id) REFERENCES provider_capabilities(provider_id, capability_id) ON DELETE CASCADE,
    FOREIGN KEY(provider_id, interface_id) REFERENCES provider_interfaces(provider_id, interface_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE provider_resource_types (
    provider_id TEXT NOT NULL REFERENCES provider_registrations(provider_id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL,
    authority TEXT NOT NULL CHECK (authority = 'provider'),
    lifetime TEXT NOT NULL CHECK (lifetime IN ('session', 'process', 'operation', 'durable', 'external')),
    PRIMARY KEY(provider_id, resource_type)
) STRICT;

CREATE TABLE provider_resource_refs (
    provider_id TEXT NOT NULL REFERENCES provider_registrations(provider_id) ON DELETE CASCADE,
    owner_uid INTEGER,
    owner_gid INTEGER,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    generation TEXT NOT NULL DEFAULT '',
    availability_state TEXT NOT NULL CHECK (availability_state IN ('available', 'stale', 'unavailable', 'removed')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY(provider_id, resource_type, resource_id, generation),
    FOREIGN KEY(provider_id, resource_type) REFERENCES provider_resource_types(provider_id, resource_type) ON DELETE CASCADE,
    CHECK ((owner_uid IS NULL AND owner_gid IS NULL) OR (owner_uid >= 0 AND owner_gid >= 0))
) STRICT;
CREATE INDEX idx_provider_resource_refs_owner ON provider_resource_refs(owner_uid, owner_gid);

CREATE TABLE provider_skills (
    provider_id TEXT NOT NULL REFERENCES provider_registrations(provider_id) ON DELETE CASCADE,
    skill_id TEXT NOT NULL,
    PRIMARY KEY(provider_id, skill_id)
) STRICT;

CREATE TABLE provider_health_contracts (
    provider_id TEXT PRIMARY KEY REFERENCES provider_registrations(provider_id) ON DELETE CASCADE,
    integration_kind TEXT NOT NULL CHECK (integration_kind IN ('none', 'openrc-service', 'structured-cli', 'unix-socket', 'adapter', 'protocol-heartbeat')),
    reference_id TEXT
) STRICT;
"#;

pub(crate) const MIGRATION_4: &str = r#"
ALTER TABLE index_observations ADD COLUMN source_id TEXT NOT NULL DEFAULT '';
ALTER TABLE index_observations ADD COLUMN native_identity TEXT NOT NULL DEFAULT '';
ALTER TABLE index_observations ADD COLUMN control_paths_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(control_paths_json));
ALTER TABLE index_observations ADD COLUMN updated_at_ms INTEGER NOT NULL DEFAULT 0;
UPDATE index_observations
SET source_id = 'legacy:' || source_kind,
    native_identity = index_handle,
    updated_at_ms = observed_at_ms
WHERE source_id = '';
CREATE UNIQUE INDEX idx_index_observations_source_identity
ON index_observations(source_id, COALESCE(source_generation, ''), native_identity);
CREATE INDEX idx_index_observations_authoritative_ref
ON index_observations(authoritative_ref);
CREATE INDEX idx_index_observations_source
ON index_observations(source_id, freshness);

ALTER TABLE index_relations ADD COLUMN source_id TEXT NOT NULL DEFAULT '';
ALTER TABLE index_relations ADD COLUMN reason_code TEXT NOT NULL DEFAULT 'unspecified';
UPDATE index_relations SET source_id = 'legacy:' || source_kind WHERE source_id = '';
CREATE INDEX idx_index_relations_source ON index_relations(source_id);
CREATE INDEX idx_index_relations_from ON index_relations(from_handle);
CREATE INDEX idx_index_relations_to ON index_relations(to_handle);

CREATE TABLE index_sources (
    source_id TEXT PRIMARY KEY,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('applications', 'proc', 'openrc', 'x11', 'i3', 'providers', 'correlation')),
    owner_uid INTEGER,
    owner_gid INTEGER,
    source_generation TEXT NOT NULL,
    health_state TEXT NOT NULL CHECK (health_state IN ('healthy', 'degraded', 'unavailable', 'unknown')),
    reason_code TEXT NOT NULL,
    last_attempt_at_ms INTEGER NOT NULL,
    last_success_at_ms INTEGER,
    updated_at_ms INTEGER NOT NULL,
    CHECK ((owner_uid IS NULL AND owner_gid IS NULL) OR (owner_uid >= 0 AND owner_gid >= 0))
) STRICT;
CREATE INDEX idx_index_sources_owner ON index_sources(owner_uid, owner_gid);
CREATE INDEX idx_index_sources_kind_health ON index_sources(source_kind, health_state);

CREATE TABLE index_runtime_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    generation INTEGER NOT NULL CHECK (generation >= 1),
    state TEXT NOT NULL CHECK (state IN ('initializing', 'healthy', 'degraded', 'rebuilding', 'unavailable')),
    reason_code TEXT NOT NULL,
    last_reconcile_at_ms INTEGER,
    updated_at_ms INTEGER NOT NULL
) STRICT;
INSERT INTO index_runtime_state(singleton, generation, state, reason_code, updated_at_ms)
VALUES (1, 1, 'initializing', 'not_reconciled', 0);
"#;

pub(crate) const MIGRATION_6: &str = r#"
CREATE TABLE significant_events (
    event_id INTEGER PRIMARY KEY,
    object_kind TEXT NOT NULL CHECK (object_kind IN ('task', 'provider', 'policy', 'runtime', 'index', 'artifact', 'health', 'privilege', 'protected_api')),
    object_ref TEXT NOT NULL,
    object_sequence INTEGER NOT NULL CHECK (object_sequence >= 1),
    principal_uid INTEGER,
    principal_gid INTEGER,
    event_kind TEXT NOT NULL,
    reason_code TEXT,
    source_ref TEXT,
    safe_summary TEXT,
    safe_data_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(safe_data_json)),
    occurred_at_ms INTEGER NOT NULL,
    UNIQUE(object_kind, object_ref, object_sequence),
    CHECK ((principal_uid IS NULL AND principal_gid IS NULL) OR (principal_uid >= 0 AND principal_gid >= 0))
) STRICT;

CREATE INDEX idx_significant_events_object_sequence
ON significant_events(object_kind, object_ref, object_sequence);
CREATE INDEX idx_significant_events_principal_time
ON significant_events(principal_uid, principal_gid, occurred_at_ms DESC)
WHERE principal_uid IS NOT NULL;
CREATE INDEX idx_significant_events_kind_time
ON significant_events(event_kind, occurred_at_ms DESC);

INSERT INTO significant_events(
    object_kind, object_ref, object_sequence, principal_uid, principal_gid,
    event_kind, reason_code, source_ref, safe_summary, safe_data_json, occurred_at_ms
)
SELECT
    'task', e.task_id, e.sequence, t.owner_uid, t.owner_gid,
    e.event_kind, NULL, e.source_ref, e.safe_summary, e.safe_data_json, e.occurred_at_ms
FROM task_events AS e
JOIN tasks AS t ON t.task_id = e.task_id;

DELETE FROM significant_events
WHERE event_id IN (
    SELECT event_id FROM (
        SELECT event_id,
               ROW_NUMBER() OVER (
                   PARTITION BY object_kind, object_ref
                   ORDER BY object_sequence DESC
               ) AS retained_rank
        FROM significant_events
    )
    WHERE retained_rank > 512
);

DROP TABLE task_events;
"#;

pub(crate) const MIGRATION_7: &str = r#"
ALTER TABLE health_observations RENAME TO health_observations_pre_v7;

CREATE TABLE health_observations (
    component_ref TEXT PRIMARY KEY,
    owner_uid INTEGER,
    owner_gid INTEGER,
    component_type TEXT NOT NULL CHECK (component_type IN ('runtime', 'state', 'index', 'index_source', 'provider_registry', 'provider', 'policy', 'audit', 'task_runtime', 'privilege', 'protected_api', 'storage', 'memory', 'service', 'codex')),
    health_state TEXT NOT NULL CHECK (health_state IN ('healthy', 'degraded', 'unavailable', 'unknown')),
    reason_code TEXT NOT NULL CHECK (reason_code IN ('ready', 'starting', 'stopping', 'not_probed', 'status_unavailable', 'service_not_running', 'service_restart_exhausted', 'socket_unavailable', 'ipc_failed', 'state_unavailable', 'state_integrity_failed', 'source_disconnected', 'source_stale', 'provider_degraded', 'provider_unavailable', 'upstream_unreachable', 'tls_failure', 'policy_unavailable', 'audit_write_failed', 'resource_low', 'resource_critical', 'resource_unavailable', 'reconciliation_required', 'reconciliation_failed', 'rebuild_required', 'rebuild_failed', 'configuration_invalid', 'incompatible', 'recovery_exhausted', 'manual_recovery_required')),
    safe_summary TEXT NOT NULL,
    source TEXT NOT NULL,
    observed_at_ms INTEGER NOT NULL,
    source_generation TEXT,
    last_healthy_at_ms INTEGER,
    recovery_disposition TEXT NOT NULL CHECK (recovery_disposition IN ('observe', 'reconcile', 'restart', 'repair', 'administrator_required', 'terminal')),
    recovery_attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (recovery_attempt_count >= 0),
    safe_details_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(safe_details_json)),
    CHECK ((owner_uid IS NULL AND owner_gid IS NULL) OR (owner_uid >= 0 AND owner_gid >= 0))
) STRICT;
CREATE INDEX idx_health_state_component ON health_observations(health_state, component_ref);
CREATE INDEX idx_health_owner_state ON health_observations(owner_uid, owner_gid, health_state);

INSERT OR REPLACE INTO health_observations(
    component_ref, component_type, health_state, reason_code, safe_summary, source,
    observed_at_ms, last_healthy_at_ms, recovery_disposition, recovery_attempt_count, safe_details_json
)
SELECT
    component_ref,
    CASE component_type
        WHEN 'runtime' THEN 'runtime'
        WHEN 'state' THEN 'state'
        WHEN 'index' THEN 'index'
        WHEN 'provider' THEN 'provider'
        WHEN 'storage' THEN 'storage'
        ELSE 'service'
    END,
    health_state,
    CASE reason_code
        WHEN 'ready' THEN 'ready'
        WHEN 'service_not_running' THEN 'service_not_running'
        WHEN 'service_restart_exhausted' THEN 'service_restart_exhausted'
        WHEN 'socket_unavailable' THEN 'socket_unavailable'
        WHEN 'ipc_failed' THEN 'ipc_failed'
        WHEN 'state_unavailable' THEN 'state_unavailable'
        WHEN 'state_integrity_failed' THEN 'state_integrity_failed'
        WHEN 'source_disconnected' THEN 'source_disconnected'
        WHEN 'source_stale' THEN 'source_stale'
        WHEN 'provider_degraded' THEN 'provider_degraded'
        WHEN 'provider_unavailable' THEN 'provider_unavailable'
        WHEN 'upstream_unreachable' THEN 'upstream_unreachable'
        WHEN 'tls_failure' THEN 'tls_failure'
        WHEN 'resource_low' THEN 'resource_low'
        WHEN 'resource_exhausted' THEN 'resource_critical'
        WHEN 'reconciliation_required' THEN 'reconciliation_required'
        WHEN 'reconciliation_failed' THEN 'reconciliation_failed'
        WHEN 'rebuild_required' THEN 'rebuild_required'
        WHEN 'rebuild_failed' THEN 'rebuild_failed'
        WHEN 'configuration_invalid' THEN 'configuration_invalid'
        WHEN 'incompatible' THEN 'incompatible'
        WHEN 'recovery_exhausted' THEN 'recovery_exhausted'
        WHEN 'manual_recovery_required' THEN 'manual_recovery_required'
        ELSE 'status_unavailable'
    END,
    safe_summary,
    source,
    observed_at_ms,
    CASE WHEN health_state = 'healthy' THEN observed_at_ms ELSE NULL END,
    recovery_disposition,
    0,
    '{}'
FROM health_observations_pre_v7
ORDER BY observed_at_ms ASC;
DROP TABLE health_observations_pre_v7;

ALTER TABLE recovery_attempts RENAME TO recovery_attempts_pre_v7;
CREATE TABLE recovery_attempts (
    recovery_id INTEGER PRIMARY KEY,
    component_ref TEXT NOT NULL,
    action_kind TEXT NOT NULL CHECK (action_kind IN ('probe', 'reconcile', 'restart', 'repair')),
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'failed', 'exhausted', 'skipped')),
    reason_code TEXT NOT NULL CHECK (reason_code IN ('ready', 'starting', 'stopping', 'not_probed', 'status_unavailable', 'service_not_running', 'service_restart_exhausted', 'socket_unavailable', 'ipc_failed', 'state_unavailable', 'state_integrity_failed', 'source_disconnected', 'source_stale', 'provider_degraded', 'provider_unavailable', 'upstream_unreachable', 'tls_failure', 'policy_unavailable', 'audit_write_failed', 'resource_low', 'resource_critical', 'resource_unavailable', 'reconciliation_required', 'reconciliation_failed', 'rebuild_required', 'rebuild_failed', 'configuration_invalid', 'incompatible', 'recovery_exhausted', 'manual_recovery_required')),
    safe_summary TEXT
) STRICT;
CREATE INDEX idx_recovery_attempts_component_time_v7 ON recovery_attempts(component_ref, started_at_ms DESC);

INSERT INTO recovery_attempts(
    recovery_id, component_ref, action_kind, attempt_number, started_at_ms,
    finished_at_ms, outcome, reason_code, safe_summary
)
SELECT
    recovery_id,
    component_ref,
    CASE action_kind
        WHEN 'reconcile' THEN 'reconcile'
        WHEN 'restart' THEN 'restart'
        WHEN 'repair' THEN 'repair'
        ELSE 'probe'
    END,
    attempt_number,
    started_at_ms,
    finished_at_ms,
    CASE outcome
        WHEN 'succeeded' THEN 'succeeded'
        WHEN 'exhausted' THEN 'exhausted'
        WHEN 'skipped' THEN 'skipped'
        ELSE 'failed'
    END,
    'status_unavailable',
    safe_summary
FROM recovery_attempts_pre_v7;
DROP TABLE recovery_attempts_pre_v7;
"#;

pub(crate) const MIGRATION_8: &str = r#"
ALTER TABLE artifacts RENAME TO artifacts_pre_v8;
DROP INDEX IF EXISTS idx_artifacts_owner;
DROP INDEX IF EXISTS idx_artifacts_task;

CREATE TABLE artifacts (
    artifact_id TEXT PRIMARY KEY CHECK (artifact_id GLOB 'artifact_*'),
    owner_uid INTEGER NOT NULL CHECK (owner_uid >= 0),
    owner_gid INTEGER NOT NULL CHECK (owner_gid >= 0),
    artifact_type TEXT NOT NULL CHECK (artifact_type IN ('file', 'report', 'release', 'diagnostic_bundle', 'screenshot', 'archive', 'other')),
    confidentiality TEXT NOT NULL CHECK (confidentiality IN ('private', 'shared', 'public')),
    retention_kind TEXT NOT NULL CHECK (retention_kind IN ('temporary', 'retained', 'until')),
    expires_at_ms INTEGER,
    availability_state TEXT NOT NULL CHECK (availability_state IN ('available', 'missing', 'unavailable', 'removed')),
    locator_kind TEXT NOT NULL CHECK (locator_kind IN ('filesystem', 'provider_resource')),
    filesystem_path TEXT,
    provider_id TEXT,
    provider_resource_type TEXT,
    provider_resource_id TEXT,
    provider_generation TEXT,
    integrity_kind TEXT NOT NULL CHECK (integrity_kind IN ('verified', 'mismatch', 'provider_authoritative', 'unverified', 'not_applicable')),
    sha256 TEXT,
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    media_type TEXT,
    created_at_ms INTEGER NOT NULL,
    registered_at_ms INTEGER NOT NULL,
    project_ref TEXT,
    safe_display_name TEXT,
    safe_metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(safe_metadata_json)),
    last_verified_at_ms INTEGER,
    removed_at_ms INTEGER,
    cleanup_authority TEXT NOT NULL DEFAULT 'none' CHECK (cleanup_authority IN ('none', 'portus', 'task', 'provider')),
    cleanup_ref TEXT,
    CHECK (retention_kind = 'until' OR expires_at_ms IS NULL),
    CHECK (retention_kind != 'until' OR expires_at_ms IS NOT NULL),
    CHECK (
        (locator_kind='filesystem' AND filesystem_path IS NOT NULL AND provider_id IS NULL AND provider_resource_type IS NULL AND provider_resource_id IS NULL AND provider_generation IS NULL)
        OR
        (locator_kind='provider_resource' AND filesystem_path IS NULL AND provider_id IS NOT NULL AND provider_resource_type IS NOT NULL AND provider_resource_id IS NOT NULL)
    ),
    CHECK (cleanup_authority != 'none' OR cleanup_ref IS NULL),
    CHECK (cleanup_authority NOT IN ('task','provider') OR cleanup_ref IS NOT NULL),
    CHECK (sha256 IS NULL OR (length(sha256)=64 AND sha256 NOT GLOB '*[^0-9a-f]*'))
) STRICT;
CREATE INDEX idx_artifacts_owner_v8 ON artifacts(owner_uid, owner_gid, artifact_id);
CREATE INDEX idx_artifacts_registered_v8 ON artifacts(registered_at_ms DESC, artifact_id DESC);
CREATE INDEX idx_artifacts_provider_v8 ON artifacts(provider_id, provider_resource_type);

CREATE TABLE artifact_task_relationships (
    artifact_id TEXT NOT NULL REFERENCES artifacts(artifact_id) ON DELETE CASCADE,
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    relationship_kind TEXT NOT NULL CHECK (relationship_kind IN ('produced_by', 'required_by')),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY(artifact_id, task_id, relationship_kind)
) STRICT;
CREATE INDEX idx_artifact_task_relationships_task_v8 ON artifact_task_relationships(task_id, artifact_id);

CREATE TABLE artifact_provider_relationships (
    artifact_id TEXT PRIMARY KEY REFERENCES artifacts(artifact_id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL REFERENCES provider_registrations(provider_id),
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    generation TEXT NOT NULL DEFAULT '',
    created_at_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_artifact_provider_relationships_provider_v8 ON artifact_provider_relationships(provider_id, resource_type);

CREATE TABLE artifact_grants (
    artifact_id TEXT NOT NULL REFERENCES artifacts(artifact_id) ON DELETE CASCADE,
    principal_uid INTEGER NOT NULL CHECK (principal_uid >= 0),
    principal_gid INTEGER NOT NULL CHECK (principal_gid >= 0),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY(artifact_id, principal_uid, principal_gid)
) STRICT;

CREATE TABLE artifact_holds (
    artifact_id TEXT NOT NULL REFERENCES artifacts(artifact_id) ON DELETE CASCADE,
    hold_kind TEXT NOT NULL CHECK (hold_kind IN ('explicit', 'task', 'recovery', 'audit', 'delivery')),
    holder_ref TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER,
    PRIMARY KEY(artifact_id, hold_kind, holder_ref)
) STRICT;
CREATE INDEX idx_artifact_holds_expiry_v8 ON artifact_holds(expires_at_ms, artifact_id);

CREATE TABLE artifact_tombstones (
    artifact_id TEXT PRIMARY KEY CHECK (artifact_id GLOB 'artifact_*'),
    owner_uid INTEGER NOT NULL CHECK (owner_uid >= 0),
    owner_gid INTEGER NOT NULL CHECK (owner_gid >= 0),
    artifact_type TEXT NOT NULL CHECK (artifact_type IN ('file', 'report', 'release', 'diagnostic_bundle', 'screenshot', 'archive', 'other')),
    confidentiality TEXT NOT NULL CHECK (confidentiality IN ('private', 'shared', 'public')),
    registered_at_ms INTEGER NOT NULL,
    tombstoned_at_ms INTEGER NOT NULL,
    reason_code TEXT NOT NULL
) STRICT;
CREATE INDEX idx_artifact_tombstones_owner_v8 ON artifact_tombstones(owner_uid, owner_gid, tombstoned_at_ms DESC);

INSERT INTO artifacts(
    artifact_id, owner_uid, owner_gid, artifact_type, confidentiality, retention_kind,
    expires_at_ms, availability_state, locator_kind, filesystem_path,
    integrity_kind, sha256, size_bytes, media_type, created_at_ms, registered_at_ms,
    safe_metadata_json, last_verified_at_ms, cleanup_authority
)
SELECT
    artifact_id, owner_uid, owner_gid, artifact_type, confidentiality, retention_kind,
    expires_at_ms, availability_state, 'filesystem', locator,
    integrity_kind, sha256, size_bytes, media_type, created_at_ms, created_at_ms,
    '{}', CASE WHEN integrity_kind='verified' THEN created_at_ms ELSE NULL END, 'none'
FROM artifacts_pre_v8
WHERE locator_kind='filesystem';

INSERT INTO artifact_task_relationships(artifact_id, task_id, relationship_kind, created_at_ms)
SELECT artifact_id, task_id, 'produced_by', created_at_ms
FROM artifacts_pre_v8
WHERE locator_kind='filesystem' AND task_id IS NOT NULL;

INSERT INTO artifact_tombstones(
    artifact_id, owner_uid, owner_gid, artifact_type, confidentiality,
    registered_at_ms, tombstoned_at_ms, reason_code
)
SELECT
    artifact_id, owner_uid, owner_gid, artifact_type, confidentiality,
    created_at_ms, created_at_ms, 'legacy_provider_locator_unresolvable'
FROM artifacts_pre_v8
WHERE locator_kind='provider_resource';

DROP TABLE artifacts_pre_v8;
"#;

pub(crate) const MIGRATION_5: &str = r#"
ALTER TABLE projects ADD COLUMN display_name TEXT;
ALTER TABLE projects ADD COLUMN updated_at_ms INTEGER NOT NULL DEFAULT 0;
UPDATE projects SET updated_at_ms = created_at_ms WHERE updated_at_ms = 0;

ALTER TABLE session_refs ADD COLUMN session_name TEXT;
ALTER TABLE session_refs ADD COLUMN working_directory TEXT;
ALTER TABLE session_refs ADD COLUMN role TEXT;
ALTER TABLE session_refs ADD COLUMN model_name TEXT;
ALTER TABLE session_refs ADD COLUMN status_observation TEXT;
ALTER TABLE session_refs ADD COLUMN updated_at_ms INTEGER NOT NULL DEFAULT 0;
UPDATE session_refs SET updated_at_ms = created_at_ms WHERE updated_at_ms = 0;

ALTER TABLE tasks ADD COLUMN title TEXT;
ALTER TABLE tasks ADD COLUMN state_reason TEXT;
ALTER TABLE tasks ADD COLUMN waiting_reason TEXT CHECK (waiting_reason IS NULL OR waiting_reason IN ('approval', 'user_input', 'provider', 'resource', 'dependency', 'rate_limit', 'external_condition'));
ALTER TABLE tasks ADD COLUMN requester_surface TEXT NOT NULL DEFAULT 'internal';
ALTER TABLE tasks ADD COLUMN retry_safety TEXT NOT NULL DEFAULT 'never' CHECK (retry_safety IN ('never', 'idempotent', 'contract_safe'));
ALTER TABLE tasks ADD COLUMN result_kind TEXT CHECK (result_kind IS NULL OR result_kind IN ('success', 'failure', 'cancelled', 'interrupted'));
ALTER TABLE tasks ADD COLUMN result_summary TEXT;
ALTER TABLE tasks ADD COLUMN last_event_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_event_sequence >= 0);
ALTER TABLE tasks ADD COLUMN updated_at_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN cancellation_requested_at_ms INTEGER;
UPDATE tasks SET updated_at_ms = created_at_ms WHERE updated_at_ms = 0;
CREATE INDEX idx_tasks_owner_created ON tasks(owner_uid, owner_gid, created_at_ms DESC, task_id DESC);
CREATE INDEX idx_tasks_nonterminal ON tasks(state) WHERE state NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted');

ALTER TABLE task_attempts ADD COLUMN retry_reason TEXT;
ALTER TABLE task_attempts ADD COLUMN failure_classification TEXT;
ALTER TABLE task_attempts ADD COLUMN retry_safe INTEGER NOT NULL DEFAULT 0 CHECK (retry_safe IN (0, 1));
ALTER TABLE task_attempts ADD COLUMN exit_code INTEGER;

ALTER TABLE task_events ADD COLUMN source_ref TEXT;
ALTER TABLE task_events ADD COLUMN safe_data_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(safe_data_json));

CREATE TABLE task_execution_relationships (
    relation_id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    mode TEXT NOT NULL CHECK (mode IN ('managed', 'associated')),
    backend_kind TEXT NOT NULL CHECK (backend_kind IN ('native_process', 'codex_root', 'codex_subagent', 'provider', 'openrc_service', 'application', 'child_task')),
    backend_ref TEXT NOT NULL,
    generation_ref TEXT NOT NULL,
    process_id INTEGER CHECK (process_id IS NULL OR process_id > 0),
    correlation_ref TEXT,
    status TEXT NOT NULL CHECK (status IN ('starting', 'running', 'stopped', 'lost', 'unknown')),
    cancellation_supported INTEGER NOT NULL DEFAULT 0 CHECK (cancellation_supported IN (0, 1)),
    reconciliation_supported INTEGER NOT NULL DEFAULT 0 CHECK (reconciliation_supported IN (0, 1)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    UNIQUE(task_id, backend_kind, backend_ref, generation_ref)
) STRICT;
CREATE INDEX idx_task_relationships_task ON task_execution_relationships(task_id, mode, status);
CREATE INDEX idx_task_relationships_process ON task_execution_relationships(process_id) WHERE process_id IS NOT NULL;
"#;
