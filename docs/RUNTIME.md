# PortusOS Runtime Architecture

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative runtime architecture
**Scope:** PortusOS runtime process model, local IPC, Portus OS CLI relationship, provider boundaries, managed execution, events, lifecycle, degradation, and recovery

## 1. Purpose and authority

This document is authoritative for the PortusOS runtime architecture.

It defines the runtime boundary between:

- the Portus OS CLI;
- the local PortusOS daemon;
- Portus-owned durable state;
- ordinary Codex and Linux execution;
- OpenRC supervision;
- the system index;
- managed tasks;
- capability providers such as PortusBrowser;
- events, health, degradation, and recovery.

`docs/STATE.md` is authoritative for PortusOS-owned mutable state and SQLite. `docs/CONTROL_PLANE.md` is authoritative for what functionality belongs in the PortusOS layer at all. `docs/CLI.md` is authoritative for the detailed `portus-os` command/output/error/pagination/precondition contract. `docs/POLICY.md` is authoritative for identity, privilege, permission bundles and policy enforcement. `docs/SECRETS.md` is authoritative for protected reusable credentials and secret-backed operations.

## 2. Locked runtime model

PortusOS uses one small modular local daemon named `portusd` for Portus-specific responsibilities that require persistent coordination, shared mutable state, events, or runtime ownership.

The normal structured path is:

```text
Master Portus / human
        |
        v
  Portus OS CLI
    (`portus-os`)
        |
        v
/run/portus/portusd.sock
        |
        v
      portusd
        |
        +-- SQLite Portus state
        +-- system-index coordination
        +-- durable registries
        +-- managed-task coordination
        +-- events
        +-- health/degradation state
        +-- capability/provider relationships
```

This daemon is not a replacement shell, filesystem API, Git service, package manager, window manager, browser engine, or universal mediator for computer activity.

## 3. Portus CLI naming convention

The operating-system CLI is canonically named **Portus OS CLI** and its executable is:

```text
portus-os
```

There is no generic `portus` executable and no `portus-cli` executable or compatibility alias.

User-facing Portus CLIs must identify the component they control in the executable name. The naming pattern is:

```text
portus-<component>
```

Examples:

```text
portus-os       = PortusOS operating-system control plane
portus-browser  = PortusBrowser
portus-master   = Master Portus-specific CLI/launcher, if such a dedicated interface is required
portus-window   = PortusWindow, when implemented
```

`portusd` is the daemon name, not a user-facing CLI, and therefore does not follow the CLI hyphen form.

OS-level operations exposed through the Portus OS CLI are subcommands of `portus-os`, for example:

```text
portus-os status
portus-os doctor
portus-os index ...
portus-os task ...
portus-os capability ...
portus-os policy ...
```

Detailed command names, structured output/error behavior, pagination, preconditions and CLI compatibility are locked by `docs/CLI.md`; the executable naming/Rust/thin-client rules remain locked here and apply project-wide.

## 4. Why `portusd` exists

A persistent PortusOS process is justified only for responsibilities that ordinary Codex tools, Linux commands, or provider-owned interfaces do not already solve cleanly.

`portusd` owns or coordinates the runtime portions of:

- Portus-owned durable registries;
- system-index refresh, reconciliation, relationships, and cached observations;
- managed task identity and lifecycle metadata;
- supervision coordination for Portus-managed work;
- significant runtime events and live event subscriptions;
- capability/provider registration relationships and health state;
- PortusOS health and degradation state;
- approval and policy workflow coordination where later security contracts require shared runtime state;
- task, process, application, provider, and artifact relationships.

Exact task, capability, policy, artifact, and index schemas belong to their dedicated documents.

## 5. What bypasses `portusd`

PortusOS must preserve direct use of normal computer capabilities.

Ordinary operations do not route through `portusd` merely because the daemon exists.

Examples:

```text
files and source code      -> Codex / normal Linux tools
Git                        -> git
ordinary shell commands    -> Codex shell / shell
ordinary builds and tests  -> project commands and scripts
one-off process inspection -> ps, pgrep, /proc
package inspection         -> pacman
OpenRC inspection          -> rc-service, rc-status, rc-update
i3 control                 -> i3-msg and X11 tools
browser-domain work        -> portus-browser
```

A PortusOS interface is justified only when it adds the coordination, durability, policy mediation, expected-state handling, health, recovery, indexing, or artifact relationships defined by `docs/CONTROL_PLANE.md`.

## 6. Portus OS CLI relationship

The Portus OS CLI is the primary human/agent command surface for PortusOS-specific runtime capabilities.

Commands that operate on Portus-owned runtime state normally communicate with `portusd` through the local Unix socket.

The CLI must not write the SQLite database directly during normal operation.

The CLI may contain daemon-independent operations when that independence is necessary for bootstrap, diagnosis, or recovery. In particular, `portus-os doctor` must be able to identify and explain an unavailable or broken `portusd` rather than depending on a healthy daemon to diagnose the daemon itself.

### 6.1 CLI implementation language and code boundary

The `portus-os` executable is implemented in **Rust**.

This is a hard implementation decision, not a package suggestion. `/usr/bin/portus-os` must be a native compiled executable and must not require a Python interpreter, Node.js runtime, virtual environment, or application package tree merely to operate the PortusOS control surface or its recovery diagnostics.

The normal CLI architecture is intentionally thin:

```text
CLI arguments
  -> typed Rust client request
  -> versioned JSONL over /run/portus/portusd.sock
  -> portusd
  -> typed response/error
  -> human or structured CLI rendering
```

The Rust CLI owns argument parsing, wire/client validation, bounded presentation, exit-code mapping, and daemon-independent diagnostics explicitly assigned to the CLI. It does **not** own a second implementation of task transitions, System Index correlation, provider lifecycle, policy decisions, or normal SQLite mutation merely because those types are visible to the CLI.

Rust-side wire/client types should live in shared reusable crates rather than being redefined independently inside each `portus-os` subcommand. Exact crate names remain implementation detail, but the structural direction is equivalent to separating:

- protocol/wire types and stable IDs;
- Unix-socket client/framing behavior;
- the `portus-os` command/presentation layer.

The on-wire protocol remains the language-neutral JSONL contract defined by this document. This Rust decision therefore does not require every process that speaks the protocol to be implemented in Rust.

Whether ordinary request/response commands use synchronous I/O or an async runtime is an implementation choice to be justified by actual event-streaming/concurrency needs. The CLI must not add a large async/runtime dependency solely by convention.

`portus-os doctor` is the deliberate exception in which the CLI itself may perform bounded daemon-independent native checks to diagnose bootstrap/runtime failure, including the socket, relevant OpenRC/service state, required paths, and approved database/integrity checks. Other explicit security-boundary paths such as root-only `policy admin` remain thin clients of their separately defined service (`portus-privd`) rather than moving privileged business logic into the CLI. Doctor's local independence must remain narrowly diagnostic and must not become an alternate control-plane implementation.

The machine-wide **`portus-os-cli` skill is not the executable** and is not implemented in Rust. It is Codex instruction/reference content that teaches agents how to use `portus-os`; unrelated reusable workflows remain independent skills.

Native Linux commands remain available independently of the Portus OS CLI.

## 7. IPC transport

### 7.1 Local-only Unix domain socket

The runtime IPC transport is a Unix domain socket at the canonical runtime path:

```text
/run/portus/portusd.sock
```

The first implementation must not expose `portusd` through:

- TCP;
- HTTP;
- WebSocket;
- gRPC;
- D-Bus;
- a network listener.

Remote users reach PortusOS through approved remote surfaces such as SSH. They do not connect directly to `portusd` over the network.

Socket ownership and permissions must enforce `docs/POLICY.md`. Every accepted local connection must derive the caller's authenticated OS principal from Unix peer credentials rather than trusting a caller-supplied username. `portusd` must map at least the peer UID/GID to the effective Portus principal and enforce principal ownership on user-scoped tasks, artifacts, annotations, provider relationships and other private Portus state. Cross-user access requires explicit administrator authority.

### 7.2 Message format

The first runtime protocol uses newline-delimited JSON messages over the Unix stream socket.

A request envelope contains at least:

```json
{
  "version": 1,
  "request_id": "<id>",
  "method": "index.query",
  "params": {}
}
```

A successful response contains at least:

```json
{
  "version": 1,
  "request_id": "<id>",
  "ok": true,
  "result": {}
}
```

An error response contains a stable machine-readable error code:

```json
{
  "version": 1,
  "request_id": "<id>",
  "ok": false,
  "error": {
    "code": "provider_unavailable",
    "message": "..."
  }
}
```

The protocol must not rely on parsing human prose to determine success or failure.

The user/agent command mappings, structured error taxonomy, exit-code families, pagination, preconditions, event-stream presentation, and CLI compatibility behavior are locked by `docs/CLI.md`. Command-specific wire/data schemas remain shared implementation contracts with the owning subsystems.

## 8. Events

PortusOS requires events for unattended operation, but it does not require an external message bus.

The first runtime model is:

- significant durable events are recorded in Portus-owned state;
- live local subscribers may receive event messages through a long-lived `portusd` Unix-socket connection;
- event streams are bounded and clients must tolerate disconnect/reconnect;
- event payloads must be structured and secret-safe.

Representative event families include:

```text
task.started
task.progress
task.completed
task.failed
approval.required
approval.received
artifact.created
capability.degraded
capability.recovered
system.degraded
system.recovered
```

PortusOS does not add Redis, NATS, RabbitMQ, Kafka, or another broker unless a later demonstrated requirement justifies one.

P8 concretizes this architecture: durable significant events are stored per object in schema-v6 `significant_events`, while the in-memory `EventHub` is only a bounded wake-up mechanism. Stream consumers reread durable sequence data before delivery, so a missed wake-up is not equivalent to a lost event. Resume requests older than retained history fail explicitly rather than silently skipping a gap. The first live stream is `task.events.follow` over the existing local Unix connection with versioned event/end/error frames. Security audit records are deliberately separate from runtime state and are governed by `docs/EVENTS_AUDIT.md`.

## 9. OpenRC lifecycle

`portusd` is an OpenRC-managed system service.

OpenRC owns daemon startup, shutdown, service status, and restart integration. The exact runlevel, dependency ordering, service account, and bounded restart mechanism will be locked in `docs/OPENRC_SERVICES.md` and `docs/POLICY.md` after Artix/OpenRC validation.

Runtime invariants are already locked:

- `portusd` must not depend on a graphical login or tmux;
- it must be able to start before an interactive Master Portus session;
- it must create or validate its runtime socket safely;
- it must validate/migrate required Portus state before reporting ready;
- a daemon crash must not terminate unrelated native Linux processes or independent provider services;
- repeated failure must become an observable degraded condition rather than an infinite hidden crash loop.

## 10. Managed execution boundary

`docs/TASKS.md` is authoritative for execution routing and Portus task promotion.

Ordinary work may remain direct Master execution, a Codex subagent, an independent Codex root session, a visible interactive Codex session, a native process, or a provider operation without automatically becoming a Portus task.

Portus-managed task execution is used only when work needs durable Portus-specific lifecycle/relationship behavior such as:

- explicit durable task identity/status requested by the user;
- cross-component lifecycle or cleanup;
- Portus-owned supervision, cancellation, or reconciliation;
- policy/approval waiting;
- structured significant progress/events;
- task/process/application/provider relationships;
- artifact/result lifecycle;
- preserved failure evidence or health/recovery responsibility.

Conceptually:

```text
Master / user
  -> choose native/Codex/provider execution surface
  -> promote/register Portus task only if Portus lifecycle is needed
  -> portusd records task + safe execution references
  -> backend remains authoritative for its own internal state
```

PortusOS must reuse Codex-native subagent, session, resume, fork, queue, goal, and compaction facilities rather than duplicating them in `portusd`. A visible terminal is not required merely because work is complex or independent.

The first ISO does not require arbitrary processes to continue through a reboot. Full reboot-resumable task execution remains deferred. The initial runtime may preserve durable task metadata and terminal outcomes across daemon/service restart without claiming that every running process survives reboot.
## 11. Provider boundary

`docs/CAPABILITIES.md` is authoritative for registered provider identity, manifests, direct-use interfaces, lifecycle/health/policy ownership, opaque resource references, operation affordances, compatibility and uninstall behavior.

Specialised Portus applications remain separate capability providers. Provider registration does not make `portusd` a mandatory invocation proxy: Master Portus or delegated Codex workers may use a provider's native structured CLI/API/IPC directly while `portusd` coordinates only the Portus-owned registration, health, task, artifact, policy and resource relationships that require shared system state.

For PortusBrowser:

```text
portusd
   |
   | lifecycle/health/task/artifact/provider relationships
   v
PortusBrowser Broker
   |
   v
native-messaging host / extension / browser
```

`portusd` may know and coordinate:

- provider identity and version;
- installation/registration state;
- service and process relationships;
- health/degradation state;
- task associations;
- artifact relationships;
- provider resource references exposed through the system index.

It must not absorb or duplicate PortusBrowser-owned state or behavior such as:

- tab/page state;
- DOM snapshots;
- browser navigation implementation;
- page actions;
- browser-domain policy;
- browser credentials or authenticated-session internals.

The same principle applies to PortusWindow and future providers.

## 12. Cancellation and timeouts

Every daemon request must have bounded resource behavior.

The runtime contract must support:

- request cancellation where the operation is cancellable;
- operation-specific timeouts rather than one unbounded global wait;
- clear distinction between request timeout, provider timeout, task failure, and daemon failure;
- preservation of durable task state when a client disconnects;
- no assumption that closing the CLI terminates a managed task.

Exact timeout values and cancellation methods are implementation-level decisions to be validated with each subsystem.

## 13. Startup and readiness

`portusd` readiness requires more than process existence.

Before reporting ready it must at minimum:

1. load its administrator-controlled configuration;
2. open and validate the Portus SQLite database;
3. apply any required transactional schema migration;
4. establish its Unix socket with correct permissions;
5. initialize required in-memory state;
6. expose health that distinguishes ready, degraded, and unavailable conditions.

Provider outages do not necessarily prevent daemon readiness. Optional or independently recoverable provider failures should normally produce degraded capability state rather than make the complete OS runtime unavailable.

## 14. Degraded and failure behavior

Failure of `portusd` degrades PortusOS coordination; it does not make the Linux computer unusable.

When `portusd` is unavailable:

- Linux remains booted and administrable;
- SSH remains independently usable;
- Codex can still use ordinary permitted files, Git, code, and shell commands;
- OpenRC remains directly operable;
- native diagnostics remain usable;
- independent providers such as PortusBrowser are not forcibly terminated merely because `portusd` failed;
- `portus-os doctor` must surface the daemon failure and available recovery path;
- durable SQLite state remains on disk for recovery;
- rebuildable index observations may be reconstructed after restart.

No Portus abstraction may become the only path to repair the underlying system.

## 15. Security boundary dependency

The account and authority architecture is now locked by `docs/POLICY.md` and `docs/SECRETS.md`.

Runtime invariants are:

- Master Portus is a real non-root Linux user and is not represented merely by Portus metadata;
- the Unix socket is local and permission-controlled;
- every local connection is authenticated from kernel-provided Unix peer credentials;
- `portusd` enforces principal ownership for user-scoped Portus state;
- cross-user private state is denied by default;
- `portusd` does not expose a network listener;
- the daemon must not run permanently as unrestricted root merely for convenience;
- privileged operations use the explicit administrator-controlled sudo/polkit/helper/service design selected under `docs/POLICY.md`;
- root-equivalent capabilities must remain explicitly classified;
- raw reusable secrets do not belong in runtime messages, logs, task state, or the SQLite database;
- protected credential operations follow `docs/SECRETS.md` rather than returning reusable values through generic runtime IPC;
- `portusd` may carry non-secret credential/capability references, provider identity, availability and policy associations but must not store, receive or forward reusable protected credential values;
- protected API/provider use is performed by a separate protected provider/broker boundary that attaches credentials internally to approved upstream operations;
- a provider-compatible local proxy may exist outside generic `portusd` IPC for SDK/application compatibility, but it must authenticate/authorize local callers and must not expose a reusable upstream credential;
- generic runtime APIs must not provide an arbitrary caller-selected destination/header/command facility capable of turning a protected credential reference into credential exfiltration;
- request identity and effective policy are available to every operation requiring authorization.

## 16. Explicit non-goals

The first runtime architecture does not include:

- a distributed control plane;
- a remote `portusd` API;
- an external message broker;
- a generic filesystem service;
- a generic shell-execution RPC layer;
- a duplicate Git interface;
- a duplicate browser object model;
- arbitrary reboot-transparent process checkpointing;
- a mandatory GUI dependency;
- a requirement that every OS operation pass through PortusOS.

## 17. Decisions locked by this document

The following are now locked:

1. One modular general Portus coordination/state daemon: `portusd`. Separate narrowly scoped security/provider services such as root-owned `portus-privd` or provider-owned `portus-apid` remain independent boundaries and do not become additional general control-plane daemons.
2. OpenRC supervises `portusd`.
3. The operating-system CLI is named **Portus OS CLI** and executes as `portus-os`.
4. `portus-os` is implemented in Rust as a native compiled executable without a Python/Node runtime dependency.
5. The normal Portus state/coordination `portus-os` path is a thin typed client over the versioned `portusd` JSONL Unix-socket contract; explicit security-boundary operations such as root-only policy administration may use their separately defined local IPC boundary without duplicating its business logic in the CLI.
6. Rust wire/client types are shared across the CLI implementation boundary instead of being redefined per subcommand; exact crate names/dependencies remain implementation detail and the on-wire protocol remains language-neutral.
7. `portus-os doctor` retains a bounded daemon-independent diagnostic path and must not become a second runtime/control-plane implementation.
8. The `portus-os-cli` machine-wide Codex skill is instruction/reference content, not the Rust executable or a general workflow library.
9. No generic `portus` or `portus-cli` executable/alias is part of the architecture.
10. User-facing Portus CLI executable names identify their component using `portus-<component>`.
11. Runtime IPC is local-only through `/run/portus/portusd.sock`.
12. The first IPC format is newline-delimited JSON with versioned request/response envelopes and typed errors.
13. No HTTP/TCP/D-Bus/gRPC runtime API is required.
14. Significant events may be persisted locally and live subscribers use the Unix-socket runtime; no external event broker is required.
15. Ordinary Codex/Linux operations bypass `portusd`.
16. Portus-managed execution is reserved for work that needs Portus-specific durability or coordination.
17. PortusBrowser and other providers remain independent services/processes with integration relationships rather than absorbed implementations.
18. `portusd` failure degrades PortusOS but must not disable native Linux, SSH, Codex ordinary operation, OpenRC, or independent recovery.
19. Full reboot-resumable arbitrary process execution is not required for the first ISO.
20. Local IPC caller identity is derived from Unix peer credentials and user-scoped Portus state is principal-isolated by default.

## 18. Remaining dependent decisions

The architecture above does not block these later specialised decisions:

- exact `portusd` service identity/socket group/mode consistent with `docs/POLICY.md`; the first root privilege-helper identity/interface is already locked separately by `docs/POLICY_IMPLEMENTATION.md`;
- exact Artix OpenRC init-script/dependency identities and per-daemon restart parameters within the locked `sysinit -> boot -> default` service strategy (`docs/OPENRC_SERVICES.md`);
- exact command-specific CLI JSON `data` schemas, Rust crate names/dependency set, parser implementation details, pagination limits/cursor mechanics, and justified synchronous-versus-async client implementation consistent with `docs/CLI.md`;
- exact task SQLite tables/indexes, retention values, and execution-correlation implementation consistent with `docs/TASKS.md`;
- exact system-index resource schemas and refresh policy (`docs/SYSTEM_INDEX.md`);
- capability/provider manifest schema (`docs/CAPABILITIES.md`);
- exact artifact SQLite/registration/cleanup implementation consistent with locked `docs/ARTIFACTS.md`;
- exact health reason/probe/OpenRC/diagnostic implementation consistent with locked `docs/HEALTH_RECOVERY.md`.

Those documents must conform to this runtime boundary rather than reopening the one-daemon/local-socket/non-duplicative model without a new explicit owner decision.
