# PortusOS Boot, Encryption, Storage and Recovery Contract

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative for first-ISO encrypted boot behavior, storage topology, and default boot/storage engineering baseline; exact command/package behavior and recovery procedures still require Linux verification
**Target:** x86_64 VMware first ISO
**Acceptance authority:** `docs/ACCEPTANCE.md`
**Installer authority:** `docs/ISO_BUILD_INSTALLER.md`
**Kernel/hardware authority:** `docs/HARDWARE.md`

## 1. Purpose and authority

This document owns the first-ISO boot, encryption, storage and low-level recovery contract.

The project locks both the encrypted reboot boundary and the first-ISO storage topology:

> **The first ISO prioritizes strong owner-unlocked encrypted boot. It does not claim automatic unattended completion of a cold boot or reboot while the encrypted system is locked.**

> **The first ISO uses a simple GPT/UEFI layout with a FAT32 EFI System Partition, an unencrypted ext4 `/boot`, and the remaining system storage inside LUKS2 with LVM containing an ext4 root logical volume and encrypted swap logical volume.**

The default engineering baseline now selects GRUB/UEFI, a 512 MiB ESP, 2 GiB `/boot`, LUKS2 AES-XTS/Argon2id, a simple `portus` LVM layout, 4 GiB encrypted swap, mkinitcpio, and dual-kernel GRUB entries. Exact Artix package behavior, generated command lines, repair commands, and installer integration still require Linux/VMware verification.

## 2. Firmware boundary

`docs/ACCEPTANCE.md` remains authoritative for the first-ISO firmware boundary:

```text
architecture    x86_64
firmware        UEFI
Secure Boot     disabled/not required
legacy BIOS     not required
partitioning    GPT
```

The selected storage/boot design must satisfy that boundary and boot from the installed virtual disk after the ISO is removed.

## 3. First-ISO disk/storage topology

The first-ISO installed disk topology is:

```text
GPT disk
│
├─ EFI System Partition
│    size: 512 MiB
│    filesystem: FAT32
│    mount: /boot/efi
│    encryption: none
│
├─ /boot
│    size: 2 GiB
│    filesystem: ext4
│    encryption: none
│
└─ LUKS2 container
     │
     └─ LVM volume group: portus
          ├─ root logical volume
          │    mount: /
          │    filesystem: ext4
          │    size: remaining allocatable space
          │
          ├─ swap logical volume
          │    size: 4 GiB
          │    encrypted by being inside LUKS2
          │
          └─ ~5% VG space left unallocated
```

For the first ISO:

- the EFI System Partition uses **FAT32** because it is the firmware interoperability partition read directly by UEFI before Linux filesystems, LUKS, LVM or OpenRC are available;
- FAT32 is not the PortusOS root filesystem and is not used for normal OS/user data;
- `/boot` is a separate **unencrypted ext4** filesystem;
- the main installed system uses **LUKS2**;
- **LVM is inside LUKS2**;
- the root logical volume uses **ext4**;
- swap is a **4 GiB** logical volume inside the encrypted LVM stack;
- no separate `/home`, `/var`, `/srv` or similar logical volumes are required for the first ISO;
- hibernation support is not a first-ISO requirement;
- roughly **5% of the VG remains unallocated** as a simple maintenance/recovery reserve, not as an automatic snapshot pool.

### 3.1 Why the EFI System Partition is FAT32

The EFI System Partition exists for UEFI firmware and EFI executables, not as the general PortusOS filesystem.

Its first-ISO role is limited to firmware-readable boot material such as EFI loader files. FAT32 is therefore selected for compatibility and predictability with the UEFI/VMware boot path.

Using ext4 for the ESP would make boot depend on firmware support for a Linux filesystem that is not part of the normal UEFI interoperability contract. PortusOS instead keeps Linux-native filesystems behind the firmware handoff.

### 3.2 Why `/boot` is outside LUKS2

Keeping `/boot` unencrypted avoids making the first-ISO boot chain depend on the bootloader's ability to unlock and understand the selected LUKS2 parameters before the kernel/initramfs is running.

This reduces bootloader/encryption coupling and keeps live-ISO repair simpler while the actual root filesystem, system state, user data and swap remain protected by LUKS2.

Secure Boot is already outside the first-ISO contract, so encrypting `/boot` would add complexity without by itself creating a complete pre-boot integrity guarantee.

### 3.3 Size and filesystem rationale

The **512 MiB ESP** intentionally exceeds Calamares' generic default recommendation while remaining trivial relative to the first-ISO disk size.

The **2 GiB `/boot`** is intentionally generous enough for both Artix kernel lines plus normal and fallback/recovery-capable initramfs artifacts.

The root ext4 filesystem stays close to normal distro defaults: ordinary journaling, normal fsck support, and `relatime`. PortusOS does not add exotic ext4 features or first-ISO performance flags without measured need.

No thin provisioning is required.

## 4. Encrypted boot and unattended-operation decision

The installed first-ISO system must use real LUKS2-protected storage for the main system volume.

After a cold boot or reboot, an owner-provided unlock credential is required before the encrypted installed system can continue into the normal root filesystem/OpenRC startup path.

For the first ISO:

- no reusable disk-unlock key is embedded in the installed system merely to obtain automatic boot;
- no network-bound automatic unlock is required or accepted as the normal boot path;
- no TPM-based automatic unlock is required or accepted as the normal boot path;
- no hidden plaintext fallback is allowed when encrypted unlock is unavailable;
- the normal unlock is performed through the local/VMware console path before normal installed-system services become available.

This means a remotely requested reboot or unexpected power cycle may leave the machine waiting at encrypted-disk unlock until the owner reaches the VMware/local console and supplies a valid unlock credential.

For the first ISO, **unattended operation means the machine can run without continuous physical interaction after successful encrypted boot; it does not mean the machine can autonomously cross the encrypted pre-boot boundary after every reboot.**

### 4.1 LUKS2 engineering baseline

The first-ISO default encryption parameters are:

```text
format          LUKS2
data cipher     AES-XTS-plain64
XTS key size    512 bits
PBKDF           Argon2id
target time     ~2000 ms
PBKDF memory    cap at 256 MiB (262144 KiB)
```

Cryptsetup should benchmark Argon2id within those bounds rather than hard-coding a fixed iteration count. The 256 MiB memory cap is intentionally conservative for the 4 GiB minimum reference VM while retaining a memory-hard KDF.

The exact installed Artix cryptsetup build and effective parameters must be recorded during validation. A materially different value discovered at runtime is not silently accepted as equivalent.

Remote/unattended encrypted unlock may be researched later only if it preserves this security model rather than weakening encryption for convenience.

## 5. Recovery credential/keyslot

The first-ISO encrypted-storage design uses:

- one normal owner passphrase keyslot;
- one independent high-entropy recovery-credential keyslot.

Requirements:

- the recovery credential must not be stored on the installed encrypted machine in a form that silently enables automatic unlock;
- possession/use remains owner/administrator recovery authority rather than Master Portus authority;
- installer and recovery logs must not record reusable unlock/recovery values;
- the selected static `Storage & Recovery` Calamares page collects an independent masked recovery credential separately from the owner passphrase; `portus-storage` consumes it through anonymous Linux memory when adding the second LUKS keyslot and does not persist it as an automatic-unlock key file. Recovery-credential storage guidance and later rotation procedure still require live/recovery validation.

## 6. GRUB, initramfs and boot flow

The first ISO uses **GRUB 2 in UEFI mode** as the installed EFI bootloader.

Baseline:

- ESP mount: `/boot/efi`;
- bootloader ID: `PortusOS`;
- also provide the standard x86_64 EFI fallback path `EFI/BOOT/BOOTX64.EFI` for firmware/recovery robustness;
- normal/default top-level kernel: `/boot/vmlinuz-linux-lts` via `GRUB_TOP_LEVEL` or the verified Artix-equivalent generated configuration;
- `GRUB_DEFAULT=0`;
- approximately **5-second visible menu timeout**;
- official Artix `linux` remains visibly selectable as the alternate/recovery kernel;
- no saved-last-boot or hidden-menu mechanism is required;
- `os-prober` is not required for the PortusOS first-ISO installation path.

The first ISO uses **mkinitcpio with the non-systemd/BusyBox-style boot path**.

Intended normal-image hook order:

```text
base udev autodetect microcode modconf kms keyboard keymap block encrypt lvm2 filesystems fsck
```

`encrypt` precedes `lvm2` because the LVM volume group lives inside LUKS2. No `resume` hook is required because hibernation is outside first-ISO scope.

Both `linux-lts` and `linux` require normal and recovery-capable initramfs artifacts:

- normal images may use hardware autodetection;
- fallback/recovery-capable images omit/bypass autodetection so they retain broader module coverage;
- leave mkinitcpio's normal zstd compression behavior unchanged unless Artix verification requires otherwise.

The update/recovery research in `docs/UPDATES_RECOVERY.md` now locks the required preset/artifact semantics and native rebuild commands: because mkinitcpio v40+ no longer enables fallback images in new default presets, PortusOS explicitly requires `default` + `fallback` presets for both `linux-lts` and `linux`, yielding `/boot/initramfs-linux-lts.img`, `/boot/initramfs-linux-lts-fallback.img`, `/boot/initramfs-linux.img`, and `/boot/initramfs-linux-fallback.img`. `mkinitcpio -P` is the deterministic all-preset rebuild/repair command; after kernel/initramfs/GRUB configuration changes, `grub-mkconfig -o /boot/grub/grub.cfg` followed by `grub-script-check /boot/grub/grub.cfg` is the configuration regeneration/syntax-check baseline. Normal kernel updates do not routinely run `grub-install`. The exact kernel command line, Artix package-hook persistence behavior, generated menu shape and live execution remain Linux-verified implementation details.

The conceptual installed boot flow is:

```text
power on / reboot
  -> UEFI reads FAT32 ESP
  -> GRUB
  -> selected kernel + initramfs from unencrypted ext4 /boot
  -> owner unlocks LUKS2
  -> LVM activates
  -> ext4 root + encrypted swap become available
  -> OpenRC
  -> networking / SSH / Portus and other required machine services
  -> local Master presentation path where applicable
```

Ordinary installed-system SSH is not expected to solve a machine that is still stopped at encrypted pre-boot unlock.

## 7. Live-ISO and recovery relationship

The live-ISO recovery route must remain capable of reaching and unlocking the installed encrypted storage with an authorized owner credential independently of Master Portus and the installed graphical session.

The supported first-ISO installer is Calamares under `docs/ISO_BUILD_INSTALLER.md`.

Calamares must implement this topology and collect disk-unlock/recovery material through a secret-safe path. Direct inspection of signed Artix Calamares `3.4.2-4` and the exact 3.4.2 partition source proved that its automated/custom partition-layout path does not create the required LVM VG/LVs; volume-group creation is reached through the manual partition editor. Requiring that undocumented manual construction would violate the accepted installer path.

PortusOS therefore selects exactly one verified-gap job, `portus-storage`. Its prepare mode creates the explicit-target GPT layout, separate ESP and `/boot`, LUKS2 container, VG `portus`, root and 4-GiB swap LVs plus free-VG reserve, and two independent LUKS keyslots. Its finalize mode writes only non-secret dynamic `fstab`, `crypttab`, mkinitcpio and GRUB storage state needed for LVM-inside-LUKS. The recovery value is supplied through anonymous memory for `luksAddKey`; stock `luksbootkeyfile` is not used because embedded automatic-unlock material conflicts with this document.

The custom job does not become a general-purpose credential UI or installer. Signed stock `notesqml` hosts the static Portus `Storage & Recovery` page, which explicitly supplies the target and separate owner/recovery credentials through Calamares GlobalStorage. A non-destructive `portus-storage@preflight` validates the current target and computes the size-bound plan hash; `prepare` immediately revalidates it before destructive work. Root/admin and Master credentials are never silently reused for recovery.

## 8. Remaining boot/storage verification

The engineering direction is selected. The following still require direct verification/freeze on the selected Artix/OpenRC reference environment:

- exact GRUB package/version and generated UEFI installation commands/config paths;
- exact release-candidate mkinitcpio build plus verification that Artix/libalpm kernel and mkinitcpio updates preserve/materialize the Portus `default` + `fallback` preset invariant for both kernels;
- exact generated kernel command line and proof that all four required initramfs artifacts are produced by `mkinitcpio -P` under the selected BusyBox/non-systemd hook configuration;
- exact effective cryptsetup parameters produced by the selected Artix build under the 2-second/256-MiB Argon2id policy;
- live proof of the implemented Calamares `Storage & Recovery`/GlobalStorage/preflight path, including QML loading, target enumeration, bypass rejection, credential clearing and recovery-credential storage guidance;
- destructive VM proof that the selected `portus-storage` implementation actually produces the 512 MiB ESP, 2 GiB `/boot`, LUKS2 container, `portus` VG, 4 GiB swap and ~5% free-VG reserve with the intended effective cryptsetup parameters and two usable keyslots;
- exact live-ISO unlock/mount/`artix-chroot` recovery sequence;
- live proof of the locked `mkinitcpio -P` + `grub-mkconfig` + `grub-script-check` repair sequence, including LTS-default/alternate-kernel menu behavior and whether narrow custom fallback entries are required;
- controlled failed/interrupted-update recovery behavior that intersects the boot chain;
- later remote/unattended unlock architecture, if the product ever requires it.

These items may refine command syntax or package-specific wiring but must not silently replace the engineering baseline without evidence of a concrete incompatibility.

## 9. First-ISO acceptance implications

The first-ISO acceptance run must prove:

- GPT/UEFI boot with a 512 MiB FAT32 ESP mounted at `/boot/efi`;
- separate 2 GiB unencrypted ext4 `/boot`;
- LUKS2 with the intended cipher/PBKDF policy containing LVM;
- VG `portus`, ext4 root LV, 4 GiB swap LV and the intended free-VG reserve;
- owner unlock after fresh boot and reboot;
- separate recovery credential/keyslot unlock;
- GRUB normal boot selects `linux-lts` and the `linux` alternate remains explicitly bootable;
- both kernel lines have valid normal/recovery-capable initramfs artifacts;
- without an unlock credential, the system does not silently bypass LUKS2;
- after unlock, OpenRC, TTY, networking and SSH become available according to their contracts;
- live-ISO/equivalent recovery can reach the encrypted installed storage independently of Master Portus.

ISO-04, ISO-05 and ISO-36 in `docs/ACCEPTANCE.md` remain the blocking acceptance tests that cover this chain; `docs/VALIDATION.md` now provides the executable procedure/evidence contract and leaves device-specific discovery values to the live fixture.

## 10. Decisions locked by this document

1. GPT/UEFI is the first-ISO disk/firmware path.
2. The ESP is 512 MiB FAT32, unencrypted, mounted at `/boot/efi`.
3. `/boot` is a separate 2 GiB unencrypted ext4 filesystem.
4. The main installed-system storage uses LUKS2.
5. LVM VG `portus` is inside LUKS2.
6. The root logical volume uses ext4 and consumes the remaining allocatable VG space except the reserve.
7. Swap is a 4 GiB LV inside the encrypted LVM stack.
8. Roughly 5% of VG space remains unallocated as a maintenance/recovery reserve.
9. Separate `/home`, `/var`, `/srv` or similar LVs are not required.
10. Hibernation is not required.
11. LUKS2 uses AES-XTS-plain64 with a 512-bit XTS key and Argon2id targeted at about 2 seconds with a 256 MiB PBKDF memory cap.
12. One normal owner-passphrase keyslot and one independent high-entropy recovery keyslot are required.
13. Cold boot/reboot requires owner-provided encrypted-storage unlock.
14. Automatic network, TPM-based and embedded-key unlock are deferred.
15. GRUB 2 UEFI is the first-ISO bootloader, with `PortusOS` bootloader identity, normal `linux-lts` top-level boot and explicit `linux` alternate.
16. The GRUB menu remains visible for roughly 5 seconds; saved-last-boot/hidden-menu behavior is not required.
17. A standard x86_64 EFI fallback loader path is included for recovery robustness.
18. mkinitcpio is the first-ISO initramfs tool using the non-systemd `encrypt -> lvm2` path; no `resume` hook is required.
19. Both Artix kernel lines require normal and broader recovery-capable initramfs artifacts.
20. PortusOS explicitly enables `default` + `fallback` mkinitcpio presets for both kernel lines because mkinitcpio v40+ new-install defaults no longer guarantee fallback-image generation.
21. `mkinitcpio -P`, followed when applicable by `grub-mkconfig -o /boot/grub/grub.cfg` and `grub-script-check /boot/grub/grub.cfg`, is the first-ISO native boot-artifact rebuild/repair baseline; routine kernel updates do not run `grub-install`.
22. Root ext4 uses conservative normal journaling/fsck/`relatime` behavior without first-ISO tuning flags.
23. SSH/normal Portus services are recovery/control paths after encrypted boot, not substitutes for pre-boot unlock.
24. The live ISO/equivalent recovery path remains independent and must be able to reach the encrypted installed system with authorized owner recovery material.
