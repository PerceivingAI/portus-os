# PortusOS First-ISO Validation Authority

**Last reviewed:** 2026-08-29T09:15:54Z
**Last updated:** 2026-08-29T09:15:54Z

**Status:** Authoritative executable validation contract for the first ISO
**Acceptance scope:** ISO-01 through ISO-38 from `docs/ACCEPTANCE.md`
**Primary environment:** x86_64 VMware reference VM
**Evidence schema version:** 1
**Acceptance authority:** `docs/ACCEPTANCE.md`
**Release authority:** `docs/RELEASE.md`

## 1. Purpose and authority

`docs/ACCEPTANCE.md` defines **what** the first PortusOS ISO must prove. This document defines **how the proof is executed, recorded, repeated, invalidated and accepted**.

A green implementation/unit suite is useful development evidence but is not first-ISO acceptance. First-ISO acceptance requires the real installed Artix/OpenRC system, the real VMware target, the real supported installer path and the real security/recovery boundaries specified here.

When documents overlap:

1. `docs/ACCEPTANCE.md` controls acceptance scope and blocking test meaning.
2. The owning subsystem authority controls implementation/security semantics.
3. This document controls test orchestration, evidence, adjudication, reset, redaction and reporting.
4. `docs/RELEASE.md` controls candidate/release identity and artifact publication.

If a subsystem command, package, or service identity remains intentionally unresolved until verified Artix execution, this document does not guess it. The owning public authority and machine-readable contract must freeze the exact value before the first release candidate that depends on it is validated.

---

## 2. Acceptance principles

The validation system follows these rules:

- **Same candidate:** all acceptance evidence refers to one immutable ISO candidate by SHA-256.
- **Real boundary:** mocks/fakes may support development tests but cannot satisfy a blocking ISO proof.
- **No hidden repair:** undocumented manual repair makes the affected test fail.
- **Fail closed:** missing prerequisites, ambiguous identity, missing evidence or unsafe redaction never become a pass.
- **Secret-safe evidence:** reusable credentials must not appear in evidence, logs, screenshots, state exports or reports.
- **Repeatable environment:** destructive tests start from a named clean snapshot/baseline rather than accumulated developer state.
- **Independent recovery:** recovery tests intentionally exercise TTY/SSH/OpenRC/live-media paths rather than using the failed Portus component as their only repair mechanism.
- **Machine evidence first:** use structured output and native machine-readable inspection where available; screenshots/manual notes supplement rather than replace it.
- **No acceptance by inference:** a host-safe test may support a claim but does not substitute for the installed-system proof when the acceptance row requires one.

---

## 3. Test execution classes

Every blocking test is assigned one primary execution class.

### 3.1 Automated

The harness can perform setup, action, assertion and evidence collection without a human making a semantic pass/fail judgement after prerequisites are available.

A human may initiate the run, unlock an encrypted boot boundary or provide an external secret, but those actions do not make the result manual if the assertions remain machine adjudicated.

### 3.2 Assisted

The harness performs and records as much as possible, but the test contains an intentional human/interactive boundary such as:

- Calamares interaction;
- owner LUKS unlock;
- selecting an alternate GRUB entry;
- graphical observation/input;
- entering a protected credential through the non-echo root TTY path;
- intentionally breaking/recovering a graphical/session component.

The evidence must still contain objective assertions. A note saying “looked good” is insufficient.

### 3.3 Manual

A manual-only test requires human judgement that cannot currently be reduced to reliable structured evidence.

**No ISO-01..ISO-38 test is intended to remain manual-only.** If implementation work discovers one, this document must be revised before candidate freeze with an explicit reason and evidence checklist.

---

## 4. Candidate identity

Before validation begins, create one candidate record containing at least:

```json
{
  "schema_version": 1,
  "candidate_id": "<release-authority candidate id>",
  "iso_filename": "<basename>",
  "iso_sha256": "<64 lowercase hex>",
  "source_revision": "<40 hex git revision>",
  "build_metadata_ref": "<relative evidence path>",
  "package_source_manifest_ref": "<relative evidence path>",
  "validation_authority_revision": "<git revision containing this procedure>",
  "created_at": "<UTC RFC3339>"
}
```

The candidate record is stored at:

```text
out/validation/<candidate-id>/candidate.json
```

The candidate ID and ISO SHA-256 are repeated in every test result.

Validation must not rely on a mutable “latest.iso” identity.

---

## 5. VMware profiles

### 5.1 Reference profile

The complete blocking suite runs on:

```text
architecture     x86_64
vCPU             4
RAM              8 GiB
virtual disk     80 GiB
firmware         UEFI
Secure Boot      off
network          one NAT NIC
3D acceleration  not required
```

ISO-38 is the assertion that this profile completes the full suite.

### 5.2 Minimum profile

ISO-37 uses a separate clean VM:

```text
architecture     x86_64
vCPU             2
RAM              4 GiB
virtual disk     40 GiB
firmware         UEFI
Secure Boot      off
network          one NAT NIC
3D acceleration  not required
```

The minimum profile must install, boot and reach usable local/SSH recovery state. It does not replace the reference-profile full-suite run.

### 5.3 VM environment evidence

Each VM run records at minimum:

- VMware product/build identity where available;
- VM configuration export or equivalent machine-readable hardware summary;
- virtual disk size;
- firmware/Secure Boot state;
- NIC/network mode;
- candidate ISO SHA-256 attached to the VM;
- clean snapshot/baseline name and creation timestamp.

---

## 6. Evidence layout

The canonical evidence root is:

```text
out/validation/<candidate-id>/
  PortusOS-<candidate-artifact>-x86_64.iso
  SHA256SUMS
  build-metadata.json
  candidate.json
  package-source-manifest.yaml
  packages.lock.yaml              # present once verified Artix resolution has generated it
  codex-pin.yaml
  portus-browser-pin.yaml
  environment/
    reference.json
    minimum.json
  tests/
    ISO-01/
      result.json
      commands.jsonl
      stdout.log
      stderr.log
      artifacts/
    ...
    ISO-38/
  imported/
    update/
    protected-api/
    host-safe/
  redactions.json
  validation-report.json
  validation-report.md
```

A test directory may omit an empty `stdout.log`/`stderr.log`, but `result.json` is mandatory.

`candidate-init` creates this root and initializes `validation-report.json`/Markdown as `incomplete` with all 38 rows `not_run`. Its initial `SHA256SUMS` covers only immutable candidate inputs (ISO, candidate/build metadata and snapshotted package/component inputs). Test evidence, environment records, redaction state and validation reports remain mutable until validation/release finalization and therefore are not part of that initialization checksum set.

Large binary evidence such as screenshots is stored under the test's `artifacts/` directory and referenced by filename plus SHA-256. Do not embed image/database/blob payloads directly into `result.json`.

---

## 7. Per-test result schema

Each `tests/ISO-XX/result.json` uses this logical shape:

```json
{
  "schema_version": 1,
  "candidate_id": "...",
  "iso_sha256": "...",
  "test_id": "ISO-XX",
  "execution_class": "automated|assisted|manual",
  "status": "pass|fail|blocked|not_run",
  "environment": "reference|minimum|build-host|recovery",
  "started_at": "<UTC RFC3339>",
  "ended_at": "<UTC RFC3339>",
  "assertions": [
    {"id":"...","status":"pass|fail","evidence_refs":["..."]}
  ],
  "commands_ref": "commands.jsonl",
  "evidence_refs": ["..."],
  "redaction_applied": true,
  "notes": [],
  "blocking_reason": null
}
```

Rules:

- `blocked` and `not_run` are never acceptance passes.
- Every pass has at least one objective assertion and evidence reference.
- Assertions must describe observable behavior, not implementation intent.
- A human-assisted step is recorded in `commands.jsonl`/notes with the operator action and resulting objective evidence.

---

## 8. Command/evidence recording

`commands.jsonl` records each validation action without storing secret input:

```json
{"sequence":1,"actor":"root|master|harness|operator","command":"<safe command representation>","exit_code":0,"stdout_ref":"stdout-001.log","stderr_ref":"stderr-001.log"}
```

For secret-entry actions, record only a safe operation marker such as:

```json
{"sequence":17,"actor":"operator","operation":"protected_credential_entered_via_non_echo_tty","secret_recorded":false}
```

Do not reconstruct secret-bearing argv/environment data merely for completeness.

### 8.1 Implemented harness boundary

The repository implements the candidate-scoped orchestration layer in `portus-build`.

Host-safe commands:

```text
validation-harness-check
validation-materialize
validation-action
validation-record
validation-redactions
validation-report
validation-verify
```

These commands materialize and validate the evidence structure defined by this document. They do not execute VMware.

Actual VMware execution remains behind:

```text
validation-vm-run
```

While the real VMware execution adapter is unavailable, `validation-vm-run` must fail closed with exit 78. `scripts/test.* graphical-vm` validates the host-safe harness and confirms that unavailable VM execution cannot be mistaken for acceptance; a green `graphical-vm` category therefore means **harness ready**, not **VM acceptance passed**.

Per-test referenced evidence is hash-recorded in `evidence-manifest.json`. A changed referenced file invalidates the result. Retry history is retained under the test's `attempts/` directory rather than silently replacing a prior fail/result. Because `commands.jsonl` is intentionally append-only across retries, archived attempts validate the exact command-log prefix recorded for that attempt.

---

---

## 9. Redaction and secret safety

### 9.1 Forbidden evidence

Validation evidence must never contain:

- raw protected API keys/tokens/passwords;
- `Authorization`/`Proxy-Authorization` header values;
- reusable secret-bearing cookies;
- complete protected-provider database contents;
- secret-bearing process environments;
- private keys;
- unredacted crash/core output containing secrets;
- arbitrary user/project private content unrelated to the test.

### 9.2 Permitted comparison

The privileged security harness may compute a one-way fingerprint of a test credential solely to search outputs for leakage. The raw value is not written to the evidence tree.

### 9.3 Redaction ledger

`redactions.json` records:

```json
{
  "schema_version": 1,
  "candidate_id": "...",
  "files_scanned": 0,
  "files_redacted": [],
  "forbidden_material_detected": false,
  "notes": []
}
```

If forbidden reusable-secret material reaches ordinary validation evidence, the affected test fails and the candidate cannot be accepted until the leakage path is corrected and evidence is regenerated.

---

## 10. Clean-VM and reset rules

### 10.1 Required baselines

Maintain at least these logical states:

```text
blank-reference     blank 80-GiB UEFI/NAT VM
blank-minimum       blank 40-GiB UEFI/NAT VM
installed-baseline  clean reference install from the candidate ISO
```

Additional snapshots may be created for destructive suites, but they must descend from the exact candidate installation rather than a manually assembled machine.

### 10.2 Reset after destructive tests

Tests that intentionally alter boot, provider, GUI, policy, package or service state must either:

1. restore the system through the supported recovery path being tested and prove the baseline is healthy again; or
2. restore a named clean snapshot before the next logically independent test.

The report records which path was used.

### 10.3 Forbidden reset shortcut

Do not repair the VM by copying repository files, editing the virtual disk from the host, manually installing undeclared packages or applying an undocumented fix and then continuing as if the candidate passed.

If such intervention is needed, the candidate/test fails until the supported implementation or documented recovery procedure incorporates the required action.

---

## 11. Candidate invalidation

### 11.1 New candidate required

A new candidate ID and ISO build are required when any release-critical input changes, including:

- shipped source/binaries;
- package/source selection or pins;
- `artools`/Calamares configuration;
- rootfs overlay or installed configuration;
- OpenRC service definitions;
- policy/security defaults;
- kernel/initramfs/GRUB configuration;
- PortusBrowser/Codex source pin or compatibility set;
- protected-provider implementation/definition;
- licence/notices when validating a public RC;
- any other input that changes ISO bytes or installed behavior.

### 11.2 Evidence rerun without ISO rebuild

A validation-harness/procedure change that does not alter candidate bytes may require only affected tests to rerun **if** it does not change acceptance meaning. The validation report must record the procedure revision used for every result.

If the change alters acceptance semantics, all affected prior results are invalid and must be rerun.

### 11.3 Documentation-only clarification

A documentation correction that changes neither candidate bytes, supported behavior nor test semantics does not invalidate technical evidence, but the final report records the documentation revision published with the candidate.

---

## 12. Execution order

The recommended acceptance sequence minimizes cross-test contamination:

```text
V0 build/provenance           ISO-01
V1 boot/install/base          ISO-02..17
V2 policy/index               ISO-18..22
V3 browser/visual             ISO-23..26
V4 protected credential       ISO-27..29 + ISO-35
V5 health/recovery            ISO-30..36
V6 resource profiles          ISO-37..38
```

ISO-05 includes the controlled kernel-affecting update evidence slice defined in Section 16 of this document.

Tests may be reordered only when dependency/reset semantics remain explicit in the report.

---

## 13. Common native evidence commands

These are stable evidence primitives where applicable. The supported Artix/native execution path may add package-specific commands, but it must not silently replace the meaning of these checks.

### Identity/base

```text
uname -m
cat /etc/os-release
id
ps -eo pid,ppid,uid,gid,comm,args
```

### Storage/boot

```text
lsblk -o NAME,SIZE,TYPE,FSTYPE,MOUNTPOINTS
blkid
findmnt /
findmnt /boot
findmnt /boot/efi
swapon --show
cryptsetup luksDump <resolved-luks-device>
pvs
vgs
lvs
```

The LUKS device path is discovered from the installed topology; it is not hard-coded into the validation authority.

### OpenRC/services

```text
rc-status
rc-service <resolved-service> status
```

The first-ISO base service names/runlevels are already frozen in `portusos-build/system/base-services.yaml`; RC validation resolves commands from that manifest and separately proves the installed scripts/dependencies and Portus-owned service definitions.

### CLI/runtime

```text
portus-os version --json
portus-os status --json
portus-os doctor
portus-os help --json
```

### Index

Use the installed finite P6/P15 command contract, including:

```text
portus-os index query [filters] [--limit N] [--cursor C]
```

Exact filters/actions used for ISO-20..22 are recorded in the test procedure after the live resource fixture is frozen.

---

## 14. ISO-01 through ISO-38 executable matrix

The table below fixes the execution class, minimum procedure and required evidence. An installed-system or native-build harness may automate more of an assisted test, but it may not weaken its assertions.

The build contracts materialize only the harness orchestration subset at `portusos-build/validation/matrix.yaml`: test ID, execution class, environment and blocking status. `portus-build-contract` requires exactly ISO-01..ISO-38 in order and mechanically checks each class against this table. Procedure/evidence meaning remains authoritative here and is not duplicated into YAML.

| ID | Class | Minimum executable procedure | Required evidence / pass condition |
| --- | --- | --- | --- |
| ISO-01 | Automated | On a clean native x86_64 Linux build host, enter the documented isolated verified Artix build context and run the `artools` clean-build entry point; resolve package/source manifest; compute ISO SHA-256. | Outer-host identity + Artix-context identity, build command/log, source revision, package/source manifest, ISO hash; no host-package leakage, no AUR helper/dependency in supported path; every non-Artix component has approved source. |
| ISO-02 | Assisted | Create/restore `blank-reference`, attach exact candidate ISO, boot UEFI with Secure Boot off, enter the live Master Portus path and launch Codex. | VM config + boot/session evidence showing networking, non-root Master, X11/i3 + Alacritty, `MasterPortus` tmux, `portus-bootstrap`/`portus-master`, Codex, Chromium as the working default HTTP/HTTPS browser, first-party Portus binaries, bundled Portus MCP/`tunnel-client`, Calamares and recovery tools are available without firmware workaround or pre-provisioned reusable secrets. |
| ISO-03 | Assisted | From `blank-reference`, complete the supported Calamares flow exactly as documented. | Installer log, selected disk/plan summary, completion result; no undocumented terminal/manual repair. |
| ISO-04 | Assisted | Inspect installed partition/filesystem/LUKS/LVM topology; prove owner unlock; separately prove recovery keyslot; test absence of configured automatic network/TPM/embedded-key unlock. | `lsblk`/`blkid`/`findmnt`/`cryptsetup luksDump`/LVM/swap evidence; locked sizes/filesystems/PBKDF/cipher/topology; owner + recovery credential proof with secrets omitted. |
| ISO-05 | Assisted | Remove ISO; boot normal LTS path; boot explicit alternate `linux`; run controlled kernel-affecting managed update; validate four initramfs artifacts + GRUB; explicitly reboot/unlock and re-prove both paths. | Boot/menu/kernel identity evidence, Section 16 update packet, four initramfs file hashes/metadata, GRUB syntax/config evidence, `reboot_required` result where applicable. |
| ISO-06 | Automated | Inspect OS/init/service environment and required dependency set. | Artix identity, OpenRC version/status, absence of required systemd dependency in supported first-ISO path. |
| ISO-07 | Assisted | Boot installed baseline; record OpenRC runlevels/services and graphical/session process ownership. | `rc-status`, per-service status for resolved first-ISO services, process/session evidence showing machine services OpenRC-owned and graphical/session processes session-owned. |
| ISO-08 | Assisted | Reach/switch to local TTY and authenticate after normal boot and during at least one degraded graphical condition. | TTY login command evidence and recovery notes proving shell usability independent of GUI/Master. |
| ISO-09 | Automated | SSH from validation peer into installed VM; run shell diagnostics; verify no X11/i3 graphical session is created solely by SSH login. | SSH command transcript, peer/network evidence, before/after graphical process/index evidence. |
| ISO-10 | Assisted | Local Master TTY login follows PAM/elogind -> conditional `startx` -> `dbus-run-session` -> session launcher -> i3/Alacritty; verify 3D disabled/irrelevant and SSH does not trigger path. | Session/process/environment evidence plus screenshot; no display manager dependency; no VMware 3D requirement. |
| ISO-11 | Automated | Inspect Master UID/GID and relevant processes. | `id`, process ownership, policy/profile evidence proving Master UID != 0 and root/admin remains distinct. |
| ISO-12 | Assisted | Invoke supported local bootstrap path from a fresh Codex auth state, select **Sign in with ChatGPT**, complete the bundled-Chromium localhost callback, then invoke the bootstrap path again to reattach/resume; exercise or explicitly skip the optional tunnel setup. | Exact `MasterPortus` tmux session/window evidence, Chromium OAuth launch/callback and authenticated Codex status, Master workspace path, Codex resume evidence, optional Portus MCP/tunnel setup disposition; no duplicate session and no API-key/device-code dependency for the normal local path. |
| ISO-13 | Assisted | From Master Codex, perform one allowed operation outside a workspace-only sandbox while remaining non-root. | Effective UID, operation result, relevant path/environment evidence; no implicit root or unintended workspace-only confinement. |
| ISO-14 | Automated | Inspect OpenRC state/socket; run runtime-backed status. | `portusd` service status, `/run/portus/portusd.sock` type/mode/owner/group, successful `portus-os status --json`. |
| ISO-15 | Automated | Use two authenticated local principals or owner/unauthorized fixture against a principal-sensitive runtime operation; attempt caller identity spoof field where applicable. | Kernel-derived peer principal evidence, allowed owner result, cross-principal/spoof rejection, no caller-supplied UID trust. |
| ISO-16 | Automated | Create/observe required durable state; restart `portusd` through supported OpenRC action; re-read state. | Pre/post IDs/data, DB integrity/readiness evidence, service restart evidence; no silent DB recreation/generation loss. |
| ISO-17 | Automated | Execute at least one runtime-backed CLI command in JSON mode through the canonical socket. | Valid CLI schema/result, request succeeds through daemon/state path, version/protocol metadata. |
| ISO-18 | Automated | Execute the frozen first-ISO administrator-granted typed `portus-privd` operation as Master. | Policy effective/check evidence, privilege audit, successful typed operation and postcondition; no repeated approval beyond configured allow. |
| ISO-19 | Automated | Execute a frozen operation outside ordinary Master grant (or prompt case); separately confirm root-only policy administration. | `reject`/`approval_required`, no privileged side effect, audit evidence, non-root admin mutation denied. |
| ISO-20 | Automated | Query each required index class: applications, processes, OpenRC services, graphical windows, i3 workspaces, Portus providers/capabilities. | Structured index outputs with real source/generation/freshness data for every required class. |
| ISO-21 | Automated | Create a real observable fixture resource, rescan relevant domain, prove appearance/change; remove it, rescan, prove disappearance/stale transition as defined. | Before/after structured index results and source generation evidence. |
| ISO-22 | Automated | Inspect the same fixture through its native Linux/application interface without using System Index. | Native command/application output plus matching high-level identity; proves index is optional rather than authority monopoly. |
| ISO-23 | Assisted | Install/freeze PortusBrowser payload/native messaging for Chromium; start Chromium; probe broker/sessions. | Clean PortusBrowser source pin, installed component versions, native-messaging config, broker health, opaque browser-session provider resource; real connection. |
| ISO-24 | Assisted | Through PortusBrowser structured interface obtain real snapshot/observation, perform one structured action, re-observe state. | Structured request/result artifacts before/after; no browser DOM/page state copied into generic `portusd` state. |
| ISO-25 | Assisted | Invoke selected real P14 screenshot backend against a non-sensitive graphical target. | Captured PNG/JPEG artifact hash, target/generation evidence, policy/audit result; image is real screen content and bounded. |
| ISO-26 | Assisted | Against a controlled non-sensitive target, execute target-relative pointer action and keyboard action through P14 policy path. | Before/after target state, generation revalidation, visual audit, proof reject/prompt has no side effect where exercised. |
| ISO-27 | Assisted | As root/admin, provision test reusable credential using `portus-auth` non-echo input into real `portus-apid`; inspect generic surfaces. | Provision success/safe ref; no secret argv, generic DB/log/project evidence; service/store ownership evidence. |
| ISO-28 | Assisted | As authorized Master, invoke one real named protected operation against the real upstream/equivalent provider. | Successful upstream result/status, safe provider response, Master never receives reusable value. |
| ISO-29 | Assisted | Run all applicable Groups B-G from `docs/PROTECTED_API_VALIDATION.md`; Group G is required only when the compatibility proxy/token path is implemented. | Imported adversarial report; every applicable direct/indirect export, confused-deputy, redirect/TLS, cross-user, failure/recovery and compatibility-token case passes; leak scan clean. |
| ISO-30 | Automated | On healthy installed baseline run aggregate doctor and relevant typed health queries. | `portus-os doctor` plus structured health evidence covering required domains with expected healthy/known states. |
| ISO-31 | Automated | Stop `portusd` through supported admin/OpenRC path; run daemon-independent doctor. | Doctor identifies runtime/socket failure without using successful daemon RPC; no hang/hidden fallback. |
| ISO-32 | Assisted | While `portusd` is down, prove SSH/TTY/native OpenRC work; restart daemon; validate state retained. | SSH/TTY/OpenRC transcript, restart/status, pre/post durable state identity. |
| ISO-33 | Assisted | Induce controlled Master/Codex startup failure before normal Master workflow; recover through TTY/SSH/root path. | Failure log, TTY/SSH/root availability, shell recovery, no dependency on successful Master startup. |
| ISO-34 | Assisted | Stop/break X11/i3 in controlled fixture; use TTY/SSH/non-graphical doctor/recovery; restore supported graphical path. | GUI failure evidence, surviving recovery surfaces, restored session evidence. |
| ISO-35 | Assisted | Stop/fail `portus-apid`; inspect capability/provider health; test protected use failure; verify unrelated systems remain usable; recover provider. | Degraded/unavailable protected capability, safe error, no fallback/leak, unrelated runtime/browser/Linux evidence, recovery result. |
| ISO-36 | Assisted | Boot candidate/release recovery media; unlock LUKS; activate VG; mount root + `/boot` + ESP; enter installed environment with verified Artix chroot path; perform non-destructive inspection. | Exact device-discovery/mount/chroot transcript, storage identity and successful installed-root access; secrets omitted. |
| ISO-37 | Assisted | From `blank-minimum`, install candidate and boot installed system; prove local TTY + SSH recovery availability under 2 vCPU/4 GiB/40 GiB. | Minimum VM config, install/boot result, resource observation, TTY/SSH evidence; no architecture-changing workaround. |
| ISO-38 | Assisted | On reference 4/8/80 profile, aggregate the same candidate's ISO-01..ISO-36 reference-profile results and link the separate ISO-37 minimum-profile result. | Validation report shows ISO-01..ISO-38 pass for the same candidate; reference VM identity matches Section 5.1 and ISO-37 identifies the separate minimum environment. |

---

## 15. Host-safe evidence imported from P17

Host-safe P17 evidence is regression support and must run for every candidate source revision before ISO build acceptance begins.

Required current-host/source-neutral gates include:

```text
scripts/check.*
scripts/test.* hardening
scripts/test.* security-negative
scripts/test.* all
```

Use the platform-appropriate entry point. The build/validation worker records exact exit codes and toolchain identity.

This imported evidence covers malformed/oversized protocol input, stale generations/preconditions, principal filtering, privilege-route rejection, secret-shaped durable-state rejection, bounded retention/output, provider failure isolation, artifact safety and install path/replacement defense. It **does not** close Linux socket/filesystem/OpenRC/VM acceptance rows by itself.

---

## 16. Managed-update evidence imported from `UPDATES_RECOVERY`

ISO-05 and release update qualification import `docs/UPDATES_RECOVERY.md` Section 15.4.

### 16.1 Before mutation

Capture at minimum:

```text
pacman --version
pacman-conf --repo-list
pacman-conf DBPath CacheDir GPGDir HookDir SigLevel HoldPkg IgnorePkg IgnoreGroup
pacman -Dk
pacman -Q
rc-status
find /etc \( -name '*.pacnew' -o -name '*.pacsave' \) -type f -print
```

Also record metadata/checksums for:

```text
/boot/vmlinuz-linux-lts
/boot/initramfs-linux-lts.img
/boot/initramfs-linux-lts-fallback.img
/boot/vmlinuz-linux
/boot/initramfs-linux.img
/boot/initramfs-linux-fallback.img
/boot/grub/grub.cfg
```

Record the complete reviewed package transaction through the live-verified pacman print resolver or the narrow read-only libalpm resolver selected during verified Artix execution.

### 16.2 After mutation

Capture at minimum:

```text
pacman -Dk
pacman -Q
rc-status
find /etc \( -name '*.pacnew' -o -name '*.pacsave' \) -type f -print
```

For every explicitly restarted service:

```text
rc-service <service> status
```

For early-boot/kernel-affecting changes:

```text
mkinitcpio -P
grub-mkconfig -o /boot/grub/grub.cfg
grub-script-check /boot/grub/grub.cfg
```

Then assert:

- all four required initramfs images exist and are non-empty;
- GRUB still contains the normal `linux-lts` and alternate `linux` paths;
- relevant Portus/provider health checks pass or explicitly report degradation;
- `portus-os doctor` explains remaining state;
- typed update result records `reboot_required` and finite reasons correctly;
- no automatic reboot occurred;
- required-domain `.pacnew`/`.pacsave` conflicts prevent a fully healthy result until reconciled.

The explicit owner-unlocked reboot/alternate boot remains ISO-05, not an implicit part of the update task.

---

## 17. Protected-provider adversarial evidence import

ISO-27..29 and ISO-35 import `docs/PROTECTED_API_VALIDATION.md` rather than duplicating its attack definitions.

The final installed-system report must contain group-level results for:

```text
A positive protected use
B direct export/retrieval
C confused-deputy/destination exfiltration
D redirects/TLS/plain HTTP
E policy/cross-user isolation
F rotation/revocation/failure/recovery
G compatibility proxy/token (only if implemented)
```

The report may store safe credential refs and one-way fingerprints but never the raw reusable credential.

Any applicable subtest failure fails ISO-29. Provider-stop behavior is additionally reflected in ISO-35.

### 17.1 Imported evidence machine contract

For final harness adjudication, each reference listed under `imported_evidence.host_safe`, `imported_evidence.update`, or `imported_evidence.protected_api` points to a candidate-relative JSON file under the matching `imported/<group>/` directory.

A required imported result is considered passing only when the JSON contains:

```json
{"status":"pass"}
```

Additional group-specific fields are allowed in the imported file and remain owned by the producing subsystem. The validation report stores references rather than copying provider/update/host-safe evidence payloads.

An absent required group, missing referenced file, non-passing structured result, or unsafe path prevents `accepted` adjudication.

---

---

## 18. Failure, blocked and retry semantics

### 18.1 Fail

Use `fail` when:

- an assertion is false;
- an undocumented repair is needed;
- secret-bearing evidence is emitted;
- the candidate differs from the recorded SHA-256;
- the environment violates the required profile;
- a security negative test reaches the forbidden side effect.

### 18.2 Blocked

Use `blocked` only when an external prerequisite genuinely prevents execution, such as unavailable VMware infrastructure or an intentionally unavailable external provider fixture.

`blocked` is not a pass and cannot appear in an accepted report.

A missing implementation/package/service that the candidate is supposed to provide is normally **fail**, not blocked.

### 18.3 Retry

A test may be rerun after restoring its documented baseline. The report retains failed attempts or references them; it must not rewrite history so that only the final pass exists.

---

## 19. Acceptance-report schema

`validation-report.json` aggregates all results:

```json
{
  "schema_version": 1,
  "candidate_id": "...",
  "iso_filename": "...",
  "iso_sha256": "...",
  "source_revision": "...",
  "validation_authority_revision": "...",
  "reference_environment_ref": "environment/reference.json",
  "minimum_environment_ref": "environment/minimum.json",
  "started_at": "<UTC RFC3339>",
  "ended_at": "<UTC RFC3339>",
  "status": "accepted|rejected|incomplete",
  "counts": {"pass":38,"fail":0,"blocked":0,"not_run":0},
  "tests": [
    {"test_id":"ISO-01","status":"pass","result_ref":"tests/ISO-01/result.json"}
  ],
  "imported_evidence": {
    "host_safe": [],
    "update": [],
    "protected_api": []
  },
  "known_limitations_ref": "<release-authority path>",
  "redactions_ref": "redactions.json",
  "accepted_at": "<UTC RFC3339 or null>"
}
```

`validation-report.md` is a human-readable rendering of the same authoritative result and may add links/notes, but it must not contradict the JSON report.

---

## 20. Acceptance adjudication

The first ISO is accepted only when:

1. the candidate identity is immutable and verified;
2. ISO-01..ISO-38 all have `status=pass` as defined by `docs/ACCEPTANCE.md`;
3. ISO-37 uses the minimum profile and passes its limited required scope;
4. the complete blocking suite passes on the reference profile as represented by ISO-38;
5. no result relies on hidden manual repair;
6. the redaction/secret scan is clean;
7. all imported required update/protected-provider evidence passes;
8. known limitations are documented;
9. the report contains no `blocked` or `not_run` blocker;
10. all evidence identifies the same candidate ISO SHA-256 and required source/build inputs.

A useful partially passing candidate remains a development artifact but is not the accepted first ISO.

---

## 21. Pre-RC Linux resolution gates

Before the first candidate can run this matrix end-to-end, the supported Artix/native build and installed-system verification path must freeze the exact identities that remain intentionally unresolved, including as applicable:

The public native path has already proved real rootfs/livefs construction, both installed kernel lines, and `linux-lts` live-initramfs generation, but it has **not** yet produced a candidate ISO. Run `20260829T060019Z-3afadb080c36-dev-first-live` exposed the missing memtest boot asset; `memtest86+` is now tracked with a fail-closed source validator. Run `20260829T063320Z-658f8230fa32-dev-first-live` demonstrated rolling-repository drift. Run `20260829T072729Z-4164361b115a-dev-first-live` then proved that the new closure gate freshly synchronizes and hashes the three stable repository databases and resolves the exact graph before construction: 667 packages and about 1469.51 MiB of download were identified. It failed during the initial bulk prefetch on repeated mirror low-speed timeouts/TLS EOFs, so local-only validation and `buildiso` were never reached. The failure was correctly classified as `repository-closure` and cleanup passed. A0 preserves the complete resolved package graph in `repository-closure.json` before acquisition begins, with a regression proving a simulated later acquisition failure cannot revert the record to an empty package list. A1 additionally anchors repository identity to one healthy active HTTPS server from the official Artix mirrorlist: all three stable DBs must synchronize from that same server, the selection/attempt/config/DB-hash evidence is recorded, and the resulting per-run sync DB is made read-only before resolution. A2 now makes acquisition deterministic and bounded: exact unresolved identities are grouped under a 192 MiB byte ceiling, downloaded from the A1 anchor without dependency expansion, SHA-256 verified after every batch, and removed from the pending set only after verification; aggregate batch and pending progress is persisted even on failure. A3 now requires bounded anchor-first package-mirror failover with at most four HTTPS mirror attempts per batch, no repository refresh during acquisition, exact frozen `repository/package` targets, SHA validation after each attempt, removal of corrupt/incomplete payload before fallback, and a clean pacman transaction before the batch can pass. Passing `repository-closure.json` evidence must prove the attempt limit, distinct anchor-first mirror order, contiguous per-batch attempts, zero pending files and a successful terminal attempt for every downloaded batch. A4 now requires a deterministic cross-run persistent-cache audit after resolution and before batching: exact-hash complete archives are reusable, corrupt completed archives are removed, stale partials are removed, and missing identities remain pending. Passing closure evidence must prove that reused and pending filename sets are disjoint and together cover the entire frozen closure, that their counts agree, and that final package `cached_before` annotations match the audit. The currently retained failed-run cache contains 13 complete candidate archives and no `.part` files; reuse of any of them remains conditional on the next frozen SHA identities. A5 now requires current per-package state evidence (`pending`, `reused_verified`, `downloaded_verified`, `corrupt_removed`, or `failed`), exact resolved/verified/pending package and byte arithmetic, and per-attempt requested/verified/pending filename/byte sets with normalized failure classes. Current failed records must pass those internal consistency checks; PASS additionally requires every package verified and zero pending. Before candidate validation begins, broader repository-closure substage/cause classification remains; then a canonical native run must produce a passing closure record and prove the memtest bootfs correction through squashfs/ISO creation and checksum evidence.

- Artix repository/keyring/build snapshot and `artools` entry point;
- a passing checksum-bound `repository-closure.json` proving fresh stable repository database identities/hashes, exact resolved transitive package identities/SHA-256s, bounded/resumable acquisition with accurate per-package cache status, verified persistent-cache reuse or recovery, identical local-only re-resolution, and read-only cache handoff to `buildiso`;
- exact Calamares modules/package identities;
- installed dependency/order behavior for the already-locked base OpenRC service names/runlevels, plus `portusd` service/socket ownership;
- P14 Linux backend binding/VM behavior for the already-selected `maim`/`xdotool`/X11 inspection package set;
- installed PortusBrowser payload/native-messaging/Chromium extension identity for already-frozen source revision `c263c3997b4e6f2f7df5922e062a9e949e22f755`;
- exact typed `portus-privd` operations/resources used by ISO-18/19;
- exact pacman transaction resolver selected after live completeness proof;
- exact live-media device discovery/mount/chroot procedure;
- final runtime serialization of update evidence/result fields.

These are execution/verification gates, not permission to silently alter the locked first-ISO architecture.

---

## 22. Decisions locked by this document

1. ISO-01..ISO-38 are executed against one immutable candidate identified by ISO SHA-256.
2. Reference and minimum VMware profiles are separate evidence environments.
3. No blocking test is intended to remain manual-only.
4. Every pass requires objective evidence and a machine-readable `result.json`.
5. `blocked`/`not_run` are never acceptance passes.
6. Validation evidence uses a candidate-scoped `out/validation/<candidate-id>/` hierarchy.
7. Reusable secrets are forbidden from ordinary validation evidence.
8. Destructive tests restore a documented healthy state or a named clean snapshot before unrelated tests continue.
9. Hidden/manual repair outside the supported path fails the affected test.
10. Candidate byte/behavior changes require a new candidate; test-procedure changes invalidate affected evidence according to Section 11.
11. P17 host-safe hardening is imported as regression evidence but never substitutes for installed Linux/VM proof.
12. The update evidence packet from `docs/UPDATES_RECOVERY.md` Section 15.4 is imported into ISO-05/release validation.
13. The installed adversarial suite from `docs/PROTECTED_API_VALIDATION.md` is imported into ISO-27..29/35.
14. `validation-report.json` schema version 1 is the authoritative acceptance summary; Markdown is a rendering.
15. First-ISO acceptance requires all 38 blocking rows to pass with no hidden repair, secret leakage or candidate mismatch.
