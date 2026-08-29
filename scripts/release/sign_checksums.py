#!/usr/bin/env python3
"""Sign a canonical PortusOS SHA256SUMS file with the external release key."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NAMESPACE = "portusos-release"


def inside_repo(path: Path) -> bool:
    try:
        path.resolve().relative_to(ROOT.resolve())
        return True
    except ValueError:
        return False


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sha256sums", type=Path, required=True)
    parser.add_argument("--private-key", type=Path, required=True)
    args = parser.parse_args([arg for arg in sys.argv[1:] if arg.strip()])

    manifest = args.sha256sums.resolve()
    private_key = args.private_key.resolve()
    if manifest.name != "SHA256SUMS" or not manifest.is_file():
        raise ValueError("--sha256sums must reference an existing file named SHA256SUMS")
    if not private_key.is_file():
        raise ValueError("release private key does not exist")
    if inside_repo(private_key):
        raise ValueError("release private key must remain outside the PortusOS repository")
    if shutil.which("ssh-keygen") is None:
        raise RuntimeError("ssh-keygen is required for PortusOS release signing")

    signature = Path(str(manifest) + ".sig")
    if signature.exists():
        raise ValueError(f"refusing to overwrite existing signature: {signature}")

    proc = subprocess.run(
        ["ssh-keygen", "-Y", "sign", "-f", str(private_key), "-n", NAMESPACE, str(manifest)],
        cwd=manifest.parent,
        check=False,
    )
    if proc.returncode != 0:
        return proc.returncode
    if not signature.is_file():
        raise RuntimeError("ssh-keygen reported success but SHA256SUMS.sig was not created")
    print(signature)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as exc:
        print(f"release signing failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
