# PortusOS System Index and Machine Map

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative detailed architecture and product decision
**Adopted:** 2026-07-27
**Detailed contract aligned:** 2026-08-21
**Scope:** How PortusOS builds, correlates, queries, refreshes, secures, and recovers a live operational map of the machine without duplicating Linux, Codex, provider, task, or durable-knowledge authority

## 1. Core decision

PortusOS provides a **freshness-aware operational graph of the current machine**.

The System Index correlates observations from Linux, Portus-owned registries, Codex integration, and registered capability providers so Master Portus can understand the computer without repeatedly reconstructing the same relationships from raw commands.

It exists to answer questions such as:

- what software and registered capabilities are available;
- what applications are currently running;
- how many application instances exist;
- which processes and windows belong to an instance;
- which workspaces and displays contain those windows;
- what is focused, visible, hidden, iconified, or in the i3 scratchpad;
- which OpenRC services and selected system resources are active;
- which project, task, Codex session, provider, or user is related to a resource;
- which control paths are available for a resource;
- which provider owns richer domain-specific state;
- how fresh and trustworthy each observation/relationship is;
- which source must be consulted before an important mutation.

The defining rule is:

> The System Index is a correlated operational map, not the universal source of truth for the machine.

The index helps the agent navigate and reason about the computer. It does not define what the agent may inspect, execute, control, install, or create.

---

## 2. Authority and related contracts

This document is authoritative for:

- System Index resource/relationship semantics;
- index handles and source-generation identity;
- observation provenance;
- correlation evidence;
- live/recent/stale/unavailable/historical freshness semantics;
- source generations and invalidation;
- targeted refresh, rescans, reconciliation, and rebuild;
- quick machine-map views;
- index annotations;
- principal filtering/redaction;
- index runtime/degradation behavior.

Related authorities remain separate:

- `docs/STATE.md` controls durable/derived/transient/provider-owned state classes and SQLite boundaries;
- `docs/TASKS.md` controls task identity, task state, execution relationships, and managed-versus-associated execution;
- `docs/CAPABILITIES.md` controls registered provider identity, semantic capabilities, provider interfaces, opaque provider-resource references, lifecycle, health, and compatibility;
- `docs/POLICY.md` controls caller principal, cross-user access, and administrator-enforced authority;
- `docs/SECRETS.md` controls protected credentials and prohibited secret material;
- `OKF.md` controls durable per-machine knowledge;
- `docs/CONTROL_PLANE.md` controls the non-duplication boundary;
- `docs/CLI.md` controls exact `portus-os` command names, output envelopes, pagination, exit-code families, and CLI compatibility behavior.

The System Index must reuse identities and authority from those documents rather than invent parallel truth.

---

## 3. The index is not prompt injection

PortusOS must not inject the complete machine graph into Master Portus context automatically.

The graph can be large and frequently changing. Wholesale injection would:

- waste context;
- distract Master with unrelated state;
- expose more operational metadata than required;
- become stale during long sessions;
- encourage cached observations to be mistaken for authoritative current state;
- work against the Master-context economy locked in `docs/TASKS.md`.

Master `AGENTS.md` should state concisely that:

- the System Index exists;
- it is useful for current machine topology and relationships;
- it can be queried/refreshed when useful;
- it is not authoritative over native/provider sources;
- the durable machine knowledge map is separate;
- detailed System Index command usage lives in the machine-wide `portus-os-cli` skill.

Normal use follows progressive disclosure:

```text
Master knows the index exists
  -> asks a small question / quick view
  -> receives bounded resources + handles
  -> inspects only relevant nodes/relationships
  -> refreshes/revalidates when current truth matters
  -> invokes provider/native interface directly where appropriate
```

---

## 4. Three different machine-awareness layers

PortusOS must keep three concepts separate.

```text
System Index
  = what the machine currently appears to contain/do

Master machine knowledge
  = stable facts learned about this particular installation

Configuration/policy
  = what the machine is deliberately configured/allowed to do
```

### 4.1 System Index

Examples:

- Chromium is running now;
- this X11 window is on workspace `WEB`;
- process generation X currently belongs to application instance Y;
- provider registration Z is healthy;
- browser session R currently exists;
- task T is associated with process P.

### 4.2 Durable per-machine knowledge

Lives under the Master knowledge bundle, recommended at:

```text
/workspace/<user>/master/knowledge/machine/
```

Examples:

- `/mnt/archive` is the owner's historical-data volume;
- the external office monitor is normally used for development;
- a locally installed application has a machine-specific integration quirk;
- a project normally uses a particular local service/provider;
- a stable recovery note is worth reusing.

This is governed by `OKF.md`.

### 4.3 Configuration/policy

Examples:

- workspace `DEV` should be placed on a configured display role;
- a service should start in a selected OpenRC runlevel;
- a capability is allowed/prompted/rejected for a principal.

The System Index must not silently convert learned knowledge into configuration, nor convert current observations into durable knowledge simply because they were seen once.

---

## 5. The index is not a capability or policy boundary

The System Index must not become:

- a general filesystem-content index;
- a source-code/project search engine;
- a replacement for Codex file/search/shell/Git tools;
- an allowlist of actions;
- the sole launch/control path for software;
- a replacement for provider CLIs/APIs/IPC;
- a policy engine;
- a secrets store;
- a transcript store;
- a screenshot/page/terminal-memory archive;
- proof that something does not exist merely because it is absent from cached state.

An authorised agent remains free to:

- use the index;
- query `/proc`, i3, X11, OpenRC, pacman, filesystem or device interfaces directly;
- use a registered provider directly;
- use an ordinary application CLI/API;
- run a script or one-off native command;
- force a targeted refresh/reconciliation;
- ignore the index when direct inspection is faster or more reliable.

Index failure degrades convenience and correlation. It must not disable ordinary Linux/Codex operation.

---

## 6. Operational graph architecture

The index is conceptually built in four stages/layers:

```text
AUTHORITATIVE / DOMAIN SOURCES
/proc  i3  X11/EWMH  OpenRC  pacman
Portus tasks  Portus capability registry  Codex integration
registered providers
        |
        v
SOURCE OBSERVATIONS
facts exactly attributed to those sources
        |
        v
CORRELATION
resource-generation matching + relationship evidence
        |
        v
MATERIALIZED OPERATIONAL GRAPH
bounded query/inspect/quick views
        +
DURABLE PORTUS OVERLAYS
explicit Portus relationships + annotations
```

The graph is an efficient, correlated view. It does not erase source authority.

An application instance may therefore combine facts from several sources, for example:

```text
VSCodium instance
  application definition <- package/.desktop discovery
  process generation      <- /proc
  window properties       <- X11/EWMH
  workspace/focus         <- i3 IPC
  project relationship    <- Portus project relationship
  task relationship       <- TASKS registry
  Codex relationship      <- Codex/session integration
```

Each fact/relationship must retain enough provenance for the index to explain what is authoritative versus inferred.

---

## 7. Index domains

The System Index exposes related but distinct domains.

### 7.1 Registered Portus capability catalogue

Answers what intentionally registered providers say the machine can do.

First-ISO examples:

```text
browser.control
protected-api.request
```

`docs/CAPABILITIES.md` owns these semantic capabilities and provider registrations.

The System Index makes them searchable and relates them to health, applications, processes, tasks and provider resources.

### 7.2 Observed native abilities

The machine map may also expose useful native/ordinary abilities, such as:

- Git is installed;
- `ffmpeg` is installed;
- i3 IPC is available;
- OpenRC service control exists;
- a screenshot/input fallback is present;
- an application exposes an ordinary structured CLI.

These observations do **not** become registered Portus providers merely because Master can use them.

The index should distinguish registered Portus capabilities from observed native/ordinary abilities.

### 7.3 Installed-software catalogue

Answers what software is installed or launchable.

Potential sources include:

- pacman package records;
- `.desktop` entries;
- approved executable/PATH discovery;
- MIME handlers;
- OpenRC service definitions;
- Portus capability manifests;
- approved records for externally installed software.

This changes slowly and may be persisted as a rebuildable materialized cache.

### 7.4 Live runtime topology

Answers what exists now.

Includes as needed:

- processes/process trees;
- application instances;
- graphical sessions;
- displays;
- i3 workspaces/containers;
- X11 windows;
- focus/visibility/geometry/scratchpad state;
- headless application processes;
- OpenRC service observations;
- selected mounts/devices/endpoints;
- registered provider resources.

### 7.5 Portus relationship overlay

Relates live/installed resources to Portus-owned durable identity such as:

- `task_<UUIDv7>`;
- project references;
- provider registrations `provider_<UUIDv7>`;
- artifact references;
- requester/principal relationships;
- explicitly recorded ownership/cleanup relationships.

The index does not become the task, provider, project or artifact database merely because it exposes these edges.

### 7.6 Agent-managed annotations

Stores small structured labels/relationships that native sources do not provide, subject to policy.

Examples:

- friendly label;
- role such as `research-browser`;
- display/workspace alias;
- project association;
- expected cleanup relationship;
- short operator note/reference.

Long explanatory machine knowledge belongs in OKF, not an index annotation blob.

---

## 8. Core resource classes

The initial architecture recognizes a bounded core set.

### Software/runtime

- application definition;
- application instance;
- process generation.

### Graphical

- graphical session;
- display;
- workspace;
- window generation.

### System

- OpenRC service;
- selected mount;
- selected device;
- selected network/listening endpoint where useful.

### Agent/Portus

- Codex root-session/thread reference where correlation is useful;
- project reference;
- task reference;
- artifact reference.

### Capability/provider

- provider registration;
- registered semantic capability;
- opaque provider-owned resource reference.

PortusOS must not index every Linux object merely because it can.

Threads, every socket, every inode, complete environments, arbitrary filesystem contents and high-frequency telemetry remain outside the normal machine map unless a later demonstrated use justifies a bounded domain.

---

## 9. Index handles versus authoritative identities

Every resource returned through the operational graph may have a convenient **index handle** for bounded follow-up operations.

The external conceptual form is:

```text
idx_<UUIDv7>
```

An index handle means:

> this observed/correlated resource generation in the current System Index.

It does **not** mean that the resource has permanent identity across reboot, provider reinstall, session restart, or index rebuild.

### 9.1 Authoritative/durable identities are reused

Where another subsystem already owns identity, the index must reuse/reference it rather than invent a competing ID.

Examples:

```text
task_<UUIDv7>
provider_<UUIDv7>
project reference
artifact reference
Codex thread/session ID
provider resource reference:
  provider_registration_id
  resource_type
  resource_id
```

### 9.2 Native source identity

Transient native resources retain source-generation identity.

A process identity must include enough information to avoid PID reuse, conceptually including:

```text
boot generation
PID
process start generation/time
```

with additional executable/launcher/process-group evidence where useful.

A window identity must be scoped to the graphical/source generation rather than treating a raw X11 window ID as globally durable.

### 9.3 Index handles are not workflow persistence keys

A saved workflow/scene should not normally persist an old `idx_...` handle as the enduring target.

It should prefer:

- a durable authoritative reference when one exists; or
- a semantic selector/role resolved against current index state.

Example:

```text
application = VSCodium
project = portus-os
workspace role = development
```

rather than:

```text
window = idx_old_window
```

---

## 10. Source generations and invalidation

Timestamp freshness alone is insufficient. The index must understand source/resource generations where relevant.

Important generation concepts include:

- machine boot generation;
- graphical/X11 session generation;
- i3 IPC/session generation where necessary;
- process generation;
- provider registration generation;
- provider session/resource generation where exposed.

Examples:

### Machine reboot

All boot-scoped process identities become invalid and live topology must be reconstructed.

### X11 session restart

Prior X11 window-generation observations become stale/invalid even if raw numeric IDs later repeat.

### Provider reinstall

A new `provider_<UUIDv7>` registration generation must not silently adopt old opaque resource IDs.

### Provider/session reconnect

Session/ephemeral resources are revalidated according to `docs/CAPABILITIES.md` rather than assumed continuous.

Index handles may be preserved across reconciliation only when continuity of the underlying resource generation is established safely.

---

## 11. Source authority and observation provenance

The System Index is a materialized view over authoritative/domain sources.

Typical source ownership is:

| Fact/domain | Authoritative/domain source |
| --- | --- |
| installed package state | pacman |
| registered provider definition/registration | CAPABILITIES registry + trusted manifest |
| provider resource state | provider |
| task state | TASKS registry |
| process PID/start/executable observations | `/proc`/Linux |
| OpenRC service definition/state | OpenRC |
| workspace/container/focus placement | i3 IPC |
| X11 window identity/properties | X11/EWMH |
| provider health/domain resource detail | provider/provider adapter |
| project contents/Git | filesystem/Git, not the index |

An observation should retain conceptually:

```text
source_id/source_kind
source_generation where applicable
observed_at
freshness
safe source identity/reference
```

Fields that originate from different sources may coexist in one materialized resource, but their provenance must remain recoverable rather than being flattened into unexplained truth.

---

## 12. Correlation and evidence strength

Relationships are a major value of the index, but not every relationship has equal certainty.

The common evidence-strength vocabulary is:

```text
authoritative
strong
heuristic
```

### `authoritative`

The owning/authoritative subsystem explicitly establishes the relationship.

Examples:

- Portus task launch record explicitly identifies a managed process generation;
- provider explicitly identifies its resource;
- i3 explicitly reports a window/container on a workspace.

### `strong`

Several stable source facts strongly support the relationship.

Example:

- X11 `_NET_WM_PID`, process generation, executable and application metadata agree on window ownership.

### `heuristic`

Useful but uncertain inference.

Examples:

- window class/title pattern suggests an application instance;
- process ancestry and naming suggest a relationship without a stronger owner signal.

The index should preserve concise evidence/reason codes where useful.

It must not fabricate fake numerical precision such as an arbitrary `83.7%` confidence when no calibrated probability exists.

Unknown relationships are valid and must be represented honestly.

---

## 13. Application instances

Processes, application instances and windows are distinct.

One application instance may:

- own multiple processes;
- own multiple windows;
- have no visible window;
- share helper/background processes;
- open dialogs/helpers;
- move between workspaces;
- remain alive after its visible windows disappear.

Correlation priority should generally be:

1. provider/application-owned stable identity;
2. application-specific IPC or explicit launch correlation;
3. process generation + window ownership/source metadata;
4. bounded heuristics.

The index should not collapse all processes with the same executable name into one application instance automatically.

For each instance, the index should be able to expose when known:

- application definition;
- leader/related process generations;
- windows;
- graphical/workspace placement;
- project/task relationships;
- provider/resource relationships;
- principal scope;
- available control paths;
- correlation evidence/freshness.

---

## 14. Available control paths, not one control level

The previous idea of assigning every resource one "best control level" is replaced.

A resource may have several useful control paths simultaneously.

Conceptual control-path kinds include:

```text
registered-provider
structured-api
structured-cli
application-adapter
native-system
accessibility
process-window
visual-fallback
```

Example: PortusBrowser-controlled Chromium instance

```text
control_paths:
  - registered-provider
      provider = provider_...
      capability = browser.control
      interface = cli
  - process-window
  - visual-fallback
```

Example: VSCodium

```text
control_paths:
  - structured-cli (when installed/usable)
  - process-window
  - visual-fallback
```

The index helps Master discover available paths. It does not force a universal priority order or prohibit another authorized native method.

Provider-specific command/API detail remains in provider skills/contracts, not the System Index.

---

## 15. Freshness model

Every live/derived observation must expose freshness honestly.

The common freshness vocabulary is:

```text
live
recent
stale
unavailable
historical
```

### `live`

The observation is backed by a currently healthy event/query relationship and the relevant source generation remains valid. `live` must not mean merely "observed some arbitrary number of seconds ago."

### `recent`

Observed recently enough for ordinary navigation, but not continuously event-confirmed/current enough to guarantee mutation safety.

### `stale`

May still be correct, but should be refreshed/revalidated before an important mutation where incorrect targeting matters.

### `unavailable`

The source/provider cannot currently be queried or the observation cannot currently be validated.

### `historical`

Retained only because bounded diagnostic/history or a durable relationship needs to explain a past resource.

Freshness policy is source/domain-specific. Package discovery and process/window state must not share one magic TTL.

Exact intervals and reconciliation cadences are implementation tuning and must be measured rather than hardcoded into this architecture document.

---

## 16. Mutation revalidation

An index result is not automatically a safe mutation target indefinitely.

Before important/destructive mutations, the caller/control surface must revalidate when:

- the record is `stale` or `unavailable`;
- the resource is ephemeral/session scoped;
- the source generation may have changed;
- a PID/XID/provider resource could have been reused/replaced;
- selecting the wrong resource could produce material harm.

A stale/invalid generation must fail clearly rather than silently targeting a different resource.

Read-only navigation may tolerate `recent` observations where exact current state is not critical.

---

## 17. Keeping the graph current

The index uses a hybrid update model.

### 17.1 Initial bounded discovery

During/well after `portusd` startup, populate required domains from their sources:

- provider registrations/capabilities;
- installed applications;
- OpenRC observations;
- processes;
- i3/X11 topology when a graphical session exists;
- provider resources where appropriate;
- durable Portus relationships.

### 17.2 Event-driven updates

Subscribe where reliable and useful, including:

- i3 workspace/window events;
- X11 client/property events where appropriate;
- Portus task lifecycle events;
- provider events such as PortusBrowser session changes;
- service/device/mount events where a reliable interface justifies it.

### 17.3 Targeted on-demand refresh

A query should refresh only the domains/resources needed for the user's question when practical.

For example:

- "what apps are open?" should not rescan every package/device/provider;
- "is PortusBrowser healthy?" should query provider/health state rather than rebuilding the entire graph.

### 17.4 Periodic reconciliation

Events can be lost. Bounded periodic reconciliation compares cached/materialized state to source truth.

### 17.5 Full reconciliation

A deliberate full reconciliation walks all enabled index sources/domains and repairs the derived graph without deleting durable Portus truth.

### 17.6 Rebuild

A rebuild discards/recreates rebuildable index observations/materialization while preserving:

- task truth;
- provider registrations/tombstones;
- project/artifact truth;
- explicit durable Portus relationships;
- authorized annotations.

---

## 18. `portusd` startup and index readiness

`portusd` must not wait for a perfect machine graph before becoming usable.

The startup relationship is conceptually:

```text
portusd
  -> config/database/socket/core runtime ready
  -> report runtime ready/degraded as appropriate
  -> index sources warm/reconcile asynchronously within the daemon lifecycle
```

Index subsystem health may conceptually be:

```text
initializing
healthy
degraded
rebuilding
unavailable
```

A degraded or warming index does not make ordinary runtime-backed task/policy/provider operations unavailable unless those operations specifically depend on the missing index source.

System-index coordination remains inside `portusd` for the initial architecture; there is no separate `portus-indexd` requirement.

---

## 19. Persistence model

The state classes in `docs/STATE.md` remain authoritative.

### 19.1 Rebuildable persisted cache

Useful candidates include:

- installed application catalogue;
- latest provider/capability health observations;
- bounded latest service observations;
- derived correlation/materialization useful for faster restart;
- source/freshness/reconciliation metadata.

These records remain marked as derived/rebuildable.

### 19.2 Primarily live/in-memory topology

Examples:

- current processes;
- windows;
- focus;
- workspace placements;
- live ephemeral/session provider resources.

A bounded SQLite snapshot may be retained for reconciliation/diagnostics where useful, but it remains derived observation rather than durable source of truth.

### 19.3 Durable Portus overlays

Persisted independently of derived observations:

- explicit Portus task/resource relationships;
- provider registrations;
- project/artifact relationships where Portus owns them;
- approved annotations.

### 19.4 Do not retain by default

The index must not become a historical surveillance store for:

- every process over time;
- every window/title change;
- browser contents/history;
- screenshots;
- terminal contents;
- arbitrary shell commands;
- full URLs where not required;
- high-frequency topology events with no defined product purpose.

---

## 20. Restart and reboot reconciliation

### 20.1 `portusd` restart within the same boot

On restart:

1. load durable Portus relationships/registrations;
2. load useful derived cache as non-authoritative prior observation;
3. mark uncertain live observations for reconciliation;
4. query authoritative/domain sources;
5. preserve an index handle only when resource-generation continuity is established safely;
6. drop/replace stale derived observations without deleting durable Portus truth.

### 20.2 Machine reboot

On reboot:

- durable task/provider/project/artifact/annotation truth survives according to its own subsystem policy;
- installed-software cache may be reused then verified;
- boot-scoped process identities expire;
- graphical session/window topology is rebuilt;
- provider session/ephemeral resources revalidate;
- the index does not silently resurrect prior live resources.

This aligns with `docs/TASKS.md`: arbitrary prior processes are not silently relaunched because the index once observed them.

---

## 21. Provider integration

`docs/CAPABILITIES.md` owns provider registration/resource semantics.

The System Index may expose:

- provider registration identity/type;
- compatibility/health observations;
- registered semantic capabilities;
- safe live provider-resource references;
- relationships from provider resources to tasks/processes/windows/applications/users;
- available provider control paths.

It must not copy the provider's complete domain state.

### PortusBrowser example

The index may know:

```text
provider_... = PortusBrowser
browser-session resource ref = ...
related Chromium process generation = ...
related window/workspace = ...
related task = ...
```

Detailed tabs, snapshots, elements, browser-domain policy, retained steps and page content remain PortusBrowser-owned and are retrieved through `portus-browser`.

A healthy PortusBrowser Broker with zero connected browser sessions must be distinguishable from a crashed/unavailable provider.

### Protected API example

The index may expose safe information such as:

```text
provider registration = protected-api
capability = protected-api.request
safe credential/provider ref = openai/main
availability = ...
```

It must never expose the reusable key or create a generic credential metadata surface that violates `docs/SECRETS.md`.

---

## 22. Task and Codex integration

`docs/TASKS.md` owns task lifecycle. `CODEX_UPDATES.md` owns Codex session/subagent behavior.

The index may expose relationships such as:

```text
task_... -> manages -> process generation

task_... -> associated-with -> provider resource

task_... -> uses -> Codex root session

application instance -> related-to -> project
```

The index must not mirror full task state history or Codex transcripts.

A Codex subagent does not automatically become a durable Portus/index resource merely because it exists. Store/correlate a Codex/subagent reference only when it provides useful machine/task relationship value.

Managed-versus-associated semantics come from `docs/TASKS.md` and must not be inferred merely because the index sees a relationship.

---

## 23. Quick views and progressive query model

The System Index should support three conceptual levels:

```text
quick view / summary
  -> query/filter
      -> inspect resource + relationships
```

`docs/CLI.md` locks the first bounded quick views as:

```text
portus-os index apps
portus-os index windows
portus-os index workspaces
portus-os index displays
portus-os index providers
portus-os index stale
```

and the first general operations as:

```text
portus-os index query
portus-os index show <resource-ref>
portus-os index topology <resource-ref>
portus-os index refresh <resource-ref>
portus-os index rescan <domain>
portus-os index reconcile
portus-os index rebuild
portus-os index status
```

These are bounded/progressive views over the operational graph. Installed application definitions, task/project relationships, changed-since/source/evidence/control-path filters and other architecture-level queries remain available through the typed `index query` contract rather than requiring a new top-level command for every question.

Example topology view:

```text
Task: task_...
└── Codex session
    └── Alacritty instance
        ├── process generation
        └── window
            └── workspace DEV
```

Example workspace view:

```text
Workspace: WEB
├── Chromium instance
│   ├── window
│   └── PortusBrowser browser-session ref
└── Alacritty instance
```

A topology tree is a view of **indexed/Portus-owned relationships**, not a recursive arbitrary-filesystem tree.

For arbitrary directory/file trees, Codex/native tools such as `find`, `fd`, `tree`, and `ls` remain the correct path.

---

## 24. Query behavior

Queries must be bounded, composable, principal-filtered, and suitable for progressive disclosure.

Useful conceptual filters include:

- resource type;
- application/provider/capability;
- task/project relationship;
- workspace/display;
- principal scope;
- freshness;
- changed-since observation;
- evidence strength;
- source/domain;
- control-path availability.

A query should return concise records/handles/references. Detailed source/provider content should require explicit inspection or provider-native follow-up.

The index must not automatically return:

- screenshots;
- full terminal contents;
- full browser pages;
- complete process environments;
- unbounded command lines;
- arbitrary source files;
- unrelated resources.

---

## 25. `portus-os-cli` skill integration

Detailed System Index usage belongs in the machine-wide `portus-os-cli` skill because the index is a `portus-os` domain.

The skill should teach Master/agents:

- when a quick view is useful;
- how to query/inspect resources;
- what an `idx_...` handle means;
- how authoritative references differ from index handles;
- freshness/revalidation rules;
- targeted refresh versus reconciliation/rebuild;
- how to read evidence strength/source provenance;
- how to follow provider references into provider-owned interfaces;
- when direct native inspection is preferable.

Do not create a separate System Index skill merely to duplicate the `portus-os-cli` skill unless later complexity demonstrates a real need.

---

## 26. Annotation boundary

Annotations are explicit Portus-owned overlays and must be distinguishable from observed facts.

An annotation may add:

- short friendly label;
- role;
- project/task relationship where authorized;
- display/workspace alias;
- cleanup expectation;
- short safe note/reference.

An annotation must not overwrite source-derived values to make the graph claim that the underlying system said something it did not.

Long-form learned knowledge belongs in OKF.

Configuration belongs in its configuration/policy owner.

---

## 27. Principal scope and privacy

Every resource/relationship exposed through the index must have enough scope information for correct filtering.

Conceptual scopes include:

```text
machine/system
principal/user
explicitly shared
```

Because one `portusd` may serve multiple Master Portus users:

- caller identity comes from Unix peer credentials;
- user-private task/provider/project/application relationships are filtered by effective principal;
- another user's private window titles, URLs, provider resources, project relationships and task details are not exposed by default merely because a machine-wide source can technically observe them;
- cross-user inspection requires explicit administrator authority;
- system-wide health/installed-software facts may be visible without exposing private resource details.

The implementation should use field-aware redaction rather than assuming the whole record is either public or secret.

---

## 28. Sensitive-data exclusions

The index must not retain or expose as ordinary graph data:

- raw reusable credentials;
- authorization headers/tokens;
- password fields;
- complete browser credential/session stores;
- complete browser pages/DOM merely for indexing;
- clipboard history;
- raw terminal output;
- complete process environments;
- screenshots by default;
- secret-bearing command lines/URLs;
- arbitrary source/project file contents.

Window titles, command lines, paths, URLs and application labels may themselves be sensitive and therefore require principal filtering/redaction/bounded retention.

The protected-secret rules in `docs/SECRETS.md` remain absolute.

---

## 29. Index health and degradation

The index must expose source/subsystem health sufficiently to explain incomplete results.

A healthy index does not mean every source is healthy.

Conceptually the agent should be able to determine:

- overall index subsystem state;
- which source/domain is unavailable/degraded;
- last successful observation/reconciliation;
- whether results are partial;
- whether rebuild is recommended;
- whether durable overlays remain healthy even if derived observations are unavailable.

When one provider/source fails, unrelated index domains remain useful where possible.

`portus-os doctor` must be able to diagnose index/database/source failures even when normal `portusd` operation is degraded, consistent with `docs/ACCEPTANCE.md`, `docs/RUNTIME.md`, and `docs/STATE.md`.

---

## 30. Failure and recovery

Index failure must degrade convenience rather than disable the computer.

When the index is unavailable:

- native Linux commands remain usable;
- Codex remains usable within its authority;
- i3/X11/OpenRC remain directly inspectable/manageable;
- provider CLIs remain directly usable when providers are healthy;
- SSH/TTY/recovery remain independent;
- durable Portus truth is preserved;
- rebuildable observations may be reconstructed.

Derived-index corruption must never trigger silent deletion of unrelated durable task/provider/artifact/annotation truth.

If the underlying SQLite database itself is corrupted, the evidence-preservation/recovery rules in `docs/STATE.md` apply; the index must not silently recreate the whole database as though only derived state were affected.

---

## 31. First-ISO boundary

This detailed architecture does **not** expand the blocking first-ISO acceptance contract in `docs/ACCEPTANCE.md`.

The first-ISO required index classes remain:

- installed/available applications;
- running processes;
- OpenRC services;
- graphical windows;
- i3 workspaces;
- registered Portus capabilities/providers.

The first implementation should prove the architecture by providing:

- real source-backed observations;
- resource-generation-safe identity for the required live classes;
- application/process/window/workspace correlation using available evidence;
- provider/capability discovery using `docs/CAPABILITIES.md`;
- explicit freshness;
- targeted refresh/rescan;
- full reconciliation/rebuild behavior;
- bounded structured query/quick views;
- source/index health;
- no dependency on a visible terminal;
- safe behavior when index/`portusd` is degraded.

Task/project/provider-resource/artifact/device/mount/Codex-session correlations may be implemented when another first-ISO subsystem needs them; their existence in this architecture does not independently add a new first-ISO gate.

PortusWindow remains deferred and therefore is not a first-ISO index requirement.

---

## 32. Validation requirements

Before the detailed System Index architecture is considered implementation-ready, validation must cover at least:

### Non-duplication/context

- [ ] The complete graph is not injected into Master context automatically.
- [ ] `AGENTS.md` routes detailed index usage to `portus-os-cli`.
- [ ] An arbitrary filesystem tree remains a Codex/native operation, not an index command.
- [ ] Registered provider capabilities are distinguishable from observed native abilities.

### Identity/generation

- [ ] Query results can return `idx_<UUIDv7>` generation-scoped handles.
- [ ] Existing authoritative IDs such as `task_<UUIDv7>` and `provider_<UUIDv7>` are reused rather than replaced.
- [ ] Provider resources preserve the opaque reference contract from `docs/CAPABILITIES.md`.
- [ ] PID reuse cannot cause an old process handle to target a different process.
- [ ] X11/session-generation changes invalidate prior window observations safely.
- [ ] Reboot expires boot-scoped live resource identities.

### Correlation/provenance

- [ ] Application/process/window/workspace relationships preserve source/evidence.
- [ ] `authoritative`, `strong`, and `heuristic` relationships are distinguishable.
- [ ] Unknown relationships are represented honestly rather than guessed.
- [ ] A resource can expose multiple available control paths.

### Freshness/update

- [ ] `live`, `recent`, `stale`, `unavailable`, and `historical` semantics are represented.
- [ ] Targeted refresh updates a selected domain/resource without rebuilding everything.
- [ ] Missed events are repaired by reconciliation.
- [ ] Rebuild discards derived observations while preserving durable Portus truth/annotations.
- [ ] Important mutation using stale/generation-unsafe state is revalidated or rejected.

### Restart/degradation

- [ ] `portusd` can become ready while index warming/reconciliation continues.
- [ ] `portusd` restart reconciles live state instead of assuming cached observations remain current.
- [ ] Provider/source failure produces partial/degraded index state rather than disabling unrelated domains.
- [ ] Native Linux/provider operation remains available when the index is unavailable.

### Privacy/security

- [ ] Cross-user private graph fields/relationships are filtered by authenticated principal.
- [ ] Raw reusable credentials never enter index records.
- [ ] Full browser pages, terminal contents, screenshots and complete process environments are excluded by default.
- [ ] Sensitive titles/paths/URLs/command lines follow redaction/scope rules.

### First-ISO live-state proof

- [ ] Required first-ISO resource classes are observed from real machine sources.
- [ ] Launch/create/change/close of an observable resource is reflected after targeted refresh/reconciliation.
- [ ] Provider/capability discovery uses the real provider registry rather than a static demo list.

---

## 33. Decisions locked by this document

1. The System Index is a freshness-aware correlated operational graph, not a universal source of truth.
2. It is progressively disclosed and is not injected wholesale into Master context.
3. System Index, durable machine knowledge, and configuration/policy are separate layers.
4. The index does not duplicate Codex/Linux/provider capabilities or become a policy boundary.
5. Graph construction separates source observations, correlation, materialization, and durable Portus overlays.
6. Registered Portus capabilities are distinguished from observed native/ordinary abilities.
7. The core resource set remains bounded; PortusOS does not index every Linux object by default.
8. Generation-scoped index handles use conceptual external form `idx_<UUIDv7>`.
9. Index handles are not permanent workflow identities.
10. Existing task/provider/project/artifact/Codex/provider-resource identities remain authoritative in their domains.
11. Native transient resources retain source-generation identity; PID/XID alone are insufficient durable identity.
12. Boot, graphical, provider and other relevant source generations invalidate stale resource observations safely.
13. Source authority/provenance remains recoverable per observation/fact.
14. Relationship evidence uses `authoritative`, `strong`, or `heuristic`; fake numerical confidence is rejected.
15. Application instances, processes, and windows are separate resources and are correlated using strongest available evidence.
16. A resource exposes multiple available control paths instead of one global "best control level."
17. Freshness vocabulary is `live`, `recent`, `stale`, `unavailable`, `historical`.
18. Freshness policy is source/domain-specific; no one global TTL is architecturally required.
19. Important mutations revalidate stale/generation-sensitive resources before action.
20. Update strategy combines initial discovery, events, targeted refresh, periodic reconciliation, and explicit full reconciliation/rebuild.
21. `portusd` core readiness does not require a perfectly warmed index.
22. System-index coordination remains inside `portusd` for the initial architecture.
23. Derived observations may be cached but remain rebuildable and separate from durable Portus truth.
24. Reboot expires boot/session-scoped resources and reconstructs live topology rather than resurrecting it.
25. Provider resources remain provider-owned opaque references and provider domain state is not duplicated.
26. Task/Codex relationships are references/correlations; task state and Codex transcripts remain owned elsewhere.
27. Quick views/topology trees operate on indexed/Portus relationships and are not generic filesystem listing commands.
28. Detailed index operation belongs in the `portus-os-cli` skill.
29. Annotations are small structured Portus overlays; long-form learned machine knowledge belongs in OKF.
30. Index queries are principal-filtered and field-aware for sensitive data.
31. The index must not become a passive historical surveillance archive.
32. Index/source failure degrades only affected awareness/correlation and preserves native recovery paths.
33. This detailed contract does not expand the first-ISO blocking resource classes in `docs/ACCEPTANCE.md`.
34. Exact CLI syntax, SQLite tables/columns, TTL values, reconciliation intervals, and low-level source adapters remain implementation contracts, not product architecture.

---

## 34. P6 implementation contract and remaining dependent work

P6 has now concretized the first System Index implementation contract without changing the product architecture above:

- P6 migration v4 stores source identity/generation/health, generation-safe observations, bounded safe metadata, control paths, relation provenance/reason codes, and index runtime generation/state; later state migrations do not change those System Index ownership semantics;
- an `idx_<UUIDv7>` handle is preserved only when the same source generation + native identity is re-observed; PID/XID or other native numeric identifiers alone never establish continuity;
- process identity is boot ID + PID + `/proc` start ticks;
- graphical resource identity is scoped to the caller-owned i3/X11 session generation;
- the first Linux adapters are `/proc`, system `.desktop` definitions with pacman ownership linkage, OpenRC `rc-status`, i3 IPC, X11/EWMH via bounded `xprop`, and the real Portus provider registry;
- normal application catalogue changes do not rotate the source generation; duplicate desktop IDs resolve with the local `/usr/local/share/applications` definition overriding the system `/usr/share/applications` definition;
- external read-only probes run with cleared inherited environment, fixed PATH/locale, null stdin, bounded output, and bounded timeout;
- P6 application-instance grouping uses exact/strong evidence where available and refuses ambiguous executable matches rather than guessing;
- the P6 CLI query/filter enums, pagination bounds, topology depth/resource limits, quick views, refresh/rescan/reconcile/rebuild/status schemas, and daemon-independent index metadata diagnostic are implemented in `portus-os`/`portusd`;
- healthy complete, degraded/partial, unavailable, restart-stale, and historical-generation reconciliation semantics are implemented and host-tested;
- the recorded disposable Linux VM now proves the real `/proc` plus pacman/application observation path through authenticated `portusd` transport, including rescan/query and native-bypass behavior.

Remaining dependent work is now limited to behavior that requires the installed Linux/Artix environment or later owning subsystems:

- repeat `/proc` and pacman/desktop observation on the exact Artix package set and execute the still-unproven OpenRC, i3 and X11 adapters there, including XID/session-generation behavior and real source transitions;
- add event subscriptions/loss detection and choose periodic reconciliation cadence only after live source behavior is measured;
- tune source-specific `live` versus `recent` aging thresholds rather than inventing one global TTL;
- define bounded cache retention/pruning values from real workload evidence;
- refine field redaction/audit behavior where live application/window metadata demonstrates a need;
- P15 now supplies principal-filtered opaque provider-resource observations plus authoritative provider-registration relationships; add only the remaining task/project/Codex/artifact and provider-resource-to-process/window/task correlations when their owning phases or installed providers expose authoritative/strong evidence;
- replace P6's restrictive root-only rebuild check with the final P9 typed policy/elevation path;
- measure admission/performance limits on the minimum 4 GiB and reference 8 GiB VMware profiles;
- complete ISO-20/21/22 live-state, refresh and native-bypass acceptance on the installed system.

Those remaining items must preserve the graph/provenance/generation/non-duplication boundaries above.

---

## 35. Defining statement

> PortusOS maintains a freshness-aware operational graph so Master Portus can quickly understand the current machine and its relationships without repeatedly rediscovering them. The graph correlates authoritative Linux, Portus, Codex, and provider observations while preserving their ownership, generation, provenance, privacy, and freshness. It is queryable and replaceable, not a policy boundary or universal truth store.