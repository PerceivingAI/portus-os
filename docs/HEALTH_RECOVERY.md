# PortusOS Health, Diagnostics, Degradation, and Recovery

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative health/recovery architecture and first-ISO contract; P11 host-safe implementation and distribution-neutral native Linux probe path complete, installed Artix/OpenRC recovery acceptance pending

**Scope:** Common health observations, source authority, degradation semantics, recovery dispositions, bounded automatic recovery, service restart limits, index/task/provider health relationships, resource warnings, diagnostic evidence/bundles, `portus-os health`/`doctor` boundaries, and recovery when Master Portus or the Portus runtime is unavailable

## 1. Purpose and authority

This document defines how PortusOS answers four distinct questions:

1. **What is the current health of a component?**
2. **Why is it unhealthy or uncertain?**
3. **What effect does that have on the rest of the machine?**
4. **What recovery action, if any, is safe?**

It is authoritative for common PortusOS health/degradation/recovery semantics.

Related authorities remain separate:

- `docs/RUNTIME.md` owns the `portusd` process/IPC boundary;
- `docs/CLI.md` owns the `portus-os` command/output/error contract;
- `docs/STATE.md` owns durable versus derived Portus state and corruption rules;
- `docs/TASKS.md` owns task lifecycle/reconciliation/retry semantics;
- `docs/CAPABILITIES.md` owns provider health/lifecycle/availability semantics;
- `docs/SYSTEM_INDEX.md` owns index/source health and freshness;
- `docs/POLICY_IMPLEMENTATION.md` owns typed privileged mutation through `portus-privd`;
- `docs/SECRETS.md` and provider-specific security documents own secret-safe diagnostics;
- `docs/ACCEPTANCE.md` owns the blocking acceptance proof;
- `docs/BOOT_STORAGE_RECOVERY.md` owns low-level boot/storage topology and recovery details;
- `docs/OPENRC_SERVICES.md` owns service/session lifecycle ownership and OpenRC supervision details.

This document must not turn PortusOS into a replacement for native Linux diagnostics or administration.

---

## 2. Core principles

### 2.1 Health is an observation, not universal truth

A health record reports what an authoritative or reviewed source established at a particular time.

It must preserve:

- source;
- observation time;
- reason;
- relevant generation/identity;
- whether the source itself was reachable.

`portusd` may aggregate observations but is not automatically authoritative for OpenRC, `/proc`, X11, i3, provider-domain state, storage, or external network services.

### 2.2 Health state is separate from lifecycle state

Do not overload one vocabulary for every subsystem.

Examples:

```text
Task lifecycle:      running / waiting / succeeded / failed / ...
Provider health:     healthy / degraded / unavailable / unknown
Index freshness:     live / recent / stale / unavailable / historical
OpenRC service:      started / stopped / crashed / unknown
```

A common health projection may summarize those sources, but it must not replace their authoritative state machines.

### 2.3 Degradation should be local

Failure of one component should degrade only the capabilities that depend on it where possible.

Examples:

```text
portus-apid unavailable
  -> protected API operations unavailable
  -> Linux, SSH, Codex ordinary work, portusd, PortusBrowser remain usable

X11 unavailable
  -> graphical/window index domains degraded
  -> non-graphical tasks/providers/SSH remain usable

portusd unavailable
  -> runtime-backed portus-os operations unavailable
  -> native Linux/OpenRC/SSH/Codex/provider interfaces remain independently usable
```

### 2.4 Recovery must not invent authoritative state

Automatic recovery may restore a known configuration, reconnect a source, restart a restart-safe service, or rebuild derived state.

It must not silently:

- recreate a corrupt authoritative database as empty;
- rewrite administrator policy;
- generate/reclassify credentials;
- weaken a secret boundary;
- guess task completion;
- substitute a different provider with weaker semantics;
- change firewall/network exposure without a declared operation;
- reinstall arbitrary packages;
- repartition storage;
- modify boot configuration speculatively.

---

## 3. Common health vocabulary

The common top-level PortusOS health states are:

```text
healthy
degraded
unavailable
unknown
```

These match the provider health vocabulary already locked by `docs/CAPABILITIES.md`.

### `healthy`

The component/source is functioning sufficiently for its declared current responsibilities and the observation is fresh enough to support that statement.

Healthy does not mean every optional resource exists. A healthy PortusBrowser provider can have zero connected browser sessions.

### `degraded`

The component remains partly usable but one or more expected functions, sources, dependencies, performance/resource conditions, or relationships are impaired.

Examples:

- System Index healthy for process/OpenRC data but X11 source unavailable;
- provider reachable but upstream network unavailable;
- task runtime relationship uncertain while task state is reconciling;
- filesystem writable but low-space warning active.

### `unavailable`

The component/capability required for the referenced operation cannot currently be used.

Examples:

- service not running after restart attempts are exhausted;
- provider socket missing;
- authoritative database cannot be opened safely;
- required upstream is unreachable and the operation cannot proceed.

### `unknown`

PortusOS cannot currently establish health reliably.

Unknown is not silently treated as healthy or unavailable.

---

## 4. Structured health observation

A health observation contains conceptually:

```text
component_ref
component_type
health_state
reason_code
summary
source
observed_at
source_generation/reference where applicable
last_healthy_at where known
recovery_disposition
recovery_attempt_count where applicable
impact/dependency summary where useful
safe_details
```

### 4.1 Component references

Health does not invent a new permanent identity when an authoritative identity already exists.

Examples:

```text
runtime:portusd
privilege:portus-privd
provider_<UUIDv7>
index:system
index-source:x11
state:portus.db
openrc:sshd
storage:/
task_<UUIDv7>   (derived task-health view only)
```

Provider registrations use their immutable provider registration IDs.

### 4.2 Reason codes

Reason codes are typed, stable semantic identifiers. They are not arbitrary prose.

Common families include conceptually:

```text
service_not_running
service_restart_exhausted
socket_unavailable
ipc_failed
state_unavailable
state_integrity_failed
source_disconnected
source_stale
provider_degraded
provider_unavailable
upstream_unreachable
tls_failure
policy_denied
approval_required
resource_low
resource_exhausted
reconciliation_required
reconciliation_failed
rebuild_required
rebuild_failed
configuration_invalid
incompatible
recovery_exhausted
manual_recovery_required
```

Subsystems may define narrower reason codes, but generic health output should map them to a bounded safe family without discarding the provider/native detail reference.

---

## 5. Source authority

The health aggregator must preserve authority boundaries.

| Question | Primary authority/source |
| --- | --- |
| Is an OpenRC service started? | OpenRC/native service state |
| Does a process generation exist? | `/proc`/native process state |
| Is `portusd` responding? | socket probe + OpenRC/process evidence |
| Is `portus.db` structurally usable? | SQLite integrity/schema checks defined by `docs/STATE.md` |
| Is an index source connected/fresh? | System Index/source adapter |
| Is a provider healthy? | provider/provider adapter + known service relationship |
| Is a provider compatible? | capability registry/provider contract, not health state |
| Is a task running/succeeded? | task registry + authoritative backend reconciliation |
| Is storage low/full? | native filesystem/storage observations |
| Is an upstream service usable? | provider-domain bounded probe/operation evidence |

`portusd` may materialize the latest bounded observations in SQLite for inspection/recovery history, but native/provider authority remains authoritative.

---

## 6. Recovery dispositions

Health state and recovery action are separate dimensions.

The common recovery dispositions are:

```text
observe
reconcile
restart
repair
administrator_required
terminal
```

### `observe`

No state-changing recovery action is justified. Continue monitoring/reporting.

### `reconcile`

Re-query authoritative sources and repair only derived relationships/cache state.

Examples:

- refresh/reconcile System Index source observations;
- reconcile task/backend relationships after daemon restart;
- revalidate provider resources after provider reconnect.

### `restart`

Restart a component only when its service/provider contract declares restart safe and the operation cannot corrupt or duplicate authoritative work.

### `repair`

Perform a bounded deterministic repair that restores known valid configuration/derived state.

Examples may include:

- rebuild derived index cache;
- recreate an expected transient runtime directory/socket location during service startup;
- restore a generated file from an administrator-controlled canonical template when that behavior is explicitly defined.

Repair is not permission to improvise.

### `administrator_required`

Human/root intervention is required because the correct action changes authoritative/security-sensitive state or requires a judgment not safely encoded.

Examples:

- protected credential store corruption;
- policy corruption;
- destructive storage repair;
- ambiguous package transaction;
- credential reprovisioning;
- boot repair requiring device/layout decisions.

### `terminal`

Automatic recovery for the current incident is exhausted or unsafe. The affected component/workflow stops attempting automatic repair and exposes stable failure evidence.

`terminal` does not mean the whole OS is unusable.

---

## 7. Automatic recovery boundary

Automatic recovery is allowed only when all of the following hold:

1. the responsible component/contract explicitly declares the action safe;
2. the recovery target is identified unambiguously;
3. expected state/generation is checked where relevant;
4. the action cannot silently broaden authority or weaken security;
5. the action cannot duplicate an unknown side effect;
6. the action is bounded by retry/restart limits;
7. failure evidence remains visible after exhaustion;
8. unrelated components remain unaffected where possible.

### 7.1 Safe first classes

The first architecture permits automatic behavior such as:

- reconnecting an event/query source;
- targeted System Index refresh/reconciliation;
- rebuilding derived index cache;
- revalidating provider resources after reconnect;
- restarting a declared restart-safe first-party Portus service;
- rebuilding transient runtime directories on clean service startup;
- retrying a health probe with bounded backoff.

### 7.2 Prohibited automatic behavior

Automatic health recovery must not:

- reset/recreate corrupt durable Portus state as empty;
- reset tasks to convenient states;
- delete evidence to make health appear green;
- create/rotate/export protected credentials;
- downgrade to Master-readable credential storage;
- modify administrator policy;
- grant privileged groups/capabilities;
- perform arbitrary package install/upgrade/rollback;
- rewrite bootloader/storage layout;
- follow an unsafe provider fallback;
- repeatedly run a destructive operation after uncertain completion.

---

## 8. Restart and crash-loop limits

PortusOS requires bounded restart behavior for automatically recovered Portus-owned services.

The first architectural default is:

```text
maximum automatic restart attempts: 3
window:                           10 minutes
minimum backoff sequence:        1s, 5s, 30s
```

A service may choose a slower/more conservative contract but may not silently exceed the common default without an explicit component-specific reason documented in the OpenRC/health implementation.

After the limit is exhausted:

```text
health_state          -> unavailable or degraded as appropriate
reason_code           -> service_restart_exhausted / recovery_exhausted
recovery_disposition  -> administrator_required or terminal
```

No tight infinite restart loop is permitted.

A successful stable recovery resets the crash-loop window only after the component has remained healthy long enough to show that it is not immediately cycling. The exact stable interval is an OpenRC/service implementation value; it must be at least 60 seconds for the first implementation.

This restart budget applies to automatic Portus/OpenRC recovery. An administrator may deliberately restart a component after exhaustion.

---

## 9. System Index health

`docs/SYSTEM_INDEX.md` remains authoritative for index/source state.

Health aggregation must distinguish:

```text
index subsystem health
source health
result freshness/completeness
```

A partially degraded source does not automatically make the whole index unavailable.

Example:

```text
index:system
  health: degraded
  reason: source_disconnected

index-source:x11
  health: unavailable

index-source:proc
  health: healthy

index-source:openrc
  health: healthy
```

Safe automatic actions include targeted reconnect, refresh, reconcile, and rebuild of derived data.

Rebuild must never delete durable task/provider/artifact/approval/annotation truth.

---

## 10. Task health

Task lifecycle remains authoritative in `docs/TASKS.md`.

PortusOS may expose a derived health assessment for active tasks only to explain execution relationships. It must not replace task state.

Examples:

```text
task state: running
backend confirmed running
  -> derived health healthy

task state: running
backend temporarily unreachable
  -> derived health degraded/unknown
  -> task may enter reconciling according to TASKS

task state: succeeded
  -> terminal lifecycle; no ongoing task-health requirement
```

Automatic task retry/restart follows `docs/TASKS.md`, not generic service restart rules.

A task is never restarted merely because generic health aggregation says it is degraded.

---

## 11. Provider health

Provider health remains governed by `docs/CAPABILITIES.md` and provider-specific contracts.

Common requirements:

- provider health is separate from compatibility;
- provider health is separate from per-capability availability;
- provider health is separate from existence of live provider resources;
- provider probes are bounded/typed, never arbitrary manifest shell fragments;
- provider recovery respects declared lifecycle ownership;
- provider-owned resources are not blindly recreated after service restart;
- a provider may be restarted automatically only if its lifecycle contract marks the service restart-safe.

For first-party providers such as `portus-apid`, service restart may be attempted automatically within Section 8 limits, but protected credentials must never be copied out of the protected store as a recovery mechanism.

---

## 12. Privilege helper health

`portus-privd` is a narrow root security boundary.

Its failure means typed Portus privileged operations are unavailable. It does not mean the machine has lost root/native administration.

Automatic restart may be attempted within the common restart budget, but health recovery must not:

- replace its root-owned policy with defaults;
- widen socket permissions;
- bypass peer-credential checks;
- fall back to generic passwordless sudo/root shell execution.

If its policy/configuration cannot be validated, the service fails closed and requires administrator recovery.

---

## 13. State/database health

`docs/STATE.md` controls database integrity and recovery.

Health may report:

```text
healthy
  schema understood and required checks pass

degraded
  non-critical derived domains need rebuild/reconciliation

unavailable
  durable database cannot be opened/validated sufficiently for normal mutation

unknown
  diagnostic check could not establish condition safely
```

A corrupt durable `portus.db` is never automatically replaced with a fresh empty database.

Derived index/cache data may be rebuilt only while preserving durable Portus truth.

Database recovery that risks durable records is `administrator_required`.

---

## 14. Disk and resource warnings

PortusOS must expose resource pressure when it materially threatens reliable operation, without becoming a high-frequency telemetry platform.

First required resource observations are:

- filesystem containing `/var/lib/portus`;
- filesystem containing `/workspace` when separate;
- filesystem containing `/var/log/portus` when separate;
- memory availability sufficient for control-plane operation;
- inability to create required runtime/state files.

### 14.1 Storage warning semantics

The first default storage warning thresholds are:

```text
warning/degraded:
  free space < 10% OR < 2 GiB

critical/degraded:
  free space < 5% OR < 512 MiB

unavailable:
  required write actually fails or filesystem is not writable/mounted as required
```

The percentage and absolute threshold are OR conditions so small and large filesystems are both handled conservatively.

These are defaults, not permission to delete data automatically.

Automatic cleanup may remove only explicitly disposable Portus-owned transient/cache data under a separately defined retention contract. It may not delete projects, user files, retained artifacts, task evidence, protected stores, or durable state merely to clear the warning.

### 14.2 Memory/resource pressure

Memory pressure may produce a bounded warning based on native `MemAvailable`/equivalent observations, but the first ISO does not require a resident telemetry daemon or automatic process killing.

The implementation must report actual resource pressure/failure evidence rather than invent a health downgrade from instantaneous cosmetic utilization alone.

---

## 15. `portus-os status`, `health`, and `doctor`

These surfaces remain intentionally distinct.

```text
portus-os status
  -> cheap overall operational summary through the normal runtime

portus-os health
  -> bounded structured current health catalogue

portus-os health show <component-ref>
  -> one detailed health observation

portus-os health degraded
  -> degraded/unavailable caller-visible components

portus-os doctor [domain]
  -> active bounded diagnosis, including when portusd is broken
```

### 15.1 No generic automatic repair command in the first contract

This document does **not** add a generic:

```text
portus-os health repair --everything
```

or similar command.

Recovery normally occurs through:

- component-owned bounded automatic recovery;
- System Index refresh/reconcile/rebuild commands already defined;
- task/provider-specific recovery contracts;
- native OpenRC/Linux administration;
- root-only typed Portus operations where already defined;
- administrator/live-ISO recovery for higher-risk failures.

This preserves the non-duplication rule and avoids turning health aggregation into a universal mutation API.

---

## 16. `portus-os doctor` scope

`doctor` gathers and explains bounded evidence. It does not reimplement every native diagnostic tool.

The existing doctor groups remain:

```text
runtime
state
index
providers
codex
```

Bare `doctor` aggregates supported groups.

The implementation should diagnose at least the first-ISO domains already listed by `docs/ACCEPTANCE.md`, including OpenRC/network/SSH/Master identity/graphical availability/Codex/Portus runtime/state/index/providers/core storage.

### 16.1 Independent checks

Where necessary to diagnose `portusd`, doctor may directly inspect bounded native state such as:

- expected socket/path existence and ownership;
- OpenRC service status;
- SQLite file/schema/integrity at the diagnostic level allowed by `docs/STATE.md`;
- required account/path existence;
- safe provider/service availability evidence;
- core filesystem condition.

It does not mutate authoritative state merely to make diagnosis pass.

---

## 17. Failure evidence

Significant health/recovery incidents retain bounded evidence sufficient to explain:

```text
what failed
when it was observed
which source reported it
what was affected
what automatic action was attempted
how many attempts occurred
what the latest outcome is
what recovery disposition remains
```

Do not retain an unbounded telemetry/event stream.

High-frequency logs remain component/native-log owned.

Portus state may retain only significant health/recovery observations required for diagnosis, task/provider relationships, crash-loop accounting, and recent recovery history.

Exact retention period is an implementation policy, but the first implementation must bound both event count and age rather than retaining health history forever.

---

## 18. Diagnostic bundles

A diagnostic bundle is an explicitly requested artifact containing allowlisted evidence for support/recovery.

It is not automatically generated for every failure.

A diagnostic bundle may include bounded/redacted:

- component/version inventory;
- current health observations;
- relevant OpenRC status;
- selected safe configuration metadata;
- recent significant Portus events;
- bounded selected logs;
- state/schema/integrity result summaries;
- System Index source-health summaries;
- provider-safe health metadata;
- recovery attempts/outcomes.

It must not blindly copy entire configuration, home, project, browser profile, process environment, credential store, or databases.

Diagnostic bundles are Portus artifacts under `docs/ARTIFACTS.md` when registered.

### 18.1 Secret safety

Diagnostic bundles must exclude by construction:

- reusable credentials;
- authorization headers;
- private keys;
- browser cookies/session databases;
- protected provider-store contents;
- arbitrary environment variables;
- complete request/response bodies unless explicitly safe/needed;
- raw database pages merely for convenience.

Redaction failure is a security defect.

The first ISO does not require automated external delivery of diagnostic bundles.

---

## 19. Recovery without Master Portus

Master Portus is not a recovery prerequisite.

At minimum:

```text
local TTY
SSH
root/administrator access
OpenRC/native commands
logs
installed portus-os doctor
```

remain usable according to the failure boundary.

If Master/Codex startup is broken, the operator can repair the machine without first making Master Portus functional.

If `portusd` is broken, native diagnostics and `portus-os doctor` remain usable.

If `portus-privd` is broken, native root administration remains available.

If `portus-apid` is broken, protected API use fails closed while ordinary Linux/Portus work remains available.

---

## 20. Live-ISO recovery boundary

The installed system must have an independent live-ISO/equivalent recovery route as required by `docs/ACCEPTANCE.md`.

The live environment is expected eventually to support the selected boot/storage design sufficiently to:

- detect the installation;
- unlock encrypted storage with administrator-controlled credentials;
- inspect/repair filesystems as supported;
- mount the installed system;
- inspect OpenRC/configuration/state/logs;
- repair boot/service/package configuration through documented native/admin procedures;
- recover or restore Portus state according to `docs/STATE.md`;
- preserve protected-secret boundaries during repair.

Exact GRUB/LUKS/LVM/filesystem commands and tooling belong to `docs/BOOT_STORAGE_RECOVERY.md` after those choices are locked.

This document locks the independence requirement, not premature storage commands.

---

## 21. Restart/reboot reconciliation

### `portusd` restart

After same-boot restart:

- load durable Portus truth;
- mark uncertain live observations for reconciliation;
- reconcile task/provider/index relationships against authoritative sources;
- never blindly restart unknown externally owned work.

### Provider restart

- provider registration identity remains the same installed registration generation unless uninstall/re-registration occurred;
- provider-session/resource references are revalidated;
- stale session-scoped resources do not silently rebind.

### Machine reboot

- boot-scoped live resources expire;
- durable Portus truth survives according to its subsystem contract;
- providers/services start through OpenRC contracts;
- live topology/index is reconstructed;
- tasks reconcile according to `docs/TASKS.md` rather than being blindly restarted.

---

## 22. First-ISO contract

The first ISO does not need universal self-healing.

It must prove:

1. common health states/reasons are exposed for required first-ISO components;
2. `portus-os health` can report the running runtime's bounded health view;
3. `portus-os doctor` can diagnose required domains independently enough to identify dead `portusd`;
4. stopping `portusd` preserves native Linux/SSH/OpenRC/admin recovery and does not erase state;
5. protected-provider failure fails closed without credential downgrade;
6. graphical failure preserves non-graphical recovery;
7. at least one restart/reconcile recovery path succeeds without inventing authoritative state;
8. automatic service recovery, if enabled, is bounded by the restart/crash-loop contract;
9. low/exhausted storage conditions are represented without automatic deletion of authoritative/user data;
10. diagnostic evidence remains bounded and secret-safe;
11. live-ISO/equivalent recovery remains an independent architectural path.

This refines existing acceptance tests; it does not add a new first-ISO feature family.

---

## 23. Validation checklist

### Health

- [x] `healthy`, `degraded`, `unavailable`, `unknown` are represented consistently in the host-safe implementation.
- [x] Reason codes are typed and source-attributed.
- [x] Health observations remain separate from subsystem lifecycle/freshness semantics.
- [x] Partial source/provider degradation remains localized in host-safe aggregation tests.

### Automatic recovery

- [x] Recovery actions are finite/declared and the common restart budget is bounded.
- [ ] Real restart-safe OpenRC services stop automatic retries after the configured budget on Linux.
- [ ] Real supervised-service execution proves no infinite restart loop occurs.
- [x] Derived System Index state can be reconciled without destroying durable truth.
- [x] Corrupt durable state is not silently reset by the state engine.
- [x] Host-safe recovery tests prove policy is unchanged and no weaker recovery path is introduced.

### Native Linux probe path

- [x] Real Linux memory/storage observations execute through the authenticated Unix runtime and CLI on the recorded disposable Linux VM.
- [ ] Repeat/tune those probes on canonical installed Artix paths and under real OpenRC supervision.

### Diagnostics

- [x] `health` is daemon-backed current typed health.
- [x] `doctor` can identify dead `portusd` without depending on it.
- [x] Diagnostic evidence is bounded.
- [x] `doctor --bundle` uses allowlisted content, a 64 KiB bound, and create-new/no-overwrite semantics without reusable secrets.

### Recovery boundaries

- [ ] TTY/SSH/root recovery survive Master failure.
- [ ] Native administration survives `portusd`/`portus-privd` failure.
- [ ] Protected API provider failure degrades only protected API use.
- [ ] GUI failure does not disable non-GUI recovery.
- [ ] Reboot/restart reconciliation does not blindly restart unknown work.

---

## 24. Decisions locked by this document

1. Common PortusOS health states are `healthy`, `degraded`, `unavailable`, and `unknown`.
2. Health observations are source-attributed, timestamped, typed, and separate from authoritative subsystem lifecycle/freshness state.
3. Common recovery dispositions are `observe`, `reconcile`, `restart`, `repair`, `administrator_required`, and `terminal`.
4. Automatic recovery may restore known configuration/rebuild derived state but may not invent or silently replace authoritative/security-sensitive state.
5. Automatic restart is allowed only for components explicitly declared restart-safe.
6. The first common automatic restart budget is three attempts in ten minutes with minimum backoff 1s/5s/30s and at least 60 seconds of stable health before crash-loop reset.
7. Exhausted automatic recovery becomes visible degraded/unavailable state rather than an infinite hidden retry loop.
8. System Index reconciliation/rebuild may repair derived observations but cannot destroy durable Portus truth.
9. Task lifecycle remains authoritative; generic health never blindly retries/restarts tasks.
10. Provider health remains separate from compatibility, capability availability, and live-resource existence.
11. `portus-privd` failure never falls back to generic root execution or weaker policy.
12. Durable `portus.db` corruption never causes automatic empty-state recreation.
13. First default storage warnings are degraded below 10% or 2 GiB free, critical below 5% or 512 MiB, while true unavailability requires a failed/unusable required write/filesystem condition.
14. Resource warnings do not authorize deletion of projects, user files, durable state, retained artifacts, protected stores, or evidence.
15. `status`, `health`, and `doctor` are separate surfaces with separate purposes.
16. No generic automatic `health repair --everything` command is part of the first contract.
17. `doctor` gathers/explains bounded evidence and does not become a second control plane.
18. Diagnostic bundles are explicitly requested, allowlisted, redacted Portus artifacts and are not automatically externally delivered.
19. Master Portus, `portusd`, `portus-privd`, `portus-apid`, and the GUI each have independent failure/recovery boundaries.
20. Same-boot restart and reboot use reconciliation against authoritative sources rather than blind work restart.
21. Independent TTY/SSH/root/OpenRC/live-ISO recovery remains a required architecture boundary.

---

## 25. P11 implementation status and remaining Linux work

P11 resolves the source-neutral implementation details that were previously open:

- `portus-protocol` now owns the finite component/reason/recovery-action/outcome vocabularies and principal-aware `HealthObservation`/`RecoveryAttempt` wire types;
- backup-required `portus.db` schema v7 normalizes the original loose health/recovery tables into current observations plus recovery-attempt history;
- current health responses are bounded to 128 visible components; safe details are at most 16 key/value fields with bounded key/value/JSON sizes and secret-like field names rejected;
- significant health changes use the common `significant_events` store, retain the existing 512-events-per-object ceiling, and additionally prune health changes older than 30 days;
- recovery history is bounded to 32 attempts/component and seven days;
- the restart budget is exactly three attempts in ten minutes, with 1s/5s/30s minimum backoff and 60 seconds stable-health reset;
- first storage classification implements the locked warning/critical thresholds; the first memory hook reports low below 5% or 512 MiB available and critical below 2% or 256 MiB, without killing processes or authorizing cleanup;
- Linux native probes are read-only `statvfs` checks for `/var/lib/portus`, `/workspace`, `/var/log/portus` plus `/proc/meminfo` `MemTotal`/`MemAvailable`; probe failure yields `unknown` rather than fabricated health;
- `portusd` owns fresh caller-filtered aggregation and the typed `health.list`/`health.show`/`health.degraded` methods; only System Index reconciliation is currently registered as an automatic source-neutral recovery adapter;
- `portus-os doctor [domain] --bundle <path>` writes schema-version-1 allowlisted JSON, capped at 64 KiB and refusing overwrite. P12 may register that resulting ordinary file as an artifact; P11 does not duplicate artifact ownership.

Remaining P11 work requires real Linux/Artix/OpenRC evidence rather than another source-neutral design:

- repeat and tune the already-proven native resource probes on the actual installed Artix paths;
- lock service-specific restart-safety declarations and implement the reviewed OpenRC restart adapters/startup ordering;
- prove the common restart budget and stable reset against real supervised service failures;
- choose periodic probe cadence/timeouts only after Linux measurements justify them; the host-safe implementation performs bounded on-demand aggregation;
- validate installed diagnostic file/path permissions and provider/service degradation behavior;
- prove TTY/SSH/root/native OpenRC recovery independently of Master/`portusd` failure;
- define and exercise live-ISO repair commands only after the boot/storage authorities are locked.

Those Linux mechanisms may tune probe cadence and component-specific adapters, but they must not reopen the health vocabulary, conservative recovery boundary, restart budget, diagnostic safety, or independent recovery model without an explicit architecture change.
