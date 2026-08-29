# PortusOS Open Knowledge Format Policy

**Status:** Authoritative hard-cutover document
**Adopted:** 2026-07-25
**Target specification:** Open Knowledge Format (OKF) v0.2
**Scope:** How PortusOS creates, stores, validates, retrieves, trusts, updates, and distributes durable agent-readable knowledge

## 1. Authority and decision

PortusOS adopts Open Knowledge Format v0.2 as its standard representation for curated, durable, repository-owned knowledge.

This document is authoritative for every PortusOS decision concerning OKF, durable project knowledge, and the boundary between curated knowledge and other persistence mechanisms.

The adoption decision is:

> OKF is the PortusOS durable knowledge format. It complements Codex instructions, skills, sessions, Git, project documentation, and the Portus runtime state registry. It replaces any future bespoke PortusOS memory-file or project-wiki schema. It does not replace those other systems.

PortusOS must not implement a competing custom durable-memory format alongside OKF unless a later owner decision explicitly supersedes this document.

Codex native memories remain disabled. PortusOS will use explicit, inspectable, source-backed knowledge rather than silently generated cross-session recollections as its default persistence model.

## 2. Why PortusOS is adopting OKF

PortusOS needs durable context that can be:

- read and edited by humans;
- read and drafted by agents;
- stored beside the projects it describes;
- reviewed through Git diffs;
- moved between machines and agent implementations;
- searched without a proprietary service;
- attributed to its sources;
- filtered by trust, freshness, and lifecycle;
- progressively disclosed rather than injected wholesale into every prompt.

OKF v0.2 provides a suitable interoperability layer using Markdown concept files, YAML frontmatter, normal Markdown links, optional hierarchy indexes, and optional update logs. It adds explicit provenance, generation, verification, lifecycle, freshness, and attested-computation vocabulary while remaining a file format rather than a hosted platform.

This fits PortusOS because the operating system is intended to remain local-first, Git-friendly, recoverable through ordinary Linux tools, and independent of a mandatory cloud knowledge service.

## 3. What OKF is and is not

### 3.1 What OKF is in PortusOS

OKF is the canonical file format for curated knowledge concepts such as:

- project profiles;
- architectural decisions;
- system constraints;
- operational runbooks;
- known issues;
- dependencies;
- external references;
- system capabilities;
- project checkpoints;
- deprecation notices;
- stable environment facts;
- source-backed summaries that agents need repeatedly.

Each concept is a Markdown file with YAML frontmatter. The path identifies the concept. Normal Markdown links connect concepts and source documents. Git supplies history, attribution, review, and recovery.

### 3.2 What OKF is not

OKF is not:

- a vector database;
- a semantic-search service;
- a conversation transcript store;
- a Codex session replacement;
- a tmux replacement;
- a project runtime-state database;
- a task queue;
- a secrets store;
- an access-control system;
- an instruction-precedence mechanism;
- an automatic context-injection engine;
- an execution runtime;
- an MCP server by itself;
- a replacement for long-form documentation;
- a replacement for Git.

PortusOS tooling may index OKF content for faster local retrieval, but every index or cache must be disposable and reproducible from the Markdown bundle. The OKF files remain the source of truth.

## 4. Relationship to the rest of the PortusOS context system

PortusOS uses separate persistence layers for separate jobs.

| Layer | Responsibility | OKF relationship |
| --- | --- | --- |
| Codex system and developer policy | Non-negotiable agent behaviour and security boundaries | Always higher authority than OKF |
| Current user instruction | The active task and explicit owner direction | Always higher authority than OKF |
| `AGENTS.md` | Automatically discovered operating instructions, repository guidance, and knowledge-routing rules | Directs Codex to relevant OKF bundles; is not replaced by OKF |
| Skills | Reusable procedures, scripts, references, and workflow instructions | May read or update OKF; are not represented as OKF merely because they contain knowledge |
| Long-form project documentation | Coherent specifications, plans, reports, and implementation documentation | Remains in normal documents; OKF concepts may summarise and link to it |
| OKF bundles | Curated durable concepts, provenance, trust, freshness, and relationships | The standard PortusOS durable knowledge layer |
| Codex sessions | Conversation history, plans, approvals, and task continuation | Complementary; sessions are not converted into OKF automatically |
| Git | Authoritative project content history, diffs, branches, and recovery | Stores and reviews project OKF bundles |
| Portus state registry | Live project, workspace, tmux, Codex-session, and operating metadata | Separate transactional state; must not be stored as OKF concepts |
| tmux | Live terminal continuity | Unrelated to durable knowledge storage |
| Codex native memories | Automatically generated cross-session recollection | Disabled in the PortusOS baseline |

### 4.1 `AGENTS.md` remains mandatory

Codex automatically discovers `AGENTS.md`; it does not automatically discover every OKF concept.

Every PortusOS-managed workspace that has an OKF bundle must use `AGENTS.md` to state:

- where the relevant bundle is located;
- when Codex must consult it;
- which trust and freshness rules apply;
- which knowledge types are authoritative;
- how drafts and stale concepts must be handled;
- which skill or deterministic command manages the bundle.

`AGENTS.md` must stay concise. It must route Codex to the knowledge rather than duplicate the knowledge.

### 4.2 Skills remain workflow containers

A skill explains how to perform work. An OKF concept records durable knowledge about the system or project.

Examples:

- A deployment procedure and its scripts belong in a skill.
- The architectural decision explaining the deployment model belongs in OKF.
- A deterministic project-discovery workflow belongs in a skill or Portus OS CLI (`portus-os`).
- The resulting stable project profile may be written as an OKF concept.

PortusOS must not convert executable workflows into OKF merely to place all context in one format.

### 4.3 Documentation remains the home for coherent documents

A complete project definition, implementation plan, architecture report, or release guide should remain a normal document when its value depends on reading it as a coherent whole.

OKF concepts should capture the durable atoms that agents repeatedly need and link to the full documents as sources.

PortusOS must not mechanically split every document into many tiny concepts. Concepts should be independently useful, stable enough to maintain, and meaningfully retrievable.

## 5. Version and maturity policy

PortusOS targets OKF v0.2.

Every PortusOS bundle root must declare:

```yaml
---
okf_version: "0.2"
---
```

This frontmatter is permitted only in the bundle-root `index.md`, consistent with the OKF specification.

PortusOS tooling must:

- parse v0.2 natively;
- preserve unknown frontmatter keys during round trips;
- tolerate unknown concept types;
- provide best-effort read-only access to newer minor versions;
- refuse automatic write migration to an unsupported version;
- require an explicit migration operation before rewriting a bundle for a new major version;
- recognise legacy v0.1 `timestamp` and `# Citations` only for import compatibility;
- write only v0.2 `generated.at` and `sources` in new or migrated concepts.

OKF is young and may change. PortusOS must isolate parsing, validation, indexing, and migration behind Portus-owned deterministic tooling rather than scattering format assumptions across launchers, skills, and scripts.

Basic Codex operation, login, shell recovery, and OS boot must never depend on successful OKF parsing.

## 6. Portus OKF Profile

Base OKF conformance is intentionally permissive: only a parseable frontmatter block and non-empty `type` are universally required for concept documents. That is not strict enough for an authoritative PortusOS knowledge base.

PortusOS therefore defines the **Portus OKF Profile**, a stricter producer and validation contract layered on top of OKF v0.2.

### 6.1 Required fields for every Portus concept

Every non-reserved concept document must contain:

```yaml
---
type: Decision
title: Use OpenRC instead of systemd
description: PortusOS uses OpenRC and rejects systemd as an implementation dependency.
tags: [portus-os, architecture, init]
status: stable
generated:
  by: human:owner
  at: 2026-07-25T10:00:00-04:00
verified:
  - by: human:owner
    at: 2026-07-25T10:00:00-04:00
portus:
  authority: authoritative
  scope: project
---
```

Required fields:

- `type`;
- `title`;
- `description`;
- `tags`;
- `status`;
- `generated.by`;
- `generated.at`;
- `portus.authority`;
- `portus.scope`.

Requirements:

- `title` must be specific and human-readable.
- `description` must be one concise sentence that allows relevance filtering without reading the body.
- `tags` must be a YAML list, not a comma-separated string.
- `status` must be `draft`, `stable`, or `deprecated`.
- `generated.at` must be an ISO 8601 datetime with timezone.
- actor identifiers must follow the actor policy in section 9.
- the Markdown body must contain substantive content; frontmatter alone is insufficient under the Portus profile.

### 6.2 Portus extension fields

OKF permits custom keys. PortusOS uses a namespaced `portus` mapping to avoid colliding with future standard fields.

```yaml
portus:
  authority: authoritative
  scope: project
  owner: human:owner
  supersedes:
    - decisions/old-init-policy.md
```

Defined fields:

- `portus.authority`: `reference`, `operational`, or `authoritative`;
- `portus.scope`: `system`, `master`, or `project`;
- `portus.owner`: optional actor responsible for maintenance or approval;
- `portus.supersedes`: optional list of bundle-relative concept paths replaced by this concept.

Authority meanings:

- `reference`: informative context; it does not independently set policy.
- `operational`: an approved fact or runbook used in routine work.
- `authoritative`: an owner-approved decision, constraint, policy, or definition that controls project work within its scope.

`portus.authority` is a knowledge classification, not a Codex instruction override. An authoritative concept cannot override system policy, managed Codex requirements, security controls, current user instructions, or applicable `AGENTS.md` instructions.

### 6.3 Conditional required fields

#### Sources

`sources` is required when a concept:

- derives from another document;
- reports external facts;
- summarises research;
- records package, API, model, service, or product behaviour;
- is generated by an agent from discovered material;
- claims that another artifact supports a decision.

Each source must have:

- a stable `id`;
- `resource`;
- `title`.

Include `author`, `last_modified`, and other objective signals when known.

Specific claims in the body should use Markdown footnotes keyed to source IDs where practical.

A concept must not list a source it did not actually use.

#### Verification

`verified` is required before a generated concept may become `stable`.

Rules:

- `draft` concepts may be unverified.
- A machine-generated `stable` concept must have at least one verification event.
- A concept with `portus.authority: authoritative` must have at least one `human:` verification event.
- A machine-only verification may support `operational` knowledge when deterministic evidence exists, but it cannot create an owner decision.
- Verification must be repeated after a material content change; old verification does not silently transfer to changed content.

#### Freshness

`stale_after` is required for information that may change independently of the repository, including:

- Codex capabilities and configuration keys;
- package names and repository availability;
- service names;
- installation commands;
- software release information;
- APIs and provider behaviour;
- external pricing, quotas, limits, compatibility, and policy;
- current model or product availability;
- external URLs whose content drives implementation.

`stale_after` must be an absolute `YYYY-MM-DD` date. A concept is stale when the current date is on or after that date.

Stable owner decisions that remain true until explicitly superseded do not require artificial expiry dates.

### 6.4 Concept body guidance

Bodies must favour structured Markdown:

- headings;
- lists;
- tables;
- code blocks;
- explicit relationships;
- examples;
- consequences;
- validation evidence.

A concept should answer one coherent knowledge question. It must not be fragmented merely to minimise file size, and it must not combine unrelated decisions simply to reduce file count.

## 7. Standard Portus concept types

PortusOS defines these initial concept types:

| Type | Purpose |
| --- | --- |
| `Project Profile` | Stable identity, purpose, boundaries, owners, and key locations for a project |
| `Decision` | An adopted choice and its rationale and consequences |
| `Constraint` | A hard requirement, prohibition, compatibility boundary, or invariant |
| `Architecture` | A durable system structure, component relationship, or data/control flow |
| `Runbook` | Operational knowledge for a repeatable procedure; executable implementation may live in a skill or command |
| `Dependency` | An external or internal dependency and the conditions attached to it |
| `Known Issue` | A confirmed defect, limitation, workaround, and resolution status |
| `Checkpoint` | A durable project milestone, acceptance state, or handoff condition |
| `System Capability` | A verified capability exposed by PortusOS, Codex, or an installed component |
| `External Reference` | A curated source-backed summary of an external specification, product, or service |
| `Deprecation` | A retired concept, replacement path, and migration implications |

These values are descriptive conventions, not a closed registry. A project may add a type when it has a clear retrieval or maintenance purpose. New types must be documented in that bundle’s root `index.md` or linked profile documentation.

`Attested Computation` is part of OKF v0.2 but is not enabled as an executable PortusOS workflow in the first ISO. Section 17 defines the boundary.

## 8. Bundle locations and ownership

PortusOS uses three knowledge scopes.

### 8.1 System knowledge bundle

Target location:

```text
/etc/portus/knowledge/
```

Purpose:

- shipped PortusOS architecture and operating concepts;
- system constraints;
- supported recovery paths;
- stable command and layout references;
- distro-specific runbooks;
- installed capability descriptions.

Ownership:

- supplied and versioned by PortusOS;
- read-only to ordinary project agents;
- changed through PortusOS package/update mechanisms or explicit administrative work;
- never used for mutable runtime state.

### 8.2 Master Portus knowledge bundle

Target location:

```text
/workspace/<user>/master/knowledge/
```

Purpose:

- owner-curated cross-project knowledge;
- Master Portus operating decisions;
- durable project relationships;
- shared operational practices;
- knowledge that is not owned by a single project repository.

Ownership:

- managed in the Master Portus workspace;
- writable only under the Master Portus permission and review policy;
- not a copy of every project bundle;
- may link to project concepts without absorbing them.

The Master bundle also owns the **durable per-machine knowledge map** for the particular PortusOS installation. A recommended sub-scope is:

```text
/workspace/<user>/master/knowledge/machine/
```

This map records stable, repeatedly useful facts that Master Portus learns about the actual machine and that should not need rediscovery on every request, for example:

- important local filesystem roots, custom directories, mount roles, and deviations from the shipped PortusOS layout;
- installed or user-added capabilities whose presence or role is useful across tasks;
- stable application/provider conventions and local integration notes;
- device/display/workspace aliases or roles that the owner or Master has established;
- stable relationships between projects, applications, providers, or machine resources;
- machine-specific operational facts, quirks, or recovery notes worth reusing.

This is **not** a duplicate of live runtime state. Open applications, current windows, current PIDs, service status, browser tabs, and other rapidly changing observations belong to the system index or authoritative providers. The machine knowledge map may describe durable meaning and conventions around those resources, while live truth remains queryable separately.

The shipped system bundle under `/etc/portus/knowledge/` describes the expected PortusOS baseline. The Master machine map describes the actual installation where it differs or has learned additional stable facts. `AGENTS.md` must route Master Portus to both layers and require freshness/revalidation when a stored machine fact could have changed materially.

### 8.3 Project knowledge bundle

Target location:

```text
/workspace/<user>/projects/<project>/knowledge/
```

Repository-relative form:

```text
knowledge/
```

Purpose:

- repository-owned project profile;
- project decisions and constraints;
- architecture;
- runbooks;
- dependencies;
- known issues;
- checkpoints;
- relevant external references.

Ownership:

- versioned with the project in Git;
- governed by the project’s `AGENTS.md` and owner decisions;
- available to any compatible agent or human working in the repository;
- isolated from other project bundles by default.

### 8.4 Recommended bundle tree

```text
knowledge/
  index.md
  log.md
  project/
    index.md
    profile.md
  decisions/
    index.md
  constraints/
    index.md
  architecture/
    index.md
  runbooks/
    index.md
  dependencies/
    index.md
  known-issues/
    index.md
  checkpoints/
    index.md
  references/
    index.md
```

Directories should be created only when they contain or are about to contain useful concepts. PortusOS must not generate a large empty hierarchy merely to match this example.

## 9. Actor and trust policy

Actors identify who generated or verified content.

PortusOS uses these conventions:

- `human:owner` for the project owner when a more specific stable identifier is unnecessary;
- `human:<id>` for another identified human;
- `agent:codex/<version-or-profile>` for an interactive Codex agent;
- `agent:<name>/<version>` for another agent;
- `process:<name>` for deterministic automation;
- `import:<source>` for migrated material whose original author is not preserved in OKF metadata.

Actor IDs must not contain secrets or volatile session IDs.

### 9.1 Derived trust tiers

PortusOS consumers derive these trust tiers:

1. **Human-reviewed:** at least one valid `human:` verification.
2. **Machine-confirmed:** verification exists, but only by `process:` or agent actors.
3. **Unverified:** no valid verification event.

Trust tier is evidence, not access control. File permissions, Codex requirements, sudo, polkit, Git review, and owner approval remain the enforcement mechanisms.

### 9.2 Knowledge precedence

When concepts conflict, a consumer must not blindly select the most recently modified file.

Resolve conflicts using:

1. system, developer, managed Codex, security, and current user instructions;
2. scope relevance;
3. `portus.authority`;
4. human verification;
5. freshness;
6. source authority and direct evidence;
7. lifecycle status;
8. generation time only as a final tie-breaker.

A stale concept does not become correct merely because it was once human-reviewed.

A project-scoped concept must not redefine a system constraint. A Master bundle concept must not silently override a repository-owned project decision without an explicit owner-approved superseding concept.

## 10. Lifecycle policy

### 10.1 Drafting

Agents may create or update `draft` concepts when:

- asked by the user;
- directed by a relevant skill;
- converting completed research into durable knowledge;
- recording a newly discovered project fact that merits preservation.

Agents must not write every conversation, transient observation, or tentative idea into OKF.

Draft creation must be deliberate and bounded. The concept must state uncertainty and sources rather than presenting an inference as fact.

### 10.2 Promotion to stable

A concept may become `stable` only when:

- its required profile fields validate;
- its sources are present where required;
- broken links and duplicate identity problems are resolved;
- material claims have been checked;
- required verification is recorded;
- freshness is defined when applicable;
- any associated documentation or implementation has been aligned.

Agents may propose promotion. They may not self-promote machine-generated authoritative decisions without owner verification.

### 10.3 Material updates

A material update changes meaning, policy, implementation consequences, factual claims, sanctioned procedure, or dependency requirements.

A material update must:

- update `generated.by` and `generated.at`;
- invalidate or replace prior verification as appropriate;
- update sources and `stale_after` when applicable;
- update affected indexes;
- record a meaningful Git diff;
- add a `log.md` entry when the change matters at bundle or directory scope.

Formatting-only changes do not require new verification.

### 10.4 Deprecation and supersession

Deprecated concepts are retained when they provide historical, migration, or reproducibility value.

A deprecated concept must:

- use `status: deprecated`;
- state why it was deprecated;
- link to its replacement when one exists;
- not be surfaced as the default current answer;
- remain readable through explicit historical retrieval.

A replacement concept should use `portus.supersedes` where appropriate.

Deletion is reserved for accidental duplicates, secrets, legally required removal, or content with no continuing historical value. Git history is not treated as permission to leave secrets in the repository.

## 11. Retrieval and context-loading policy

OKF is valuable only when retrieval is selective and predictable.

### 11.1 Discovery order

A Codex or Portus knowledge consumer should:

1. read the applicable `AGENTS.md` routing instructions;
2. identify the relevant system, Master, and project bundle scopes;
3. inspect root and relevant directory `index.md` files;
4. filter candidates by type, title, description, tags, scope, status, trust, and freshness;
5. open only the concepts required for the task;
6. follow source or relationship links when the selected concept requires deeper evidence;
7. report stale, conflicting, unverified, or missing knowledge instead of silently improvising.

### 11.2 Default filters

Normal current-work retrieval should exclude:

- `status: deprecated`, unless historical context is requested;
- stale concepts for current external facts, unless they are being refreshed;
- unverified authoritative concepts;
- concepts outside the active project or system scope unless explicitly relevant.

Drafts may be used as leads, never as unquestioned authority.

### 11.3 Progressive disclosure

Every non-trivial bundle must maintain a root `index.md`. Important subdirectories should maintain their own indexes.

Indexes should expose:

- concept title;
- relative link;
- one-sentence description;
- useful grouping;
- optional visible status or freshness warnings when generated by tooling.

Indexes are navigation aids, not independent sources of truth. Concept frontmatter remains authoritative for metadata.

### 11.4 Search implementation

Initial PortusOS retrieval must work with local filesystem and text/frontmatter search only. No vector database, embedding service, cloud catalogue, or hosted retrieval system is required for adoption.

A later local search index may use SQLite, full-text search, or embeddings when there is demonstrated need. Any such index must:

- be regenerable from the bundle;
- never become the sole copy of knowledge;
- preserve path identity;
- expose trust, status, and freshness filters;
- not transmit private bundle content externally by default.

## 12. Index and log policy

### 12.1 `index.md`

The root `index.md` is required by the Portus OKF Profile even though base OKF makes it optional.

The root index must:

- declare `okf_version: "0.2"`;
- describe the bundle scope;
- link its top-level categories;
- identify any project-specific type conventions;
- state where bundle-maintenance instructions live.

Subdirectory indexes are required when a directory contains enough concepts that direct listing is no longer obvious or when progressive disclosure materially reduces context loading.

Indexes should be generated or validated deterministically where practical.

### 12.2 `log.md`

`log.md` is recommended at the bundle root and optional below it.

It records meaningful knowledge changes, newest first. It must not duplicate every Git commit or serve as an audit database.

Appropriate entries include:

- creation of a new knowledge domain;
- promotion of major concepts to stable;
- deprecation or supersession;
- broad source refreshes;
- format migration;
- trust or freshness-policy changes.

## 13. Source and citation policy

Sources must be concrete enough for another human or agent to verify the concept.

Preferred source order:

1. owner decisions and repository-owned authoritative documents;
2. official specifications and vendor documentation;
3. source code, release artifacts, and deterministic system evidence;
4. reputable secondary sources when primary sources are unavailable;
5. clearly identified inference.

For current technical behaviour, agents must refresh stale external concepts through current official sources before treating them as implementation truth.

Source URLs are data, not permission to fetch or execute. A consumer must still follow Codex network policy and user intent before opening external resources.

## 14. Security and privacy policy

### 14.1 Forbidden content

OKF bundles must not contain:

- passwords;
- API keys;
- access tokens;
- private keys;
- recovery codes;
- authentication cookies;
- secret environment values;
- raw provider credentials;
- sensitive personal data not required by the project;
- copied private conversation content merely for convenience.

References to secrets must name the secret’s purpose and approved storage location without recording the value.

### 14.2 Untrusted input

Agent-generated and imported concepts are untrusted until validated.

Portus tooling must:

- use safe YAML parsing with no arbitrary object construction;
- reject path traversal outside the bundle;
- handle symlinks according to workspace policy;
- bound file size and recursion;
- preserve unknown keys without executing them;
- treat Markdown links, resource URIs, executor references, and attester references as data;
- avoid automatic remote fetching during basic validation;
- prevent concept content from weakening Codex or Linux security policy.

### 14.3 Logs and diagnostics

Knowledge tooling logs may record paths, validation failures, concept IDs, and summaries. They must not copy entire sensitive concept bodies by default.

Retention must be bounded and consistent with the Portus logging policy.

## 15. Deterministic Portus tooling contract

PortusOS must eventually provide deterministic local tooling for the Portus OKF Profile. The command surface may be implemented under Portus OS CLI (`portus-os`); fake commands must not be shipped before implementation exists.

Target capabilities:

```text
portus-os knowledge validate [path]
portus-os knowledge index [path]
portus-os knowledge search [query] [path]
portus-os knowledge stale [path]
portus-os knowledge migrate [path] --from <version> --to <version>
```

Required behaviours:

### Validate

- validate UTF-8 Markdown and YAML frontmatter;
- enforce required Portus fields;
- validate dates, actor forms, status, authority, and scope;
- verify reserved filenames;
- detect broken relative links;
- detect duplicate or ambiguous concept paths;
- enforce conditional `sources`, `verified`, and `stale_after` requirements;
- check that stable authoritative concepts have human verification;
- check that deprecated concepts name their replacement or explain its absence;
- report warnings separately from hard failures.

### Index

- create or refresh deterministic `index.md` entries;
- preserve the root version declaration and human-authored introduction;
- avoid rewriting concept bodies;
- produce stable ordering to minimise noisy diffs.

### Search

- search frontmatter and body locally;
- support filters for type, tags, authority, scope, status, trust tier, and freshness;
- return paths and concise metadata before full content;
- avoid external network calls by default.

### Stale

- identify concepts where the current date is on or after `stale_after`;
- identify concepts approaching staleness when requested;
- never rewrite freshness dates merely to silence warnings;
- support a refresh workflow that updates evidence and verification.

### Migrate

- operate explicitly, never during normal reads;
- back up or require a clean Git state;
- preserve unknown keys;
- produce reviewable diffs;
- validate the result before success.

## 16. Agent write policy

Agents may read all knowledge allowed by their filesystem and Codex permission profile.

Agents may write knowledge only when:

- the current task calls for durable knowledge creation or maintenance;
- the applicable `AGENTS.md` permits it;
- the concept belongs within the writable workspace scope;
- the operation follows this profile;
- the agent does not present its own unreviewed decision as owner-approved.

Normal code implementation must not automatically generate knowledge churn.

When an agent discovers a potentially durable fact during unrelated work, it should either:

- update the relevant concept when the fact is material and the workflow permits it; or
- report the missing/stale knowledge for explicit follow-up.

It must not create low-value concepts for every command result, transient failure, speculative idea, or conversational aside.

## 17. Attested Computation boundary

OKF v0.2 defines `Attested Computation`, but the full runtime protocol, attester ABI, portability, and sandboxing remain deferred by the upstream specification.

First ISO policy:

- PortusOS may read and display Attested Computation concepts.
- PortusOS may use them as documentation of a sanctioned calculation.
- PortusOS must not automatically execute an `executor` or `attester` merely because an OKF file references it.
- Executable workflows must remain reviewed skills, scripts, or Portus OS CLI (`portus-os`) commands governed by Codex permissions and Linux security.
- A future implementation requires an explicit threat model, receipt schema, deterministic attester interface, sandbox policy, and failure policy.
- A failing or unavailable attestation must be surfaced, not hidden.

OKF metadata never grants execution permission.

## 18. Native Codex memories policy

Codex native memories remain disabled in the PortusOS baseline:

```toml
[features]
memories = false
```

OKF is preferred because it is explicit, inspectable, project-scoped, source-backed, reviewable, and portable.

This does not mean OKF duplicates every benefit of automatic memories. It deliberately rejects automatic recollection as the source of project truth.

A future proposal to enable Codex memories must prove a specific benefit that is not adequately met by:

- OKF bundles;
- `AGENTS.md`;
- skills;
- project documentation;
- Codex session resume;
- Git;
- the Portus state registry.

Any future native-memory proposal must separately define generation, use, project isolation, retention, inspection, deletion, exclusion of externally sourced chats, secrets handling, and threat modelling. Enabling native memories would complement OKF; it would not replace it.

## 19. Backup, distribution, and portability

Git is the preferred distribution and history mechanism for project and Master knowledge bundles.

PortusOS may also package bundles as archives or include a read-only system bundle in the ISO.

Rules:

- project bundles travel with their repositories;
- system knowledge is versioned with the PortusOS release;
- Master knowledge has an explicit backup path;
- generated indexes and caches must be reproducible;
- no Google Cloud account or Google service is required;
- no agent vendor is required to read the files;
- export must preserve Markdown, frontmatter, relative links, and path identity.

## 20. Adoption and migration plan

OKF adoption must be incremental and deliberate.

### Phase A — policy and profile

- adopt this document;
- keep native Codex memories disabled;
- add the OKF relationship to `CODEX_UPDATES.md`;
- propagate the architecture into permanent product, structure, knowledge, implementation, and validation documents.

### Phase B — minimum tooling

- implement local Portus profile validation;
- implement deterministic index generation or validation;
- implement local metadata-aware search;
- implement stale reporting;
- add tests and fixtures for valid, invalid, stale, deprecated, and unverified concepts.

### Phase C — initial bundles

- create the system bundle only after its concepts are derived from aligned PortusOS documentation;
- create the Master bundle when stable cross-project knowledge exists;
- add project bundles only where durable knowledge justifies them;
- add concise routing instructions to applicable `AGENTS.md` files.

### Phase D — operational integration

- integrate knowledge checks into `portus-os doctor` without making login depend on them;
- integrate relevant bundle search into Master Portus and project workflows;
- add stale-knowledge reporting;
- measure retrieval usefulness before considering semantic indexing.

### Migration rule

PortusOS must not automatically convert every existing Markdown file into OKF.

Existing long-form documents remain documents. Migration should identify durable concepts worth extracting, create source-linked OKF concepts, and leave the original artifact intact unless an explicit documentation restructuring decision says otherwise.

## 21. Validation gates

PortusOS must not claim OKF support complete until these checks pass:

### Format and profile

- [ ] Root `index.md` declares `okf_version: "0.2"`.
- [ ] Every concept has required Portus profile fields.
- [ ] Unknown fields survive a parse/write round trip.
- [ ] Legacy v0.1 fields are read but not emitted.
- [ ] Reserved filenames validate correctly.

### Trust and lifecycle

- [ ] Draft, stable, and deprecated behaviour is tested.
- [ ] Machine-generated authoritative concepts cannot become stable without human verification.
- [ ] Stale concepts are detected using absolute dates.
- [ ] Deprecated concepts are excluded from default current retrieval.
- [ ] Material edits invalidate or update verification correctly.

### Retrieval

- [ ] Index-first discovery works.
- [ ] Search filters by scope, type, tags, status, trust, and freshness.
- [ ] Retrieval does not require external network access.
- [ ] The consumer surfaces conflicts, staleness, and failed validation.
- [ ] Large bundles load progressively rather than wholesale.

### Security

- [ ] Safe YAML parsing is used.
- [ ] Path traversal is rejected.
- [ ] Unknown keys and resource links are never executed.
- [ ] Secret scanning covers knowledge paths.
- [ ] Project agents cannot write the system bundle.
- [ ] OKF failure cannot block TTY, shell, Codex login, or recovery.

### Integration

- [ ] `AGENTS.md` routes agents to the correct bundle.
- [ ] Skills and OKF retain separate responsibilities.
- [ ] State registry data is not duplicated into OKF.
- [ ] Codex session continuity remains independent.
- [ ] `portus-os doctor` can report knowledge health without reimplementing the entire validator.

## 22. Required documentation alignment

The next full PortusOS documentation-alignment pass must incorporate this policy.

### `docs/PROJECT.md`

It must:

- identify OKF as the curated durable knowledge layer;
- keep Codex native memories disabled;
- distinguish OKF from instructions, skills, sessions, Git, documentation, and runtime state;
- add system, Master, and project bundle surfaces;
- include local-first, no-cloud-required operation;
- state that boot and recovery do not depend on OKF.

### Implementation requirements

PortusOS implementation and validation must:

- add Portus OKF Profile validation tooling;
- add bundle, trust, lifecycle, stale, security, and retrieval tests;
- add initial bundle creation after authoritative docs are aligned;
- add `AGENTS.md` routing integration;
- add `portus-os doctor` knowledge-health checks;
- keep vector search, native memories, and automatic attested execution outside the first implementation unless separately approved.

### `docs/STRUCTURE.md`

It must add:

- `/etc/portus/knowledge/`;
- `/workspace/<user>/master/knowledge/`;
- repository-local `knowledge/` bundles;
- OKF tooling, schemas, tests, and fixtures;
- disposable local search-index/cache locations if implemented;
- explicit separation from `/var/lib/portus/state/`;
- ownership and write boundaries for each scope.

## 23. Official source register

This decision is based on the official OKF materials:

1. [Introducing the Open Knowledge Format — Google Cloud, 2026-06-12](https://cloud.google.com/blog/products/data-analytics/how-the-open-knowledge-format-can-improve-data-sharing/)
2. [Open Knowledge Format v0.2 tackles agentic trust — Google Cloud, 2026-07-24](https://cloud.google.com/blog/products/data-analytics/okf-v0-2-adds-trust-signals)
3. [Open Knowledge Format v0.2 specification](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md)
4. [Official OKF repository directory and reference materials](https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf)

## 24. Final implementation position

The PortusOS persistent-context architecture is:

```text
System/developer policy and current user instruction
  + AGENTS.md for mandatory routing and operating instructions
  + Skills for reusable workflows and executable procedures
  + Long-form documentation for coherent specifications and plans
  + OKF v0.2 bundles for curated durable knowledge
  + Git for history, review, and distribution
  + Codex sessions for conversational and task continuity
  + Portus state registry for live operational metadata
  + tmux for live terminal continuity
  + Codex native memories disabled
```

OKF is the durable knowledge layer—not a universal replacement for every form of context or state.
