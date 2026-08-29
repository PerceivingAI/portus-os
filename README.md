# PortusOS

PortusOS is a privacy-focused, agent-operated Linux operating system built on **Artix Linux** and **OpenRC**.

The core idea is simple: a user gives intent to an agent, and the agent operates the machine through native commands, code, APIs, local IPC, and policy-governed system controls. The desktop remains available for authentication, visual applications, monitoring, administration, and recovery.

> **Release target:** PortusOS **0.1.0 Technical Preview** for **x86_64 VMware**.
>
> **Current status:** the native Artix build path and hardened repository-closure pipeline are implemented and host-safe tested. The next milestone is a canonical native build that passes the completed closure gate, reaches `buildiso`, proves the tracked `memtest86+` correction, and produces the first candidate ISO. **No ISO has been accepted or released yet.**

## What PortusOS contains

The first Technical Preview is designed around:

- **Artix Linux + OpenRC**;
- **GRUB UEFI** boot;
- **LUKS2 + LVM** encrypted installed storage;
- `linux-lts` as the normal/live kernel and `linux` as an installed alternate;
- **X11 + i3 + Alacritty** for the graphical session;
- a non-root **Master Portus** operating user;
- **Codex** as the primary agent runtime;
- the Rust-native **`portus-os` CLI** and **`portusd`** runtime;
- policy, task, capability, system-index, artifact, health, event, and audit subsystems;
- protected reusable API credentials through **`portus-apid`**;
- **PortusBrowser** integration;
- optional Portus MCP + OpenAI `tunnel-client` remote access;
- **Calamares** installation with Portus-owned encrypted-storage integration;
- deterministic build, validation, provenance, checksum, and release tooling.

The exact release boundary is defined by [`docs/ACCEPTANCE.md`](docs/ACCEPTANCE.md) and [`KNOWN_LIMITATIONS.md`](KNOWN_LIMITATIONS.md).

## Architecture at a glance

```text
User intent
   │
   ▼
Master Portus (non-root Linux user)
   │
   ├── Codex + native Linux tools
   │     files, Git, code, shell, ordinary commands
   │
   └── PortusOS control plane
         policy / approvals / tasks / capabilities
         system index / artifacts / health / audit
         │
         ├── portusd
         ├── portus-os CLI
         ├── portus-privd
         ├── portus-apid
         ├── OpenRC / Linux
         └── providers such as PortusBrowser
```

PortusOS does **not** try to wrap every normal Linux or Codex capability. It adds durable coordination, policy, security boundaries, system knowledge, and managed operations where the underlying tools do not already provide them. See [`docs/CONTROL_PLANE.md`](docs/CONTROL_PLANE.md).

## Repository map

| Path | Purpose |
| --- | --- |
| `runtime/` | Rust runtime, CLI, protocol, policy, state, tasks, health, artifacts, privilege and provider integration |
| `runtime/install/` | Installed PortusOS files, identities, service declarations and policy templates |
| `portusos-build/` | ISO, installer, package, component, validation and machine-readable build contracts |
| `scripts/artix/` | Verified Artix context preparation and native ISO construction |
| `scripts/` | Repository checks, tests and build orchestration |
| `scripts/oss/` | Public-repository, dependency, provenance and redistribution audits |
| `scripts/release/` | Release checksum signing and verification tooling |
| `docs/` | Public architecture, security, build, validation and operational authorities |
| `oss/` | Release policy, third-party inventory and public schemas |
| `release/` | Public release-key and verification structure |

For the full documentation authority map, start with [`DOCUMENTATION.md`](DOCUMENTATION.md). Coding agents should also read [`AGENTS.md`](AGENTS.md).

## Build and test

### Source checks

```bash
bash scripts/check.sh
```

### Full repository tests

```bash
bash scripts/test.sh all
```

### Build/installer contract tests

```bash
bash scripts/test.sh build-skeleton
```

Test categories and host-safe boundaries are documented in [`docs/TESTING.md`](docs/TESTING.md).

### Canonical ISO build

Read [`BUILD_REQUIREMENTS.md`](BUILD_REQUIREMENTS.md) and [`BUILD_HARNESS.md`](BUILD_HARNESS.md) first.

On the supported x86_64 Linux build host:

```bash
bash portusos-build/build-iso.sh --config portusos-build/configs/first-live.json
```

Keep the build attached to an interactive terminal. Native construction performs an owner-authorized sudo handoff; PortusOS never reads or records the sudo password.

Every build attempt is recorded under:

```text
portusos-build/out/builds/<run-id>/
```

A successful ISO build is **not** release acceptance. It only creates bytes that can become a candidate for the validation process.

## Build security model

The ISO pipeline is deliberately fail-closed. Before `buildiso` may start, the native builder must prove that:

1. `system`, `world`, and `galaxy` were synchronized from a selected official HTTPS Artix mirror into a run-owned repository database;
2. the exact package closure was resolved and persisted before acquisition;
3. every required archive is present and SHA-256 verified, with resumable cache reuse and bounded mirror failover;
4. the frozen repository databases and local `file://` repository are read-only;
5. the local-only repository independently resolves to the same frozen package closure;
6. the locked pacman configuration points only at the run-local repository;
7. package bytes are reverified after local validation;
8. the persistent cache has restored outer ownership and both source/consumer cache views are read-only;
9. a live pre-exec guard rechecks the package ledger, package hashes, repository DB hashes, pacman-config hash, repository links, and mount state immediately before launching `buildiso`.

If even one frozen package remains unresolved, `buildiso` is unreachable.

The detailed implementation and evidence contract live in [`BUILD_HARNESS.md`](BUILD_HARNESS.md), [`docs/ISO_BUILD_INSTALLER.md`](docs/ISO_BUILD_INSTALLER.md), and [`docs/PACKAGE_POLICY.md`](docs/PACKAGE_POLICY.md).

## Validation and release flow

PortusOS separates **building**, **accepting**, and **publishing** an ISO:

```text
source + contracts
      │
      ▼
canonical ISO build
      │
      ▼
immutable candidate + SHA-256
      │
      ▼
ISO-01 ... ISO-38 on VMware
      │
      ▼
accepted candidate
      │
      ▼
public release gates + signed checksums
      │
      ▼
PortusOS 0.1.0 Technical Preview
```

The reference validation VM is **4 vCPU / 8 GiB RAM / 80 GiB disk / UEFI**. A separate minimum-profile test uses **2 vCPU / 4 GiB RAM / 40 GiB disk**.

- [`docs/ACCEPTANCE.md`](docs/ACCEPTANCE.md) — what the first ISO must prove.
- [`docs/VALIDATION.md`](docs/VALIDATION.md) — how ISO-01 through ISO-38 are executed and recorded.
- [`docs/RELEASE.md`](docs/RELEASE.md) — immutable candidate and release identity.
- [`docs/PUBLIC_RELEASE_POLICY.md`](docs/PUBLIC_RELEASE_POLICY.md) — public versioning, signing and support policy.

## Key documentation

### Product and architecture

- [`docs/PROJECT.md`](docs/PROJECT.md) — product definition and durable scope.
- [`docs/STRUCTURE.md`](docs/STRUCTURE.md) — repository and system ownership boundaries.
- [`docs/CONTROL_PLANE.md`](docs/CONTROL_PLANE.md) — what PortusOS should and should not implement.
- [`docs/RUNTIME.md`](docs/RUNTIME.md) — `portusd` and runtime architecture.
- [`docs/CLI.md`](docs/CLI.md) — `portus-os` CLI contract.

### Security and authority

- [`docs/POLICY.md`](docs/POLICY.md) — Master Portus and administrator authority.
- [`docs/POLICY_IMPLEMENTATION.md`](docs/POLICY_IMPLEMENTATION.md) — concrete policy/elevation implementation.
- [`docs/PROTECTED_API_PROVIDER.md`](docs/PROTECTED_API_PROVIDER.md) — reusable API-secret protection boundary.
- [`docs/CREDENTIAL_CLASSES.md`](docs/CREDENTIAL_CLASSES.md) — credential treatment by class.
- [`SECURITY.md`](SECURITY.md) — vulnerability reporting.

### OS and installation

- [`docs/ISO_BUILD_INSTALLER.md`](docs/ISO_BUILD_INSTALLER.md) — Artix/artools and Calamares architecture.
- [`docs/BOOT_STORAGE_RECOVERY.md`](docs/BOOT_STORAGE_RECOVERY.md) — GRUB, LUKS, LVM and recovery.
- [`docs/HARDWARE.md`](docs/HARDWARE.md) — VMware-first hardware boundary.
- [`docs/OPENRC_SERVICES.md`](docs/OPENRC_SERVICES.md) — service ownership.
- [`docs/GUI_SYSTEM.md`](docs/GUI_SYSTEM.md) — graphical session architecture.

See [`DOCUMENTATION.md`](DOCUMENTATION.md) for the complete index instead of treating this README as an exhaustive documentation list.

## Design principles

- **Agent-first, not agent-only.** Humans retain direct administrative and recovery authority.
- **Policy defines authority.** Tool scarcity is not used as access control.
- **Non-root Master.** Master Portus is capable but is not root.
- **Local-first.** Normal operation does not require a central cloud control service.
- **Use native interfaces.** CLI, APIs and provider contracts are preferred over GUI automation.
- **Visual automation is fallback.** Screenshot/pointer/keyboard control exists for applications without better interfaces.
- **Protect reusable secrets.** Credentials are kept outside ordinary agent-readable state.
- **Fail closed.** Unknown release-critical facts do not silently become assumptions.
- **Evidence-bound releases.** Accepted releases are tied to exact source, ISO bytes, hashes, validation evidence and provenance.

## Licensing and project policy

PortusOS-owned source is licensed under the **Apache License 2.0**. Third-party components retain their upstream licences and redistribution obligations. See [`LICENSE`](LICENSE) and [`THIRD_PARTY.md`](THIRD_PARTY.md).

External pull requests and code contributions are not accepted. Reproducible bugs and documentation defects may be reported through GitHub Issues after public launch. Security issues must use the private reporting path in [`SECURITY.md`](SECURITY.md).

The Technical Preview has no LTS promise, response-time SLA, broad bare-metal support guarantee, Secure Boot guarantee, or guaranteed update cadence. See [`KNOWN_LIMITATIONS.md`](KNOWN_LIMITATIONS.md).
