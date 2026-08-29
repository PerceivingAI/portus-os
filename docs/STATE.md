# PortusOS State and Storage Architecture

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative PortusOS mutable-state architecture
**Storage engine:** SQLite
**Scope:** Portus-owned durable, derived, transient, provider-owned, configuration, log, and secret state boundaries

## 1. Purpose and authority

This document is authoritative for PortusOS-owned mutable state and local state storage.

It defines:

- the SQLite database decision;
- what state PortusOS owns;
- durable versus rebuildable state;
- transient runtime state;
- provider-owned state;
- configuration/state/log/secrets separation;
- write ownership;
- schema migration and transaction rules;
- corruption and recovery behavior;
- retention and cleanup boundaries.

`docs/RUNTIME.md` is authoritative for the runtime process and IPC model. Subsystem documents define their detailed schemas while remaining inside the state boundaries defined here.

## 2. Locked storage decision

PortusOS uses SQLite as its initial and primary local transactional store.

The canonical database path is:

```text
/var/lib/portus/state/portus.db
```

The first implementation uses **one PortusOS database**, not separate databases for tasks, index, capabilities, artifacts, and approvals.

Separate databases may be introduced only if a demonstrated isolation, lifecycle, performance, or recovery requirement makes them materially better. They are not the default architecture.

## 3. Why SQLite fits PortusOS

PortusOS is a local single-machine control plane. It does not require a distributed database.

SQLite provides the properties needed here:

- transactional local state;
- crash-safe commits;
- simple deployment without another service;
- schema migration support;
- strong relational constraints;
- inspectable recovery tooling;
- low operational overhead;
- adequate concurrency when `portusd` is the normal writer.

PortusOS must not add PostgreSQL, MySQL, Redis, or another database/service merely to hold local control-plane metadata.

## 4. State classes

PortusOS must distinguish four state classes instead of treating everything in SQLite as equally authoritative.

### 4.1 Durable Portus-owned truth

This is intentional state that PortusOS owns and cannot always recreate correctly from a live system scan.

Examples include:

- Portus task identity and lifecycle metadata;
- task/requester/project relationships;
- task/process relationships that must survive client disconnect;
- capability/provider registrations and Portus-owned lifecycle metadata;
- Portus-owned application-instance relationships;
- artifact identity and task relationships;
- approval records and approval lifecycle metadata;
- explicit Portus relationships between otherwise independent resources;
- agent-created system-index annotations;
- schema/migration metadata;
- significant durable runtime events required for lifecycle/recovery.

Durable state must not be silently discarded merely because it is inconvenient to repair.
User-scoped durable records must carry an owning/effective principal where privacy or authority depends on the Linux user boundary. This includes, as applicable, tasks, artifacts, annotations, session relationships, user-owned application/provider relationships, and other records that one Master Portus user must not be able to inspect merely because another user created them.

System-wide records may remain machine-scoped where appropriate, but any user-private fields or relationships must still respect principal isolation.


### 4.2 Derived or rebuildable state

This state is a cached observation or materialized machine view whose authoritative source remains Linux or a capability provider.

Examples include observations of:

- installed applications/packages;
- running processes;
- OpenRC service state;
- graphical sessions;
- windows and workspaces;
- selected devices and mounts;
- provider health observations;
- provider-owned resource references;
- freshness and scan metadata.

Derived state may be stored in SQLite for query performance and relationship reconstruction, but it must be identifiable as derived and rebuildable.

Deleting or rebuilding derived index data must not destroy durable Portus-owned task, artifact, approval, capability, or annotation truth.

### 4.3 Transient runtime state

State that has value only while the current runtime is alive belongs in memory or `/run/portus/`, not in durable database rows merely because persistence is easy.

Examples include:

- current Unix-socket connection state;
- live subscriber lists;
- short-lived locks;
- current in-flight request objects;
- temporary IPC buffers;
- runtime PIDs and socket filesystem entries where persistence would be misleading.

Transient state is expected to disappear after reboot.

### 4.4 Provider-owned or external authoritative state

PortusOS must not duplicate complete application-domain state simply because it integrates with the provider.

Examples:

- Codex transcripts and complete Codex runtime state remain Codex-owned;
- Git history remains Git-owned;
- project file contents remain filesystem/project-owned;
- PortusBrowser tab/page/DOM/browser-policy state remains PortusBrowser-owned;
- browser credentials and authenticated-session internals remain provider/keyring/browser-owned;
- OpenRC service truth remains OpenRC/Linux-owned.

PortusOS may store stable references, relationships, observations, and Portus-owned metadata about those resources.

## 5. Source-of-truth rule

Each record must have a clear authority class.

The model is:

```text
Portus durable record   -> PortusOS authoritative
Linux/provider state    -> Linux/provider authoritative
Portus index observation-> cached/derived observation
Transient runtime state -> current runtime only
```

PortusOS must never present stale cached state as though it had replaced the underlying authoritative system.

## 6. Database ownership and access

During normal operation, `portusd` is the sole owner of PortusOS database writes.

The Portus OS CLI communicates through `portusd` for normal state mutations and must not directly edit `portus.db`.

Diagnostic and recovery tooling may inspect the database when necessary, but raw database mutation is not a normal user or agent API.

Filesystem ownership and permissions for `/var/lib/portus/state/` follow the account model locked in `docs/POLICY.md`.

Because one system `portusd` may serve multiple Master Portus users, database access control is enforced through the daemon using the caller principal derived from Unix peer credentials. A single physical SQLite file does not imply that every local user may inspect every row.

User-scoped records must be filtered/authorized by owning principal; cross-user private access requires explicit administrator authority. Root/admin recovery may inspect state as required to repair the system.

The database must not be directly writable by Master users or ordinary project agents merely because they can operate files elsewhere on the machine.

## 7. SQLite operational baseline

The implementation must use SQLite with:

- WAL journaling for normal runtime operation;
- foreign-key enforcement;
- explicit schema versioning;
- transactional migrations;
- bounded busy/wait behavior rather than indefinite blocking;
- integrity checking available through diagnostics;
- crash-safe transaction boundaries.

Exact PRAGMA values and performance tuning are implementation details and must be justified by testing rather than copied as magic constants.

## 8. Conceptual database domains

The first database is expected to contain relational domains such as:

```text
schema/migrations
runtime metadata
tasks
significant events
capabilities/providers
application-instance relationships
resource relationships
artifacts
approvals
index observations
index annotations
health/degradation observations
```

This remains the domain map rather than a promise that every later subsystem schema is finished. The active implementation is currently **schema v8**: P1/P2 established the common state engine, P5 introduced the provider registration/interface/capability/resource/tombstone model in migration v3, P6 migration v4 added the System Index source/generation/health model, P7 migration v5 added principal-scoped project/session metadata, durable task lifecycle/result fields, attempts, task-event data, and managed/associated execution relationships, P8 migration v6 hard-cuts the task-only event table into common per-object `significant_events` storage while preserving existing P7 task history, P11 migration v7 normalizes the original loose health/recovery skeleton into typed current health observations plus bounded recovery-attempt history, and P12 migration v8 hard-cuts the preliminary artifact skeleton into normalized artifact metadata, exact task/provider relationships, principal grants, holds, cleanup ownership and tombstones. Security audit records remain deliberately outside `portus.db` under the bounded audit sink defined by `docs/EVENTS_AUDIT.md`.

P11 health state stores only bounded, non-secret evidence: at most 128 caller-visible current components per health response, optional owner UID/GID, finite component/state/reason/recovery vocabularies, a short safe summary/source/generation, at most 16 bounded safe-detail fields, last-healthy evidence, and recovery-attempt count. Recovery history is capped at 32 records per component and seven days. Significant health changes use the common P8 event store and receive a 30-day age bound in addition to the common 512-events-per-object limit. Health/recovery records never store request bodies, environments, reusable credentials, provider response bodies, or generic telemetry.

P12 artifact state likewise stores **metadata only**. Active rows contain immutable artifact ID, owner, coarse type/confidentiality/retention/availability/integrity, one normalized filesystem or exact provider-resource locator, optional SHA-256/size/media/project/display metadata, cleanup ownership and observation timestamps. Separate normalized tables carry task relationships, provider-generation relationship, explicit shared-principal grants and holds. Artifact file/provider bytes are never copied into SQLite. Safe metadata is bounded to 16 fields / 8 KiB JSON and secret-like keys are rejected; forgetting registration leaves only a minimal tombstone and never deletes content as a side effect.

## 9. What must not be stored in `portus.db`

The Portus database must not become a general dumping ground.

It must not contain:

- raw reusable secrets;
- API keys;
- passwords;
- private SSH keys;
- password-manager vault contents;
- browser credential-store contents;
- Codex authentication tokens;
- complete Codex transcripts;
- arbitrary project files or source-code copies;
- Git history;
- large generated artifacts or binary blobs when a filesystem/resource locator is sufficient;
- complete browser pages or DOM snapshots merely for system indexing;
- unbounded command stdout/stderr;
- high-frequency telemetry with no defined product use;
- a generic filesystem-content index.

A database record may reference a protected external resource without embedding the secret or large content itself.

## 10. Configuration, state, runtime, logs, and secrets are separate

PortusOS uses explicit storage boundaries:

```text
/etc/portus/              administrator-controlled configuration
/var/lib/portus/state/    mutable Portus-owned durable/derived state
/run/portus/              transient runtime sockets/PIDs/state
/var/log/portus/          bounded logs and audit output
protected secret stores   credentials/recovery material outside generic Portus state
```

Configuration must not disappear into opaque SQLite rows when it is intended to be administrator-controlled and reviewable.

Examples that remain configuration/files rather than general database state include:

- system policy definitions;
- package/build source contracts;
- capability/provider manifests;
- daemon configuration;
- administrator-selected defaults.

## 11. Secrets exclusion

SQLite is not a Portus secrets store.

Raw secret values must not enter:

- `portus.db`;
- generic task payloads;
- event payloads;
- artifact metadata;
- index observations;
- annotations;
- ordinary logs or diagnostics.

Secret-backed operations may store non-secret metadata such as:

- stable credential/capability reference such as `openai/main`;
- provider identifier;
- safe account/display label;
- protected-versus-Master-readable classification metadata where useful and non-sensitive;
- which capability/operation was invoked;
- availability/health/revocation state;
- success/failure;
- policy/approval association;
- timestamps;

They must not persist the reusable secret value.

The complete protected-credential and secret-backed-operation contract belongs to `docs/SECRETS.md`.

## 12. Transaction boundaries

A Portus operation that mutates several Portus-owned records and must appear atomic must use one SQLite transaction whenever all of those records are in the same database.

Examples may include:

- creating a task plus its initial lifecycle event;
- registering an artifact plus its task relationship;
- changing an approval state plus the corresponding durable event;
- updating a set of derived index records during one reconciliation unit.

External Linux/provider actions cannot be made atomic merely by wrapping database writes in a SQLite transaction. Managed workflows that cross the database and external systems must explicitly define preflight, expected state, compensation/rollback, and recovery rather than pretending a database commit controls the external world.

## 13. Schema versioning and migrations

The database has one explicit schema version and an ordered migration history.

Rules:

1. `portusd` checks the schema before becoming ready.
2. Migrations execute transactionally whenever SQLite permits.
3. A failed migration prevents normal state-mutating readiness rather than continuing against a partially understood schema.
4. Destructive or non-trivial migrations require a pre-migration database backup.
5. Migration code must be versioned with the PortusOS runtime that understands the resulting schema.
6. Downgrade compatibility is not assumed unless explicitly designed and tested.
7. A migration must never silently drop durable Portus-owned state merely to make an upgrade succeed.

PortusOS follows a hard-cutover development model; compatibility shims should not accumulate without an explicit release-support requirement.

## 14. Database backup and restore

Full automated machine backup remains outside the first ISO attempt, but the runtime needs minimal database protection.

Required state behavior includes:

- a consistent SQLite backup before destructive/non-trivial schema migrations;
- a documented local database backup/restore mechanism;
- backups stored outside the live database file path;
- backup retention bounded by policy;
- secret-exclusion rules applying equally to backups;
- restore validation before normal daemon readiness.

A database backup is not a substitute for future system/project/artifact backup architecture.

## 15. Corruption and recovery

`portus-os doctor` must be able to detect database-open or integrity failures even when `portusd` cannot start normally.

Recovery rules:

- do not silently delete or recreate a corrupt database containing durable Portus-owned truth;
- preserve the failed database as recovery evidence;
- distinguish corruption of rebuildable derived state from corruption affecting durable Portus records;
- derived index observations may be discarded and regenerated when their durable boundaries are intact;
- durable-state corruption produces an explicit degraded/terminal state requiring a documented repair/restore path;
- recovery attempts must not overwrite the last available evidence before a backup/copy is preserved.

The complete operator recovery procedure belongs to `docs/HEALTH_RECOVERY.md`.

## 16. Rebuild and reconciliation

The system index must support rebuilding derived observations from authoritative sources.

A rebuild must preserve unrelated durable Portus-owned state, including at least:

- tasks;
- approvals;
- artifacts;
- capability registration metadata that is Portus-owned;
- explicit relationships that cannot be recreated safely;
- agent annotations.

Reconciliation updates observations and relationships while preserving the distinction between observed state and intentional Portus state.

## 17. Retention and cleanup

Every potentially growing state domain must have a bounded retention or cleanup rule.

This applies particularly to:

- task events;
- historical health/degradation observations;
- stale derived index records;
- old migration backups;
- completed task metadata;
- delivery/attempt metadata when artifact delivery is implemented.

Retention must not erase records still required by an active task, pending approval, retained artifact, recovery workflow, or audit policy.

Exact retention periods belong to the relevant subsystem/policy documents and must not be invented globally without need.

## 18. Events and audit

Significant runtime lifecycle events may be stored transactionally in SQLite when they are part of Portus-owned state and recovery.

Audit output is a distinct concern. The project may use protected bounded audit logs under `/var/log/portus/` while maintaining references or lifecycle state in SQLite.

The database must not become an unbounded duplicate of every log line or command output.

## 19. Artifacts and large data

Artifact bytes remain in approved filesystem or provider storage.

SQLite stores metadata and relationships such as:

- artifact ID;
- task ID;
- path or provider locator;
- type;
- size;
- checksum;
- classification/confidentiality metadata;
- retention state;
- delivery state when implemented.

Large file contents do not belong in SQLite merely because SQLite can store blobs.

## 20. Failure isolation

Loss of the Portus database or an unavailable `portusd` must not make native Linux unrecoverable.

The computer must still retain independent access to:

- Linux files and shell;
- SSH;
- OpenRC;
- Codex-owned sessions/state where otherwise healthy;
- Git/project data;
- provider-owned state;
- live-ISO recovery.

Portus state may be essential to PortusOS coordination, but it is not allowed to become the sole container for unrelated underlying system truth.

## 21. Decisions locked by this document

The following are now locked:

1. SQLite is the PortusOS local transactional state store.
2. The first implementation uses one database at `/var/lib/portus/state/portus.db`.
3. `portusd` is the normal sole writer; the Portus OS CLI does not directly mutate SQLite.
4. SQLite uses WAL mode, foreign keys, explicit schema versioning, and transactional migrations.
5. Portus-owned durable truth, rebuildable observations, transient runtime state, and provider-owned state are distinct classes.
6. Derived system-index data must be rebuildable without erasing unrelated durable state.
7. Administrator configuration remains in `/etc/portus/`, not hidden in the operational database.
8. Transient runtime state belongs under `/run/portus/` or memory.
9. Logs/audit remain bounded and separate from general operational state.
10. Raw reusable secrets are prohibited from the generic Portus database.
11. Provider-domain state, Codex transcripts, Git history, project contents, large artifact bytes, generic filesystem contents, and unbounded command output are not duplicated into SQLite.
12. Non-trivial/destructive schema migration requires a pre-migration backup.
13. Corrupt durable state is never silently reset; derived state may be rebuilt only when its durable boundaries are preserved.
14. User-scoped Portus records carry principal ownership where needed and one Master Portus user cannot inspect another user's private Portus state by default.

## 22. Remaining dependent decisions

Later documents still need to define:

- task retention/pruning, measured admission/timeout/retry budgets, and Linux process-generation reconciliation beyond the implemented P7 schema-v5 task contract (`docs/TASKS.md`);
- later System Index event/cadence/retention/performance details beyond the implemented P6 schema-v4 observation/source/relation contract (`docs/SYSTEM_INDEX.md`);
- later provider-specific extensions beyond the provider registry records introduced in migration v3 (`docs/CAPABILITIES.md`);
- artifact schema/retention (`docs/ARTIFACTS.md`);
- approval/policy records (`docs/POLICY.md`);
- recovery commands and diagnostic bundle behavior (`docs/HEALTH_RECOVERY.md`);
- exact update implementation and deferred future general-backup/rollback mechanisms under the locked `docs/UPDATES_RECOVERY.md` boundary.

Those decisions may refine table/schema details but must preserve the state ownership and SQLite boundaries locked here.
