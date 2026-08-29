use crate::{StateError, StateResult, migration};
use portus_protocol::{Principal, TaskId};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatabaseReadiness {
    Ready,
    IntegrityFailure,
    UnsupportedSchema,
}

#[derive(Clone, Debug)]
pub struct StateOpenOptions {
    pub busy_timeout: Duration,
    pub create_parent: bool,
}

impl Default for StateOpenOptions {
    fn default() -> Self {
        Self {
            busy_timeout: DEFAULT_BUSY_TIMEOUT,
            create_parent: true,
        }
    }
}

pub struct PortusState {
    pub(crate) connection: Connection,
    path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalTaskRecord {
    pub task_id: TaskId,
    pub owner: Principal,
    pub objective_summary: String,
    pub state: String,
}

impl PortusState {
    pub fn open(path: impl AsRef<Path>) -> StateResult<Self> {
        Self::open_with_options(path, StateOpenOptions::default())
    }

    pub fn open_with_options(
        path: impl AsRef<Path>,
        options: StateOpenOptions,
    ) -> StateResult<Self> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(StateError::InvalidPath("database path is empty".into()));
        }
        if options.create_parent {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut connection = Connection::open_with_flags(path, flags)?;
        configure_connection(&connection, options.busy_timeout, false)?;
        migration::migrate(&mut connection, path)?;
        verify_integrity(&connection)?;

        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }

    pub fn open_read_only(path: impl AsRef<Path>) -> StateResult<Self> {
        let path = path.as_ref();
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags)?;
        configure_connection(&connection, DEFAULT_BUSY_TIMEOUT, true)?;
        let version = migration::current_schema_version(&connection)?;
        if version == 0 {
            return Err(StateError::ReadOnlySchemaMissing);
        }
        if version > migration::LATEST_SCHEMA_VERSION {
            return Err(StateError::UnsupportedSchemaVersion {
                found: version,
                latest: migration::LATEST_SCHEMA_VERSION,
            });
        }
        verify_integrity(&connection)?;
        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn schema_version(&self) -> StateResult<u32> {
        migration::current_schema_version(&self.connection)
    }

    pub fn migration_applied(&self, version: u32) -> StateResult<bool> {
        migration::has_migration(&self.connection, version)
    }

    pub fn integrity_check(&self) -> StateResult<()> {
        verify_integrity(&self.connection)
    }

    pub fn readiness(&self) -> DatabaseReadiness {
        match self.schema_version() {
            Ok(version) if version <= migration::LATEST_SCHEMA_VERSION => {
                match self.integrity_check() {
                    Ok(()) => DatabaseReadiness::Ready,
                    Err(StateError::IntegrityFailure(_)) => DatabaseReadiness::IntegrityFailure,
                    Err(_) => DatabaseReadiness::IntegrityFailure,
                }
            }
            Ok(_) | Err(StateError::UnsupportedSchemaVersion { .. }) => {
                DatabaseReadiness::UnsupportedSchema
            }
            Err(_) => DatabaseReadiness::IntegrityFailure,
        }
    }

    /// Minimal P2 task fixture API used to prove durable principal-scoped state.
    /// Higher-level task lifecycle semantics remain owned by later task/runtime phases.
    pub fn insert_task_fixture(
        &self,
        task_id: &TaskId,
        owner: Principal,
        objective_summary: &str,
        state: &str,
        created_at_ms: i64,
    ) -> StateResult<()> {
        self.connection.execute(
            "INSERT INTO tasks(task_id, owner_uid, owner_gid, objective_summary, state, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![task_id.to_string(), owner.uid(), owner.gid(), objective_summary, state, created_at_ms],
        )?;
        Ok(())
    }

    pub fn task_for_principal(
        &self,
        task_id: &TaskId,
        principal: Principal,
    ) -> StateResult<Option<PrincipalTaskRecord>> {
        self.connection
            .query_row(
                "SELECT task_id, owner_uid, owner_gid, objective_summary, state FROM tasks WHERE task_id = ?1 AND owner_uid = ?2 AND owner_gid = ?3",
                params![task_id.to_string(), principal.uid(), principal.gid()],
                |row| {
                    let task_id_string: String = row.get(0)?;
                    let task_id = task_id_string.parse::<TaskId>().map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(PrincipalTaskRecord {
                        task_id,
                        owner: Principal::new(row.get(1)?, row.get(2)?),
                        objective_summary: row.get(3)?,
                        state: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(StateError::from)
    }

    /// Removes only derived index observations older than the caller-provided
    /// cutoff. Durable tasks, artifacts, approvals, providers, and annotations
    /// are never touched by this generic cleanup primitive.
    pub fn cleanup_stale_index_observations(
        &self,
        cutoff_observed_at_ms: i64,
        completed_at_ms: i64,
    ) -> StateResult<usize> {
        let removed = self.connection.execute(
            "DELETE FROM index_observations WHERE observed_at_ms < ?1 AND freshness IN ('stale', 'historical', 'unavailable')",
            params![cutoff_observed_at_ms],
        )?;
        self.connection.execute(
            "INSERT INTO state_cleanup_watermarks(domain, last_completed_at_ms, last_cutoff_at_ms, rows_removed) VALUES ('index_observations', ?1, ?2, ?3) ON CONFLICT(domain) DO UPDATE SET last_completed_at_ms=excluded.last_completed_at_ms, last_cutoff_at_ms=excluded.last_cutoff_at_ms, rows_removed=excluded.rows_removed",
            params![completed_at_ms, cutoff_observed_at_ms, removed as i64],
        )?;
        Ok(removed)
    }

    #[cfg(test)]
    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}

fn configure_connection(
    connection: &Connection,
    busy_timeout: Duration,
    read_only: bool,
) -> StateResult<()> {
    connection.busy_timeout(busy_timeout)?;
    connection.execute_batch("PRAGMA foreign_keys = ON;")?;
    if !read_only {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
    }
    Ok(())
}

fn verify_integrity(connection: &Connection) -> StateResult<()> {
    let result: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(StateError::IntegrityFailure(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LATEST_SCHEMA_VERSION, migration};
    use portus_protocol::TaskId;
    use rusqlite::Connection;
    use std::{fs, path::PathBuf};

    struct TestDb {
        dir: PathBuf,
        path: PathBuf,
    }

    impl TestDb {
        fn new(name: &str) -> Self {
            let unique = TaskId::new().to_string();
            let dir = std::env::temp_dir().join(format!("portus-state-{name}-{unique}"));
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

    #[test]
    fn migration_plan_is_ordered_and_marks_backup_boundary() {
        let plan = crate::migration_plan().collect::<Vec<_>>();
        assert_eq!(
            plan,
            vec![
                crate::MigrationInfo {
                    version: 1,
                    backup_required: false,
                },
                crate::MigrationInfo {
                    version: 2,
                    backup_required: true,
                },
                crate::MigrationInfo {
                    version: 3,
                    backup_required: true,
                },
                crate::MigrationInfo {
                    version: 4,
                    backup_required: true,
                },
                crate::MigrationInfo {
                    version: 5,
                    backup_required: true,
                },
                crate::MigrationInfo {
                    version: 6,
                    backup_required: true,
                },
                crate::MigrationInfo {
                    version: 7,
                    backup_required: true,
                },
                crate::MigrationInfo {
                    version: 8,
                    backup_required: true,
                },
            ]
        );
    }

    #[test]
    fn fresh_database_is_created_at_latest_schema_with_required_pragmas() {
        let test_db = TestDb::new("fresh");
        let state = PortusState::open(&test_db.path).unwrap();
        assert_eq!(state.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
        assert_eq!(state.readiness(), DatabaseReadiness::Ready);

        let foreign_keys: i64 = state
            .connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        let journal_mode: String = state
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        let synchronous: i64 = state
            .connection
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        assert_eq!(synchronous, 2);
        assert!(state.migration_applied(1).unwrap());
        assert!(state.migration_applied(2).unwrap());
        assert!(state.migration_applied(3).unwrap());
        assert!(state.migration_applied(4).unwrap());
        assert!(state.migration_applied(5).unwrap());
        assert!(state.migration_applied(6).unwrap());
        assert!(state.migration_applied(7).unwrap());
        assert!(state.migration_applied(8).unwrap());
    }

    #[test]
    fn reopen_preserves_durable_state() {
        let test_db = TestDb::new("restart");
        let task_id = TaskId::new();
        let owner = Principal::new(1000, 1000);
        {
            let state = PortusState::open(&test_db.path).unwrap();
            state
                .insert_task_fixture(&task_id, owner, "persist me", "running", 10)
                .unwrap();
        }
        let reopened = PortusState::open(&test_db.path).unwrap();
        let record = reopened
            .task_for_principal(&task_id, owner)
            .unwrap()
            .unwrap();
        assert_eq!(record.objective_summary, "persist me");
        assert_eq!(record.state, "running");
    }

    #[test]
    fn principal_filtering_denies_cross_principal_task_visibility() {
        let test_db = TestDb::new("principal");
        let state = PortusState::open(&test_db.path).unwrap();
        let task_id = TaskId::new();
        let owner = Principal::new(1000, 1000);
        state
            .insert_task_fixture(&task_id, owner, "private", "created", 1)
            .unwrap();

        assert!(state.task_for_principal(&task_id, owner).unwrap().is_some());
        assert!(
            state
                .task_for_principal(&task_id, Principal::new(1001, 1001))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn existing_v1_database_upgrades_through_v8_with_required_backups() {
        let test_db = TestDb::new("upgrade");
        let mut raw = Connection::open(&test_db.path).unwrap();
        configure_connection(&raw, DEFAULT_BUSY_TIMEOUT, false).unwrap();
        migration::migrate_through_for_test(&mut raw, &test_db.path, 1).unwrap();
        assert_eq!(migration::current_schema_version(&raw).unwrap(), 1);
        drop(raw);

        let upgraded = PortusState::open(&test_db.path).unwrap();
        assert_eq!(upgraded.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
        let backup = test_db
            .dir
            .join("migration-backups")
            .join("portus.db.pre-v2.sqlite");
        assert!(backup.is_file());
        let backup_connection = Connection::open(backup).unwrap();
        assert_eq!(
            migration::current_schema_version(&backup_connection).unwrap(),
            1
        );
        let cleanup_table_exists: bool = backup_connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='state_cleanup_watermarks')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!cleanup_table_exists);

        let v3_backup = test_db
            .dir
            .join("migration-backups")
            .join("portus.db.pre-v3.sqlite");
        assert!(v3_backup.is_file());
        let v3_backup_connection = Connection::open(v3_backup).unwrap();
        assert_eq!(
            migration::current_schema_version(&v3_backup_connection).unwrap(),
            2
        );

        let v4_backup = test_db
            .dir
            .join("migration-backups")
            .join("portus.db.pre-v4.sqlite");
        assert!(v4_backup.is_file());
        let v4_backup_connection = Connection::open(v4_backup).unwrap();
        assert_eq!(
            migration::current_schema_version(&v4_backup_connection).unwrap(),
            3
        );

        let v5_backup = test_db
            .dir
            .join("migration-backups")
            .join("portus.db.pre-v5.sqlite");
        assert!(v5_backup.is_file());
        let v5_backup_connection = Connection::open(v5_backup).unwrap();
        assert_eq!(
            migration::current_schema_version(&v5_backup_connection).unwrap(),
            4
        );

        let v6_backup = test_db
            .dir
            .join("migration-backups")
            .join("portus.db.pre-v6.sqlite");
        assert!(v6_backup.is_file());
        let v6_backup_connection = Connection::open(v6_backup).unwrap();
        assert_eq!(
            migration::current_schema_version(&v6_backup_connection).unwrap(),
            5
        );

        let v7_backup = test_db
            .dir
            .join("migration-backups")
            .join("portus.db.pre-v7.sqlite");
        assert!(v7_backup.is_file());
        let v7_backup_connection = Connection::open(v7_backup).unwrap();
        assert_eq!(
            migration::current_schema_version(&v7_backup_connection).unwrap(),
            6
        );

        let v8_backup = test_db
            .dir
            .join("migration-backups")
            .join("portus.db.pre-v8.sqlite");
        assert!(v8_backup.is_file());
        let v8_backup_connection = Connection::open(v8_backup).unwrap();
        assert_eq!(
            migration::current_schema_version(&v8_backup_connection).unwrap(),
            7
        );
    }

    #[test]
    fn v6_migrates_p7_task_events_without_parallel_legacy_table() {
        let test_db = TestDb::new("v6-events");
        let mut raw = Connection::open(&test_db.path).unwrap();
        configure_connection(&raw, DEFAULT_BUSY_TIMEOUT, false).unwrap();
        migration::migrate_through_for_test(&mut raw, &test_db.path, 5).unwrap();
        let task_id = TaskId::new();
        raw.execute(
            "INSERT INTO tasks(task_id, owner_uid, owner_gid, objective_summary, state, created_at_ms, requester_surface, retry_safety, last_event_sequence, updated_at_ms) VALUES (?1, 1000, 1000, 'migration fixture', 'running', 1, 'test', 'never', 2, 2)",
            params![task_id.to_string()],
        )
        .unwrap();
        raw.execute(
            "INSERT INTO task_events(task_id, sequence, event_kind, safe_summary, occurred_at_ms, source_ref, safe_data_json) VALUES (?1, 1, 'task.created', 'created', 1, 'fixture', '{}')",
            params![task_id.to_string()],
        )
        .unwrap();
        raw.execute(
            "INSERT INTO task_events(task_id, sequence, event_kind, safe_summary, occurred_at_ms, source_ref, safe_data_json) VALUES (?1, 2, 'task.running', 'running', 2, 'fixture', '{\"phase\":\"run\"}')",
            params![task_id.to_string()],
        )
        .unwrap();
        migration::migrate_through_for_test(&mut raw, &test_db.path, 6).unwrap();
        assert_eq!(migration::current_schema_version(&raw).unwrap(), 6);

        let legacy_exists: bool = raw
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='task_events')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!legacy_exists);
        let migrated: Vec<(i64, String, i64, i64, String)> = raw
            .prepare("SELECT object_sequence, event_kind, principal_uid, principal_gid, safe_data_json FROM significant_events WHERE object_kind='task' AND object_ref=?1 ORDER BY object_sequence")
            .unwrap()
            .query_map(params![task_id.to_string()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(migrated.len(), 2);
        assert_eq!(migrated[0].0, 1);
        assert_eq!(migrated[1].0, 2);
        assert_eq!(migrated[0].1, "task.created");
        assert_eq!(migrated[1].1, "task.running");
        assert_eq!((migrated[0].2, migrated[0].3), (1000, 1000));
        assert_eq!(migrated[1].4, "{\"phase\":\"run\"}");
    }

    #[test]
    fn v8_hard_cuts_preliminary_artifacts_without_provider_rebinding() {
        let test_db = TestDb::new("v8-artifacts");
        let mut raw = Connection::open(&test_db.path).unwrap();
        configure_connection(&raw, DEFAULT_BUSY_TIMEOUT, false).unwrap();
        migration::migrate_through_for_test(&mut raw, &test_db.path, 7).unwrap();
        let filesystem_id = portus_protocol::ArtifactId::new();
        let provider_id = portus_protocol::ArtifactId::new();
        raw.execute(
            "INSERT INTO artifacts(artifact_id, owner_uid, owner_gid, task_id, artifact_type, confidentiality, retention_kind, expires_at_ms, availability_state, locator_kind, locator, integrity_kind, media_type, size_bytes, sha256, created_at_ms) VALUES (?1,1000,1000,NULL,'report','private','retained',NULL,'available','filesystem','/tmp/report.pdf','verified','application/pdf',3,?2,10)",
            params![filesystem_id.to_string(), "0".repeat(64)],
        )
        .unwrap();
        raw.execute(
            "INSERT INTO artifacts(artifact_id, owner_uid, owner_gid, task_id, artifact_type, confidentiality, retention_kind, expires_at_ms, availability_state, locator_kind, locator, integrity_kind, media_type, size_bytes, sha256, created_at_ms) VALUES (?1,1000,1000,NULL,'other','private','retained',NULL,'unavailable','provider_resource','legacy-opaque-provider-ref','unverified',NULL,NULL,NULL,11)",
            params![provider_id.to_string()],
        )
        .unwrap();
        migration::migrate_through_for_test(&mut raw, &test_db.path, 8).unwrap();
        assert_eq!(migration::current_schema_version(&raw).unwrap(), 8);

        let filesystem_path: String = raw
            .query_row(
                "SELECT filesystem_path FROM artifacts WHERE artifact_id=?1",
                params![filesystem_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(filesystem_path, "/tmp/report.pdf");
        let provider_active: bool = raw
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM artifacts WHERE artifact_id=?1)",
                params![provider_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!provider_active);
        let tombstone_reason: String = raw
            .query_row(
                "SELECT reason_code FROM artifact_tombstones WHERE artifact_id=?1",
                params![provider_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tombstone_reason, "legacy_provider_locator_unresolvable");
    }

    #[test]
    fn failed_test_migration_rolls_back_completely() {
        let test_db = TestDb::new("rollback");
        let mut state = PortusState::open(&test_db.path).unwrap();
        let before = state.schema_version().unwrap();
        let result = migration::apply_sql_as_test_migration(
            state.connection_mut(),
            before + 1,
            "CREATE TABLE should_rollback(id INTEGER); INSERT INTO table_that_does_not_exist VALUES (1);",
        );
        assert!(result.is_err());
        assert_eq!(state.schema_version().unwrap(), before);
        let table_exists: bool = state.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='should_rollback')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(!table_exists);
    }

    #[test]
    fn migration_history_gap_is_rejected_without_repairing_it() {
        let test_db = TestDb::new("migration-gap");
        {
            let state = PortusState::open(&test_db.path).unwrap();
            state
                .connection
                .execute("DELETE FROM schema_migrations WHERE version = 1", [])
                .unwrap();
        }

        let error = match PortusState::open(&test_db.path) {
            Ok(_) => panic!("gapped migration history unexpectedly opened"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            StateError::InvalidMigrationHistory {
                expected: 1,
                found: 2
            }
        ));

        let raw = Connection::open(&test_db.path).unwrap();
        let remaining: Vec<u32> = raw
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(remaining, vec![2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn newer_schema_is_rejected_without_mutation() {
        let test_db = TestDb::new("future");
        {
            let state = PortusState::open(&test_db.path).unwrap();
            state
                .connection
                .execute(
                    "INSERT INTO schema_migrations(version, applied_at_ms) VALUES (?1, 1)",
                    params![LATEST_SCHEMA_VERSION + 1],
                )
                .unwrap();
        }
        let error = match PortusState::open(&test_db.path) {
            Ok(_) => panic!("future schema unexpectedly opened"),
            Err(error) => error,
        };
        assert!(matches!(error, StateError::UnsupportedSchemaVersion { .. }));

        let raw = Connection::open(&test_db.path).unwrap();
        let max_version: u32 = raw
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(max_version, LATEST_SCHEMA_VERSION + 1);
    }

    #[test]
    fn read_only_open_does_not_migrate_or_create() {
        let test_db = TestDb::new("readonly");
        let state = PortusState::open(&test_db.path).unwrap();
        drop(state);
        let read_only = PortusState::open_read_only(&test_db.path).unwrap();
        assert_eq!(read_only.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn corrupt_database_is_not_silently_recreated() {
        let test_db = TestDb::new("corrupt");
        fs::write(&test_db.path, b"not a sqlite database but durable evidence").unwrap();
        let original = fs::read(&test_db.path).unwrap();
        assert!(PortusState::open(&test_db.path).is_err());
        assert_eq!(fs::read(&test_db.path).unwrap(), original);
    }

    #[test]
    fn schema_rejects_invalid_locked_semantic_values() {
        let test_db = TestDb::new("semantic-checks");
        let state = PortusState::open(&test_db.path).unwrap();
        let task_id = TaskId::new();
        let owner = Principal::new(1000, 1000);

        let invalid_task = state.insert_task_fixture(
            &task_id,
            owner,
            "invalid state must fail",
            "definitely_not_a_task_state",
            1,
        );
        assert!(invalid_task.is_err());

        let invalid_freshness = state.connection.execute(
            "INSERT INTO index_observations(index_handle, resource_type, source_kind, freshness, observed_at_ms) VALUES (?1, 'process', 'proc', 'fresh-ish', 1)",
            params![portus_protocol::IndexHandle::new().to_string()],
        );
        assert!(invalid_freshness.is_err());

        let invalid_health = state.connection.execute(
            "INSERT INTO health_observations(component_ref, component_type, health_state, reason_code, source, observed_at_ms, recovery_disposition, safe_summary) VALUES ('runtime:portusd', 'runtime', 'mostly-fine', 'test', 'test', 1, 'observe', 'test')",
            [],
        );
        assert!(invalid_health.is_err());
    }

    #[test]
    fn schema_contains_no_secret_value_columns_or_blob_payloads() {
        let test_db = TestDb::new("nosecrets");
        let state = PortusState::open(&test_db.path).unwrap();
        let mut statement = state.connection.prepare(
            "SELECT sql FROM sqlite_master WHERE type='table' AND sql IS NOT NULL ORDER BY name",
        ).unwrap();
        let definitions = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            .to_ascii_lowercase();

        for forbidden in [
            "api_key",
            "password",
            "private_key",
            "secret_value",
            "credential_value",
            "token_value",
            " BLOB",
        ] {
            assert!(
                !definitions.contains(&forbidden.to_ascii_lowercase()),
                "schema contains forbidden secret/blob marker: {forbidden}"
            );
        }
    }

    #[test]
    fn cleanup_removes_only_eligible_derived_index_rows() {
        let test_db = TestDb::new("cleanup");
        let state = PortusState::open(&test_db.path).unwrap();
        state.connection.execute(
            "INSERT INTO index_observations(index_handle, resource_type, source_kind, freshness, observed_at_ms) VALUES (?1, 'process', 'proc', 'stale', 10)",
            params![portus_protocol::IndexHandle::new().to_string()],
        ).unwrap();
        let task_id = TaskId::new();
        let owner = Principal::new(1000, 1000);
        state
            .insert_task_fixture(&task_id, owner, "durable", "succeeded", 1)
            .unwrap();

        assert_eq!(state.cleanup_stale_index_observations(20, 30).unwrap(), 1);
        assert!(state.task_for_principal(&task_id, owner).unwrap().is_some());
    }
}
