#!/usr/bin/env python3
"""Prepare PortusOS OpenSSH allowed-signers verification material from a public Ed25519 key."""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

IDENTITY = "portusos-release"
FINGERPRINT = re.compile(r"SHA256:[A-Za-z0-9+/=]+")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--public-key", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args([arg for arg in sys.argv[1:] if arg.strip()])

    public_key = args.public_key.resolve()
    output = args.output.resolve()
    if not public_key.is_file():
        raise ValueError("public key does not exist")
    if output.exists():
        raise ValueError(f"refusing to overwrite existing allowed-signers file: {output}")
    text = public_key.read_text(encoding="utf-8").strip()
    parts = text.split()
    if len(parts) < 2 or parts[0] != "ssh-ed25519":
        raise ValueError("PortusOS release public key must be OpenSSH Ed25519")
    if shutil.which("ssh-keygen") is None:
        raise RuntimeError("ssh-keygen is required")

    fingerprint = subprocess.run(
        ["ssh-keygen", "-lf", str(public_key), "-E", "sha256"],
        text=True,
        capture_output=True,
        check=False,
    )
    if fingerprint.returncode != 0:
        raise ValueError(f"unable to fingerprint public key: {fingerprint.stderr.strip()}")
    match = FINGERPRINT.search(fingerprint.stdout)
    if not match:
        raise ValueError("ssh-keygen did not return a SHA256 fingerprint")

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(f"{IDENTITY} {parts[0]} {parts[1]}\n", encoding="utf-8")
    print(match.group(0))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as exc:
        print(f"public key preparation failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
