# PortusOS Build Contracts and ISO Harness

This directory contains the machine-readable whole-image contracts, deterministic builder/installer/validation machinery, and traceable per-iteration build ledger used to construct and validate PortusOS ISO images.

It does **not** replace subsystem authorities. It composes them.

## Ownership rule

One fact has one owner:

- `runtime/install/install.toml` remains authoritative for Portus-owned installed binaries, files, directories, identities and Portus OpenRC-service staging declarations.
- `runtime/integrations/portus-browser/integration.toml` and its provider manifest remain authoritative for the PortusBrowser compatibility/lifecycle contract.
- `CODEX_UPDATES.md` remains the prose authority for Codex behavior; `components/codex.yaml` is the selected-build pin/verification input.
- `docs/BOOT_STORAGE_RECOVERY.md` remains the prose authority for storage; `system/storage.yaml` is its machine-readable first-ISO materialization.
- `docs/POLICY.md` remains authoritative for root/Master identity; `system/identities.yaml` materializes installer ownership while referencing P16 for Portus service identities.
- `docs/PACKAGE_POLICY.md` remains authoritative for source ownership; `packages/packages.yaml` stores source-class verification/install/update/failure/redistribution policy once per class instead of repeating it on every package entry.
- `docs/VALIDATION.md` remains the meaning/evidence authority for ISO-01..ISO-38; `validation/matrix.yaml` contains orchestration metadata only.
- `docs/RELEASE.md` remains the release authority; `schemas/*.schema.json` materialize its generated-output shapes.

Build and installer code must consume these contracts rather than maintaining hidden package, service, storage, validation or release lists.

## Resolution states

Human-maintained source contracts use finite resolution states:

- `locked` — the value is already selected by an authority and may be consumed directly;
- `linux-verified` — the fact requires verified Artix/Linux evidence before it may become release-resolved;
- `owner-decision` — a genuine owner/release-policy choice remains open;
- `generated` — the value is produced by a resolver/build step and must not be hand-written as a release fact.

A `linux-verified` or `owner-decision` field that is unresolved carries a null value where applicable, an `unresolved_reason`, and a `required_gate`.

`packages/packages.lock.yaml` is intentionally **not** committed as a fake Windows resolution. It is generated on the selected Artix environment from `packages/packages.yaml` and validated against `schemas/package-lock.schema.json`.

## Host-safe gate

The `portus-build-contract` Rust crate validates the source graph and distinguishes:

- `source-valid`: all contracts parse, cross-references agree, no secret/AUR/ownership violation exists, and unresolved Linux facts are explicit;
- `release-resolved`: every release-required Linux/build pin is resolved. Development builds may proceed only through explicitly supported non-release paths while unresolved release gates remain visible and fail closed for candidate freeze.

The `portus-build` crate consumes that source-valid graph. `builder/layout.yaml` bounds generated work/cache/out roots; `installer/responsibilities.yaml` fixes Calamares responsibilities without guessing module IDs; `iso/artools-profile/adapter.yaml` fixes the native Artix/artools boundary; `rootfs/overlay/` and `packages/local/` are tracked source roots; and `build-iso.sh`/`.ps1` are the single repository-owned native-build entry points.

`powershell -File scripts/test.ps1 build-skeleton` or `bash scripts/test.sh build-skeleton` validates the host-safe build/installer layer. The canonical Linux build entry point performs its own mandatory environment and contract preflight. A release candidate may not be frozen from a graph that is not release-resolved.

## Config-driven build-iteration harness

`BUILD_HARNESS.md` is authoritative for repeated ISO-build orchestration and usage.

Every normal ISO attempt is driven by a versioned repository config. The canonical first interactive ISO request is:

```text
portusos-build/configs/first-live.json
```

Normal Linux invocation:

```bash
./portusos-build/build-iso.sh --config portusos-build/configs/first-live.json
```

The harness validates the config, snapshots its exact bytes as `build-config.json`, records its SHA-256 and resolved values in the timestamped log/run metadata, hashes every tracked repository input, runs the mandatory non-skippable environment gate from `BUILD_REQUIREMENTS.md` into `preflight.json`, then executes the configured repository-test preflight depth. Only an environment without `BLOCK` results may reach contract/build-plan generation and the native Artix adapter; success still requires exactly one ISO.

Every successful, failed, or blocked attempt is preserved under:

```text
portusos-build/out/builds/<run-id>/
```

with `build-config.json`, `build-run.json`, `build.log`, `preflight.json` once the mandatory environment gate runs, `inputs.json`, any reached `contract-report.json`/`build-plan.json`, `RUN_SHA256SUMS`, and the harness-owned `artifacts/` directory. `portusos-build/out/builds/LATEST` points to the newest run ID.

Validate a config without building:

```bash
python -B scripts/build_iteration.py --check-config portusos-build/configs/first-live.json
```

The build config is an invocation spec; it selects existing composition/profile authorities rather than duplicating package or product decisions. See `BUILD_HARNESS.md` for the config schema, run lifecycle, native-adapter environment contract, failure semantics, regression workflow, and candidate handoff.

## Validation harness

The ISO-01..38 acceptance/evidence harness is implemented in the existing `portus-build` crate. It consumes `validation/matrix.yaml` and the validation schemas; it does not duplicate the procedure/assertion authority from `docs/VALIDATION.md`.

Primary commands:

```text
portus-build validation-harness-check
portus-build validation-materialize --candidate-json <candidate.json> --output-root <validation-root>
portus-build validation-action --candidate-root <candidate-root> --test-id ISO-XX --record-json <record.json>
portus-build validation-record --candidate-root <candidate-root> --result-json <result.json>
portus-build validation-redactions --candidate-root <candidate-root> --input-json <redactions.json>
portus-build validation-report --candidate-root <candidate-root> --input-json <report-input.json>
portus-build validation-verify --candidate-root <candidate-root>
portus-build validation-vm-run
```

`validation-vm-run` intentionally exits 78 while a real VMware execution adapter is unavailable. `scripts/test.* graphical-vm` is a host-safe harness verification category: it runs the harness tests/check and proves unavailable VM execution fails closed rather than claiming graphical/VM acceptance.

## Candidate initialization

`portus-build candidate-init` turns produced ISO bytes into the canonical development/public-RC candidate state without inventing another release model:

```text
portus-build candidate-init --artifact <produced.iso> --input-json <candidate-init.json>
portus-build candidate-verify --candidate-root portusos-build/out/validation/<candidate-id>
```

The candidate ID and frozen ISO basename are derived from `docs/RELEASE.md`; callers do not supply either identity. The initializer copies the ISO under the canonical basename, snapshots the package/source, Codex, PortusBrowser, Portus MCP and tunnel-client contract inputs, includes `packages.lock.yaml` when it exists, writes `build-metadata.json`, initializes `candidate.json` + all 38 `not_run` result slots, writes a deterministic basename-sorted `SHA256SUMS`, and emits `validation-report.json`/Markdown in `incomplete` state.

Initialization `SHA256SUMS` covers only immutable candidate inputs. Mutable validation evidence/report files are deliberately excluded until later release finalization. Candidate reuse is fail-closed: an existing candidate root is never overwritten, and `candidate-verify` rejects ISO/metadata/pin/checksum mutation or identity mismatch.
