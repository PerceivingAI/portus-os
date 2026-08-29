use super::{
    BuildError, BuildMetadata, BuildMetadataInput, BuildResult, ImportedEvidence,
    ValidationReportInput, W6_SCHEMA_VERSION, aggregate_validation_report, build_metadata_preview,
    expected_candidate_identity, hex_sha256, materialize_validation_harness,
    validate_builder_layout, validate_w6, verify_validation_harness,
};
use crate::ValidationCandidate;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const BUILD_METADATA_FILE: &str = "build-metadata.json";
const CANDIDATE_FILE: &str = "candidate.json";
const SHA256SUMS_FILE: &str = "SHA256SUMS";
const PACKAGE_MANIFEST_SOURCE: &str = "portusos-build/packages/packages.yaml";
const PACKAGE_MANIFEST_FILE: &str = "package-source-manifest.yaml";
const PACKAGE_LOCK_SOURCE: &str = "portusos-build/packages/packages.lock.yaml";
const PACKAGE_LOCK_FILE: &str = "packages.lock.yaml";
const CODEX_PIN_SOURCE: &str = "portusos-build/components/codex.yaml";
const CODEX_PIN_FILE: &str = "codex-pin.yaml";
const BROWSER_PIN_SOURCE: &str = "portusos-build/components/portus-browser.yaml";
const BROWSER_PIN_FILE: &str = "portus-browser-pin.yaml";
const PORTUS_MCP_PIN_SOURCE: &str = "portusos-build/components/portus-mcp.yaml";
const PORTUS_MCP_PIN_FILE: &str = "portus-mcp-pin.yaml";
const TUNNEL_CLIENT_PIN_SOURCE: &str = "portusos-build/components/tunnel-client.yaml";
const TUNNEL_CLIENT_PIN_FILE: &str = "tunnel-client-pin.yaml";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateInitInput {
    pub release_class: String,
    pub version: Option<String>,
    pub rc_number: u32,
    pub source_revision: String,
    pub source_tree_clean: bool,
    pub build_started_at: String,
    pub build_finished_at: String,
    pub distribution_snapshot: String,
    pub artools_version: String,
    pub rust_toolchain: String,
    pub validation_authority_revision: String,
    pub release_authority_revision: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateInitSummary {
    pub schema_version: u32,
    pub candidate_id: String,
    pub candidate_root: String,
    pub iso_filename: String,
    pub iso_sha256: String,
    pub validation_tests: usize,
    pub initial_not_run: u32,
    pub package_lock_included: bool,
    pub sha256sums_ref: String,
    pub build_metadata_ref: String,
    pub candidate_ref: String,
    pub validation_report_ref: String,
    pub initial_report_status: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateBundleVerifySummary {
    pub schema_version: u32,
    pub candidate_id: String,
    pub iso_filename: String,
    pub iso_sha256: String,
    pub immutable_files_checked: usize,
    pub validation_tests: usize,
    pub pass: u32,
    pub fail: u32,
    pub blocked: u32,
    pub not_run: u32,
    pub validation_report_present: bool,
}

pub fn initialize_candidate(
    repo_root: &Path,
    artifact_path: &Path,
    input: &CandidateInitInput,
) -> BuildResult<CandidateInitSummary> {
    validate_w6(repo_root)?;
    let layout = validate_builder_layout(repo_root)?;
    let output_root = repo_root.join(&layout.generated.out).join("validation");
    initialize_candidate_to(repo_root, artifact_path, input, &output_root)
}

pub fn verify_candidate_bundle(
    repo_root: &Path,
    candidate_root: &Path,
) -> BuildResult<CandidateBundleVerifySummary> {
    validate_w6(repo_root)?;
    let candidate: ValidationCandidate =
        read_json(&candidate_root.join(CANDIDATE_FILE), "candidate")?;
    let build: BuildMetadata =
        read_json(&candidate_root.join(BUILD_METADATA_FILE), "build metadata")?;

    verify_build_candidate_linkage(&candidate, &build)?;
    let expected = expected_candidate_identity(
        &build.release_class,
        build.version.as_deref(),
        build.rc_number,
        &build.source_revision,
    )?;
    if expected.candidate_id != candidate.candidate_id
        || expected.artifact_filename != candidate.iso_filename
    {
        return invalid("candidate identity does not match release authority derivation");
    }

    let iso_path = candidate_root.join(&candidate.iso_filename);
    let iso = regular_file_metadata(&iso_path, "candidate ISO")?;
    if iso.len() == 0 || iso.len() != build.artifact.size_bytes {
        return invalid("candidate ISO size does not match build metadata");
    }
    let iso_sha = hash_file(&iso_path)?;
    if iso_sha != candidate.iso_sha256 || iso_sha != build.artifact.sha256 {
        return invalid("candidate ISO SHA-256 does not match candidate/build metadata");
    }

    for (reference, label) in [
        (
            &candidate.build_metadata_ref,
            "candidate build metadata reference",
        ),
        (
            &candidate.package_source_manifest_ref,
            "candidate package/source reference",
        ),
        (
            &build.package_source_manifest_ref,
            "build package/source reference",
        ),
        (&build.codex_pin_ref, "build Codex pin reference"),
        (
            &build.portus_browser_pin_ref,
            "build PortusBrowser pin reference",
        ),
    ] {
        validate_local_basename(reference, label)?;
        regular_file_metadata(&candidate_root.join(reference), label)?;
    }

    let checksum_path = candidate_root.join(SHA256SUMS_FILE);
    let checksum_entries = read_checksum_manifest(&checksum_path)?;
    let expected_names = immutable_bundle_names(candidate_root, &candidate)?;
    let actual_names: Vec<_> = checksum_entries.keys().cloned().collect();
    if actual_names != expected_names {
        return invalid("SHA256SUMS does not contain the exact immutable candidate file set");
    }
    for (name, expected_hash) in &checksum_entries {
        let actual_hash = hash_file(&candidate_root.join(name))?;
        if &actual_hash != expected_hash {
            return invalid(format!("SHA256SUMS mismatch for {name}"));
        }
    }

    let validation = verify_validation_harness(repo_root, candidate_root)?;
    Ok(CandidateBundleVerifySummary {
        schema_version: W6_SCHEMA_VERSION,
        candidate_id: candidate.candidate_id,
        iso_filename: candidate.iso_filename,
        iso_sha256: candidate.iso_sha256,
        immutable_files_checked: checksum_entries.len(),
        validation_tests: validation.validation_tests,
        pass: validation.pass,
        fail: validation.fail,
        blocked: validation.blocked,
        not_run: validation.not_run,
        validation_report_present: validation.report_present,
    })
}

fn initialize_candidate_to(
    repo_root: &Path,
    artifact_path: &Path,
    input: &CandidateInitInput,
    output_root: &Path,
) -> BuildResult<CandidateInitSummary> {
    validate_candidate_init_input(input)?;
    let source_meta = regular_file_metadata(artifact_path, "source ISO")?;
    if source_meta.len() == 0 {
        return invalid("source ISO must be non-empty");
    }

    let identity = expected_candidate_identity(
        &input.release_class,
        input.version.as_deref(),
        input.rc_number,
        &input.source_revision,
    )?;
    let metadata_input = BuildMetadataInput {
        release_class: input.release_class.clone(),
        candidate_id: identity.candidate_id.clone(),
        version: input.version.clone(),
        rc_number: input.rc_number,
        source_revision: input.source_revision.clone(),
        source_tree_clean: input.source_tree_clean,
        build_started_at: input.build_started_at.clone(),
        build_finished_at: input.build_finished_at.clone(),
        distribution_snapshot: input.distribution_snapshot.clone(),
        artools_version: input.artools_version.clone(),
        rust_toolchain: input.rust_toolchain.clone(),
        artifact_filename: identity.artifact_filename.clone(),
        validation_authority_revision: input.validation_authority_revision.clone(),
        release_authority_revision: input.release_authority_revision.clone(),
    };
    let preview = build_metadata_preview(repo_root, artifact_path, &metadata_input)?;
    let iso_sha256 = preview.metadata.artifact.sha256.clone();
    let candidate = ValidationCandidate {
        schema_version: W6_SCHEMA_VERSION,
        candidate_id: identity.candidate_id.clone(),
        iso_filename: identity.artifact_filename.clone(),
        iso_sha256: iso_sha256.clone(),
        source_revision: input.source_revision.clone(),
        build_metadata_ref: BUILD_METADATA_FILE.to_string(),
        package_source_manifest_ref: PACKAGE_MANIFEST_FILE.to_string(),
        validation_authority_revision: input.validation_authority_revision.clone(),
        created_at: input.created_at.clone(),
    };

    let harness = materialize_validation_harness(repo_root, output_root, &candidate)?;
    let candidate_root = PathBuf::from(&harness.candidate_root);
    let result = (|| {
        copy_new_regular(
            artifact_path,
            &candidate_root.join(&identity.artifact_filename),
            "candidate ISO",
        )?;
        copy_source_snapshot(
            repo_root,
            PACKAGE_MANIFEST_SOURCE,
            &candidate_root.join(PACKAGE_MANIFEST_FILE),
        )?;
        copy_source_snapshot(
            repo_root,
            CODEX_PIN_SOURCE,
            &candidate_root.join(CODEX_PIN_FILE),
        )?;
        copy_source_snapshot(
            repo_root,
            BROWSER_PIN_SOURCE,
            &candidate_root.join(BROWSER_PIN_FILE),
        )?;
        copy_source_snapshot(
            repo_root,
            PORTUS_MCP_PIN_SOURCE,
            &candidate_root.join(PORTUS_MCP_PIN_FILE),
        )?;
        copy_source_snapshot(
            repo_root,
            TUNNEL_CLIENT_PIN_SOURCE,
            &candidate_root.join(TUNNEL_CLIENT_PIN_FILE),
        )?;
        let package_lock_included = if repo_root.join(PACKAGE_LOCK_SOURCE).exists() {
            copy_source_snapshot(
                repo_root,
                PACKAGE_LOCK_SOURCE,
                &candidate_root.join(PACKAGE_LOCK_FILE),
            )?;
            true
        } else {
            false
        };

        let mut build_metadata = preview.metadata;
        build_metadata.package_source_manifest_ref = PACKAGE_MANIFEST_FILE.to_string();
        build_metadata.codex_pin_ref = CODEX_PIN_FILE.to_string();
        build_metadata.portus_browser_pin_ref = BROWSER_PIN_FILE.to_string();
        build_metadata.portus_mcp_pin_ref = PORTUS_MCP_PIN_FILE.to_string();
        build_metadata.tunnel_client_pin_ref = TUNNEL_CLIENT_PIN_FILE.to_string();
        write_json_new(&candidate_root.join(BUILD_METADATA_FILE), &build_metadata)?;

        let checksum_names = immutable_bundle_names(&candidate_root, &candidate)?;
        let checksum = render_checksum_manifest(&candidate_root, &checksum_names)?;
        write_bytes_new(&candidate_root.join(SHA256SUMS_FILE), checksum.as_bytes())?;

        let initial_report = aggregate_validation_report(
            repo_root,
            &candidate_root,
            &ValidationReportInput {
                started_at: input.created_at.clone(),
                ended_at: input.created_at.clone(),
                imported_evidence: ImportedEvidence {
                    host_safe: Vec::new(),
                    update: Vec::new(),
                    protected_api: Vec::new(),
                },
                known_limitations_ref: "KNOWN_LIMITATIONS.md".to_string(),
            },
        )?;
        if initial_report.status != "incomplete" || initial_report.counts.not_run != 38 {
            return invalid(
                "fresh candidate report must initialize as incomplete with 38 not_run rows",
            );
        }

        let verified = verify_candidate_bundle(repo_root, &candidate_root)?;
        if verified.validation_tests != 38
            || verified.pass != 0
            || verified.fail != 0
            || verified.blocked != 0
            || verified.not_run != 38
            || !verified.validation_report_present
        {
            return invalid("fresh candidate did not verify as exact 38-row not_run state");
        }

        Ok(CandidateInitSummary {
            schema_version: W6_SCHEMA_VERSION,
            candidate_id: candidate.candidate_id.clone(),
            candidate_root: candidate_root.to_string_lossy().into_owned(),
            iso_filename: candidate.iso_filename.clone(),
            iso_sha256,
            validation_tests: verified.validation_tests,
            initial_not_run: verified.not_run,
            package_lock_included,
            sha256sums_ref: SHA256SUMS_FILE.to_string(),
            build_metadata_ref: BUILD_METADATA_FILE.to_string(),
            candidate_ref: CANDIDATE_FILE.to_string(),
            validation_report_ref: "validation-report.json".to_string(),
            initial_report_status: initial_report.status,
        })
    })();

    if result.is_err() {
        fs::remove_dir_all(&candidate_root)?;
    }
    result
}

fn validate_candidate_init_input(input: &CandidateInitInput) -> BuildResult<()> {
    let identity = expected_candidate_identity(
        &input.release_class,
        input.version.as_deref(),
        input.rc_number,
        &input.source_revision,
    )?;
    let metadata = BuildMetadataInput {
        release_class: input.release_class.clone(),
        candidate_id: identity.candidate_id,
        version: input.version.clone(),
        rc_number: input.rc_number,
        source_revision: input.source_revision.clone(),
        source_tree_clean: input.source_tree_clean,
        build_started_at: input.build_started_at.clone(),
        build_finished_at: input.build_finished_at.clone(),
        distribution_snapshot: input.distribution_snapshot.clone(),
        artools_version: input.artools_version.clone(),
        rust_toolchain: input.rust_toolchain.clone(),
        artifact_filename: identity.artifact_filename,
        validation_authority_revision: input.validation_authority_revision.clone(),
        release_authority_revision: input.release_authority_revision.clone(),
    };
    super::validate_metadata_input(&metadata)?;
    validate_timestamp(&input.created_at, "candidate created_at")
}

fn verify_build_candidate_linkage(
    candidate: &ValidationCandidate,
    build: &BuildMetadata,
) -> BuildResult<()> {
    if build.builder.architecture != "x86_64" || build.builder.distribution != "Artix Linux" {
        return invalid("build metadata builder must remain x86_64 Artix Linux");
    }
    let semantic_input = BuildMetadataInput {
        release_class: build.release_class.clone(),
        candidate_id: build.candidate_id.clone(),
        version: build.version.clone(),
        rc_number: build.rc_number,
        source_revision: build.source_revision.clone(),
        source_tree_clean: build.source_tree_clean,
        build_started_at: build.build_started_at.clone(),
        build_finished_at: build.build_finished_at.clone(),
        distribution_snapshot: build.builder.distribution_snapshot.clone(),
        artools_version: build.builder.artools_version.clone(),
        rust_toolchain: build.builder.rust_toolchain.clone(),
        artifact_filename: build.artifact.filename.clone(),
        validation_authority_revision: build.validation_authority_revision.clone(),
        release_authority_revision: build.release_authority_revision.clone(),
    };
    super::validate_metadata_input(&semantic_input)?;
    if build.schema_version != W6_SCHEMA_VERSION
        || build.candidate_id != candidate.candidate_id
        || build.artifact.filename != candidate.iso_filename
        || build.artifact.sha256 != candidate.iso_sha256
        || build.source_revision != candidate.source_revision
        || build.validation_authority_revision != candidate.validation_authority_revision
        || candidate.build_metadata_ref != BUILD_METADATA_FILE
        || candidate.package_source_manifest_ref != PACKAGE_MANIFEST_FILE
        || build.package_source_manifest_ref != PACKAGE_MANIFEST_FILE
        || build.codex_pin_ref != CODEX_PIN_FILE
        || build.portus_browser_pin_ref != BROWSER_PIN_FILE
        || build.portus_mcp_pin_ref != PORTUS_MCP_PIN_FILE
        || build.tunnel_client_pin_ref != TUNNEL_CLIENT_PIN_FILE
    {
        return invalid("candidate.json and build-metadata.json linkage mismatch");
    }
    Ok(())
}

fn immutable_bundle_names(
    candidate_root: &Path,
    candidate: &ValidationCandidate,
) -> BuildResult<Vec<String>> {
    let mut names = vec![
        BUILD_METADATA_FILE.to_string(),
        CANDIDATE_FILE.to_string(),
        CODEX_PIN_FILE.to_string(),
        PACKAGE_MANIFEST_FILE.to_string(),
        BROWSER_PIN_FILE.to_string(),
        PORTUS_MCP_PIN_FILE.to_string(),
        TUNNEL_CLIENT_PIN_FILE.to_string(),
        candidate.iso_filename.clone(),
    ];
    if candidate_root.join(PACKAGE_LOCK_FILE).exists() {
        names.push(PACKAGE_LOCK_FILE.to_string());
    }
    names.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    names.dedup();
    for name in &names {
        validate_local_basename(name, "immutable candidate filename")?;
        regular_file_metadata(&candidate_root.join(name), "immutable candidate file")?;
    }
    Ok(names)
}

fn render_checksum_manifest(candidate_root: &Path, names: &[String]) -> BuildResult<String> {
    let mut lines = Vec::with_capacity(names.len());
    for name in names {
        let hash = hash_file(&candidate_root.join(name))?;
        lines.push(format!("{hash}  {name}"));
    }
    Ok(format!("{}\n", lines.join("\n")))
}

fn read_checksum_manifest(path: &Path) -> BuildResult<BTreeMap<String, String>> {
    regular_file_metadata(path, "SHA256SUMS")?;
    let text = fs::read_to_string(path)?;
    if text.is_empty() || !text.ends_with('\n') {
        return invalid("SHA256SUMS must be non-empty and newline-terminated");
    }
    let mut entries = BTreeMap::new();
    let mut ordered = Vec::new();
    for line in text.lines() {
        let (hash, name) = line
            .split_once("  ")
            .ok_or_else(|| BuildError::Invalid("invalid SHA256SUMS line format".to_string()))?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return invalid("SHA256SUMS contains a non-lowercase SHA-256 value");
        }
        validate_local_basename(name, "SHA256SUMS filename")?;
        if entries.insert(name.to_string(), hash.to_string()).is_some() {
            return invalid("SHA256SUMS contains a duplicate filename");
        }
        ordered.push(name.to_string());
    }
    let sorted: Vec<_> = entries.keys().cloned().collect();
    if ordered != sorted {
        return invalid("SHA256SUMS entries must be bytewise basename-sorted");
    }
    Ok(entries)
}

fn copy_source_snapshot(
    repo_root: &Path,
    source_relative: &str,
    destination: &Path,
) -> BuildResult<()> {
    let source = repo_root.join(source_relative);
    copy_new_regular(&source, destination, source_relative)
}

fn copy_new_regular(source: &Path, destination: &Path, label: &str) -> BuildResult<()> {
    regular_file_metadata(source, label)?;
    if destination.exists() {
        return invalid(format!("refusing to overwrite existing {label}"));
    }
    let bytes = fs::read(source)?;
    if bytes.is_empty() {
        return invalid(format!("{label} must be non-empty"));
    }
    write_bytes_new(destination, &bytes)
}

fn regular_file_metadata(path: &Path, label: &str) -> BuildResult<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return invalid(format!("{label} must be a regular non-symlink file"));
    }
    Ok(metadata)
}

fn hash_file(path: &Path) -> BuildResult<String> {
    Ok(hex_sha256(&fs::read(path)?))
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> BuildResult<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        BuildError::Invalid(format!("cannot serialize candidate JSON: {error}"))
    })?;
    write_bytes_new(path, &bytes)
}

fn write_bytes_new(path: &Path, bytes: &[u8]) -> BuildResult<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, label: &str) -> BuildResult<T> {
    serde_json::from_str(&fs::read_to_string(path)?)
        .map_err(|error| BuildError::Invalid(format!("invalid {label} JSON: {error}")))
}

fn validate_local_basename(value: &str, label: &str) -> BuildResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return invalid(format!("{label} must be a safe local basename"));
    }
    Ok(())
}

fn validate_timestamp(value: &str, label: &str) -> BuildResult<()> {
    if value.len() < 20
        || !value.contains('T')
        || !value.ends_with('Z')
        || value.chars().any(char::is_control)
    {
        return invalid(format!(
            "{label} must be a UTC RFC3339-like timestamp ending in Z"
        ));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> BuildResult<T> {
    Err(BuildError::Invalid(message.into()))
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
            "portus-candidate-{label}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn input() -> CandidateInitInput {
        CandidateInitInput {
            release_class: "development_rc".to_string(),
            version: None,
            rc_number: 3,
            source_revision: "1".repeat(40),
            source_tree_clean: false,
            build_started_at: "2026-08-27T00:00:00Z".to_string(),
            build_finished_at: "2026-08-27T00:01:00Z".to_string(),
            distribution_snapshot: "fixture-artix".to_string(),
            artools_version: "fixture-artools".to_string(),
            rust_toolchain: "1.85.0".to_string(),
            validation_authority_revision: "2".repeat(40),
            release_authority_revision: "3".repeat(40),
            created_at: "2026-08-27T00:02:00Z".to_string(),
        }
    }

    fn initialized(label: &str) -> (PathBuf, CandidateInitSummary) {
        let root = temp_dir(label);
        let artifact = root.join("arbitrary-builder-output.iso");
        fs::write(&artifact, b"PortusOS deterministic fixture ISO").unwrap();
        let output = root.join("out");
        let summary = initialize_candidate_to(&repo_root(), &artifact, &input(), &output).unwrap();
        (root, summary)
    }

    #[test]
    fn candidate_identity_is_derived_and_source_artifact_name_is_irrelevant() {
        let (root, summary) = initialized("identity");
        assert_eq!(summary.candidate_id, "first-iso-rc.3-g111111111111");
        assert_eq!(summary.iso_filename, "PortusOS-first-iso-rc.3-x86_64.iso");
        assert_eq!(summary.validation_tests, 38);
        assert_eq!(summary.initial_not_run, 38);
        assert_eq!(summary.initial_report_status, "incomplete");
        assert!(!summary.package_lock_included);
        assert!(
            PathBuf::from(&summary.candidate_root)
                .join(&summary.iso_filename)
                .is_file()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initialized_candidate_bundle_is_self_consistent_and_checksum_bound() {
        let (root, summary) = initialized("verify");
        let candidate_root = PathBuf::from(&summary.candidate_root);
        let verified = verify_candidate_bundle(&repo_root(), &candidate_root).unwrap();
        assert_eq!(verified.candidate_id, summary.candidate_id);
        assert_eq!(verified.iso_sha256, summary.iso_sha256);
        assert_eq!(verified.immutable_files_checked, 8);
        assert_eq!(verified.not_run, 38);
        assert!(verified.validation_report_present);
        let sums = fs::read_to_string(candidate_root.join(SHA256SUMS_FILE)).unwrap();
        assert!(sums.contains("  build-metadata.json\n"));
        assert!(sums.contains("  candidate.json\n"));
        assert!(sums.contains("  package-source-manifest.yaml\n"));
        assert!(sums.contains("  portus-mcp-pin.yaml\n"));
        assert!(sums.contains("  tunnel-client-pin.yaml\n"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_reuse_and_changed_iso_bytes_are_rejected() {
        let (root, summary) = initialized("immutable");
        let artifact = root.join("second.iso");
        fs::write(&artifact, b"different fixture bytes").unwrap();
        let output = root.join("out");
        assert!(initialize_candidate_to(&repo_root(), &artifact, &input(), &output).is_err());

        let candidate_root = PathBuf::from(&summary.candidate_root);
        fs::write(
            candidate_root.join(&summary.iso_filename),
            b"mutated candidate bytes",
        )
        .unwrap();
        assert!(verify_candidate_bundle(&repo_root(), &candidate_root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checksum_or_metadata_substitution_is_detected() {
        let (root, summary) = initialized("substitution");
        let candidate_root = PathBuf::from(&summary.candidate_root);
        fs::write(candidate_root.join(CODEX_PIN_FILE), b"substituted pin\n").unwrap();
        assert!(verify_candidate_bundle(&repo_root(), &candidate_root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn public_rc_requires_semver_and_clean_source() {
        let mut value = input();
        value.release_class = "public_rc".to_string();
        value.version = Some("0.1.0".to_string());
        assert!(validate_candidate_init_input(&value).is_err());
        value.source_tree_clean = true;
        validate_candidate_init_input(&value).unwrap();
        let identity = expected_candidate_identity(
            &value.release_class,
            value.version.as_deref(),
            value.rc_number,
            &value.source_revision,
        )
        .unwrap();
        assert_eq!(identity.candidate_id, "0.1.0-rc.3-g111111111111");
        assert_eq!(identity.artifact_filename, "PortusOS-0.1.0-rc.3-x86_64.iso");
        value.version = Some("0.1".to_string());
        assert!(validate_candidate_init_input(&value).is_err());
    }
}
