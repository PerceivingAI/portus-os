# PortusOS Credential-Class Decisions

**Last reviewed:** 2026-08-29T01:24:50Z

**Status:** Authoritative credential-class classification direction; selected integrations still require implementation research

**Scope:** Codex authentication, browser credentials/sessions, SSH/signing keys, password managers, hardware-backed credentials, AUTH, and first-ISO protected-secret backup/recovery expectations

## 1. Purpose

`docs/SECRETS.md` defines the general protected-secret boundary. `docs/PROTECTED_API_PROVIDER.md` defines the concrete first protected API-key provider.

This document prevents other credential classes from being forced inaccurately into the API-provider architecture or mislabeled as protected merely because an application hides an export command.

Core rule:

> A credential is protected from Master Portus only when the actual Linux/application/provider boundary prevents the Master UID from recovering the reusable value. Otherwise it is Master-readable, regardless of UI labels.

---

## 2. Codex authentication

Codex runs as the Master Portus Linux user and therefore requires explicit classification.

Preferred direction:

1. if the current supported Codex authentication path can use `portus-apid` or a local compatibility proxy without receiving the upstream reusable credential, that path may be validated as protected;
2. if Codex requires possession of a reusable credential through a same-UID-readable file/keyring/environment/API, that credential is **Master-readable**;
3. PortusOS must not redesign the general secret boundary merely to preserve a protected label for Codex.

For the first ISO, a Master-readable Codex credential is acceptable if required by the supported Codex authentication mechanism. It is separate from the blocking protected API proof in `docs/ACCEPTANCE.md`.

A Codex credential must not be described as protected until negative testing proves the actual selected Codex version/path cannot export it through Master authority.

---

## 3. Browser credentials and sessions

The first-ISO normal Chromium/PortusBrowser profile runs as the Master user unless a later isolation design explicitly changes that.

Therefore:

> Browser credentials/session material stored in a profile readable by the Master UID are Master-readable in security classification.

PortusBrowser omitting cookie/password export commands does not by itself create a hard OS secret boundary.

The first ISO does not claim hard protection of normal Master-owned Chromium session material.

A future protected browser-authentication design must introduce a real non-exporting/profile/process boundary and pass the negative tests in `docs/SECRETS.md`.

---

## 4. SSH and signing keys

PortusOS distinguishes key purpose.

### 4.1 Agent/development keys

A development key created specifically for autonomous Master use may intentionally be Master-readable.

If stored in the Master's normal SSH/GPG files or exposed through a same-user agent with export/read access, it must be classified accordingly.

### 4.2 Owner/signing identity keys

An owner identity, release-signing key or similarly sensitive private key may be called protected only when Master receives an operation interface such as:

```text
sign approved payload
authenticate approved operation
```

without receiving the private key.

Preferred mechanisms include:

- hardware/FIDO/token-backed non-exporting keys;
- a dedicated protected signing/authentication agent with action-only API;
- another reviewed service identity/provider boundary.

A generic `ssh-agent`/GPG agent does not automatically prove non-export from Master if the underlying key file or agent APIs remain available to that same UID.

Protected SSH/signing is not required for the first ISO unless a first-ISO workflow explicitly depends on it.

---

## 5. Password managers

A password manager is not automatically a Portus protected provider.

Many password-manager CLIs intentionally return secret values. Such a CLI cannot be used as evidence that credentials are protected from Master Portus.

A password-manager integration may be classified protected only if it exposes a genuinely non-exporting action/fill/submit interface under an independent enforcement boundary and passes negative tests.

Until such a provider is selected and validated:

- password-manager support is not a first-ISO requirement;
- no password-manager vault is described as protected from Master merely because it is encrypted or locked behind a same-user CLI.

---

## 6. Hardware-backed credentials

Hardware-backed/FIDO/token credentials are a preferred future mechanism when they provide the natural property:

```text
USE/sign/authenticate succeeds
PRIVATE KEY EXPORT is unavailable
```

Hardware-backed status alone is not enough; the selected operation interface and recovery material still require policy, provenance and negative testing.

Hardware-token support remains optional for the first ISO.

---

## 7. AUTH first-ISO implementation

AUTH is the human-controlled provisioning surface, not a universal secret store.

The first ISO uses a root-run TTY utility:

```text
portus-auth
```

for protected credential provisioning/recovery interactions that need human secret entry.

For the protected API provider, the intended path is equivalent to:

```text
sudo portus-auth protected-api provision openai/main --provider openai
```

The reusable secret itself is **not** supplied as an argument.

`portus-auth`:

- requires administrator/root authorization for protected-store mutation;
- reads secret input directly from a TTY or another approved non-echo human input channel;
- does not echo the secret;
- does not write the secret to shell history, stdout, normal logs or generic Portus state;
- sends it directly to the relevant protected admin interface;
- clears/bounds in-memory secret buffers where the implementation permits;
- may display only safe reference/status metadata after success.

A graphical AUTH surface may be added later without changing this security boundary.

AUTH must not become a generic `show me the secret` interface.

---

## 8. Backups and first-ISO recovery

Ordinary Master-readable backups must exclude protected reusable credentials.

The first ISO does **not** require automatic backup of the protected credential store.

For the first ISO it is acceptable that loss of protected provider storage requires administrator reprovisioning of the external credential.

This is preferable to weakening the secret boundary with a Master-readable backup.

A future protected backup mechanism must use an administrator/recovery-controlled encryption/access boundary at least as strong as the live protected store and must not place recovery keys beside the data they protect.

Automated protected-secret backup remains deferred until that contract is defined.

---

## 9. First-ISO classification summary

| Credential class | First-ISO classification/direction |
| --- | --- |
| Reference API key in `portus-apid` | Protected; must pass `USE succeeds / EXPORT fails` |
| Codex credential | Protected only if actual supported path uses a proven non-export broker/provider; otherwise Master-readable |
| Normal Chromium/PortusBrowser profile/session | Master-readable under the normal Master-owned profile model |
| Agent-owned development SSH/signing key | May deliberately be Master-readable |
| Owner/release-signing identity | Protected only through non-exporting action/hardware boundary; not required by first ISO unless needed |
| Password-manager vault | No protected claim until action-only non-exporting provider is selected and tested |
| Hardware token | Optional preferred future protected mechanism |
| Protected-secret backup | Not automated in first ISO; reprovisioning is acceptable |
| AUTH | Root-run human TTY provisioning surface; does not expose reusable values back to Master |

---

## 10. Decisions locked by this document

1. Credential classification follows real retrievability by the Master UID, not application/UI labels.
2. Codex may use a protected broker/proxy when technically supported, but a same-user-readable Codex credential is honestly classified Master-readable.
3. A Master-owned Chromium profile/session is not claimed hard-protected in the first ISO.
4. Development SSH/signing keys may deliberately be Master-readable; sensitive owner/signing identities require a non-exporting action/hardware boundary to be called protected.
5. Password-manager encryption/CLI presence does not establish the Portus protected-secret property.
6. Hardware-backed non-exporting operation interfaces are preferred future protected identity mechanisms but are optional first-ISO features.
7. The first AUTH implementation is the root-run `portus-auth` TTY provisioning surface; protected secrets are never passed as CLI arguments or returned to Master.
8. Ordinary Master-readable backups exclude protected credentials.
9. Automated protected-secret backup is not required for the first ISO; administrator reprovisioning after loss is acceptable.
10. `portus-apid` remains API-credential-specific and is not generalized automatically to these other credential classes.

---

## 11. Remaining research/implementation work

P10 now implements the root-run `portus-auth` protected-API administration surface and its direct secret-safe admin IPC path. Provision/rotation secrets are absent from argv and the production input path requires a non-echo interactive TTY. Installed UID/socket/filesystem proof remains part of the P10 Linux acceptance track rather than an unresolved credential-class design.

Remaining credential-class work is:

- verify the current supported Codex authentication mechanisms against the protected/local-proxy option;
- select exact browser isolation design only if protected browser sessions become a requirement;
- select protected SSH/signing provider/hardware mechanism when needed;
- select a password-manager provider only if a non-exporting action contract is available;
- define future protected backup encryption/recovery format if automated protected-secret backup becomes required.

None of that work may silently reclassify a Master-readable credential as protected without negative-test evidence.
