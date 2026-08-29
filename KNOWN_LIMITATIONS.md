# PortusOS 0.1.0 Technical Preview — Known Limitations

PortusOS 0.1.0 is a **Technical Preview**. This file defines the public support boundary and must be read together with the release validation report for the exact published candidate.

## Platform scope

- x86_64 only.
- VMware-first validation.
- UEFI boot is the supported first-release path.
- Broad bare-metal compatibility is not claimed.
- Secure Boot is not part of the first-release acceptance contract.
- ARM and other CPU architectures are not supported by the first release.

## Hardware and graphics

- X11 + i3 is the supported graphical baseline.
- A display manager and compositor are not required.
- VMware 3D acceleration is not required.
- Hardware outside the accepted VMware fixture may work but is not covered unless explicitly demonstrated in release evidence.

## Storage and recovery

- The supported installed storage topology is the documented GRUB + unencrypted `/boot` + LUKS2 + LVM + ext4 design.
- Automatic whole-system rollback is not provided.
- General backup automation is not part of the first release.
- Recovery depends on owner/root access and the documented live-ISO/equivalent recovery path.

## Agent/runtime scope

- Master Portus is a broad **non-root** Linux user. Root remains a separate administrator/recovery authority.
- Full reboot-resumable long-running Portus tasks are not part of the first release.
- Sophisticated remote approval workflows are deferred.
- PortusWindow is not part of the first release.
- GUI automation is a fallback and may be less reliable than structured CLI/API/provider controls.

## Remote access

- Local operation does not require a remote service.
- Portus MCP + OpenAI `tunnel-client` support is optional.
- General-purpose messaging control is not part of the first release.
- General external artifact-delivery adapters are deferred.

## Updates and support

- Best-effort support applies only to the current public final release.
- There is no LTS commitment.
- There is no response-time or fix-time SLA.
- There is no guaranteed update cadence.
- A serious defect may cause a release to be withdrawn instead of patched in place.

## Release maturity

Until the exact public candidate completes `docs/ACCEPTANCE.md`, `docs/VALIDATION.md`, and `docs/RELEASE.md`, repository source or a locally generated ISO must not be represented as an accepted PortusOS release.

The release validation report is authoritative for what was actually demonstrated on the published candidate.