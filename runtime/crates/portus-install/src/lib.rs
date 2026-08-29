//! Host-safe installed-stack staging contract for PortusOS P16.
//!
//! This crate deliberately stages into an arbitrary root and never mutates the
//! developer machine's real `/etc`, `/usr`, `/var`, or `/run`. Ownership and
//! mode declarations are validated as package metadata; real `chown`, OpenRC
//! enablement, service execution, and Artix package integration remain Track L.

use portus_browser_integration::PortusBrowserContract;
use portus_policy::{
    ActionRegistry, BundleDefinition, GlobalPolicy, PolicyPaths, PolicySnapshot, PolicyTrust,
};
use portus_protected_api::{DefinitionCatalog, ProviderDefinition};
use portus_provider::ProviderManifest;
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashSet},
    fmt, fs,
    io::{ErrorKind, Write},
    path::{Component, Path, PathBuf},
};

pub const INSTALL_VERSION: u32 = 1;
pub const MAX_SOURCE_FILE_BYTES: u64 = 1024 * 1024;
pub const OPENRC_RESOLUTION_MARKER: &str = "P16-LINUX-RESOLUTION-REQUIRED";

const REQUIRED_BINARIES: [&str; 8] = [
    "portus-api",
    "portus-apid",
    "portus-auth",
    "portus-bootstrap",
    "portus-master",
    "portus-os",
    "portus-privd",
    "portusd",
];
const REQUIRED_SERVICES: [&str; 3] = ["portus-apid", "portus-privd", "portusd"];
const REQUIRED_BUNDLES: [&str; 8] = [
    "applications-desktop",
    "development",
    "devices-hardware",
    "external-data-delivery",
    "files-workspaces",
    "network-internet",
    "remote-access",
    "system-administration",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum FilePolicy {
    PackageOwned,
    AdministratorConfig,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Resolution {
    Locked,
    LinuxVerified,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DirectoryLifetime {
    Config,
    PersistentConfig,
    PackageOwned,
    PersistentState,
    PersistentLog,
    Runtime,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BinarySpec {
    pub name: String,
    pub target: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileSpec {
    pub source: String,
    pub target: String,
    pub policy: FilePolicy,
    pub mode: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectorySpec {
    pub path: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    pub lifetime: DirectoryLifetime,
    pub resolution: Resolution,
    #[serde(default)]
    pub unresolved_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IdentitySpec {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    pub resolution: Resolution,
    #[serde(default)]
    pub unresolved_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ServiceSpec {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub command_user: Option<String>,
    pub template_source: String,
    pub template_target: String,
    pub lifecycle_owner: String,
    pub resolution: Resolution,
    #[serde(default)]
    pub unresolved_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExternalRequirement {
    pub id: String,
    pub required_for_first_iso: bool,
    pub items: Vec<String>,
    pub resolution: Resolution,
    pub unresolved_reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UninstallSpec {
    pub preserve_prefixes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InstallManifest {
    pub install_version: u32,
    pub binaries: Vec<BinarySpec>,
    pub files: Vec<FileSpec>,
    pub directories: Vec<DirectorySpec>,
    pub identities: Vec<IdentitySpec>,
    pub services: Vec<ServiceSpec>,
    pub external_requirements: Vec<ExternalRequirement>,
    pub uninstall: UninstallSpec,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StageReport {
    pub created: usize,
    pub replaced: usize,
    pub unchanged: usize,
    pub preserved_modified: usize,
    pub unresolved_linux_items: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UninstallReport {
    pub removed: usize,
    pub preserved_modified: usize,
    pub persistent_paths_preserved: usize,
}

#[derive(Debug)]
pub enum InstallError {
    Io(std::io::Error),
    Parse(String),
    Invalid(String),
    Provider(String),
    Policy(String),
    ProtectedApi(String),
    Browser(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "install staging I/O error: {error}"),
            Self::Parse(message) => write!(f, "install manifest parse error: {message}"),
            Self::Invalid(message) => write!(f, "invalid install staging contract: {message}"),
            Self::Provider(message) => write!(f, "provider staging validation failed: {message}"),
            Self::Policy(message) => write!(f, "policy staging validation failed: {message}"),
            Self::ProtectedApi(message) => {
                write!(f, "protected API staging validation failed: {message}")
            }
            Self::Browser(message) => {
                write!(f, "PortusBrowser staging validation failed: {message}")
            }
        }
    }
}

impl std::error::Error for InstallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for InstallError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type InstallResult<T> = Result<T, InstallError>;

impl InstallManifest {
    pub fn parse(contents: &str) -> InstallResult<Self> {
        let manifest: Self =
            toml::from_str(contents).map_err(|error| InstallError::Parse(error.to_string()))?;
        manifest.validate_shape()?;
        Ok(manifest)
    }

    pub fn load(path: &Path) -> InstallResult<Self> {
        Self::parse(&fs::read_to_string(path)?)
    }

    pub fn validate_shape(&self) -> InstallResult<()> {
        if self.install_version != INSTALL_VERSION {
            return invalid("unsupported install manifest version");
        }
        validate_binaries(&self.binaries)?;
        validate_files(&self.files)?;
        validate_directories(&self.directories)?;
        validate_identities(&self.identities)?;
        validate_services(&self.services)?;
        validate_external_requirements(&self.external_requirements)?;
        validate_uninstall(&self.uninstall)?;
        Ok(())
    }

    pub fn validate_sources(&self, repo_root: &Path) -> InstallResult<()> {
        self.validate_shape()?;
        let root_metadata = fs::symlink_metadata(repo_root)?;
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return invalid("repository source root must be a real directory");
        }
        let canonical_root = fs::canonicalize(repo_root)?;
        for file in &self.files {
            let source = source_path(repo_root, &file.source)?;
            let metadata = fs::symlink_metadata(&source)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > MAX_SOURCE_FILE_BYTES
            {
                return invalid("install source is not a bounded regular file");
            }
            let canonical_source = fs::canonicalize(&source)?;
            if !canonical_source.starts_with(&canonical_root) {
                return invalid("install source escapes canonical repository root");
            }
            let bytes = fs::read(&source)?;
            scan_for_reusable_secret(&bytes, &file.source)?;
        }
        self.validate_source_contracts(repo_root)
    }

    #[must_use]
    pub fn release_ready(&self) -> bool {
        self.directories
            .iter()
            .all(|entry| entry.resolution == Resolution::Locked)
            && self
                .identities
                .iter()
                .all(|entry| entry.resolution == Resolution::Locked)
            && self
                .services
                .iter()
                .all(|entry| entry.resolution == Resolution::Locked)
            && self
                .external_requirements
                .iter()
                .all(|entry| entry.resolution == Resolution::Locked)
    }

    #[must_use]
    pub fn unresolved_linux_items(&self) -> usize {
        self.directories
            .iter()
            .filter(|entry| entry.resolution == Resolution::LinuxVerified)
            .count()
            + self
                .identities
                .iter()
                .filter(|entry| entry.resolution == Resolution::LinuxVerified)
                .count()
            + self
                .services
                .iter()
                .filter(|entry| entry.resolution == Resolution::LinuxVerified)
                .count()
            + self
                .external_requirements
                .iter()
                .filter(|entry| entry.resolution == Resolution::LinuxVerified)
                .count()
    }

    pub fn stage(
        &self,
        repo_root: &Path,
        binary_dir: &Path,
        staging_root: &Path,
    ) -> InstallResult<StageReport> {
        self.validate_sources(repo_root)?;
        validate_binary_payload(&self.binaries, binary_dir)?;
        prepare_staging_root(staging_root)?;

        for directory in &self.directories {
            ensure_real_directory_tree(
                staging_root,
                &installed_path(staging_root, &directory.path)?,
            )?;
        }

        let mut report = StageReport {
            unresolved_linux_items: self.unresolved_linux_items(),
            ..StageReport::default()
        };
        for binary in &self.binaries {
            let source = binary_dir.join(&binary.name);
            copy_package_owned(
                staging_root,
                &source,
                &installed_path(staging_root, &binary.target)?,
                &mut report,
            )?;
        }
        for file in &self.files {
            let source = source_path(repo_root, &file.source)?;
            let target = installed_path(staging_root, &file.target)?;
            match file.policy {
                FilePolicy::PackageOwned => {
                    copy_package_owned(staging_root, &source, &target, &mut report)?
                }
                FilePolicy::AdministratorConfig => {
                    copy_administrator_config(staging_root, &source, &target, &mut report)?
                }
            }
        }
        self.validate_staged(staging_root)?;
        Ok(report)
    }

    pub fn uninstall(
        &self,
        repo_root: &Path,
        binary_dir: &Path,
        staging_root: &Path,
    ) -> InstallResult<UninstallReport> {
        self.validate_sources(repo_root)?;
        validate_binary_payload(&self.binaries, binary_dir)?;
        let mut report = UninstallReport::default();
        if !validate_existing_staging_root(staging_root)? {
            report.persistent_paths_preserved = self.uninstall.preserve_prefixes.len();
            return Ok(report);
        }
        for binary in &self.binaries {
            remove_if_unmodified(
                staging_root,
                &binary_dir.join(&binary.name),
                &installed_path(staging_root, &binary.target)?,
                &mut report,
            )?;
        }
        for file in &self.files {
            remove_if_unmodified(
                staging_root,
                &source_path(repo_root, &file.source)?,
                &installed_path(staging_root, &file.target)?,
                &mut report,
            )?;
        }
        report.persistent_paths_preserved = self.uninstall.preserve_prefixes.len();
        Ok(report)
    }

    fn validate_source_contracts(&self, repo_root: &Path) -> InstallResult<()> {
        let protected_manifest_text = read_target_source(
            self,
            repo_root,
            "/etc/portus/capabilities/protected-api.toml",
        )?;
        let protected_manifest =
            ProviderManifest::parse("protected-api.toml", &protected_manifest_text)
                .map_err(|error| InstallError::Provider(error.to_string()))?;
        if !protected_manifest
            .skills
            .iter()
            .any(|skill| skill == "protected-api")
        {
            return invalid(
                "protected API provider manifest must reference its machine-wide skill",
            );
        }

        let browser_manifest_text = read_target_source(
            self,
            repo_root,
            "/etc/portus/capabilities/portus-browser.toml",
        )?;
        let browser_manifest =
            ProviderManifest::parse("portus-browser.toml", &browser_manifest_text)
                .map_err(|error| InstallError::Provider(error.to_string()))?;
        let browser_contract_text = read_target_source(
            self,
            repo_root,
            "/usr/share/portus/integrations/portus-browser/integration.toml",
        )?;
        let browser_contract = PortusBrowserContract::parse(&browser_contract_text)
            .map_err(|error| InstallError::Browser(error.to_string()))?;
        browser_contract
            .validate_provider_manifest(&browser_manifest)
            .map_err(|error| InstallError::Browser(error.to_string()))?;

        let provider_text = read_target_source(
            self,
            repo_root,
            "/etc/portus/protected-api/providers.d/openai.toml",
        )?;
        let provider: ProviderDefinition = toml::from_str(&provider_text)
            .map_err(|error| InstallError::ProtectedApi(error.to_string()))?;
        DefinitionCatalog::from_definitions(vec![provider])
            .map_err(|error| InstallError::ProtectedApi(error.to_string()))?;

        let global: GlobalPolicy = toml::from_str(&read_target_source(
            self,
            repo_root,
            "/etc/portus/policy/policy.toml",
        )?)
        .map_err(|error| InstallError::Policy(error.to_string()))?;
        let actions: ActionRegistry = toml::from_str(&read_target_source(
            self,
            repo_root,
            "/usr/share/portus/policy/actions.toml",
        )?)
        .map_err(|error| InstallError::Policy(error.to_string()))?;
        let mut bundles = Vec::new();
        for id in REQUIRED_BUNDLES {
            let target = format!("/usr/share/portus/policy/bundles/{id}.toml");
            bundles.push(
                toml::from_str::<BundleDefinition>(&read_target_source(self, repo_root, &target)?)
                    .map_err(|error| InstallError::Policy(error.to_string()))?,
            );
        }
        PolicySnapshot::from_documents(global, actions, bundles, Vec::new())
            .map_err(|error| InstallError::Policy(error.to_string()))?;

        for service in &self.services {
            let template = fs::read_to_string(source_path(repo_root, &service.template_source)?)?;
            validate_openrc_template(service, &template)?;
        }
        Ok(())
    }

    fn validate_staged(&self, staging_root: &Path) -> InstallResult<()> {
        let policy_paths = PolicyPaths {
            policy_path: installed_path(staging_root, "/etc/portus/policy/policy.toml")?,
            subjects_dir: installed_path(staging_root, "/etc/portus/policy/subjects.d")?,
            actions_path: installed_path(staging_root, "/usr/share/portus/policy/actions.toml")?,
            bundles_dir: installed_path(staging_root, "/usr/share/portus/policy/bundles")?,
        };
        PolicySnapshot::load(&policy_paths, PolicyTrust::PretrustedFixture)
            .map_err(|error| InstallError::Policy(error.to_string()))?;
        DefinitionCatalog::load(
            &portus_protected_api::DefinitionPaths {
                directory: installed_path(staging_root, "/etc/portus/protected-api/providers.d")?,
            },
            portus_protected_api::DefinitionTrust::PretrustedFixture,
        )
        .map_err(|error| InstallError::ProtectedApi(error.to_string()))?;

        let protected_store = installed_path(staging_root, "/var/lib/portus/protected-api")?;
        if protected_store.join("credentials.db").exists() {
            return invalid("package staging must never create a protected credential database");
        }
        Ok(())
    }
}

fn validate_binaries(entries: &[BinarySpec]) -> InstallResult<()> {
    let expected: BTreeSet<&str> = REQUIRED_BINARIES.into_iter().collect();
    let mut names = BTreeSet::new();
    let mut targets = HashSet::new();
    for entry in entries {
        validate_name(&entry.name, "binary name")?;
        validate_linux_path(&entry.target)?;
        if entry.target != format!("/usr/bin/{}", entry.name)
            || !names.insert(entry.name.as_str())
            || !targets.insert(entry.target.as_str())
        {
            return invalid("binary set contains a duplicate or noncanonical target");
        }
    }
    if names != expected {
        return invalid("first-ISO Portus binary set is incomplete or unexpected");
    }
    Ok(())
}

fn validate_files(entries: &[FileSpec]) -> InstallResult<()> {
    let mut targets = HashSet::new();
    for entry in entries {
        validate_relative_source(&entry.source)?;
        validate_linux_path(&entry.target)?;
        validate_mode(&entry.mode)?;
        if !targets.insert(entry.target.as_str()) {
            return invalid("duplicate installed file target");
        }
        if !(entry.target.starts_with("/etc/portus/")
            || entry.target.starts_with("/etc/codex/skills/")
            || entry.target.starts_with("/usr/share/portus/"))
        {
            return invalid("static package/config file escapes approved installed prefixes");
        }
        if (entry.target.starts_with("/etc/portus/")
            || entry.target.starts_with("/etc/codex/skills/"))
            && entry.policy != FilePolicy::AdministratorConfig
        {
            return invalid("administrator configuration must preserve local modifications");
        }
        if entry.target.starts_with("/usr/share/portus/")
            && entry.policy != FilePolicy::PackageOwned
        {
            return invalid("/usr/share/portus files must be package-owned");
        }
    }
    Ok(())
}

fn validate_directories(entries: &[DirectorySpec]) -> InstallResult<()> {
    let mut paths = HashSet::new();
    for entry in entries {
        validate_linux_path(&entry.path)?;
        if !paths.insert(entry.path.as_str()) {
            return invalid("duplicate installed directory declaration");
        }
        match entry.resolution {
            Resolution::Locked => {
                if entry.owner.as_deref().unwrap_or_default().is_empty()
                    || entry.group.as_deref().unwrap_or_default().is_empty()
                    || entry.mode.as_deref().unwrap_or_default().is_empty()
                    || entry.unresolved_reason.is_some()
                {
                    return invalid("locked directory lacks complete ownership/mode data");
                }
                validate_mode(entry.mode.as_deref().unwrap_or_default())?;
            }
            Resolution::LinuxVerified => {
                if entry
                    .unresolved_reason
                    .as_deref()
                    .unwrap_or_default()
                    .is_empty()
                {
                    return invalid("Linux-verified directory lacks an explicit unresolved reason");
                }
                if let Some(mode) = entry.mode.as_deref() {
                    validate_mode(mode)?;
                }
            }
        }
    }
    require_directory(
        entries,
        "/var/lib/portus/protected-api",
        "portus-api",
        "portus-api",
        "0700",
    )?;
    require_directory(
        entries,
        "/run/portus/priv",
        "root",
        "portus-priv-users",
        "0750",
    )?;
    require_directory(
        entries,
        "/run/portus/protected-api",
        "portus-api",
        "portus-api-users",
        "02750",
    )?;
    for unresolved in [
        "/var/lib/portus/state",
        "/var/log/portus/audit",
        "/run/portus",
    ] {
        let Some(entry) = entries.iter().find(|entry| entry.path == unresolved) else {
            return invalid("required Linux-resolved directory is missing");
        };
        if entry.resolution != Resolution::LinuxVerified {
            return invalid("P16 must not falsely lock unresolved portusd/audit runtime ownership");
        }
    }
    Ok(())
}

fn validate_identities(entries: &[IdentitySpec]) -> InstallResult<()> {
    let mut ids = HashSet::new();
    for entry in entries {
        validate_name(&entry.id, "identity id")?;
        validate_name(&entry.kind, "identity kind")?;
        if !ids.insert(entry.id.as_str()) {
            return invalid("duplicate service/group identity declaration");
        }
        match entry.resolution {
            Resolution::Locked => {
                if entry.unresolved_reason.is_some()
                    || entry.group.as_deref().unwrap_or_default().is_empty()
                {
                    return invalid("locked identity is incomplete or marked unresolved");
                }
            }
            Resolution::LinuxVerified => {
                if entry
                    .unresolved_reason
                    .as_deref()
                    .unwrap_or_default()
                    .is_empty()
                {
                    return invalid("Linux-verified identity lacks unresolved reason");
                }
            }
        }
    }
    require_identity(
        entries,
        "portus-privd",
        Some("root"),
        Some("root"),
        Resolution::Locked,
    )?;
    require_identity(
        entries,
        "portus-apid",
        Some("portus-api"),
        Some("portus-api"),
        Resolution::Locked,
    )?;
    require_identity(
        entries,
        "portus-api-users",
        None,
        Some("portus-api-users"),
        Resolution::Locked,
    )?;
    require_identity(
        entries,
        "portus-priv-users",
        None,
        Some("portus-priv-users"),
        Resolution::Locked,
    )?;
    require_identity(entries, "portusd", None, None, Resolution::LinuxVerified)?;
    Ok(())
}

fn validate_services(entries: &[ServiceSpec]) -> InstallResult<()> {
    let expected: BTreeSet<&str> = REQUIRED_SERVICES.into_iter().collect();
    let mut names = BTreeSet::new();
    for entry in entries {
        validate_name(&entry.name, "service name")?;
        validate_linux_path(&entry.command)?;
        validate_relative_source(&entry.template_source)?;
        validate_linux_path(&entry.template_target)?;
        if entry.lifecycle_owner != "openrc"
            || entry.resolution != Resolution::LinuxVerified
            || entry
                .unresolved_reason
                .as_deref()
                .unwrap_or_default()
                .is_empty()
            || !entry
                .template_target
                .starts_with("/usr/share/portus/openrc/templates/")
            || entry.template_target.starts_with("/etc/init.d/")
            || !names.insert(entry.name.as_str())
        {
            return invalid("OpenRC service template declaration is unsafe or incomplete");
        }
        if entry.command != format!("/usr/bin/{}", entry.name) {
            return invalid("service command does not match packaged daemon binary");
        }
    }
    if names != expected {
        return invalid("machine-service set must be exactly portusd/portus-privd/portus-apid");
    }
    let privd = entries
        .iter()
        .find(|entry| entry.name == "portus-privd")
        .unwrap();
    let apid = entries
        .iter()
        .find(|entry| entry.name == "portus-apid")
        .unwrap();
    let runtime = entries
        .iter()
        .find(|entry| entry.name == "portusd")
        .unwrap();
    if privd.command_user.as_deref() != Some("root:root")
        || apid.command_user.as_deref() != Some("portus-api:portus-api")
        || runtime.command_user.is_some()
    {
        return invalid(
            "service identities contradict locked P9/P10 or unresolved portusd identity",
        );
    }
    Ok(())
}

fn validate_external_requirements(entries: &[ExternalRequirement]) -> InstallResult<()> {
    if entries.len() != 1 {
        return invalid("P16 expects one external PortusBrowser requirement set");
    }
    let entry = &entries[0];
    let expected: BTreeSet<&str> = [
        "extension",
        "portus-browser",
        "portus-browser-skill",
        "portus-broker",
        "portus-native-host",
    ]
    .into_iter()
    .collect();
    let actual: BTreeSet<&str> = entry.items.iter().map(String::as_str).collect();
    if entry.id != "portus-browser"
        || !entry.required_for_first_iso
        || entry.resolution != Resolution::LinuxVerified
        || entry.unresolved_reason.is_empty()
        || actual != expected
    {
        return invalid("PortusBrowser external packaging gate is incomplete");
    }
    Ok(())
}

fn validate_uninstall(spec: &UninstallSpec) -> InstallResult<()> {
    let expected: BTreeSet<&str> = [
        "/etc/portus/policy/subjects.d",
        "/var/lib/portus",
        "/var/log/portus",
        "/workspace",
    ]
    .into_iter()
    .collect();
    let actual: BTreeSet<&str> = spec.preserve_prefixes.iter().map(String::as_str).collect();
    if actual != expected {
        return invalid("uninstall preservation boundary changed");
    }
    Ok(())
}

fn validate_binary_payload(entries: &[BinarySpec], binary_dir: &Path) -> InstallResult<()> {
    let root_metadata = fs::symlink_metadata(binary_dir)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return invalid("binary payload root must be a real directory");
    }
    let canonical_root = fs::canonicalize(binary_dir)?;
    for entry in entries {
        let path = binary_dir.join(&entry.name);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
            return invalid("binary payload contains a missing/empty/non-regular entry");
        }
        if !fs::canonicalize(&path)?.starts_with(&canonical_root) {
            return invalid("binary payload entry escapes canonical payload root");
        }
    }
    Ok(())
}

fn validate_openrc_template(service: &ServiceSpec, text: &str) -> InstallResult<()> {
    if !text.starts_with("#!/sbin/openrc-run\n")
        || !text.contains(OPENRC_RESOLUTION_MARKER)
        || !text.contains(&format!("command=\"{}\"", service.command))
        || text.contains("supervisor=")
        || text.contains("respawn_")
        || text.contains("depend()")
        || text.contains("rc-update")
    {
        return invalid("OpenRC template prematurely locks or omits Linux-resolved behavior");
    }
    match service.command_user.as_deref() {
        Some(user) if !text.contains(&format!("command_user=\"{user}\"")) => {
            return invalid("OpenRC template does not preserve locked service identity");
        }
        None if text.contains("command_user=") => {
            return invalid("portusd template must not guess a service identity");
        }
        _ => {}
    }
    Ok(())
}

fn prepare_staging_root(root: &Path) -> InstallResult<()> {
    match fs::symlink_metadata(root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return invalid("staging root must be a real directory");
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir_all(root)?;
            let metadata = fs::symlink_metadata(root)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return invalid("created staging root is not a real directory");
            }
        }
        Err(error) => return Err(InstallError::Io(error)),
    }
    Ok(())
}

fn validate_existing_staging_root(root: &Path) -> InstallResult<bool> {
    match fs::symlink_metadata(root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return invalid("staging root must be a real directory");
            }
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(InstallError::Io(error)),
    }
}

fn ensure_real_directory_tree(root: &Path, directory: &Path) -> InstallResult<()> {
    if !validate_existing_staging_root(root)? {
        return invalid("staging root disappeared during package operation");
    }
    let relative = directory
        .strip_prefix(root)
        .map_err(|_| InstallError::Invalid("installed directory escapes staging root".into()))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return invalid("installed directory contains a non-normal component");
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return invalid("staging directory path contains a symlink or non-directory");
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                fs::create_dir(&current)?;
                let metadata = fs::symlink_metadata(&current)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return invalid("created staging directory is not a real directory");
                }
            }
            Err(error) => return Err(InstallError::Io(error)),
        }
    }
    Ok(())
}

fn regular_target_or_absent(root: &Path, target: &Path) -> InstallResult<bool> {
    let parent = target
        .parent()
        .ok_or_else(|| InstallError::Invalid("installed file has no parent".into()))?;
    ensure_real_directory_tree(root, parent)?;
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return invalid("installed file target is a symlink or non-regular file");
            }
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(InstallError::Io(error)),
    }
}

fn copy_package_owned(
    staging_root: &Path,
    source: &Path,
    target: &Path,
    report: &mut StageReport,
) -> InstallResult<()> {
    let bytes = fs::read(source)?;
    if regular_target_or_absent(staging_root, target)? {
        if fs::read(target)? == bytes {
            report.unchanged += 1;
            return Ok(());
        }
        write_atomicish(staging_root, target, &bytes)?;
        report.replaced += 1;
    } else {
        write_atomicish(staging_root, target, &bytes)?;
        report.created += 1;
    }
    Ok(())
}

fn copy_administrator_config(
    staging_root: &Path,
    source: &Path,
    target: &Path,
    report: &mut StageReport,
) -> InstallResult<()> {
    let bytes = fs::read(source)?;
    if regular_target_or_absent(staging_root, target)? {
        if fs::read(target)? == bytes {
            report.unchanged += 1;
        } else {
            report.preserved_modified += 1;
        }
        return Ok(());
    }
    write_atomicish(staging_root, target, &bytes)?;
    report.created += 1;
    Ok(())
}

fn write_atomicish(staging_root: &Path, target: &Path, bytes: &[u8]) -> InstallResult<()> {
    let target_exists = regular_target_or_absent(staging_root, target)?;
    let parent = target
        .parent()
        .ok_or_else(|| InstallError::Invalid("installed file has no parent".into()))?;
    let temp = parent.join(format!(
        ".{}.portus-install-tmp",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
    ));
    match fs::symlink_metadata(&temp) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return invalid("staging temp path is a symlink or non-regular file");
        }
        Ok(_) => fs::remove_file(&temp)?,
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(InstallError::Io(error)),
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.flush()) {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(InstallError::Io(error));
    }
    drop(file);

    let target_still_exists = regular_target_or_absent(staging_root, target)?;
    if target_still_exists != target_exists {
        let _ = fs::remove_file(&temp);
        return invalid("installed file target changed during staging");
    }
    if target_still_exists {
        fs::remove_file(target)?;
    }
    if let Err(error) = fs::rename(&temp, target) {
        let _ = fs::remove_file(&temp);
        return Err(InstallError::Io(error));
    }
    Ok(())
}

fn remove_if_unmodified(
    staging_root: &Path,
    source: &Path,
    target: &Path,
    report: &mut UninstallReport,
) -> InstallResult<()> {
    if !regular_target_or_absent(staging_root, target)? {
        return Ok(());
    }
    if fs::read(source)? == fs::read(target)? {
        fs::remove_file(target)?;
        report.removed += 1;
    } else {
        report.preserved_modified += 1;
    }
    Ok(())
}

fn scan_for_reusable_secret(bytes: &[u8], source: &str) -> InstallResult<()> {
    let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    for marker in [
        "-----begin private key-----",
        "-----begin openssh private key-----",
        "openai_api_key=",
        "aws_secret_access_key=",
        "github_token=",
        "api_key =",
        "password =",
        "secret =",
    ] {
        if text.contains(marker) {
            return invalid(&format!("reusable secret-like material found in {source}"));
        }
    }
    Ok(())
}

fn read_target_source(
    manifest: &InstallManifest,
    repo_root: &Path,
    target: &str,
) -> InstallResult<String> {
    let spec = manifest
        .files
        .iter()
        .find(|entry| entry.target == target)
        .ok_or_else(|| {
            InstallError::Invalid(format!("required staged target missing: {target}"))
        })?;
    Ok(fs::read_to_string(source_path(repo_root, &spec.source)?)?)
}

fn source_path(repo_root: &Path, relative: &str) -> InstallResult<PathBuf> {
    validate_relative_source(relative)?;
    Ok(repo_root.join(relative))
}

fn installed_path(root: &Path, linux_path: &str) -> InstallResult<PathBuf> {
    validate_linux_path(linux_path)?;
    Ok(root.join(linux_path.trim_start_matches('/')))
}

fn validate_relative_source(value: &str) -> InstallResult<()> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return invalid("source path must be a traversal-free repository-relative path");
    }
    Ok(())
}

fn validate_linux_path(value: &str) -> InstallResult<()> {
    if !value.starts_with('/') || value.contains(['\0', '\n', '\r']) || value.contains("//") {
        return invalid("installed path is not a canonical absolute Linux path");
    }
    if value
        .split('/')
        .skip(1)
        .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return invalid("installed path contains an invalid segment");
    }
    Ok(())
}

fn validate_mode(value: &str) -> InstallResult<()> {
    if !(value.len() == 4 || value.len() == 5)
        || !value.starts_with('0')
        || u32::from_str_radix(value, 8).is_err()
    {
        return invalid("mode must be an explicit octal string");
    }
    Ok(())
}

fn validate_name(value: &str, field: &str) -> InstallResult<()> {
    if value.is_empty()
        || value.len() > 96
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
    {
        return invalid(&format!("{field} is invalid"));
    }
    Ok(())
}

fn require_directory(
    entries: &[DirectorySpec],
    path: &str,
    owner: &str,
    group: &str,
    mode: &str,
) -> InstallResult<()> {
    let Some(entry) = entries.iter().find(|entry| entry.path == path) else {
        return invalid("required locked directory declaration is missing");
    };
    if entry.resolution != Resolution::Locked
        || entry.owner.as_deref() != Some(owner)
        || entry.group.as_deref() != Some(group)
        || entry.mode.as_deref() != Some(mode)
    {
        return invalid("locked runtime/store directory contradicts its owning subsystem");
    }
    Ok(())
}

fn require_identity(
    entries: &[IdentitySpec],
    id: &str,
    user: Option<&str>,
    group: Option<&str>,
    resolution: Resolution,
) -> InstallResult<()> {
    let Some(entry) = entries.iter().find(|entry| entry.id == id) else {
        return invalid("required service/group identity declaration is missing");
    };
    if entry.user.as_deref() != user
        || entry.group.as_deref() != group
        || entry.resolution != resolution
    {
        return invalid("service/group identity contradicts locked subsystem contract");
    }
    Ok(())
}

fn invalid<T>(message: &str) -> InstallResult<T> {
    Err(InstallError::Invalid(message.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn manifest() -> InstallManifest {
        InstallManifest::load(&repo_root().join("runtime/install/install.toml")).unwrap()
    }

    struct Fixture {
        root: PathBuf,
        binaries: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let base = std::env::temp_dir().join(format!("portus-install-{name}-{stamp}"));
            let root = base.join("root");
            let binaries = base.join("bin");
            fs::create_dir_all(&binaries).unwrap();
            for binary in REQUIRED_BINARIES {
                fs::write(
                    binaries.join(binary),
                    format!("fixture executable {binary}\n"),
                )
                .unwrap();
            }
            Self { root, binaries }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Some(parent) = self.root.parent() {
                let _ = fs::remove_dir_all(parent);
            }
        }
    }

    #[test]
    fn shipped_manifest_and_sources_are_strict_and_not_release_ready_on_windows_phase() {
        let manifest = manifest();
        manifest.validate_sources(&repo_root()).unwrap();
        assert!(!manifest.release_ready());
        assert!(manifest.unresolved_linux_items() >= 7);
        assert_eq!(manifest.services.len(), 3);
        assert!(
            manifest
                .services
                .iter()
                .all(|service| service.name != "portus-browser")
        );
    }

    #[test]
    fn clean_stage_materializes_canonical_stack_without_secret_store() {
        let fixture = Fixture::new("clean");
        let manifest = manifest();
        let report = manifest
            .stage(&repo_root(), &fixture.binaries, &fixture.root)
            .unwrap();
        assert!(report.created >= REQUIRED_BINARIES.len());
        for binary in REQUIRED_BINARIES {
            assert!(fixture.root.join("usr/bin").join(binary).is_file());
        }
        assert!(
            fixture
                .root
                .join("etc/portus/capabilities/portus-browser.toml")
                .is_file()
        );
        assert!(
            fixture
                .root
                .join("etc/portus/capabilities/protected-api.toml")
                .is_file()
        );
        assert!(
            fixture
                .root
                .join("etc/codex/skills/protected-api/SKILL.md")
                .is_file()
        );
        assert!(
            fixture
                .root
                .join("usr/share/portus/openrc/templates/portusd.in")
                .is_file()
        );
        assert!(!fixture.root.join("etc/init.d/portusd").exists());
        assert!(
            !fixture
                .root
                .join("var/lib/portus/protected-api/credentials.db")
                .exists()
        );
        assert!(fixture.root.join("etc/portus/policy/subjects.d").is_dir());
        assert_eq!(
            fs::read_dir(fixture.root.join("etc/portus/policy/subjects.d"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn reinstall_is_idempotent_and_preserves_modified_administrator_config() {
        let fixture = Fixture::new("reinstall");
        let manifest = manifest();
        manifest
            .stage(&repo_root(), &fixture.binaries, &fixture.root)
            .unwrap();
        let second = manifest
            .stage(&repo_root(), &fixture.binaries, &fixture.root)
            .unwrap();
        assert_eq!(second.created, 0);
        assert_eq!(second.replaced, 0);
        assert_eq!(second.preserved_modified, 0);
        let config = fixture.root.join("etc/portus/policy/policy.toml");
        fs::write(
            &config,
            "policy_version = 1\ndefault_effect = \"reject\"\n# administrator note\n",
        )
        .unwrap();
        let third = manifest
            .stage(&repo_root(), &fixture.binaries, &fixture.root)
            .unwrap();
        assert_eq!(third.preserved_modified, 1);
        assert!(
            fs::read_to_string(config)
                .unwrap()
                .contains("administrator note")
        );
    }

    #[test]
    fn uninstall_removes_unmodified_package_payload_but_preserves_state_subjects_and_modified_config()
     {
        let fixture = Fixture::new("uninstall");
        let manifest = manifest();
        manifest
            .stage(&repo_root(), &fixture.binaries, &fixture.root)
            .unwrap();
        let state = fixture.root.join("var/lib/portus/state/portus.db");
        fs::write(&state, b"durable state fixture").unwrap();
        let subject = fixture.root.join("etc/portus/policy/subjects.d/1000.toml");
        fs::write(&subject, "policy_version = 1\nuid = 1000\n").unwrap();
        let config = fixture.root.join("etc/portus/policy/policy.toml");
        fs::write(
            &config,
            "policy_version = 1\ndefault_effect = \"reject\"\n# changed\n",
        )
        .unwrap();
        let report = manifest
            .uninstall(&repo_root(), &fixture.binaries, &fixture.root)
            .unwrap();
        assert!(report.removed >= REQUIRED_BINARIES.len());
        assert!(report.preserved_modified >= 1);
        assert!(state.is_file());
        assert!(subject.is_file());
        assert!(config.is_file());
        assert!(!fixture.root.join("usr/bin/portus-os").exists());
        assert!(
            !fixture
                .root
                .join("etc/portus/capabilities/portus-browser.toml")
                .exists()
        );
    }

    #[test]
    fn staging_rejects_non_regular_destination_replacement() {
        let fixture = Fixture::new("replacement");
        let manifest = manifest();
        fs::create_dir_all(fixture.root.join("usr/bin/portus-os")).unwrap();
        let result = manifest.stage(&repo_root(), &fixture.binaries, &fixture.root);
        assert!(matches!(result, Err(InstallError::Invalid(_))));
        assert!(fixture.root.join("usr/bin/portus-os").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_symlinked_destination_parent_without_touching_outside_tree() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new("symlink-parent");
        let manifest = manifest();
        fs::create_dir_all(&fixture.root).unwrap();
        let outside = fixture.root.parent().unwrap().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let sentinel = outside.join("sentinel.txt");
        fs::write(&sentinel, b"unchanged").unwrap();
        symlink(&outside, fixture.root.join("etc")).unwrap();

        let result = manifest.stage(&repo_root(), &fixture.binaries, &fixture.root);
        assert!(matches!(result, Err(InstallError::Invalid(_))));
        assert_eq!(fs::read(&sentinel).unwrap(), b"unchanged");
        assert!(!outside.join("portus").exists());
    }

    #[test]
    fn secret_like_static_source_is_rejected() {
        assert!(scan_for_reusable_secret(b"OPENAI_API_KEY=sk-test", "fixture").is_err());
        assert!(scan_for_reusable_secret(b"-----BEGIN PRIVATE KEY-----", "fixture").is_err());
        assert!(scan_for_reusable_secret(b"provider_id = \"openai\"", "fixture").is_ok());
    }

    #[test]
    fn path_and_manifest_escape_attempts_fail_closed() {
        assert!(validate_relative_source("../secret").is_err());
        assert!(validate_linux_path("relative/path").is_err());
        assert!(validate_linux_path("/etc/../shadow").is_err());
        let text = fs::read_to_string(repo_root().join("runtime/install/install.toml")).unwrap();
        let invalid = text.replace(
            "target = \"/usr/bin/portus-os\"",
            "target = \"/usr/local/bin/portus-os\"",
        );
        assert!(InstallManifest::parse(&invalid).is_err());
    }

    #[test]
    fn openrc_templates_preserve_locked_identity_but_refuse_to_freeze_linux_behavior() {
        let manifest = manifest();
        for service in &manifest.services {
            let text = fs::read_to_string(repo_root().join(&service.template_source)).unwrap();
            validate_openrc_template(service, &text).unwrap();
            assert!(text.contains(OPENRC_RESOLUTION_MARKER));
            assert!(!text.contains("supervisor="));
            assert!(!text.contains("respawn_"));
            assert!(!text.contains("depend()"));
        }
    }

    #[test]
    fn locked_identity_and_directory_contracts_match_security_boundaries() {
        let manifest = manifest();
        require_identity(
            &manifest.identities,
            "portus-apid",
            Some("portus-api"),
            Some("portus-api"),
            Resolution::Locked,
        )
        .unwrap();
        require_identity(
            &manifest.identities,
            "portusd",
            None,
            None,
            Resolution::LinuxVerified,
        )
        .unwrap();
        require_directory(
            &manifest.directories,
            "/run/portus/protected-api",
            "portus-api",
            "portus-api-users",
            "02750",
        )
        .unwrap();
    }
}
