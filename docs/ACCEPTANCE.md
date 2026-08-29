# PortusOS 0.1.0 Technical Preview Acceptance Contract

**Status:** Public acceptance authority
**Release:** PortusOS 0.1.0 Technical Preview
**Architecture:** x86_64
**Reference environment:** VMware virtual machine
**Executable validation authority:** `docs/VALIDATION.md`
**Machine-readable validation matrix:** `portusos-build/validation/matrix.yaml`

## Purpose

This document defines what the first public PortusOS Technical Preview must prove. It is a release boundary, not a development plan and not a statement that every gate has already passed.

A successful ISO build is necessary but insufficient. Acceptance requires the frozen release candidate to pass the complete validation/evidence contract in `docs/VALIDATION.md` for the exact ISO bytes that are published.

## Reference platform

The first Technical Preview is validated for:

- x86_64;
- UEFI boot;
- VMware virtual hardware;
- Artix Linux userspace;
- OpenRC service supervision;
- X11/i3 graphical session.

Broader bare-metal, Secure Boot, ARM, alternate hypervisor, and unsupported hardware claims are outside the first release unless separately demonstrated and recorded.

## Boot and installed storage

The accepted candidate must prove the documented boot/storage chain, including:

- GRUB UEFI boot;
- 512 MiB FAT32 ESP mounted at `/boot/efi`;
- 2 GiB unencrypted ext4 `/boot`;
- LUKS2 encrypted system storage;
- LVM volume group `portus`;
- ext4 root filesystem;
- 4 GiB encrypted swap;
- reserved VG headroom;
- owner unlock after reboot;
- independent recovery credential/keyslot;
- normal and fallback initramfs paths;
- installed `linux-lts` normal kernel plus `linux` alternate kernel;
- recovery through an independent live-ISO/equivalent path.

The live ISO itself uses `linux-lts` as its boot kernel. Both kernels remain installed in the target system.

## Installation

The candidate must prove the supported Calamares installation path using the Portus-owned storage integration described in `docs/ISO_BUILD_INSTALLER.md` and `docs/BOOT_STORAGE_RECOVERY.md`.

Installation must fail closed on unsafe or unsupported target conditions and must not expose owner/recovery credentials through command-line arguments, logs, generated public state, or build evidence.

## Master Portus and local session

The installed system must prove:

- a non-root Master Portus Linux user;
- independent root/administrator authority;
- PAM/elogind-backed local login;
- conditional X11 startup;
- `dbus-run-session` + i3 + Alacritty;
- Master workspace and tmux continuity;
- `portus-bootstrap` / `portus-master` behavior;
- Codex startup/authentication path;
- independent TTY/root/recovery access if the graphical or agent layer fails.

SSH remains independent from graphical startup and must not implicitly start the GUI session.

## Portus runtime

The exact candidate must validate the shipped Portus runtime and CLI surface, including:

- `portusd` local runtime;
- authenticated Unix-socket IPC using kernel peer credentials;
- `portus-os` status/doctor/index/task/capability/policy/artifact/health behavior;
- durable state and migration boundaries;
- task and execution lifecycle;
- provider/capability reconciliation;
- System Index freshness and principal filtering;
- artifact integrity and principal visibility;
- bounded events and audit behavior;
- health/degradation/recovery paths;
- fail-closed behavior for unsupported or unsafe operations.

## Policy and protected credentials

The candidate must prove the documented separation between Master Portus authority and administrator/root authority.

For protected reusable API credentials, acceptance requires the negative security boundary summarized as:

```text
authorized USE succeeds
reusable-secret EXPORT fails
```

The proof must include filesystem, API, process, administrative, redirect/TLS, cross-user, rotation/revocation, and degradation cases described by `docs/PROTECTED_API_VALIDATION.md`.

## Browser and Codex integration

The candidate must prove:

- the pinned Codex build is installed and reports the selected version;
- Codex works in the Master terminal/tmux workflow;
- supported browser authentication works through the documented Chromium-based browser path;
- PortusBrowser is packaged at the pinned source revision;
- browser native-messaging/extension/Broker integration works;
- connect/snapshot/action behavior works on the reference browser;
- PortusOS consumes PortusBrowser as a provider rather than duplicating browser-domain policy or state.

## Optional remote access

Portus MCP and OpenAI `tunnel-client` are bundled first-release components, but remote tunnel configuration is optional and user-owned.

Acceptance must prove that:

- local PortusOS remains usable without tunnel configuration;
- the ISO contains no pre-provisioned tunnel ID or reusable owner API key;
- the bundled Portus MCP local endpoint can be started as documented;
- the pinned tunnel client can be configured and exercised on the accepted environment when remote validation is performed.

## Validation matrix

The blocking executable acceptance set is ISO-01 through ISO-38 as defined by:

- `docs/VALIDATION.md` — procedure and evidence semantics;
- `portusos-build/validation/matrix.yaml` — machine-readable orchestration metadata.

All blocking rows must have objective candidate-scoped evidence and `status=pass`. `not_run`, missing, stale, substituted, or mismatched evidence is not acceptance.

## Explicit first-release exclusions

The first Technical Preview does not require:

- PortusWindow;
- broad bare-metal hardware support;
- Secure Boot support;
- ARM support;
- general background messaging control beyond the optional Portus MCP/tunnel path;
- general external artifact-delivery adapters;
- automatic backup automation;
- automatic whole-system rollback;
- full reboot-resumable task execution;
- sophisticated remote approval workflows;
- a display manager or compositor;
- VMware 3D acceleration;
- an LTS/SLA/update-cadence commitment.

See `KNOWN_LIMITATIONS.md` for the user-facing boundary.

## Acceptance rule

The first Technical Preview may be described as accepted only when:

1. the source tree used to produce the candidate is identified and immutable;
2. the canonical build produces exactly one expected ISO;
3. the ISO SHA-256 is recorded;
4. native build/cleanup evidence is successful;
5. the exact ISO passes ISO-01 through ISO-38 under `docs/VALIDATION.md`;
6. release metadata, provenance, third-party review, known limitations and security/publication gates in `docs/RELEASE.md` pass;
7. the published ISO bytes are identical to the accepted candidate bytes.

No source-equivalent rebuild may reuse validation evidence if the ISO hash changes.