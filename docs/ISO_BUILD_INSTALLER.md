# PortusOS ISO Build and Installer Contract

**Last reviewed:** 2026-08-29T08:59:01Z
**Last updated:** 2026-08-29T08:59:01Z

**Status:** Authoritative for the first-ISO `artools`/Calamares architecture and installer engineering baseline; Artix Calamares 3.4.2-4, the verified-gap `portus-storage` module and its runtime target/credential input surface are implemented, while live loading/destructive VM proof and complete native ISO output remain verification
**Target:** First accepted x86_64 VMware development ISO
**Acceptance authority:** `docs/ACCEPTANCE.md`
**Boot/storage authority:** `docs/BOOT_STORAGE_RECOVERY.md`
**Package/supply-chain authority:** `docs/PACKAGE_POLICY.md`
**Service authority:** `docs/OPENRC_SERVICES.md`
**GUI/session authority:** `docs/GUI_SYSTEM.md`
**Repeated-build harness authority:** `BUILD_HARNESS.md`
**Build-environment requirements authority:** `BUILD_REQUIREMENTS.md`

## 1. Purpose and authority

This document owns the first-ISO ISO-build framework, installer framework, ownership boundaries, and default installer engineering baseline.

Locked framework decisions:

> **PortusOS uses the Artix `artools` stack as its supported first-ISO ISO-build framework.**
>
> **PortusOS uses Calamares as its supported first-ISO installer framework.**

The authoritative first-ISO build runs on a native x86_64 Linux host using an **isolated verified Artix build context** for `artools`, Artix repository/keyring/package resolution, rootfs/profile construction and final ISO filesystem production. The outer Linux host does not need to be Artix; the current EndeavourOS VM is the selected outer build host. Artix mount/chroot work is isolated in short-lived private mount namespaces, while only verified upper/evidence/cache/output state persists on the host. Windows remains preparation/test-only.

PortusOS does not build a second general-purpose ISO construction framework or installer framework merely to recreate functionality already provided by `artools`, Calamares and standard Linux tooling.

The Calamares flow below fixes the intended user-facing pages and execution ordering by responsibility. Direct Artix inspection has now locked Calamares `3.4.2-4`, its signed stock-module inventory, exactly one verified-gap PortusOS job module, `portus-storage`, and the signed-stock-`notesqml` runtime input/settings path for explicit target selection plus independent owner/recovery credentials. Remaining Artix/VM installer work is live loading, destructive blank-disk execution proof, logging/interruption behavior and installed-target validation.

## 2. First-ISO ISO-build model

The build path must preserve `docs/PACKAGE_POLICY.md`: official Artix repositories are the normal third-party source, AUR/AUR helpers are excluded, and every non-Artix component must have an explicit approved component-specific source contract.

Conceptually:

```text
native x86_64 Linux build host
  -> isolated verified Artix build context
  -> verified Artix repositories/keyrings and approved pinned external inputs
  -> PortusOS-owned artools profile/configuration
  -> PortusOS package manifests + local-package staging
  -> PortusOS rootfs/live-ISO overlays
  -> Calamares + PortusOS installer configuration
  -> artools ISO construction
  -> ISO metadata/checksum/validation handoff
```

### 2.1 `artools` owns generic Artix ISO construction

Where the selected `artools` version provides the required mechanism cleanly, PortusOS reuses it for generic ISO/profile/rootfs construction rather than wrapping or replacing it with a second distribution builder.

### 2.2 PortusOS owns the PortusOS build definition

PortusOS owns:

- the shipped `artools` profile/configuration;
- package/profile manifests and local Portus package staging;
- live-root/rootfs overlays and configuration material;
- Calamares inclusion/configuration;
- recovery tooling required by the first-ISO contract;
- build metadata, validation handoff and release-input generation;
- deterministic policy for approved external pinned artifacts such as Codex;
- build-time safety rules, including no reusable user secrets in build configuration.

`artools` does not become the authority for PortusOS product semantics, package policy, installer semantics, storage architecture, service ownership or acceptance criteria.

### 2.3 Build and native-construction foundation

This ownership boundary is implemented through `portus-build`, `scripts/artix/stage_first_iso.py`, `scripts/artix/context.py`, and tracked sources under `portusos-build/`. The builder compiles package/profile, service/identity and semantic disk plans; reuses the Portus install staging contract; materializes the run-owned first-ISO root overlay; records `staging-evidence.json`; requires exact destructive-target confirmation where applicable; and invokes one repository-owned `build-iso` entry point. The observed `artools 0.39.1` profile/build mapping is implemented. Native construction clones the prepared Artix upper into a unique run-scoped upper/work pair, executes it in a propagation-private mount namespace, and emits `native-cleanup.json` after namespace exit. A successful native result is rejected unless that evidence proves no current-run mount/process/seed-loop leakage and removal of the run-scoped Artix scratch. A direct native call without the run-owned staging manifest fails closed with exit 78; remaining Calamares/target-runtime facts must be proven through the supported build and candidate-validation paths rather than guessed.

### 2.3 Build-host boundary

The accepted first-ISO build artifact is produced on native x86_64 Linux. The outer host may be EndeavourOS or another suitable Linux distribution, but all distribution-authoritative work executes inside the verified Artix context. For the current implementation, privileged Artix mount/chroot/`buildiso` operations run inside a private util-linux mount namespace; persistent host state is limited to the verified seed/cache, reusable OverlayFS upper/evidence, run-owned staging/work roots, logs and final artifacts.

The Artix context is authoritative for:

- `artools` execution;
- Artix repository/keyring/package resolution used by the ISO;
- rootfs/profile construction;
- local package staging into the Artix image;
- final ISO filesystem production;
- Artix-side evidence required to claim an artifact is a first-ISO candidate.

The outer Linux host supplies kernel/filesystem/build resources and orchestration only; its package database, init system and distribution packages must not silently become PortusOS target inputs. Chroot/container/build-root isolation is acceptable once the exact supported mechanism is verified.

For the current first-ISO path, the isolated context bootstrap is concretely defined by `portusos-build/artix/bootstrap.json` and `scripts/artix/context.py`: a verified official Artix OpenRC ISO is the immutable seed, its embedded `/LIVEOS/ROOTFS.IMG` SquashFS extent is mounted read-only **inside a private namespace**, and the reusable writable upper under `portusos-build/work/artix-context/` supplies isolated mutable Artix package/build state. Preparation records Artix/keyring/repository/`artools` evidence and exits with no host-visible root/lower mounts. Native construction never remounts that canonical upper directly. It clones the prepared upper into `portusos-build/work/native-runs/<run-id>/artix-context/`, uses a unique OverlayFS workdir, makes mount propagation private before mounting, bind-mounts only the run-owned native workspace at `/run/portus-build`, and invokes `/usr/bin/buildiso -p portus -R stable -a x86_64 -i openrc` with run-owned chroot/output roots. Preflight also inspects process mount tables for historical inherited canonical mounts; those are warnings, while current-run references are blockers. After namespace exit the privileged helper proves current-run cleanup and records it before the harness can accept an ISO.

The same helper makes native construction noninteractive by contract. It verifies the installed `buildiso`/`basestrap` relationship immediately before execution: buildiso may not opt into basestrap `-i`, and basestrap must retain its default pacman `--noconfirm` path. `buildiso` then runs with stdin set to `/dev/null`. The first real native run exposed pacman's normal `:: Proceed with installation? [Y/n]` display while `/proc` showed the actual pacman argv already contained `--noconfirm`; future builds therefore require no operator response at that point and will fail closed if the Artix tooling contract changes.

The same verified `artools 0.39.1-1` seam also requires `basestrap -c` host-cache sharing and the selected `-C "${pacman_conf}"` path. PortusOS uses those properties to close rolling-repository state before construction. After mounting the run-owned Artix context, the helper performs a fresh stable-repository sync, resolves the complete profile graph against an empty run-owned database, acquires and SHA-256 verifies the exact archives into the persistent Artix package cache, constructs a run-local repository from the captured `system`/`world`/`galaxy` databases and those archives, and independently validates that local repository resolves the identical graph. It then replaces only the run-owned stable artools pacman config with the local `file://` repository and remounts the verified cache read-only inside the private namespace. Canonical run `20260829T072729Z-4164361b115a-dev-first-live` proved the fresh sync and 667-package resolution but failed in the acquisition phase before local-repository validation or `buildiso`: a single roughly 1.47-GiB `pacman -Sw` transaction encountered repeated low-speed timeouts and TLS EOFs across several mirrors. A2 now keeps that graph immutable while replacing the network prefetch with deterministic 192 MiB byte-bounded batches of exact `repository/package` targets against the A1 anchor. Pacman dependency expansion is disabled for those download-only transactions, every batch is SHA-256 verified immediately, only verified files leave the pending set, and aggregate batch/pending progress is persisted after each attempt. A single archive larger than the ceiling is isolated in its own batch. A3 now adds controlled package-mirror retry/failover: each batch uses the A1 anchor first, then at most three distinct HTTPS fallbacks in official mirrorlist order, while continuing to read the same frozen DB. Pacman 7.1 was verified locally: `-Sw` is download-only and doubled `--nodeps` skips all dependency checks, so the adapter can request exact already-resolved `repository/package` identities without provider/dependency re-resolution. Acquisition never uses `-y`/`-Sy`/`-Syy`; SHA-valid completed files are retained, corrupt or partial payload is removed before switching mirrors, and a nonzero pacman result cannot become success from hash evidence alone. A4 now adds explicit cross-run cache reconciliation before batching: after the new run freezes its package graph, each expected persistent-cache archive is re-hashed against that graph; exact matches become reused, mismatched completed archives are removed, stale `.part`/`.sig.part` payload is removed, and only pending identities enter A2/A3. The currently retained failed-run cache has 13 complete candidate archives and no `.part` files, but each remains conditional on matching the next frozen SHA rather than being trusted as historical state. Passing evidence must show a complete reused/pending partition and consistent final `cached_before` annotations. Richer per-package progress evidence remains; the later whole-graph `-Sw` is local-only validation after the cache is complete, not a network prefetch. Only after every archive is verified may subsequent rootfs/livefs/bootfs basestrap transactions see the frozen local repository.

The live medium and installed-system kernel sets intentionally differ. The installable rootfs retains both the normal/default `linux-lts` kernel and alternate `linux` kernel. Verified `artools 0.39.1-1` assumes one rootfs kernel when constructing `/iso/boot`, so the adapter locks `live_boot_kernel_package: linux-lts` and applies a fail-closed compatibility patch only to the run-scoped Artix `buildiso` copy. That patch selects `vmlinuz-linux-lts`, derives its exact module version from the unique `pkgbase=linux-lts` module tree visible in temporary bootfs, and supplies that version only to the temporary live-initramfs path. It does not remove or rewrite either installed kernel.

This kernel compatibility path now has real native evidence. Public run `20260829T060019Z-3afadb080c36-dev-first-live` installed both `linux 7.1.9.artix1-2` and `linux-lts 6.18.46-1`, generated the installed initramfs artifacts, selected the LTS module tree for the live path, and generated `/run/artools/bootfs/boot/initramfs-x86_64.img`. The run then failed at the next bootfs step because `artools 0.39.1-1` attempted to copy `/run/artools/livefs/boot/memtest86+/memtest.bin`. Direct inspection of official Artix `world` package `memtest86+ 7.20-2` proved that it installs exactly `/boot/memtest86+/memtest.bin` (plus its GRUB integration), with no package dependencies. The first-ISO package contract and `packages-boot` therefore now include `memtest86+`; `portus-build` fails closed if the locked `artools 0.39.1` profile omits it. The following canonical run stopped earlier on rolling-repository drift. The closure gate now prevents that drift from reaching `buildiso`, but its first real exercise stopped during mirror-backed package acquisition before a complete local snapshot could be validated. Neither correction is considered empirically complete until acquisition is hardened, a canonical native run produces a passing frozen closure, reaches the memtest bootfs step, and continues through squashfs/final ISO output.

## 3. First-ISO installer model

The one supported installation path is:

```text
PortusOS live ISO
  -> Calamares
  -> inspect/select installation target and user choices
  -> construct PortusOS installation plan
  -> summary/destructive-operation review
  -> execute installation
  -> apply PortusOS-specific configuration
  -> validate installed target
  -> report success/failure
```

The accepted path must be repeatable from a blank reference VMware disk and require no undocumented repair after Calamares exits.

A recovery shell and live-ISO Linux tools remain available but do not count as a second accepted installer path.

The live ISO is also a required **interactive agent environment before installation**. It must expose the Master Portus non-root login/session path, X11/i3, Alacritty/tmux, `portus-bootstrap`/`portus-master`, Codex and the first-party Portus binaries alongside Calamares. This lets Codex investigate the actual live Artix VM and report missing packages, services, permissions, paths and integration defects during the build/test loop.

### 3.1 First-ISO installer UI baseline

Keep the development installer small:

```text
welcome
  -> locale
  -> keyboard
  -> partition/storage
  -> user/credentials
  -> summary/destructive-operation review
  -> install
  -> completion/reboot
```

Do not turn Calamares into a large Portus settings wizard. Portus runtime/policy defaults come from repository-owned configuration and the selected installer permission/profile contract.

## 4. Ownership boundary

### 4.1 Calamares and standard Linux tooling own generic mechanics

Reuse supported mechanisms for:

- disk and partition operations;
- filesystem creation/mounting;
- encrypted storage;
- locale/keyboard/timezone;
- user creation;
- package installation;
- bootloader installation/configuration;
- target-root configuration.

The module/tool mapping is verified against signed Artix Calamares `3.4.2-4`. Stock modules remain preferred for generic work; the exact storage gap identified below is owned by the single `portus-storage` job.

### 4.2 PortusOS owns PortusOS installation semantics

PortusOS owns:

- shipped Calamares configuration and module sequence;
- installation profiles/package inputs;
- minimal PortusOS branding;
- Master Portus user/profile requirements;
- policy materialization inputs;
- Portus service/provider enablement requirements;
- runtime/state/config directory setup;
- Master workspace creation;
- Codex/PortusBrowser integration inputs;
- stricter safety/secret requirements;
- installed-target validation and first-boot handoff.

Calamares is the implementation framework, not the product/security authority.

### 4.3 Execution-order baseline

The selected Calamares module IDs are verified; preserve this responsibility order when materializing the final settings sequence:

```text
validate selected disk + plan
  -> partition / create locked GPT layout
  -> create/mount filesystems and LUKS2 -> LVM layout
  -> unpack/install target root
  -> machine identity
  -> locale / keyboard / timezone
  -> generate fstab + crypttab/storage target config
  -> install package/profile inputs and Portus target configuration
  -> configure users / Master Portus
  -> configure networking / clock
  -> configure mkinitcpio for both Artix kernels
  -> enable verified OpenRC machine services
  -> install/configure GRUB UEFI + fallback EFI path
  -> apply Portus policy/runtime/workspace integration
  -> installed-target validation
  -> unmount / finish
```

Where current Calamares exposes clean stock modules for these responsibilities, use them. Do not copy module names/order from another distribution without verifying the selected Artix build.

## 5. Custom-module rule

The project began with a zero-custom-module preference. Direct inspection of Calamares `3.4.2` proved a concrete first-ISO blocker: automated/custom-layout execution calls `layoutApply()` and creates partitions, while volume-group creation is reached from the manual partition-page VG dialog. The accepted PortusOS path may not require undocumented manual VG/LV assembly, so configuration-only stock Calamares cannot implement the locked `LUKS2 -> VG portus -> root LV + swap LV + reserve` topology.

Exactly one narrow custom job is therefore selected: `portus-storage` under `portusos-build/installer/modules/portus-storage/`.

Its bounded responsibilities are:

- require an explicit `/dev/...` target and a matching SHA-256 of that target plus the locked storage plan before destructive work;
- create the locked GPT/ESP/`/boot`/LUKS2/LVM/root/swap topology;
- consume separate owner and recovery credentials without putting either value in argv, persistent installer configuration or public state;
- provision the recovery keyslot through anonymous Linux memory rather than a reusable key file;
- expose only non-secret filesystem/LUKS identity needed by the remaining Calamares jobs;
- finalize dynamic `fstab`, `crypttab`, mkinitcpio and GRUB storage configuration that stock Calamares cannot infer for LVM-inside-LUKS;
- perform bounded best-effort cleanup of its own transient mounts/VG/mapper on failure.

It does **not** become a general-purpose installer, user manager, package installer or secret-entry UI. It is staged only into the live Calamares filesystem, not the installed rootfs. Stock `luksbootkeyfile` is deliberately not selected because an embedded automatic-unlock key conflicts with the owner-unlocked boot contract.

The runtime input/settings path is now implemented without adding a second custom module. Signed stock `notesqml` hosts a static Portus-owned `Storage & Recovery` QML page immediately before summary. Calamares 3.4.2 exposes writable GlobalStorage to QML and forwards the page lifecycle hooks, so the page can clear stale inputs, collect an explicit whole-disk target plus masked independent owner/recovery credentials, require erase acknowledgement, and arm the execution path only through its own Continue action.

Because stock `notesqml` cannot disable Calamares' global Next button, UI validity is not the destructive authority. Execution begins with non-destructive `portus-storage@preflight`, which requires the armed flag, validates the current block device/writability/size/mount/name state and computes the size-bound plan hash. `portus-storage@prepare` immediately revalidates that hash before its first destructive command. A bypassed/incomplete page therefore fails before disk writes. Runtime target/hash/credential/armed keys are removed after successful prepare and on preflight/prepare failure.

## 6. Destructive-operation and secret-safety requirements

The supported path must preserve:

- explicit selected installation target;
- summary/review before destructive writes;
- no silent default to an arbitrary host disk;
- destructive work limited to the selected target and locked disk plan;
- visible failure rather than false success;
- no disk/user reusable credentials in argv, ordinary logs or public build metadata;
- no reusable user credentials in build-time configuration;
- no undocumented manual fix between installer completion and accepted installed boot.

Exact log paths, confirmation controls and interruption/restart behavior remain implementation-verification items.

## 7. Relationship to boot/storage, services and GUI

Calamares must implement `docs/BOOT_STORAGE_RECOVERY.md` rather than a generic weaker layout:

```text
GPT/UEFI
512 MiB FAT32 ESP mounted /boot/efi
2 GiB unencrypted ext4 /boot
LUKS2 AES-XTS/Argon2id
  -> VG portus
       -> ext4 root
       -> 4 GiB swap
       -> ~5% VG reserve
GRUB UEFI
mkinitcpio for linux-lts + linux
```

It must also install/enable the verified OpenRC machine-service graph from `docs/OPENRC_SERVICES.md` and install the session configuration from `docs/GUI_SYSTEM.md` without promoting user-session processes into machine services.

If stock partitioning cannot implement the locked storage behavior safely, use the Section 5 escalation rule rather than changing the architecture.

## 8. Remaining build/install verification

Still to verify/freeze on native Artix/Linux/VMware:

- completion of genuine `artools` execution through the corrected live bootfs path, squashfs and final ISO output, plus generated package-lock/repeat-build evidence;
- live proof that signed Calamares loads the implemented `users -> notesqml@portus-storage-input -> summary` show order and `portus-storage@preflight -> portus-storage@prepare -> unpackfs` execution prefix without unresolved QML/module/config dependencies;
- destructive blank-disk proof that `portus-storage` produces the locked GPT/LUKS2/LVM layout, effective Argon2id parameters and two working independent keyslots without persistent automatic-unlock material;
- exact installer logging/redaction locations and interruption/restart behavior, including bounded cleanup after a failed storage job;
- exact installed-target validation and first-boot handoff commands;
- live proof that both kernels receive the required normal/fallback initramfs state and GRUB boots `root=/dev/portus/root` through the outer LUKS mapper;
- final ISO filesystem/build metadata details.

These are implementation-verification items, not reasons to redesign the installer without evidence.

## 9. Validation requirements

Before the installer contract is complete, prove at minimum:

- the live ISO reaches the required interactive Master Portus/Codex environment before installation;
- the live environment contains the first-party Portus binaries and lets Codex inspect real live machine state without requiring pre-provisioned reusable secrets;
- Calamares is present/launchable in the live ISO;
- selected module/configuration set loads without unresolved dependencies;
- storage matches `docs/BOOT_STORAGE_RECOVERY.md` exactly enough for acceptance;
- invalid/destructive target choices are safely handled;
- a blank VMware disk installs without undocumented manual assembly;
- credentials/secrets are absent from ordinary logs/argv/public metadata;
- both Artix kernel/initramfs paths and GRUB entries are installed correctly;
- verified OpenRC services are enabled in the correct lifecycle;
- local graphical-session configuration is installed without making GUI helpers machine daemons;
- installed system boots after ISO removal and reaches encrypted unlock -> OpenRC;
- installer failure yields actionable evidence rather than false success.

Final acceptance remains governed by ISO-01 through ISO-38 in `docs/ACCEPTANCE.md`.

## 10. Decisions locked by this document

1. Artix `artools` is the first-ISO ISO-build framework.
2. Native x86_64 Linux is the supported build host; an isolated verified Artix build context is authoritative for `artools`, Artix packages/keyrings, rootfs construction and ISO production.
3. Calamares is the first-ISO installer framework.
4. PortusOS creates no parallel general-purpose ISO builder or installer.
5. The accepted installer path is the PortusOS-configured Calamares path from the live ISO.
6. The installer UI stays minimal: welcome, locale, keyboard, user/credentials, Storage & Recovery, summary, install, completion.
7. Generic mechanics remain with Calamares/Linux tooling where they satisfy the PortusOS contract.
8. PortusOS owns configuration, module ordering, Portus-specific semantics, safety, service/session integration and installed-target validation.
9. The intended execution order is fixed by responsibility and the stock module IDs are verified against signed Artix Calamares `3.4.2-4`.
10. The initial zero-custom-module preference was tested and a blocking automated-LVM gap was proven.
11. Exactly one custom job, `portus-storage`, is selected for that verified gap and must remain narrowly scoped.
12. Calamares must implement the locked boot/storage/kernel/service/session contracts and may not weaken them for convenience.
13. Destructive writes require explicit target/review and secret-safe handling.
14. Recovery/live shell is independent but is not a second accepted installation path.
