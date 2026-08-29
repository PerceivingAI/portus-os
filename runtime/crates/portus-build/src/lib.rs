//! Host-safe W6 builder/installer skeleton for PortusOS.
//!
//! This crate compiles the W5 source graph into deterministic plans and bounded
//! generated artifacts. It deliberately does not resolve Artix packages, map
//! Calamares modules, invoke artools, touch block devices, or claim VM evidence.

use portus_build_contract::{ContractReport, validate_repository};
use portus_install::{InstallManifest, StageReport};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Component, Path, PathBuf},
};

mod validation;
pub use validation::*;
mod candidate;
pub use candidate::*;

pub const W6_SCHEMA_VERSION: u32 = 1;
pub const EXIT_UNRESOLVED: u8 = 78;

const BUILDER_LAYOUT: &str = "portusos-build/builder/layout.yaml";
const PACKAGES: &str = "portusos-build/packages/packages.yaml";
const STORAGE: &str = "portusos-build/system/storage.yaml";
const VM_PROFILES: &str = "portusos-build/system/vm-profiles.yaml";
const SERVICES: &str = "portusos-build/system/base-services.yaml";
const IDENTITIES: &str = "portusos-build/system/identities.yaml";
const VALIDATION_MATRIX: &str = "portusos-build/validation/matrix.yaml";
const CALAMARES_RESPONSIBILITIES: &str = "portusos-build/installer/responsibilities.yaml";
const ARTOOLS_ADAPTER: &str = "portusos-build/iso/artools-profile/adapter.yaml";
const P16_INSTALL: &str = "runtime/install/install.toml";

const EXPECTED_HOOKS: [&str; 13] = [
    "base",
    "udev",
    "autodetect",
    "microcode",
    "modconf",
    "kms",
    "keyboard",
    "keymap",
    "block",
    "encrypt",
    "lvm2",
    "filesystems",
    "fsck",
];

const REQUIRED_INITRAMFS: [&str; 4] = [
    "/boot/initramfs-linux-lts.img",
    "/boot/initramfs-linux-lts-fallback.img",
    "/boot/initramfs-linux.img",
    "/boot/initramfs-linux-fallback.img",
];

const EXPECTED_CALAMARES_RESPONSIBILITIES: [&str; 16] = [
    "preflight-disk-plan",
    "partition-layout",
    "luks-lvm-filesystems",
    "target-root",
    "machine-identity",
    "locale-keyboard-timezone",
    "fstab-crypttab",
    "packages-portus",
    "master-user",
    "networking-clock",
    "mkinitcpio",
    "openrc-services",
    "grub-uefi",
    "portus-integration",
    "installed-target-validation",
    "finish-unmount",
];

#[derive(Debug)]
pub enum BuildError {
    Io(std::io::Error),
    Parse { path: String, message: String },
    Contract(String),
    Install(String),
    Invalid(String),
    Unresolved(String),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "W6 I/O error: {error}"),
            Self::Parse { path, message } => write!(f, "W6 parse error in {path}: {message}"),
            Self::Contract(message) => write!(f, "W5 contract validation failed: {message}"),
            Self::Install(message) => write!(f, "P16 staging failed: {message}"),
            Self::Invalid(message) => write!(f, "invalid W6 build input: {message}"),
            Self::Unresolved(message) => write!(f, "W6 native build unresolved: {message}"),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for BuildError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type BuildResult<T> = Result<T, BuildError>;

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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuilderLayout {
    schema_version: u32,
    generated: GeneratedRoots,
    sources: SourceRoots,
    clean_policy: CleanPolicy,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratedRoots {
    work: String,
    cache: String,
    out: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceRoots {
    rootfs_overlay: String,
    local_package_stage: String,
    artools_profile: String,
    calamares_responsibilities: String,
    calamares_modules: String,
    calamares_config: String,
    calamares_live: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanPolicy {
    allowed_roots: Vec<String>,
    arbitrary_path_delete: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageContract {
    schema_version: u32,
    source_policies: BTreeMap<String, SourcePolicy>,
    packages: Vec<PackageEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePolicy {
    verification: String,
    installation_owner: String,
    update_owner: String,
    failure_behavior: String,
    public_redistribution: String,
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
    esp: FixedPartition,
    boot: FixedPartition,
    system: SystemPartition,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixedPartition {
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
    key_bits: u16,
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
    free_reserve_percent: u64,
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
    menu_timeout_seconds: u64,
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
struct VmProfiles {
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
    portus_services: AuthorityRef,
    services: Vec<ServiceEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityRef {
    authority: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceEntry {
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
    portus_service_identities: AuthorityRef,
    non_root_administrator_account: NonRootAdmin,
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
struct NonRootAdmin {
    required: bool,
    resolution: Resolution,
    unresolved_reason: String,
    required_gate: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationMatrix {
    schema_version: u32,
    authority: String,
    tests: Vec<ValidationMatrixEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationMatrixEntry {
    id: String,
    #[serde(rename = "class")]
    execution_class: String,
    environment: String,
    blocking: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalamaresAdapter {
    schema_version: u32,
    framework: String,
    custom_modules: Vec<String>,
    destructive_preflight: DestructivePreflight,
    responsibilities: Vec<CalamaresResponsibility>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DestructivePreflight {
    explicit_target_required: bool,
    plan_hash_required: bool,
    matching_confirmation_required: bool,
    default_target_allowed: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalamaresResponsibility {
    id: String,
    mapping_resolution: Resolution,
    module_ids: Vec<String>,
    required_gate: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtoolsAdapter {
    schema_version: u32,
    framework: String,
    mapping_resolution: Resolution,
    required_gate: String,
    native_build_host: NativeBuildHost,
    artix_build_context: ArtixBuildContext,
    context_manager: String,
    bootstrap_contract: String,
    profile_name: String,
    workspace_profiles_dir: String,
    profile_source_root: String,
    rootfs_overlay_source: String,
    local_package_stage_source: String,
    stable_pacman_config: String,
    buildiso_executable: String,
    buildiso_fixed_args: Vec<String>,
    buildiso_chroots_flag: String,
    buildiso_target_flag: String,
    live_boot_kernel_package: String,
    output_subdirectory: String,
    output_filename_prefix: String,
    output_filename_suffix: String,
    #[serde(default)]
    unresolved_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtoolsProfileDocument {
    #[serde(rename = "live-session")]
    live_session: ArtoolsLiveSession,
    rootfs: ArtoolsProfilePackageSection,
    livefs: ArtoolsProfilePackageSection,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtoolsLiveSession {
    user: String,
    password: String,
    autologin: bool,
    #[serde(rename = "use-xlibre")]
    use_xlibre: bool,
    services: Vec<String>,
    #[serde(rename = "user-services")]
    user_services: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtoolsProfilePackageSection {
    packages: Vec<String>,
    #[serde(rename = "packages-init")]
    packages_init: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtoolsCommonProfile {
    #[serde(rename = "packages-base")]
    packages_base: Vec<String>,
    #[serde(rename = "packages-init")]
    packages_init: BTreeMap<String, Vec<String>>,
    #[serde(rename = "packages-apps")]
    packages_apps: Vec<String>,
    #[serde(rename = "packages-xorg")]
    packages_xorg: Vec<String>,
    #[serde(rename = "packages-xlibre")]
    packages_xlibre: Vec<String>,
    #[serde(rename = "packages-misc")]
    packages_misc: Vec<String>,
    #[serde(rename = "packages-boot")]
    packages_boot: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeBuildHost {
    distribution: String,
    architecture: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtixBuildContext {
    distribution: String,
    isolated_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PackagePlanEntry {
    pub id: String,
    pub role: String,
    pub source_class: SourceClass,
    pub profile: String,
    pub resolution: Resolution,
    pub selected_names: Vec<String>,
    pub source_ref: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ServicePlanEntry {
    pub id: String,
    pub role: String,
    pub package_id: String,
    pub lifecycle_owner: String,
    pub service_resolution: Resolution,
    pub service_name: Option<String>,
    pub runlevel_resolution: Resolution,
    pub runlevel: Option<String>,
    pub required_gate: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IdentityPlan {
    pub root_uid: u32,
    pub root_independent_from_master: bool,
    pub master_username_source: String,
    pub master_uid_resolution: Resolution,
    pub master_non_root_required: bool,
    pub master_private_home_required: bool,
    pub master_workspace_root_template: String,
    pub portus_service_identity_authority: String,
    pub optional_non_root_admin_resolution: Resolution,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiskPlan {
    pub total_mib: u64,
    pub esp_mib: u64,
    pub boot_mib: u64,
    pub encrypted_system_mib: u64,
    pub vg: String,
    pub swap_mib: u64,
    pub reserve_mib: u64,
    pub root_mib: u64,
    pub root_filesystem: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdapterPlan {
    pub artools_mapping_resolution: Resolution,
    pub calamares_responsibility_count: usize,
    pub calamares_resolved_count: usize,
    pub custom_calamares_modules: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct BuildPlan {
    pub schema_version: u32,
    pub source_valid: bool,
    pub release_resolved: bool,
    pub disk: DiskPlan,
    pub packages: Vec<PackagePlanEntry>,
    pub services: Vec<ServicePlanEntry>,
    pub identities: IdentityPlan,
    pub adapters: AdapterPlan,
    pub unresolved: Vec<W6Unresolved>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct W6Unresolved {
    pub id: String,
    pub resolution: Resolution,
    pub required_gate: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DestructivePlan {
    pub target_disk: String,
    pub disk: DiskPlan,
    pub plan_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIdentifiers {
    pub esp_uuid: String,
    pub boot_uuid: String,
    pub luks_uuid: String,
    pub crypt_name: String,
    pub root_mapper: String,
    pub swap_mapper: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RenderedTargetConfig {
    pub fstab: String,
    pub crypttab: String,
    pub mkinitcpio_plan: MkinitcpioPlan,
    pub grub_plan: GrubPlan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MkinitcpioPlan {
    pub framework: String,
    pub hooks: Vec<String>,
    pub presets: Vec<String>,
    pub required_artifacts: Vec<String>,
    pub fallback_omits_autodetect: bool,
    pub rebuild_command: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GrubPlan {
    pub bootloader_id: String,
    pub esp_mount: String,
    pub default_kernel_role: String,
    pub alternate_kernel_role: String,
    pub menu_timeout_seconds: u64,
    pub fallback_efi_path: String,
    pub luks_uuid: String,
    pub crypt_name: String,
    pub root_mapper: String,
    pub command_line_resolution: Resolution,
    pub rebuild_commands: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ValidationPlan {
    pub schema_version: u32,
    pub candidate_id: String,
    pub iso_sha256: String,
    pub authority: String,
    pub tests: Vec<ValidationPlanEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ValidationPlanEntry {
    pub test_id: String,
    pub execution_class: String,
    pub environment: String,
    pub blocking: bool,
    pub status: String,
    pub result_ref: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildMetadataInput {
    pub release_class: String,
    pub candidate_id: String,
    pub version: Option<String>,
    pub rc_number: u32,
    pub source_revision: String,
    pub source_tree_clean: bool,
    pub build_started_at: String,
    pub build_finished_at: String,
    pub distribution_snapshot: String,
    pub artools_version: String,
    pub rust_toolchain: String,
    pub artifact_filename: String,
    pub validation_authority_revision: String,
    pub release_authority_revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateIdentity {
    pub candidate_id: String,
    pub artifact_filename: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildMetadata {
    pub schema_version: u32,
    pub release_class: String,
    pub candidate_id: String,
    pub version: Option<String>,
    pub rc_number: u32,
    pub source_revision: String,
    pub source_tree_clean: bool,
    pub build_started_at: String,
    pub build_finished_at: String,
    pub builder: BuilderMetadata,
    pub artifact: ArtifactMetadata,
    pub package_source_manifest_ref: String,
    pub codex_pin_ref: String,
    pub portus_browser_pin_ref: String,
    pub portus_mcp_pin_ref: String,
    pub tunnel_client_pin_ref: String,
    pub validation_authority_revision: String,
    pub release_authority_revision: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderMetadata {
    pub architecture: String,
    pub distribution: String,
    pub distribution_snapshot: String,
    pub artools_version: String,
    pub rust_toolchain: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactMetadata {
    pub filename: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BundlePreview {
    pub metadata: BuildMetadata,
    pub sha256sums: String,
}

pub fn validate_w6(repo_root: &Path) -> BuildResult<ContractReport> {
    ensure_repo_root(repo_root)?;
    let report =
        validate_repository(repo_root).map_err(|error| BuildError::Contract(error.to_string()))?;
    validate_builder_layout(repo_root)?;
    validate_artools_adapter(repo_root)?;
    validate_calamares_adapter(repo_root)?;
    Ok(report)
}

pub fn build_plan(repo_root: &Path, disk_size_mib: u64) -> BuildResult<BuildPlan> {
    let report = validate_w6(repo_root)?;
    let packages: PackageContract = load_yaml(repo_root, PACKAGES)?;
    validate_package_contract_for_planning(&packages)?;
    let storage: StorageContract = load_yaml(repo_root, STORAGE)?;
    let vms: VmProfiles = load_yaml(repo_root, VM_PROFILES)?;
    let disk = calculate_disk_plan(&storage, &vms, disk_size_mib)?;
    let services: ServiceContract = load_yaml(repo_root, SERVICES)?;
    let identities: IdentityContract = load_yaml(repo_root, IDENTITIES)?;
    let calamares: CalamaresAdapter = load_yaml(repo_root, CALAMARES_RESPONSIBILITIES)?;
    let artools: ArtoolsAdapter = load_yaml(repo_root, ARTOOLS_ADAPTER)?;

    let package_plan = resolve_packages(&packages)?;
    let service_plan = resolve_services(&services, &package_plan)?;
    let identity_plan = resolve_identities(&identities)?;

    let mut unresolved: Vec<W6Unresolved> = report
        .unresolved
        .iter()
        .map(|item| W6Unresolved {
            id: item.id.clone(),
            resolution: match item.resolution {
                portus_build_contract::Resolution::Locked => Resolution::Locked,
                portus_build_contract::Resolution::LinuxVerified => Resolution::LinuxVerified,
                portus_build_contract::Resolution::OwnerDecision => Resolution::OwnerDecision,
                portus_build_contract::Resolution::Generated => Resolution::Generated,
            },
            required_gate: item.required_gate.clone(),
            reason: item.reason.clone(),
        })
        .collect();
    if calamares
        .responsibilities
        .iter()
        .any(|item| item.mapping_resolution != Resolution::Locked)
    {
        unresolved.push(W6Unresolved {
            id: "calamares.responsibility-mapping".to_string(),
            resolution: Resolution::LinuxVerified,
            required_gate: "L2".to_string(),
            reason: "Calamares responsibility mapping contains non-locked entries".to_string(),
        });
    }
    unresolved.sort_by(|left, right| left.id.cmp(&right.id));
    unresolved.dedup_by(|left, right| left.id == right.id);

    Ok(BuildPlan {
        schema_version: W6_SCHEMA_VERSION,
        source_valid: report.source_valid,
        release_resolved: report.release_resolved && unresolved.is_empty(),
        disk,
        packages: package_plan,
        services: service_plan,
        identities: identity_plan,
        adapters: AdapterPlan {
            artools_mapping_resolution: artools.mapping_resolution,
            calamares_responsibility_count: calamares.responsibilities.len(),
            calamares_resolved_count: calamares
                .responsibilities
                .iter()
                .filter(|item| item.mapping_resolution == Resolution::Locked)
                .count(),
            custom_calamares_modules: calamares.custom_modules.len(),
        },
        unresolved,
    })
}

pub fn stage_portus_to(
    repo_root: &Path,
    binary_dir: &Path,
    staging_root: &Path,
) -> BuildResult<StageReport> {
    validate_w6(repo_root)?;
    let manifest = InstallManifest::load(&repo_root.join(P16_INSTALL))
        .map_err(|error| BuildError::Install(error.to_string()))?;
    manifest
        .stage(repo_root, binary_dir, staging_root)
        .map_err(|error| BuildError::Install(error.to_string()))
}

pub fn default_portus_stage_root(repo_root: &Path) -> BuildResult<PathBuf> {
    let layout = validate_builder_layout(repo_root)?;
    Ok(repo_root
        .join(&layout.generated.work)
        .join("portus-package-root"))
}

pub fn prepare_destructive_plan(
    target_disk: Option<&str>,
    disk: DiskPlan,
) -> BuildResult<DestructivePlan> {
    let target = target_disk
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BuildError::Invalid("an explicit installation target disk is required".to_string())
        })?;
    if !target.starts_with("/dev/") || target.contains('\n') || target.contains('\r') {
        return Err(BuildError::Invalid(
            "installation target must be an explicit Linux /dev path".to_string(),
        ));
    }
    let payload = serde_json::to_vec(&(target, &disk))
        .map_err(|error| BuildError::Invalid(format!("cannot hash destructive plan: {error}")))?;
    Ok(DestructivePlan {
        target_disk: target.to_string(),
        disk,
        plan_sha256: hex_sha256(&payload),
    })
}

pub fn confirm_destructive_plan(plan: &DestructivePlan, supplied_hash: &str) -> BuildResult<()> {
    if supplied_hash != plan.plan_sha256 {
        return Err(BuildError::Invalid(
            "destructive confirmation does not match the exact target/disk plan".to_string(),
        ));
    }
    Ok(())
}

pub fn render_target_config(
    repo_root: &Path,
    identifiers: &TargetIdentifiers,
) -> BuildResult<RenderedTargetConfig> {
    validate_w6(repo_root)?;
    validate_target_identifiers(identifiers)?;
    let storage: StorageContract = load_yaml(repo_root, STORAGE)?;
    validate_storage_contract(&storage)?;

    let fstab = format!(
        "UUID={} /boot/efi vfat defaults 0 2\nUUID={} /boot ext4 defaults,relatime 0 2\n/dev/mapper/{} / ext4 defaults,relatime 0 1\n/dev/mapper/{} none swap defaults 0 0\n",
        identifiers.esp_uuid,
        identifiers.boot_uuid,
        identifiers.root_mapper,
        identifiers.swap_mapper
    );
    let crypttab = format!(
        "{} UUID={} none luks\n",
        identifiers.crypt_name, identifiers.luks_uuid
    );
    let hooks = EXPECTED_HOOKS
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    let mkinitcpio_plan = MkinitcpioPlan {
        framework: storage.boot.initramfs.framework.clone(),
        hooks,
        presets: vec!["default".to_string(), "fallback".to_string()],
        required_artifacts: REQUIRED_INITRAMFS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        fallback_omits_autodetect: true,
        rebuild_command: vec!["mkinitcpio".to_string(), "-P".to_string()],
    };
    let grub_plan = GrubPlan {
        bootloader_id: storage.boot.bootloader_id,
        esp_mount: storage.boot.esp_mount,
        default_kernel_role: storage.boot.default_kernel_role,
        alternate_kernel_role: storage.boot.alternate_kernel_role,
        menu_timeout_seconds: storage.boot.menu_timeout_seconds,
        fallback_efi_path: storage.boot.fallback_efi_path,
        luks_uuid: identifiers.luks_uuid.clone(),
        crypt_name: identifiers.crypt_name.clone(),
        root_mapper: identifiers.root_mapper.clone(),
        command_line_resolution: Resolution::LinuxVerified,
        rebuild_commands: vec![
            vec![
                "grub-mkconfig".to_string(),
                "-o".to_string(),
                "/boot/grub/grub.cfg".to_string(),
            ],
            vec![
                "grub-script-check".to_string(),
                "/boot/grub/grub.cfg".to_string(),
            ],
        ],
    };

    Ok(RenderedTargetConfig {
        fstab,
        crypttab,
        mkinitcpio_plan,
        grub_plan,
    })
}

pub fn validation_plan(
    repo_root: &Path,
    candidate_id: &str,
    iso_sha256: &str,
) -> BuildResult<ValidationPlan> {
    validate_w6(repo_root)?;
    validate_nonempty(candidate_id, "candidate id")?;
    validate_lower_hex(iso_sha256, 64, "ISO SHA-256")?;
    let matrix: ValidationMatrix = load_yaml(repo_root, VALIDATION_MATRIX)?;
    if matrix.schema_version != 1
        || matrix.authority != "docs/VALIDATION.md"
        || matrix.tests.len() != 38
    {
        return Err(BuildError::Invalid(
            "validation matrix drifted from W5 authority".to_string(),
        ));
    }
    let tests = matrix
        .tests
        .iter()
        .map(|test| ValidationPlanEntry {
            test_id: test.id.clone(),
            execution_class: test.execution_class.clone(),
            environment: test.environment.clone(),
            blocking: test.blocking,
            status: "not_run".to_string(),
            result_ref: format!("tests/{}/result.json", test.id),
        })
        .collect();
    Ok(ValidationPlan {
        schema_version: W6_SCHEMA_VERSION,
        candidate_id: candidate_id.to_string(),
        iso_sha256: iso_sha256.to_string(),
        authority: matrix.authority,
        tests,
    })
}

pub fn build_metadata_preview(
    repo_root: &Path,
    artifact_path: &Path,
    input: &BuildMetadataInput,
) -> BuildResult<BundlePreview> {
    validate_w6(repo_root)?;
    validate_metadata_input(input)?;
    let metadata = fs::metadata(artifact_path)?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(BuildError::Invalid(
            "artifact must be a non-empty regular file".to_string(),
        ));
    }
    let bytes = fs::read(artifact_path)?;
    let artifact_sha = hex_sha256(&bytes);
    let build_metadata = BuildMetadata {
        schema_version: W6_SCHEMA_VERSION,
        release_class: input.release_class.clone(),
        candidate_id: input.candidate_id.clone(),
        version: input.version.clone(),
        rc_number: input.rc_number,
        source_revision: input.source_revision.clone(),
        source_tree_clean: input.source_tree_clean,
        build_started_at: input.build_started_at.clone(),
        build_finished_at: input.build_finished_at.clone(),
        builder: BuilderMetadata {
            architecture: "x86_64".to_string(),
            distribution: "Artix Linux".to_string(),
            distribution_snapshot: input.distribution_snapshot.clone(),
            artools_version: input.artools_version.clone(),
            rust_toolchain: input.rust_toolchain.clone(),
        },
        artifact: ArtifactMetadata {
            filename: input.artifact_filename.clone(),
            sha256: artifact_sha,
            size_bytes: metadata.len(),
        },
        package_source_manifest_ref: PACKAGES.to_string(),
        codex_pin_ref: "portusos-build/components/codex.yaml".to_string(),
        portus_browser_pin_ref: "portusos-build/components/portus-browser.yaml".to_string(),
        portus_mcp_pin_ref: "portusos-build/components/portus-mcp.yaml".to_string(),
        tunnel_client_pin_ref: "portusos-build/components/tunnel-client.yaml".to_string(),
        validation_authority_revision: input.validation_authority_revision.clone(),
        release_authority_revision: input.release_authority_revision.clone(),
    };
    let metadata_bytes = serde_json::to_vec_pretty(&build_metadata).map_err(|error| {
        BuildError::Invalid(format!("cannot serialize build metadata: {error}"))
    })?;
    let metadata_sha = hex_sha256(&metadata_bytes);
    let mut entries = [
        (
            build_metadata.artifact.filename.clone(),
            build_metadata.artifact.sha256.clone(),
        ),
        ("build-metadata.json".to_string(), metadata_sha),
    ];
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let lines: Vec<_> = entries
        .iter()
        .map(|(name, hash)| format!("{hash}  {name}"))
        .collect();
    Ok(BundlePreview {
        metadata: build_metadata,
        sha256sums: format!("{}\n", lines.join("\n")),
    })
}

pub fn native_iso_build_gate(repo_root: &Path, release_candidate: bool) -> BuildResult<()> {
    let report = validate_w6(repo_root)?;
    let adapter: ArtoolsAdapter = load_yaml(repo_root, ARTOOLS_ADAPTER)?;
    if adapter.mapping_resolution != Resolution::Locked {
        return Err(BuildError::Unresolved(format!(
            "artools adapter remains {} at {}; native ISO construction must wait for L2",
            resolution_name(adapter.mapping_resolution),
            adapter.required_gate
        )));
    }
    if release_candidate && !report.release_resolved {
        return Err(BuildError::Unresolved(
            "release-candidate ISO requires a release-resolved W5 build graph".to_string(),
        ));
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(BuildError::Unresolved(
            "ISO builds require a native x86_64 Linux host with the verified isolated Artix build context".to_string(),
        ))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(())
    }
}

fn validate_builder_layout(repo_root: &Path) -> BuildResult<BuilderLayout> {
    let layout: BuilderLayout = load_yaml(repo_root, BUILDER_LAYOUT)?;
    if layout.schema_version != W6_SCHEMA_VERSION || layout.clean_policy.arbitrary_path_delete {
        return Err(BuildError::Invalid(
            "builder layout safety contract is invalid".to_string(),
        ));
    }
    let generated = [
        layout.generated.work.as_str(),
        layout.generated.cache.as_str(),
        layout.generated.out.as_str(),
    ];
    let allowed: BTreeSet<_> = layout
        .clean_policy
        .allowed_roots
        .iter()
        .map(String::as_str)
        .collect();
    let expected: BTreeSet<_> = generated.into_iter().collect();
    if allowed != expected {
        return Err(BuildError::Invalid(
            "clean policy must allow exactly work/cache/out".to_string(),
        ));
    }
    for path in [
        layout.generated.work.as_str(),
        layout.generated.cache.as_str(),
        layout.generated.out.as_str(),
        layout.sources.rootfs_overlay.as_str(),
        layout.sources.local_package_stage.as_str(),
        layout.sources.artools_profile.as_str(),
        layout.sources.calamares_responsibilities.as_str(),
        layout.sources.calamares_modules.as_str(),
        layout.sources.calamares_config.as_str(),
        layout.sources.calamares_live.as_str(),
    ] {
        validate_repo_relative(path)?;
    }
    for source in [
        &layout.sources.rootfs_overlay,
        &layout.sources.local_package_stage,
        &layout.sources.artools_profile,
        &layout.sources.calamares_responsibilities,
        &layout.sources.calamares_modules,
        &layout.sources.calamares_config,
        &layout.sources.calamares_live,
    ] {
        if !repo_root.join(source).exists() {
            return Err(BuildError::Invalid(format!(
                "declared W6 source does not exist: {source}"
            )));
        }
    }
    Ok(layout)
}

fn validate_artools_adapter(repo_root: &Path) -> BuildResult<()> {
    let adapter: ArtoolsAdapter = load_yaml(repo_root, ARTOOLS_ADAPTER)?;
    let expected_args = [
        "-p", "portus", "-R", "stable", "-a", "x86_64", "-i", "openrc",
    ];
    if adapter.schema_version != W6_SCHEMA_VERSION
        || adapter.framework != "artools"
        || adapter.mapping_resolution != Resolution::Locked
        || adapter.required_gate != "L2"
        || adapter.native_build_host.distribution != "Linux"
        || adapter.native_build_host.architecture != "x86_64"
        || adapter.artix_build_context.distribution != "Artix Linux"
        || !adapter.artix_build_context.isolated_required
        || adapter.context_manager != "scripts/artix/context.py"
        || adapter.bootstrap_contract != "portusos-build/artix/bootstrap.json"
        || adapter.profile_name != "portus"
        || adapter.workspace_profiles_dir != "iso-profiles"
        || adapter.profile_source_root != "portusos-build/iso/artools-profile/workspace"
        || adapter.rootfs_overlay_source != "portusos-build/rootfs/overlay"
        || adapter.local_package_stage_source != "portusos-build/packages/local"
        || adapter.stable_pacman_config != "/usr/share/artools/pacman.conf.d/iso-x86_64.conf"
        || adapter.buildiso_executable != "/usr/bin/buildiso"
        || adapter
            .buildiso_fixed_args
            .iter()
            .map(String::as_str)
            .ne(expected_args)
        || adapter.buildiso_chroots_flag != "-r"
        || adapter.buildiso_target_flag != "-t"
        || adapter.live_boot_kernel_package != "linux-lts"
        || adapter.output_subdirectory != "portus"
        || adapter.output_filename_prefix != "artix-portus-openrc-"
        || adapter.output_filename_suffix != "-x86_64.iso"
        || adapter.unresolved_reason.is_some()
    {
        return Err(BuildError::Invalid(
            "locked artools adapter conflicts with verified Artix 0.39.1 L2 mapping".to_string(),
        ));
    }
    for source in [
        &adapter.context_manager,
        &adapter.bootstrap_contract,
        &adapter.profile_source_root,
        &adapter.rootfs_overlay_source,
        &adapter.local_package_stage_source,
    ] {
        validate_repo_relative(source)?;
        if !repo_root.join(source).exists() {
            return Err(BuildError::Invalid(format!(
                "locked artools adapter source does not exist: {source}"
            )));
        }
    }
    validate_artools_profile_sources(repo_root, &adapter)?;
    Ok(())
}

fn validate_artools_boot_packages(packages_boot: &[String]) -> BuildResult<()> {
    if !packages_boot.iter().any(|package| package == "memtest86+") {
        return Err(BuildError::Invalid(
            "artools 0.39.1 boot profile must provide memtest86+ for /boot/memtest86+/memtest.bin"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_artools_profile_sources(repo_root: &Path, adapter: &ArtoolsAdapter) -> BuildResult<()> {
    let profile_path = format!("{}/portus/profile.yaml", adapter.profile_source_root);
    let common_path = format!("{}/common/common.yaml", adapter.profile_source_root);
    let profile: ArtoolsProfileDocument = load_yaml(repo_root, &profile_path)?;
    let common: ArtoolsCommonProfile = load_yaml(repo_root, &common_path)?;
    let packages: PackageContract = load_yaml(repo_root, PACKAGES)?;
    let services: ServiceContract = load_yaml(repo_root, SERVICES)?;

    if profile.live_session.user != "portus"
        || profile.live_session.password != "portus-live"
        || !profile.live_session.autologin
        || profile.live_session.use_xlibre
        || !profile.live_session.user_services.is_empty()
    {
        return Err(BuildError::Invalid(
            "Portus artools live-session contract drifted from the first-ISO bootstrap path"
                .to_string(),
        ));
    }

    let expected_services: Vec<_> = services
        .services
        .iter()
        .map(|service| {
            service.service_name.clone().ok_or_else(|| {
                BuildError::Invalid(format!(
                    "locked artools profile requires a selected OpenRC service name for {}",
                    service.id
                ))
            })
        })
        .collect::<BuildResult<_>>()?;
    if profile.live_session.services != expected_services {
        return Err(BuildError::Invalid(
            "artools live-session services differ from base-services.yaml".to_string(),
        ));
    }

    let mut expected_packages = BTreeSet::new();
    for entry in &packages.packages {
        if entry.source_class != SourceClass::OfficialArtix || entry.profile == "build-host" {
            continue;
        }
        let package = entry.package.as_ref().ok_or_else(|| {
            BuildError::Invalid(format!(
                "official Artix role {} must carry a package mapping",
                entry.id
            ))
        })?;
        if package.resolution != Resolution::Locked {
            return Err(BuildError::Invalid(format!(
                "artools profile cannot consume unresolved package role {}",
                entry.id
            )));
        }
        expected_packages.extend(package.names.iter().cloned());
    }

    validate_artools_boot_packages(&common.packages_boot)?;

    let expected_live_only = BTreeSet::from([
        "artix-live-base".to_string(),
        "artix-live-openrc".to_string(),
        "calamares".to_string(),
    ]);
    let expected_installed: BTreeSet<_> = expected_packages
        .difference(&expected_live_only)
        .cloned()
        .collect();

    let mut actual_installed = BTreeSet::new();
    for value in [
        &profile.rootfs.packages,
        &common.packages_base,
        &common.packages_apps,
        &common.packages_xorg,
        &common.packages_xlibre,
        &common.packages_misc,
        &common.packages_boot,
    ] {
        actual_installed.extend(value.iter().cloned());
    }
    for mapping in [&profile.rootfs.packages_init, &common.packages_init] {
        for values in mapping.values() {
            actual_installed.extend(values.iter().cloned());
        }
    }
    let mut actual_live_only: BTreeSet<_> = profile.livefs.packages.iter().cloned().collect();
    for values in profile.livefs.packages_init.values() {
        actual_live_only.extend(values.iter().cloned());
    }
    if expected_installed != actual_installed || expected_live_only != actual_live_only {
        let installed_missing: Vec<_> = expected_installed
            .difference(&actual_installed)
            .cloned()
            .collect();
        let installed_extra: Vec<_> = actual_installed
            .difference(&expected_installed)
            .cloned()
            .collect();
        let live_missing: Vec<_> = expected_live_only
            .difference(&actual_live_only)
            .cloned()
            .collect();
        let live_extra: Vec<_> = actual_live_only
            .difference(&expected_live_only)
            .cloned()
            .collect();
        return Err(BuildError::Invalid(format!(
            "artools rootfs/livefs package placement differs from the installed-target contract; installed_missing={installed_missing:?} installed_extra={installed_extra:?} live_missing={live_missing:?} live_extra={live_extra:?}"
        )));
    }

    for required in [
        format!(
            "{}/portus/live-overlay/etc/skel/.bash_profile",
            adapter.profile_source_root
        ),
        format!(
            "{}/portus/live-overlay/etc/skel/.xinitrc",
            adapter.profile_source_root
        ),
        format!(
            "{}/portus/live-overlay/etc/skel/.config/i3/config",
            adapter.profile_source_root
        ),
        format!(
            "{}/portus/live-overlay/etc/ssh/sshd_config.d/20-portus-live.conf",
            adapter.profile_source_root
        ),
    ] {
        if !repo_root.join(&required).is_file() {
            return Err(BuildError::Invalid(format!(
                "locked artools live profile source is missing: {required}"
            )));
        }
    }
    Ok(())
}

fn validate_calamares_adapter(repo_root: &Path) -> BuildResult<()> {
    let adapter: CalamaresAdapter = load_yaml(repo_root, CALAMARES_RESPONSIBILITIES)?;
    if adapter.schema_version != W6_SCHEMA_VERSION
        || adapter.framework != "calamares"
        || adapter.custom_modules != ["portus-storage"]
        || !adapter.destructive_preflight.explicit_target_required
        || !adapter.destructive_preflight.plan_hash_required
        || !adapter.destructive_preflight.matching_confirmation_required
        || adapter.destructive_preflight.default_target_allowed
        || adapter.responsibilities.len() != EXPECTED_CALAMARES_RESPONSIBILITIES.len()
    {
        return Err(BuildError::Invalid(
            "Calamares adapter skeleton violates locked safety baseline".to_string(),
        ));
    }
    let expected_modules = BTreeMap::from([
        (
            "preflight-disk-plan",
            vec!["notesqml", "portus-storage", "summary"],
        ),
        ("partition-layout", vec!["portus-storage"]),
        ("luks-lvm-filesystems", vec!["portus-storage"]),
        ("target-root", vec!["portus-storage", "unpackfs"]),
        ("machine-identity", vec!["machineid", "users"]),
        (
            "locale-keyboard-timezone",
            vec!["locale", "keyboard", "localecfg", "hwclock"],
        ),
        ("fstab-crypttab", vec!["portus-storage"]),
        ("packages-portus", vec!["unpackfs"]),
        ("master-user", vec!["users"]),
        ("networking-clock", vec!["networkcfg", "hwclock"]),
        ("mkinitcpio", vec!["portus-storage", "initcpio"]),
        ("openrc-services", vec!["services-openrc"]),
        ("grub-uefi", vec!["portus-storage", "bootloader"]),
        ("portus-integration", vec!["unpackfs", "shellprocess"]),
        ("installed-target-validation", vec!["shellprocess"]),
        ("finish-unmount", vec!["umount", "finished"]),
    ]);
    for (actual, expected) in adapter
        .responsibilities
        .iter()
        .zip(EXPECTED_CALAMARES_RESPONSIBILITIES)
    {
        let expected_ids = expected_modules.get(expected).ok_or_else(|| {
            BuildError::Invalid(format!("missing locked Calamares mapping for {expected}"))
        })?;
        if actual.id != expected
            || actual.mapping_resolution != Resolution::Locked
            || actual.required_gate != "L2"
            || actual
                .module_ids
                .iter()
                .map(String::as_str)
                .ne(expected_ids.iter().copied())
        {
            return Err(BuildError::Invalid(format!(
                "Calamares responsibility {} differs from the verified Artix stock-module mapping",
                actual.id
            )));
        }
    }
    for required in [
        "portusos-build/installer/modules/portus-storage/module.desc",
        "portusos-build/installer/modules/portus-storage/main.py",
        "portusos-build/installer/modules/portus-storage/storage_engine.py",
        "portusos-build/installer/modules/portus-storage/portus-storage-preflight.conf",
        "portusos-build/installer/modules/portus-storage/portus-storage.conf",
        "portusos-build/installer/modules/portus-storage/portus-storage-finalize.conf",
        "portusos-build/installer/config/settings.conf",
        "portusos-build/installer/config/portus-storage-input.qml",
        "portusos-build/installer/config/modules/portus-storage-input.conf",
        "portusos-build/installer/config/modules/bootloader.conf",
        "portusos-build/installer/config/modules/unpackfs.conf",
        "portusos-build/installer/config/modules/initcpio.conf",
        "portusos-build/installer/config/modules/users.conf",
        "portusos-build/installer/config/modules/services-openrc.conf",
        "portusos-build/installer/live/portus-install",
        "portusos-build/installer/live/90-portus-installer.rules",
    ] {
        if !repo_root.join(required).is_file() {
            return Err(BuildError::Invalid(format!(
                "verified-gap Calamares module source is missing: {required}"
            )));
        }
    }
    let settings =
        fs::read_to_string(repo_root.join("portusos-build/installer/config/settings.conf"))
            .map_err(BuildError::Io)?;
    let _: serde_yaml::Value =
        serde_yaml::from_str(&settings).map_err(|error| BuildError::Parse {
            path: "portusos-build/installer/config/settings.conf".to_string(),
            message: error.to_string(),
        })?;
    let expected_show = "      - users\n      - notesqml@portus-storage-input\n      - summary\n";
    if !settings.contains(expected_show) {
        return Err(BuildError::Invalid(
            "PortusOS Calamares show order must place Storage & Recovery immediately after users and before summary"
                .to_string(),
        ));
    }
    let expected_exec_prefix = "  - exec:\n      - portus-storage@preflight\n      - portus-storage@prepare\n      - unpackfs\n";
    if !settings.contains(expected_exec_prefix) {
        return Err(BuildError::Invalid(
            "PortusOS Calamares execution must run non-destructive storage preflight immediately before prepare/unpackfs"
                .to_string(),
        ));
    }
    for required in [
        "notesqml@portus-storage-input",
        "portus-storage@preflight",
        "portus-storage@prepare",
        "portus-storage@finalize",
        "services-openrc",
    ] {
        if !settings.contains(required) {
            return Err(BuildError::Invalid(format!(
                "PortusOS Calamares settings omitted required sequence item {required}"
            )));
        }
    }
    for forbidden in ["\n      - partition\n", "luksbootkeyfile"] {
        if settings.contains(forbidden) {
            return Err(BuildError::Invalid(format!(
                "PortusOS Calamares settings retain forbidden stock storage path {forbidden:?}"
            )));
        }
    }
    let input_qml = fs::read_to_string(
        repo_root.join("portusos-build/installer/config/portus-storage-input.qml"),
    )
    .map_err(BuildError::Io)?;
    for required in [
        "Qt.labs.folderlistmodel",
        "TextInput.Password",
        "portusStorageInputArmed",
        "Global.insert(\"portusTargetDevice\"",
        "ViewManager.next()",
    ] {
        if !input_qml.contains(required) {
            return Err(BuildError::Invalid(format!(
                "PortusOS storage-input QML omitted safety element {required}"
            )));
        }
    }
    Ok(())
}

fn validate_package_contract_for_planning(contract: &PackageContract) -> BuildResult<()> {
    if contract.schema_version != 1
        || contract.packages.len() != 25
        || contract.source_policies.len() != 5
    {
        return Err(BuildError::Invalid(
            "package contract is not the W5 first-ISO inventory".to_string(),
        ));
    }
    for policy in contract.source_policies.values() {
        for value in [
            &policy.verification,
            &policy.installation_owner,
            &policy.update_owner,
            &policy.failure_behavior,
            &policy.public_redistribution,
        ] {
            validate_nonempty(value, "source policy value")?;
        }
    }
    Ok(())
}

fn resolve_packages(contract: &PackageContract) -> BuildResult<Vec<PackagePlanEntry>> {
    let mut output = Vec::with_capacity(contract.packages.len());
    let mut ids = BTreeSet::new();
    for package in &contract.packages {
        if !package.required_for_first_iso || !ids.insert(package.id.as_str()) {
            return Err(BuildError::Invalid(
                "package inventory must be unique and first-ISO required".to_string(),
            ));
        }
        let (resolution, selected_names, source_ref) = if let Some(value) = &package.package {
            validate_nonempty(&value.required_gate, "package required gate")?;
            let unique_names: BTreeSet<_> = value.names.iter().map(String::as_str).collect();
            if unique_names.len() != value.names.len() {
                return Err(BuildError::Invalid(format!(
                    "package role {} contains duplicate selected package names",
                    package.id
                )));
            }
            for name in &value.names {
                validate_nonempty(name, "selected package name")?;
                if name.chars().any(char::is_whitespace) {
                    return Err(BuildError::Invalid(format!(
                        "package role {} contains an invalid whitespace-bearing package name",
                        package.id
                    )));
                }
            }
            if value.resolution == Resolution::Locked {
                if value.names.is_empty() || value.unresolved_reason.is_some() {
                    return Err(BuildError::Invalid(format!(
                        "locked package role {} requires selected names and no unresolved reason",
                        package.id
                    )));
                }
            } else {
                if !value.names.is_empty() {
                    return Err(BuildError::Invalid(format!(
                        "unresolved package role {} must not preselect package names",
                        package.id
                    )));
                }
                validate_nonempty(
                    value.unresolved_reason.as_deref().unwrap_or_default(),
                    "package unresolved reason",
                )?;
            }
            (value.resolution, value.names.clone(), None)
        } else if let Some(reference) = &package.install_contract {
            (Resolution::Locked, Vec::new(), Some(reference.clone()))
        } else if let Some(reference) = &package.component_contract {
            (Resolution::Locked, Vec::new(), Some(reference.clone()))
        } else {
            return Err(BuildError::Invalid(format!(
                "package {} has no resolution source",
                package.id
            )));
        };
        output.push(PackagePlanEntry {
            id: package.id.clone(),
            role: package.role.clone(),
            source_class: package.source_class,
            profile: package.profile.clone(),
            resolution,
            selected_names,
            source_ref,
        });
    }
    output.sort_by(|left, right| (&left.profile, &left.id).cmp(&(&right.profile, &right.id)));
    Ok(output)
}

fn resolve_services(
    contract: &ServiceContract,
    packages: &[PackagePlanEntry],
) -> BuildResult<Vec<ServicePlanEntry>> {
    if contract.schema_version != 1 || contract.portus_services.authority != P16_INSTALL {
        return Err(BuildError::Invalid(
            "service contract must defer Portus services to P16".to_string(),
        ));
    }
    let package_ids: BTreeSet<_> = packages.iter().map(|entry| entry.id.as_str()).collect();
    let mut output = Vec::with_capacity(contract.services.len());
    for service in &contract.services {
        if !package_ids.contains(service.package_id.as_str()) || service.lifecycle_owner != "openrc"
        {
            return Err(BuildError::Invalid(format!(
                "invalid service/package relationship for {}",
                service.id
            )));
        }
        validate_nonempty(&service.required_gate, "service required gate")?;
        if service.service_resolution == Resolution::Locked
            && service.runlevel_resolution == Resolution::Locked
        {
            validate_nonempty(
                service.service_name.as_deref().unwrap_or_default(),
                "locked service name",
            )?;
            validate_nonempty(
                service.runlevel.as_deref().unwrap_or_default(),
                "locked service runlevel",
            )?;
            if service.unresolved_reason.is_some() {
                return Err(BuildError::Invalid(format!(
                    "locked service {} retains an unresolved reason",
                    service.id
                )));
            }
        } else {
            validate_nonempty(
                service.unresolved_reason.as_deref().unwrap_or_default(),
                "service unresolved reason",
            )?;
        }
        output.push(ServicePlanEntry {
            id: service.id.clone(),
            role: service.role.clone(),
            package_id: service.package_id.clone(),
            lifecycle_owner: service.lifecycle_owner.clone(),
            service_resolution: service.service_resolution,
            service_name: service.service_name.clone(),
            runlevel_resolution: service.runlevel_resolution,
            runlevel: service.runlevel.clone(),
            required_gate: service.required_gate.clone(),
        });
    }
    output.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(output)
}

fn resolve_identities(contract: &IdentityContract) -> BuildResult<IdentityPlan> {
    if contract.schema_version != 1
        || contract.root_administration.uid != 0
        || !contract.root_administration.ultimate_authority
        || !contract.root_administration.independent_from_master
        || contract.root_administration.credential_source != "installer-owner-input"
        || contract.master_user.creation_owner != "installer"
        || contract.master_user.username_source != "installer-owner-input"
        || !contract.master_user.non_root_required
        || !contract.master_user.private_home_required
        || contract.master_user.workspace_root_template != "/workspace/{user}"
        || contract.master_user.permission_bundle_source != "runtime/install/policy/bundles"
        || contract.portus_service_identities.authority != P16_INSTALL
        || contract.non_root_administrator_account.required
        || contract.non_root_administrator_account.required_gate != "L6"
        || contract
            .non_root_administrator_account
            .unresolved_reason
            .trim()
            .is_empty()
    {
        return Err(BuildError::Invalid(
            "identity contract violates root/Master/P16 boundaries".to_string(),
        ));
    }
    Ok(IdentityPlan {
        root_uid: 0,
        root_independent_from_master: true,
        master_username_source: contract.master_user.username_source.clone(),
        master_uid_resolution: contract.master_user.uid_resolution,
        master_non_root_required: true,
        master_private_home_required: true,
        master_workspace_root_template: contract.master_user.workspace_root_template.clone(),
        portus_service_identity_authority: contract.portus_service_identities.authority.clone(),
        optional_non_root_admin_resolution: contract.non_root_administrator_account.resolution,
    })
}

fn calculate_disk_plan(
    storage: &StorageContract,
    vms: &VmProfiles,
    disk_size_mib: u64,
) -> BuildResult<DiskPlan> {
    validate_storage_contract(storage)?;
    if vms.schema_version != 1 || vms.resolution != Resolution::Locked {
        return Err(BuildError::Invalid(
            "VM profile contract must be locked".to_string(),
        ));
    }
    let minimum = vms
        .profiles
        .get("minimum")
        .ok_or_else(|| BuildError::Invalid("minimum VM profile missing".to_string()))?;
    validate_vm_profile_shape(minimum)?;
    let minimum_mib = minimum
        .disk_gib
        .checked_mul(1024)
        .ok_or_else(|| BuildError::Invalid("minimum VM disk size overflow".to_string()))?;
    if disk_size_mib < minimum_mib {
        return Err(BuildError::Invalid(format!(
            "disk size {disk_size_mib} MiB is below the locked minimum {minimum_mib} MiB"
        )));
    }
    let fixed = storage
        .partitions
        .esp
        .size_mib
        .checked_add(storage.partitions.boot.size_mib)
        .ok_or_else(|| BuildError::Invalid("fixed partition size overflow".to_string()))?;
    let system_mib = disk_size_mib
        .checked_sub(fixed)
        .ok_or_else(|| BuildError::Invalid("disk cannot contain fixed partitions".to_string()))?;
    let reserve_mib = system_mib
        .checked_mul(storage.partitions.system.lvm.free_reserve_percent)
        .ok_or_else(|| BuildError::Invalid("VG reserve calculation overflow".to_string()))?
        / 100;
    let root_mib = system_mib
        .checked_sub(storage.partitions.system.lvm.swap_mib)
        .and_then(|value| value.checked_sub(reserve_mib))
        .ok_or_else(|| {
            BuildError::Invalid("disk cannot contain swap plus VG reserve".to_string())
        })?;
    if root_mib == 0 {
        return Err(BuildError::Invalid("root LV would be empty".to_string()));
    }
    Ok(DiskPlan {
        total_mib: disk_size_mib,
        esp_mib: storage.partitions.esp.size_mib,
        boot_mib: storage.partitions.boot.size_mib,
        encrypted_system_mib: system_mib,
        vg: storage.partitions.system.lvm.vg.clone(),
        swap_mib: storage.partitions.system.lvm.swap_mib,
        reserve_mib,
        root_mib,
        root_filesystem: storage.partitions.system.lvm.root_filesystem.clone(),
    })
}

fn validate_storage_contract(storage: &StorageContract) -> BuildResult<()> {
    let encryption = &storage.partitions.system.encryption;
    let lvm = &storage.partitions.system.lvm;
    let boot = &storage.boot;
    if storage.schema_version != 1
        || storage.resolution != Resolution::Locked
        || storage.target.architecture != "x86_64"
        || storage.target.firmware != "uefi"
        || storage.target.partition_table != "gpt"
        || storage.target.secure_boot
        || storage.partitions.esp.size_mib != 512
        || storage.partitions.esp.filesystem != "fat32"
        || storage.partitions.esp.mount != "/boot/efi"
        || storage.partitions.esp.encrypted
        || storage.partitions.boot.size_mib != 2048
        || storage.partitions.boot.filesystem != "ext4"
        || storage.partitions.boot.mount != "/boot"
        || storage.partitions.boot.encrypted
        || encryption.format != "luks2"
        || encryption.cipher != "aes-xts-plain64"
        || encryption.key_bits != 512
        || encryption.pbkdf != "argon2id"
        || encryption.target_time_ms != 2000
        || encryption.memory_limit_kib != 262_144
        || !encryption.owner_keyslot_required
        || !encryption.recovery_keyslot_required
        || encryption.automatic_unlock
        || lvm.vg != "portus"
        || lvm.root_filesystem != "ext4"
        || lvm.swap_mib != 4096
        || lvm.free_reserve_percent != 5
        || lvm.split_home
        || lvm.split_var
        || lvm.split_srv
        || boot.bootloader != "grub-uefi"
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
        return Err(BuildError::Invalid(
            "storage contract drifted from W5 authority".to_string(),
        ));
    }
    Ok(())
}

fn validate_vm_profile_shape(profile: &VmProfile) -> BuildResult<()> {
    if profile.vcpu == 0
        || profile.memory_mib == 0
        || profile.disk_gib == 0
        || profile.firmware != "uefi"
        || profile.secure_boot
        || profile.network != "nat"
        || profile.three_d_acceleration_required
    {
        return Err(BuildError::Invalid(
            "VM profile violates first-ISO baseline".to_string(),
        ));
    }
    Ok(())
}

fn validate_target_identifiers(value: &TargetIdentifiers) -> BuildResult<()> {
    for (field, candidate) in [
        ("ESP UUID", &value.esp_uuid),
        ("boot UUID", &value.boot_uuid),
        ("LUKS UUID", &value.luks_uuid),
    ] {
        validate_identifier_token(candidate, field)?;
    }
    for (field, candidate) in [
        ("crypt name", &value.crypt_name),
        ("root mapper", &value.root_mapper),
        ("swap mapper", &value.swap_mapper),
    ] {
        validate_mapper_name(candidate, field)?;
    }
    Ok(())
}

fn validate_metadata_input(input: &BuildMetadataInput) -> BuildResult<()> {
    if input.rc_number == 0 {
        return Err(BuildError::Invalid(
            "rc_number must be positive".to_string(),
        ));
    }
    validate_lower_hex(&input.source_revision, 40, "source revision")?;
    let identity = expected_candidate_identity(
        &input.release_class,
        input.version.as_deref(),
        input.rc_number,
        &input.source_revision,
    )?;
    if input.candidate_id != identity.candidate_id {
        return Err(BuildError::Invalid(format!(
            "candidate_id must be derived from release class, RC number and source revision: {}",
            identity.candidate_id
        )));
    }
    if input.artifact_filename != identity.artifact_filename {
        return Err(BuildError::Invalid(format!(
            "artifact filename must match release authority: {}",
            identity.artifact_filename
        )));
    }
    if input.release_class == "public_rc" && !input.source_tree_clean {
        return Err(BuildError::Invalid(
            "public RC build metadata requires a clean source tree".to_string(),
        ));
    }
    validate_lower_hex(
        &input.validation_authority_revision,
        40,
        "validation authority revision",
    )?;
    validate_lower_hex(
        &input.release_authority_revision,
        40,
        "release authority revision",
    )?;
    for (field, value) in [
        ("build_started_at", &input.build_started_at),
        ("build_finished_at", &input.build_finished_at),
        ("distribution_snapshot", &input.distribution_snapshot),
        ("artools_version", &input.artools_version),
        ("rust_toolchain", &input.rust_toolchain),
    ] {
        validate_nonempty(value, field)?;
    }
    Ok(())
}

pub fn expected_candidate_identity(
    release_class: &str,
    version: Option<&str>,
    rc_number: u32,
    source_revision: &str,
) -> BuildResult<CandidateIdentity> {
    if rc_number == 0 {
        return Err(BuildError::Invalid(
            "rc_number must be positive".to_string(),
        ));
    }
    validate_lower_hex(source_revision, 40, "source revision")?;
    let short = &source_revision[..12];
    let (candidate_id, artifact_filename) = match release_class {
        "development_rc" => {
            if version.is_some() {
                return Err(BuildError::Invalid(
                    "development_rc must not carry a public semantic version".to_string(),
                ));
            }
            (
                format!("first-iso-rc.{rc_number}-g{short}"),
                format!("PortusOS-first-iso-rc.{rc_number}-x86_64.iso"),
            )
        }
        "public_rc" => {
            let version = version.ok_or_else(|| {
                BuildError::Invalid("public_rc requires a semantic version".to_string())
            })?;
            validate_release_semver(version)?;
            (
                format!("{version}-rc.{rc_number}-g{short}"),
                format!("PortusOS-{version}-rc.{rc_number}-x86_64.iso"),
            )
        }
        _ => {
            return Err(BuildError::Invalid(
                "unsupported build metadata release class".to_string(),
            ));
        }
    };
    Ok(CandidateIdentity {
        candidate_id,
        artifact_filename,
    })
}

fn validate_release_semver(value: &str) -> BuildResult<()> {
    let components: Vec<_> = value.split('.').collect();
    if components.len() != 3
        || components
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(BuildError::Invalid(
            "public release version must be three dot-separated numeric components".to_string(),
        ));
    }
    Ok(())
}

fn ensure_repo_root(repo_root: &Path) -> BuildResult<()> {
    let metadata = fs::symlink_metadata(repo_root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(BuildError::Invalid(
            "repository root must be a real directory".to_string(),
        ));
    }
    for required in [
        "Cargo.toml",
        "portusos-build/contracts/build.yaml",
        P16_INSTALL,
    ] {
        if !repo_root.join(required).exists() {
            return Err(BuildError::Invalid(format!(
                "repository root missing {required}"
            )));
        }
    }
    Ok(())
}

fn load_yaml<T>(repo_root: &Path, relative: &str) -> BuildResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let contents = fs::read_to_string(repo_root.join(relative))?;
    serde_yaml::from_str(&contents).map_err(|error| BuildError::Parse {
        path: relative.to_string(),
        message: error.to_string(),
    })
}

fn validate_repo_relative(path: &str) -> BuildResult<()> {
    let parsed = Path::new(path);
    if parsed.is_absolute()
        || parsed.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(BuildError::Invalid(format!(
            "unsafe repository-relative path: {path}"
        )));
    }
    Ok(())
}

fn validate_nonempty(value: &str, field: &str) -> BuildResult<()> {
    if value.trim().is_empty()
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(BuildError::Invalid(format!("{field} is empty or unsafe")));
    }
    Ok(())
}

fn validate_identifier_token(value: &str, field: &str) -> BuildResult<()> {
    validate_nonempty(value, field)?;
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(BuildError::Invalid(format!(
            "{field} contains unsupported characters"
        )));
    }
    Ok(())
}

fn validate_mapper_name(value: &str, field: &str) -> BuildResult<()> {
    validate_nonempty(value, field)?;
    if value.len() > 127
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(BuildError::Invalid(format!(
            "{field} is not a safe mapper name"
        )));
    }
    Ok(())
}

fn validate_lower_hex(value: &str, length: usize, field: &str) -> BuildResult<()> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(BuildError::Invalid(format!(
            "{field} must be {length} lowercase hex characters"
        )));
    }
    Ok(())
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

fn resolution_name(value: Resolution) -> &'static str {
    match value {
        Resolution::Locked => "locked",
        Resolution::LinuxVerified => "linux-verified",
        Resolution::OwnerDecision => "owner-decision",
        Resolution::Generated => "generated",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap()
    }

    fn temp_dir(label: &str) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "portus-w6-{label}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn shipped_w6_graph_compiles_to_source_valid_non_release_plan() {
        let plan = build_plan(&repo_root(), 40 * 1024).unwrap();
        assert!(plan.source_valid);
        assert!(!plan.release_resolved);
        assert_eq!(plan.packages.len(), 25);
        assert_eq!(plan.services.len(), 7);
        assert_eq!(plan.adapters.calamares_responsibility_count, 16);
        assert_eq!(plan.adapters.calamares_resolved_count, 16);
        assert_eq!(plan.adapters.custom_calamares_modules, 1);
        assert!(
            plan.unresolved
                .iter()
                .any(|item| item.id == "calamares.storage-implementation")
        );
        assert!(
            !plan
                .unresolved
                .iter()
                .any(|item| item.id == "calamares.responsibility-mapping")
        );
    }

    #[test]
    fn disk_plan_locks_minimum_and_reference_arithmetic_and_rejects_smaller_disk() {
        let minimum = build_plan(&repo_root(), 40 * 1024).unwrap().disk;
        assert_eq!(minimum.esp_mib, 512);
        assert_eq!(minimum.boot_mib, 2048);
        assert_eq!(minimum.encrypted_system_mib, 38_400);
        assert_eq!(minimum.swap_mib, 4096);
        assert_eq!(minimum.reserve_mib, 1920);
        assert_eq!(minimum.root_mib, 32_384);

        let reference = build_plan(&repo_root(), 80 * 1024).unwrap().disk;
        assert_eq!(reference.encrypted_system_mib, 79_360);
        assert_eq!(reference.reserve_mib, 3968);
        assert_eq!(reference.root_mib, 71_296);
        assert!(build_plan(&repo_root(), 39 * 1024).is_err());
    }

    #[test]
    fn package_and_service_plans_are_deterministic_and_reference_existing_authorities() {
        let first = build_plan(&repo_root(), 40 * 1024).unwrap();
        let second = build_plan(&repo_root(), 40 * 1024).unwrap();
        assert_eq!(first.packages, second.packages);
        assert_eq!(first.services, second.services);
        assert!(
            first
                .packages
                .windows(2)
                .all(|pair| { (&pair[0].profile, &pair[0].id) <= (&pair[1].profile, &pair[1].id) })
        );
        assert!(
            first
                .services
                .iter()
                .all(|service| service.lifecycle_owner == "openrc")
        );
        assert!(
            first
                .packages
                .iter()
                .any(|package| package.id == "portus-runtime"
                    && package.source_ref.as_deref() == Some(P16_INSTALL))
        );
    }

    #[test]
    fn destructive_preflight_requires_explicit_target_and_exact_confirmation_hash() {
        let disk = build_plan(&repo_root(), 40 * 1024).unwrap().disk;
        assert!(prepare_destructive_plan(None, disk.clone()).is_err());
        assert!(prepare_destructive_plan(Some(""), disk.clone()).is_err());
        assert!(prepare_destructive_plan(Some("C:\\disk"), disk.clone()).is_err());
        let plan = prepare_destructive_plan(Some("/dev/disk/by-id/fixture"), disk).unwrap();
        assert!(confirm_destructive_plan(&plan, &plan.plan_sha256).is_ok());
        assert!(confirm_destructive_plan(&plan, &"0".repeat(64)).is_err());
    }

    #[test]
    fn target_config_requires_typed_ids_and_never_emits_placeholder_values() {
        let identifiers = TargetIdentifiers {
            esp_uuid: "A1B2-C3D4".to_string(),
            boot_uuid: "11111111-2222-3333-4444-555555555555".to_string(),
            luks_uuid: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
            crypt_name: "cryptportus".to_string(),
            root_mapper: "root".to_string(),
            swap_mapper: "swap".to_string(),
        };
        let rendered = render_target_config(&repo_root(), &identifiers).unwrap();
        assert!(rendered.fstab.contains("UUID=A1B2-C3D4 /boot/efi"));
        assert!(
            rendered
                .crypttab
                .contains("cryptportus UUID=aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
        );
        assert!(!rendered.fstab.to_ascii_lowercase().contains("placeholder"));
        assert!(
            !rendered
                .crypttab
                .to_ascii_lowercase()
                .contains("placeholder")
        );
        assert_eq!(
            rendered.mkinitcpio_plan.hooks,
            vec![
                "base",
                "udev",
                "autodetect",
                "microcode",
                "modconf",
                "kms",
                "keyboard",
                "keymap",
                "block",
                "encrypt",
                "lvm2",
                "filesystems",
                "fsck"
            ]
        );
        assert_eq!(rendered.mkinitcpio_plan.required_artifacts.len(), 4);
        assert_eq!(
            rendered.grub_plan.command_line_resolution,
            Resolution::LinuxVerified
        );
    }

    #[test]
    fn validation_plan_contains_exact_38_not_run_rows_and_minimum_iso_37() {
        let plan = validation_plan(&repo_root(), "first-iso-rc.1", &"a".repeat(64)).unwrap();
        assert_eq!(plan.tests.len(), 38);
        assert!(
            plan.tests
                .iter()
                .all(|test| test.status == "not_run" && test.blocking)
        );
        let iso37 = plan
            .tests
            .iter()
            .find(|test| test.test_id == "ISO-37")
            .unwrap();
        assert_eq!(iso37.environment, "minimum");
        assert_eq!(plan.tests.last().unwrap().test_id, "ISO-38");
    }

    #[test]
    fn metadata_preview_hashes_fixture_artifact_and_sorts_sha256sums() {
        let root = temp_dir("metadata");
        let artifact = root.join("fixture.iso");
        fs::write(&artifact, b"PortusOS fixture ISO bytes").unwrap();
        let input = BuildMetadataInput {
            release_class: "development_rc".to_string(),
            candidate_id: "first-iso-rc.1-g111111111111".to_string(),
            version: None,
            rc_number: 1,
            source_revision: "1".repeat(40),
            source_tree_clean: true,
            build_started_at: "2026-08-27T00:00:00Z".to_string(),
            build_finished_at: "2026-08-27T00:01:00Z".to_string(),
            distribution_snapshot: "fixture".to_string(),
            artools_version: "fixture".to_string(),
            rust_toolchain: "1.85.0".to_string(),
            artifact_filename: "PortusOS-first-iso-rc.1-x86_64.iso".to_string(),
            validation_authority_revision: "2".repeat(40),
            release_authority_revision: "3".repeat(40),
        };
        let preview = build_metadata_preview(&repo_root(), &artifact, &input).unwrap();
        assert_eq!(preview.metadata.artifact.size_bytes, 26);
        assert_eq!(preview.metadata.artifact.sha256.len(), 64);
        let lines: Vec<_> = preview.sha256sums.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("PortusOS-first-iso-rc.1-x86_64.iso"));
        assert!(lines[1].ends_with("build-metadata.json"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn p16_staging_is_reused_without_mutating_the_real_host() {
        let root = temp_dir("stage");
        let binaries = root.join("bin");
        let staged = root.join("root");
        fs::create_dir_all(&binaries).unwrap();
        for name in [
            "portus-api",
            "portus-apid",
            "portus-auth",
            "portus-bootstrap",
            "portus-master",
            "portus-os",
            "portus-privd",
            "portusd",
        ] {
            fs::write(binaries.join(name), format!("fixture-{name}")).unwrap();
        }
        let report = stage_portus_to(&repo_root(), &binaries, &staged).unwrap();
        assert!(report.created > 0);
        assert!(staged.join("usr/bin/portus-os").exists());
        assert!(
            !staged
                .join("var/lib/portus/protected-api/credentials.db")
                .exists()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn builder_layout_has_only_bounded_generated_roots() {
        let layout = validate_builder_layout(&repo_root()).unwrap();
        assert_eq!(layout.generated.work, "portusos-build/work");
        assert_eq!(layout.generated.cache, "portusos-build/cache");
        assert_eq!(layout.generated.out, "portusos-build/out");
        assert!(!layout.clean_policy.arbitrary_path_delete);
        assert_eq!(layout.clean_policy.allowed_roots.len(), 3);
    }

    #[test]
    fn artools_boot_packages_require_memtest_payload_provider() {
        let valid = vec!["grub".to_string(), "memtest86+".to_string()];
        assert!(validate_artools_boot_packages(&valid).is_ok());

        let invalid = vec!["grub".to_string(), "iso-initcpio".to_string()];
        match validate_artools_boot_packages(&invalid) {
            Err(BuildError::Invalid(message)) => {
                assert!(message.contains("memtest86+"));
                assert!(message.contains("memtest.bin"));
            }
            other => panic!("expected missing memtest86+ to fail closed, got {other:?}"),
        }
    }

    #[test]
    fn native_iso_build_gate_accepts_the_locked_artools_mapping_on_linux() {
        let result = native_iso_build_gate(&repo_root(), false);
        #[cfg(target_os = "linux")]
        assert!(
            result.is_ok(),
            "locked Linux artools mapping should pass: {result:?}"
        );
        #[cfg(not(target_os = "linux"))]
        match result {
            Err(BuildError::Unresolved(message)) => {
                assert!(message.contains("native x86_64 Linux"))
            }
            other => panic!("expected non-Linux native build block, got {other:?}"),
        }
    }
}
