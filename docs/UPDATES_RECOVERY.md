# PortusOS Update and Recovery Contract

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative for the first-ISO update/recovery architecture and researched command/evidence baseline; exact behavior still requires execution on the selected Artix/VMware candidate before release acceptance
**Target:** x86_64 VMware-first PortusOS first ISO
**Package authority:** `docs/PACKAGE_POLICY.md`
**State authority:** `docs/STATE.md`
**Health/recovery authority:** `docs/HEALTH_RECOVERY.md`
**Boot/kernel authorities:** `docs/BOOT_STORAGE_RECOVERY.md` and `docs/HARDWARE.md`
**Codex authority:** `CODEX_UPDATES.md`
**PortusBrowser authority:** `docs/PORTUS_BROWSER.md`
**Acceptance authority:** `docs/ACCEPTANCE.md`

## 1. Purpose and authority

This document owns the first-ISO operating-system/component update workflow, update-specific recovery state, interrupted-update handling, post-update health proof, reboot boundary, and the limits of rollback/backup automation.

The first-ISO rule is deliberately conservative:

> **Updates are explicit, observable operations. Artix/pacman remains the distribution package authority; PortusOS adds preflight, compatibility, bounded state protection, health validation and recovery semantics rather than replacing the package manager.**

The first ISO does not implement unattended background OS upgrades, automatic reboot after updates, complete system rollback, or general backup automation.

## 2. Update initiation and owner control

PortusOS must not silently update the operating system or critical bundled components in the background.

For the first ISO:

- OS/package updates are explicit operations requested by an authorized agent, owner or administrator under normal Portus policy;
- no OpenRC service, cron job, login hook, `portus-bootstrap`, `portus-master`, or graphical-session startup performs an automatic package upgrade;
- update availability may be inspected or reported without applying the update;
- an update result must make clear what changed, what validation ran, whether degradation remains, and whether a reboot is required;
- direct administrator use of native Artix tools remains possible, but the supported Portus-managed update path is the path that provides the full preflight/audit/recovery contract.

This preserves owner control without manufacturing tool scarcity around normal Linux administration.

## 3. Native package-manager boundary

Distribution packages are updated through normal Artix/pacman mechanisms using the source rules in `docs/PACKAGE_POLICY.md`.

PortusOS must not build a second package manager or maintain a private shadow package database.

The managed update layer may coordinate pacman and component-specific installers/updaters when the operation needs cross-component safety, but native package state remains authoritative for Artix packages.

The supported update path must not introduce AUR/AUR-helper fallback. If an approved non-Artix component has its own explicit update contract, that contract remains component-specific and must participate in the same compatibility/health workflow.

## 4. Update preflight

Before mutating the installed system, the supported update workflow must establish enough evidence to know whether it is safe to proceed.

The first-ISO preflight checks, as applicable to the requested update, include:

- selected Artix repositories/keyrings/signature path is usable;
- package database is not already in an unresolved/broken transaction state;
- sufficient free storage exists for the package transaction, temporary files, required state backups and regenerated boot artifacts;
- required package/source classifications remain allowed by `docs/PACKAGE_POLICY.md`;
- Portus runtime/component version compatibility is known well enough for the requested transition;
- any required schema/config migration is identified before the new runtime is treated as ready;
- `portus.db` is readable/integrity-checkable enough to create the backup required by `docs/STATE.md` when a destructive/non-trivial migration is involved;
- both required kernel roles are installed before a kernel-affecting update proceeds;
- TTY/root/OpenRC recovery remains available, and SSH remains available when the update can safely preserve it;
- critical managed work that cannot tolerate the version transition is not silently invalidated underneath a running task;
- Codex and PortusBrowser changes have passed their component-specific pin/compatibility gates before their selected versions are advanced.

A failed preflight stops the managed update before destructive mutation and reports the blocking condition. It must not silently weaken a security/recovery requirement merely to continue.

### 4.1 Researched pacman command baseline

The current first-ISO research baseline uses native pacman tooling directly. The 2026-08-27 Artix system-repository snapshot exposes pacman `7.1.0.r9.g54d...`, mkinitcpio `41.1-1`, GRUB `2:2.14-1`, OpenRC `0.63.3-2`, `linux` `7.1.9.artix1-2`, and `linux-lts` `6.18.46-1`. These versions are research evidence, **not permanent release pins**; the exact release-candidate builds are resolved again from the selected Artix repositories during validation.

Before transaction resolution, the managed path records the effective pacman environment rather than assuming `/etc/pacman.conf` defaults:

```text
pacman --version
pacman-conf --repo-list
pacman-conf DBPath CacheDir GPGDir HookDir SigLevel HoldPkg IgnorePkg IgnoreGroup
pacman -Dk
```

For every configured repository, the implementation also records the resolved repository block through `pacman-conf --repo <repo>` or the verified equivalent so repository-specific server/signature behavior is part of the evidence.

`pacman -Dk` is the mandatory local package-database consistency gate. `pacman -Dkk` may be used as an additional sync-database dependency-availability check after repositories have been refreshed, but it does not replace `-Dk`.

The full-system transaction is resolved with pacman itself rather than with a second dependency solver:

```text
pacman -Syu --print --print-format '%r\t%n\t%v\t%f\t%s\t%h\t%l'
```

The recorded target fields are repository, package name, target version, package filename, size, SHA-256 and package location. The installed pre-update package state is recorded separately with native `pacman -Q` evidence because `--print-format` describes targets rather than the installed old version.

The print-based resolver is accepted as the first implementation only if the live Artix fixture proves that the resulting approved plan also identifies every implicit removal/replacement side effect needed for safety review. Pacman's public `--print` contract promises target output, not a complete machine-readable add/remove transaction graph. If the fixture shows that it is insufficient, the narrow read-only planner may use pacman's own libalpm transaction API (`alpm_sync_sysupgrade`, `alpm_trans_prepare`, `alpm_trans_get_add`, `alpm_trans_get_remove`) to obtain the complete planned add/remove sets. That does **not** replace pacman as the package mutation authority; it uses pacman's native transaction library only for exact preflight resolution.

Important boundary: `-y` refreshes the sync databases even though `--print` prevents the package targets from being installed. Therefore this resolution command is not a passive background update check. It runs only inside an explicit managed update task after the Portus update lock and basic preflight have been established.

The first managed apply baseline is a **full system upgrade**, never a partial-upgrade shortcut. After the resolved plan has passed compatibility/preflight checks, the executor re-resolves against the same refreshed sync database and requires the complete reviewed add/remove/replacement plan to match the approved plan. The apply transaction then uses:

```text
pacman -Su --noconfirm
```

inside the reviewed privileged update executor. `-Su` here is the second half of the same tightly bounded managed `-Syu` operation whose sync databases were refreshed during plan resolution; it must not be separated into an unrelated later package-install workflow. The ordinary administrator/native equivalent remains `pacman -Syu` with pacman's confirmation prompt.

`--noconfirm` is permitted only because the Portus managed path has already produced, checked and authorized the exact plan. It must never be used as a generic way to accept an unreviewed changed transaction. Immediately before `-Su --noconfirm`, the selected resolver (print-based if proven sufficient, otherwise the narrow libalpm read-only planner) runs again against the unchanged refreshed sync database and must match the approved complete transaction; otherwise the update returns to preflight/plan review instead of mutating the machine.

The live Artix gate must verify this split resolve/apply behavior, including replacement/removal completeness, against the selected pacman/libalpm build before it is implemented as the release path.

## 5. Update-specific recovery state

General automated machine backup is deferred, but an update may require **bounded local recovery material** for the state it is about to migrate or replace.

This is part of update safety, not the future backup product.

Required rules:

- destructive/non-trivial `portus.db` migrations use the consistent pre-migration backup already required by `docs/STATE.md`;
- other small Portus-owned configuration/state that cannot be reconstructed safely may receive a bounded update-specific copy before an irreversible transition;
- recovery copies are stored outside the live file being replaced;
- recovery material follows the same ownership, permissions and secret-exclusion requirements as its source;
- retention is bounded and does not become an accumulating general backup history;
- projects, arbitrary user files, large artifacts and generic filesystem trees are not silently copied merely because an update is running.

A bounded state copy does not imply that the complete prior OS can be automatically restored.

## 6. Managed update sequence

The first-ISO conceptual workflow is:

```text
explicit update request
  -> create durable update task + exclusive managed-update lock
  -> basic preflight (native DB/recovery/storage/component prerequisites)
  -> refresh/resolve full pacman transaction without installing targets
  -> validate resolved compatibility + require unchanged approved plan
  -> record expected change set
  -> create required bounded recovery state
  -> apply full Artix/pacman system upgrade + approved component updates
  -> run required schema/config migrations
  -> regenerate affected initramfs/GRUB artifacts
  -> restart/reconcile only the services/components that require it
  -> run post-update validation
       |
       +-> healthy
       |     -> record completion
       |     -> report reboot_required when applicable
       |
       +-> unhealthy/incomplete
             -> record degraded/update-recovery state
             -> preserve evidence and recovery surfaces
             -> require deterministic resume/repair/admin path
```

The researched command ordering is now defined by Sections 4.1, 8.1 and 10.1. Live Artix execution may refine package-specific details, but it must preserve this responsibility ordering and the full-system-upgrade invariant.

## 7. Schema and configuration migrations

`docs/STATE.md` remains authoritative for database versioning and migration semantics.

The update workflow must therefore preserve these rules:

- migrations execute transactionally whenever the underlying store permits;
- destructive/non-trivial SQLite migration requires the consistent pre-migration backup;
- a failed migration prevents the affected runtime from claiming normal state-mutating readiness;
- migration code is versioned with the runtime that understands the resulting schema;
- downgrade compatibility is not assumed;
- a migration must not silently discard durable Portus truth merely to make an upgrade succeed.

Configuration migrations follow the same principle: known changes are explicit and validated; unknown/incompatible configuration does not silently reset to defaults if that would discard administrator-controlled meaning.

## 8. Kernel and boot-artifact update rules

The dual-kernel policy in `docs/HARDWARE.md` is mandatory during updates.

For a kernel-affecting update:

- official Artix `linux-lts` remains the normal/default kernel role;
- official Artix `linux` remains the independently bootable alternate/recovery role;
- updating one must not remove or invalidate the other;
- mkinitcpio artifacts for affected kernels are regenerated through the selected Artix mechanism;
- GRUB configuration continues to expose the intended default and alternate paths;
- boot-artifact generation errors make the update unhealthy/incomplete rather than being ignored;
- the managed workflow must not automatically delete the final alternate kernel to resolve a package conflict.

A package transaction can finish successfully while the overall PortusOS update remains unhealthy if required boot artifacts are not valid.

Automatic boot into a previous historical kernel build and automatic last-known-good rollback are not first-ISO requirements.

### 8.1 mkinitcpio and GRUB command baseline

Current Artix ships mkinitcpio 41.x in the system repository. Upstream mkinitcpio changed its packaging in v40 so **new default kernel presets no longer include a fallback image**. PortusOS requires fallback/recovery-capable initramfs images, so this must be an explicit installed-system invariant rather than an assumed distro default.

The installed system must therefore maintain both kernel preset files under `/etc/mkinitcpio.d/` with `default` and `fallback` presets. Their required semantic shape is:

```text
# /etc/mkinitcpio.d/linux.preset
PRESETS=('default' 'fallback')
ALL_kver='/boot/vmlinuz-linux'
default_image='/boot/initramfs-linux.img'
fallback_image='/boot/initramfs-linux-fallback.img'
fallback_options='-S autodetect'

# /etc/mkinitcpio.d/linux-lts.preset
PRESETS=('default' 'fallback')
ALL_kver='/boot/vmlinuz-linux-lts'
default_image='/boot/initramfs-linux-lts.img'
fallback_image='/boot/initramfs-linux-lts-fallback.img'
fallback_options='-S autodetect'
```

Equivalent syntax generated by the selected Artix/mkinitcpio package is acceptable only if it produces the same four required initramfs artifacts and keeps the Portus BusyBox/non-systemd hook contract from `docs/BOOT_STORAGE_RECOVERY.md`.

Pacman's normal libalpm mkinitcpio hook remains enabled and is allowed to perform the normal package-triggered rebuild. PortusOS does not replace that mechanism. For a managed update that changes either kernel, mkinitcpio itself, initramfs configuration/hooks, cryptsetup/LVM early-boot integration, firmware/microcode material used in initramfs, or another declared early-boot dependency, the deterministic post-transaction rebuild/repair command is:

```text
mkinitcpio -P
```

`-P` processes all presets in `/etc/mkinitcpio.d`. The command must finish successfully and all four required image files must exist and be non-empty before the boot-artifact health gate passes.

After a kernel/initramfs/GRUB configuration change, regenerate and syntax-check the GRUB configuration with:

```text
grub-mkconfig -o /boot/grub/grub.cfg
grub-script-check /boot/grub/grub.cfg
```

Normal kernel updates do **not** run `grub-install`. `grub-install` remains an installation/EFI-repair operation, not a per-kernel-update step.

Post-update validation must prove that generated GRUB configuration still exposes the normal `linux-lts` path and the alternate `linux` path and preserves the configured LTS default. Whether the selected Artix GRUB scripts automatically expose both fallback initramfs images as menu choices is a live-VM verification item. If they do not, PortusOS may add narrow explicit recovery entries through the normal GRUB custom-entry mechanism rather than abandoning the required fallback images.

## 9. Reboot boundary

The update workflow **does not automatically reboot the machine**.

When the applied changes require reboot to become fully active, the update result reports the typed `reboot_required` boolean and finite `reboot_reasons` defined in Section 15.2.

This rule is particularly important because the first-ISO encrypted boot contract requires owner-provided LUKS unlock after reboot.

Therefore:

- successful package installation does not authorize an implicit reboot;
- service restarts that can safely complete the update without reboot may occur under the normal service/recovery policy;
- a reboot is a separate explicit action;
- the system must not intentionally strand itself at the encrypted-unlock boundary as an unnoticed side effect of applying updates.

## 10. Post-update health proof

An update is not healthy merely because pacman or a component installer returned success.

The managed update path validates the domains affected by the change and the critical first-ISO recovery surfaces. Depending on the update, this includes:

- package/version/source state matches the expected transition;
- required Portus schema/configuration is readable at the expected version;
- affected OpenRC services start/reconcile into their expected native state;
- `portusd`, `portus-privd`, `portus-apid` and affected providers report usable/degraded state accurately;
- protected-credential boundaries have not fallen back to plaintext/weaker behavior;
- networking/SSH remain available when they were expected to remain uninterrupted;
- both required kernel/GRUB/initramfs roles remain represented correctly after kernel/boot changes;
- Codex reports the selected pinned version and basic compatibility proof where it changed;
- PortusBrowser's CLI/Broker/native-host/extension/browser integration remains compatible where it changed;
- `portus-os doctor` can explain any remaining degraded or reboot-required condition.

Exact validation commands and evidence belong in `docs/VALIDATION.md`.

### 10.1 OpenRC restart/reconcile command baseline

A successful package transaction does not justify globally resetting the whole OpenRC service graph.

For each affected restart-safe service explicitly named by the update compatibility plan, the normal managed action is:

```text
rc-service <service> restart
rc-service <service> status
```

and the broader before/after evidence uses:

```text
rc-status
```

OpenRC handles declared dependencies for service actions. PortusOS therefore does not duplicate dependency traversal in its update code.

Calling `openrc` with no arguments can reset all services to the current runlevel's desired state, including stopping services that were manually started outside that runlevel. It is consequently a **repair/reconciliation action**, not the default post-update step.

Likewise, `rc-service <service> zap` is reserved for a specifically diagnosed stale/crashed OpenRC service-state condition; it is never routine update cleanup.

Networking and SSH are recovery surfaces. A remote update does not blindly restart NetworkManager, SSH, firewall or other connectivity-critical services merely because their packages changed. Their restart must be explicitly classified by the update plan with a safe recovery path; otherwise the update records the change and requires local/administrator-controlled activation.

## 11. Interrupted or failed update

PortusOS does not pretend that a multi-system package update can be made globally atomic.

If an update is interrupted or post-update validation fails:

- do not report success;
- persist enough update/recovery evidence to identify the expected transition and the last completed stage where Portus-owned state is available;
- do not silently recreate/reset durable state;
- do not automatically roll back the whole filesystem/OS;
- keep TTY/root/OpenRC recovery available and SSH where the underlying system/network remains usable;
- preserve the alternate Artix kernel and live-ISO repair path;
- expose a deterministic resume, reconcile, repair or administrator-required disposition based on the observed state;
- if recovery cannot be proven safe, stop at visible degraded/unavailable state rather than repeatedly mutating the machine.

`docs/HEALTH_RECOVERY.md` remains authoritative for bounded restart/recovery behavior. Generic health logic must not turn an interrupted OS update into an infinite hidden retry loop.

### 11.1 Interrupted pacman repair sequence

The researched first-ISO repair sequence is conservative and evidence-driven.

For an installed system that still boots sufficiently for local/root recovery:

1. inspect the resolved pacman `DBPath` and determine whether `<DBPath>/db.lck` exists;
2. if the lock exists, use `fuser <DBPath>/db.lck` (or the verified equivalent) to prove that no pacman process still owns it;
3. remove a lock file only after it is proven stale;
4. run `pacman -Dk`; a failed consistency check escalates to administrator/live-media repair rather than continuing blindly;
5. compare the durable Portus update plan/stage evidence with `/var/log/pacman.log` and the currently installed package state;
6. when package reapplication is required, rerun the recorded interrupted package-name set through a full sync/upgrade transaction so pacman scriptlets and libalpm hooks execute again, conceptually:

```text
pacman -Syu <recorded-package-names...>
```

7. rerun any still-required Portus schema/config migration according to `docs/STATE.md`;
8. when the recorded change set touched early boot, run the Section 8.1 `mkinitcpio -P`, `grub-mkconfig`, and `grub-script-check` sequence;
9. restart only the explicitly affected restart-safe OpenRC services;
10. rerun the complete post-update health/evidence gate.

The Portus update record is the primary intended change-set evidence; `/var/log/pacman.log` is the independent native corroborating source. Recovery must not depend on fragile log parsing alone when Portus successfully recorded the planned transaction before mutation.

If the installed system is not bootable enough for this path, use the Artix live ISO, unlock the LUKS2 container, activate VG `portus`, mount root plus the separate `/boot` and ESP at `/mnt`, and enter the installed target with Artix's chroot helper (research indicates `artix-chroot /mnt`). The exact device-discovery/mount commands and the presence/path of `artix-chroot` remain blocking live-ISO verification because device names are installation-specific.

Do not automatically delete partially downloaded package files, use `--overwrite`, force database operations, or reinstall GRUB merely because an update was interrupted. Each is a targeted repair only after its corresponding failure is observed.

## 12. Codex update contract

`CODEX_UPDATES.md` remains authoritative for Codex-specific behavior.

The OS-level update contract inherits these requirements:

- Codex is pinned in machine-readable build/update metadata;
- automatic update checks/startup upgrades are disabled;
- changing the selected pin is an explicit workflow;
- a new candidate is validated in a disposable/reference-compatible VM before the PortusOS pin advances;
- the supported workflow records the resulting Codex version;
- `codex update`, when supported, may be used only inside the explicit upgrade workflow and never as a hidden startup action.

Codex does not drift independently of the PortusOS-supported component set.

## 13. PortusBrowser update contract

PortusBrowser remains an independently owned component integrated by PortusOS.

A supported PortusBrowser update must treat its cooperating pieces as a compatibility set rather than blindly updating one piece and assuming the others remain compatible.

The update/validation path must account for, as applicable:

- `portus-browser` CLI;
- Broker;
- native-messaging host;
- extension assets/version;
- protocol-envelope compatibility;
- native-messaging registration;
- selected Chromium reference browser compatibility;
- PortusOS provider/health integration.

PortusOS owns the pin/source/installation/compatibility gate and post-update proof, while PortusBrowser continues to own browser-domain implementation and policy semantics.

## 14. Backup and rollback boundary

The following are **not** first-ISO features:

- scheduled general backup automation;
- remote backup-provider integration;
- complete filesystem/system snapshot rollback;
- automatic rollback to the previous OS state after a failed update;
- automatic last-known-good kernel selection;
- sophisticated unattended rolling-upgrade orchestration.

PortusOS therefore does not introduce Btrfs/snapshot infrastructure merely to manufacture first-ISO update rollback.

The first ISO relies on:

- preflight;
- bounded update-specific recovery copies where necessary;
- transactional migrations where possible;
- dual Artix kernel paths;
- explicit degraded state;
- TTY/SSH/OpenRC/root recovery;
- live-ISO repair.

A future backup/rollback design may add stronger guarantees without changing these first-ISO recovery principles.

## 15. Audit and evidence

Managed updates are significant system operations and must leave bounded, secret-safe evidence sufficient to answer:

- who/what requested the update;
- what package/component transition was intended;
- which preflight checks passed/failed;
- which bounded recovery material was created;
- which update stages completed;
- what post-update checks ran and their disposition;
- whether reboot is required;
- whether the system ended healthy, degraded, unavailable or administrator-required.

Do not copy reusable secrets, package-manager credential material or protected-provider values into update logs/evidence.

Event/audit representation remains subordinate to `docs/EVENTS_AUDIT.md` and task lifecycle remains subordinate to `docs/TASKS.md`. The update-specific plan/result fields are locked below and must be serialized through those existing boundaries rather than creating a second lifecycle store.

### 15.1 Update compatibility metadata

A managed update is a Portus task under `docs/TASKS.md`; it does **not** create a parallel update identity/lifecycle. The existing immutable `task_<UUIDv7>` is the durable operation identity, and `succeeded`, `failed`, `cancelled`, or `interrupted` remain the terminal task states.

Update-specific progress is an orthogonal bounded phase value:

```text
preflight
resolve
prepare_recovery
apply_packages
migrate
reconcile_services
rebuild_boot
validate
repair
complete
```

The first update-plan/evidence schema is version `1`. At minimum it records/references:

- task ID and resolution timestamp;
- resolved pacman configuration/repository evidence;
- pre-update installed package manifest/evidence reference;
- planned package targets: name, current version when installed, target version, repository, package filename, SHA-256, size and location;
- whether package sync databases were refreshed during resolution;
- required Portus runtime/state-schema transition, if any;
- provider compatibility dimensions already defined by `docs/CAPABILITIES.md`: provider software version, manifest schema version, interface contract version and capability contract version;
- Codex pin before/after and its validation reference where changed;
- PortusBrowser CLI/Broker/native-host/extension/protocol/reference-browser compatibility set where changed;
- required pre-migration recovery copy;
- exact OpenRC services approved for restart;
- whether initramfs regeneration, GRUB regeneration or later explicit reboot is required;
- preflight and post-update evidence references.

Provider compatibility continues to use the existing `compatible` / `incompatible` / `unknown` states. Update code must not invent a competing provider-compatibility vocabulary.

Large plan/package/evidence payloads become a deliberate secret-safe Portus artifact under `docs/ARTIFACTS.md`; the task result keeps only bounded summaries and artifact/reference IDs.

### 15.2 `reboot_required` result representation

The first typed update-result representation includes:

```json
{
  "reboot_required": true,
  "reboot_reasons": ["kernel_changed"]
}
```

`reboot_reasons` is empty when `reboot_required=false` and non-empty when it is true. The first finite reason set is:

```text
kernel_changed
microcode_changed
boot_chain_changed
component_contract_requires_reboot
```

This flag is independent of task terminal state. A managed update may complete successfully with `reboot_required=true`: the update has been safely applied and validated as far as possible in the running boot, but activation of the changed kernel/microcode/boot-chain component requires a later explicit reboot and owner LUKS unlock.

The result also carries/references the final update phase, task terminal state, resulting system health observation, recovery disposition when non-healthy, changed-package count, newly created `.pacnew`/`.pacsave` evidence, and the update evidence artifact/reference. It does not embed unbounded pacman output.

### 15.3 `.pacnew` / `.pacsave` handling

Pacman deliberately leaves administrator-modified configuration in place and may write new package configuration as `.pacnew`; removal/upgrade paths may create `.pacsave`. PortusOS must not auto-merge or auto-overwrite these files.

Before and after package application, record the bounded `/etc` set returned by:

```text
find /etc \( -name '*.pacnew' -o -name '*.pacsave' \) -type f -print
```

New files are included in update evidence. A newly created `.pacnew`/`.pacsave` affecting a required first-ISO domain—boot/initramfs, pacman/repositories, OpenRC, Portus configuration/policy, SSH/network/firewall, protected providers or another update-critical service—prevents the managed update from claiming fully healthy state until explicitly reconciled. Non-critical files remain visible administrative follow-up evidence.

### 15.4 Executable update-validation evidence packet

`docs/VALIDATION.md` now owns the complete ISO-01 through ISO-38 execution/evidence matrix and imports this update slice directly into ISO-05/release update qualification.

Before mutation, capture at minimum:

```text
pacman --version
pacman-conf --repo-list
pacman-conf DBPath CacheDir GPGDir HookDir SigLevel HoldPkg IgnorePkg IgnoreGroup
pacman -Dk
pacman -Q
rc-status
find /etc \( -name '*.pacnew' -o -name '*.pacsave' \) -type f -print
```

For the dual-kernel/boot baseline also record file metadata/checksums for:

```text
/boot/vmlinuz-linux-lts
/boot/initramfs-linux-lts.img
/boot/initramfs-linux-lts-fallback.img
/boot/vmlinuz-linux
/boot/initramfs-linux.img
/boot/initramfs-linux-fallback.img
/boot/grub/grub.cfg
```

Record the planned transaction with the Section 4.1 `pacman -Syu --print --print-format ...` command before package mutation.

After package application, capture at minimum:

```text
pacman -Dk
pacman -Q
rc-status
find /etc \( -name '*.pacnew' -o -name '*.pacsave' \) -type f -print
```

For each explicitly restarted service:

```text
rc-service <service> status
```

For an early-boot/kernel-affecting update, execute and capture:

```text
mkinitcpio -P
grub-mkconfig -o /boot/grub/grub.cfg
grub-script-check /boot/grub/grub.cfg
```

then prove all required kernel/initramfs files exist and are non-empty, and prove the generated GRUB configuration still contains the normal `linux-lts` and alternate `linux` kernel paths.

Finally run the relevant Portus health/provider checks, including `portus-os doctor`, and record the typed update result including `reboot_required`/reasons. The update validation step itself does not reboot. ISO-05's controlled explicit reboot/alternate-kernel boot proof remains a separate owner-unlocked acceptance action.

## 16. Remaining live Artix/VMware verification

The research/engineering baseline is now selected. The remaining work is execution evidence on the actual Artix/VMware candidate rather than another design pass:

- verify the selected Artix pacman build's exact `pacman-conf`, `-Dk`, `-Syu --print`, and managed `-Su --noconfirm` behavior, including prompts/replacements/removals and the split resolve/apply invariant; prove whether `--print` is complete enough for plan approval or freeze the narrow libalpm read-only add/remove resolver instead;
- freeze the actual enabled repositories, per-repository signature settings, DB/cache/keyring/hook paths and keyring bootstrap behavior;
- measure the free-space requirement for the resolved package transaction, cache/temp use, bounded migration copy and four initramfs images on the minimum VM;
- verify that current Artix mkinitcpio/libalpm hooks materialize/preserve the Portus-owned `linux.preset` and `linux-lts.preset` fallback requirements across both kernel updates and mkinitcpio updates;
- prove `mkinitcpio -P` creates all four required images under the selected BusyBox/non-systemd hook configuration;
- prove `grub-mkconfig` + `grub-script-check` preserve LTS default and the alternate `linux` entry, and determine whether fallback-image menu entries need a narrow Portus custom GRUB entry;
- freeze actual OpenRC service identities and dependency-safe restart ordering for each first-ISO component; test that the normal update path does not need global `openrc` reconciliation;
- execute a controlled interrupted pacman transaction fixture and prove stale-lock detection, `pacman -Dk`, recorded-plan/log reconciliation, package reapplication, boot-artifact rebuild and final health behavior;
- verify the Artix live-ISO encrypted-root mount/chroot repair path, including the exact `artix-chroot` availability/command;
- implement/freeze the update-plan evidence schema version 1, bounded task result and finite reboot-reason serialization in the runtime/CLI wire contract;
- freeze PortusBrowser's exact component-version/pin compatibility representation using the existing provider compatibility contract;
- import the Section 15.4 update evidence packet into `docs/VALIDATION.md` when that whole-ISO authority is created.

These live gates may refine command syntax or package-specific wiring. They must not silently reintroduce background updating, automatic reboot, partial-upgrade behavior, or automatic whole-system rollback.

## 17. First-ISO acceptance implications

The first-ISO validation/release work must be able to prove at minimum that:

- no supported background/startup path silently upgrades Artix packages, Codex or PortusBrowser;
- a managed update preflight can refuse an unsafe transition before destructive mutation;
- a destructive/non-trivial Portus database migration produces the required bounded pre-migration backup and fails closed if migration cannot complete safely;
- a kernel-affecting update preserves both Artix kernel roles and valid boot artifacts;
- post-update validation distinguishes package success from overall PortusOS health;
- an update requiring reboot reports that fact without automatically rebooting;
- interrupted/failed update state remains diagnosable and does not trigger automatic whole-system rollback or silent durable-state reset;
- TTY/OpenRC/root and available SSH/alternate-kernel/live-ISO recovery remain independent.

These proofs may be incorporated into the executable matrix in `docs/VALIDATION.md`; this document does not require adding a separate backup or automatic-rollback feature family to the 38-test first-ISO contract.

## 18. Decisions locked by this document

1. First-ISO OS/package updates are explicit authorized operations; no background/startup automatic OS upgrade path is supported.
2. Artix/pacman remains authoritative for Artix package state; PortusOS adds coordination rather than a replacement package manager.
3. The supported managed update path performs preflight before destructive mutation.
4. Required preflight includes source/keyring/package state, storage, compatibility/migration readiness, recovery surfaces, kernel-role protection, and component-specific gates as applicable.
5. Destructive/non-trivial `portus.db` migration uses the bounded consistent pre-migration backup already required by `docs/STATE.md`.
6. Update-specific bounded recovery copies do not constitute general backup automation and must remain scoped, secret-safe and retention-bounded.
7. Interrupted external/package update work is recovered through observed-state resume/reconcile/repair; global atomicity is not falsely claimed.
8. Post-update health validation is mandatory for the supported managed path; package-manager success alone is insufficient.
9. Failed/incomplete updates enter visible degraded/unavailable/administrator-required recovery rather than automatic whole-system rollback.
10. Kernel-affecting updates preserve both official Artix kernel roles and regenerate/validate required boot artifacts.
11. PortusOS does not automatically reboot after an update; reboot is a separate explicit action and a reboot-required condition is reported when applicable.
12. Codex updates remain explicit, pinned and compatibility-validated under `CODEX_UPDATES.md`.
13. PortusBrowser updates are pinned/controlled and validated as a cooperating component set rather than independent uncoordinated drift.
14. Scheduled general backup automation, remote backup integration, complete system rollback, automatic last-known-good selection and sophisticated unattended upgrade orchestration are deferred.
15. The first-ISO recovery model remains preflight + bounded state protection + dual kernels + health/degraded state + TTY/SSH/OpenRC/root/live-ISO repair rather than snapshot-based automatic rollback.
16. Managed Artix updates preserve a full-system-upgrade transaction; the first resolver candidate is native `-Syu --print`, but it is accepted only if live proof shows complete replacement/removal visibility. If not, a narrow read-only libalpm planner supplies the complete add/remove plan while pacman remains the mutation authority; the same-task apply baseline is `-Su --noconfirm` only after exact plan re-match.
17. Pacman database consistency uses `pacman -Dk`; a package DB lock is removed only after proving it stale.
18. Routine post-update service handling is targeted `rc-service <service> restart` + `status`; global `openrc` reset and `zap` are repair-only mechanisms.
19. PortusOS explicitly maintains `default` + `fallback` mkinitcpio presets for both `linux-lts` and `linux`; `mkinitcpio -P` is the all-preset rebuild baseline.
20. Affected GRUB configuration is regenerated with `grub-mkconfig -o /boot/grub/grub.cfg` and syntax-checked with `grub-script-check`; routine kernel updates do not run `grub-install`.
21. Managed update identity/lifecycle reuses `task_<UUIDv7>` and the canonical task terminal states rather than creating a separate update state machine.
22. The first update-plan/evidence schema version is `1`; provider compatibility reuses the existing software/manifest/interface/capability version dimensions and `compatible`/`incompatible`/`unknown` states.
23. `reboot_required` is a boolean with finite reasons `kernel_changed`, `microcode_changed`, `boot_chain_changed`, or `component_contract_requires_reboot`; it never authorizes an implicit reboot.
24. Newly created `.pacnew`/`.pacsave` files are recorded and never auto-merged/overwritten; required-domain conflicts prevent a fully healthy update result until reconciled.
25. Section 15.4 is the executable update evidence packet imported by the authoritative whole-ISO `docs/VALIDATION.md` matrix.
