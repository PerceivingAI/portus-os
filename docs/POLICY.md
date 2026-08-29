# PortusOS Identity, Authority, and Policy Architecture

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative security and authority architecture
**Scope:** Owner/root authority, Master Portus identity, Linux user isolation, installer permission bundles, post-install granular policy, privilege/elevation, project-agent isolation, multi-user behavior, runtime caller identity, and policy enforcement

## 1. Purpose and authority

This document is authoritative for who may do what on a PortusOS machine and which operating-system boundaries enforce that authority.

It defines:

- the relationship between the machine administrator and Master Portus;
- the Linux account model;
- the rule that Master Portus is a real non-root Linux user;
- the rule that Master Portus is not placed inside a general-purpose workspace sandbox;
- the boundary between Master Portus and project/task agents;
- installer-time permission bundles;
- post-install granular policy management;
- cross-user isolation;
- root-equivalent capabilities;
- sudo, polkit, privileged-helper, service, device, network, and filesystem authority;
- policy ownership and self-expansion prevention;
- local `portusd` caller identity and per-user state isolation;
- validation requirements for the security model.

`docs/POLICY_IMPLEMENTATION.md` is authoritative for the concrete first-ISO Linux policy/elevation mapping, installer bundle defaults, `portus-privd`, root-equivalent inventory and administrator mutation path. `docs/SECRETS.md` is authoritative for reusable credentials, authentication material, recovery material, and secret-backed operations; `docs/CREDENTIAL_CLASSES.md` locks class-specific first-ISO classification for Codex/browser/SSH-signing/password-manager/AUTH/backup cases. `docs/RUNTIME.md` is authoritative for `portusd` and local IPC. `docs/CONTROL_PLANE.md` remains authoritative for which capabilities PortusOS should expose at all. `CODEX_UPDATES.md` controls Codex-specific implementation details only where they conform to this document.

When an older active document describes Master Portus as a workspace-sandboxed or least-privilege Codex process, this document supersedes that assumption.

## 2. Core authority model

PortusOS separates administrator authority from agent authority.

The fundamental model is:

```text
root / machine administrator
        |
        | creates accounts and defines enforced authority
        v
Master Portus Linux user
        |
        | broad control of that user's computer environment
        | according to administrator-selected policy
        v
projects, applications, files, sessions, devices,
network capabilities and PortusOS services
        |
        +--> project/task agents may receive narrower delegated sandboxes
```

The administrator remains the source of authority.

Master Portus is powerful by design, but it is not the administrator merely because it can operate most of the machine.

## 3. Root and administrator authority

PortusOS must preserve a machine-administration path that is distinct from Master Portus.

Linux UID 0/root remains the ultimate local authority over the installed machine. The installer must establish an owner-controlled path to root administration and recovery. A convenient non-root administrator account or sudo path may also be provided, but it derives authority from root-controlled policy and does not replace UID 0 as the ultimate operating-system authority.

Master Portus must not be the root identity by default, and Master Portus credentials must remain distinct from the administrator/root authority boundary.

The administrator can:

- create and remove Master Portus users;
- grant, revoke, or modify permission bundles;
- change individual permissions after installation;
- modify Unix groups, ACLs, ownership and modes;
- modify sudo and polkit policy;
- change device and socket access;
- change network and firewall authority;
- modify PortusOS policy;
- manage protected credential infrastructure;
- inspect and repair all users and PortusOS state;
- deliberately grant root-equivalent authority when desired;
- revoke Master Portus access without depending on Master Portus itself.

Master Portus must not be able to convert its own current authority into broader enforced authority without an administrator-authorised path.

## 4. Master Portus identity

### 4.1 Real Linux user

Each Master Portus instance runs as a normal Linux user account with its own UID, primary group, home directory, login/session state, Codex state, application state, graphical session state, and user-owned files.

Master Portus is not a permanently running root process.

Master Portus is not a pseudo-user implemented only inside PortusOS metadata.

Linux user identity is a primary isolation and ownership boundary.

### 4.2 Not a general-purpose sandboxed agent

Master Portus is not intended to operate inside a general-purpose Codex/Bubblewrap workspace sandbox.

The PortusOS machine, VM, server, or other installed environment is the broad containment boundary for Master Portus. Inside that environment, the Master Portus user should be able to operate the resources granted to that Linux user normally.

This means Master Portus should not be restricted to a small workspace root merely because it is an agent.

It may use:

- its home directory;
- its Master workspace;
- its project workspaces;
- applications installed for or available to that user;
- its graphical session;
- its tmux/Codex sessions;
- its user-owned application configuration and ordinary user data;
- system capabilities granted by administrator policy.

A separate Codex filesystem sandbox around Master Portus would duplicate the intended Linux user boundary and would make PortusOS less capable without providing the correct ownership model.

### 4.3 Broad authority is normal

The normal PortusOS use case assumes that Master Portus receives broad authority over its own user environment and a useful set of system capabilities.

PortusOS is not designed around requiring approval for every ordinary action.

Within authority already granted by the administrator, Master Portus may act without repeated human confirmation unless policy explicitly marks a capability as prompt/approval-required.

Users may choose a more restrictive profile, a more permissive profile, or granular custom policy.

### 4.4 Not root

Broad authority does not make Master Portus root.

Master Portus cannot by default:

- read another user's private files;
- change ownership or permissions outside its authority;
- change enforced policy;
- alter root-owned security configuration;
- bypass protected secret boundaries;
- perform arbitrary root-only operations;
- grant itself membership in privileged groups;
- replace root-owned services or helpers;
- take over another Master Portus identity.

Capabilities that effectively provide arbitrary root takeover must be treated as root-equivalent authority, even if Linux technically grants them through a group, socket, helper, or sudo rule rather than UID 0.

## 5. Linux user isolation

### 5.1 Default user boundary

Different Linux users are private from one another by default.

For two Master Portus users:

```text
root
  |
  +-- user1 / Master Portus A
  |     +-- user1 home
  |     +-- user1 Codex state
  |     +-- user1 applications/configuration
  |     +-- user1 workspaces
  |     +-- user1 sessions
  |
  +-- user2 / Master Portus B
        +-- user2 home
        +-- user2 Codex state
        +-- user2 applications/configuration
        +-- user2 workspaces
        +-- user2 sessions
```

By default:

```text
user1 X--> user2 private state
user2 X--> user1 private state
root  ---> both
```

Cross-user access requires an explicit administrator decision using normal Linux or PortusOS policy mechanisms.

### 5.2 Multiple Master Portus users

Running several independent Master Portus users on one PortusOS machine is not the primary product use case, but the architecture must not make it impossible.

A second Master Portus instance should be representable by creating another Linux user and granting that user an appropriate policy profile.

No global singleton assumption may require all Master Portus state to belong to one UID.

### 5.3 User-scoped workspaces

Master and project workspaces must be scoped to the owning Linux user.

The canonical direction is:

```text
/workspace/<user>/master
/workspace/<user>/projects/<project>
```

The exact account-name normalization and migration rules are implementation details, but `/workspace/master` and `/workspace/projects` must not be treated as globally singleton writable roots shared by every Master Portus user.

Linux ownership and modes must enforce the user boundary independently of Portus metadata.

## 6. Permission bundles at installation

### 6.1 Installer UX

PortusOS installation must include a clear section for selecting the authority granted to the Master Portus user.

The installer should present understandable **permission bundles**, not hundreds of individual Linux permissions.

A permission bundle is an administrator-facing policy choice that may configure several underlying enforcement mechanisms.

It is not synonymous with a Unix group.

A bundle may map to:

- Unix supplementary groups;
- filesystem ownership or ACLs;
- sudo rules;
- polkit rules;
- privileged helpers;
- service/socket access;
- device permissions;
- firewall/network policy;
- PortusOS policy records;
- application/provider permissions.

### 6.2 Bundle domains

The initial installer should group authority into broad domains such as:

- **Files and Workspaces** — ordinary user files, Master/project workspace ownership and shared locations;
- **Applications and Desktop** — application launching, graphical session, display/input/audio and ordinary user application control;
- **Development** — compilers, package/tool execution, project processes, local development services and related developer capabilities;
- **Network and Internet** — outbound network use, local-network access and controlled network operations;
- **System Administration** — package management, selected service management, system configuration and privileged maintenance operations;
- **Devices and Hardware** — selected removable media, audio/video/input or other device classes;
- **Remote Access** — SSH-related operation and remote-control capabilities appropriate to the Master user;
- **External Data and Delivery** — transmission of artifacts/data to external destinations under policy.

The bundle labels above are canonical. `docs/POLICY_IMPLEMENTATION.md` now locks the first Broad-profile defaults and the rule that bundles materialize into explicit Linux/resource/Portus grants rather than remaining the runtime enforcement abstraction. Package-specific native group/device mappings may still be refined when the Artix package set is locked.

### 6.3 Recommended broad profile

The normal PortusOS profile should grant Master Portus broad practical control of its own user environment and the common capabilities required to operate the computer usefully.

The system must not equate "broad" with "root-equivalent".

Root-equivalent capabilities must be called out separately and must not be hidden inside an innocently named bundle.

### 6.4 Prompt is optional policy, not the default safety model

For any capability, the administrator may configure policy behavior such as:

```text
allow
prompt/approval
reject
```

where the implementation supports that distinction.

The default PortusOS experience should not require prompts for routine actions that the administrator already granted.

Approvals remain useful for selected destructive, external-delivery, privilege, or owner-designated boundaries.

## 7. Granular post-install control

After installation, the administrator must be able to inspect and change the exact effective authority of a Master Portus user.

Post-install management may go substantially more granular than the installation wizard.

Granular policy should be able to control, where technically meaningful:

- individual filesystem locations or shared trees;
- individual Unix groups;
- specific privileged commands/helpers;
- individual OpenRC services or service classes;
- package-management authority;
- firewall/network operations;
- outbound/inbound network classes;
- local/private network access;
- device classes;
- selected provider/application capabilities;
- artifact delivery destinations/classes;
- reboot/shutdown authority;
- policy prompt/deny/allow behavior;
- cross-user access;
- time- or task-bounded grants.

`docs/CLI.md` and `docs/POLICY_IMPLEMENTATION.md` lock the policy CLI surface: `portus-os policy effective` and typed `portus-os policy check ...` are normal inspection/preflight commands; root-only `portus-os policy admin ...` commands mutate administrator policy through the `portus-privd` admin boundary. The existence of those commands does not let Master Portus expand its own authority because administrative mutation requires authenticated UID 0 at the privileged boundary.

Native Linux administration remains available and must not be hidden behind PortusOS.

## 8. Policy ownership and precedence

### 8.1 Administrator-enforced policy

Administrator-enforced policy is authoritative and cannot be weakened by Master Portus user configuration, project configuration, Codex project configuration, hooks, skills, plugins, or provider configuration.

### 8.2 User policy

The Master Portus user may configure preferences and may further narrow its own behavior where supported.

A user-level setting must not silently broaden administrator-enforced authority.

### 8.3 Project/task policy

Project or task policy may further narrow authority for delegated work.

A project cannot grant access that the owning Master Portus user does not possess.

### 8.4 Provider policy

Specialised providers such as PortusBrowser may enforce stricter domain policy.

PortusOS must not overwrite or duplicate provider-domain rules merely because the OS user has broad authority.

For the first protected API provider, `docs/PROTECTED_API_PROVIDER.md` locks the Linux transport identities:

```text
service identity:  portus-api
use transport:     portus-api-users
admin operations:  peer UID 0
```

Membership in `portus-api-users` permits reaching the protected API use socket but does not grant credential-store read access, provider-administration authority, or unconditional use of every credential/operation. Effective protected API use still requires provider/caller policy and the provider binding. The `portus-api` service/store identity remains separate from the client group.

## 9. Project agents and delegated sandboxes

Master Portus and project/task agents intentionally use different security models.

A project agent may be sandboxed because its task is narrower than the whole machine.

A normal project agent may receive:

- one project/worktree as writable scope;
- task temporary storage;
- task-approved network access;
- selected tools/providers;
- no unrelated user data;
- no cross-project access by default;
- no policy-management authority;
- no protected credential retrieval.

Codex permission profiles, Bubblewrap, worktrees, namespaces, and related mechanisms remain appropriate for project/task agents, inspection agents, untrusted repositories, and bounded automation.

The existence of those mechanisms must not be used to re-sandbox the top-level Master Portus user.

## 10. Codex policy relationship

Codex remains one enforcement and execution component, but it is not the primary identity boundary for Master Portus.

For Master Portus:

- the Linux UID and administrator policy define the base authority;
- Codex must not impose a workspace-only filesystem sandbox that contradicts that authority model;
- system-managed Codex configuration may still disable unsafe feature modes, enforce project trust, configure hooks/rules, protect secret workflows, and govern delegated project agents;
- project-agent profiles remain sandboxed as appropriate;
- Codex-native project trust remains required for repository-supplied `.codex` behavior;
- Master Portus does not become root because Codex itself is unsandboxed inside the user's permitted Linux environment.

`CODEX_UPDATES.md` must be maintained so its examples do not reintroduce a `portus-master` workspace sandbox.

## 11. Privilege and elevation

### 11.1 Privileged operations

The first Portus-owned root-mutation boundary is the narrowly typed root-owned `portus-privd` service defined by `docs/POLICY_IMPLEMENTATION.md`. It authenticates local peers from Unix credentials, reads root-owned policy and exposes no generic root shell/exec/arbitrary-root-file primitive.

Native Linux administration through root, sudo, polkit or other reviewed mechanisms remains available for human repair/administration, but PortusOS unattended privileged operations use the typed `portus-privd` boundary rather than unrestricted passwordless sudo.

### 11.1.1 Privileged transport is not privilege by itself

The first privileged-operation client transport group is:

```text
portus-priv-users
```

Membership permits reaching `/run/portus/priv/use.sock`; it does not itself authorize any typed root operation. `portus-privd` still authenticates the peer UID and evaluates the action/resource policy.

The root-only policy administration socket remains separately protected and additionally requires peer UID 0.

### 11.2 Pre-granted privilege

The administrator may pre-authorize selected privileged operations for Master Portus.

This is desirable for unattended operation where repeated password entry would defeat the intended use case.

Pre-granted privilege should be scoped to the operation or capability when possible rather than implemented as unrestricted passwordless root.

### 11.3 Root-equivalent classification

PortusOS must identify and document permissions that effectively allow arbitrary root takeover.

Examples can include, depending on configuration:

- unrestricted passwordless sudo;
- unrestricted root shell execution through a helper/service;
- write access to arbitrary root-owned executables or service definitions;
- direct write access to raw system block devices sufficient to replace protected system content;
- privileged container/virtualization sockets that permit mounting or modifying the host filesystem;
- kernel/module control sufficient to bypass normal security.

The exact list is platform-dependent and must be validated.

A root-equivalent permission may be deliberately granted by the administrator, but PortusOS must label it as such rather than presenting the resulting Master Portus account as meaningfully non-root.

## 12. Cross-user and shared-resource policy

Shared project trees, shared artifact locations, team directories, devices, or services may be explicitly granted to more than one user.

Shared access must be deliberate and represented by normal Linux ownership/groups/ACLs and, where needed, Portus policy.

PortusOS must not infer cross-user access merely because two users both run Master Portus.

A user's system index, task records, application relationships, and provider references must not automatically expose another user's private state.

## 13. `portusd` identity and authorization

PortusOS uses one system runtime daemon, `portusd`, but it may serve more than one local user.

Therefore every local IPC request must carry an authenticated OS principal derived from the Unix-domain-socket peer credentials rather than trusting a caller-supplied username.

`portusd` must determine at least the caller UID/GID and map it to the effective PortusOS principal before authorizing operations.

Rules:

- caller identity comes from the kernel/local IPC boundary;
- ordinary clients do not send a bearer token merely to prove which local Linux user they are;
- user-owned Portus records carry owner/principal association where needed;
- one Master Portus user cannot read or mutate another user's private tasks, artifacts, annotations, provider relationships, or user-scoped state without explicit cross-user authority;
- system-wide records may be visible according to policy without exposing user-private details;
- root/administrator recovery retains access required to repair the system;
- provider integrations must preserve their own user/session identity where relevant.

Detailed schema fields belong to subsystem documents, but the principal-isolation requirement is locked.

## 14. Application and user-environment authority

Master Portus is expected to be able to operate applications installed for or available to its Linux user, subject to provider-specific restrictions and administrator policy.

Ordinary application files and configuration owned by the Master user are ordinary user data unless deliberately classified into a protected credential boundary.

PortusOS should not create artificial per-application agent sandboxes merely because the caller is Master Portus.

Applications that need stronger isolation may provide their own service identity, sandbox, or provider policy.

Protected credential **use authority** and protected credential **export authority** are separate. Administrator policy may allow Master Portus to use a protected API/provider credential repeatedly and unattended through its approved broker/provider while continuing to deny retrieval/export of the reusable value. A permission to use `openai/main`, for example, does not imply permission to obtain the underlying API key.

When a credential is deliberately Master-readable, ordinary Master-user policy governs how it may be read and passed to applications. When a credential is protected, provider/broker policy governs the authenticated operation and must enforce destination/provider binding and the non-export contract defined by `docs/SECRETS.md`.

## 15. Network policy

Network authority is part of the administrator-selected profile.

The architecture must distinguish at least:

- ordinary outbound Internet access;
- local/private network access;
- listening services;
- firewall mutation;
- remote-access services;
- provider-specific network policy;
- external artifact/data delivery.

The broad default may permit ordinary outbound Internet use while retaining stricter rules for network exposure, firewall changes, local/private targets, and external delivery.

Exact defaults belong to the networking/security implementation workstream.

## 16. Devices and hardware

Device access should primarily follow Linux users/groups/ACLs/udev and application/provider mechanisms.

Installer bundles may grant useful device classes without exposing every individual device permission.

After installation, the administrator may refine device authority granularly.

Raw hardware permissions that create root-equivalent compromise paths must be classified accordingly.

## 17. Policy changes and audit

Significant administrator policy changes should be auditable without storing secrets.

Records should identify, where practical:

- administrator actor/principal;
- target user/principal;
- policy/bundle changed;
- previous/new effective classification or a safe diff reference;
- timestamp;
- result;
- whether root-equivalent authority was introduced or removed.

Master Portus must be able to inspect its effective policy in machine-readable form.

Inspection does not imply authority to change it.

## 18. Recovery behavior

Policy failure must not lock the administrator out of the machine.

Recovery must remain possible through root/admin access, TTY, SSH where configured, OpenRC, filesystem repair, and the live ISO.

A broken `portusd`, malformed user policy, or failed Master Portus session must not become the only gate to root administration.

The administrator must be able to disable a Master Portus account or remove its elevated permissions without running that Master Portus instance.

## 19. Installer requirements

The installer security flow must eventually prove all of the following:

1. Administrator/root authority is established independently from Master Portus.
2. A Master Portus Linux user is created with a unique UID and private home.
3. Master/project workspace paths are user-scoped.
4. The administrator selects understandable permission bundles.
5. The installer clearly distinguishes ordinary broad authority from root-equivalent authority.
6. The resulting Linux groups/ACLs/polkit/sudo/helper/socket/device/network configuration is applied consistently.
7. Protected credential/recovery material does not become readable merely because a broad permission bundle is selected.
8. The administrator can inspect the resulting effective policy before installation is considered complete.
9. The Master user cannot modify the administrator-owned policy files controlling its own enforced authority.
10. A recovery path exists if the policy or Master startup path is broken.

## 20. Validation requirements

The first supported policy implementation must include positive and negative tests showing that:

- Master Portus runs as its intended Linux UID, not root;
- Master Portus is not constrained to a Codex workspace-only sandbox;
- Master Portus can operate ordinary files/applications belonging to its user;
- Master Portus cannot read another user's private home/workspace by default;
- a second Master Portus Linux user can coexist without receiving the first user's private state;
- root/admin can inspect and modify both users;
- permission bundles produce the documented effective rights;
- granular administrator changes take effect;
- Master Portus cannot self-expand enforced policy;
- project agents remain sandboxable independently of the Master user;
- prompt/deny/allow policy behaves as configured;
- root-equivalent grants are detectable and labelled;
- `portusd` derives caller identity from Unix peer credentials and enforces user-scoped state;
- loss of Portus runtime policy services does not remove native root recovery.

## 21. Decisions locked by this document

The following are now locked:

1. Master Portus is a real non-root Linux user.
2. Linux user identity is the primary Master Portus ownership/isolation boundary.
3. Master Portus is not placed inside a general-purpose Codex/Bubblewrap workspace sandbox.
4. The machine/VM/server is the broad containment environment for Master Portus.
5. Master Portus normally receives broad authority over its user environment.
6. Routine actions already granted by policy do not inherently require repeated human approval.
7. The administrator/root authority remains separate and can grant/revoke/change Master Portus authority.
8. Installation uses broad permission bundles rather than exposing every granular permission.
9. After installation the administrator may manage authority granularly.
10. Permission bundles may map to Linux groups, ACLs, sudo, polkit, helpers, sockets, devices, network policy and Portus policy; they are not merely Unix groups.
11. Root-equivalent permissions must be classified and disclosed as root-equivalent.
12. Project/task agents may still use Codex/Bubblewrap/project sandboxes.
13. Multiple independent Master Portus Linux users may coexist even though that is not the primary use case.
14. Cross-user private access is denied by default.
15. Master/project workspace ownership must be user-scoped rather than globally singleton.
16. `portusd` authenticates local callers from Unix peer credentials and enforces principal ownership on user-scoped Portus state.
17. Master Portus cannot silently broaden administrator-enforced policy.
18. Permission to **use** a protected credential-backed capability is distinct from permission to **retrieve/export** the reusable credential.
19. Administrator policy may allow repeated unattended use of a protected credential through its approved provider/broker while export remains denied.
20. Protected provider/broker policy must bind credential use to the approved provider/domain/operation contract and prevent indirect export paths.
21. Native root/admin recovery remains independent of Master Portus and PortusOS runtime health.

## 22. Remaining implementation decisions

The authority model and first policy/elevation mapping are now locked by this document and `docs/POLICY_IMPLEMENTATION.md`. Remaining specialised work must not reopen that boundary:

- exact package/service/device-specific Unix group/ACL/resource mappings after the Artix package set is locked;
- exact action-argument/resource constraint schemas for package/OpenRC/firewall/storage operations;
- actual Artix/OpenRC service mechanics, installed socket/policy/audit ownership, and native typed-operation adapter bindings for `portus-privd`; its Rust implementation, bounded JSONL protocol, peer-authenticated use/admin split, audit sink and host-safe policy semantics are fixed by P9;
- exact user-scoped workspace path normalization/migration behavior;
- exact `portusd` principal/ownership database fields;
- future authenticated one-time approval transport if privileged `prompt` operations need execution rather than safe stop;
- provider-specific policy mapping details consistent with the shared allow/prompt/reject model;
- exact tests proving the selected Codex Master invocation is not workspace-sandboxed while delegated agents remain bounded.

Those implementation details must conform to this document and `docs/POLICY_IMPLEMENTATION.md`.