# PortusOS Codex Implementation Baseline

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative hard-cutover document

**Last reconciled:** 2026-08-28

**Behavioral evidence baseline:** `0.149.0`, released 2026-08-20

**Selected first-ISO build pin:** `0.150.1`, released 2026-08-27

**Research boundary:** The detailed behavior/configuration audit in this document was performed against stable `rust-v0.149.0` and remains the behavioral evidence baseline until the corresponding areas are re-audited. The first-ISO build input is separately frozen to stable `rust-v0.150.1`. Post-release `main` and prerelease `0.151.0-alpha.*` builds may be noted as forward-looking research, but first-ISO or release-critical behavior must not depend on untagged/prerelease changes.

**Scope:** Codex information and implementation policy required to build PortusOS as a Codex-first Linux distribution

## 1. Authority and hard-cutover rule

This document replaces every older PortusOS assumption about Codex.

This file remains authoritative for Codex-specific matters. PortusOS build, package, session, policy and ISO integration must conform to this baseline together with the specialized public authorities under `docs/` and the machine-readable build contracts.

Older statements must be treated as superseded when they conflict with this document, including assumptions that:

- Codex must be installed through npm;
- Node.js and npm are required solely because PortusOS includes Codex;
- tmux alone provides Codex continuity;
- one root-like or independently privileged Master Codex process, rather than a normal non-root Linux user governed by administrator policy, is the correct security model;
- Codex configuration consists only of a user `config.toml`;
- subagents are merely a possible future capability;
- rules and hooks are optional details rather than part of the operating policy;
- project sessions can safely share one checkout while writing concurrently;
- `codex exec` is a secondary convenience rather than the stable automation interface;
- project-local `.codex/` content always loads;
- `features.codex_hooks` is the current hooks feature key;
- the OS may automatically install the newest Codex release during a reproducible build.

This is an implementation baseline, not a changelog or a list of suggestions. PortusOS implementation must follow the decisions below unless a later, source-backed owner decision explicitly replaces them.

Only official OpenAI documentation, the official `openai/codex` repository, and official Codex releases were used for this cutover.

## 2. Current Codex model relevant to PortusOS

Codex is no longer just an interactive coding command running inside a terminal. The current local platform includes:

- an interactive terminal agent;
- persistent sessions that can be resumed, named, forked, archived, restored, and deleted;
- stable non-interactive execution through `codex exec`, including headless resume and session fork;
- `codex queue` for sending follow-up text work to an existing local or remote Codex session;
- the interactive `codex agents` dashboard for searching, starting, opening, renaming, stopping, and observing loaded root-session tasks and their subagent status;
- persisted thread goals for long-running objective tracking inside a Codex thread;
- automatic and manual context compaction with configurable thresholds and `PreCompact`/`PostCompact` lifecycle hooks;
- machine-readable JSONL event streams and JSON Schema-constrained final output;
- stable code review commands;
- Linux sandboxing through Bubblewrap;
- layered system, user, profile, and trusted-project configuration;
- admin-enforced requirements through `/etc/codex/requirements.toml`;
- named permission profiles for filesystem and network policy;
- rules for command-prefix approval policy;
- lifecycle hooks;
- repository, user, admin, and built-in skills, including the bundled `skill-creator` workflow for creating or updating skills;
- skill discovery/reload surfaces and change notifications in the app-server protocol;
- installable plugins and plugin marketplaces;
- local and remote MCP servers;
- stable multi-agent/subagent workflows;
- custom agents with their own model, instructions, sandbox, skills, and MCP configuration;
- image inputs and optional web search;
- an SDK and app-server for later programmatic integrations;
- a built-in `codex doctor` diagnostic command.

PortusOS must therefore treat Codex as a managed local agent runtime with configuration, policy, state, extensions, diagnostics, and lifecycle—not merely as a package and shell command.

## 3. Immediate hard-cutover decisions

### 3.1 Installation is standalone-package-first, not npm-first

OpenAI provides a standalone installer for macOS and Linux:

```bash
curl -fsSL https://chatgpt.com/codex/install.sh | sh
```

The installer supports a pinned release through `--release`, non-interactive operation through `CODEX_NON_INTERACTIVE`, and release-package SHA-256 verification. Inspection of the current installer and a disposable-HOME smoke test showed that it also manages `~/.codex/packages/standalone`, a `current` link, and shell-profile PATH state. Therefore PortusOS must **not** run that installer against the real ISO-builder user's HOME.

PortusOS uses the same official standalone **package artifact**, but stages it directly into the target filesystem with exact digest verification. The selected first-ISO package is:

```text
version:          0.150.1
release tag:      rust-v0.150.1
target:           x86_64-unknown-linux-musl
package:          codex-package-x86_64-unknown-linux-musl.tar.gz
package SHA-256:  00aba704f029f6dc0d948be407a756e0c97cc840132fd691353b2c6b0a505b17
checksum manifest: codex-package_SHA256SUMS
manifest SHA-256: 5580070dd9e1c06a603421111f32aa107fd77de2ab306986c11a26166b78e6fa
```

The package digest was independently confirmed both from official release metadata and from the downloaded official checksum manifest. The complete package contains the main `codex` executable plus the upstream `codex-code-mode-host`, bundled `rg`, bundled `bwrap`, bundled `zsh`, and `codex-package.json`; PortusOS must preserve that package-relative layout rather than extracting only the convenience single binary.

The selected target layout is:

```text
/usr/lib/codex/standalone/releases/0.150.1-x86_64-unknown-linux-musl/
/usr/local/bin/codex
  -> ../../lib/codex/standalone/releases/0.150.1-x86_64-unknown-linux-musl/bin/codex
```

A relocatable layout smoke test on the Linux build host succeeded and reported `codex-cli 0.150.1`. That is **build-input/layout evidence**, not yet target Artix compatibility evidence.

The build must:

1. consume the exact locked stable package/version from `portusos-build/components/codex.yaml`;
2. verify the package archive and checksum-manifest identities before staging;
3. preserve the complete upstream standalone-package layout under the versioned system root;
4. expose `/usr/local/bin/codex` through the locked relative symlink;
5. fail if target `codex --version` does not report the pinned version once native target staging exists;
6. keep automatic Codex update checks disabled;
7. retain the full Artix/OpenRC runtime compatibility matrix as a separate blocking acceptance gate.

The official installer remains useful upstream evidence/reference, but it is not the PortusOS target-staging mechanism. `npm install -g @openai/codex` remains an upstream-supported alternative, but it is not the PortusOS installation contract.

Consequences:

- Node.js and npm are not required solely for Codex.
- Node.js/npm may still be installed for JavaScript development, user-selected tooling, or stdio MCP servers launched through `npx`, but those are separate package decisions.
- The PortusOS package manifest must stop coupling Codex readiness to npm readiness.

### 3.2 Codex updates are explicit and versioned

A reproducible OS image must not silently move to whatever Codex release is newest during a build or normal startup.

PortusOS must:

- pin Codex in the build manifest;
- set `check_for_update_on_startup = false` in the system baseline;
- expose an explicit, logged Codex upgrade workflow;
- validate a new version in a disposable VM before changing the pinned version;
- rebuild or deliberately upgrade the installed system rather than allowing untracked drift;
- record `codex --version` in `portus-os doctor` output.

The stable `codex update` command may be used by the explicit upgrade workflow when the installed release supports self-update. It must not be run automatically by `portus-bootstrap`, `portus-master`, shell startup, OpenRC, cron, or login hooks.

The audited `0.149.0` release remains the detailed behavioral/source evidence baseline for this document. The selected first-ISO build pin is now `0.150.1` and is stored in the machine-readable `portusos-build/components/codex.yaml` contract. Changing that pin requires new release-asset verification and the appropriate Artix/VM compatibility evidence rather than silently following latest.

### 3.3 Artix is a required compatibility gate

OpenAI currently documents Ubuntu/Debian as the supported Linux baseline. PortusOS targets Artix/OpenRC, which is not explicitly listed as an upstream-supported distribution.

The official Linux release includes a musl target, and Codex is expected to be portable, but PortusOS must not convert that expectation into an untested claim.

Before Codex support is considered locked, Stage 1 must verify on the exact selected Artix/OpenRC base:

- standalone installation;
- command startup;
- ChatGPT authentication;
- credential storage through the selected Codex credential path and its real retrievability properties under the Master UID;
- interactive TUI rendering in Alacritty and tmux;
- Master Codex startup without a workspace-only filesystem sandbox;
- ordinary Master-user filesystem/application access matching the Linux UID;
- cross-user private-file denial by Linux ownership/modes;
- Bubblewrap sandbox startup for delegated/project agents;
- unprivileged user namespaces;
- project-agent workspace writes and denied writes outside the assigned scope;
- configured delegated-agent network restrictions/approval behavior;
- rules loading;
- hooks loading;
- trusted and untrusted project behavior;
- session persistence and resume after reboot;
- subagent creation and bounded background delegation;
- configured subagent model/reasoning defaults and runtime model-catalog visibility;
- `codex exec --json`, `codex exec resume`, and `codex exec fork`;
- `codex queue` to an existing controlled session;
- manual/automatic compaction behavior and `PreCompact`/`PostCompact` hooks;
- skill creation/reload through the supported skill surfaces;
- `codex doctor`.

Failure of one of these checks is a blocker to claiming that the ISO is Codex-ready.

## 4. Linux runtime and package requirements

### 4.1 Bubblewrap is required

On Linux, Codex uses Bubblewrap for local sandboxing. Codex can fall back to a bundled helper, but the official guidance is to install the distribution package and ensure unprivileged user namespace creation works.

PortusOS must include the Artix package providing `bwrap` as a required package.

`portus-os doctor` must validate:

```bash
command -v bwrap
bwrap --version
```

It must also run a real delegated/project-agent Codex sandbox smoke test. Merely finding the binary is insufficient. The Master Portus validation path must separately prove that Master Codex execution is not constrained by that project-agent sandbox.

The OS must not globally weaken kernel or namespace protections just to suppress a Codex warning. Any required Artix-specific user-namespace or security-profile configuration must be documented, narrowly scoped, and tested.

### 4.2 Git is operationally required

Codex works best in version-controlled directories, and `codex exec` requires a Git repository by default unless `--skip-git-repo-check` is deliberately supplied.

PortusOS must therefore treat Git as required and should initialize or clone projects as Git repositories unless the user explicitly chooses otherwise.

`--skip-git-repo-check` must not become a default launcher flag. It is an explicit exception for controlled non-repository automation.

### 4.3 Supporting packages

The Codex runtime readiness set must include at least:

- `git`;
- `curl`;
- CA certificates;
- `bubblewrap`;
- `sha256sum` or an equivalent digest tool;
- a functioning terminal and UTF-8 locale;
- a Secret Service-compatible keyring path for the selected credential-storage policy;
- Chromium and working default HTTP/HTTPS `xdg-open` integration for the ordinary ChatGPT browser-login flow; device-code authentication remains a headless/recovery fallback rather than the first-ISO default;
- common shell and process utilities required by development commands.

The final package names must be resolved against the selected Artix repositories.

### 4.4 VM resources

Upstream documents 4 GiB RAM as a minimum and 8 GiB as recommended. PortusOS VMware validation should use at least 8 GiB RAM for the normal single-agent path and should separately test the memory impact of concurrent subagents.

Multi-agent defaults must be conservative enough for the minimum supported PortusOS VM profile.

## 5. Codex state, continuity, and recovery

### 5.1 tmux and Codex solve different problems

The corrected continuity model is:

```text
tmux             = live terminal continuity while the OS remains running
Codex sessions    = agent-conversation continuity across terminal closure, crash, logout, and reboot
Git/filesystem    = project-state source of truth
Portus registry   = convenience index and launcher metadata
```

A tmux session disappears after reboot. A Codex session can be resumed from persisted Codex state.

PortusOS must never describe tmux alone as the Codex continuity layer.

### 5.2 Supported session operations

Current stable session operations relevant to PortusOS include:

```bash
codex resume
codex resume --last
codex resume --all
codex resume <SESSION_ID_OR_NAME>
codex fork
codex fork --last
codex archive <SESSION>
codex unarchive <SESSION>
codex delete <SESSION>
```

`codex resume --last` is scoped to the current working directory unless `--all` is supplied. That behavior makes the working directory part of the session-selection contract.

Launchers must always set the intended working directory before starting or resuming Codex.

### 5.3 Launcher behavior

`portus-master` must support explicit modes rather than blindly resuming or always creating a new session:

- `new`: start a new Master Portus session in `/workspace/<user>/master`;
- `resume`: open the session picker scoped to `/workspace/<user>/master`;
- `resume-last`: resume the latest interactive session associated with `/workspace/<user>/master`;
- `resume-id`: resume a recorded session ID or name;
- `shell`: remain in a usable shell without Codex;
- `doctor`: run Codex and PortusOS diagnostics.

The default may resume the last Master Portus session only when the registry and Codex session metadata agree that it belongs to the current Linux user's `/workspace/<user>/master`. If that cannot be established, the launcher must present the scoped picker or start a new session rather than resuming an unrelated chat.

Project launchers require the same modes, scoped to `/workspace/<user>/projects/<project>`.

`--all` is a recovery/inspection option, not the normal project launcher behavior.

### 5.4 Portus session registry

The Portus state registry must not duplicate full Codex transcripts. Codex owns its session data under `CODEX_HOME`.

The Portus registry should track only integration metadata:

- Portus project ID;
- project display name;
- canonical project path;
- Git repository root;
- tmux session name;
- Codex session ID and optional session name;
- session kind: master, project, maintenance, review, or automation;
- last Codex working directory;
- last opened timestamp;
- last known Codex version;
- permission profile used;
- i3 workspace assignment when relevant;
- archived/deleted status when known.

The registry is an index. Codex state and the project filesystem remain authoritative.

### 5.5 `CODEX_HOME`

`CODEX_HOME` defaults to `~/.codex` and contains configuration, authentication state, logs, sessions, skills-related state, caches, memories when enabled, worktrees, and standalone package metadata.

PortusOS should use the normal per-user location unless a tested requirement justifies changing it.

Do not place `CODEX_HOME` under `/workspace/<user>/master` or inside a project repository. Doing so would mix agent runtime state with project source and increase the chance of accidental commits or project-level access.

If `CODEX_HOME` is overridden, the directory must exist before Codex starts, and every launcher, backup rule, permission policy, and diagnostic must use the same value.

### 5.6 Root sessions are now a first-class delegation surface

Codex 0.149 changes the PortusOS execution picture because durable work does not require a visible terminal or a child subagent.

Codex now provides several distinct thread/session operations:

- interactive top-level sessions;
- headless `codex exec` sessions;
- `codex exec resume` for headless continuation;
- `codex exec fork` for creating a new independent branch from persisted session history;
- `codex queue` for sending text work to an existing local or explicitly remote session;
- an interactive `codex agents` dashboard that surfaces loaded root sessions and their subagent activity.

PortusOS must therefore distinguish a **Codex root thread/session** from a **Codex subagent** and from a **PortusOS task**.

A root Codex session may be interactive or headless. It owns its Codex conversation/history and can be resumed or forked independently. A subagent is delegated work inside a Codex session tree. A PortusOS task is the outer OS-level durable relationship used only when Portus-specific lifecycle, requester, policy, process, artifact, health, or recovery semantics are needed.

The existence of tmux must not determine which of those abstractions is used. A visible tmux/TUI Codex session is a presentation and human-interaction choice; it is not the default durability mechanism for complex work.

`codex queue` is useful for steering or appending work to a known existing Codex session, but PortusOS must not treat Codex's queue as the global Portus task queue. It is a Codex-thread input mechanism. Portus task truth, if required, remains Portus-owned.

### 5.7 Master Portus should remain a long-lived manager thread

The Master Portus Codex session should be designed as a long-lived orchestration thread rather than a disposable command worker.

Its context should preferentially contain:

- user intent and current conversation;
- orchestration decisions;
- task/delegation relationships;
- concise status and result synthesis;
- durable high-level operating instructions and references.

It should avoid accumulating long command-by-command execution traces when the work can be cleanly delegated to a subagent or separate headless Codex session.

Codex compaction materially improves the viability of this model by allowing a long-lived Master thread to compress older conversational context instead of forcing PortusOS to create a new Master session merely because the context window grows. Compaction is still conversational context management, not durable Portus state; PortusOS must not rely on a compacted summary as the authoritative task registry, workflow definition, policy record, or machine state.

## 6. Configuration and policy architecture

### 6.1 Required layers

Codex currently resolves ordinary configuration in this order, highest precedence first:

1. CLI flags and `--config` overrides;
2. trusted project `.codex/config.toml` layers, from project root toward the working directory, with the closest layer winning;
3. a selected profile file such as `~/.codex/review.config.toml`;
4. user configuration at `~/.codex/config.toml`;
5. system configuration at `/etc/codex/config.toml`;
6. built-in defaults.

PortusOS must use all layers deliberately:

- `/etc/codex/config.toml`: machine-wide defaults and PortusOS-defined profiles;
- `/etc/codex/requirements.toml`: non-overridable safety constraints;
- `~/.codex/config.toml`: user choices and user-owned integrations;
- `~/.codex/<name>.config.toml`: named execution profiles where appropriate;
- `<repo>/.codex/config.toml`: trusted project-specific behavior only;
- launcher flags: one-run selection, not a hidden replacement for durable policy.

### 6.2 Project trust is mandatory

Project `.codex/` configuration, hooks, and rules load only when the project is trusted. Untrusted projects skip those project layers while system and user layers remain active.

PortusOS must not automatically mark every directory or every cloned repository as trusted.

Trust policy:

- each user's `/workspace/<user>/master` may be pre-declared trusted because it is PortusOS-created for that Master Portus identity;
- a user-created local project may be trusted after creation if its source is known;
- a cloned or imported repository must begin untrusted unless the user explicitly approves it or the creation workflow verifies a trusted source under a documented policy;
- trust changes must be visible and logged;
- `portus-os doctor` must test that untrusted project hooks/rules/config do not load.

### 6.3 System defaults are not enough

Because project and CLI layers can override ordinary Codex configuration, security invariants must not live only in `/etc/codex/config.toml`.

PortusOS must use the combination of administrator-owned Codex configuration, Linux account/permission boundaries, PortusOS policy, provider policy, and protected credential infrastructure defined in `docs/POLICY.md` and `docs/SECRETS.md`.

Codex managed requirements remain useful for constraints that belong specifically to Codex, such as:

- which permission/execution profiles Codex may select;
- project-agent sandbox profiles;
- project trust behavior;
- rules, hooks, plugin/MCP restrictions where required;
- feature pins that must not be weakened by project configuration;
- protections for delegated agents and untrusted repositories.

Codex requirements must not be treated as the primary Master Portus user-isolation boundary. Linux user identity and administrator-owned OS policy are authoritative for that role.

### 6.4 Master versus delegated Codex execution

PortusOS now uses two distinct Codex security models.

#### Master Portus

Master Portus runs as the Master Portus Linux user defined by `docs/POLICY.md`.

The Master Codex session must not be constrained to a `:workspace`-style filesystem sandbox that prevents it from operating resources already granted to that Linux user.

The desired Master execution contract is:

```text
Codex Master session
  -> runs as Master Portus Linux UID
  -> ordinary Linux user access is available directly
  -> administrator policy controls extra system authority
  -> root-only actions still cross Linux privilege boundaries
  -> protected reusable credentials remain governed by SECRETS
```

The exact Codex configuration/profile syntax that expresses this on the pinned release must be validated before implementation. PortusOS must not assume that a profile name such as `:danger-full-access` is acceptable merely from its name; the selected configuration must be tested for its actual filesystem, network, approval and command behavior.

The hard requirement is the behavior: **Master Portus is not workspace-sandboxed.**

#### Project/task agents

Project agents, inspection agents, automation agents, and other delegated roles may and normally should use Codex permission profiles and Bubblewrap to enforce narrower task boundaries.

Typical delegated profiles include:

- project-scoped writable execution;
- read-only inspection;
- network-restricted automation;
- custom project/task scopes.

These profiles must remain subordinate to the owning Master user's Linux authority and administrator policy.

### 6.5 Permission-profile cutover

For Codex releases that support managed permission profiles, PortusOS should use them for delegated agents and any other bounded Codex role where they provide the required behavior.

Critical upstream constraint retained from the verified Codex baseline:

> Permission profiles do not compose with legacy `sandbox_mode` and `sandbox_workspace_write` settings.

Therefore active configuration must not accidentally mix incompatible legacy and profile-based sandbox models.

PortusOS target:

- validate permission profiles on the pinned Codex release;
- use a project-scoped profile for normal project agents;
- use a read-only profile for inspection/review roles;
- define a distinct Master invocation/profile whose behavior does **not** impose the project workspace sandbox;
- remove obsolete `portus-master` workspace-root rules from the active baseline;
- retain legacy sandbox settings only in fixtures where migration/fallback behavior is intentionally tested;
- block release if the selected Codex release cannot support the required separation between a non-workspace-sandboxed Master session and sandboxed delegated agents.

### 6.6 Baseline managed requirements direction

The exact file must be generated only after validation against the pinned Codex release.

Conceptually, the installed system needs separate allowed roles:

```text
Master Portus role
  -> native Linux-user execution boundary

Project agent role
  -> project/worktree sandbox

Inspection role
  -> read-only sandbox
```

The previous example that defined `portus-master` as a `:workspace` profile rooted at `/workspace/master` and `/workspace/projects` is superseded and must not be copied into implementation.

A future `/etc/codex/requirements.toml` may still allow named profiles such as a project and inspection profile while permitting the dedicated Master launcher to select the tested native-user execution behavior. The exact keys and profile names remain pinned-version implementation details.

Project-agent secret-path deny rules may be used as defense in depth, but the Master Portus hard credential boundary must not depend on Codex deny-read globs. `docs/SECRETS.md` requires an enforcement boundary that remains valid even though Master Portus has broad ordinary access to its own UID's files.

### 6.7 System config baseline

The exact generated Codex system config remains pinned-version dependent.

The stable PortusOS direction remains:

- automatic Codex update checks disabled;
- keyring/credential configuration selected deliberately and validated against `docs/SECRETS.md`;
- project trust enabled and tested;
- hooks/rules used only for appropriate deterministic guardrails and integration;
- multi-agent support available;
- native Codex memories disabled by owner policy;
- conservative measured subagent concurrency;
- Master launch behavior distinct from delegated project-agent sandbox behavior.

A same-user keyring configuration must not be described as protecting reusable Codex credentials from Master Portus until negative tests prove that the Master UID cannot export the value through ordinary user authority.

### 6.8 Model policy

PortusOS must not hardcode the model advertised as the current default in documentation into the OS contract.

Models, names, availability, and recommended reasoning settings change faster than the distro architecture.

Policy:

- allow Codex's supported default model unless the user or a task-specific profile chooses another;
- permit supported model selection;
- record the model used in automation output when relevant;
- define task-specific model choices in custom agents or profile files, not package manifests;
- treat any model pin as separately updateable configuration;
- never make the ISO build depend on one model slug remaining available.

## 7. Master Portus, project Codex, and operating authority

### 7.1 Master Portus uses the Linux user boundary

Master Portus is a broad system orchestrator running as a normal non-root Linux user.

The old workspace-sandboxed Master model is rejected.

The Master user is expected to operate ordinary resources available to that UID, including:

- its home directory;
- its user-scoped Master workspace;
- its user-scoped project workspaces;
- user-installed/available applications;
- its graphical and terminal sessions;
- ordinary user-owned configuration and data;
- system capabilities granted through administrator policy.

The containing PortusOS VM/PC/server is the broad environment boundary for Master Portus. Linux UID isolation separates it from other users. Administrator policy controls privileged/system authority beyond normal user access.

Master Portus remains non-root even when it is configured with broad capabilities.

### 7.2 Linux privilege remains authoritative

Codex configuration is not a replacement for Unix users, groups, file modes, ACLs, sudo, polkit, firewall rules, service ownership, device permissions, or PortusOS administrator policy.

Master Portus runs as its dedicated Linux user.

For operations that require root authority, PortusOS uses the explicit privilege mechanisms defined by `docs/POLICY.md`, which may include pre-authorized narrowly scoped sudo/polkit/helper/service capabilities suitable for unattended operation.

The system must not require a human Codex approval for every operation that the administrator already granted. Prompt/approval behavior is configurable policy, not the universal default safety boundary.

A capability that effectively grants arbitrary root takeover must be classified as root-equivalent even if implemented through sudo, a group, a socket, a container runtime, a helper, or another mechanism.

Master Portus must not be able to alter administrator-enforced policy to expand its own authority.

### 7.3 Master Portus and Codex approvals

The dedicated Master launcher must select a Codex execution mode consistent with the administrator's configured Master policy.

The implementation must not blindly use `--dangerously-bypass-approvals-and-sandbox` or another compatibility flag without validating its full behavior on the pinned release.

Instead the implementation must prove that:

- Master Portus is not workspace-sandboxed;
- allowed routine work can proceed unattended where policy grants it;
- denied operations remain denied by Linux/Portus/provider boundaries;
- prompt-configured operations can still request approval where the owner chose that behavior;
- root authority is not obtained merely because the Codex workspace sandbox is absent;
- protected credentials remain protected by their independent boundary.

### 7.4 Project Codex role

A normal project Codex session remains deliberately narrower than Master Portus.

It:

- starts in the owning user's project Git root, conceptually `/workspace/<user>/projects/<project>`;
- uses the validated project-agent permission/sandbox profile;
- writes only within the assigned workspace/worktree and task temporary space;
- does not receive unrelated projects as writable roots;
- receives only task-approved network/provider access;
- cannot alter administrator policy;
- cannot retrieve protected reusable credentials;
- loads project `.codex/`, `AGENTS.md`, hooks, rules, agents, and skills only after trust is established;
- maintains a separate Codex session identity from Master Portus.

### 7.5 Inspection and review role

Read-only investigation, audits, dependency review, and initial analysis should use a validated read-only profile or custom agent.

Codex's stable review surfaces may be used for uncommitted changes, base-branch diffs, commits, and custom review instructions.

The review path remains read-only unless the user explicitly starts a follow-up implementation run.

## 8. Authentication and secrets

### 8.1 Supported authentication

Codex supports ChatGPT sign-in, API-key authentication, device-code authentication, and other documented access-token flows.

PortusOS interactive use must default to ChatGPT sign-in through the bundled Chromium browser. The first-ISO live and installed graphical Master sessions must provide a working default HTTP/HTTPS handler so Codex can open its browser authentication URL and receive the localhost callback on the same machine.

For a terminal or remote environment where browser callback login is inconvenient, `codex login --device-auth` is the preferred headless/recovery path when device-code authentication is enabled for the account. API-key login remains available for users who deliberately want usage-based Platform authentication. Neither fallback replaces the first-ISO Chromium/ChatGPT subscription-login path.

Automation should prefer a scoped API key where appropriate rather than reusing a user’s interactive ChatGPT token cache.

### 8.2 Credential storage

`cli_auth_credentials_store` supports keyring/file/auto behavior on the verified Codex baseline, but the PortusOS security question is no longer merely whether the token is encrypted at rest.

Because Master Portus is a broad non-sandboxed Linux user, the implementation must determine whether the selected Codex credential mechanism prevents that same UID from exporting the reusable credential through ordinary shell/API access.

PortusOS must therefore classify the selected Codex credential path accurately:

- **protected from Master Portus** only if negative tests prove the Master UID cannot retrieve/export the reusable value;
- **user-readable credential storage** if the Master UID can retrieve it, even if the value is encrypted on disk or stored in a same-user keyring.

A keyring remains preferable to plaintext file storage for at-rest security, but it must not be described as the hard Master-Portus secret boundary without evidence.

`auth.json` contains access tokens and must be treated like a password. It must never be committed, included in an ISO, copied into a workspace, logged, pasted into Codex, or included in diagnostics.

The first ISO must test the actual selected mechanism under OpenRC/elogind, local graphical login, tmux attachment, and SSH continuation. `docs/CREDENTIAL_CLASSES.md` now locks the classification rule for Codex specifically: use a protected `portus-apid`/local-proxy path only if the supported Codex authentication mechanism can consume it without receiving the upstream reusable credential; otherwise the Codex credential is deliberately Master-readable and must be documented as such.

Silent downgrade from a credential explicitly configured as protected to plaintext/user-readable storage is prohibited.

### 8.3 AUTH terminal remains useful but is not the only boundary

The `AUTH` terminal/tmux surface remains useful for logins, passphrases, device codes and administrator interaction. For protected provisioning, the first concrete human-entry mechanism is the root-run `portus-auth` TTY utility defined by `docs/CREDENTIAL_CLASSES.md`; reusable secrets are not passed as CLI arguments or returned into the Master conversation.

AUTH is a workflow/provisioning surface, not the complete secret boundary.

PortusOS also requires:

- the protected-credential classification and enforcement rules in `docs/SECRETS.md`;
- correct Unix/service ownership;
- redaction in logs and diagnostics;
- no secret values in `AGENTS.md`, skills, hooks, rules, project config, Portus state, or knowledge files;
- explicit handling of browser/provider credentials;
- narrowly scoped credential use where a provider must expose a temporary value;
- negative tests for every credential claimed to be inaccessible to Master Portus.

### 8.4 Automation credentials

Automation credentials require separate classification.

A credential may be:

- intentionally readable by the owning Master user for a particular automation workflow; or
- protected behind a broker/provider that performs the authenticated action without exposing the reusable value.

Globally exporting reusable credentials in shell startup remains prohibited.

If a temporary credential is injected into a process environment, the implementation must consider same-UID `/proc` or process-inspection access. Environment injection cannot be called a hard boundary from Master Portus when the Master UID can inspect the value.

For protected credentials, prefer an operation-oriented broker/provider rather than a shell pattern that first returns the raw secret and then launches `codex exec`.

Where Codex-created code or another Master-owned SDK needs a provider API but the upstream credential is protected, the preferred compatibility path is a local authenticated provider-compatible proxy/broker endpoint that performs the upstream authenticated request without exposing the real key to the Master-owned process. A local proxy credential, if required by the SDK, must authorize only the local broker boundary and must not be a reusable upstream credential.

If a Codex/Master-owned process strictly requires possession of the real upstream credential and cannot use the protected provider/proxy, that path cannot honestly use the protected credential. The administrator must either choose a compatible path or deliberately provision/reclassify a Master-readable credential for that workflow.

Exact supported Codex-auth compatibility with the protected local proxy remains implementation research under `docs/CREDENTIAL_CLASSES.md`; until it is proven, any same-UID-readable Codex credential is classified Master-readable rather than being treated as a failed implementation of the general protected-secret architecture.


## 9. Instructions, skills, plugins, and project customization

### 9.1 `AGENTS.md` is the primary instruction hierarchy

Codex instruction discovery is layered:

1. global `AGENTS.override.md` or `AGENTS.md` under `CODEX_HOME`;
2. project instructions from the project root down to the current working directory;
3. at each directory, `AGENTS.override.md` wins over `AGENTS.md`, followed by configured fallback filenames;
4. one instruction file per directory is loaded;
5. combined project instruction content stops at `project_doc_max_bytes`, which defaults to 32 KiB.

PortusOS must provide:

- a concise global `~/.codex/AGENTS.md` describing PortusOS-wide invariants;
- `/workspace/<user>/master/AGENTS.md` defining that user's Master Portus role and state/CLI contracts;
- a root `AGENTS.md` template for projects;
- nested overrides only where a project area genuinely differs;
- tests that verify which instruction files load from representative directories.

Do not put secrets, transient session state, huge manuals, or complete project documentation into `AGENTS.md`.

Use it for durable operating instructions and links to authoritative project docs.

For Master Portus, `/workspace/<user>/master/AGENTS.md` is the concise **operating charter for the device manager/orchestrator role**. It should tell Master Portus, at minimum, that it must:

- treat itself as the primary manager/orchestrator of the owning user's PortusOS environment rather than as a generic command worker;
- keep its long-lived context focused on user intent, orchestration, decisions, delegation, status, and synthesis;
- choose direct execution versus Codex subagent versus independent headless Codex root session versus visible interactive session according to the routing policy;
- use Codex/native tools directly when they already solve ordinary file, search, Git, shell, code, or project work;
- use `portus-os` only for Portus-owned system/index/task/policy/health/cross-component capabilities rather than as a replacement shell;
- consult the shipped `portus-os-cli` skill for detailed Portus OS CLI command usage rather than carrying the command manual in `AGENTS.md`;
- consult the live system index and the durable per-machine knowledge map before repeatedly rediscovering known machine facts;
- update the durable machine knowledge map when it learns a stable, useful machine-specific fact that is likely to matter again;
- use Codex skills as independent reusable workflow units, and avoid accumulating unrelated personal/operational workflows inside the `portus-os-cli` skill;
- preserve the authority, freshness, secrets, and non-duplication boundaries defined by PortusOS documentation.

`AGENTS.md` must not become the machine inventory, CLI reference, workflow library, or task database. It defines the Master role and routes Codex to the correct durable surfaces.

### 9.2 Skills locations

Codex currently loads local skills from:

- repository paths: `.agents/skills` from the current working directory up to the repository root;
- user path: `$HOME/.agents/skills`;
- admin path: `/etc/codex/skills`;
- built-in system skills shipped by OpenAI.

PortusOS layout remains valid with these corrections:

- Master-only skills: `/workspace/<user>/master/.agents/skills`;
- project skills: `/workspace/<user>/projects/<project>/.agents/skills`;
- user-wide skills: `$HOME/.agents/skills`;
- machine-wide PortusOS skills: `/etc/codex/skills`.

Repository skill discovery depends on the launch directory and repository root. Launchers must start Codex in the intended project path.

PortusOS ships a machine-wide **`portus-os-cli` skill** under the machine-wide skill layer. Its scope is deliberately narrow: teach agents how to use the `portus-os` executable and its Portus-owned domains, structured outputs, common CLI usage patterns, freshness/inspection conventions, and CLI-specific recovery/error handling. `docs/CLI.md` is authoritative for the installed command/output/error contract, and `portus-os help --json` provides the machine-readable installed syntax that the skill/validation should be checked against. The skill is instruction/reference content; it is not the Rust implementation of the `portus-os` executable and does not determine the executable's implementation language.

The `portus-os-cli` skill may include common examples or short workflows whose purpose is to demonstrate correct CLI use. It must not become the general Master Portus workflow library. New reusable user/system workflows such as `daily-setup`, project-start routines, review routines, or other independent procedures belong in their own skills. This keeps CLI knowledge stable and prevents unrelated workflows from turning the CLI skill into a monolith.

The skill name does not change the executable contract: the command remains `portus-os`; there is no `portus-cli` or generic `portus` executable.

### 9.3 Plugins are the distribution unit

Skills are the authoring format for reusable workflows. Plugins are the installable distribution unit that can bundle skills, MCP servers/connectors, and install metadata.

PortusOS policy:

- use direct skill directories while authoring first-party PortusOS workflows;
- package stable, reusable, multi-machine PortusOS extensions as plugins later;
- pin plugin marketplace Git refs where reproducibility matters;
- do not install third-party plugins by default without a reviewed source and permission surface;
- use `codex plugin` and `codex plugin marketplace` as the stable management commands;
- treat plugin installation as code/tool installation, not harmless documentation import.

### 9.4 Skills are the native reusable-procedure format

Current Codex already has a first-class skill system and ships a `skill-creator` skill whose purpose is to create or update skills containing specialized knowledge, workflows, and tool integrations. The app-server surface can list skills, force a reload, emit `skills/changed`, configure skill enablement, and invoke a skill explicitly.

PortusOS must therefore **not invent a second generic text-based workflow language** for procedures that fit naturally as Codex skills.

Examples that may be represented as a skill include:

- a repeatable project bootstrap procedure;
- a standard repository review routine;
- a multi-step application setup whose steps are procedural rather than captured live state;
- a user-defined sequence such as "prepare my daily work environment" when the durable content is primarily instructions and decision logic.

Master Portus may propose creating or updating a skill when repeated work would clearly benefit from a reusable procedure. Owner intent still controls whether a personal workflow is saved, and generated skills must remain inspectable/editable rather than hidden model memory.

### 9.5 Saved machine setups may require Portus structured workflow state

A user request such as "save the current setup as my work setup" can include more than procedural instructions. It may capture live machine relationships such as:

- selected applications;
- particular provider/browser resources;
- URLs or tab groups;
- i3 workspaces;
- window placement or display relationships;
- project/workspace references;
- optional launch ordering or readiness conditions.

That kind of saved state should not be flattened into prose merely because skills exist. The likely PortusOS direction is:

```text
Codex skill
  = reusable procedure/instructions/decision logic

Portus workflow/scene record
  = structured saved machine/provider state and references

optional skill + workflow record
  = reusable procedure that loads or operates a structured saved setup
```

The exact Portus workflow/scene schema belongs to a later task/capability/index decision. This document locks only the non-duplication rule: use Codex skills for reusable agent procedure where they fit; add Portus-owned workflow state only for durable OS/provider relationships that Codex skills do not themselves model safely.

## 10. Rules, hooks, and deterministic policy

### 10.1 Rules

Codex rules live in a `rules/` directory next to an active config layer, including:

- `/etc/codex/rules/` for machine policy;
- `~/.codex/rules/` for user rules;
- `<repo>/.codex/rules/` for trusted project rules.

Rules use command-prefix matching and decisions such as `allow`, `prompt`, and `forbidden`. When more than one rule matches, the strictest decision wins.

PortusOS should use rules for commands whose safety can be determined from a stable prefix, such as:

- read-only Git inspection;
- read-only `gh` inspection;
- safe Portus status/doctor commands;
- explicit prompts for package installation, service mutation, firewall changes, disk tools, destructive Git operations, and shell entrypoints;
- forbidden commands or prefixes that violate PortusOS policy.

Smart approvals may propose a prefix rule. The user must review the entire prefix before accepting it; a broad prefix such as `sh`, `bash`, `sudo`, `python`, or a package manager must not be automatically allowed.

Every shipped rule file requires positive and negative cases and must be tested with:

```bash
codex execpolicy check --pretty --rules <file> -- <command>
```

`codex execpolicy` is currently marked experimental even though rules themselves are operational. PortusOS may use it as the official rule-test command, but must not make a release-critical runtime path depend on undocumented output fields.

### 10.2 Hooks

The current feature key is:

```toml
[features]
hooks = true
```

`features.codex_hooks` is a deprecated alias and must not appear in new PortusOS configuration.

Hooks may be defined in `hooks.json` or inline `[hooks]` tables next to active config layers. Use one representation per layer.

Relevant events include:

- `SessionStart` and `SessionEnd`;
- `UserPromptSubmit`;
- `PreToolUse` and `PostToolUse`;
- `PermissionRequest`;
- `PreCompact` and `PostCompact`;
- `SubagentStart`, `SubagentStop`, and `Stop`.

PortusOS hook uses may include:

- adding concise PortusOS session context at startup;
- recording non-secret session metadata;
- verifying the working directory and role profile;
- detecting obvious accidental secret submission;
- enforcing selected local tool policies;
- collecting command outcomes for diagnostics;
- updating the Portus session registry;
- recording subagent lifecycle and resource usage.

Hooks are guardrails, not the sole security boundary. Hosted tools are not covered by the same local hook path, some specialized tool paths may opt out, and transcript formats are not stable hook APIs.

Security must remain enforced by permission profiles, requirements, rules, Unix permissions, sudo/polkit, firewall policy, and user approval.

Hook output must never contain secrets. Hooks must have bounded timeouts and fail visibly without trapping the user in a broken login loop.

## 11. MCP policy

Codex has stable MCP management for:

- local stdio servers;
- streamable HTTP servers;
- OAuth login;
- bearer tokens referenced by environment variable;
- static headers;
- enable/disable state;
- startup and per-tool timeouts;
- required-server behavior;
- per-server tool allowlists and denylists;
- per-tool approval modes.

PortusOS must treat MCP as an optional extension surface, not as the core OS control layer.

Policy:

- Portus OS CLI (`portus-os`) remains the deterministic local OS workflow surface;
- MCP adds external tools, services, or context where it is the appropriate protocol;
- no third-party MCP server is enabled globally without source review and tool-surface review;
- remote MCP URLs must use TLS unless explicitly loopback/local;
- bearer tokens must use `bearer_token_env_var` or OAuth storage, never literal committed tokens;
- use `enabled_tools` to expose only needed operations;
- use `required = true` only when a workflow must fail closed without that server;
- set explicit startup and tool timeouts;
- record MCP identity and configuration source in diagnostics without exposing credentials;
- project-local MCP config loads only for trusted projects;
- MCP tools have their own approval controls and are not automatically constrained by local filesystem permission profiles in the same way as shell commands.

Potential MCP candidates must be selected by actual need. A speculative list is not an install requirement.

## 12. Multi-agent and parallel-work policy

### 12.1 Subagents are now a normal Codex capability

Multi-agent tools are stable and enabled by default in current Codex. Codex includes built-in agent roles such as `default`, `worker`, and `explorer`, and supports custom agents under:

- `~/.codex/agents/`;
- `<repo>/.codex/agents/` for trusted projects.

Custom agents can define:

- `name`;
- `description`;
- `developer_instructions`;
- model and reasoning effort;
- sandbox/permission-related configuration supported by that agent file;
- MCP servers;
- skill configuration.

PortusOS must not describe subagents as merely future functionality.

### 12.2 Project sessions and subagents are different

A Portus project Codex session is a durable top-level working session associated with a project, tmux session, and registry entry.

A Codex subagent is a delegated thread inside a parent Codex workflow.

Subagents do not replace:

- project workspaces;
- project tmux sessions;
- separate project histories;
- Portus registry entries for top-level sessions;
- Git branch/worktree isolation.

### 12.3 Safe delegation policy

Use parallel subagents primarily for independent read-heavy work:

- codebase mapping;
- documentation research;
- test/log analysis;
- security review;
- review by category;
- summarization;
- independent verification.

Parallel write-heavy work requires explicit isolation. Multiple agents must not edit the same checkout without a proven coordination strategy.

For concurrent implementation, use separate Git worktrees or separate project clones/branches. Each writing agent must receive:

- a distinct working directory;
- a distinct branch or detached worktree plan;
- a bounded task;
- clear ownership of files or changes;
- a merge/review step.

The first ISO must cap `agents.max_concurrent_threads_per_session` at a conservative measured value. No fixed concurrency count is architecture. The cap must be validated separately on the 4 GiB minimum VM and the 8 GiB reference VM, and PortusOS must prefer preserving a responsive Master session over maximizing parallel agent count.

### 12.4 Portus custom-agent direction

Useful first-party custom agents may include:

- `portus-explorer`: read-only system/project mapping;
- `portus-reviewer`: read-only correctness/security/test review;
- `portus-docs-researcher`: read-only official-documentation verification through a controlled MCP server;
- `portus-test-runner`: executes bounded tests and reports evidence;
- `portus-maintainer`: scoped maintenance after the workflow is proven.

These definitions belong in user, master, or project `.codex/agents/` layers according to scope. They must not receive broader permissions than their jobs require.

### 12.5 Delegation authorization belongs in Master instructions

Codex 0.149's built-in subagent guidance does not treat "this looks complicated" as sufficient authorization to spawn agents. It permits delegation when the user or applicable `AGENTS.md`/skill instructions explicitly authorize subagents, delegation, or parallel work.

PortusOS should use that contract rather than bypass it. `/workspace/<user>/master/AGENTS.md` must explicitly authorize Master Portus to delegate according to the PortusOS task-routing policy now locked in `docs/TASKS.md`.

That instruction should preserve Codex's own useful delegation discipline:

- delegate concrete, bounded, self-contained sidecar work;
- keep urgent critical-path blockers local when waiting would only add latency;
- avoid duplicating delegated work in the parent;
- keep doing useful non-overlapping work while children run;
- call `wait_agent` only when the child result is actually required for the next critical-path step;
- isolate concurrent writers through disjoint worktrees/write scopes.

This is a major reason PortusOS should not build a competing subagent scheduler.

### 12.6 Subagent model choice is policy, not a hardcoded model slug

Codex 0.149 supports `agents.default_subagent_model` and `agents.default_subagent_reasoning_effort`, and spawn calls can use supported model/reasoning overrides. The runtime model catalog determines which overrides are actually available.

PortusOS may configure a fast, economical model as the preferred default for routine delegated work when an appropriate model is available. A model such as GPT-5.6 Luna is a plausible candidate when present in the validated catalog, but **`gpt-5.6-luna` or any other model slug must not become an OS architectural dependency**.

The task-routing policy should select by role/capability first (for example `fast-runner`, `researcher`, `reviewer`) and resolve the current validated model through updateable Codex configuration. If the preferred model is unavailable, Codex/PortusOS should use an allowed fallback rather than fail the operating-system workflow solely because a model name changed.

### 12.7 Subagents are not the detached-work primitive

Subagents are best treated as bounded delegated work in a parent Codex session tree. They are valuable for parallel cognition and bounded operational sidecars, but PortusOS must not use them as the universal representation of every background job.

When work should have its own durable top-level history, independent lifecycle, resumability, queue, or continued execution while Master Portus handles unrelated requests, a separate headless Codex root session is usually the cleaner Codex-native primitive. Whether PortusOS additionally registers that work as a Portus task depends on the lifecycle requirements defined in `docs/TASKS.md`.

## 13. Interactive and non-interactive operating surfaces

### 13.1 Interactive Codex

Interactive Codex remains the primary **human-visible conversational** surface for Master Portus and project work. It is not the default execution surface for every complex task merely because a TUI can display it.

Important stable commands and slash commands include:

- `/init` for project instruction setup;
- `/status` for current workspace/config state;
- `/permissions` for permission inspection/selection;
- `/model` for model selection;
- `/review` for read-only review;
- `codex resume`, `fork`, `archive`, `unarchive`, and `delete` for session management;
- `codex doctor` for runtime diagnostics;
- image inputs through the supported image flags;
- optional web search when enabled by policy.

PortusOS wrappers must not hide these native capabilities unnecessarily.

They also must not duplicate Codex's ordinary file reading, source search, editing, patching, shell execution, project navigation, or access to Git and installed commands. PortusOS should add only missing system-level coordination, durable lifecycle, policy-mediated workflows, indexing, health, recovery, artifact handling, and application integration, as defined by `docs/CONTROL_PLANE.md`.

### 13.2 `codex exec` is the automation contract

`codex exec` is stable and is the correct surface for deterministic Portus automation, scripts, CI-like checks, and machine-consumed output.

Capabilities include:

- read-only sandbox by default;
- explicit permission selection;
- ephemeral runs through `--ephemeral`;
- JSONL event output through `--json`;
- final-message output files;
- JSON Schema-constrained final responses through `--output-schema`;
- session continuation through `codex exec resume --last` or a session ID;
- independent headless branching through `codex exec fork`;
- required-MCP fail-closed behavior;
- stdin pipelines.

New Portus automation must not use the deprecated `codex exec --full-auto` compatibility flag. It must select permissions explicitly.

Automation pattern:

```bash
codex exec \
  --json \
  --output-schema /etc/portus/schemas/task-result.schema.json \
  -o /var/lib/portus/results/task-result.json \
  "<bounded task>" \
  > /var/log/portus/codex-task.jsonl
```

Output handling must:

- keep machine events separate from human summaries;
- validate schema-constrained results;
- retain exit status;
- redact secrets;
- record Codex version, working directory, permission profile, and session/thread ID;
- cap log retention.

### 13.3 `codex agents` and `codex queue` are native session-management surfaces

Codex 0.149 adds two important stable-release CLI/TUI surfaces for orchestration:

- `codex agents` / `/agents` provides a dashboard over loaded root sessions, including project/status grouping and subagent activity;
- `codex queue --thread <THREAD> --message <TEXT>` sends a text message to an existing local or explicitly remote Codex session by UUID or exact name.

These features reduce the amount of session plumbing PortusOS needs to invent.

PortusOS may use native Codex session inspection and queueing where they cleanly satisfy the need. It should not copy Codex's internal thread queue into Portus SQLite or create a second TUI for the same purpose merely for ownership aesthetics.

However, `codex agents` is a human/agent-facing Codex session dashboard, not the Portus system index, and `codex queue` is a thread-input facility, not the Portus durable task registry. PortusOS still owns cross-component task identity only where OS-level relationships are required.

### 13.4 `codex doctor` and `portus-os doctor`

Codex now ships a stable `codex doctor` command that diagnoses installation, config, auth, runtime, Git, terminal, app-server, and thread inventory issues.

`portus-os doctor` must call or integrate `codex doctor`; it must not reimplement all Codex diagnostics from scratch.

`portus-os doctor` remains responsible for PortusOS-specific checks such as:

- package pin and installed version;
- Bubblewrap package and namespace behavior;
- OpenRC/elogind keyring startup;
- generated system config and requirements files;
- Portus permission-profile policy;
- Master/project launchers;
- tmux and i3 integration;
- state-registry consistency;
- no-secret logging;
- Artix-specific compatibility.

### 13.5 PortusOS execution-routing direction

The authoritative `docs/TASKS.md` contract routes work through the **least heavy Codex/Portus surface that preserves correctness and responsiveness**. It must not automatically turn every user request into a Portus task, a subagent, or a visible Codex terminal.

The current research direction is:

| Work shape | Preferred surface | Why |
| --- | --- | --- |
| Immediate, small, tightly coupled action | Master Portus directly | Lowest latency; no delegation overhead. |
| Bounded sidecar that supports the current Master turn | Codex subagent | Keeps detailed work out of Master context and can run in parallel. |
| Independent multi-step work that should continue while Master handles new requests | Separate headless Codex root session, normally through `codex exec`/resume/fork | Independent history and lifecycle without requiring a visible terminal. |
| Long-running iterative objective inside one Codex thread | Codex thread plus goal where useful | Codex tracks objective/budget/status inside its own thread. |
| Work needing live human observation, conversation, manual terminal interaction, or recovery | Interactive Codex in tmux/terminal | Visibility is useful here; not required merely because the task is complex. |
| Reusable procedural behavior | Codex skill | Native reusable workflow format. |
| Reusable captured machine/application arrangement | Portus structured workflow/scene record, optionally invoked by a skill | Requires durable OS/provider references beyond prose procedure. |

Examples:

```text
"Open Gmail"
  -> Master Portus directly

"Open my daily setup"
  -> execute the saved procedure/setup through a fast delegated or headless worker
  -> Master Portus remains focused on the user and orchestration

"Refactor this repository and verify the migration"
  -> separate project/headless Codex session or Portus-managed task when lifecycle/artifact/recovery semantics justify it

"Work with me interactively on this difficult debugging session"
  -> visible project Codex/tmux session
```

`docs/TASKS.md` now locks the shape-based routing rules, task-promotion boundary, canonical task state machine, and notification truth boundary. Exact resource-admission values, CLI schemas, and implementation mechanics remain dependent work rather than Codex responsibilities.

### 13.6 Master-context economy is an explicit design goal

Master Portus is the manager of the device. Its ability to perform an operation directly does not imply that it should perform all detailed steps itself.

The orchestration policy should optimize for:

1. **responsiveness** — the user can continue interacting with Master while independent work proceeds;
2. **context economy** — Master context is used primarily for intent, orchestration, decisions, and synthesis rather than long low-level command sequences;
3. **specialization** — bounded work can use a faster or more appropriate model/agent role;
4. **durability** — work that needs independent history or lifecycle receives a root session/Portus task rather than being hidden in the Master transcript;
5. **non-duplication** — Codex-native session, queue, fork, subagent, goal, compaction, and skill primitives are used before PortusOS invents equivalents.

This is a routing policy, not an instruction to delegate everything. Direct Master execution remains correct for short actions and immediate critical-path work.

### 13.7 Codex goals are thread objectives, not Portus task IDs

Codex 0.149 exposes a persisted single goal per materialized thread with objective, status, token-budget accounting, and usage/time information. Goal state can be active or reflect conditions such as blocked, budget-limited, or usage-limited.

Goals are useful for a Codex thread that should iteratively pursue an objective such as improving a benchmark or completing an extended maintenance goal. They must not replace Portus task identity because they do not encode the complete OS-level requester, process, provider, artifact, approval, and recovery relationships PortusOS may require.

The relationship should be:

```text
Portus task (when needed)
  -> may own/associate a Codex root thread
       -> Codex thread may have a goal
```

A simple Codex-only thread can also use a goal with no Portus task when no Portus-specific durable lifecycle is needed.

## 14. Search, network, and external context

Codex can use web search and MCP-provided context, while shell commands have a separate network boundary.

PortusOS must distinguish:

- model web-search capability;
- shell-command network access;
- MCP server network access;
- browser/computer-use surfaces;
- local/private network access.

Allowing one does not automatically make the others safe or permitted.

First ISO policy:

- shell network is not globally unrestricted;
- local/private network targets are denied unless deliberately allowed;
- web search may be available to the model under user/workspace policy, but sensitive project work should use explicit sourcing rules;
- MCP servers require individual configuration and approval policy;
- browser/computer-use features are not required to declare the CLI-based distro complete;
- experimental network-proxy enforcement must not be release-critical until validated on Artix.

## 15. OKF, memories, Chronicle, and persistent context

PortusOS has adopted Open Knowledge Format v0.2 as its curated durable knowledge format. `OKF.md` is the authoritative policy for bundle structure, the Portus OKF Profile, trust, lifecycle, freshness, retrieval, validation, security, and the boundary between knowledge and other persistence layers.

The persistent-context architecture is deliberately separated:

- `AGENTS.md` provides mandatory operating instructions and routes Codex to relevant knowledge;
- skills provide reusable workflows and executable procedures;
- normal documentation provides coherent specifications, plans, and reports;
- OKF bundles provide explicit, source-backed, reviewable durable knowledge;
- Codex sessions provide conversational and task continuity;
- Git provides project history and review;
- the Portus state registry provides live operational metadata;
- tmux provides live terminal continuity.

OKF does not replace any of those layers, and it must not become a vector database, session store, runtime registry, secrets store, access-control mechanism, execution runtime, or boot dependency.

### 15.1 Context compaction is a Codex responsibility

Codex 0.149 has first-class conversation compaction. Configuration exposes the model context window, an automatic compaction token threshold, threshold scope, and an optional compact prompt. Manual compaction is also available, and app-server exposes `thread/compact/start` for a materialized thread.

The hooks contract includes both `PreCompact` and `PostCompact`, alongside `SubagentStart` and `SubagentStop`. These events give PortusOS an integration point for non-secret observability such as recording that a Master/session compacted, updating health metadata, or taking a lightweight Portus-side checkpoint of references that are already Portus-owned.

PortusOS must **not** implement a competing conversation compactor or treat compaction as data loss that requires creating a new Master session. The desired long-lived Master model is:

```text
Master Portus thread
  -> conversation grows
  -> Codex compacts context as needed
  -> Master continues with summarized conversational context
  -> durable Portus facts remain in their authoritative stores
```

Compaction creates an important persistence boundary:

- conversational reasoning/history may be summarized by Codex;
- task IDs, task lifecycle, provider relationships, policy state, saved workflows/scenes, system-index truth, artifacts, and durable knowledge must remain outside the transient conversational context when they matter beyond the conversation;
- `AGENTS.md`, skills, documentation, OKF, Portus state, and provider state remain the correct durable mechanisms for their respective domains.

`PreCompact`/`PostCompact` hooks are useful integration signals but must not become a correctness dependency. A missed or delayed hook must not corrupt task state or make recovery impossible. Portus task/workflow persistence must be correct even if no compaction hook runs.

The baseline should prefer Codex's built-in compaction behavior. A custom `compact_prompt` should be introduced only if validation proves that Portus-specific durable instructions are not preserved adequately through the normal Codex instruction layers; replacing the built-in compaction prompt merely to customize style is not justified.

Local Codex memories are off by default and stored under `CODEX_HOME` when enabled. PortusOS must keep them disabled in the baseline:

```toml
[features]
memories = false
```

OKF is preferred because it is explicit, inspectable, project-scoped, source-backed, Git-reviewable, and portable. A future proposal to enable native Codex memories must demonstrate a concrete benefit not adequately met by OKF, `AGENTS.md`, skills, project documentation, session resume, Git, and the state registry. It must also define generation, use, retention, project isolation, external-context exclusion, inspection, deletion, secrets handling, and threat modelling. Native memories, if ever enabled, would complement OKF rather than replace it.

Chronicle is macOS-only as of this verification and uses screen capture to generate memories. It is not part of PortusOS and must not appear in the Linux implementation plan.

## 16. SDK, app-server, remote control, and cloud boundaries

### 16.1 First ISO control surface

The first ISO control surface remains:

- Codex CLI;
- tmux;
- Master Portus and project launchers;
- native session management;
- `codex exec` for automation;
- Portus OS CLI (`portus-os`) for deterministic OS workflows;
- SSH for remote shell continuation.

The ISO does not require the Codex SDK, app-server, remote control, or Codex cloud commands to satisfy the product promise.

### 16.2 SDK

The Codex SDK is a future option for a custom Portus dashboard, local service, or structured orchestrator. It must not be added merely to wrap CLI operations that already have stable command and JSON interfaces.

Adopt it only when a concrete application requires programmatic thread lifecycle, event handling, or deeper embedding than `codex exec --json` provides.

### 16.3 App-server

Codex app-server supports stdio, Unix-socket, and experimental WebSocket transports.

Policy:

- do not expose app-server as part of the first ISO;
- prefer stdio or a Unix socket for a future local integration;
- WebSocket transport is experimental and unsupported;
- any future WebSocket listener must bind to loopback by default;
- non-loopback exposure requires explicit supported authentication and firewall configuration;
- never expose an unauthenticated non-loopback listener;
- SSH port forwarding is the preferred remote path for a loopback listener.

### 16.4 Remote control and cloud

`codex remote-control` and `codex cloud` are currently experimental command surfaces. They must not be dependencies of the first ISO startup, continuity, project management, or recovery model.

They may be evaluated later behind an opt-in feature gate after a separate security and reliability review.

## 17. Feature-maturity policy

OpenAI labels Codex capabilities by maturity. PortusOS must map that into release policy:

- **Stable:** may be used in the required production path after Artix validation.
- **Beta:** may be used when it is strategically necessary, pinned to a validated Codex release, covered by tests, and backed by a documented fallback or migration path.
- **Experimental:** may be opt-in for evaluation; must not be required for installation, startup, recovery, authentication, permissions, or project continuity.
- **Under development:** excluded from the PortusOS implementation contract.

Current implications:

- stable-release path: interactive CLI, persisted resume/fork/archive/delete, `codex exec` including headless fork, `codex queue`, the `codex agents` dashboard, review, doctor, MCP management, plugins, skills, hooks, compaction, and multi-agent support;
- beta: permission profiles and device-code authentication require explicit compatibility testing;
- app-server thread goals exist in the stable tag and may be evaluated as Codex-thread objective state, but they are not a substitute for Portus task identity;
- the stable CLI `codex queue` may be used through its documented command surface, while the lower-level app-server queue mutation APIs are still marked experimental and must not become a first-ISO dependency;
- experimental: app-server WebSocket, remote control, Codex cloud command path, execpolicy command output, experimental queue APIs, and experimental network controls cannot be critical dependencies;
- memories remain disabled by owner policy even where supported.

Every Codex version update must re-check the maturity and schema of features PortusOS depends on.

## 18. Required PortusOS-owned Codex files

The aspirational installed-system layout must add these paths:

```text
/etc/codex/
  config.toml
  requirements.toml
  rules/
    portus-system.rules
  skills/
    <machine-wide PortusOS skills>/
  hooks/
    <managed hook programs>/

/etc/portus/
  codex-version
  codex-install-metadata.json
  schemas/
    task-result.schema.json

/usr/local/bin/
  portus-master
  portus-project
  portus-codex-update
  portus-os

$HOME/.codex/
  config.toml
  AGENTS.md
  agents/
  rules/
  hooks.json or hook config, when user-owned
  sessions and other Codex-managed state

/workspace/<user>/master/
  AGENTS.md
  .agents/skills/
  .codex/
    config.toml, only where a trusted project-level override is needed
    agents/
    rules/
    hooks.json or hook config

/workspace/<user>/projects/<project>/
  AGENTS.md
  .agents/skills/
  .codex/
    config.toml
    agents/
    rules/
    hooks.json or hook config
```

PortusOS must not pre-create empty or fake files for capabilities that have not been implemented. Every shipped file needs a clear owner, source template, validation test, and upgrade behavior.

## 19. Required implementation and validation gates

PortusOS is not Codex-ready until all applicable checks pass on a fresh installed VMware VM.

### Installation and version

- [ ] Codex installs through the pinned standalone release path.
- [ ] Release digest verification succeeds.
- [ ] Installed version equals the manifest pin.
- [ ] Node/npm absence does not break Codex itself.
- [ ] Automatic update checks are disabled.
- [ ] Explicit update workflow is documented and tested.

### Master Linux-user boundary and delegated sandboxes

- [ ] Artix package providing `bwrap` is installed for delegated/project-agent sandboxing.
- [ ] Unprivileged user namespaces work for delegated agents.
- [ ] The Master Portus Codex session runs as the intended non-root Linux UID.
- [ ] The Master session can operate ordinary files/applications available to that UID without a workspace-only Codex sandbox.
- [ ] The Master session cannot read another Linux user's private home/workspace by default.
- [ ] A project-agent profile can edit its assigned workspace/worktree.
- [ ] The project agent cannot edit outside its assigned scope without separately granted authority.
- [ ] Project-agent secret-path protections behave as documented.
- [ ] Project `.codex` and `.agents` trust/protection behavior is verified.
- [ ] The Master execution mode does not implicitly grant root authority.

### Configuration and trust

- [ ] `/etc/codex/config.toml` loads.
- [ ] `/etc/codex/requirements.toml` enforces the intended profile allowlist.
- [ ] No active layer contains legacy `sandbox_mode` when permission profiles are enabled.
- [ ] User config can change allowed preferences but cannot weaken requirements.
- [ ] Trusted project config loads.
- [ ] Untrusted project config, hooks, and rules do not load.
- [ ] Config precedence is covered by tests.

### Authentication

- [ ] ChatGPT login works.
- [ ] The selected Codex credential path works after a fresh local graphical login.
- [ ] The selected Codex credential path works after SSH attach to the existing tmux session.
- [ ] If the credential is claimed to be protected from Master Portus, negative tests prove the Master UID cannot export the reusable value through ordinary filesystem/keyring/provider access.
- [ ] Device-code login is tested as a beta fallback.
- [ ] No token appears in project files, Portus logs, shell history, diagnostics, or ISO contents.
- [ ] Logout removes the stored Codex credentials as expected.

### Sessions and continuity

- [ ] New Master Portus session starts in the owning user's `/workspace/<user>/master`.
- [ ] `resume --last` resumes only the expected workspace session.
- [ ] Resume works after terminal closure.
- [ ] Resume works after reboot.
- [ ] Session ID/name is recorded in the Portus registry without copying transcripts.
- [ ] Fork, archive, unarchive, and delete behavior are understood by the registry.
- [ ] Broken registry data fails open to the Codex picker or a shell.

### Extensions and instructions

- [ ] Global and nested `AGENTS.md` precedence is verified.
- [ ] Instruction-size behavior is verified.
- [ ] Repository, user, and admin skills load from the documented paths.
- [ ] Rules have positive/negative tests.
- [ ] Hooks run with bounded timeouts and no secrets.
- [ ] Hook failure does not break OS login or shell recovery.
- [ ] MCP is absent or disabled unless explicitly configured.
- [ ] Configured MCP servers expose only reviewed tools.

### Multi-agent

- [ ] Subagents start under the expected inherited or overridden permissions.
- [ ] The configured concurrency cap is respected.
- [ ] A conservative subagent concurrency cap is measured separately on the 4 GiB minimum VM and the 8 GiB reference VM; the Master session remains responsive under the selected cap.
- [ ] Parallel writing is blocked or isolated through distinct worktrees.
- [ ] Custom read-only agents cannot modify the workspace.

### Compaction, delegation, and routing

- [ ] Automatic compaction occurs and the Master session remains usable afterward.
- [ ] Manual compaction works on a controlled session.
- [ ] `PreCompact` and `PostCompact` hooks are observable when configured, without making task correctness depend on them.
- [ ] Durable Portus task/workflow/index/policy state remains correct across Master-session compaction.
- [ ] Master `AGENTS.md` explicitly authorizes delegation according to the Portus task-routing policy rather than relying on ad-hoc prompts.
- [ ] A bounded sidecar task can run as a subagent without polluting the Master transcript with its detailed execution trace.
- [ ] A separate headless Codex root session can perform independent multi-step work while Master Portus remains available for unrelated user requests.
- [ ] The selected fast subagent role resolves through the active model catalog and has a documented fallback when the preferred model is unavailable.
- [ ] `codex agents` can inspect loaded root-session/task activity in the validated CLI/TUI environment.
- [ ] `codex queue` can send a controlled text follow-up to a known session by unambiguous ID/name.
- [ ] A saved procedural workflow can be created/updated as a Codex skill and re-discovered after reload.
- [ ] A captured machine setup that requires structured application/window/provider state is not reduced to an unstructured skill-only representation.

### Automation and diagnostics

- [ ] `codex exec` defaults to a safe non-interactive permission path.
- [ ] JSONL output is captured and parseable.
- [ ] JSON Schema-constrained output validates.
- [ ] Exit failures propagate to Portus scripts.
- [ ] `codex exec resume` works for a controlled multi-stage task.
- [ ] `codex exec fork` creates an independent headless branch with the intended working directory and permission profile.
- [ ] Resume and fork restore the active permission profile rather than silently falling back to current defaults.
- [ ] `codex doctor` runs successfully.
- [ ] `portus-os doctor` incorporates Codex diagnostics and adds Portus-specific checks.

## 20. Required corrections to the active documentation

The next documentation-alignment pass must apply at least these changes.

### `docs/PROJECT.md`

- replace npm-first Codex installation with pinned standalone installation;
- remove Node/npm as Codex-only dependencies;
- redefine continuity as tmux plus persisted Codex sessions;
- define Master Portus as a broad non-root Linux user whose Codex session is not workspace-sandboxed, while delegated project/task agents remain sandboxable;
- add system config, requirements, trust, permission profiles, rules, hooks, MCP, plugins, subagents, and `codex exec` to the product surfaces;
- add Artix Codex compatibility as a hard validation gate;
- add `codex doctor` to readiness;
- keep memories, app-server, remote control, and cloud outside the first ISO baseline;
- update active-document authority to include this file until propagation is complete.

### Implementation requirements

Codex implementation and whole-ISO integration must continue to preserve the following requirements:

- use the standalone pinned installer in package and installation phases;
- add build metadata and digest/version verification;
- add Bubblewrap and user-namespace validation for delegated agents;
- add Master Linux-UID and cross-user isolation validation;
- add system config and requirements implementation;
- add non-workspace-sandboxed Master execution tests plus project-agent permission-profile and trust tests;
- add session resume/fork/archive/delete integration;
- add AGENTS, skills, rules, hooks, MCP, and subagent phases/tests;
- add worktree isolation for concurrent writers;
- add `codex exec` JSON/schema automation tests;
- add `codex doctor` integration;
- remove obsolete npm dependency ordering;
- explicitly gate beta/experimental features.

### `docs/STRUCTURE.md`

- add `/etc/codex/config.toml`, `/etc/codex/requirements.toml`, rules, hooks, and admin skills;
- add Codex installation/version metadata to `/etc/portus`;
- add Master/project Codex launchers and update workflow;
- add project `.codex/agents`, rules, hooks, and config ownership;
- add system/user/project config precedence;
- add Codex session metadata to the Portus state model;
- add non-interactive result schemas/logs;
- add tests for Master Linux-user authority, cross-user isolation, delegated-agent sandboxing, policy, trust, session continuity, extensions, and multi-agent isolation.

## 21. Official source register

Latest stable-tag research used for the 2026-08-21 refresh:

- [Codex 0.149.0 release](https://github.com/openai/codex/releases/tag/rust-v0.149.0)
- [Codex 0.149.0 configuration source](https://github.com/openai/codex/blob/rust-v0.149.0/codex-rs/config/src/config_toml.rs)
- [Codex 0.149.0 hook configuration source](https://github.com/openai/codex/blob/rust-v0.149.0/codex-rs/config/src/hook_config.rs)
- [Codex 0.149.0 multi-agent tool specification](https://github.com/openai/codex/blob/rust-v0.149.0/codex-rs/core/src/tools/handlers/multi_agents_spec.rs)
- [Codex 0.149.0 app-server protocol documentation](https://github.com/openai/codex/blob/rust-v0.149.0/codex-rs/app-server/README.md)
- [0.149 agents dashboard implementation](https://github.com/openai/codex/pull/39094)
- [0.149 session queue CLI implementation](https://github.com/openai/codex/pull/39092)

1. [Codex CLI overview and standalone installation](https://learn.chatgpt.com/docs/codex/cli)
2. [Official Codex repository](https://github.com/openai/codex)
3. [Official Codex releases](https://github.com/openai/codex/releases)
4. [Official standalone installer source](https://github.com/openai/codex/blob/main/scripts/install/install.sh)
5. [Codex installation and Linux requirements](https://github.com/openai/codex/blob/main/docs/install.md)
6. [Developer commands / CLI reference](https://learn.chatgpt.com/docs/developer-commands?surface=cli)
7. [Configuration basics and precedence](https://learn.chatgpt.com/docs/config-file/config-basic)
8. [Advanced configuration and state locations](https://learn.chatgpt.com/docs/config-file/config-advanced)
9. [Configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference)
10. [Environment variables](https://learn.chatgpt.com/docs/config-file/environment-variables)
11. [Managed configuration and requirements](https://learn.chatgpt.com/docs/enterprise/managed-configuration)
12. [Permission profiles](https://learn.chatgpt.com/docs/permissions)
13. [Sandboxing](https://learn.chatgpt.com/docs/sandboxing)
14. [Agent approvals and security](https://learn.chatgpt.com/docs/agent-approvals-security)
15. [Authentication](https://learn.chatgpt.com/docs/auth)
16. [AGENTS.md instruction discovery](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
17. [Rules](https://learn.chatgpt.com/docs/agent-configuration/rules)
18. [Hooks](https://learn.chatgpt.com/docs/hooks)
19. [Skills](https://learn.chatgpt.com/docs/build-skills)
20. [MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)
21. [Subagents and custom agents](https://learn.chatgpt.com/docs/agent-configuration/subagents)
22. [Non-interactive mode](https://learn.chatgpt.com/docs/non-interactive-mode)
23. [Git worktrees](https://learn.chatgpt.com/docs/environments/git-worktrees)
24. [Memories](https://learn.chatgpt.com/docs/customization/memories)
25. [Chronicle](https://learn.chatgpt.com/docs/customization/chronicle)
26. [Codex SDK](https://learn.chatgpt.com/docs/codex-sdk)
27. [Codex app-server](https://learn.chatgpt.com/docs/app-server)
28. [Feature maturity](https://learn.chatgpt.com/docs/feature-maturity)

## 22. Final implementation position

The PortusOS Codex architecture is now:

```text
Pinned standalone Codex release
  + Artix/OpenRC compatibility validation
  + Linux user identity as the Master Portus isolation boundary
  + non-workspace-sandboxed Master Codex execution
  + Bubblewrap/permission-profile sandboxes for delegated project and inspection agents
  + /etc/codex/config.toml managed defaults
  + /etc/codex/requirements.toml delegated-agent and Codex-specific boundaries
  + trusted-project configuration
  + root-owned typed `portus-privd` for Portus unattended privileged mutations, with native root/sudo/polkit retained for human administration/recovery
  + protected credential behavior governed by docs/SECRETS.md
  + long-lived Master Portus Codex thread focused on intent/orchestration/synthesis
  + Codex-native compaction for long-session context management without treating conversation as durable OS state
  + direct Master execution for immediate tightly coupled actions
  + Codex subagents for bounded sidecar delegation and parallel work
  + separate headless Codex root sessions for independent multi-step/background work
  + codex exec resume/fork and codex queue for native headless/session continuation and steering
  + codex agents for native loaded-session/task visibility
  + optional Codex thread goals for objectives inside a Codex thread, not as Portus task IDs
  + tmux only where live terminal visibility/interaction/recovery is useful
  + Codex sessions for persisted conversation continuity
  + AGENTS.md for durable instructions and explicit Master delegation authorization
  + skills for reusable procedural workflows, including skill creation/update
  + Portus structured workflow/scene state only where saved machine/provider relationships exceed what a skill should represent
  + plugins for distribution
  + rules and hooks for deterministic policy/automation
  + MCP for reviewed optional integrations
  + model-catalog-resolved subagent roles with conservative measured concurrency and no hardcoded fast-model slug
  + Git worktrees for concurrent writers
  + Portus task orchestration layered over Codex-native execution primitives rather than replacing them
  + codex exec for machine automation and independent headless root work
  + codex doctor integrated into portus-os doctor
```

This is the baseline that permanent Codex implementation, build, validation, and operational documentation must follow.