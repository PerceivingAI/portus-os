# PortusOS ISO Build Environment Requirements

**Last reviewed:** 2026-08-29T09:40:32Z

**Status:** Authoritative for the environment prerequisites that must be satisfied before the PortusOS ISO harness may enter native ISO construction.

**Last reconciled:** 2026-08-29T09:40:32Z

**Build-harness authority:** `BUILD_HARNESS.md`

**ISO architecture authority:** `docs/ISO_BUILD_INSTALLER.md`

**First-ISO acceptance authority:** `docs/ACCEPTANCE.md`

**Canonical development build request:** `portusos-build/configs/first-live.json`

---

## 1. Purpose

PortusOS is expected to be built repeatedly while ISO iterations are booted in VMware, inspected with Codex, corrected, and rebuilt.

The build environment therefore must be treated as part of the build contract rather than as undocumented operator knowledge.

This document defines what must exist before the repository-owned harness is allowed to enter the expensive or privileged native ISO-construction phase.

The normal build topology is:

```text
native x86_64 Linux outer host
  -> repository-owned config-driven harness
  -> isolated verified Artix build context
  -> Artix repositories/keyrings + approved pinned external inputs
  -> local Portus package/source staging
  -> artools
  -> PortusOS live ISO
```

The outer host provides orchestration and machine resources. The isolated Artix context is authoritative for Artix package resolution, `artools`, rootfs/profile construction, and final ISO production.

---

## 2. Requirement classes

Build requirements are divided into four classes so failures can be reported precisely.

### 2.1 Harness-bootstrap requirements

These are needed before the Python build harness can run at all.

### 2.2 Outer-host native-build requirements

These are needed on the Linux machine that owns the checkout and orchestrates the build.

### 2.3 Isolated-Artix-context requirements

These are needed inside the Artix build environment that owns package/rootfs/ISO construction.

### 2.4 Input-availability requirements

These ensure all package/source/component inputs required by the selected build config can actually be resolved or are already present in a verified cache.

A missing hard requirement must stop native ISO construction before expensive work begins.

A recommendation or unmeasured capacity threshold may produce a warning rather than a hard failure until real build evidence is available.

---

## 3. Harness-bootstrap requirements

The repository-owned Linux wrapper is:

```bash
./portusos-build/build-iso.sh --config <config>
```

Before the harness can execute, the outer environment must provide:

| Requirement | Current contract |
| --- | --- |
| Operating system | Linux |
| Architecture | `x86_64` |
| Git | required |
| Python | Python 3.10 or newer |
| POSIX shell | `/bin/sh` capable of running the wrapper |
| Bash | required by repository checks and Artix helper scripts |
| Repository checkout | valid Git work tree containing the selected config |
| Config | strict schema-v1 build config accepted by `scripts/build_iteration.py --check-config` |
| Generated-output access | current user can create/write `portusos-build/work`, `cache`, and `out` |

Python 3.10 is the minimum harness language baseline because the harness uses Python 3.10-era syntax and standard-library behavior. A newer supported Python is acceptable.

The build harness must not depend on an IDE, graphical desktop, browser, or VMware merely to start.

Because Python and Git are required before a traceable timestamped run can be allocated, `portusos-build/build-iso.sh` performs a minimal bootstrap guard for Python 3.10+ and Git. Missing bootstrap prerequisites return a direct `BUILD_REQUIREMENTS.md` error before `build-run.json`/`preflight.json` can exist. Once the Python harness starts, all remaining native-build readiness belongs to the mandatory structured environment preflight.

---

## 4. Outer Linux build-host requirements

### 4.1 Platform

The native build host must be:

```text
Linux
x86_64
```

The host does **not** need to be Artix.

The current EndeavourOS VM is a supported outer-host candidate because the Artix package/rootfs authority is isolated from the host distribution.

The host's systemd service graph, pacman package database, and installed distribution packages must never silently become PortusOS target inputs.

### 4.2 Repository/toolchain commands

The outer host must provide the commands required by the current source/check/build orchestration:

```text
git
python3 / python
sh
bash
rustc
cargo
rustfmt
cargo clippy
node
npm
unshare
stat
sha256sum
```

The Rust workspace declares:

```text
edition = 2024
rust-version = 1.85
```

Therefore `rustc`/Cargo older than the repository's declared Rust 1.85 minimum are unsupported for the normal build path.

`rustfmt` and Clippy are hard requirements for normal `standard` and `full` harness preflight because `scripts/check.sh` runs formatting and `cargo clippy -D warnings`.

### 4.3 HTTPS/input acquisition

The outer build system must have either:

```text
working outbound HTTPS + trusted CA certificates
```

or:

```text
a complete verified local cache for every network input needed by the selected build
```

At least one verified HTTPS fetch mechanism must be available for bootstrap/input acquisition. The current supported environment provides `curl`.

A transiently unreachable network does not make a previously verified cached input invalid. The build must fail only when a required input cannot be verified or resolved from either the approved source or the approved cache.

### 4.4 Privilege boundary

The native build path may require privileged Linux operations for the isolated Artix context and ISO/rootfs construction, such as mount/chroot/loop-device or equivalent namespace/build-root operations.

The supported L2 isolation method is a verified Artix live-SquashFS lower layer plus a prepared writable OverlayFS upper, but the **mount tree itself is private and short-lived**. `scripts/artix/context.py prepare` enters a util-linux mount namespace, makes propagation private, performs loop/OverlayFS/chroot work there, and exits without leaving Artix mounts in the outer VM. Native `build-iso` does **not** remount the canonical prepared upper directly: it clones that upper into a fresh run-owned context with a unique OverlayFS upper/work pair, then mounts only that snapshot inside its private namespace. The canonical prepared upper/evidence under `portusos-build/work/` is reusable build input; the per-run upper/work context is disposable. A host-visible `artix-context/root` or `lower` mount is an invalid legacy state.

Current requirements are:

- `sudo` and util-linux `unshare` must be available for the bounded privileged namespace helper;
- privilege is owner-authorized at the native handoff: the builder reuses a valid `sudo -n -v` ticket when present, or, only when attached to an interactive terminal, invokes `sudo -v` at that handoff before returning to `sudo -n` for the bounded helper;
- Portus code must never request, read, store, echo, log, or embed the user's sudo password; the interactive refresh, when required, is owned directly by `sudo` and the terminal;
- a noninteractive build with no valid cached sudo ticket must fail closed with unresolved exit `78` rather than attempting to prompt;
- privilege is used only for the private namespace, mount/chroot and `artools` operation that requires it;
- the outer build must not run the entire repository/harness as root merely for convenience;
- lack of cached sudo authorization is not itself a configuration defect: an interactive canonical build refreshes authorization at the native handoff, while a noninteractive build without a valid ticket returns deterministic unresolved exit `78`;
- preflight must inspect `/proc/*/mountinfo`, not only the caller's `findmnt` view: historical references to the canonical prepared upper/work are WARN because native builds use a fresh run-scoped clone, while any surviving prior `native-runs/.../artix-context` reference or current-run context reference is BLOCK;
- the native helper must make mount propagation private before its first native mount and must never mount the canonical prepared upper/work pair directly during `build-iso`;
- the installed PortusOS rootfs must retain both `linux-lts` (normal/default) and `linux` (alternate), while the first live ISO boot payload uses only `linux-lts`; the `artools 0.39.1-1` compatibility shim may patch only the run-owned Artix `buildiso` copy, must verify the exact upstream single-kernel seam before patching, must pair the LTS kernel image with exactly one `pkgbase=linux-lts` module version for live initramfs generation, and must fail closed on drift or ambiguity;
- native `artools` construction must be unattended: before `buildiso`, the helper must verify that the selected buildiso/basestrap path does not enable basestrap interactive mode and that basestrap still supplies pacman `--noconfirm`; `buildiso` itself receives `/dev/null` as stdin so operator keystrokes cannot become build inputs;
- after every privileged namespace action, success or failure, the helper must verify zero current-run mount-table references, zero process root/cwd/fd references, zero surviving `_build-iso-inner` helper for that run ID, and zero newly leaked loop devices backed by the Artix seed;
- native success additionally requires removal of the run-scoped Artix context and a checksum-bound `native-cleanup.json` with `status=pass`; missing/invalid/failing cleanup evidence makes the build fail;
- interruption handling must preserve terminal foreground-group SIGINT propagation to privileged descendants, perform bounded waits/reaping of the top-level builder, and rely on `native-cleanup.json` plus the next preflight's prior-run mount-leak BLOCK to prevent silent surviving native state;
- legacy `teardown` remains a recovery tool only for old globally mounted contexts and must refuse unsafe detach when an active process references the path.

### 4.5 Writable bounded generated roots

All generated build state must remain under:

```text
portusos-build/work/
portusos-build/cache/
portusos-build/out/
```

The normal build user must be able to create and write those roots.

The build must fail rather than silently redirecting generated data to arbitrary paths.

### 4.6 Source state

The source-cleanliness policy comes from the selected build config.

For the canonical `first-live.json` config:

```text
source_cleanliness = require_clean
```

so the working tree must be clean before native construction.

Release-candidate configs must always require a clean source tree.

---

## 5. Host resource requirements

### 5.1 CPU architecture

`x86_64` is required.

The current project is not building ARM or another architecture in the first-ISO path.

### 5.2 CPU and RAM

The current EndeavourOS VMware build environment uses the practical reference profile of approximately:

```text
4 vCPU
8 GiB RAM
```

This is a recommended working profile, not yet a measured hard minimum for ISO construction.

The harness should report CPU and memory capacity, but it should not invent a hard lower threshold until real `artools` build evidence is available.

### 5.3 Free storage

The build filesystem must have enough free capacity for, at minimum:

```text
isolated Artix userspace/build root
Artix package cache
Portus local-package/source staging
artools work/rootfs trees
Calamares/profile material
final ISO bytes
per-run logs/metadata/checksums
retained prior iteration evidence
```

No final hard free-space number is yet authoritative because the repository has not completed its first real `artools` ISO build.

Until measured evidence exists:

```text
30 GiB free before native construction = recommended operating headroom
below 30 GiB = preflight warning
```

The first successful builds must record actual peak working-set/storage behavior. After that evidence exists, this document should lock a hard minimum with a safety margin instead of preserving a guess.

The harness must always report free bytes on the filesystem containing the generated roots before native construction.

### 5.4 Current outer-host observation

Current reference observations for the expanded EndeavourOS VMware build host:

```text
root filesystem usable size: approximately 68.6 GiB
operator-reported free space immediately before public native run 20260829T060019Z-3afadb080c36-dev-first-live: approximately 32 GiB
free space after that failed bootfs-stage run, measured from filesystem blocks: approximately 21.7 GiB
```

The public run entered real `artools` construction and failed for the memtest bootfs input issue rather than disk exhaustion. The post-failure filesystem remains below the recommended 30-GiB headroom because failed `artools` construction can retain large root-owned run workspace/chroot data even after the privileged Artix-context cleanup has safely removed mount/process/loop references. Before another full retry, reclaim only verified-dead failed-run workspace while preserving the run ledger/evidence and reusable caches needed for diagnosis/repeatability.

The command/toolchain portion of the current outer host remains suitable. These resource observations are evidence, not a permanent hard free-space floor; the final minimum still requires peak-working-set evidence from successful complete builds.

---

## 6. Isolated Artix build-context requirements

Before native ISO construction can proceed, the harness/adapter must have a reproducible isolated Artix context.

The context must prove:

```text
Linux userspace identifies as Artix Linux
architecture is x86_64
Artix repositories are explicitly known
Artix mirror configuration is known
Artix keyring/signature verification is active
pacman is functional
artools is installed and its version is recorded
selected Artix packages can be resolved from approved repositories
OpenRC package/service identities needed by the image can be resolved
```

The isolated context must not reuse the outer EndeavourOS package database as though it were Artix state.

The first-ISO build has a concrete bootstrap mechanism. `portusos-build/artix/bootstrap.json` locks the official `artix-base-openrc-20260813-x86_64.iso` seed, its SHA-256, exact byte size, and the embedded `/LIVEOS/ROOTFS.IMG` SquashFS extent. `scripts/artix/context.py` verifies that seed unprivileged and retains the canonical prepared OverlayFS upper plus Artix identity/repository/keyring/`artools` evidence under `portusos-build/work/` after privileged preparation completes. OverlayFS work directories are not reusable build identity; native construction creates a fresh run-owned upper/work pair from the prepared upper for every build attempt.

The bounded privileged bootstrap remains:

```bash
sudo python -B scripts/artix/context.py prepare
```

`prepare` now creates a **private mount namespace** with `unshare --mount --fork`, makes mount propagation private, loop-mounts only the verified embedded SquashFS, layers the retained writable upper, binds the repository read-only for the preparation work, performs pacman-key/repository/`artools` setup in chroot, writes the persistent evidence, and exits. The namespace teardown is kernel-owned when the helper exits; normal successful preparation leaves no Artix filesystem visible to Thunar/GVFS or the outer host mount table.

Unprivileged inspection is available with:

```bash
python -B scripts/artix/context.py verify-seed
python -B scripts/artix/context.py status
```

A healthy `status` after preparation reports the verified state while `root_mounted=false` and `lower_mounted=false`. `teardown` is retained only to recover legacy/global mounts created by the earlier discovery implementation; it is not part of the normal build lifecycle.

A clean caller mount table is necessary but not sufficient to prove historical mount state is gone. Linux processes can retain inherited mount namespaces after the caller unmounts a path. The mandatory preflight therefore scans process mount tables for the canonical prepared `upper`/`overlay-work` paths. Such historical references are surfaced as a warning and must never be remediated by killing unrelated host services merely to make a build proceed. The native build remains correct by cloning the prepared upper to a unique per-run pair and separately proving the current run has no pre-existing or surviving references.

The first L2 preparation has now proved and locked the current `artools` profile/build mapping. Observed `artools-base`, `artools-iso`, and `artools-pkg` are all `0.39.1-1`; the selected adapter invokes `/usr/bin/buildiso` with profile `portus`, stable repositories, `x86_64`, OpenRC, and run-owned chroot/output roots. The real `portus` profile workspace is tracked under `portusos-build/iso/artools-profile/workspace/`.

The Artix package-resolution pass also locked every selected official-Artix top-level package identity used by the first-ISO contract and the exact selected OpenRC service names/runlevel. In particular, the browser role resolves to official `ungoogled-chromium` plus `xdg-utils`, while the installed executable/desktop identities remain `/usr/bin/chromium` and `chromium.desktop`.

### 6.1 Required Artix-side command classes

The exact executable set is finalized by the native adapter, but the Artix context must provide the functional equivalents required for:

```text
pacman package resolution/installation
Artix keyring verification
artools/buildiso
Artix root/bootstrap/chroot operations required by the selected adapter
shell/core filesystem utilities
Git/source staging where required
verified HTTPS downloads where required
Rust/Cargo package builds for first-party Portus binaries
OpenRC fact/service inspection used by L2
```

The existing `scripts/artix/collect-l0-l2-facts.sh` expects the Artix context to expose the current package/OpenRC evidence commands and must pass there rather than on the outer EndeavourOS host.

### 6.2 Portus MCP packaging requirement

The first ISO includes Portus MCP.

Its current source contract requires Node.js 20.9 or newer plus npm for the verified packaging/launch path unless the packaging implementation later produces and verifies a self-contained artifact that removes that build-time dependency.

Therefore the Artix build pipeline must be able to provide the Node/npm capability needed by the selected Portus MCP packaging method.

### 6.3 Calamares

The selected first-ISO installer input is signed Artix `world/calamares 3.4.2-4` x86_64. The verified package SHA-256 is `4e8e70ebd9a4f6834b7c592ac698c14d6709aef6b0391f3d8673a4b0ab06130f`; the detached signature was verified with the retained Artix keyring as `Artix Buildbot <buildbot@artixlinux.org>` using key `0A3EB6BB142C56653300420C1247D995F165BBAC`.

The first-ISO Calamares composition uses the verified Artix stock modules, including signed stock `notesqml`, plus exactly one Portus-owned verified-gap Python job, `portus-storage`. Its canonical sources live under `portusos-build/installer/modules/portus-storage/`; the build graph must reject a missing or differently selected custom module. The locked module-set identity is `artix-3.4.2-4-stock-notesqml-plus-portus-storage-v2`.

`portus-storage` and its input surface are installer-only inputs. Unprivileged staging copies runtime code only to `portus/live-overlay/usr/lib/calamares/modules/portus-storage/`, instance/stock overrides only to `portus/live-overlay/etc/calamares/modules/`, the Portus settings sequence to `portus/live-overlay/usr/share/calamares/settings.conf`, the static QML page to `portus/live-overlay/usr/share/portus-installer/`, and the launcher plus narrow Calamares-only polkit authorization into the live overlay. None may be copied into `portus/root-overlay` or the installed system.

Calamares `3.4.2` Python jobs do not add their module directory to `sys.path`; the job directory is exposed separately as `libcalamares.job.working_path`. Therefore `main.py` and `storage_engine.py` must remain co-located in the staged module directory, and sibling support code must be loaded from that explicit working path rather than relying on ambient Python import-path behavior. The host-safe regression suite must continue to prove this with the module directory absent from `sys.path`.

The signed stock `notesqml` wrapper is the selected input view because Calamares `3.4.2` registers writable `io.calamares.core.Global`/GlobalStorage in QML and forwards `onActivate()`/`onLeave()` lifecycle calls. It cannot disable Calamares' global Next button, so the execution path must fail closed independently: `portus-storage@preflight` requires the page's armed flag before it performs any inspection, computes the size-bound plan hash only after validating the current target, and `portus-storage@prepare` rechecks the hash immediately before destructive work.

No target disk, owner LUKS passphrase, recovery credential or plan-confirmation value is a build input. Those values are installation-runtime inputs and must never appear in tracked configuration, `staging-evidence.json`, build logs or generated ISO metadata. The static QML, preflight and prepare paths must clear those GlobalStorage keys on page re-entry, success and failure. The harness/installer must block or fail closed rather than invent or embed them.

---

## 7. Required build inputs

### 6.4 Unprivileged first-ISO staging

Before privilege is used for `artools`, `scripts/artix/stage_first_iso.py` materializes the run-owned `iso-profiles/portus/root-overlay` plus the installer-only live overlay additions and emits `staging-evidence.json`. The staging step builds/reuses P16 for the eight Portus binaries, verifies the complete Codex standalone package, exact-checkouts/builds the frozen PortusBrowser revision with `pnpm@9.15.4`, exact-checkouts Portus MCP and installs its locked npm dependency graph including development runtime dependencies, verifies/extracts the pinned Linux amd64 `tunnel-client` asset, and stages the complete live-only installer composition: `portus-storage` runtime + preflight/prepare/finalize configs, Portus Calamares settings, stock-module overrides, static `notesqml` storage-input QML, `portus-install`, and the narrow live-only Calamares polkit rule, all checksum-bound in staging evidence.

The resulting paths are bounded below `portusos-build/work/native-runs/<run-id>/`; final artifact ownership remains below the corresponding `portusos-build/out/builds/<run-id>/artifacts/`. The privileged helper accepts only this manifest and rejects source-revision mismatch, changed `artools` mapping, incomplete required payloads, traversal/unbounded paths, or a pre-existing ISO artifact.

The current reference builder must be able to reach `registry.npmjs.org` directly over verified HTTPS for PortusBrowser/Portus-MCP dependency material unless a complete verified local cache is available. The reference HTTPS proxy was measured to produce invalid/expired certificate-chain failures for npm registry tarballs while direct registry TLS validated correctly, so the sanitized staging environment places only `registry.npmjs.org` in `NO_PROXY`/`no_proxy`. Do not remediate registry failures by disabling TLS verification.

The selected build must be able to resolve or verify every first-ISO input referenced by the composition graph.

This includes, at minimum:

- selected official Artix repositories/keyring and first-ISO package set;
- both selected kernels and boot/storage tooling;
- X11/i3/Alacritty/tmux and live-session dependencies;
- Chromium and URL-handler dependencies required for normal Codex ChatGPT login;
- selected Codex release artifact and verification metadata;
- all first-party Portus runtime binaries/packages;
- PortusBrowser frozen source/package inputs;
- Portus MCP pinned source and npm dependency set/build output;
- OpenAI `tunnel-client` pinned Linux x86_64 artifact and digest/provenance inputs;
- Calamares and installer material;
- Artix `artools` and the repository-owned profile/rootfs overlay;
- any VMware/firmware packages actually selected for the first ISO.

A build may use an approved local cache, but cached files must still match the authoritative hash/signature/source contract.

A missing source should be reported as a missing input, not silently replaced by an AUR package, alternate mirror/source class, or unpinned download.

---

## 8. Network requirements

Network access is required only to the extent that required build inputs are not already present in a verified cache.

For a cold build, the environment must be able to reach the approved sources needed by the selected composition, including Artix package mirrors and approved external component sources/releases.

The build process must not rely on arbitrary internet search, AUR helpers, or opportunistic fallback downloads.

A network preflight should distinguish:

```text
required uncached source unreachable -> BLOCK
required source already present and verified in cache -> PASS
optional metadata endpoint unreachable -> WARN when safe
```

---

## 9. Environment contamination rules

The outer host must remain orchestration-only.

Before native construction, the environment must not contain a configuration that would cause the build to silently resolve PortusOS target packages from the outer host's package database or repositories.

In particular:

- do not add Artix repositories to the EndeavourOS host pacman configuration for the supported build path;
- do not treat host systemd units as evidence for target OpenRC services;
- do not copy arbitrary host `/etc` state into the ISO;
- do not consume host credentials/secrets as ISO inputs;
- do not let an untracked host-installed package silently satisfy a target package contract;
- do not use AUR/AUR-helper fallback for first-ISO target packages.

The isolated Artix context, package lock, component pins, and repository source tree are the authorities.

Unprivileged payload staging and the native adapter must also use a constructed allowlist environment rather than inheriting the outer desktop/session environment wholesale. The allowed class is limited to required toolchain paths, run-owned `HOME`/cache paths, locale, and explicitly required proxy/CA transport variables. API keys, access tokens, tunnel credentials and unrelated application variables must not be forwarded to Cargo/npm/Git/component build subprocesses or recorded in staging/native evidence.

---

## 10. Credentials and secrets

No reusable user credential is a build prerequisite that may be stored in the repository or ISO.

The environment must **not** require any of the following to be baked into the build:

```text
ChatGPT/Codex login token
OpenAI API key
Secure MCP Tunnel runtime/control-plane key
GitHub personal token
SSH private key
sudo password
protected API-provider user secrets
```

Credentials needed after boot are entered/configured at runtime under the relevant PortusOS workflow.

If a private credential is needed to access a build source in the future, that source must receive an explicit approved non-repository credential mechanism rather than being silently read from arbitrary host state.

---

## 11. What is not required on the outer build host

The following are **not** outer-host prerequisites merely because they are first-ISO payload or post-build validation requirements:

```text
Artix as the host distribution
OpenRC as the host init system
Chromium on the host
Codex on the host
Portus MCP running on the host
OpenAI tunnel-client running on the host
Calamares running on the host
VMware merely to construct ISO bytes
PortusOS user/service identities installed on the host
```

VMware becomes required for the subsequent boot/install/acceptance loop, not for ISO byte construction itself.

---

## 12. Required pre-native-build checks

Before the native `artools` stage, the mandatory environment preflight proves or explicitly blocks at least:

```text
outer OS = Linux
outer architecture = x86_64
build config valid
repository/config/source paths exist
source cleanliness satisfies config
required harness commands exist
Rust version satisfies Cargo.toml
rustfmt + Clippy available when configured tests require them
generated roots are writable and remain bounded
available disk capacity measured
isolated Artix context exists and identifies as Artix/x86_64
Artix package manager/keyring are functional
required native-adapter commands exist
artools version can be recorded
required selected Artix package inputs can resolve
required external pinned inputs are reachable or verified in cache
no forbidden outer-host repository contamination is detected
privilege mechanism required by the selected adapter is available without credential capture
```

A build should not discover these conditions one at a time halfway through an expensive image build if they can be proven beforehand.

---

## 13. Preflight result classes

Environment checks should use three outcomes:

```text
PASS   requirement satisfied
WARN   build may proceed, but a non-authoritative capacity/reliability concern exists
BLOCK  native construction must not start
```

Examples:

```text
missing rustc                         -> BLOCK
wrong CPU architecture                -> BLOCK
Artix context identifies as Endeavour -> BLOCK
missing verified Codex artifact with no network/cache path -> BLOCK
stale Artix repository DB with no verified matching package-file closure -> BLOCK
17 GiB free while 30 GiB is only recommended and not yet measured as hard minimum -> WARN
optional network metadata unavailable while required verified cache is complete -> WARN
```

Warnings must be recorded in the run ledger rather than lost in terminal output.

---

## 14. Relationship to harness test preflight

`BUILD_HARNESS.md` currently exposes config values:

```text
build.preflight = standard | full | skip
```

That setting controls repository/test depth.

It must **not** be interpreted as permission to skip hard environment readiness checks.

The intended distinction is:

```text
mandatory environment preflight
  -> cannot be disabled for native construction

configurable test preflight
  -> standard | full | skip
```

A narrow debugging config may skip expensive test suites, but it still must not attempt native construction on an unsupported OS/architecture, with missing required commands, without a valid Artix context, or without required build inputs.

---

## 15. Evidence ownership

The environment preflight is implemented by:

```text
scripts/build_environment_preflight.py
```

The build harness runs it automatically and it produces the machine-readable per-run record:

```text
preflight.json
```

The record should contain stable check IDs, requirement class, observed value/version/path, result (`pass`/`warn`/`block`), reason, and remediation hint where useful.

The preflight record should be:

- created inside the timestamped build-run directory;
- referenced from `build-run.json`;
- summarized in `build.log`;
- covered by `RUN_SHA256SUMS`;
- retained for failed/blocked runs;
- free of reusable secret values.

This makes environment regressions comparable across ISO iterations in the same way source/config regressions are already traceable.

The current checker covers the outer Linux/x86_64 boundary, non-root harness execution, Python/Git/shell/Rust/tool availability, Rust minimum, rustfmt/Clippy requirement based on configured test depth, selected source paths, config identity, source cleanliness, bounded-root writability, disk/CPU/memory observation, explicit Artix-repository contamination on a non-Artix host, privilege-command availability, verified Artix seed/context identity, pacman/keyring state, prepared-context repository synchronization evidence, `artools`/`buildiso` evidence, locked adapter mapping, selected official-Artix package identities, Codex/PortusBrowser freeze state, locked Portus-MCP/tunnel identities, and the presence of the native-run repository/package-closure gate. `input.network-cache-closure` is now PASS when that fail-closed implementation is present and BLOCK if it is removed. The stronger runtime proof is `repository-closure.json`: native construction must freshly synchronize the stable repos, resolve/prefetch/verify the exact graph, freeze a local-only snapshot, and validate that snapshot before `buildiso`.

Historical checkpoint `7aca133` produced **`32 PASS / 2 WARN / 0 BLOCK`** before native execution was implemented. Later native runs proved the sudo handoff, rootfs/livefs construction, dual-kernel path, `linux-lts` live initramfs, memtest package omission, and rolling-repository drift described above. Canonical run `20260829T072729Z-4164361b115a-dev-first-live` then exercised the new closure gate: fresh `system`/`world`/`galaxy` synchronization and hash capture succeeded, and pacman resolved a 667-package closure requiring about 1469.51 MiB of downloads. The run failed during the initial bulk package prefetch because several mirrors hit low-speed timeouts or TLS EOFs; `buildiso` was never invoked, `repository-closure.json` recorded `status: fail`, and cleanup passed. A0 guarantees that exact resolved identities survive into failed-run evidence: the full graph is written to `repository-closure.json` immediately after resolution, before acquisition starts, and regression-tested against a simulated later failure. A1 adds a single repository anchor: one active HTTPS Artix mirrorlist server must successfully synchronize `system`, `world` and `galaxy` into the empty per-run database; its identity, attempts, generated-config hash and DB hashes are recorded, and the sync directory is made read-only before resolution. A2 now closes the monolithic-prefetch defect: unresolved exact identities are deterministically grouped under a 192 MiB byte ceiling, fetched as exact `repository/package` targets from the A1 anchor with dependency expansion disabled, SHA-256 verified after each batch, and removed from the pending set only after verification. Batch count, completed-batch count, verified count and pending count are persisted during acquisition. A3 closes the controlled-failover boundary: each batch is allowed at most four mirror attempts, always anchor first then distinct HTTPS fallbacks from the official mirrorlist, with the same frozen DB and exact identities throughout. Pacman 7.1 remains authoritative for the download transaction; no acquisition attempt may refresh repository metadata, SHA-valid partial progress is retained, unverified/corrupt partials are removed before fallback, and successful closure evidence must show a clean final pacman attempt for every downloaded batch. A4 closes the explicit cross-run resumability/state-reconciliation boundary: after every new frozen graph is resolved, the persistent cache is audited against its exact filenames and SHA-256s; matching complete archives become reused, mismatches are removed, stale partial payload is removed, and only missing/unusable identities remain pending for A2/A3. Passing evidence must prove a complete reused/pending partition and consistent final `cached_before` state. The currently retained failed-run cache contains 13 complete candidate archives and no `.part` files; the next run may reuse any of those only if they match its newly frozen identities. A5 closes the per-package acquisition-observability boundary: every frozen package has a deterministic current state, verification/acquisition flags and last attempt context; aggregate resolved/verified/pending package and byte totals are persisted; and mirror attempts include requested/verified/pending filename/byte sets plus normalized failure classes. Current failed records are validated for internally consistent progress rather than being allowed to report false zero progress. A6 now closes the substage/cause-classification boundary: current failed closure evidence must identify one of the locked substages (`mirror-selection`, `repository-sync`, `resolution`, `acquisition`, `cache-verification`, `local-validation`) and a normalized cause; the outer run remains at stage `repository-closure` but includes that diagnosis in its reason. Historical string-only failures remain readable. A7 closes the final local-only/buildiso handoff requirement: repository databases remain in a separate read-only snapshot until all frozen packages are verified; only then may the `file://` repository be constructed, independently re-resolved, followed by outer-owner restoration and a read-only cache remount. An explicit `buildiso_gate` must prove those conditions both in the native adapter and outer ledger, and regression tests directly prove that even one unresolved package prevents buildiso invocation. A canonical run must now prove the completed A0-A7 local-only closure, the tracked memtest correction and final ISO generation.

---

## 16. Change policy

Do not expand this requirements document with a command merely because one build host happened to have it installed.

A tool becomes a hard requirement only when the supported harness/native adapter actually depends on it.

Conversely, when the real Artix adapter introduces a dependency, this document and the environment preflight must be updated in the same change so the build does not rely on hidden machine state.

Resource thresholds should be measured and then locked. They should not be invented for appearance of precision.

---

## 17. Current unresolved requirement details

The broad environment boundary and core Artix L2 build facts are now locked. The remaining requirement work is narrower:

```text
Codex target-installed-version + full Artix/OpenRC runtime compatibility proof
PortusBrowser Artix packaging/native-messaging/runtime proof at the frozen revision
signed-Calamares live loading plus destructive blank-VM/cleanup/logging/installed-boot proof of the implemented `notesqml` -> `portus-storage` preflight/prepare/finalize path
empirical native proof of structured A6 repository-closure substage/cause diagnosis on top of the implemented per-package observable/resumable bounded mirror-failover acquisition
empirical native proof of the completed verified persistent-cache + local-only frozen-repository gate
native proof that the tracked `memtest86+` boot dependency closes the `artools 0.39.1-1` bootfs path, then final ISO generation, generated package-lock evidence and repeat-build behavior
Portus MCP Artix packaging/live launch proof
pinned tunnel-client Artix/live compatibility proof
remaining P16 installed ownership/service/filesystem evidence
measured peak disk working set and resulting hard free-space floor
measured practical CPU/RAM minimum
measured cold/warm cache behavior of the implemented native-run repository/package closure
VMware integration sourcing only if first boot proves native drivers insufficient
```

The multi-artifact package-lock **contract** is now defined and tested: one logical PortusOS role may contain multiple exact package artifacts with version, repository, licences, optional SHA-256, and verification evidence. The real `packages.lock.yaml` is still generated later, after the remaining component identities are frozen, so it can represent one coherent first-ISO input graph.

These unresolved details are not reasons to perform ad hoc manual builds. They must be closed through the supported harness/contracts and recorded in the run-scoped build ledger under `portusos-build/out/builds/<run-id>/` as real build evidence is acquired.
