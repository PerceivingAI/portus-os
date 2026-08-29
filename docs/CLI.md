# PortusOS CLI Contract

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative CLI architecture and command contract
**Executable:** `portus-os`
**Implementation:** Rust native executable
**IPC:** Versioned JSONL over `/run/portus/portusd.sock`
**Scope:** Human/agent command surface for PortusOS-owned runtime domains, structured output, errors, pagination, preconditions, events, diagnostics, and compatibility

## 1. Purpose and authority

This document is authoritative for the user/agent contract of the Portus OS CLI.

It defines:

- the top-level `portus-os` command domains;
- exact architectural command names for the first contract;
- human versus machine-readable output behavior;
- JSON/JSONL envelopes;
- stable CLI exit-code families and semantic error codes;
- bounded list/query behavior and cursor pagination;
- expected-state/generation checks;
- dry-run/preflight behavior;
- task event streaming;
- capability/provider discovery behavior;
- policy inspection/preflight behavior;
- health versus `doctor` behavior;
- machine-readable help and version/compatibility behavior;
- the boundary between the Rust executable and the `portus-os-cli` Codex skill.

`docs/RUNTIME.md` remains authoritative for `portusd`, the Unix-socket/JSONL runtime protocol, caller-principal derivation, daemon degradation, and the Rust thin-client boundary. `docs/TASKS.md`, `docs/SYSTEM_INDEX.md`, `docs/CAPABILITIES.md`, `docs/POLICY.md`, `docs/SECRETS.md`, `docs/ARTIFACTS.md`, and `docs/HEALTH_RECOVERY.md` remain authoritative for their domain semantics.

This document does not authorize the CLI to reimplement those subsystems.

---

## 2. Core CLI decision

The Portus OS CLI is:

```text
/usr/bin/portus-os
```

It is a Rust native executable and the primary human/agent command surface for PortusOS-owned operating-system coordination.

The normal path is:

```text
CLI arguments
  -> typed Rust client request
  -> versioned JSONL over /run/portus/portusd.sock
  -> portusd
  -> typed response/error
  -> human or structured rendering
```

The CLI is deliberately thin.

It owns:

- argument parsing and local syntactic validation;
- construction/validation of typed client requests;
- Unix-socket client/framing behavior through shared Rust code;
- protocol compatibility checking;
- bounded output rendering;
- stable CLI exit-code mapping;
- machine-readable help/version presentation;
- bounded daemon-independent `doctor` diagnostics explicitly allowed by `docs/RUNTIME.md`.

It does not own:

- task state transitions;
- System Index correlation or source authority;
- provider-domain behavior;
- provider lifecycle semantics;
- policy decisions;
- raw secret retrieval;
- normal SQLite mutation;
- a second runtime scheduler;
- generic Linux/file/project/Git/shell behavior.

---

## 3. Non-duplication boundary

`portus-os` must not become a replacement shell or a convenience wrapper around capabilities Codex/Linux already provide cleanly.

The CLI must not add generic commands equivalent to:

```text
files list
files tree
files read
files search
exec
shell
process list
package list
git ...
project patch
```

for ordinary operations where native/Codex tools are already the correct interface.

Examples that remain native/direct:

```text
filesystem/source work    -> Codex, ls, find, fd, tree, rg, etc.
Git                       -> git
ordinary commands         -> shell through Codex
process inspection        -> ps, pgrep, /proc
package inspection        -> pacman
OpenRC inspection         -> rc-status, rc-service, rc-update
i3 control                -> i3-msg / X11 tools
browser-domain work       -> portus-browser
```

A `portus-os` command is justified only when it exposes Portus-owned indexing, durable task lifecycle, capability/provider registration metadata, policy/preflight, artifact identity, structured health/recovery, or another coordinated OS contract defined by the active architecture.

---

## 4. Locked top-level command surface

The first architectural command tree is:

```text
portus-os
├── status
├── doctor
├── index
├── task
├── capability
├── policy
├── artifact
├── health
├── help
└── version
```

`help` and `version` are CLI metadata commands rather than PortusOS control domains.

No generic top-level `system`, `files`, `exec`, `shell`, `provider-invoke`, or miscellaneous `util` bucket is part of this contract.

New top-level domains require a demonstrated Portus-owned responsibility and must pass the non-duplication test in `docs/CONTROL_PLANE.md`.

---

## 5. Global output model

### 5.1 Human output

Human-readable output is the default when no structured-output flag is supplied.

Human output should be concise and deterministic enough for terminal use, but its spacing/table presentation is not the machine contract.

Successful primary output goes to stdout.

Warnings and diagnostic commentary go to stderr.

Errors in human mode go to stderr.

The CLI must not require ANSI color to convey meaning. If color is added later, structured output remains unaffected and non-TTY output must remain readable without terminal escape codes.

### 5.2 JSON output

Machine-readable request/response commands support:

```text
--json
```

Agents and automation should use `--json` whenever they need stable field-level parsing.

Human formatting must not be scraped as a substitute for the JSON contract.

### 5.3 JSONL output

Streaming/event commands support:

```text
--jsonl
```

JSONL is for event streams or intentionally streamed bounded records. It is not the default representation for ordinary one-response commands.

A command that does not expose streaming semantics must reject unsupported `--jsonl` use rather than silently changing meaning.

### 5.4 Output-mode exclusivity

`--json` and `--jsonl` are mutually exclusive.

A command must not mix human prose into stdout while structured output is active.

Warnings that must be visible outside the structured result belong either in the structured metadata/error object or on stderr when doing so does not force callers to parse stderr for semantic success.

---

## 6. Structured response envelope

The CLI JSON presentation layer is a stable user/agent contract distinct from the internal runtime request/response framing.

The first response envelope is conceptually:

```json
{
  "schema_version": 1,
  "command": "index.apps",
  "ok": true,
  "data": {},
  "meta": {}
}
```

Required top-level fields:

- `schema_version` — CLI output schema contract version;
- `command` — stable command identifier such as `index.apps`;
- `ok` — boolean success indicator;
- `data` — command-specific successful result when `ok=true`;
- `meta` — bounded common metadata.

Useful `meta` fields may include:

- request/correlation ID where available;
- generated/observed timestamp where meaningful;
- `next_cursor` for paginated results;
- result count;
- degraded/partial-observation indicator;
- source/freshness summary where relevant;
- CLI/runtime protocol version metadata where relevant.

Unknown additive `meta` fields must not change command semantics.

A command-specific breaking data-schema change requires an explicit CLI output-schema compatibility decision rather than silently repurposing existing fields.

---

## 7. Structured error envelope

Structured errors use the same outer contract:

```json
{
  "schema_version": 1,
  "command": "task.cancel",
  "ok": false,
  "error": {
    "code": "precondition_failed",
    "message": "Task state no longer matches the requested precondition.",
    "retryable": false,
    "details": {}
  },
  "meta": {}
}
```

Required error fields:

- `code` — stable machine-readable semantic error code;
- `message` — concise safe human explanation;
- `retryable` — whether immediate retry may plausibly succeed without changing the request/precondition;
- `details` — bounded structured safe detail where useful.

Error details must not expose reusable secrets, unbounded provider payloads, raw process environments, or unrelated private data.

The semantic error code is more precise than the process exit code. Callers needing exact behavior should inspect structured errors rather than trying to infer all meaning from one integer.

---

## 8. Semantic error taxonomy

The initial common error codes are:

### Request/CLI contract

```text
invalid_argument
invalid_request
unsupported_output_mode
```

### Runtime/protocol

```text
daemon_unavailable
protocol_error
incompatible_protocol
```

### Identity/resource

```text
not_found
stale_resource
precondition_failed
conflict
```

### Authority/policy

```text
permission_denied
approval_required
```

### Availability/capability

```text
unavailable
provider_unavailable
source_unavailable
unsupported
```

### Execution/lifecycle

```text
timeout
cancelled
interrupted
```

### Internal

```text
internal
```

Subsystems may define additional stable detail/reason codes, but new generic CLI error codes should be added sparingly.

Examples:

- task already terminal when a running-state precondition was supplied -> `precondition_failed`;
- old `idx_...` handle no longer refers to the same source generation -> `stale_resource`;
- caller lacks authority -> `permission_denied`;
- operation needs a human/admin approval -> `approval_required`;
- provider exists but cannot currently serve the request -> `provider_unavailable`;
- task/backend does not implement pause/cancel semantics -> `unsupported`;
- CLI and daemon speak incompatible contract versions -> `incompatible_protocol`.

---

## 9. Stable exit-code families

The first CLI exit-code families are:

| Exit | Meaning |
| ---: | --- |
| `0` | Command completed successfully. |
| `2` | CLI usage/argument/request-shape error. |
| `3` | Daemon/transport/protocol connectivity failure. |
| `4` | Requested object/reference not found. |
| `5` | Permission/policy/approval prevents the requested operation. |
| `6` | Conflict, stale target, or expected-state/precondition failure. |
| `7` | Required provider/source/capability unavailable, incompatible, or unsupported. |
| `8` | Operation timed out. |
| `9` | Operation/result is cancelled or interrupted where that is the command outcome. |
| `10` | Internal PortusOS/CLI failure not covered above. |

Mappings include:

```text
invalid_argument         -> 2
daemon_unavailable       -> 3
protocol_error           -> 3
incompatible_protocol    -> 3
not_found                -> 4
permission_denied        -> 5
approval_required        -> 5
stale_resource           -> 6
precondition_failed      -> 6
conflict                 -> 6
provider_unavailable     -> 7
source_unavailable       -> 7
unsupported              -> 7
timeout                  -> 8
cancelled                -> 9
interrupted              -> 9
internal                 -> 10
```

`incompatible_protocol` is locked to exit family `3`. The semantic JSON code remains the precise machine-readable distinction and must never be hidden behind a generic prose-only failure.

Shell-level signal termination follows normal operating-system conventions and is separate from Portus semantic exit-code assignment.

---

## 10. Partial/degraded results

A command can succeed while reporting degraded observations.

Example:

- the System Index returns valid applications/windows from healthy sources while one optional provider source is temporarily unavailable.

In that case the command may exit `0` if the requested result is valid and the degraded condition is explicitly represented in `meta`/records.

The CLI must not silently present incomplete/degraded data as complete/current truth.

If the unavailable component prevents satisfying the actual requested operation, the command fails with the appropriate semantic error/exit family.

This distinction is especially important for `status`, `index`, `capability`, and `health`.

---

## 11. Pagination and bounded results

Any command that can return an unbounded collection must support bounded results.

The common list/query pattern is:

```text
--limit <N>
--cursor <opaque-cursor>
```

Rules:

- default and maximum limits are bounded implementation constants validated against minimum/reference hardware;
- callers may request a smaller limit;
- callers cannot request an unbounded result by using an arbitrarily large limit;
- `cursor` is opaque and must not be parsed by callers;
- successful paginated JSON output exposes `meta.next_cursor` when more results exist;
- cursors are not durable resource identities and may expire after relevant state/schema/reconciliation changes;
- an expired/invalid cursor fails clearly instead of silently restarting at an unrelated position.

Page-number pagination is not the base contract for live/mutable machine state.

---

## 12. Stable references and ID handling

The CLI accepts authoritative IDs/references from their owning subsystems rather than exposing SQLite row numbers.

Examples include:

```text
task_<UUIDv7>
provider_<UUIDv7>
idx_<UUIDv7>
artifact reference/ID once ARTIFACTS.md locks it
project references
Codex thread/session references where a command explicitly accepts them
provider resource references
```

Rules:

- human labels never replace immutable IDs where identity matters;
- `idx_...` is a generation-scoped System Index navigation handle, not a permanent workflow identity;
- stale index handles fail with `stale_resource` when generation continuity cannot be proven;
- provider resource IDs remain opaque under `docs/CAPABILITIES.md`;
- PID/X11 IDs alone must not be accepted as durable Portus identities merely because they are short to type.

---

## 13. Expected-state and generation checks

State-sensitive mutations must support typed preconditions where stale state could cause an incorrect action.

The CLI uses command-specific `--if-*` precondition flags rather than one untyped global free-form expectation string.

Examples include conceptual forms such as:

```text
--if-state <task-state>
--if-generation <generation-token>
--if-version <contract/version-token>
```

Exact availability depends on the command/resource type.

Rules:

- preconditions are sent to the authoritative runtime/subsystem and checked atomically with the intended operation where possible;
- the CLI does not fetch state, compare locally, and then assume it stayed unchanged;
- failed preconditions return `precondition_failed` or `stale_resource` rather than silently continuing;
- a command that cannot enforce a claimed expected-state check must not expose the option as if it were safe.

For `idx_...` resources, source generation/revalidation from `docs/SYSTEM_INDEX.md` remains authoritative.

---

## 14. Dry-run and preflight

Commands with meaningful side effects may support:

```text
--dry-run
```

The semantic contract is:

> validate/resolve/preflight the requested operation as far as the backend safely can, while committing no intended mutation/side effect.

A dry run may report:

- effective policy outcome;
- approval requirement;
- resolved targets/references;
- expected-state mismatch;
- unavailable dependency/provider;
- likely Portus task creation/association;
- intended managed/associated relationship class;
- planned high-level actions.

A dry run must not secretly perform the mutation and then roll it back merely to simulate success.

Commands for which no meaningful side-effect-free preflight exists should not claim `--dry-run` support.

`policy check` is the dedicated policy-oriented preflight surface where the policy schema can describe the action without performing it.

---

# Command domains

## 15. `portus-os status`

`status` is the cheap normal operational summary.

```text
portus-os status
portus-os status --json
```

It should summarize bounded high-level state such as:

- `portusd` runtime availability/health;
- Portus state availability;
- System Index state (`initializing`, healthy/degraded/rebuilding/unavailable as appropriate);
- provider/capability availability summary;
- active Portus task count/state summary visible to the caller;
- policy/config load summary;
- major degradation requiring attention.

`status` is not a deep diagnostic scan and does not independently recreate daemon state when `portusd` is unavailable.

If the daemon-backed status path cannot be reached, the command returns `daemon_unavailable` and directs the human presentation toward:

```text
portus-os doctor
```

The structured error remains machine-readable rather than relying on that suggestion.

---

## 16. `portus-os doctor`

`doctor` is the bounded daemon-independent diagnostic surface defined by `docs/RUNTIME.md`.

Locked command forms:

```text
portus-os doctor
portus-os doctor runtime
portus-os doctor state
portus-os doctor index
portus-os doctor providers
portus-os doctor codex
```

`portus-os doctor` without a domain runs the supported diagnostic groups and returns a bounded aggregate report.

### Doctor responsibilities

`doctor` may directly inspect only what is necessary to diagnose/bootstrap/recover the PortusOS control plane, including approved checks such as:

- canonical `portusd` socket existence/connectivity;
- OpenRC service/process state required to diagnose `portusd`;
- required PortusOS directories/files/ownership where safe;
- SQLite database presence and approved read-only/integrity checks;
- System Index storage/source integration health evidence available without relying on a healthy daemon;
- provider manifest/registration installation evidence and selected safe provider health probes where contractually appropriate;
- Codex installation/config/trust prerequisites owned by the PortusOS integration.

### Doctor restrictions

`doctor` does not become:

- a second task engine;
- a second System Index implementation;
- a provider-domain command proxy;
- a policy bypass;
- an automatic repair-everything mechanism;
- a database mutation tool.

Repair/mutation commands belong to explicitly defined recovery/system workflows after `docs/HEALTH_RECOVERY.md` and the relevant policy contracts are locked.

A diagnostic check may report a recommended native/admin recovery action without performing it automatically.

P6 implements `doctor index` as a daemon-independent **read-only SQLite metadata diagnostic**. It opens `portus.db` through the safe read-only state path and inspects machine-scoped System Index runtime/source metadata. It does not call `portusd`, execute native index source probes, rebuild the graph, or expose another user's private graph records.

---

## 17. `portus-os index`

`index` exposes the freshness-aware operational machine graph from `docs/SYSTEM_INDEX.md`.

### 17.1 Quick views

Locked quick-view forms:

```text
portus-os index apps
portus-os index windows
portus-os index workspaces
portus-os index displays
portus-os index providers
portus-os index stale
```

Semantics:

- `apps` — current/open/running application instances known to the graph, not all installed packages;
- `windows` — current graphical window resources visible to the caller under policy/redaction;
- `workspaces` — current i3 workspace resources/relationships;
- `displays` — current display resources/relationships;
- `providers` — live/indexed provider registrations/resources/availability observations, distinct from the durable capability registry views under `capability`;
- `stale` — bounded resources/observations currently requiring or recommending refresh/revalidation.

Installed application definitions remain queryable through the general index query rather than overloading `apps` with two meanings.

### 17.2 General query

```text
portus-os index query [filters] [--limit N] [--cursor C]
```

The current P6/P15 implementation exposes the following finite filters:

```text
--type <application-definition|application-instance|process|openrc-service|window|workspace|display|provider-registration|provider-resource|registered-capability>
--freshness <live|recent|stale|unavailable|historical>
--source <applications|proc|openrc|x11|i3|providers|correlation>
--application <value>
--provider <value>
--capability <value>
--workspace <value>
--display <value>
--evidence <authoritative|strong|heuristic>
--changed-since-ms <unix-ms>
--control-path <registered-provider|structured-api|structured-cli|application-adapter|native-system|accessibility|process-window|visual-fallback>
--limit <1..200>
--cursor <opaque-idx-cursor>
```

The common default page limit is 50 and the maximum is 200. There is no free-form SQL or generic query language. Task/project/Codex relationship filters remain part of the wider architecture but are not claimed by the P6 CLI; they are added only when their owning registry phases provide those relationships.

### 17.3 Inspect

```text
portus-os index show <resource-ref>
```

Returns a bounded detailed resource view including relevant:

- authoritative/source identity;
- `idx_...` handle where applicable;
- resource type;
- source/provenance;
- freshness/generation;
- evidence-backed relationships;
- task/project/provider references;
- available control paths;
- safe annotations;
- principal/redaction-aware metadata.

It does not automatically include provider-owned deep state such as browser page snapshots or full terminal content.

### 17.4 Topology

```text
portus-os index topology <resource-ref>
```

Returns a bounded relationship tree/graph rooted at the selected resource.

P6 uses a default traversal depth of 3 with an accepted range of 1-6, and a default resource bound of 100 with the common maximum of 200. No unbounded entire-machine graph dump is exposed.

This is a machine-resource topology command, not a filesystem tree command.

### 17.5 Targeted refresh

```text
portus-os index refresh <resource-ref>
```

Requests targeted authoritative refresh/revalidation of the selected resource/domain relationship as defined by `docs/SYSTEM_INDEX.md`.

The command must not claim success merely because cached timestamps were rewritten.

The P6 implementation resolves the selected visible resource, maps it to its owning bounded source domain, performs that source-domain reconciliation, and then rechecks the **same `idx_...` generation handle**. If the resource disappeared, became historical, or changed generation, refresh returns `stale_resource` rather than rebinding the handle to a replacement.

### 17.6 Domain rescan

```text
portus-os index rescan <domain>
```

The initial architectural domains are:

```text
applications
runtime
providers
services
```

These are bounded System Index/source-adapter rescans. They are not generic filesystem scans.

Additional domains such as selected devices/mounts may be added when their first-class index implementation is required.

### 17.7 Reconcile

```text
portus-os index reconcile
```

Runs an explicit full supported-source reconciliation while preserving authoritative/durable Portus truth.

This is heavier than normal targeted refresh and should not be invoked reflexively before ordinary queries.

### 17.8 Rebuild

```text
portus-os index rebuild
```

Rebuilds disposable/derived index state while preserving durable task/provider/artifact/policy/annotation truth according to `docs/STATE.md` and `docs/SYSTEM_INDEX.md`.

It is a state-changing/recovery operation and therefore must use policy/preflight/expected-state protections appropriate to the final implementation.

Until P9 implements the final typed policy/elevation path, P6 fails closed by allowing the runtime `index.rebuild` operation only when the authenticated Unix peer UID is 0. This is a temporary restrictive implementation boundary, not the final Master/admin authorization model, and the CLI contains no local privilege shortcut.

### 17.9 Index status

```text
portus-os index status
```

Returns detailed System Index/source state beyond the short top-level `status` summary, including warming/reconciling/degraded/source-unavailable information.

---

## 18. `portus-os task`

`task` exposes durable Portus task lifecycle from `docs/TASKS.md`.

The first locked task command surface is:

```text
portus-os task list
portus-os task show <task-id>
portus-os task events <task-id>
portus-os task cancel <task-id>
```

There is no generic first-contract:

```text
portus-os task create
```

merely to manufacture task rows.

Portus tasks are created/registered when a Portus-managed operation requires the lifecycle semantics in `docs/TASKS.md` or when an explicitly designed adoption/tracking operation later requires one.

### P7/P8 implementation contract

The host-safe P7/P8 implementation uses the locked command surface above with these concrete first forms:

```text
portus-os task list [--state <state>] [--project <project-ref>] [--limit N] [--cursor C]
portus-os task show <task-id>
portus-os task events <task-id> [--after <sequence>] [--limit N]
portus-os task events <task-id> --follow
portus-os task events <task-id> --follow --jsonl
portus-os task cancel <task-id> [--if-state <state>]
```

`task list` and retained `task events` use the common bounded limit of 1..200. The default task-list limit remains the common CLI default of 50; retained events default to 100. List cursors remain opaque task-ID-based continuation tokens at the CLI boundary, while retained event continuation uses the durable per-task event sequence. P8 additionally caps retained significant history at 512 events per object and reports stale retained-history gaps explicitly.

P7 deliberately did not fake `--follow` with polling. P8 now implements the real stream foundation: the runtime stream rereads authoritative durable task-event sequences, human and JSONL output are incremental, and indefinite `--follow --json` is rejected rather than buffered.

The ordinary task request/response wire surface remains `task.list`, `task.show`, `task.events`, and `task.cancel`; P8 adds the dedicated long-lived `task.events.follow` stream method on the local runtime connection. There is still no `task.create`, process-start, shell, or generic command-execution RPC. The first managed child-process backend is an internal first-party runtime facility used only by operations that already require Portus task semantics.


### 18.1 Task list

```text
portus-os task list [filters] [--limit N] [--cursor C]
```

Returns tasks visible to the authenticated caller, with bounded summaries such as:

- task ID;
- title/objective summary;
- state/reason;
- project/workspace reference;
- relevant execution/provider relationship summary;
- created/started/finished timestamps;
- terminal/result reference summary where appropriate.

It must not return complete Codex transcripts or unbounded backend logs.

### 18.2 Task show

```text
portus-os task show <task-id>
```

Returns one task's durable lifecycle state and significant relationships/results consistent with `docs/TASKS.md`.

### 18.3 Task events

```text
portus-os task events <task-id>
portus-os task events <task-id> --after <sequence>
portus-os task events <task-id> --follow
portus-os task events <task-id> --follow --jsonl
```

Without `--follow`, returns a bounded retained set of significant task events.

`--after <sequence>` resumes after a caller-known significant event sequence without requiring a transcript/log replay.

If that resume point predates the retained event window, the daemon returns `stale_resource` with safe retained/latest sequence evidence. It does not silently continue from the oldest surviving event.

With `--follow`, the command remains attached to the live event stream until terminal completion, caller interruption, timeout/disconnection, or another documented stream termination.

Plain `--json` is not valid for indefinite follow because that would require buffering an unbounded array; callers that need machine streaming use `--jsonl`.

In JSONL mode, every line is a complete structured event envelope with at least:

- CLI schema version;
- command identifier;
- event type;
- task ID;
- durable/significant event sequence where applicable;
- timestamp;
- bounded event data.

The event stream does not mirror every high-frequency Codex/provider log line.

### 18.4 Task cancel

```text
portus-os task cancel <task-id>
```

The command requests durable cancellation under `docs/TASKS.md`.

It does **not** find a PID and kill it locally.

Useful precondition form:

```text
portus-os task cancel <task-id> --if-state <state>
```

The task remains `cancelling` until authoritative required backends confirm stop/termination. The CLI must not print/report terminal `cancelled` merely because the cancellation request was accepted.

If cancellation is unsupported for the required backend relationship, the command returns `unsupported` rather than pretending that a stop can be guaranteed.

### 18.5 Future task commands

Commands such as `pause`, `resume`, explicit `retry/rerun`, adoption, or manual task creation are not part of the first locked CLI surface merely because `docs/TASKS.md` permits those lifecycle concepts.

They may be added only once backend semantics and a real user/agent use case justify them.

---

## 19. `portus-os capability`

`capability` is a **discovery/inspection surface**, not a universal provider invocation proxy.

The first locked surface is:

```text
portus-os capability list
portus-os capability show <capability-id>
portus-os capability provider list
portus-os capability provider show <provider-id>
```

There is intentionally no generic:

```text
portus-os capability invoke ...
```

The agent discovers the provider/interface/skill and then uses the provider's own structured interface directly unless a separate Portus-managed workflow is required.

### 19.1 Capability list

Returns intentionally registered semantic capabilities under `docs/CAPABILITIES.md`, including bounded availability/provider summaries.

It must distinguish registered Portus capabilities from ordinary installed/native abilities that the System Index may observe.

### 19.2 Capability show

```text
portus-os capability show browser.control
```

Returns safe capability metadata such as:

- capability ID/contract version;
- available/degraded/unavailable state;
- provider registrations implementing it;
- compatible interfaces;
- associated provider skill/documentation reference;
- high-level policy/lifecycle/health relationships.

Detailed provider command syntax remains provider-skill/provider-interface territory.

### 19.3 Provider list/show

```text
portus-os capability provider list
portus-os capability provider show provider_<UUIDv7>
```

Exposes durable provider registration metadata from `docs/CAPABILITIES.md`, including:

- provider registration/type;
- scope/owner where authorized;
- software/interface/capability contract versions;
- compatibility;
- lifecycle ownership;
- health summary;
- declared semantic capabilities/interfaces/resource types;
- tombstone/removed state where historical references require it.

It does not expose raw provider-domain state or reusable credentials.

---

## 20. `portus-os policy`

The normal Master-facing policy surface is inspection/preflight focused, while the administrator has a separate root-only mutation surface locked by `docs/POLICY_IMPLEMENTATION.md`.

Locked forms:

```text
portus-os policy effective
portus-os policy check [typed action/context arguments]

portus-os policy admin show <uid>
portus-os policy admin grant <uid> <action> --effect allow|prompt|reject [typed constraints]
portus-os policy admin revoke <uid> <action> [typed constraints]
portus-os policy admin bundle set <uid> <bundle-id> --enabled|--disabled
```

Root-equivalent administrator grants additionally require `--ack-root-equivalent`.

### 20.1 Effective policy

```text
portus-os policy effective
```

Returns the caller's effective PortusOS policy summary, including safe information such as:

- effective principal;
- selected installer/admin bundle/profile where applicable;
- relevant Portus permission domains;
- important narrowing task/project/provider policy relationships;
- approval requirements/categories;
- root-equivalent warnings where the policy layer can establish them.

It must not leak another user's private policy/state by default.

### 20.2 Policy check

```text
portus-os policy check ...
```

Performs a typed policy preflight for an operation/context without performing the requested mutation.

The final action vocabulary/arguments are locked by the policy implementation mapping, not invented as arbitrary shell command strings.

The command returns one of the policy outcomes defined by `docs/POLICY.md`, conceptually:

```text
allow
prompt/approval
reject
```

with safe reason/context information.

The provider's own domain policy remains separately authoritative where applicable.

### 20.3 No self-expansion

`policy` inspection/preflight must not provide a path for Master Portus to grant itself broader authority.

`policy admin` commands are rejected unless the effective caller is UID 0 and the `portus-privd` admin protocol also authenticates peer UID 0. They mutate versioned root-owned policy atomically and never convert a non-root Master request into administrator authority.

### 20.4 Privileged-operation boundary

The policy administration commands above are an explicit exception to the normal `portusd`-backed state path: root administration crosses the separate `portus-privd` security boundary defined by `docs/POLICY_IMPLEMENTATION.md`.

Future Portus-owned typed root mutations must preserve the real requesting peer principal at `portus-privd`; no generic `portusd` field asserting `caller_uid` is accepted as identity proof.

---

## 21. `portus-os health`

`health` exposes normal runtime-owned structured component health.

Locked forms:

```text
portus-os health
portus-os health show <component-ref>
portus-os health degraded
```

`portus-os health` returns the bounded caller-visible health catalogue/summary.

`health show` returns one component/provider/source's structured health state and safe reason information.

`health degraded` returns currently degraded/unavailable components relevant to PortusOS operation.

Health queries are daemon-backed normal runtime operations.

They differ from `doctor`:

```text
health
  = what the running PortusOS runtime currently reports

doctor
  = how to diagnose the control plane even when that runtime is unavailable/broken
```

`docs/HEALTH_RECOVERY.md` now locks conservative recovery ownership and restart limits. The first CLI contract still deliberately exposes no generic `health repair --everything` command: component-owned bounded recovery, index reconciliation/rebuild, provider/task contracts, native OpenRC/Linux administration, and typed privileged operations remain the correct recovery surfaces.

---

## 22. `portus-os artifact`

The artifact domain is intentionally minimal even though `docs/ARTIFACTS.md` is now authoritative. First-ISO presentation remains metadata/locator based rather than a duplicate file-management API.

The first safe read surface is implemented as:

```text
portus-os artifact list
portus-os artifact show <artifact-id>
```

These commands expose Portus-registered artifact metadata visible to the caller.

P12 deliberately does **not** add generic `register`, `deliver`, `delete`, `forget`, `verify`, or retention-management commands. Bare `portus-os artifact` requires a read subcommand.

Ordinary files do not become Portus artifacts merely because they exist.

Task/provider operations may internally register artifacts through the deliberate registration contract in `docs/ARTIFACTS.md`.

Future explicit register/verify/forget/retention/delivery commands may be added only when a real workflow requires them; `artifact list/show` remains sufficient for the first-ISO local presentation boundary.

P12 runtime/CLI details are now concrete:

- `artifact list [--limit <1..200>] [--cursor <opaque>]` calls `artifact.list`; default page size is the common 50 and the returned cursor is surfaced in CLI metadata;
- list rows expose only bounded summary metadata: artifact ID, owner, type, confidentiality, retention, availability, integrity, optional size/display name, and registration time;
- `artifact show <artifact-id>` calls `artifact.show` and returns the full caller-visible metadata/relationship view; human output shows type/state/confidentiality/retention and the native locator, while JSON preserves the typed locator and relationships;
- private/shared/public filtering is performed by `portusd` using the authenticated Unix principal rather than a caller-supplied UID;
- neither command reads artifact bytes or proxies provider content; native/provider tooling remains the content-access path;
- `status` now reports `artifact-registry` as an implemented domain and `help --json` reports artifact `list/show` as implemented with `mutation_surface: internal_typed_only`.

---

# CLI metadata and compatibility

## 23. `portus-os version`

Human-readable version:

```text
portus-os version
```

Structured form:

```text
portus-os version --json
```

The structured result should expose at least:

- `portus_os_cli_version`;
- CLI output `schema_version`;
- supported runtime protocol version;
- build/release identifier needed for diagnostics;
- target architecture where useful.

`portus-os --version` may remain a normal concise alias/presentation convenience, but `portus-os version --json` is the stable machine-readable form.

---

## 24. Help and machine-readable command discovery

Human help uses conventional forms:

```text
portus-os --help
portus-os <domain> --help
```

Machine-readable command discovery uses:

```text
portus-os help --json
```

It returns a bounded description of the installed CLI contract, including:

- CLI version;
- output schema version;
- runtime protocol version;
- top-level commands;
- subcommands;
- stable option names/types/enums where applicable;
- supported structured-output modes;
- whether a command supports pagination, `--dry-run`, preconditions, or streaming.

Machine-readable help is useful for:

- release/compatibility validation;
- generating/checking the shipped `portus-os-cli` skill;
- external tooling;
- automated smoke tests.

It does not replace the agent-facing skill. The skill explains when/how to use commands and architectural freshness/non-duplication rules; machine help describes the installed syntax/contract.

---

## 25. CLI/runtime protocol compatibility

Every normal daemon-backed CLI request participates in explicit runtime protocol compatibility checking.

The initial hard-cutover rule is:

> `portus-os` and `portusd` must agree on the supported active runtime protocol contract before a normal operation proceeds.

When a package/update changes the contract incompatibly, the first-party CLI and daemon are updated together rather than relying on hidden compatibility shims.

On mismatch:

- no state-changing request is guessed/reformatted silently;
- the CLI returns `incompatible_protocol`;
- human output explains the installed CLI/runtime mismatch and directs diagnostics/recovery as appropriate;
- `portus-os doctor` remains available to diagnose the installation because it does not depend entirely on normal daemon-backed request success.

The exact wire version field serialization remains owned by `docs/RUNTIME.md`/implementation contracts.

---

## 26. `portus-os-cli` skill relationship

The machine-wide Codex skill is:

```text
portus-os-cli
```

It is instruction/reference content, not executable code.

The skill should be aligned with this document and the installed `portus-os help --json` contract.

It should teach:

- the non-duplication rule;
- when `portus-os` is appropriate versus Codex/native/provider tools;
- structured-output usage;
- quick System Index views and freshness/revalidation;
- task IDs/events/cancellation semantics;
- capability/provider discovery and provider-skill handoff;
- policy preflight;
- health versus `doctor`;
- common error classes and recovery patterns;
- bounded/paginated query behavior.

It must not duplicate:

- provider-specific command manuals;
- general reusable user workflows;
- the machine inventory;
- the task database;
- Rust implementation details irrelevant to using the CLI.

Independent procedures such as daily setup, project startup, review flows, or maintenance routines remain independent skills/workflows.

---

## 27. Rust implementation structure

This document locks functional separation, not final crate names.

The Rust implementation should preserve the equivalent boundaries of:

```text
protocol/wire crate or module
  stable IDs
  request/response DTOs
  output/error types where shared
  runtime protocol version types

client crate or module
  Unix socket connection
  framing
  compatibility handshake/check
  request/response handling
  event stream handling

portus-os executable
  command tree
  argument validation
  human renderer
  JSON/JSONL renderer
  exit-code mapping
  bounded doctor diagnostics
```

Shared types must not become a backdoor for duplicating daemon business logic into the client.

The exact crate names, dependency versions, and whether ordinary requests use synchronous I/O or an async runtime remain implementation decisions.

Event-following is the primary plausible reason to introduce async behavior; simple request/response commands should not acquire unnecessary runtime complexity merely because Rust supports it.

---

## 28. Security and secret handling

The CLI must obey `docs/POLICY.md` and `docs/SECRETS.md`.

Rules include:

- caller authority comes from the daemon/provider's authenticated local principal path, not a CLI-supplied username;
- the CLI must not offer a flag that impersonates another user by string value;
- raw reusable protected credentials must not enter normal CLI arguments, JSON output, errors, shell history, logs, diagnostics, or task metadata;
- protected provider operations use safe references, not exported secret values;
- structured output must apply principal filtering/redaction;
- diagnostics must use allowlisted safe fields rather than dumping configuration/process environments indiscriminately;
- approval-required responses must not include secret material merely because a human is expected to approve something.

---

## 29. First-ISO CLI boundary

This detailed CLI contract does not expand the blocking first-ISO acceptance scope in `docs/ACCEPTANCE.md`.

The first implementation should provide the subset needed to prove the already-required vertical slice, with real command implementations rather than placeholders.

At minimum, first-ISO work will need useful real paths for:

- `status`;
- `doctor` sufficient for runtime/Codex/provider/index readiness diagnosis;
- System Index quick query/show/refresh/reconciliation needed by the acceptance tests;
- capability/provider discovery for PortusBrowser/protected-provider integration;
- task inspection/cancellation/event behavior to the extent the selected first task backend requires it;
- policy/effective/preflight behavior required by the protected-provider proof;
- health reporting required by degradation/recovery proof.

Artifact commands are implemented only to the extent the first-ISO artifact acceptance flow actually requires after `docs/ARTIFACTS.md` is locked.

No first-ISO requirement is created merely because this document reserves a future-safe command name.

---

## 30. Validation requirements

Before the CLI contract is considered implementation-ready, validation must cover at least:

### Naming/non-duplication

- [ ] `/usr/bin/portus-os` is the OS CLI executable.
- [ ] No generic `portus` or `portus-cli` alias exists.
- [ ] Generic file/search/edit/shell/Git commands are absent.
- [ ] `capability` has no universal provider `invoke` command.
- [ ] Arbitrary filesystem tree/list commands are absent from `index`.

### Rust/thin-client boundary

- [ ] `portus-os` is a Rust native executable with no Python/Node runtime dependency.
- [ ] Normal daemon-backed operations use the shared typed Unix-socket client/protocol boundary.
- [ ] CLI code does not directly mutate normal Portus SQLite state.
- [ ] CLI code does not duplicate task/index/provider/policy business logic.
- [ ] `doctor` remains bounded/diagnostic.

### Structured output

- [ ] `--json` produces only valid structured stdout for supported request/response commands.
- [x] `--jsonl` produces self-contained event records for the implemented task-event stream.
- [ ] Human formatting changes do not alter JSON field contracts.
- [ ] Errors carry stable semantic codes and safe details.
- [ ] Exit-code families match the documented mapping.

### Pagination/bounds

- [ ] Large list/query commands enforce bounded default/maximum limits.
- [ ] Opaque cursors page results without exposing DB row IDs.
- [ ] Expired/invalid cursors fail clearly.
- [ ] No ordinary command unexpectedly dumps the entire System Index/task/event history.

### Identity/preconditions

- [ ] `task_...`, `provider_...`, `idx_...` and other authoritative refs remain distinct.
- [ ] Stale `idx_...` generation fails safely.
- [ ] Task cancellation can enforce `--if-state` atomically through the backend.
- [ ] Unsupported preconditions are rejected rather than emulated unsafely.

### Tasks/events

- [ ] `task list/show/events/cancel` preserve `docs/TASKS.md` semantics.
- [ ] Cancellation acceptance is distinguishable from terminal cancellation completion.
- [x] `task events --after` resumes significant event delivery by sequence and rejects resume points older than retained history.
- [x] `--follow --jsonl` streams bounded significant events without copying backend transcripts.

### Capabilities/providers

- [ ] Capability discovery distinguishes registered capabilities from ordinary native abilities.
- [ ] Provider registrations can be inspected by `provider_<UUIDv7>`.
- [ ] Provider resource/secret internals are not leaked.
- [ ] The CLI points agents toward provider-owned interfaces/skills rather than proxying commands generically.

### Policy/security

- [ ] `policy effective` reports the authenticated caller's effective policy safely.
- [ ] `policy check` does not perform the requested mutation.
- [ ] `policy admin` rejects non-root callers even if socket filesystem permissions are deliberately widened during a negative test.
- [ ] No CLI username flag substitutes for authenticated Unix peer identity.
- [ ] Cross-user private state is filtered.
- [ ] Raw reusable protected credentials never appear in CLI output/errors/diagnostics.

### Health/doctor/degradation

- [ ] `status` remains cheap and daemon-backed.
- [ ] `health` reports runtime-owned health.
- [ ] `doctor` can identify a missing/dead `portusd` without requiring a healthy daemon.
- [ ] A dead daemon produces `daemon_unavailable` for normal commands rather than hidden fallback business logic.
- [ ] Provider/index degradation can still return valid partial results when explicitly represented as degraded.

### Compatibility/help

- [ ] `version --json` exposes CLI/output/protocol versions.
- [ ] `help --json` exposes the installed machine-readable command contract.
- [ ] Incompatible CLI/runtime protocol versions fail closed with `incompatible_protocol`.
- [ ] The shipped `portus-os-cli` skill is checked against the installed command contract.

---

## 31. Decisions locked by this document

1. The first top-level CLI domains are `status`, `doctor`, `index`, `task`, `capability`, `policy`, `artifact`, and `health`, plus metadata commands `help` and `version`.
2. No generic `system`, `files`, `exec`, `shell`, `provider-invoke`, or miscellaneous utility bucket is part of the architecture.
3. Human output is default; `--json` is the stable request/response machine-readable mode and `--jsonl` is reserved for supported streams.
4. Structured CLI output uses a versioned envelope with command identifier, success/error state, command data, and bounded metadata.
5. Semantic error codes are stable and more precise than process exit codes.
6. Stable exit-code families use `0`, `2` through `10` for success, usage, daemon/protocol (including `incompatible_protocol`), not-found, authority, conflict/staleness, unavailable/unsupported, timeout, cancelled/interrupted, and internal failure families.
7. Large collections use bounded `--limit` plus opaque cursor pagination; no unbounded default result exists.
8. CLI resource identity uses authoritative subsystem references and generation-scoped `idx_...` handles rather than database row IDs/PID-only identity.
9. State-sensitive mutations use typed backend-enforced `--if-*` preconditions where supported.
10. `--dry-run` means side-effect-free validation/preflight and is exposed only where that guarantee is meaningful.
11. `status` is cheap daemon-backed operational summary; it does not become a deep diagnostic scan.
12. `doctor` is the bounded daemon-independent diagnostic exception and is not a second runtime or automatic repair engine.
13. Locked doctor groups are `runtime`, `state`, `index`, `providers`, and `codex`, with bare `doctor` aggregating supported groups.
14. Locked System Index quick views are `apps`, `windows`, `workspaces`, `displays`, `providers`, and `stale`.
15. Locked System Index operations include `query`, `show`, `topology`, `refresh`, `rescan`, `reconcile`, `rebuild`, and `status`.
16. Initial index rescan domains are `applications`, `runtime`, `providers`, and `services`.
17. `index topology` is a bounded machine-resource relationship view and never a generic filesystem tree command.
18. First task commands are `list`, `show`, `events`, and `cancel`; no generic first-contract `task create` command exists.
19. Task event streaming supports retained events, `--after`, `--follow`, and JSONL streaming of significant events.
20. `task cancel` requests durable cancellation through `portusd`; it never substitutes local PID killing for task semantics.
21. First capability commands are `list`, `show`, `provider list`, and `provider show`.
22. Capability CLI is discovery/inspection only; no universal `capability invoke` command exists.
23. Policy commands include normal `effective`/typed `check` plus root-only `policy admin show/grant/revoke/bundle set`; administrator mutation crosses `portus-privd` and root-equivalent grants require explicit acknowledgement.
24. `health`, `health show`, and `health degraded` expose normal runtime health under `docs/HEALTH_RECOVERY.md`; the first CLI deliberately adds no generic automatic-repair command.
25. Artifact CLI remains first-contract read-side `list` and `show`; deliberate registration/integrity/retention/cleanup semantics are locked by `docs/ARTIFACTS.md`, while mutation/delivery CLI remains deferred until a real workflow requires it.
26. `version --json` exposes CLI/output/protocol version information.
27. `help --json` is the machine-readable installed command contract and complements rather than replaces the `portus-os-cli` Codex skill.
28. Initial CLI/runtime compatibility is hard-cutover/explicit: incompatible active protocol versions fail with `incompatible_protocol` rather than hidden shims.
29. The `portus-os-cli` skill is aligned to this contract but remains instruction/reference content and not executable code.
30. Rust implementation preserves protocol/client/executable separation. P4 concretely uses the existing `portus-protocol`, `portus-client`, and `portus-os` crates with synchronous request/response I/O; async remains unnecessary until an actual streaming/concurrency requirement justifies it.
31. This CLI contract does not expand first-ISO blocking scope beyond `docs/ACCEPTANCE.md`.

---

## 31.1 P4 concrete implementation decisions

The following implementation details are now fixed for the P4 foundation:

- parser: `clap` in the native Rust `portus-os` crate;
- CLI output schema version: `1`;
- normal daemon transport: synchronous `portus-client` request/response over the canonical local socket;
- production socket selection: fixed `/run/portus/portusd.sock`; there is no user-facing alternate-socket option;
- global structured modes: human default, `--json`, and `--jsonl` with mutual exclusion; non-streaming commands reject JSONL, while P8 enables JSONL for `task events --follow`;
- global request timeout option: `--timeout-ms`, default `30000`, minimum `100`, maximum `300000`;
- first common pagination constants: default `50`, maximum `200`, opaque cursor; these constants are only surfaced by commands that actually become pageable in later phases;
- P4 structured `status` data contains the current `runtime.status` result plus an explicit list of currently implemented runtime domains;
- P4 originally implemented a minimal health projection from `runtime.status`/`state.status`; P11 hard-cuts that implementation to the typed `health.list`/`health.show`/`health.degraded` runtime contract;
- `doctor` returns a bounded list of structured checks and remains successful when it successfully diagnoses an unavailable component; P11 adds optional `--bundle <path>` allowlisted JSON evidence with create-new/no-overwrite semantics;
- `version --json` fields include CLI version, output schema version, runtime protocol version, build ID, target architecture, and target OS;
- `help --json` reports installed top-level commands, implementation status, structured-output support, common option limits, doctor-domain values, the P11 diagnostic-bundle option, and implemented health subcommands;
- P4 originally reserved `index`, `task`, `capability`, `policy`, and `artifact`; P5-P12 have now implemented capability, index, task/events, policy, health depth, and the artifact read surface;
- `health show` and `health degraded`, reserved syntactically since P4, are implemented by P11 and use one typed daemon-backed health catalogue rather than local CLI inference;
- the `portus-os-cli` Codex skill remains explicitly deferred until the executable has been tested on the actual PortusOS/Linux runtime.

### P8 task-event streaming implementation

P8 implements the previously reserved task-event stream without changing the ordinary request/response command contract:

- `task events --follow` is handled by the CLI application streaming path rather than the one-response command renderer;
- the transport uses one long-lived local runtime connection and versioned `event` / `end` / `error` stream frames tied to the original request ID;
- every delivered event comes from the durable task-event sequence; the in-memory runtime hub is only a wake-up path;
- `--follow --jsonl` emits one complete CLI-schema event record per line;
- human follow writes each event incrementally;
- plain indefinite `--follow --json` is rejected;
- an `--after` sequence older than retained history fails with `stale_resource` rather than skipping missing history;
- direct internal `execute()` continues to reject follow because streaming ownership belongs to the application/writer path;
- no task transcript/log streaming, generic task creation, shell execution, or generic runtime log subscription was added.

The distribution-neutral Linux Unix-socket stream gate now passes on the recorded disposable Linux VM: a real peer-authenticated connection is abandoned, task cancellation commits while disconnected, and reconnect from the last durable sequence replays the missing events in order and reaches the terminal frame. Installed OpenRC/audit-path behavior remains a separate target-system gate.

### P11 health and diagnostic implementation

P11 completes the locked health surface without adding a generic repair command:

- bare `portus-os health` calls `health.list` and returns the fresh bounded caller-visible catalogue plus a degraded summary bit;
- `portus-os health show <component-ref>` calls `health.show` and returns one visible typed observation or `not_found` across the principal boundary;
- `portus-os health degraded` calls `health.degraded` and returns only `degraded`/`unavailable` components; `unknown` remains distinct rather than being silently treated as failure;
- human output shows component reference, health state, reason code, and recovery disposition; JSON preserves the full typed observation including safe details;
- no `health repair`, `health restart`, arbitrary service-control, or provider invocation command exists;
- `portus-os doctor [runtime|state|index|providers|codex]` remains daemon-independent; bare `doctor` is the aggregate form;
- `doctor --bundle <path>` writes the same allowlisted report as schema-versioned JSON to a **new** file only, caps encoded evidence at 64 KiB, syncs the file, and refuses overwrite rather than copying databases/configuration/logs/environments into an evidence package;
- P12 now provides an explicit private/temporary first-party diagnostic-bundle registration shape, but `doctor` remains daemon-independent and does not silently require `portusd` to register the file it creates; external delivery remains deferred.

The real Linux native-resource/OpenRC recovery behavior remains an installed-system gate and is not simulated by the CLI on Windows.

### P9 policy CLI implementation

P9 implements the locked policy surface without collapsing the normal runtime and root security boundary:

- `policy effective` calls `portusd` and returns only the authenticated caller's effective policy;
- `policy check <action> [--resource <resource>]` calls `portusd`, is side-effect-free, and returns typed `allow` / `prompt` / `reject` decision data;
- `policy admin show <uid>` uses the separate `portus-privd` admin socket;
- `policy admin grant <uid> <action> --effect allow|prompt|reject [--resource ...] [--ack-root-equivalent]` uses the admin socket and cannot use the normal `portusd` transport as an authority shortcut;
- `policy admin revoke <uid> <action> [--resource ...]` and `policy admin bundle set <uid> <bundle-id> --enabled|--disabled` likewise use `portus-privd` directly;
- the installed CLI has no username/UID impersonation option for normal policy inspection/preflight and no generic `policy exec`, `shell`, or privileged command passthrough;
- `status`, bare `health`, and `help --json` now expose policy implementation/health and the runtime-versus-admin transport split;
- `index rebuild` remains authenticated UID-0 in the current P6 implementation until a later explicitly approved typed policy mapping replaces that narrow temporary authorization; P9 does not silently rewrite it into a generic privileged action.

The Linux admin transport path is source-complete and cross-target checked, but real peer credentials and root socket enforcement remain Linux acceptance evidence.

### P5 capability CLI implementation

P5 implements the locked read-only provider discovery surface:

- `capability list` and `capability provider list` use the common default limit `50`, maximum `200`, and opaque cursors; daemon `next_cursor` is surfaced as CLI `meta.next_cursor`;
- `capability show <capability-id>` returns the registered capability and caller-visible provider implementations;
- `capability provider show <provider_<UUIDv7>>` returns safe registration, interface, capability, resource-type, lifecycle, compatibility, health, and skill metadata;
- historical removed provider IDs remain showable with `removed_at_ms` plus safe tombstone/successor metadata; active list commands continue to return only active registrations;
- normal runtime reads are principal-filtered by `portusd`/state rather than by a caller-supplied username;
- `status` and bare `health` now include provider-registry health without adding a separate health probe RPC;
- `help --json` marks the capability domain and its four discovery commands implemented;
- no `capability invoke`, provider install/remove/reconcile/refresh command, or provider-domain command forwarding was added.

---

## 32. Remaining dependent implementation decisions

The CLI architecture/command contract and P4 foundation are locked, but later implementation work still needs to define:

- generated shell completion strategy if later shown useful;
- exact CLI output `data` schemas for domains still unimplemented after P5;
- additional common `meta` fields and timestamp serialization where later commands actually need them;
- resource-specific pagination limits where the P4 common 50/200 defaults are not appropriate;
- cursor serialization/lifetime/storage strategy;
- exact query-filter flag names/enums under `index query`;
- exact topology depth/relationship limit options;
- exact `index rebuild` authorization/preflight rules;
- action-specific policy constraint flags beyond the current typed action + optional resource shape when later Linux adapters require richer reviewed schemas;
- exact health component/reason schemas after `docs/HEALTH_RECOVERY.md`;
- exact artifact IDs/fields/mutations after `docs/ARTIFACTS.md`;
- exact event stream reconnect/heartbeat/timeout behavior;
- exact command-specific `--if-*` generation/version flags beyond the locked task-state example;
- exact event-stream transport design if/when a later streaming command requires behavior beyond the P4 synchronous request/response client;
- exact skill packaging/content tests for `/etc/codex/skills/portus-os-cli/`;
- exact first-ISO subset implemented in each phase.

Those decisions must preserve this command tree, non-duplication boundary, structured-output contract, thin-client/runtime ownership split, and hard failure on incompatible protocol semantics.

---

## 33. Defining statement

> `portus-os` is a small, typed, Rust-native control client for PortusOS-owned operating-system coordination. It exposes stable bounded commands for status, diagnostics, the live machine graph, durable tasks, registered capabilities, policy preflight, artifacts, and health without recreating Codex, Linux, Git, provider-domain CLIs, or the `portusd` runtime. Human output is convenient; structured JSON/JSONL is the automation contract; stale state, policy boundaries, degraded sources, and protocol incompatibility are represented explicitly rather than guessed around.
