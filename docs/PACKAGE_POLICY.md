# PortusOS Package and Supply-Chain Policy

**Last reviewed:** 2026-08-29T09:40:32Z
**Last updated:** 2026-08-29T09:40:32Z

**Status:** Authoritative for the locked first-ISO package-source boundary and top-level package inventory; candidate versions/repository lock, remaining profile decisions and candidate redistribution/installed evidence still require verification
**Target:** First accepted x86_64 VMware development ISO
**Build/install authority:** `docs/ISO_BUILD_INSTALLER.md`
**Capability inventory:** `docs/SYSTEM_CAPABILITIES.md`

## 1. Purpose and authority

This document owns the first-ISO package-source, software-supply-chain, package-profile, provenance and manifest policy.

The core source decision is locked:

> **PortusOS uses official Artix repositories as the normal source for selected third-party system software and does not use the AUR as part of the PortusOS first-ISO supply chain.**

If selected software is unavailable from the official Artix repositories, PortusOS does not automatically fall through to the AUR or to any other generic community source. That component becomes an explicit case-by-case sourcing decision.

This policy does not prevent PortusOS from shipping Portus-owned software built from this project or other owned Portus projects, and it does not reopen already explicit upstream-distribution decisions such as the pinned Codex installation contract in `CODEX_UPDATES.md`.

## 2. Official Artix baseline

For third-party Linux/system software selected for the supported first ISO, the default source is an official Artix repository appropriate to the selected Artix release/build environment.

The stable first-ISO artools path now locks the enabled official repositories to `system`, `world` and `galaxy`. Exact rolling package versions are captured per native run by the repository-closure evidence, while final candidate package identities and redistribution treatment remain candidate-specific. The baseline rules are:

- prefer the official Artix package when it provides the selected capability cleanly;
- use Artix package signatures/keyrings and normal package metadata as part of source verification;
- do not replace an available suitable official Artix package merely because another source is newer or more convenient;
- do not add software merely because it exists in an Artix repository; package inclusion remains capability-driven;
- the selected first-ISO top-level package names are now locked in `portusos-build/packages/packages.yaml`; future additions or candidate-version changes still require repository verification rather than assumption.

### 2.1 Boot-package evidence from the first native build

The first public native `buildiso` run exposed a concrete package-contract omission rather than an `artools` implementation defect. Locked `artools 0.39.1-1` attempted to copy `/run/artools/livefs/boot/memtest86+/memtest.bin` during bootfs assembly. Direct inspection of the official Artix `world` repository then established the required source and payload:

```text
package: memtest86+
version observed: 7.20-2
repository: official Artix world
package file: memtest86+-7.20-2-any.pkg.tar.zst
installed payload: /boot/memtest86+/memtest.bin
additional payload: /etc/grub.d/60_memtest86+
package dependencies: none
Artix package licence metadata: GPL2
```

Because the official package supplies the exact file expected by the locked `artools` path, PortusOS resolves this failure by tracking `memtest86+` as a first-ISO boot input. It is present in `portusos-build/packages/packages.yaml` and in the `packages-boot` section of the locked artools profile. The build validator fails closed if that boot package is later removed.

This is the preferred package-policy outcome: use the verified official Artix package that satisfies the build/runtime contract rather than creating a placeholder file or patching upstream tooling to ignore a required asset. The observed `7.20-2` identity is current build evidence, not yet the final candidate package lock; the exact candidate identity and licence/redistribution evidence still belong in generated `packages.lock.yaml` and the R8 audit.

### 2.2 Rolling-repository closure evidence

Canonical run `20260829T063320Z-658f8230fa32-dev-first-live` demonstrated that successful synchronization during preparation is not sufficient for a later native build against rolling Artix mirrors. The run cloned a prepared pacman database that resolved `libopenmpt 0.8.8-1`; when `make_livefs()` later fetched its Calamares/KDE dependency set, the current official `world` mirror carried `libopenmpt 0.8.9-1` instead. The stale identity therefore produced widespread 404 responses, mixed with unrelated slow-mirror errors, and the livefs transaction failed before bootfs construction.

The closure architecture is implemented, and canonical run `20260829T072729Z-4164361b115a-dev-first-live` proved its repository-synchronization and resolution boundary. The run freshly synchronized the locked stable `system`, `world` and `galaxy` repositories, captured their database SHA-256 values, and resolved an exact 667-package dependency graph requiring about 1469.51 MiB of downloads. It then failed during the initial bulk `pacman -Sw` acquisition because multiple mirrors hit low-speed timeouts or TLS EOFs. `buildiso` was never started, which is the intended fail-closed behavior.

This run narrows the remaining defect to acquisition resilience rather than closure identity. A0 preserves the resolved graph before acquisition begins. A1 now removes cross-mirror database ambiguity: active HTTPS servers are parsed from the official Artix mirrorlist in order, and a candidate is accepted only if pacman can synchronize `system`, `world` and `galaxy` from that same server into the empty per-run DB. The selected anchor, mirrorlist line, attempt results, generated single-server pacman-config hash and all three repository DB hashes are recorded. The per-run sync directory is then made read-only, so the anchored repository universe cannot be refreshed later in that run. A2 now acquires those frozen identities in deterministic 192 MiB byte-bounded batches. Existing hash-valid cache entries are excluded before batching; each network batch contains only exact `repository/package` targets from the frozen graph and disables dependency expansion; every completed batch archive is SHA-256 verified immediately; only verified identities are removed from the pending set; and aggregate completion/pending counts are persisted after each attempt. A single package larger than 192 MiB is allowed only as its own oversize batch. A3 now adds controlled package-mirror retry/failover without changing the frozen DB universe. Each batch uses the A1 anchor first and at most three distinct subsequent HTTPS entries after that anchor from the official mirrorlist; each fallback receives only the same frozen exact identities still needing acquisition, except that a nonzero pacman result after all bytes are SHA-valid forces the same identities through a later clean pacman transaction. No acquisition command refreshes repository metadata. SHA-valid completed archives remain reusable, while corrupt complete files and `.part` payload are removed before switching mirrors. A failed pacman transaction that nevertheless leaves SHA-valid archives must still be retried cleanly; detached signatures for those identities are removed before that online retry so stale signature cache state cannot be carried across every fallback mirror. A4 now makes that reuse policy executable across runs. Once a new run has frozen its exact closure, the persistent cache is audited filename-by-filename against the new repository-provided SHA-256 values. Exact-hash complete archives are recorded as reused; mismatched complete archives and associated stale signatures are removed; stale `.part`/`.sig.part` payload and orphan detached signatures are removed; and absent identities remain pending for bounded A2/A3 acquisition. Frozen package evidence also retains the repository-provided PGP-signature identity; where the frozen DB carries that signature, detached `.sig` files beside exact-hash reused archives are redundant and are removed instead of being trusted across runs. The currently retained failed-run cache contains 13 complete candidate archives—including both kernels, Chromium and several firmware/KDE payloads—and no `.part` files. They are not trusted because they came from a prior run: any candidate is reusable only if it matches the newly frozen closure. A5 now records that progress explicitly. Every frozen identity has one current state (`pending`, `reused_verified`, `downloaded_verified`, `corrupt_removed`, or `failed`) plus verification/acquisition flags and last attempt context. Aggregate resolved/verified/pending package and byte totals and per-state totals are persisted after cache audit and mirror attempts; mirror attempts additionally record requested/verified/pending filenames and bytes, result and normalized failure class. A current failed closure therefore preserves real partial progress instead of collapsing to zero. Cache-level verified/pending/downloaded counters are updated with each mirror-attempt evidence write, and current failed evidence is accepted only when its attempt partitions and each attempted package's final batch/attempt/mirror/failure class agree. A6 now records structured closure failure diagnosis without weakening the outer `repository-closure` boundary: the exact substage and normalized cause are retained, including timeout, TLS EOF, HTTP 404, hash mismatch, signature failure, DNS/connection failures and resolution/local-validation mismatches where detectable. Acquisition pacman stderr is replayed to the terminal while also being captured for classification. Attempt context is terminal-failure-only: a mirror/anchor failure that was later recovered cannot contaminate the diagnosis of a subsequent error, and current acquisition cause/context must agree with the terminal A5 failure class. Generic `target not found` output is not HTTP 404. SIGINT during an active repository closure preserves the outer `repository-closure` stage with cause `interrupted`; an interrupt after closure has passed remains `native-iso-build`. Historical string-only failure records remain valid history; current A6 failures are semantically as well as structurally validated. A7 now enforces the final consumption boundary: anchored repository DBs and their separate snapshot are hard-frozen as read-only bind mounts and are not exposed through a `file://` repository until every exact package archive is SHA-valid and the per-package ledger reports zero pending identities. The constructed local repository is then frozen read-only, and the locked stable pacman config is rewritten to the exact local `file://` source, SHA-bound and frozen read-only before local-only pacman independently re-resolves and validates it. After that validation every package archive is rehashed, cache ownership is restored, and both the backing cache source and chroot cache bind are made read-only. A live pre-exec gate rechecks the pacman-config hash, package/DB hashes, exact repository links and those mounts immediately before permitting `buildiso`, and the outer ledger requires the corresponding evidence. Regression coverage includes mirror failover, true failed-batch reconciliation/resume, cache reuse/corrupt cleanup, frozen-DB immutability, accurate partial evidence, post-validation tamper/mount-loss rejection and direct no-launch proof with one unresolved package. Retrying arbitrary mirrors while changing repository state remains unsupported.

## 3. AUR boundary

The AUR is not a PortusOS first-ISO package source.

Therefore the supported first-ISO architecture:

- has no required AUR packages;
- has no optional PortusOS profile whose installation depends on AUR packages;
- does not use AUR as an automatic fallback when an Artix package is absent;
- does not require or ship `yay` or another AUR helper as part of the PortusOS baseline or supported package profiles;
- does not run AUR builds as part of the authoritative `artools` ISO build or Calamares installation path.

This is a PortusOS distribution/support boundary, not an artificial Linux capability restriction. An administrator may later modify an installed machine outside the supported baseline, but such modifications do not become part of the PortusOS package contract merely because Linux permits them.

## 4. Missing-software case review

When a selected required or optional PortusOS component is not available from the official Artix repositories, stop and evaluate that component individually.

There is deliberately no universal second source.

A case review must determine at minimum:

1. whether the capability is actually required for the first ISO/profile;
2. whether an official Artix alternative can satisfy the capability cleanly;
3. the upstream/vendor source and ownership model of the desired component;
4. whether source or binary distribution is appropriate;
5. version/pinning/update strategy;
6. cryptographic verification or other provenance mechanism;
7. licence and redistribution rights;
8. transitive dependency and build-chain implications;
9. OpenRC/systemd and general Artix compatibility;
10. security and secret-boundary implications;
11. who owns installation and updates;
12. failure/degradation behavior when the component cannot be obtained or updated;
13. whether inclusion should be local-package staging, a pinned standalone artifact, a reviewed upstream repository, or another explicitly approved mechanism.

The result must be documented before the component enters the supported PortusOS manifest. A case-specific approval does not create a precedent or generic fallback for unrelated software.

## 5. Portus-owned software

Portus-owned first-party components are not third-party package-source exceptions merely because they are absent from Artix repositories.

PortusOS owns the build/staging/install contract for components such as the Portus runtime/CLI/services and for separately owned Portus projects included by first-ISO scope.

Their source, build, version, integrity and installation metadata must still be explicit and reproducible, and they should be staged into the `artools`/Calamares path through the machine-readable package/build contracts rather than copied ad hoc during acceptance.

## 6. Already explicit external components

Some selected software may already have an authoritative non-Artix distribution contract elsewhere in the repository.

For example, `CODEX_UPDATES.md` owns the pinned standalone Codex installation policy. Such a contract is a component-specific decision and must satisfy this document's provenance, versioning, verification, licensing and update requirements; it does not weaken the no-AUR rule or create a general external-source preference.

Any additional non-Artix component must receive equivalent explicit treatment before becoming supported.

## 7. Machine-readable package inventory

The first ISO must eventually derive from one inspectable machine-readable installation contract.

At minimum it must distinguish:

```text
required Artix base
VMware profile
first-ISO required browser/authentication profile
hardware-selected packages
Portus-owned/local packages
explicitly approved external components
optional supported profiles
deferred components
```

Each entry must record enough metadata to determine:

- capability/role;
- source class;
- package/artifact identity;
- version or resolution policy;
- verification method;
- installation owner;
- required/optional/profile classification;
- licence/redistribution status where relevant;
- update owner;
- failure behavior.

The manifest must not contain an `aur` source class for the supported first-ISO package contract.

This contract is materialized at `portusos-build/packages/packages.yaml` with the exact finite source classes `official-artix`, `portus-owned`, `approved-external`, `validation-only`, and `hardware-selected`. Verification/install/update/failure policy is recorded once per source class rather than duplicated across entries; package entries own role/profile/source selection, while public redistribution remains explicitly pending the OSS audit where it has not yet been approved. `aur` is absent from both the source schema and Rust enum. The first real isolated-Artix L2 pass has now locked the selected top-level official-Artix package names and OpenRC integration package names for the first ISO. The resolved `packages.lock.yaml` remains generated evidence: it must hash-link back to `packages.yaml` and preserve one logical PortusOS role with one-or-more exact concrete artifacts beneath it. Every artifact requires version, verification and a non-empty `licenses` set, with repository identity required for official-Artix artifacts, so the exact candidate graph can feed the R8 redistribution review and SPDX inventory rather than reconstructing licence facts later.

## 8. Optional profiles

Optional profiles may still exist for supported software such as browsers, development tools, desktop conveniences or containers, but they follow the same source boundary:

- official Artix package when selected and available;
- Portus-owned package where the software is Portus-owned;
- otherwise explicit per-component review before support.

An optional profile is not permission to bypass the supply-chain policy.

The exact optional profile contents remain open and must be capability-driven.

## 9. Updates and package ownership

Package update ownership must follow source ownership:

- official Artix packages update through the selected Artix/pacman path;
- Portus-owned packages update through the PortusOS-owned package/release path;
- explicitly approved external components follow their documented component-specific update path.

A supported update process must not silently replace a component with a different source class.

Cross-component compatibility, preflight, interrupted-update handling, post-update validation and fallback/repair are owned by `docs/UPDATES_RECOVERY.md`.

## 10. Supply-chain invariants

The first-ISO package/build system must preserve these invariants:

- no AUR dependency in the authoritative build/install path;
- no undocumented external repository or download endpoint;
- no unpinned or unverified external artifact where the component contract requires pinning/verification;
- no developer-machine convenience package silently promoted into the supported image;
- no manual post-install copying used to satisfy a blocking first-ISO component;
- package/source provenance remains inspectable from repository-owned configuration and build metadata;
- one native build must not mix repository databases from one rolling state with package files from another; the supported path requires a passing run-owned `repository-closure.json` before `buildiso` output can be accepted;
- verified warm-cache reuse is identity-based: exact package filename and repository SHA-256 must match, and the native cache is read-only once construction begins;
- the repository database used to resolve a native run must be coherent with the package files consumed by that same run; stale prepared pacman metadata may not be paired with newer rolling-mirror contents;
- the exact transitive native package closure must be verified or prefetched before expensive ISO construction is allowed to depend on it;
- a missing package fails clearly or triggers a documented component decision rather than silently changing source.

## 11. Remaining decisions and verification

The following remain open:

- implementation and native proof of the required per-run repository metadata refresh/freeze, database identity capture, exact transitive package closure and package-file availability/prefetch gate;
- final candidate repository/mirror snapshot identity and exact package versions;
- generation of the now-defined multi-artifact `portusos-build/packages/packages.lock.yaml`, followed by verification against `portusos-build/schemas/package-lock.schema.json`;
- candidate/install proof of the implemented Portus local-package and frozen external-component staging path;
- case-by-case decisions for any newly selected software that Artix does not provide and is not already covered by an explicit component contract;
- exact optional browser/development/desktop/container profile contents;
- licence and redistribution verification per selected candidate component;
- exact package compatibility metadata and handoff into the locked managed-update workflow in `docs/UPDATES_RECOVERY.md`.

## 12. First-ISO acceptance implications

Before the first ISO is accepted, validation must be able to show that:

- every required installed component has an inspectable source classification;
- normal third-party system packages resolve from the selected official Artix repositories;
- the supported build/install path does not invoke AUR tooling or depend on AUR packages;
- every non-Artix component in the candidate has an explicit component-specific source/verification/update contract;
- Portus-owned packages are reproducibly staged through the supported build/install path;
- no undocumented package source is required to install and boot the reference VM.

Exact evidence and commands belong in `docs/VALIDATION.md` once created.

## 13. Decisions locked by this document

1. Official Artix repositories are the normal/default source for selected third-party first-ISO system software.
2. The AUR is not part of the supported PortusOS first-ISO supply chain.
3. PortusOS does not require or ship an AUR helper as part of its supported baseline/profiles.
4. Absence from Artix does not select an automatic fallback source; the component is evaluated individually.
5. A case-specific external source requires explicit provenance, verification, licensing, compatibility, installation, update and failure-behavior decisions before support.
6. One approved exception does not establish a generic second-source precedence.
7. Portus-owned software uses PortusOS-owned build/staging contracts and is not treated as a third-party missing-package exception.
8. Already explicit component-specific external distribution contracts, such as the pinned Codex path, remain valid subject to this supply-chain policy.
9. The supported machine-readable first-ISO manifest contains no AUR source class.
