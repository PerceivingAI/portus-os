#!/usr/bin/env python3
"""Generate SPDX 2.3 JSON and PortusOS provenance for a frozen candidate.

Preparatory mode is intentionally allowed to produce complete=false evidence.
--release-ready fails closed (exit 78) until public-release prerequisites exist.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXIT_UNRESOLVED = 78
THIRD_PARTY_SOURCE = ROOT / "oss" / "third-party-components.json"
RELEASE_POLICY = ROOT / "oss" / "release-policy.json"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
SAFE_REF = re.compile(r"^[A-Za-z0-9._/-]+$")


def run(args: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=False)
    if check and proc.returncode != 0:
        raise RuntimeError(f"command failed ({proc.returncode}): {' '.join(args)}\n{proc.stderr.strip()}")
    return proc


def load_json(path: Path, label: str) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"invalid {label} JSON at {path}: {exc}") from exc


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_component(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9.-]+", "-", value).strip("-.")
    return cleaned or "component"


def validate_ref(value: str, label: str) -> None:
    if not value or value.startswith(("/", "\\")) or ".." in Path(value).parts or not SAFE_REF.match(value):
        raise ValueError(f"{label} must be a safe relative reference")


def cargo_metadata() -> dict:
    proc = run(["cargo", "metadata", "--locked", "--format-version", "1"])
    return json.loads(proc.stdout)


def workspace_license_state(metadata: dict) -> tuple[list[str], list[dict]]:
    workspace = set(metadata["workspace_members"])
    missing = []
    packages = []
    for pkg in metadata["packages"]:
        if pkg["id"] not in workspace:
            continue
        license_expr = pkg.get("license") or ("LicenseRef-File" if pkg.get("license_file") else None)
        if not license_expr:
            missing.append(pkg["name"])
        packages.append({"name": pkg["name"], "version": pkg["version"], "license": license_expr})
    return sorted(missing), packages


def external_cargo_packages(metadata: dict) -> list[dict]:
    workspace = set(metadata["workspace_members"])
    packages = []
    for pkg in metadata["packages"]:
        if pkg["id"] in workspace:
            continue
        license_expr = pkg.get("license") or ("NOASSERTION" if not pkg.get("license_file") else "LicenseRef-File")
        packages.append({
            "name": pkg["name"],
            "version": pkg["version"],
            "license": license_expr,
            "source": pkg.get("source") or "NOASSERTION",
            "repository": pkg.get("repository"),
        })
    packages.sort(key=lambda item: (item["name"], item["version"], item["source"]))
    return packages


def validate_system_inventory(data: dict, candidate_id: str, lock_hash: str) -> list[dict]:
    if data.get("schema_version") != 1 or data.get("candidate_id") != candidate_id:
        raise ValueError("system-package inventory candidate/schema mismatch")
    if data.get("package_lock_sha256") != lock_hash:
        raise ValueError("system-package inventory does not bind the candidate package lock")
    packages = data.get("packages")
    if not isinstance(packages, list) or not packages:
        raise ValueError("system-package inventory must contain packages")
    seen = set()
    for item in packages:
        required = {"id", "name", "version", "source_class", "repository", "licenses", "verification"}
        if set(item) - (required | {"sha256"}) or not required.issubset(item):
            raise ValueError("system-package inventory item has invalid fields")
        if item["id"] in seen or not item["licenses"]:
            raise ValueError("system-package inventory IDs must be unique and licences non-empty")
        seen.add(item["id"])
        if item.get("sha256") is not None and not HEX64.fullmatch(item["sha256"]):
            raise ValueError("system-package inventory sha256 must be lowercase 64-hex")
    return packages


def validate_third_party_review(data: dict, candidate_id: str) -> list[dict]:
    if data.get("schema_version") != 1 or data.get("candidate_id") != candidate_id:
        raise ValueError("third-party review candidate/schema mismatch")
    entries = data.get("entries")
    if not isinstance(entries, list) or not entries:
        raise ValueError("third-party review must contain entries")
    seen = set()
    for item in entries:
        required = {"id", "distributed", "exact_identity", "licenses", "redistribution_status", "notices", "evidence_refs"}
        if set(item) != required or item["id"] in seen:
            raise ValueError("third-party review entries must use exact unique v1 shape")
        seen.add(item["id"])
        if item["redistribution_status"] not in {"approved", "not-distributed", "pending", "rejected"}:
            raise ValueError("invalid redistribution status")
        if not item["evidence_refs"]:
            raise ValueError("third-party review entry requires evidence references")
        if item["distributed"] and item["redistribution_status"] != "approved":
            raise ValueError(f"distributed component {item['id']} is not approved")
    return entries


def validate_release_policy(data: dict) -> dict:
    if data.get("schema_version") != 1 or data.get("authority") != "docs/RELEASE.md":
        raise ValueError("release policy schema/authority mismatch")
    if data.get("unresolved_owner_decisions") != []:
        raise ValueError("release policy must have no unresolved owner decisions")
    first = data.get("first_public_release", {})
    if first.get("version") != "0.1.0" or first.get("maturity") != "technical-preview":
        raise ValueError("first public release policy mismatch")
    first_party_license = data.get("first_party_license", {})
    if first_party_license != {"spdx": "Apache-2.0", "license_file": "LICENSE"}:
        raise ValueError("first-party licence policy mismatch")
    repository = data.get("repository", {})
    if repository.get("host") != "github" or repository.get("external_code_contributions") is not False:
        raise ValueError("repository governance policy mismatch")
    issues = repository.get("issues", {})
    if issues.get("enabled") is not True or issues.get("security_reports_allowed") is not False:
        raise ValueError("public issue policy mismatch")
    signing = data.get("signing", {})
    if (
        signing.get("mechanism") != "openssh-sshsig-ed25519"
        or signing.get("signed_object") != "SHA256SUMS"
        or signing.get("namespace") != "portusos-release"
        or signing.get("signer_identity") != "portusos-release"
        or signing.get("private_key_custody") != "external-offline-owner-controlled-encrypted"
    ):
        raise ValueError("release signing policy mismatch")
    validate_ref(signing.get("public_key_ref", ""), "release public key reference")
    support = data.get("support", {})
    if (
        support.get("level") != "best-effort"
        or support.get("supported_release") != "current-public-final-only"
        or support.get("end_condition") != "superseded-or-withdrawn"
        or support.get("lts") is not False
        or support.get("sla") is not False
        or support.get("guaranteed_update_cadence") is not False
    ):
        raise ValueError("support/update policy mismatch")
    return data


def spdx_expression_or_noassertion(value: str | None) -> str:
    if not value:
        return "NOASSERTION"
    # Cargo metadata occasionally carries legacy slash-separated values such as
    # MIT/Apache-2.0, which are not valid SPDX expressions. Preserve those only
    # in comments and fail safe to NOASSERTION in the SPDX field.
    if "/" in value or not re.fullmatch(r"[A-Za-z0-9.+() -]+", value):
        return "NOASSERTION"
    return value


def spdx_license(licenses: list[str]) -> str:
    if not licenses:
        return "NOASSERTION"
    simple = re.compile(r"^[A-Za-z0-9.+-]+$")
    if all(simple.fullmatch(value) for value in licenses):
        return " AND ".join(licenses)
    return "NOASSERTION"


def spdx_package(spdx_id: str, name: str, version: str, license_expr: str | None, download: str = "NOASSERTION", comment: str | None = None, checksum: str | None = None) -> dict:
    package = {
        "SPDXID": spdx_id,
        "name": name,
        "versionInfo": version,
        "downloadLocation": download or "NOASSERTION",
        "filesAnalyzed": False,
        "licenseConcluded": "NOASSERTION",
        "licenseDeclared": license_expr or "NOASSERTION",
        "copyrightText": "NOASSERTION",
    }
    if comment:
        package["comment"] = comment
    if checksum:
        package["checksums"] = [{"algorithm": "SHA256", "checksumValue": checksum}]
    return package


def generate(args) -> tuple[dict, dict, list[str]]:
    candidate_root = args.candidate_root.resolve()
    candidate = load_json(candidate_root / "candidate.json", "candidate")
    build = load_json(candidate_root / "build-metadata.json", "build metadata")
    validation = load_json(candidate_root / "validation-report.json", "validation report")
    candidate_id = candidate.get("candidate_id")
    if not candidate_id or build.get("candidate_id") != candidate_id or validation.get("candidate_id") != candidate_id:
        raise ValueError("candidate/build/validation candidate IDs do not agree")
    iso_name = candidate.get("iso_filename")
    iso_path = candidate_root / iso_name
    iso_hash = sha256(iso_path)
    if iso_hash != candidate.get("iso_sha256") or iso_hash != build.get("artifact", {}).get("sha256") or iso_hash != validation.get("iso_sha256"):
        raise ValueError("candidate ISO hashes do not agree")

    verify = run(["cargo", "run", "--locked", "-q", "-p", "portus-build", "--", "candidate-verify", "--candidate-root", str(candidate_root)], check=False)
    if verify.returncode != 0:
        raise ValueError(f"candidate-verify failed: {verify.stderr.strip()}")

    metadata = cargo_metadata()
    missing_workspace_license, workspace_packages = workspace_license_state(metadata)
    external_packages = external_cargo_packages(metadata)
    third_party_source = load_json(THIRD_PARTY_SOURCE, "third-party source inventory")
    release_policy = validate_release_policy(load_json(RELEASE_POLICY, "release policy"))
    source_component_ids = {item["id"] for item in third_party_source.get("components", [])}
    if not source_component_ids:
        raise ValueError("third-party source inventory is empty")

    reasons: list[str] = []
    license_path = ROOT / release_policy["first_party_license"]["license_file"]
    if not license_path.is_file():
        reasons.append("Apache-2.0 first-party LICENSE file is absent")
    elif "Apache License\n                           Version 2.0, January 2004" not in license_path.read_text(encoding="utf-8"):
        reasons.append("first-party LICENSE file does not contain the Apache License 2.0 text")
    if missing_workspace_license:
        reasons.append("Portus-owned Cargo packages do not declare Apache-2.0")
    elif any(item["license"] != "Apache-2.0" for item in workspace_packages):
        reasons.append("Portus-owned Cargo package licence metadata is not uniformly Apache-2.0")
    if build.get("release_class") != "public_rc":
        reasons.append("candidate is not a public_rc")
    elif build.get("version") != release_policy["first_public_release"]["version"]:
        reasons.append("public_rc version does not match the selected first public release policy")
    if not build.get("source_tree_clean"):
        reasons.append("candidate source_tree_clean is false")
    if validation.get("status") != "accepted":
        reasons.append("validation report is not accepted")
    if not args.source_tag:
        reasons.append("public source tag is not supplied")
    if not HEX40.fullmatch(args.build_instructions_revision):
        raise ValueError("build-instructions-revision must be 40 lowercase hex")
    validate_ref(args.builder_environment_ref, "builder environment reference")
    if not (candidate_root / args.builder_environment_ref).is_file():
        reasons.append("builder environment evidence is absent")
    if args.source_tag:
        tag = run(["git", "rev-parse", f"{args.source_tag}^{{commit}}"], check=False)
        if tag.returncode != 0 or tag.stdout.strip() != candidate["source_revision"]:
            reasons.append("public source tag is absent or does not point to the candidate source revision")
    security_path = ROOT / "SECURITY.md"
    security_host = release_policy["repository"]["private_vulnerability_reporting"]
    if not security_path.is_file():
        reasons.append("SECURITY.md is absent")
    if security_host["activation_status"] != "active" or security_host["notifications_reviewed"] is not True:
        reasons.append("GitHub private vulnerability reporting is not activated/reviewed")

    signing = release_policy["signing"]
    public_key = ROOT / signing["public_key_ref"]
    if signing["public_key_status"] != "active" or not public_key.is_file():
        reasons.append("public release verification key is not active/published")

    lock_path = candidate_root / "packages.lock.yaml"
    lock_hash = sha256(lock_path) if lock_path.is_file() else None
    if lock_hash is None:
        reasons.append("candidate package lock is absent")

    system_packages: list[dict] = []
    if args.system_package_inventory:
        if lock_hash is None:
            raise ValueError("system-package inventory supplied without candidate package lock")
        system_packages = validate_system_inventory(load_json(args.system_package_inventory, "system-package inventory"), candidate_id, lock_hash)
    else:
        reasons.append("candidate-specific system-package licence inventory is absent")

    reviewed_components: list[dict] = []
    if args.third_party_review:
        reviewed_components = validate_third_party_review(load_json(args.third_party_review, "third-party review"), candidate_id)
        reviewed_ids = {item["id"] for item in reviewed_components}
        if reviewed_ids != source_component_ids:
            missing = sorted(source_component_ids - reviewed_ids)
            extra = sorted(reviewed_ids - source_component_ids)
            raise ValueError(f"third-party review coverage mismatch: missing={missing}, extra={extra}")
    else:
        reasons.append("candidate-specific third-party redistribution review is absent")

    packages = []
    describes = []
    for item in workspace_packages:
        sid = f"SPDXRef-Portus-{safe_component(item['name'])}-{safe_component(item['version'])}"
        declared = spdx_expression_or_noassertion(item["license"])
        comment = None if declared == (item["license"] or "NOASSERTION") else f"Cargo licence metadata: {item['license']}"
        packages.append(spdx_package(sid, item["name"], item["version"], declared, comment=comment))
        describes.append(sid)
    for item in external_packages:
        sid = f"SPDXRef-Cargo-{safe_component(item['name'])}-{safe_component(item['version'])}"
        declared = spdx_expression_or_noassertion(item["license"])
        comment = None if declared == (item["license"] or "NOASSERTION") else f"Cargo licence metadata: {item['license']}"
        packages.append(spdx_package(
            sid,
            item["name"],
            item["version"],
            declared,
            item["repository"] or item["source"],
            comment=comment,
        ))
        describes.append(sid)
    for item in system_packages:
        sid = f"SPDXRef-System-{safe_component(item['id'])}-{safe_component(item['version'])}"
        packages.append(spdx_package(
            sid,
            item["name"],
            item["version"],
            spdx_license(item["licenses"]),
            item["repository"] or "NOASSERTION",
            comment="source_class=" + item["source_class"] + "; declared licences=" + ", ".join(item["licenses"]),
            checksum=item.get("sha256"),
        ))
        describes.append(sid)
    for item in reviewed_components:
        if not item["distributed"]:
            continue
        sid = f"SPDXRef-Component-{safe_component(item['id'])}"
        packages.append(spdx_package(
            sid,
            item["id"],
            item["exact_identity"],
            spdx_license(item["licenses"]),
            comment="candidate redistribution review: approved; licences=" + ", ".join(item["licenses"]),
        ))
        describes.append(sid)

    packages.sort(key=lambda item: item["SPDXID"])
    describes.sort()
    complete = not reasons
    sbom = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"PortusOS-{candidate_id}",
        "documentNamespace": f"https://portusos.local/spdx/{safe_component(candidate_id)}/{iso_hash}",
        "creationInfo": {
            "created": args.created_at,
            "creators": ["Tool: PortusOS scripts/oss/release_evidence.py"],
        },
        "documentDescribes": describes,
        "packages": packages,
        "comment": "release_ready=" + str(complete).lower() + ("; incomplete=" + " | ".join(reasons) if reasons else ""),
        "annotations": [{
            "annotationDate": args.created_at,
            "annotationType": "OTHER",
            "annotator": "Tool: PortusOS OSS preparation",
            "comment": "Source inventory authority: oss/third-party-components.json; source entries=" + str(len(third_party_source.get("components", []))),
        }],
    }

    output_dir = args.output_dir.resolve() if args.output_dir else candidate_root
    output_dir.mkdir(parents=True, exist_ok=True)
    sbom_name = f"PortusOS-{safe_component(build.get('version') or candidate_id)}.spdx.json"
    sbom_path = output_dir / sbom_name
    sbom_path.write_text(json.dumps(sbom, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    provenance = {
        "schema_version": 1,
        "candidate_id": candidate_id,
        "source_revision": candidate["source_revision"],
        "source_tag": args.source_tag,
        "build_metadata_sha256": sha256(candidate_root / "build-metadata.json"),
        "package_source_manifest_sha256": sha256(candidate_root / candidate["package_source_manifest_ref"]),
        "package_lock_sha256": lock_hash,
        "first_party_license": release_policy["first_party_license"]["spdx"],
        "license_file_sha256": sha256(license_path),
        "release_policy_sha256": sha256(RELEASE_POLICY),
        "sbom_sha256": sha256(sbom_path),
        "validation_report_sha256": sha256(candidate_root / "validation-report.json"),
        "iso_sha256": iso_hash,
        "builder_environment_ref": args.builder_environment_ref,
        "build_instructions_revision": args.build_instructions_revision,
        "created_at": args.created_at,
        "complete": complete,
        "incomplete_reasons": sorted(set(reasons)),
    }
    provenance_path = output_dir / "provenance.json"
    provenance_path.write_text(json.dumps(provenance, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return sbom, provenance, reasons


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate-root", type=Path, required=True)
    parser.add_argument("--created-at", required=True)
    parser.add_argument("--source-tag")
    parser.add_argument("--builder-environment-ref", default="environment/reference.json")
    parser.add_argument("--build-instructions-revision", required=True)
    parser.add_argument("--system-package-inventory", type=Path)
    parser.add_argument("--third-party-review", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--release-ready", action="store_true")
    args = parser.parse_args([arg for arg in sys.argv[1:] if arg.strip()])

    _, provenance, reasons = generate(args)
    print(json.dumps({
        "candidate_id": provenance["candidate_id"],
        "complete": provenance["complete"],
        "incomplete_reasons": provenance["incomplete_reasons"],
    }, indent=2))
    if args.release_ready and reasons:
        return EXIT_UNRESOLVED
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as exc:
        print(f"release evidence generation failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
