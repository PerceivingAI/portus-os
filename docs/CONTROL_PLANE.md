# PortusOS Control-Plane Scope

**Status:** Authoritative architecture and product decision
**Adopted:** 2026-07-27
**Scope:** What the PortusOS layer must add, what it must leave to Codex and normal computer tools, and how policy—not tool scarcity—defines agent authority

## 1. Core decision

PortusOS is an enabling and coordination layer for an already capable computer-operating agent.

It must help the agent perform system-level work cleanly when ordinary Codex tools, standard commands, or application-owned interfaces are insufficient. It must not babysit the agent, micromanage normal computer use, or duplicate capabilities the agent already has.

> PortusOS adds missing operating-system capabilities and shared coordination. It does not replace the agent's ability to use files, commands, Git, code, or application CLIs directly.

The agent's authority is determined by effective owner policy, Codex permissions, Linux permissions, privilege boundaries, provider policy, and the hard secrets boundary. It is not determined by whether PortusOS has created a named tool for an action.

## 2. The local-agent distinction

Portus MCP is an important architectural reference, but its exact seven-tool surface must not be copied into PortusOS.

Portus MCP operates across a connector boundary to a remote computer. It therefore needs general project read, search, edit, patch, execution, context, and policy operations because the remote agent would otherwise lack those local capabilities.

Master Portus runs locally through Codex. Codex already gives the agent strong capabilities for:

- reading files;
- searching source and text;
- editing and patching files;
- running shell commands;
- writing and executing scripts;
- using Git and other installed CLIs;
- navigating projects;
- running builds and tests;
- inspecting command results;
- creating new code when no existing tool solves the problem.

PortusOS must preserve the lesson from Portus MCP—broad capability bounded by policy—without recreating the connector's file and shell tools on the local machine.

## 3. Capability-source order

Before adding a PortusOS command or service, implementation should consider capability sources in this order.

### 3.1 Codex-native capabilities

Use Codex's own file, search, edit, patch, shell, session, automation, image-input, subagent, and permission capabilities when they already solve the task correctly.

PortusOS may configure, secure, launch, or integrate Codex, but it should not reimplement those capabilities behind differently named wrappers.

### 3.2 Standard operating-system and application commands

Use normal commands directly when they are sufficient.

Examples include:

- `ls`, `find`, `fd`, `rg`, `cat`, `less`, and `file`;
- `mkdir`, `cp`, `mv`, `rm`, `ln`, `chmod`, and `chown` under effective policy;
- `git clone`, `git status`, `git diff`, `git commit`, and other Git operations;
- `ps`, `pgrep`, `ss`, `ip`, `mount`, `df`, and ordinary diagnostics;
- `rc-service` and `rc-update` for direct OpenRC administration;
- `pacman` for direct package queries and owner-authorised package operations;
- `i3-msg` and X11 commands;
- `ffmpeg`, `qpdf`, ImageMagick, `jq`, `rsync`, and other installed tools;
- an application's own stable CLI, API, IPC, or socket.

Policies may permit, deny, or require approval for these operations. A Portus wrapper is not required merely to enforce policy.

### 3.3 Specialised Portus applications

Use the domain-specific interface when PortusOS ships or integrates an application that already owns the capability.

Examples:

- `portus-browser` for browsers, tabs, pages, snapshots, browser actions, and browser-domain policy;
- the future PortusWindow CLI for PortusWindow instances and displayed content.

PortusOS integrates these applications without duplicating their internal object models or commands.

### 3.4 PortusOS-owned additions

Add a PortusOS interface only when it provides material capability that is missing from the sources above.

## 4. Non-goals and prohibited duplication

The base PortusOS layer should not add generic commands whose only purpose is to rename ordinary computer operations.

Examples that should not become PortusOS tools:

```text
portus files list <directory>
portus files read <path>
portus files search <query>
portus files write <path>
portus git clone <repository>
portus git status
portus shell run <ordinary-command>
portus directory create <path>
portus file copy <source> <destination>
```

The agent can perform those operations through Codex and normal commands. Creating another layer would:

- duplicate maintained upstream functionality;
- increase code and testing burden;
- narrow the agent to Portus-defined workflows;
- create inconsistent behaviour between native and wrapped operations;
- hide useful native output and options;
- encourage unnecessary context and tool selection;
- turn the OS into a babysitter instead of an enabling platform.

PortusOS must also avoid a generic filesystem-content index. The system index maps installed capabilities and live machine state; it does not replace directory listing, source search, project navigation, or the filesystem itself.

## 5. When a PortusOS capability is justified

A new PortusOS component, CLI command, adapter, or service is justified when at least one substantial requirement is not already met cleanly by Codex, standard commands, or an application-owned interface.

### 5.1 Cross-component coordination

The operation must coordinate several independent systems or applications.

Examples:

- launch an application, place its window on a workspace, associate it with a task, record its instance, and define cleanup behaviour;
- connect a PortusBrowser session to a system task and artifact flow;
- relate a process, graphical window, project, requester, and delivery target.

### 5.2 Durable state or lifecycle

The operation must survive a Codex turn, terminal closure, disconnect, agent restart, or reboot.

Examples:

- durable tasks;
- supervised long-running jobs;
- artifact delivery state;
- application instance lifecycle;
- session and recovery metadata.

### 5.3 Policy-mediated privileged operation

The operation crosses an authority boundary that benefits from a stable, reviewable contract.

Examples:

- temporary privilege grants;
- remote-safe firewall exposure;
- secret-backed actions that do not reveal the secret, including provider-bound API/broker operations where Master Portus uses a protected credential without receiving it;
- policy changes requiring owner authorisation;
- destructive storage operations.

The Portus interface helps apply and explain policy; the absence of an interface is not the security boundary.

### 5.4 Transaction, preflight, rollback, or expected-state checks

The operation can strand, damage, or materially alter the machine and needs more than a one-line command.

Examples:

- system updates with preflight, bounded update-specific recovery state where required, validation, and recovery;
- firewall changes with automatic rollback if remote access is lost;
- storage migration;
- boot configuration changes;
- coordinated application upgrades.

### 5.5 Shared machine awareness

The agent needs a current relationship map that ordinary commands would require repeatedly reconstructing.

Examples:

- installed capability discovery;
- running application instances;
- process-to-window-to-workspace relationships;
- provider-owned resources;
- task and ownership relationships.

This is the role of the agent-managed system index.

### 5.6 Health, degradation, and recovery

The operation needs a stable health contract, machine-readable diagnosis, bounded repair, or fallback path.

Examples:

- `portus-os doctor`;
- detecting a failed Broker, keyring, graphical session, network route, or task supervisor;
- reporting degraded capability state;
- recovering an index or registry;
- preserving evidence after failure.

### 5.7 Artifact, presentation, communication, or delivery integration

The operation needs system-wide tracking of outputs and their approved presentation or transfer.

Examples:

- register a generated artifact;
- classify and deliver it through an approved surface;
- show it through PortusWindow;
- retry failed delivery without rerunning the producing task.

### 5.8 Missing device-control interface

The computer does not otherwise expose a suitable agent-operable interface.

Examples:

- policy-controlled screenshots;
- pointer and keyboard fallback;
- deterministic application-instance mapping;
- a structured adapter for a critical application that exposes no usable CLI or API.

## 6. Inclusion test for proposed tools

Before adding a PortusOS tool, answer these questions:

1. Can Codex already perform the operation with its native tools?
2. Can the agent perform it cleanly with a standard installed command?
3. Does the relevant application already expose a suitable CLI, API, IPC, or provider interface?
4. Would a Portus wrapper add durable state, cross-component coordination, policy-mediated privilege, rollback, recovery, health, or artifact integration?
5. Would the wrapper materially improve reliability rather than merely rename the underlying command?
6. Can the same result be achieved by installing a well-maintained standard tool instead of building Portus-specific code?
7. Does the proposal preserve direct access to the underlying command and recovery path?

If the first three answers show that the operation is already covered and the fourth and fifth answers are no, PortusOS should not build the tool.

## 7. Examples

| Task | Normal path | PortusOS addition |
| --- | --- | --- |
| List files in a folder | Codex file tools, `ls`, `find` | None |
| Read or search project code | Codex read/search, `rg` | None |
| Clone and inspect a repository | `git clone`, Codex navigation | None |
| Edit or patch project files | Codex edit/patch or normal tools | None |
| Run a normal build or test | Codex shell, project scripts | None |
| Run a job that must survive disconnect and reboot | Normal command launched through durable task management | Task state, supervision, recovery, result capture |
| Query installed packages | `pacman -Q` and related commands | None for ordinary queries |
| Perform a managed OS update | pacman underneath | Preflight, backup, expected state, validation, rollback, audit |
| Check an OpenRC service once | `rc-service <name> status` | None required |
| Manage a critical service as part of an unattended workflow | OpenRC underneath | Task association, health validation, bounded restart, audit, recovery |
| Launch an application normally | Application command | None required |
| Launch, place, track, and later clean up an application instance | Application command plus i3 underneath | Instance identity, workspace placement, task relationship, lifecycle |
| List processes | `ps`, `pgrep`, `/proc` | None for direct inspection |
| Determine which windows and workspaces belong to several app instances | i3/X11 plus providers | System index and relationship model |
| Browse websites | `portus-browser` | PortusOS lifecycle, task, health, and artifact integration only |
| Take a screenshot or operate an unsupported GUI | X11/input mechanisms | Policy-controlled visual-input capability |
| Open a firewall port while preserving remote recovery | nftables underneath | Lease, preflight, rollback, ownership, audit |
| Copy a result to another folder | `cp`, `rsync` | None unless it is an artifact-delivery workflow |
| Deliver a classified result through a remote adapter | Native transfer underneath | Artifact policy, destination approval, delivery state, retry, audit |

## 8. Portus OS CLI scope

The **Portus OS CLI** (`portus-os`) is required, but its job is not to become a second shell, file API, Git client, package manager, window manager, or application automation framework.

The operating-system executable name is locked as `portus-os`. There is no generic `portus` or `portus-cli` executable/alias. Other user-facing Portus CLIs identify their component, for example `portus-browser` and, when a dedicated Master Portus interface is required, `portus-master`.

`portus-os` may provide short **machine-map convenience views** when the value comes from Portus-owned indexed/correlated state rather than from renaming an ordinary command. Examples include a concise list of currently open application instances, windows/workspaces, registered providers, or a tree-like representation of known machine/resource relationships.

This does not authorize a generic filesystem wrapper. A command whose only job is "recursively list arbitrary directory X" would duplicate Codex/native tools and remains out of scope. A Portus machine-map tree is justified only when it exposes cached/indexed relationships, annotations, stable resource identity, or other Portus-owned context that a raw filesystem command does not provide.

Its likely domains include:

- system-index query, refresh, reconciliation, and health;
- capability and application registration, lifecycle, and health;
- durable task creation, association, supervision, resume, and cancellation;
- policy inspection, approval, and owner-authorised changes;
- artifact registration, classification, presentation, delivery, and cleanup;
- health, diagnostics, degradation, and recovery;
- managed network, update, storage, session, display, or service workflows where coordination and rollback justify an adapter;
- Master Portus and project-session integration.

The `portus-os` executable name is locked. Exact subcommand names and detailed schemas remain open. The domain boundary is locked.

## 9. Ordinary execution versus managed execution

`docs/TASKS.md` is authoritative for execution routing and Portus task promotion.

Codex/native/provider execution remains the default. Master Portus may work directly, delegate a bounded Codex subagent, launch an independent headless Codex root session, use a visible interactive session when visibility is useful, or invoke a native/provider operation without automatically creating a Portus task.

PortusOS needs a task only when the work requires durable Portus-owned semantics such as explicit tracking, cross-component lifecycle/cleanup, supervision/cancellation/reconciliation, approval waiting, significant progress/events, artifact/result lifecycle, requester relationships, or preserved health/recovery evidence.

A separate Codex session, a subagent, a long command sequence, or a complex task does not by itself require Portus task registration. PortusOS must not duplicate Codex's native session, subagent, queue, fork, resume, goal, or compaction machinery merely to call the result managed execution.
## 10. Inspection, search, and mutation boundaries

PortusOS may expose query and mutation operations over Portus-owned domains, but these must not be confused with general file tools.

Appropriate Portus query domains include:

- the system index;
- tasks;
- artifacts;
- registered capabilities;
- policy and approvals;
- health and degradation;
- Portus-owned events and audit metadata;
- application integration state.

Appropriate Portus mutation domains include:

- Portus annotations and relationships;
- task lifecycle;
- artifact metadata and delivery state;
- approved policy changes;
- managed configuration transactions;
- adapter-owned coordinated workflows.

General project files, source code, directory contents, Git state, and ordinary command output remain accessible through Codex and native tools.

## 11. Policy is the perimeter

PortusOS must not attempt to achieve safety by withholding normal tools or by exposing only a small catalogue of blessed workflows.

The effective perimeter is produced by:

- root/administrator-owned policy;
- the Master Portus Linux UID/GID, filesystem ownership/modes/ACLs, sockets and device permissions;
- sudo, polkit, root-owned helpers/services and other explicit privilege boundaries;
- PortusOS permission bundles and granular policy;
- network and firewall policy;
- application-owned policy such as PortusBrowser's browser policy;
- the protected credential boundary in `docs/SECRETS.md`;
- Codex permission profiles/Bubblewrap for delegated project, task and inspection agents;
- optional approvals and temporary grants;
- logging, review, and recovery.

Master Portus is not placed inside a general-purpose Codex workspace sandbox. Its broad base boundary is the non-root Linux user account. The normal broad profile may allow almost every useful non-root capability of that user's computer environment plus selected privileged operations, while root-equivalent grants remain explicitly classified and administrator-controlled.

## 12. Master Portus instructions and skills

The Master Portus system instruction should state concise operating principles:

- use Codex and normal computer tools directly for ordinary work;
- prefer structured application interfaces where available;
- use PortusOS capabilities when they add coordination, durable state, policy-safe privileged workflows, health, recovery, indexing, artifacts, or delivery;
- do not assume a Portus wrapper is required;
- do not treat absence from the Portus index as absence from the computer;
- operate broadly within the owning Linux user's effective authority without assuming a workspace sandbox;
- use sandboxed/project-scoped delegation when assigning narrower project/task agents;
- remain within administrator-enforced policy and the protected credential boundary.

Detailed command usage belongs in machine-wide skills rather than being injected wholesale into the main prompt. PortusOS should ship a dedicated `portus-os-cli` skill scoped to the `portus-os` executable. It may contain CLI-specific examples and common CLI usage patterns, but unrelated reusable workflows must live in separate skills rather than being appended indefinitely to the CLI skill.

The Master `AGENTS.md` also routes Master Portus to the live system index and the durable per-machine knowledge map. It should instruct Master to reuse previously learned stable machine facts and update that knowledge deliberately, while leaving live/transient truth to the index and authoritative providers.

## 13. Implementation and review rule

Every proposed PortusOS CLI command, daemon endpoint, adapter method, or top-level agent tool must include a written justification explaining:

- which missing capability it provides;
- why Codex or a standard command is insufficient;
- what state or coordination PortusOS owns;
- what underlying native interface remains available;
- what policy and recovery boundaries apply;
- how the component fails without disabling ordinary computer use.

Tools that cannot pass this review should not be added.

## 14. First supported release requirements

The first supported PortusOS release must demonstrate that:

- Master Portus can use Codex file and shell capabilities directly;
- no Portus tool is required to list, read, search, edit, or navigate ordinary files;
- Git workflows use Git rather than a Portus reimplementation;
- ordinary commands can run without a Portus wrapper;
- Portus-managed execution is reserved for durable or coordinated work;
- Portus-specific state can be queried without exposing unrelated filesystem content;
- specialised applications remain directly operable through their own CLIs;
- policy applies consistently whether the agent uses a Portus interface or a native command;
- disabling a Portus convenience or index service does not remove normal Linux and Codex operation;
- required missing capabilities—indexing, lifecycle, health, recovery, artifacts, visual fallback, and application integration—remain available.

## 15. Defining statement

> PortusOS does not teach a capable agent how to use a computer one command at a time. It supplies the system-level awareness, coordination, durability, policy integration, and recovery that the agent and ordinary commands do not already provide.
