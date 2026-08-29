# PortusOS Policy and Elevation Implementation

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative first-ISO policy/elevation implementation contract

**Scope:** Installer permission bundles, effective authority materialization, Linux groups/ACLs/socket grants, Portus policy files, privileged typed operations, root-equivalent classification, administrator mutation, `allow`/`prompt`/`reject` enforcement, and the first privileged-operation boundary

## 1. Purpose and authority

This document turns the authority model in `docs/POLICY.md` into a concrete first-ISO Linux implementation.

It is subordinate to:

- `docs/POLICY.md` for the meaning and ownership of authority;
- `docs/SECRETS.md` for protected-secret boundaries;
- `docs/PROTECTED_API_PROVIDER.md` for protected API use/admin identities;
- `docs/CLI.md` for the `portus-os` user/agent command contract;
- `docs/RUNTIME.md` for `portusd` and its non-root coordination boundary.

The defining rule is:

> Permission bundles are installer UX. Effective authority is implemented by explicit Linux/resource grants, root-owned Portus policy, provider policy, and narrowly typed privileged operations. No bundle silently grants generic root execution.

---

## 2. Five enforcement classes

Every PortusOS permission belongs to one of five implementation classes.

### 2.1 User-native

The Linux user already possesses the authority directly.

Examples:

- read/write its own files;
- run ordinary user processes;
- use Git/compilers/project tools;
- operate its own graphical session.

No Portus privilege helper is involved.

### 2.2 Resource grant

Linux grants access to a specific resource through a group, ownership, ACL, socket mode or device rule.

Examples:

- `portus-api-users` for the protected API use socket;
- `portus-priv-users` for the privileged-operation transport socket;
- audio/video/input device access where the selected Artix packages use those groups/ACLs;
- explicitly shared project/artifact directories.

Resource access is transport/resource eligibility, not unconditional semantic authorization.

### 2.3 Portus/provider policy

A Portus or provider operation is evaluated as:

```text
allow
prompt
reject
```

against an authenticated principal and typed operation context.

Examples:

- use `openai/main` for `openai.responses.create`;
- external delivery to an approved destination;
- a selected managed system operation.

### 2.4 Privileged typed operation

A root-only Linux mutation is exposed through a narrowly defined operation whose arguments and resource scope are validated by a root-owned privileged boundary.

Examples may include:

- install/remove a named package from administrator-approved signed repositories;
- restart an approved OpenRC service;
- apply an approved firewall operation;
- reboot/shutdown under policy;
- perform a selected mount/device operation.

The privileged boundary never accepts arbitrary shell text or a generic root command.

### 2.5 Root-equivalent grant

A permission is root-equivalent when it gives the Master user a practical path to arbitrary root takeover or equivalent replacement of trusted system state.

Root-equivalent grants are outside the normal broad profile and require explicit administrator action and warning.

---

## 3. First privileged-operation boundary

PortusOS uses a small root-owned local service:

```text
daemon:          portus-privd
service identity: root
client group:    portus-priv-users
```

Its sole purpose is to execute reviewed typed privileged operations under root-owned policy.

It is not a second shell, package manager, file API, or general root RPC service.

### 3.1 Runtime interfaces

Canonical runtime directory:

```text
/run/portus/priv/
```

Sockets:

```text
/run/portus/priv/use.sock
/run/portus/priv/admin.sock
```

Transport permissions:

```text
use.sock
  owner: root
  group: portus-priv-users
  mode:  0660

admin.sock
  owner: root
  group: root
  mode:  0600
```

Both interfaces authenticate local callers from Unix peer credentials.

Administrative policy mutation additionally requires:

```text
peer UID == 0
```

at the protocol level.

Socket modes are defense in depth; they are not the only authorization check.

### 3.2 No generic root primitive

`portus-privd` must not expose operations equivalent to:

```text
exec
shell
run_as_root
sudo_anything
write_arbitrary_root_file
chmod_arbitrary_path
chown_arbitrary_path
load_arbitrary_service_definition
install_arbitrary_local_package
```

An operation is eligible only when its semantic contract and argument constraints are implemented explicitly.

### 3.3 Direct principal preservation

The first privileged-operation implementation prefers direct Unix-socket calls from the requesting local principal so `portus-privd` can authenticate the actual peer UID/GID itself.

`portusd` must not relay an arbitrary caller-supplied UID to `portus-privd` as identity proof.

A future trusted delegation mechanism may allow `portusd` to relay managed-task authority, but that requires an explicit authenticated delegation contract. It is not part of the first implementation.

### 3.4 Relationship to `portusd`

`portusd` remains the non-root Portus coordination daemon.

`portus-privd` owns only the privileged Linux mutation boundary. Safe result/audit metadata may later be correlated into Portus state, but `portusd` does not become root merely because privileged operations exist.

Failure of `portus-privd` makes affected privileged operations unavailable; it must not disable Linux, SSH, ordinary Codex work, `portusd`, or independent providers.

---

## 4. Root-owned policy material

Administrator-enforced Portus policy lives under:

```text
/etc/portus/policy/
```

The first layout is:

```text
/etc/portus/policy/
  policy.toml
  subjects.d/
    <uid>.toml
```

Shipped installer bundle templates and action metadata live under package-owned read-only content:

```text
/usr/share/portus/policy/
  bundles/
    files-workspaces.toml
    applications-desktop.toml
    development.toml
    network-internet.toml
    system-administration.toml
    devices-hardware.toml
    remote-access.toml
    external-data-delivery.toml
  actions.toml
```

Runtime enforcement does not interpret prose documentation as configuration.

### 4.1 File ownership

Administrator policy is root-owned and not writable by Master Portus:

```text
/etc/portus/policy             root:root
/etc/portus/policy/policy.toml root:root
/etc/portus/policy/subjects.d  root:root
```

Policy contains no reusable secrets. Safe portions may be world-readable where useful, but write authority remains root-only.

### 4.2 Versioned TOML

The first policy configuration format is versioned TOML.

Conceptually:

```toml
policy_version = 1

[subject]
uid = 1000
name = "master"

[[grants]]
action = "package.repo.install"
effect = "allow"

[[grants]]
action = "external.delivery"
effect = "prompt"
scope = "new-destination"
```

Exact field serialization for action-specific constraints may be refined during implementation, but:

- the format is versioned;
- subject identity is UID-based, with usernames only as safe labels;
- actions are typed IDs, not shell strings;
- unknown actions/effects fail closed;
- malformed policy does not broaden authority.

### 4.3 Installer bundles compile to grants

The installer records which bundles were selected for provenance/UX, but runtime authority is the resolved grants/resources produced from those choices.

The system must not require every runtime component to reason about installer bundle labels.

---

## 5. First installer bundle defaults

The normal PortusOS **Broad** profile enables useful machine control while excluding root-equivalent grants.

| Bundle | Broad-profile default | First implementation direction |
| --- | --- | --- |
| Files and Workspaces | Enabled | Own home/workspaces; explicit ACLs for shared locations only |
| Applications and Desktop | Enabled | User session/application control and ordinary desktop/audio/video/input resources |
| Development | Enabled | Compilers, project processes, developer tools; no generic root package execution |
| Network and Internet | Enabled | Ordinary outbound Internet/network use; privileged firewall changes remain typed operations |
| System Administration | Enabled, constrained | Approved typed package/service/power/maintenance operations only; no arbitrary root execution |
| Devices and Hardware | Enabled, ordinary devices only | User-facing devices selected by the installation; raw disk/kernel-equivalent access excluded |
| Remote Access | Enabled | SSH access/recovery for the Master user under normal SSH policy |
| External Data and Delivery | Approved destinations allow; new destinations prompt | Delivery remains destination/policy scoped |

A Restricted profile may disable any of these bundles. A Custom profile may materialize finer grants.

### 5.1 System Administration does not mean root

The Broad profile may allow unattended typed maintenance such as an approved repository package install or approved service restart.

It does not grant:

- unrestricted sudo/doas;
- root shell;
- arbitrary root file write;
- arbitrary package payload install;
- arbitrary service definition installation;
- boot/kernel/module replacement;
- raw storage access.

---

## 6. First typed privileged-operation classes

The first action registry may define operations in these semantic families:

```text
package.repo.install
package.repo.remove
service.start
service.stop
service.restart
service.enable
service.disable
firewall.apply
power.reboot
power.shutdown
mount.manage
```

The existence of an action ID does not automatically authorize every target.

Root-owned policy constrains resources such as:

- approved package repositories/packages;
- approved OpenRC service names;
- approved firewall operation classes;
- approved mount/device classes.

Package/OpenRC/storage/network documents may narrow or extend the typed registry when their exact implementations are locked.

### 6.1 Package boundary

Installing/removing a package by name from administrator-approved signed repositories may be exposed as a normal privileged typed operation.

The following are root-equivalent unless separately proven otherwise and are **not** part of the normal package grant:

- installing an arbitrary caller-supplied local package/archive;
- changing package repositories/mirrors/trust/keyrings through the Master path;
- caller-supplied package hooks/scripts;
- arbitrary package-manager command-line passthrough.

### 6.2 Service boundary

Starting/stopping/restarting/enabling/disabling an administrator-approved installed OpenRC service may be a typed privileged operation.

Writing/replacing arbitrary init scripts or selecting an arbitrary executable as a root service is root-equivalent and prohibited from the normal operation surface.

---

## 7. Policy effects

### 7.1 `allow`

The operation may execute without human interaction when all Linux/resource/provider constraints are satisfied.

This is the normal mechanism for unattended authority deliberately granted by the administrator.

### 7.2 `reject`

The operation fails closed and no privileged side effect occurs.

### 7.3 `prompt`

The first implementation does **not** invent an insecure approval relay.

For a privileged operation whose effective policy is `prompt`, `portus-privd` returns a typed `approval_required` result and performs no privileged mutation.

A future approval transport may create a bounded one-time grant only after its authentication/replay/expiry contract is explicitly locked. Until then, `prompt` is a safe stop rather than an implicit fallback to `allow`.

This does not prevent non-privileged/provider domains from implementing their own already-defined approval mechanisms where appropriate.

---

## 8. Root-equivalent classification

The action registry and administrator UI/CLI must mark a grant as root-equivalent whenever it can reasonably provide arbitrary root takeover or replacement of trusted root state.

The first explicit root-equivalent categories include:

1. unrestricted sudo/doas or equivalent;
2. generic root shell/command execution;
3. arbitrary write/replace access to root-owned executables, init scripts, policy, sudo/polkit/PAM/auth configuration or package hooks;
4. arbitrary local/unsigned package installation or package trust/repository/keyring modification;
5. arbitrary root service creation or executable selection;
6. raw system block-device write sufficient to replace installed trusted content;
7. arbitrary bootloader/initramfs/kernel/module control;
8. `/dev/mem`/`/dev/kmem` or equivalent kernel-memory authority;
9. privileged container/virtualization sockets capable of mounting/modifying the host root filesystem;
10. arbitrary setuid/file-capability ownership manipulation;
11. root scheduled-job/cron creation;
12. arbitrary udev/device-rule installation;
13. debugging/injection authority over root processes sufficient to execute code as root;
14. authority to grant oneself or another principal any of the preceding capabilities.

This inventory is extended when platform research identifies additional root-equivalent paths.

### 8.1 Explicit acknowledgement

Normal installer bundles never include a root-equivalent grant.

Administrator CLI attempts to add one must require an explicit acknowledgement flag equivalent to:

```text
--ack-root-equivalent
```

The warning is UX/audit evidence, not the security boundary; root remains capable of editing policy directly.

---

## 9. Policy CLI administration

`docs/CLI.md` retains the normal inspection surface:

```text
portus-os policy effective
portus-os policy check ...
```

This document adds root-only administrator mutation forms:

```text
portus-os policy admin show <uid>
portus-os policy admin grant <uid> <action> --effect allow|prompt|reject [typed constraints]
portus-os policy admin revoke <uid> <action> [typed constraints]
portus-os policy admin bundle set <uid> <bundle-id> --enabled|--disabled
```

Root-equivalent grants additionally require:

```text
--ack-root-equivalent
```

### 9.1 Root-only administration

`policy admin` is rejected unless the local effective caller is UID 0 and the `portus-privd` admin protocol authenticates peer UID 0.

Master Portus cannot use the existence of these commands to expand its own authority.

### 9.2 Atomic policy updates

Administrator policy mutation must be transactional at the file level:

1. parse/validate current version;
2. apply typed mutation;
3. validate complete resulting policy;
4. write a root-owned temporary file in the same filesystem;
5. `fsync` as appropriate;
6. atomic rename;
7. reload/revalidate policy;
8. report the new effective result.

A malformed/incompatible new policy must not replace the last valid policy.

---

## 10. Audit boundary

Significant privileged-policy events record safe metadata such as:

- request ID;
- peer UID/principal;
- action ID;
- target/resource safe identifier;
- effective result (`allow`, `prompt`, `reject`);
- whether the grant was root-equivalent;
- timestamp;
- success/failure category.

Audit must not contain protected reusable secrets or arbitrary request bodies.

Correct execution does not depend on successful delivery of a secondary audit notification.

---

## 11. First-ISO validation

The first ISO must prove at least:

1. Master Portus is non-root;
2. Broad-profile bundle materialization produces expected user/group/policy state;
3. Master cannot modify `/etc/portus/policy/`;
4. `portus-privd` authenticates callers from Unix peer credentials;
5. a permitted typed operation succeeds without a password prompt when policy is `allow`;
6. a rejected operation performs no privileged side effect;
7. a `prompt` operation returns `approval_required` and performs no side effect in the first implementation;
8. a caller cannot turn a typed operation into arbitrary command/root-file execution;
9. socket membership alone does not bypass policy;
10. a non-root caller cannot use the admin socket even under deliberately widened filesystem permissions;
11. normal Broad profile contains no root-equivalent grant;
12. an explicit root-equivalent administrator grant is clearly classified/warned;
13. `portus-privd` failure leaves Linux/SSH/Codex/`portusd`/independent providers usable.

---

## 12. Decisions locked by this document

1. Permission bundles compile into explicit effective grants/resources rather than remaining the runtime enforcement abstraction.
2. The first policy implementation uses user-native, resource-grant, Portus/provider-policy, privileged-typed-operation and root-equivalent classes.
3. A dedicated root-owned `portus-privd` service is the first privileged Linux mutation boundary.
4. `portus-privd` exposes separate use/admin Unix sockets under `/run/portus/priv/` and authenticates peers from kernel credentials.
5. `portus-priv-users` is transport eligibility only and does not itself authorize operations.
6. `portus-privd` never exposes generic root shell/exec/arbitrary-root-file primitives.
7. First privileged requests preserve the real local peer principal; `portusd` does not assert arbitrary caller UIDs to the helper.
8. Root-owned active policy lives under `/etc/portus/policy/`; shipped bundle/action templates live under `/usr/share/portus/policy/`.
9. First policy format is versioned TOML with UID-based subject identity and typed actions.
10. The normal Broad profile enables all eight bundle domains in constrained form while excluding root-equivalent grants.
11. System Administration means approved typed maintenance, not generic root authority.
12. Package/service privileged operations are constrained to administrator-approved repositories/resources; arbitrary package payload/root-service creation is root-equivalent.
13. `allow` executes, `reject` denies, and first-implementation `prompt` returns `approval_required` without executing until a secure approval-grant contract exists.
14. Root-equivalent categories are explicitly classified and never silently included in normal bundles.
15. Root-only `portus-os policy admin ...` commands manage typed policy; root-equivalent grants require explicit acknowledgement.
16. Policy updates are validated and atomically replaced rather than edited partially in place.
17. `portus-privd` failure degrades privileged operations only and does not become a single point of failure for ordinary Linux/Portus operation.

---

## 13. P9 concrete implementation status

P9 fixes the source-neutral first implementation as follows:

- first-party implementation language: Rust, in the shared workspace;
- shared resolver crate: `portus-policy`;
- normal inspection/preflight transport: `portusd` methods `policy.effective` and `policy.check`;
- privileged use method: `privilege.execute` on `/run/portus/priv/use.sock`;
- administrator methods: `policy.admin.show`, `policy.admin.grant`, `policy.admin.revoke`, and `policy.admin.bundle.set` on `/run/portus/priv/admin.sock`;
- framing: the existing versioned bounded JSONL `RequestEnvelope` / `ResponseEnvelope` contract;
- first `portus-privd` connection bound: 32 concurrent local connections, common 1 MiB frame bound, common 30-second I/O timeout;
- `portus-privd` refuses to start unless its effective UID is 0;
- `use.sock` is explicitly owned `root:portus-priv-users` mode `0660`; `admin.sock` is explicitly `root:root` mode `0600`;
- both sockets derive UID/GID from kernel peer credentials; JSON identity fields are not part of the privileged request schema;
- policy v1 requires `default_effect = "reject"`, strict unknown-field rejection, bounded action/bundle/subject/grant/resource counts, and `subjects.d/<uid>.toml` filename/identity agreement;
- production root-owned trust validates policy parent directories plus policy/action/bundle/subject files as root-owned, non-symlink, and not group/world writable;
- administrator subject changes are validated, serialized to a same-filesystem temporary file, `fsync`ed, atomically renamed, directory-`fsync`ed, reread/revalidated, and only then reflected in the live snapshot;
- `prompt` is a typed `approval_required` stop with no executor call; `reject` is permission denial with no executor call;
- root-equivalent non-reject grants require explicit `--ack-root-equivalent`, and successful root-equivalent grant/revoke changes are distinguishable in policy audit reason metadata;
- `portus-privd` uses the P8 bounded audit format at `/var/log/portus/audit/portus-privd.jsonl` and has no generic request-payload audit field;
- the production typed-operation executor remains intentionally unavailable until reviewed Linux adapters are bound to the package/OpenRC/network/storage contracts. This is fail-closed Linux carry-over, not a generic command fallback.

Host-safe tests prove the resolver, CLI routing, root/admin semantics, allow/prompt/reject behavior, no generic privileged methods, root-equivalent acknowledgement and audit behavior. The Linux-only policy/privilege source also cross-type-checks for `x86_64-unknown-linux-gnu`; actual UID-0/socket/group/action execution remains Linux acceptance evidence.

---

## 13. Remaining implementation detail

Later Linux/installer work must still define or validate:

- exact package/service/firewall/mount native adapters and action-specific argument/resource schemas after those subsystems are locked;
- exact native Artix group/ACL/device mappings for the selected package set beyond the already-locked `portus-priv-users` transport group;
- actual OpenRC service/runlevel/restart configuration for `portus-privd`;
- installer materialization of the eight Broad-profile bundle files and machine-specific approved resource grants;
- installed policy/audit directory ownership, modes and recovery behavior;
- real UID-0/peer-credential/socket-group adversarial execution on Artix;
- a future authenticated one-time approval-grant mechanism if privileged `prompt` operations ever need execution rather than safe stop.

Those details must not reopen the no-generic-root-primitive, peer-identity, root-equivalent, or bundle-materialization decisions above.
