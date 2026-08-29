# PortusOS Test Policy

**Status:** Active public test policy
**Scope:** Portus-owned runtime, build, installer, security and release-validation source in this repository

## 1. Principle

Tests are layered by subsystem and risk. A green low-level test does not stand in for higher-level Linux integration, security, graphical, installer, VMware, or release acceptance proof.

No placeholder test category may return success merely because it has no tests.

## 2. Test categories

### Unit

Pure logic contained within one crate/module. Unit tests run without privileged host mutation, graphical launch, Portus services, or a VM.

### Protocol

Wire envelopes, semantic errors, identifiers, compatibility, framing, malformed input, pagination and serialization behavior shared across clients/services.

### State

SQLite creation, migrations, rollback, corruption handling, principal filtering, retention, durable/derived boundaries and restart persistence against isolated temporary state.

### Integration

Real local component boundaries such as Unix sockets, kernel peer credentials, daemon/client round trips, provider registration, task/index relationships and native Linux observations.

Integration tests must use temporary/runtime-isolated resources where possible and must not mutate the host's real PortusOS installation state.

### Security-negative

Tests whose success condition is rejection or non-disclosure: principal spoofing, cross-user access, secret export/exfiltration, arbitrary privileged execution, unsafe redirects/TLS, stale-resource mutation, log/diagnostic leakage and recovery downgrade attempts.

Security-negative tests are mandatory for any boundary described as protected, privileged, private or non-exporting.

### Build contract

Validates the machine-readable `portusos-build/` source graph, schema strictness, package/source vocabulary, storage/VM invariants, cross-authority references, validation-matrix shape and explicit unresolved release facts.

`source_valid=true` does not imply `release_resolved=true`.

### Build skeleton

Validates the deterministic builder, installer planning, disk arithmetic, package/identity/service composition, candidate initialization, validation-harness materialization, canonical build config and fail-closed native-build boundary.

### Hardening

Runs the cross-system Rust suites that enforce principal isolation, privilege routing, provider failure isolation, path containment, secret-shaped-data rejection and other security invariants across subsystem boundaries.

### Graphical/VM harness

Validates the candidate-scoped ISO-01 through ISO-38 evidence harness: exact row materialization, candidate/hash/environment substitution rejection, objective evidence requirements, mutation detection, retry history, redaction rules and accepted-report aggregation.

The current `validation-vm-run` command is a separate real-VM adapter boundary and must fail closed with exit 78 while that execution adapter is unavailable. A green `graphical-vm` host-safe category means **acceptance harness ready**, not **VM acceptance passed**.

### OSS

Runs public repository/dependency/release-preparation checks, including:

- OSS tooling unit tests;
- Cargo dependency/licence/source inventory;
- `cargo audit`;
- `cargo deny`;
- strict current-tree and reachable-history secret/private-data scanning;
- release-signing mechanism tests with temporary external keys;
- candidate fixture SPDX/provenance generation.

A green `oss` category proves the tooling and repository checks. It does not by itself prove candidate-specific redistribution, repository-host security settings, signing-key activation, or final publication readiness.

### All

Runs the complete non-graphical Rust workspace suite. It intentionally does not silently include real graphical/VM acceptance or the network/history-dependent OSS publication gate.

## 3. Common entry points

Linux:

```bash
bash scripts/check.sh
bash scripts/build.sh
bash scripts/test.sh unit
bash scripts/test.sh protocol
bash scripts/test.sh state
bash scripts/test.sh integration
bash scripts/test.sh security-negative
bash scripts/test.sh build-contract
bash scripts/test.sh build-skeleton
bash scripts/test.sh hardening
bash scripts/test.sh graphical-vm
bash scripts/test.sh oss
bash scripts/test.sh all
```

Windows host-safe equivalents use `scripts/check.ps1`, `scripts/build.ps1`, and `scripts/test.ps1`.

The Artix fact collector is intentionally separate:

```bash
bash scripts/artix/collect-l0-l2-facts.sh
```

It is read-only and exits 78 on unsupported/non-Artix contexts rather than inventing target evidence.

## 4. What host-safe tests prove

The Linux integration suite exercises real distribution-neutral boundaries including:

- Unix sockets and `SO_PEERCRED`;
- bounded malformed-frame handling;
- runtime restart persistence;
- daemon-backed CLI transport;
- provider reconciliation;
- native `/proc`/package/application observations;
- managed child launch/show/cancel;
- task event disconnect/replay;
- native resource observations;
- deliberate artifact registration over authenticated IPC;
- install-staging path-containment defenses.

These tests do **not** replace installed Artix/OpenRC/VMware proof. Installed ownership/modes, OpenRC service supervision, root sockets, real protected-provider upstream behavior, X11/i3 integration, Codex/tmux/login/SSH, visual effects, Chromium/PortusBrowser, native package installation, encrypted boot and installer behavior remain candidate-level validation responsibilities.

## 5. Build and release testing

Normal ISO construction goes through:

```bash
bash portusos-build/build-iso.sh --config portusos-build/configs/first-live.json
```

The harness snapshots and hashes the config, runs the mandatory `BUILD_REQUIREMENTS.md` preflight, validates source contracts, stages the exact payload, invokes the bounded native Artix adapter, captures logs/results/cleanup evidence and requires exactly one ISO artifact on success.

A config with release-candidate semantics additionally requires the release-resolved source graph. Unresolved release facts must fail closed rather than being guessed.

Candidate acceptance is governed by `docs/ACCEPTANCE.md` and `docs/VALIDATION.md`, not by the ordinary source test suite.

## 6. Developer-machine safety

Ordinary checks and unit tests must not:

- start GUI applications;
- start or stop real host services;
- alter real firewall/network configuration;
- mount or repartition disks;
- mutate host credentials or keyrings;
- write protected credentials;
- launch a VM;
- write to production PortusOS state paths.

Tests requiring those effects must use an explicit installed-system/VM validation path.

## 7. Failure semantics

An unavailable required harness exits nonzero or uses the documented fail-closed exit 78 boundary. Missing tests are never a passing result.

Missing external prerequisites may be represented as a typed skip only when the category has a real harness and the skip policy is explicit.