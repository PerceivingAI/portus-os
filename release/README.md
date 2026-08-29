# PortusOS release verification

Public PortusOS releases authenticate `SHA256SUMS` with an OpenSSH SSHSIG Ed25519 signature. `scripts/release/prepare_public_key.py` converts the owner-selected Ed25519 public key into the canonical allowed-signers file and prints the SHA-256 fingerprint that must be recorded in public release metadata.

Canonical files:

```text
SHA256SUMS
SHA256SUMS.sig
release/keys/portusos-release.allowed_signers
```

Verify a downloaded release bundle from its directory with:

```bash
python scripts/release/verify_release.py \
  --sha256sums SHA256SUMS \
  --signature SHA256SUMS.sig \
  --allowed-signers release/keys/portusos-release.allowed_signers
```

Verification succeeds only when:

1. the SSHSIG signature is valid for identity `portusos-release` and namespace `portusos-release` using the published allowed-signers key;
2. every `SHA256SUMS` line uses the canonical lowercase SHA-256 + two-space + basename format;
3. every referenced file is in the same release directory;
4. every referenced file hash matches.

The real public key is added only after the owner-controlled release-key ceremony. Never put a private signing key in this repository.
