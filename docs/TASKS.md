# PortusOS Tasks and Managed Execution

**Status:** Authoritative architecture and product decision
**Adopted:** 2026-08-21
**Scope:** How Master Portus routes work, when work becomes a PortusOS task, how task identity/lifecycle relates to Codex sessions and subagents, and how PortusOS supervises, cancels, reconciles, and records durable work without duplicating the Codex harness

## 1. Authority and core decision

This document is authoritative for PortusOS task identity, task lifecycle, managed-execution boundaries, execution routing, task/process/provider/Codex relationships, progress, cancellation, retry, approval waiting, result handling, cleanup, and service-restart reconciliation.

Related authority remains separate:

- `CODEX_UPDATES.md` controls Codex-native sessions, root threads, subagents, goals, queue/fork/resume behavior, compaction, skills, model configuration, and Codex execution surfaces;
- `docs/RUNTIME.md` controls `portusd`, local IPC, request cancellation/timeouts, provider separation, and runtime degradation;
- `docs/STATE.md` controls SQLite ownership, state classes, storage boundaries, migrations, and corruption handling;
- `docs/POLICY.md` controls Linux identity, effective authority, approvals, privilege boundaries, delegated-agent policy, and principal isolation;
- `docs/SECRETS.md` controls protected reusable credentials and secret-backed operations;
- `docs/SYSTEM_INDEX.md` controls the live machine map and task-to-resource index relationships;
- `docs/CAPABILITIES.md` controls provider manifests, provider identity/lifecycle, provider resource references, direct-use interfaces, and provider-side operation affordances;
- `docs/ARTIFACTS.md` controls artifact identity, locator/integrity, retention, cleanup, local presentation, and deferred delivery semantics.

The core decision is:

> PortusOS tasks are an outer durable OS-level lifecycle and relationship layer. They do not replace Codex sessions, Codex subagents, native processes, OpenRC, provider operations, skills, or normal shell execution.

Two questions must always be answered separately:

1. **Which execution surface should perform the work?**
2. **Does the work require a Portus task identity?**

A complex job may run in a headless Codex root session without becoming a Portus task. A Portus task may coordinate work performed by Codex, a native process, a provider, or several of those together.

---

## 2. A Portus task is not the default unit of work

Most computer operations must not become Portus task records.

Ordinary execution remains available directly through Codex and native tools:

```text
Master Portus
  -> Codex/native capability
  -> shell / application CLI / provider / system command
```

Examples that normally do **not** require a Portus task include:

- open Gmail in an already controlled browser;
- inspect a file or directory;
- run `git status`;
- edit source code during the current interactive turn;
- inspect an OpenRC service once;
- launch a normal application when no persistent coordination is needed;
- ask a bounded Codex subagent to inspect logs and return a result to the current Master turn;
- run a Codex-only headless investigation whose independent Codex session is sufficient and for which no Portus-owned lifecycle is required.

A task record must exist because the work needs durable PortusOS semantics, not merely because the work contains several commands or may take a while.

The following are **not sufficient by themselves** to require a Portus task:

- complexity;
- number of shell commands;
- use of a subagent;
- use of a separate Codex root session;
- context compaction;
- use of tmux;
- use of a visible terminal;
- project work;
- use of `portus-os` commands;
- use of multiple applications in one short, bounded transaction.

---

## 3. Master Portus is the routing authority

The Master Portus `AGENTS.md` is the concise operating charter that establishes Master as the manager and orchestrator of the device.

It must direct Master Portus to:

- understand user intent before selecting an execution surface;
- preserve its own long-lived context primarily for intent, orchestration, decisions, task/status relationships, and result synthesis;
- perform small, immediate, tightly coupled work directly;
- delegate bounded sidecar work when doing so protects Master context or allows useful parallelism;
- use an independent headless Codex root session for work that should have its own history/lifecycle or continue independently while Master returns to the user;
- open a visible Codex/tmux session only when human visibility, interaction, inspection, or recovery is useful;
- create a Portus task only when Portus-owned lifecycle semantics are actually required;
- reuse Codex-native session, fork, resume, queue, goal, compaction, skill, and subagent facilities rather than building equivalents in PortusOS;
- use the `portus-os-cli` skill for detailed Portus OS CLI usage;
- keep independent reusable workflows in independent skills rather than appending them to the CLI skill;
- consult the live system index and durable per-machine knowledge map rather than repeatedly rediscovering known machine facts.

The detailed contents of the installed Master `AGENTS.md` remain an implementation artifact, but the routing responsibilities above are architectural requirements.

Codex compaction is expected and desirable for the long-lived Master thread. Portus task truth must therefore remain outside conversational context. After compaction, Master should be able to recover authoritative task status from PortusOS rather than depending on remembered prose.

---

## 4. Execution surfaces

PortusOS recognizes several distinct execution surfaces. They are not interchangeable.

### 4.1 Direct Master execution

Use Master Portus directly when the action is:

- immediate;
- small or tightly coupled to the current request;
- on the current critical path;
- unlikely to benefit from independent history or parallelism;
- better completed than delegated.

Example:

```text
User: "Open Gmail."
  -> Master Portus uses PortusBrowser directly
  -> no subagent
  -> no new Codex root session
  -> no Portus task
```

Direct execution minimizes latency and avoids unnecessary scheduling overhead.

### 4.2 Codex subagent

Use a Codex subagent for concrete, bounded sidecar work that belongs to a parent Codex session and whose result is primarily consumed by that parent.

Good subagent shapes include:

- map a codebase while Master continues other reasoning;
- inspect logs;
- research one bounded question;
- perform an independent review category;
- verify a result;
- execute a bounded setup procedure when the subagent lifecycle is sufficient.

Subagents remain Codex-owned. Their internal messages, tool traces, context, lifecycle details, and result transport are not copied into Portus state.

A Codex subagent does **not** automatically become a child Portus task.

Create a Portus child task only if that delegated work independently crosses the Portus task-promotion boundary in Section 6.

### 4.3 Independent headless Codex root session

Use an independent headless Codex root session when work should:

- have its own persisted Codex history;
- continue independently of the current Master turn;
- be resumed or forked independently;
- avoid filling the Master transcript with detailed execution;
- allow Master to return to the user after the handoff;
- remain Codex-native rather than requiring a visible terminal.

The preferred Codex-native surfaces include the validated `codex exec`, `codex exec resume`, `codex exec fork`, `codex queue`, and session inspection facilities documented in `CODEX_UPDATES.md`.

A headless root session may remain **Codex-only** when its Codex session identity/history are sufficient.

It becomes associated with a Portus task only when Section 6 requires OS-level durable lifecycle or the user explicitly asks PortusOS to track the work as a task.

### 4.4 Visible interactive Codex session

Use an interactive Codex session in tmux/terminal when visibility itself is useful, for example:

- the user wants to work interactively with the project agent;
- the user needs to observe a debugging session;
- a terminal UI or authentication flow requires human interaction;
- manual recovery is needed;
- the work benefits from a persistent visible workspace.

A visible terminal is a presentation choice, not a durability requirement.

Complexity alone must not cause PortusOS to open a new terminal.

### 4.5 Native process or application operation

Some tasks are best performed by a normal process, script, standard application CLI, OpenRC operation, or dedicated application interface without launching another Codex session.

PortusOS may associate or manage that execution when Section 6 requires task semantics, but it must not wrap every normal command merely because a task system exists.

### 4.6 Provider operation

A provider such as PortusBrowser, a protected credential/API provider, future PortusWindow, or another capability provider may perform task-related work.

The provider remains authoritative for its domain state. PortusOS stores only the provider reference and Portus-owned task relationship necessary for lifecycle, status, policy, artifacts, health, or recovery.

Provider-specific operation/cancellation/reconciliation semantics belong to `docs/CAPABILITIES.md` and the provider contract.

### 4.7 Composite execution

A Portus task may coordinate several execution surfaces at once.

For example:

```text
Portus task
  -> headless Codex root session
  -> native build process
  -> PortusBrowser resource
  -> generated artifact
```

The task represents the durable objective and cross-component lifecycle. It does not make those child systems cease to own their own state.

---

## 5. Routing is based on work shape, not magic thresholds

PortusOS must not encode brittle rules such as:

```text
more than N commands -> subagent
more than X seconds -> task
more than Y files -> visible terminal
```

Execution routing should instead consider:

- whether the result is needed immediately on the current critical path;
- whether independent work can proceed while Master handles other requests;
- whether detailed execution would unnecessarily pollute Master context;
- whether the work needs independent Codex history/resume/fork semantics;
- whether the user wants a visible interactive surface;
- whether the work needs durable status/cancellation/recovery outside Codex;
- whether policy or resource isolation requires a delegated role;
- whether the work is a reusable skill/workflow invocation;
- whether the work already has a suitable native/provider execution surface.

The least heavy surface that preserves correctness, responsiveness, policy, and recoverability should be preferred.

---

## 6. When work becomes a Portus task

### 6.1 Task-promotion rule

Work becomes a Portus task when at least one material requirement exists that Codex/native/provider state alone does not satisfy cleanly.

A Portus task is justified when the work needs one or more of the following:

1. **Explicit durable user tracking**
   - the user explicitly asks to track, manage, cancel, resume, or inspect the work as a task;
   - the user expects a durable completion/failure status independent of the originating Codex turn.

2. **Cross-component lifecycle**
   - several processes, applications, providers, projects, windows, or resources must remain related over time;
   - later cleanup or ownership depends on remembering why those resources exist.

3. **Portus-owned supervision**
   - a process/operation needs durable start/stop/cancel/reconcile behavior beyond the current shell invocation;
   - the task must survive loss of the client connection or Master turn.

4. **Policy or approval waiting**
   - execution may pause for an approval, privilege boundary, user decision, or policy-mediated condition without losing its durable identity.

5. **Artifact/result lifecycle**
   - outputs need durable association, classification, later presentation/delivery, or cleanup.

6. **Health/recovery responsibility**
   - PortusOS must detect interruption, preserve failure evidence, reconcile after `portusd` restart, or report degradation.

7. **External/requester relationship**
   - future communication/delivery surfaces need durable association between requester, task, result, and delivery state.

8. **Persistent cleanup responsibility**
   - temporary resources or launched application instances must later be cleaned up according to a task policy even if the originating Codex session is gone.

9. **Future scheduling/conditional execution**
   - when later implemented, scheduled or condition-triggered work requires an independent Portus lifecycle outside one Codex turn.

### 6.2 What does not force promotion

The following do not force promotion by themselves:

- a separate headless Codex session;
- a subagent;
- a long prompt;
- many tool calls;
- a difficult reasoning problem;
- compaction;
- multiple files;
- running a test suite;
- a multi-step but short deterministic setup;
- invoking a saved skill.

### 6.3 Promotion during execution

Work may begin without a Portus task and later require one.

Example:

```text
Codex headless session begins investigation
  -> discovers a long-running migration
  -> migration needs approval + artifact + recovery tracking
  -> existing Codex root session is associated with a newly created Portus task
  -> work continues; Codex history is not duplicated or restarted
```

Promotion must adopt/reference the existing execution identity when safe. PortusOS must not fork, restart, or duplicate work merely to obtain a task ID.

### 6.4 A task stays a task

Once durable Portus task identity exists, the task remains a historical Portus record through its terminal state and retention period.

PortusOS does not silently "demote" it back into an untracked Codex session because doing so would erase lifecycle history.

---

## 7. Example routing decisions

| User/work shape | Execution surface | Portus task? |
| --- | --- | --- |
| "Open Gmail." | Master Portus directly through PortusBrowser | No |
| "Check these logs while I inspect the app." | Bounded Codex subagent | Usually no |
| "Investigate this repository independently; I'll ask you about it later." | Headless Codex root session | Not necessarily; Codex session may be enough |
| "Run this migration in the background and let me track/cancel it." | Headless/project Codex or native execution | Yes |
| "Implement this feature and register the resulting release artifact." | Project/headless Codex | Yes, because artifact/lifecycle ownership is required |
| "Open my daily setup." | Saved skill/workflow through a fast delegated or headless worker | Usually no if it is a short bounded restore; yes if durable lifecycle/cleanup/status is required |
| "Save the current setup as my work setup." | Master/subagent captures structured workflow/scene state, optionally creates/updates a skill | No task merely for saving; task only if the capture process itself needs durable lifecycle |
| "Work through this debugging session with me." | Visible interactive project Codex/tmux | Not automatically |
| "Keep this service operation tracked until it is healthy or fails." | Native/OpenRC/provider operation | Yes |
| "Deliver the generated report later through an approved adapter." | Producing execution + artifact/delivery workflow | Yes when delivery lifecycle exists |

These examples are routing guidance, not command-name contracts.

---

## 8. Task identity

### 8.1 Stable external task ID

Every Portus task receives an immutable globally unique identifier before managed side effects begin.

The external form is:

```text
task_<UUIDv7>
```

Example shape:

```text
task_019c...
```

UUIDv7 is selected because it provides standard globally unique identity with useful creation-time ordering without making database row numbers part of the external contract.

Rules:

- task IDs are never reused;
- task IDs are not user names, project names, process IDs, Codex session IDs, or timestamps;
- an optional human-readable task name/title is separate and may change;
- database-local integer keys may exist internally but must not be exposed as durable identity;
- a task ID remains valid after the task reaches a terminal state or linked execution disappears.

### 8.2 Task owner

Every user-scoped task has an owning Portus principal derived from the authenticated local caller/authority path defined by `docs/POLICY.md`.

The owner is not inferred from a caller-supplied username string.

Cross-user inspection/mutation requires explicit administrator authority.

### 8.3 Requester metadata

A task may record bounded requester metadata such as:

- owning/effective principal;
- requesting surface such as local Master session, SSH, or a future communication adapter;
- requesting Portus/Codex session reference where safe;
- project/workspace reference;
- optional parent task or retry relationship.

Requester metadata is not an authentication substitute. Authorization comes from the effective principal/policy boundary.

---

## 9. Minimal durable task record

The durable task record must be sufficient to reconstruct lifecycle without copying backend transcripts.

Conceptually it contains:

```text
task_id
owner_principal
name/title (optional)
objective_summary
state
state_reason
requester/surface reference
project/workspace reference (optional)
parent_task_id (optional)
retry_of_task_id (optional)
created_at
started_at (optional)
finished_at (optional)
policy/approval references
execution relationships
result summary/reference
artifact references
cleanup policy/reference
last significant event sequence
```

`objective_summary` is a bounded, secret-safe description of what the task is meant to accomplish. It is not a complete user transcript or duplicated Codex prompt.

Portus task storage must not contain:

- complete Codex transcripts;
- raw subagent conversations;
- unbounded tool output;
- raw reusable credentials;
- arbitrary project source;
- complete provider state;
- large artifacts when a locator/reference is sufficient.

When a Codex root session is the execution surface, PortusOS stores the Codex session/thread reference and safe task metadata; Codex remains authoritative for the conversation/history.

---

## 10. Task state machine

### 10.1 Canonical task states

PortusOS uses the following task lifecycle states:

- `created`
- `queued`
- `starting`
- `running`
- `waiting`
- `paused`
- `reconciling`
- `cancelling`
- `succeeded`
- `failed`
- `cancelled`
- `interrupted`

Terminal states are:

```text
succeeded
failed
cancelled
interrupted
```

### 10.2 State meanings

| State | Meaning |
| --- | --- |
| `created` | Durable task identity exists but execution has not yet been admitted/started. |
| `queued` | Task is accepted and waiting for execution/resource admission. |
| `starting` | PortusOS has begun launching/adopting required execution relationships but readiness is not yet established. |
| `running` | Required execution is actively progressing. |
| `waiting` | Task is intentionally blocked on a typed external condition such as approval, user input, provider/resource readiness, or another dependency. |
| `paused` | Task was explicitly suspended where the backend supports safe suspension/resume. |
| `reconciling` | PortusOS temporarily does not trust its prior live observation and is reconciling task/backend state after daemon restart, observation loss, or provider reconnection. |
| `cancelling` | Cancellation has been requested and PortusOS is waiting for the relevant execution surfaces to confirm stop/termination. |
| `succeeded` | Required task objective reached its declared successful terminal condition. |
| `failed` | Task ended with a known failure and will not automatically continue. |
| `cancelled` | Task stopped because cancellation was requested and the cancellation outcome is confirmed. |
| `interrupted` | PortusOS cannot safely establish/continue the intended execution after interruption; preserving evidence is safer than guessing or restarting. |

### 10.3 Waiting reasons

`waiting` must have a typed reason rather than an opaque string.

Initial conceptual reasons include:

- `approval`
- `user_input`
- `provider`
- `resource`
- `dependency`
- `rate_limit`
- `external_condition`

Exact schema codes belong to implementation/CLI contracts, but approval waiting must always be visible as such.

### 10.4 Transition principles

Typical flow:

```text
created
   -> queued
   -> starting
   -> running
        -> waiting -> running
        -> paused  -> running/queued
        -> cancelling -> cancelled
        -> succeeded
        -> failed
```

Observation loss/restart may cause any non-terminal live task to pass through:

```text
active state
   -> reconciling
      -> restored active state
      -> succeeded / failed / cancelled
      -> interrupted
```

Illegal or stale transitions must be rejected through expected-state checks rather than silently overwriting newer task state.

A terminal task does not silently re-enter `running`.

An explicit rerun/retry after terminal completion creates a new task linked through `retry_of_task_id`, preserving the prior result as immutable history.

---

## 11. Execution relationships

A task is an objective/lifecycle record, not necessarily one process.

It may reference zero or more execution relationships such as:

- Codex root session/thread;
- Codex subagent reference when useful for correlation;
- native process/process group;
- OpenRC service action/reference;
- application instance;
- provider operation/resource;
- workflow/scene invocation;
- child Portus task.

### 11.1 Codex root sessions

For a Codex execution relationship, PortusOS may store:

- Codex session/thread ID;
- safe session name where used;
- working directory/project reference;
- selected execution role/profile;
- model name actually used when available and useful for diagnostics;
- current high-level relationship/status observation.

PortusOS must not copy Codex transcript contents into task state.

### 11.2 Codex subagents

Codex owns the subagent tree.

PortusOS may record a subagent identifier/reference for correlation when a Portus task already exists, but it does not mirror every subagent state transition or automatically create one Portus child task per Codex child.

### 11.3 Native processes

A PID alone is not a safe durable process identity.

Task/process relationships must use enough generation identity to avoid killing or adopting an unrelated process after PID reuse, for example a combination of:

- boot identity;
- PID;
- process start time/generation;
- executable/launcher identity where appropriate;
- parent/process-group relationship;
- Portus launch correlation token/reference where available.

Exact process-identity implementation belongs to runtime implementation, but PID-only durable ownership is prohibited.

### 11.4 Providers

PortusOS stores provider operation/resource references rather than provider-internal payloads.

`docs/CAPABILITIES.md` defines whether a provider operation supports:

- status query;
- cancellation;
- reconciliation;
- idempotent retry;
- result/artifact references;
- health/degradation reporting.

### 11.5 Parent tasks

A Portus task may reference another Portus task as its parent when the child independently needs Portus task semantics.

A Codex subagent is not a Portus child task merely because it is a child in Codex's agent tree.

Complex dependency graphs, generic workflow DAG scheduling, and distributed schedulers are not required for the first ISO.

---

## 12. Managed versus associated execution

A task relationship must distinguish whether PortusOS **manages** the execution lifecycle or merely **associates/observes** an independently owned execution.

Conceptually:

```text
managed
  PortusOS is responsible for lifecycle operations it claims to support
  such as start/cancel/reconcile/cleanup.

associated
  PortusOS records the relationship/status but the external system owns
  lifecycle and PortusOS must not pretend it can control what it cannot.
```

Examples:

- a Portus-launched supervised native process may be `managed`;
- an already running Codex root session promoted into a Portus task may initially be `associated` unless the Codex integration provides reliable lifecycle control;
- an OpenRC service remains OpenRC-owned even when a task coordinates an operation around it;
- a PortusBrowser tab remains PortusBrowser-owned even when linked to a task.

The exact relationship names may change in schemas, but the semantic distinction is required.

---

## 13. Managed launch ordering and crash consistency

For execution that PortusOS claims to manage, durable identity must exist before unmanaged side effects are intentionally launched whenever technically possible.

Conceptually:

```text
1. authenticate/authorize caller
2. create durable task ID + starting intent
3. commit safe execution metadata
4. launch/adopt backend with task correlation
5. record verified backend identity
6. transition to running only after readiness/launch success is established
```

PortusOS must not normally:

```text
launch arbitrary process
  -> hope task-row creation works afterward
```

because a crash between those operations can create unowned work.

If backend launch succeeds but durable relationship recording fails, implementation must use a defined compensating/reconciliation path: terminate the newly launched backend when safe, or preserve enough correlation evidence to adopt/reconcile it. Silent orphaning is not acceptable for execution described as Portus-managed.

Promotion of already existing Codex/native/provider work is different: PortusOS creates the task and then safely adopts/references the existing execution without restarting it.

---

## 14. Progress and events

### 14.1 Significant events are durable

PortusOS stores bounded significant lifecycle events such as:

- task created;
- admitted/queued;
- execution starting;
- running;
- progress checkpoint;
- waiting reason entered/resolved;
- approval requested/resolved;
- pause/resume;
- cancellation requested/result;
- retry attempt started;
- backend relationship created/lost/reconciled;
- artifact/result registered;
- task succeeded/failed/cancelled/interrupted.

Events should carry:

- task ID;
- monotonic per-task sequence or equivalent ordering;
- timestamp;
- safe source/reference;
- event kind;
- bounded/redacted event data.

### 14.2 High-frequency execution detail stays with the backend

Portus task events are not a second Codex event log.

Do not copy into `portus.db`:

- every model token;
- every Codex tool invocation;
- full command stdout/stderr;
- every browser/network event;
- complete provider traces.

Those remain Codex/provider/log-owned. PortusOS stores only significant progress/status necessary for lifecycle and user/system coordination.

### 14.3 Progress is optional but honest

Not every backend can provide a meaningful percentage.

PortusOS must prefer truthful progress forms such as:

- current phase;
- latest significant checkpoint;
- completed/total units when the backend genuinely knows them;
- unknown progress.

It must not fabricate percentage completion from elapsed time or model speculation.

---

## 15. Cancellation

Task cancellation is different from cancelling one IPC request.

A client disconnect or CLI timeout must not automatically cancel a durable task.

Cancellation rules:

1. the caller requests cancellation under policy;
2. task enters `cancelling`;
3. PortusOS asks each required managed execution relationship to cancel/stop using its supported contract;
4. task becomes `cancelled` only when the required cancellation outcome is confirmed;
5. task-owned cleanup runs according to explicit cleanup policy;
6. cancellation result is recorded durably.

PortusOS must not report `cancelled` merely because it sent a signal/request.

If cancellation fails and the backend is still confirmed running, the task returns/remains `running` with a visible cancellation failure event/status rather than lying about cancellation.

If backend state cannot be established, the task moves through `reconciling` and may become `interrupted`.

Cancellation propagation to provider operations, Codex root sessions, native process groups, or child tasks depends on their declared relationship and contract. PortusOS must not kill unrelated processes or Codex sessions merely because they are visible in the system index.

---

## 16. Timeouts, retries, and crash loops

### 16.1 Timeouts

Request timeout, task timeout, provider timeout, and execution timeout are separate concepts.

A CLI/API request may time out while the durable task continues.

A task-level deadline or execution timeout, when configured, is explicit task policy and must produce a visible lifecycle outcome.

### 16.2 Retry safety

Automatic retries are **not** the default for arbitrary side-effectful operations.

Automatic retry is allowed only when:

- the operation/provider explicitly declares the retry safe or idempotent under the relevant conditions; or
- the task contract provides an operation-specific safe retry/recovery mechanism.

Examples that must not be blindly retried include destructive storage changes, unknown-state package transactions, arbitrary external writes, and actions that may duplicate deliveries or purchases.

### 16.3 Attempts

A task may contain a bounded sequence of execution attempts while it is non-terminal.

Attempt metadata may include:

- attempt number/ID;
- start/end timestamps;
- backend reference;
- failure classification;
- retry reason.

High-volume backend logs remain outside task state.

Once the task reaches a terminal state, a later explicit rerun creates a new task linked with `retry_of_task_id` rather than rewriting history.

### 16.4 Crash-loop detection

Repeated failure/restart must become a visible condition rather than an infinite hidden loop.

Exact retry counts/time windows are policy/health implementation values, not hardcoded architecture constants.

The task contract requires:

- bounded retries;
- visible retry count;
- terminal failure when the configured limit is reached;
- a typed crash-loop/retry-exhausted reason;
- preserved failure evidence sufficient for diagnosis.

---

## 17. Approval and policy waiting

A task may need to wait without losing identity.

When approval is required:

```text
task running/starting
  -> waiting(reason=approval)
  -> approval record references task
  -> approved -> resume allowed transition
  -> denied/expired -> fail or cancel according to the operation contract
```

Rules:

- approval state is explicit and queryable;
- task execution must not busy-loop while waiting;
- privilege credentials or raw reusable secrets are never stored in task state;
- approval authority comes from `docs/POLICY.md`, not from task metadata;
- an approval may narrow or authorize one task/operation without changing global policy;
- denying approval must not be silently converted into a different unapproved execution path.

Where safe, resources should be released while waiting rather than held indefinitely.

---

## 18. Results and artifacts

A terminal task has a bounded structured result.

Conceptually:

```text
terminal state
summary
result/failure classification
safe backend references
artifact references
relevant timestamps
cleanup/recovery note when needed
```

The result summary should be concise enough for Master Portus to synthesize to the user after long execution or context compaction.

PortusOS must not use task results as a transcript archive.

Large outputs become artifacts or remain in provider/project storage and are referenced by locator/ID.

Artifact identity, checksums, retention, classification, and delivery belong to `docs/ARTIFACTS.md`. This document locks only the relationship:

```text
Portus task
  -> zero or more artifact references
```

A task may succeed with no artifact, and an artifact may outlive the task that produced it.

---

## 19. Cleanup

Task completion does not imply indiscriminate deletion.

PortusOS may clean up only resources it owns or has an explicit cleanup contract for.

Default principles:

- do not delete project files because a task ended;
- do not delete user documents because they were task inputs;
- do not delete Codex session history merely because the Portus task reached a terminal state;
- do not close unrelated browser tabs/windows/processes discovered by heuristics;
- do remove task-owned temporary files/resources according to explicit retention policy;
- do stop or close task-owned ephemeral resources when their lifecycle contract says so;
- preserve failure evidence before destructive cleanup where recovery/diagnostics require it.

Cleanup must use stable ownership/relationship evidence, not guesses from matching process names or window titles.

---

## 20. `portusd` restart reconciliation

Durable task truth must survive `portusd` restart.

On daemon startup/restart:

1. load all non-terminal tasks owned by the relevant runtime scope;
2. place tasks requiring live-state verification into `reconciling` or equivalent internal reconciliation flow;
3. query each required authoritative execution source;
4. restore the correct active/terminal state when evidence is available;
5. mark the task `interrupted` when safe continuation/status cannot be established;
6. emit significant reconciliation events;
7. never silently restart arbitrary side-effectful work merely because it was previously `running`.

### 20.1 Codex reconciliation

For a Codex root-session relationship, PortusOS should use supported Codex session/thread status/identity surfaces rather than parsing transcript internals.

If the transient wrapper process is gone but the Codex session persists, PortusOS must distinguish session persistence from active execution.

### 20.2 Native-process reconciliation

PortusOS must revalidate generation identity, not only PID existence.

If the original process is gone and no authoritative result exists, PortusOS must not adopt a new unrelated process with a reused PID.

### 20.3 Provider reconciliation

A provider operation is reconciled through the provider's documented operation/resource contract where supported.

If the provider cannot report the prior operation and PortusOS cannot establish the result, the task becomes `interrupted` rather than inventing success/failure.

---

## 21. Reboot boundary

Full arbitrary reboot-resumable execution is explicitly deferred from the first ISO.

Therefore the first implementation must not promise that every running Portus task continues execution through a machine reboot.

After reboot:

- durable task metadata remains available;
- execution surfaces that independently persist/recover may be reconciled according to their own contracts;
- OpenRC services remain OpenRC-owned and may restart through OpenRC policy;
- Codex session history remains resumable where Codex provides that persistence;
- arbitrary native processes that did not survive are not silently recreated;
- non-recoverable active work becomes `interrupted` unless a specific safe resume/restart contract exists.

A future reboot-resumable task design may add explicit restart/resume policies, but it must not retroactively turn every task into a reboot job scheduler.

---

## 22. Master responsiveness and context economy

Master Portus should complete the **handoff**, not necessarily the entire delegated job, before becoming available for the next user request.

For independent work:

```text
user request
  -> Master decides routing
  -> create/launch required worker/task/session
  -> establish durable/safe handoff
  -> return concise acknowledgement/status to user
  -> independent worker continues
```

The handoff is complete only when Master has enough durable identity/status information to avoid losing the work.

For Codex-only work this may be a stable Codex root-session ID/name.

For Portus-managed work this is the Portus task ID plus its execution relationships.

Master context should retain/reference only what is useful for orchestration:

- task/session ID;
- short objective;
- important policy/approval state;
- latest significant status;
- final concise result/artifact references.

Detailed execution remains in the delegated Codex/provider/native domain.

### 22.1 Completion notification

Completion truth is a durable task event/result, not a transient notification.

When Master is active, an integration may surface completion through Portus event subscription and/or a validated Codex-native queue/session mechanism. Failure to deliver that immediate notification must not erase the task result.

When Master is not active, status remains queryable after return.

Future messaging/delivery adapters may notify remote users later, but they are outside the first-ISO task contract.

---

## 23. Concurrency, admission, and model routing

### 23.1 Responsiveness before maximum concurrency

PortusOS must preserve enough machine capacity for the Master Portus session and core OS operation.

The system must not maximize subagent/headless/task concurrency merely because the harness permits it.

Concurrency limits are measured/configured operational policy, not architectural magic constants.

The minimum 4 GiB VM and reference 8 GiB VM must be validated separately.

### 23.2 Codex remains the subagent scheduler

PortusOS does not build a competing scheduler for Codex subagents.

Master `AGENTS.md` authorizes delegation and describes routing policy; Codex manages its own subagent execution tree and configured concurrency.

### 23.3 Independent root sessions and Portus tasks

PortusOS may need admission control over independent headless root sessions or managed tasks when their aggregate resource use could degrade the device.

The detailed resource-budget implementation remains later runtime/health work, but the required behavior is:

- bound concurrent expensive work;
- queue or defer when capacity/policy requires;
- keep task state truthful while queued;
- preserve Master responsiveness;
- allow administrator/user policy to choose stricter limits.

### 23.4 Model selection

Routing chooses an execution **role/capability**, not a hardcoded model slug.

Examples may include:

- `fast-runner`;
- `researcher`;
- `reviewer`;
- `project-worker`.

Codex configuration/model catalog resolves the current allowed model and reasoning effort.

A model such as GPT-5.6 Luna may be preferred for fast delegated work when available and validated, but PortusOS task identity/state must never depend on that specific model remaining available.

When useful for diagnostics/reproducibility, task execution metadata may record the actual model used without treating it as durable task identity.

---

## 24. Skills, workflows, scenes, and tasks are separate concepts

### 24.1 Skill

A Codex skill is a reusable procedure/instruction/tool workflow.

Examples:

- repository review routine;
- project startup procedure;
- daily setup procedure;
- maintenance checklist.

The `portus-os-cli` skill remains scoped to teaching the `portus-os` CLI and must not become the general workflow library.

### 24.2 Workflow/scene record

A Portus structured workflow/scene record represents durable machine/provider relationships that should not be flattened into prose, such as:

- applications;
- projects;
- browser resource/tab references or URLs;
- i3 workspaces;
- window placement/display relationships;
- ordering/readiness conditions;
- saved machine setup state.

The exact schema is not locked here; it depends on `SYSTEM_INDEX`, `CAPABILITIES`, and later workflow design.

### 24.3 Task

A task is one execution/lifecycle instance.

Invoking the same skill or scene multiple times may create zero, one, or multiple tasks depending on the routing/lifecycle needs of each invocation.

Example:

```text
skill: daily-setup
scene: work-setup

Invocation A
  -> short delegated restore
  -> no Portus task

Invocation B
  -> restore across several providers
  -> user asks for tracked status + cleanup
  -> Portus task created
```

Saving or editing a skill/workflow does not itself require a Portus task merely because the object is durable.

---

## 25. Relationship to the system index and machine knowledge

Task state is Portus-owned durable truth.

The system index may expose task relationships such as:

```text
application instance -> task_id
process -> task_id
window -> task_id
provider resource -> task_id
project -> active task references
```

The index does not become the task database. It may cache/reference task relationships for navigation.

The durable per-machine knowledge map may record stable operational facts about how task/workflow resources are normally used on that machine, but it must not record current task state as durable knowledge.

Master Portus should use:

```text
portus-os task/status surface
  -> current Portus task truth

system index
  -> current resource topology

machine knowledge
  -> stable learned machine-specific facts
```

---

## 26. Security and principal isolation

Tasks are user-scoped unless explicitly system/admin scoped.

Requirements:

- task ownership is tied to the effective authenticated Portus principal;
- one Master user cannot inspect/cancel/mutate another user's private tasks by default;
- task events/results follow the same principal boundary;
- cross-user access requires explicit administrator authority;
- a task cannot grant itself authority beyond the requester/effective policy;
- project/task policy may further narrow delegated execution;
- root-equivalent operations remain classified accurately;
- provider policy remains independently authoritative;
- protected credentials are referenced, not embedded.

### 26.1 Secret handling

Task records, events, summaries, results, retry reasons, and diagnostics must never become a secret side channel.

Raw reusable protected credentials must not appear in:

- objective summaries;
- command-line metadata retained by Portus when avoidable;
- events;
- result summaries;
- approval payloads;
- artifact metadata;
- failure diagnostics.

Secret-backed actions use the protected provider/broker architecture in `docs/SECRETS.md`.

---

## 27. Portus OS CLI/runtime obligations

`docs/CLI.md` locks the first task command surface as:

```text
portus-os task list
portus-os task show <task-id>
portus-os task events <task-id>
portus-os task cancel <task-id>
```

These commands expose caller-visible task summaries/details, bounded significant events/progress, and durable cancellation under this document's lifecycle rules.

There is no generic first-contract `portus-os task create` command merely to manufacture task rows. Tasks are created/registered by managed operations when this document requires Portus lifecycle semantics. Pause/resume, explicit retry/rerun, adoption, and manual task creation remain future CLI additions until their backend semantics and use cases justify them.

Normal runtime-backed task mutation goes through `portusd`. Direct SQLite mutation is not a normal API.

`portus-os doctor` must remain able to diagnose task/runtime inconsistency even when ordinary daemon-backed task operations are unavailable, consistent with `docs/RUNTIME.md` and `docs/STATE.md`.

---

## 28. First-ISO boundary

This document does not add a new first-ISO acceptance gate beyond `docs/ACCEPTANCE.md`.

The first implementation of tasks may be intentionally narrow.

It should prove the architecture with a useful minimum such as:

- real durable task identity/state in SQLite;
- authenticated owner/principal filtering;
- one real managed or associated execution backend;
- significant events;
- cancellation where the chosen backend supports it;
- terminal result/failure persistence;
- `portusd` restart reconciliation;
- no dependence on a visible terminal;
- no duplication of Codex transcripts/provider state;
- no claim of arbitrary reboot-resumable execution.

A particularly valuable validation case is a tracked headless Codex job or similarly independent execution that continues after the initiating CLI request/turn has returned, while Master Portus remains available for another request. The exact first implementation backend will be selected during implementation planning; this document does not require every task to use Codex.

---

## 29. Validation requirements

Before the task subsystem is considered implementation-ready, validation must cover at least:

### Routing/non-duplication

- [ ] "Open Gmail" or equivalent immediate application action can remain direct Master execution with no task record.
- [ ] A bounded Codex subagent can complete without automatically creating a Portus child task.
- [ ] A headless Codex root session can exist independently without requiring a visible terminal.
- [ ] A headless Codex root session can be promoted/associated with a Portus task without duplicating its transcript or restarting the work.
- [ ] A visible Codex/tmux session is selected because visibility/interaction is needed, not merely because work is complex.

### Identity/state

- [ ] Task IDs use the locked `task_<UUIDv7>` external form and are never reused.
- [ ] Task ownership is enforced by caller principal.
- [ ] Another Master user cannot inspect or cancel a private task without explicit authority.
- [ ] State transitions reject stale/illegal expected-state updates.
- [ ] Terminal task history is not overwritten by a later rerun.

### Execution relationships

- [ ] Codex task relationships store IDs/references without copying transcripts.
- [ ] Native process relationships cannot confuse a reused PID with the original execution.
- [ ] Provider references remain references rather than copied provider state.
- [ ] Portus can distinguish managed from associated execution.

### Cancellation/retry

- [ ] Client disconnect does not silently cancel a durable task.
- [ ] Cancellation is not reported complete until the required backend stop is confirmed.
- [ ] Failed cancellation does not falsely mark a still-running backend as cancelled.
- [ ] Automatic retry is bounded and used only for declared-safe retry paths.
- [ ] Crash-loop/retry exhaustion becomes visible and terminal.

### Reconciliation

- [ ] Restarting `portusd` preserves durable task metadata.
- [ ] Non-terminal tasks reconcile against authoritative backend state.
- [ ] Unknown execution state becomes `interrupted` rather than guessed/restarted.
- [ ] Reboot does not silently relaunch arbitrary prior processes.

### Context/results/security

- [ ] Master can recover task status after Codex compaction using Portus task state.
- [ ] Task events/results remain bounded and do not duplicate high-frequency Codex/provider logs.
- [ ] Large outputs are referenced as artifacts/provider/project resources rather than stored as task blobs.
- [ ] Raw reusable credentials do not enter task records/events/results.
- [ ] Completion remains queryable even if immediate Master notification fails.

---

## 30. Decisions locked by this document

The following decisions are now locked:

1. Execution-surface selection and Portus-task registration are separate decisions.
2. A Portus task is not the default unit of every Master Portus action.
3. Master `AGENTS.md` is responsible for the device-manager/orchestrator routing charter; detailed CLI usage remains in the `portus-os-cli` skill.
4. Direct Master execution is preferred for immediate tightly coupled work.
5. Codex subagents are bounded Codex-owned sidecars and do not automatically become Portus tasks.
6. Independent headless Codex root sessions are the preferred Codex-native surface for independent multi-step work that does not require a visible terminal.
7. A headless Codex root session may remain Codex-only when Codex lifecycle/history are sufficient.
8. Visible Codex/tmux sessions are for human-visible interaction/inspection/recovery, not complexity-based durability.
9. Work is promoted to a Portus task only when durable Portus lifecycle/relationship semantics are required or the user explicitly requests task tracking.
10. Existing work may be promoted/adopted without restarting or duplicating it.
11. Portus tasks may coordinate Codex roots, subagents by reference, native processes, providers, application resources, artifacts, and child Portus tasks while those systems retain domain ownership.
12. External task IDs use `task_<UUIDv7>` and are immutable/non-reused.
13. Task ownership is principal-scoped and authenticated through the PortusOS identity boundary.
14. Canonical task states are `created`, `queued`, `starting`, `running`, `waiting`, `paused`, `reconciling`, `cancelling`, `succeeded`, `failed`, `cancelled`, and `interrupted`.
15. `succeeded`, `failed`, `cancelled`, and `interrupted` are terminal states.
16. Waiting has an explicit typed reason, including approval waiting.
17. Terminal tasks do not silently restart; a later explicit rerun creates a new task linked to the prior task.
18. Task/process relationships may not rely on PID alone.
19. Managed versus associated execution is an explicit semantic distinction.
20. Durable task identity should be committed before managed side effects whenever technically possible.
21. Portus task events store significant bounded lifecycle/progress information, not complete Codex/provider logs.
22. Task cancellation is durable lifecycle behavior and is not equivalent to cancelling one IPC request.
23. A task is not `cancelled` until required cancellation is confirmed.
24. Automatic retries are bounded and permitted only for declared-safe retry paths.
25. Repeated failures become visible crash-loop/retry-exhaustion outcomes rather than infinite hidden retries.
26. Approval waiting preserves task identity without storing credentials or weakening policy.
27. Task results are bounded summaries/references; large outputs become artifacts or remain backend-owned.
28. Cleanup acts only on explicitly task-owned/contracted resources.
29. `portusd` restart triggers reconciliation against authoritative backend sources rather than blind restart.
30. Full arbitrary reboot-resumable execution remains deferred from the first ISO.
31. Master responsiveness and context economy are first-class routing goals.
32. Completion truth is durable task state/event; notification delivery is secondary.
33. PortusOS does not build a competing Codex subagent scheduler.
34. Model routing is role/capability based and must not hardcode one model slug as OS architecture.
35. Skills, structured workflow/scene records, and tasks remain separate concepts.
36. The system index may reference task relationships but does not become the task database.
37. The durable machine knowledge map may describe stable task/workflow conventions but not current task state.
38. Raw reusable protected credentials are prohibited from task state/events/results.
39. `docs/CLI.md` locks the first task CLI surface as `task list/show/events/cancel`, with structured output/error/pagination/precondition behavior that must not change these lifecycle boundaries.
40. `docs/CAPABILITIES.md` defines provider-side operation/status/cancellation/reconciliation/retry/result affordances without changing task ownership semantics.

---

## 31. P7 host-safe implementation record

P7 concretizes the first task implementation without changing the architecture above:

- shared task/project/session/relationship wire types live in `portus-protocol`;
- the active `portus-state` schema is now v7; task truth/attempts/project-session references/managed-associated relationships originate in P7 migration v5 and task lifecycle history uses the common P8 v6 `significant_events` store, while P11 v7 adds health/recovery state without changing task ownership semantics;
- `portus-task` owns lifecycle transition validation and the first narrow managed native-child backend;
- the managed backend accepts an internal argv vector rather than a shell string, bounds argument count/size, captures no stdout/stderr, persists no argv/environment, and compensates failed post-spawn persistence by stopping/reaping the child;
- `managed` and `associated` are both executable implementation semantics: associated Codex/provider-style references may be tracked without Portus claiming lifecycle control;
- retry safety is explicit (`never`, `idempotent`, `contract_safe`), while automatic retry is absent for arbitrary work; the first unsafe launch-failure proof creates exactly one failed attempt;
- cancellation is confirmed before `cancelled`; a natural process exit that wins the race is reconciled to its actual terminal outcome;
- restart reconciliation never trusts PID alone. The real Linux managed-child path has now been exercised for launch/show/confirmed cancellation over the authenticated socket; restart-time `/proc` generation revalidation is still deliberately conservative, so unverifiable prior live native work becomes `interrupted` instead of being re-adopted or restarted until the installed target-specific adapter is closed;
- `portusd` exposes only task read/cancel methods and keeps managed launch internal; normal shell/Codex execution continues to bypass the task engine;
- `portus-os task list/show/events/cancel` is implemented. P8 now adds bounded retained-history gap semantics plus real incremental `task events --follow`/`--follow --jsonl` presentation on top of the durable task event sequence.

The source-neutral implementation plus the recorded disposable-Linux run now prove durable identity, principal filtering, state transitions, attempts, managed-versus-associated ownership, real Linux child launch/show/confirmed cancellation over authenticated IPC, bounded results/events, conservative restart handling, and non-duplication. The remaining process proof is narrower: installed-target UID/GID ownership and restart-time `/proc` generation reconciliation rather than basic Linux cancellation/socket execution.

---

## 32. Remaining dependent implementation decisions

The P7 host-safe task contract is implemented. Remaining dependent work is now narrower:

- finalize installed-target `/proc` process-generation/correlation, UID/GID ownership, process-group/signal edge cases, and daemon-restart reconciliation behavior; basic real Linux managed-process cancellation already passes;
- add Codex-on-Artix root/session/process reconciliation through supported Codex identity/status surfaces when P13/Linux integration is available;
- keep the now-passing P8 live Unix-socket disconnect/reconnect/durable-replay path as regression coverage while validating any installed OpenRC-specific differences;
- select task-resource admission limits for minimum/reference hardware from measured runtime/health evidence;
- select terminal-task/significant-event retention and pruning periods rather than inventing them before storage/health testing;
- define explicit task/execution timeout values only for operations that actually need them;
- define bounded automatic-retry/crash-loop budgets in the later health/runtime policy while preserving the P7 retry-safety declaration;
- connect provider operation/cancel/reconcile affordances as provider integrations arrive;
- connect artifact registration/cleanup in P12;
- connect common health/recovery observations in P11;
- connect approval records/transport in P9 and later approval work;
- future reboot-resumable task policy;
- future scheduled/conditional task creation;
- future remote completion notification/delivery adapters;
- any future workflow/scene schema.

Those decisions must preserve the boundaries in this document rather than collapsing Portus tasks into Codex internals or turning PortusOS into a duplicate agent scheduler.
