# PortusOS Protected API Provider Validation

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative adversarial validation contract for the first protected API provider

**Scope:** Positive use, direct export attempts, confused-deputy/exfiltration attacks, TLS/redirect attacks, local compatibility-token attacks, cross-user tests, degradation/recovery, and acceptance evidence for `portus-apid`

## 1. Purpose

A successful API call does not prove that a protected credential is protected.

The first protected API provider is accepted only when it proves both:

```text
USE succeeds
EXPORT fails
```

This document turns the negative requirements in `docs/SECRETS.md`, `docs/PROTECTED_API_PROVIDER.md`, and `docs/ACCEPTANCE.md` into an explicit adversarial validation suite.

P10 now includes an executable **host-safe subset** through `scripts/test.sh security-negative` / `scripts/test.ps1 security-negative`. That subset proves source-neutral protocol/provider-binding/policy/non-export properties using isolated stores and injected upstream behavior. It is useful implementation evidence, but it does **not** satisfy this document's acceptance rule by itself.

The final blocking suite must use the actual installed service identities, filesystem modes, OpenRC service, provider definition, native client/protocol, policy mapping and Master user model.

---

## 2. Test principals and fixtures

The validation environment requires:

- `root` / administrator;
- one authorized Master user (`master-a` conceptually);
- one second unauthorized Master user (`master-b` conceptually) where cross-user testing is performed;
- `portus-api` service identity;
- a real protected API credential provisioned under a safe ref such as `openai/main`;
- one allowed named provider operation;
- controlled local attacker HTTP/HTTPS endpoints for negative destination/redirect tests;
- log/state inspection access for the test administrator.

The real credential value may be known to the human validation harness only for comparison/detection. It must not be emitted into ordinary test logs or artifacts.

A one-way digest/fingerprint computed only inside the privileged test harness may be used to search candidate outputs without printing the raw credential.

---

## 3. Group A — positive protected use

Required proof:

1. root provisions `openai/main` through `portus-auth`/admin IPC without putting the value in argv;
2. `master-a` can list/inspect the safe reference but not the value;
3. `master-a` invokes the allowed named operation through `portus-api`/`use.sock`;
4. `portus-apid` constructs the fixed approved upstream request and attaches the credential internally;
5. the real upstream provider accepts the request;
6. the result is returned to `master-a`;
7. no response field contains the reusable credential.

Acceptance requires all steps.

---

## 4. Group B — direct export/retrieval attempts

As `master-a`, attempt each applicable path.

### B1. Filesystem/store

- read/traverse `/var/lib/portus/protected-api/`;
- read `credentials.db`;
- read any SQLite journal/WAL/SHM files;
- copy the provider store through ordinary user tools.

Expected: denied.

### B2. Provider API/CLI

Attempt:

- undocumented/guessed `credential.get_raw`/`export`/`reveal` operations;
- malformed operation names intended to reach admin behavior;
- provider safe-metadata calls with injection/serialization edge cases.

Expected: no raw-secret operation exists; invalid/unauthorized requests fail structurally.

### B3. Admin socket

As non-root, attempt to connect/use `admin.sock`.

Then deliberately widen the socket filesystem mode in a controlled negative-test setup while keeping provider protocol checks unchanged.

Expected: admin mutation still rejects peer UID != 0.

### B4. Process exposure

Inspect all Master-readable process metadata available under the selected Linux configuration:

- argv;
- environment where readable;
- child process trees;
- temporary files;
- core/crash output if enabled for the service under test.

Expected: no upstream reusable credential.

### B5. Generic Portus surfaces

Inspect/search:

- `/var/lib/portus/state/portus.db`;
- task records/events/results;
- System Index output/cache;
- `portus-os` output/errors;
- provider registration metadata;
- diagnostics/doctor output;
- normal Portus logs.

Expected: safe refs/metadata only; no credential-bearing header/value.

---

## 5. Group C — confused-deputy and destination exfiltration

The caller must be unable to turn `portus-apid` into a credential-forwarding oracle.

Attempt requests that try to control or inject:

- scheme (`http`/other);
- upstream hostname;
- upstream port;
- full URL;
- `Host` override;
- `Authorization`;
- `Proxy-Authorization`;
- credential-bearing `Cookie` or equivalent auth channel;
- caller-supplied CA bundle/certificate;
- arbitrary proxy destination;
- arbitrary command/environment injection.

Expected: the named-operation schema rejects or ignores caller control over protected destination/authentication fields; no attacker endpoint receives the reusable credential.

### C1. Local attacker server

Run a controlled local endpoint and attempt to make the protected provider send authenticated traffic to it through payload/operation manipulation.

Expected: impossible under the provider definition; no credential arrives.

### C2. Alternate provider/origin

Attempt to use `openai/main` with an operation/provider definition not bound to that credential.

Expected: operation denied; credential is never attached to a mismatched provider.

---

## 6. Group D — redirects and TLS

### D1. Redirect attacks

Use a controlled endpoint/fixture that returns each relevant redirect class:

```text
301
302
303
307
308
```

for an authenticated operation.

Expected first-contract behavior: any 3xx fails closed; no follow; no credential reaches redirect destination.

### D2. TLS validation

Attempt:

- invalid/self-signed certificate;
- hostname mismatch;
- expired/not-yet-valid certificate where supported by fixture;
- caller request for `verify=false`/`--insecure`/custom CA override.

Expected: TLS validation failure; no Master-controlled bypass exists.

### D3. Plain HTTP

Attempt to direct a protected operation to plain HTTP through all request/config surfaces available to Master.

Expected: impossible/denied.

---

## 7. Group E — policy and cross-user isolation

### E1. Transport group is not authorization

Give a caller `portus-api-users` transport membership but no credential/operation grant.

Expected: socket connection may succeed; protected operation is denied.

### E2. Allowed user

Grant `master-a` the exact credential/operation policy.

Expected: allowed operation succeeds.

### E3. Unauthorized second Master

Ensure `master-b` lacks the corresponding grant.

Expected:

- cannot read provider store;
- cannot administer credentials;
- cannot use `openai/main` merely because it knows the safe reference;
- cannot infer raw credential through errors/metadata.

### E4. Prompt first-implementation behavior

Set effective policy to `prompt` for a protected/privileged operation where the first implementation has no secure approval grant.

Expected: `approval_required`; no authenticated upstream side effect.

---

## 8. Group F — rotation, revocation and failure

### F1. Rotation

1. perform successful request with generation N;
2. root rotates credential to generation N+1 through secret-safe admin path;
3. perform new request;
4. prove new requests use N+1 and safe ref remains stable;
5. ensure rotation process did not expose either secret generation to Master surfaces.

### F2. Revocation

Revoke/disable `openai/main`.

Expected: new use fails immediately at provider boundary with safe structured error.

### F3. Provider stop

Stop `portus-apid` through the approved test/admin path.

Expected:

```text
protected-api.request -> unavailable/degraded
Linux                 -> usable
SSH                   -> usable
ordinary Codex        -> usable
portusd                -> usable
PortusBrowser/others   -> independently usable
```

No plaintext/environment fallback occurs.

### F4. Restart/recovery

Restart the provider.

Expected: provider recovers the protected store/definitions and allowed use resumes without copying credentials into Master-readable locations.

---

## 9. Group G — compatibility proxy/token, when implemented

This group is mandatory only when the local compatibility proxy exists in the tested build.

Required tests:

- local token is at least 256 bits of cryptographically secure randomness before encoding;
- token is scoped to principal/credential/provider/operation set;
- default lifetime is 10 minutes;
- configured lifetime may not exceed 60 minutes in the first contract;
- token expires/revokes correctly;
- provider restart invalidates in-memory compatibility sessions/tokens;
- token cannot be used directly against the upstream provider;
- token cannot authorize an operation outside its scope;
- proxy binds loopback only;
- proxy cannot be made to escape the same provider binding/TLS/redirect constraints;
- possession of the token never exposes the upstream credential.

---

## 10. Secret-safe test evidence

Validation evidence may contain:

- test ID;
- timestamp;
- component/build version;
- safe credential reference;
- provider registration ID;
- caller UID;
- pass/fail;
- safe error/reason code;
- safe upstream status;
- hashes/fingerprints that cannot be used to reconstruct the credential.

Evidence must not contain:

- raw API key/token;
- Authorization header;
- secret-bearing database dump;
- full process environment containing secrets;
- unredacted crash/core data.

A test harness that leaks the credential while proving the credential cannot leak is itself a failed validation design.

---

## 10.1 P10 host-safe evidence already implemented

The active host-safe suite currently covers at least:

- strict protocol-v1 request/response shape and request-ID handling;
- rejection of guessed export actions and caller identity/destination/authentication fields;
- strict fixed HTTPS provider definitions and first-contract bounds;
- no administrative/raw-secret surface on `portus-api`;
- no reusable-secret argv path on `portus-auth` and secret-free output;
- P9 `allow`/`prompt`/`reject` behavior before secret/upstream use;
- rejected-caller anti-enumeration and allow-only safe credential discovery;
- redirect rejection and no upstream side effect on prompt/reject;
- audit preflight before protected upstream/store side effects;
- generation rotation/revocation behavior;
- injected-upstream proof that the provider can use the internal secret while public response/metadata paths do not export it.

Still blocking on Linux are the filesystem/store/WAL/SHM permission attempts, real socket peer identities and widened-admin-socket negative test, process argv/environment inspection, generic Portus surface scan, real TLS-certificate and redirect destination fixtures, OpenRC stop/restart/degradation behavior, cross-user installed transport behavior, and the real upstream credential proof.

---

## 11. Acceptance rule

The protected API provider is accepted only if:

- all required Group A positive tests pass;
- all applicable Groups B–F negative/degradation tests pass;
- Group G passes when compatibility proxy support is present;
- no test leaks the reusable credential into evidence;
- failures are fixed and the complete affected group is rerun.

A successful upstream call without the negative suite is not sufficient.

---

## 12. Decisions locked by this document

1. Protected-provider validation is explicitly adversarial, not happy-path only.
2. Tests cover filesystem/API/admin/process/generic-state export paths.
3. Tests include controlled destination/header/confused-deputy attacks.
4. All credential-bearing redirects are tested to fail closed in the first contract.
5. TLS bypass/self-signed/hostname/plain-HTTP paths are tested negatively.
6. Transport-group membership and semantic authorization are tested separately.
7. Cross-user credential-use isolation is a blocking security case.
8. Rotation/revocation/provider-stop/restart are part of the protected-secret proof.
9. Compatibility proxy tokens, when present, use at least 256-bit randomness, default 10-minute TTL and maximum 60-minute TTL in the first contract.
10. Validation evidence is itself secret-safe and cannot include raw credentials.
11. `USE succeeds / EXPORT fails` requires all applicable positive and adversarial cases, not one successful provider request.
