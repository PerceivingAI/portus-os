use super::{BuildError, BuildResult, W6_SCHEMA_VERSION, hex_sha256, validation_plan};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

const CANDIDATE_FILE: &str = "candidate.json";
const REDACTIONS_FILE: &str = "redactions.json";
const REPORT_JSON: &str = "validation-report.json";
const REPORT_MD: &str = "validation-report.md";
const EVIDENCE_MANIFEST: &str = "evidence-manifest.json";
const MAX_COMMAND_RECORD_BYTES: usize = 8192;
const MAX_SAFE_TEXT_BYTES: usize = 4096;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationCandidate {
    pub schema_version: u32,
    pub candidate_id: String,
    pub iso_filename: String,
    pub iso_sha256: String,
    pub source_revision: String,
    pub build_metadata_ref: String,
    pub package_source_manifest_ref: String,
    pub validation_authority_revision: String,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Pass,
    Fail,
    Blocked,
    NotRun,
}

impl ValidationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Blocked => "blocked",
            Self::NotRun => "not_run",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionStatus {
    Pass,
    Fail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationAssertion {
    pub id: String,
    pub status: AssertionStatus,
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationResult {
    pub schema_version: u32,
    pub candidate_id: String,
    pub iso_sha256: String,
    pub test_id: String,
    pub execution_class: String,
    pub status: ValidationStatus,
    pub environment: String,
    pub started_at: String,
    pub ended_at: String,
    pub assertions: Vec<ValidationAssertion>,
    pub commands_ref: String,
    pub evidence_refs: Vec<String>,
    pub redaction_applied: bool,
    pub notes: Vec<String>,
    pub blocking_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationCommandRecord {
    pub sequence: u64,
    pub actor: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout_ref: Option<String>,
    #[serde(default)]
    pub stderr_ref: Option<String>,
    #[serde(default)]
    pub secret_recorded: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedactionLedger {
    pub schema_version: u32,
    pub candidate_id: String,
    pub files_scanned: u64,
    pub files_redacted: Vec<String>,
    pub forbidden_material_detected: bool,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedEvidence {
    pub host_safe: Vec<String>,
    pub update: Vec<String>,
    pub protected_api: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationReportInput {
    pub started_at: String,
    pub ended_at: String,
    pub imported_evidence: ImportedEvidence,
    pub known_limitations_ref: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationCounts {
    pub pass: u32,
    pub fail: u32,
    pub blocked: u32,
    pub not_run: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationReportTest {
    pub test_id: String,
    pub status: ValidationStatus,
    pub result_ref: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub candidate_id: String,
    pub iso_filename: String,
    pub iso_sha256: String,
    pub source_revision: String,
    pub validation_authority_revision: String,
    pub reference_environment_ref: String,
    pub minimum_environment_ref: String,
    pub started_at: String,
    pub ended_at: String,
    pub status: String,
    pub counts: ValidationCounts,
    pub tests: Vec<ValidationReportTest>,
    pub imported_evidence: ImportedEvidence,
    pub known_limitations_ref: String,
    pub redactions_ref: String,
    pub accepted_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ValidationHarnessSummary {
    pub schema_version: u32,
    pub candidate_id: String,
    pub candidate_root: String,
    pub validation_tests: usize,
    pub initial_status: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ValidationHarnessCheck {
    pub schema_version: u32,
    pub authority: String,
    pub validation_tests: usize,
    pub harness_ready: bool,
    pub vm_execution_available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ValidationVerifySummary {
    pub schema_version: u32,
    pub candidate_id: String,
    pub validation_tests: usize,
    pub pass: u32,
    pub fail: u32,
    pub blocked: u32,
    pub not_run: u32,
    pub report_present: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct EvidenceManifest {
    schema_version: u32,
    candidate_id: String,
    iso_sha256: String,
    test_id: String,
    files: Vec<EvidenceFile>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct EvidenceFile {
    path: String,
    sha256: String,
    size_bytes: u64,
}

pub fn validation_harness_check(repo_root: &Path) -> BuildResult<ValidationHarnessCheck> {
    let plan = validation_plan(repo_root, "harness-check", &"0".repeat(64))?;
    Ok(ValidationHarnessCheck {
        schema_version: W6_SCHEMA_VERSION,
        authority: plan.authority,
        validation_tests: plan.tests.len(),
        harness_ready: true,
        vm_execution_available: false,
    })
}

pub fn validation_vm_gate() -> BuildResult<()> {
    Err(BuildError::Unresolved(
        "VMware execution adapter remains Track V; the ISO-01..38 evidence harness is available but cannot claim VM execution yet".to_string(),
    ))
}

pub fn materialize_validation_harness(
    repo_root: &Path,
    output_root: &Path,
    candidate: &ValidationCandidate,
) -> BuildResult<ValidationHarnessSummary> {
    validate_candidate(candidate)?;
    let plan = validation_plan(repo_root, &candidate.candidate_id, &candidate.iso_sha256)?;
    ensure_directory_root(output_root)?;
    let candidate_root = output_root.join(&candidate.candidate_id);
    if candidate_root.exists() {
        return invalid(format!(
            "candidate evidence root already exists: {}",
            candidate_root.display()
        ));
    }
    fs::create_dir(&candidate_root)?;
    let result = (|| {
        write_json_new(&candidate_root.join(CANDIDATE_FILE), candidate)?;
        fs::create_dir(candidate_root.join("environment"))?;
        for group in ["host-safe", "update", "protected-api"] {
            fs::create_dir_all(candidate_root.join("imported").join(group))?;
        }
        let ledger = RedactionLedger {
            schema_version: W6_SCHEMA_VERSION,
            candidate_id: candidate.candidate_id.clone(),
            files_scanned: 0,
            files_redacted: Vec::new(),
            forbidden_material_detected: false,
            notes: Vec::new(),
        };
        write_json_new(&candidate_root.join(REDACTIONS_FILE), &ledger)?;
        for entry in &plan.tests {
            let test_root = candidate_root.join("tests").join(&entry.test_id);
            fs::create_dir_all(test_root.join("artifacts"))?;
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(test_root.join("commands.jsonl"))?;
            let initial = ValidationResult {
                schema_version: W6_SCHEMA_VERSION,
                candidate_id: candidate.candidate_id.clone(),
                iso_sha256: candidate.iso_sha256.clone(),
                test_id: entry.test_id.clone(),
                execution_class: entry.execution_class.clone(),
                status: ValidationStatus::NotRun,
                environment: entry.environment.clone(),
                started_at: candidate.created_at.clone(),
                ended_at: candidate.created_at.clone(),
                assertions: Vec::new(),
                commands_ref: "commands.jsonl".to_string(),
                evidence_refs: Vec::new(),
                redaction_applied: false,
                notes: Vec::new(),
                blocking_reason: Some("not executed".to_string()),
            };
            validate_result_against_plan(&initial, candidate, &plan)?;
            write_json_new(&test_root.join("result.json"), &initial)?;
        }
        Ok(ValidationHarnessSummary {
            schema_version: W6_SCHEMA_VERSION,
            candidate_id: candidate.candidate_id.clone(),
            candidate_root: candidate_root.display().to_string(),
            validation_tests: plan.tests.len(),
            initial_status: "incomplete".to_string(),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&candidate_root);
    }
    result
}

pub fn append_validation_command(
    repo_root: &Path,
    candidate_root: &Path,
    test_id: &str,
    record: &ValidationCommandRecord,
) -> BuildResult<()> {
    let candidate = read_candidate(candidate_root)?;
    let plan = validation_plan(repo_root, &candidate.candidate_id, &candidate.iso_sha256)?;
    let entry = plan
        .tests
        .iter()
        .find(|entry| entry.test_id == test_id)
        .ok_or_else(|| BuildError::Invalid(format!("unknown validation test id {test_id}")))?;
    if entry.test_id != test_id {
        return invalid("validation test identity mismatch");
    }
    validate_command_record(record)?;
    let test_root = test_root(candidate_root, test_id)?;
    for reference in [&record.stdout_ref, &record.stderr_ref]
        .into_iter()
        .flatten()
    {
        resolve_existing_regular(&test_root, reference)?;
    }
    let commands_path = test_root.join("commands.jsonl");
    let existing = fs::read_to_string(&commands_path)?;
    let mut expected_sequence = 1_u64;
    for line in existing.lines().filter(|line| !line.trim().is_empty()) {
        let prior: ValidationCommandRecord = serde_json::from_str(line).map_err(|error| {
            BuildError::Invalid(format!("invalid existing command record: {error}"))
        })?;
        validate_command_record(&prior)?;
        if prior.sequence != expected_sequence {
            return invalid("existing command sequence is not contiguous from 1");
        }
        expected_sequence = expected_sequence
            .checked_add(1)
            .ok_or_else(|| BuildError::Invalid("command sequence overflow".to_string()))?;
    }
    if record.sequence != expected_sequence {
        return invalid(format!(
            "command sequence must be {expected_sequence}, got {}",
            record.sequence
        ));
    }
    let serialized = serde_json::to_string(record).map_err(|error| {
        BuildError::Invalid(format!("cannot serialize command record: {error}"))
    })?;
    if serialized.len() > MAX_COMMAND_RECORD_BYTES {
        return invalid("command record exceeds 8192-byte bound");
    }
    let mut file = OpenOptions::new().append(true).open(commands_path)?;
    writeln!(file, "{serialized}")?;
    Ok(())
}

pub fn record_validation_result(
    repo_root: &Path,
    candidate_root: &Path,
    result: &ValidationResult,
) -> BuildResult<()> {
    let candidate = read_candidate(candidate_root)?;
    let plan = validation_plan(repo_root, &candidate.candidate_id, &candidate.iso_sha256)?;
    validate_result_against_plan(result, &candidate, &plan)?;
    let test_root = test_root(candidate_root, &result.test_id)?;
    validate_result_evidence(&test_root, result)?;
    let result_path = test_root.join("result.json");
    let prior: ValidationResult = read_json(&result_path, "validation result")?;
    validate_result_against_plan(&prior, &candidate, &plan)?;
    if prior.status != ValidationStatus::NotRun {
        archive_prior_attempt(&test_root, &prior)?;
    }
    let manifest = build_evidence_manifest(&test_root, result)?;
    write_json_replace(&test_root.join(EVIDENCE_MANIFEST), &manifest)?;
    write_json_replace(&result_path, result)?;
    Ok(())
}

pub fn record_redaction_ledger(candidate_root: &Path, ledger: &RedactionLedger) -> BuildResult<()> {
    let candidate = read_candidate(candidate_root)?;
    if ledger.schema_version != W6_SCHEMA_VERSION || ledger.candidate_id != candidate.candidate_id {
        return invalid("redaction ledger candidate/schema mismatch");
    }
    if usize::try_from(ledger.files_scanned).unwrap_or(usize::MAX) < ledger.files_redacted.len() {
        return invalid("redaction ledger cannot redact more files than it scanned");
    }
    validate_notes(&ledger.notes)?;
    for reference in &ledger.files_redacted {
        resolve_existing_regular(candidate_root, reference)?;
    }
    write_json_replace(&candidate_root.join(REDACTIONS_FILE), ledger)
}

pub fn aggregate_validation_report(
    repo_root: &Path,
    candidate_root: &Path,
    input: &ValidationReportInput,
) -> BuildResult<ValidationReport> {
    validate_timestamp(&input.started_at, "validation report start")?;
    validate_timestamp(&input.ended_at, "validation report end")?;
    validate_repo_reference(&input.known_limitations_ref, "known limitations reference")?;
    let candidate = read_candidate(candidate_root)?;
    let plan = validation_plan(repo_root, &candidate.candidate_id, &candidate.iso_sha256)?;
    let ledger: RedactionLedger =
        read_json(&candidate_root.join(REDACTIONS_FILE), "redaction ledger")?;
    if ledger.schema_version != W6_SCHEMA_VERSION || ledger.candidate_id != candidate.candidate_id {
        return invalid("redaction ledger candidate/schema mismatch");
    }
    let mut counts = ValidationCounts {
        pass: 0,
        fail: 0,
        blocked: 0,
        not_run: 0,
    };
    let mut tests = Vec::with_capacity(plan.tests.len());
    for entry in &plan.tests {
        let root = test_root(candidate_root, &entry.test_id)?;
        let result: ValidationResult = read_json(&root.join("result.json"), "validation result")?;
        validate_result_against_plan(&result, &candidate, &plan)?;
        if result.status != ValidationStatus::NotRun {
            verify_evidence_manifest(&root, &result)?;
        }
        increment_count(&mut counts, result.status);
        tests.push(ValidationReportTest {
            test_id: entry.test_id.clone(),
            status: result.status,
            result_ref: format!("tests/{}/result.json", entry.test_id),
        });
    }
    validate_import_refs(candidate_root, &input.imported_evidence)?;
    let reference_environment = candidate_root.join("environment/reference.json");
    let minimum_environment = candidate_root.join("environment/minimum.json");
    let environments_ready =
        is_nonempty_regular(&reference_environment)? && is_nonempty_regular(&minimum_environment)?;
    let known_limitations_ready =
        resolve_existing_regular(repo_root, &input.known_limitations_ref).is_ok();
    let imports_ready = imported_groups_pass(candidate_root, &input.imported_evidence)?;
    let all_pass =
        counts.pass == 38 && counts.fail == 0 && counts.blocked == 0 && counts.not_run == 0;
    let rejected = counts.fail > 0 || ledger.forbidden_material_detected;
    let accepted =
        all_pass && !rejected && environments_ready && known_limitations_ready && imports_ready;
    let status = if accepted {
        "accepted"
    } else if rejected {
        "rejected"
    } else {
        "incomplete"
    };
    let report = ValidationReport {
        schema_version: W6_SCHEMA_VERSION,
        candidate_id: candidate.candidate_id,
        iso_filename: candidate.iso_filename,
        iso_sha256: candidate.iso_sha256,
        source_revision: candidate.source_revision,
        validation_authority_revision: candidate.validation_authority_revision,
        reference_environment_ref: "environment/reference.json".to_string(),
        minimum_environment_ref: "environment/minimum.json".to_string(),
        started_at: input.started_at.clone(),
        ended_at: input.ended_at.clone(),
        status: status.to_string(),
        counts,
        tests,
        imported_evidence: input.imported_evidence.clone(),
        known_limitations_ref: input.known_limitations_ref.clone(),
        redactions_ref: REDACTIONS_FILE.to_string(),
        accepted_at: accepted.then(|| input.ended_at.clone()),
    };
    write_json_replace(&candidate_root.join(REPORT_JSON), &report)?;
    fs::write(
        candidate_root.join(REPORT_MD),
        render_report_markdown(&report),
    )?;
    Ok(report)
}

pub fn verify_validation_harness(
    repo_root: &Path,
    candidate_root: &Path,
) -> BuildResult<ValidationVerifySummary> {
    let candidate = read_candidate(candidate_root)?;
    let plan = validation_plan(repo_root, &candidate.candidate_id, &candidate.iso_sha256)?;
    let mut counts = ValidationCounts {
        pass: 0,
        fail: 0,
        blocked: 0,
        not_run: 0,
    };
    for entry in &plan.tests {
        let root = test_root(candidate_root, &entry.test_id)?;
        let result: ValidationResult = read_json(&root.join("result.json"), "validation result")?;
        validate_result_against_plan(&result, &candidate, &plan)?;
        if result.status != ValidationStatus::NotRun {
            verify_evidence_manifest(&root, &result)?;
        }
        verify_archived_attempts(&root, &candidate, &plan)?;
        increment_count(&mut counts, result.status);
    }
    let ledger: RedactionLedger =
        read_json(&candidate_root.join(REDACTIONS_FILE), "redaction ledger")?;
    if ledger.schema_version != W6_SCHEMA_VERSION || ledger.candidate_id != candidate.candidate_id {
        return invalid("redaction ledger candidate/schema mismatch");
    }
    let report_present = candidate_root.join(REPORT_JSON).exists();
    if report_present {
        verify_report_file(repo_root, candidate_root, &candidate, &counts)?;
    }
    Ok(ValidationVerifySummary {
        schema_version: W6_SCHEMA_VERSION,
        candidate_id: candidate.candidate_id,
        validation_tests: plan.tests.len(),
        pass: counts.pass,
        fail: counts.fail,
        blocked: counts.blocked,
        not_run: counts.not_run,
        report_present,
    })
}

fn validate_candidate(candidate: &ValidationCandidate) -> BuildResult<()> {
    if candidate.schema_version != W6_SCHEMA_VERSION {
        return invalid("candidate schema_version must be 1");
    }
    validate_component(&candidate.candidate_id, "candidate id")?;
    validate_component(&candidate.iso_filename, "ISO filename")?;
    validate_lower_hex_local(&candidate.iso_sha256, 64, "ISO SHA-256")?;
    validate_lower_hex_local(&candidate.source_revision, 40, "source revision")?;
    validate_lower_hex_local(
        &candidate.validation_authority_revision,
        40,
        "validation authority revision",
    )?;
    validate_repo_reference(&candidate.build_metadata_ref, "build metadata reference")?;
    validate_repo_reference(
        &candidate.package_source_manifest_ref,
        "package/source manifest reference",
    )?;
    validate_timestamp(&candidate.created_at, "candidate created_at")
}

fn validate_result_against_plan(
    result: &ValidationResult,
    candidate: &ValidationCandidate,
    plan: &super::ValidationPlan,
) -> BuildResult<()> {
    if result.schema_version != W6_SCHEMA_VERSION
        || result.candidate_id != candidate.candidate_id
        || result.iso_sha256 != candidate.iso_sha256
    {
        return invalid("validation result candidate/schema/hash mismatch");
    }
    let expected = plan
        .tests
        .iter()
        .find(|entry| entry.test_id == result.test_id)
        .ok_or_else(|| {
            BuildError::Invalid(format!("unknown validation test id {}", result.test_id))
        })?;
    if result.execution_class != expected.execution_class
        || result.environment != expected.environment
    {
        return invalid(format!(
            "{} class/environment does not match validation matrix",
            result.test_id
        ));
    }
    validate_timestamp(&result.started_at, "validation result start")?;
    validate_timestamp(&result.ended_at, "validation result end")?;
    if result.commands_ref != "commands.jsonl" {
        return invalid("commands_ref must be commands.jsonl");
    }
    validate_notes(&result.notes)?;
    let mut assertion_ids = BTreeSet::new();
    let evidence_set: BTreeSet<_> = result.evidence_refs.iter().map(String::as_str).collect();
    if evidence_set.len() != result.evidence_refs.len() {
        return invalid("validation result contains duplicate evidence references");
    }
    for reference in &result.evidence_refs {
        validate_repo_reference(reference, "validation evidence reference")?;
        reject_reserved_evidence_ref(reference)?;
    }
    for assertion in &result.assertions {
        validate_safe_text(&assertion.id, "assertion id")?;
        if !assertion_ids.insert(assertion.id.as_str()) {
            return invalid("validation result contains duplicate assertion ids");
        }
        if assertion.evidence_refs.is_empty() {
            return invalid("every assertion requires at least one evidence reference");
        }
        for reference in &assertion.evidence_refs {
            validate_repo_reference(reference, "assertion evidence reference")?;
            if !evidence_set.contains(reference.as_str()) {
                return invalid("assertion evidence must also appear in top-level evidence_refs");
            }
        }
    }
    match result.status {
        ValidationStatus::Pass => {
            if result.assertions.is_empty()
                || result.evidence_refs.is_empty()
                || result
                    .assertions
                    .iter()
                    .any(|assertion| assertion.status != AssertionStatus::Pass)
                || !result.redaction_applied
                || result.blocking_reason.is_some()
            {
                return invalid(
                    "pass requires passing assertions, evidence, applied redaction and no blocking reason",
                );
            }
        }
        ValidationStatus::Fail => {
            if !result.assertions.is_empty()
                && result
                    .assertions
                    .iter()
                    .all(|assertion| assertion.status == AssertionStatus::Pass)
                && result.notes.is_empty()
            {
                return invalid("fail result must contain a failed assertion or explanatory note");
            }
            if result.blocking_reason.is_some() {
                return invalid("fail result must not use blocking_reason");
            }
        }
        ValidationStatus::Blocked | ValidationStatus::NotRun => {
            let reason = result.blocking_reason.as_deref().ok_or_else(|| {
                BuildError::Invalid("blocked/not_run requires blocking_reason".to_string())
            })?;
            validate_safe_text(reason, "blocking reason")?;
        }
    }
    Ok(())
}

fn validate_result_evidence(test_root: &Path, result: &ValidationResult) -> BuildResult<()> {
    resolve_existing_regular(test_root, &result.commands_ref)?;
    for reference in &result.evidence_refs {
        resolve_existing_regular(test_root, reference)?;
    }
    Ok(())
}

fn validate_command_record(record: &ValidationCommandRecord) -> BuildResult<()> {
    if record.sequence == 0 {
        return invalid("command sequence starts at 1");
    }
    if !matches!(
        record.actor.as_str(),
        "root" | "master" | "harness" | "operator"
    ) {
        return invalid("command actor must be root|master|harness|operator");
    }
    if record.command.is_some() == record.operation.is_some() {
        return invalid("command record requires exactly one of command or operation");
    }
    if let Some(command) = &record.command {
        validate_safe_text(command, "safe command representation")?;
        reject_secret_like_text(command, "safe command representation")?;
        if record.exit_code.is_none() || record.secret_recorded.is_some() {
            return invalid("command records require exit_code and may not use secret_recorded");
        }
    }
    if let Some(operation) = &record.operation {
        validate_safe_text(operation, "safe operation marker")?;
        if record.secret_recorded != Some(false) {
            return invalid("operation marker must explicitly record secret_recorded=false");
        }
    }
    for reference in [&record.stdout_ref, &record.stderr_ref]
        .into_iter()
        .flatten()
    {
        validate_repo_reference(reference, "command output reference")?;
        reject_reserved_evidence_ref(reference)?;
    }
    Ok(())
}

fn build_evidence_manifest(
    test_root: &Path,
    result: &ValidationResult,
) -> BuildResult<EvidenceManifest> {
    let mut refs: BTreeSet<String> = result.evidence_refs.iter().cloned().collect();
    refs.insert(result.commands_ref.clone());
    let commands_path = resolve_existing_regular(test_root, &result.commands_ref)?;
    let commands = fs::read_to_string(commands_path)?;
    for line in commands.lines().filter(|line| !line.trim().is_empty()) {
        let record: ValidationCommandRecord = serde_json::from_str(line).map_err(|error| {
            BuildError::Invalid(format!(
                "invalid command record while hashing evidence: {error}"
            ))
        })?;
        validate_command_record(&record)?;
        refs.extend(record.stdout_ref);
        refs.extend(record.stderr_ref);
    }
    let mut files = Vec::with_capacity(refs.len());
    for reference in refs {
        let path = resolve_existing_regular(test_root, &reference)?;
        let bytes = fs::read(&path)?;
        files.push(EvidenceFile {
            path: reference,
            sha256: hex_sha256(&bytes),
            size_bytes: u64::try_from(bytes.len())
                .map_err(|_| BuildError::Invalid("evidence file size overflow".to_string()))?,
        });
    }
    Ok(EvidenceManifest {
        schema_version: W6_SCHEMA_VERSION,
        candidate_id: result.candidate_id.clone(),
        iso_sha256: result.iso_sha256.clone(),
        test_id: result.test_id.clone(),
        files,
    })
}

fn verify_evidence_manifest(test_root: &Path, result: &ValidationResult) -> BuildResult<()> {
    let manifest: EvidenceManifest =
        read_json(&test_root.join(EVIDENCE_MANIFEST), "evidence manifest")?;
    validate_manifest_identity(&manifest, result)?;
    verify_manifest_files(test_root, &manifest, false)?;
    let expected = build_evidence_manifest(test_root, result)?;
    let expected_refs: BTreeSet<_> = expected
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let manifest_refs: BTreeSet<_> = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    if manifest_refs != expected_refs {
        return invalid(format!(
            "evidence manifest references drifted for {}",
            result.test_id
        ));
    }
    Ok(())
}

fn verify_evidence_manifest_for_archive(
    test_root: &Path,
    result: &ValidationResult,
) -> BuildResult<()> {
    let manifest: EvidenceManifest =
        read_json(&test_root.join(EVIDENCE_MANIFEST), "evidence manifest")?;
    validate_manifest_identity(&manifest, result)?;
    verify_manifest_files(test_root, &manifest, true)
}

fn validate_manifest_identity(
    manifest: &EvidenceManifest,
    result: &ValidationResult,
) -> BuildResult<()> {
    if manifest.schema_version != W6_SCHEMA_VERSION
        || manifest.candidate_id != result.candidate_id
        || manifest.iso_sha256 != result.iso_sha256
        || manifest.test_id != result.test_id
    {
        return invalid("evidence manifest identity mismatch");
    }
    Ok(())
}

fn verify_manifest_files(
    test_root: &Path,
    manifest: &EvidenceManifest,
    allow_commands_append: bool,
) -> BuildResult<()> {
    for file in &manifest.files {
        let path = resolve_existing_regular(test_root, &file.path)?;
        let bytes = fs::read(path)?;
        let recorded_size = usize::try_from(file.size_bytes).map_err(|_| {
            BuildError::Invalid("evidence size does not fit host usize".to_string())
        })?;
        let command_prefix = allow_commands_append && file.path == "commands.jsonl";
        if command_prefix {
            if bytes.len() < recorded_size || file.sha256 != hex_sha256(&bytes[..recorded_size]) {
                return invalid(format!(
                    "recorded command prefix mutated for {}",
                    manifest.test_id
                ));
            }
        } else if bytes.len() != recorded_size || file.sha256 != hex_sha256(&bytes) {
            return invalid(format!("evidence mutated for {}", manifest.test_id));
        }
    }
    Ok(())
}

fn archive_prior_attempt(test_root: &Path, prior: &ValidationResult) -> BuildResult<()> {
    verify_evidence_manifest_for_archive(test_root, prior)?;
    let attempts = test_root.join("attempts");
    fs::create_dir_all(&attempts)?;
    let next = next_attempt_number(&attempts)?;
    fs::copy(
        test_root.join("result.json"),
        attempts.join(format!("attempt-{next:03}.json")),
    )?;
    fs::copy(
        test_root.join(EVIDENCE_MANIFEST),
        attempts.join(format!("attempt-{next:03}-evidence-manifest.json")),
    )?;
    Ok(())
}

fn verify_archived_attempts(
    test_root: &Path,
    candidate: &ValidationCandidate,
    plan: &super::ValidationPlan,
) -> BuildResult<()> {
    let attempts = test_root.join("attempts");
    if !attempts.exists() {
        return Ok(());
    }
    reject_symlink_or_non_directory(&attempts, "attempts directory")?;
    let mut result_files: Vec<PathBuf> = fs::read_dir(&attempts)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("attempt-")
                        && name.ends_with(".json")
                        && !name.contains("evidence-manifest")
                })
        })
        .collect();
    result_files.sort();
    for result_path in result_files {
        let prior: ValidationResult = read_json(&result_path, "archived validation result")?;
        validate_result_against_plan(&prior, candidate, plan)?;
        let stem = result_path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| BuildError::Invalid("invalid archived attempt filename".to_string()))?;
        let manifest_path = attempts.join(format!("{stem}-evidence-manifest.json"));
        let manifest: EvidenceManifest = read_json(&manifest_path, "archived evidence manifest")?;
        validate_manifest_identity(&manifest, &prior)?;
        verify_manifest_files(test_root, &manifest, true)?;
    }
    Ok(())
}

fn next_attempt_number(attempts: &Path) -> BuildResult<u32> {
    let mut max_seen = 0_u32;
    for entry in fs::read_dir(attempts)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(number) = name
            .strip_prefix("attempt-")
            .and_then(|rest| rest.strip_suffix(".json"))
            .and_then(|value| value.parse::<u32>().ok())
        {
            max_seen = max_seen.max(number);
        }
    }
    max_seen
        .checked_add(1)
        .filter(|value| *value <= 999)
        .ok_or_else(|| {
            BuildError::Invalid("validation attempt history exceeds 999 entries".to_string())
        })
}

fn validate_import_refs(candidate_root: &Path, imported: &ImportedEvidence) -> BuildResult<()> {
    for (prefix, refs) in [
        ("imported/host-safe/", &imported.host_safe),
        ("imported/update/", &imported.update),
        ("imported/protected-api/", &imported.protected_api),
    ] {
        let unique: BTreeSet<_> = refs.iter().collect();
        if unique.len() != refs.len() {
            return invalid(format!("duplicate imported evidence reference in {prefix}"));
        }
        for reference in refs {
            validate_repo_reference(reference, "imported evidence reference")?;
            if !reference.starts_with(prefix) {
                return invalid(format!(
                    "imported evidence {reference} must remain under {prefix}"
                ));
            }
            resolve_existing_regular(candidate_root, reference)?;
        }
    }
    Ok(())
}

fn imported_groups_pass(candidate_root: &Path, imported: &ImportedEvidence) -> BuildResult<bool> {
    if imported.host_safe.is_empty()
        || imported.update.is_empty()
        || imported.protected_api.is_empty()
    {
        return Ok(false);
    }
    for reference in imported
        .host_safe
        .iter()
        .chain(&imported.update)
        .chain(&imported.protected_api)
    {
        let path = resolve_existing_regular(candidate_root, reference)?;
        let value: serde_json::Value = read_json(&path, "imported evidence")?;
        if value.get("status").and_then(serde_json::Value::as_str) != Some("pass") {
            return Ok(false);
        }
    }
    Ok(true)
}

fn verify_report_file(
    repo_root: &Path,
    candidate_root: &Path,
    candidate: &ValidationCandidate,
    counts: &ValidationCounts,
) -> BuildResult<()> {
    let report: ValidationReport =
        read_json(&candidate_root.join(REPORT_JSON), "validation report")?;
    if report.schema_version != W6_SCHEMA_VERSION
        || report.candidate_id != candidate.candidate_id
        || report.iso_filename != candidate.iso_filename
        || report.iso_sha256 != candidate.iso_sha256
        || report.source_revision != candidate.source_revision
        || report.validation_authority_revision != candidate.validation_authority_revision
        || report.reference_environment_ref != "environment/reference.json"
        || report.minimum_environment_ref != "environment/minimum.json"
        || report.redactions_ref != REDACTIONS_FILE
    {
        return invalid("validation report candidate/authority identity mismatch");
    }
    if report.counts != *counts || report.tests.len() != 38 {
        return invalid("validation report counts/test cardinality are stale");
    }
    for (index, test) in report.tests.iter().enumerate() {
        let expected_id = format!("ISO-{:02}", index + 1);
        if test.test_id != expected_id
            || test.result_ref != format!("tests/{expected_id}/result.json")
        {
            return invalid("validation report test ordering/reference drifted");
        }
        let current: ValidationResult = read_json(
            &candidate_root.join(&test.result_ref),
            "validation report referenced result",
        )?;
        if current.status != test.status {
            return invalid(format!(
                "validation report status is stale for {expected_id}"
            ));
        }
    }
    validate_import_refs(candidate_root, &report.imported_evidence)?;
    validate_repo_reference(&report.known_limitations_ref, "known limitations reference")?;
    let ledger: RedactionLedger =
        read_json(&candidate_root.join(REDACTIONS_FILE), "redaction ledger")?;
    let environments_ready =
        is_nonempty_regular(&candidate_root.join("environment/reference.json"))?
            && is_nonempty_regular(&candidate_root.join("environment/minimum.json"))?;
    let known_limitations_ready =
        resolve_existing_regular(repo_root, &report.known_limitations_ref).is_ok();
    let imports_ready = imported_groups_pass(candidate_root, &report.imported_evidence)?;
    let all_pass =
        counts.pass == 38 && counts.fail == 0 && counts.blocked == 0 && counts.not_run == 0;
    let rejected = counts.fail > 0 || ledger.forbidden_material_detected;
    let expected_status = if all_pass
        && !rejected
        && environments_ready
        && known_limitations_ready
        && imports_ready
    {
        "accepted"
    } else if rejected {
        "rejected"
    } else {
        "incomplete"
    };
    if report.status != expected_status {
        return invalid("validation report adjudication is stale or forged");
    }
    if (report.status == "accepted") != report.accepted_at.is_some() {
        return invalid("validation report accepted_at does not match adjudication");
    }
    Ok(())
}

fn render_report_markdown(report: &ValidationReport) -> String {
    let mut output = String::new();
    output.push_str("# PortusOS Validation Report\n\n");
    output.push_str(&format!("- Candidate: `{}`\n", report.candidate_id));
    output.push_str(&format!("- ISO: `{}`\n", report.iso_filename));
    output.push_str(&format!("- SHA-256: `{}`\n", report.iso_sha256));
    output.push_str(&format!("- Status: **{}**\n", report.status));
    output.push_str(&format!(
        "- Counts: pass={} fail={} blocked={} not_run={}\n\n",
        report.counts.pass, report.counts.fail, report.counts.blocked, report.counts.not_run
    ));
    output.push_str("| Test | Status | Result |\n| --- | --- | --- |\n");
    for test in &report.tests {
        output.push_str(&format!(
            "| {} | {} | `{}` |\n",
            test.test_id,
            test.status.as_str(),
            test.result_ref
        ));
    }
    output
}

fn increment_count(counts: &mut ValidationCounts, status: ValidationStatus) {
    match status {
        ValidationStatus::Pass => counts.pass += 1,
        ValidationStatus::Fail => counts.fail += 1,
        ValidationStatus::Blocked => counts.blocked += 1,
        ValidationStatus::NotRun => counts.not_run += 1,
    }
}

fn read_candidate(candidate_root: &Path) -> BuildResult<ValidationCandidate> {
    reject_symlink_or_non_directory(candidate_root, "candidate root")?;
    let candidate: ValidationCandidate =
        read_json(&candidate_root.join(CANDIDATE_FILE), "candidate")?;
    validate_candidate(&candidate)?;
    if candidate_root.file_name().and_then(|value| value.to_str()) != Some(&candidate.candidate_id)
    {
        return invalid("candidate root basename must equal candidate_id");
    }
    Ok(candidate)
}

fn test_root(candidate_root: &Path, test_id: &str) -> BuildResult<PathBuf> {
    validate_test_id(test_id)?;
    let root = candidate_root.join("tests").join(test_id);
    reject_symlink_or_non_directory(&root, "validation test directory")?;
    Ok(root)
}

fn resolve_existing_regular(base: &Path, reference: &str) -> BuildResult<PathBuf> {
    validate_repo_reference(reference, "evidence reference")?;
    let mut current = base.to_path_buf();
    let components: Vec<_> = Path::new(reference).components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(value) = component else {
            return invalid("evidence reference contains unsafe path component");
        };
        current.push(value);
        let metadata = fs::symlink_metadata(&current).map_err(BuildError::Io)?;
        if metadata.file_type().is_symlink() {
            return invalid(format!(
                "evidence path contains symlink: {}",
                current.display()
            ));
        }
        let is_last = index + 1 == components.len();
        if is_last {
            if !metadata.is_file() {
                return invalid(format!(
                    "evidence path is not a regular file: {}",
                    current.display()
                ));
            }
        } else if !metadata.is_dir() {
            return invalid(format!(
                "evidence parent is not a directory: {}",
                current.display()
            ));
        }
    }
    Ok(current)
}

fn ensure_directory_root(path: &Path) -> BuildResult<()> {
    if path.exists() {
        reject_symlink_or_non_directory(path, "validation output root")?;
    } else {
        fs::create_dir_all(path)?;
        reject_symlink_or_non_directory(path, "validation output root")?;
    }
    Ok(())
}

fn reject_symlink_or_non_directory(path: &Path, label: &str) -> BuildResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return invalid(format!(
            "{label} must be a real directory: {}",
            path.display()
        ));
    }
    Ok(())
}

fn is_nonempty_regular(path: &Path) -> BuildResult<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(path)?;
    Ok(metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() > 0)
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> BuildResult<()> {
    let bytes = json_bytes(value)?;
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn write_json_replace<T: Serialize>(path: &Path, value: &T) -> BuildResult<()> {
    fs::write(path, json_bytes(value)?)?;
    Ok(())
}

fn json_bytes<T: Serialize>(value: &T) -> BuildResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        BuildError::Invalid(format!("cannot serialize validation JSON: {error}"))
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> BuildResult<T> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return invalid(format!("{label} must be a regular non-symlink file"));
    }
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| BuildError::Invalid(format!("invalid {label} JSON: {error}")))
}

fn validate_component(value: &str, label: &str) -> BuildResult<()> {
    validate_safe_text(value, label)?;
    let mut components = Path::new(value).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return invalid(format!("{label} must be one safe path component"));
    }
    Ok(())
}

fn validate_repo_reference(value: &str, label: &str) -> BuildResult<()> {
    validate_safe_text(value, label)?;
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return invalid(format!(
            "{label} must be a clean relative path without traversal"
        ));
    }
    Ok(())
}

fn reject_reserved_evidence_ref(value: &str) -> BuildResult<()> {
    if matches!(
        value,
        "result.json"
            | EVIDENCE_MANIFEST
            | CANDIDATE_FILE
            | REPORT_JSON
            | REPORT_MD
            | REDACTIONS_FILE
    ) {
        return invalid("evidence reference targets harness-owned metadata");
    }
    Ok(())
}

fn validate_test_id(value: &str) -> BuildResult<()> {
    if value.len() != 6 || !value.starts_with("ISO-") {
        return invalid("validation test id must be ISO-01 through ISO-38");
    }
    let number = value[4..]
        .parse::<u8>()
        .map_err(|_| BuildError::Invalid("invalid validation test id".to_string()))?;
    if !(1..=38).contains(&number) {
        return invalid("validation test id must be ISO-01 through ISO-38");
    }
    Ok(())
}

fn validate_timestamp(value: &str, label: &str) -> BuildResult<()> {
    validate_safe_text(value, label)?;
    if value.len() < 20 || !value.contains('T') || !value.ends_with('Z') {
        return invalid(format!(
            "{label} must be a UTC RFC3339-like timestamp ending in Z"
        ));
    }
    Ok(())
}

fn validate_safe_text(value: &str, label: &str) -> BuildResult<()> {
    if value.is_empty()
        || value.len() > MAX_SAFE_TEXT_BYTES
        || value.chars().any(|character| character.is_control())
    {
        return invalid(format!(
            "{label} must be non-empty, bounded, printable text"
        ));
    }
    Ok(())
}

fn validate_notes(notes: &[String]) -> BuildResult<()> {
    if notes.len() > 128 {
        return invalid("validation notes exceed 128-entry bound");
    }
    for note in notes {
        validate_safe_text(note, "validation note")?;
        reject_secret_like_text(note, "validation note")?;
    }
    Ok(())
}

fn reject_secret_like_text(value: &str, label: &str) -> BuildResult<()> {
    let lower = value.to_ascii_lowercase();
    for marker in [
        "authorization:",
        "proxy-authorization:",
        "api_key=",
        "api-key=",
        "apikey=",
        "password=",
        "token=",
        "bearer ",
        "sk-proj-",
    ] {
        if lower.contains(marker) {
            return invalid(format!("{label} contains secret-like material marker"));
        }
    }
    Ok(())
}

fn validate_lower_hex_local(value: &str, length: usize, label: &str) -> BuildResult<()> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid(format!("{label} must be {length} lowercase hex characters"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> BuildResult<T> {
    Err(BuildError::Invalid(message.into()))
}

#[cfg(test)]
mod validation_harness_tests {
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
            "portus-validation-{label}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn candidate() -> ValidationCandidate {
        ValidationCandidate {
            schema_version: 1,
            candidate_id: "first-iso-rc.1".to_string(),
            iso_filename: "PortusOS-first-iso-rc.1-x86_64.iso".to_string(),
            iso_sha256: "a".repeat(64),
            source_revision: "b".repeat(40),
            build_metadata_ref: "build-metadata.json".to_string(),
            package_source_manifest_ref: "package-source-manifest.json".to_string(),
            validation_authority_revision: "c".repeat(40),
            created_at: "2026-08-27T00:00:00Z".to_string(),
        }
    }

    fn materialized(label: &str) -> (PathBuf, PathBuf) {
        let root = temp_dir(label);
        let summary = materialize_validation_harness(&repo_root(), &root, &candidate()).unwrap();
        (root, PathBuf::from(summary.candidate_root))
    }

    #[test]
    fn validation_harness_materializes_exact_candidate_scoped_tree() {
        let (root, candidate_root) = materialized("tree");
        assert!(candidate_root.join("candidate.json").is_file());
        assert!(candidate_root.join("redactions.json").is_file());
        for number in 1..=38 {
            let id = format!("ISO-{number:02}");
            let result: ValidationResult = read_json(
                &candidate_root.join("tests").join(&id).join("result.json"),
                "fixture result",
            )
            .unwrap();
            assert_eq!(result.status, ValidationStatus::NotRun);
            assert_eq!(result.test_id, id);
        }
        assert_eq!(
            verify_validation_harness(&repo_root(), &candidate_root)
                .unwrap()
                .not_run,
            38
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validation_harness_rejects_candidate_substitution_and_unsafe_paths() {
        let (root, candidate_root) = materialized("identity");
        let result_path = candidate_root.join("tests/ISO-01/result.json");
        let mut result: ValidationResult = read_json(&result_path, "fixture result").unwrap();
        result.iso_sha256 = "d".repeat(64);
        assert!(record_validation_result(&repo_root(), &candidate_root, &result).is_err());
        result.iso_sha256 = candidate().iso_sha256;
        result.evidence_refs = vec!["../escape.log".to_string()];
        assert!(record_validation_result(&repo_root(), &candidate_root, &result).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pass_requires_objective_evidence_and_detects_mutation() {
        let (root, candidate_root) = materialized("evidence");
        let test_root = candidate_root.join("tests/ISO-01");
        fs::write(test_root.join("artifacts/proof.txt"), b"proof").unwrap();
        let mut result: ValidationResult =
            read_json(&test_root.join("result.json"), "fixture result").unwrap();
        result.status = ValidationStatus::Pass;
        result.blocking_reason = None;
        assert!(record_validation_result(&repo_root(), &candidate_root, &result).is_err());
        result.started_at = "2026-08-27T00:01:00Z".to_string();
        result.ended_at = "2026-08-27T00:02:00Z".to_string();
        result.assertions = vec![ValidationAssertion {
            id: "build-produced-candidate".to_string(),
            status: AssertionStatus::Pass,
            evidence_refs: vec!["artifacts/proof.txt".to_string()],
        }];
        result.evidence_refs = vec!["artifacts/proof.txt".to_string()];
        result.redaction_applied = true;
        result.blocking_reason = None;
        record_validation_result(&repo_root(), &candidate_root, &result).unwrap();
        assert!(verify_validation_harness(&repo_root(), &candidate_root).is_ok());
        fs::write(test_root.join("artifacts/proof.txt"), b"mutated").unwrap();
        assert!(verify_validation_harness(&repo_root(), &candidate_root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retries_archive_failed_attempts_instead_of_rewriting_history() {
        let (root, candidate_root) = materialized("attempts");
        let test_root = candidate_root.join("tests/ISO-06");
        fs::write(test_root.join("artifacts/fail.txt"), b"failure evidence").unwrap();
        let mut failed: ValidationResult =
            read_json(&test_root.join("result.json"), "fixture result").unwrap();
        failed.status = ValidationStatus::Fail;
        failed.started_at = "2026-08-27T00:01:00Z".to_string();
        failed.ended_at = "2026-08-27T00:02:00Z".to_string();
        failed.assertions = vec![ValidationAssertion {
            id: "artix-identity".to_string(),
            status: AssertionStatus::Fail,
            evidence_refs: vec!["artifacts/fail.txt".to_string()],
        }];
        failed.evidence_refs = vec!["artifacts/fail.txt".to_string()];
        failed.redaction_applied = true;
        failed.blocking_reason = None;
        record_validation_result(&repo_root(), &candidate_root, &failed).unwrap();
        let retry_command = ValidationCommandRecord {
            sequence: 1,
            actor: "harness".to_string(),
            command: Some("retry fixture".to_string()),
            operation: None,
            exit_code: Some(0),
            stdout_ref: None,
            stderr_ref: None,
            secret_recorded: None,
        };
        append_validation_command(&repo_root(), &candidate_root, "ISO-06", &retry_command).unwrap();
        fs::write(test_root.join("artifacts/pass.txt"), b"pass evidence").unwrap();
        let mut passed = failed.clone();
        passed.status = ValidationStatus::Pass;
        passed.started_at = "2026-08-27T00:03:00Z".to_string();
        passed.ended_at = "2026-08-27T00:04:00Z".to_string();
        passed.assertions[0].status = AssertionStatus::Pass;
        passed.assertions[0].evidence_refs = vec!["artifacts/pass.txt".to_string()];
        passed.evidence_refs = vec!["artifacts/pass.txt".to_string()];
        record_validation_result(&repo_root(), &candidate_root, &passed).unwrap();
        assert!(test_root.join("attempts/attempt-001.json").is_file());
        assert!(
            test_root
                .join("attempts/attempt-001-evidence-manifest.json")
                .is_file()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn command_log_is_contiguous_bounded_and_secret_safe() {
        let (root, candidate_root) = materialized("commands");
        let test_root = candidate_root.join("tests/ISO-06");
        fs::write(test_root.join("stdout-001.log"), b"x86_64\n").unwrap();
        let record = ValidationCommandRecord {
            sequence: 1,
            actor: "harness".to_string(),
            command: Some("uname -m".to_string()),
            operation: None,
            exit_code: Some(0),
            stdout_ref: Some("stdout-001.log".to_string()),
            stderr_ref: None,
            secret_recorded: None,
        };
        append_validation_command(&repo_root(), &candidate_root, "ISO-06", &record).unwrap();
        assert!(
            append_validation_command(&repo_root(), &candidate_root, "ISO-06", &record).is_err()
        );
        let secret = ValidationCommandRecord {
            sequence: 2,
            actor: "operator".to_string(),
            command: Some("curl -H Authorization:Bearer secret".to_string()),
            operation: None,
            exit_code: Some(0),
            stdout_ref: None,
            stderr_ref: None,
            secret_recorded: None,
        };
        assert!(
            append_validation_command(&repo_root(), &candidate_root, "ISO-06", &secret).is_err()
        );
        let marker = ValidationCommandRecord {
            sequence: 2,
            actor: "operator".to_string(),
            command: None,
            operation: Some("protected_credential_entered_via_non_echo_tty".to_string()),
            exit_code: None,
            stdout_ref: None,
            stderr_ref: None,
            secret_recorded: Some(false),
        };

        let mut result: ValidationResult =
            read_json(&test_root.join("result.json"), "fixture result").unwrap();
        result.status = ValidationStatus::Fail;
        result.started_at = "2026-08-27T00:01:00Z".to_string();
        result.ended_at = "2026-08-27T00:02:00Z".to_string();
        result.notes = vec!["fixture failure after recorded command".to_string()];
        result.redaction_applied = true;
        result.blocking_reason = None;
        record_validation_result(&repo_root(), &candidate_root, &result).unwrap();
        fs::write(test_root.join("stdout-001.log"), b"mutated\n").unwrap();
        assert!(verify_validation_harness(&repo_root(), &candidate_root).is_err());
        append_validation_command(&repo_root(), &candidate_root, "ISO-06", &marker).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_never_accepts_not_run_blocked_or_missing_imports() {
        let (root, candidate_root) = materialized("report");
        let input = ValidationReportInput {
            started_at: "2026-08-27T00:00:00Z".to_string(),
            ended_at: "2026-08-27T00:10:00Z".to_string(),
            imported_evidence: ImportedEvidence {
                host_safe: Vec::new(),
                update: Vec::new(),
                protected_api: Vec::new(),
            },
            known_limitations_ref: "KNOWN_LIMITATIONS.md".to_string(),
        };
        let report = aggregate_validation_report(&repo_root(), &candidate_root, &input).unwrap();
        assert_eq!(report.status, "incomplete");
        assert_eq!(report.counts.not_run, 38);
        assert!(report.accepted_at.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn complete_valid_evidence_can_reach_accepted_report() {
        let (root, candidate_root) = materialized("accepted");
        fs::write(
            candidate_root.join("environment/reference.json"),
            b"{\"profile\":\"reference\"}\n",
        )
        .unwrap();
        fs::write(
            candidate_root.join("environment/minimum.json"),
            b"{\"profile\":\"minimum\"}\n",
        )
        .unwrap();
        for (group, name) in [
            ("host-safe", "host.json"),
            ("update", "update.json"),
            ("protected-api", "protected.json"),
        ] {
            fs::write(
                candidate_root.join("imported").join(group).join(name),
                b"{\"status\":\"pass\"}\n",
            )
            .unwrap();
        }
        for number in 1..=38 {
            let id = format!("ISO-{number:02}");
            let root = candidate_root.join("tests").join(&id);
            let evidence = format!("artifacts/{id}.txt");
            fs::write(root.join(&evidence), format!("evidence for {id}\n")).unwrap();
            let mut result: ValidationResult =
                read_json(&root.join("result.json"), "fixture result").unwrap();
            result.status = ValidationStatus::Pass;
            result.started_at = "2026-08-27T00:01:00Z".to_string();
            result.ended_at = "2026-08-27T00:02:00Z".to_string();
            result.assertions = vec![ValidationAssertion {
                id: format!("{id}-objective"),
                status: AssertionStatus::Pass,
                evidence_refs: vec![evidence.clone()],
            }];
            result.evidence_refs = vec![evidence];
            result.redaction_applied = true;
            result.blocking_reason = None;
            record_validation_result(&repo_root(), &candidate_root, &result).unwrap();
        }
        let input = ValidationReportInput {
            started_at: "2026-08-27T00:00:00Z".to_string(),
            ended_at: "2026-08-27T00:10:00Z".to_string(),
            imported_evidence: ImportedEvidence {
                host_safe: vec!["imported/host-safe/host.json".to_string()],
                update: vec!["imported/update/update.json".to_string()],
                protected_api: vec!["imported/protected-api/protected.json".to_string()],
            },
            known_limitations_ref: "README.md".to_string(),
        };
        let report = aggregate_validation_report(&repo_root(), &candidate_root, &input).unwrap();
        assert_eq!(report.status, "accepted");
        assert_eq!(report.counts.pass, 38);
        assert_eq!(report.accepted_at.as_deref(), Some("2026-08-27T00:10:00Z"));
        assert!(verify_validation_harness(&repo_root(), &candidate_root).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn redaction_ledger_forbidden_material_forces_rejection() {
        let (root, candidate_root) = materialized("redaction");
        let ledger = RedactionLedger {
            schema_version: 1,
            candidate_id: candidate().candidate_id,
            files_scanned: 1,
            files_redacted: Vec::new(),
            forbidden_material_detected: true,
            notes: vec!["fixture detected forbidden reusable material".to_string()],
        };
        record_redaction_ledger(&candidate_root, &ledger).unwrap();
        let input = ValidationReportInput {
            started_at: "2026-08-27T00:00:00Z".to_string(),
            ended_at: "2026-08-27T00:10:00Z".to_string(),
            imported_evidence: ImportedEvidence {
                host_safe: Vec::new(),
                update: Vec::new(),
                protected_api: Vec::new(),
            },
            known_limitations_ref: "KNOWN_LIMITATIONS.md".to_string(),
        };
        assert_eq!(
            aggregate_validation_report(&repo_root(), &candidate_root, &input)
                .unwrap()
                .status,
            "rejected"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
