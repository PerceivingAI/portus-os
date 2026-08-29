# PortusOS OSS preparation tools

These tools implement the public repository, private-data, dependency, provenance and redistribution audit surface. Generated evidence goes under `artifacts/oss/` and is ignored by Git.

## Repository/private-data audit

```bash
python scripts/oss/audit_repo.py --scope current --strict
python scripts/oss/audit_repo.py --scope history --strict
```

The scanner reports finding type and affected path/object only. It deliberately does not print matched secret/private values.

`current` scans tracked files plus risky non-ignored untracked names and inventories ignored roots. `history` scans unique reachable Git blobs and historical path names. Large/binary blobs are classified without OCR or unsafe value extraction.

A clean automated scan does not replace human review of intentional attribution, screenshots, unusual archives, repository-host settings, or candidate-specific release artifacts.

## Rust dependency inventory

```bash
python scripts/oss/dependency_inventory.py --check
```

This writes:

```text
artifacts/oss/dependency-inventory.json
artifacts/oss/dependency-inventory.md
```

It records the locked Cargo graph without writing developer-local Cargo paths, checks external licence metadata, identifies source kinds and Git dependencies, and requires every PortusOS workspace crate to declare the selected `Apache-2.0` first-party licence.

## Dependency security/source policy

The `oss` category also runs:

```bash
cargo audit -D warnings
cargo deny check advisories bans licenses sources
```

The currently validated tool versions are `cargo-audit 0.22.2` and `cargo-deny 0.20.2`. `deny.toml` fails closed on yanked crates, unknown registries/Git sources, and licence expressions outside the reviewed permissive allowlist, while treating duplicate versions as review warnings. Internal path-only workspace dependencies are allowed by cargo-deny; `dependency_inventory.py --check` separately rejects any external workspace dependency that actually uses a `*` version requirement. The allowlist is intentionally narrower than every theoretically Apache-compatible licence and must be reviewed before adding a new licence family.

## Third-party component source inventory

Human summary:

```text
THIRD_PARTY.md
```

Machine source:

```text
oss/third-party-components.json
```

Known upstream licence facts may be recorded before exact candidate pins exist. Redistribution remains pending until the exact release asset/package/revision and required notices are audited.

## Release evidence

`scripts/oss/release_evidence.py` generates the preparatory/public SPDX and Portus provenance artifacts from a frozen candidate. Its `--release-ready` mode fails closed until the first-party licence, candidate package inventory, accepted validation report and other public-release prerequisites exist.

## Public release policy

Resolved current-host public policy lives in:

```text
docs/PUBLIC_RELEASE_POLICY.md
oss/release-policy.json
oss/schemas/release-policy.schema.json
```

The public policy fixes the first version/maturity, signing mechanism and key-custody model, support/update posture, public issue policy, private security-reporting mechanism, and Apache-2.0 first-party licence. The real signing key/public key and GitHub private-reporting activation remain operational publication gates until they are completed for an actual release.

Release signature tooling lives under `scripts/release/`; its tests use only temporary external keys.

## Test category

```bash
powershell -File scripts/test.ps1 oss
# or on Linux
./scripts/test.sh oss
```

This category runs the OSS tooling unit tests, dependency check, and strict current/history scans. It proves the tooling and repository scan, not candidate-specific redistribution, host-security, signature-key, or final publication completion.
