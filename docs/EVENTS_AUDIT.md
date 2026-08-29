# PortusOS Events and Audit

**Status:** Authoritative first-ISO events/audit contract; P8 source implementation and distribution-neutral Linux stream disconnect/replay proof complete, installed audit/OpenRC evidence pending
**Scope:** Significant durable events, live local event delivery, security audit records, retention/redaction boundaries, and their relationship to `portusd`, `portus.db`, and `portus-os`

## 1. Purpose

PortusOS needs durable lifecycle evidence and security-relevant audit evidence without turning the runtime into a generic logging platform or adding an external message broker.

The first-ISO design therefore separates three mechanisms:

```text
durable significant event history
    -> portus.db / significant_events

live subscriber wake-up
    -> bounded in-memory portusd EventHub

security audit evidence
    -> bounded JSONL files under /var/log/portus/audit/
```

These mechanisms are related but are not interchangeable.

- Significant events describe meaningful lifecycle/state changes for Portus-owned objects.
- The live hub wakes subscribers that new durable data exists; it is not authoritative history.
- Audit records describe security-sensitive actions/results and remain outside generic Portus SQLite state.

Ordinary daemon logs, backend stdout/stderr, Codex transcripts, provider traces, browser content, and high-frequency telemetry are not significant events or audit payloads merely because they exist.

## 2. Significant events

The shared event object kinds are:

```text
task
provider
policy
runtime
index
artifact
health
privilege
protected_api
visual
```

A significant event contains:

- object kind;
- stable object reference;
- monotonically increasing sequence within that object identity;
- optional authenticated principal attribution;
- bounded event kind;
- optional bounded reason code;
- optional bounded source reference;
- optional bounded safe summary;
- bounded structured `safe_data`;
- occurrence timestamp.

The implementation limits are:

```text
retained events per object     512
event page maximum             200
event kind                     128 bytes
reason code                    256 bytes
object/source reference        512 bytes
safe summary                  1024 bytes
serialized safe_data          4096 bytes
```

Embedded NUL/newline/carriage-return content is rejected in bounded text fields.

The 512-event rule is an event-count retention bound, not the final time-based task-retention policy. Later health/storage work may define how long terminal task records and other durable objects remain before object-level pruning, but it must preserve explicit gap semantics for event history.

## 3. Schema-v6 hard cutover

P8 advances `portus.db` to schema v6.

Migration v6 replaces the P7 task-only event table with the common `significant_events` table and migrates existing P7 task history into the new representation. The legacy `task_events` table is not kept as a parallel compatibility store.

The migration is backup-required under the normal state migration policy.

Task lifecycle remains task-owned. P8 generalizes only the event storage contract so later provider, policy, artifact, health, privilege, and protected-API phases do not create incompatible private event tables.

Security audit records are **not** stored in `portus.db`.

## 4. Retention, replay, and gaps

Event sequence numbers are monotonically increasing per `(object_kind, object_ref)`.

When an append would exceed 512 retained events for that object, the oldest events are pruned transactionally.

Queries return enough retained-range information to distinguish:

- no events after the requested sequence;
- a normal retained page;
- more retained events after the current page;
- a requested resume sequence that predates retained history.

A stale resume must never silently jump forward. For task event RPC/stream use, a resume point older than retained history fails with `stale_resource` and includes safe retained/latest sequence evidence.

This gives reconnecting clients an explicit choice to resynchronize from current task/object state rather than treating missing history as if it had been delivered.

## 5. Live event delivery

`portusd` uses a bounded in-memory event hub only as a wake-up mechanism.

The hub:

- supports object-filtered subscriptions;
- uses bounded subscriber queues;
- never blocks the event-producing task/state transition because a subscriber is slow;
- counts lagging/disconnected subscribers;
- does not claim to be durable history.

The authoritative replay source remains SQLite. After a wake-up, the stream rereads the corresponding durable sequence from `portus.db` before sending it.

Therefore:

```text
dropped wake-up != dropped durable event
```

A lagging subscriber may miss in-memory notifications, but it can recover from the durable sequence until retention has legitimately pruned that history. If retention has pruned it, the gap is explicit.

No Redis, NATS, RabbitMQ, Kafka, or other broker is part of the first-ISO contract.

## 6. Task event streaming

The first live event consumer is:

```text
portus-os task events <task-id> --follow
portus-os task events <task-id> --follow --jsonl
```

The Linux runtime uses one long-lived local Unix-socket connection. Internally, the stream request is `task.events.follow`; after the initial request that connection carries versioned task-event stream frames rather than ordinary request/response traffic.

Frame kinds are:

```text
event
end
error
```

Every frame carries the runtime protocol version and original request ID. Event frames carry one durable task event. End frames carry terminal task state. Error frames carry one typed semantic error.

The CLI behavior is:

- human mode writes each event incrementally;
- `--jsonl` writes one complete machine-readable event record per line;
- plain `--json` with indefinite `--follow` is rejected rather than buffering an unbounded JSON array;
- retained non-follow queries remain bounded by `--limit`;
- caller interruption/transport timeout/disconnect remains distinguishable from a successful terminal stream end.

The stream is significant-event delivery, not a mirror of child stdout/stderr or Codex/provider log lines.

## 7. Audit record contract

Audit record schema version 1 contains only allowlisted top-level fields:

```text
schema_version
actor
domain
action
result
reason_code
target_ref?
task_id?
request_id?
occurred_at_ms
```

Actor kinds are:

```text
principal -> authenticated numeric UID/GID
system    -> no principal attached
```

Audit domains match the first common subsystem domains:

```text
task
provider
policy
runtime
index
artifact
health
privilege
protected_api
visual
```

Results are:

```text
succeeded
failed
denied
approval_required
cancelled
interrupted
```

The audit API deliberately has no generic payload/body/header/message/metadata-map field.

It must not record:

- request payloads merely for convenience;
- Authorization/Cookie/Proxy-Authorization values;
- reusable credentials or tokens;
- argv/environment dumps;
- arbitrary command output;
- provider request/response bodies;
- Codex transcripts;
- browser page/DOM contents;
- generic diagnostic blobs.

Secret-like markers in bounded audit text are rejected before serialization.

## 8. Audit storage and rotation

The first `portusd` audit sink is:

```text
/var/log/portus/audit/portusd.jsonl
```

It is intentionally separate from `portus.db` and ordinary project/workspace files.

Current implementation bounds:

```text
maximum encoded record        4096 bytes
maximum action                 128 bytes
maximum reason                 256 bytes
maximum target reference       512 bytes
active audit file                1 MiB
rotated archives                    4
```

Rotation is local and bounded. Exact installed-system ownership/mode and OpenRC log-directory creation remain Linux packaging/service-graph work and must be validated there; P8 does not invent those package-specific details on the Windows host.

## 9. Attribution and audit failure semantics

For authenticated runtime requests, the actor comes from the kernel-authenticated/runtime principal supplied to dispatch, never from caller JSON identity fields.

System-initiated work such as provider reconciliation uses an explicit system actor.

P8 wires initial audit production for security-relevant operations already present before P9:

- `task.cancel`;
- `index.rebuild`, including denied non-root attempts;
- provider reconciliation success/failure.

P9 extends the same contract with `policy.check`, `portus-privd` typed privilege-use decisions, and root-only policy-admin operations. `portus-privd` writes its bounded audit stream separately at `/var/log/portus/audit/portus-privd.jsonl`; successful root-equivalent administrator changes are explicitly distinguishable by safe reason metadata. No privileged request body, command string, argv, environment or reusable secret field is added to the audit schema.

P10 adds `portus-apid` protected-use and credential-administration audit records at `/var/log/portus/audit/portus-apid.jsonl`. Records carry only allowlisted actor/request/reference/operation/result metadata; provider request/response bodies and reusable credential material never enter the audit contract. Security-sensitive protected requests and secret-store mutations require a successful bounded audit preflight before the upstream/store side effect, and then emit a separate secret-free completion record.

Later phases add their own typed audit actions to the same contract rather than inventing private log schemas.

An audit-write failure must not silently change the semantic result of an operation after that operation has already committed. `portusd` tracks ordinary audit-sink failures separately and exposes their count in runtime status. P10 deliberately uses stricter precondition semantics for protected upstream/store side effects: if its audit preflight cannot be written, the protected side effect does not occur. This is an explicit security boundary, not an accidental filesystem-I/O consequence.

## 10. Relationship to logs, health, and artifacts

Events, audit, logs, health, and artifacts remain separate concepts.

- Significant events: durable meaningful lifecycle evidence.
- Audit: bounded security/action evidence.
- Ordinary logs: implementation diagnostics, independently bounded/redacted.
- Health: current component condition and recovery disposition.
- Artifacts: deliberately registered outputs/resources.

A diagnostic log line does not become an event merely because it is useful during debugging. An event does not become audit merely because it has a principal. Audit files do not become artifacts unless a later explicit diagnostic/export operation deliberately registers an allowed redacted bundle.

## 11. P8 host-safe implementation evidence

P8 host-safe validation proves:

- common event wire types and strict stream-frame shapes;
- schema-v6 migration and preservation of P7 task-event history;
- removal of the parallel legacy task-event table;
- ordered event sequencing and 512-event retention;
- explicit stale/gap detection;
- bounded event metadata;
- object-filtered wake-up delivery;
- lagging-subscriber behavior without blocking producers;
- audit allowlisting/redaction and bounded rotation;
- authenticated principal attribution and system actor attribution;
- audit-write failure visibility without changing operation result;
- incremental CLI human/JSONL stream rendering through a host-testable transport abstraction;
- no generic task creation/shell/log-stream surface added.

The host-safe workspace passes format, locked compile, warnings-denied Clippy, and all currently active host-safe tests.

## 12. Linux execution status

Completed on the recorded disposable x86_64 Linux/VMware environment:

- [x] compile and execute the Unix-domain `task.events.follow` server/client path;
- [x] authenticate the streaming connection from real kernel peer credentials;
- [x] abandon a live connection, reconnect from the last durable sequence, replay missed task events in strict order, and receive the terminal frame;
- [x] exercise that stream together with the real P7 managed-process cancellation path.

Remaining installed Artix/OpenRC evidence:

- [ ] validate `/var/log/portus/audit/` ownership, modes, creation and rotation under the actual service account;
- [ ] validate audit persistence across daemon/service restart;
- [ ] repeat disconnect/lag behavior if the installed OpenRC/service environment exposes materially different behavior;
- [ ] complete the target-specific P7 restart/process-generation reconciliation path.

The distribution-neutral P8 Linux stream gate is complete. The unchecked items above remain installed-system acceptance, not missing source implementation.

## 13. Locked decisions

1. Significant durable events live in `portus.db`; audit records do not.
2. P8 hard-cuts P7 task events into one common significant-event table rather than maintaining parallel stores.
3. Event ordering is per object identity, not one machine-global ordering promise.
4. First event retention is 512 records per object and first page maximum is 200.
5. Retention gaps are explicit; stale resume never silently skips history.
6. The live `EventHub` is a bounded wake-up path only; SQLite remains authoritative replay.
7. Live task event delivery uses the existing local Unix runtime connection; no external broker is introduced.
8. `--follow --jsonl` is incremental and one-record-per-line; indefinite follow is not buffered into plain JSON.
9. Audit records use a closed allowlisted schema with no generic payload map.
10. Audit actor identity is authenticated principal or explicit system actor, never caller-supplied username/UID JSON.
11. Audit storage is bounded/rotated under `/var/log/portus/` and separate from ordinary logs/state/workspaces.
12. Backend stdout/stderr, provider traces, Codex transcripts and other high-volume logs are not event/audit streams.
13. P9 and later security phases extend this foundation rather than creating bespoke audit formats.
