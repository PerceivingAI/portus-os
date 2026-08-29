# Security Policy

## Status

PortusOS is currently pre-release. There is no supported public PortusOS release yet.

The public security-reporting mechanism is selected but cannot be operational until the repository is public: PortusOS will use **GitHub Private Vulnerability Reporting / repository security advisories** for private vulnerability reports. Public release remains blocked until that repository setting is enabled and maintainer/security notifications are reviewed.

## Reporting a vulnerability

Do **not** publish suspected vulnerabilities, reusable credentials, exploit details, private keys, or sensitive logs in a public GitHub Issue or discussion.

Once the public repository enables Private Vulnerability Reporting, use the repository's Security/Advisories **Report a vulnerability** flow. That report is private to the repository maintainers/security managers rather than an ordinary public issue.

Before the public repository activates that feature there is intentionally no alternative public security mailbox or placeholder contact. Do not invent an email address or use an unmonitored channel merely to satisfy a release checklist.

## What to include

A private report should include, when available:

- affected PortusOS release/candidate and SHA-256;
- affected component and version/revision;
- concise vulnerability description and security impact;
- reproducible steps or proof-of-concept that does not unnecessarily expose third-party data;
- whether a reusable credential may have been exposed;
- relevant logs/evidence after removing unrelated private information and reusable secrets;
- any known workaround or mitigation.

## Supported versions

Before the first public final release, no version is advertised as security-supported.

After publication, support is best effort and applies only to the **current public final release**. A release ceases to be supported when superseded by a newer public final release or explicitly withdrawn. Release candidates are validation artifacts and are not supported releases.

PortusOS makes no LTS promise, response-time SLA, fix-time SLA, or guaranteed update cadence for the first public release.

## Security scope

Security-sensitive PortusOS surfaces include, among others:

- privilege mediation and policy enforcement;
- protected reusable credentials and `USE succeeds / EXPORT fails` behavior;
- local IPC identity and Unix peer-credential boundaries;
- cross-user isolation;
- update/install/recovery paths;
- PortusBrowser integration and native messaging;
- release provenance, package/source integrity, and signing/verification material.

The supported hardware/software scope is limited to what the release validation report and `KNOWN_LIMITATIONS.md` actually demonstrate.

## Credential exposure

If a reusable credential is suspected to have been committed, published, logged, or included in an artifact, treat it as exposed. Rotate or revoke it promptly. Removing the visible value or rewriting Git history is not by itself sufficient remediation.

Do not include the reusable credential itself in audit reports, issues, validation evidence, or release notes.

## Ordinary bugs and support

Public GitHub Issues are for reproducible PortusOS bugs and documentation defects. They are not a security-reporting path and are not a guaranteed helpdesk/support channel.

PortusOS does not accept external pull requests or code contributions. A vulnerability or bug report does not imply that a patch submission will be accepted.

## Disclosure and coordination

The project will coordinate security fixes and publication according to the actual severity and release state. This policy intentionally makes no response-time or fix-time SLA.

Public disclosure should avoid exposing users to unnecessary risk and should normally occur only after an appropriate fix, mitigation, withdrawal, or replacement release is available.

## Operational public-release gate

Before OSS-05 can pass, verify all of the following on the actual public GitHub repository:

```text
Private Vulnerability Reporting enabled
Report a vulnerability flow visible
maintainer/security-manager notifications reviewed
oss/release-policy.json activation_status = active
oss/release-policy.json notifications_reviewed = true
```

The policy decision is complete; the host-side activation remains a publication-time operational gate.
