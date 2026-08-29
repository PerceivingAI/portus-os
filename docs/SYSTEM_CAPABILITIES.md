# PortusOS System Capabilities and Installation Inventory

**Last reviewed:** 2026-08-29T01:24:50Z
**Last updated:** 2026-08-29T01:24:50Z

**Status:** Authoritative capability inventory; first-ISO top-level Artix package and base-service identities are locked, while candidate versions, optional profiles and installed/VM evidence remain verification work
**Adopted:** 2026-07-27
**Scope:** What PortusOS must install, configure, implement, integrate, and validate to deliver a real-world agent-first operating system

## 1. Authority and purpose

This document is the dedicated PortusOS inventory for:

- required installation capabilities;
- candidate packages and upstream components;
- optional application and hardware profiles;
- Portus-owned components that must be built;
- the purpose of each capability;
- how agents are expected to use it;
- when native commands are sufficient;
- when the OS needs a stable adapter, policy boundary, registry, or control-plane implementation;
- what must be researched before a package or component becomes locked.

This document does not serve as the resolved package manifest. Public package/source intent is materialized at `portusos-build/packages/packages.yaml`. Selected top-level official-Artix package names and OpenRC service identities are represented in the build contracts, while Codex, PortusBrowser, Portus MCP and tunnel-client identities are frozen in their component contracts. Candidate-specific package versions/repository snapshot, licences/redistribution, installed compatibility and the validated multi-artifact `packages.lock.yaml` remain generated release evidence.

When this document conflicts with older or informal package assumptions, this public capability inventory controls unless a more specialized current authority explicitly supersedes it.

Related authority:

- `docs/PROJECT.md` controls the product definition and operating model.
- `docs/CONTROL_PLANE.md` controls the non-duplication rule and the inclusion test for Portus-owned commands, services, and adapters.
- `docs/POLICY.md` controls root/admin authority, the non-root Master Portus Linux-user model, installer permission bundles, granular post-install policy, delegated-agent sandboxing, cross-user isolation, and root-equivalent classification.
- `docs/SECRETS.md` controls protected reusable credentials, secret-backed operations, AUTH, provider/keyring classification, non-export requirements, and negative-test obligations.

- `docs/STRUCTURE.md` controls ownership boundaries.
- `CODEX_UPDATES.md` controls Codex installation and integration.
- `OKF.md` controls durable knowledge.
- `docs/VALIDATION.md` controls executable first-ISO evidence and acceptance reporting.
- `docs/RELEASE.md` controls candidate/release identity, metadata, provenance and publication linkage.
- the future resolved package manifest controls actual installation only after it has been derived from approved decisions in this document and linked through `docs/RELEASE.md`.

## 2. Target out-of-box experience

PortusOS must install as an agent-first computer that can operate in the real world, not as a minimal Artix system that still requires the owner to assemble its basic capabilities.

The target machine should allow an authorised agent to:

- inspect and modify the non-secret operating system;
- diagnose hardware and software problems;
- repair software problems when policy permits;
- install, update, configure, launch, stop, and use applications;
- operate applications through CLIs, APIs, local IPC, file interfaces, browser control, or other structured mechanisms;
- run commands and scripts headlessly without opening a visual terminal;
- create, test, and execute new code when no installed capability solves a task;
- manage projects, files, processes, services, network state, storage, and artifacts;
- use the graphical display when visual output is useful;
- take screenshots and operate pointer/keyboard input when structured control is unavailable;
- continue approved work while the owner is disconnected;
- deliver results through local display, SSH, messaging, or other approved surfaces;
- recover from ordinary software failures without physical access where technically possible.

A visual terminal is a user-facing presentation or recovery tool. It is not the execution engine for the agent. The agent should execute work through the Portus control surface and receive stdout, stderr, exit state, events, and artifacts directly.

## 3. Broad Master Portus policy scenario

PortusOS must support a normal broad policy for a Master Portus Linux user without making that user root.

Installation presents administrator-facing permission bundles rather than every granular Linux control. A bundle may configure groups, ACLs, sudo/polkit, privileged helpers, service/socket permissions, devices, network rules and PortusOS policy. After installation, root/the administrator may refine these permissions individually.

Under a broad owner-approved policy, Master Portus may be allowed to:

- read/write ordinary files belonging to or shared with its Linux user;
- install/remove packages through selected privileged policy;
- modify allowed PortusOS/system configuration;
- manage selected OpenRC services;
- use approved sudo/polkit/helper operations;
- control applications available to its user;
- use the network according to policy;
- expose/close services where granted;
- control permitted displays, audio, input and devices;
- diagnose/repair software faults;
- reboot/shut down when granted;
- create/execute arbitrary user-level task code;
- send approved artifacts to configured destinations.

The Master Codex session is not confined to a workspace-only sandbox. Linux UID/GID ownership is the primary user boundary. Separate project/task agents may still be sandboxed.

Another user's private home/workspace is not included merely because the Master profile is broad.

Permissions that effectively permit arbitrary root takeover are **root-equivalent** and must be labelled as such rather than treated as ordinary broad non-root access.

Broad authority also does not automatically grant raw reusable credential retrieval. A credential stored in an ordinary Master-readable file is readable in practice and cannot be described as protected. Protected credentials must satisfy `docs/SECRETS.md`, including non-export behavior and negative tests. Master Portus may still use approved secret-backed capabilities without receiving the reusable value.

## 4. Classification

Every item in this inventory uses one of these classifications:

- **Required — first ISO:** Necessary on every first-release installation.
- **Required — target architecture:** Necessary to the product model, although the first implementation may be minimal.
- **Hardware-selected:** Installed when the detected or selected hardware profile requires it.
- **Optional application:** User-selected application or convenience tool.
- **Optional capability profile:** A larger selectable feature set such as Bluetooth, printing, containers, or human development.
- **Portus-owned:** Must be implemented or integrated by PortusOS rather than satisfied by installing a package alone.
- **Research required:** Required capability whose exact package, service, or design is not yet locked.

The owner remains free to install, remove, replace, or modify software after installation. This inventory defines the supported out-of-box baseline, not a restriction on the user's computer.

## 5. Control-surface rule

PortusOS must not create a dedicated top-level agent command for every package or anticipated workflow.

Use these layers:

### 5.1 Native programmable interfaces

Agents should directly use stable commands and APIs where they already provide good machine control.

Examples:

- `git`;
- `ffmpeg`;
- `jq`;
- `rg`;
- `find`;
- `xrandr`;
- `ip`;
- `ss`;
- `dig`;
- `rsync`;
- `tar`;
- `qpdf`;
- `imagemagick` commands;
- application-specific CLIs.

A Portus wrapper is unnecessary when Codex or the native interface already performs the operation cleanly and PortusOS would not add durable state, cross-application coordination, policy-mediated privilege, rollback, health, recovery, or artifact integration.

PortusOS must not build generic directory-listing, file-read, file-search, file-write, Git, or ordinary-shell wrappers. Codex and standard commands already cover those domains. `docs/CONTROL_PLANE.md` defines the required inclusion test.

### 5.2 Portus adapters

PortusOS should normalise or wrap domains where policy, rollback, ownership, or consistent results matter:

- OpenRC services;
- packages and updates;
- firewall and network exposure;
- storage, mounts, backups, and recovery;
- i3 workspaces and presentation routing;
- Codex sessions;
- task-owned process execution;
- application registration and health.

### 5.3 Portus-owned systems

PortusOS must implement shared product-level infrastructure not supplied by ordinary Linux packages:

- the deliberately non-duplicative Portus OS CLI (`portus-os`) control surface;
- the agent-managed system index;
- policy, approvals, privilege mediation, and secret-backed actions;
- project, session, task, artifact, and capability registries;
- durable managed execution and application lifecycle where persistence or coordination is required;
- event and audit state;
- unattended health and recovery;
- capability integration contracts;
- result presentation and delivery;
- remote interaction integration;
- Master Portus lifecycle.

Portus-owned infrastructure must not include generic file, source-search, Git, patch, or ordinary-shell replacements.

## 6. Decisions carried forward from the previous package evaluation

### 6.1 Optional package decisions

The supported first-ISO package-source boundary is governed by `docs/PACKAGE_POLICY.md`: official Artix repositories are the default third-party source, AUR/AUR helpers are excluded from supported profiles, and any selected software unavailable from Artix requires an explicit case-by-case sourcing decision.



The following are optional, not base requirements outside explicit first-ISO exceptions:

- VSCodium;
- Zed IDE;
- additional browsers beyond the first-ISO-required Chromium baseline, including Google Chrome;
- VLC;
- mpv;
- Nitrogen;
- CopyQ;
- Neovim;
- Obsidian;
- terminal file managers;
- additional language SDKs;
- container runtimes;
- Tailscale and other VPNs;
- Bluetooth, printing, and scanning stacks.

### 6.2 Browser decision

PortusBrowser is the required bundled agent-first browser-control capability.

PortusBrowser consists of:

- an agent-facing CLI;
- a local Broker;
- a browser native-messaging host;
- a Chromium extension;
- a versioned structured protocol;
- snapshot, action, wait, event, settings-profile, recipe, permission, and browser-policy systems.

It can operate multiple Chromium-based browsers, browser instances, profiles, windows, and tabs.

PortusBrowser remains browser-agnostic within its supported Chromium family, but the first ISO has an explicit exception: Chromium is mandatory as the default HTTP/HTTPS browser, normal Codex ChatGPT authentication surface and PortusBrowser reference target. Google Chrome and additional supported browsers remain optional future/profile choices; this first-ISO requirement does not make Chromium the permanent browser-control architecture.

PortusOS does not maintain a duplicate website whitelist or detailed browser-action policy when PortusBrowser already owns that concern. PortusOS controls only the outer boundary:

- whether PortusBrowser may run;
- task and requester association;
- process and service lifecycle;
- general filesystem perimeter;
- high-level network capability;
- artifact registration and delivery;
- health and recovery.

PortusBrowser owns:

- allowed websites and origins;
- browser profiles and instances;
- tab and window operations;
- browser navigation permissions;
- page actions;
- downloads within its domain;
- browser credential and session handling;
- browser-specific audit and policy.

### 6.3 Media decision

VLC and mpv remain optional conventional media players.

PortusWindow is the long-term agent-first media viewer, lightweight visualiser, and ephemeral presentation surface. It should progressively replace the need for conventional GUI viewers for ordinary agent presentation while remaining separate from media-processing tools such as FFmpeg.

### 6.4 IDE decision

VSCodium, Zed, and Neovim belong to optional human-development profiles.

The agent does not depend on an IDE to edit or create code. It works through files, patches, shell commands, language tools, and structured execution.

### 6.5 Visual control decision

Screenshots, visual understanding, pointer movement, clicks, keyboard input, and window control must be supported.

They are fallback and compatibility capabilities, not the default operating path.

The control preference remains:

1. application API or IPC;
2. structured CLI;
3. ordinary CLI and file operations;
4. window-manager or accessibility interface;
5. PortusBrowser for browser workflows;
6. screenshot, vision, pointer, and keyboard simulation.

## 7. Installation profile overview

The initial installer should ultimately resolve these profiles:

### Live ISO / installer profile

Required on the first-ISO live environment:

- Artix/OpenRC live userspace and networking;
- the real non-root Master Portus user/session path;
- X11, i3, Alacritty and tmux;
- Codex through the selected pinned standalone installation path;
- Chromium as the mandatory first-ISO default HTTP/HTTPS browser and normal Codex ChatGPT-subscription authentication surface, with working `xdg-open`/localhost callback integration;
- Node.js 20.9+ and npm for the canonical bundled Portus MCP runtime;
- bundled Portus MCP at `/opt/portus/portus-mcp`, with local endpoint `http://127.0.0.1:8789/mcp`;
- bundled OpenAI `tunnel-client` at `/usr/local/bin/tunnel-client`, including its Codex tunnel plugin payload;
- a discoverable but optional `portus-local` tunnel setup path after Codex login; no tunnel ID/API key is embedded in the image;
- `portus-os`, `portusd`, `portus-privd`, `portus-apid`, `portus-api`, `portus-auth`, `portus-bootstrap`, and `portus-master`;
- the Master workspace/charter and enough Portus configuration for Codex to inspect the live machine and exercise available runtime/index/diagnostic surfaces;
- Calamares as the supported installer framework;
- PortusOS-owned Calamares configuration/module ordering;
- any narrowly scoped PortusOS Calamares module proven necessary by the final install/storage contract;
- live recovery tools required by `docs/BOOT_STORAGE_RECOVERY.md`.

The live system is intentionally interactive and useful for investigation before installation. It remains ephemeral and must not contain pre-provisioned reusable owner/API secrets. Calamares is an installation surface inside that environment, not a substitute for the agent control plane.

### Core agent OS

Always installed:

- Artix/OpenRC base;
- kernel, initramfs, firmware baseline;
- boot, encryption, storage, and repair tools;
- networking, SSH, firewall, time, and logging;
- security, policy, keyring, and sandbox foundations;
- Codex runtime;
- Chromium as the first-ISO default browser for Codex ChatGPT login and PortusBrowser validation;
- Node.js 20.9+ and npm for Portus MCP;
- bundled Portus MCP and OpenAI `tunnel-client`; tunnel setup remains optional/user-session-owned;
- Portus control plane;
- core development and scripting tools;
- hardware and software diagnostics;
- minimal X11/i3 presentation and recovery stack;
- screenshot and input-control fallback;
- audio base and media-processing CLI;
- image, PDF, archive, and file-processing CLI tools;
- backup and recovery foundations.

### VMware profile

Selected for the first supported target:

- in-kernel/native VMware storage, networking and input support first;
- ordinary Xorg/modesetting for virtual display first, with no 3D requirement;
- `open-vm-tools` only if current official-Artix availability plus measured first-ISO need justify it;
- optional, policy-controlled clipboard or shared-folder integration only if a selected supported mechanism is explicitly approved.

### Hardware-selected profile

Resolved from detected or selected hardware:

- CPU microcode;
- GPU stack;
- wireless firmware;
- storage-specific tools;
- laptop battery and ACPI support;
- vendor-specific drivers where supported.

### Browser profile

The first ISO has one locked exception to the general optional-browser model: **Chromium is mandatory** because it is the normal Codex ChatGPT-subscription authentication browser, the default HTTP/HTTPS browser for the Master graphical session, and the PortusBrowser reference browser.

Additional/future browser selections remain optional:

- Google Chrome;
- future supported Chromium-based browsers.

PortusBrowser remains a separate capability component and is architecturally independent of any one browser package, even though first-ISO acceptance deliberately freezes Chromium as its reference integration target.

### Human-development profile

Optional:

- VSCodium;
- Zed;
- Neovim;
- GUI Git tools, if later approved;
- language SDKs and language servers.

### Human-desktop convenience profile

Optional:

- Thunar and extensions;
- Nitrogen;
- CopyQ;
- graphical network editor;
- graphical screenshot utility;
- conventional viewers and media players;
- additional themes and fonts.

### Hardware-expansion profiles

Optional:

- Bluetooth;
- printing;
- scanning;
- camera utilities;
- specialist drivers.

### Containers and virtual environments

Optional:

- Podman;
- Docker compatibility;
- other isolation tooling.

### Remote connectivity profile

Required payload / optional activation:

- Portus MCP;
- OpenAI Secure MCP Tunnel via bundled `tunnel-client`;
- the `portus-local` tunnel profile targeting `http://127.0.0.1:8789/mcp`, created only when the owner supplies tunnel credentials.

Optional later additions:

- Tailscale;
- another owner-selected VPN;
- messaging and delivery adapters.

Portus MCP and `tunnel-client` are session-owned connectivity components, not OpenRC machine services. SSH remains independently required for recovery.

## 8. Base Artix/OpenRC operating system

**Classification:** Required — first ISO; research exact package set

Locked service/session engineering direction is defined by `docs/OPENRC_SERVICES.md` and `docs/GUI_SYSTEM.md`: preserve `sysinit -> boot -> default`; verify D-Bus/elogind early prerequisites; prefer NetworkManager, nftables, chrony and syslog-ng from official Artix; and use PAM/elogind + conditional `startx` + `dbus-run-session` for the local graphical session rather than requiring OpenRC user services.

Candidate package domains:

- Artix base;
- OpenRC;
- OpenRC system integration;
- elogind and its OpenRC integration;
- D-Bus and its OpenRC service;
- device management;
- PAM;
- shell and core Unix utilities;
- Artix keyring and mirror configuration;
- pacman;
- emergency text editor.

Purpose:

- provide a complete locally controlled OS;
- boot and supervise system services without systemd;
- create login and device sessions;
- retain standard Linux recovery paths.

Agent use:

- direct shell and process inspection;
- file, user, permission, environment, and session operations;
- general system diagnosis.

Portus-owned work:

- OpenRC adapter;
- runlevel and dependency inspection;
- stable service results;
- health checks;
- restart and crash-loop policy;
- audit integration.

## 9. Kernel, initramfs, firmware, and microcode

**Classification:** Required — first ISO plus hardware-selected components

Selected/baseline components:

- official Artix `linux-lts` normal/default kernel;
- official Artix `linux` alternate/recovery kernel;
- mkinitcpio using the non-systemd boot path;
- Linux firmware baseline from official Artix;
- kernel module utilities;
- Intel/AMD microcode selected by the verified hardware policy;
- DKMS only when an approved required driver demonstrably needs it.

Purpose:

- stable kernel operation;
- encrypted-root boot;
- hardware support;
- security and reliability updates.

Agent use:

- inspect kernel, modules, firmware messages, and boot artifacts;
- diagnose driver failures;
- prepare approved updates.

Portus-owned work:

- preserve a bootable fallback;
- prevent removal of the last working kernel;
- rebuild and verify initramfs;
- record reboot requirements;
- expose recovery procedures.

Locked kernel direction:

- official Artix `linux-lts` is the normal/default first-ISO kernel;
- official Artix `linux` remains installed as an independently bootable alternate/recovery kernel;
- PortusOS does not maintain a first-ISO custom kernel fork;
- updates must preserve both kernel paths; automatic rollback is not required.

Linux verification:

- exact release-candidate kernel and firmware/microcode package versions;
- observed VMware native driver behavior under both kernels;
- whether `open-vm-tools` is required/available from official Artix;
- any justified DKMS case;
- package-retention/update and repair flow.

## 10. Boot, encryption, storage, and filesystems

**Classification:** Required — first ISO; engineering baseline locked, Linux verification required

Selected/baseline components:

- GRUB 2 in UEFI mode;
- 512 MiB FAT32 ESP mounted at `/boot/efi`;
- 2 GiB unencrypted ext4 `/boot`;
- cryptsetup LUKS2 AES-XTS/Argon2id under the locked ~2-second/256-MiB policy;
- LVM2 VG `portus` with ext4 root, 4 GiB encrypted swap and ~5% free reserve;
- mkinitcpio with `encrypt` before `lvm2` and no first-ISO resume hook;
- partition/FAT/ext4/mount/block-device repair and inspection tools.

Purpose:

- install and boot PortusOS;
- encrypt owner data;
- manage disk layout;
- repair filesystems and boot state.

Agent use:

- routine read-only storage inspection;
- capacity and health diagnosis;
- approved mount, backup, and repair operations.

Portus-owned work:

- safe installer disk workflow;
- expected-device and destructive-operation checks;
- storage adapter;
- backup and recovery integration;
- live-ISO repair flow;
- task and approval association for destructive actions.

Linux verification:

- exact Artix GRUB/mkinitcpio/cryptsetup commands and configuration paths;
- exact Calamares realization of the locked partition/LUKS/LVM/keyslot baseline;
- effective Argon2id values on the reference VM;
- live-ISO/GRUB/mkinitcpio repair procedures;
- exact update/repair integration follows the locked `docs/UPDATES_RECOVERY.md` contract; automatic rollback remains deferred.

BIOS is outside first-ISO scope; snapshots and automatic rollback are not part of this storage baseline.

## 11. Hardware inventory and diagnostics

**Classification:** Required — first ISO

Candidate tools:

- PCI and USB inspection;
- DMI/system inventory;
- storage health and NVMe tools;
- temperature and sensor tools;
- ACPI and battery tools;
- network-interface diagnostics;
- kernel and firmware log inspection.

Purpose:

- let agents understand the actual machine;
- select drivers and profiles;
- diagnose hardware and firmware problems;
- distinguish software faults from likely hardware faults.

Agent use:

- direct native CLI use;
- structured summaries through `portus-os doctor`.

Portus-owned work:

- bounded health results;
- severity and remediation classification;
- no false claim that hardware faults can be repaired in software;
- safe reporting to remote users.

## 12. Graphics and display drivers

**Classification:** Required generic stack; VMware-first engineering baseline locked, exact packages verified on Artix

Selected direction:

- Mesa/DRM userspace and the normal Xorg stack as required by the selected Artix packages;
- libinput;
- ordinary X11 modesetting/display tooling first;
- in-tree/native VMware kernel graphics/device support first;
- no compositor or VMware 3D acceleration required for first-ISO acceptance;
- dedicated VMware X driver only if an official Artix package plus measured reference-VM behavior justifies it;
- hardware-specific Intel/AMD/NVIDIA components remain future hardware-selected profile work rather than first-ISO VMware requirements.

Purpose:

- local X11/i3 presentation;
- browser rendering;
- screenshots and visual fallback;
- deterministic display/window observation and routing.

Agent use:

- inspect renderer/display topology;
- use display tools and i3 IPC directly;
- launch/control visual applications through structured/application paths before simulated input.

Portus-owned work:

- display inventory/correlation;
- workspace/window routing;
- bounded display health;
- sensitive-screen handling;
- no duplicate graphics configuration framework.

Broad multi-monitor/high-DPI/GPU qualification is outside first-ISO scope.

## 13. VMware integration

**Classification:** Required functional VMware profile; native support first, guest-tools necessity verified rather than assumed

Locked direction:

- verify official Artix kernels' in-tree VMware storage/network/input/device support first;
- verify ordinary Xorg/modesetting first;
- no VMware 3D dependency;
- `open-vm-tools` is not assumed required;
- if a blocking first-ISO requirement actually needs `open-vm-tools`, verify official Artix availability first and otherwise use the case-review process in `docs/PACKAGE_POLICY.md`;
- AUR/Arch is not a fallback source;
- optional clipboard/shared-folder/drag-and-drop convenience features do not justify weakening package/security policy.

Required VMware capabilities:

- stable networking;
- usable storage;
- usable display and input;
- clean shutdown/reboot through the selected supported mechanisms;
- coherent time handling with one Linux time-sync authority;
- recovery after host/guest restart.

Desired but non-blocking unless separately promoted:

- dynamic display resize.

Portus-owned work:

- health/diagnostic correlation;
- explicit policy for cross-boundary convenience features;
- no automatic unrestricted host mounts.

Linux verification must record the actual VMware drivers/devices observed under both Artix kernel paths and the final `open-vm-tools` necessity/source decision.
**Classification:** Required — first ISO

Candidate components:

- NetworkManager and OpenRC service integration;
- selected Wi-Fi backend;
- modern IP routing tools;
- OpenSSH and OpenRC service integration;
- CA certificates;
- HTTP clients;
- DNS, route, socket, packet, and connectivity diagnostics;
- secure file transfer through SSH.

Purpose:

- local and remote operation;
- downloads and APIs;
- communication adapters;
- remote recovery;
- agent-created network services.

Agent use:

- native commands for inspection and diagnostics;
- Portus adapter for mutations that could break remote access.

Portus-owned work:

- network context;
- safe connection changes;
- SSH health;
- deliberate service exposure;
- connectivity rollback;
- local/private/public boundary awareness;
- task and policy association.

A remote network change must not silently strand an unattended machine when a safe rollback is possible.

## 15. Firewall

**Classification:** Required — first ISO; frontend decision required

Preferred direction:

- nftables as the underlying firewall;
- Portus-owned policy and rule management.

Alternative:

- UFW as a temporary or human-friendly frontend, only if it is the single declared owner of persistent rules.

Do not install multiple competing firewall managers without an explicit ownership model.

Purpose:

- default-deny incoming traffic;
- protect SSH and communication adapters;
- constrain agent-created servers;
- control LAN, VPN, and public exposure.

Portus-owned work:

- declarative policy;
- validation before application;
- atomic updates;
- remote rollback;
- expiring exposure leases;
- audit and task association;
- desired-versus-actual health checks.

## 16. Time, scheduling, logging, and maintenance

**Classification:** Required — first ISO

Candidate capability domains:

- time synchronisation and OpenRC service;
- timezone data;
- cron for simple maintenance if needed;
- syslog implementation and OpenRC service;
- log rotation;
- optional Linux audit subsystem.

Purpose:

- correct authentication and certificate behaviour;
- trustworthy timestamps;
- persistent diagnostics without journald;
- maintenance scheduling;
- bounded storage use.

Rules:

- cron is not the Portus durable-task system;
- logs must be bounded and rotated;
- secrets must be redacted or excluded;
- task events and security audit are distinct from ordinary service logs.

## 17. Security, authentication, and secrets

**Classification:** Required — first ISO and target architecture

Candidate components:

- sudo;
- polkit;
- one maintained lightweight authentication agent;
- PAM;
- GNOME Keyring or another selected Secret Service provider;
- libsecret;
- GnuPG and pinentry;
- OpenSSH;
- Bubblewrap;
- optional hardware-token support;
- optional AppArmor, Audit, or fail2ban after research.

Purpose:

- authenticate the administrator and Master Portus users;
- establish separate Linux user identities and cross-user ownership boundaries;
- grant broad but non-root Master authority through installation permission bundles;
- support granular administrator changes after installation;
- secure remote access;
- sandbox delegated project/task/inspection agents where appropriate;
- store/use credentials according to the protected classification required by `docs/SECRETS.md`.

Portus-owned work:

- root-owned `portus-privd` typed privileged-operation service with peer-authenticated `/run/portus/priv/use.sock` and root-only admin socket;
- `portus-priv-users` transport group distinct from semantic operation authorization;
- effective-policy inspection;
- installer permission-bundle application;
- granular administrator policy changes;
- allow/prompt/reject decisions and bounded grants;
- root-equivalent capability classification/warnings;
- cross-user principal enforcement;
- protected credential provisioning/classification as either protected from Master Portus or deliberately Master-readable;
- secret-backed capability invocation without raw-secret retrieval;
- the first protected API provider defined by `docs/PROTECTED_API_PROVIDER.md`, including the separate `portus-apid` service identity/store and direct `portus-api` agent/client surface;
- protected provider/API operations that attach credentials internally through named root-controlled provider bindings rather than caller-selected destinations;
- optional local provider-compatible proxy adapters authenticated with short-lived scoped local capability tokens, without exposing the real upstream credential;
- verified TLS with no Master-facing insecure bypass and first-contract rejection of credential-bearing redirects;
- prevention of indirect credential exfiltration through arbitrary destination/header/redirect/command injection paths;
- root-run `portus-auth` TTY provisioning surface for protected human secret entry without secret argv/output;
- AUTH workflows;
- privileged-operation boundaries;
- policy-change auditing;
- emergency recovery.

A same-user keyring or encrypted file does not automatically make a credential unreadable to Master Portus. Any credential claimed to be protected from the broad Master UID must pass the non-export tests in `docs/SECRETS.md`; class-specific first-ISO labeling follows `docs/CREDENTIAL_CLASSES.md`, and the reference API provider must pass `docs/PROTECTED_API_VALIDATION.md`.

## 18. Core agent and development tools

**Classification:** Required — first ISO

Candidate capability domains:

- Git;
- GitHub CLI;
- tmux;
- base compilation tools;
- pkg-config support;
- Python;
- Node.js and npm;
- HTTP clients;
- JSON and YAML tools;
- ripgrep and file search;
- patch and diff tools;
- shell validation and formatting;
- process and resource tools;
- archive utilities;
- local documentation and man pages.

Purpose:

- let the agent solve novel tasks through code;
- build and test projects;
- inspect and transform data;
- diagnose software;
- interact with repositories and services.

Evaluation:

- Node.js is not required merely for Codex, but is likely justified by Portus applications and common agent workflows.
- Python is a general automation baseline.
- Rust, Go, Bun, Java, .NET, and specialist SDKs remain optional unless a required Portus component depends on them.
- global language-package installation should be policy-controlled.

Agents use these tools directly. PortusOS should not wrap normal compiler, interpreter, Git, or test commands merely to rename them.

## 19. Codex runtime

**Classification:** Required — first ISO

Installation:

- pinned standalone Codex release as defined in `CODEX_UPDATES.md`;
- Bubblewrap;
- Git;
- keyring integration;
- authentication surface;
- system and project policy configuration.

Portus-owned work:

- system config and requirements;
- permission profiles;
- rules and hooks;
- Master and project launchers;
- session integration;
- diagnostics;
- controlled updates.

## 20. PortusOS control plane

**Classification:** Portus-owned — required target architecture; core subset required in first ISO

Required Portus-specific components:

1. Portus OS CLI (`portus-os`) with a deliberately non-duplicative domain boundary.
2. Agent-managed system index and machine map.
3. Capability and application registration, lifecycle, and health.
4. Durable task registry and managed execution for persistent or coordinated work.
5. Project and Codex-session integration metadata.
6. Artifact registry, presentation, delivery, and cleanup state.
7. Policy inspection, approvals, privilege mediation, and secret-backed actions.
8. Events and audit.
9. Health, degradation, diagnostics, and recovery.
10. OpenRC integration for managed service workflows.
11. i3/display and visual-input integration.
12. Network/firewall managed workflows.
13. Package/update managed workflows.
14. Storage/backup/recovery managed workflows.
15. Portus application integration contracts.

Not included as PortusOS replacements:

- generic file reading, listing, searching, writing, or patching;
- project navigation;
- Git operations;
- ordinary shell command execution;
- stable application CLIs already usable by the agent.

The system index is queried and refreshed by the agent when useful. It must not be injected wholesale into model context, become a filesystem index, or be treated as the limit of available computer capabilities. `docs/SYSTEM_INDEX.md` defines its architecture, and `docs/CONTROL_PLANE.md` defines the overall inclusion rule.

Portus-managed execution, when durable or coordinated execution is required, must:

- run without opening a terminal emulator;
- set an explicit identity and working directory;
- apply filesystem, network, sandbox, and elevation policy;
- support timeouts and resource limits;
- capture stdout, stderr, exit code, and relevant events;
- associate the process with a task;
- optionally detach or supervise long-running work;
- return machine-readable results.

A visible terminal may be launched only when the user requests it, visual interaction is genuinely required, or recovery calls for it.

## 21. Persistent tasks and process supervision

**Classification:** Portus-owned — required target architecture

Supporting native capabilities may include:

- OpenRC supervision for stable services;
- tmux for optional live attachment;
- process-control utilities;
- file locking;
- timeouts;
- resource priority tools;
- a local durable store such as SQLite if selected.

Rules:

- OpenRC owns long-running services;
- Portus task state owns work lifecycle;
- Codex owns session/thread state;
- tmux owns interactive terminal continuity;
- no one layer substitutes for all others.

A task must be able to survive loss of the requesting SSH or messaging connection.

## 22. Minimal graphical and presentation foundation

**Classification:** Required — first ISO

Candidate components:

- X11 server and startup;
- X authentication and display utilities;
- libinput;
- i3;
- Alacritty as emergency/local terminal;
- notification daemon;
- scriptable wallpaper setter;
- optional launcher;
- optional compositor.

Purpose:

- host PortusWindow and visual applications;
- show artifacts and status;
- support authentication;
- provide emergency local administration;
- support visual fallback.

Decisions:

- Feh or an equivalent scriptable wallpaper setter is required.
- Nitrogen is optional.
- Alacritty is installed for local use and recovery, but the agent does not need to open it to run commands.
- graphical polish remains secondary to deterministic operation.

Portus-owned work:

- session startup;
- D-Bus and environment setup;
- deterministic workspaces;
- presentation routing;
- sensitive-content cleanup;
- lock and recovery behaviour.

## 23. Screenshot, pointer, keyboard, and visual fallback

**Classification:** Required capability — first ISO

Candidate X11 tools:

- a scriptable screenshot tool such as `maim` or `scrot`;
- region/window selection support if required;
- `xdotool` or another selected X11 input-control interface;
- `xprop`;
- `xwininfo`;
- `xrandr`;
- i3 IPC;
- optional `wmctrl` for compatibility.

Purpose:

- operate applications that do not expose usable structured controls;
- inspect visual failures;
- capture results;
- support user-requested visible demonstrations;
- provide compatibility with conventional applications.

Rules:

- browser navigation should use PortusBrowser, not generic mouse automation, when supported;
- screenshots are sensitive data;
- capture and input operations must be task-associated and auditable when automated;
- visual control is a fallback and must not become the primary system-management path;
- the agent must be able to launch and use GUI applications without launching a terminal window.

Portus-owned work:

- visual-control policy;
- screenshot storage and retention;
- active-display and window context;
- sensitive-screen restrictions;
- safe input targeting;
- result and error normalisation.

## 24. PortusBrowser and optional browsers

**Classification:** PortusBrowser required and bundled in the first supported PortusOS release; individual browser packages selectable

PortusBrowser components:

- CLI;
- broker;
- browser add-on;
- browser-domain policy;
- structured browser state and operations.

Optional browser applications:

- Chromium;
- Google Chrome;
- other supported Chromium-based browsers.

Agent use:

- operate multiple browsers and tabs concurrently;
- navigate and inspect pages;
- manage browser instances and profiles;
- download and register artifacts;
- use authenticated browser sessions without reading underlying credentials;
- run browser workflows headlessly or with visible rendering according to the browser and task.

PortusOS responsibilities:

- bundle and install PortusBrowser;
- discover and report its capabilities;
- install and verify the P15 provider-owned/native-host-on-demand Broker lifecycle without introducing a competing supervisor;
- apply the OS-level process and filesystem perimeter;
- associate work with tasks;
- register outputs;
- report health and recovery.

PortusBrowser responsibilities:

- website and origin restrictions;
- tab, page, and browser actions;
- browser profile isolation;
- browser-specific permissions;
- browser session and credential behaviour;
- browser-level audit.

Do not duplicate PortusBrowser's policy model inside PortusOS.

## 25. PortusWindow and optional viewers

**Classification:** PortusWindow integration required in target architecture; conventional viewers optional

PortusWindow target capabilities:

- lightweight image display;
- lightweight web display/navigation;
- media playback and visualisation;
- PDF/document presentation as implemented;
- task status and result presentation;
- ephemeral and sensitive-output lifecycle.

Optional conventional applications:

- mpv;
- VLC;
- lightweight image viewer;
- lightweight PDF viewer.

Agent use:

- prefer PortusWindow for agent-first presentation when supported;
- use conventional viewers only when the user chooses them or PortusWindow lacks the required capability.

PortusOS responsibilities:

- display routing;
- task association;
- lifecycle;
- policy and sensitive-output timeout;
- health and recovery.

## 26. Wallpaper and appearance

**Classification:** Minimal requirement plus optional convenience

Required:

- one scriptable wallpaper setter;
- a default PortusOS wallpaper;
- base font and icon configuration.

Optional:

- Nitrogen;
- additional themes and wallpapers;
- broader human customisation tools.

No Portus-specific wallpaper daemon is needed. The session configuration stores and restores the chosen path.

## 27. Fonts, icons, MIME, and XDG integration

**Classification:** Required — first ISO

Candidate capability domains:

- broad Unicode font coverage;
- emoji;
- one code/terminal font;
- compatibility fonts;
- fontconfig;
- basic icon theme;
- XDG user directories;
- XDG utilities;
- shared MIME database;
- desktop entry database;
- desktop portal and GTK portal;
- D-Bus.

Purpose:

- correct rendering in PortusWindow, browsers, terminals, and optional applications;
- default application and MIME behaviour;
- file picker and portal integration;
- predictable user paths.

The portal stack must be validated under OpenRC/elogind without systemd user services.

## 28. Polkit and keyring session

**Classification:** Required — first ISO

Candidate components:

- polkit;
- one maintained lightweight polkit agent;
- GNOME Keyring or selected Secret Service provider;
- libsecret;
- PAM integration.

Required behaviour:

- exactly one polkit agent;
- reliable local authentication prompts when policy requires them;
- support for pre-authorized unattended privileged operations where administrator policy grants them;
- keyring/Secret Service initialization where selected;
- login-keyring unlock where appropriate;
- shell recovery when the graphical agent fails;
- explicit classification of whether each same-user keyring credential is merely protected at rest or actually non-exportable to Master Portus;
- no claim of raw-secret isolation without negative retrieval tests.

## 29. Clipboard

**Classification:** Basic capability required; history optional

Required candidate:

- one scriptable X11 clipboard utility, likely `xclip` or equivalent.

Optional:

- CopyQ, disabled by default.

Rules:

- clipboard is not durable storage;
- clipboard is not the Portus application IPC layer;
- secret history is not retained by default;
- structured files and application interfaces are preferred;
- external transfer uses artifact-delivery policy.

## 30. File management and removable media

**Classification:** CLI support required; GUI optional

Required command capabilities:

- file inspection;
- copy, move, delete, link, permission, ownership, and metadata operations;
- mount and block-device inspection;
- archive handling;
- safe temporary storage.

Optional human applications:

- Thunar;
- archive plugin;
- GUI archive manager;
- GVFS;
- thumbnailing.

Automount must not be enabled merely for convenience. Removable-media discovery and mounting should be policy-controlled through the storage layer.

## 31. Archive, compression, and transfer

**Classification:** Required — first ISO

Candidate capabilities:

- tar;
- gzip;
- bzip2;
- xz;
- zstd;
- 7-Zip;
- zip/unzip;
- rsync;
- optional RAR support;
- optional rclone.

Agent use:

- direct CLI use;
- artifact registration and checksums for produced archives.

## 32. Image processing

**Classification:** Required agent capability — first ISO

Candidate tools:

- ImageMagick;
- ExifTool;
- optional libvips CLI;
- optional optimisation tools.

Purpose:

- inspect, convert, resize, crop, compose, and validate images;
- strip metadata under policy;
- prepare previews and delivery copies.

Presentation should move toward PortusWindow; processing remains command-line driven.

## 33. PDF and document processing

**Classification:** Required agent capability — first ISO

Candidate tools:

- Poppler command-line utilities;
- qpdf;
- MuPDF tools where complementary;
- Ghostscript where required;
- optional Pandoc;
- optional OCR profile.

Purpose:

- inspect text and metadata;
- render pages;
- merge, split, repair, encrypt, decrypt, and validate authorised documents;
- produce reports and previews.

Command-line document processing is more important to the base OS than a conventional PDF viewer.

## 34. Audio

**Classification:** Required base capability — first ISO

Candidate components:

- ALSA libraries, utilities, and use-case configuration;
- PipeWire audio;
- PipeWire Pulse compatibility;
- WirePlumber;
- OpenRC user-service integration;
- optional graphical volume control;
- FFmpeg.

Purpose:

- browser audio;
- media playback and processing;
- microphone and speaker access;
- future voice communication surfaces;
- audio files and generated results.

Agent use:

- FFmpeg;
- PipeWire and WirePlumber command interfaces;
- ALSA tools;
- application APIs.

Portus-owned work:

- audio device inventory;
- default input/output;
- volume and mute state;
- health and restart;
- capture permission;
- active-microphone privacy indication;
- task association for automated recording.

The OS must not silently record audio.

## 35. Video and media processing

**Classification:** FFmpeg required; players optional

Required:

- FFmpeg.

Optional:

- mpv;
- VLC;
- yt-dlp after separate source/update review;
- GStreamer tools when a Portus component requires them.

Purpose:

- transcode;
- inspect metadata;
- create thumbnails;
- extract audio;
- validate downloaded media.

PortusWindow is the long-term agent-first playback and visualisation surface.

## 36. Power, screen locking, battery, and unattended operation

**Classification:** Required policy and baseline tools; some hardware-selected

Candidate components:

- screen locker;
- idle-lock integration;
- ACPI daemon and OpenRC service;
- battery and power inspection;
- optional XFCE power manager as human UI.

Required default policy:

- no automatic suspend on AC merely because of generic desktop defaults;
- display blanking and screen locking remain separate from machine suspend;
- pending tasks are checked before agent-initiated shutdown or reboot;
- battery warnings are visible to maintenance logic;
- low-battery behaviour is configurable;
- safe remote shutdown and reboot are available.

The authoritative unattended power policy is Portus-owned even if a graphical tool is installed.

## 37. Optional IDEs and human-development applications

**Classification:** Optional application profile

Options:

- VSCodium;
- Zed;
- Neovim;
- additional IDEs installed by the owner.

Evaluation:

- VSCodium offers familiar VS Code compatibility.
- Zed should be evaluated for current Linux and Artix compatibility, package source, account requirements, telemetry, update method, and remote/agent usefulness before its install path is locked.
- Neovim is lightweight and terminal-native.

No IDE is required for agent code work.

## 38. Containers and isolated application environments

**Classification:** Optional capability profile

Options:

- rootless Podman as the preferred research candidate;
- Docker as a compatibility option;
- no container runtime in the base installation.

Evaluation:

- daemonless rootless operation better matches least privilege;
- Docker has broader compatibility but introduces daemon and group-authority concerns;
- a container runtime should not be installed unless it serves a required Portus or user workflow.

## 39. Bluetooth, printing, scanning, camera, and specialist hardware

**Classification:** Optional hardware profiles

Potential profiles:

- Bluetooth stack, tools, UI, and audio integration;
- CUPS and printing tools;
- scanning stack;
- camera utilities;
- vendor-specific drivers.

Privacy requirements:

- no automatic discoverability without policy;
- no silent microphone or camera capture;
- incoming transfers and device pairing require explicit rules;
- unnecessary daemons are not installed on machines that do not need them.

## 40. Backup, update, rollback, and recovery

**Classification:** First-ISO update/recovery architecture required; general backup and automatic rollback deferred

The first-ISO authority is `docs/UPDATES_RECOVERY.md`.

Locked first-ISO behavior:

- updates are explicit authorized operations; no background/startup automatic OS upgrade path;
- Artix/pacman remains authoritative for Artix package state;
- PortusOS adds managed preflight, expected-change recording, compatibility/migration gates, bounded recovery-state protection and post-update health proof where coordination materially improves safety;
- destructive/non-trivial Portus SQLite migrations create the bounded consistent pre-migration backup required by `docs/STATE.md`;
- kernel-affecting updates preserve both Artix kernel roles and validate regenerated mkinitcpio/GRUB artifacts;
- Codex remains explicitly pinned/validated and PortusBrowser is updated as a compatibility set;
- interrupted/failed updates remain diagnosable and enter explicit degraded/recovery state rather than silent success or automatic whole-system rollback;
- reboot requirement is reported but reboot is never automatic;
- TTY/SSH/OpenRC/root/alternate-kernel/live-ISO recovery remains independent.

Deferred capabilities may later evaluate tools such as Restic, Borg, rsync, snapshot mechanisms, or approved remote-backup adapters, but no such general backup/snapshot stack is required merely to satisfy the first-ISO update contract.

## 41. Health and monitoring

**Classification:** Required — first ISO and target architecture

Candidate native tools:

- process and resource inspection;
- disk and filesystem usage;
- storage health;
- sensor and battery state;
- network diagnostics;
- OpenRC service state;
- package state.

Portus-owned work:

`portus-os doctor` should return:

- concise human output;
- structured machine output;
- severity;
- affected capability;
- evidence;
- safe automatic repair classification;
- recommended manual action;
- repeated-failure state;
- task, delivery, backup, Codex, service, disk, memory, network, and capability health.

## 42. Artifact storage and delivery

**Classification:** Portus-owned — required target architecture

Required capabilities:

- task-associated artifact locations;
- stable artifact identifiers;
- type, size, and checksum;
- confidentiality classification;
- retention;
- delivery destinations;
- delivery state;
- retry and failure handling;
- external-transfer audit.

Supporting tools:

- MIME detection;
- hashing;
- SSH/SCP/SFTP;
- archive and media tools;
- optional approved cloud transfer tools.

External delivery remains a policy boundary even under broad general computer access.

## 43. Communication adapters

**Classification:** SSH required; additional adapters target architecture

First-release surfaces:

- local terminal;
- OpenSSH.

Future surfaces:

- messaging applications;
- web or mobile control;
- notifications;
- secure download and artifact delivery.

Portus-owned contract:

- requester identity;
- authentication and authorisation;
- shared task state;
- approval presentation;
- attachment validation;
- duplicate and replay handling;
- audit;
- outage behaviour.

A messaging account must not map directly to unrestricted shell access.

## 44. Capability integration

**Classification:** Portus-owned — required target architecture

Required contract fields:

- identity and version;
- capability domains;
- interface discovery;
- policy owner;
- required permissions;
- configuration and state ownership;
- process or service lifecycle;
- health;
- outputs and artifacts;
- compatibility;
- upgrade, recovery, and uninstall behaviour.

Initial capability integrations:

- native Linux execution;
- OpenRC;
- Codex;
- tmux;
- i3/display;
- visual input and screenshots;
- NetworkManager and firewall;
- package manager;
- storage and backups;
- PortusBrowser;
- PortusWindow.

## 45. Required Portus-owned deliverables

The PortusOS implementation plan must ultimately include:

1. ISO build and clean-build entry point.
2. Package source contract and resolved lock.
3. Safe installer.
4. Modular post-install system.
5. Portus OS CLI (`portus-os`) with the `docs/CONTROL_PLANE.md` non-duplication boundary.
6. Agent-managed system index, machine relationships, provider links, refresh, rescan, and reconciliation.
7. Capability registry plus application integration, lifecycle, and health.
8. Durable task registry and managed execution for persistent or coordinated work.
9. Project registry.
10. Codex session registry integration.
11. Policy, approval, privilege-mediation, and secret-backed-action system.
12. Hard secrets-domain boundary and secret-capability integration.
13. Master Portus launcher and recovery path.
14. Artifact registry, presentation, delivery, retention, and cleanup state.
15. Events and audit.
16. OpenRC adapter and managed service workflows.
17. i3/display adapter.
18. Screenshot and pointer/keyboard fallback adapter.
19. Network/firewall managed workflows.
20. Package/update managed workflows.
21. Storage/backup/recovery managed workflows.
22. Health framework and `portus-os doctor`.
23. PortusBrowser integration.
24. PortusWindow integration.
25. Communication-adapter contract.
26. OpenRC service configuration.
27. Keyring, PAM, polkit, and AUTH configuration.
28. Graphical session and unattended power policy.
29. Schemas and migration paths for Portus-owned state.
30. Validation covering non-duplication, broad Master Portus authority, cross-user isolation, root-equivalent classification, protected-credential isolation, unattended operation, index bypass, and recovery.

## 46. Validation expectations

PortusOS must not claim a capability merely because a package is present.

Validation must prove, as applicable:

- the command exists;
- the OpenRC service starts and survives reboot;
- the agent can use the capability headlessly;
- structured output is available or safely normalised;
- administrator policy permits/denies/prompts correctly;
- Master Portus runs as the intended non-root UID without a workspace-only Codex sandbox;
- cross-user private data remains denied by default;
- installation permission bundles produce the documented effective authority;
- granular administrator changes take effect;
- root-equivalent grants are identified accurately;
- protected credentials claimed to be non-exportable remain unreadable to the Master UID under negative tests;
- a broad Master profile can operate its granted non-secret computer capabilities;
- visual applications can be launched without a terminal window;
- screenshots and input control work under explicit policy;
- PortusBrowser controls supported browsers without duplicate OS website policy;
- artifacts are registered and delivered correctly;
- failure preserves diagnostics and recovery;
- remote changes do not unnecessarily strand the machine;
- optional applications are absent without breaking the base OS;
- the owner can later install or replace applications normally.

## 47. Research order before creating `packages.yaml`

Resolve these areas in order:

1. Exact Artix repositories, package names, source classes, and OpenRC service packages.
2. Exact `artools` package/profile/repository/build procedure within the locked native x86_64 Linux + isolated verified Artix build-context architecture.
3. Boot, initramfs, encryption, filesystem, swap, and recovery.
4. Kernel, firmware, microcode, graphics, and VMware profiles.
5. OpenRC service graph, logging, time, and user-service model.
6. Firewall ownership and remote-safe updates.
7. Security, secrets storage, hard deny boundary, and secret-backed capability use.
8. X11/i3 session, headless execution, screenshot, input control, keyring, and polkit.
9. Audio and PipeWire OpenRC user services.
10. Portus control-plane runtime and state storage.
11. PortusBrowser installation and browser-option contracts.
12. PortusWindow installation and presentation contracts.
13. Backup, update, rollback, and live-ISO recovery.
14. First-ISO package profile and optional installer selections.

## 48. Current locked application classifications

| Application or tool | Classification | Current decision |
| --- | --- | --- |
| Portus control plane | Required | Core OS capability |
| Codex | Required | Pinned standalone installation |
| Calamares | Required on first-ISO live image | Supported installer framework; not inherently part of the installed runtime |
| PortusBrowser | Required bundled capability | Agent-first multi-browser control through CLI, Broker, native host, and extension |
| PortusWindow | Target-required integration | Agent-first visual and media presentation |
| Chromium | Optional | Supported PortusBrowser browser |
| Google Chrome | Optional | Supported PortusBrowser browser |
| VSCodium | Optional | Human-development profile |
| Zed | Optional | Human-development profile; install source requires research |
| Neovim | Optional | Human-development profile |
| `yay` | Optional | Not required by base or installer runtime |
| VLC | Optional | Conventional media-player option |
| mpv | Optional | Lightweight conventional media-player option |
| FFmpeg | Required | Agent-first media processing |
| Feh or equivalent | Required | Scriptable wallpaper setting |
| Nitrogen | Optional | Human wallpaper chooser |
| CopyQ | Optional | Clipboard history; disabled by default |
| Basic clipboard CLI | Required | Scriptable GUI interoperability |
| Thunar | Optional/recommended fallback | Human file management, not agent control plane |
| Alacritty | Required local fallback | Presentation, AUTH, and recovery; not agent execution engine |
| Screenshot utility | Required | Visual fallback and diagnostics |
| Pointer/keyboard control | Required | Visual fallback only |

## 49. Final implementation position

PortusOS should ship with the smallest package set that provides the **complete agent-operated capability baseline**, not the smallest desktop and not the largest application catalogue.

The correct out-of-box system:

- is broadly programmable;
- can operate headlessly;
- can control visual applications when necessary;
- has real audio, media, document, hardware, network, storage, and recovery capabilities;
- keeps raw secrets outside agent access;
- lets Portus applications own their specialised policy domains;
- offers optional conventional applications without depending on them;
- remains a normal owner-controlled Linux system that can be extended after installation.
