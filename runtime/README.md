# PortusOS Runtime Workspace

This directory contains the first-party native PortusOS software stack.

The first-party runtime is implemented as one Rust workspace. External wire contracts remain language-neutral JSONL; Rust is an implementation choice for Portus-owned components, not a requirement for independent providers such as PortusBrowser.

## Workspace crates

- `portus-protocol` — shared wire envelopes, IDs, principals, semantic errors, health, pagination and provider-reference types.
- `portus-client` — bounded JSONL framing and synchronous Unix runtime client.
- `portus-state` — SQLite state, migrations, provider/index/task/event/health/artifact persistence and principal filtering.
- `portus-provider` — strict provider-manifest parsing and trusted reconciliation.
- `portus-index` — finite Linux/application/OpenRC/i3/X11 observation parsing and correlation for the System Index.
- `portus-task` — durable task transitions, execution relationships and narrow managed-process behavior.
- `portus-audit` — bounded allowlisted JSONL security-audit sink, separate from generic runtime state.
- `portus-policy` — strict policy/action/bundle/subject resolver and administrator-mutation semantics.
- `portus-privd` — narrow root privilege boundary; no generic root shell surface.
- `portus-protected-api` — shared protected-provider definition, protocol and secret-safety contract.
- `portus-apid` — protected API provider daemon and credential store/upstream boundary.
- `portus-api` — direct protected API client.
- `portus-auth` — root-run protected credential provisioning/rotation client.
- `portus-health` — health/recovery classification, restart budgets and native read-only resource probes.
- `portus-artifact` — canonical artifact hashing, registration preparation, reconciliation and exact-target cleanup semantics.
- `portus-master` — Master workspace/bootstrap/launcher logic plus `portus-bootstrap` and `portus-master` binaries.
- `portus-visual` — policy-controlled screenshot/pointer/keyboard fallback contracts.
- `portus-browser-integration` — narrow compatibility, health, native-messaging and provider-resource integration for PortusBrowser.
- `portus-install` — host-safe installed-stack staging contract driven by `runtime/install/install.toml`.
- `portus-build-contract` — whole-image source-contract validator for the `portusos-build/` graph.
- `portus-build` — deterministic build/installer/validation/candidate tooling used by the public ISO harness.
- `portusd` — general Portus coordination/state daemon.
- `portus-os` — public Rust CLI for status, doctor, index, tasks, capabilities, policy, artifacts and health.

## Runtime boundaries

The workspace intentionally preserves these ownership rules:

- SQLite state is not a secrets store.
- The audit sink is separate from ordinary runtime state and logs.
- Policy resolution is separate from privileged execution.
- `portus-privd` exposes typed privileged actions rather than arbitrary root command execution.
- Protected reusable API credentials are owned by the protected-provider boundary and are not exported through `portusd`.
- PortusBrowser remains an independent provider; PortusOS does not copy browser-domain page/session state into its generic runtime.
- Visual automation is a fallback, not the default control path.
- `portus-install` stages only into an explicit target root and does not mutate the development host's real `/etc`, `/usr`, `/var`, or `/run`.
- A successful host build or unit test does not represent installed-system or release acceptance.

See `docs/RUNTIME.md`, `docs/CLI.md`, `docs/POLICY.md`, `docs/SECRETS.md`, `docs/TASKS.md`, `docs/CAPABILITIES.md`, and `docs/ACCEPTANCE.md` for the public subsystem contracts.

## Common entry points

Linux:

```bash
bash scripts/check.sh
bash scripts/build.sh
bash scripts/test.sh unit
bash scripts/test.sh all
```

Windows host-safe equivalents:

```powershell
powershell -File scripts/check.ps1
powershell -File scripts/build.ps1
powershell -File scripts/test.ps1 unit
```

The check entry points perform formatting, compile checking, and Clippy across the workspace. The build entry points compile the workspace. Test categories are defined in `docs/TESTING.md`.