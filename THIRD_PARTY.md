# Third-Party and Redistribution Inventory

This file is the human-readable entry point for the PortusOS third-party/licence/redistribution audit.

Machine-maintained source inventory:

```text
oss/third-party-components.json
```

Generated locked Rust dependency inventory:

```text
python scripts/oss/dependency_inventory.py --check
```

Generated audit evidence is written under `artifacts/oss/` and is not source authority.

## Current preparatory status

PortusOS-owned source is licensed under **Apache-2.0**, with the canonical licence text at root `LICENSE` and Cargo workspace packages inheriting `license = "Apache-2.0"`.

Current external Rust dependency metadata is mechanically inspectable from `Cargo.lock`/`cargo metadata`. The preparatory scan found no Git dependencies and no external locked crate lacking licence metadata. `cargo-deny` enforces the reviewed dependency licence allowlist; third-party components retain their own upstream licences and obligations.

Major non-Cargo release components are tracked explicitly in `oss/third-party-components.json`. Known upstream licence facts may be recorded before the release pin exists, but **redistribution approval remains candidate-specific** where the exact Artix package, release asset, firmware set, copied notice set, or immutable component revision is not yet frozen.

In particular:

- Codex upstream currently states Apache-2.0; the exact L3 standalone release asset and its notices still require release review.
- Portus MCP is a separate Portus-owned Apache-2.0 repository now pinned at `a963c56a72ca106cbac3b848256bb393fb9dc2da`; its exact npm dependency/licence/notices payload remains candidate review evidence.
- OpenAI `tunnel-client` is Apache-2.0 and is pinned to v0.0.13 for the first-ISO path; the machine contract records the official Linux x86_64 zip digest plus the matching upstream licence/SPDX/Sigstore provenance asset names for later candidate capture.
- Calamares upstream source uses GPL-3.0-or-later identifiers; the selected Artix package and shipped notices remain L2/R8 evidence.
- Chromium is treated as a multi-licence codebase; the exact Artix package-provided third-party notices are required rather than collapsing it to one licence label.
- Artix runtime packages, firmware/microcode and any VMware guest package are finalized from the generated L2 package lock and package metadata.
- PortusBrowser is Portus-owned but comes from a separate repository; its current root licence is MIT. The exact clean source revision and component notices/licence state must still be rechecked at freeze.

## Release rule

`license known` is not equivalent to `redistribution approved`.

A public candidate is blocked until every component actually distributed by that candidate has:

```text
exact identity/version or source revision
source/provenance evidence
licence information
redistribution treatment
required notices/attribution/source obligations
candidate-specific audit status
```

Unknown or pending redistribution treatment is an unresolved blocker, not implicit permission.

The final public notice artifact (for example `THIRD_PARTY_NOTICES.md`) is generated/finalized only after the exact release payload is frozen. This preparatory inventory does not pretend to be that final notice bundle.
