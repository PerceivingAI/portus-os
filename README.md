# PortusOS

PortusOS is a privacy-focused, secure, agent-operated Linux operating system based on **Artix Linux** and **OpenRC**.

The system is designed around a simple operating model: the user communicates intent to an agent, and the agent operates the computer primarily through code, command-line interfaces, APIs, local IPC, and policy-governed system controls. The graphical desktop is primarily a presentation, authentication, visual-application, monitoring, and recovery surface.

## Release status

The first public target is **PortusOS 0.1.0 Technical Preview** for **x86_64 VMware**.

**Current build frontier (2026-08-29):** native Artix construction is implemented and the repository-closure gate has now been exercised by canonical run `20260829T072729Z-4164361b115a-dev-first-live`. That run passed preflight, tests, staging and the owner-authorized sudo handoff; freshly synchronized the stable `system`/`world`/`galaxy` databases; recorded their hashes; and resolved an exact 667-package closure totaling about 1.47 GiB of downloads. It then failed **before `buildiso`** during the initial bulk `pacman -Sw` prefetch because multiple Artix mirrors were too slow or terminated TLS transfers. The gate correctly emitted `repository-closure.json` with `status: fail`, classified the run as `repository-closure` failure, prevented construction from starting with an incomplete package universe, and cleanup again passed. This proves the stale-prepared-database defect has been moved out of `buildiso`. The resolved graph is now persisted to `repository-closure.json` immediately after resolution and before any package download starts, so later acquisition failures can no longer collapse the evidence back to `packages: []`. The remaining acquisition hardening is bounded resumable exact-package fetching, controlled mirror selection/failover, and accurate partial verified-cache progress. The tracked `memtest86+` correction remains native-unproven because this run never reached `buildiso`. No candidate ISO has been produced or accepted yet.

This repository contains the PortusOS source, build harness, installer integration, machine-readable contracts, validation tooling, release tooling, and public technical documentation. The Technical Preview remains subject to the acceptance and release gates documented in `docs/ACCEPTANCE.md`, `docs/VALIDATION.md`, and `docs/RELEASE.md`.

PortusOS-owned source is licensed under the **Apache License 2.0**. Third-party components retain their own upstream licences and redistribution obligations. See `LICENSE` and `THIRD_PARTY.md`.

External pull requests and code contributions are not accepted. Reproducible bugs and documentation defects may be reported through GitHub Issues after the repository is published. Security issues must use the private path described in `SECURITY.md`.

## What the first Technical Preview includes

The current first-release architecture includes:

- Artix Linux with OpenRC;
- x86_64, VMware-first validation;
- GRUB UEFI boot;
- LUKS2 + LVM encrypted installed storage;
- `linux-lts` as the normal/live kernel and `linux` as an installed alternate kernel;
- X11 + i3 + Alacritty;
- a non-root **Master Portus** operating user governed by administrator-owned policy;
- Codex as the primary agent runtime;
- Chromium-based browser support for Codex authentication and browser workflows;
- PortusBrowser integration;
- the Rust-native `portus-os` CLI and `portusd` runtime;
- policy, task, capability, system-index, artifact, health, event, and audit subsystems;
- protected reusable API credentials through the `portus-apid` boundary;
- optional Portus MCP + OpenAI `tunnel-client` remote access;
- Calamares-based installation with Portus-owned encrypted-storage integration;
- deterministic build, validation, provenance, checksum, and release tooling.

The exact validated scope is bounded by `docs/ACCEPTANCE.md` and `KNOWN_LIMITATIONS.md`.

## Build PortusOS

Read these first:

- `BUILD_REQUIREMENTS.md` — build-host prerequisites and environment gates;
- `BUILD_HARNESS.md` — canonical build orchestration and evidence model;
- `docs/ISO_BUILD_INSTALLER.md` — ISO and installer architecture.

The canonical development ISO request is:

```bash
bash portusos-build/build-iso.sh --config portusos-build/configs/first-live.json
```

Keep the canonical build attached to an interactive terminal. Privileged native construction is owner-authorized at the point it begins: a valid cached sudo ticket is reused, otherwise `sudo` prompts there directly. Noninteractive execution without a valid ticket fails closed instead of prompting. Portus never reads or records the sudo password.

The build intentionally fails closed when required source, package, Artix-context, sudo, staging, or validation prerequisites are not satisfied. A successful ISO build is not by itself a release acceptance result.

## Validate the source tree

Run the standard repository checks:

```bash
bash scripts/check.sh
```

Run the complete Rust workspace test suite:

```bash
bash scripts/test.sh all
```

Run the build/installer contract suite:

```bash
bash scripts/test.sh build-skeleton
```

See `docs/TESTING.md` for the test categories and `docs/VALIDATION.md` for the VMware/ISO acceptance evidence model.

## Repository layout

```text
runtime/          PortusOS Rust runtime, CLI, policy, state, task and integration source
portusos-build/   ISO, installer, package, component and machine-readable build contracts
scripts/artix/    verified Artix-context preparation and native ISO construction helpers
scripts/oss/      repository, dependency, provenance and redistribution audit tooling
scripts/release/  checksum signing and public release verification tooling
docs/             public architecture, security, validation and operational documentation
oss/              public release-policy, third-party inventory and schemas
release/          public release-key and verification structure
```

`DOCUMENTATION.md` is the public documentation index and authority map.

## Core architecture documents

- `docs/PROJECT.md` — product definition and scope.
- `docs/ACCEPTANCE.md` — first Technical Preview acceptance boundary.
- `docs/CONTROL_PLANE.md` — PortusOS control-plane scope and non-duplication rule.
- `docs/STRUCTURE.md` — repository/runtime ownership boundaries.
- `docs/RUNTIME.md` — `portusd` and runtime architecture.
- `docs/CLI.md` — `portus-os` CLI contract.
- `docs/POLICY.md` and `docs/POLICY_IMPLEMENTATION.md` — Master/admin authority and concrete privilege boundary.
- `docs/SECRETS.md` and `docs/PROTECTED_API_PROVIDER.md` — reusable-secret protection model.
- `docs/STATE.md` — durable mutable-state architecture.
- `docs/TASKS.md` — task and managed-execution architecture.
- `docs/CAPABILITIES.md` — provider/capability integration.
- `docs/SYSTEM_INDEX.md` — operational machine graph.
- `docs/ARTIFACTS.md` — artifact registration and lifecycle.
- `docs/HEALTH_RECOVERY.md` — health, degradation and recovery.
- `docs/EVENTS_AUDIT.md` — significant events and security audit.
- `CODEX_UPDATES.md` — Codex implementation baseline.
- `OKF.md` — durable knowledge policy.

## System, build and installation documents

- `docs/ISO_BUILD_INSTALLER.md`
- `docs/BOOT_STORAGE_RECOVERY.md`
- `docs/HARDWARE.md`
- `docs/PACKAGE_POLICY.md`
- `docs/OPENRC_SERVICES.md`
- `docs/GUI_SYSTEM.md`
- `docs/UPDATES_RECOVERY.md`
- `docs/PORTUS_BROWSER.md`
- `TUNNEL_INSTRUCTIONS.md`

## Release and security documents

- `SECURITY.md`
- `THIRD_PARTY.md`
- `docs/PUBLIC_RELEASE_POLICY.md`
- `docs/RELEASE.md`
- `KNOWN_LIMITATIONS.md`
- `RELEASE_NOTES.md`

Public release authenticity uses `SHA256SUMS` signed with OpenSSH SSHSIG Ed25519 as defined by `docs/PUBLIC_RELEASE_POLICY.md` and `docs/RELEASE.md`. The private release-signing key is never part of this repository or the ISO build tree.

## Design principles

PortusOS follows several hard boundaries:

- **Agent-first, not agent-only.** Humans retain direct recovery and administrative authority.
- **Policy defines authority.** Tool scarcity is not used as a substitute for access control.
- **Non-root Master.** Master Portus is powerful but is not root.
- **Local-first.** Normal local operation does not depend on an external control service.
- **Structured controls first.** CLI/API/provider interfaces are preferred to GUI automation.
- **Visual automation is fallback.** Screenshot, pointer and keyboard control exist for applications that lack better interfaces.
- **Secrets stay out of ordinary agent state.** Reusable protected credentials use dedicated boundaries and negative export tests.
- **Builds fail closed.** Unresolved release-critical facts are explicit rather than silently guessed.
- **Releases are evidence-bound.** A release is tied to exact source, exact ISO bytes, hashes, validation evidence and provenance.

## Support boundary

The first public release is a Technical Preview with best-effort support for the current public final release only. There is no LTS promise, response-time SLA, broad bare-metal support guarantee, Secure Boot guarantee, or guaranteed update cadence.

See `KNOWN_LIMITATIONS.md` for the explicit boundary.