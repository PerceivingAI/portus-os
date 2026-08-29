#!/usr/bin/env python3
"""Generate a secret-safe Cargo dependency/licence/provenance inventory."""

from __future__ import annotations

import argparse
import collections
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = ROOT / "artifacts" / "oss"
SCHEMA_VERSION = 1


def run(args: list[str]) -> str:
    proc = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=False)
    if proc.returncode != 0:
        raise RuntimeError(f"command failed ({proc.returncode}): {' '.join(args)}\n{proc.stderr.strip()}")
    return proc.stdout


def cargo_metadata() -> dict:
    return json.loads(run(["cargo", "metadata", "--locked", "--format-version", "1"]))


def relative_manifest(path: str) -> str:
    candidate = Path(path)
    try:
        return candidate.resolve().relative_to(ROOT.resolve()).as_posix()
    except (OSError, ValueError):
        return "<external>"


def build_inventory() -> dict:
    data = cargo_metadata()
    workspace_ids = set(data["workspace_members"])
    nodes = {node["id"]: node for node in (data.get("resolve") or {}).get("nodes", [])}
    direct_ids: set[str] = set()
    for workspace_id in workspace_ids:
        node = nodes.get(workspace_id)
        if not node:
            continue
        for dep in node.get("deps", []):
            direct_ids.add(dep["pkg"])

    packages = []
    missing_external_license = []
    git_dependencies = []
    external_wildcard_dependencies = []
    license_counts: collections.Counter[str] = collections.Counter()
    workspace_license_counts: collections.Counter[str] = collections.Counter()
    duplicate_versions: dict[str, set[str]] = collections.defaultdict(set)

    for package in data["packages"]:
        workspace = package["id"] in workspace_ids
        source = package.get("source")
        source_kind = "workspace" if workspace else "registry"
        if source and source.startswith("git+"):
            source_kind = "git"
        elif source and not source.startswith("registry+"):
            source_kind = "other"
        elif not source and not workspace:
            source_kind = "path"

        license_expr = package.get("license")
        license_file_present = bool(package.get("license_file"))
        if workspace:
            workspace_license_counts[license_expr or "LICENSE_FILE"] += 1
        if not workspace:
            duplicate_versions[package["name"]].add(package["version"])
            if not (license_expr or license_file_present):
                missing_external_license.append(f"{package['name']}@{package['version']}")
            if source_kind == "git":
                git_dependencies.append(f"{package['name']}@{package['version']}")
            license_counts[license_expr or "LICENSE_FILE"] += 1

        record = {
            "name": package["name"],
            "version": package["version"],
            "workspace": workspace,
            "direct_workspace_dependency": (not workspace and package["id"] in direct_ids),
            "source_kind": source_kind,
            "source": None if workspace else source,
            "license": license_expr,
            "license_file_present": license_file_present,
            "repository": package.get("repository"),
            "rust_version": package.get("rust_version"),
        }
        if workspace:
            record["manifest"] = relative_manifest(package["manifest_path"])
        packages.append(record)

    for package in data["packages"]:
        if package["id"] not in workspace_ids:
            continue
        for dep in package.get("dependencies", []):
            if dep.get("path") is None and dep.get("source") and dep.get("req") == "*":
                external_wildcard_dependencies.append(f"{package['name']}->{dep['name']}")

    packages.sort(key=lambda item: (not item["workspace"], item["name"], item["version"], item["source_kind"]))
    duplicate_major = {
        name: sorted(versions)
        for name, versions in sorted(duplicate_versions.items())
        if len({version.split(".", 1)[0] for version in versions}) > 1
    }
    external_count = sum(1 for package in packages if not package["workspace"])
    direct_external_count = sum(1 for package in packages if package["direct_workspace_dependency"])

    revision = run(["git", "rev-parse", "HEAD"]).strip()
    return {
        "schema_version": SCHEMA_VERSION,
        "authority": "THIRD_PARTY.md",
        "source_revision": revision,
        "cargo_lock_tracked": bool(run(["git", "ls-files", "Cargo.lock"]).strip()),
        "workspace_package_count": len(workspace_ids),
        "external_package_count": external_count,
        "direct_external_package_count": direct_external_count,
        "missing_external_license_metadata": sorted(missing_external_license),
        "git_dependencies": sorted(git_dependencies),
        "external_wildcard_dependencies": sorted(set(external_wildcard_dependencies)),
        "duplicate_major_versions": duplicate_major,
        "external_license_expressions": dict(sorted(license_counts.items())),
        "workspace_license_expressions": dict(sorted(workspace_license_counts.items())),
        "packages": packages,
    }


def render_markdown(inventory: dict) -> str:
    lines = [
        "# PortusOS Cargo Dependency Inventory",
        "",
        f"- Source revision: `{inventory['source_revision']}`",
        f"- Workspace packages: {inventory['workspace_package_count']}",
        f"- Locked external packages: {inventory['external_package_count']}",
        f"- Direct external dependencies: {inventory['direct_external_package_count']}",
        f"- External packages missing licence metadata: {len(inventory['missing_external_license_metadata'])}",
        f"- Git dependencies: {len(inventory['git_dependencies'])}",
        f"- External wildcard dependency requirements: {len(inventory['external_wildcard_dependencies'])}",
        f"- Cargo.lock tracked: {str(inventory['cargo_lock_tracked']).lower()}",
        "",
        "## Licence expressions",
        "",
        "| Expression | Packages |",
        "| --- | ---: |",
    ]
    for expression, count in inventory["external_license_expressions"].items():
        lines.append(f"| `{expression}` | {count} |")
    lines += ["", "## Release interpretation", ""]
    if inventory["missing_external_license_metadata"]:
        lines.append("External packages with missing licence metadata require review before publication.")
    else:
        lines.append("All locked external Rust packages expose licence metadata or a licence file through Cargo metadata.")
    if inventory["git_dependencies"]:
        lines.append("Git dependencies are present and require explicit provenance review.")
    else:
        lines.append("No Git dependencies are present in the locked Cargo graph.")
    if inventory["external_wildcard_dependencies"]:
        lines.append("External wildcard dependency requirements are present and must be pinned or reviewed.")
    else:
        lines.append("No workspace dependency uses an external `*` version requirement; internal path-only workspace dependencies are not treated as external wildcards.")
    lines.append("All PortusOS workspace crates are required to declare `Apache-2.0`; third-party dependencies retain their own upstream licence expressions and obligations.")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--check", action="store_true", help="fail if external licence metadata/provenance is incomplete")
    args = parser.parse_args([arg for arg in sys.argv[1:] if arg.strip()])

    inventory = build_inventory()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    json_path = args.output_dir / "dependency-inventory.json"
    md_path = args.output_dir / "dependency-inventory.md"
    json_path.write_text(json.dumps(inventory, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    md_path.write_text(render_markdown(inventory), encoding="utf-8")

    print(json.dumps({
        "workspace_packages": inventory["workspace_package_count"],
        "external_packages": inventory["external_package_count"],
        "workspace_apache_2_0": inventory["workspace_license_expressions"].get("Apache-2.0", 0),
        "missing_external_license_metadata": len(inventory["missing_external_license_metadata"]),
        "git_dependencies": len(inventory["git_dependencies"]),
        "external_wildcard_dependencies": len(inventory["external_wildcard_dependencies"]),
        "output": str(args.output_dir.relative_to(ROOT) if args.output_dir.is_relative_to(ROOT) else args.output_dir),
    }, indent=2))

    if args.check and (
        inventory["missing_external_license_metadata"]
        or inventory["git_dependencies"]
        or inventory["external_wildcard_dependencies"]
        or inventory["workspace_license_expressions"] != {"Apache-2.0": inventory["workspace_package_count"]}
        or not inventory["cargo_lock_tracked"]
    ):
        return 1
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, json.JSONDecodeError) as exc:
        print(f"dependency inventory failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
