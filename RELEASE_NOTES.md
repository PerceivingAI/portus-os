# PortusOS 0.1.0 Technical Preview — Release Notes

**Status:** Pre-release public notes. These notes do not claim that a final ISO has been accepted or published.

PortusOS 0.1.0 is the first public Technical Preview of a privacy-focused, agent-operated Linux distribution built on Artix Linux and OpenRC.

## Highlights

- Codex-first agent operating model with a non-root Master Portus user.
- Rust-native Portus control plane and `portus-os` CLI.
- Policy-governed privilege boundary instead of unrestricted root access.
- Protected reusable API-credential provider designed around non-exportable use.
- Durable task, state, capability, System Index, artifact, health, event and audit subsystems.
- X11 + i3 + Alacritty graphical baseline.
- Chromium-based authentication/browser path and PortusBrowser integration.
- Optional Portus MCP + OpenAI `tunnel-client` remote connection path.
- GRUB + LUKS2 + LVM encrypted installation design.
- `linux-lts` normal/live kernel with `linux` installed as an alternate kernel.
- Deterministic source/build contracts, candidate validation, provenance and signed-checksum release tooling.

## Target environment

The first release is x86_64 and VMware-first. See `docs/ACCEPTANCE.md` and `KNOWN_LIMITATIONS.md` for the exact support boundary.

## Build and validation

The repository contains the public build harness and acceptance tooling. A successful local build is not equivalent to an accepted release. Final publication requires the exact ISO candidate to pass the complete validation and release gates documented in:

- `docs/ACCEPTANCE.md`;
- `docs/VALIDATION.md`;
- `docs/RELEASE.md`;
- `docs/PUBLIC_RELEASE_POLICY.md`.

## Licence and contributions

PortusOS-owned source is Apache-2.0. Third-party software retains its own licences and obligations.

External pull requests and code contributions are not accepted. Public issues are intended for reproducible bugs and documentation defects after publication. Security reports use the private process in `SECURITY.md`.