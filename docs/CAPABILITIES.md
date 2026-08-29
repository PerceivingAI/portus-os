# PortusOS Capability and Provider Contract

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative architecture and product decision
**Adopted:** 2026-08-21
**Scope:** How PortusOS discovers, identifies, relates, validates, monitors, and coordinates specialised capability providers without duplicating Codex, Linux, application-owned interfaces, or provider-owned state

## 1. Authority and core decision

This document is authoritative for PortusOS capability-provider integration.

It is subordinate to:

- `docs/POLICY.md` for identity, authority, and policy;
- `docs/SECRETS.md` for protected credentials and secret-backed operations;
- `docs/TASKS.md` for execution routing, task promotion, task lifecycle, and managed-versus-associated execution;
- `CODEX_UPDATES.md` for Codex-native execution, sessions, subagents, queue/fork/resume, compaction, skills, and related harness behavior;
- `docs/CONTROL_PLANE.md` for the non-duplication rule.

The core decision is:

> **A PortusOS capability is a discoverable semantic ability supplied by a known source/provider. A registered capability provider is a specialised component that PortusOS can identify, health-check, relate to resources/tasks/policy, and recover or degrade safely. Capability registration does not turn PortusOS into the provider's invocation proxy or copy the provider's domain model into `portusd`.**

A provider normally remains directly usable through its own CLI, API, IPC, proxy, or other supported interface.

PortusOS owns only the general integration facts that have system-wide value:

- provider identity and registration;
- semantic capabilities;
- compatible interfaces;
- lifecycle ownership;
- health and degradation;
- caller/policy relationships;
- provider-owned resource references;
- task/artifact relationships;
- update compatibility;
- recovery and uninstall history.

The provider remains authoritative for its own internal resources, operations, policy, data, and implementation.

---

## 2. Five concepts must remain separate

PortusOS uses five related but distinct concepts.

### 2.1 Capability

A **capability** answers:

> What class of need can this machine satisfy through this provider?

Examples across current and possible provider classes include:

```text
browser.control
protected-api.request
visual.capture
visual.input
presentation.window
```

The first-ISO PortusBrowser manifest intentionally exposes only `browser.control`; additional browser capability classes are not implied by individual CLI subcommands.

A capability is semantic and intentionally coarse. It is not a complete command list.

### 2.2 Provider

A **provider** is the component that implements one or more capabilities.

Examples include:

- PortusBrowser;
- the selected protected API/credential provider;
- future PortusWindow;
- future specialised delivery or communication providers.

A provider can expose several capabilities and several interfaces.

### 2.3 Interface

An **interface** tells an authorised caller how the provider is used.

Examples include:

- a provider-owned executable/CLI;
- a Unix-domain socket protocol;
- a local authenticated provider-compatible HTTP proxy;
- a reviewed Portus adapter around a native/provider API;
- a future explicitly supported interface class.

An interface is not itself a capability.

### 2.4 Resource

A **resource** is an object owned by the provider that PortusOS may need to reference or relate to the rest of the machine.

Examples:

- a PortusBrowser browser session;
- a browser tab or other provider resource when a durable relationship is justified;
- a protected credential reference such as `openai/main`;
- a future PortusWindow instance.

PortusOS stores opaque references and safe relationship metadata rather than duplicating complete provider state.

### 2.5 Operation

An **operation** is one invocation or provider-side unit of work.

Examples:

- open/navigate a browser resource;
- perform an authenticated API request;
- render or present a resource through a future display provider.

A provider operation may be synchronous and complete immediately or may expose a provider-owned operation reference that can be queried, cancelled, reconciled, or related to a Portus task.

These five concepts must not be collapsed into one generic "tool" abstraction.

---

## 3. Capability registration is provider-centric, not tool-centric

PortusOS must not construct a second catalogue of every action available to the agent.

Codex already owns or exposes capabilities such as:

- file read/edit/search;
- patching;
- shell execution;
- Git/project work;
- Codex root sessions;
- subagents;
- queue/fork/resume;
- context compaction;
- skills;
- Codex review/automation surfaces.

Linux and installed applications already expose capabilities such as:

- directory/file inspection;
- process inspection;
- package inspection;
- OpenRC operations;
- i3/X11 operations;
- ordinary application CLIs/APIs.

PortusOS must not manufacture provider records such as:

```text
capability.file.read
capability.shell.execute
capability.git.commit
capability.codex.spawn-subagent
capability.codex.compact
```

merely to make every computer action look uniform.

The machine map may still report that Codex, Git, OpenRC, a screenshot mechanism, or an application is available. That does not mean every source must implement this provider contract.

The provider contract exists for **intentionally registered specialised integrations** that benefit from shared discovery, lifecycle, health, policy, task/resource, compatibility, or recovery semantics.

---

## 4. Codex is the execution harness, not a normal provider

Codex is a foundational execution/orchestration harness for Master Portus.

Codex owns:

- root sessions/threads;
- subagent trees;
- queue/fork/resume behavior;
- goals;
- compaction;
- skills;
- Codex-native permission profiles and execution behavior.

PortusOS integrates Codex through the dedicated Codex adapter and the rules in `CODEX_UPDATES.md`.

PortusOS must not force Codex-native concepts through the generic provider model merely for symmetry.

For example, PortusOS should not require:

```text
provider = codex
capability = subagent.spawn
```

before Master Portus can use a Codex subagent.

A Portus task may reference a Codex session/subagent as defined in `docs/TASKS.md`, but that relationship does not turn Codex into a generic capability provider.

---

## 5. Registered providers remain directly usable

Provider registration does not imply that every invocation must pass through `portusd` or `portus-os`.

The normal PortusBrowser path may remain:

```text
Master Portus / delegated Codex worker
  -> portus-browser
  -> PortusBrowser Broker
  -> browser
```

A protected API provider may expose:

```text
Master-owned application / Codex-created client
  -> authenticated local provider-compatible proxy
  -> protected provider/broker
  -> approved upstream service
```

PortusOS may discover, authorize, relate, health-check, or task-associate those providers without becoming a mandatory byte-for-byte forwarding path.

The first architecture therefore does **not** require a universal command such as:

```text
portus-os capability invoke <provider> <operation> ...
```

A future generic invocation abstraction may be proposed only if it adds a real cross-provider semantic capability and passes the inclusion test in `docs/CONTROL_PLANE.md`.

The default remains:

> Use the provider's native structured interface directly when no Portus-owned coordination is required.

---

## 6. Provider definitions and runtime registrations are separate

PortusOS must distinguish a provider implementation/type from a particular installed/configured registration generation.

### 6.1 Provider type

A provider type is the stable human/technical identity of the implementation family, for example:

```text
portus-browser
```

Provider type IDs must be:

- stable across normal software upgrades;
- lowercase ASCII;
- machine-readable;
- independent of display label;
- independent of process/PID;
- independent of provider software version.

The initial allowed form is:

```text
[a-z0-9][a-z0-9._-]*
```

with implementation enforcing a bounded length.

### 6.2 Provider registration

A provider registration is the PortusOS record for one installed/configured provider generation in a given scope.

Its immutable external ID is:

```text
provider_<UUIDv7>
```

The registration ID is used for durable Portus relationships because a provider may later be removed, replaced, reconfigured, or installed again.

A registration records or references at least:

- `provider_registration_id`;
- `provider_type`;
- scope;
- owning principal where user-scoped;
- manifest/schema identity;
- installed software version observation where available;
- compatibility state;
- lifecycle ownership;
- current health observation;
- registration/tombstone timestamps.

A reinstall may create a new registration generation rather than causing historical resource/task relationships to silently bind to a different installation.

Implementation may preserve/restore a prior registration only when identity continuity is explicitly proven and safe.

---

## 7. Provider scope and principal ownership

Provider scope must be explicit.

The first contract supports the conceptual scopes:

```text
system
user
```

### System-scoped provider

Installed and registered for the machine, while individual use/resources may still be user-scoped.

PortusBrowser and a system protected-provider service may be system-scoped even though browser sessions or credential references belong to individual users.

### User-scoped provider

Configured specifically for one Linux/Portus principal.

User-scoped provider registrations and private provider resources must not become visible to other Master users by default.

The provider manifest never overrides the principal-isolation rules in `docs/POLICY.md`.

For the first ISO, root/admin-controlled system provider manifests are sufficient. General untrusted user-defined provider manifests are not required.

---

## 8. Capability IDs and granularity

Capability IDs describe semantic ability classes.

They use lowercase dot-separated names such as:

```text
browser.control
protected-api.request
visual.capture
```

For the first-ISO PortusBrowser integration, `browser.control` is the only registered browser capability.

The rules are:

1. capabilities are coarser than commands;
2. adding a provider command does not automatically add a new Portus capability;
3. a capability should be meaningful for discovery/routing across provider implementations;
4. provider-specific details belong in provider skills/docs/interfaces;
5. capability names must not encode volatile package versions or executable paths.

Bad capability decomposition:

```text
browser.open-tab
browser.close-tab
browser.click
browser.type
browser.scroll
browser.wait
```

Those are operations/features of the browser interface, not separate machine-level capabilities merely because commands exist for them.

A provider may publish finer feature information inside its own contract, but the Portus capability catalogue remains intentionally small.

---

## 9. Capability contract versions

Provider versioning uses separate dimensions.

At minimum PortusOS must distinguish:

1. **manifest schema version** — how the provider definition is parsed;
2. **provider software version** — the installed provider release;
3. **interface contract version** — the protocol/CLI/API contract exposed through one interface;
4. **capability contract version** — the semantic version of a capability relationship where Portus integration depends on specific semantics.

These versions must not be collapsed into one field.

Example:

```text
PortusBrowser software: 0.1.0
Portus provider manifest schema: 1
portus-browser CLI contract: 1
browser.control capability contract: 1
```

A provider software upgrade may remain compatible with the same interface/capability contracts.

PortusOS must validate compatibility explicitly rather than assuming a newer provider is always compatible.

---

## 10. Provider manifest

Every intentionally registered first-class provider must have a declarative provider manifest controlled by the appropriate trusted installation/configuration boundary.

For first-ISO system providers, the target location is under:

```text
/etc/portus/capabilities/
```

P5 locks first-ISO system-provider manifests to TOML schema version `1`, with filenames exactly `<provider-type>.toml`. The durable registration ID still lives in Portus state and is never inferred from the filename.

### 10.1 Required manifest domains

A provider manifest must define at least:

- manifest schema version;
- provider type and display label;
- declared scope support;
- provider integration/version metadata;
- one or more semantic capabilities;
- one or more supported agent/system interfaces where applicable;
- provider-owned resource types that Portus may reference, if any;
- lifecycle ownership/integration mode;
- health/compatibility integration metadata;
- policy-domain ownership metadata;
- skill/documentation references where supplied;
- operation/task affordance mechanism where the provider exposes trackable operations.

### 10.2 First implementation manifest serialization

P5 locks the first machine-readable serialization to strict TOML schema version `1`. Unknown fields are rejected. A representative provider definition is:

```toml
manifest_version = 1
skills = ["portus-browser"]

[provider]
type = "portus-browser"
label = "PortusBrowser"
scope_support = ["system"]
software_version = "0.1.0"

[[interfaces]]
id = "cli"
type = "executable"
contract_version = 1
executable = "/usr/bin/portus-browser"
structured_output = true

[[capabilities]]
id = "browser.control"
contract_version = 1
interfaces = ["cli"]

[[resources]]
type = "browser-session"
authority = "provider"
lifetime = "session"

[lifecycle]
owner = "provider-owned"

[health]
kind = "structured-cli"
reference = "cli"

[policy]
domain_owner = "provider"
```

This example is now the concrete first-ISO PortusBrowser P15 shape rather than a hypothetical richer capability set. Broker startup remains provider-owned/native-host-on-demand as defined in `docs/PORTUS_BROWSER.md`.

The first parser is bounded and declarative: at most 128 manifests per reconciliation, at most 256 KiB per manifest, provider types at most 64 bytes, labels at most 128 bytes, general machine identifiers at most 96 bytes, versions at most 64 bytes, interface targets at most 512 bytes, and bounded interface/capability/resource/skill collection sizes. Detailed provider operations remain outside this manifest.

### 10.3 Manifest restrictions

A manifest is declarative metadata, not a script.

It must not contain arbitrary shell fragments such as:

```text
health_check: curl ... | sh
start_command: arbitrary user-selected shell string
```

Provider integration may use schema-bounded executable paths, typed arguments/operations, adapters, service identities, or known protocol contracts.

A manifest must not:

- contain raw reusable secrets;
- grant permissions to itself;
- bypass administrator policy;
- execute arbitrary code merely because the manifest is discovered;
- silently install packages/plugins;
- define unrestricted credential forwarding;
- make a provider trusted merely because a file exists.

---

## 11. Interface discovery

A provider may expose more than one interface.

The first architecture recognizes interface classes such as:

### 11.1 Executable/CLI

A structured provider-owned executable such as:

```text
/usr/bin/portus-browser
```

The manifest identifies the executable and interface contract. Detailed command usage belongs in provider documentation/skill, not in the manifest or Master `AGENTS.md`.

### 11.2 Unix-domain IPC

A local provider protocol with a documented socket/interface contract.

Caller authentication and authorization must follow the provider/security contract and must not trust arbitrary caller-supplied identity.

### 11.3 Local authenticated compatibility proxy

A loopback/provider-compatible interface used when an SDK/application requires HTTP-style access.

Loopback transport alone is not authentication. The protected-provider rules in `docs/SECRETS.md` apply.

### 11.4 Portus/native adapter

A reviewed adapter may translate a stable native/provider interface into Portus relationships/health/lifecycle when the provider itself does not expose a suitable generic registration protocol.

The adapter must not duplicate the provider's whole domain model.

### 11.5 Future interface classes

Additional interface classes may be added explicitly when justified. A new interface type must define its trust, identity, lifecycle, and compatibility behavior rather than being treated as an arbitrary URI/command escape hatch.

---

## 12. Skills and provider documentation

Provider manifests describe machine-readable integration. Skills describe agent-usable procedures.

For example:

```text
provider manifest
  -> PortusBrowser exists
  -> browser.control is available
  -> use interface cli
  -> health/lifecycle/resource contracts

portus-browser skill
  -> how to list sessions
  -> how snapshots work
  -> how to navigate/click/type/wait
  -> provider-specific recovery and usage patterns
```

Provider skills do not grant authority and do not make a provider installed.

A provider may reference an associated skill ID/location so Master Portus can progressively disclose detailed instructions when needed.

`AGENTS.md` should not copy the provider's command manual.

The general `portus-os-cli` skill must not absorb provider-specific usage instructions that belong to independent provider skills.

---

## 13. Provider-owned resource references

PortusOS must be able to relate provider resources to tasks, applications, processes, windows, users, artifacts, and the system index without copying provider state.

The common resource reference contains conceptually:

```text
provider_registration_id
resource_type
resource_id
```

Optional safe metadata may include:

- resource generation/version token where the provider exposes one;
- provider-safe display label;
- observed validity/freshness;
- owner principal where required;
- safe relationship hints.

### 13.1 Opaque provider resource ID

`resource_id` is provider-owned and opaque to generic PortusOS code.

`portusd` must not parse semantic meaning out of a browser tab ID, credential ref, future window-provider ID, or other provider identifier unless a dedicated provider adapter contract explicitly requires it.

### 13.2 Resource lifetime

Providers must describe or expose resource lifetime semantics sufficient for PortusOS not to treat ephemeral resources as indefinitely durable.

Conceptual lifetime classes are:

```text
ephemeral
session
durable
```

Examples:

- a browser tab may be ephemeral/session scoped;
- a browser session is normally session scoped;
- a credential reference may be durable while the credential remains configured;
- a future presentation instance may be ephemeral.

Lifetime does not imply Portus ownership.

### 13.3 Revalidation

Before an important mutation using an old provider resource reference, the caller/provider integration must revalidate the resource when staleness could target the wrong object.

A stale reference must fail clearly rather than silently bind to a different provider resource.

---

## 14. Provider operation references and task affordances

Not every provider command needs a durable operation object.

Short synchronous operations may return a bounded result and no persistent provider operation reference.

When a provider operation is asynchronous, independently trackable, or participates in a Portus task, the provider integration should expose an opaque operation reference plus enough affordance information for PortusOS to behave honestly.

The common task-relevant affordances are:

- **status query** — can current operation state be queried?;
- **cancellation** — can cancellation be requested and confirmed?;
- **pause/resume** — supported or not supported where applicable;
- **reconciliation** — after `portusd` restart, can the operation be matched to authoritative provider state?;
- **retry safety** — is retry forbidden, idempotent, idempotency-key protected, or otherwise provider-declared safe?;
- **progress/events** — can significant progress be polled/subscribed?;
- **result reference** — can a stable bounded result/provider reference be obtained?;
- **artifact candidate** — can output files/resources be identified for later Portus artifact registration?;
- **cleanup semantics** — who owns cleanup and what action, if any, is supported?;
- **timeout behavior** — what provider-level timeout semantics apply?;
- **health dependency** — which capability/provider health state affects continuation?

These affordances may be declared by operation class or returned for a concrete operation. The provider manifest does not need to enumerate every CLI subcommand merely to express them.

`docs/TASKS.md` remains authoritative for what Portus does with those affordances.

If a provider does not support cancellation, Portus task cancellation must not pretend otherwise.

If a provider cannot reconcile an operation after restart, the task layer must be able to transition to `interrupted` rather than inventing state.

---

## 15. Managed versus associated provider relationships

The `docs/TASKS.md` managed-versus-associated distinction applies to provider relationships.

Examples:

```text
associated:
  task references an existing PortusBrowser browser session
  provider owns the session
  Portus does not claim session lifecycle ownership

managed/contracted operation:
  Portus starts a provider operation through a contract that explicitly
  supports start/status/cancel/reconcile/cleanup semantics
```

Provider registration never implies that PortusOS manages all provider resources.

Lifecycle control must be claimed only for the exact provider component/operation/resource where the contract supports it.

---

## 16. Provider lifecycle ownership

Provider lifecycle ownership must be explicit.

The common lifecycle ownership classes are:

```text
portus-supervised
provider-owned
user-owned
external
```

### `portus-supervised`

PortusOS/OpenRC owns startup/supervision of a provider service component under a defined service contract.

### `provider-owned`

The provider controls its own internal lifecycle, and PortusOS only observes/integrates it.

### `user-owned`

The resource/process is intentionally controlled by the user/application rather than by PortusOS.

### `external`

The lifecycle is outside the local PortusOS/provider control boundary.

A provider can contain resources with different lifecycle ownership from the provider service itself.

For example:

```text
PortusBrowser Broker
  -> provider-owned
  -> native host starts it on demand when missing

Chromium browser process/session
  -> user/application/provider resource
  -> not automatically Portus-managed merely because the Broker is integrated
```

This distinction must be visible to task cleanup/recovery logic.

---

## 17. Health and availability

Health, compatibility, capability availability, and resource availability are separate concepts.

### 17.1 Provider health

The common provider health vocabulary is:

```text
healthy
degraded
unavailable
unknown
```

A health observation should carry:

- observed timestamp;
- bounded reason code;
- optional safe detail/reference;
- source of the observation.

### 17.2 Compatibility

The provider registration compatibility state is conceptually:

```text
compatible
incompatible
unknown
```

An incompatible provider must not crash `portusd` or cause the complete OS runtime to fail.

It should be reported clearly as unusable/degraded for the affected integration.

### 17.3 Per-capability availability

A healthy provider may expose some unavailable or degraded capabilities.

A capability observation therefore may be:

```text
available
degraded
unavailable
unknown
```

with a bounded reason.

### 17.4 Resources are separate

A capability can be available even when no current target resource exists.

For example, PortusBrowser Broker may be healthy and `browser.control` may be implemented while zero browser sessions are currently connected.

The system index/resource query should report zero resources rather than misclassifying all provider health solely from resource count.

---

## 18. Health checks must be bounded and typed

Provider health integration must not execute arbitrary manifest-provided shell strings.

Health may be derived from reviewed mechanisms such as:

- a typed provider health operation;
- an OpenRC/service/process check where service ownership is known;
- a structured CLI health/status contract;
- a provider adapter;
- a protocol heartbeat;
- bounded combination of those sources.

Detailed diagnostics remain provider-owned where appropriate.

PortusOS stores or surfaces only enough health metadata for discovery, degradation, task reconciliation, diagnostics, and recovery.

Provider logs/diagnostic payloads must not be copied wholesale into generic Portus state.

---

## 19. Policy ownership and effective availability

Provider registration does not grant authority.

Effective use requires the applicable combination of:

```text
provider registered/installed
AND compatible
AND provider/capability available
AND caller has PortusOS authority
AND provider-domain policy allows the operation
AND required provider resource exists/is usable
```

Not every condition applies identically to every provider, but no manifest bypasses policy.

### 19.1 PortusOS outer policy

PortusOS may control matters such as:

- which principals may use the provider;
- whether the provider service may run;
- provider-level network/device/filesystem access;
- which task/project contexts may invoke it;
- selected privileged or protected operation classes.

### 19.2 Provider-domain policy

The provider owns domain-specific policy.

For PortusBrowser this includes browser origin/action/settings policy.

For a protected API provider this includes the provider/credential/target/operation binding required by `docs/SECRETS.md`.

The effective rule is:

```text
PortusOS outer policy
  AND
provider-domain policy
```

Neither layer silently overrides the other.

### 19.3 Secret-backed providers

Permission to use a secret-backed capability does not imply permission to export the credential.

The protected provider contract must preserve:

```text
USE succeeds
EXPORT fails
```

where the credential is classified as protected.

---

## 20. Caller identity

System provider integrations must preserve the authenticated caller/principal boundary where provider use or resources are user-scoped.

`portusd` derives caller identity from Unix peer credentials for its own IPC as defined in `docs/POLICY.md`.

A separate provider interface must use an appropriate local identity/authentication mechanism for its transport and threat model.

Rules:

- a caller-supplied username is not identity proof;
- loopback networking alone is not identity/authentication;
- provider resources belonging to one Master user must not leak to another by default;
- system provider registration does not imply cross-user resource visibility;
- provider-owned authorization may be stricter than PortusOS outer authorization.

---

## 21. Outputs and artifacts

Provider outputs remain provider-owned unless PortusOS explicitly registers an artifact or relationship.

A provider operation may return:

- a bounded inline result;
- a provider resource reference;
- a file/resource locator that may become an artifact candidate;
- an already registered artifact reference where the integration explicitly supports it.

The existence of a downloaded/generated file does not automatically require artifact registration.

`docs/ARTIFACTS.md` defines when output becomes a Portus artifact and locks identity, locator, integrity, confidentiality, retention, cleanup, and deferred-delivery boundaries.

Provider output content must not be duplicated into task state merely because a task references the operation.

---

## 22. Capability registry versus system index

The capability registry and system index are related but not interchangeable.

### Capability registry

Answers intentionally registered integration questions such as:

- which provider registrations exist?;
- what capabilities do they declare?;
- which interfaces/contracts are available?;
- are they compatible?;
- what lifecycle/policy domains apply?;
- what is their current high-level health?

### System index

Answers machine-state questions such as:

- which provider components/processes are running now?;
- which provider resources are currently present?;
- which windows/processes/tasks/users relate to them?;
- what is fresh/stale?;
- which installed applications exist even without a provider registration?

The relationship is:

```text
provider definition/registration
  -> capability catalogue
  -> system index observes live provider/resources/relationships
  -> provider remains authoritative for detailed domain state
```

The index may cache provider health/resource observations, but it must not replace provider authority.

`docs/SYSTEM_INDEX.md` is authoritative for the live graph built around these registrations/resources. It adds generation-scoped `idx_<UUIDv7>` handles only as current-index navigation identities, preserves the opaque provider-resource reference unchanged, and correlates provider resources with processes/windows/tasks/users using source-attributed evidence. A stale index handle must never cause an old provider resource reference to bind silently to a different registration/session generation.

---

## 23. Ordinary installed software is not automatically a provider

PortusOS may discover an installed application through pacman, PATH, `.desktop` files, or other native sources without registering it as a capability provider.

Examples:

- Alacritty can be installed and launchable without a provider manifest;
- `git` can be installed without a provider manifest;
- an editor can be installed and controllable through its normal CLI/window without a provider manifest.

Provider registration is intentional and justified by a stable integration contract.

This keeps the provider registry small and meaningful.

---

## 24. Provider update compatibility

Provider updates must not silently change the contract PortusOS relies on.

After a provider update, PortusOS should re-evaluate:

- provider software version;
- manifest schema compatibility;
- interface contract compatibility;
- capability contract compatibility;
- health;
- provider skill/documentation compatibility where relevant;
- native integration registration such as browser native messaging where applicable.

If compatibility cannot be established:

```text
provider remains installed
but integration is incompatible/degraded/unavailable
```

rather than:

```text
portusd crashes
or
PortusOS silently guesses the new contract
```

Compatibility shims are not implied. If a hard cutover is selected for a first-party provider contract, the manifest/adapter and PortusOS integration must update together.

---

## 25. Degradation and provider failure

A provider failure degrades the capabilities supplied by that provider; it does not make ordinary Linux/Codex operation unavailable.

When a provider becomes unavailable:

- affected capabilities report unavailable/degraded;
- provider resources become unavailable/stale as appropriate;
- tasks referencing the provider reconcile according to `docs/TASKS.md` and the operation affordances;
- PortusOS does not silently substitute a materially different/security-weaker provider path;
- provider recovery may be attempted only within the defined lifecycle/health contract;
- `portus-os doctor` should identify the provider/integration failure when relevant;
- native recovery remains possible.

A protected provider must never degrade by exposing plaintext credentials.

---

## 26. Uninstall and historical tombstones

Removing a provider must not destroy the intelligibility of historical Portus records.

When a provider registration is uninstalled/removed:

- current capability discovery no longer reports it as available;
- live provider resources are removed/staled through normal reconciliation;
- historical task/artifact relationships retain the immutable provider registration ID;
- a bounded tombstone retains enough safe metadata to explain what provider registration existed;
- provider-owned private/domain content is not copied into the tombstone;
- a later reinstall does not silently rebind historical resource IDs to new resources.

A tombstone may retain safe fields such as:

- provider registration ID;
- provider type;
- last known software/interface version;
- removed timestamp;
- predecessor/successor link where explicitly established.

Retention details belong to state/health implementation.

---

## 27. PortusBrowser reference mapping

PortusBrowser is the first rich-resource provider reference case.

### Provider

```text
provider type: portus-browser
```

### Semantic capabilities

The first-ISO P15 integration exposes one coarse semantic capability:

```text
browser.control
```

Additional capabilities such as independent inspect/event classes may be added only when they represent a real stable contract rather than mirroring CLI subcommands. Capability IDs remain coarse and stable.

### Interface

Primary agent interface:

```text
portus-browser CLI
```

Master Portus and delegated Codex workers may invoke it directly.

PortusOS does not need a `portus-os` wrapper around every browser action.

### Provider-owned resources

PortusBrowser owns resources such as:

- browser sessions;
- tabs/pages/snapshots/elements where referenced inside browser workflows.

The Portus general index should normally retain only the provider resource references and system relationships that have cross-component value.

Detailed browser state remains in PortusBrowser.

P15 materializes `browser-session` as a real principal-filtered opaque provider-resource class. The System Index may retain the provider registration relationship and safe availability/generation metadata, but not tabs, URLs, DOM, snapshots, page contents or browser credential/session stores.

### Policy

```text
PortusOS outer provider/caller policy
  AND
PortusBrowser browser-domain policy
```

### Lifecycle

The first-ISO P15 contract records the Broker as **provider-owned**. The native-messaging host starts the Broker on demand when it is missing; PortusOS installs, observes, validates and repairs that path but does not create a competing OpenRC supervisor. Browser processes/sessions remain user/application/provider-owned resources.

### Health

Provider health can distinguish Broker/native-host/bridge integration from the existence of current browser sessions.

A healthy Broker with zero connected browser sessions is not equivalent to a crashed provider.

### Tasks

A browser session/tab may be associated with a Portus task without becoming Portus-owned.

If a future PortusBrowser operation exposes trackable status/cancel/reconcile semantics, those are described through the operation-affordance contract rather than inferred from browser complexity.

---

## 28. Protected API provider reference mapping

The protected API provider is the contrasting no-GUI/security-sensitive reference case.

### Provider

The first protected API provider implementation is locked by `docs/PROTECTED_API_PROVIDER.md`:

```text
provider type:  protected-api
service:        portus-apid
service user:   portus-api
client CLI:     portus-api
```

### Semantic capability

The initial common semantic capability is:

```text
protected-api.request
```

Provider-specific operation names remain inside the provider contract rather than becoming a generic credential-forwarding interface.

### Interfaces

The canonical interface is peer-credential-authenticated versioned JSONL over:

```text
/run/portus/protected-api/use.sock
```

`portus-api` is the direct Master/Codex client. An authenticated provider-compatible local proxy may be layered on the provider for SDK/application compatibility using the short-lived local capability-token architecture in `docs/PROTECTED_API_PROVIDER.md`.

It must not expose a generic reusable-secret retrieval interface.

### Provider-owned resources

A protected credential reference such as:

```text
openai/main
```

is a provider-owned safe reference, not the credential value.

The generic provider/resource record may know that the reference exists and is available/degraded under policy. It must not contain the reusable key.

### Policy

PortusOS outer authorization combines with the protected provider's credential/provider/host/operation restrictions.

### Tasks

A Portus task may reference an authenticated provider operation when durable tracking is needed. Most ordinary API calls do not automatically become Portus tasks.

### Security proof

For a credential classified protected:

```text
USE succeeds
EXPORT fails
```

The provider manifest, health output, task relationships, system index, diagnostics, and operation results must not become indirect credential-exfiltration paths.

---

## 29. Provider discovery and registration flow

The first system-provider flow should conceptually be:

```text
trusted package/install step
  -> installs provider implementation
  -> installs root/admin-controlled provider manifest
  -> installs provider skill/docs where applicable
  -> Portus provider reconciliation discovers manifest
  -> schema + trust validation
  -> creates/updates provider registration generation
  -> validates interface compatibility
  -> establishes lifecycle integration where applicable
  -> probes bounded health
  -> publishes capability/provider observations
  -> system index relates live resources as they appear
```

Discovery of an arbitrary executable does not automatically register a provider.

Registration failure should produce a clear provider/integration error without preventing ordinary use of unrelated parts of the computer.

---

## 30. Portus OS CLI obligations

`docs/CLI.md` now locks the first discovery/inspection surface as:

```text
portus-os capability list
portus-os capability show <capability-id>
portus-os capability provider list
portus-os capability provider show <provider-id>
```

These commands expose safe registered capability/provider metadata, availability, interfaces/versions, health/compatibility, resource/task relationships where appropriate, and tombstone/history information under this document's authority.

There is intentionally no universal `portus-os capability invoke ...` command. Provider operations remain directly usable through provider-owned interfaces/skills unless a separately defined Portus-managed workflow is required.

Provider reconciliation/repair/install/uninstall operations are not implied by the discovery surface; those commands require their own lifecycle/policy/recovery contract before being added.

Conceptual examples:

```text
portus-os capability list
portus-os provider list
portus-os provider show <provider-registration-id>
portus-os provider refresh <provider-registration-id>
```

are illustrative only.

The CLI must not become a universal provider command proxy.

The provider's own CLI/API remains the normal place for provider-domain operations.

---

## 31. First-ISO boundary

This document does not add a new first-ISO product feature beyond `docs/ACCEPTANCE.md`.

It specifies how the already required first-ISO provider proofs fit one common integration architecture.

The first ISO should prove the provider contract with two deliberately different cases:

1. **PortusBrowser** — rich live resources, provider CLI, provider-domain policy, process/window relationships, health/degradation;
2. **protected API provider** — protected credential reference, non-exporting authenticated operation, local protected interface/proxy, strict security boundary.

The first implementation may support only the provider/interface classes required by these proofs.

It does not need:

- arbitrary third-party provider installation;
- dynamic online provider marketplaces;
- generic remote providers;
- universal provider invocation;
- automatic semantic provider substitution;
- distributed provider discovery;
- broad user-authored provider manifests.

---

## 32. Validation requirements

Before capability-provider integration is considered implementation-ready, validation must cover at least:

### Non-duplication/direct use

- [ ] Master can use PortusBrowser directly through `portus-browser` without routing each command through `portus-os`.
- [ ] Provider registration does not create duplicate wrappers for Codex/Linux capabilities.
- [ ] Codex subagent/session/compaction/skill primitives are not represented as generic provider capabilities.
- [ ] Ordinary installed software can remain discoverable without a provider manifest.

### Identity/manifest

- [ ] Provider type is stable across an ordinary provider software upgrade.
- [ ] Provider registration IDs use `provider_<UUIDv7>` and are immutable/non-reused across unrelated registration generations.
- [ ] Invalid/untrusted manifests are rejected without executing arbitrary manifest content.
- [ ] Manifests cannot grant themselves authority.
- [ ] Raw reusable secrets are rejected from provider manifests/generic provider state.

### Interface/version compatibility

- [ ] Interface contract version is distinguished from provider software version.
- [ ] An incompatible provider is reported clearly without crashing `portusd`.
- [ ] Provider-specific detailed command knowledge remains in provider skills/docs rather than the manifest.

### Resources/index

- [ ] A PortusBrowser session can be represented as an opaque provider resource reference and related to live process/window/task state without copying browser-domain state.
- [ ] Stale/expired provider resource references fail or revalidate rather than silently bind to another resource.
- [ ] Provider resource lifetime is distinguishable from durable provider registration identity.
- [ ] Capability registry and system-index observations remain distinct.

### Policy/principal isolation

- [ ] Provider registration alone does not grant use authority.
- [ ] PortusOS outer policy and provider-domain policy both apply.
- [ ] User-private provider resources are filtered by principal.
- [ ] A second Master user cannot inspect another user's private provider resources without explicit authority.

### Health/degradation

- [ ] Provider health supports `healthy`, `degraded`, `unavailable`, and `unknown` semantics.
- [ ] Provider compatibility and health are separate observations.
- [ ] A healthy provider with zero live target resources is distinguishable from a crashed provider.
- [ ] Provider failure degrades only affected capabilities and does not make native Linux/Codex unusable.

### Tasks/operations

- [ ] A provider resource may be associated with a Portus task without Portus claiming lifecycle ownership.
- [ ] A trackable provider operation can declare/query its status/cancel/reconcile/retry/result affordances.
- [ ] Task cancellation does not report success if the provider cannot confirm cancellation.
- [ ] Provider operations that cannot reconcile can become `interrupted` after runtime loss rather than guessed/restarted.

### Protected provider

- [ ] A protected credential reference is visible as safe provider/resource metadata without exposing the reusable credential.
- [ ] The protected provider performs an authorized authenticated operation.
- [ ] Master Portus cannot export the protected credential through provider interface, manifest, Portus state, task state, index, logs, health, diagnostics, or error output.
- [ ] The provider cannot be coerced to attach protected credentials to arbitrary unapproved destinations.

### Uninstall/history

- [ ] Removing a provider makes current capabilities unavailable while preserving bounded historical provider-registration references.
- [ ] Reinstalling a provider does not silently rebind old opaque resource IDs to the new registration generation.

---

## 33. Decisions locked by this document

The following decisions are now locked:

1. A capability is a coarse semantic ability, not a command/tool list.
2. Provider, interface, resource, operation, and capability are separate concepts.
3. Registered providers are specialised integrations; Codex/Linux/ordinary commands are not forced into the provider model.
4. Codex remains the execution harness and is not represented as a normal generic provider.
5. Provider registration does not make `portusd`/`portus-os` the mandatory invocation path.
6. Direct provider CLI/API/IPC use remains normal when no Portus-specific coordination is required.
7. A universal `portus-os capability invoke` proxy is not required by the architecture.
8. Provider type and provider registration generation are separate identities.
9. Provider registration IDs use immutable `provider_<UUIDv7>` external identifiers.
10. Provider scope/principal ownership is explicit; system registration does not imply cross-user resource visibility.
11. Capability IDs are lowercase dot-separated semantic names and remain coarse relative to provider operations.
12. Manifest schema, provider software, interface contract, and capability contract versions are separate version dimensions.
13. First-class providers use declarative trusted manifests; manifests are not arbitrary execution scripts.
14. First-ISO system provider manifests live under the administrator-controlled `/etc/portus/capabilities/` architecture.
15. Provider skills/docs own detailed agent usage; manifests own machine-readable integration metadata.
16. The `portus-os-cli` skill does not absorb provider-specific command manuals.
17. Provider resource references use provider registration + resource type + opaque provider-owned resource ID.
18. Generic PortusOS code does not parse provider resource IDs for domain meaning.
19. Provider resource lifetime/freshness must be represented sufficiently to prevent stale-resource confusion.
20. Short synchronous provider operations do not require durable operation records.
21. Trackable provider operations expose task-relevant affordances such as status/cancel/reconcile/retry/progress/result/cleanup where supported.
22. Unsupported provider affordances are represented honestly; Portus tasks do not pretend to control them.
23. Managed-versus-associated semantics from `docs/TASKS.md` apply to provider relationships.
24. Provider lifecycle ownership is explicit and can differ between provider service components and provider resources.
25. Common provider health vocabulary is `healthy`, `degraded`, `unavailable`, `unknown`.
26. Compatibility, provider health, capability availability, and live resource availability remain distinct.
27. Health checks are bounded/typed and cannot be arbitrary manifest shell strings.
28. Provider registration never grants authority.
29. Effective provider use requires applicable PortusOS outer policy and provider-domain policy.
30. Secret-backed capability use remains separate from credential export.
31. Provider interfaces must preserve caller/principal isolation appropriate to the transport.
32. Provider outputs remain provider-owned unless explicitly related/registered as Portus artifacts.
33. Capability registry and system index are separate but linked layers.
34. Ordinary installed software is not automatically a provider.
35. Provider updates trigger explicit compatibility validation; incompatible integrations degrade cleanly.
36. Provider failures degrade affected capabilities rather than the whole OS.
37. Provider uninstall preserves bounded historical registration tombstones and does not silently rebind old resources after reinstall.
38. PortusBrowser and the protected API provider are the two first-ISO reference providers for validating that the common contract is not browser-specific.
39. PortusBrowser remains directly usable through `portus-browser` and keeps browser-domain state/policy.
40. Protected API providers expose safe credential references/operations without making raw reusable credentials available to generic Portus state or Master Portus when classified protected.
41. `docs/TASKS.md` remains authoritative for task promotion/lifecycle; this document supplies provider-side affordances only.
42. `docs/SYSTEM_INDEX.md` remains authoritative for live machine-map schemas/freshness; this document supplies provider/resource identity and ownership semantics.
43. `docs/CLI.md` locks the first capability/provider CLI surface as `capability list/show` plus `capability provider list/show`, with no universal provider invocation proxy; command output must preserve these architecture boundaries.

---

## 34. P5 implementation record and remaining dependent decisions

P5 now fixes the shared first-ISO registry implementation:

- strict TOML manifest schema version `1` under `/etc/portus/capabilities/`, with exact `<provider-type>.toml` filename mapping;
- bounded manifest/file/identifier/interface/capability/resource/skill validation and rejection of unknown authority/secret/arbitrary-command fields;
- Linux production trust validation requiring a real root-owned manifest directory/files that are not group/world writable; isolated tests use an explicit pretrusted-fixture mode;
- SQLite schema version `3` provider registration, interface, capability, capability-interface, resource-type/resource-reference, skill, health-contract, and tombstone fields/tables;
- ordinary updates to the same active provider type preserve the registration generation; removal creates a tombstone; later reinstallation creates a new `provider_<UUIDv7>` and cannot silently rebind old provider-resource references;
- a schema-valid manifest begins with compatibility `unknown` and health `unknown/not_probed`; manifest parsing alone is not treated as proof that the installed provider interface is actually compatible or healthy;
- removed provider IDs remain inspectable through `capability provider show` with bounded tombstone/successor metadata, while normal provider/capability lists include only active registrations;
- a missing/untrusted manifest directory fails reconciliation and degrades registry health without tombstoning previously known registrations; only an existing trusted manifest directory is authoritative for removal reconciliation;
- startup reconciliation inside `portusd`; invalid provider manifests degrade the provider-registry status without preventing unrelated runtime operation;
- principal-filtered, bounded runtime reads for capability/provider discovery;
- implemented CLI surfaces `capability list`, `capability show`, `capability provider list`, and `capability provider show`, using the common 50/default and 200/maximum pagination contract and opaque cursors;
- no universal provider invocation, install, removal, refresh, or reconciliation CLI was introduced.

Remaining provider-specific/later-phase work includes:

- exact OpenRC service integration for Portus-supervised providers;
- exact provider health probe adapters and reason-code registry;
- exact compatibility-range representation and hard-cutover/update behavior per first-party provider;
- exact operation-affordance wire representation;
- exact task/provider relationship schema under `docs/TASKS.md`;
- exact artifact registration relationship under `docs/ARTIFACTS.md`;
- exact provider-resource freshness/index schema under `docs/SYSTEM_INDEX.md`;
- exact protected API provider SQLite/protocol/provider-definition schemas and implementation dependencies consistent with `docs/PROTECTED_API_PROVIDER.md`;
- exact PortusBrowser package/manifest/interface versions selected for the ISO;
- exact uninstall/tombstone retention periods;
- future user-scoped/custom provider manifest policy;
- future remote provider model if ever justified;
- future semantic provider substitution/selection only if a real use case requires it.

Those implementation decisions must preserve the non-duplication, direct-provider-use, provider-domain-ownership, task-boundary, principal-isolation, and protected-credential rules locked here.
