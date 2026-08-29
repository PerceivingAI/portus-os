# PortusOS Artifacts

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative artifact architecture + P12 implementation; distribution-neutral authenticated Linux Unix/filesystem proof passes, installed-system/maintenance evidence remains

**Scope:** When outputs become Portus artifacts, artifact IDs, owner/principal scope, locators, checksums/integrity, type, confidentiality, task/provider relationships, retention, availability, cleanup, local presentation, diagnostic bundles, and boundaries for deferred delivery adapters

## 1. Purpose and authority

PortusOS needs durable identity for selected outputs without turning the control plane into a duplicate filesystem, object store, or provider database.

The defining rule is:

> **A file/resource becomes a Portus artifact only when PortusOS deliberately registers it because durable identity, relationship, retention, presentation, integrity, or future delivery semantics are useful. Mere file existence does not create an artifact.**

Related authorities remain separate:

- `docs/STATE.md` owns SQLite and the rule that large artifact bytes are not stored in generic Portus state;
- `docs/TASKS.md` owns task-to-artifact relationships and task result semantics;
- `docs/CAPABILITIES.md` owns provider-resource identity and artifact candidates;
- `docs/POLICY.md` owns principal visibility/authority;
- `docs/SECRETS.md` owns prohibited secret material;
- `docs/HEALTH_RECOVERY.md` owns diagnostic-bundle health/recovery behavior;
- `docs/CLI.md` owns the `portus-os artifact` command surface;
- `docs/ACCEPTANCE.md` owns the blocking acceptance scope.

External/messaging delivery adapters remain deferred from the first ISO.

---

## 2. Artifact versus ordinary file/resource

### Ordinary file/resource

Examples:

```text
project source file
Git repository content
random file in ~/Downloads
browser tab
provider resource
ordinary screenshot
build output not retained/related by Portus
```

These remain native/provider-owned and need no Portus artifact record merely because Master Portus can access them.

### Portus artifact

Examples:

```text
explicitly retained generated report
release package registered as a task output
diagnostic bundle intentionally preserved
provider output promoted for later presentation/delivery
result file whose checksum/retention relationship matters
```

Registration adds Portus-owned metadata/relationships. It does not transfer domain ownership unless an explicit storage/copy operation does so.

---

## 3. Registration criteria

An output should be registered as an artifact when at least one of these is true:

1. a user/task explicitly requests durable artifact identity;
2. a Portus task contract declares the output an artifact/result;
3. the output must outlive or remain discoverable independently of the producing task/session;
4. integrity/checksum verification is useful;
5. retention/cleanup policy must be tracked;
6. the output needs durable relationship to a task/provider/project;
7. it is a diagnostic bundle or other Portus-managed result requiring later local presentation;
8. future delivery/sharing lifecycle will need a stable artifact identity.

Do **not** register merely because:

- a command wrote a file;
- Codex edited source;
- a build produced temporary intermediates;
- a browser/provider owns a live resource;
- the System Index discovered something;
- a file is large/important-looking.

Artifact registration is deliberate lifecycle metadata, not filesystem discovery.

---

## 4. Artifact identity

External artifact IDs use:

```text
artifact_<UUIDv7>
```

Rules:

- assigned once at registration;
- immutable;
- never reused;
- not derived from filename/path/checksum/task ID;
- remains meaningful after the producing task completes;
- may remain as a tombstone/history record after content is removed when retention/audit relationships require it.

SQLite row IDs are never external artifact identity.

---

## 5. Artifact record

A durable artifact record contains conceptually:

```text
artifact_id
owner_principal
artifact_type
confidentiality
retention_policy
availability_state
locator_kind
locator
integrity_kind
sha256 when applicable
size_bytes when known
media_type when known/safe
created_at
registered_at
expires_at when applicable
source_task_id when applicable
source_provider_registration_id when applicable
source_provider_resource_ref when applicable
project_ref when applicable
safe_display_name
safe_metadata
last_verified_at when applicable
removed_at when applicable
```

Detailed SQL belongs to implementation work, but these semantics are authoritative.

Raw reusable secrets are prohibited from all artifact metadata.

---

## 6. Locator model

Artifact bytes/content remain in an approved native/provider storage domain.

The first locator kinds are:

```text
filesystem
provider_resource
```

### 6.1 Filesystem locator

A filesystem artifact stores an absolute canonical path plus the owner principal and captured content metadata.

The path is a locator, not artifact identity.

If the file is renamed/moved outside a Portus-managed operation, the artifact may become `missing` until explicitly relocated/re-registered; PortusOS does not scan the whole filesystem by checksum trying to guess where it went.

Filesystem locators must not escape principal/policy visibility merely because another user knows an artifact ID.

### 6.2 Provider-resource locator

Provider-owned content uses the opaque provider reference from `docs/CAPABILITIES.md`:

```text
provider_registration_id
resource_type
resource_id
```

PortusOS does not interpret provider-internal resource IDs.

If a provider resource is ephemeral/session-scoped, the artifact contract must not pretend it is durable unless the provider supplies a durable resource/export contract.

### 6.3 No arbitrary remote URL locator in the first contract

A generic URL is not a first artifact locator because remote lifetime/authentication/integrity semantics are ambiguous and external delivery is deferred.

A provider may internally own a remote resource and expose it through a provider-resource reference.

---

## 7. Content ownership and storage

Artifact registration does not automatically copy bytes.

Normal pattern:

```text
existing/project/provider output
  -> remains in authoritative storage
  -> Portus records locator + metadata + relationships
```

A future/explicit Portus-managed copy/materialization operation may place content in an approved artifact storage location, but no hidden copy is required merely to register an artifact.

Generic `portus.db` stores metadata only, not artifact blobs.

---

## 8. Integrity and checksum contract

### 8.1 Local regular files

For a filesystem artifact backed by a stable regular file, registration captures at minimum:

```text
SHA-256
size_bytes
```

The digest represents the content observed at registration.

If later verification shows different bytes at the same path:

```text
artifact identity does not silently retarget to the new bytes
availability/integrity becomes changed/mismatch
```

The caller must explicitly register/update according to the intended workflow rather than Portus quietly changing the artifact's meaning.

### 8.2 Provider resources

For provider resources:

- use a provider-supplied cryptographic digest when the provider contract supplies one;
- otherwise record `integrity_kind = provider_authoritative` or `unverified` as appropriate;
- never fabricate a digest from metadata that is not the content.

### 8.3 Mutable resources

A mutable file/provider resource can be referenced, but the artifact must honestly report that captured integrity may no longer match current content.

For release/delivery workflows that require immutable content, verification before delivery/use is mandatory.

---

## 9. Artifact type

`artifact_type` is a coarse semantic class, not a MIME replacement.

First semantic types are:

```text
file
report
release
diagnostic_bundle
screenshot
archive
other
```

`media_type` may separately record a safe MIME/media type where known.

Do not create a top-level artifact type for every extension/application/provider command.

---

## 10. Confidentiality

First confidentiality classes are:

```text
private
shared
public
```

### `private`

Visible/useable by the owning principal and administrator/root according to policy. This is the default.

### `shared`

May be visible/useable by explicitly authorized additional principals/groups according to policy/relationship data.

### `public`

Content is deliberately classified non-private within Portus artifact semantics.

`public` does **not** itself authorize network publication, external delivery, anonymous web serving, or provider upload.

Confidentiality classification and delivery authorization are separate.

Protected reusable secret material cannot be made a normal Portus artifact merely by labeling it private.

---

## 11. Principal isolation

Every user-private artifact record carries an owning principal.

Knowing:

```text
artifact_<UUIDv7>
```

does not grant access.

`portusd` applies Unix-peer authenticated principal policy before revealing:

- metadata;
- filesystem path;
- provider reference;
- task/project relationships;
- safe display name;
- integrity metadata when sensitive.

Another Master Portus user cannot inspect private artifact records by default.

Underlying filesystem/provider access remains independently authoritative; Portus metadata cannot grant access that the caller does not have.

---

## 12. Task relationship

A task may produce zero or more artifacts:

```text
task_<UUIDv7>
  -> artifact_<UUIDv7>
  -> artifact_<UUIDv7>
```

A task may succeed with no artifact.

An artifact may outlive the task.

Deleting/forgetting task history must not silently delete a retained artifact.

Task result summaries should reference artifact IDs rather than embedding large output bytes.

Registration of an artifact plus its task relationship is one Portus-owned transaction when both records are in `portus.db`.

---

## 13. Provider relationship

Provider output remains provider-owned until Portus deliberately registers it.

A provider may return:

```text
bounded result
provider resource ref
artifact candidate
existing artifact ref
```

Registration of a provider resource as an artifact creates a Portus relationship around the opaque provider locator; it does not copy the provider's domain state.

Provider uninstall/removal does not make old artifact history silently bind to a new provider registration generation.

If content becomes unavailable after provider removal, the artifact becomes `missing`/`unavailable` while its bounded historical metadata may remain.

---

## 14. Availability and integrity states

Artifact content availability uses:

```text
available
missing
unavailable
removed
```

Integrity uses a separate assessment:

```text
verified
mismatch
provider_authoritative
unverified
not_applicable
```

Do not collapse these.

Examples:

```text
file exists + SHA-256 matches
  availability = available
  integrity = verified

file exists + digest changed
  availability = available
  integrity = mismatch

provider temporarily down
  availability = unavailable
  integrity = provider_authoritative/unverified

underlying content intentionally deleted
  availability = removed
```

`missing` means expected locator content cannot currently be found; `removed` means Portus has a known intentional removal outcome.

---

## 15. Retention model

Retention policy is explicit and separate from availability.

First retention policies are:

```text
temporary
retained
until
```

### `temporary`

Artifact may be cleaned up when its owning workflow/task declares it no longer needed and no durable relationship/hold prevents cleanup.

### `retained`

No automatic content deletion by ordinary Portus cleanup. Explicit owner/admin action or later retention-policy change is required.

### `until`

Artifact has an explicit `expires_at` timestamp. Expiry makes it eligible for cleanup; it does not guarantee immediate deletion.

Default for explicitly registered user-visible artifacts is:

```text
retained
```

Default for diagnostic bundles is:

```text
temporary
```

unless the caller explicitly retains them.

The first architecture does not invent arbitrary global 30/90-day retention for user artifacts.

---

## 16. Holds and relationship-safe cleanup

Content/metadata cannot be automatically removed while required by:

- an active Portus task;
- an active delivery attempt in a future delivery system;
- explicit owner/admin retention/hold;
- a recovery/diagnostic process that declares the artifact required;
- an audit/release record whose policy requires preservation.

Exact hold schema belongs to implementation, but cleanup must check relationships rather than delete by age alone.

---

## 17. Forget versus delete content

These are fundamentally different operations.

### Forget artifact metadata

Removes or tombstones the Portus artifact registration/relationships according to policy while leaving underlying content untouched unless separately requested.

### Delete artifact content

Attempts to remove the underlying filesystem/provider content through the authoritative storage/provider contract and policy.

The system must never treat:

```text
forget artifact
```

as implicit permission to delete the underlying file/resource.

Likewise deleting a native file outside Portus does not automatically delete the artifact record; it makes the locator missing until reconciled/explicitly cleaned up.

The first ISO does not require a general user-facing delete/forget CLI. These semantics are locked for future implementation.

---

## 18. Cleanup ownership

Portus may automatically delete content only when all of these are true:

1. Portus or the task/provider contract explicitly owns cleanup authority for that content;
2. retention says the content is eligible;
3. no hold/relationship blocks cleanup;
4. expected locator/generation/integrity checks prevent deleting a different resource that reused a path/ID;
5. policy permits the operation;
6. cleanup failure remains visible rather than being reported as success.

Portus never recursively deletes arbitrary parent directories simply because one artifact path lives beneath them.

For externally/provider-owned resources, cleanup uses only provider-declared cleanup affordances.

---

## 19. Local presentation

The first artifact presentation model deliberately uses native paths/provider interfaces rather than creating a duplicate file browser.

```text
portus-os artifact list
portus-os artifact show artifact_<UUIDv7>
```

returns safe metadata and, when authorized:

- filesystem locator; or
- provider/resource reference and provider interface/skill information.

Master Portus then uses normal native/provider tools to open/read/copy the resource.

There is no need for:

```text
portus-os artifact cat
portus-os artifact filesystem-browser
```

because normal Codex/Linux/provider tools already solve those operations.

---

## 20. CLI boundary

The first blocking/read surface remains:

```text
portus-os artifact list
portus-os artifact show <artifact-ref>
```

Artifact-producing Portus tasks/providers may register artifacts through internal typed runtime contracts.

A later CLI may add explicit `register`, `verify`, `forget`, retention, or delivery operations when implementation/user workflows require them. This document defines their semantics but does not require those commands for the first ISO.

Structured output must remain bounded/paginated under `docs/CLI.md`.

---

## 21. Diagnostic bundles

Diagnostic bundles defined by `docs/HEALTH_RECOVERY.md` are artifacts of type:

```text
diagnostic_bundle
```

Default:

```text
confidentiality = private
retention       = temporary
```

They must be generated from allowlisted/redacted evidence and cannot include raw reusable credentials.

External sending/upload is not implied by bundle creation.

---

## 22. Delivery boundary

The first ISO does not implement a general delivery subsystem.

Deferred work includes:

- messaging delivery;
- email/Slack/Drive/S3-style adapters;
- delivery retries across external providers;
- remote sharing workflows;
- sophisticated remote presentation.

Future delivery must treat artifact identity/content integrity separately from destination authorization.

Conceptually:

```text
artifact_<UUIDv7>
  + explicit destination/provider
  + policy approval/authorization
  + integrity verification where required
  -> delivery attempt
```

Artifact confidentiality does not by itself grant delivery permission.

Ordinary native copy/scp/rsync remains usable outside a Portus delivery workflow when the user/agent simply wants ordinary file transfer and no durable delivery lifecycle is needed.

---

## 23. Artifact content changes

Artifact identity refers to the registered output generation/relationship, not “whatever bytes happen to be at this pathname forever.”

For local files:

- registration captures digest/size;
- later mismatch is surfaced;
- Portus does not silently update digest and pretend nothing changed.

If the new content should become the new canonical artifact output, implementation may:

- register a new artifact; or
- use an explicit version/update operation whose semantics preserve history.

The first architecture prefers a **new artifact ID for materially new content** when provenance/integrity matters.

---

## 24. Missing content and reconciliation

Artifact reconciliation is bounded and locator-specific.

Filesystem:

- stat the known path;
- verify digest when requested/required;
- mark missing/mismatch honestly.

Provider resource:

- query/revalidate through the provider contract where supported;
- preserve provider registration generation;
- never search unrelated providers trying to guess a replacement.

Portus does not perform whole-filesystem checksum searches to relocate missing artifacts automatically.

---

## 25. State/storage contract

Artifact metadata lives in the common `/var/lib/portus/state/portus.db` according to `docs/STATE.md`.

Large bytes do not.

Conceptual relational domains include:

```text
artifacts
artifact_task_relationships
artifact_provider_relationships
artifact_holds/retention metadata where needed
future artifact_delivery_attempts
```

Exact table names/schema are implementation detail.

Artifact metadata changes that must remain consistent with task relationships use SQLite transactions.

---

## 26. Security and redaction

Artifact metadata must not leak private content indirectly.

Potentially sensitive fields include:

- filename/display name;
- full filesystem path;
- project/task relationship;
- provider resource identity;
- media type;
- safe metadata.

These are filtered by caller principal/policy.

Artifact metadata never stores:

- raw reusable credentials;
- Authorization headers;
- private-key material;
- browser session databases/cookies merely as metadata;
- complete file contents;
- arbitrary unbounded stdout/stderr.

A protected secret store/database itself is not made a normal artifact for Master access.

---

## 27. First-ISO contract

Artifact support may remain local/minimal as already allowed by `docs/ACCEPTANCE.md`.

The first implementation must be sufficient to prove, when an artifact is used by another blocking subsystem:

1. a deliberate output can receive `artifact_<UUIDv7>` identity;
2. the registry stores metadata/locator rather than large bytes;
3. task/provider relationship can be represented without copying provider/task domain state;
4. private principal isolation is enforced;
5. local regular-file artifacts can capture SHA-256/size and later detect mismatch/missing content;
6. retention/cleanup semantics do not delete unrelated/native content;
7. `artifact list/show` returns bounded authorized metadata;
8. no external delivery adapter is required.

This does not add a standalone artifact-production acceptance test to the first ISO unless another blocking workflow actually registers an artifact.

---

## 28. Validation checklist

### Registration/identity

- [ ] Ordinary files are not auto-registered.
- [ ] Registered artifacts receive immutable `artifact_<UUIDv7>` IDs.
- [ ] Artifact ID is independent of path/checksum/task ID.
- [ ] Private artifact metadata is principal-filtered.

### Integrity

- [ ] Local regular-file registration captures SHA-256 and size.
- [ ] Changed bytes produce integrity mismatch rather than silent digest replacement.
- [ ] Missing path is represented honestly.
- [ ] Provider resource digest is not fabricated when unavailable.

### Relationships

- [ ] Task can reference zero or more artifacts.
- [ ] Artifact may outlive producing task.
- [ ] Provider resource remains provider-owned.
- [ ] Provider registration generation prevents silent rebind after reinstall.

### Retention/cleanup

- [ ] Explicit user-visible artifacts default to retained.
- [ ] Diagnostic bundles default private/temporary.
- [ ] Holds/active relationships prevent cleanup.
- [ ] Forgetting metadata does not imply content deletion.
- [ ] Cleanup checks expected target identity and never deletes arbitrary parent content.

### Security

- [ ] Raw reusable secrets never enter artifact metadata.
- [ ] Another Master user cannot inspect private artifact locators/relationships by default.
- [ ] No artifact command becomes a duplicate general file reader/browser.

---

## 29. Decisions locked by this document

1. Artifact registration is deliberate; ordinary file/resource existence does not create a Portus artifact.
2. External artifact IDs are immutable non-reused `artifact_<UUIDv7>` values.
3. Artifact bytes remain in approved filesystem/provider storage; generic `portus.db` stores metadata/relationships only.
4. First locator kinds are `filesystem` and opaque `provider_resource`; generic remote URL locators are not part of the first contract.
5. Local regular-file artifact registration captures SHA-256 and size.
6. Integrity mismatch never silently updates artifact meaning.
7. Provider resources use provider-supplied digest where available or honest provider-authoritative/unverified integrity state.
8. Coarse artifact types begin with `file`, `report`, `release`, `diagnostic_bundle`, `screenshot`, `archive`, `other`.
9. Confidentiality classes are `private`, `shared`, `public`, with `private` default; confidentiality is separate from delivery authorization.
10. Every private artifact has an owner principal and caller access is enforced through the normal principal/policy boundary.
11. Artifact availability states are `available`, `missing`, `unavailable`, `removed`; integrity state is separate.
12. Retention policies are `temporary`, `retained`, and `until`; explicitly registered user-visible artifacts default to retained.
13. Diagnostic bundles default to private + temporary.
14. Active task/delivery/recovery/audit/explicit holds prevent cleanup.
15. Forgetting artifact metadata and deleting underlying content are separate operations.
16. Automatic cleanup acts only on explicitly cleanup-owned eligible content with expected-target checks.
17. Task results reference artifacts rather than embedding large output bytes.
18. Provider output remains provider-owned until deliberately registered; registration does not duplicate provider domain state.
19. First local presentation is metadata + native/provider locator through `artifact list/show`, followed by normal native/provider tools.
20. No duplicate `artifact cat`/filesystem-browser surface is required.
21. External/messaging delivery adapters and delivery retries are deferred; ordinary native file transfer remains available outside durable artifact-delivery workflows.
22. Materially new content should normally receive a new artifact ID when integrity/provenance matters.
23. Missing artifact reconciliation is targeted to the known locator; Portus does not search the whole filesystem/providers trying to guess replacements.
24. First-ISO artifact support remains local/minimal and does not create a new standalone blocking feature gate.

---

## 30. P12 implementation status and remaining work

P12 resolves the source-neutral implementation details required by the first-ISO artifact gate:

- artifact wire semantics live in `portus-protocol`; the finite first types are `file`, `report`, `release`, `diagnostic_bundle`, `screenshot`, `archive`, and `other`, with typed confidentiality/retention/availability/integrity/cleanup/hold/task-relationship enums;
- `portus-artifact` owns filesystem inspection rather than SQLite: filesystem registration requires an **absolute** path, resolves symlinks with filesystem canonicalization, stores the resulting canonical UTF-8 target path, accepts regular files only, and rejects canonical paths longer than 4096 bytes;
- local file integrity uses streaming SHA-256 via RustCrypto `sha2`, a 1 MiB read buffer, and an 8 GiB synchronous hashing ceiling; size and pre/post file metadata are checked so an obviously changing file fails registration rather than being presented as stable;
- a registered file keeps its original SHA-256/size. Reconciliation returns `verified`, `mismatch`, `missing`, or `unavailable` evidence and never rewrites the digest because the path's bytes changed;
- provider artifacts retain the exact `ProviderResourceRef`, including provider registration ID and generation; provider removal/reinstall makes the old resource stale/missing and cannot silently retarget the artifact to the successor provider;
- `portus.db` migration v8 is backup-required and replaces the old preliminary v1 artifact table with normalized active records plus task relationships, exact provider relationships, explicit principal grants, holds, and minimal tombstones. Legacy filesystem locators migrate; old generic provider locator strings are tombstoned with `legacy_provider_locator_unresolvable` instead of being guessed into a provider generation;
- artifact metadata is bounded: list page maximum 200, explicit share list maximum 64 principals, safe metadata maximum 16 fields / 8 KiB encoded JSON, and secret-like metadata keys are rejected at both artifact-business and state boundaries;
- private artifacts are visible only to owner/root; shared artifacts add explicit principal grants; public artifacts are visible to authenticated local callers. This visibility never changes native filesystem/provider authorization;
- task `produced_by` registration is transactional and both `produced_by`/`required_by` relationships are supported after registration while remaining within the artifact owner's principal;
- holds are explicit typed rows and can be added/released by owner/root; an active hold or active related task blocks cleanup;
- cleanup ownership is locator-specific: filesystem content may be `portus`/`task` cleanup-owned, provider-resource content may be `provider` cleanup-owned, and ownership is not inferred from file existence;
- the implemented filesystem cleanup adapter deletes only an eligible temporary/expired-until registered file after canonical-path + SHA-256 + size expected-target verification. Retained, held, active-task, missing/unavailable, cross-owner, or changed targets fail closed. Provider-resource deletion remains unsupported until a provider declares a concrete deletion affordance;
- forgetting registration creates a minimal tombstone and removes artifact metadata/relationships without deleting the underlying file/provider content;
- normal runtime JSONL exposes only `artifact.list` and `artifact.show`; registration/reconcile/forget/cleanup remain typed first-party internal APIs. `portus-os` correspondingly exposes only `artifact list` and `artifact show`, using the common default page 50 / maximum 200 cursor contract;
- `FilesystemRegistrationRequest::diagnostic_bundle` provides the explicit private/temporary registration shape needed by diagnostic producers. `doctor --bundle` remains daemon-independent and therefore does not silently require `portusd` merely to create its evidence file;
- ordinary filesystem existence still has zero registry effect; no watcher or filesystem inventory was added;
- `portus-artifact` compiles/tests natively on the recorded disposable Linux VM, and the real authenticated Unix/filesystem fixture passes: a deliberately registered filesystem artifact is resolved through `RuntimeServer` and `portus-os artifact list/show` over the actual local socket.

Still intentionally open after P12:

- repeat the authenticated artifact path in the installed Artix layout only where canonical ownership/path behavior differs; the distribution-neutral Linux fixture itself is complete;
- any automatic cleanup scheduler/cadence;
- provider-declared provider-resource deletion adapters;
- a bounded long-term artifact-tombstone pruning policy once state-maintenance requirements justify pruning without weakening identity history;
- optional manual register/verify/forget CLI only if a concrete workflow later proves that direct native/internal paths are insufficient;
- future delivery-attempt schema/adapters when external delivery enters scope.

These remaining items do not reopen deliberate registration, principal isolation, locator/domain ownership, integrity honesty, retention safety, or the no-blob-store/no-duplicate-file-API boundaries above.
