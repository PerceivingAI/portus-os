//! Bounded, secret-safe audit sink for PortusOS first-party components.
//!
//! Audit is intentionally separate from `portus.db` operational state. The
//! record contract contains only typed/allowlisted metadata and deliberately
//! exposes no generic payload, headers, argv, environment, or log-message map.

use portus_protocol::{AuditActorKind, AuditRecord};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

pub const CANONICAL_AUDIT_PATH: &str = "/var/log/portus/audit/portusd.jsonl";
pub const DEFAULT_MAX_AUDIT_FILE_BYTES: u64 = 1024 * 1024;
pub const DEFAULT_AUDIT_ARCHIVES: usize = 4;
pub const MAX_AUDIT_RECORD_BYTES: usize = 4096;
pub const MAX_AUDIT_ACTION_BYTES: usize = 128;
pub const MAX_AUDIT_REASON_BYTES: usize = 256;
pub const MAX_AUDIT_TARGET_BYTES: usize = 512;

#[derive(Debug)]
pub enum AuditError {
    Io(io::Error),
    InvalidRecord(&'static str),
    RecordTooLarge,
    InvalidConfiguration(&'static str),
}

impl std::fmt::Display for AuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "audit I/O error: {error}"),
            Self::InvalidRecord(message) => write!(f, "invalid audit record: {message}"),
            Self::RecordTooLarge => f.write_str("audit record exceeds bounded size"),
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid audit configuration: {message}")
            }
        }
    }
}

impl std::error::Error for AuditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for AuditError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub type AuditResult<T> = Result<T, AuditError>;

pub trait AuditSink: Send + Sync {
    fn record(&self, record: &AuditRecord) -> AuditResult<()>;
}

#[derive(Default)]
pub struct NullAuditSink;

impl AuditSink for NullAuditSink {
    fn record(&self, record: &AuditRecord) -> AuditResult<()> {
        validate_record(record)
    }
}

pub struct FileAuditSink {
    path: PathBuf,
    max_file_bytes: u64,
    archives: usize,
    lock: Mutex<()>,
}

impl FileAuditSink {
    pub fn open(path: impl Into<PathBuf>) -> AuditResult<Self> {
        Self::open_with_limits(path, DEFAULT_MAX_AUDIT_FILE_BYTES, DEFAULT_AUDIT_ARCHIVES)
    }

    pub fn open_with_limits(
        path: impl Into<PathBuf>,
        max_file_bytes: u64,
        archives: usize,
    ) -> AuditResult<Self> {
        if max_file_bytes < MAX_AUDIT_RECORD_BYTES as u64 {
            return Err(AuditError::InvalidConfiguration(
                "audit file limit must fit one maximum record",
            ));
        }
        let path = path.into();
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(AuditError::InvalidConfiguration("audit path has no parent"))?;
        if !parent.is_dir() {
            return Err(AuditError::InvalidConfiguration(
                "audit parent directory does not exist",
            ));
        }
        OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            path,
            max_file_bytes,
            archives,
            lock: Mutex::new(()),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn rotate_if_needed(&self, incoming: u64) -> AuditResult<()> {
        let current = fs::metadata(&self.path).map_or(0, |metadata| metadata.len());
        if current.saturating_add(incoming) <= self.max_file_bytes {
            return Ok(());
        }
        if self.archives == 0 {
            File::create(&self.path)?;
            return Ok(());
        }
        let oldest = archive_path(&self.path, self.archives);
        if oldest.exists() {
            fs::remove_file(oldest)?;
        }
        for index in (1..self.archives).rev() {
            let from = archive_path(&self.path, index);
            if from.exists() {
                fs::rename(from, archive_path(&self.path, index + 1))?;
            }
        }
        if self.path.exists() {
            fs::rename(&self.path, archive_path(&self.path, 1))?;
        }
        File::create(&self.path)?;
        Ok(())
    }
}

impl AuditSink for FileAuditSink {
    fn record(&self, record: &AuditRecord) -> AuditResult<()> {
        validate_record(record)?;
        let mut encoded = serde_json::to_vec(record)
            .map_err(|_| AuditError::InvalidRecord("record is not serializable"))?;
        encoded.push(b'\n');
        if encoded.len() > MAX_AUDIT_RECORD_BYTES {
            return Err(AuditError::RecordTooLarge);
        }
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.rotate_if_needed(encoded.len() as u64)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(&encoded)?;
        file.flush()?;
        Ok(())
    }
}

fn archive_path(path: &Path, index: usize) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(format!(".{index}"));
    PathBuf::from(value)
}

pub fn validate_record(record: &AuditRecord) -> AuditResult<()> {
    if record.schema_version != AuditRecord::SCHEMA_VERSION {
        return Err(AuditError::InvalidRecord(
            "unsupported record schema version",
        ));
    }
    if !record.actor.is_valid() {
        return Err(AuditError::InvalidRecord("actor shape is inconsistent"));
    }
    validate_safe_text(&record.action, MAX_AUDIT_ACTION_BYTES, "action")?;
    validate_safe_text(&record.reason_code, MAX_AUDIT_REASON_BYTES, "reason")?;
    if let Some(target) = record.target_ref.as_deref() {
        validate_safe_text(target, MAX_AUDIT_TARGET_BYTES, "target")?;
    }
    if record.actor.kind == AuditActorKind::System && record.actor.principal.is_some() {
        return Err(AuditError::InvalidRecord(
            "system actor cannot contain principal",
        ));
    }
    Ok(())
}

fn validate_safe_text(value: &str, max_bytes: usize, field: &'static str) -> AuditResult<()> {
    if value.trim().is_empty() || value.len() > max_bytes || value.contains(['\n', '\r', '\0']) {
        return Err(AuditError::InvalidRecord(field));
    }
    let lower = value.to_ascii_lowercase();
    for marker in [
        "authorization:",
        "bearer ",
        "api_key=",
        "apikey=",
        "password=",
        "secret=",
        "token=",
        "private_key",
    ] {
        if lower.contains(marker) {
            return Err(AuditError::InvalidRecord("secret-like material rejected"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_protocol::{AuditActor, AuditDomain, AuditResult as RecordResult, Principal};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn record(sequence: usize) -> AuditRecord {
        let mut record = AuditRecord::new(
            AuditActor::principal(Principal::new(1000, 1000)),
            AuditDomain::Task,
            "task.cancel",
            RecordResult::Succeeded,
            "cancellation_confirmed",
            sequence as i64,
        );
        record.target_ref = Some(format!("task:{sequence}"));
        record
    }

    fn temp_dir() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("portus-audit-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn secret_like_metadata_is_rejected_before_serialization() {
        let mut unsafe_record = record(1);
        unsafe_record.target_ref = Some("Authorization: Bearer should-not-appear".into());
        assert!(matches!(
            validate_record(&unsafe_record),
            Err(AuditError::InvalidRecord("secret-like material rejected"))
        ));
    }

    #[test]
    fn bounded_file_sink_rotates_and_never_grows_unbounded() {
        let dir = temp_dir();
        let path = dir.join("audit.jsonl");
        let sink = FileAuditSink::open_with_limits(&path, 4096, 2).unwrap();
        for sequence in 0..80 {
            sink.record(&record(sequence)).unwrap();
        }
        assert!(fs::metadata(&path).unwrap().len() <= 4096);
        assert!(archive_path(&path, 1).is_file());
        assert!(archive_path(&path, 2).is_file());
        assert!(!archive_path(&path, 3).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn audit_json_contains_no_generic_payload_field() {
        let encoded = serde_json::to_string(&record(1)).unwrap();
        assert!(!encoded.contains("payload"));
        assert!(!encoded.contains("stdout"));
        assert!(!encoded.contains("stderr"));
        assert!(!encoded.contains("environment"));
    }
}
