# PortusOS Protected API Provider

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative first-ISO protected API provider implementation contract; P10 source implementation and native Linux build/tests complete, installed Artix service/security/real-upstream acceptance pending

**Scope:** Protected reusable API keys/bearer credentials, service identity, protected storage, native IPC, direct agent/client use, provider binding, TLS/redirect behavior, policy, AUTH provisioning, rotation/revocation, optional SDK compatibility proxy, health/audit behavior, and first-ISO security proof

## 1. Purpose and authority

This document defines the concrete PortusOS implementation direction for **protected API credentials**.

It implements the security architecture in `docs/SECRETS.md` for the first reference class:

```text
reusable API key / bearer credential
  -> provider-bound authenticated API operation
```

The defining property remains:

```text
USE succeeds
EXPORT fails
```

This document does **not** define a universal secret broker for every credential class.

Separate decisions remain for:

- Codex authentication where it cannot use this provider/proxy cleanly;
- browser profile/session isolation;
- password-manager integration;
- SSH/signing keys;
- hardware tokens;
- backup/recovery implementation beyond the requirements stated here.

Authority boundaries:

- `docs/SECRETS.md` owns the general protected-secret classification and non-export requirements;
- `docs/POLICY.md` owns administrator authority, Master Portus authority, caller policy and privilege;
- `docs/CAPABILITIES.md` owns provider registration/capability/resource semantics;
- `docs/RUNTIME.md` owns `portusd`; the protected API provider is a separate provider and is not absorbed into `portusd`;
- `docs/ACCEPTANCE.md` owns the blocking acceptance proof;
- this document owns the concrete protected API provider implementation direction.

---

## 2. Defining architecture

The protected API provider is a separate local provider boundary:

```text
administrator / AUTH
        |
        | provision / rotate / revoke
        v
+--------------------------------------+
| portus-apid                          |
| Linux identity: portus-api           |
|                                      |
| protected credential store           |
| provider bindings / policy checks    |
| upstream request construction        |
+-------------------+------------------+
                    |
                    | TLS-authenticated request
                    | real credential attached here
                    v
             approved upstream API
                    |
                    v
              response/result
                    |
                    v
Master Portus / Codex / approved client

Master-visible data:
  credential reference
  provider/account safe metadata
  operation/result
  health/policy state

Never Master-visible through this path:
  reusable upstream credential value
```

`portusd` may know safe references/relationships but does not carry the reusable credential.

---

## 3. Provider identity and naming

The first protected API provider uses these canonical identities:

```text
provider type:     protected-api
semantic capability:
                   protected-api.request
service daemon:    portus-apid
system user:       portus-api
system group:      portus-api
client group:      portus-api-users
agent/client CLI:  portus-api
```

The provider registration itself receives the normal immutable `provider_<UUIDv7>` registration identity defined by `docs/CAPABILITIES.md`.

`portus-api` follows the Portus component-specific CLI naming rule. It is a provider-owned interface, not a `portus-os capability invoke` wrapper.

The architecture does not depend on a language-specific wire contract. The P10 first-party implementation is now Rust inside the common workspace, while the provider protocol remains language-neutral.

---

## 4. Linux service boundary

### 4.1 Dedicated unprivileged service account

`portus-apid` runs as the dedicated system user/group:

```text
portus-api:portus-api
```

The account:

- is a system/service identity;
- has no interactive login shell;
- has no ordinary user home/workspace;
- is not root;
- is not a member of unrelated privileged groups;
- receives only filesystem, socket and network access required for the protected API provider.

The service must not run as the Master Portus user.

### 4.2 Client access group

Permission to connect to the ordinary-use provider socket is represented at the Linux transport layer by membership in:

```text
portus-api-users
```

Installer/admin policy may grant a Master Portus user this group as part of an approved permission bundle.

Group membership permits reaching the provider interface. It does **not** itself authorize every credential or operation. `portus-apid` must still authenticate the peer and enforce provider/credential/operation policy.

### 4.3 No store access through the client group

The protected credential store must not be group-readable by `portus-api-users`.

The transport permission and secret-storage permission are deliberately different boundaries:

```text
portus-api-users
  -> may connect to use interface
  X-> may read provider credential store
```

---

## 5. Runtime paths and socket ownership

The first provider runtime directory is:

```text
/run/portus/protected-api/
```

The canonical sockets are:

```text
/run/portus/protected-api/use.sock
/run/portus/protected-api/admin.sock
```

The runtime directory is created by the OpenRC/service startup path with ownership/mode allowing `portus-api` to create its sockets while allowing approved client-group traversal.

The intended transport permissions are conceptually:

```text
use.sock
  owner: portus-api
  group: portus-api-users
  mode:  0660

admin.sock
  owner: portus-api
  ordinary group access: none
  mode:  0600
```

Exact OpenRC `start_pre`/directory-creation mechanics remain for the OpenRC implementation document, but those mechanics must produce this security boundary.

### 5.1 Peer credential authentication

Both sockets use Unix peer credentials as the authenticated local identity source.

At minimum the provider derives:

```text
UID
GID
PID where available/useful
```

from the kernel (`SO_PEERCRED` or the equivalent Linux mechanism), not from caller-supplied JSON.

A request field such as `user = ...` is never identity proof.

### 5.2 Admin socket authorization

Administrative credential operations are accepted only from an authenticated administrative principal.

For the first implementation, the admin socket is **root-only at the provider protocol level**:

```text
peer UID == 0
```

must be true for provisioning, rotation, revocation, deletion or other protected-store administration.

Filesystem mode is defense in depth; an accidental socket-mode widening must not make Master Portus an administrator.

Future narrowly delegated administrative helpers may be added only through an explicit policy/elevation decision.

---

## 6. Protected storage

The provider-owned persistent directory is:

```text
/var/lib/portus/protected-api/
```

It is owned by:

```text
portus-api:portus-api
```

and is not readable/traversable by ordinary Master users or `portus-api-users`.

The first provider may use a provider-owned SQLite store, conceptually:

```text
/var/lib/portus/protected-api/credentials.db
```

This database is **not** `/var/lib/portus/state/portus.db` and is not part of generic Portus state.

Raw reusable API credentials may exist inside this provider-owned store because the store itself is inside the protected service boundary.

### 6.1 At-rest protection decision for the first ISO

The first ISO relies on:

```text
LUKS encrypted installed storage
+
Linux ownership/mode isolation
+
dedicated provider service identity
```

as the baseline at-rest/runtime protection for this provider store.

The first ISO does **not** introduce a home-grown second application-layer encryption scheme merely to encrypt `credentials.db` with a key that the same service must automatically possess.

A later hardware-backed or separately sealed encryption layer may be added if it provides a demonstrable additional security property.

This decision does not weaken backup requirements: a copy of the protected provider store remains protected credential material and must not be placed in Master-readable ordinary backups.

### 6.2 Provider-owned SQLite schema

The first implementation uses SQLite for the protected credential store with this minimum logical schema:

```text
meta
  schema_version INTEGER

credentials
  credential_ref TEXT PRIMARY KEY
  provider_id TEXT NOT NULL
  safe_label TEXT NULL
  secret BLOB NOT NULL
  generation INTEGER NOT NULL
  state TEXT NOT NULL            # active | revoked
  created_at TEXT NOT NULL
  rotated_at TEXT NULL
  revoked_at TEXT NULL
  updated_at TEXT NOT NULL
```

The first implementation deliberately keeps only the active reusable secret in the logical credential row rather than maintaining a queryable history of prior secret generations.

Rotation increments `generation` and atomically replaces `secret` in one transaction. Historical SQLite pages/backups may still contain prior encrypted-disk data; the first ISO does not claim forensic secure erasure.

Database requirements:

```text
PRAGMA foreign_keys = ON
PRAGMA journal_mode = WAL
PRAGMA synchronous = FULL
PRAGMA secure_delete = ON
```

The provider starts with a restrictive process umask equivalent to `0077`. The database and any `-wal`/`-shm` side files remain inside `/var/lib/portus/protected-api/` and are inaccessible to Master/client groups.

Store migrations are versioned, transactional and fail closed. `portus-apid` must not start an incompatible schema in writable service mode.

### 6.2 No normal store access by `portusd`

`portusd` has no normal filesystem permission to read the provider credential store and receives no API that returns raw values.

Safe references/availability/policy relationships may be replicated into generic Portus state as allowed by `docs/SECRETS.md` and `docs/CAPABILITIES.md`.

---

## 7. Credential references and generations

A protected credential has a non-secret stable reference such as:

```text
openai/main
```

The reference is safe to expose to Master Portus when policy permits.

The reusable value behind that reference is provider-owned.

Credential rotation preserves the stable reference while replacing its active secret generation.

Conceptually:

```text
openai/main
  -> credential generation N
```

becomes:

```text
openai/main
  -> credential generation N+1
```

without requiring tasks, skills or configuration to learn the reusable value.

Generation metadata may be exposed only at a safe non-secret level where useful for expected-state/reconciliation behavior.

---

## 8. Native provider protocol

### 8.1 Transport

The canonical native provider interface is versioned newline-delimited JSON over:

```text
/run/portus/protected-api/use.sock
```

Administrative messages use the separate admin socket.

The protected API protocol is provider-owned and distinct from the `portusd` runtime protocol, even if both use versioned JSONL framing.

### 8.2 Request envelope

The exact wire schema is an implementation contract, but the architecture requires fields equivalent to:

```text
protocol_version
request_id
operation
credential_ref
payload
```

and a structured success/error response.

### 8.3 First native wire contract

Use-socket request:

```json
{
  "protocol_version": 1,
  "request_id": "<opaque request id>",
  "action": "request",
  "credential_ref": "openai/main",
  "operation": "openai.responses.create",
  "payload": {}
}
```

Successful response:

```json
{
  "protocol_version": 1,
  "request_id": "<same request id>",
  "ok": true,
  "provider_id": "openai",
  "operation": "openai.responses.create",
  "upstream_status": 200,
  "body": {}
}
```

Error response:

```json
{
  "protocol_version": 1,
  "request_id": "<same request id>",
  "ok": false,
  "error": {
    "code": "operation_not_allowed",
    "message": "..."
  }
}
```

The first error vocabulary includes at least:

```text
invalid_request
permission_denied
approval_required
credential_not_found
credential_revoked
provider_definition_invalid
operation_not_allowed
request_too_large
response_too_large
timeout
tls_error
redirect_rejected
upstream_error
store_unavailable
internal
```

Clients must branch on `error.code`, not parse human prose.

The first provider request path is non-streaming. Provider operations that require indefinite/SSE streaming are not silently tunnelled through this contract until a bounded streaming protocol is explicitly added.

### 8.3 Named operations, not caller-supplied destinations

The ordinary use interface accepts **named provider operations**, not an arbitrary URL to which a credential should be attached.

Conceptually valid:

```text
credential_ref = openai/main
operation      = openai.responses.create
payload        = {...}
```

Conceptually invalid as an interface design:

```text
credential_ref = openai/main
url            = https://caller-selected.example/
header_name    = Authorization
```

The provider adapter maps a named operation to the approved upstream method/path/authentication behavior.

The caller cannot supply:

- scheme;
- upstream host;
- upstream port;
- authentication header/value;
- TLS-disable behavior;
- arbitrary redirect policy.

### 8.4 No secret retrieval operation

Neither use nor admin protocol exposes an ordinary operation equivalent to:

```text
credential.get_raw
credential.export
credential.reveal
authorization_header.get
```

Root/administrator deliberate export, if ever implemented, must use a separate explicit administrative recovery/reclassification path and cannot be part of the ordinary Master-facing provider contract.

---

### 8.5 First administrative wire contract

The root-authenticated admin socket supports only typed credential-management actions:

```text
credential.provision
credential.rotate
credential.revoke
credential.delete
credential.show
credential.list
```

`credential.show` and `credential.list` return safe metadata only.

Provision request conceptually contains:

```json
{
  "protocol_version": 1,
  "request_id": "...",
  "action": "credential.provision",
  "credential_ref": "openai/main",
  "provider_id": "openai",
  "safe_label": "Main OpenAI credential",
  "secret": "<secret received only on root admin IPC>"
}
```

The presence of `secret` on the privileged in-memory admin message does not make it an ordinary exportable field: it is accepted only on the root-authenticated admin socket, never returned, never logged and never represented in generic Portus state.

Rotation uses the same secret-safe admin path and performs an atomic replacement/increment. Revoke/delete require no secret input.

No admin message named `export`, `reveal`, `get_raw`, or equivalent exists in the first protocol.

---

## 9. Direct agent/client interface

PortusOS ships a provider-owned client executable:

```text
portus-api
```

Its purpose is to make the native protected API provider usable directly by Master Portus/Codex without forcing agents to manually construct Unix-socket traffic.

The CLI is a thin client to the provider. It may expose safe operations such as:

- list safe credential/provider references visible to the caller;
- inspect safe availability/health metadata;
- perform a named authenticated API operation;
- request a local compatibility-proxy session where supported.

It must not expose raw secret retrieval.

Detailed `portus-api` command syntax belongs to the provider implementation/skill contract and does not expand the `portus-os` command tree.

The first direct CLI surface is now locked as:

```text
portus-api credential list
portus-api credential show <credential-ref>
portus-api request <credential-ref> <operation> [--input <path|->]
portus-api health
```

`portus-api request` reads the provider operation payload from stdin by default (`--input -`) or from an explicitly selected file. It does not accept the reusable credential as an argument or environment variable.

Structured output is available through `--json`; provider response bodies are returned as result data and are not copied into logs by default.

Administrative credential mutation is **not** exposed through the Master-facing `portus-api` CLI. The administrator uses `portus-auth`/root admin IPC.

A dedicated machine-wide/provider skill should teach agents how to use `portus-api` and the supported provider operations.

---

## 10. Provider bindings

A protected credential is bound to a root/administrator-controlled provider definition.

Provider definitions are non-secret configuration under a protected system configuration location, conceptually:

```text
/etc/portus/protected-api/providers.d/
```

Master Portus cannot modify these definitions under normal authority.

A provider definition controls at least:

- provider ID;
- approved upstream origin/service identity;
- allowed named operations;
- HTTP method/path mapping for each operation where applicable;
- authentication placement/scheme;
- request/response bounds;
- timeout policy;
- TLS behavior;
- redirect behavior;
- safe logging/redaction rules.

Changing a provider binding is administrator configuration, not an ordinary API request.

### 10.1 Provider-definition file format

The first provider-definition format is versioned TOML under:

```text
/etc/portus/protected-api/providers.d/<provider-id>.toml
```

Minimum shape:

```toml
schema_version = 1
provider_id = "openai"
origin = "https://api.openai.com"

[authentication]
kind = "bearer"
header = "Authorization"
prefix = "Bearer "

[limits]
max_request_bytes = 8388608
max_response_bytes = 33554432
timeout_ms = 120000

[operations."openai.responses.create"]
method = "POST"
path = "/v1/responses"
streaming = false
```

First-contract limits therefore are:

```text
request body:  8 MiB maximum
response body: 32 MiB maximum
operation timeout: 120 seconds maximum
```

The administrator/package may define lower limits per provider/operation. Master-facing requests cannot raise them.

Provider definitions are root-owned. Unknown schema fields that affect destination/authentication/security must not be silently interpreted as permissive defaults.

The provider definition—not the caller—owns origin, authentication placement, TLS requirements, redirect behavior, method/path mapping and limits.

### 10.1 Reference OpenAI-style provider

The first ISO validates the design using an OpenAI-style bearer/API-key provider.

Reference binding verified 2026-08-21 against the official OpenAI Platform quickstart: the documented Responses request uses `POST https://api.openai.com/v1/responses` with `Authorization: Bearer <API key>`. The root-owned provider definition may be updated by a later hard cutover if the official contract changes; Master callers never control that origin/authentication binding.

The active provider definition must use the current approved official API origin selected for that acceptance build/run and bind `openai/main` to that provider definition.

The caller selects an allowed named operation, not the API origin.

---

## 11. TLS policy

Credential-bearing upstream requests require verified TLS.

For the first implementation:

- protected HTTP API operations use HTTPS;
- normal system CA trust/hostname validation is required;
- SNI/hostname verification is not disabled;
- caller-supplied CA bundles/certificates do not override the protected provider definition;
- there is no Master-facing `--insecure`, `verify=false`, or equivalent bypass;
- TLS failures fail the operation closed without credential fallback or disclosure.

Administrator-controlled enterprise/custom trust configuration may be added later through explicit provider configuration, not through caller-controlled request fields.

---

## 12. Redirect policy

The first protected API provider automatically follows **no credential-bearing redirects**.

Any upstream 3xx response during an authenticated protected operation is rejected and surfaced as a structured provider/upstream error.

This deliberately strict first contract prevents credential forwarding to an origin not explicitly selected by the provider adapter.

A future provider-specific redirect rule may be added only if the destination chain can be statically/administratively constrained and validated without caller control.

---

## 13. Authorization and policy

Effective permission for a protected operation requires all applicable layers:

```text
Linux transport access
AND
peer-credential principal identity
AND
PortusOS outer policy
AND
provider registration availability
AND
credential-specific/provider-specific operation policy
AND
provider binding rules
```

Possession of `portus-api-users` membership alone does not grant arbitrary credential use.

Policy may classify protected use as:

```text
allow
prompt/approval
reject
```

A protected credential configured for `allow` may be used unattended within its approved provider/operation constraints.

Credential export remains unavailable regardless of ordinary use approval.

---

## 14. AUTH and provisioning

AUTH is the human/admin secret-entry surface, but provisioning ultimately crosses the protected provider admin socket.

The first provisioning chain is conceptually:

```text
human administrator
  -> AUTH / privileged provisioning helper
  -> root-authenticated admin.sock request
  -> portus-apid
  -> provider-owned credential store
```

The reusable credential must not be passed through:

- Master Portus conversation;
- ordinary `portus-os` arguments;
- ordinary `portus-api` arguments intended for Master use;
- shell command-line arguments;
- persistent environment variables;
- clipboard history;
- `portusd` IPC;
- generic Portus SQLite;
- normal logs.

A provisioning helper must accept the credential through a secret-safe channel such as a direct protected UI/IPC payload, pipe/file descriptor, or equivalent mechanism that avoids process arguments and ordinary shell history.

The exact AUTH UI and privilege-elevation mechanism remain in the AUTH/policy implementation workstream, but the root-only `admin.sock` boundary is locked here.

---

## 15. Rotation, revocation and deletion

### 15.1 Rotation

Rotation preserves the credential reference and atomically activates a new secret generation.

New requests after successful rotation use the new generation.

Rotation failure must leave either the old valid generation active or the credential unavailable; it must not produce an ambiguous partially written value.

### 15.2 Revocation/disable

Revocation disables new use immediately at the provider boundary while preserving safe metadata/history required for diagnosis/audit.

### 15.3 Deletion

Deletion removes the active provider credential record after policy/admin checks.

The first ISO does not claim forensic secure erasure of every historical storage block or encrypted-disk remnant. Backups and prior protected store copies remain protected material and must follow protected retention/recovery policy.

### 15.4 No silent fallback

Rotation/revocation/provider failure must never copy the credential into a Master-readable file/environment as a compatibility fallback.

---

## 16. Local SDK/application compatibility proxy

The canonical security interface remains the Unix socket.

Some third-party SDKs/applications require an HTTP/provider-compatible base URL and cannot speak the native provider protocol. PortusOS therefore permits an optional local compatibility proxy layered on `portus-apid`.

### 16.1 Proxy is not the primary trust boundary

The compatibility proxy:

- binds only to loopback/local interfaces selected by implementation;
- never relies on `localhost` alone for authorization;
- does not expose the upstream reusable credential;
- maps provider-compatible requests to the same named/provider-bound internal operation contract.

### 16.2 Authentication mechanism

Compatibility access uses a short-lived **local capability token** minted through authenticated `use.sock` IPC.

The token:

- is generated only after Unix peer-credential authorization;
- is scoped to the approved principal/provider/credential reference/operation set;
- has a bounded lifetime;
- is valid only against the local Portus compatibility proxy;
- is not accepted by the upstream API provider;
- is not treated as the protected upstream credential;
- may be intentionally readable by the Master-owned client process because possession of the token cannot reveal or directly reuse the upstream credential outside the local broker boundary.

Because ordinary TCP/loopback does not provide the Unix peer identity used by `use.sock`, the compatibility token is itself the authorization capability for the proxy session. It must therefore be random/unpredictable, narrowly scoped and short-lived.

### 16.3 First compatibility-token bounds

When compatibility proxy support is implemented, the first token contract is:

```text
entropy:         at least 256 random bits before encoding
default lifetime: 10 minutes
maximum lifetime: 60 minutes
storage:         provider memory only
scope:           issuing UID + provider + credential ref + allowed operation set
restart behavior: provider restart invalidates all outstanding tokens
```

Tokens are opaque random capability strings, encoded in a transport-safe form such as base64url. They are never derived from the upstream credential.

An explicit token-revoke operation may invalidate a live token before expiry.

The first proxy binds loopback only and remains subject to the same fixed provider binding, request/response limits, TLS and redirect rules as native use.

### 16.3 Compatibility proxy is not first-ISO blocking

The first ISO protected-credential proof may use native `portus-api`/Unix-socket operation only.

If the compatibility proxy is implemented in the first ISO, the negative tests in `docs/ACCEPTANCE.md` apply to it. Otherwise it remains a later compatibility adapter whose authentication architecture is already fixed by this document.

---

## 17. Request, response and logging boundaries

The provider must not log or return:

- upstream Authorization headers;
- reusable API keys/tokens;
- credential-store records;
- secret-bearing process environments;
- raw internal request objects containing attached authentication material.

Safe audit/diagnostic metadata may include:

- request ID;
- authenticated caller UID/principal;
- credential reference;
- provider ID;
- named operation;
- policy result;
- timestamp/duration;
- success/failure category;
- safe upstream status code/class where appropriate;
- provider health/degradation reason.

Request bodies and response bodies are not logged by default merely because a protected request passed through the provider. They may contain user/private data unrelated to the credential.

Redaction is defense in depth; the preferred design is that the reusable secret never enters generic logging structures.

---

## 18. `portusd`, tasks and System Index relationship

### `portusd`

May store/transport:

```text
provider_<UUIDv7>
protected-api provider ID
openai/main safe reference
availability/health
policy/task relationships
safe result/artifact references
```

Must not store/transport:

```text
real reusable API credential
credential-bearing Authorization header
provider store contents
```

### Tasks

Most API calls are ordinary provider operations and do not become Portus tasks.

When a Portus task uses the provider, task state may reference the provider operation/resource safely but does not copy protected credentials into task payloads/events/results.

### System Index

May show provider registration, safe credential/resource refs, availability and relationships. It remains non-authoritative for provider secret state and cannot expose provider store contents.

---

## 19. Health and degradation

The protected API provider integrates with the common provider health model:

```text
healthy
degraded
unavailable
unknown
```

Health may safely distinguish conditions such as:

- service not running;
- provider store unavailable/corrupt;
- no configured credential for a requested ref;
- provider definition incompatible;
- upstream TLS/network unavailable;
- credential rejected/revoked;
- policy denies caller/operation.

Health output must not reveal the credential value.

Stopping `portus-apid` must make only affected protected API capabilities unavailable/degraded. It must not disable Linux, SSH, Codex ordinary operation, `portusd`, unrelated providers, or Master-readable credential paths.

---

## 20. Provider installation and OpenRC relationship

`portus-apid` is an independent OpenRC-supervised provider service.

Its eventual service definition must:

- create/verify runtime directories with the ownership model in this document;
- run as `portus-api` rather than root or Master Portus;
- start without requiring a Master graphical session;
- expose health/restart state to Portus provider integration;
- use bounded restart policy defined by the future OpenRC/health-recovery contract;
- fail without causing `portusd` or unrelated providers to be terminated.

Exact runlevel/dependency/restart counts remain for `docs/OPENRC_SERVICES.md`/health-recovery work.

---

## 21. First-ISO reference proof

The first ISO must prove this concrete architecture using one real protected API credential with OpenAI-style bearer/API-key semantics.

Required positive proof:

1. root/admin provisions the credential through the protected admin path;
2. Master Portus can discover the safe credential/provider reference;
3. Master Portus performs a real allowed provider operation through `portus-api`/native IPC;
4. `portus-apid` attaches the reusable credential internally;
5. the upstream authenticated request succeeds;
6. Master receives the provider result without receiving the reusable credential.

Required negative proof:

1. Master cannot read `/var/lib/portus/protected-api/` or the provider store;
2. client-group membership does not grant store access;
3. use/admin protocol exposes no raw-secret read/export method to Master;
4. admin operations reject a non-root peer even if filesystem/socket access is deliberately misconfigured during a negative test;
5. caller cannot supply an arbitrary upstream scheme/host/port;
6. caller cannot supply/obtain the credential-bearing Authorization header;
7. protected requests reject credential-bearing redirects rather than following them;
8. TLS verification cannot be disabled by the Master-facing request/client;
9. `portusd`, generic SQLite, tasks, index records, logs and diagnostics do not contain the key;
10. process arguments/environments used by the protected path do not contain the upstream key in Master-readable form;
11. stopping the provider fails the protected capability closed without plaintext fallback;
12. another Master user cannot use the protected credential without explicit corresponding authorization;
13. if compatibility proxy exists, its local token is not accepted by the upstream provider and proxy requests cannot escape the provider binding.

Passing the positive request alone is insufficient. The provider is not accepted as protected until the negative tests pass.

---

## 22. Explicit exclusions

This provider does not automatically solve:

- Codex credentials that require a different auth mechanism;
- browser cookie/password/session database isolation;
- password-manager vault architecture;
- SSH private-key/signing action providers;
- hardware-backed credentials;
- arbitrary secret injection into Master-owned processes;
- generic secret retrieval;
- generic arbitrary-destination authenticated HTTP;
- remote/network exposure of the native provider socket.

Those are separate capability/credential decisions.

---

## 23. Decisions locked by this document

1. Protected API credentials use a dedicated provider rather than `portusd` or a same-user keyring as the protection boundary.
2. Provider type is `protected-api`; semantic capability begins with `protected-api.request`.
3. The service daemon is `portus-apid`.
4. The daemon runs as the dedicated unprivileged `portus-api` system user/group.
5. Ordinary client transport access uses the separate `portus-api-users` group and does not grant credential-store access.
6. Native runtime path is `/run/portus/protected-api/` with separate `use.sock` and root-only-protocol `admin.sock` interfaces.
7. Unix peer credentials are authoritative for native caller identity.
8. Administrative credential mutation requires peer UID 0 in the first implementation.
9. Provider storage is separate at `/var/lib/portus/protected-api/`, conceptually using provider-owned `credentials.db`, never generic `portus.db`.
10. First-ISO at-rest/runtime baseline is LUKS + Linux ownership + separate service identity; no home-grown second encryption layer is required.
11. Stable non-secret references such as `openai/main` survive credential rotation while secret generations remain provider-owned.
12. Native provider protocol is versioned JSONL over Unix sockets.
13. Ordinary use accepts named provider operations and payloads, not caller-selected credential destinations/hosts/ports/authentication headers.
14. No ordinary protected-provider secret retrieval/export operation exists.
15. `portus-api` is the direct provider-owned Master/Codex client surface; `portus-os capability` remains discovery-only.
16. Provider bindings are administrator/root-controlled non-secret configuration and map named operations to fixed approved upstream behavior.
17. Credential-bearing API use requires verified HTTPS/TLS with no Master-facing insecure bypass.
18. The first provider automatically follows no credential-bearing redirects; any 3xx fails the protected request.
19. Effective use requires Linux transport access + authenticated principal + Portus policy + credential/provider operation policy + provider binding rules.
20. Protected use may run unattended under `allow` policy; export remains unavailable.
21. AUTH/root provisioning crosses the separate admin socket and must not place the credential in shell arguments, generic Portus IPC/state or ordinary logs.
22. Rotation is generation-based and atomic at the stable credential reference; revocation disables new use immediately.
23. Provider failure never falls back to Master-readable plaintext storage/injection.
24. Optional SDK compatibility uses a loopback/local proxy authenticated by short-lived scoped local capability tokens minted through authenticated Unix IPC.
25. A local compatibility token is intentionally not the upstream credential and is valid only against the local broker boundary.
26. Compatibility proxy implementation is not itself required for the first-ISO proof; the native protected operation is sufficient.
27. `portusd`, tasks and System Index carry only safe references/relationships and never reusable protected API credentials.
28. `portus-apid` is independently OpenRC-supervised and its failure degrades only the affected provider capability.
29. First-ISO acceptance requires both successful authenticated use and explicit failed export/exfiltration tests.
30. This document is API-credential-specific and does not create a universal secret broker for browser, SSH/signing, password-manager, hardware-token or every Codex credential use case.
31. The first provider-owned SQLite store uses the locked `meta`/`credentials` logical schema, WAL/FULL/secure-delete PRAGMAs, restrictive provider-owned permissions and transactional migrations.
32. Native use protocol version 1 has typed `request` messages and structured semantic errors; the first request path is non-streaming.
33. The first direct client surface is `portus-api credential list`, `credential show`, `request`, and `health`; administrative mutation remains in root `portus-auth`/admin IPC.
34. Provider definitions use root-owned versioned TOML; the first ceilings are 8 MiB request, 32 MiB response and 120-second operation timeout, with callers unable to raise them.
35. The first reference named operation is `openai.responses.create`; the provider definition owns its origin/method/path/authentication behavior.
36. Compatibility tokens, when implemented, have at least 256 bits of randomness, 10-minute default TTL, 60-minute maximum TTL, in-memory storage and scope to UID/provider/credential/operation set.
37. The blocking adversarial security suite is `docs/PROTECTED_API_VALIDATION.md`.

---

## 24. Implementation status and remaining dependent work

P10 has implemented the host-safe contract in Rust:

- `portus-protected-api` owns the strict protocol-v1 serialization, semantic errors, provider-definition validation, request IDs, bounds and zeroizing secret material;
- `portus-apid` owns provider SQLite schema/migration v1, fixed provider-bound HTTPS execution, P9 policy mapping, audit preflight/completion, safe health, generation rotation/revocation/deletion, and the Linux service/socket/store source path;
- `portus-api` implements the locked direct-client command surface and bounded stdin/file payload path;
- `portus-auth` implements root-oriented safe administration with provisioning/rotation secrets absent from argv and production secret entry restricted to a non-echo interactive TTY;
- shipped source configuration includes `runtime/integrations/manifests/protected-api.toml` and `runtime/integrations/protected-api/providers/openai.toml` for later installed placement;
- the host-side `security-negative` category executes the source-neutral P9/P10 adversarial subset.

Remaining dependent work is installed-system evidence or explicitly deferred scope:

- apply/verify the P16 runtime/store/audit/config directory declarations with real Artix identities, groups, ownership and modes, including the still-Linux-resolved shared audit placement;
- execute the daemon as the real `portus-api` service user and prove kernel peer credentials, root-only admin semantics and `portus-api-users` transport separation;
- install the P16-staged provider manifest/definition and load the now-shipped dedicated `/etc/codex/skills/protected-api` provider skill through real Codex;
- run the full Linux filesystem/process/env/argv/state/index/artifact/log non-export inspection;
- run controlled TLS-certificate, plain-HTTP, arbitrary-destination and all required redirect fixtures;
- stop/restart the real OpenRC service and prove failure isolation with no plaintext fallback;
- provision a real reusable credential and complete the installed real-upstream `USE succeeds / EXPORT fails` acceptance suite in `docs/PROTECTED_API_VALIDATION.md`;
- define exact loopback port/session lifecycle only if the optional compatibility proxy is implemented;
- add provider-specific audit rate limiting only if later health/abuse requirements justify it; current audit records already use the bounded common audit sink/rotation contract;
- define future protected backup/restore only if that deferred capability becomes required.

The implementation language/dependencies, SQLite migration, protocol serialization, policy mapping, direct CLI formatting/core, AUTH secret-buffer handling and host-independent adversarial tests are therefore no longer open P10 decisions.

---

## 25. Defining statement

> PortusOS protects reusable API credentials by giving a separate provider service the credential and giving Master Portus an operation interface. `portus-apid` authenticates the local caller, applies policy and a root-controlled provider binding, constructs the approved TLS request, attaches the secret internally, and returns only the provider result. The Master may be broadly authorised to use that capability without gaining a path to retrieve or redirect the reusable credential.