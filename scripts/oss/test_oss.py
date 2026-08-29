#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import audit_repo
import dependency_inventory

ROOT = Path(__file__).resolve().parents[2]


def run(args: list[str], *, expected: int = 0) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=False)
    if proc.returncode != expected:
        raise AssertionError(
            f"expected exit {expected}, got {proc.returncode}: {' '.join(args)}\nstdout={proc.stdout}\nstderr={proc.stderr}"
        )
    return proc


class OssToolingTests(unittest.TestCase):
    def test_secret_detectors_flag_reusable_values_without_exposing_them(self) -> None:
        payloads = [
            b"AWS=" + b"AKIA" + b"A" * 16,
            b"token=" + b"ghp_" + b"A" * 24,
            b"api_key=" + b"A" * 32,
            b"-----BEGIN PRIVATE KEY-----\n" + b"A" * 64 + b"\n-----END PRIVATE KEY-----",
        ]
        for payload in payloads:
            findings = audit_repo.scan_payload(payload, "fixture")
            self.assertTrue(findings)
            rendered = json.dumps([finding.__dict__ for finding in findings])
            self.assertNotIn(payload.decode("ascii", "ignore"), rendered)

        self.assertEqual(audit_repo.scan_payload(b"api_key=<example-key>", "fixture"), [])
        self.assertEqual(audit_repo.scan_payload(b"the secret boundary is documented", "fixture"), [])

    def test_personal_path_scanner_allows_generic_fixtures_but_flags_realistic_user(self) -> None:
        generic = b"/home/master /home/demo /home/user /Users/example C:\\Users\\username\\repo"
        self.assertEqual(audit_repo.scan_payload(generic, "fixture"), [])
        private_user = b"alice" + b".private"
        windows_path = b"C:" + b"\\Users\\" + private_user + b"\\repo"
        findings = audit_repo.scan_payload(windows_path, "fixture")
        self.assertTrue(any(item.kind == "personal-windows-user-path" for item in findings))
        unix_path = b"/home/" + private_user + b"/repo"
        findings = audit_repo.scan_payload(unix_path, "fixture")
        self.assertTrue(any(item.kind == "personal-unix-user-path" for item in findings))

    def test_third_party_inventory_is_unique_and_fail_closed(self) -> None:
        data = json.loads((ROOT / "oss/third-party-components.json").read_text(encoding="utf-8"))
        self.assertEqual(data["schema_version"], 1)
        ids = [item["id"] for item in data["components"]]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertIn("codex", ids)
        self.assertIn("artix-runtime-packages", ids)
        for item in data["components"]:
            self.assertTrue(item["required_gate"])
            self.assertNotEqual(item["redistribution_status"], "approved")
        codex = next(item for item in data["components"] if item["id"] == "codex")
        self.assertEqual(codex["license"], "Apache-2.0")

    def test_oss_schemas_and_package_lock_license_requirement(self) -> None:
        schema_paths = [
            ROOT / "oss/schemas/system-package-inventory.schema.json",
            ROOT / "oss/schemas/third-party-review.schema.json",
            ROOT / "oss/schemas/provenance.schema.json",
            ROOT / "oss/schemas/release-policy.schema.json",
            ROOT / "portusos-build/schemas/package-lock.schema.json",
        ]
        for path in schema_paths:
            data = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(data["$schema"], "https://json-schema.org/draft/2020-12/schema")
            self.assertFalse(data.get("additionalProperties", True))
        release_policy_schema = json.loads((ROOT / "oss/schemas/release-policy.schema.json").read_text(encoding="utf-8"))
        self.assertEqual(release_policy_schema["properties"]["unresolved_owner_decisions"]["const"], [])
        self.assertEqual(release_policy_schema["properties"]["first_party_license"]["properties"]["spdx"]["const"], "Apache-2.0")
        release_metadata_schema = json.loads((ROOT / "portusos-build/schemas/release-metadata.schema.json").read_text(encoding="utf-8"))
        release_required = release_metadata_schema["required"]
        self.assertIn("release_policy_sha256", release_required)
        self.assertIn("checksum_signature_ref", release_required)
        self.assertIn("signing_key_fingerprint", release_required)
        package_lock = json.loads(schema_paths[-1].read_text(encoding="utf-8"))
        required = package_lock["properties"]["resolved"]["items"]["required"]
        self.assertIn("licenses", required)

    def test_dependency_security_policy_is_fail_closed_without_path_dependency_churn(self) -> None:
        policy = (ROOT / "deny.toml").read_text(encoding="utf-8")
        self.assertIn('yanked = "deny"', policy)
        self.assertIn('ignore = []', policy)
        self.assertIn('multiple-versions = "warn"', policy)
        self.assertIn('wildcards = "allow"', policy)
        self.assertIn('unknown-registry = "deny"', policy)
        self.assertIn('unknown-git = "deny"', policy)
        self.assertIn('[licenses]', policy)
        self.assertIn('"Apache-2.0"', policy)
        self.assertNotIn('"GPL-3.0', policy)
        self.assertNotIn('"AGPL-', policy)

    def test_release_policy_and_security_governance_are_explicit(self) -> None:
        policy = json.loads((ROOT / "oss/release-policy.json").read_text(encoding="utf-8"))
        self.assertEqual(policy["first_public_release"]["version"], "0.1.0")
        self.assertEqual(policy["first_public_release"]["maturity"], "technical-preview")
        self.assertEqual(policy["unresolved_owner_decisions"], [])
        self.assertEqual(policy["first_party_license"], {"spdx": "Apache-2.0", "license_file": "LICENSE"})
        self.assertFalse(policy["repository"]["external_code_contributions"])
        self.assertEqual(policy["repository"]["private_vulnerability_reporting"]["mechanism"], "github-private-vulnerability-reporting")
        self.assertEqual(policy["signing"]["mechanism"], "openssh-sshsig-ed25519")
        self.assertEqual(policy["support"]["level"], "best-effort")
        security = (ROOT / "SECURITY.md").read_text(encoding="utf-8")
        self.assertIn("GitHub Private Vulnerability Reporting", security)
        self.assertIn("no supported public PortusOS release", security)
        self.assertIn("does not accept external pull requests or code contributions", security)
        self.assertFalse((ROOT / "CONTRIBUTING.md").exists())
        self.assertTrue((ROOT / "LICENSE").is_file())
        self.assertIn("Apache License", (ROOT / "LICENSE").read_text(encoding="utf-8"))

    def test_locked_dependency_inventory_has_licenses_and_no_git_dependencies(self) -> None:
        inventory = dependency_inventory.build_inventory()
        self.assertEqual(inventory["workspace_package_count"], 23)
        self.assertGreater(inventory["external_package_count"], 0)
        self.assertEqual(inventory["missing_external_license_metadata"], [])
        self.assertEqual(inventory["git_dependencies"], [])
        self.assertEqual(inventory["external_wildcard_dependencies"], [])
        self.assertEqual(inventory["workspace_license_expressions"], {"Apache-2.0": 23})
        self.assertTrue(inventory["cargo_lock_tracked"])
        for package in inventory["packages"]:
            if package["workspace"]:
                manifest = package.get("manifest", "")
                self.assertFalse(":" in manifest[:3], manifest)

    def test_release_evidence_fixture_is_incomplete_and_release_ready_fails_closed(self) -> None:
        revision = run(["git", "rev-parse", "HEAD"]).stdout.strip()
        rc_number = 990000 + (os.getpid() % 9000)
        candidate_id = f"first-iso-rc.{rc_number}-g{revision[:12]}"
        candidate_root = ROOT / "portusos-build/out/validation" / candidate_id
        if candidate_root.exists():
            shutil.rmtree(candidate_root)

        with tempfile.TemporaryDirectory(prefix="portus-oss-") as temp:
            temp_path = Path(temp)
            artifact = temp_path / "fixture.iso"
            artifact.write_bytes(b"PortusOS OSS release evidence fixture")
            init = {
                "release_class": "development_rc",
                "version": None,
                "rc_number": rc_number,
                "source_revision": revision,
                "source_tree_clean": False,
                "build_started_at": "2026-08-27T20:20:00Z",
                "build_finished_at": "2026-08-27T20:21:00Z",
                "distribution_snapshot": "windows-host-fixture-only",
                "artools_version": "fixture-only",
                "rust_toolchain": "1.85.0",
                "validation_authority_revision": revision,
                "release_authority_revision": revision,
                "created_at": "2026-08-27T20:22:00Z",
            }
            init_path = temp_path / "candidate-init.json"
            init_path.write_text(json.dumps(init), encoding="utf-8")
            output = temp_path / "evidence"
            try:
                run([
                    "cargo", "run", "--locked", "-q", "-p", "portus-build", "--",
                    "candidate-init", "--artifact", str(artifact), "--input-json", str(init_path),
                ])
                base = [
                    "python", "scripts/oss/release_evidence.py",
                    "--candidate-root", str(candidate_root),
                    "--created-at", "2026-08-27T20:23:00Z",
                    "--build-instructions-revision", revision,
                    "--output-dir", str(output),
                ]
                run(base)
                provenance = json.loads((output / "provenance.json").read_text(encoding="utf-8"))
                self.assertFalse(provenance["complete"])
                self.assertEqual(len(provenance["release_policy_sha256"]), 64)
                self.assertEqual(provenance["first_party_license"], "Apache-2.0")
                self.assertEqual(len(provenance["license_file_sha256"]), 64)
                self.assertIn("GitHub private vulnerability reporting is not activated/reviewed", provenance["incomplete_reasons"])
                self.assertIn("public release verification key is not active/published", provenance["incomplete_reasons"])
                sboms = list(output.glob("*.spdx.json"))
                self.assertEqual(len(sboms), 1)
                sbom = json.loads(sboms[0].read_text(encoding="utf-8"))
                self.assertEqual(sbom["spdxVersion"], "SPDX-2.3")
                self.assertEqual(sbom["dataLicense"], "CC0-1.0")
                run(base + ["--release-ready"], expected=78)
            finally:
                if candidate_root.exists():
                    shutil.rmtree(candidate_root)


if __name__ == "__main__":
    argv = [sys.argv[0], *[arg for arg in sys.argv[1:] if arg.strip()]]
    unittest.main(argv=argv)
