# PortusOS Install and Staging Sources

This directory contains the host-safe Portus-owned package/install staging contract used by the ISO/package builder and tests.

It is deliberately **not** a replacement package manager and it does not claim that OpenRC, ownership, service users, runlevels, or Artix package hooks have been validated on Windows.

## Authoritative source

`install.toml` is the machine-readable Portus install staging contract. The `portus-install` Rust crate parses and validates it, validates the referenced provider/policy/skill/OpenRC-template sources, and can stage the first-party Portus payload into an arbitrary root supplied by tests or a later ISO/package builder.

The staging contract covers:

- the eight current first-party Portus executables under `/usr/bin`;
- `/etc/portus` provider/policy/protected-provider configuration;
- the protected API machine-wide Codex skill under `/etc/codex/skills/protected-api`;
- package-owned policy, PortusBrowser integration, and OpenRC template material under `/usr/share/portus`;
- persistent `/var/lib/portus` and `/var/log/portus` paths;
- transient `/run/portus` paths;
- locked `portus-privd` and `portus-apid` identities/groups;
- explicitly unresolved `portusd`, shared-audit, and parent-runtime ownership;
- PortusBrowser external package requirements from P15;
- install/reinstall/uninstall preservation rules.

## Safety boundary

Host-safe staging always targets a caller-supplied root. It never writes the development machine's real `/etc`, `/usr`, `/var`, or `/run`.

The stager:

- requires real non-empty binary payload files supplied by the caller rather than fabricating executables;
- validates static provider, protected-provider, policy, PortusBrowser, skill, and OpenRC-template sources before staging;
- rejects traversal and static files outside approved installed prefixes;
- rejects common reusable-secret material in static install sources;
- never creates `/var/lib/portus/protected-api/credentials.db`;
- treats `/etc` material as administrator configuration and preserves local modifications on reinstall;
- preserves modified files rather than deleting them during uninstall;
- never deletes `/var/lib/portus`, `/var/log/portus`, `/workspace`, or policy subjects as an uninstall side effect;
- does not mutate Portus provider history/state merely because package files are added or removed.

## OpenRC boundary

`openrc/*.in` are **package-owned templates only**. They are staged under `/usr/share/portus/openrc/templates`, not `/etc/init.d`.

Every unresolved template contains `P16-LINUX-RESOLUTION-REQUIRED` as an internal fail-closed marker. The host-safe validator rejects templates that prematurely add a `depend()` function, supervisor/respawn settings, or a guessed `portusd` service identity. The supported Artix build/validation path must render and validate the real `/etc/init.d` scripts only after service identity, dependencies, runlevel placement, supervision, restart budgets, and health behavior have been verified.

The PortusBrowser integration contract keeps its Broker provider-owned/native-host-on-demand, so there is intentionally no PortusBrowser OpenRC template.

## Ownership/modes

`install.toml` distinguishes `locked` declarations from `linux-verified` declarations. Windows staging creates directory/file structure but does not pretend that Windows filesystem metadata proves Linux `chown`/mode behavior.

A manifest containing any unresolved `linux-verified` item reports `release_ready() == false`. Those items become release-resolved only after the supported Artix build/package path applies and verifies the declared ownership and modes.

## Reinstall and uninstall

Same-version reinstall is deterministic:

- identical package/config files are unchanged;
- package-owned `/usr/share` files can be replaced from the package source;
- modified administrator configuration is preserved.

Uninstall removes only files that still exactly match the package source. Modified configuration and persistent state/log/subject data are preserved. Native Artix packaging may use pacman backup semantics, but it must preserve these install safety invariants.
