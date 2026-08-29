---
name: protected-api
description: Use PortusOS protected API credentials through the non-exporting `portus-api` client. Use when an agent needs to list safe credential references, inspect one reference, check provider health, or perform an approved provider operation without obtaining the reusable credential value.
---

# Protected API

Use `portus-api` for authenticated API operations backed by protected credentials.

## Core rule

A protected credential is **use-only**. Do not try to retrieve, print, copy, export, place in an environment variable, or pass the reusable credential to another process.

Prefer structured output:

```text
portus-api --json credential list
portus-api --json credential show <credential-ref>
portus-api --json health
```

Perform an approved operation by sending only the provider request body:

```text
portus-api --json request <credential-ref> <operation> --input <path>
```

or pipe the request body to stdin and leave `--input` at its default `-`.

The currently shipped reference operation is defined by the administrator-controlled provider definition under `/etc/portus/protected-api/providers.d/`. Discover or use only credential references and operation IDs the provider exposes; do not invent destinations, headers, or credential values.

## Failure handling

- Permission/policy rejection: report the denial; do not bypass it with direct secret-file access or a plaintext credential fallback.
- Provider unavailable/degraded: use `portus-api --json health` and PortusOS diagnostics. Do not copy a protected credential into ordinary config to keep working.
- Unknown credential or operation: rediscover the safe reference/operation contract rather than guessing.
- Administrative provisioning, rotation, revocation, or deletion: these are owner/root workflows through `portus-auth`, not Master-facing `portus-api` commands.

Provider response bodies are operation results, not proof that the reusable credential became readable. Keep returned application data under the normal task/artifact/confidentiality rules.
