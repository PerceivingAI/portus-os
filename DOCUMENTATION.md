# PortusOS Public Documentation Index

This file is the public navigation map for the PortusOS repository.

It intentionally contains only public product, build, validation, security, release, and operational authorities. Private development plans, historical build journals, internal readiness checklists, and maintainer-only staging material are not part of this repository.

## Start here

1. `README.md` — public project orientation, build/test entry points and repository layout.
2. `docs/PROJECT.md` — product definition and durable scope decisions.
3. `docs/ACCEPTANCE.md` — first Technical Preview acceptance boundary.
4. `BUILD_REQUIREMENTS.md` — supported build-host prerequisites.
5. `BUILD_HARNESS.md` — canonical repeated ISO build flow.
6. `docs/ISO_BUILD_INSTALLER.md` — ISO and installer architecture.
7. `docs/VALIDATION.md` — ISO-01 through ISO-38 validation procedure/evidence contract.
8. `docs/RELEASE.md` — candidate and public-release artifact contract.
9. `KNOWN_LIMITATIONS.md` — current support and capability limits.

## Product and control-plane authorities

- `docs/PROJECT.md` — product intent and overall public scope.
- `docs/CONTROL_PLANE.md` — what PortusOS owns versus native Linux/Codex/application interfaces.
- `docs/STRUCTURE.md` — repository, runtime and installed-system ownership boundaries.
- `docs/RUNTIME.md` — `portusd`, local IPC and runtime failure boundaries.
- `docs/CLI.md` — public `portus-os` CLI contract.
- `docs/STATE.md` — Portus-owned SQLite state and migration rules.
- `docs/TASKS.md` — task identity/lifecycle and managed execution.
- `docs/CAPABILITIES.md` — provider/capability registration and lifecycle.
- `docs/SYSTEM_INDEX.md` — source-attributed operational machine graph.
- `docs/ARTIFACTS.md` — artifact identity, integrity and lifecycle.
- `docs/HEALTH_RECOVERY.md` — health, diagnostics and recovery.
- `docs/EVENTS_AUDIT.md` — durable significant events and security audit.

## Security and authority

- `docs/POLICY.md` — Master Portus and administrator authority model.
- `docs/POLICY_IMPLEMENTATION.md` — concrete policy/elevation implementation.
- `docs/SECRETS.md` — reusable-secret classification and protection boundary.
- `docs/PROTECTED_API_PROVIDER.md` — protected API provider architecture.
- `docs/PROTECTED_API_VALIDATION.md` — adversarial validation for the protected provider.
- `docs/CREDENTIAL_CLASSES.md` — credential-class treatment.
- `SECURITY.md` — public vulnerability-reporting policy.

## OS, build and installation

- `BUILD_REQUIREMENTS.md` — native x86_64 Linux build-host requirements.
- `BUILD_HARNESS.md` — configuration-driven build orchestration and evidence.
- `portusos-build/README.md` — machine-readable build-contract layout.
- `docs/ISO_BUILD_INSTALLER.md` — Artix/artools and Calamares integration.
- `docs/BOOT_STORAGE_RECOVERY.md` — GRUB/LUKS/LVM/initramfs/recovery contract.
- `docs/HARDWARE.md` — kernel, firmware and VMware-first hardware boundary.
- `docs/PACKAGE_POLICY.md` — package/source provenance rules.
- `docs/OPENRC_SERVICES.md` — OpenRC service/session ownership.
- `docs/GUI_SYSTEM.md` — X11/i3/Alacritty session architecture.
- `docs/UPDATES_RECOVERY.md` — managed update and recovery rules.

## Agent and integration surfaces

- `CODEX_UPDATES.md` — Codex implementation baseline and selected build pin.
- `OKF.md` — durable knowledge policy.
- `docs/PORTUS_BROWSER.md` — PortusBrowser integration boundary.
- `TUNNEL_INSTRUCTIONS.md` — optional Portus MCP/tunnel-client setup.
- `runtime/integrations/master/AGENTS.md` — installed Master-agent integration instructions.

## Testing, release and provenance

- `docs/TESTING.md` — repository test categories and host-safe test rules.
- `docs/VALIDATION.md` — release-candidate acceptance evidence.
- `docs/PUBLIC_RELEASE_POLICY.md` — public version, support, issue, security and signing policy.
- `docs/RELEASE.md` — immutable candidate/release artifact contract.
- `THIRD_PARTY.md` — third-party and redistribution inventory entry point.
- `scripts/oss/README.md` — repository/privacy/dependency audit tooling.
- `release/README.md` — public release verification structure.
- `KNOWN_LIMITATIONS.md` — release scope limits.
- `RELEASE_NOTES.md` — Technical Preview release notes.

## Authority order

When public documents overlap, use this order:

1. `docs/PROJECT.md` for product intent and scope.
2. A more specialized public authority for its own domain.
3. `docs/ACCEPTANCE.md` for what the first Technical Preview must prove.
4. `docs/VALIDATION.md` for how acceptance proof is executed and recorded.
5. `docs/RELEASE.md` and `docs/PUBLIC_RELEASE_POLICY.md` for publication and release artifacts.
6. Machine-readable contracts under `portusos-build/`, `runtime/install/`, `runtime/integrations/`, and `oss/` for implementation/build inputs.

Private development history is not an authority in the public repository.