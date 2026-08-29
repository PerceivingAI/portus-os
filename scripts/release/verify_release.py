#!/usr/bin/env python3
"""Verify PortusOS SHA256SUMS authenticity and every referenced artifact hash."""

from __future__ import annotations

import argparse
import hashlib
import re
import shutil
import subprocess
import sys
from pathlib import Path

IDENTITY = "portusos-release"
NAMESPACE = "portusos-release"
LINE = re.compile(r"^([0-9a-f]{64})  ([A-Za-z0-9][A-Za-z0-9._+-]*)$")


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def parse_manifest(path: Path) -> list[tuple[str, str]]:
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines:
        raise ValueError("SHA256SUMS is empty")
    parsed: list[tuple[str, str]] = []
    names: set[str] = set()
    for index, line in enumerate(lines, start=1):
        match = LINE.fullmatch(line)
        if not match:
            raise ValueError(f"SHA256SUMS line {index} is not canonical lowercase SHA-256 + two spaces + basename")
        expected, name = match.groups()
        if name in names:
            raise ValueError(f"SHA256SUMS contains duplicate basename: {name}")
        names.add(name)
        parsed.append((expected, name))
    if [name for _, name in parsed] != sorted(name for _, name in parsed):
        raise ValueError("SHA256SUMS entries must be basename-sorted")
    return parsed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sha256sums", type=Path, required=True)
    parser.add_argument("--signature", type=Path, required=True)
    parser.add_argument("--allowed-signers", type=Path, required=True)
    args = parser.parse_args([arg for arg in sys.argv[1:] if arg.strip()])

    manifest = args.sha256sums.resolve()
    signature = args.signature.resolve()
    allowed = args.allowed_signers.resolve()
    if manifest.name != "SHA256SUMS" or not manifest.is_file():
        raise ValueError("--sha256sums must reference an existing file named SHA256SUMS")
    if signature.name != "SHA256SUMS.sig" or not signature.is_file():
        raise ValueError("--signature must reference an existing file named SHA256SUMS.sig")
    if not allowed.is_file():
        raise ValueError("allowed-signers public key file does not exist")
    if shutil.which("ssh-keygen") is None:
        raise RuntimeError("ssh-keygen is required for PortusOS release verification")

    manifest_bytes = manifest.read_bytes()
    verify = subprocess.run(
        [
            "ssh-keygen", "-Y", "verify", "-f", str(allowed), "-I", IDENTITY,
            "-n", NAMESPACE, "-s", str(signature),
        ],
        input=manifest_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if verify.returncode != 0:
        message = verify.stderr.decode("utf-8", "replace").strip()
        raise ValueError(f"SHA256SUMS SSHSIG verification failed: {message}")

    base = manifest.parent.resolve()
    entries = parse_manifest(manifest)
    for expected, name in entries:
        candidate = base / name
        if candidate.is_symlink() or not candidate.is_file() or candidate.resolve().parent != base:
            raise ValueError(f"release artifact is missing, non-regular, symlinked, or outside bundle: {name}")
        actual = digest(candidate)
        if actual != expected:
            raise ValueError(f"SHA-256 mismatch for {name}")

    print(f"verified signature and {len(entries)} artifact hash(es)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as exc:
        print(f"release verification failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
