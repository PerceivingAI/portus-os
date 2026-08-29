# Release public keys

This directory contains public verification material only.

The active public release key will be stored as:

```text
portusos-release.allowed_signers
```

using OpenSSH `allowed_signers` format with signer identity:

```text
portusos-release
```

The corresponding private Ed25519 key must never be stored in this repository, build roots, ISO contents, CI artifacts, or release evidence.

The public key is intentionally absent until the owner performs the real release-key ceremony. After generating the external encrypted Ed25519 keypair, use `scripts/release/prepare_public_key.py --public-key <external.pub> --output release/keys/portusos-release.allowed_signers` and record the printed SHA-256 fingerprint in release metadata. Historical public keys should be retained so historical release signatures remain independently verifiable.
