#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def run(args: list[str], expected: int = 0) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=False)
    if proc.returncode != expected:
        raise AssertionError(
            f"expected exit {expected}, got {proc.returncode}: {' '.join(args)}\nstdout={proc.stdout}\nstderr={proc.stderr}"
        )
    return proc


class ReleaseSigningTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if shutil.which("ssh-keygen") is None:
            raise unittest.SkipTest("ssh-keygen not available")

    def test_sign_verify_and_tamper_detection(self) -> None:
        with tempfile.TemporaryDirectory(prefix="portus-release-signing-") as temp:
            root = Path(temp)
            key = root / "owner-release-key"
            run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(key)])
            allowed = root / "portusos-release.allowed_signers"
            prepared = run([
                sys.executable, "scripts/release/prepare_public_key.py",
                "--public-key", str(root / "owner-release-key.pub"),
                "--output", str(allowed),
            ])
            self.assertTrue(prepared.stdout.strip().startswith("SHA256:"))

            bundle = root / "bundle"
            bundle.mkdir()
            artifact = bundle / "PortusOS-0.1.0-x86_64.iso"
            artifact.write_bytes(b"fixture PortusOS release bytes")
            checksum = hashlib.sha256(artifact.read_bytes()).hexdigest()
            manifest = bundle / "SHA256SUMS"
            manifest.write_text(f"{checksum}  {artifact.name}\n", encoding="utf-8")

            run([
                sys.executable, "scripts/release/sign_checksums.py",
                "--sha256sums", str(manifest), "--private-key", str(key),
            ])
            signature = bundle / "SHA256SUMS.sig"
            self.assertTrue(signature.is_file())

            verify_args = [
                sys.executable, "scripts/release/verify_release.py",
                "--sha256sums", str(manifest), "--signature", str(signature),
                "--allowed-signers", str(allowed),
            ]
            run(verify_args)

            artifact.write_bytes(b"tampered PortusOS release bytes")
            run(verify_args, expected=1)

    def test_signer_rejects_private_key_inside_repository(self) -> None:
        fake = ROOT / "release-private-key-test-fixture"
        manifest_dir = None
        try:
            fake.write_text("not a real key", encoding="utf-8")
            with tempfile.TemporaryDirectory(prefix="portus-release-signing-") as temp:
                manifest_dir = Path(temp)
                manifest = manifest_dir / "SHA256SUMS"
                manifest.write_text("0" * 64 + "  fixture.iso\n", encoding="utf-8")
                proc = run([
                    sys.executable, "scripts/release/sign_checksums.py",
                    "--sha256sums", str(manifest), "--private-key", str(fake),
                ], expected=1)
                self.assertIn("must remain outside", proc.stderr)
        finally:
            if fake.exists():
                fake.unlink()


if __name__ == "__main__":
    argv = [sys.argv[0], *[arg for arg in sys.argv[1:] if arg.strip()]]
    unittest.main(argv=argv)
