# PortusOS Project and System Structure

## 1. Document purpose

This document defines the intended ownership boundaries and structural direction for:

- the PortusOS repository;
- the ISO build and installer;
- the installed operating system;
- the agent control plane;
- policy, task, artifact, capability, health, and audit state;
- Codex and OKF integration;
- Portus application integration;
- the minimal graphical presentation layer;
- remote and recovery surfaces.

It is an aspirational structure, not a claim that these files or components already exist.

`docs/PROJECT.md` controls product intent and public scope. `DOCUMENTATION.md` is the public documentation/authority map. `docs/ACCEPTANCE.md` controls the exact first-release acceptance contract, reference target, required proofs, exclusions, and blocking acceptance matrix. `docs/POLICY.md` controls Master Portus Linux-user identity, administrator/root authority, permission bundles, granular policy, cross-user isolation, delegated-agent sandboxing, root-equivalent classification, and local principal enforcement. `docs/SECRETS.md` controls protected reusable credentials and secret-backed operations. `docs/RUNTIME.md` controls the PortusOS runtime, `portusd`, Portus OS CLI relationship, local IPC, and provider/runtime failure boundaries. `docs/STATE.md` controls SQLite and Portus-owned mutable-state boundaries. `docs/TASKS.md` controls Master execution routing, Portus task promotion, task identity/lifecycle, execution relationships, cancellation/retry, results, cleanup, and reconciliation. `docs/EVENTS_AUDIT.md` controls significant-event persistence/retention, live wake-up/stream semantics, security-audit records, redaction, attribution, and audit storage boundaries. `docs/CAPABILITIES.md` controls registered capability/provider/interface/resource/operation identity, manifests, direct provider use, versioning, lifecycle/health/policy integration, task affordances, compatibility and uninstall history. `CODEX_UPDATES.md` controls Codex details subject to the POLICY/SECRETS/TASKS authority boundaries. `docs/CONTROL_PLANE.md` controls the PortusOS layer's scope and non-duplication rule. `OKF.md` controls durable knowledge. `docs/SYSTEM_INDEX.md` controls the system-index and machine-map architecture. `docs/PORTUS_BROWSER.md` controls the required PortusBrowser integration baseline. `docs/SYSTEM_CAPABILITIES.md` controls the capability and installation inventory. `BUILD_HARNESS.md`, `docs/VALIDATION.md`, and `docs/RELEASE.md` control the supported public build, acceptance, and publication flow.

## 2. Structural principles

### 2.1 The OS is a non-duplicative control plane

PortusOS owns general system coordination and common agent infrastructure that Codex, ordinary commands, and application-owned interfaces do not already provide.

It should provide:

- effective policy and approvals;
- the agent-managed system index;
- durable task and session relationships;
- capability discovery and application lifecycle;
- managed process and service lifecycle where persistence or coordination is required;
- artifact registration, presentation, and delivery metadata;
- health, diagnostics, events, audit, degradation, and recovery;
- transactional adapters for system operations that require preflight, rollback, or cross-component coordination.

It should not directly implement every specialised capability, duplicate Codex file and shell tools, or wrap ordinary commands merely to rename them.

### 2.2 Portus applications are capability providers

PortusBrowser is a required bundled domain component. PortusWindow and future Portus applications remain specialised domain components whose required release status is decided separately.

They integrate through versioned contracts and structured interfaces rather than being hardcoded into Master Portus or expanded into dozens of permanent top-level agent tools.

### 2.3 Policy boundaries and tool inclusion

Policy determines what the agent may do. The PortusOS tool catalogue must not be used as a substitute for Linux permissions, Codex permissions, sandboxing, approvals, provider policy, or the secrets boundary.

The PortusOS surface should remain small because it exposes only Portus-specific domains, not because the agent is expected to work from a narrow map of permitted workflows.

Expected Portus-owned domains include:

1. system index and machine relationships;
2. capability and application lifecycle;
3. durable tasks and managed execution;
4. policy, approvals, privilege mediation, and secret-backed actions;
5. health, diagnostics, degradation, and recovery;
6. artifacts, presentation, communication, and delivery;
7. coordinated system adapters for updates, network, storage, services, sessions, and displays.

Generic filesystem reading, project search, editing, patching, Git operations, and ordinary shell execution remain Codex and native-command capabilities. `docs/CONTROL_PLANE.md` defines the mandatory inclusion test.

### 2.4 Linux remains accessible

The Portus abstraction must not become the only repair path.

OpenRC, the shell, files, processes, package tools, Git, SSH, logs, and the live ISO remain available for diagnostics and recovery.

### 2.5 Local-first and replaceable

Required operation must not depend on a central cloud service.

Runtime state, policy, knowledge, tasks, and audit data should be inspectable locally. Optional remote providers and communication adapters must be replaceable.

### 2.6 Clear state ownership

Different persistence layers must not be collapsed into one ambiguous database.

- OpenRC owns service lifecycle.
- Codex owns its root-session/thread history, subagent trees, goals, queue/fork/resume behavior, conversation compaction, and skill execution state.
- tmux owns live terminal visibility and attachment; it is not required merely to make Codex work durable or independent.
- Git owns project history.
- OKF owns curated durable knowledge files, including stable per-machine facts learned by Master Portus.
- the system index owns refreshable, derived installed-software and live-machine views plus explicitly separated annotations.
- Portus registries own durable intentional identity, lifecycle, policy-association, and relationship metadata.
- task state owns durable work lifecycle.
- artifact state owns outputs and delivery metadata.
- applications own their domain state unless a contract explicitly delegates it.

### 2.7 Linux-user security boundary

Master Portus runs as a real non-root Linux user and is not constrained by a general-purpose Codex workspace sandbox. Linux user ownership/isolation is the primary boundary for the Master role. Project/task agents may still receive narrower Codex/Bubblewrap/worktree sandboxes.

The administrator/root authority owns enforced permission bundles and granular policy. Multiple Master Portus users may coexist, so user-owned workspaces, Codex state, application state, tasks, artifacts and provider relationships must remain principal-scoped. `docs/POLICY.md` controls this architecture.

Protected reusable credentials use a separate non-exporting boundary defined by `docs/SECRETS.md`; broad access to ordinary files belonging to the Master UID is not evidence that protected credentials may be exposed.

### 2.8 Portus CLI naming

User-facing Portus command-line interfaces identify the component they control. The operating-system CLI is **Portus OS CLI** with executable `portus-os`. The project does not use a generic `portus` or `portus-cli` executable/alias.

The naming form is `portus-<component>`, for example `portus-browser`, `portus-master` when a dedicated Master Portus interface is required, and future `portus-window`. The daemon `portusd` is not a CLI and retains its daemon-style name.

## 3. High-level runtime architecture

```text
Authenticated user intent
  -> SSH / local terminal / future communication adapter
  -> Master Portus
  -> Codex and native computer tools for ordinary files, Git, code, and commands
  -> PortusOS control surface when system-level additions are needed
       - system index
       - capabilities and application lifecycle
       - durable tasks and managed execution
       - policy, approvals, and privilege mediation
       - health and recovery
       - artifacts, presentation, and delivery
       - coordinated system adapters
  -> policy, approval, task, event, and audit context
  -> adapters and capabilities
       - native Linux and OpenRC
       - Codex sessions and project agents
       - i3, tmux, network, packages, and storage
       - PortusBrowser
       - PortusWindow
       - future Portus applications
  -> artifact registry and presentation/delivery surfaces
```

The system index is not an automatic stage through which every operation must pass. Master Portus queries it only when useful, and may instead use native commands or specialised provider interfaces directly.

The runtime uses the already locked small modular `portusd` daemon only for Portus-specific durable coordination, shared state, events, health, provider relationships, and managed lifecycle. Ordinary Codex/Linux work remains direct, and additional daemons/services should be introduced only when a specialised provider or demonstrated lifecycle requirement justifies them.

## 4. Aspirational repository tree

```text
portus-os/
  AGENTS.md
  README.md
  CODEX_UPDATES.md
  OKF.md

  docs/
    PROJECT.md
    ACCEPTANCE.md
    PUBLIC_RELEASE_POLICY.md
    STRUCTURE.md
    RUNTIME.md
    STATE.md
    ISO_BUILD_INSTALLER.md
    BOOT_STORAGE_RECOVERY.md
    HARDWARE.md
    PACKAGE_POLICY.md
    CONTROL_PLANE.md
    PORTUS_BROWSER.md
    SYSTEM_INDEX.md
    SYSTEM_CAPABILITIES.md
    OPENRC_SERVICES.md
    POLICY.md
    SECRETS.md
    CLI.md
    CAPABILITIES.md
    TASKS.md
    EVENTS_AUDIT.md
    ARTIFACTS.md
    COMMUNICATION.md
    GUI_SYSTEM.md
    UPDATES_RECOVERY.md
    CODEX.md
    KNOWLEDGE.md
    CONFIG.md
    CONTRACTS.md
    STARTUP_CHAIN.md
    POSTINSTALL.md
    VALIDATION.md
    RELEASE.md

  schemas/
    package-manifest.schema.yaml
    capability-manifest.schema.yaml
    system-index-resource.schema.json
    task.schema.json
    artifact.schema.json
    event.schema.json
    policy.schema.yaml
    result.schema.json

  runtime/
    README.md

    cli/
      portus-os

    control/
      applications/
      tasks/
      policy/
      artifacts/
      delivery/
      health/
      recovery/
      managed-workflows/

    index/
      catalogue/
      topology/
      providers/
      annotations/

    registry/
      projects/
      sessions/
      tasks/
      artifacts/
      capabilities/

    adapters/
      openrc/
      codex/
      tmux/
      i3/
      visual-input/
      network/
      packages/
      storage/

    health/
      checks/

    events/
    audit/
    recovery/

  integrations/
    README.md
    manifests/
      portus-browser.yaml
      portus-window.yaml
    installers/
    policies/
    tests/

  portusos-build/
    README.md
    build-iso.sh
    build-iso.ps1

    configs/
      first-live.json

    contracts/
      build.yaml

    builder/
      layout.yaml

    packages/
      packages.yaml
      local/
        README.md
      # packages.lock.yaml is generated on Artix; intentionally absent now

    components/
      codex.yaml
      portus-browser.yaml
      portus-mcp.yaml
      tunnel-client.yaml

    system/
      storage.yaml
      vm-profiles.yaml
      base-services.yaml
      identities.yaml

    installer/
      calamares.yaml
      responsibilities.yaml

    iso/
      profile.yaml
      artools-profile/
        adapter.yaml
    artix/
      bootstrap.json

    rootfs/
      overlay/
        README.md

    validation/
      matrix.yaml

    schemas/
      build-config.schema.json
      environment-preflight.schema.json
      build-run.schema.json
      package-source.schema.json
      package-lock.schema.json
      build-metadata.schema.json
      release-metadata.schema.json
      validation-result.schema.json
      validation-report.schema.json
  fixtures/
    policy/
    capabilities/
    tasks/
    artifacts/
    okf/

```

This tree communicates ownership boundaries. Exact programming languages, package names, service processes, and file names must be decided during implementation. Empty scaffolding must not be created merely to match the diagram.

### 4.1 Implemented runtime workspace

The concrete Rust workspace contains the first-party native Portus components. It should grow only when a real subsystem boundary justifies another crate. The current implementation layout is:

```text
Cargo.toml
Cargo.lock
rustfmt.toml
.editorconfig

runtime/
  README.md
  crates/
    portus-protocol/
    portus-client/
    portus-audit/
    portus-policy/
    portus-health/
    portus-artifact/
    portus-protected-api/
    portus-state/
    portus-provider/
    portus-index/
    portus-task/
    portus-master/
    portus-visual/
    portus-browser-integration/
    portus-install/
    portusd/
    portus-os/
    portus-privd/
    portus-apid/
    portus-api/
    portus-auth/
  integrations/
    manifests/
      protected-api.toml
      portus-browser.toml
    portus-browser/
      integration.toml
    protected-api/
      providers/
        openai.toml
  install/
    README.md
    install.toml
    policy/
      policy.toml
      actions.toml
      bundles/
    skills/
      protected-api/
        SKILL.md
    openrc/
      portusd.in
      portus-privd.in
      portus-apid.in

scripts/
  build_environment_preflight.py
  build_iteration.py
  check.sh
  check.ps1
  build.sh
  build.ps1
  test.sh
  test.ps1
  artix/
    collect-l0-l2-facts.sh
    context.py
  oss/
    audit_repo.py
    dependency_inventory.py
    release_evidence.py
    test_oss.py
  release/
    prepare_public_key.py
    sign_checksums.py
    verify_release.py
    test_signing.py

oss/
  release-policy.json
  third-party-components.json
  schemas/
    release-policy.schema.json
    system-package-inventory.schema.json
    third-party-review.schema.json
    provenance.schema.json

release/
  README.md
  keys/
    README.md

LICENSE
SECURITY.md
THIRD_PARTY.md
deny.toml

docs/
  PUBLIC_RELEASE_POLICY.md
  TESTING.md
```

This is not a competing ownership model. The earlier conceptual `runtime/control`, `runtime/index`, `runtime/registry`, adapter, health, event and recovery domains remain architectural module boundaries and become Rust modules/crates only when their PLAN phase demonstrates that separation is useful. `portus-index` was added in P6 for the finite native source/correlation boundary, `portus-audit` in P8 because security audit storage/validation is deliberately separate from `portus.db`, `portus-policy` in P9 so `portusd`, `portus-privd`, installer tooling and tests share one fail-closed policy resolver rather than duplicating authority decisions, `portus-protected-api` in P10 so the independent daemon/direct client/AUTH client share one strict provider wire/definition/secret-safety contract without moving provider-owned storage or upstream execution into a generic broker crate, `portus-health` in P11 so typed health/restart-budget/native resource-probe semantics are shared without moving durable state or service execution into the classification crate, and `portus-artifact` in P12 so canonical-file hashing/reconciliation/expected-target cleanup semantics stay outside SQLite and outside a duplicate filesystem API. The checked-in provider integration TOML plus `runtime/install/` are now concrete P16 installation source material, not live `/etc` state. `portus-install` stages these sources only into an explicit target root and keeps unresolved Linux ownership/OpenRC/package behavior machine-readable rather than inventing it on Windows. Empty scaffolding is still not created merely to mimic the conceptual diagram.

The first-party native implementation uses one Rust workspace. Independent provider protocols remain language-neutral, and native OpenRC/configuration/install resources keep their domain-native formats.

Root `LICENSE` is the Apache-2.0 licence for PortusOS-owned source. The root `oss/`, `scripts/oss/`, `release/`, and `scripts/release/` trees are release-preparation infrastructure, not installed runtime components. `oss/` owns source-side third-party/review/provenance/release-policy contracts; `scripts/oss/` owns host-safe repository/history/dependency/SBOM/provenance checks. `release/` contains public verification documentation/key material only, while `scripts/release/` implements signing/verification without owning the private key. Candidate-specific generated evidence remains under ignored build/evidence roots and does not become source authority.

P17 adds no competing runtime owner or generic security wrapper. It hardens existing boundaries in place: `portus-state` owns reusable secret-shaped-value rejection for generic durable safe fields, `portus-install` owns canonical source/destination staging containment, and `portusd` owns cross-surface coordination tests proving principal and degradation isolation.

## 5. Root documentation ownership

### `docs/PROJECT.md`

Owns:

- product definition;
- core principles;
- intended users and use cases;
- owner decisions;
- product and release boundaries.

### `DOCUMENTATION.md`

Owns the public documentation index, authority routing, and the separation between public product/build/release documentation and private development history.

### `docs/ACCEPTANCE.md`

Owns:

- the exact first-ISO success contract;
- the reference VMware target and supported boot/install/access path;
- which first-ISO components must be real versus intentionally minimal;
- required Portus runtime, index, browser, protected-credential, health, degradation, and recovery proofs;
- explicit first-ISO exclusions;
- the blocking acceptance matrix.

### `CODEX_UPDATES.md`

Owns the current Codex implementation baseline until that material is fully integrated into dedicated Codex and implementation documents.

### `OKF.md`

Owns the PortusOS durable knowledge format and Portus OKF Profile.

### `docs/CONTROL_PLANE.md`

Owns the non-duplicative PortusOS control-plane boundary, the distinction between ordinary Codex/native operations and Portus-owned additions, and the review test every proposed Portus command, endpoint, adapter method, or top-level tool must pass.

### `docs/SYSTEM_INDEX.md`

Owns the freshness-aware operational machine graph: source observations/provenance, generation-scoped `idx_<UUIDv7>` handles, resource/relationship correlation and evidence strength, available control paths, source-aware freshness, targeted refresh/reconciliation/rebuild, provider/task/Codex references, principal filtering, and the boundary between live index state and durable machine knowledge. The index remains an optional agent aid rather than injected context or a control boundary.

### `docs/CAPABILITIES.md`

Owns registered provider integration: capability/provider/interface/resource/operation definitions, declarative manifests, provider type and immutable registration generations, direct provider use, semantic capability IDs, version compatibility, lifecycle and health ownership, provider policy boundaries, opaque resource references, task affordances, degradation and uninstall history.

### `docs/TASKS.md`

Owns execution-surface routing, the Portus task-promotion boundary, task identity/state, Codex/process/provider relationships, managed-versus-associated execution, cancellation/retry/approval waiting, results, cleanup, principal scoping, and daemon-restart reconciliation.

### `docs/TESTING.md`

Owns the implementation test categories, host-safety rules, common check/build/test entry points, false-green prohibition, and separation between host-safe tests and graphical/VM acceptance.

### `BUILD_HARNESS.md`

Owns the supported repeated ISO-build orchestration, build configuration, mandatory preflight, staging, native Artix adapter boundary, run-scoped evidence, failure semantics, and candidate handoff.

### `AGENTS.md`

Owns concise repository instructions and routes agents to the relevant product, structure, Codex, knowledge, Linux, and implementation documents.

### `scripts/artix/collect-l0-l2-facts.sh`

Owns read-only target-host evidence collection for the second L0/L2 Artix handoff. It may report host/toolchain/repository/package-candidate/OpenRC facts, but it must not install packages, enable services, or convert candidate availability into release-resolved package/service selections by itself.

### `README.md`

Owns human-facing orientation and verified build, test, install, and recovery entry points once they exist.

## 6. Durable documentation ownership

The `docs/` directory should separate product decisions from implementation contracts.

Expected documents:

- `ISO_BUILD_INSTALLER.md`: locked `artools` + native Artix/Linux build architecture, minimal Calamares UI/execution baseline, zero-custom-module-unless-proven rule, PortusOS install ownership, and remaining exact module/build verification.
- `BOOT_STORAGE_RECOVERY.md`: locked GRUB/UEFI, 512 MiB ESP, 2 GiB `/boot`, LUKS2/Argon2id, `portus` LVM, ext4/swap, mkinitcpio dual-kernel baseline, and remaining exact command/repair verification.
- `HARDWARE.md`: locked Artix `linux-lts` normal + `linux` alternate, native-VMware/modesetting-first baseline, `open-vm-tools` case review, firmware/microcode and supported hardware/VM boundary.
- `PACKAGE_POLICY.md`: locked official-Artix-first/no-AUR supply-chain boundary, per-component exception review, source classes, signing, pinning, licences, update ownership, and manifest semantics.
- `SYSTEM_CAPABILITIES.md`: required and optional installed capabilities, candidate packages, agent-use paths, Portus-owned components, and the inventory used to derive the package manifest.
- `OPENRC_SERVICES.md`: locked `sysinit -> boot -> default` strategy, machine/session ownership, preferred Artix service candidates, `supervise-daemon` boundary, exact init-script/dependency verification, health and recovery.
- `CONTROL_PLANE.md`: broad primitives and non-duplication boundary.
- `RUNTIME.md`: authoritative `portusd`, Portus OS CLI/runtime relationship, IPC, provider, managed-execution, event, and degradation architecture.
- `STATE.md`: authoritative SQLite and Portus-owned mutable-state architecture.
- `POLICY.md`: authoritative root/admin authority, Master Portus Linux-user identity, installer permission bundles, granular post-install policy, privilege/elevation, delegated-agent sandboxing, cross-user isolation, principal ownership, and root-equivalent classification.
- `POLICY_IMPLEMENTATION.md`: authoritative first-ISO Linux policy/elevation mapping, bundle materialization/defaults, `portus-privd`, root-owned TOML policy, typed privileged actions, root-equivalent inventory and administrator mutation path.
- `SECRETS.md`: authoritative protected credential, AUTH, secret-backed operation, provider/keyring classification, non-export, redaction, backup, and validation boundary.
- `PROTECTED_API_PROVIDER.md`: authoritative first protected API provider implementation contract: `portus-apid`, dedicated Linux identity/store, use/admin Unix sockets, direct `portus-api` client, provider bindings, TLS/redirect policy, provisioning/rotation, compatibility proxy authentication, health/audit, and negative tests.
- `CREDENTIAL_CLASSES.md`: authoritative first-ISO classification direction for Codex/browser/SSH-signing/password-manager/hardware/AUTH/backup credential classes.
- `PROTECTED_API_VALIDATION.md`: authoritative adversarial first protected API-provider validation contract.
- `CLI.md`: authoritative Portus OS CLI (`portus-os`) command tree, human/JSON/JSONL output contract, errors/exit codes, pagination, preconditions, task events, diagnostics, help/version, and compatibility behavior.
- `CAPABILITIES.md`: authoritative registered-provider identity, manifest, interface, resource, task-affordance, lifecycle, health, policy, compatibility, degradation, and uninstall contracts.
- `TASKS.md`: authoritative execution routing, task-promotion boundary, durable task lifecycle, execution relationships, cancellation/retry, results, cleanup, and reconciliation.
- `EVENTS_AUDIT.md`: authoritative significant-event object/sequence/retention contract, live subscriber wake-up/streaming behavior, audit schema/redaction/attribution, bounded audit storage, and Linux carry-over gates.
- `ARTIFACTS.md`: authoritative deliberate artifact registration, `artifact_<UUIDv7>` identity, filesystem/provider locators, integrity, confidentiality/principal isolation, task/provider relationships, retention/cleanup, local presentation, and deferred delivery semantics.
- `HEALTH_RECOVERY.md`: authoritative common health/degradation observations, recovery dispositions, bounded restart/repair policy, diagnostic evidence/bundles, resource warnings, and independent recovery boundaries.

- `COMMUNICATION.md`: SSH and adapter identity, security, approvals, attachments, and outage handling.
- `GUI_SYSTEM.md`: locked conditional `startx` + `dbus-run-session` + minimal/config-driven i3 session baseline, session-owned helpers, visual fallback, sensitive-screen handling and recovery.
- `UPDATES_RECOVERY.md`: locked explicit-update/no-auto-reboot architecture, Artix/pacman ownership, preflight/migration safety, bounded update-specific recovery state, dual-kernel protection, post-update health proof, and deferred general backup/automatic rollback.
- `CODEX.md`: installed Codex configuration and Portus integration.
- `KNOWLEDGE.md`: OKF bundle locations and operational integration, subordinate to `OKF.md`.
- `CONFIG.md`: all configuration sources and precedence.
- `CONTRACTS.md`: shared code and data contracts.
- `STARTUP_CHAIN.md`: boot through OpenRC, agent runtime, local session, and recovery paths.
- `POSTINSTALL.md`: module ownership and ordering.
- `VALIDATION.md`: executable and manual validation matrix.
- `RELEASE.md`: artifact metadata, checksums, signing, validation evidence, and upgrade support.

Implementation must not consume prose documents as runtime configuration.

## 7. Runtime control-plane ownership

### 7.1 `runtime/cli/`

Owns the Portus OS CLI executable `portus-os` and its human/agent command contract. No generic `portus` or `portus-cli` entry point is provided.

`portus-os` is implemented in Rust as a native compiled executable. Its normal architecture is a thin typed client over the versioned local JSONL/Unix-socket contract; it must not duplicate `portusd` task/index/provider/policy/state business logic. Rust-side protocol/wire types and Unix-socket client/framing behavior should be factored into reusable crates/modules shared by the CLI implementation rather than copied across subcommands. Exact crate names and dependency choices remain subordinate to `docs/CLI.md`.

The wire contract remains language-neutral; this decision does not implicitly require every PortusOS runtime component to use Rust.

`portus-os doctor` intentionally retains bounded daemon-independent diagnostic logic for bootstrap and recovery. This exception must remain diagnostic rather than growing into an alternate control plane.

The machine-wide `portus-os-cli` Codex skill is separate from this executable: it is instruction/reference content that teaches agents how to use `portus-os` and must not become either Rust implementation code or the general workflow library.

The CLI should:

- expose only Portus-specific system capabilities and coordinated workflows;
- avoid duplicating Codex file, search, edit, patch, shell, Git, and normal application capabilities;
- return stable exit codes;
- support structured output;
- bound potentially large results;
- support dry runs, expected-state checks, and preflight checks where relevant;
- associate managed lifecycle operations with task and policy context;
- surface approval requirements;
- log significant actions without exposing secrets;
- preserve access to underlying native commands for direct use and recovery.

The CLI is a core runtime component and must not be left as a fake placeholder.

### 7.2 `runtime/control/`

Owns PortusOS-specific coordination and lifecycle logic. It is not a general file API, source-search engine, patch service, Git wrapper, or second shell.

#### Applications and capabilities

Owns capability registration, discovery, compatibility, health, application-instance relationships, lifecycle integration, and provider references. Application-domain operations remain in application-owned CLIs and APIs.

#### Tasks and managed execution

Owns durable tasks and execution only when work requires persistence, detachment, supervision, reboot continuation, structured progress, policy-mediated elevation, artifact capture, or recovery.

Ordinary builds, tests, scripts, and commands run directly through Codex and the shell.

#### Policy and approvals

Owns effective-policy inspection, bundle/grant coordination, preflight checks, allow/prompt/reject decisions, bounded grants, root-equivalent warnings, non-secret protected-credential references, and audit relationships. Root-only mutation/execution crosses the separate `portus-privd` boundary defined by `docs/POLICY_IMPLEMENTATION.md`; `runtime/control/` must not execute arbitrary privileged shell operations. Raw protected credential behavior remains owned by `docs/SECRETS.md`.

Protected credentials are used through a separate protected provider/broker boundary that owns the reusable value and performs provider-bound authenticated operations. `portusd` may coordinate references and policy but must not become the raw credential store or generic credential-forwarding proxy. Where ordinary SDK/application compatibility requires it, a protected provider may expose a local authenticated provider-compatible proxy whose local credential/session identifier is not a reusable upstream secret.

The agent must not be able to use this surface to grant itself broader authority without an owner-authorised path.

#### Artifacts and delivery

Owns artifact identity, classification, retention, presentation, delivery state, retry, cleanup, and cross-surface relationships. It does not replace ordinary file copy or transfer commands when no artifact workflow is required.

#### Health and recovery

Owns structured health, degraded-state reporting, bounded repair, failure evidence, reconciliation, and recovery entry points.

#### Managed system workflows

Owns transactions where PortusOS adds preflight, expected-state checking, task association, rollback, or remote-safe recovery around native systems such as:

- updates and package changes;
- firewall and network exposure;
- storage and boot changes;
- critical service lifecycle;
- session, display, and workspace coordination;
- capability upgrades.

Underlying native commands remain available and authoritative for direct inspection and recovery.

General file reading, directory listing, source search, project mutation, Git operations, and ordinary command execution are explicitly outside `runtime/control/`.

### 7.3 `runtime/privileged/`

Owns the implementation of the narrowly scoped root-owned `portus-privd` security boundary defined by `docs/POLICY_IMPLEMENTATION.md`.

It owns:

- `/run/portus/priv/use.sock` and `/run/portus/priv/admin.sock` protocol implementations;
- kernel peer-credential authentication;
- root-owned policy loading/validation;
- typed privileged-operation adapters;
- root-equivalent classification enforcement/warnings;
- atomic administrator policy mutation;
- safe privileged-operation audit metadata.

It explicitly does **not** own:

- generic root shell/exec;
- arbitrary root filesystem mutation;
- provider secret storage;
- ordinary user-native operations;
- generic package/service command passthrough.

`portus-privd` is a security boundary service, not a second general Portus control plane.

## 8. Index and registry ownership

The system index and durable registries are related but not interchangeable.

The system index is a refreshable materialised map of installed and live machine state. It is queried on demand and may be rebuilt from native sources, Portus providers, and durable relationships. It must not be injected wholesale into agent context or become the sole control path.

Durable registries store intentional Portus-owned identity, lifecycle, policy association, and relationship data that cannot always be reconstructed from a live scan.

### 8.1 System index

Materializes and relates source-attributed observations of installed capabilities/software, application definitions/instances, process generations, services, graphical sessions, displays, workspaces, window generations, selected devices/mounts, task/project/Codex relationships, provider registrations, and opaque provider resources.

It uses generation-scoped `idx_<UUIDv7>` handles for bounded follow-up while preserving authoritative/source identities. It supports bounded quick views, query, inspection, targeted refresh, rescans, reconciliation, rebuild, evidence-aware correlation, multiple available control paths, and small agent-managed annotations. Native sources, Portus registries and specialised providers remain authoritative for their domains. Detailed requirements are defined in `docs/SYSTEM_INDEX.md`.

### 8.2 Project registry

Tracks project identity, safe path, display name, repository metadata, and policy association.

The filesystem and Git remain authoritative for project contents.

### 8.3 Session registry

Tracks integration metadata such as:

- Codex session or thread ID;
- workspace;
- tmux session;
- last-opened time;
- launcher state;
- archived or recoverable status.

It must not duplicate complete Codex transcripts.

### 8.4 Task registry

Tracks durable work lifecycle independently of terminal attachment.

A task record exists only when OS-level durable work coordination is needed; ordinary Master actions, Codex-only subagents, and Codex-only root sessions do not become Portus tasks automatically.

A task record should be able to reference:

- requester and communication surface;
- project or workspace;
- Codex root session/thread and subagent relationships where applicable;
- agent or process;
- policy and approvals;
- progress and events;
- artifacts;
- final result;
- failure and retry state;
- resume rules.

### 8.5 Artifact registry

Tracks outputs and delivery metadata.

Artifact bytes remain in approved filesystem or application storage. The registry stores identity, path or resource locator, checksum, type, classification, retention, task association, and delivery state.

### 8.6 Capability registry

Tracks installed Portus applications and other registered capability providers.

It should expose identity, version, health, interface discovery, policy profile, lifecycle, and output domains without duplicating application-owned state.

## 9. Adapter ownership

Adapters translate PortusOS-level operations into native systems.

An adapter is not a mandatory route for every native operation in its domain. Agents may use the underlying command directly when no Portus-specific coordination, durable state, policy mediation, rollback, health validation, or recovery behaviour is needed.

### OpenRC adapter

Owns service inspection, start, stop, restart, enablement, runlevel, dependency, and health operations.

It must not contain systemd fallbacks.

### Codex adapter

Owns Portus integration with pinned Codex installation, configuration, root sessions, headless exec/resume/fork, queueing, subagents, goals, compaction, skills, diagnostics, and permission profiles as defined by `CODEX_UPDATES.md`. It must expose/use those native Codex capabilities rather than reimplementing them inside `portusd`.

### tmux adapter

Owns live terminal sessions and attachment. It does not own durable task truth or service supervision.

### i3 adapter

Owns deterministic workspace, window, application launch, and presentation routing through i3 IPC and commands.

### Visual-input adapter

Owns policy-governed screenshots, active-window context, pointer movement, clicks, keyboard input, and visual fallback for applications without adequate structured interfaces.

It must not become the normal browser-control path when PortusBrowser supports the workflow. Screen capture and simulated input are sensitive, task-associated operations with bounded retention and audit requirements.

### Network adapter

Owns inspected and policy-controlled NetworkManager, SSH, firewall, VPN, exposure, and connectivity operations.

### Package adapter

Owns package inspection and controlled update/install operations under the package and supply-chain policy.

### Storage adapter

Owns safe inspection and approved operations for disks, mounts, capacity, backups, snapshots or recovery mechanisms selected by the Linux design.

## 10. Capability integration structure

### 10.1 Integration manifests

PortusOS should maintain installation and integration metadata for supported Portus applications without absorbing their implementation.

A conceptual capability manifest may include:

```yaml
id: portus-browser
version: 1.0.0
capabilities:
  - browser.instances
  - browser.tabs
  - browser.navigation
  - browser.downloads
interfaces:
  cli: /usr/bin/portus-browser
  rpc: /run/portus/browser.sock
policy_profile: networked-application
lifecycle:
  supervisor: openrc
  service: portus-browser
health:
  command: portus-browser health --json
outputs:
  - browser-session
  - downloaded-file
  - screenshot
  - page-result
```

This is illustrative, not a final schema.

### 10.2 Application contract rules

A registered application must define:

- stable identity and version;
- capability domains;
- concise interface discovery;
- required permissions;
- configuration and state ownership;
- service or process lifecycle;
- health check;
- structured errors;
- output and artifact behaviour;
- upgrade and compatibility policy;
- recovery and uninstall behaviour.

### 10.3 PortusBrowser integration

PortusBrowser is required in PortusOS. It consists of an agent-facing CLI, local Broker, browser native-messaging host, Chromium extension, structured protocol, snapshot/action system, events, settings profiles, recipes, and browser-domain policy. It owns supported browser discovery, connected sessions, profiles, windows, tabs, page snapshots, element targeting, navigation, page actions, browser automation, downloads, browser credential/session behaviour, and browser-domain state.

Website allowlists, navigation restrictions, browser-action policy, and profile isolation belong to PortusBrowser. PortusOS must not maintain a duplicate website-policy layer.

PortusOS owns application lifecycle, task association, the outer process/filesystem perimeter, artifact registration and delivery integration, health, and recovery. Browser implementations remain provider applications rather than PortusOS control-plane code, but the first-ISO installation profile requires Chromium as the Codex ChatGPT-authentication/default-browser surface and P15 reference browser. Additional supported browsers remain optional.

### 10.4 PortusWindow integration

PortusWindow owns lightweight windows, image or web presentation, ephemeral navigation, and visual lifecycle.

PortusOS owns display routing, task association, content policy, sensitive-output timeout, health, and recovery integration.

## 11. Build-system ownership

`portusos-build/` owns everything required to produce, install, validate, and release the OS.

### Composition and package contract

`contracts/build.yaml` is the whole-image composition root. It references the package, component, system, installer, ISO, validation and generated-output schema contracts without copying their values.

`packages/packages.yaml` is the human-maintained first-release package/source intent. Exact Artix identities are explicit `linux-verified` values. `packages/packages.lock.yaml` is generated only after verified Artix repository resolution; it hash-links back to `packages.yaml` and is not a hand-authored source file.

`runtime/install/install.toml` remains authoritative for Portus-owned installed paths, identities and services, and the PortusBrowser runtime integration files remain authoritative for browser protocol/lifecycle facts. The build contracts reference those sources instead of creating parallel manifests.

Installer/build modules must not maintain hidden independent package, service, storage or validation lists.

### ISO profile

`iso/profile.yaml` owns the image-level artools/Calamares composition and the native x86_64 Linux outer-host + isolated verified Artix build-context requirement. `portusos-build/artix/bootstrap.json` owns the locked Artix live seed/rootfs extent and bounded OverlayFS context layout; `scripts/artix/context.py` verifies, mounts, probes and tears down that generated context without installing Artix packages into the outer host. `BUILD_HARNESS.md` owns repeated build orchestration; `configs/first-live.json` is the canonical iterative request. The deterministic layer is implemented through `portus-build`, `builder/layout.yaml`, `installer/responsibilities.yaml`, `iso/artools-profile/adapter.yaml`, the tracked `rootfs/overlay/` and `packages/local/` source boundaries, plus `build-iso.sh`/`.ps1`. Release-critical Artix/package/installer facts remain fail-closed `linux-verified` inputs until established by the isolated verified Artix path.

### Installer

Owns:

- environment preflight;
- safe network setup;
- disk selection and destructive-operation confirmation;
- boot, encryption, storage, and filesystem creation;
- users and authentication;
- package and external artifact installation;
- Portus runtime installation;
- post-install invocation;
- final validation and cleanup;
- secret-safe logs.

### Post-install

Owns target-system configuration after filesystems and packages exist.

Modules should be ordered, idempotent where practical, independently testable, and explicit about failure.

### Root filesystem overlay

Owns static files installed into the target system.

Generated configuration should be produced from schemas and implementation logic rather than committed as unexplained generated output.

### Build outputs

Generated ISO, metadata, checksums, logs, and validation evidence belong under `out/` and are not source inputs for a clean build.

## 12. Installed system structure

P16 now concretizes the current Portus-owned installed subset. `runtime/install/install.toml` is authoritative for the host-safe staging inputs and exact first-party targets; later Artix/package/ISO work may add verified files but must not silently move these canonical paths or invent ownership for entries still marked `linux-verified`.

```text
/
  workspace/
    <user>/
      master/
        AGENTS.md
      projects/
        <project>/

  usr/
    bin/
      portus-os
      portusd
      portus-privd
      portus-apid
      portus-api
      portus-auth
      portus-bootstrap
      portus-master
    share/
      portus/
        policy/
          actions.toml
          bundles/
        integrations/
          portus-browser/
            integration.toml
        openrc/
          templates/
            portusd.in
            portus-privd.in
            portus-apid.in

  etc/
    codex/
      skills/
        protected-api/
          SKILL.md
    portus/
      capabilities/
        protected-api.toml
        portus-browser.toml
      policy/
        policy.toml
        subjects.d/
      protected-api/
        providers.d/
          openai.toml

  var/
    lib/
      portus/
        state/
        protected-api/
    log/
      portus/
        audit/

  run/
    portus/
      portusd.sock                 # runtime-created; parent ownership Linux-verified
      priv/
        use.sock                   # runtime-created by portus-privd
        admin.sock                 # runtime-created by portus-privd
      protected-api/
        use.sock                   # runtime-created by portus-apid
        admin.sock                 # runtime-created by portus-apid
```

The tree deliberately omits speculative files and empty component directories. `/etc/init.d/portusd`, `/etc/init.d/portus-privd`, and `/etc/init.d/portus-apid` are not host-safe staging outputs: only package-owned templates are staged until the verified Artix installation path renders and validates real OpenRC scripts. PortusBrowser likewise has no OpenRC service because its Broker lifecycle is provider-owned/native-host-on-demand.
      sockets/
      pids/
      transient/
```

Rules:

- `/etc/portus/` owns administrator-controlled configuration and system knowledge.
- `/var/lib/portus/` owns durable mutable Portus runtime metadata.
- `/run/portus/` owns transient runtime sockets and process metadata.
- `/var/log/portus/` owns bounded logs and audit output.
- `/workspace/<user>/` owns that Linux user's Master and project workspaces; ownership/modes must prevent cross-user private access by default.
- application-owned state remains in its documented location unless explicitly delegated.
- secrets must not be stored in generic Portus state or logs.

## 13. Configuration precedence and ownership

Configuration needs one owner and explicit precedence.

| Configuration | Owner | Primary consumers |
| --- | --- | --- |
| Product and owner decisions | `docs/PROJECT.md` | maintainers and implementation planning |
| Public documentation and authority map | `DOCUMENTATION.md` | users, maintainers and coding agents |
| First ISO acceptance contract | `docs/ACCEPTANCE.md` | implementation planning, validation, release engineering |
| First ISO executable validation contract | `docs/VALIDATION.md` | validation harness and release engineering |
| Candidate/release artifact contract | `docs/RELEASE.md` | build metadata, checksums, provenance, release engineering |
| Codex baseline | `CODEX_UPDATES.md`, later `/etc/codex/` templates | Codex adapter, installer, doctor |
| Durable knowledge policy | `OKF.md` | knowledge tooling and agents |
| Portus OS CLI contract | `docs/CLI.md` | `portus-os`, shared Rust protocol/client modules, `portusd`, Master Portus, `portus-os-cli` skill, validation |
| Task-routing and managed-execution contract | `docs/TASKS.md` | Master Portus, `portusd`, CLI, task/state/index/provider integrations |
| Capability/provider contract | `docs/CAPABILITIES.md` | provider manifests, `portusd`, CLI, System Index, tasks, provider integrations |
| System Index operational-graph contract | `docs/SYSTEM_INDEX.md` | `portusd`, CLI, index/source adapters, Master Portus, tasks/providers/knowledge integrations |
| Build composition root | `portusos-build/contracts/build.yaml` | builder, installer and validation orchestration |
| Package source contract | `portusos-build/packages/packages.yaml` | build and installer |
| Resolved build package set | `portusos-build/packages/packages.lock.yaml` | release metadata and verification; generated only on Artix |
| System Portus config | `/etc/portus/config/` | CLI, services, adapters |
| Administrator-enforced OS policy | `/etc/portus/policy/` plus Linux groups/ACLs/sudo/polkit/helpers | installer, root/admin tools, `portusd`, adapters |
| Codex-specific enforced requirements | `/etc/codex/requirements.toml` | Codex delegated-agent/trust/feature boundaries |
| Capability manifests | `/etc/portus/capabilities/` | discovery, lifecycle, health |
| User preferences | user configuration path selected later | presentation and allowed preferences |
| Project policy | repository-owned config within allowed boundaries | project agents and tasks |
| Operational state | `/var/lib/portus/state/` | CLI, Master Portus, health, recovery |
| Transient IPC | `/run/portus/` | local processes only |

User or project configuration may narrow behaviour or choose allowed preferences. It must not weaken administrator-enforced requirements.

## 14. Runtime startup and supervision

The architecture must distinguish interactive continuity from service supervision.

### System startup

```text
firmware
  -> GRUB
  -> initramfs and encrypted storage
  -> kernel
  -> OpenRC
  -> required system services
  -> Portus runtime services that must survive logout
  -> network and remote recovery availability
```

### Local graphical startup

The current direction is:

```text
local TTY login
  -> conditional startx
  -> D-Bus/elogind-compatible session setup
  -> i3
  -> presentation and authentication support
  -> Alacritty / MasterPortus attachment
```

The exact sequence must be validated without systemd user services.

### Interactive Master session

```text
portus-bootstrap
  -> create or attach tmux session: MasterPortus
  -> MASTER window
  -> portus-master
  -> start or resume Codex in /workspace/<user>/master
```

The `AUTH` window remains a human-operated secret and login surface.

### Long-running components

A process that must survive logout or reboot must not depend solely on tmux. It should be supervised by OpenRC or another explicitly selected durable runtime mechanism.

Durable task records coordinate work across supervised processes, Codex sessions, tmux, and applications.

## 15. Communication structure

### SSH

SSH is required and must remain independent of Master Portus.

A remote user should be able to:

- obtain a normal shell according to policy;
- inspect health;
- attach to tmux;
- use recovery commands;
- disable or restart broken Portus components.

SSH login must not automatically start the graphical session.

### Portus MCP + Secure MCP Tunnel

The first ISO bundles Portus MCP and OpenAI `tunnel-client` as the canonical optional remote **agent-access** path after Master/Codex login. They do not replace `portusd`, Portus policy/state/task ownership, or SSH recovery.

Lifecycle and scope:

- both are present in the live ISO and installed system;
- setup/start is explicit and user/session-owned, not an OpenRC machine service;
- the canonical Portus MCP listener is loopback-only at `http://127.0.0.1:8789/mcp`;
- the default MCP project is the current Master workspace `/workspace/<user>/master`, not `/`;
- the bundled PortusOS MCP policy disables subagents and grants the remote main agent a bounded non-root development/diagnostic command surface;
- the Secure MCP Tunnel is outbound-only; no inbound listening firewall exception is required for the tunnel path;
- tunnel ID/API credentials are supplied at runtime and are never embedded into the ISO;
- `portus-tunnel-setup` is the first-ISO one-command setup/start path; `portus-mcp-local` starts the local MCP server without a tunnel.

The live setup may hold the tunnel API key in the Master-owned process environment for that session. That is **not** a protected/non-exportable credential claim. A later persistent protected-secret integration must use the `portus-apid` boundary rather than silently writing the key into Master-readable startup files.

### Future adapters

Messaging, web, or mobile adapters should live behind a common interaction boundary.

They must not each own separate task truth, agent identity, approval logic, or artifact storage.

Adapter implementations should integrate with:

- authenticated identity;
- policy and approval;
- task creation and resumption;
- attachment ingestion;
- artifact delivery;
- event and audit recording;
- outage and retry behaviour.

## 16. Graphical and presentation structure

The graphical stack is a capability host and recovery surface.

Expected roles include:

- system and diagnostic display;
- Master Portus terminal;
- AUTH;
- browser surfaces;
- PortusWindow and ephemeral display;
- project visual applications;
- emergency file manager and editor.

Workspace names or numbers should be policy-driven rather than deeply hardcoded, but i3 IPC must allow deterministic placement and focus.

The system should prefer commands such as `i3-msg` and application APIs over simulated input.

Sensitive output should support concealment, closure, or screen locking according to policy.

## 17. Knowledge structure

System knowledge:

```text
/etc/portus/knowledge/
  index.md
  ... OKF concepts
```

Master knowledge:

```text
/workspace/<user>/master/knowledge/
  index.md
  machine/
    index.md
    ... stable machine-specific OKF concepts
  ... other Master OKF concepts
```

The `machine/` scope is the durable learned map of this particular installation: stable paths, capabilities, conventions, aliases, relationships, and machine-specific operational knowledge. It is not the live process/window/application inventory; current state remains in the system index and authoritative providers.

Project knowledge:

```text
/workspace/<user>/projects/<project>/knowledge/
  index.md
  ... OKF concepts
```

`AGENTS.md` routes Codex to relevant bundles. The Master `AGENTS.md` specifically establishes the device-manager/orchestrator role, routes detailed `portus-os` use to the `portus-os-cli` skill, and directs Master Portus to consult/update durable machine knowledge without copying the inventory into the prompt. OKF files do not replace operating instructions, skills, task state, or normal long-form documentation.

Caches or indexes derived from OKF must be disposable and reproducible.

## 18. Artifact structure

Task outputs should be stored in approved task, project, or application locations and registered centrally enough for discovery and delivery.

A possible direction is:

```text
/var/lib/portus/state/artifacts/<artifact-id>.json
/workspace/<user>/projects/<project>/.portus/artifacts/<task-id>/...
```

The final paths require a data-retention and project-portability decision.

Artifact metadata should not duplicate large file contents. It should reference them and record checksums, type, classification, retention, and delivery state.

## 19. Event and audit structure

A common event vocabulary should allow components to report events such as:

```text
task.started
task.progress
task.completed
task.failed
approval.required
approval.received
artifact.created
artifact.delivered
artifact.delivery_failed
capability.started
capability.degraded
capability.failed
system.degraded
system.recovered
```

P8 concretely uses common durable `significant_events` rows in `portus.db` plus a bounded in-memory `portusd` wake-up hub. The hub is not authoritative history: stream delivery rereads durable sequence data, so missed wake-ups remain recoverable until explicit retention pruning. A message bus must not be introduced without a demonstrated requirement. `docs/EVENTS_AUDIT.md` is authoritative for the detailed event/stream contract.

Security audit records are separate from `portus.db` and currently use bounded JSONL under `/var/log/portus/audit/`. Audit records must be:

- bounded and rotated;
- protected from ordinary project writes;
- clear about actor, task, operation, target, policy decision, and result;
- redacted of secrets;
- usable for recovery and review.

## 20. Security boundaries

### Administrator/root identity

Root or another explicit administrator-controlled path owns machine security policy and recovery. It can create/remove Master Portus users, grant/revoke permission bundles, make granular policy changes, administer protected credential infrastructure, and recover all user state.

### Master Portus identity

Each Master Portus instance runs as a real non-root Linux UID. It is not placed inside a general-purpose workspace sandbox. It normally receives broad practical access to resources owned by or granted to that user.

Linux ownership/modes/ACLs, administrator policy, sudo/polkit/helpers, device/socket permissions, network policy and provider policy define its authority.

### Delegated agents

Project/task/inspection agents may receive narrower Codex/Bubblewrap/worktree sandboxes. Their authority cannot exceed the owning Master user's authority or administrator-enforced policy.

### Cross-user filesystem/state

Policy and Unix permissions must distinguish:

- root-owned system configuration;
- user-scoped Master/project workspaces;
- user homes and application state;
- Portus runtime state associated with a principal;
- task temporary space;
- shared resources explicitly granted to multiple users;
- protected credential infrastructure;
- artifact delivery staging.

One Master Portus UID must not read another user's private files or user-scoped Portus records by default.

### Network

Network policy distinguishes ordinary outbound Internet use, local/private network access, listening services, firewall mutation, browser/provider access, remote-control surfaces, MCP access, and external artifact delivery. Installer permission bundles may configure broad domains; root/admin may refine them granularly.

### Elevation

Portus-owned unattended root mutation crosses the peer-authenticated typed `portus-privd` service defined by `docs/POLICY_IMPLEMENTATION.md`; unrestricted passwordless sudo or generic root command execution is not the normal Portus path. Native root/sudo/polkit administration remains available for human repair. Any permission that effectively permits arbitrary root takeover is classified root-equivalent and excluded from the normal Broad profile.

### Protected credentials

Protected reusable credentials are governed by `docs/SECRETS.md`; first-ISO class-specific labeling is governed by `docs/CREDENTIAL_CLASSES.md`, and the reference API provider/attack proof by `docs/PROTECTED_API_PROVIDER.md` / `docs/PROTECTED_API_VALIDATION.md`. Broad ordinary access to files owned by the Master UID does not automatically include protected credential export.

A credential stored in an ordinary Master-readable file or exported through an API available to that UID must be treated as readable by Master Portus. A same-user keyring is not automatically a hard boundary. Secret-backed providers must expose approved operations without a general reusable-secret retrieval path, and claimed protection requires negative tests.

Raw reusable secrets must not enter generic Portus state, OKF, task records, artifact metadata, audit payloads, ordinary logs, or generic runtime IPC.

## 21. Allowed and disallowed interaction directions

### Allowed

- Master Portus may use Codex, native commands, application-owned interfaces, and Portus-specific capabilities within effective policy.
- Project agents may operate within assigned projects and task spaces.
- The CLI may delegate to reviewed adapters and application contracts.
- Applications may emit structured results, health, events, and artifacts.
- Direct Linux commands may be used for novel workflows, diagnostics, and recovery when policy permits.
- The live ISO may inspect and repair the installed system through documented procedures.

### Disallowed

- Duplicating Codex file reading, source search, editing, patching, shell execution, project navigation, or Git behind Portus wrappers.
- Requiring Portus-managed execution for ordinary commands that Codex can run directly.
- A separate top-level agent tool for every anticipated workflow.
- Hardcoding every Portus application operation into Master Portus.
- Treating tool names as the primary security boundary.
- Allowing an agent to alter its own enforced policy without owner authority.
- Treating tmux as a service supervisor or complete task database.
- Treating screenshots and mouse simulation as the default control path.
- Requiring a visible terminal for ordinary agent command execution.
- Requiring the GUI for ordinary administration or recovery.
- Making a cloud service mandatory for local operation.
- Storing secrets in task, audit, knowledge, or ordinary log files.
- Runtime dependence on private development history or generated build outputs.
- systemd commands, units, timers, user services, or login assumptions in the required implementation.
- Duplicating package lists across independent installer modules.
- Automatically executing code merely because an OKF concept references it.

## 22. Validation structure

Validation must cover more than successful installation.

### Static and contract validation

- schemas;
- package source policy;
- forbidden systemd assumptions;
- shell and code checks;
- capability manifests;
- policy precedence;
- secret scanning;
- OKF profile validation.

### Build validation

- clean ISO build;
- pinned source resolution;
- checksums and metadata;
- boot artifacts;
- installer inclusion;
- recovery utilities.

### Installation validation

- safe disk selection;
- encryption and boot;
- required packages;
- OpenRC services;
- Codex installation and sandbox;
- Portus runtime and CLI;
- graphical presentation environment;
- SSH recovery.

### Runtime validation

- broad primitive behaviour;
- policy denial and approval;
- task and session continuity;
- capability discovery;
- application health;
- artifact registration and delivery;
- machine-readable diagnostics;
- crash-loop and degraded-state handling.

### Failure and recovery validation

- agent failure;
- Codex failure;
- task-process failure;
- broken OpenRC service;
- malformed configuration;
- network outage;
- provider outage;
- disk-full condition;
- interrupted update;
- failed graphical startup;
- reboot and power-loss recovery;
- live-ISO repair;
- independent SSH recovery.

### VMware validation

- boot and install;
- networking;
- display resizing;
- clipboard/shared-folder policy;
- suspend/resume;
- shutdown integration;
- minimum and recommended resource profiles;
- multi-agent resource behaviour.

## 23. Generated artifacts

The public build harness owns the concrete output directory implementation. Generated candidate/release filenames must follow `docs/RELEASE.md`; this document does not maintain a parallel naming convention.

Expected logical output structure:

```text
portusos-build/out/
  iso/
    <candidate ISO filename from docs/RELEASE.md>

  checksums/
    SHA256SUMS

  metadata/
    build-metadata.json
    release-metadata.json          # public RC/final only
    packages.lock.yaml             # resolved Artix package/source evidence copy/reference

  logs/
    build.log
    install-validation.log
    runtime-validation.log
    recovery-validation.log

  validation/
    <candidate-id>/
      validation-report.json
      validation-report.md
      tests/
      imported/
```

Release metadata records source revision, builder identity, package/source resolution, external component pins, artifact digest, authority revisions and validation linkage according to `docs/RELEASE.md`.
    capability-results.json
    vmware-results.json
```

Release metadata should record source commit, build environment, package resolution, external artifact hashes, Codex version, schema versions, build time, and validation status.

Generated outputs must not be required as source inputs for a clean build.

## 24. Implementation staging rule

The structure distinguishes target architecture from initial implementation scope.

The first implementation may use simple files and direct CLI adapters where a daemon, database, event bus, or RPC service is not yet justified.

However, it must preserve these boundaries from the start:

- CLI is core;
- policy is separate from capability enumeration;
- task state is separate from tmux;
- application contracts are separate from application internals;
- artifact metadata is separate from task conversation;
- OpenRC supervision is separate from interactive Codex sessions;
- recovery remains independent of Master Portus;
- local operation does not depend on an external control service.

Complex infrastructure should be added only when the operating requirement is demonstrated. Missing structure must not be hidden by conflating unrelated responsibilities.

## 25. Public documentation consequence

The public repository documents current product, build, validation, security, release, and operational contracts rather than private implementation sequencing.

Forward changes should therefore:

1. update the specialized public authority when a product or technical contract changes;
2. update machine-readable build/runtime contracts in the same change when they encode that decision;
3. keep `DOCUMENTATION.md` and `README.md` aligned with the public source tree;
4. keep private planning, historical build journals, and maintainer-only release staging outside the public repository;
5. preserve `docs/ACCEPTANCE.md`, `docs/VALIDATION.md`, and `docs/RELEASE.md` as the acceptance/publication boundary.
