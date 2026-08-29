# PortusOS Protected Credential and Secret Boundary

**Status:** Authoritative protected-secret architecture
**Scope:** Raw reusable credentials, authentication material, recovery material, protected credential storage, secret-backed operations, AUTH workflows, provider/browser credentials, logging/redaction, backups, classification, and interaction with broad Master Portus user authority

## 1. Purpose and authority

This document is authoritative for the PortusOS boundary between:

- ordinary user data that Master Portus may read;
- private data belonging to another Linux user;
- protected reusable credentials and recovery material;
- authenticated capabilities that Master Portus may use without retrieving the reusable credential;
- administrator/root access to protected material;
- deliberate administrator reclassification of material into or out of the protected domain.

`docs/POLICY.md` is authoritative for Linux user identity, administrator authority, permission bundles and privilege; `docs/POLICY_IMPLEMENTATION.md` owns the first concrete Linux/elevation mapping. `docs/STATE.md` is authoritative for PortusOS SQLite/state storage and must exclude raw reusable secrets. `docs/PROTECTED_API_PROVIDER.md` is authoritative for the concrete first protected API credential provider implementation and `docs/PROTECTED_API_VALIDATION.md` for its adversarial acceptance suite. `docs/CREDENTIAL_CLASSES.md` owns first-ISO classification direction for Codex/browser/SSH-signing/password-manager/AUTH/backup classes while remaining subordinate to the general secret boundary here. Provider-specific documents such as `docs/PORTUS_BROWSER.md` may define stricter domain behavior but must respect this boundary.

This document is the canonical PortusOS authority for protected credentials and secret-backed operations.

## 2. Core distinction

PortusOS distinguishes **using an authenticated capability** from **retrieving the reusable secret that authenticates it**.

The desired pattern is:

```text
Master Portus
    |
    | authorised operation
    v
secret-backed capability/provider
    |
    | uses protected credential internally
    v
external/local authenticated service

Master Portus receives:
    result / capability output

Master Portus does not receive:
    password / private key / API token / reusable session credential
```

Broad control of a computer does not require every reusable secret on that computer to become readable data.

## 3. Interaction with the Master Portus Linux-user model

Master Portus is intentionally a broad, non-root Linux user and is not generally sandboxed.

Therefore the secret boundary must not depend on pretending that Master Portus cannot read ordinary files belonging to its own UID.

Any credential stored as an ordinary Master-readable file must be assumed readable by Master Portus.

A genuine protected secret must be enforced by a boundary independent of ordinary Master-user filesystem access, for example a separately protected service/account/provider store, hardware-backed mechanism, reviewed broker, or another mechanism that does not expose a general read-secret operation to the Master UID.

A same-user credential store or keyring must not automatically be assumed to provide this property merely because it encrypts data at rest. The selected implementation must prove that Master Portus can use the required authenticated capability without being able to retrieve the reusable value through its normal shell/application authority.

This is a validation requirement, not an assumption.

## 4. Secret classes

### 4.1 Raw reusable secrets

Raw reusable secrets are values that can generally be copied and reused to impersonate an identity or bypass a security boundary.

Examples include:

- passwords;
- long-lived API keys;
- refresh tokens;
- reusable bearer tokens;
- private signing/authentication keys;
- password-manager master credentials;
- browser credential databases where they expose reusable credentials;
- selected browser cookies/session tokens when they are reusable outside the intended provider boundary;
- recovery keys/passphrases;
- selected SSH private keys;
- administrator/root passwords;
- hardware-token recovery material;
- owner-designated credential material.

These belong to the protected domain unless the administrator explicitly chooses otherwise.

### 4.2 Short-lived scoped credentials

A short-lived token or task-scoped grant may be exposed to a tightly bounded process when necessary, but it remains sensitive.

Short-lived does not mean safe to log, persist, place in project files, or return to the model.

The preferred design is still to keep the credential inside the smallest responsible capability boundary.

### 4.3 Ordinary private user data

Ordinary user files are not automatically secrets merely because they are private or personal.

Files owned by the Master Portus Linux user are generally readable by Master Portus according to `docs/POLICY.md` unless they are intentionally placed behind the protected credential boundary.

Examples of ordinary private data may include documents, source code, application settings, media, notes, ordinary application caches, and user-created project files.

### 4.4 Other users' private data

Another Linux user's private files are protected by the Linux user boundary, not by this credential classification.

Master Portus A cannot read Master Portus B's private home/workspace by default even when neither file contains credentials.

## 5. Administrator/root authority

Root/the machine administrator remains capable of administering the protected secret infrastructure.

The secret boundary protects secrets from the general Master Portus authority; it is not intended to make the OS administrator cryptographically powerless over their own machine in all cases.

The administrator may:

- create/import/remove protected credentials;
- rotate credentials;
- select credential providers;
- grant or revoke secret-backed capabilities;
- perform recovery;
- deliberately export or reclassify credential material;
- repair broken secret infrastructure.

Administrator operations that reveal raw secrets must require an explicit administrator-controlled path and must not be callable merely because Master Portus has a broad normal policy profile.

## 6. Reclassification rule

The protected domain is defined by both classification and enforcement.

If the administrator deliberately writes an API key into a normal file readable by Master Portus, exports a private key into a Master-readable directory, or configures a provider with a general read-secret command available to Master Portus, that value is no longer protected from Master Portus in practice.

PortusOS should make that consequence explicit rather than claiming the secret boundary still exists.

Conversely, moving ordinary data into a protected provider does not automatically make every operation secret-backed; the provider contract must define how it can be used.

## 7. Protected storage requirements

A protected credential store must provide the security property required by its use case, not merely an encrypted file format.

For a credential intended to be unreadable by Master Portus, the implementation must prevent the Master UID from obtaining the raw value through:

- normal file reads;
- ordinary database access;
- provider CLI/API read/export operations;
- environment inspection;
- process arguments;
- diagnostics/logs;
- generic PortusOS IPC;
- browser/provider metadata interfaces;
- backups available to the Master UID.

Potential mechanisms may include:

- dedicated service identities;
- privileged local brokers;
- provider-owned protected stores;
- hardware-backed keys/tokens;
- kernel/user-key mechanisms with appropriate access semantics;
- password managers with non-exporting action APIs;
- narrowly scoped root-owned helpers;
- application/provider isolation that exposes operations rather than secret retrieval.

Exact providers remain implementation decisions and must be tested on Artix/OpenRC.

## 8. Keyring and Secret Service

PortusOS may use a Secret Service-compatible keyring where appropriate, but the implementation must distinguish two different goals:

1. **encrypted credential storage at rest**; and
2. **credential unreadability from the broad Master Portus user**.

A same-user keyring can satisfy the first property without necessarily satisfying the second.

Before a keyring is used for a credential that is promised to be inaccessible to Master Portus, validation must prove that the Master user cannot simply invoke an available keyring API/CLI to export the value.

If the chosen keyring does not provide the required non-export property, it may still be used for credentials that are intentionally user-readable, but those credentials must not be described as protected from Master Portus.

## 9. Secret-backed capability contract

A secret-backed capability exposes an operation, not a secret-fetch primitive.

Examples:

```text
git provider:
  allowed: perform authenticated repository operation
  not allowed: return stored access token

signing provider:
  allowed: sign approved payload
  not allowed: export private signing key

password manager:
  allowed: fill/submit approved credential to target
  not allowed: dump vault contents to Master Portus

browser provider:
  allowed: use authenticated browser session
  not allowed: export password/cookie/token database through the general agent interface
```

A capability contract should specify:

- provider identity;
- authenticated principal/account where safe to reveal;
- allowed operations;
- target/destination constraints;
- whether human approval is required;
- whether the credential is hardware-backed or software-backed;
- timeout/session lifetime;
- failure behavior;
- revocation behavior;
- audit metadata;
- explicit prohibition on returning raw reusable credentials.

### 9.1 Credential classification at provisioning

When a reusable credential is added, PortusOS must distinguish whether the administrator intends it to be **protected from Master Portus** or **Master-readable**.

Conceptually:

```text
Protected credential
  Master Portus may invoke authorised authenticated operations.
  Master Portus may not retrieve/export the reusable credential value.

Master-readable credential
  Master Portus may use, read, export, rotate, replace, or pass the credential to software within its effective policy.
  PortusOS must not describe this credential as protected from Master Portus.
```

Both modes are valid. An API key created specifically for an autonomous Master Portus instance may deliberately be Master-readable. A personal, shared, administrator, billing-sensitive, signing, recovery, or otherwise reusable credential may instead be protected.

The classification must reflect real retrievability, not merely a UI label. If the selected storage/provider allows the Master UID to export a supposedly protected value, the credential is user-readable in practice until that enforcement gap is fixed.

### 9.2 Protected API credentials and brokered use

A protected API credential must still be **usable** by Master Portus. Non-exportability must not turn the credential into an inert value that requires the human to perform every authenticated operation.

The target pattern for API keys and bearer credentials is:

```text
administrator / AUTH
        |
        | provision credential once
        v
protected credential provider
        |
        | stores reusable credential
        | does not expose raw value to Master Portus
        |
Master Portus
        |
        | provider-bound authenticated request
        v
protected provider / API broker
        |
        | attaches credential internally
        v
approved upstream service
        |
        v
response/result
        |
        v
Master Portus
```

For example, a protected OpenAI API key may be provisioned under a non-secret reference such as:

```text
openai/main
```

Master Portus may know that the reference exists, which provider/account it represents where safe, whether it is available, and what operations policy permits. Master Portus must not receive the underlying `sk-...` value.

A provider operation may conceptually accept:

```text
credential_ref = openai/main
provider_operation = API request
request_body = ...
```

The protected provider constructs the authenticated upstream request and returns the upstream result. The reusable API credential remains inside the protected boundary.

This pattern is not specific to OpenAI. It applies to API providers, repository providers, cloud services, signing systems, password managers, and other authenticated capabilities where the protected component can perform an operation on behalf of Master Portus.

### 9.3 Provider binding and indirect-exfiltration prevention

A credential broker must not become a generic credential-injection oracle.

For a protected credential, Master Portus must not be able to request behavior equivalent to:

```text
attach credential openai/main to arbitrary URL supplied by caller
return arbitrary Authorization header
follow an attacker-controlled redirect while forwarding the credential
run an arbitrary caller-supplied command with the credential in its environment
```

The protected provider must bind each credential to its intended provider/domain and contract. As applicable, the contract must enforce:

- approved upstream host/origin/service identity;
- TLS certificate verification;
- safe redirect behavior that never leaks authentication material to an unapproved origin;
- allowed authentication-header placement;
- provider/API operation classes;
- optional endpoint/model/account constraints selected by administrator policy;
- request/response size and timeout bounds;
- safe logging/redaction;
- revocation and failure behavior.

A provider may expose broad functionality when administrator policy permits, but the credential itself must remain bound to the approved provider boundary rather than becoming a reusable value or arbitrary network header.

### 9.4 Native interface and compatibility proxy

The preferred protected-provider control interface is local and identity-aware. Where practical, Unix-domain IPC should be preferred because the service can authenticate the calling Linux principal from kernel-provided peer credentials.

Some ordinary SDKs and applications require an HTTP API or expect an OpenAI-compatible/provider-compatible base URL rather than a custom Unix-socket capability. PortusOS may provide a local compatibility proxy for those applications.

Conceptually:

```text
Master-owned application / SDK
        |
        | provider-compatible request
        | no upstream reusable credential
        v
local protected compatibility proxy
        |
        | authenticates local caller
        | selects approved credential reference
        | attaches real credential internally
        v
approved upstream provider
```

A compatibility proxy must not require the application to receive the real upstream API key. Any local capability/session token used for compatibility must authorize only the local broker/proxy boundary and must not itself be a reusable upstream credential.

A loopback HTTP proxy must authenticate and authorize its local caller; `localhost` alone is not an authorization boundary. The exact authentication mechanism remains an implementation decision.

### 9.5 Arbitrary Master-owned programs

A protected credential is not directly injected into arbitrary code merely because Master Portus wrote or launched that code.

If an SDK, script, or application can operate through the protected provider or local compatibility proxy, it may use the protected credential without seeing it.

If software strictly requires possession of the real reusable credential—for example by requiring the upstream API key in a process environment with no broker/proxy option—then PortusOS has only two honest choices:

1. do not use that software path for the protected credential; or
2. deliberately reclassify/provision a Master-readable credential for that use case.

Passing the real credential into an arbitrary Master-owned process and then continuing to call it protected is prohibited.

### 9.6 Unattended protected-credential use

Protected does not mean interactive-only.

Administrator policy may classify a secret-backed operation as:

```text
allow
prompt/approval
reject
```

A credential configured for `allow` may be used repeatedly and unattended by Master Portus within the provider and operation constraints. The human does not need to approve every API request merely because the credential value itself is protected.

The protection boundary controls **credential export**. Policy separately controls **what authenticated operations Master Portus may perform**.

### 9.7 References and observable metadata

PortusOS may expose non-secret credential metadata to Master Portus and store it in generic Portus state when useful, for example:

- stable credential/capability reference such as `openai/main`;
- provider identifier;
- safe account/display label;
- configured/available/degraded state;
- allowed operation class or policy association;
- last-use timestamp where policy permits;
- revocation/health state.

Generic Portus state and `portusd` may coordinate these references and policy relationships, but they must not contain or transport the reusable credential value.

The protected credential provider owns raw credential material. `portusd` must not become the secret store merely because it coordinates the capability.

## 10. AUTH surface

`AUTH` is the dedicated human-operated authentication and secret-entry surface.

It is appropriate for:

- administrator/root password entry;
- initial Codex/provider authentication;
- password-manager unlock;
- recovery-key handling;
- device-code/browser login instructions;
- hardware-token interaction;
- other workflows where the human must directly handle authentication material.

AUTH is not itself the entire secret boundary.

After authentication, credentials must be stored/used according to the selected protected provider model.

Secrets entered through AUTH must not be copied into Master Portus conversation context, normal project files, Portus state, ordinary shell history, or logs.

## 11. Codex credentials

Codex authentication is a special case because Codex runs as part of the Master user workflow.

PortusOS must distinguish:

- a credential merely stored more safely than plaintext; and
- a credential genuinely inaccessible to the broad Master Portus user.

The first ISO must test the selected Codex credential storage path under the actual Master Portus account model.

If Codex must retrieve a reusable token through an API equally available to arbitrary commands running as the Master UID, PortusOS must not claim that token is protected from Master Portus merely because it resides in a keyring.

Acceptable outcomes include:

- a broker/provider path that allows Codex authentication without general Master-readable export;
- a provider mechanism whose credential cannot be retrieved through ordinary Master authority;
- an explicit classification that a particular credential is user-readable and therefore outside the hard protected domain.

The documentation must state which case applies.

## 12. Browser credentials and sessions

PortusBrowser may allow Master Portus to use an authenticated browser session without exposing raw credentials through its structured interface.

However, the operating-system boundary must also consider browser profile files accessible to the Master Linux UID.

If the Master user can directly read/copy reusable browser credential or session material from the browser profile, PortusOS cannot claim those values are protected solely because PortusBrowser omits an export command.

A protected browser-authentication design therefore requires either:

- storage/profile isolation that prevents raw retrieval by the Master UID;
- browser/credential mechanisms that keep reusable material non-exportable;
- or an explicit statement that the browser session material is user-readable and outside the hard protected domain.

PortusBrowser's own website/action policy remains separate from this OS-level credential boundary.

## 13. SSH and signing keys

SSH/private signing keys may use different classifications depending on owner intent.

Examples:

- an agent-owned development key intentionally readable by the Master user may be ordinary user credential material and is not protected from Master Portus;
- an owner identity key intended only for signing/authentication actions may remain protected behind an agent/non-exporting service;
- a recovery/admin SSH key must not become readable by Master Portus merely because SSH is installed.

The system should distinguish key identity/purpose rather than treating every `~/.ssh` file identically.

## 14. Environment variables and process boundaries

Environment variables are not a durable secret store.

Reusable credentials must not be globally exported from shell startup files or Portus configuration.

When a short-lived credential must be injected into a process:

- scope it to the smallest process tree possible;
- avoid command-line arguments where they may be observable;
- avoid persistence in task metadata/logs;
- clear/expire it promptly;
- consider whether same-UID process inspection makes the value readable by Master Portus.

If same-UID inspection exposes the value, environment injection cannot be used to claim a hard secret boundary from Master Portus.

A brokered operation is preferred when non-export is required.

## 15. PortusOS runtime and SQLite

Raw reusable secrets must never be placed in generic PortusOS state.

They are prohibited from:

- `/var/lib/portus/state/portus.db`;
- task payloads/results;
- system-index records;
- annotations;
- artifact metadata;
- event payloads;
- policy audit payloads;
- `portusd` generic request/response messages;
- diagnostic bundles;
- ordinary Portus logs.

Portus state may record safe metadata such as:

- protected provider/capability identifier;
- account label where safe;
- operation requested;
- policy/approval result;
- success/failure;
- timestamps;
- revocation/health state.

It must not record the reusable credential value.

## 16. Logs, audit, diagnostics, and crash data

Logs and diagnostics must be secret-safe by construction.

Components should redact or omit:

- authorization headers;
- bearer tokens;
- passwords;
- API keys;
- private keys;
- cookies/session tokens;
- credential-bearing URLs;
- process environments containing secrets;
- full provider responses that may echo credentials.

A redaction failure is a security defect, not a harmless logging issue.

Diagnostic bundles must use explicit allowlisted fields where practical rather than copying complete configuration directories.

## 17. Shell history, clipboard, and conversation context

Reusable credentials must not be intentionally placed in:

- ordinary shell command history;
- clipboard history;
- Master Portus prompts/conversation transcripts;
- project documentation;
- `AGENTS.md`;
- skills/rules/hooks source files;
- OKF knowledge;
- Git repositories;
- screenshots retained beyond the required authentication interaction.

Clipboard use during human authentication must avoid persistent history where possible.

## 18. Backups and recovery material

Backup copies of protected credentials remain protected credentials.

Backup policy must preserve:

- equivalent or stronger access control;
- encryption where required;
- retention limits;
- separation from Master-readable ordinary backups when the source secret is intended to be unreadable by Master Portus;
- recovery documentation controlled by the administrator.

Recovery keys and administrator credentials must not be stored in the same Master-readable state that they are intended to recover from.

## 19. Provider and application requirements

A provider that claims to be secret-backed must document:

- where credential material is stored;
- which OS principal can read/export it;
- whether Master Portus can retrieve it through any provider interface;
- which authenticated operations are exposed;
- whether the provider can run under a separate service identity;
- how revocation/logout works;
- how logs are scrubbed;
- how backups are handled;
- what happens when the provider is unavailable.

Applications that store credentials as ordinary Master-readable files are still usable, but those credentials are not protected from Master Portus.

PortusOS must prefer accurate classification over pretending every third-party application has a non-exporting credential boundary.

## 20. Secret-backed operation approvals

Secret-backed use does not inherently require a prompt every time.

The administrator may grant a capability for unattended use.

Policy may classify an operation as:

```text
allow
prompt/approval
reject
```

Independent restrictions may apply to:

- target account;
- remote host/origin;
- operation class;
- data being signed/transmitted;
- task/project;
- duration;
- amount/rate;
- artifact confidentiality.

The secret itself remains non-exportable regardless of whether the operation is pre-approved.

## 21. Failure and degradation

When protected credential infrastructure is unavailable:

- the system must report the affected capability as unavailable/degraded;
- Master Portus must not fall back silently to plaintext credential storage;
- raw credentials must not be copied into project/user files merely to keep automation running;
- unaffected Linux/Codex/Portus operations remain available;
- human re-authentication through AUTH may be requested where appropriate;
- administrator recovery must remain possible.

Failing closed for secret retrieval must not mean failing the entire OS closed.

## 22. Validation requirements

The first supported secret-boundary implementation must prove, for each credential claimed to be protected from Master Portus, that:

- Master Portus can perform the intended authenticated operation without receiving the reusable credential value;
- Master Portus cannot read the raw credential through normal filesystem access;
- Master Portus cannot retrieve it through an exposed provider/keyring CLI or API;
- Master Portus cannot obtain it from generic Portus state/IPC;
- a protected provider/API broker cannot be coerced into returning the credential in headers, metadata, diagnostics, error text, or response bodies;
- a protected provider/API broker cannot be instructed to attach the credential to an arbitrary caller-selected destination;
- redirect handling cannot forward authentication material to an unapproved host/origin;
- TLS verification and approved upstream identity checks behave as documented;
- any local compatibility proxy authenticates/authorizes local callers rather than treating loopback access alone as trusted;
- any local compatibility credential/token is usable only against the local proxy and is not itself a reusable upstream credential;
- arbitrary Master-owned scripts/programs cannot obtain the protected credential merely because they can invoke the broker/proxy;
- logs/diagnostics do not contain it;
- shell history and project files do not contain it;
- user-readable backups do not contain it;
- revocation/logout behaves as documented;
- administrator/root recovery remains possible;
- another Master Portus user cannot read or use the protected credential unless explicitly granted a corresponding capability;
- a broad permission bundle does not accidentally expose it.

Negative tests are mandatory. Merely confirming that an authenticated CLI, broker, SDK proxy, or provider call works is insufficient evidence of secret isolation.

For provider-bound API credentials, validation must demonstrate both halves of the contract:

```text
USE succeeds
EXPORT fails
```

A protected OpenAI-style API credential, for example, is correctly implemented only if Master Portus can make the approved API request while attempts to recover the upstream API key remain unsuccessful.

## 23. Decisions locked by this document

The following are now locked:

1. Broad Master Portus authority does not automatically include raw reusable credential retrieval.
2. A credential stored in an ordinary file readable by the Master UID must be treated as readable by Master Portus.
3. A same-user keyring is not automatically considered a hard boundary from Master Portus; the selected mechanism must prove non-export behavior.
4. Protected secrets require an enforcement boundary independent of ordinary broad Master-user filesystem authority.
5. Secret-backed capabilities expose operations rather than generic secret-fetch functions.
6. Credentials are deliberately classified at provisioning as either protected from Master Portus or Master-readable; both modes are valid and the classification must match real retrievability.
7. A protected credential must remain usable by Master Portus through approved provider-bound operations; non-exportability must not require the human to perform every authenticated request.
8. Protected API keys/bearer credentials use a broker/provider pattern in which the protected component attaches the reusable credential internally and returns the provider result without returning the credential.
9. Credential brokers/providers must bind credentials to approved upstream providers/domains/contracts and must not expose arbitrary destination/header/command injection paths that could exfiltrate the credential indirectly.
10. Provider-bound protected use may be broad and unattended when policy is `allow`; credential protection and operation approval are separate concerns.
11. Unix-domain IPC is preferred for native protected-provider integration because the service can authenticate the local Linux principal from kernel peer credentials.
12. A local provider-compatible/HTTP compatibility proxy may be used for ordinary SDKs/applications when needed, but it must authenticate/authorize callers, must not rely on `localhost` alone as an authorization boundary, and must not expose the real upstream credential.
13. Any compatibility token/session identifier exposed to Master-owned software may authorize only the local proxy/broker boundary; it must not be a reusable upstream credential.
14. Arbitrary Master-owned software does not receive a protected credential directly. If software strictly requires possession of the real reusable credential and cannot use the broker/proxy, the credential must either remain unusable through that software path or be deliberately reclassified/provisioned as Master-readable.
15. Passing a real protected credential through a Master-owned process environment, command line, file, or other same-UID-readable channel invalidates the claim that it is protected from Master Portus.
16. Root/the administrator controls protected credential provisioning, recovery and deliberate reclassification.
17. Raw reusable secrets are prohibited from generic Portus SQLite state, `portusd` IPC, logs, diagnostics, tasks, artifacts, index records and knowledge.
18. Generic Portus state may store non-secret credential references, provider IDs, safe labels, availability/health and policy relationships, but not reusable credential values.
19. `portusd` may coordinate credential references and policy relationships but is not the protected credential store and must not transport reusable credential values through generic runtime IPC.
20. AUTH is the human secret-entry surface but not the sole security boundary.
21. Provider/browser/Codex credentials must be classified according to their real retrievability by the Master UID.
22. Environment-variable injection does not count as non-exporting if same-UID process inspection exposes the value.
23. Protected-secret backups must preserve equivalent protection.
24. Secret-provider failure must not silently downgrade to plaintext/user-readable storage.
25. The administrator may deliberately reclassify/export a credential, but PortusOS must then stop claiming that value is protected from Master Portus.
26. Multiple Master Portus users must remain isolated from one another's protected credential material and protected capabilities unless explicitly authorised.
27. Every claimed protected credential path requires negative tests proving Master Portus cannot retrieve the reusable value or exfiltrate it indirectly through the provider interface.
28. The first protected API credential implementation is locked by `docs/PROTECTED_API_PROVIDER.md`: `portus-apid` runs as `portus-api`, stores protected API credentials outside generic Portus state, authenticates native callers through Unix peer credentials, exposes named provider operations rather than arbitrary credential forwarding, requires verified TLS, rejects credential-bearing redirects in the first contract, and uses root-only administration plus optional scoped local compatibility tokens.
29. `portus-apid` is API-credential-specific and must not silently become a universal secret broker for unrelated credential domains.

## 24. Remaining implementation decisions

The general secret architecture, first protected API data/wire contract, adversarial validation contract and first credential-class classification are locked by `docs/PROTECTED_API_PROVIDER.md`, `docs/PROTECTED_API_VALIDATION.md`, and `docs/CREDENTIAL_CLASSES.md`. P10 has now implemented the host-safe `portus-apid`/`portus-api`/`portus-auth` stack, provider-owned store, policy/audit mapping, strict HTTPS provider binding and non-echo TTY AUTH path. Remaining work is installed-system evidence or separate credential-class research rather than a reason to weaken classification:

- install/integrate `portus-apid` under OpenRC, install its provider manifest/definition/skill, prove service/store/socket/audit ownership and peer identities, and complete the full Linux/real-upstream adversarial suite;
- verify the currently supported Codex authentication mechanisms against the protected local-proxy option; if that proof fails, keep the credential Master-readable as already required by `docs/CREDENTIAL_CLASSES.md`;
- design a real browser-profile/session isolation boundary only if protected browser sessions become a requirement;
- select protected SSH/signing/password-manager/hardware mechanisms when those capabilities are required;
- define future protected backup/restore encryption/recovery only when automated protected-secret backup becomes required;
- define any later provider-specific audit/rate-limit or migration policy beyond the bounded common P10 audit contract when a concrete need appears.

Those decisions must conform to the boundary above rather than weakening it implicitly.
