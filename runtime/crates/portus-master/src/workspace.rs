use crate::{LaunchContext, MasterLaunchError, MasterLaunchResult};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub const MASTER_AGENTS_CONTENT: &str = include_str!("../../../integrations/master/AGENTS.md");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedWorkspace {
    pub path: PathBuf,
    pub agents_path: PathBuf,
    pub agents_created: bool,
}

pub fn prepare_workspace(context: &LaunchContext) -> MasterLaunchResult<PreparedWorkspace> {
    context.validate()?;
    let workspace_root = &context.layout.workspace_root;
    validate_existing_directory(workspace_root, None)?;

    let user_workspace = context.user_workspace();
    ensure_user_directory(&user_workspace, context.identity.uid)?;
    let master_workspace = context.master_workspace();
    ensure_user_directory(&master_workspace, context.identity.uid)?;

    let agents_path = master_workspace.join("AGENTS.md");
    let agents_created = ensure_agents_file(&agents_path, context.identity.uid)?;
    Ok(PreparedWorkspace {
        path: master_workspace,
        agents_path,
        agents_created,
    })
}

fn validate_existing_directory(path: &Path, expected_uid: Option<u32>) -> MasterLaunchResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            MasterLaunchError::WorkspaceRootUnavailable(path.to_path_buf())
        } else {
            MasterLaunchError::Io {
                operation: "inspect workspace directory",
                path: path.to_path_buf(),
                source: error,
            }
        }
    })?;
    if metadata.file_type().is_symlink() {
        return Err(MasterLaunchError::UnsafeSymlink(path.to_path_buf()));
    }
    if !metadata.is_dir() {
        return Err(MasterLaunchError::UnsafeFileType(path.to_path_buf()));
    }
    validate_owner(path, &metadata, expected_uid)
}

fn ensure_user_directory(path: &Path, expected_uid: Option<u32>) -> MasterLaunchResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(MasterLaunchError::UnsafeSymlink(path.to_path_buf()));
            }
            if !metadata.is_dir() {
                return Err(MasterLaunchError::UnsafeFileType(path.to_path_buf()));
            }
            validate_owner(path, &metadata, expected_uid)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Err(source) = fs::create_dir(path)
                && source.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(MasterLaunchError::Io {
                    operation: "create Master workspace directory",
                    path: path.to_path_buf(),
                    source,
                });
            }
            let metadata = fs::symlink_metadata(path).map_err(|source| MasterLaunchError::Io {
                operation: "inspect created Master workspace directory",
                path: path.to_path_buf(),
                source,
            })?;
            if metadata.file_type().is_symlink() {
                return Err(MasterLaunchError::UnsafeSymlink(path.to_path_buf()));
            }
            if !metadata.is_dir() {
                return Err(MasterLaunchError::UnsafeFileType(path.to_path_buf()));
            }
            validate_owner(path, &metadata, expected_uid)
        }
        Err(source) => Err(MasterLaunchError::Io {
            operation: "inspect Master workspace directory",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn ensure_agents_file(path: &Path, expected_uid: Option<u32>) -> MasterLaunchResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(MasterLaunchError::UnsafeSymlink(path.to_path_buf()));
            }
            if !metadata.is_file() {
                return Err(MasterLaunchError::UnsafeFileType(path.to_path_buf()));
            }
            validate_owner(path, &metadata, expected_uid)?;
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(file) => file,
                Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                    return ensure_agents_file(path, expected_uid).map(|_| false);
                }
                Err(source) => {
                    return Err(MasterLaunchError::Io {
                        operation: "create Master AGENTS.md",
                        path: path.to_path_buf(),
                        source,
                    });
                }
            };
            if let Err(source) = file
                .write_all(MASTER_AGENTS_CONTENT.as_bytes())
                .and_then(|()| file.sync_all())
            {
                drop(file);
                let _ = fs::remove_file(path);
                return Err(MasterLaunchError::Io {
                    operation: "write Master AGENTS.md",
                    path: path.to_path_buf(),
                    source,
                });
            }
            let metadata = fs::symlink_metadata(path).map_err(|source| MasterLaunchError::Io {
                operation: "inspect created Master AGENTS.md",
                path: path.to_path_buf(),
                source,
            })?;
            validate_owner(path, &metadata, expected_uid)?;
            Ok(true)
        }
        Err(source) => Err(MasterLaunchError::Io {
            operation: "inspect Master AGENTS.md",
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(unix)]
fn validate_owner(
    path: &Path,
    metadata: &fs::Metadata,
    expected_uid: Option<u32>,
) -> MasterLaunchResult<()> {
    use std::os::unix::fs::MetadataExt;
    if let Some(expected_uid) = expected_uid {
        let found_uid = metadata.uid();
        if found_uid != expected_uid {
            return Err(MasterLaunchError::OwnershipMismatch {
                path: path.to_path_buf(),
                expected_uid,
                found_uid,
            });
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_owner(
    _path: &Path,
    _metadata: &fs::Metadata,
    _expected_uid: Option<u32>,
) -> MasterLaunchResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LaunchIdentity, LaunchLayout};
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    fn fixture_context() -> (LaunchContext, PathBuf) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "portus-master-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let context = LaunchContext {
            identity: LaunchIdentity {
                username: "demo".into(),
                uid: None,
                gid: None,
                home: root.join("home"),
                shell: PathBuf::from("shell"),
            },
            layout: LaunchLayout {
                workspace_root: root.clone(),
                ..LaunchLayout::default()
            },
        };
        (context, root)
    }

    #[test]
    fn first_prepare_creates_workspace_and_charter_without_overwriting_it_later() {
        let (context, root) = fixture_context();
        let first = prepare_workspace(&context).unwrap();
        assert!(first.agents_created);
        assert_eq!(
            fs::read_to_string(&first.agents_path).unwrap(),
            MASTER_AGENTS_CONTENT
        );

        fs::write(&first.agents_path, "user-controlled charter\n").unwrap();
        let second = prepare_workspace(&context).unwrap();
        assert!(!second.agents_created);
        assert_eq!(
            fs::read_to_string(&second.agents_path).unwrap(),
            "user-controlled charter\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_workspace_root_fails_instead_of_inventing_system_layout() {
        let (mut context, root) = fixture_context();
        fs::remove_dir_all(&root).unwrap();
        context.layout.workspace_root = root.clone();
        assert!(matches!(
            prepare_workspace(&context),
            Err(MasterLaunchError::WorkspaceRootUnavailable(path)) if path == root
        ));
    }
}
