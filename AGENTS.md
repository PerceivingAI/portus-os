# AGENTS.md

This file is the public onboarding entry point for coding agents working in the PortusOS repository.

## Repository purpose

PortusOS is an Artix/OpenRC, Codex-first, agent-operated Linux distribution. The repository contains both the Portus-owned Rust runtime and the public ISO/installer/build/validation machinery.

## Read first

1. `README.md`
2. `DOCUMENTATION.md`
3. `docs/PROJECT.md`
4. `docs/ACCEPTANCE.md`
5. the specialized authority for the subsystem being changed

Do not invent architecture from filenames. Public documentation and machine-readable contracts are intentional authorities.

## Main source areas

- `runtime/crates/` — Rust runtime, CLI, protocol, policy, state, task, health, artifact and integrations.
- `runtime/install/` — Portus-owned installed payload and policy/service templates.
- `runtime/integrations/` — provider/integration contracts.
- `portusos-build/` — ISO, installer, package, component, storage, validation and release schemas/contracts.
- `scripts/artix/` — isolated verified Artix context and native ISO build support.
- `scripts/oss/` — repository/dependency/provenance audit tooling.
- `scripts/release/` — public checksum-signing and verification tooling.

## Standard checks

```bash
bash scripts/check.sh
bash scripts/test.sh all
bash scripts/test.sh build-skeleton
```

Use narrower test categories from `scripts/test.sh` while iterating, but run the relevant broader checks before considering a change complete.

## Canonical ISO build

Read `BUILD_REQUIREMENTS.md` and `BUILD_HARNESS.md` first.

```bash
sudo -v
bash portusos-build/build-iso.sh --config portusos-build/configs/first-live.json
```

Do not bypass preflight, staging, package/source verification, native cleanup verification, or candidate evidence gates to make a build appear successful.

## Engineering rules

- Inspect current implementation and contracts before editing.
- Prefer root-cause fixes over symptoms.
- Preserve fail-closed behavior for authority, secrets, build inputs, validation and release evidence.
- Do not weaken tests merely to accept changed behavior.
- Keep Master Portus non-root and keep administrator/root authority separate.
- Do not add generic shell/file/browser tools to PortusOS when Codex/Linux/application-owned interfaces already provide the capability; follow `docs/CONTROL_PLANE.md`.
- Do not expose reusable secrets in CLI arguments, logs, state, evidence or agent-readable output.
- Keep generated build work under the bounded ignored roots defined by the build harness.
- A successful ISO build is not release acceptance; release acceptance is governed by `docs/ACCEPTANCE.md`, `docs/VALIDATION.md`, and `docs/RELEASE.md`.

## Contributions

This public repository does not accept external pull requests or code contributions. Do not add `CONTRIBUTING.md` or documentation that invites pull requests.