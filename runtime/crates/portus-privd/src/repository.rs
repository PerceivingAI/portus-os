#[cfg(target_os = "linux")]
use portus_policy::serialize_subject;
use portus_policy::{PolicyError, PolicyPaths, PolicyResult, SubjectPolicy};

pub trait PolicyRepository: Send + Sync {
    fn commit_subject(&self, subject: &SubjectPolicy) -> PolicyResult<()>;
}

#[derive(Clone, Debug)]
pub struct FilesystemPolicyRepository {
    paths: PolicyPaths,
}

impl FilesystemPolicyRepository {
    #[must_use]
    pub const fn new(paths: PolicyPaths) -> Self {
        Self { paths }
    }
}

#[cfg(target_os = "linux")]
impl PolicyRepository for FilesystemPolicyRepository {
    fn commit_subject(&self, subject: &SubjectPolicy) -> PolicyResult<()> {
        use std::{
            fs::{self, OpenOptions},
            io::Write,
            os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            sync::atomic::{AtomicU64, Ordering},
        };
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let dir_meta = fs::symlink_metadata(&self.paths.subjects_dir)?;
        if dir_meta.file_type().is_symlink()
            || !dir_meta.is_dir()
            || dir_meta.uid() != 0
            || dir_meta.permissions().mode() & 0o022 != 0
        {
            return Err(PolicyError::Permission(
                "subjects directory is not trusted root-owned policy material".into(),
            ));
        }
        let encoded = serialize_subject(subject)?;
        let temp = self.paths.subjects_dir.join(format!(
            ".{}.{}.{}.tmp",
            subject.uid,
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&temp)?;
        let target = self.paths.subject_path(subject.uid);
        if let Err(error) = (|| -> std::io::Result<()> {
            file.write_all(encoded.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temp, &target)?;
            let directory = OpenOptions::new()
                .read(true)
                .open(&self.paths.subjects_dir)?;
            directory.sync_all()?;
            Ok(())
        })() {
            let _ = fs::remove_file(&temp);
            return Err(PolicyError::Io(error));
        }
        // Re-read the committed bytes before allowing the in-memory policy
        // snapshot to advance. This verifies that the durable file is still a
        // valid v1 subject and that its embedded identity matches its filename.
        let committed = fs::read_to_string(&target)?;
        let decoded: SubjectPolicy = toml::from_str(&committed).map_err(|_| {
            PolicyError::Parse("committed subject policy failed revalidation".into())
        })?;
        if decoded.uid != subject.uid {
            return Err(PolicyError::Invalid(
                "committed subject identity changed during policy update".into(),
            ));
        }
        if serialize_subject(&decoded)? != encoded {
            return Err(PolicyError::Invalid(
                "committed subject policy does not match validated candidate".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
impl PolicyRepository for FilesystemPolicyRepository {
    fn commit_subject(&self, _subject: &SubjectPolicy) -> PolicyResult<()> {
        let _ = &self.paths;
        Err(PolicyError::UnsupportedPlatform)
    }
}
