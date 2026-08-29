# PortusOS Public Release Policy

**Status:** Authoritative public release policy.
**Machine-readable source:** `oss/release-policy.json`
**Technical release authority:** `docs/RELEASE.md`

## First public release

The first public release is:

```text
PortusOS 0.1.0 Technical Preview
```

It is an x86_64 VMware-first technical preview. The version and label do not claim production readiness, broad bare-metal support, Secure Boot support, LTS status, or any capability not demonstrated by the accepted release candidate.


## Repository governance and issues

The public repository host is GitHub.

External pull requests and code contributions are not accepted. `CONTRIBUTING.md` remains intentionally absent.

GitHub Issues are enabled for:

- reproducible PortusOS bugs;
- documentation defects.

GitHub Issues are not a general support/helpdesk channel and must not be used for vulnerabilities, credentials, exploit details, or other sensitive security material.

Feature/product requests may be recorded by maintainers at their discretion, but the public issue contract does not promise a feature-request intake or roadmap process.

## Security reporting

The selected private security-reporting mechanism is **GitHub Private Vulnerability Reporting / repository security advisories**.

Before the repository/public release gate can pass:

1. the repository must be public on GitHub;
2. Private Vulnerability Reporting must be enabled in repository security settings;
3. administrator/security-manager notifications must be configured and reviewed;
4. `oss/release-policy.json` must set the activation state to `active` and `notifications_reviewed` to `true` only after that host-side verification.

Security reports must not be opened as public issues.

## Release authenticity


Public releases authenticate the canonical `SHA256SUMS` file using OpenSSH SSHSIG with an Ed25519 release key:

```text
algorithm        Ed25519
signed object    SHA256SUMS
SSHSIG namespace portusos-release
signer identity  portusos-release
signature        SHA256SUMS.sig
```

The private release-signing key:

- is generated in a deliberate owner-controlled key ceremony;
- is encrypted at rest;
- remains outside this repository;
- remains outside ISO/build roots and ordinary development workspaces;
- is not passed through environment variables, command-line secret values, CI logs, or release metadata;
- is not required to build or test PortusOS;
- is used only to sign release checksum manifests.

The public verification key is published through:

```text
release/keys/portusos-release.allowed_signers
```

The public key is not created until the real signing-key ceremony. Historical public keys remain available for verification of historical releases even after retirement. A compromised key is marked revoked through the release/security documentation; already-published artifacts are not silently re-signed in place.

`scripts/release/sign_checksums.py` and `scripts/release/verify_release.py` implement the signing/verification contract. The signing tool rejects private keys located inside the PortusOS repository.

## Support and updates


Support is **best effort** and applies only to the current public final release.

A public release ceases to be supported when it is superseded by a newer public final release or explicitly withdrawn. Release candidates are validation artifacts and are not supported releases.

PortusOS makes no LTS promise, response-time SLA, fix-time SLA, or guaranteed update cadence for the first public release. Updates and security fixes may be issued when maintainers determine they are appropriate. A serious defect may cause a release to be withdrawn rather than patched in place.

This policy does not expand the validated hardware/software scope. Support claims remain bounded by `docs/ACCEPTANCE.md`, the release validation report, and `KNOWN_LIMITATIONS.md`.

## First-party licence

The PortusOS-owned first-party source licence is **Apache-2.0**.

```text
PortusOS-owned source: Apache-2.0
Licence text:          LICENSE
Cargo SPDX metadata:   Apache-2.0
```

Third-party software, packages, firmware and copied assets retain their own licences, notices and redistribution obligations. Aggregation into the PortusOS ISO does not silently relicense third-party components or make their licence PortusOS's first-party licence.

This choice does not grant trademark rights beyond the Apache-2.0 terms.
