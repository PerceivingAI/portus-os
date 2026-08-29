# PortusOS Project Definition

## 1. Document authority

This document is the authoritative product definition and decision register for PortusOS.

It defines:

- what PortusOS is;
- why it exists;
- its core principles;
- its intended operating model;
- the boundary between the operating system and Portus applications;
- the required control surfaces;
- the product scope and release direction;
- the owner decisions that implementation must preserve.

Related authority:

- `DOCUMENTATION.md` is the public navigation and authority map for the repository; public scope and deferred first-release capabilities are captured by this document, `docs/ACCEPTANCE.md`, and `KNOWN_LIMITATIONS.md`.
- `docs/ACCEPTANCE.md` is authoritative for the exact first-ISO acceptance contract: reference VMware resources, supported boot/install/access path, required real versus minimal implementation, blocking demonstrations, exclusions, and acceptance matrix.
- `docs/POLICY.md` is authoritative for the Master Portus Linux-user model, administrator/root authority, permission bundles, granular post-install policy, cross-user isolation, delegated-agent sandboxing, root-equivalent classification, and local principal enforcement.
- `docs/POLICY_IMPLEMENTATION.md` is authoritative for the concrete first-ISO Linux policy/elevation mapping: installer bundle defaults, explicit grant classes, `portus-privd`, root-owned policy, typed privileged operations, root-equivalent inventory and root-only administrator mutation.
- `docs/SECRETS.md` is authoritative for protected reusable credentials, secret-backed operations, AUTH, provider/browser/Codex credential classification, redaction, backup behavior, and non-export validation.
- `docs/PROTECTED_API_PROVIDER.md` is authoritative for the first protected API credential provider implementation architecture: `portus-apid`, service/store identity, native use/admin IPC, provider binding/TLS/redirect rules, direct `portus-api` use, provisioning/rotation, compatibility-token behavior, and first-ISO positive/negative proof.
- `docs/CREDENTIAL_CLASSES.md` is authoritative for first-ISO credential-class classification direction for Codex, browser sessions, SSH/signing, password-manager, hardware, AUTH and protected-secret backup cases.
- `docs/PROTECTED_API_VALIDATION.md` is authoritative for the adversarial security proof required before the first protected API provider is accepted.
- `docs/HEALTH_RECOVERY.md` is authoritative for common health states/reasons, recovery dispositions, restart/crash-loop limits, safe automatic versus administrator recovery, resource warnings, diagnostic evidence/bundles, `doctor` recovery boundaries, and restart/reboot reconciliation.
- `docs/ARTIFACTS.md` is authoritative for deliberate artifact registration, `artifact_<UUIDv7>` identity, locators/integrity, confidentiality/principal isolation, task/provider relationships, retention/cleanup, local presentation, and deferred delivery semantics.
- `docs/RUNTIME.md` is authoritative for the `portusd` runtime, Portus OS CLI/runtime relationship, local IPC, managed-execution boundary, provider separation, events, and runtime degradation/recovery behavior.
- `docs/CLI.md` is authoritative for the Rust-native `portus-os` command tree, human/JSON/JSONL presentation contract, semantic errors/exit codes, pagination, preconditions/dry-run behavior, task event streaming, capability/policy/health/doctor command boundaries, machine-readable help, and CLI/runtime compatibility behavior.
- `docs/STATE.md` is authoritative for SQLite, Portus-owned mutable-state classes, migrations, transaction boundaries, storage separation, and corruption/recovery behavior.
- `docs/TASKS.md` is authoritative for Master execution routing, Portus task-promotion boundaries, task identity/state, Codex/process/provider relationships, cancellation/retry/approval waiting, results, cleanup, and restart reconciliation.
- `docs/CAPABILITIES.md` is authoritative for registered capability/provider/interface/resource/operation boundaries, manifests, provider identity/versioning, direct provider use, health/lifecycle/policy integration, resource/task affordances, compatibility, degradation, and uninstall history.
- `CODEX_UPDATES.md` is authoritative for Codex runtime, configuration, sessions, delegated-agent sandboxing, extensions, and automation, subject to `docs/POLICY.md` for the Master Linux-user authority model, `docs/SECRETS.md` for protected credential behavior, and `docs/TASKS.md` for the outer Portus task lifecycle boundary.
- `OKF.md` is authoritative for durable knowledge, the Portus OKF Profile, knowledge trust, lifecycle, freshness, retrieval, and validation.
- `docs/CONTROL_PLANE.md` is authoritative for what the PortusOS layer adds, what remains native to Codex and normal commands, and the rule that policy—not tool scarcity—defines agent authority.
- `docs/SYSTEM_INDEX.md` is authoritative for the freshness-aware operational machine graph, generation-scoped index handles, source provenance/evidence, application/process/window/task/provider relationships, available control paths, refresh/reconciliation/rebuild behavior, live-index versus durable-machine-knowledge boundary, principal-filtered quick views, and the rule that index use remains agent-controlled and optional.
- `docs/STRUCTURE.md` is authoritative for the intended repository, runtime, control-plane, and installed-system ownership boundaries.
- `docs/PORTUS_BROWSER.md` is authoritative for the required bundled PortusBrowser capability, its current architecture, browser-domain ownership, and PortusOS integration requirements.
- `docs/SYSTEM_CAPABILITIES.md` is authoritative for required installation capabilities, optional application profiles, Portus-owned components, and the package/component inventory used to derive the future manifest.
- `docs/ISO_BUILD_INSTALLER.md` is authoritative for the locked first-ISO `artools` ISO-build architecture, native x86_64 Linux + isolated-Artix build boundary, interactive live Master/Codex environment, Calamares installer framework, PortusOS build/install ownership, safety invariants, and remaining exact build/install research.
- `docs/BOOT_STORAGE_RECOVERY.md` is authoritative for the locked first-ISO storage topology, encrypted boot/unattended-reboot boundary, required owner unlock after reboot, separate recovery keyslot/credential, and the remaining exact boot/storage/recovery research.
- `docs/HARDWARE.md` is authoritative for the locked first-ISO `linux-lts` normal + `linux` alternate/recovery kernel policy and the remaining firmware, microcode, VMware, driver and hardware research.
- `docs/PACKAGE_POLICY.md` is authoritative for the first-ISO official-Artix-first package-source boundary, no-AUR rule, per-component exception process, package provenance, profiles, manifest requirements, and remaining exact package/source research.
- `docs/OPENRC_SERVICES.md` is authoritative for the first-ISO service/session lifecycle ownership boundary, OpenRC-native supervision direction, Portus health/task non-supervisor boundary, and the remaining exact service-graph research.
- `docs/GUI_SYSTEM.md` is authoritative for the locked first-ISO conditional-`startx`/`dbus-run-session`/minimal-i3 graphical-session baseline, session-owned helpers, visual fallback, and remaining Artix/VMware GUI verification.
- `docs/UPDATES_RECOVERY.md` is authoritative for explicit first-ISO managed updates, Artix/pacman ownership, preflight/migration safety, no-auto-reboot behavior, post-update health proof, bounded update-specific recovery state, and the deferred general-backup/automatic-rollback boundary.
- `BUILD_HARNESS.md` and `BUILD_REQUIREMENTS.md` own the supported public build path and build-host requirements.
- `docs/VALIDATION.md` and `docs/RELEASE.md` own candidate acceptance and publication; private development planning/history is intentionally outside the public repository.

## 2. Product definition

PortusOS is a privacy-focused, secure, reliable, agent-operated Linux operating system based on Artix and OpenRC.

Its defining model is:

> The user communicates intent to an agent. The agent operates the computer primarily through code, command-line interfaces, APIs, local IPC, and structured system controls. The graphical environment exists mainly for presentation, authentication, visual applications, monitoring, and recovery.

PortusOS is not a conventional Linux desktop with an AI assistant added to it. It is an agent control plane for an entire computer, with human-accessible communication, presentation, approval, and recovery surfaces.

The desired long-term user experience is:

```text
user intent
  -> local or remote communication surface
  -> Master Portus
  -> PortusOS control plane
  -> Linux, projects, services, and Portus applications
  -> validated result or artifact
  -> user-selected presentation or delivery surface
```

A user should eventually be able to operate a PortusOS machine without physically using its keyboard, mouse, or display after initial setup. Initial releases may require direct interaction for installation, authentication, recovery, or unfinished workflows, but that is transitional rather than the final operating model.

## 3. Project goal

The project goal is to produce a bootable x86_64 ISO that installs a VMware-first, Artix/OpenRC-based operating system capable of reliable local and remote agent operation.

The installed system must provide:

- an owner-controlled Linux foundation without a systemd dependency;
- a small, non-duplicative, policy-governed agent control surface that complements rather than recreates Codex and normal Linux tools;
- the Portus OS CLI (`portus-os`) and related structured interfaces;
- Master Portus as the primary system orchestrator;
- project-scoped agents and Codex sessions;
- a queryable, agent-managed system index for installed capabilities and live machine state;
- PortusBrowser as a required bundled browser-control capability;
- support for additional specialised Portus applications such as PortusWindow;
- durable state, task, artifact, health, and audit foundations appropriate to unattended operation;
- remote shell access and future communication adapters;
- a minimal graphical presentation and recovery environment;
- normal Linux recovery paths independent of Master Portus.

The ISO is the first delivery artifact. The product is not complete merely because the ISO boots, starts i3, and launches Codex. The installed computer must be able to execute the intended agent-operated loop safely and recoverably.

## 4. Core principles

### 4.1 Privacy

PortusOS must minimise unnecessary disclosure of user data and system state.

Core operation must not require sending local files, activity, screenshots, logs, or system telemetry to an OS vendor or central management provider.

PortusOS must:

- keep operating state local by default;
- make external transmission deliberate and policy-controlled;
- treat artifact delivery, messaging, browser use, MCP, and remote services as data-boundary crossings;
- separate secrets from agent conversations and ordinary logs;
- support inspection and deletion of persistent state;
- avoid hidden telemetry and opaque remote dependencies;
- permit the owner to choose providers and replace them.

### 4.2 Security

The agent needs broad programmable control, but broad control does not mean unrestricted authority.

Security must come from enforceable policy and Linux boundaries rather than from an incomplete catalogue of narrow tools.

The security model is layered around real Linux identities.

- root/the machine administrator remains the source of enforced authority;
- each Master Portus instance runs as a real non-root Linux user;
- Linux UID/GID ownership, modes and ACLs provide the primary Master Portus user boundary;
- Master Portus is **not** placed inside a general-purpose workspace sandbox; the installed VM/PC/server is its broad operating environment;
- administrator-selected permission bundles grant useful authority at install time and may map to groups, ACLs, sudo, polkit, helpers, sockets, devices, network policy and PortusOS policy;
- after installation the administrator may manage the same authority granularly;
- project/task agents may still use Codex/Bubblewrap/worktree sandboxes because their delegated scope is narrower than the Master user;
- sudo, polkit, root-owned helpers/services and Linux ownership remain the privilege boundaries for root-only operations;
- specialised providers may enforce stricter domain policy;
- protected reusable credentials use the independent boundary defined by `docs/SECRETS.md`;
- audit and recovery remain available independently of Master Portus.

The Master user normally receives broad practical control of its own user environment. Routine actions already granted by administrator policy do not inherently require repeated human confirmation. Policy may still classify selected capabilities as allow, prompt/approval, or reject.

Master Portus must not be able to expand its own administrator-enforced authority. Capabilities that effectively permit arbitrary root takeover must be identified as root-equivalent rather than hidden inside an ordinary permission bundle.

Multiple Master Portus users may coexist on one machine: each owns separate home, Codex, application, session, workspace and Portus state, while root can administer all of them. Cross-user private access is denied by default.

Broad Master authority does not automatically include raw reusable credential retrieval. When a reusable credential is provisioned, it is classified according to owner intent and real enforcement as either **protected from Master Portus** or **Master-readable**. Both are valid. A credential that is readable as an ordinary file, same-user provider export, process environment, or other Master-readable channel is Master-readable in practice and must be classified honestly.

A protected credential remains usable. Master Portus may invoke provider-bound authenticated operations through a protected provider/broker that attaches the credential internally and returns the provider result without returning the reusable value. Such operations may run unattended when administrator policy says `allow`; credential non-exportability and operation approval are separate concerns. For SDKs/applications that require a provider-compatible endpoint, a local authenticated compatibility proxy may be used without exposing the real upstream credential. The broker/proxy must bind credentials to approved upstream providers and must not provide arbitrary destination/header/command paths that can be used to exfiltrate them indirectly.

Generic `portusd` state and IPC may coordinate non-secret credential references, provider identity, health and policy relationships, but `portusd` is not the protected credential store and does not transport reusable protected values. If arbitrary Master-owned software strictly requires possession of a real credential and cannot use a broker/proxy, that credential must either remain unavailable through that software path or be deliberately provisioned/reclassified as Master-readable.

### 4.3 Reliability

PortusOS is intended to operate while physically unattended, potentially for months.

Reliability therefore includes:

- deterministic startup;
- durable task and session state;
- recovery after terminal closure, agent crash, reboot, or power loss;
- bounded restart behaviour and crash-loop detection;
- machine-readable health checks;
- actionable logs;
- stable command contracts;
- predictable error classes and exit codes;
- last-known-working and manual recovery paths;
- operation when an optional provider is unavailable;
- safe suspension of work when required approval cannot be obtained.

A local shell remaining available is necessary, but unattended reliability also requires the system to detect failures, preserve evidence, notify through available surfaces, and expose a remote recovery route.

### 4.4 Operational sovereignty and free use

Artix and OpenRC are locked project decisions because owner-controlled operation is more important than maximum ecosystem compatibility.

PortusOS must remain bootable, administrable, updateable, recoverable, and locally operable without mandatory:

- identity verification imposed by the OS foundation;
- vendor attestation;
- proprietary remote approval;
- central account enrolment;
- continuous cloud connectivity;
- externally controlled policy service.

Optional services may require their own accounts, but losing any optional provider must not prevent the owner from accessing files, using the local system, administering services, running local agents, or replacing that provider.

The project does not optimise for the largest software catalogue. It optimises for the capabilities required by agent operation, privacy, security, reliability, and owner control.

### 4.5 Agent-native and code-first operation

The default question for an agent should be:

> Which structured capability or code path performs this operation, and what result did it return?

It should not be:

> What pixels are visible, and where should I click?

Preferred control order:

1. stable local API or Unix-socket interface;
2. stable CLI with structured output;
3. ordinary CLI and scripts;
4. window-manager and accessibility interfaces;
5. browser automation;
6. screenshots, vision, and simulated input as a fallback.

The agent must be able to write, test, and execute code within policy when no existing capability solves a task. This programmability is what prevents PortusOS from requiring a predefined tool for every possible workflow.

PortusOS must not babysit normal agent work. Listing a directory, reading or searching project files, cloning a repository, editing code, running an ordinary command, or using Git are already covered by Codex and standard computer tools. The OS layer should not duplicate them behind `portus-os` commands. It should add only missing system-level awareness, coordination, durable lifecycle, policy-mediated privilege, rollback, health, recovery, artifact handling, and application integration.

Execution must be headless by default. The agent should not need to open a terminal emulator to run a command, script, build, test, or long-running process. Ordinary execution uses Codex and the shell directly. When work requires durable Portus task identity, supervision, reboot continuation, policy-mediated elevation, artifact capture, or recovery, Portus-managed execution starts and tracks the process. A visible terminal is opened only when the user requests it, visual interaction genuinely requires it, or recovery calls for it.

PortusOS must still support screenshots, visual inspection, pointer movement, clicks, keyboard input, and window control. These provide compatibility with conventional applications and user-requested demonstrations, but remain fallback paths after structured interfaces, native commands, window-manager control, and PortusBrowser.

### 4.6 Broad authority, non-duplicative tools

PortusOS adopts the central lesson from Portus MCP:

> Capabilities are bounded primarily by policy, not by enumerating every allowed use case as a separate agent tool.

The exact seven-tool Portus MCP surface is not the PortusOS CLI template. Portus MCP needed broad read, search, edit, patch, execution, context, and policy operations because it crossed a connector boundary to another computer. Master Portus runs locally through Codex, which already provides file access, source search, editing, patching, shell execution, project navigation, and access to Git and installed commands.

Recreating those functions inside the Portus OS CLI (`portus-os`) would add a second filesystem interface, shell, and project toolset without adding computer capability. PortusOS must instead expose only the missing operating-system domains, expected to include:

- the agent-managed system index and machine relationships;
- capability and application registration, lifecycle, and health;
- durable tasks and managed execution when work must outlive the current Codex process or terminal;
- policy inspection, approvals, privilege mediation, and secret-backed actions;
- transactional system workflows requiring preflight, expected-state checks, rollback, or recovery;
- artifacts, presentation, communication, and delivery state;
- system health, degradation, diagnostics, and recovery;
- adapters for network, updates, storage, services, sessions, displays, and other domains where cross-component coordination materially improves reliability.

Ordinary files, source code, Git, shell commands, builds, tests, scripts, and stable application CLIs remain directly usable. The complete inclusion test and boundary are defined in `docs/CONTROL_PLANE.md`.

### 4.7 Agent-managed system index

PortusOS must provide a queryable machine map so an agent can quickly discover installed capabilities, running applications, application instances, processes, windows, workspaces, services, devices, tasks, and specialised provider resources.

The index is not automatically injected into every Master Portus prompt or task context. The Master system instruction should state that the index exists and what it is for; a machine-wide skill should explain its commands, freshness rules, refresh and reconciliation behaviour, and provider-specific drill-down.

The agent decides whether to use the index. It may query cached state, request an on-demand refresh, perform a broader rescan or reconciliation, call a specialised provider such as PortusBrowser directly, or bypass the index and inspect native Linux sources.

The index is a navigation aid, not an authority boundary. Policy controls what the agent may do. Absence from the index must not imply that an authorised operation is unavailable, and index failure must not prevent use of native commands or direct application interfaces.

Observed state, agent-managed annotations, durable registry data, and application-owned state must remain distinguishable. Full requirements are defined in `docs/SYSTEM_INDEX.md`.

### 4.8 Composable specialised applications

PortusOS is the operating control plane. It must not directly implement every specialised capability.

Portus applications provide focused functionality on top of the OS control surface. Examples include:

- PortusBrowser, a required bundled subsystem implemented as a CLI, local Broker, browser native-messaging host, browser extension, structured protocol, and browser-domain policy layer, for operating multiple Chromium-based browsers, sessions, profiles, windows, tabs, pages, and elements;
- PortusWindow for lightweight ephemeral display, media presentation, visualisation, and simple navigation without requiring a full browser;
- future messaging adapters;
- future media, document, device, data-processing, or delivery tools.

PortusOS owns shared concerns such as process lifecycle, task context, general filesystem and execution perimeter, artifact registration, health, logging, discovery, and recovery. Each application owns its specialised domain and may enforce a stricter internal policy.

For PortusBrowser, browser-level website restrictions, navigation permissions, page actions, browser profiles, and browser-session policy belong to PortusBrowser. PortusOS must not duplicate that policy as a second website whitelist. Browser packages normally remain replaceable applications, but the first-ISO profile deliberately requires Chromium because the Codex-first boot experience needs ordinary ChatGPT subscription authentication in a local browser and P15 needs one concrete reference browser. Additional supported Chromium-family browsers remain optional.

### 4.9 Minimal presentation desktop

The graphical environment is not the primary work surface.

Its main purposes are:

- showing results and artifacts;
- presenting browser or application output when visual rendering is necessary;
- browser-based authentication;
- displaying status, diagnostics, or approval prompts;
- hosting PortusWindow and other visual Portus applications;
- emergency direct operation and recovery.

The desktop should be lightweight, deterministic, scriptable, and easy for agents to control through code. It does not need to reproduce a broad consumer desktop environment.

### 4.10 Human-independent operation, human-controlled authority

The agent may operate without continuous human presence, but the owner remains the source of authority.

PortusOS must distinguish:

- actions the agent may take automatically;
- actions allowed only within a configured scope;
- actions requiring ordinary approval;
- actions requiring elevated approval;
- actions that are prohibited.

When approval is unavailable, the agent should preserve progress, queue or suspend the blocked action when possible, and report what is needed. It must not bypass the boundary simply because the user is absent.

## 5. Problem statement

Conventional operating systems and current computer-use agents are organised around a human-operated graphical desktop.

Typical computer-use systems give an agent screenshots and simulated keyboard or mouse control. That approach:

- treats the agent like a remote human user;
- makes operation fragile and slow;
- depends on visual state and layout;
- provides weak structured feedback;
- makes concurrent application control difficult;
- requires a new interaction strategy for every graphical application;
- underuses the agent's ability to write and execute code.

A normal Linux distribution also does not provide a coherent agent control plane. It may expose thousands of commands, but it does not automatically provide:

- a stable agent authority model;
- broad policy-governed primitives;
- durable task and artifact ownership;
- capability discovery;
- application integration contracts;
- remote approval and delivery paths;
- agent-oriented health and recovery;
- consistent machine-readable system operations.

PortusOS exists to provide that missing layer while retaining ordinary Linux as the underlying recovery and implementation substrate.

## 6. Intended users

Primary users are people who want a dedicated computer or VM that agents can operate on their behalf.

They may interact through:

- SSH;
- a local terminal;
- a messaging application;
- a future Portus web or mobile interface;
- another approved communication adapter.

A user may be physically near the machine, but the architecture must not assume that they are.

The intended owner may:

- leave the device unattended for long periods;
- use it from another city or country;
- delegate development, research, downloading, processing, monitoring, maintenance, and delivery work;
- use several Portus applications together;
- require an auditable record of what the agent changed or transmitted;
- need an independent recovery path if the primary agent fails.

PortusOS is not primarily designed for users seeking a conventional general-purpose desktop distribution.

## 7. Intended operating model

### 7.1 Normal request flow

```text
user sends request
  -> communication surface authenticates the requester
  -> Master Portus interprets intent and effective policy
  -> existing task/session is resumed or a new task is created
  -> Master Portus uses broad PortusOS primitives
  -> Linux commands, code, projects, and Portus applications perform the work
  -> results are validated
  -> artifacts are registered
  -> results are displayed or delivered through the requested surface
  -> actions, failures, approvals, and delivery state remain auditable
```

### 7.2 Example: downloading a video

Conventional PC model:

```text
user opens an IDE
  -> user asks an agent to help write a script
  -> user runs or fixes the script
  -> user finds the downloaded file
```

PortusOS model:

```text
user asks remotely for a video
  -> agent evaluates policy and available capabilities
  -> agent inspects or creates the required code
  -> agent tests the workflow
  -> agent downloads and validates the file
  -> PortusOS registers the artifact
  -> the artifact is shown locally, sent through the communication surface,
     or placed at another owner-approved destination
```

No dedicated `download-youtube-video` OS tool is required. The agent uses general execution and file capabilities, or a specialised application if one is available.

### 7.3 Example: browser research

PortusBrowser is a required PortusOS component. It combines an agent-facing CLI, local Broker, native-messaging host, Chromium extension, structured protocol, snapshot/action model, and browser-domain policy layer. It can control multiple connected browser sessions, windows, tabs, pages, and structured page elements concurrently.

The agent can:

- list and target connected browsers and tabs;
- navigate many tabs and browser sessions concurrently;
- capture structured page snapshots and screenshots;
- click, type, fill forms, press keys, scroll, hover, and drag through structured commands;
- wait for page, URL, text, role, or element conditions;
- inspect browser console and network data when policy permits;
- use authenticated browser sessions without reading the underlying credentials;
- download and register artifacts;
- associate browser state with a task;
- detect disconnected, expired, blocked, unavailable, or stale browser state and recover appropriately.

PortusOS does not need a separate top-level agent tool for every browser action. It discovers PortusBrowser, invokes its structured interface, tracks task and artifact context, and supervises the outer process boundary.

Website allowlists, page-action restrictions, browser-profile permissions, and other browser-domain policy belong to PortusBrowser. PortusOS must not duplicate them. Chromium and Google Chrome are optional browser installations supported by this architecture.

### 7.4 Example: ephemeral display

When the user is near the device or watching its display, the agent may use PortusWindow to:

- show an image;
- preview a generated artifact;
- display a light web page;
- present a QR code or authentication instruction;
- show task status;
- close sensitive output after a policy-defined period.

The display is an output surface, not evidence that the user must manually complete the task.

### 7.5 Example: months of remote operation

A user may leave the machine unattended and continue issuing instructions remotely.

The system should be able to:

- survive user disconnection;
- keep approved long-running work alive;
- resume Codex and task state after reboot;
- restart supervised components;
- preserve logs and artifacts;
- request approval remotely;
- report degraded services;
- expose SSH recovery independently of Master Portus;
- avoid requiring physical access for ordinary administration.

## 8. Product architecture

### 8.1 Layered model

```text
Owner policy and authenticated user intent
  -> Master Portus orchestration
  -> Codex and native tools for ordinary files, Git, code, and commands
  -> PortusOS additions when system-level coordination is required
       - machine index and relationships
       - capability and application lifecycle
       - durable tasks and managed execution
       - policy, approvals, and privilege mediation
       - health, degradation, and recovery
       - artifacts, presentation, communication, and delivery
       - transactional system adapters
  -> Linux and registered capabilities
       - OpenRC services
       - files, processes, network, packages, and code
       - Codex sessions and project agents
       - PortusBrowser
       - PortusWindow
       - future Portus applications
       - ordinary Linux applications
```

### 8.2 Native Linux layer

Ordinary Linux remains the final implementation and recovery layer.

It includes:

- OpenRC;
- shell and process tools;
- files and Unix permissions;
- package management;
- Git;
- SSH;
- networking tools;
- sockets and IPC;
- logs;
- normal boot and repair mechanisms.

PortusOS must not hide Linux so completely that failure of the Portus abstraction makes the machine unrepairable.

### 8.3 PortusOS control plane

The control plane supplies system-level capabilities that Codex, normal commands, Git, and application-owned interfaces do not already provide.

It owns or coordinates:

- the agent-managed system index and machine relationships;
- capability registration, application integration, lifecycle, and health;
- durable task state and managed execution for work requiring persistence or coordination;
- effective policy, approvals, privilege mediation, and secret-backed actions;
- prepared, conditional, transactional, or recoverable system changes;
- task, session, project, process, application, and artifact relationships;
- artifact presentation, delivery, retention, and cleanup state;
- health, degradation, diagnostics, audit, and recovery;
- network, update, storage, service, session, display, and other adapters where PortusOS adds preflight, rollback, or cross-component reliability.

It does not own ordinary directory listing, file reading or search, source editing, project patching, Git operations, builds, tests, scripts, or shell commands. Those remain direct Codex and native capabilities.

The control plane uses the Portus OS CLI (`portus-os`), one modular local `portusd` service where persistent coordination is required, SQLite-backed Portus state, OpenRC, Unix sockets, Codex configuration, and application CLIs. Ordinary Codex/Linux operations remain direct and do not pass through `portusd`. `docs/RUNTIME.md` and `docs/STATE.md` define the authoritative runtime and state boundaries.

### 8.4 Portus applications

A Portus application is a specialised capability provider built to work with the OS control plane.

A compatible application should expose enough structured information for PortusOS to determine:

- identity and version;
- available capability domains;
- invocation interface;
- configuration ownership;
- required filesystem and network permissions;
- lifecycle and supervision model;
- health check;
- accepted inputs;
- produced outputs and artifacts;
- log and recovery behaviour.

Applications should expose stable structured CLI or local RPC contracts. They should not require their complete command surface to be permanently expanded into the primary agent tool list.

### 8.5 Capability discovery

Capabilities must be discoverable at runtime.

The agent should be able to learn:

- which Portus applications are installed;
- which version and contract they expose;
- whether they are healthy;
- which policies apply;
- how to inspect their concise schema or help;
- what outputs they can produce.

Capability discovery prevents Master Portus from hardcoding internal details of every application and allows implementations to be replaced without changing the whole OS model.

## 9. Core product surfaces

### 9.1 ISO and installer surface

The ISO is the installable delivery artifact.

It must:

- boot on the selected x86_64 VMware target;
- provide a recoverable live environment;
- run a safe installer;
- install the Artix/OpenRC base and PortusOS control surfaces;
- validate required components before success;
- preserve install diagnostics without exposing secrets;
- support documented repair of an installed system.

The ISO-build and installer framework decisions are now locked by `docs/ISO_BUILD_INSTALLER.md`: PortusOS uses Artix `artools` in an isolated verified Artix build context on native x86_64 Linux, requires the live image itself to expose the Master Portus/Codex investigation path, and uses Calamares for installation, while PortusOS owns the relevant profiles/configuration, manifests, overlays, staging, safety and validation inputs. The first-ISO encrypted reboot/unlock boundary is locked by `docs/BOOT_STORAGE_RECOVERY.md`. Exact `artools` package/profile/build procedure, Calamares package/module configuration, bootloader/initramfs details, and other Linux-specific mechanics still require research and Linux verification before implementation is fully locked.

### 9.2 Portus OS CLI control surface

The **Portus OS CLI** is a core PortusOS interface, not a deferred convenience layer. Its executable is `portus-os` and is implemented in **Rust** as a native compiled binary.

The normal CLI is intentionally a thin typed client of `portusd`: it owns argument parsing, wire/client validation, bounded human/structured rendering, exit-code mapping, and explicitly assigned daemon-independent diagnostics, while subsystem business logic remains in the authoritative runtime/task/index/provider/policy layers. `portus-os doctor` is the deliberate bounded recovery exception and must be able to diagnose a dead daemon without becoming a second control plane.

The Rust implementation should share protocol/wire/client types across the CLI boundary rather than redefining them per subcommand. The JSONL Unix-socket contract itself remains language-neutral; this decision does not require every PortusOS component to be written in Rust.

The `portus-os-cli` machine-wide Codex skill is distinct from the executable. It is instruction/reference content for using `portus-os`, while independent workflows remain independent skills.

Portus CLI executables must identify the component they control. There is no generic `portus` or `portus-cli` executable/alias. The project-wide naming form is `portus-<component>`, including `portus-os` for the OS itself, `portus-browser` for PortusBrowser, and `portus-master` if a dedicated Master Portus CLI/launcher is required.

Its purpose is to provide broad, composable, machine-readable operations with:

- stable exit codes;
- bounded output;
- JSON or equivalent structured output where appropriate;
- dry-run support for relevant mutations;
- expected-state or hash checks where relevant;
- effective-policy checks;
- approval integration;
- task association;
- audit logging;
- actionable error classes.

It may delegate to native commands such as `rc-service`, `tmux`, `i3-msg`, `nmcli`, Git, or a Portus application. Direct Linux execution remains available when policy permits and when a Portus abstraction is missing or unsuitable.

### 9.3 Master Portus surface

Master Portus is the primary system orchestrator.

It should:

- understand the current user request and communication surface;
- use Codex and normal commands directly for ordinary files, projects, Git, code, builds, tests, scripts, and shell work;
- query system, task, capability, application, and health context when useful;
- create, resume, delegate, queue, fork, or suspend work through Codex-native execution primitives where they fit;
- keep its own long-lived context focused primarily on user intent, orchestration, decisions, and result synthesis, using Codex compaction rather than treating the Master transcript as durable OS state;
- choose between direct Master execution, bounded Codex subagents, independent headless Codex root sessions, and visible interactive sessions according to the work shape;
- invoke PortusOS capabilities only when they add indexing, cross-component coordination, durable lifecycle, policy-safe system workflows, health, recovery, artifacts, or delivery beyond Codex's own harness;
- coordinate project agents and subagents without duplicating Codex's native scheduler/session machinery;
- use specialised applications through their contracts;
- register and deliver artifacts;
- request approvals;
- report failures and degraded state;
- use its `AGENTS.md` operating charter to preserve the manager/orchestrator role, delegation discipline, and non-duplication rules across compaction and session continuation;
- use the machine-wide `portus-os-cli` skill for detailed Portus OS CLI operation while keeping independent reusable workflows in independent skills;
- maintain and consult a durable per-machine knowledge map for stable machine-specific facts, while using the system index/providers for live truth;
- preserve recovery paths.

Master Portus runs as a real non-root Linux user and is not constrained to a Codex workspace-only sandbox. It normally has broad access to the resources available to that user. Administrator policy controls additional system authority, and root-only operations cross explicit Linux privilege boundaries. Master Portus cannot silently grant itself broader enforced authority.

### 9.4 Project agent surface

Project agents work inside assigned project folders and worktrees.

They should normally have:

- full operation inside the assigned project and task temporary space;
- build and test execution;
- project-approved network access;
- no unrelated project access;
- no unilateral system-policy changes;
- no broad secrets access;
- isolated worktrees for concurrent writers.

Project agents remain distinct from Master Portus even when Codex subagents are used inside a session.

### 9.5 Policy and approval surface

The policy surface must allow the user and agent to inspect effective authority without allowing the agent to silently broaden it.

It should support:

- effective-policy inspection;
- administrator-defined permission bundles during installation;
- granular administrator changes after installation;
- allow/prompt/reject behavior where appropriate;
- time- or task-bounded grants;
- owner-authorised policy updates;
- root-equivalent permission warnings;
- audit review;
- clear denied-action explanations.

Policy applies to filesystem/shared-resource access, privileged command classes, network access/exposure, services, package installation, devices, external delivery, destructive operations, reboots, cross-user access, and policy changes. Protected reusable credentials are governed separately by `docs/SECRETS.md` and are not made exportable merely by selecting a broad normal Master profile.

### 9.6 Task and session surface

Long-running work must not exist only in a tmux pane or chat transcript.

The target architecture requires durable task metadata including:

- task identifier;
- requesting user and surface;
- project or workspace;
- current state;
- responsible agent or process;
- policy and approval state;
- timestamps;
- progress events;
- outputs and artifacts;
- failure and retry information;
- resume policy.

Codex owns its root-session/thread history, subagent trees, session resume/fork/queue behavior, goals, and conversation compaction. tmux preserves live terminal visibility and attachment only. OpenRC supervises OS services. Portus task state is required only when work needs durable OS-level requester, lifecycle, policy, process, provider, artifact, health, or recovery relationships that Codex's own thread/session model does not own.

A complex task does not inherently require a visible terminal or a Portus task record. Independent multi-step work may run as a headless Codex root session while Master Portus remains available to the user; bounded sidecar work may remain a Codex subagent. The authoritative routing and Portus task-promotion rules are locked in `docs/TASKS.md` and reuse the Codex-native execution primitives in `CODEX_UPDATES.md`.

The first implementation may deliver this incrementally, but the architecture must not treat tmux as the task supervisor or duplicate Codex's native execution harness.

### 9.7 Artifact and delivery surface

A completed task often produces a file or other deliverable.

PortusOS must be able to associate artifacts with tasks and record:

- path or resource identifier;
- type and size;
- checksum;
- confidentiality or delivery policy;
- retention;
- delivery destinations;
- delivery state and failures.

Potential presentation or delivery paths include:

- PortusWindow or the local display;
- an approved messaging adapter;
- SSH, SCP, or SFTP;
- a secure download surface;
- an owner-approved cloud destination;
- another registered Portus capability.

External delivery is a data-exfiltration boundary and must be policy-controlled.

### 9.8 Communication surface

SSH is the required independent remote shell and recovery surface.

The first ISO additionally bundles **Portus MCP + OpenAI Secure MCP Tunnel** as the canonical optional remote **agent-access** path. After Master Portus reaches Codex, the owner may configure a `portus-local` tunnel targeting the local Portus MCP endpoint. Portus MCP and `tunnel-client` are user/session-owned and are not OpenRC machine daemons by default; declining setup must leave local Codex and PortusOS fully usable.

Future communication adapters may include messaging, web, or mobile interfaces. They should connect to the same task, policy, and Master Portus system rather than creating unrelated agent silos.

Each adapter must define:

- requester identity mapping;
- authentication and authorisation;
- transport security;
- replay and duplicate handling;
- approval presentation;
- attachment handling;
- delivery guarantees;
- outage behaviour;
- audit events.

A compromised messaging account must not automatically grant unrestricted control of the machine.

### 9.9 Graphical presentation surface

The locked current direction remains X11, i3, and Alacritty, subject to exact OpenRC session validation.

The graphical surface should support:

- deterministic workspace and window control through commands and IPC;
- PortusWindow and visual Portus applications;
- optional Chromium-based browsers and optional IDEs when installed;
- authentication prompts;
- artifact previews;
- status and diagnostics;
- emergency local shell and file access;
- screen locking and concealment of sensitive output.

It must not be the only way to perform ordinary system operations.

### 9.10 AUTH surface

`AUTH` is the dedicated human-operated authentication and secret-entry environment.

It is used when direct user action is necessary for:

- Codex login;
- provider CLI login;
- SSH key import or generation;
- browser authentication;
- recovery-key handling;
- other secrets that must not enter normal agent conversations.

Remote and future communication flows may add secure approval or authentication mechanisms, but they must not weaken the separation between agent reasoning and raw secret entry.

### 9.11 Health and recovery surface

`portus-os doctor` is the reusable PortusOS diagnostic surface, not only a first-boot checklist.

It should eventually inspect:

- required OpenRC services;
- storage and filesystem health;
- memory and resource pressure;
- network, DNS, and clock state;
- SSH and firewall state;
- Codex installation, auth, config, sandbox, and sessions;
- Portus control-plane state;
- task backlog and failures;
- application health;
- communication adapters;
- artifact delivery failures;
- backups and stale updates;
- repeated crash or restart patterns.

Diagnostics must be available in human-readable and machine-readable forms.

Recovery must remain possible through Linux, SSH, a TTY, OpenRC commands, and the live ISO even when Master Portus or the graphical session is broken.

## 10. Linux foundation decisions

### 10.1 Artix and OpenRC

Artix/OpenRC is a hard project constraint.

The reason is not merely lower resource usage. It is part of the project's sovereignty, privacy, and long-term control model.

PortusOS must not depend on systemd units, `systemctl`, systemd timers, systemd user services, or systemd login behaviour.

Required software that assumes systemd must be explicitly tested and either adapted, isolated, replaced, or rejected.

The Linux research must optimise the selected Artix/OpenRC path rather than reopening the decision based on broader package compatibility.

#### 10.1.1 OpenRC lifecycle and supervision ownership

`docs/OPENRC_SERVICES.md` locks the first-ISO ownership boundary: OpenRC owns machine-scoped services and Portus machine daemons; authenticated user/session lifecycle owns X11/i3 and intrinsically graphical/session processes; Portus health observes and coordinates bounded recovery without becoming a competing init system; Portus tasks do not replace service supervision; Master Portus/Codex is not a generic boot daemon; and tmux owns interactive attachment rather than machine-service lifecycle.

`portusd`, `portus-privd`, and `portus-apid` are OpenRC-owned machine services for the first ISO. Restart-safe Portus daemons should use OpenRC-native supervision, including `supervise-daemon` where appropriate, rather than introducing a second general-purpose supervisor. P15 separately locks the PortusBrowser Broker as provider-owned/native-host-on-demand, so Linux verification must preserve that owner rather than add an OpenRC Broker service. Exact Portus service scripts, package names, runlevels, dependencies and restart parameters remain Linux-verified.

### 10.2 VMware-first, physically unattended

VMware is the first validation target, but the machine must be capable of safe unattended operation.

The VMware contract must research and validate:

- display resizing;
- networking;
- clipboard and shared-folder boundaries;
- shutdown and reboot integration;
- time handling;
- suspend and resume;
- resource sizing for agents and subagents;
- recovery after host or guest restart.

Bare-metal support means the system should boot, operate safely, connect to common networks, and remain recoverable. It does not mean parity with a full consumer desktop distribution.

### 10.3 Boot, encryption, storage, and recovery

The first-ISO boot/storage engineering baseline is now locked by `docs/BOOT_STORAGE_RECOVERY.md`: GPT/UEFI; 512 MiB FAT32 ESP at `/boot/efi`; 2 GiB unencrypted ext4 `/boot`; LUKS2 AES-XTS-plain64/512 with Argon2id targeted around 2 seconds and capped at 256 MiB PBKDF memory; VG `portus`; ext4 root; 4 GiB encrypted swap; roughly 5% VG reserve; conservative ext4 `relatime`; GRUB 2 UEFI with visible alternate-kernel recovery; and mkinitcpio's non-systemd `encrypt -> lvm2` path with normal + broader fallback images for both Artix kernels.

The encrypted-boot behavior remains owner-controlled: cold boot/reboot requires an owner-provided unlock credential; automatic network/TPM/embedded-key unlock is deferred; and a separate independent high-entropy recovery keyslot/credential is required.

The remaining Linux work is verification rather than owner redesign: exact Artix package versions/commands/config paths, effective Argon2id results, Calamares implementation, generated kernel command line, recovery-credential UX, and the live-ISO/GRUB/mkinitcpio repair procedure. Update/recovery behavior is governed by the locked `docs/UPDATES_RECOVERY.md` contract and only its exact Artix/repair implementation remains to be verified.

### 10.4 Package and supply-chain policy

PortusOS uses one machine-readable installation contract.

Package selection is capability-driven rather than catalogue-driven.

The supported first-ISO supply-chain boundary is locked by `docs/PACKAGE_POLICY.md`:

- official Artix repositories are the normal/default source for selected third-party system software;
- the AUR is not part of the supported PortusOS first-ISO supply chain and PortusOS does not ship an AUR helper as part of its supported baseline/profiles;
- if selected software is unavailable from official Artix repositories, it is evaluated case by case rather than falling through to a generic second source;
- Portus-owned software uses PortusOS-owned build/staging contracts;
- already explicit component-specific external distribution contracts, such as the pinned Codex path, remain valid subject to the same provenance/verification/update requirements.

Every required package or external artifact must have a known source, version or resolution policy, verification method, licence status, installation owner, and failure behaviour.

The current package names in older documentation are candidates, not a validated manifest.

Codex must follow the pinned standalone installation policy from `CODEX_UPDATES.md`; npm is not the PortusOS Codex installation contract.

## 11. Persistent context and knowledge

PortusOS separates persistence by responsibility:

- `AGENTS.md`: automatically discovered instructions and routing;
- skills: reusable procedures and scripts;
- normal project documentation: coherent specifications, plans, and reports;
- OKF bundles: curated durable knowledge with provenance, trust, lifecycle, and freshness;
- Codex sessions: conversational and task-continuation context;
- Git: authoritative project history;
- Portus state registries: operational metadata;
- tmux: live terminal attachment;
- OpenRC: service supervision.

Codex native memories remain disabled in the baseline. `OKF.md` controls durable knowledge policy.

## 12. Scope

### 12.1 In scope for the PortusOS product

- Artix/OpenRC operating-system foundation.
- Bootable x86_64 ISO.
- VMware-first installation and validation.
- Encrypted storage and documented recovery.
- Codex runtime and managed policy.
- Master Portus and project-agent operation.
- Broad policy-governed control primitives.
- Core Portus OS CLI (`portus-os`).
- Capability discovery and application integration contracts.
- OpenRC service and process lifecycle.
- SSH and remote recovery.
- Task, session, state, health, audit, and artifact foundations.
- Minimal X11/i3 presentation environment.
- Authentication and secret-entry separation.
- PortusBrowser, PortusWindow, and future application integration.
- Local-first OKF knowledge.
- Machine-readable diagnostics and validation.
- Update, rollback, and recovery architecture.
- Privacy, security, reliability, and sovereignty policies.

### 12.2 First-release versus target architecture

The target architecture includes unattended remote operation, durable tasks, communication adapters, artifact delivery, application registration, health supervision, and policy-based approvals.

The first ISO may implement only the minimum viable form of some of these surfaces. However:

- the architecture must reserve their ownership boundaries;
- early shortcuts must not contradict the target model;
- tmux must not be mistaken for a complete durable-task system;
- SSH must not be mistaken for the complete communication architecture;
- direct shell scripts must not become an unstructured substitute for the control plane;
- the CLI cannot be deferred as an optional Stage 2 convenience.

### 12.3 Out of scope

PortusOS is not intended to be:

- a full general-purpose consumer desktop distribution;
- a catalogue of one tool per anticipated user workflow;
- a GUI-first computer-use system based mainly on screenshots and mouse simulation;
- dependent on one messaging, cloud, browser, model, or agent provider;
- a systemd-compatible dual-init distribution;
- a central cloud-management service;
- a replacement implementation for every specialised Portus application;
- an unrestricted autonomous root agent;
- dependent on the graphical session for recovery;
- dependent on native Codex memories.

## 13. Hard constraints

Implementation must preserve these constraints:

- Artix/OpenRC remains the operating-system base direction.
- No required systemd execution path.
- Privacy, security, reliability, and owner control outrank broad package compatibility.
- Core OS operation remains local and replaceable.
- The agent uses broad programmable primitives within configured policy.
- The primary agent tool surface remains small enough for effective selection and context use.
- The OS must not attempt to enumerate every workflow as a separate tool.
- the Portus OS CLI (`portus-os`) is a core control surface; no generic `portus`/`portus-cli` executable is part of the architecture.
- Portus applications integrate through contracts rather than hardcoded internal coupling.
- GUI automation is a fallback, not the default operating mechanism.
- Master Portus is a broad non-root Linux user, not a workspace-sandboxed agent; administrator/root policy remains the higher authority and an independent recovery route.
- The user can inspect effective policy and audit significant operations.
- Required or selected components must be validated before installation succeeds.
- External delivery and network exposure are deliberate policy-controlled actions.
- Secrets do not enter ordinary agent conversations or logs.

## 14. Current implementation status

The public repository contains implemented runtime, build, installer, validation and release tooling. Release readiness remains evidence-bound: a generated ISO is not accepted until the exact candidate passes the public acceptance, validation and publication gates.

At the time of this update:

- `CODEX_UPDATES.md` defines the current Codex baseline.
- `docs/POLICY.md` and `docs/SECRETS.md` define the locked Master Portus Linux-user authority model and protected reusable-credential boundary.

- `OKF.md` defines durable knowledge policy.
- this document defines the clarified agent-operated OS product.
- `docs/STRUCTURE.md` defines the corresponding ownership and runtime structure.
- public build and release work is driven by the checked-in build contracts, `BUILD_HARNESS.md`, `docs/ACCEPTANCE.md`, `docs/VALIDATION.md`, and `docs/RELEASE.md`;
- the Portus-owned Rust workspace, Linux IPC/runtime integration tests, machine-readable build contracts, installer/storage integration, Artix build adapter, candidate/evidence harness, OSS audit tooling and release tooling are implemented and testable from this repository. Exact release readiness remains conditional on the supported Artix/native ISO build and the full VMware candidate validation evidence;
- PortusBrowser is a required bundled capability sourced from its independent project; this repository owns its packaging, lifecycle, capability, task, artifact, health, recovery, and release integration rather than reimplementing its browser domain.
- PortusWindow and other Portus applications remain separate capability projects integrated according to their release scope.

## 15. Required Linux research before final ISO plan reconciliation and broad implementation

The Linux research pass must focus on the selected Artix/OpenRC architecture and resolve:

1. Artix/OpenRC ISO tooling and Linux build environment.
2. UEFI/BIOS, GRUB, initramfs, LUKS, LVM, filesystem, swap, fallback boot, and repair.
3. Exact official Artix repositories, package names, signatures/keyrings, mirrors, machine-readable package resolution, and case-by-case sourcing decisions for selected software unavailable from Artix within the locked no-AUR boundary.
4. OpenRC service inventory, runlevels, dependencies, health checks, and recovery commands.
5. X11/i3/elogind session lifecycle, D-Bus, keyring, polkit, portals, audio, lock, and remote attachment.
6. VMware integration and minimum/recommended resource profiles.
7. NetworkManager, SSH, firewall, VPN, DNS, remote exposure, and recovery access.
8. Linux host security compatible with Codex Bubblewrap and unprivileged user namespaces.
9. Update independence, backups, rollback, preservation/validation of the locked dual-kernel recovery paths, and recovery architecture.
10. Persistent agent runtime, service supervision, durable task ownership, and crash-loop policy.
11. Capability registration, local IPC, application lifecycle, and structured interface conventions.
12. Artifact storage, classification, retention, and delivery boundaries.
13. Communication-adapter security, identity, approval, and outage behaviour.
14. Installer trust boundaries, destructive-operation safety, resumability, and secret-safe logging.
15. Validation matrix for boot, installation, unattended operation, failure, recovery, upgrade, and security.

## 16. Key owner decisions

### Agent-operated computer decision

PortusOS is an agent-operated computer, not a desktop distribution with optional agent assistance.

### Control-plane decision

The OS is the central control plane. Specialised Portus applications build on it.

### Broad-primitives decision

The primary control surface uses a small number of broad, composable primitives. User-configured policy defines their perimeter.

### CLI decision

The **Portus OS CLI** is the core OS API for agents and humans and executes as `portus-os`. It is implemented in Rust as a native compiled executable with a thin typed-client boundary over the language-neutral Portus JSONL/Unix-socket protocol. `portus-os doctor` retains bounded daemon-independent diagnostics; ordinary runtime business logic is not duplicated into the CLI. The `portus-os-cli` Codex skill is instruction/reference content, not the executable. Portus user-facing CLI names identify their component using `portus-<component>`; generic `portus` and `portus-cli` executables/aliases are not used. It is not deferred to a later convenience phase.

### Application-boundary decision

PortusBrowser, PortusWindow, and future applications own specialised capability domains. PortusOS owns shared lifecycle, policy, task, artifact, health, discovery, and recovery concerns.

### Remote-first decision

The system is designed for prolonged operation without physical attendance. SSH remains the required independent remote shell/recovery path. Portus MCP + OpenAI Secure MCP Tunnel is the bundled optional remote agent-access path from the first ISO; setup follows Master/Codex login and remains user/session-owned. Later messaging/web/mobile adapters must share the same task and policy system.

### Presentation-desktop decision

The desktop is primarily a presentation, authentication, visual-application, and recovery host. The agent works through code first.

### First-ISO ISO-build architecture decision

The Artix `artools` stack is the supported first-ISO ISO-build framework. The accepted-ISO build runs on native x86_64 Linux with an isolated verified Artix build context that is authoritative for `artools`, Artix repository/keyring/package resolution, rootfs/profile construction and ISO production; the current EndeavourOS VM is therefore an acceptable outer build host. PortusOS owns its `artools` profile/configuration, package manifests, local-package staging, rootfs/live-ISO overlays, build metadata and validation/release inputs. The first produced live ISO must already be an interactive PortusOS environment with Master Portus non-root login, X11/i3, Alacritty/tmux, `portus-bootstrap`/`portus-master`, Codex, first-party Portus binaries, bundled Portus MCP and bundled OpenAI `tunnel-client` available before installation so Codex can investigate the VM directly and the owner can optionally expose that VM through the Secure MCP Tunnel path. A parallel Portus-owned general-purpose ISO builder is not part of the first-ISO architecture.

### First-ISO installer decision

Calamares is the supported first-ISO installer framework. The engineering flow is minimal (welcome -> locale -> keyboard -> storage -> user/credentials -> summary -> install -> completion), with execution ordered by responsibility. The original zero-custom-module preference was tested against signed Artix Calamares `3.4.2-4`; source inspection proved that stock automated/custom-layout execution cannot construct the required LUKS2-to-LVM VG/LV topology without manual partition-editor work. PortusOS therefore selects exactly one narrow verified-gap job, `portus-storage`, while keeping generic installer work with stock Calamares/Linux tooling. PortusOS owns configuration, ordering, Portus-specific integration, safety requirements and installed-target/first-boot validation.

### Encrypted reboot decision

The first ISO requires owner-provided LUKS unlock after cold boot/reboot and a separate recovery keyslot/credential. It deliberately does not use automatic network, TPM-based automatic, or embedded-key unlock merely to make reboot unattended. Unattended operation is expected after successful encrypted boot; autonomous passage through the encrypted pre-boot boundary is deferred.

### First-ISO storage-topology decision

The first ISO uses GPT/UEFI with a 512 MiB unencrypted FAT32 ESP at `/boot/efi`, a 2 GiB unencrypted ext4 `/boot`, and LUKS2 containing VG `portus` with ext4 root, 4 GiB encrypted swap and roughly 5% free-VG reserve. LUKS2 uses the conservative AES-XTS-plain64/512 + Argon2id (~2 seconds, 256 MiB cap) baseline. GRUB 2 UEFI and mkinitcpio are the selected boot mechanisms. Separate `/home`, `/var`, `/srv` LVs and hibernation are not required.

### First-ISO kernel decision

The official Artix `linux-lts` package is the normal/default first-ISO kernel and the official Artix `linux` package remains installed as an independently bootable alternate/recovery kernel. PortusOS does not maintain a first-ISO kernel fork; Artix owns the kernel package lifecycle, while PortusOS requires both paths to remain installed, represented by valid initramfs/boot entries, and protected from accidental removal during updates. Automatic kernel rollback is not a first-ISO requirement.

### First-ISO package-source decision

Official Artix repositories are the normal/default source for selected third-party first-ISO system software. The AUR is excluded from the supported PortusOS first-ISO supply chain, including optional supported profiles. If selected software is not available from Artix, that component is evaluated individually with an explicit source, provenance, verification, licensing, compatibility, installation, update and failure-behavior decision; there is no automatic second-source fallback. Portus-owned packages and already explicit component-specific external contracts such as Codex remain separately governed.

### Service-lifecycle ownership decision

OpenRC owns machine-scoped services and Portus machine daemons; authenticated sessions own graphical/user processes. The engineering baseline preserves `sysinit -> boot -> default`, uses D-Bus/elogind as early prerequisites, prefers NetworkManager/nftables/chrony/syslog-ng after official-Artix verification, intends `portusd`/`portus-privd`/`portus-apid` for `default`, and uses OpenRC `supervise-daemon` for restart-safe Portus daemons. PAM/elogind + `dbus-run-session` own the GUI session; OpenRC user services are not a first-ISO dependency.

### First-ISO graphical-session decision

The local graphical path is designated Master TTY -> PAM/elogind -> conditional `startx` -> `dbus-run-session` -> small Portus session launcher -> minimal i3. No display manager, compositor or VMware 3D acceleration is required. i3 uses configuration-driven workspaces, `Mod4`, direct Alacritty/recovery controls and i3 IPC for deterministic placement. PipeWire/WirePlumber and graphical auth/notification helpers remain session-owned; screenshot + pointer/keyboard is fallback rather than normal orchestration.

### First-ISO update/recovery decision

First-ISO updates are explicit authorized operations rather than background/startup upgrades. Artix/pacman remains the package authority; PortusOS adds preflight, compatibility/migration checks, bounded update-specific recovery state, mandatory post-update health validation, dual-kernel/boot-artifact protection, and deterministic interrupted-update repair semantics. Updates never reboot automatically; reboot is a separate explicit action because the machine will return to the owner-controlled LUKS unlock boundary. Codex remains pinned/explicitly validated, PortusBrowser updates as a compatible component set, and failed updates enter visible degraded/recovery state rather than automatic whole-system rollback. General backup automation, remote backup integration and complete automatic rollback remain deferred.

### Sovereignty decision

Artix/OpenRC remains locked because owner-controlled free operation is more important than broad ecosystem compatibility.

### Security and identity decision

Master Portus runs as a real non-root Linux user. The Linux user account is its primary ownership/isolation boundary, not a general-purpose Codex workspace sandbox. The normal profile grants broad practical authority over that user's environment, while root/the administrator controls installation-time permission bundles and may refine authority granularly afterward. Master Portus cannot unilaterally weaken or expand administrator-enforced policy. Project/task agents may remain sandboxed as delegated workers, and root-equivalent grants must be explicitly classified.

### Protected-credential decision

Broad Master Portus authority does not imply raw reusable credential access. Protected credentials must use a real non-exporting boundary independent of ordinary Master-user filesystem authority. Secret-backed capabilities may be used without returning reusable secret values, and every claimed protected path requires negative tests. `docs/SECRETS.md` controls the complete contract.

### Recovery decision

Linux, SSH, TTY, OpenRC, and live-ISO repair remain available independently of Master Portus and the GUI.

### Persistence decision

Codex sessions, tasks, durable registries, the refreshable system index, OKF knowledge, Git, tmux, and OpenRC each have separate responsibilities. The index provides a derived machine map and agent-managed annotations; it does not replace authoritative native state, application-owned data, or durable lifecycle records. None of these layers should be overloaded to impersonate all the others.

## 17. Open questions

These questions remain unresolved and require research or implementation evidence:

- exact `artools` package/version/source, profile/configuration conventions, native Artix/Linux build bootstrap, repository/keyring integration, and clean-build procedure within the locked ISO-build architecture;
- exact Artix GRUB/mkinitcpio/cryptsetup versions, generated command/config paths, kernel command line, effective Argon2id values, Calamares storage/keyslot implementation, and live-ISO/boot repair commands within the locked boot/storage baseline;
- exact release-candidate `linux-lts`/`linux` versions, firmware/microcode selection, observed VMware native drivers, `open-vm-tools` availability/necessity, and kernel-retention/update mechanics within the locked hardware policy;
- exact package names and source classifications;
- exact OpenRC package/init-script names, native runlevel placement/dependencies and per-daemon `supervise-daemon` parameters, plus installed verification that PortusBrowser preserves its locked provider-owned/native-host-on-demand Broker lifecycle;
- exact Xorg/i3/Alacritty packages and PAM/elogind/startx/`dbus-run-session`/session-launcher files within the locked GUI baseline;
- exact selected user-session helpers, visual-fallback tools, workspace defaults and sensitive-screen controls;
- exact package/service/device-specific native group/ACL and typed privileged-action constraints for the selected Artix package set, subordinate to locked `docs/POLICY_IMPLEMENTATION.md`;
- current Codex-auth compatibility with the protected local-proxy option and implementation of any future protected browser/signing/password-manager/hardware/backup mechanisms when required, subordinate to `docs/CREDENTIAL_CLASSES.md`;

- exact durable task schema, lifecycle, retention, and recovery behavior on the locked SQLite state architecture;
- exact command-specific Portus OS CLI JSON `data` schemas, Rust crate/dependency choices, query/filter enum serialization, pagination limits/cursor mechanics, event-stream reconnect/heartbeat behavior, and sync-versus-async client implementation consistent with locked `docs/CLI.md`;
- exact capability-manifest format and versioning policy;
- exact system-index service boundary, storage, schemas, query interface, freshness, reconciliation, and application-instance grouping rules;
- exact artifact SQLite/internal-registration/checksum/cleanup implementation plus future external delivery adapters consistent with locked `docs/ARTIFACTS.md`;
- exact communication gateway and adapter contracts;
- exact approval transport for remote users;
- exact Artix/pacman/OpenRC/mkinitcpio/GRUB managed-update commands, compatibility metadata, interrupted-update repair sequence and result representation within the locked `docs/UPDATES_RECOVERY.md` architecture; general backup automation and production-grade automatic rollback remain deferred;
- exact VMware feature requirements and resource profiles;
- exact first-ISO boundary for task, artifact, communication, and application integration features;
- exact settings, status, and local presentation surfaces;
- exact release artifact naming and signing policy.

## 18. Product completeness criterion

PortusOS reaches its intended product state when this loop works reliably:

```text
user sends an authenticated local or remote request
  -> Master Portus creates or resumes durable work
  -> effective policy is applied
  -> agents operate the machine through code and structured capabilities
  -> work survives ordinary disconnection and appropriate restart scenarios
  -> sensitive actions wait for required approval
  -> results and artifacts are validated
  -> results are displayed or delivered through the selected surface
  -> significant actions, failures, approvals, and deliveries are auditable
  -> the machine remains independently recoverable if the agent fails
```

A successful ISO build is necessary, but this operating loop is the product.