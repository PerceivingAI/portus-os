use crate::{StateError, StateResult, schema};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub const LATEST_SCHEMA_VERSION: u32 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MigrationInfo {
    pub version: u32,
    pub backup_required: bool,
}

struct Migration {
    info: MigrationInfo,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        info: MigrationInfo {
            version: 1,
            backup_required: false,
        },
        sql: schema::MIGRATION_1,
    },
    Migration {
        info: MigrationInfo {
            version: 2,
            backup_required: true,
        },
        sql: schema::MIGRATION_2,
    },
    Migration {
        info: MigrationInfo {
            version: 3,
            backup_required: true,
        },
        sql: schema::MIGRATION_3,
    },
    Migration {
        info: MigrationInfo {
            version: 4,
            backup_required: true,
        },
        sql: schema::MIGRATION_4,
    },
    Migration {
        info: MigrationInfo {
            version: 5,
            backup_required: true,
        },
        sql: schema::MIGRATION_5,
    },
    Migration {
        info: MigrationInfo {
            version: 6,
            backup_required: true,
        },
        sql: schema::MIGRATION_6,
    },
    Migration {
        info: MigrationInfo {
            version: 7,
            backup_required: true,
        },
        sql: schema::MIGRATION_7,
    },
    Migration {
        info: MigrationInfo {
            version: 8,
            backup_required: true,
        },
        sql: schema::MIGRATION_8,
    },
];

pub fn migration_plan() -> impl ExactSizeIterator<Item = MigrationInfo> {
    MIGRATIONS.iter().map(|migration| migration.info)
}

pub(crate) fn current_schema_version(connection: &Connection) -> StateResult<u32> {
    let table_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(0);
    }

    let mut statement =
        connection.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
    let versions = statement
        .query_map([], |row| row.get::<_, u32>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut expected = 1_u32;
    for version in versions {
        if version != expected {
            return Err(StateError::InvalidMigrationHistory {
                expected,
                found: version,
            });
        }
        expected = expected.saturating_add(1);
    }
    Ok(expected - 1)
}

pub(crate) fn migrate(connection: &mut Connection, database_path: &Path) -> StateResult<()> {
    let current = current_schema_version(connection)?;
    if current > LATEST_SCHEMA_VERSION {
        return Err(StateError::UnsupportedSchemaVersion {
            found: current,
            latest: LATEST_SCHEMA_VERSION,
        });
    }

    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.info.version > current)
    {
        if migration.info.backup_required && current > 0 {
            create_pre_migration_backup(connection, database_path, migration.info.version)?;
        }
        apply_migration(connection, migration)?;
    }
    Ok(())
}

fn apply_migration(connection: &mut Connection, migration: &Migration) -> StateResult<()> {
    let transaction = connection.transaction()?;
    let result = apply_migration_in_transaction(&transaction, migration);
    match result {
        Ok(()) => transaction.commit().map_err(StateError::from),
        Err(error) => {
            let _ = transaction.rollback();
            Err(StateError::MigrationFailed {
                version: migration.info.version,
                message: error.to_string(),
            })
        }
    }
}

fn apply_migration_in_transaction(
    transaction: &Transaction<'_>,
    migration: &Migration,
) -> rusqlite::Result<()> {
    transaction.execute_batch(migration.sql)?;
    transaction.execute(
        "INSERT INTO schema_migrations(version, applied_at_ms) VALUES (?1, ?2)",
        params![migration.info.version, unix_time_ms()],
    )?;
    if migration.info.version == 1 {
        transaction.execute(
            "INSERT INTO runtime_metadata(singleton, created_at_ms, updated_at_ms) VALUES (1, ?1, ?1)",
            params![unix_time_ms()],
        )?;
    }
    Ok(())
}

fn create_pre_migration_backup(
    connection: &Connection,
    database_path: &Path,
    target_version: u32,
) -> StateResult<()> {
    let parent = database_path.parent().ok_or_else(|| {
        StateError::InvalidPath("database has no parent directory for migration backup".into())
    })?;
    let file_name = database_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| StateError::InvalidPath("database file name is not valid UTF-8".into()))?;
    let backup_dir = parent.join("migration-backups");
    fs::create_dir_all(&backup_dir)?;
    let backup_path = backup_dir.join(format!("{file_name}.pre-v{target_version}.sqlite"));
    if backup_path.exists() {
        fs::remove_file(&backup_path)?;
    }

    let mut destination = Connection::open(&backup_path)?;
    {
        let backup = rusqlite::backup::Backup::new(connection, &mut destination)?;
        backup.run_to_completion(64, std::time::Duration::from_millis(10), None)?;
    }
    destination.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

pub(crate) fn has_migration(connection: &Connection, version: u32) -> StateResult<bool> {
    let value = connection
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE version = ?1",
            params![version],
            |_| Ok(true),
        )
        .optional()?;
    Ok(value.unwrap_or(false))
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
pub(crate) fn migrate_through_for_test(
    connection: &mut Connection,
    database_path: &Path,
    max_version: u32,
) -> StateResult<()> {
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.info.version <= max_version)
    {
        let current = current_schema_version(connection)?;
        if migration.info.version <= current {
            continue;
        }
        if migration.info.backup_required && current > 0 {
            create_pre_migration_backup(connection, database_path, migration.info.version)?;
        }
        apply_migration(connection, migration)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn apply_sql_as_test_migration(
    connection: &mut Connection,
    version: u32,
    sql: &str,
) -> StateResult<()> {
    let migration = Migration {
        info: MigrationInfo {
            version,
            backup_required: false,
        },
        sql: Box::leak(sql.to_owned().into_boxed_str()),
    };
    apply_migration(connection, &migration)
}
