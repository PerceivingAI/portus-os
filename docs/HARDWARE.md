# PortusOS Kernel, Firmware, Drivers and Hardware Contract

**Last reviewed:** 2026-08-29T01:24:50Z
**Last updated:** 2026-08-29T01:24:50Z

**Status:** Authoritative for the first-ISO kernel policy and VMware-first engineering baseline; exact firmware/microcode packages and `open-vm-tools` sourcing/necessity still require Linux verification
**Target:** x86_64 VMware-first PortusOS first ISO
**Acceptance authority:** `docs/ACCEPTANCE.md`
**Boot/storage authority:** `docs/BOOT_STORAGE_RECOVERY.md`
**Package authority:** `docs/PACKAGE_POLICY.md`
**GUI authority:** `docs/GUI_SYSTEM.md`

## 1. Purpose and authority

This document owns the first-ISO kernel, firmware, driver and hardware baseline.

The kernel policy is locked:

> **PortusOS uses the official Artix `linux-lts` package as its normal/default kernel and keeps the official Artix `linux` package installed as an alternate/recovery kernel.**

PortusOS does not fork, independently patch, or independently maintain either kernel line for the first ISO. Artix remains responsible for the kernel packages and their normal package lifecycle.

The VMware baseline prefers in-kernel/native VMware device support and ordinary Xorg modesetting first, requires no 3D acceleration, and treats `open-vm-tools` as a case-review item rather than an assumed dependency.

## 2. Kernel policy

### 2.1 Normal kernel

```text
Artix official package: linux-lts
role: normal/default PortusOS kernel
```

The LTS line is preferred because the first ISO prioritizes stability, recoverability and prolonged unattended operation over immediate adoption of the newest kernel feature set.

### 2.2 Alternate/recovery kernel

```text
Artix official package: linux
role: alternate/recovery kernel
```

The regular Artix kernel is not a Portus-maintained historical rollback image. It is a separately maintained official Artix kernel line retained to provide an alternate boot path when the normal LTS path is unusable or under diagnosis.

### 2.3 No Portus kernel fork

For the first ISO:

- no Portus kernel fork;
- no independent Portus kernel patch queue merely for differentiation;
- kernel security/reliability updates come from the selected official Artix package sources;
- a future custom/hardened kernel requires a separate justified architecture decision.

## 3. Bootability and recovery requirements

Both installed kernel lines must remain independently bootable through the GRUB/mkinitcpio design in `docs/BOOT_STORAGE_RECOVERY.md`.

The live ISO boot payload is intentionally narrower than the installed kernel set. The first-ISO `artools` adapter selects `linux-lts` for the live kernel/initramfs path while leaving both `linux-lts` and `linux` untouched in the installable rootfs. This avoids upstream `artools 0.39.1-1`'s one-kernel `make_bootfs()` assumption without weakening installed alternate-kernel recovery.

Requirements:

- `linux-lts` has valid kernel, modules, normal initramfs, broader fallback/recovery-capable initramfs and GRUB entry;
- `linux` has the same independent artifact set;
- normal/default boot selects `linux-lts`;
- `linux` remains explicitly selectable;
- updating one kernel must not remove or invalidate the other;
- kernel-affecting updates must regenerate/validate relevant boot artifacts before the update is considered healthy.

Automatic kernel rollback or automatic last-known-good selection is not required for the first ISO.

## 4. Relationship to update and recovery policy

The two-kernel policy is a bounded recovery primitive, not a complete rollback system.

```text
normal operation
  -> linux-lts

if normal kernel path is suspect/unusable
  -> owner/admin selects Artix linux alternate
  -> reaches recovery-capable installed system
  -> diagnoses/repairs normal path
```

`docs/UPDATES_RECOVERY.md` owns package-retention, explicit-update/preflight, no-auto-reboot, post-update boot proof and failed-update repair semantics; exact Artix commands remain Linux-verified.

## 5. VMware-first engineering baseline

The first validation target remains x86_64 VMware.

Engineering order:

1. use the official Artix kernels' in-tree VMware virtual-device support first;
2. prove storage, networking, input and X11/i3 using normal kernel/Xorg mechanisms;
3. use the ordinary Xorg modesetting path first and require no 3D acceleration;
4. add a dedicated VMware X driver only if current official Artix packages and measured behavior prove it is necessary or materially improves the supported path;
5. do not pull a VMware component from AUR/Arch merely because it is convenient.

Expected virtual-device capability domains:

- virtual CPU;
- VMware storage controller support through in-tree kernel drivers where available;
- VMware network adapter support through in-tree kernel drivers where available;
- virtual graphics through ordinary Xorg/modesetting first;
- keyboard/pointer input;
- clock/time behavior coordinated with the selected Linux time-sync authority;
- shutdown/reboot behavior.

Suspend/resume, 3D acceleration, passthrough, multi-GPU, multi-monitor and broad graphics qualification are not first-ISO requirements.

### 5.1 `open-vm-tools` case review

`open-vm-tools` is **not assumed to be required** and is not automatically sourced outside Artix.

The Linux/VMware pass must first determine which blocking requirements, if any, actually depend on it, for example guest shutdown coordination, display/clipboard conveniences or time integration.

- if all blocking requirements work cleanly through kernel/native mechanisms, omit it;
- if a blocking requirement needs it and official Artix does not provide an acceptable package, it enters the explicit case-by-case sourcing review in `docs/PACKAGE_POLICY.md`;
- AUR/Arch fallback is not permitted.

## 6. Firmware and microcode

The exact official Artix firmware and CPU-microcode package set remains to be verified against the reference VM and generic x86_64 baseline.

Rules:

- use official Artix packages when selected and available;
- include Intel/AMD microcode according to the final hardware-selection policy rather than inventing a Portus microcode mechanism;
- preserve mkinitcpio's normal `microcode` hook path;
- firmware inclusion remains capability-driven rather than a maximal hardware catalogue.

## 7. Driver and DKMS boundary

Prefer in-tree kernel drivers for the first ISO.

DKMS is not part of the baseline merely for flexibility. Add DKMS only when an explicitly approved required driver actually needs it, then verify the module against both `linux-lts` and `linux`.

Audio/PipeWire and graphical/user-input lifecycle are owned by `docs/GUI_SYSTEM.md`.

No driver choice may introduce a required systemd dependency into the Artix/OpenRC baseline.

## 8. Minimum diagnostics

The installed system and recovery docs must preserve normal Linux diagnostics sufficient to inspect:

- running kernel identity;
- installed kernel packages;
- GRUB entries and initramfs artifacts;
- loaded modules;
- kernel/driver logs;
- detected PCI/virtual devices;
- VMware integration state where applicable.

PortusOS may aggregate these into `doctor`, but does not replace native Linux diagnostics.

## 9. Remaining Linux verification

Still to verify/freeze:

- release-candidate versions of `linux-lts` and `linux` from official Artix;
- exact firmware package inventory;
- Intel/AMD microcode package/selection policy;
- exact VMware virtual storage/network drivers observed in the reference VM;
- exact Xorg modesetting behavior and whether any dedicated official-Artix VMware X driver is justified;
- whether `open-vm-tools` is available from official Artix and whether any blocking first-ISO requirement actually needs it;
- if needed and unavailable from Artix, the explicit `open-vm-tools` source review under `docs/PACKAGE_POLICY.md`;
- any required DKMS case;
- minimum generic x86_64 assumptions retained for future bare metal;
- exact hardware/VM diagnostics used by validation and `portus-os doctor`.

The GRUB/mkinitcpio details are no longer an open hardware design question; they are governed by `docs/BOOT_STORAGE_RECOVERY.md` and require implementation verification there.

## 10. First-ISO acceptance implications

Validation must demonstrate at minimum that:

- both official Artix kernels are installed;
- normal boot uses `linux-lts`;
- the `linux` alternate reaches a usable recovery-capable installed system;
- kernel updates/configuration do not silently remove either role;
- reference VMware storage/network/input/display work under `linux-lts`;
- the alternate kernel supports enough required VMware devices to perform its recovery role;
- X11/i3 reaches the required GUI without VMware 3D acceleration;
- no unsupported AUR/Arch VMware package is required for acceptance.

## 11. Decisions locked by this document

1. Official Artix `linux-lts` is the normal/default first-ISO kernel.
2. Official Artix `linux` remains installed as the alternate/recovery kernel.
3. Both kernel lines remain independently bootable.
4. PortusOS relies on Artix kernel maintenance rather than maintaining a custom first-ISO kernel.
5. Kernel-affecting updates preserve and validate both paths; automatic rollback is not required.
6. VMware is the first validation target; broad bare-metal certification remains deferred.
7. Use in-tree/native VMware kernel support first.
8. Use ordinary Xorg/modesetting first; no first-ISO 3D-acceleration dependency.
9. Add a dedicated VMware X driver only if official Artix availability and measured behavior justify it.
10. `open-vm-tools` is not assumed required; necessity and official-Artix availability are verified first, and any non-Artix inclusion requires explicit case review.
11. AUR/Arch is not a VMware-package fallback.
12. DKMS is not baseline; it is added only for an approved required driver that demonstrably needs it.
