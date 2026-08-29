use portus_protected_api::{
    CredentialMetadata, CredentialState, SecretMaterial, validate_credential_ref,
    validate_provider_id,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fmt,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const STORE_SCHEMA_VERSION: u32 = 1;
pub const MAX_CREDENTIALS: usize = 1024;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    Invalid(&'static str),
    NotFound,
    Revoked,
    IncompatibleSchema(u32),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sql(error) => write!(f, "protected credential store error: {error}"),
            Self::Invalid(message) => write!(f, "invalid protected credential record: {message}"),
            Self::NotFound => f.write_str("credential not found"),
            Self::Revoked => f.write_str("credential is revoked"),
            Self::IncompatibleSchema(version) => {
                write!(f, "unsupported credential store schema version {version}")
            }
        }
    }
}
impl std::error::Error for StoreError {}
impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}
pub type StoreResult<T> = Result<T, StoreError>;

pub struct CredentialStore {
    connection: Connection,
}

impl CredentialStore {
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        let connection = Connection::open(path)?;
        Self::configure(connection)
    }

    pub fn open_in_memory() -> StoreResult<Self> {
        let connection = Connection::open_in_memory()?;
        Self::configure(connection)
    }

    fn configure(connection: Connection) -> StoreResult<Self> {
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let _ = connection.pragma_update(None, "journal_mode", "WAL");
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "secure_delete", "ON")?;
        let mut store = Self { connection };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&mut self) -> StoreResult<()> {
        let has_meta: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta')",
            [],
            |row| row.get(0),
        )?;
        if !has_meta {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(
                "CREATE TABLE meta(schema_version INTEGER NOT NULL CHECK(schema_version > 0));
                 INSERT INTO meta(schema_version) VALUES(1);
                 CREATE TABLE credentials(
                    credential_ref TEXT PRIMARY KEY,
                    provider_id TEXT NOT NULL,
                    safe_label TEXT NULL,
                    secret BLOB NOT NULL,
                    generation INTEGER NOT NULL CHECK(generation > 0),
                    state TEXT NOT NULL CHECK(state IN ('active','revoked')),
                    created_at TEXT NOT NULL,
                    rotated_at TEXT NULL,
                    revoked_at TEXT NULL,
                    updated_at TEXT NOT NULL
                 );",
            )?;
            transaction.commit()?;
            return Ok(());
        }
        let version: u32 =
            self.connection
                .query_row("SELECT schema_version FROM meta LIMIT 1", [], |row| {
                    row.get(0)
                })?;
        if version != STORE_SCHEMA_VERSION {
            return Err(StoreError::IncompatibleSchema(version));
        }
        let count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM meta", [], |row| row.get(0))?;
        if count != 1 {
            return Err(StoreError::Invalid(
                "meta table must contain exactly one schema row",
            ));
        }
        Ok(())
    }

    pub fn integrity_check(&self) -> StoreResult<bool> {
        let result: String = self
            .connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        Ok(result == "ok")
    }

    pub fn schema_version(&self) -> StoreResult<u32> {
        Ok(self
            .connection
            .query_row("SELECT schema_version FROM meta LIMIT 1", [], |row| {
                row.get(0)
            })?)
    }

    pub fn count(&self) -> StoreResult<usize> {
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM credentials", [], |row| row.get(0))?;
        usize::try_from(count).map_err(|_| StoreError::Invalid("credential count overflow"))
    }

    pub fn list(&self) -> StoreResult<Vec<CredentialMetadata>> {
        let mut statement = self.connection.prepare(
            "SELECT credential_ref, provider_id, safe_label, generation, state, created_at, rotated_at, revoked_at, updated_at FROM credentials ORDER BY credential_ref"
        )?;
        let rows = statement.query_map([], metadata_from_row)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn show(&self, credential_ref: &str) -> StoreResult<CredentialMetadata> {
        validate_credential_ref(credential_ref)
            .map_err(|_| StoreError::Invalid("credential reference is invalid"))?;
        self.connection.query_row(
            "SELECT credential_ref, provider_id, safe_label, generation, state, created_at, rotated_at, revoked_at, updated_at FROM credentials WHERE credential_ref=?1",
            [credential_ref], metadata_from_row
        ).optional()?.ok_or(StoreError::NotFound)
    }

    pub fn load_secret(&self, credential_ref: &str) -> StoreResult<SecretMaterial> {
        let (secret, state): (Vec<u8>, String) = self
            .connection
            .query_row(
                "SELECT secret, state FROM credentials WHERE credential_ref=?1",
                [credential_ref],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or(StoreError::NotFound)?;
        if state == "revoked" {
            return Err(StoreError::Revoked);
        }
        let text = String::from_utf8(secret)
            .map_err(|_| StoreError::Invalid("stored credential is not valid UTF-8"))?;
        SecretMaterial::new(text).map_err(StoreError::Invalid)
    }

    pub fn provision(
        &mut self,
        credential_ref: &str,
        provider_id: &str,
        safe_label: Option<&str>,
        secret: &SecretMaterial,
    ) -> StoreResult<CredentialMetadata> {
        validate_credential_ref(credential_ref)
            .map_err(|_| StoreError::Invalid("credential reference is invalid"))?;
        validate_provider_id(provider_id)
            .map_err(|_| StoreError::Invalid("provider id is invalid"))?;
        if self.count()? >= MAX_CREDENTIALS {
            return Err(StoreError::Invalid("credential catalogue is full"));
        }
        let now = now_stamp();
        let transaction = self.connection.transaction()?;
        let changed = transaction.execute(
            "INSERT OR IGNORE INTO credentials(credential_ref, provider_id, safe_label, secret, generation, state, created_at, rotated_at, revoked_at, updated_at) VALUES(?1,?2,?3,?4,1,'active',?5,NULL,NULL,?5)",
            params![credential_ref, provider_id, safe_label, secret.as_bytes(), now],
        )?;
        if changed != 1 {
            return Err(StoreError::Invalid("credential reference already exists"));
        }
        transaction.commit()?;
        self.show(credential_ref)
    }

    pub fn rotate(
        &mut self,
        credential_ref: &str,
        secret: &SecretMaterial,
    ) -> StoreResult<CredentialMetadata> {
        let now = now_stamp();
        let transaction = self.connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE credentials SET secret=?2, generation=generation+1, state='active', rotated_at=?3, revoked_at=NULL, updated_at=?3 WHERE credential_ref=?1",
            params![credential_ref, secret.as_bytes(), now],
        )?;
        if changed != 1 {
            return Err(StoreError::NotFound);
        }
        transaction.commit()?;
        self.show(credential_ref)
    }

    pub fn revoke(&mut self, credential_ref: &str) -> StoreResult<CredentialMetadata> {
        let now = now_stamp();
        let transaction = self.connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE credentials SET state='revoked', revoked_at=?2, updated_at=?2 WHERE credential_ref=?1",
            params![credential_ref, now],
        )?;
        if changed != 1 {
            return Err(StoreError::NotFound);
        }
        transaction.commit()?;
        self.show(credential_ref)
    }

    pub fn delete(&mut self, credential_ref: &str) -> StoreResult<()> {
        let transaction = self.connection.transaction()?;
        let changed = transaction.execute(
            "DELETE FROM credentials WHERE credential_ref=?1",
            [credential_ref],
        )?;
        if changed != 1 {
            return Err(StoreError::NotFound);
        }
        transaction.commit()?;
        Ok(())
    }
}

fn metadata_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CredentialMetadata> {
    let state: String = row.get(4)?;
    let generation: i64 = row.get(3)?;
    let generation = u64::try_from(generation)
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(3, generation))?;
    Ok(CredentialMetadata {
        credential_ref: row.get(0)?,
        provider_id: row.get(1)?,
        safe_label: row.get(2)?,
        generation,
        state: if state == "active" {
            CredentialState::Active
        } else {
            CredentialState::Revoked
        },
        created_at: row.get(5)?,
        rotated_at: row.get(6)?,
        revoked_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn now_stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_has_locked_pragmas_and_rotation_replaces_one_generation() {
        let mut store = CredentialStore::open_in_memory().unwrap();
        let secret1 = SecretMaterial::new("secret-one".into()).unwrap();
        let secret2 = SecretMaterial::new("secret-two".into()).unwrap();
        let first = store
            .provision("openai/main", "openai", Some("Main"), &secret1)
            .unwrap();
        assert_eq!(first.generation, 1);
        let rotated = store.rotate("openai/main", &secret2).unwrap();
        assert_eq!(rotated.generation, 2);
        assert_eq!(
            store.load_secret("openai/main").unwrap().as_bytes(),
            b"secret-two"
        );
        assert!(store.integrity_check().unwrap());
        let secure_delete: i64 = store
            .connection
            .query_row("PRAGMA secure_delete", [], |row| row.get(0))
            .unwrap();
        let synchronous: i64 = store
            .connection
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(secure_delete, 1);
        assert_eq!(synchronous, 2);
    }

    #[test]
    fn rotating_revoked_reference_creates_new_active_generation() {
        let mut store = CredentialStore::open_in_memory().unwrap();
        let first = SecretMaterial::new("generation-one".into()).unwrap();
        let second = SecretMaterial::new("generation-two".into()).unwrap();
        store
            .provision("openai/main", "openai", None, &first)
            .unwrap();
        store.revoke("openai/main").unwrap();
        let rotated = store.rotate("openai/main", &second).unwrap();
        assert_eq!(rotated.generation, 2);
        assert_eq!(rotated.state, CredentialState::Active);
        assert!(rotated.revoked_at.is_none());
        assert_eq!(
            store.load_secret("openai/main").unwrap().as_bytes(),
            b"generation-two"
        );
    }

    #[test]
    fn revoked_credential_fails_secret_load_and_delete_removes_metadata() {
        let mut store = CredentialStore::open_in_memory().unwrap();
        let secret = SecretMaterial::new("secret-one".into()).unwrap();
        store
            .provision("openai/main", "openai", None, &secret)
            .unwrap();
        assert_eq!(
            store.revoke("openai/main").unwrap().state,
            CredentialState::Revoked
        );
        assert!(matches!(
            store.load_secret("openai/main"),
            Err(StoreError::Revoked)
        ));
        store.delete("openai/main").unwrap();
        assert!(matches!(
            store.show("openai/main"),
            Err(StoreError::NotFound)
        ));
    }
}
