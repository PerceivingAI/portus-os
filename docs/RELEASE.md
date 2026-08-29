# PortusOS Release Authority

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative technical release contract
**Scope:** Accepted development first ISO and first public OSS release
**Release metadata schema version:** 1
**Validation authority:** `docs/VALIDATION.md`
**Public release policy:** `docs/PUBLIC_RELEASE_POLICY.md`

## 1. Purpose and authority

This document defines how a PortusOS build becomes an identifiable, immutable, verifiable release artifact.

It governs:

- development first-ISO candidates;
- the accepted first-ISO development milestone;
- public release candidates;
- the first public OSS release;
- artifact naming;
- source/tag relationships;
- checksum and verification material;
- build metadata;
- package/component pin linkage;
- SBOM/provenance outputs;
- validation linkage;
- known limitations and release notes;
- update-support declarations;
- candidate replacement/revocation rules.

The first public version/maturity, Apache-2.0 first-party licence, release-signing mechanism/key-custody model, support/update policy and public issue/security workflow are locked by `docs/PUBLIC_RELEASE_POLICY.md` / `oss/release-policy.json`. No owner/release-policy decision remains unresolved.

`docs/VALIDATION.md` determines whether a technical candidate passed the acceptance suite. This document determines **which exact candidate those results belong to and what may be published**.

---

## 2. Release classes

PortusOS uses four distinct artifact states.

### 2.1 Development candidate

A development candidate is an immutable ISO built to exercise the native Artix build path, VMware acceptance path, and ISO-01 through ISO-38 matrix.

It may fail acceptance tests and is not a public supported release.

Naming template:

```text
PortusOS-first-iso-rc.<N>-x86_64.iso
```

Candidate numbers start at `1` and increase monotonically. Reusing a candidate number for different bytes is forbidden.

### 2.2 Accepted first-ISO development milestone

When one development candidate passes ISO-01..ISO-38 under `docs/VALIDATION.md`, that **same candidate** becomes the accepted first-ISO development milestone.

Do not rebuild or silently replace it merely to produce an “accepted” file.

Its original RC filename/hash remain the canonical artifact identity. Acceptance is represented by the linked validation report and release metadata, not by modifying the ISO.

### 2.3 Public release candidate

After OSS legal/provenance/documentation/security gates are complete and a public semantic version is selected, public candidates use:

```text
PortusOS-<version>-rc.<N>-x86_64.iso
```

Git tag template:

```text
v<version>-rc.<N>
```

Every public RC is immutable. A release-critical change creates `rc.<N+1>`.

### 2.4 Public final release

The public final release uses:

```text
PortusOS-<version>-x86_64.iso
```

Git tag:

```text
v<version>
```

The final ISO payload must be byte-identical to the accepted public RC payload selected for promotion, or the complete public-release validation must be rerun against the rebuilt bytes.

Preferred practice is promotion of the already-validated RC bytes without rebuilding. The final publication may copy those exact bytes to the canonical final basename. Validation remains tied to the promoted RC candidate ID and ISO SHA-256; `release-metadata.json` records the final version/tag/filename and the `promoted_from_candidate_id` relationship. Renaming/copying identical bytes does not create new technical validation evidence.

---

## 3. Versioning and maturity

### 3.1 Semantic version structure

Public releases use a three-component semantic version:

```text
<major>.<minor>.<patch>
```

The first public value is locked to:

```text
0.1.0
```

with the public label:

```text
PortusOS 0.1.0 Technical Preview
```

The version communicates release identity; it does not by itself imply production maturity or broad hardware support.

### 3.2 First public maturity boundary

The selected wording must not imply:

- broad bare-metal qualification;
- Secure Boot support;
- production-grade backup/rollback;
- guaranteed unattended reboot recovery;
- hardware/support coverage not present in the validation report.

### 3.3 Repository / first-party licence

PortusOS-owned source is licensed under the Apache License 2.0 (`Apache-2.0`) with the canonical text in root `LICENSE`. All Portus Cargo workspace crates inherit `license = "Apache-2.0"` from the workspace package metadata.

Third-party components retain their own upstream licences and obligations. The PortusOS first-party licence does not replace package-specific licence, notice, source-offer or redistribution requirements captured by the R8/OSS audit.

---

## 4. Candidate identity

Every candidate has one immutable logical identifier.

### 4.1 Development candidate ID

```text
first-iso-rc.<N>-g<12-char-source-sha>
```

Example shape only:

```text
first-iso-rc.3-g0123456789ab
```

### 4.2 Public candidate ID

```text
<version>-rc.<N>-g<12-char-source-sha>
```

### 4.3 Final release ID

```text
<version>-g<12-char-source-sha>
```

The full 40-character source revision is always stored in machine-readable metadata even when the concise ID uses 12 characters.

Candidate identity is subordinate to artifact hash: if the ISO SHA-256 changes, it is a different candidate even if someone accidentally reused a label.

---

## 5. Required release bundle

### 5.1 Development first-ISO candidate

At minimum:

```text
PortusOS-first-iso-rc.<N>-x86_64.iso
SHA256SUMS
build-metadata.json
package-source-manifest.<machine-readable-format>
validation-report.json        # when validation has run
validation-report.md          # when validation has run
KNOWN_LIMITATIONS.md
```

A cryptographic release signature, public SBOM, public licence/notices and public-facing release notes are not required merely to run a private development first-ISO candidate, but source/package provenance must still be inspectable for ISO-01.

### 5.1.1 Candidate initialization

After ISO construction, use the repository-owned initializer rather than manually assembling candidate identity/evidence state:

```text
portus-build candidate-init --artifact <produced.iso> --input-json <candidate-init.json>
portus-build candidate-verify --candidate-root portusos-build/out/validation/<candidate-id>
```

The initializer derives `candidate_id` and the canonical frozen ISO basename from release class, RC number and the full source revision according to Section 4/6. Callers do not supply those two identities. It snapshots `package-source-manifest.yaml`, `codex-pin.yaml`, `portus-browser-pin.yaml`, `portus-mcp-pin.yaml`, `tunnel-client-pin.yaml`, and `packages.lock.yaml` when that verified Artix-generated lock exists; writes `build-metadata.json` and `candidate.json`; initializes ISO-01..ISO-38 as `not_run`; and creates an `incomplete` validation report.

The initialization `SHA256SUMS` covers immutable candidate inputs only. Validation evidence/report files change as tests run and are added to the public/final checksum set after they are finalized. Reusing an existing candidate root, changing the frozen ISO bytes, or substituting metadata/pin snapshots causes `candidate-verify` to fail.

### 5.2 Public RC/final bundle

Required public assets:

```text
PortusOS-<artifact-version>-x86_64.iso
SHA256SUMS
SHA256SUMS.sig
build-metadata.json
release-metadata.json
package-source-manifest.<machine-readable-format>
PortusOS-<artifact-version>.spdx.json
provenance.json
validation-report.json
validation-report.md
KNOWN_LIMITATIONS.md
RELEASE_NOTES.md
source tag/archive
LICENSE
third-party licence/notices required by the final redistribution audit
```

For an RC, `<artifact-version>` includes `-rc.<N>`.

The public release may include additional human-readable summaries, but none may replace these machine-verifiable core outputs.

---

## 6. ISO filename rules

Rules:

- basename contains only ASCII letters, digits, `.`, `_`, and `-`;
- architecture suffix is exactly `x86_64` for the first release family;
- no spaces;
- no mutable words such as `latest`;
- candidate number is part of every RC basename;
- public final filename contains the selected semantic version;
- renaming an artifact does not create a new accepted candidate, but public release metadata must identify the canonical published basename.

---

## 7. SHA-256 manifest

The checksum manifest is named:

```text
SHA256SUMS
```

Each line uses:

```text
<64 lowercase hex><two spaces><basename>
```

Entries are sorted by basename using a deterministic bytewise order.

At minimum the public `SHA256SUMS` covers all downloadable release assets whose integrity matters independently, including the ISO, machine-readable metadata, SBOM/provenance and validation report.

The build pipeline computes hashes after artifacts are finalized. It must not modify a hashed artifact afterward.

For validation, the ISO SHA-256 in `SHA256SUMS`, `build-metadata.json`, `candidate.json` and `validation-report.json` must agree exactly.

---

## 8. Public cryptographic verification

### 8.1 Requirement

The first public release must provide a public cryptographic mechanism that lets a downloader verify the authenticity of `SHA256SUMS` or an equivalent root verification object.

### 8.2 Selected mechanism and custody

Public releases authenticate `SHA256SUMS` using OpenSSH SSHSIG with a dedicated Ed25519 release key:

```text
signature       SHA256SUMS.sig
namespace       portusos-release
signer identity portusos-release
public key      release/keys/portusos-release.allowed_signers
```

The owner-controlled private key is encrypted at rest, remains external to the repository/build roots/ISO/ordinary development environment, and is used only for release checksum signing. It must not be transported through environment variables, CLI secret values, logs or release metadata.

The public key is published only after the real owner key ceremony. Historical verification keys remain available for historical releases. `scripts/release/sign_checksums.py` rejects a private key located inside the repository; `scripts/release/verify_release.py` verifies both the SSHSIG signature and every canonical checksum entry.

Development first-ISO candidates may remain checksum-only until promotion into the public release track.

---

## 9. Git source/tag relationship

### 9.1 Source revision

Every candidate records exactly one full Git source revision for this repository.

A public candidate must be built from a clean, reproducible source state. Uncommitted working-tree content is not a valid public source identity.

### 9.2 External component source revisions

Non-Artix components that are part of the supported candidate, including PortusBrowser and Codex where applicable, carry their own immutable source/version/hash references in package/source metadata.

PortusBrowser is frozen for the first-ISO line at clean immutable revision `c263c3997b4e6f2f7df5922e062a9e949e22f755`. Candidate metadata must preserve that exact revision (or a later explicitly reviewed replacement); the remaining PortusBrowser blockers are package/install/native-messaging/VM evidence, not source identity.

### 9.3 Tags

A public RC tag points to the exact repository source revision recorded in its metadata.

The final `v<version>` tag points to the exact source revision used by the published final candidate.

Moving/reusing a public release tag to represent different source is forbidden.

Development candidates may use annotated internal tags such as:

```text
first-iso/rc.<N>
```

if desired, but the candidate ID + full revision + artifact hash remain authoritative even if no development tag is created.

---

## 10. `build-metadata.json`

Schema version 1 contains at least:

```json
{
  "schema_version": 1,
  "release_class": "development_rc|public_rc",
  "candidate_id": "...",
  "version": "<public semver or null>",
  "rc_number": 1,
  "source_revision": "<40 hex>",
  "source_tree_clean": true,
  "build_started_at": "<UTC RFC3339>",
  "build_finished_at": "<UTC RFC3339>",
  "builder": {
    "architecture": "x86_64",
    "distribution": "Artix Linux",
    "distribution_snapshot": "<resolved evidence>",
    "artools_version": "<resolved>",
    "rust_toolchain": "<resolved>"
  },
  "artifact": {
    "filename": "...iso",
    "sha256": "<64 lowercase hex>",
    "size_bytes": 0
  },
  "package_source_manifest_ref": "...",
  "codex_pin_ref": "...",
  "portus_browser_pin_ref": "...",
  "portus_mcp_pin_ref": "...",
  "tunnel_client_pin_ref": "...",
  "validation_authority_revision": "<40 hex>",
  "release_authority_revision": "<40 hex>"
}
```

For development work where the working tree is intentionally dirty, `source_tree_clean=false` may describe a non-release build, but that build cannot become a public RC/final and cannot satisfy any first-ISO source-freeze requirement that demands an immutable clean source.

A candidate accepted under ISO-01 must have sufficient source metadata to reproduce the intended source set.

### 10.1 `release-metadata.json`

Public RC/final publication adds a release-layer manifest separate from immutable build metadata:

```json
{
  "schema_version": 1,
  "release_class": "public_rc|public_final",
  "release_id": "...",
  "version": "<semver>",
  "rc_number": 1,
  "source_tag": "v<version>-rc.<N>|v<version>",
  "validated_candidate_id": "...",
  "promoted_from_candidate_id": null,
  "published_iso_filename": "...iso",
  "iso_sha256": "<64 lowercase hex>",
  "build_metadata_ref": "build-metadata.json",
  "validation_report_ref": "validation-report.json",
  "release_policy_sha256": "<64 lowercase hex>",
  "verification_method": "openssh-sshsig-ed25519",
  "checksum_signature_ref": "SHA256SUMS.sig",
  "signing_key_fingerprint": "SHA256:<OpenSSH public-key fingerprint>",
  "created_at": "<UTC RFC3339>"
}
```

For a public RC, `validated_candidate_id` is that RC and `promoted_from_candidate_id` is null. For a final promotion without rebuild, `validated_candidate_id` and `promoted_from_candidate_id` both identify the accepted RC, while `published_iso_filename` carries the final basename and `iso_sha256` must remain identical. A rebuilt final is a new candidate and must be validated as such instead of using promotion metadata.

Public release metadata also binds the exact public release-policy hash and the active OpenSSH signing-key fingerprint. Changing the policy or signing key after RC freeze requires explicit release review and, where release-critical, a new candidate/public RC rather than silently changing verification material.

---

## 11. Package/source manifest linkage

The machine-readable package/source manifest is the authority for what the build selected.

The release metadata links it by path/hash.

It must distinguish at least:

```text
official Artix package
Portus-owned local package
approved external/non-Artix component
validation-only package/profile
hardware-selected package
```

For every non-Artix component it records an explicit approved source contract and immutable version/revision/hash as applicable.

A supported public release must not contain an undeclared AUR/AUR-helper dependency.

The package/source and package-lock schemas live at `portusos-build/schemas/package-source.schema.json` and `package-lock.schema.json`, with human-maintained intent at `portusos-build/packages/packages.yaml`. `build-metadata.schema.json` and `release-metadata.schema.json` likewise materialize Section 10 without inventing another release identity model. The Artix-resolved package lock is generated evidence from the verified Artix resolution/build path rather than a hand-authored source file.

---

## 12. Codex pin contract

The release records the selected Codex build/version/source according to `CODEX_UPDATES.md`.

At minimum the candidate metadata can resolve:

- selected Codex version;
- source/distribution mechanism;
- integrity/hash information available under that mechanism;
- compatibility validation result/reference;
- whether the candidate includes/uses that exact pin.

Codex is not allowed to drift through a startup auto-update between build and validation.

Changing the Codex pin after a candidate is built creates a new candidate.

---

## 13. PortusBrowser pin contract

The release records the PortusBrowser cooperating compatibility set:

- clean immutable source revision;
- software version;
- protocol version;
- CLI;
- Broker;
- native host;
- extension assets/version/ID as applicable;
- Chromium reference compatibility;
- provider manifest/integration contract version.

The clean source revision must replace P15's current pending freeze before an accepted packaged first ISO/public RC can claim a fixed PortusBrowser source.

Changing any release-critical PortusBrowser cooperating component after candidate freeze creates a new candidate.

---

## 14. SBOM

### 14.1 Public requirement

The first public RC/final produces a machine-readable SPDX JSON SBOM:

```text
PortusOS-<artifact-version>.spdx.json
```

Baseline format target:

```text
SPDX 2.3 JSON
```

The SBOM must distinguish, to the extent available from the build/package metadata:

- PortusOS-owned components;
- Artix packages;
- approved external components;
- bundled browser/provider assets;
- build/runtime dependencies relevant to redistribution.

If later tooling requires another additional format, it may be emitted in parallel; the SPDX JSON artifact remains the first public baseline unless this authority is explicitly revised.

### 14.2 Current implementation

The repository implements the SPDX/provenance preparation path in:

```text
scripts/oss/release_evidence.py
oss/third-party-components.json
oss/schemas/system-package-inventory.schema.json
oss/schemas/third-party-review.schema.json
oss/schemas/provenance.schema.json
```

Preparatory generation may emit `complete=false` so the format and linkage can be tested before an Artix/public RC exists. `--release-ready` is the publication gate and exits 78 while required candidate/public facts are unresolved. It must not manufacture package licences, redistribution approvals, source tags, accepted validation, repository licensing or security-host evidence.

Candidate-specific Artix package data comes from the L2 package lock plus the reviewed system-package inventory. Candidate-specific non-package redistribution approval comes from the third-party review. Both are required for release-ready public evidence.

### 14.2 Development milestone

A complete public SBOM is not required to accept the private development first ISO, but package/source provenance sufficient for ISO-01 remains required.

---

## 15. Provenance

Public releases include:

```text
provenance.json
```

Schema version 1 records at least:

```json
{
  "schema_version": 1,
  "candidate_id": "...",
  "source_revision": "...",
  "source_tag": "...",
  "build_metadata_sha256": "...",
  "package_source_manifest_sha256": "...",
  "package_lock_sha256": "...",
  "first_party_license": "Apache-2.0",
  "license_file_sha256": "...",
  "release_policy_sha256": "...",
  "sbom_sha256": "...",
  "validation_report_sha256": "...",
  "iso_sha256": "...",
  "builder_environment_ref": "...",
  "build_instructions_revision": "...",
  "created_at": "<UTC RFC3339>",
  "complete": true,
  "incomplete_reasons": []
}
```

This is a PortusOS release provenance manifest, not a claim of conformance to a broader provenance framework that has not been explicitly adopted/tested.

---

## 16. Validation linkage

Every accepted development/public candidate includes or links:

```text
validation-report.json
validation-report.md
```

The JSON report is authoritative.

For a development candidate or public RC, release acceptance requires the candidate ID, ISO filename, ISO SHA-256, source revision and validation-authority revision in the validation report to match `build-metadata.json` exactly.

For a final release promoted without rebuilding, the validation report remains tied to the accepted RC candidate. `release-metadata.json` must link the final release/tag/basename to that exact validated candidate and identical ISO SHA-256. If final ISO bytes differ, promotion is forbidden and the rebuilt final becomes a new candidate requiring the complete applicable validation suite.

The public final release must retain ISO-01..ISO-38 evidence for the exact promoted/rebuilt payload under `docs/ACCEPTANCE.md` and `docs/VALIDATION.md`, plus the public legal/security/provenance/repository gates defined by this document and `docs/PUBLIC_RELEASE_POLICY.md`.

A validation report from another ISO hash cannot be reused merely because source code is “equivalent.”

---

## 17. Known limitations

Every accepted first-ISO/public release includes:

```text
KNOWN_LIMITATIONS.md
```

It must contain at least:

1. release/candidate identity;
2. supported architecture/primary environment;
3. firmware/Secure Boot boundary;
4. minimum/reference VMware profile;
5. intentionally unsupported/deferred first-release capabilities;
6. hardware qualification limits;
7. update/rollback/backup limitations;
8. encrypted reboot/unlock boundary;
9. known functional defects that remain acceptable under the release scope;
10. recovery limitations;
11. support/maturity statement for the public release.

Known limitations must describe actual limitations, not hide blockers that should have failed validation.

A limitation cannot waive ISO-01 through ISO-38 or any required legal, security, provenance, repository-hygiene, signing, or publication gate without an explicit authority change before candidate freeze.

---

## 18. Release notes

Public RC/final releases include:

```text
RELEASE_NOTES.md
```

Required sections:

```text
Release identity
Maturity/status
Supported target
Highlights
Installation/recovery links
Security model notes
Update notes
Known limitations link
Validation summary
Checksums/verification instructions
Source/SBOM/provenance links
Breaking changes / migration notes
```

The development first-ISO milestone may use an internal summary instead, but any public publication uses the file above.

Release notes must not claim support beyond the validation evidence.

---

## 19. Update-support statement

Every public release explicitly states its supported update posture.

The first-release architecture already locks:

- no background/startup automatic OS upgrades;
- explicit authorized managed updates;
- native Artix/pacman remains package authority;
- preflight and post-update health are required for the supported managed path;
- no automatic reboot;
- owner LUKS unlock remains required after reboot;
- no automatic whole-system rollback/general backup guarantee;
- Codex/PortusBrowser/Portus-MCP/tunnel-client pins are explicit compatibility-controlled transitions.

A release may claim the managed update path as supported only for the transitions that have executable validation evidence under `docs/UPDATES_RECOVERY.md` / `docs/VALIDATION.md`.

It must not imply arbitrary downgrade compatibility.

### First public support window/channel

Support is best effort and applies only to the current public final release. A release becomes unsupported when superseded by a newer public final release or explicitly withdrawn. Public release candidates are validation artifacts and are not supported releases.

The first public release has no LTS promise, response/fix SLA or guaranteed update cadence. Updates/security fixes may be issued when maintainers determine they are appropriate; a serious defect may cause withdrawal rather than in-place asset replacement.

---

## 20. Candidate immutability

After a candidate is frozen, any change to the following creates a new candidate:

- ISO bytes;
- shipped source/binaries;
- rootfs/build/install configuration;
- package/source manifest;
- kernel/boot configuration;
- policy/security defaults;
- OpenRC definitions;
- Codex/PortusBrowser/Portus-MCP/tunnel-client pins;
- protected-provider definitions;
- release-critical licence/notices for a public candidate;
- validation harness/procedure when the change materially alters acceptance semantics.

Do not patch a candidate in place.

Pure announcement/website copy outside the source/artifact bundle does not create a new technical candidate.

---

## 21. Replacement and revocation

### 21.1 Pre-release candidate

A bad RC is marked rejected/superseded and a higher RC number is built.

Do not delete history/evidence merely to make the sequence appear clean.

### 21.2 Published final release

A published final release is immutable.

If a blocking defect is discovered:

- do not replace assets under the same version/hash;
- mark the affected release as withdrawn/revoked/superseded in release metadata/public notice as appropriate;
- publish a new version after correcting and rerunning required gates.

### 21.3 Verification-key compromise

If the active release-signing private key is suspected compromised:

- stop using the key immediately;
- mark the corresponding public verification key revoked in release/security documentation;
- retain historical public-key material needed to inspect historical signatures, clearly marked with its status;
- generate a new owner-controlled encrypted Ed25519 release key through a deliberate key ceremony;
- publish the new public verification identity before signing a new release;
- do not silently re-sign or replace already-published release assets under the same release identity.

A key compromise may require withdrawal/replacement of affected release claims, but it never permits mutating an existing published version in place.

---

## 22. Public publication gate

A public final release may be published only when all of the following are true:

1. selected repository licence exists and redistribution audit passes;
2. first public maturity/version policy is selected;
3. public signing/verification mechanism is selected and operational;
4. support/update statement is published;
5. public RC source tree is clean and immutable;
6. all release-critical package/Codex/PortusBrowser/Portus-MCP/tunnel-client pins are immutable;
7. ISO-01..ISO-38 pass on the frozen public RC;
8. the public legal, security, provenance, repository-hygiene and publication gates defined by this document and `docs/PUBLIC_RELEASE_POLICY.md` pass;
9. `SHA256SUMS` and public verification material exist;
10. `build-metadata.json`, `release-metadata.json`, package/source manifest, SPDX SBOM and `provenance.json` exist and agree;
11. validation reports identify the same validated candidate/hash, and final promotion metadata (if used) points to that candidate without changing ISO bytes;
12. `KNOWN_LIMITATIONS.md` and `RELEASE_NOTES.md` are accurate;
13. required `SECURITY.md`, licence/notices and public build/install/recovery/update docs exist, and public repository documentation clearly states that external pull requests/code contributions are not accepted;
14. secret/private-data scan is clean;
15. the published ISO is the exact accepted candidate bytes.

---

## 23. Public asset verification workflow

Public instructions use this sequence:

```text
download release assets + SHA256SUMS + SHA256SUMS.sig
-> obtain the published PortusOS allowed-signers public key from the trusted source tree/release instructions
-> verify SSHSIG authenticity of SHA256SUMS
-> verify every file hash listed by SHA256SUMS
-> compare candidate/version/source identity in metadata
```

Canonical verification command:

```bash
python scripts/release/verify_release.py \
  --sha256sums SHA256SUMS \
  --signature SHA256SUMS.sig \
  --allowed-signers release/keys/portusos-release.allowed_signers
```

The verifier rejects malformed/non-sorted manifests, duplicate basenames, path traversal/symlinked artifacts, signature failure and hash mismatch.

---

## 24. Build reproduction rule

A public RC/final must be buildable by following published build instructions from the recorded source/tag and declared source/package inputs on the supported native Artix/Linux build environment.

The release does not claim bit-for-bit reproducibility until that property is actually implemented and tested.

The minimum first-public requirement is **procedure reproducibility**:

- no unpublished manual step;
- no secret developer-local source dependency;
- no undeclared AUR dependency;
- no unrecorded local package patch;
- the build produces the expected candidate class and complete metadata/provenance set.

If future work proves bit-reproducible ISO output, that stronger claim may be added explicitly.

---

## 25. Release metadata consistency checks

Before candidate publication, automatically assert:

```text
ISO hash == SHA256SUMS ISO entry
ISO hash == build-metadata artifact.sha256
ISO hash == validation-report iso_sha256
validated candidate id consistent across build metadata/validation report/provenance/release metadata
source revision identical across build metadata/validation report/provenance/tag
package/source manifest hash identical across metadata/provenance
SBOM hash identical across SHA256SUMS/provenance
validation report hash identical across SHA256SUMS/provenance
all referenced files exist and are non-empty
no release filename contains "latest"
no public RC/final has source_tree_clean=false
```

A mismatch blocks publication.

---

## 26. Development first-ISO acceptance bundle

The accepted development milestone is deliberately narrower than the later public OSS release.

It requires:

```text
immutable development candidate ISO
SHA256SUMS
build metadata
inspectable package/source provenance
ISO-01..ISO-38 accepted validation report
known first-ISO limitations
source revision/component pins sufficient to identify the build
```

It does **not** by itself require:

- final candidate-specific public redistribution clearance/notices;
- active public release-signing key material;
- activated public security-reporting host settings;
- public SBOM;
- contribution/security-support process;
- production support guarantees.

Those operational/candidate-evidence items become blockers when the candidate is promoted toward the public release track. The first-party Apache-2.0 licence and public version/signing/support policy choices are already resolved.

---

## 27. Release-policy status

`docs/PUBLIC_RELEASE_POLICY.md` and `oss/release-policy.json` define the public release policy:

```text
release             PortusOS 0.1.0 Technical Preview
first-party licence Apache-2.0
release authenticity OpenSSH SSHSIG Ed25519 over SHA256SUMS
support             best-effort, current-public-final only
```

The policy is selected. The signing mechanism still has an operational key-ceremony/public-key activation gate, and GitHub Private Vulnerability Reporting still has a repository-host activation/notification gate. Those are execution gates rather than open policy questions.

---

## 28. Decisions locked by this document

1. Development first-ISO candidates use immutable monotonic `PortusOS-first-iso-rc.<N>-x86_64.iso` identities.
2. Public RCs use `PortusOS-<version>-rc.<N>-x86_64.iso`; public finals use `PortusOS-<version>-x86_64.iso`.
3. Public Git tags are `v<version>-rc.<N>` and `v<version>` and are never moved/reused for different source.
4. Accepted development status promotes the same tested candidate; it does not justify rebuilding the ISO.
5. `SHA256SUMS` uses lowercase SHA-256, two spaces and basename-sorted entries.
6. Public release authenticates `SHA256SUMS` as `SHA256SUMS.sig` using OpenSSH SSHSIG Ed25519, namespace/identity `portusos-release`, with the public allowed-signers key published under `release/keys/`.
7. `build-metadata.json` schema version 1 carries immutable candidate/source/builder/artifact/component-authority linkage; public promotion uses separate `release-metadata.json` rather than rewriting build identity.
8. Public releases carry `release-metadata.json`, a machine-readable package/source manifest, SPDX 2.3 JSON SBOM and PortusOS `provenance.json`.
9. Public validation report must identify the exact same candidate ISO hash/source as release metadata.
10. `KNOWN_LIMITATIONS.md` is required for accepted development and public release; `RELEASE_NOTES.md` is required for public RC/final.
11. Managed-update claims are limited to transitions actually validated; no auto-update/reboot/rollback guarantee is introduced here.
12. Published final assets are immutable; corrections require a new release/version rather than replacing bytes in place.
13. Public publication requires ISO-01 through ISO-38 plus the complete legal, security, provenance, repository-hygiene, signing, and documentation bundle.
14. Development first-ISO acceptance and public OSS release are separate gates.
15. All owner release-policy decisions are locked by `docs/PUBLIC_RELEASE_POLICY.md`, including Apache-2.0 for PortusOS-owned source; remaining public-release gates require operational/candidate evidence rather than new owner policy choices.
