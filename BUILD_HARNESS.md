# PortusOS ISO Build Harness

**Last reviewed:** 2026-08-29T09:40:32Z

**Status:** Authoritative for repeated PortusOS ISO build orchestration, build-configuration semantics, per-attempt logging/metadata, artifact capture, and the handoff from repository source to the native Artix ISO adapter.

**Last reconciled:** 2026-08-29T09:40:32Z

**ISO architecture authority:** `docs/ISO_BUILD_INSTALLER.md`

**Build-environment requirements authority:** `BUILD_REQUIREMENTS.md`

**First-ISO acceptance authority:** `docs/ACCEPTANCE.md`

**Package/source authority:** `docs/PACKAGE_POLICY.md`

**Canonical development build config:** `portusos-build/configs/first-live.json`

---

## 1. Purpose

PortusOS is expected to be built repeatedly while the first ISO is booted in VMware, inspected with Codex, corrected, and rebuilt.

The build process therefore must not depend on a remembered sequence of manual commands.

The supported model is:

```text
tracked build configuration
  -> repository build harness
  -> deterministic preflight + contract/build-plan capture
  -> unprivileged frozen first-ISO payload staging
  -> run-owned staging-evidence.json
  -> bounded sudo adapter
  -> private Artix mount namespace + artools
  -> exactly one ISO artifact
  -> timestamped build ledger + log + hashes
  -> VMware boot / Codex inspection
  -> source fix
  -> next build configuration/run
```

The harness exists so every successful, failed, and blocked build can be traced to the exact configuration, source tree, environment, commands, and artifact involved.

---

## 2. Core rule

For normal ISO iteration, do **not** invoke `buildiso`, `artools`, or a hand-written sequence directly.

Use the repository wrapper with an explicit build configuration:

```bash
./portusos-build/build-iso.sh --config portusos-build/configs/first-live.json
```

PowerShell wrapper:

```powershell
powershell -File portusos-build/build-iso.ps1 --config portusos-build/configs/first-live.json
```

The wrapper delegates to `scripts/build_iteration.py`.

A direct lower-level command is appropriate only while diagnosing or implementing the native adapter itself. It is not a normal reproducible ISO build and must not be substituted for the harness in iteration history or release evidence.

---

## 3. What the build configuration owns

A build configuration is an **invocation specification**. It does not duplicate PortusOS product decisions.

The configuration controls:

- development versus release-candidate build mode;
- preflight depth;
- clean-source requirement;
- the human-readable build/run label;
- the composition contract to consume;
- the ISO profile to consume;
- the deterministic installer/storage plan size used for that build attempt.

The configuration does **not** redefine:

- package lists or package-source policy;
- the first-ISO live capability requirements;
- Master Portus authority;
- Codex/Chromium requirements;
- Portus MCP/tunnel-client requirements;
- storage topology;
- OpenRC service ownership;
- installer semantics;
- validation criteria.

Those facts remain owned by the authority documents and machine contracts referenced by the selected composition/ISO profile.

This separation prevents two competing sources of truth.

---

## 4. Canonical first-live configuration

The tracked first-ISO development configuration is:

```text
portusos-build/configs/first-live.json
```

Its schema is:

```text
portusos-build/schemas/build-config.schema.json
```

Current canonical configuration:

```json
{
  "schema_version": 1,
  "name": "first-live",
  "description": "Canonical iterative build request for the first interactive, connectable PortusOS live ISO.",
  "build": {
    "mode": "development",
    "preflight": "standard",
    "source_cleanliness": "require_clean",
    "label": "first-live"
  },
  "target": {
    "composition_contract": "portusos-build/contracts/build.yaml",
    "iso_profile": "portusos-build/iso/profile.yaml",
    "installer_plan_disk_mib": 81920
  }
}
```

`81920` MiB is the deterministic 80-GiB reference installer/storage plan used while compiling the build plan. It is not the ISO file size and does not itself resize a VMware virtual disk.

---

## 5. Build-configuration fields

### `schema_version`

Must be `1`.

Unknown fields are rejected. A new shape requires an explicit schema/version change rather than silent interpretation.

### `name`

Stable identifier for the build specification.

Allowed characters:

```text
letters, digits, dot, underscore, hyphen
```

Maximum length is 64 characters.

### `description`

Optional non-empty explanatory text.

It is copied into the run's config snapshot and appears in the timestamped build log.

### `build.mode`

Allowed values:

```text
development
release_candidate
```

`development` permits ordinary iterative ISO construction once the native adapter gate is resolved.

`release_candidate` also invokes the release-candidate build gate and therefore requires the whole first-ISO build graph to be release-resolved.

### `build.preflight`

Allowed values:

```text
standard
full
skip
```

`standard` runs:

```text
scripts/check.sh
portus-build-contract tests
portus-build tests
```

`full` additionally runs:

```text
Linux integration suite
hardening suite
```

`skip` bypasses preflight only for a narrowly diagnosed builder/debugging run. The skip is recorded permanently in `build-run.json`; it cannot masquerade as a normal checked build.

### `build.source_cleanliness`

Allowed values:

```text
require_clean
allow_dirty
```

`require_clean` is the normal value and is required for reproducible builds.

`allow_dirty` is development-only. It permits a scratch run while marking the source as non-reproducible and recording the dirty Git status.

A `release_candidate` configuration is invalid unless this field is `require_clean`.

### `build.label`

Optional run-label component. If `null`, the config `name` is used for run naming.

The label changes run identification only; it does not change PortusOS product semantics.

### `target.composition_contract`

Schema v1 requires:

```text
portusos-build/contracts/build.yaml
```

This is the machine-readable first-ISO composition root.

### `target.iso_profile`

Schema v1 requires:

```text
portusos-build/iso/profile.yaml
```

This is the first-ISO live/rootfs/profile contract.

### `target.installer_plan_disk_mib`

Controls the disk size passed to the deterministic `portus-build plan` step for this run.

Allowed schema-v1 range:

```text
40960 .. 1048576 MiB
```

The canonical first-live config uses `81920` MiB, matching the 80-GiB reference profile. The 40-GiB minimum remains separately tested.

---

## 6. Validate a config without building

Before committing a new build specification:

```bash
python -B scripts/build_iteration.py \
  --check-config portusos-build/configs/first-live.json
```

A valid result reports:

- `valid: true`;
- repository-relative config path;
- exact SHA-256 of the supplied config bytes;
- parsed/resolved configuration.

`scripts/check.sh` validates the canonical `first-live.json` automatically.

---

## 7. Creating another build specification

When a genuinely different iteration specification is needed:

1. copy `portusos-build/configs/first-live.json` to a new tracked JSON file;
2. change only fields allowed by `build-config.schema.json`;
3. validate it with `--check-config`;
4. commit the config with the source changes it is intended to build;
5. invoke the harness with that config.

Example:

```bash
cp portusos-build/configs/first-live.json portusos-build/configs/first-live-full.json
# edit build.preflight to "full" and name/label accordingly
python -B scripts/build_iteration.py --check-config portusos-build/configs/first-live-full.json
./portusos-build/build-iso.sh --config portusos-build/configs/first-live-full.json
```

Do not add package lists or ad-hoc shell fragments to a build config. If the actual distro contents must change, update the owning machine contract/source and let the config continue to reference it.

---

## 8. Run identity

Every invocation gets a new UTC-derived run ID:

```text
<YYYYMMDDTHHMMSSZ>-<git-short-sha>-<dev|rc>-<label>
```

Example:

```text
20260828T123214Z-7ce4e5e64e1e-dev-first-live
```

If two runs would receive the same identifier, an ordinal suffix is added. Existing run directories are never reused.

The most recent run ID is written to:

```text
portusos-build/out/builds/LATEST
```

---

## 9. Per-run directory

Every attempt creates:

```text
portusos-build/out/builds/<run-id>/
```

The directory contains:

```text
build-config.json       exact byte-for-byte snapshot of the supplied config
build-run.json          structured run identity/status/config/source/host/steps/failure/artifact
build.log               timestamped combined harness + preflight + builder log
preflight.json           mandatory environment readiness results with stable PASS/WARN/BLOCK check IDs
inputs.json             every tracked repo file with Git mode/blob, byte size and SHA-256
contract-report.json    machine-readable composition-resolution state when reached
build-plan.json         deterministic plan compiled using the config's disk-size value when reached
staging-evidence.json   frozen run-owned payload/profile/native-work manifest when staging is reached
native-build-result.json native namespace/buildiso/artifact evidence when native construction is reached
native-cleanup.json      post-namespace proof of zero current-run mount/process/loop leakage and scratch removal
RUN_SHA256SUMS          checksums binding all available run records and ISO when one exists
artifacts/              harness-owned native builder output directory
```

Failed and blocked attempts keep the same evidence structure. They are not discarded because regression analysis often depends on failed runs.

---

## 10. Build-config snapshot and log binding

The exact supplied configuration bytes are copied to:

```text
<run>/build-config.json
```

The harness computes its SHA-256 before the build and records that digest in `build-run.json`.

At the beginning of `build.log`, the harness writes a UTC timestamped header containing:

- source config path;
- config SHA-256;
- resolved config contents.

`RUN_SHA256SUMS` also includes `build-config.json`.

Therefore a run cannot be meaningfully described without identifying the exact configuration that produced it.

---

## 11. Source/input capture

For every run the harness records:

- full Git revision;
- short revision;
- branch;
- clean/dirty status;
- complete dirty-status lines when applicable;
- whether the source qualifies as reproducible;
- every tracked file's Git mode;
- every tracked file's Git blob identity;
- every tracked file's byte size;
- every tracked file's SHA-256.

This is intentionally broader than hashing only files currently believed to affect the ISO. If a later regression originates in an overlay, launcher, policy file, build helper, or other tracked source, the historical input ledger remains useful.

---

## 12. Outer-host capture

`build-run.json` records the outer orchestration host:

- OS/system;
- architecture;
- kernel release;
- distribution ID/version/name;
- Git version;
- Rust version;
- Cargo version.

The outer host is **not** the PortusOS distribution authority. Artix package/rootfs/ISO construction remains owned by the isolated Artix build context defined in `docs/ISO_BUILD_INSTALLER.md`.

### Mandatory environment preflight

The Linux wrapper first performs the irreducible bootstrap guard from `BUILD_REQUIREMENTS.md`: Python 3.10+ and Git must exist before a traceable run can be allocated. A failure there is reported directly because the ledger cannot yet exist.

Every native build attempt runs `scripts/build_environment_preflight.py` regardless of `build.preflight`. `BUILD_REQUIREMENTS.md` is authoritative for what it checks. The checker writes `preflight.json`, emits stable `PASS`/`WARN`/`BLOCK` results into `build.log`, records its SHA-256 in `build-run.json`, and returns before configurable repository tests/native construction when a hard requirement is blocked. `build.preflight=skip` skips repository test depth only; it cannot disable this environment gate.

For the isolated-Artix checks, the preflight consumes the repository-owned bootstrap contract at `portusos-build/artix/bootstrap.json` and the persistent readiness evidence produced by `scripts/artix/context.py`. The verified ISO seed can be checked without privilege. A healthy prepared state means the verified Artix upper/evidence exists **and no Artix mount is exposed in the outer VM**. If preparation is required, preflight reports `sudo python -B scripts/artix/context.py prepare`; that action now mounts the Artix seed/OverlayFS only inside a private mount namespace, persists the upper/evidence, and exits with no global mount tree left behind.

As of the first completed L2 preparation on 2026-08-28, that evidence proves Artix/x86_64 identity, pacman/keyring operation, synchronized Artix repositories, `artools-base`/`artools-iso`/`artools-pkg` 0.39.1-1, `/usr/bin/buildiso`, `basestrap`, and `artix-chroot`. The repository now also has a locked `artools` mapping, a real `portus` profile workspace, all selected official-Artix package identities, and exact selected OpenRC service names/runlevels. These are no longer placeholder readiness checks.

---

## 13. Harness execution sequence

The harness executes these phases in order:

```text
1. parse + strictly validate config
2. read Git/source identity
3. allocate never-overwritten timestamped run ID
4. snapshot + hash exact config bytes
5. initialize build-run.json and timestamped build.log
6. hash every tracked repository input
7. run mandatory environment preflight -> preflight.json (includes source-cleanliness policy)
8. stop on any BLOCK; otherwise execute configured repository/test preflight depth
9. generate contract-report.json
10. generate build-plan.json using config target.installer_plan_disk_mib
11. run unprivileged scripts/artix/stage_first_iso.py with a sanitized child environment
12. write/hash staging-evidence.json and the run-owned native workspace
13. invoke portus-build build-iso with PORTUS_BUILD_STAGING_MANIFEST
14. authorize privilege at the native handoff: reuse a valid `sudo -n -v` ticket, otherwise refresh with terminal-owned `sudo -v` only when interactive; noninteractive/no-ticket runs fail closed with exit `78`
15. clone the prepared Artix upper into a fresh run-owned upper/work pair, enter a private mount namespace, and invoke the locked Artix buildiso mapping
16. after the namespace exits, write/validate native-cleanup.json proving zero current-run cross-namespace mount references, zero process root/cwd/fd references, zero newly leaked Artix-seed loop devices, and removal of the run-scoped Artix scratch tree
17. reject native success unless cleanup evidence is PASS, then require exactly one produced .iso and write native-build-result.json
18. compute ISO SHA-256 and size
19. finalize build-run.json + RUN_SHA256SUMS
```

A failure at any stage records the stage, reason, exit code, finish time, and available checksums before returning to the caller.

---

## 14. Native Artix adapter contract

The harness passes the native adapter these environment variables:

```text
PORTUS_BUILD_RUN_ID
PORTUS_BUILD_RUN_DIR
PORTUS_BUILD_ARTIFACT_DIR
PORTUS_BUILD_CONFIG
PORTUS_BUILD_CONFIG_SHA256
PORTUS_BUILD_COMPOSITION_CONTRACT
PORTUS_BUILD_ISO_PROFILE
PORTUS_BUILD_INSTALLER_PLAN_DISK_MIB
PORTUS_BUILD_MODE
PORTUS_BUILD_STAGING_MANIFEST
```

`PORTUS_BUILD_CONFIG` points at the run-owned `build-config.json` snapshot, not a mutable outside copy. `PORTUS_BUILD_STAGING_MANIFEST` points at the checksum-bound `staging-evidence.json` produced by the immediately preceding unprivileged staging step.

The staging/native child processes do **not** receive a copy of the outer process environment. The harness constructs an allowlisted environment containing the required toolchain paths, run-owned `HOME`/cache paths, locale, and only required network/CA transport variables. API keys, access tokens and unrelated application state are not build inputs.

For the current reference builder, `registry.npmjs.org` is forced into both `NO_PROXY` and `no_proxy` inside that sanitized environment. The measured HTTPS proxy path returned invalid/expired certificate-chain failures for locked npm tarballs, while direct HTTPS to the same exact registry artifacts validated successfully. This is a routing correction, not a TLS relaxation: npm/pnpm certificate verification remains enabled, and `strict-ssl=false`, `curl -k` or equivalent bypasses are forbidden.

The staging manifest binds the run-owned `artools` workspace, chroot/output roots and artifact directory to the frozen Portus/Codex/PortusBrowser/Portus-MCP/tunnel-client payload evidence. The native adapter rejects unbounded paths, mismatched source revision, incomplete payloads, a changed `artools` mapping, or a pre-existing ISO in the artifact directory.

Installer staging is part of that frozen manifest. `scripts/artix/stage_first_iso.py` copies exactly one verified-gap custom Calamares job, `portus-storage`, into the run-owned **live overlay only**. Runtime code lands below `portus/live-overlay/usr/lib/calamares/modules/portus-storage/`; the preflight/prepare/finalize instance configurations and stock-module overrides land below `portus/live-overlay/etc/calamares/modules/`; the Portus settings sequence replaces `/usr/share/calamares/settings.conf`; the static storage-input page lands at `/usr/share/portus-installer/portus-storage-input.qml`; and the live-only launcher/polkit rule land at `/usr/local/bin/portus-install` and `/etc/polkit-1/rules.d/90-portus-installer.rules`. `staging-evidence.json` records the module/input-view identity, `live_only: true`, and SHA-256 of every canonical installer source/config/live file. The staging self-test rejects omission of those files or accidental placement in `portus/root-overlay`.

The two runtime Python files are a single placement unit: Calamares `3.4.2` exposes the job module directory through `libcalamares.job.working_path` but does not add it to `sys.path` before `py::eval_file()` executes `main.py`. The adapter therefore resolves `storage_engine.py` from that explicit working path. Staging must preserve their sibling placement; the storage regression test fails if that loader contract regresses.

The installer input surface uses the signed stock `notesqml` view module, not a second custom Portus module. The static root-owned QML page enumerates supported whole disks from `/sys/block`, masks and confirms independent owner/recovery credentials, requires explicit erase acknowledgement, and arms GlobalStorage only through its own Continue action. The immediately following `portus-storage@preflight` is non-destructive: it verifies the block device, writability, minimum size, unmounted state and free Portus mapper/VG names, then computes the current size-bound plan hash. `portus-storage@prepare` immediately revalidates that hash before its first destructive command. Missing/bypassed input fails before disk writes, and success or failure clears the runtime target/hash/credential/armed keys.

Installer-runtime secrets remain intentionally outside the build manifest. The target device and credentials exist only while Calamares runs in the live system and must not be supplied through build configuration or inherited environment. `calamares.storage-implementation` remains release-unresolved only for signed-Calamares live loading, destructive blank-VM behavior, cleanup/logging evidence and installed-boot validation—not because the runtime input/settings path is still missing.

Privilege is owner-authorized at the native handoff. `portus-build build-iso` first checks `sudo -n -v`; a still-valid cached ticket is reused. If that ticket has expired and stdin is an interactive terminal, the builder runs `sudo -v` exactly at the privileged handoff so long unprivileged preflight/staging work cannot consume the authorization window. `sudo` owns the terminal prompt directly: Portus code never requests, reads, stores, or echoes the password. After a successful refresh, the bounded helper still runs with `sudo -n`. If no ticket is valid and no interactive terminal is attached, the build fails closed with unresolved exit `78` rather than attempting a prompt.

The privileged helper first clones the canonical prepared Artix `upper` into a fresh run-owned Artix context below that build's native work root. The run-owned context has its own OverlayFS `upper` and `workdir`; the canonical prepared `upper/work` pair is never mounted directly by native `buildiso`. The helper then creates a private mount namespace, changes mount propagation to private **before the first native mount**, mounts the verified Artix seed plus the run-owned OverlayFS and required `proc`/`sys`/`dev`/repo views inside that namespace, bind-mounts only the run-owned native workspace at `/run/portus-build`, and invokes the locked `buildiso` mapping.

Native `artools` execution is also required to be unattended. The first real package-install phase proved the selected `artools 0.39.1-1` path invokes `basestrap -GMc`, and the live pacman argv included `--noconfirm`; the visible `:: Proceed with installation? [Y/n]` line was therefore transaction display, not an input requirement. Before every future `buildiso` invocation, `scripts/artix/context.py` verifies that `buildiso` does not enable basestrap `-i`, that it still calls basestrap through its locked argument array, and that the installed basestrap default still appends `--noconfirm` outside interactive mode. The helper then launches `buildiso` with stdin connected to `/dev/null`. An Artix/tooling change that breaks those invariants fails before package mutation instead of silently introducing an operator prompt into the canonical build.

Preflight now scans `/proc/*/mountinfo` in addition to the caller's mount table. References to the historical canonical prepared `upper/work` pair are reported as `artix.historical-mount-namespaces` WARN because a long-lived service can retain an inherited mount even after normal `findmnt` is clean; those references are safe for new builds only because native construction uses a unique cloned upper/work pair. Any surviving mount namespace below **any** `portusos-build/work/native-runs/.../artix-context` is `artix.native-run-mount-leaks` BLOCK, and a reference to the newly allocated current-run path is separately `artix.current-run-mount-cleanliness` BLOCK.

After the private namespace exits, the privileged outer helper scans all readable process mount tables plus process root/cwd/fd references for the current run path, compares Artix-seed loop devices before versus after the namespace action, removes the run-scoped Artix context only when those checks are clean, and writes `native-cleanup.json`. The harness validates that evidence and refuses to call a native builder successful unless cleanup status is `pass`, the after-reference lists are empty, no new seed-backed loop device survived, and `scratch_removed=true`. Cleanup evidence is retained and checksum-bound on native failure too when the privileged helper was reached. On user interruption, the builder remains in the terminal foreground process group so kernel-delivered SIGINT reaches the cargo/sudo/unshare/buildiso tree regardless of UID; the harness then performs a bounded wait and reaps the top-level builder before finalizing the interrupted run. Any surviving run-scoped mount namespace is blocked by the next preflight.

A host-visible canonical Artix mount is still a preflight failure, not normal build state. `teardown` remains only as a recovery path for legacy/global mounts from the earlier discovery workflow. A normal VM reboot can retire historical inherited namespaces, but correctness never depends on rebooting or killing unrelated host services.

The observed `artools 0.39.1-1` `make_bootfs()` single-kernel assumption is handled by a narrow Portus compatibility shim. The locked adapter declares `live_boot_kernel_package: linux-lts`. After the prepared Artix upper is cloned and mounted as a run-owned overlay, but before `buildiso` executes, `scripts/artix/context.py` verifies the exact upstream `make_bootfs()` source seam and patches **only that run-owned `/usr/bin/buildiso`**. The patched live-boot path copies `vmlinuz-linux-lts` to the ISO kernel filename, resolves exactly one matching `usr/lib/modules/*/pkgbase == linux-lts` version from the temporary bootfs view, and writes that version only to temporary `bootfs/usr/src/linux/version` so upstream `prepare_initramfs_mkinitcpio()` builds a matching LTS live initramfs. The installable rootfs is never rewritten for this workaround and continues to contain both `linux-lts` and `linux`. A changed upstream seam, unsupported live-kernel selection, missing LTS image, or zero/multiple matching LTS module trees fails closed. Successful native evidence records the compatibility kind, selected live kernel package, and source/patched `buildiso` SHA-256 values.

The harness rejects a successful native-builder exit that produces zero or multiple ISO artifacts.

---

## 15. Build status and exit behavior

### Success

```text
exit 0
status = succeeded
```

The run contains exactly one ISO, with SHA-256 and size recorded.

### Usage/config error

```text
exit 64
```

Examples include invalid/missing config, unknown fields, invalid labels, unsupported contract references, or an invalid release-candidate cleanliness policy.

### Dirty-source block

```text
exit 65
status = blocked
```

Returned by the mandatory environment preflight when a config requires a clean tree and the source is dirty. `preflight.json` is still retained and may show additional environment blockers/warnings from the same inspection.

### Unresolved target/build fact

```text
exit 78
status = blocked
```

Used for mandatory environment blockers other than dirty source and for unresolved native-build prerequisites. Historical clean checkpoint `7aca133` reached **`32 PASS / 2 WARN / 0 BLOCK`** before the native adapter existed. The first complete cold staging run from clean revision `4e01a55` completed `first-iso-staging` successfully for Portus, Codex, PortusBrowser, Portus MCP and tunnel-client, then returned `78` only because the owner sudo ticket was not current. A direct `portus-build build-iso` without a run-owned staging manifest also intentionally returns `78`. Later native runs proved the private Artix execution path and motivated run-scoped upper/work isolation plus cross-namespace pre/post cleanup verification. Native execution is no longer an unresolved capability. The `artools 0.39.1-1` memtest bootfs expectation is now resolved in tracked inputs as the official Artix `memtest86+` package; only a new native run can prove that correction through final bootfs/ISO construction.

### Other build failure

```text
status = failed
```

The underlying nonzero exit code is preserved where possible.

### User interruption

A terminal SIGINT during any ordinary pre-native harness step is recorded on that step with exit `130`, then finalized through the normal failed-run path so `build-run.json` cannot remain falsely `running`. A native-build interruption keeps the stronger foreground-process-group cleanup path: SIGINT reaches the cargo/sudo/unshare/buildiso tree, the top-level builder is reaped, available cleanup evidence is retained, and the run is recorded as failed at the native-build stage with exit `130`.

---

## 16. Reading a failed or blocked build

The harness prints the run ID plus the direct paths to:

```text
build-run.json
build.log
```

Start with `build-run.json` to identify:

- configuration SHA;
- source revision;
- failure stage;
- exact command/exit code;
- preflight state.

Then inspect `build.log` for the timestamped command output.

For regressions between two iterations, compare:

```text
build-config.json
preflight.json
inputs.json
contract-report.json
build-plan.json
build.log
```

in that order. This normally distinguishes a deliberate spec change, source change, resolution/environment change, and runtime builder failure quickly.

---

## 17. Build iteration versus candidate acceptance

A successful harness run is a **build iteration**, not automatically an accepted candidate.

The intended loop is:

```text
config + source
  -> build harness
  -> ISO
  -> VMware boot
  -> Codex/PortusOS investigation
  -> fixes
  -> next build
```

When an ISO is worth freezing for formal acceptance, use the candidate tooling and evidence flow described by `docs/VALIDATION.md` and `docs/RELEASE.md`.

Candidate initialization copies/fixes the accepted candidate inputs and begins ISO-01..ISO-38 evidence collection. Build-run history remains useful provenance/debugging context but does not substitute for candidate acceptance evidence.

---

## 18. Security rules

Build configs are source-controlled orchestration data and must contain **no reusable credentials or secrets**.

Do not put any of the following in a config:

- API keys;
- tunnel runtime keys;
- Codex/session credentials;
- passwords;
- private signing keys;
- SSH private keys;
- secret-bearing environment assignments.

The config schema intentionally has no generic environment-variable, arbitrary-command, or free-form secret field.

Credentials needed after boot are provisioned through their own runtime/setup boundaries, not baked into an ISO build specification.

---

## 19. Current implementation boundary

The config-driven iteration harness and **native Artix execution path are implemented**. The repository knows the verified isolated context, `artools 0.39.1-1` interface, `portus` profile layout, fixed `buildiso` arguments, selected package set, OpenRC service identities, live-session path, expected ISO output naming, run-scoped privilege boundary, and cleanup proof contract.

The current implementation frontier is **resilient exact-package acquisition for the repository closure, then late bootfs/final-ISO construction**, not adapter discovery, payload staging, sudo handoff, native process invocation, repository refresh, or dependency resolution. Public run `20260829T060019Z-3afadb080c36-dev-first-live` reached `linux-lts` live-initramfs generation before exposing the missing memtest boot input; that package contract is corrected. Run `20260829T063320Z-658f8230fa32-dev-first-live` demonstrated stale prepared pacman metadata against newer rolling mirrors. The closure implementation then moved repository synchronization and resolution ahead of `buildiso`. Canonical run `20260829T072729Z-4164361b115a-dev-first-live` empirically proved that front half: it freshly synchronized `system`/`world`/`galaxy`, captured their database hashes, and resolved an exact 667-package closure with about 1469.51 MiB to download. It failed during the first bulk `pacman -Sw` prefetch when multiple mirrors timed out or terminated TLS transfers, so `buildiso` never started. `repository-closure.json` correctly recorded failure, the outer harness classified the stage as `repository-closure`, and native cleanup passed.

The supported progression is therefore:

```text
config/source capture
  -> mandatory environment preflight
  -> locked component input gates
  -> configured repository tests
  -> contract report + build plan
  -> run-owned Artix workspace/profile/payload staging
  -> native-boundary Artix repository metadata refresh/freeze
  -> exact package dependency closure
  -> resilient exact-package acquisition + verified-cache completion proof
  -> native buildiso inside the verified Artix context
  -> rootfs/livefs + installed dual-kernel initramfs
  -> linux-lts live initramfs
  -> bootfs assembly [memtest86+ dependency now tracked; native proof pending]
  -> squashfs/final ISO construction
  -> exactly one ISO under PORTUS_BUILD_ARTIFACT_DIR
  -> ISO SHA-256 + final run evidence
```

The memtest correction is now locked: official Artix `memtest86+ 7.20-2` was inspected directly and supplies `/boot/memtest86+/memtest.bin`, so the package is included in both the package contract and `packages-boot`. `portus-build` rejects an `artools 0.39.1` boot profile that omits it. Do not replace this with an untracked file or manual build-root mutation; the next canonical native run is the proof that the tracked dependency closes this seam.

The repository-closure correction is fail-closed in `scripts/artix/context.py`. After cloning and mounting the run-owned Artix context, but before `buildiso`, the helper forces a fresh synchronization of the locked stable `system`/`world`/`galaxy` repositories; copies and hashes those databases into a run-owned closure tree; resolves the complete package graph from an empty per-run pacman database; acquires the exact package files into `portusos-build/cache/artix-packages`; verifies them against repository-provided SHA-256 values; builds a local run snapshot; independently re-resolves that snapshot; then remounts the verified cache read-only before `buildiso`. Run `20260829T072729Z-4164361b115a-dev-first-live` proved fresh synchronization and exact graph resolution but exposed a weakness in the acquisition step: one monolithic `pacman -Sw` transaction over a roughly 1.47-GiB/667-package cold closure was vulnerable to several slow or prematurely terminating mirrors. The persistent cache retained substantial successfully downloaded payload, while the failure evidence reported `packages: []` and zero cache progress because those fields were committed only after the whole prefetch succeeded. A0 closes the first half of that observability gap: immediately after exact resolution and explicit-target completeness checks, the full resolved package graph is defensively copied into `repository-closure.json` before acquisition begins, and a host-safe regression proves a simulated later acquisition failure retains those identities. A1 now removes cross-mirror repository-DB ambiguity: the helper parses active HTTPS servers from the official Artix mirrorlist in order, probes each candidate by synchronizing all three locked repositories into the empty per-run database through a generated single-server pacman configuration, records the first successful anchor and all attempts, hashes the resulting DBs, and makes the per-run sync directory read-only before resolution. No later network refresh uses that frozen DB path. A2 now replaces the network-side monolithic prefetch with deterministic byte-bounded acquisition. Missing/corrupt identities are sorted by the frozen closure identity and partitioned under a 192 MiB download ceiling; a single package larger than the ceiling is permitted only as a one-package oversize batch. Each batch is requested as exact `repository/package` targets against the A1 anchor with pacman dependency checks disabled twice (`-dd` semantics), so the batch cannot expand a new dependency graph. After every attempt, completed batch files are SHA-256 verified, only verified identities leave the pending set, and aggregate batch/completion/pending counts are written back to `repository-closure.json`; a failed batch may therefore retain verified payload for later reuse while still failing the run closed. A3 now supplies controlled bounded mirror failover for those batches: the A1 anchor is attempt 1, followed by at most three distinct active HTTPS mirrors in official mirrorlist order. Every attempt uses a generated one-server pacman config while reading the unchanged frozen DB; the pacman 7.1 command is download-only `-Sw` with doubled `--nodeps`, exact `repository/package` targets, and no refresh flag. SHA-valid payload completed before a failed attempt is retained, corrupt/full or `.part` payload is removed before fallback, and if pacman returns nonzero after all bytes happen to match the frozen SHA the same identities require a clean transaction on a later mirror rather than being promoted from hash evidence alone. Attempt records are persisted and a passing closure must prove anchor-first bounded mirror ordering, contiguous attempts, zero pending files and a successful terminal attempt for every downloaded batch. A4 now makes the persistent cache explicitly resumable across runs. Immediately after the new run freezes its package graph, the adapter audits every expected archive in `portusos-build/cache/artix-packages` against that run's exact SHA-256 identity. Matching complete archives are recorded as reused and removed from the network-pending set; mismatched complete archives and their stale detached signatures are removed; stale `.part`/`.sig.part` payload for closure identities is removed; and missing identities remain pending for A2/A3. The audit is persisted before acquisition, and a passing closure must prove that its reused/pending filename sets are disjoint, complete over the frozen graph, count-consistent and reflected by each final package's `cached_before` flag. The current failed-run cache contains 13 complete candidate archives and no `.part` files, but no historical file is trusted by name/version alone: reuse requires a hash match to the newly frozen closure. A5 now closes the per-package observability gap. `repository-closure.json` carries one deterministic current-state record for every frozen package (`pending`, `reused_verified`, `downloaded_verified`, `corrupt_removed`, or `failed`), its size, verification/acquisition flags and last batch/attempt/mirror/failure class. Aggregate `progress_summary` data records resolved/verified/pending package and byte totals plus per-state totals, so a failed run can accurately preserve a statement such as `667 resolved / 143 verified / 524 pending`. Mirror-attempt evidence now records requested/verified/pending filenames and bytes as well as mirror, batch, result, removal evidence and a normalized failure class. The outer ledger validates the state/byte arithmetic for current failed records and requires complete zero-pending progress evidence for PASS. A6 now closes the broader failure-classification boundary. The closure adapter tracks six explicit substages (`mirror-selection`, `repository-sync`, `resolution`, `acquisition`, `cache-verification`, `local-validation`) and writes a structured failure object containing substage, normalized cause, bounded detail and relevant mirror/batch context. Acquisition stderr remains visible to the operator but is also captured so timeout/TLS EOF/404/signature diagnostics are machine-classifiable. The outer build stage remains `repository-closure`, while its reason includes the validated substage/cause. A7 now closes the final buildiso security boundary. Repository DBs synchronized through A1 are copied into a separate immutable snapshot; the actual `file://` repository is not constructed until the complete frozen package graph is present, SHA-valid and represented as verified with zero pending identities. The helper then independently resolves the local-only repository, restores persistent-cache ownership, remounts that cache read-only, and marks an explicit `buildiso_gate` PASS. `buildiso` is invoked only through a guard that rechecks that in-memory proof, and the outer ledger independently requires the same gate. Host-safe regressions cover mirror failover, batch failure/resume, cache reuse, corrupt-file removal, frozen-DB immutability, partial-progress evidence and direct proof that one unresolved package prevents the buildiso runner from being called. The next required proof is a canonical native run of the completed A0-A7 closure path. The later whole-graph `pacman -Sw` is local-only validation after all archives are already cached and does not perform the former 1.47-GiB network prefetch. Only a fully acquired and SHA-verified closure may be exposed to the local-only `buildiso` phase.

Blocked and failed runs remain valid build-history records and should remain inspectable. The config-driven entry point remains the normal interface; callers must not learn or maintain a separate manual `buildiso` sequence. The privileged cleanup contract removes the run-scoped Artix context and proves no leaked mounts/process references/seed loops, but failed `artools` construction may still leave root-owned run workspace/chroot data for diagnosis. Reclaim such failed-run disk usage only after proving no live references and preserving the run ledger/evidence required for regression analysis.
