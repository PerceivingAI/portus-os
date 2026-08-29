use portus_protocol::{
    ArtifactAvailabilityState, ArtifactCleanupAuthority, ArtifactConfidentiality, ArtifactId,
    ArtifactIntegrityKind, ArtifactLocator, ArtifactRecord, ArtifactRegistrationSpec,
    ArtifactRetentionKind, ArtifactType, Principal, ProviderResourceRef, TaskId,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    error::Error,
    fmt,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

pub const MAX_FILESYSTEM_PATH_BYTES: usize = 4096;
pub const MAX_SYNC_HASH_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const HASH_BUFFER_BYTES: usize = 1024 * 1024;
pub const MAX_SAFE_DISPLAY_NAME_BYTES: usize = 256;
pub const MAX_MEDIA_TYPE_BYTES: usize = 128;
pub const MAX_PROJECT_REF_BYTES: usize = 512;
pub const MAX_SAFE_METADATA_FIELDS: usize = 16;
pub const MAX_SAFE_METADATA_KEY_BYTES: usize = 64;
pub const MAX_SAFE_METADATA_VALUE_BYTES: usize = 512;
pub const MAX_CLEANUP_REF_BYTES: usize = 512;

#[derive(Debug)]
pub enum ArtifactError {
    Io(io::Error),
    InvalidPath(&'static str),
    InvalidSpec(&'static str),
    InvalidMetadata(&'static str),
    NotRegularFile,
    FileTooLarge { size_bytes: u64, limit_bytes: u64 },
    ChangedDuringInspection,
    ExpectedTargetMismatch,
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "artifact filesystem error: {error}"),
            Self::InvalidPath(message) => write!(f, "invalid artifact path: {message}"),
            Self::InvalidSpec(message) => write!(f, "invalid artifact registration: {message}"),
            Self::InvalidMetadata(message) => write!(f, "invalid artifact metadata: {message}"),
            Self::NotRegularFile => {
                f.write_str("artifact filesystem locator is not a regular file")
            }
            Self::FileTooLarge {
                size_bytes,
                limit_bytes,
            } => write!(
                f,
                "artifact file size {size_bytes} exceeds synchronous hashing limit {limit_bytes}"
            ),
            Self::ChangedDuringInspection => {
                f.write_str("artifact file changed while integrity metadata was being captured")
            }
            Self::ExpectedTargetMismatch => {
                f.write_str("artifact filesystem target no longer matches the registered content")
            }
        }
    }
}

impl Error for ArtifactError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ArtifactError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub type ArtifactResult<T> = Result<T, ArtifactError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemSnapshot {
    pub canonical_path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconciliationResult {
    pub availability: ArtifactAvailabilityState,
    pub integrity: ArtifactIntegrityKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesystemCleanupOutcome {
    Deleted,
    AlreadyMissing,
}

#[derive(Clone, Debug)]
pub struct FilesystemRegistrationRequest {
    pub owner: Principal,
    pub path: PathBuf,
    pub artifact_type: ArtifactType,
    pub confidentiality: ArtifactConfidentiality,
    pub retention_kind: ArtifactRetentionKind,
    pub expires_at_ms: Option<i64>,
    pub media_type: Option<String>,
    pub created_at_ms: Option<i64>,
    pub project_ref: Option<String>,
    pub safe_display_name: Option<String>,
    pub safe_metadata: BTreeMap<String, String>,
    pub source_task_id: Option<TaskId>,
    pub shared_with: Vec<Principal>,
    pub cleanup_authority: ArtifactCleanupAuthority,
    pub cleanup_ref: Option<String>,
}

impl FilesystemRegistrationRequest {
    #[must_use]
    pub fn retained(
        owner: Principal,
        path: impl Into<PathBuf>,
        artifact_type: ArtifactType,
    ) -> Self {
        Self {
            owner,
            path: path.into(),
            artifact_type,
            confidentiality: ArtifactConfidentiality::Private,
            retention_kind: ArtifactRetentionKind::Retained,
            expires_at_ms: None,
            media_type: None,
            created_at_ms: None,
            project_ref: None,
            safe_display_name: None,
            safe_metadata: BTreeMap::new(),
            source_task_id: None,
            shared_with: Vec::new(),
            cleanup_authority: ArtifactCleanupAuthority::None,
            cleanup_ref: None,
        }
    }

    #[must_use]
    pub fn diagnostic_bundle(owner: Principal, path: impl Into<PathBuf>) -> Self {
        let mut request = Self::retained(owner, path, ArtifactType::DiagnosticBundle);
        request.retention_kind = ArtifactRetentionKind::Temporary;
        request
    }
}

#[derive(Clone, Debug)]
pub struct ProviderRegistrationRequest {
    pub owner: Principal,
    pub reference: ProviderResourceRef,
    pub artifact_type: ArtifactType,
    pub confidentiality: ArtifactConfidentiality,
    pub retention_kind: ArtifactRetentionKind,
    pub expires_at_ms: Option<i64>,
    pub provider_digest_sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub media_type: Option<String>,
    pub created_at_ms: Option<i64>,
    pub project_ref: Option<String>,
    pub safe_display_name: Option<String>,
    pub safe_metadata: BTreeMap<String, String>,
    pub source_task_id: Option<TaskId>,
    pub shared_with: Vec<Principal>,
    pub cleanup_authority: ArtifactCleanupAuthority,
    pub cleanup_ref: Option<String>,
}

pub fn prepare_filesystem_registration(
    request: FilesystemRegistrationRequest,
    registered_at_ms: i64,
) -> ArtifactResult<ArtifactRegistrationSpec> {
    let snapshot = inspect_filesystem(&request.path)?;
    let spec = ArtifactRegistrationSpec {
        artifact_id: ArtifactId::new(),
        owner: request.owner,
        artifact_type: request.artifact_type,
        confidentiality: request.confidentiality,
        retention_kind: request.retention_kind,
        expires_at_ms: request.expires_at_ms,
        locator: ArtifactLocator::Filesystem {
            path: snapshot.canonical_path,
        },
        integrity_kind: ArtifactIntegrityKind::Verified,
        sha256: Some(snapshot.sha256),
        size_bytes: Some(snapshot.size_bytes),
        media_type: request.media_type,
        created_at_ms: request.created_at_ms.unwrap_or(registered_at_ms),
        registered_at_ms,
        project_ref: request.project_ref,
        safe_display_name: request.safe_display_name,
        safe_metadata: request.safe_metadata,
        source_task_id: request.source_task_id,
        shared_with: request.shared_with,
        cleanup_authority: request.cleanup_authority,
        cleanup_ref: request.cleanup_ref,
    };
    validate_registration_spec(&spec)?;
    Ok(spec)
}

pub fn prepare_provider_registration(
    request: ProviderRegistrationRequest,
    registered_at_ms: i64,
) -> ArtifactResult<ArtifactRegistrationSpec> {
    let integrity_kind = if request.provider_digest_sha256.is_some() {
        ArtifactIntegrityKind::Verified
    } else {
        ArtifactIntegrityKind::ProviderAuthoritative
    };
    let spec = ArtifactRegistrationSpec {
        artifact_id: ArtifactId::new(),
        owner: request.owner,
        artifact_type: request.artifact_type,
        confidentiality: request.confidentiality,
        retention_kind: request.retention_kind,
        expires_at_ms: request.expires_at_ms,
        locator: ArtifactLocator::ProviderResource {
            reference: request.reference,
        },
        integrity_kind,
        sha256: request.provider_digest_sha256,
        size_bytes: request.size_bytes,
        media_type: request.media_type,
        created_at_ms: request.created_at_ms.unwrap_or(registered_at_ms),
        registered_at_ms,
        project_ref: request.project_ref,
        safe_display_name: request.safe_display_name,
        safe_metadata: request.safe_metadata,
        source_task_id: request.source_task_id,
        shared_with: request.shared_with,
        cleanup_authority: request.cleanup_authority,
        cleanup_ref: request.cleanup_ref,
    };
    validate_registration_spec(&spec)?;
    Ok(spec)
}

pub fn inspect_filesystem(path: &Path) -> ArtifactResult<FilesystemSnapshot> {
    if !path.is_absolute() {
        return Err(ArtifactError::InvalidPath("path must be absolute"));
    }
    let canonical = fs::canonicalize(path)?;
    let canonical_text = canonical.to_str().ok_or(ArtifactError::InvalidPath(
        "canonical path must be valid UTF-8",
    ))?;
    if canonical_text.len() > MAX_FILESYSTEM_PATH_BYTES {
        return Err(ArtifactError::InvalidPath(
            "canonical path exceeds bounded length",
        ));
    }
    let before = fs::metadata(&canonical)?;
    if !before.is_file() {
        return Err(ArtifactError::NotRegularFile);
    }
    if before.len() > MAX_SYNC_HASH_BYTES {
        return Err(ArtifactError::FileTooLarge {
            size_bytes: before.len(),
            limit_bytes: MAX_SYNC_HASH_BYTES,
        });
    }

    let mut file = File::open(&canonical)?;
    let mut context = Sha256::new();
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        context.update(&buffer[..count]);
    }
    let after = fs::metadata(&canonical)?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(ArtifactError::ChangedDuringInspection);
    }
    let digest = context.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    Ok(FilesystemSnapshot {
        canonical_path: canonical_text.to_owned(),
        sha256: encoded,
        size_bytes: before.len(),
    })
}

pub fn reconcile_filesystem(record: &ArtifactRecord) -> ArtifactResult<ReconciliationResult> {
    let ArtifactLocator::Filesystem { path } = &record.locator else {
        return Err(ArtifactError::InvalidSpec(
            "artifact is not filesystem-backed",
        ));
    };
    let snapshot = match inspect_filesystem(Path::new(path)) {
        Ok(snapshot) => snapshot,
        Err(ArtifactError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ReconciliationResult {
                availability: ArtifactAvailabilityState::Missing,
                integrity: record.integrity_kind,
            });
        }
        Err(ArtifactError::Io(_)) => {
            return Ok(ReconciliationResult {
                availability: ArtifactAvailabilityState::Unavailable,
                integrity: record.integrity_kind,
            });
        }
        Err(error) => return Err(error),
    };
    let matches = record.sha256.as_deref() == Some(snapshot.sha256.as_str())
        && record.size_bytes == Some(snapshot.size_bytes)
        && snapshot.canonical_path == *path;
    Ok(ReconciliationResult {
        availability: ArtifactAvailabilityState::Available,
        integrity: if matches {
            ArtifactIntegrityKind::Verified
        } else {
            ArtifactIntegrityKind::Mismatch
        },
    })
}

pub fn delete_expected_filesystem_content(
    record: &ArtifactRecord,
) -> ArtifactResult<FilesystemCleanupOutcome> {
    let ArtifactLocator::Filesystem { path } = &record.locator else {
        return Err(ArtifactError::InvalidSpec(
            "artifact is not filesystem-backed",
        ));
    };
    let snapshot = match inspect_filesystem(Path::new(path)) {
        Ok(snapshot) => snapshot,
        Err(ArtifactError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(FilesystemCleanupOutcome::AlreadyMissing);
        }
        Err(error) => return Err(error),
    };
    if snapshot.canonical_path != *path
        || record.sha256.as_deref() != Some(snapshot.sha256.as_str())
        || record.size_bytes != Some(snapshot.size_bytes)
    {
        return Err(ArtifactError::ExpectedTargetMismatch);
    }
    fs::remove_file(Path::new(path))?;
    Ok(FilesystemCleanupOutcome::Deleted)
}

pub fn validate_registration_spec(spec: &ArtifactRegistrationSpec) -> ArtifactResult<()> {
    if spec.registered_at_ms < 0 || spec.created_at_ms < 0 {
        return Err(ArtifactError::InvalidSpec(
            "timestamps must not be negative",
        ));
    }
    match spec.retention_kind {
        ArtifactRetentionKind::Until if spec.expires_at_ms.is_none() => {
            return Err(ArtifactError::InvalidSpec(
                "until retention requires expiry",
            ));
        }
        ArtifactRetentionKind::Temporary | ArtifactRetentionKind::Retained
            if spec.expires_at_ms.is_some() =>
        {
            return Err(ArtifactError::InvalidSpec(
                "expiry is valid only for until retention",
            ));
        }
        _ => {}
    }
    if spec.confidentiality != ArtifactConfidentiality::Shared && !spec.shared_with.is_empty() {
        return Err(ArtifactError::InvalidSpec(
            "explicit shares require shared confidentiality",
        ));
    }
    let unique = spec.shared_with.iter().copied().collect::<HashSet<_>>();
    if unique.len() != spec.shared_with.len() || unique.len() > 64 {
        return Err(ArtifactError::InvalidSpec(
            "shared principals must be unique and bounded",
        ));
    }
    if let Some(value) = &spec.safe_display_name {
        validate_single_line(value, MAX_SAFE_DISPLAY_NAME_BYTES, "safe display name")?;
    }
    if let Some(value) = &spec.media_type {
        validate_single_line(value, MAX_MEDIA_TYPE_BYTES, "media type")?;
    }
    if let Some(value) = &spec.project_ref {
        validate_single_line(value, MAX_PROJECT_REF_BYTES, "project reference")?;
    }
    if let Some(value) = &spec.cleanup_ref {
        validate_single_line(value, MAX_CLEANUP_REF_BYTES, "cleanup reference")?;
    }
    match spec.cleanup_authority {
        ArtifactCleanupAuthority::None if spec.cleanup_ref.is_some() => {
            return Err(ArtifactError::InvalidSpec(
                "cleanup reference requires cleanup authority",
            ));
        }
        ArtifactCleanupAuthority::Task | ArtifactCleanupAuthority::Provider
            if spec.cleanup_ref.is_none() =>
        {
            return Err(ArtifactError::InvalidSpec(
                "task/provider cleanup authority requires a reference",
            ));
        }
        _ => {}
    }
    if spec.safe_metadata.len() > MAX_SAFE_METADATA_FIELDS {
        return Err(ArtifactError::InvalidMetadata(
            "too many safe metadata fields",
        ));
    }
    for (key, value) in &spec.safe_metadata {
        if key.is_empty() || key.len() > MAX_SAFE_METADATA_KEY_BYTES || !is_safe_key(key) {
            return Err(ArtifactError::InvalidMetadata(
                "metadata key is unsafe or invalid",
            ));
        }
        validate_single_line(value, MAX_SAFE_METADATA_VALUE_BYTES, "metadata value")?;
    }
    if let Some(sha256) = &spec.sha256 {
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ArtifactError::InvalidSpec(
                "SHA-256 must be lowercase hexadecimal",
            ));
        }
    }
    match &spec.locator {
        ArtifactLocator::Filesystem { path } => {
            if spec.cleanup_authority == ArtifactCleanupAuthority::Provider {
                return Err(ArtifactError::InvalidSpec(
                    "provider cleanup authority is invalid for filesystem content",
                ));
            }
            if path.is_empty()
                || path.len() > MAX_FILESYSTEM_PATH_BYTES
                || !Path::new(path).is_absolute()
            {
                return Err(ArtifactError::InvalidPath(
                    "filesystem locator must be a bounded absolute path",
                ));
            }
            if spec.integrity_kind != ArtifactIntegrityKind::Verified
                || spec.sha256.is_none()
                || spec.size_bytes.is_none()
            {
                return Err(ArtifactError::InvalidSpec(
                    "filesystem registration requires verified SHA-256 and size",
                ));
            }
        }
        ArtifactLocator::ProviderResource { .. } => {
            if matches!(
                spec.cleanup_authority,
                ArtifactCleanupAuthority::Portus | ArtifactCleanupAuthority::Task
            ) {
                return Err(ArtifactError::InvalidSpec(
                    "provider resources may only use provider cleanup authority",
                ));
            }
            if matches!(
                spec.integrity_kind,
                ArtifactIntegrityKind::Verified | ArtifactIntegrityKind::Mismatch
            ) && spec.sha256.is_none()
            {
                return Err(ArtifactError::InvalidSpec(
                    "verified provider integrity requires SHA-256",
                ));
            }
        }
    }
    Ok(())
}

fn validate_single_line(value: &str, max_bytes: usize, field: &'static str) -> ArtifactResult<()> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.contains(['\r', '\n'])
        || secret_like_text(value)
    {
        return Err(ArtifactError::InvalidMetadata(field));
    }
    Ok(())
}

fn secret_like_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "authorization:",
        "bearer ",
        "api_key=",
        "api_key =",
        "apikey=",
        "password=",
        "password =",
        "secret=",
        "secret =",
        "token=",
        "token =",
        "private_key",
        "-----begin private key-----",
        "-----begin openssh private key-----",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn is_safe_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    ![
        "authorization",
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "cookie",
        "private_key",
        "credential",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("portus-artifact-{label}-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn filesystem_registration_is_canonical_and_sha256_correct() {
        let dir = temp_dir("hash");
        let path = dir.join("report.txt");
        fs::write(&path, b"abc").unwrap();
        let spec = prepare_filesystem_registration(
            FilesystemRegistrationRequest::retained(
                Principal::new(1000, 1000),
                &path,
                ArtifactType::Report,
            ),
            20,
        )
        .unwrap();
        assert_eq!(
            spec.sha256.as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(spec.size_bytes, Some(3));
        assert!(matches!(spec.locator, ArtifactLocator::Filesystem { .. }));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn changed_file_is_mismatch_without_rewriting_registered_digest() {
        let dir = temp_dir("mismatch");
        let path = dir.join("result.txt");
        fs::write(&path, b"first").unwrap();
        let spec = prepare_filesystem_registration(
            FilesystemRegistrationRequest::retained(
                Principal::new(1000, 1000),
                &path,
                ArtifactType::File,
            ),
            20,
        )
        .unwrap();
        let original = spec.sha256.clone();
        let record = ArtifactRecord {
            artifact_id: spec.artifact_id,
            owner: spec.owner,
            artifact_type: spec.artifact_type,
            confidentiality: spec.confidentiality,
            retention_kind: spec.retention_kind,
            expires_at_ms: spec.expires_at_ms,
            availability_state: ArtifactAvailabilityState::Available,
            locator: spec.locator,
            integrity_kind: spec.integrity_kind,
            sha256: spec.sha256,
            size_bytes: spec.size_bytes,
            media_type: spec.media_type,
            created_at_ms: spec.created_at_ms,
            registered_at_ms: spec.registered_at_ms,
            project_ref: spec.project_ref,
            safe_display_name: spec.safe_display_name,
            safe_metadata: spec.safe_metadata,
            last_verified_at_ms: Some(20),
            removed_at_ms: None,
            cleanup_authority: spec.cleanup_authority,
            cleanup_ref: spec.cleanup_ref,
        };
        fs::write(&path, b"second").unwrap();
        let result = reconcile_filesystem(&record).unwrap();
        assert_eq!(result.integrity, ArtifactIntegrityKind::Mismatch);
        assert_eq!(record.sha256, original);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn diagnostic_bundle_defaults_private_and_temporary() {
        let request = FilesystemRegistrationRequest::diagnostic_bundle(
            Principal::new(1000, 1000),
            PathBuf::from("/tmp/bundle.json"),
        );
        assert_eq!(request.confidentiality, ArtifactConfidentiality::Private);
        assert_eq!(request.retention_kind, ArtifactRetentionKind::Temporary);
    }

    #[test]
    fn cleanup_authority_must_match_locator_ownership() {
        let dir = temp_dir("cleanup-owner");
        let path = dir.join("report.txt");
        fs::write(&path, b"safe").unwrap();
        let mut request = FilesystemRegistrationRequest::retained(
            Principal::new(1000, 1000),
            &path,
            ArtifactType::Report,
        );
        request.cleanup_authority = ArtifactCleanupAuthority::Provider;
        request.cleanup_ref = Some("provider-owned".into());
        assert!(matches!(
            prepare_filesystem_registration(request, 1),
            Err(ArtifactError::InvalidSpec(_))
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn secret_like_metadata_is_rejected() {
        let dir = temp_dir("secret");
        let path = dir.join("report.txt");
        fs::write(&path, b"safe").unwrap();
        let mut request = FilesystemRegistrationRequest::retained(
            Principal::new(1000, 1000),
            &path,
            ArtifactType::Report,
        );
        request
            .safe_metadata
            .insert("api_key".into(), "do-not-store".into());
        assert!(matches!(
            prepare_filesystem_registration(request, 1),
            Err(ArtifactError::InvalidMetadata(_))
        ));
        let mut value_request = FilesystemRegistrationRequest::retained(
            Principal::new(1000, 1000),
            &path,
            ArtifactType::Report,
        );
        value_request
            .safe_metadata
            .insert("note".into(), "Authorization: Bearer do-not-store".into());
        assert!(matches!(
            prepare_filesystem_registration(value_request, 1),
            Err(ArtifactError::InvalidMetadata(_))
        ));
        fs::remove_dir_all(dir).unwrap();
    }
}
