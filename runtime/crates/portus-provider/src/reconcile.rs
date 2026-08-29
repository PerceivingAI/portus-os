use crate::{ProviderError, ProviderManifest, ProviderResult};
use portus_protocol::ProviderRegistrationId;
use portus_state::{PortusState, ProviderReconcileResult};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const CANONICAL_MANIFEST_DIR: &str = "/etc/portus/capabilities";
pub const MAX_MANIFESTS: usize = 128;
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestTrust {
    RootOwnedSystem,
    PretrustedFixture,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconcileReport {
    pub active: Vec<ProviderRegistrationId>,
    pub created: Vec<ProviderRegistrationId>,
    pub removed: Vec<ProviderRegistrationId>,
}

pub fn reconcile_directory(
    state: &mut PortusState,
    directory: impl AsRef<Path>,
    trust: ManifestTrust,
) -> ProviderResult<ReconcileReport> {
    reconcile_directory_at(state, directory, trust, unix_time_ms())
}

pub fn reconcile_directory_at(
    state: &mut PortusState,
    directory: impl AsRef<Path>,
    trust: ManifestTrust,
    now_ms: i64,
) -> ProviderResult<ReconcileReport> {
    let directory = directory.as_ref();
    if !directory.exists() {
        return Err(ProviderError::UntrustedPath {
            path: directory.display().to_string(),
            message: "provider manifest directory does not exist".into(),
        });
    }
    validate_trust(directory, trust)?;
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    paths.retain(|path| path.extension().and_then(|value| value.to_str()) == Some("toml"));
    if paths.len() > MAX_MANIFESTS {
        return Err(ProviderError::TooManyManifests {
            limit: MAX_MANIFESTS,
        });
    }

    let mut parsed = Vec::with_capacity(paths.len());
    let mut provider_types = HashSet::new();
    for path in paths {
        validate_manifest_file(&path, trust)?;
        let metadata = fs::metadata(&path)?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| ProviderError::InvalidManifest {
                file: path.display().to_string(),
                message: "manifest filename must be valid UTF-8".into(),
            })?;
        if metadata.len() > MAX_MANIFEST_BYTES {
            return Err(ProviderError::ManifestTooLarge {
                file: file_name.into(),
                limit: MAX_MANIFEST_BYTES,
            });
        }
        let contents = fs::read_to_string(&path)?;
        let manifest = ProviderManifest::parse(file_name, &contents)?;
        if !provider_types.insert(manifest.provider.provider_type.clone()) {
            return Err(ProviderError::InvalidManifest {
                file: file_name.into(),
                message: "duplicate provider type in reconciliation set".into(),
            });
        }
        parsed.push((file_name.to_string(), manifest));
    }

    let mut active = Vec::with_capacity(parsed.len());
    let mut created = Vec::new();
    let mut seen_manifest_ids = Vec::with_capacity(parsed.len());
    for (manifest_id, manifest) in parsed {
        let spec = manifest.to_system_registration_spec(manifest_id.clone())?;
        let ProviderReconcileResult {
            provider_id,
            created: was_created,
        } = state.reconcile_provider_registration(&spec, now_ms)?;
        if was_created {
            created.push(provider_id);
        }
        active.push(provider_id);
        seen_manifest_ids.push(manifest_id);
    }
    let removed = state.tombstone_missing_system_providers(&seen_manifest_ids, now_ms)?;
    Ok(ReconcileReport {
        active,
        created,
        removed,
    })
}

fn validate_manifest_file(path: &Path, trust: ManifestTrust) -> ProviderResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(ProviderError::UntrustedPath {
            path: path.display().to_string(),
            message: "manifest must be a regular file and not a symlink".into(),
        });
    }
    validate_metadata_trust(path, &metadata, trust)
}

fn validate_trust(path: &Path, trust: ManifestTrust) -> ProviderResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(ProviderError::UntrustedPath {
            path: path.display().to_string(),
            message: "manifest root must be a real directory and not a symlink".into(),
        });
    }
    validate_metadata_trust(path, &metadata, trust)
}

#[cfg(target_os = "linux")]
fn validate_metadata_trust(
    path: &Path,
    metadata: &fs::Metadata,
    trust: ManifestTrust,
) -> ProviderResult<()> {
    use std::os::unix::fs::MetadataExt;
    if trust == ManifestTrust::PretrustedFixture {
        return Ok(());
    }
    if metadata.uid() != 0 {
        return Err(ProviderError::UntrustedPath {
            path: path.display().to_string(),
            message: "production manifest path must be owned by UID 0".into(),
        });
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(ProviderError::UntrustedPath {
            path: path.display().to_string(),
            message: "production manifest path must not be group/world writable".into(),
        });
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn validate_metadata_trust(
    path: &Path,
    _metadata: &fs::Metadata,
    trust: ManifestTrust,
) -> ProviderResult<()> {
    if trust == ManifestTrust::PretrustedFixture {
        Ok(())
    } else {
        Err(ProviderError::UntrustedPath {
            path: path.display().to_string(),
            message: "root-owned production manifest validation requires Linux".into(),
        })
    }
}

fn unix_time_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

pub fn fixture_manifest_path(root: impl AsRef<Path>, provider_type: &str) -> PathBuf {
    root.as_ref().join(format!("{provider_type}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_protocol::{
        Principal, ProviderResourceId, ProviderResourceRef, ResourceType, TaskId,
    };
    use std::fs;

    struct Fixture {
        dir: PathBuf,
        manifests: PathBuf,
        state_path: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("portus-provider-{}", TaskId::new()));
            let manifests = dir.join("manifests");
            fs::create_dir_all(&manifests).unwrap();
            let state_path = dir.join("portus.db");
            Self {
                dir,
                manifests,
                state_path,
            }
        }

        fn write(&self, provider_type: &str, contents: &str) {
            fs::write(
                fixture_manifest_path(&self.manifests, provider_type),
                contents,
            )
            .unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn alpha(version: &str) -> String {
        format!(
            r#"manifest_version = 1
skills = ["alpha-provider"]

[provider]
type = "alpha-provider"
label = "Alpha Provider"
scope_support = ["system"]
software_version = "{version}"

[[interfaces]]
id = "cli"
type = "executable"
contract_version = 1
executable = "/usr/bin/alpha-provider"
structured_output = true

[[capabilities]]
id = "alpha.control"
contract_version = 1
interfaces = ["cli"]

[[resources]]
type = "alpha-session"
authority = "provider"
lifetime = "session"

[lifecycle]
owner = "provider-owned"

[health]
kind = "structured-cli"
reference = "cli"

[policy]
domain_owner = "provider"
"#
        )
    }

    fn beta() -> &'static str {
        r#"manifest_version = 1

[provider]
type = "beta-provider"
label = "Beta Provider"
scope_support = ["system"]
software_version = "2.0.0"

[[interfaces]]
id = "ipc"
type = "unix-socket"
contract_version = 1
socket = "/run/beta/provider.sock"
structured_output = true

[[capabilities]]
id = "beta.inspect"
contract_version = 1
interfaces = ["ipc"]

[lifecycle]
owner = "portus-supervised"

[health]
kind = "protocol-heartbeat"
reference = "ipc"

[policy]
domain_owner = "provider"
"#
    }

    #[test]
    fn two_distinct_provider_shapes_register_without_invocation_proxy() {
        let fixture = Fixture::new();
        fixture.write("alpha-provider", &alpha("1.0.0"));
        fixture.write("beta-provider", beta());
        let mut state = PortusState::open(&fixture.state_path).unwrap();
        let report = reconcile_directory_at(
            &mut state,
            &fixture.manifests,
            ManifestTrust::PretrustedFixture,
            100,
        )
        .unwrap();
        assert_eq!(report.active.len(), 2);
        assert_eq!(report.created.len(), 2);
        assert!(report.removed.is_empty());

        let page = state
            .list_providers_visible(Principal::new(1000, 1000), 10, None)
            .unwrap();
        assert_eq!(page.items.len(), 2);
        let alpha_capability = state
            .capability_visible_by_id("alpha.control", Principal::new(1000, 1000))
            .unwrap()
            .unwrap();
        assert_eq!(alpha_capability.providers.len(), 1);
        assert_eq!(alpha_capability.providers[0].interfaces, vec!["cli"]);
    }

    #[test]
    fn ordinary_manifest_update_preserves_registration_generation() {
        let fixture = Fixture::new();
        fixture.write("alpha-provider", &alpha("1.0.0"));
        let mut state = PortusState::open(&fixture.state_path).unwrap();
        let first = reconcile_directory_at(
            &mut state,
            &fixture.manifests,
            ManifestTrust::PretrustedFixture,
            100,
        )
        .unwrap();
        let provider_id = first.active[0];

        fixture.write("alpha-provider", &alpha("1.1.0"));
        let second = reconcile_directory_at(
            &mut state,
            &fixture.manifests,
            ManifestTrust::PretrustedFixture,
            200,
        )
        .unwrap();
        assert_eq!(second.active, vec![provider_id]);
        assert!(second.created.is_empty());
        let view = state
            .provider_visible_by_id(&provider_id, Principal::new(1000, 1000))
            .unwrap()
            .unwrap();
        assert_eq!(view.registration.software_version, "1.1.0");
    }

    #[test]
    fn missing_manifest_directory_fails_without_tombstoning_existing_registration() {
        let fixture = Fixture::new();
        fixture.write("alpha-provider", &alpha("1.0.0"));
        let mut state = PortusState::open(&fixture.state_path).unwrap();
        let first = reconcile_directory_at(
            &mut state,
            &fixture.manifests,
            ManifestTrust::PretrustedFixture,
            100,
        )
        .unwrap();
        let provider_id = first.active[0];
        fs::remove_dir_all(&fixture.manifests).unwrap();

        let result = reconcile_directory_at(
            &mut state,
            &fixture.manifests,
            ManifestTrust::PretrustedFixture,
            200,
        );
        assert!(result.is_err());
        assert_eq!(state.active_system_provider_count().unwrap(), 1);
        let view = state
            .provider_visible_by_id(&provider_id, Principal::new(1000, 1000))
            .unwrap()
            .unwrap();
        assert_eq!(view.registration.removed_at_ms, None);
        assert!(view.tombstone.is_none());
    }

    #[test]
    fn remove_and_reinstall_gets_new_generation_and_old_resource_stays_stale() {
        let fixture = Fixture::new();
        fixture.write("alpha-provider", &alpha("1.0.0"));
        let mut state = PortusState::open(&fixture.state_path).unwrap();
        let first = reconcile_directory_at(
            &mut state,
            &fixture.manifests,
            ManifestTrust::PretrustedFixture,
            100,
        )
        .unwrap();
        let old_provider_id = first.active[0];
        let old_resource = ProviderResourceRef::new(
            old_provider_id,
            ResourceType::new("alpha-session").unwrap(),
            ProviderResourceId::new("same-provider-owned-value").unwrap(),
        )
        .with_generation("session-a");
        state
            .record_provider_resource_ref(&old_resource, None, "available", 110)
            .unwrap();

        fs::remove_file(fixture_manifest_path(&fixture.manifests, "alpha-provider")).unwrap();
        let removed = reconcile_directory_at(
            &mut state,
            &fixture.manifests,
            ManifestTrust::PretrustedFixture,
            200,
        )
        .unwrap();
        assert_eq!(removed.removed, vec![old_provider_id]);
        let removed_view = state
            .provider_visible_by_id(&old_provider_id, Principal::new(1000, 1000))
            .unwrap()
            .unwrap();
        assert_eq!(removed_view.registration.removed_at_ms, Some(200));
        assert_eq!(
            removed_view
                .tombstone
                .as_ref()
                .unwrap()
                .safe_reason
                .as_deref(),
            Some("manifest_removed")
        );
        assert_eq!(
            state
                .provider_resource_availability(&old_resource)
                .unwrap()
                .as_deref(),
            Some("stale")
        );

        fixture.write("alpha-provider", &alpha("1.0.0"));
        let reinstalled = reconcile_directory_at(
            &mut state,
            &fixture.manifests,
            ManifestTrust::PretrustedFixture,
            300,
        )
        .unwrap();
        let new_provider_id = reinstalled.active[0];
        assert_ne!(new_provider_id, old_provider_id);
        let historical = state
            .provider_visible_by_id(&old_provider_id, Principal::new(1000, 1000))
            .unwrap()
            .unwrap();
        assert_eq!(
            historical.tombstone.unwrap().successor_provider_id,
            Some(new_provider_id)
        );
        assert_eq!(
            state
                .provider_resource_availability(&old_resource)
                .unwrap()
                .as_deref(),
            Some("stale")
        );
        let new_resource = ProviderResourceRef::new(
            new_provider_id,
            ResourceType::new("alpha-session").unwrap(),
            ProviderResourceId::new("same-provider-owned-value").unwrap(),
        )
        .with_generation("session-a");
        assert!(
            state
                .provider_resource_availability(&new_resource)
                .unwrap()
                .is_none()
        );
    }
}
