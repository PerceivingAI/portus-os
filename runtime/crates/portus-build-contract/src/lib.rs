//! Host-safe validator for the PortusOS W5 whole-image build contract graph.
//!
//! The validator deliberately distinguishes a structurally/semantically valid
//! source graph from a release-resolved graph. Windows is expected to prove the
//! former and fail the latter until Track L resolves Artix/package/service and
//! external-component pins.

use portus_browser_integration::PortusBrowserContract;
use portus_install::InstallManifest;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Component, Path},
};

pub const CONTRACT_SCHEMA_VERSION: u32 = 1;

const BUILD_CONTRACT: &str = "portusos-build/contracts/build.yaml";
const PACKAGE_CONTRACT: &str = "portusos-build/packages/packages.yaml";
const PACKAGE_LOCK: &str = "portusos-build/packages/packages.lock.yaml";
const CODEX_CONTRACT: &str = "portusos-build/components/codex.yaml";
const BROWSER_COMPONENT_CONTRACT: &str = "portusos-build/components/portus-browser.yaml";
const PORTUS_MCP_COMPONENT_CONTRACT: &str = "portusos-build/components/portus-mcp.yaml";
const TUNNEL_CLIENT_COMPONENT_CONTRACT: &str = "portusos-build/components/tunnel-client.yaml";
const STORAGE_CONTRACT: &str = "portusos-build/system/storage.yaml";
const SERVICE_CONTRACT: &str = "portusos-build/system/base-services.yaml";
const IDENTITY_CONTRACT: &str = "portusos-build/system/identities.yaml";
const VM_CONTRACT: &str = "portusos-build/system/vm-profiles.yaml";
const CALAMARES_CONTRACT: &str = "portusos-build/installer/calamares.yaml";
const ISO_CONTRACT: &str = "portusos-build/iso/profile.yaml";
const VALIDATION_MATRIX: &str = "portusos-build/validation/matrix.yaml";
const P16_INSTALL_CONTRACT: &str = "runtime/install/install.toml";
const P15_BROWSER_CONTRACT: &str = "runtime/integrations/portus-browser/integration.toml";
const P15_BROWSER_MANIFEST: &str = "runtime/integrations/manifests/portus-browser.toml";

const REQUIRED_SCHEMA_FILES: [&str; 9] = [
    "portusos-build/schemas/build-config.schema.json",
    "portusos-build/schemas/environment-preflight.schema.json",
    "portusos-build/schemas/package-source.schema.json",
    "portusos-build/schemas/package-lock.schema.json",
    "portusos-build/schemas/build-metadata.schema.json",
    "portusos-build/schemas/build-run.schema.json",
    "portusos-build/schemas/release-metadata.schema.json",
    "portusos-build/schemas/validation-result.schema.json",
    "portusos-build/schemas/validation-report.schema.json",
];

const REQUIRED_PACKAGE_IDS: [&str; 25] = [
    "artix-base",
    "linux-lts",
    "linux",
    "boot-storage-tooling",
    "dbus",
    "elogind",
    "networkmanager",
    "openssh",
    "nftables",
    "chrony",
    "syslog",
    "x11-i3-session",
    "tmux",
    "git",
    "curl-ca",
    "bubblewrap",
    "chromium",
    "calamares",
    "artools",
    "nodejs-npm",
    "portus-runtime",
    "portus-browser",
    "portus-mcp",
    "tunnel-client",
    "codex",
];

const REQUIRED_BASE_SERVICES: [&str; 7] = [
    "system-dbus",
    "elogind",
    "networkmanager",
    "openssh",
    "nftables",
    "chrony",
    "syslog",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Resolution {
    Locked,
    LinuxVerified,
    OwnerDecision,
    Generated,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceClass {
    OfficialArtix,
    PortusOwned,
    ApprovedExternal,
    ValidationOnly,
    HardwareSelected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublicRedistribution {
    PendingReview,
    Approved,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionClass {
    Automated,
    Assisted,
    Manual,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationEnvironment {
    Reference,
    Minimum,
    BuildHost,
    Recovery,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildContract {
    schema_version: u32,
    target: BuildTarget,
    contracts: BTreeMap<String, String>,
    schemas: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildTarget {
    architecture: String,
    firmware: String,
    secure_boot: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageSourceContract {
    schema_version: u32,
    source_policies: BTreeMap<SourceClass, SourcePolicy>,
    packages: Vec<PackageEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePolicy {
    verification: String,
    installation_owner: String,
    update_owner: String,
    failure_behavior: String,
    public_redistribution: PublicRedistribution,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageEntry {
    id: String,
    role: String,
    source_class: SourceClass,
    required_for_first_iso: bool,
    profile: String,
    #[serde(default)]
    package: Option<PackageResolution>,
    #[serde(default)]
    install_contract: Option<String>,
    #[serde(default)]
    component_contract: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageResolution {
    resolution: Resolution,
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    unresolved_reason: Option<String>,
    required_gate: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexContract {
    schema_version: u32,
    id: String,
    source_class: SourceClass,
    authority: String,
    distribution: CodexDistribution,
    pin: ResolvedValue,
    verification: CodexVerification,
    compatibility: CodexCompatibility,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexDistribution {
    kind: String,
    executable: String,
    package_root: String,
    visible_symlink_target: String,
    release_tag: String,
    target: String,
    package_asset: String,
    checksum_asset: String,
    auto_update_on_startup: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolvedValue {
    resolution: Resolution,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    evidence_baseline_version: Option<String>,
    #[serde(default)]
    unresolved_reason: Option<String>,
    required_gate: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexVerification {
    package_sha256: ResolvedValue,
    checksum_manifest_sha256: ResolvedValue,
    installed_version: ResolvedValue,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexCompatibility {
    artix: EvidenceResolution,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceResolution {
    resolution: Resolution,
    #[serde(default)]
    evidence_ref: Option<String>,
    unresolved_reason: String,
    required_gate: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserComponentContract {
    schema_version: u32,
    id: String,
    source_class: SourceClass,
    integration_contract: String,
    provider_manifest: String,
    source: BrowserSource,
    packaging: EvidenceResolution,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserSource {
    repository: String,
    revision: ResolvedValue,
    source_tree_required_clean: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortusMcpContract {
    schema_version: u32,
    id: String,
    source_class: SourceClass,
    authority: String,
    source: BrowserSource,
    runtime: PortusMcpRuntime,
    packaging: EvidenceResolution,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortusMcpRuntime {
    install_root: String,
    local_mcp_url: String,
    start_command: String,
    node_minimum: String,
    dependency_install_command: String,
    runtime_dev_dependencies_required: bool,
    policy_path: String,
    policy_source: String,
    local_launcher: String,
    local_launcher_source: String,
    tunnel_launcher: String,
    tunnel_launcher_source: String,
    default_project_root_template: String,
    subagents_enabled: bool,
    bundled_required: bool,
    setup_required: bool,
    lifecycle_owner: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TunnelClientContract {
    schema_version: u32,
    id: String,
    source_class: SourceClass,
    authority: String,
    source: TunnelClientSource,
    runtime: TunnelClientRuntime,
    codex_plugin: TunnelCodexPlugin,
    compatibility: TunnelClientCompatibility,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TunnelClientSource {
    repository: String,
    release: ResolvedValue,
    linux_amd64_asset: String,
    linux_amd64_sha256: ResolvedValue,
    licenses_asset: String,
    spdx_asset: String,
    provenance_asset: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TunnelClientRuntime {
    executable: String,
    default_profile: String,
    portus_mcp_url: String,
    control_plane_base_url: String,
    outbound_only: bool,
    bundled_required: bool,
    setup_required: bool,
    lifecycle_owner: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TunnelCodexPlugin {
    bundled_in_binary: bool,
    install_command: String,
    setup_required: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TunnelClientCompatibility {
    artix: EvidenceResolution,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StorageContract {
    schema_version: u32,
    resolution: Resolution,
    target: StorageTarget,
    partitions: StoragePartitions,
    boot: BootContract,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StorageTarget {
    architecture: String,
    firmware: String,
    partition_table: String,
    secure_boot: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoragePartitions {
    esp: Partition,
    boot: Partition,
    system: SystemPartition,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Partition {
    size_mib: u64,
    filesystem: String,
    mount: String,
    encrypted: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemPartition {
    encryption: EncryptionContract,
    lvm: LvmContract,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EncryptionContract {
    format: String,
    cipher: String,
    key_bits: u32,
    pbkdf: String,
    target_time_ms: u64,
    memory_limit_kib: u64,
    owner_keyslot_required: bool,
    recovery_keyslot_required: bool,
    automatic_unlock: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LvmContract {
    vg: String,
    root_filesystem: String,
    swap_mib: u64,
    free_reserve_percent: u8,
    split_home: bool,
    split_var: bool,
    split_srv: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootContract {
    bootloader: String,
    bootloader_id: String,
    esp_mount: String,
    default_kernel_role: String,
    alternate_kernel_role: String,
    menu_timeout_seconds: u8,
    fallback_efi_path: String,
    initramfs: InitramfsContract,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InitramfsContract {
    framework: String,
    systemd_hooks: bool,
    normal_and_fallback_for_both_kernels: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VmContracts {
    schema_version: u32,
    resolution: Resolution,
    profiles: BTreeMap<String, VmProfile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VmProfile {
    vcpu: u8,
    memory_mib: u64,
    disk_gib: u64,
    firmware: String,
    secure_boot: bool,
    network: String,
    three_d_acceleration_required: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceContract {
    schema_version: u32,
    portus_services: PortusServiceAuthority,
    services: Vec<BaseService>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortusServiceAuthority {
    authority: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaseService {
    id: String,
    role: String,
    lifecycle_owner: String,
    package_id: String,
    service_resolution: Resolution,
    #[serde(default)]
    service_name: Option<String>,
    runlevel_resolution: Resolution,
    #[serde(default)]
    runlevel: Option<String>,
    #[serde(default)]
    unresolved_reason: Option<String>,
    required_gate: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityContract {
    schema_version: u32,
    root_administration: RootAdministration,
    master_user: MasterUser,
    portus_service_identities: PortusServiceIdentityAuthority,
    non_root_administrator_account: OptionalAdministratorAccount,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootAdministration {
    uid: u32,
    ultimate_authority: bool,
    independent_from_master: bool,
    credential_source: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MasterUser {
    creation_owner: String,
    username_source: String,
    uid_resolution: Resolution,
    non_root_required: bool,
    private_home_required: bool,
    workspace_root_template: String,
    permission_bundle_source: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortusServiceIdentityAuthority {
    authority: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OptionalAdministratorAccount {
    required: bool,
    resolution: Resolution,
    unresolved_reason: String,
    required_gate: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalamaresPackageEvidence {
    repository: String,
    version: String,
    architecture: String,
    sha256: String,
    signature_signer: String,
    signing_key: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalamaresContract {
    schema_version: u32,
    framework: String,
    custom_modules_policy: String,
    ui_sequence: Vec<String>,
    inputs: BTreeMap<String, String>,
    package_evidence: CalamaresPackageEvidence,
    verified_modules: Vec<String>,
    module_set: ResolvedValue,
    storage_implementation: ResolvedValue,
    installed_target_validation: GeneratedAuthority,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratedAuthority {
    resolution: Resolution,
    source_authority: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IsoContract {
    schema_version: u32,
    framework: String,
    architecture: String,
    firmware: String,
    secure_boot: bool,
    installer: String,
    inputs: BTreeMap<String, String>,
    live_environment: LiveEnvironment,
    artools_profile: ResolvedValue,
    build_host: BuildHost,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LiveEnvironment {
    interactive_required: bool,
    networking_required: bool,
    master_user_required: bool,
    x11_i3_required: bool,
    alacritty_tmux_required: bool,
    codex_required: bool,
    chromium_required: bool,
    codex_chatgpt_browser_login_required: bool,
    portus_runtime_required: bool,
    portus_mcp_bundled_required: bool,
    tunnel_client_bundled_required: bool,
    tunnel_setup_optional: bool,
    calamares_required: bool,
    recovery_tools_required: bool,
    protected_credentials_preprovisioned: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildHost {
    distribution: String,
    architecture: String,
    native_required: bool,
    artix_build_context: String,
    artix_context_isolated_required: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationMatrix {
    schema_version: u32,
    authority: String,
    tests: Vec<ValidationTest>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationTest {
    id: String,
    #[serde(rename = "class")]
    execution_class: ExecutionClass,
    environment: ValidationEnvironment,
    blocking: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageLock {
    schema_version: u32,
    source_manifest_sha256: String,
    generated_at: String,
    artix: LockArtix,
    resolved: Vec<LockedPackageRole>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockArtix {
    architecture: String,
    repositories: Vec<BTreeMap<String, YamlValue>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockedPackageRole {
    id: String,
    source_class: SourceClass,
    artifacts: Vec<LockedArtifact>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockedArtifact {
    identity: String,
    version: String,
    #[serde(default)]
    repository: Option<String>,
    licenses: Vec<String>,
    #[serde(default)]
    sha256: Option<String>,
    verification: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UnresolvedItem {
    pub id: String,
    pub resolution: Resolution,
    pub required_gate: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContractReport {
    pub schema_version: u32,
    pub source_valid: bool,
    pub release_resolved: bool,
    pub files_checked: usize,
    pub validation_tests: usize,
    pub package_entries: usize,
    pub unresolved: Vec<UnresolvedItem>,
}

#[derive(Debug)]
pub enum ContractError {
    Io(std::io::Error),
    Parse { path: String, message: String },
    Invalid(String),
    Install(String),
    Browser(String),
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "W5 contract I/O error: {error}"),
            Self::Parse { path, message } => write!(f, "W5 parse error in {path}: {message}"),
            Self::Invalid(message) => write!(f, "invalid W5 contract: {message}"),
            Self::Install(message) => write!(f, "P16 install contract mismatch: {message}"),
            Self::Browser(message) => write!(f, "P15 PortusBrowser contract mismatch: {message}"),
        }
    }
}

impl std::error::Error for ContractError {}

impl From<std::io::Error> for ContractError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type ContractResult<T> = Result<T, ContractError>;

pub fn validate_repository(repo_root: &Path) -> ContractResult<ContractReport> {
    ensure_real_repo_root(repo_root)?;
    let build: BuildContract = load_yaml(repo_root, BUILD_CONTRACT)?;
    validate_build_contract(repo_root, &build)?;

    let packages: PackageSourceContract = load_yaml(repo_root, PACKAGE_CONTRACT)?;
    validate_package_source(repo_root, &packages)?;

    let codex: CodexContract = load_yaml(repo_root, CODEX_CONTRACT)?;
    validate_codex(repo_root, &codex)?;

    let browser_component: BrowserComponentContract =
        load_yaml(repo_root, BROWSER_COMPONENT_CONTRACT)?;
    validate_browser_component(repo_root, &browser_component)?;

    let portus_mcp: PortusMcpContract = load_yaml(repo_root, PORTUS_MCP_COMPONENT_CONTRACT)?;
    validate_portus_mcp(repo_root, &portus_mcp)?;

    let tunnel_client: TunnelClientContract =
        load_yaml(repo_root, TUNNEL_CLIENT_COMPONENT_CONTRACT)?;
    validate_tunnel_client(&tunnel_client)?;

    let storage: StorageContract = load_yaml(repo_root, STORAGE_CONTRACT)?;
    validate_storage(&storage)?;

    let vms: VmContracts = load_yaml(repo_root, VM_CONTRACT)?;
    validate_vm_profiles(&vms)?;

    let services: ServiceContract = load_yaml(repo_root, SERVICE_CONTRACT)?;
    validate_services(&services, &packages)?;

    let identities: IdentityContract = load_yaml(repo_root, IDENTITY_CONTRACT)?;
    validate_identities(&identities)?;

    let calamares: CalamaresContract = load_yaml(repo_root, CALAMARES_CONTRACT)?;
    validate_calamares(&calamares)?;

    let iso: IsoContract = load_yaml(repo_root, ISO_CONTRACT)?;
    validate_iso(&iso)?;

    let matrix: ValidationMatrix = load_yaml(repo_root, VALIDATION_MATRIX)?;
    validate_validation_matrix(repo_root, &matrix)?;

    let install = InstallManifest::load(&repo_root.join(P16_INSTALL_CONTRACT))
        .map_err(|error| ContractError::Install(error.to_string()))?;
    install
        .validate_sources(repo_root)
        .map_err(|error| ContractError::Install(error.to_string()))?;

    validate_schema_documents(repo_root)?;
    scan_machine_contracts_for_secrets(repo_root, &build)?;

    let mut unresolved = Vec::new();
    collect_package_unresolved(&packages, &mut unresolved);
    collect_codex_unresolved(&codex, &mut unresolved);
    collect_browser_unresolved(&browser_component, &mut unresolved);
    collect_portus_mcp_unresolved(&portus_mcp, &mut unresolved);
    collect_tunnel_client_unresolved(&tunnel_client, &mut unresolved);
    collect_service_unresolved(&services, &mut unresolved);
    collect_resolved_value(
        "calamares.module-set",
        &calamares.module_set,
        &mut unresolved,
    );
    collect_resolved_value(
        "calamares.storage-implementation",
        &calamares.storage_implementation,
        &mut unresolved,
    );
    collect_resolved_value("iso.artools-profile", &iso.artools_profile, &mut unresolved);

    if !install.release_ready() {
        unresolved.push(UnresolvedItem {
            id: "runtime.install".to_string(),
            resolution: Resolution::LinuxVerified,
            required_gate: "L2/L6".to_string(),
            reason: format!(
                "P16 install contract contains {} Linux-unresolved identity/service/filesystem items",
                install.unresolved_linux_items()
            ),
        });
    }

    if repo_root.join(PACKAGE_LOCK).exists() {
        validate_package_lock(repo_root, &packages)?;
    } else {
        unresolved.push(UnresolvedItem {
            id: "packages.lock".to_string(),
            resolution: Resolution::Generated,
            required_gate: "L2".to_string(),
            reason: "Artix-resolved packages.lock.yaml is intentionally generated only after repository/package verification".to_string(),
        });
    }

    unresolved.sort_by(|left, right| left.id.cmp(&right.id));
    unresolved.dedup_by(|left, right| left.id == right.id);

    Ok(ContractReport {
        schema_version: CONTRACT_SCHEMA_VERSION,
        source_valid: true,
        release_resolved: unresolved.is_empty(),
        files_checked: build.contracts.len() + build.schemas.len() + 1,
        validation_tests: matrix.tests.len(),
        package_entries: packages.packages.len(),
        unresolved,
    })
}

fn validate_build_contract(repo_root: &Path, contract: &BuildContract) -> ContractResult<()> {
    require_schema(contract.schema_version, BUILD_CONTRACT)?;
    if contract.target.architecture != "x86_64"
        || contract.target.firmware != "uefi"
        || contract.target.secure_boot
    {
        return invalid("build target must remain x86_64 UEFI with Secure Boot disabled");
    }
    let expected_contracts = BTreeMap::from([
        ("packages", PACKAGE_CONTRACT),
        ("codex", CODEX_CONTRACT),
        ("portus_browser", BROWSER_COMPONENT_CONTRACT),
        ("portus_mcp", PORTUS_MCP_COMPONENT_CONTRACT),
        ("tunnel_client", TUNNEL_CLIENT_COMPONENT_CONTRACT),
        ("portus_install", P16_INSTALL_CONTRACT),
        ("storage", STORAGE_CONTRACT),
        ("services", SERVICE_CONTRACT),
        ("identities", IDENTITY_CONTRACT),
        ("vm_profiles", VM_CONTRACT),
        ("calamares", CALAMARES_CONTRACT),
        ("iso", ISO_CONTRACT),
        ("validation", VALIDATION_MATRIX),
    ]);
    if contract.contracts.len() != expected_contracts.len() {
        return invalid("composition root must contain exactly the locked W5 contract set");
    }
    for (key, expected) in expected_contracts {
        let actual = contract
            .contracts
            .get(key)
            .ok_or_else(|| ContractError::Invalid(format!("build contract missing {key}")))?;
        if actual != expected {
            return invalid_owned(format!("build contract {key} must reference {expected}"));
        }
        validate_repo_reference(repo_root, actual)?;
    }
    let schema_paths: BTreeSet<_> = contract.schemas.values().map(String::as_str).collect();
    let expected_schemas: BTreeSet<_> = REQUIRED_SCHEMA_FILES.into_iter().collect();
    if schema_paths != expected_schemas {
        return invalid(
            "composition root schema references must match the nine first-ISO schemas exactly",
        );
    }
    for path in contract.schemas.values() {
        validate_repo_reference(repo_root, path)?;
    }
    Ok(())
}

fn validate_source_policies(policies: &BTreeMap<SourceClass, SourcePolicy>) -> ContractResult<()> {
    let expected_classes = BTreeSet::from([
        SourceClass::OfficialArtix,
        SourceClass::PortusOwned,
        SourceClass::ApprovedExternal,
        SourceClass::ValidationOnly,
        SourceClass::HardwareSelected,
    ]);
    let actual_classes: BTreeSet<_> = policies.keys().copied().collect();
    if actual_classes != expected_classes {
        return invalid("package source policies must match the finite no-AUR W5 vocabulary");
    }

    for (source_class, policy) in policies {
        validate_text(&policy.verification, "source verification")?;
        validate_text(&policy.installation_owner, "source installation owner")?;
        validate_text(&policy.update_owner, "source update owner")?;
        validate_text(&policy.failure_behavior, "source failure behavior")?;
        let expected = match source_class {
            SourceClass::OfficialArtix => (
                "artix-signature-keyring",
                "pacman-artools",
                "pacman",
                "fail-closed",
            ),
            SourceClass::PortusOwned => (
                "source-revision-local-package-hash",
                "portusos-build",
                "portusos-release",
                "fail-build",
            ),
            SourceClass::ApprovedExternal => (
                "component-contract",
                "component-contract",
                "explicit-component-workflow",
                "fail-build-or-degrade-per-contract",
            ),
            SourceClass::ValidationOnly => (
                "selected-source-contract",
                "validation-profile",
                "validation-profile",
                "fail-validation-profile",
            ),
            SourceClass::HardwareSelected => (
                "selected-source-contract",
                "hardware-profile",
                "source-owner",
                "fail-selected-profile",
            ),
        };
        if (
            policy.verification.as_str(),
            policy.installation_owner.as_str(),
            policy.update_owner.as_str(),
            policy.failure_behavior.as_str(),
        ) != expected
        {
            return invalid_owned(format!(
                "source policy {source_class:?} conflicts with the W5 package ownership contract"
            ));
        }
        if !matches!(
            policy.public_redistribution,
            PublicRedistribution::PendingReview
                | PublicRedistribution::Approved
                | PublicRedistribution::NotApplicable
        ) {
            return invalid("invalid public redistribution state");
        }
    }
    Ok(())
}

fn validate_package_source(
    repo_root: &Path,
    contract: &PackageSourceContract,
) -> ContractResult<()> {
    require_schema(contract.schema_version, PACKAGE_CONTRACT)?;
    validate_source_policies(&contract.source_policies)?;
    let mut ids = BTreeSet::new();
    for package in &contract.packages {
        validate_text(&package.id, "package id")?;
        validate_text(&package.role, "package role")?;
        validate_text(&package.profile, "package profile")?;
        if !ids.insert(package.id.as_str()) {
            return invalid_owned(format!("duplicate package id {}", package.id));
        }
        match package.source_class {
            SourceClass::OfficialArtix
            | SourceClass::ValidationOnly
            | SourceClass::HardwareSelected => {
                if package.package.is_none()
                    || package.install_contract.is_some()
                    || package.component_contract.is_some()
                {
                    return invalid_owned(format!(
                        "package {} must use only a package-resolution block",
                        package.id
                    ));
                }
            }
            SourceClass::PortusOwned | SourceClass::ApprovedExternal => {
                if package.package.is_some()
                    || usize::from(package.install_contract.is_some())
                        + usize::from(package.component_contract.is_some())
                        != 1
                {
                    return invalid_owned(format!(
                        "package {} must reference exactly one existing install/component contract",
                        package.id
                    ));
                }
            }
        }
        if let Some(resolution) = &package.package {
            validate_package_resolution(&package.id, resolution)?;
        }
        for reference in [
            package.install_contract.as_ref(),
            package.component_contract.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_repo_reference(repo_root, reference)?;
        }
        if !package.required_for_first_iso {
            return invalid_owned(format!(
                "current W5 package inventory item {} is not first-ISO-required; optional inventory belongs in a later profile expansion",
                package.id
            ));
        }
    }
    let chromium = contract
        .packages
        .iter()
        .find(|entry| entry.id == "chromium")
        .ok_or_else(|| {
            ContractError::Invalid("first-ISO Chromium package entry missing".to_string())
        })?;
    if chromium.role != "codex-chatgpt-auth-default-browser-and-portusbrowser-reference"
        || chromium.profile != "base"
    {
        return invalid(
            "Chromium must be a first-ISO base component for Codex ChatGPT browser authentication and PortusBrowser validation",
        );
    }
    let expected_ids: BTreeSet<_> = REQUIRED_PACKAGE_IDS.into_iter().collect();
    if ids != expected_ids {
        return invalid(
            "package contract must contain exactly the current first-ISO source-role inventory",
        );
    }
    require_package_reference(contract, "portus-runtime", P16_INSTALL_CONTRACT, true)?;
    require_package_reference(
        contract,
        "portus-browser",
        BROWSER_COMPONENT_CONTRACT,
        false,
    )?;
    require_package_reference(contract, "portus-mcp", PORTUS_MCP_COMPONENT_CONTRACT, false)?;
    require_package_reference(
        contract,
        "tunnel-client",
        TUNNEL_CLIENT_COMPONENT_CONTRACT,
        false,
    )?;
    require_package_reference(contract, "codex", CODEX_CONTRACT, false)?;
    Ok(())
}

fn validate_package_resolution(id: &str, value: &PackageResolution) -> ContractResult<()> {
    validate_gate(&value.required_gate)?;
    let unique_names: BTreeSet<_> = value.names.iter().map(String::as_str).collect();
    if unique_names.len() != value.names.len() {
        return invalid_owned(format!(
            "package role {id} contains duplicate selected package names"
        ));
    }
    for name in &value.names {
        validate_text(name, "selected package name")?;
        if name.chars().any(char::is_whitespace) {
            return invalid_owned(format!(
                "package role {id} contains an invalid whitespace-bearing package name"
            ));
        }
    }
    if value.resolution == Resolution::Locked {
        if value.names.is_empty() {
            return invalid_owned(format!(
                "locked package role {id} requires at least one package name"
            ));
        }
        if value.unresolved_reason.is_some() {
            return invalid_owned(format!(
                "locked package role {id} must not retain an unresolved reason"
            ));
        }
    } else {
        if !value.names.is_empty() {
            return invalid_owned(format!(
                "unresolved package role {id} must not preselect package names"
            ));
        }
        let reason = value.unresolved_reason.as_deref().ok_or_else(|| {
            ContractError::Invalid(format!("unresolved package role {id} requires a reason"))
        })?;
        validate_text(reason, "unresolved package reason")?;
    }
    Ok(())
}

fn require_package_reference(
    contract: &PackageSourceContract,
    id: &str,
    expected: &str,
    install: bool,
) -> ContractResult<()> {
    let package = contract
        .packages
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| ContractError::Invalid(format!("missing package entry {id}")))?;
    let actual = if install {
        package.install_contract.as_deref()
    } else {
        package.component_contract.as_deref()
    };
    if actual != Some(expected) {
        return invalid_owned(format!("package {id} must reference {expected}"));
    }
    Ok(())
}

fn validate_codex(repo_root: &Path, contract: &CodexContract) -> ContractResult<()> {
    require_schema(contract.schema_version, CODEX_CONTRACT)?;
    let selected_version = contract.pin.version.as_deref();
    if contract.id != "codex"
        || contract.source_class != SourceClass::ApprovedExternal
        || contract.authority != "CODEX_UPDATES.md"
        || contract.distribution.kind != "official-standalone-package"
        || contract.distribution.executable != "/usr/local/bin/codex"
        || contract.distribution.package_root
            != "/usr/lib/codex/standalone/releases/0.150.1-x86_64-unknown-linux-musl"
        || contract.distribution.visible_symlink_target
            != "../../lib/codex/standalone/releases/0.150.1-x86_64-unknown-linux-musl/bin/codex"
        || contract.distribution.release_tag != "rust-v0.150.1"
        || contract.distribution.target != "x86_64-unknown-linux-musl"
        || contract.distribution.package_asset != "codex-package-x86_64-unknown-linux-musl.tar.gz"
        || contract.distribution.checksum_asset != "codex-package_SHA256SUMS"
        || contract.distribution.auto_update_on_startup
        || contract.pin.resolution != Resolution::Locked
        || selected_version != Some("0.150.1")
    {
        return invalid(
            "Codex machine contract conflicts with the selected first-ISO standalone package",
        );
    }
    if contract.pin.evidence_baseline_version.as_deref() != Some("0.149.0") {
        return invalid(
            "Codex behavioral evidence baseline must remain the audited 0.149.0 source baseline",
        );
    }
    let codex_doc = fs::read_to_string(repo_root.join("CODEX_UPDATES.md"))?;
    if !codex_doc.contains("**Behavioral evidence baseline:** `0.149.0`")
        || !codex_doc.contains("**Selected first-ISO build pin:** `0.150.1`")
    {
        return invalid("Codex build pin/evidence baseline drifted from CODEX_UPDATES.md");
    }
    for (id, value) in [
        ("codex.pin", &contract.pin),
        (
            "codex.package-sha256",
            &contract.verification.package_sha256,
        ),
        (
            "codex.checksum-manifest-sha256",
            &contract.verification.checksum_manifest_sha256,
        ),
        (
            "codex.installed-version",
            &contract.verification.installed_version,
        ),
    ] {
        validate_resolved_value(id, value)?;
    }
    if contract.verification.package_sha256.value.as_deref()
        != Some("00aba704f029f6dc0d948be407a756e0c97cc840132fd691353b2c6b0a505b17")
        || contract
            .verification
            .checksum_manifest_sha256
            .value
            .as_deref()
            != Some("5580070dd9e1c06a603421111f32aa107fd77de2ab306986c11a26166b78e6fa")
    {
        return invalid(
            "Codex first-ISO package/checksum-manifest digest differs from verified 0.150.1 release evidence",
        );
    }
    validate_evidence_resolution("codex.artix-compatibility", &contract.compatibility.artix)?;
    Ok(())
}

fn validate_browser_component(
    repo_root: &Path,
    component: &BrowserComponentContract,
) -> ContractResult<()> {
    require_schema(component.schema_version, BROWSER_COMPONENT_CONTRACT)?;
    if component.id != "portus-browser"
        || component.source_class != SourceClass::PortusOwned
        || component.integration_contract != P15_BROWSER_CONTRACT
        || component.provider_manifest != P15_BROWSER_MANIFEST
        || component.source.repository != "https://github.com/PerceivingAI/portus-browser.git"
        || !component.source.source_tree_required_clean
    {
        return invalid("PortusBrowser component contract conflicts with P15 authorities");
    }
    validate_repo_reference(repo_root, &component.integration_contract)?;
    validate_repo_reference(repo_root, &component.provider_manifest)?;
    validate_resolved_value("portus-browser.source-revision", &component.source.revision)?;
    validate_evidence_resolution("portus-browser.packaging", &component.packaging)?;

    let actual =
        PortusBrowserContract::parse(&fs::read_to_string(repo_root.join(P15_BROWSER_CONTRACT))?)
            .map_err(|error| ContractError::Browser(error.to_string()))?;
    if actual.source_repository != component.source.repository {
        return invalid("PortusBrowser repository differs between W5 and P15");
    }
    if actual.release_pin_ready() {
        if component.source.revision.value.as_deref() != actual.source_revision.as_deref() {
            return invalid("PortusBrowser W5 revision must exactly match the pinned P15 revision");
        }
    } else if component.source.revision.value.is_some() {
        return invalid("W5 must not invent a PortusBrowser revision while P15 is not pinned");
    }
    Ok(())
}

fn validate_portus_mcp(repo_root: &Path, contract: &PortusMcpContract) -> ContractResult<()> {
    require_schema(contract.schema_version, PORTUS_MCP_COMPONENT_CONTRACT)?;
    if contract.id != "portus-mcp"
        || contract.source_class != SourceClass::PortusOwned
        || contract.authority != "TUNNEL_INSTRUCTIONS.md"
        || contract.source.repository != "https://github.com/PerceivingAI/portus-mcp.git"
        || !contract.source.source_tree_required_clean
        || contract.runtime.install_root != "/opt/portus/portus-mcp"
        || contract.runtime.local_mcp_url != "http://127.0.0.1:8789/mcp"
        || contract.runtime.start_command != "npm run start:tunnel"
        || contract.runtime.node_minimum != "20.9.0"
        || contract.runtime.dependency_install_command != "npm ci"
        || !contract.runtime.runtime_dev_dependencies_required
        || contract.runtime.policy_path != "/etc/portus/portus-mcp/policy.json"
        || contract.runtime.policy_source
            != "portusos-build/rootfs/overlay/etc/portus/portus-mcp/policy.json"
        || contract.runtime.local_launcher != "/usr/local/bin/portus-mcp-local"
        || contract.runtime.local_launcher_source
            != "portusos-build/rootfs/overlay/usr/local/bin/portus-mcp-local"
        || contract.runtime.tunnel_launcher != "/usr/local/bin/portus-tunnel-setup"
        || contract.runtime.tunnel_launcher_source
            != "portusos-build/rootfs/overlay/usr/local/bin/portus-tunnel-setup"
        || contract.runtime.default_project_root_template != "/workspace/{user}/master"
        || contract.runtime.subagents_enabled
        || !contract.runtime.bundled_required
        || contract.runtime.setup_required
        || contract.runtime.lifecycle_owner != "master-session"
    {
        return invalid(
            "Portus MCP component contract conflicts with the first-ISO tunnel authority",
        );
    }
    validate_repo_reference(repo_root, &contract.runtime.policy_source)?;
    validate_repo_reference(repo_root, &contract.runtime.local_launcher_source)?;
    validate_repo_reference(repo_root, &contract.runtime.tunnel_launcher_source)?;
    validate_resolved_value("portus-mcp.source-revision", &contract.source.revision)?;
    if let Some(value) = contract.source.revision.value.as_deref()
        && !is_lower_hex(value, 40)
    {
        return invalid("Portus MCP source revision must be a 40-character lowercase Git SHA-1");
    }
    validate_evidence_resolution("portus-mcp.packaging", &contract.packaging)?;
    Ok(())
}

fn validate_tunnel_client(contract: &TunnelClientContract) -> ContractResult<()> {
    require_schema(contract.schema_version, TUNNEL_CLIENT_COMPONENT_CONTRACT)?;
    if contract.id != "tunnel-client"
        || contract.source_class != SourceClass::ApprovedExternal
        || contract.authority != "TUNNEL_INSTRUCTIONS.md"
        || contract.source.repository != "https://github.com/openai/tunnel-client.git"
        || contract.runtime.executable != "/usr/local/bin/tunnel-client"
        || contract.runtime.default_profile != "portus-local"
        || contract.runtime.portus_mcp_url != "http://127.0.0.1:8789/mcp"
        || contract.runtime.control_plane_base_url != "https://api.openai.com"
        || !contract.runtime.outbound_only
        || !contract.runtime.bundled_required
        || contract.runtime.setup_required
        || contract.runtime.lifecycle_owner != "master-session"
        || !contract.codex_plugin.bundled_in_binary
        || contract.codex_plugin.install_command != "tunnel-client codex plugin install"
        || contract.codex_plugin.setup_required
    {
        return invalid(
            "tunnel-client component contract conflicts with the first-ISO tunnel authority",
        );
    }
    validate_resolved_value("tunnel-client.release", &contract.source.release)?;
    validate_resolved_value(
        "tunnel-client.linux-amd64-sha256",
        &contract.source.linux_amd64_sha256,
    )?;
    let version = contract.source.release.version.as_deref().ok_or_else(|| {
        ContractError::Invalid("locked tunnel-client release requires version".to_string())
    })?;
    let expected_prefix = format!("tunnel-client-v{version}-linux-amd64");
    if contract.source.linux_amd64_asset != format!("{expected_prefix}.zip")
        || contract.source.licenses_asset != format!("{expected_prefix}-licenses.txt")
        || contract.source.spdx_asset != format!("{expected_prefix}.spdx.json")
        || contract.source.provenance_asset
            != format!("tunnel-client-v{version}-provenance.sigstore.json")
    {
        return invalid("tunnel-client release asset names must match the pinned version");
    }
    let sha256 = contract
        .source
        .linux_amd64_sha256
        .value
        .as_deref()
        .ok_or_else(|| {
            ContractError::Invalid("locked tunnel-client Linux asset requires SHA-256".to_string())
        })?;
    if !is_lower_hex(sha256, 64) {
        return invalid(
            "tunnel-client Linux asset SHA-256 must be 64 lowercase hexadecimal characters",
        );
    }
    validate_evidence_resolution(
        "tunnel-client.artix-compatibility",
        &contract.compatibility.artix,
    )?;
    Ok(())
}

fn validate_storage(contract: &StorageContract) -> ContractResult<()> {
    require_schema(contract.schema_version, STORAGE_CONTRACT)?;
    if contract.resolution != Resolution::Locked
        || contract.target.architecture != "x86_64"
        || contract.target.firmware != "uefi"
        || contract.target.partition_table != "gpt"
        || contract.target.secure_boot
        || contract.partitions.esp.size_mib != 512
        || contract.partitions.esp.filesystem != "fat32"
        || contract.partitions.esp.mount != "/boot/efi"
        || contract.partitions.esp.encrypted
        || contract.partitions.boot.size_mib != 2048
        || contract.partitions.boot.filesystem != "ext4"
        || contract.partitions.boot.mount != "/boot"
        || contract.partitions.boot.encrypted
    {
        return invalid("storage topology differs from the locked first-ISO authority");
    }
    let encryption = &contract.partitions.system.encryption;
    if encryption.format != "luks2"
        || encryption.cipher != "aes-xts-plain64"
        || encryption.key_bits != 512
        || encryption.pbkdf != "argon2id"
        || encryption.target_time_ms != 2000
        || encryption.memory_limit_kib != 262_144
        || !encryption.owner_keyslot_required
        || !encryption.recovery_keyslot_required
        || encryption.automatic_unlock
    {
        return invalid("LUKS2 contract differs from the locked first-ISO authority");
    }
    let lvm = &contract.partitions.system.lvm;
    if lvm.vg != "portus"
        || lvm.root_filesystem != "ext4"
        || lvm.swap_mib != 4096
        || lvm.free_reserve_percent != 5
        || lvm.split_home
        || lvm.split_var
        || lvm.split_srv
    {
        return invalid("LVM/root/swap contract differs from the locked first-ISO authority");
    }
    let boot = &contract.boot;
    if boot.bootloader != "grub-uefi"
        || boot.bootloader_id != "PortusOS"
        || boot.esp_mount != "/boot/efi"
        || boot.default_kernel_role != "linux-lts"
        || boot.alternate_kernel_role != "linux"
        || boot.menu_timeout_seconds != 5
        || boot.fallback_efi_path != "EFI/BOOT/BOOTX64.EFI"
        || boot.initramfs.framework != "mkinitcpio"
        || boot.initramfs.systemd_hooks
        || !boot.initramfs.normal_and_fallback_for_both_kernels
    {
        return invalid("boot/initramfs contract differs from the locked first-ISO authority");
    }
    Ok(())
}

fn validate_vm_profiles(contract: &VmContracts) -> ContractResult<()> {
    require_schema(contract.schema_version, VM_CONTRACT)?;
    if contract.resolution != Resolution::Locked || contract.profiles.len() != 2 {
        return invalid("VM profile contract must contain only locked minimum/reference profiles");
    }
    let minimum = contract
        .profiles
        .get("minimum")
        .ok_or_else(|| ContractError::Invalid("minimum VM profile missing".to_string()))?;
    let reference = contract
        .profiles
        .get("reference")
        .ok_or_else(|| ContractError::Invalid("reference VM profile missing".to_string()))?;
    validate_vm_profile(minimum, 2, 4096, 40)?;
    validate_vm_profile(reference, 4, 8192, 80)
}

fn validate_vm_profile(
    profile: &VmProfile,
    vcpu: u8,
    memory: u64,
    disk: u64,
) -> ContractResult<()> {
    if profile.vcpu != vcpu
        || profile.memory_mib != memory
        || profile.disk_gib != disk
        || profile.firmware != "uefi"
        || profile.secure_boot
        || profile.network != "nat"
        || profile.three_d_acceleration_required
    {
        return invalid("VM profile differs from docs/VALIDATION.md locked values");
    }
    Ok(())
}

fn validate_services(
    contract: &ServiceContract,
    packages: &PackageSourceContract,
) -> ContractResult<()> {
    require_schema(contract.schema_version, SERVICE_CONTRACT)?;
    if contract.portus_services.authority != P16_INSTALL_CONTRACT {
        return invalid("Portus machine-service facts must be consumed from P16 install.toml");
    }
    let ids: BTreeSet<_> = contract
        .services
        .iter()
        .map(|service| service.id.as_str())
        .collect();
    let expected: BTreeSet<_> = REQUIRED_BASE_SERVICES.into_iter().collect();
    if ids != expected {
        return invalid(
            "base service contract must contain exactly the selected first-ISO machine services",
        );
    }
    for service in &contract.services {
        validate_text(&service.role, "service role")?;
        let package = packages
            .packages
            .iter()
            .find(|package| package.id == service.package_id)
            .ok_or_else(|| {
                ContractError::Invalid(format!(
                    "base service {} references unknown package {}",
                    service.id, service.package_id
                ))
            })?;
        if package.source_class != SourceClass::OfficialArtix || service.lifecycle_owner != "openrc"
        {
            return invalid_owned(format!(
                "base service {} must remain OpenRC-owned and reference an official Artix package role",
                service.id
            ));
        }
        validate_gate(&service.required_gate)?;
        if service.service_resolution == Resolution::Locked
            && service.runlevel_resolution == Resolution::Locked
        {
            validate_text(
                service.service_name.as_deref().unwrap_or_default(),
                "locked OpenRC service name",
            )?;
            validate_text(
                service.runlevel.as_deref().unwrap_or_default(),
                "locked OpenRC runlevel",
            )?;
            if service.unresolved_reason.is_some() {
                return invalid_owned(format!(
                    "locked base service {} must not retain an unresolved reason",
                    service.id
                ));
            }
        } else {
            if service.service_name.is_some() || service.runlevel.is_some() {
                return invalid_owned(format!(
                    "unresolved base service {} must not claim concrete service/runlevel names",
                    service.id
                ));
            }
            validate_text(
                service.unresolved_reason.as_deref().unwrap_or_default(),
                "service unresolved reason",
            )?;
        }
    }
    Ok(())
}

fn validate_identities(contract: &IdentityContract) -> ContractResult<()> {
    require_schema(contract.schema_version, IDENTITY_CONTRACT)?;
    let root = &contract.root_administration;
    if root.uid != 0
        || !root.ultimate_authority
        || !root.independent_from_master
        || root.credential_source != "installer-owner-input"
    {
        return invalid("root administration contract must preserve independent UID-0 authority");
    }

    let master = &contract.master_user;
    if master.creation_owner != "installer"
        || master.username_source != "installer-owner-input"
        || master.uid_resolution != Resolution::Generated
        || !master.non_root_required
        || !master.private_home_required
        || master.workspace_root_template != "/workspace/{user}"
        || master.permission_bundle_source != "runtime/install/policy/bundles"
    {
        return invalid(
            "Master identity contract conflicts with the locked non-root/user-scoped policy model",
        );
    }

    if contract.portus_service_identities.authority != P16_INSTALL_CONTRACT {
        return invalid("Portus service identities must remain owned by P16 install.toml");
    }

    let optional_admin = &contract.non_root_administrator_account;
    if optional_admin.required
        || optional_admin.resolution != Resolution::LinuxVerified
        || optional_admin.required_gate != "L6"
    {
        return invalid(
            "optional non-root administrator account must remain optional Linux verification",
        );
    }
    validate_text(
        &optional_admin.unresolved_reason,
        "optional administrator unresolved reason",
    )?;
    Ok(())
}

fn validate_calamares(contract: &CalamaresContract) -> ContractResult<()> {
    require_schema(contract.schema_version, CALAMARES_CONTRACT)?;
    let expected_sequence = [
        "welcome",
        "locale",
        "keyboard",
        "user-credentials",
        "storage-recovery",
        "summary-destructive-review",
        "install",
        "completion-reboot",
    ];
    if contract.framework != "calamares"
        || contract.custom_modules_policy != "verified-gap-only"
        || contract
            .ui_sequence
            .iter()
            .map(String::as_str)
            .ne(expected_sequence)
        || contract.installed_target_validation.resolution != Resolution::Generated
        || contract.installed_target_validation.source_authority != "docs/VALIDATION.md"
    {
        return invalid("Calamares contract differs from the locked installer authority");
    }
    let expected_inputs = BTreeMap::from([
        ("storage", STORAGE_CONTRACT),
        ("packages", PACKAGE_CONTRACT),
        ("portus_install", P16_INSTALL_CONTRACT),
        ("services", SERVICE_CONTRACT),
        ("identities", IDENTITY_CONTRACT),
    ]);
    validate_reference_map(&contract.inputs, &expected_inputs, "Calamares")?;
    if contract.package_evidence.repository != "world"
        || contract.package_evidence.version != "3.4.2-4"
        || contract.package_evidence.architecture != "x86_64"
        || contract.package_evidence.sha256
            != "4e8e70ebd9a4f6834b7c592ac698c14d6709aef6b0391f3d8673a4b0ab06130f"
        || contract.package_evidence.signature_signer != "Artix Buildbot <buildbot@artixlinux.org>"
        || contract.package_evidence.signing_key != "0A3EB6BB142C56653300420C1247D995F165BBAC"
    {
        return invalid("Calamares package evidence differs from the verified Artix package");
    }
    let expected_modules = [
        "welcome",
        "locale",
        "keyboard",
        "partition",
        "users",
        "notesqml",
        "summary",
        "mount",
        "unpackfs",
        "machineid",
        "localecfg",
        "fstab",
        "initcpiocfg",
        "initcpio",
        "networkcfg",
        "hwclock",
        "services-openrc",
        "openrcdmcryptcfg",
        "grubcfg",
        "bootloader",
        "shellprocess",
        "umount",
        "finished",
    ];
    if contract
        .verified_modules
        .iter()
        .map(String::as_str)
        .ne(expected_modules)
    {
        return invalid("Calamares verified module set differs from signed Artix package evidence");
    }
    validate_resolved_value("calamares.module-set", &contract.module_set)?;
    if contract.module_set.resolution != Resolution::Locked
        || contract.module_set.version.as_deref() != Some("3.4.2-4")
        || contract.module_set.value.as_deref()
            != Some("artix-3.4.2-4-stock-notesqml-plus-portus-storage-v2")
    {
        return invalid("Calamares module set must bind the verified Artix 3.4.2-4 package");
    }
    validate_resolved_value(
        "calamares.storage-implementation",
        &contract.storage_implementation,
    )?;
    if contract.storage_implementation.resolution != Resolution::LinuxVerified {
        return invalid("Calamares storage implementation must remain Linux/VM verified");
    }
    Ok(())
}

fn validate_iso(contract: &IsoContract) -> ContractResult<()> {
    require_schema(contract.schema_version, ISO_CONTRACT)?;
    if contract.framework != "artools"
        || contract.architecture != "x86_64"
        || contract.firmware != "uefi"
        || contract.secure_boot
        || contract.installer != "calamares"
        || !contract.live_environment.interactive_required
        || !contract.live_environment.networking_required
        || !contract.live_environment.master_user_required
        || !contract.live_environment.x11_i3_required
        || !contract.live_environment.alacritty_tmux_required
        || !contract.live_environment.codex_required
        || !contract.live_environment.chromium_required
        || !contract
            .live_environment
            .codex_chatgpt_browser_login_required
        || !contract.live_environment.portus_runtime_required
        || !contract.live_environment.portus_mcp_bundled_required
        || !contract.live_environment.tunnel_client_bundled_required
        || !contract.live_environment.tunnel_setup_optional
        || !contract.live_environment.calamares_required
        || !contract.live_environment.recovery_tools_required
        || contract
            .live_environment
            .protected_credentials_preprovisioned
        || contract.build_host.distribution != "Linux"
        || contract.build_host.architecture != "x86_64"
        || !contract.build_host.native_required
        || contract.build_host.artix_build_context != "Artix Linux"
        || !contract.build_host.artix_context_isolated_required
    {
        return invalid("ISO profile contract differs from the locked build authority");
    }
    let expected_inputs = BTreeMap::from([
        ("packages", PACKAGE_CONTRACT),
        ("storage", STORAGE_CONTRACT),
        ("vm_profiles", VM_CONTRACT),
        ("calamares", CALAMARES_CONTRACT),
        ("portus_install", P16_INSTALL_CONTRACT),
    ]);
    validate_reference_map(&contract.inputs, &expected_inputs, "ISO")?;
    validate_resolved_value("iso.artools-profile", &contract.artools_profile)
}

fn validate_reference_map(
    actual: &BTreeMap<String, String>,
    expected: &BTreeMap<&str, &str>,
    label: &str,
) -> ContractResult<()> {
    if actual.len() != expected.len() {
        return invalid_owned(format!("{label} input set has unexpected entries"));
    }
    for (key, expected_path) in expected {
        if actual.get(*key).map(String::as_str) != Some(*expected_path) {
            return invalid_owned(format!(
                "{label} input {key} must reference {expected_path}"
            ));
        }
    }
    Ok(())
}

fn validate_validation_matrix(repo_root: &Path, matrix: &ValidationMatrix) -> ContractResult<()> {
    require_schema(matrix.schema_version, VALIDATION_MATRIX)?;
    if matrix.authority != "docs/VALIDATION.md" || matrix.tests.len() != 38 {
        return invalid(
            "validation matrix must contain exactly ISO-01..ISO-38 from docs/VALIDATION.md",
        );
    }
    let authority = fs::read_to_string(repo_root.join("docs/VALIDATION.md"))?;
    let mut ids = BTreeSet::new();
    for (index, test) in matrix.tests.iter().enumerate() {
        let expected_id = format!("ISO-{:02}", index + 1);
        if test.id != expected_id || !test.blocking || !ids.insert(test.id.as_str()) {
            return invalid(
                "validation matrix IDs must be unique, ordered ISO-01..ISO-38 and blocking",
            );
        }
        let authority_class = match test.execution_class {
            ExecutionClass::Automated => "Automated",
            ExecutionClass::Assisted => "Assisted",
            ExecutionClass::Manual => "Manual",
        };
        if !authority.contains(&format!("| {} | {} |", test.id, authority_class)) {
            return invalid_owned(format!(
                "validation matrix class for {} differs from docs/VALIDATION.md",
                test.id
            ));
        }
    }
    if matrix.tests[35].environment != ValidationEnvironment::Recovery
        || matrix.tests[36].environment != ValidationEnvironment::Minimum
        || matrix.tests[37].environment != ValidationEnvironment::Reference
    {
        return invalid(
            "ISO-36/37/38 validation environments must remain recovery/minimum/reference",
        );
    }
    Ok(())
}

fn validate_schema_documents(repo_root: &Path) -> ContractResult<()> {
    for path in REQUIRED_SCHEMA_FILES {
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(repo_root.join(path))?).map_err(|error| {
                ContractError::Parse {
                    path: path.to_string(),
                    message: error.to_string(),
                }
            })?;
        if value.get("$schema").and_then(serde_json::Value::as_str)
            != Some("https://json-schema.org/draft/2020-12/schema")
            || value.get("type").and_then(serde_json::Value::as_str) != Some("object")
            || value
                .get("additionalProperties")
                .and_then(serde_json::Value::as_bool)
                != Some(false)
        {
            return invalid_owned(format!(
                "schema {path} must be strict draft-2020-12 object schema"
            ));
        }
    }
    Ok(())
}

fn validate_package_lock(repo_root: &Path, packages: &PackageSourceContract) -> ContractResult<()> {
    let lock: PackageLock = load_yaml(repo_root, PACKAGE_LOCK)?;
    let expected_hash = hex_sha256(&fs::read(repo_root.join(PACKAGE_CONTRACT))?);
    validate_package_lock_data(&lock, packages, &expected_hash)
}

fn validate_package_lock_data(
    lock: &PackageLock,
    packages: &PackageSourceContract,
    expected_hash: &str,
) -> ContractResult<()> {
    require_schema(lock.schema_version, PACKAGE_LOCK)?;
    validate_text(&lock.generated_at, "package lock generated_at")?;
    if lock.artix.architecture != "x86_64" || lock.artix.repositories.is_empty() {
        return invalid("package lock must identify x86_64 Artix repositories");
    }
    if lock.source_manifest_sha256 != expected_hash {
        return invalid("package lock source_manifest_sha256 does not match packages.yaml");
    }
    let required_ids: BTreeSet<_> = packages
        .packages
        .iter()
        .filter(|entry| entry.required_for_first_iso)
        .map(|entry| entry.id.as_str())
        .collect();
    let resolved_ids: BTreeSet<_> = lock
        .resolved
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    if required_ids != resolved_ids {
        return invalid(
            "package lock must resolve every and only current first-ISO package/source entry",
        );
    }
    for entry in &lock.resolved {
        let source = packages
            .packages
            .iter()
            .find(|package| package.id == entry.id)
            .ok_or_else(|| {
                ContractError::Invalid(format!("package lock references unknown role {}", entry.id))
            })?;
        if source.source_class != entry.source_class {
            return invalid_owned(format!(
                "package lock source class differs from packages.yaml for {}",
                entry.id
            ));
        }
        if entry.artifacts.is_empty() {
            return invalid_owned(format!(
                "package lock role {} must contain at least one concrete artifact",
                entry.id
            ));
        }
        let mut artifact_ids = BTreeSet::new();
        for artifact in &entry.artifacts {
            validate_text(&artifact.identity, "locked package identity")?;
            validate_text(&artifact.version, "locked package version")?;
            validate_text(&artifact.verification, "locked package verification")?;
            if !artifact_ids.insert(artifact.identity.as_str()) {
                return invalid_owned(format!(
                    "package lock role {} contains duplicate artifact identity {}",
                    entry.id, artifact.identity
                ));
            }
            if artifact.licenses.is_empty() {
                return invalid_owned(format!(
                    "package lock artifact {} requires at least one licence",
                    artifact.identity
                ));
            }
            let mut licenses = BTreeSet::new();
            for license in &artifact.licenses {
                validate_text(license, "locked package licence")?;
                if !licenses.insert(license.as_str()) {
                    return invalid_owned(format!(
                        "package lock artifact {} contains duplicate licence {}",
                        artifact.identity, license
                    ));
                }
            }
            if let Some(repository) = &artifact.repository {
                validate_text(repository, "locked repository")?;
            }
            if let Some(sha256) = &artifact.sha256 {
                if !is_lower_hex(sha256, 64) {
                    return invalid(
                        "package lock SHA-256 must be 64 lowercase hexadecimal characters",
                    );
                }
            }
            if entry.source_class == SourceClass::OfficialArtix && artifact.repository.is_none() {
                return invalid("official Artix locked package requires repository identity");
            }
        }
        if entry.source_class == SourceClass::OfficialArtix {
            let selected = source.package.as_ref().ok_or_else(|| {
                ContractError::Invalid(format!(
                    "official Artix package role {} has no package selection",
                    entry.id
                ))
            })?;
            if selected.resolution != Resolution::Locked {
                return invalid_owned(format!(
                    "official Artix package role {} must be locked before package-lock generation",
                    entry.id
                ));
            }
            let expected: BTreeSet<_> = selected.names.iter().map(String::as_str).collect();
            if artifact_ids != expected {
                return invalid_owned(format!(
                    "package lock artifacts do not exactly match selected Artix packages for {}",
                    entry.id
                ));
            }
        }
    }
    Ok(())
}

fn scan_machine_contracts_for_secrets(
    repo_root: &Path,
    build: &BuildContract,
) -> ContractResult<()> {
    let mut paths: BTreeSet<String> = build.contracts.values().cloned().collect();
    paths.extend(build.schemas.values().cloned());
    paths.insert(BUILD_CONTRACT.to_string());
    paths.insert(BROWSER_COMPONENT_CONTRACT.to_string());
    paths.insert(PORTUS_MCP_COMPONENT_CONTRACT.to_string());
    paths.insert(TUNNEL_CLIENT_COMPONENT_CONTRACT.to_string());
    paths.insert(CODEX_CONTRACT.to_string());
    paths.insert(PACKAGE_CONTRACT.to_string());
    paths.insert(STORAGE_CONTRACT.to_string());
    paths.insert(SERVICE_CONTRACT.to_string());
    paths.insert(VM_CONTRACT.to_string());
    paths.insert(CALAMARES_CONTRACT.to_string());
    paths.insert(ISO_CONTRACT.to_string());
    paths.insert(VALIDATION_MATRIX.to_string());

    for path in paths {
        let bytes = fs::read(repo_root.join(&path))?;
        let lower = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
        for marker in [
            "-----begin private key-----",
            "authorization: bearer ",
            "authorization: basic ",
            "sk-proj-",
            "ghp_",
        ] {
            if lower.contains(marker) {
                return invalid_owned(format!(
                    "machine contract {path} contains secret-like material"
                ));
            }
        }
        if path.ends_with(".yaml") {
            let value: YamlValue =
                serde_yaml::from_slice(&bytes).map_err(|error| ContractError::Parse {
                    path: path.clone(),
                    message: error.to_string(),
                })?;
            reject_secret_keys(&value, &path)?;
        }
    }
    Ok(())
}

fn reject_secret_keys(value: &YamlValue, path: &str) -> ContractResult<()> {
    match value {
        YamlValue::Mapping(mapping) => {
            for (key, child) in mapping {
                if let Some(key) = key.as_str() {
                    let normalized = key.to_ascii_lowercase().replace('-', "_");
                    if matches!(
                        normalized.as_str(),
                        "password"
                            | "token"
                            | "secret"
                            | "api_key"
                            | "authorization"
                            | "credential_value"
                    ) {
                        return invalid_owned(format!(
                            "machine contract {path} contains forbidden secret-bearing field {key}"
                        ));
                    }
                }
                reject_secret_keys(child, path)?;
            }
        }
        YamlValue::Sequence(sequence) => {
            for child in sequence {
                reject_secret_keys(child, path)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_package_unresolved(contract: &PackageSourceContract, output: &mut Vec<UnresolvedItem>) {
    for package in &contract.packages {
        if let Some(resolution) = &package.package {
            if resolution.resolution != Resolution::Locked {
                output.push(UnresolvedItem {
                    id: format!("package.{}", package.id),
                    resolution: resolution.resolution,
                    required_gate: resolution.required_gate.clone(),
                    reason: resolution
                        .unresolved_reason
                        .clone()
                        .expect("validated unresolved package resolution requires a reason"),
                });
            }
        }
    }
}

fn collect_codex_unresolved(contract: &CodexContract, output: &mut Vec<UnresolvedItem>) {
    for (id, value) in [
        ("codex.pin", &contract.pin),
        (
            "codex.package-sha256",
            &contract.verification.package_sha256,
        ),
        (
            "codex.checksum-manifest-sha256",
            &contract.verification.checksum_manifest_sha256,
        ),
        (
            "codex.installed-version",
            &contract.verification.installed_version,
        ),
    ] {
        collect_resolved_value(id, value, output);
    }
    collect_evidence_resolution(
        "codex.artix-compatibility",
        &contract.compatibility.artix,
        output,
    );
}

fn collect_browser_unresolved(
    contract: &BrowserComponentContract,
    output: &mut Vec<UnresolvedItem>,
) {
    collect_resolved_value(
        "portus-browser.source-revision",
        &contract.source.revision,
        output,
    );
    collect_evidence_resolution("portus-browser.packaging", &contract.packaging, output);
}

fn collect_portus_mcp_unresolved(contract: &PortusMcpContract, output: &mut Vec<UnresolvedItem>) {
    collect_resolved_value(
        "portus-mcp.source-revision",
        &contract.source.revision,
        output,
    );
    collect_evidence_resolution("portus-mcp.packaging", &contract.packaging, output);
}

fn collect_tunnel_client_unresolved(
    contract: &TunnelClientContract,
    output: &mut Vec<UnresolvedItem>,
) {
    collect_resolved_value("tunnel-client.release", &contract.source.release, output);
    collect_resolved_value(
        "tunnel-client.linux-amd64-sha256",
        &contract.source.linux_amd64_sha256,
        output,
    );
    collect_evidence_resolution(
        "tunnel-client.artix-compatibility",
        &contract.compatibility.artix,
        output,
    );
}

fn collect_service_unresolved(contract: &ServiceContract, output: &mut Vec<UnresolvedItem>) {
    for service in &contract.services {
        if service.service_resolution != Resolution::Locked
            || service.runlevel_resolution != Resolution::Locked
        {
            output.push(UnresolvedItem {
                id: format!("service.{}", service.id),
                resolution: if service.service_resolution != Resolution::Locked {
                    service.service_resolution
                } else {
                    service.runlevel_resolution
                },
                required_gate: service.required_gate.clone(),
                reason: service
                    .unresolved_reason
                    .clone()
                    .expect("validated unresolved service requires a reason"),
            });
        }
    }
}

fn collect_resolved_value(id: &str, value: &ResolvedValue, output: &mut Vec<UnresolvedItem>) {
    let resolved = match value.resolution {
        Resolution::Locked => value.version.is_some() || value.value.is_some(),
        Resolution::Generated | Resolution::LinuxVerified | Resolution::OwnerDecision => false,
    };
    if !resolved {
        output.push(UnresolvedItem {
            id: id.to_string(),
            resolution: value.resolution,
            required_gate: value.required_gate.clone(),
            reason: value
                .unresolved_reason
                .clone()
                .unwrap_or_else(|| "value is generated or not yet resolved".to_string()),
        });
    }
}

fn collect_evidence_resolution(
    id: &str,
    value: &EvidenceResolution,
    output: &mut Vec<UnresolvedItem>,
) {
    if value.resolution != Resolution::Locked || value.evidence_ref.is_none() {
        output.push(UnresolvedItem {
            id: id.to_string(),
            resolution: value.resolution,
            required_gate: value.required_gate.clone(),
            reason: value.unresolved_reason.clone(),
        });
    }
}

fn validate_resolved_value(id: &str, value: &ResolvedValue) -> ContractResult<()> {
    validate_gate(&value.required_gate)?;
    match value.resolution {
        Resolution::Locked => {
            if value.version.is_none() && value.value.is_none() {
                return invalid_owned(format!("locked value {id} requires a concrete value"));
            }
        }
        Resolution::LinuxVerified | Resolution::OwnerDecision | Resolution::Generated => {
            if value.version.is_some() || value.value.is_some() {
                return invalid_owned(format!(
                    "unresolved/generated value {id} must not claim a concrete release value"
                ));
            }
            if value.resolution != Resolution::Generated {
                validate_text(
                    value.unresolved_reason.as_deref().unwrap_or_default(),
                    "unresolved reason",
                )?;
            }
        }
    }
    Ok(())
}

fn validate_evidence_resolution(id: &str, value: &EvidenceResolution) -> ContractResult<()> {
    validate_gate(&value.required_gate)?;
    if value.resolution == Resolution::Locked {
        if value.evidence_ref.is_none() {
            return invalid_owned(format!(
                "locked evidence {id} requires an evidence reference"
            ));
        }
    } else if value.evidence_ref.is_some() {
        return invalid_owned(format!(
            "unresolved evidence {id} must not claim an evidence reference"
        ));
    }
    validate_text(&value.unresolved_reason, "evidence unresolved reason")
}

fn validate_repo_reference(repo_root: &Path, relative: &str) -> ContractResult<()> {
    if relative.is_empty() || Path::new(relative).is_absolute() {
        return invalid("W5 references must be non-empty repository-relative paths");
    }
    let path = Path::new(relative);
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return invalid_owned(format!("unsafe W5 repository reference {relative}"));
    }
    let full = repo_root.join(path);
    let metadata = fs::symlink_metadata(&full)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return invalid_owned(format!("W5 reference {relative} must be a real file"));
    }
    Ok(())
}

fn load_yaml<T>(repo_root: &Path, relative: &str) -> ContractResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    validate_repo_reference(repo_root, relative)?;
    serde_yaml::from_str(&fs::read_to_string(repo_root.join(relative))?).map_err(|error| {
        ContractError::Parse {
            path: relative.to_string(),
            message: error.to_string(),
        }
    })
}

fn require_schema(version: u32, path: &str) -> ContractResult<()> {
    if version != CONTRACT_SCHEMA_VERSION {
        return invalid_owned(format!("{path} uses unsupported schema version {version}"));
    }
    Ok(())
}

fn ensure_real_repo_root(repo_root: &Path) -> ContractResult<()> {
    let metadata = fs::symlink_metadata(repo_root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return invalid("repository root must be a real directory");
    }
    Ok(())
}

fn validate_text(value: &str, label: &str) -> ContractResult<()> {
    if value.trim().is_empty() || value.len() > 1024 || value.contains(['\n', '\r', '\0']) {
        return invalid_owned(format!(
            "{label} must be bounded non-empty single-line text"
        ));
    }
    Ok(())
}

fn validate_gate(value: &str) -> ContractResult<()> {
    validate_text(value, "required gate")?;
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '/' | '_'))
    {
        return invalid("required gate contains unsupported characters");
    }
    Ok(())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn invalid<T>(message: &str) -> ContractResult<T> {
    Err(ContractError::Invalid(message.to_string()))
}

fn invalid_owned<T>(message: String) -> ContractResult<T> {
    Err(ContractError::Invalid(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn shipped_w5_graph_is_source_valid_but_intentionally_not_release_resolved() {
        let report = validate_repository(&repo_root()).unwrap();
        assert!(report.source_valid);
        assert!(!report.release_resolved);
        assert_eq!(report.validation_tests, 38);
        assert_eq!(report.package_entries, 25);
        assert!(
            report
                .unresolved
                .iter()
                .any(|item| item.id == "packages.lock")
        );
        assert!(!report.unresolved.iter().any(|item| item.id == "codex.pin"));
        assert!(
            report
                .unresolved
                .iter()
                .any(|item| item.id == "codex.installed-version")
        );
        assert!(
            report
                .unresolved
                .iter()
                .any(|item| item.id == "codex.artix-compatibility")
        );
        assert!(
            !report
                .unresolved
                .iter()
                .any(|item| item.id == "portus-browser.source-revision")
        );
        assert!(
            report
                .unresolved
                .iter()
                .any(|item| item.id == "portus-mcp.packaging")
        );
        assert!(
            report
                .unresolved
                .iter()
                .any(|item| item.id == "tunnel-client.artix-compatibility")
        );
        assert!(
            report
                .unresolved
                .iter()
                .any(|item| item.id == "runtime.install")
        );
    }

    #[test]
    fn package_source_vocabulary_has_no_aur_and_references_existing_authorities() {
        let contract: PackageSourceContract = load_yaml(&repo_root(), PACKAGE_CONTRACT).unwrap();
        validate_package_source(&repo_root(), &contract).unwrap();
        assert_eq!(contract.source_policies.len(), 5);
        assert!(serde_yaml::from_str::<SourceClass>("aur").is_err());
    }

    #[test]
    fn multi_artifact_package_lock_exactly_covers_selected_artix_names() {
        let packages = PackageSourceContract {
            schema_version: 1,
            source_policies: BTreeMap::new(),
            packages: vec![PackageEntry {
                id: "dbus".to_string(),
                role: "system-dbus".to_string(),
                source_class: SourceClass::OfficialArtix,
                required_for_first_iso: true,
                profile: "base".to_string(),
                package: Some(PackageResolution {
                    resolution: Resolution::Locked,
                    names: vec!["dbus".to_string(), "dbus-openrc".to_string()],
                    unresolved_reason: None,
                    required_gate: "L2".to_string(),
                }),
                install_contract: None,
                component_contract: None,
            }],
        };
        let mut lock = PackageLock {
            schema_version: 1,
            source_manifest_sha256: "a".repeat(64),
            generated_at: "2026-08-28T00:00:00Z".to_string(),
            artix: LockArtix {
                architecture: "x86_64".to_string(),
                repositories: vec![BTreeMap::from([(
                    "name".to_string(),
                    YamlValue::String("system".to_string()),
                )])],
            },
            resolved: vec![LockedPackageRole {
                id: "dbus".to_string(),
                source_class: SourceClass::OfficialArtix,
                artifacts: vec![
                    LockedArtifact {
                        identity: "dbus".to_string(),
                        version: "1.16.2-1".to_string(),
                        repository: Some("system".to_string()),
                        licenses: vec!["AFL-2.1 OR GPL-2.0-or-later".to_string()],
                        sha256: None,
                        verification: "synchronized Artix repository metadata".to_string(),
                    },
                    LockedArtifact {
                        identity: "dbus-openrc".to_string(),
                        version: "20210505-2".to_string(),
                        repository: Some("system".to_string()),
                        licenses: vec!["GPL-2.0-only".to_string()],
                        sha256: None,
                        verification: "synchronized Artix repository metadata".to_string(),
                    },
                ],
            }],
        };

        validate_package_lock_data(&lock, &packages, &"a".repeat(64)).unwrap();
        lock.resolved[0].artifacts.pop();
        assert!(validate_package_lock_data(&lock, &packages, &"a".repeat(64)).is_err());
    }

    #[test]
    fn installer_identity_contract_preserves_root_master_and_p16_boundaries() {
        let identities: IdentityContract = load_yaml(&repo_root(), IDENTITY_CONTRACT).unwrap();
        validate_identities(&identities).unwrap();
        assert_eq!(identities.root_administration.uid, 0);
        assert!(identities.master_user.non_root_required);
        assert_eq!(
            identities.portus_service_identities.authority,
            P16_INSTALL_CONTRACT
        );
    }

    #[test]
    fn storage_and_vm_values_are_locked_to_validation_authorities() {
        let storage: StorageContract = load_yaml(&repo_root(), STORAGE_CONTRACT).unwrap();
        let vms: VmContracts = load_yaml(&repo_root(), VM_CONTRACT).unwrap();
        validate_storage(&storage).unwrap();
        validate_vm_profiles(&vms).unwrap();
    }

    #[test]
    fn validation_matrix_is_exact_iso_01_through_38_and_has_no_manual_only_row() {
        let matrix: ValidationMatrix = load_yaml(&repo_root(), VALIDATION_MATRIX).unwrap();
        validate_validation_matrix(&repo_root(), &matrix).unwrap();
        assert_eq!(matrix.tests.len(), 38);
        assert!(
            matrix
                .tests
                .iter()
                .all(|test| test.execution_class != ExecutionClass::Manual)
        );
    }

    #[test]
    fn schema_documents_are_strict_parseable_draft_2020_12_objects() {
        validate_schema_documents(&repo_root()).unwrap();
    }

    #[test]
    fn machine_contracts_reject_secret_material_and_secret_bearing_keys() {
        let build: BuildContract = load_yaml(&repo_root(), BUILD_CONTRACT).unwrap();
        scan_machine_contracts_for_secrets(&repo_root(), &build).unwrap();
        let bad: YamlValue = serde_yaml::from_str("api_key: sk-proj-example").unwrap();
        assert!(reject_secret_keys(&bad, "fixture").is_err());
    }

    #[test]
    fn p15_and_p16_are_consumed_not_redeclared() {
        let services: ServiceContract = load_yaml(&repo_root(), SERVICE_CONTRACT).unwrap();
        assert_eq!(services.portus_services.authority, P16_INSTALL_CONTRACT);
        assert!(
            services
                .services
                .iter()
                .all(|service| !service.id.starts_with("portus-"))
        );
        let browser: BrowserComponentContract =
            load_yaml(&repo_root(), BROWSER_COMPONENT_CONTRACT).unwrap();
        assert_eq!(browser.integration_contract, P15_BROWSER_CONTRACT);
        assert_eq!(browser.provider_manifest, P15_BROWSER_MANIFEST);
    }
}
