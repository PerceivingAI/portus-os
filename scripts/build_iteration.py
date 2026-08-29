#!/usr/bin/env python3
"""Repeatable PortusOS ISO build-attempt harness.

Every invocation creates a new, never-overwritten build-attempt directory under
portusos-build/out/builds/. The harness records source identity, outer-host
identity, tracked build-input hashes, preflight results, builder output, artifact
identity, and a checksum manifest. Failed/blocked attempts are retained too.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

SCHEMA_VERSION = 1
EXIT_UNRESOLVED = 78
EXIT_USAGE = 64
EXIT_DIRTY = 65
EXIT_INTERRUPTED = 130
SAFE_LABEL = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")
DEFAULT_COMPOSITION_CONTRACT = "portusos-build/contracts/build.yaml"
DEFAULT_ISO_PROFILE = "portusos-build/iso/profile.yaml"
CONFIG_SNAPSHOT_FILE = "build-config.json"
ENVIRONMENT_PREFLIGHT_FILE = "preflight.json"
STAGING_EVIDENCE_FILE = "staging-evidence.json"
NATIVE_BUILD_RESULT_FILE = "native-build-result.json"
NATIVE_CLEANUP_FILE = "native-cleanup.json"
REPOSITORY_CLOSURE_FILE = "repository-closure.json"
PACKAGE_PROGRESS_STATES = {
    "pending",
    "reused_verified",
    "downloaded_verified",
    "corrupt_removed",
    "failed",
}
PACKAGE_ACQUISITION_FAILURE_CLASSES = {
    "verification_incomplete",
    "interrupted",
    "timeout",
    "http_not_found",
    "tls",
    "dns",
    "connection",
    "integrity",
    "process_failed",
    "runtime_error",
}
REPOSITORY_CLOSURE_SUBSTAGES = {
    "mirror-selection",
    "repository-sync",
    "resolution",
    "acquisition",
    "cache-verification",
    "local-validation",
}
REPOSITORY_CLOSURE_FAILURE_CAUSES = {
    "interrupted",
    "timeout",
    "tls_eof",
    "tls_failure",
    "http_404",
    "hash_mismatch",
    "signature_failure",
    "dns",
    "connection",
    "no_eligible_mirror",
    "mirror_configuration",
    "repository_unavailable",
    "resolution_incomplete",
    "resolution_mismatch",
    "missing_file",
    "verification_incomplete",
    "process_failed",
    "runtime_error",
}
MIN_INSTALLER_PLAN_DISK_MIB = 40960
MAX_INSTALLER_PLAN_DISK_MIB = 1048576
SECRET_MARKERS = (
    b"-----begin private key-----",
    b"authorization: bearer ",
    b"authorization: basic ",
    b"sk-proj-",
    b"ghp_",
)

def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def compact_utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sanitized_build_env(repo: Path, home: Path) -> dict[str, str]:
    home.mkdir(parents=True, exist_ok=True)
    tool_dirs: list[str] = []
    for name in ("python", "python3", "cargo", "rustc", "git", "npm", "node", "curl", "sudo"):
        found = shutil.which(name)
        if found:
            parent = str(Path(found).resolve().parent)
            if parent not in tool_dirs:
                tool_dirs.append(parent)
    for parent in ("/usr/local/sbin", "/usr/local/bin", "/usr/sbin", "/usr/bin", "/sbin", "/bin"):
        if parent not in tool_dirs:
            tool_dirs.append(parent)
    env = {
        "PATH": os.pathsep.join(tool_dirs),
        "HOME": str(home),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "LANG": os.environ.get("LANG", "C.UTF-8"),
        "LC_ALL": "C.UTF-8",
        "CARGO_HOME": str(repo / "portusos-build/cache/cargo-home"),
        "CARGO_TERM_COLOR": "never",
        "npm_config_cache": str(repo / "portusos-build/cache/npm"),
        "npm_config_update_notifier": "false",
        "npm_config_fund": "false",
        "npm_config_audit": "false",
    }
    original_home = Path.home()
    rustup_home = Path(os.environ.get("RUSTUP_HOME", original_home / ".rustup"))
    if rustup_home.is_dir():
        env["RUSTUP_HOME"] = str(rustup_home)
    for key in (
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "NO_PROXY",
        "https_proxy",
        "http_proxy",
        "no_proxy",
        "NODE_EXTRA_CA_CERTS",
        "SSL_CERT_FILE",
        "CURL_CA_BUNDLE",
        "CARGO_HTTP_CAINFO",
    ):
        value = os.environ.get(key)
        if value:
            env[key] = value
    return env


def run_capture(repo: Path, args: list[str], *, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=repo,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def git_text(repo: Path, args: list[str]) -> str:
    result = run_capture(repo, ["git", *args])
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def read_os_release() -> dict[str, str]:
    path = Path("/etc/os-release")
    values: dict[str, str] = {}
    if not path.is_file():
        return values
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if not raw or raw.startswith("#") or "=" not in raw:
            continue
        key, value = raw.split("=", 1)
        if value.startswith('"') and value.endswith('"'):
            value = value[1:-1].replace(r"\"", '"').replace(r"\\", "\\")
        values[key] = value
    return values


def command_version(repo: Path, command: list[str]) -> str | None:
    try:
        result = run_capture(repo, command)
    except FileNotFoundError:
        return None
    if result.returncode != 0:
        return None
    text = (result.stdout or result.stderr).strip()
    return text.splitlines()[0] if text else None


def tracked_input_manifest(repo: Path) -> list[dict[str, Any]]:
    command = ["git", "ls-files", "--stage", "-z"]
    result = subprocess.run(command, cwd=repo, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.decode("utf-8", errors="replace").strip())
    entries: list[dict[str, Any]] = []
    for raw in result.stdout.split(b"\0"):
        if not raw:
            continue
        header, path_bytes = raw.split(b"\t", 1)
        mode, blob, stage = header.decode("ascii").split(" ")
        relative = path_bytes.decode("utf-8")
        absolute = repo / relative
        if not absolute.is_file():
            raise RuntimeError(f"tracked build input is not a regular file: {relative}")
        entries.append(
            {
                "path": relative,
                "git_mode": mode,
                "git_blob": blob,
                "git_stage": int(stage),
                "size_bytes": absolute.stat().st_size,
                "sha256": sha256_file(absolute),
            }
        )
    entries.sort(key=lambda entry: entry["path"].encode("utf-8"))
    return entries


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def require_exact_keys(value: dict[str, Any], required: set[str], optional: set[str], label: str) -> None:
    actual = set(value)
    missing = required - actual
    unknown = actual - required - optional
    if missing:
        raise ValueError(f"{label} is missing required keys: {', '.join(sorted(missing))}")
    if unknown:
        raise ValueError(f"{label} contains unknown keys: {', '.join(sorted(unknown))}")


def validate_build_config_value(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("build config root must be a JSON object")
    require_exact_keys(value, {"schema_version", "name", "build", "target"}, {"description"}, "build config")
    if value["schema_version"] != SCHEMA_VERSION:
        raise ValueError(f"build config schema_version must be {SCHEMA_VERSION}")
    name = value["name"]
    if not isinstance(name, str) or not SAFE_LABEL.fullmatch(name):
        raise ValueError("build config name must be 1-64 letters/digits/dot/underscore/hyphen")
    description = value.get("description")
    if description is not None and (not isinstance(description, str) or not description.strip()):
        raise ValueError("build config description must be a non-empty string when present")

    build = value["build"]
    if not isinstance(build, dict):
        raise ValueError("build config build must be an object")
    require_exact_keys(build, {"mode", "preflight", "source_cleanliness", "label"}, set(), "build config build")
    if build["mode"] not in {"development", "release_candidate"}:
        raise ValueError("build.mode must be development or release_candidate")
    if build["preflight"] not in {"standard", "full", "skip"}:
        raise ValueError("build.preflight must be standard, full, or skip")
    if build["source_cleanliness"] not in {"require_clean", "allow_dirty"}:
        raise ValueError("build.source_cleanliness must be require_clean or allow_dirty")
    label = build["label"]
    if label is not None and (not isinstance(label, str) or not SAFE_LABEL.fullmatch(label)):
        raise ValueError("build.label must be null or 1-64 letters/digits/dot/underscore/hyphen")
    if build["mode"] == "release_candidate" and build["source_cleanliness"] != "require_clean":
        raise ValueError("release_candidate builds must require a clean source tree")

    target = value["target"]
    if not isinstance(target, dict):
        raise ValueError("build config target must be an object")
    require_exact_keys(target, {"composition_contract", "iso_profile", "installer_plan_disk_mib"}, set(), "build config target")
    if target["composition_contract"] != DEFAULT_COMPOSITION_CONTRACT:
        raise ValueError(f"target.composition_contract must be {DEFAULT_COMPOSITION_CONTRACT} for schema v1")
    if target["iso_profile"] != DEFAULT_ISO_PROFILE:
        raise ValueError(f"target.iso_profile must be {DEFAULT_ISO_PROFILE} for schema v1")
    disk_mib = target["installer_plan_disk_mib"]
    if isinstance(disk_mib, bool) or not isinstance(disk_mib, int):
        raise ValueError("target.installer_plan_disk_mib must be an integer")
    if disk_mib < MIN_INSTALLER_PLAN_DISK_MIB or disk_mib > MAX_INSTALLER_PLAN_DISK_MIB:
        raise ValueError(f"target.installer_plan_disk_mib must be between {MIN_INSTALLER_PLAN_DISK_MIB} and {MAX_INSTALLER_PLAN_DISK_MIB}")
    return value


def load_build_config(repo: Path, supplied_path: str) -> tuple[dict[str, Any], Path, str, bytes, str]:
    raw_path = Path(supplied_path)
    candidate = raw_path if raw_path.is_absolute() else repo / raw_path
    if not candidate.exists() or not candidate.is_file():
        raise ValueError(f"build config is not a regular file: {supplied_path}")
    if candidate.is_symlink():
        raise ValueError("build config must not be a symlink")
    resolved = candidate.resolve()
    try:
        relative = resolved.relative_to(repo.resolve()).as_posix()
    except ValueError as error:
        raise ValueError("build config must live inside the repository") from error
    raw = resolved.read_bytes()
    lower_raw = raw.lower()
    for marker in SECRET_MARKERS:
        if marker in lower_raw:
            raise ValueError("build config contains secret-like material")
    try:
        parsed = json.loads(raw.decode("utf-8-sig"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"invalid build config JSON: {error}") from error
    config = validate_build_config_value(parsed)
    for reference in (config["target"]["composition_contract"], config["target"]["iso_profile"]):
        if not (repo / reference).is_file():
            raise ValueError(f"build config references missing repository file: {reference}")
    return config, resolved, relative, raw, hashlib.sha256(raw).hexdigest()


def validate_package_progress_evidence(report: dict[str, Any]) -> dict[str, Any] | None:
    """Validate A5 package-level closure progress when present, including failed runs."""
    has_progress = "package_progress" in report or "progress_summary" in report
    if not has_progress:
        return None
    progress = report.get("package_progress")
    summary = report.get("progress_summary")
    if not isinstance(progress, list) or not isinstance(summary, dict):
        raise ValueError("repository closure package progress evidence is malformed")
    packages = report.get("packages")
    if not isinstance(packages, list):
        raise ValueError("repository closure package progress lacks package identities")
    packages_by_filename: dict[str, dict[str, Any]] = {}
    for package in packages:
        if not isinstance(package, dict):
            raise ValueError("repository closure package identity is malformed")
        filename = package.get("filename")
        size_bytes = package.get("size_bytes")
        if (
            not isinstance(filename, str)
            or not filename
            or filename in packages_by_filename
            or not isinstance(size_bytes, int)
            or isinstance(size_bytes, bool)
            or size_bytes < 0
        ):
            raise ValueError("repository closure package identity cannot support progress accounting")
        packages_by_filename[filename] = package

    state_totals = {
        state: {"packages": 0, "bytes": 0}
        for state in PACKAGE_PROGRESS_STATES
    }
    verified_packages = verified_bytes = pending_packages = pending_bytes = 0
    progress_filenames: set[str] = set()
    for record in progress:
        if not isinstance(record, dict):
            raise ValueError("repository closure package progress record is malformed")
        filename = record.get("filename")
        state = record.get("state")
        size_bytes = record.get("size_bytes")
        verified = record.get("verified")
        needs_acquisition = record.get("needs_acquisition")
        if (
            not isinstance(filename, str)
            or filename not in packages_by_filename
            or filename in progress_filenames
            or state not in PACKAGE_PROGRESS_STATES
            or not isinstance(size_bytes, int)
            or isinstance(size_bytes, bool)
            or size_bytes < 0
            or not isinstance(verified, bool)
            or not isinstance(needs_acquisition, bool)
        ):
            raise ValueError("repository closure package progress record has invalid state fields")
        package = packages_by_filename[filename]
        for key in ("repository", "name", "version", "filename", "size_bytes"):
            if record.get(key) != package.get(key):
                raise ValueError("repository closure package progress identity drifted from frozen package evidence")
        if state in {"reused_verified", "downloaded_verified"}:
            if verified is not True or needs_acquisition is not False:
                raise ValueError("verified package progress state has inconsistent flags")
        elif verified is not False or needs_acquisition is not True:
            raise ValueError("unresolved package progress state has inconsistent flags")
        failure_class = record.get("failure_class")
        batch = record.get("batch")
        attempt = record.get("attempt")
        mirror = record.get("mirror")
        if state == "reused_verified":
            if any(value is not None for value in (batch, attempt, mirror, failure_class)):
                raise ValueError("reused package progress unexpectedly carries acquisition-attempt state")
        elif state == "downloaded_verified":
            if (
                not isinstance(batch, int)
                or isinstance(batch, bool)
                or batch < 1
                or not isinstance(attempt, int)
                or isinstance(attempt, bool)
                or attempt < 1
                or not isinstance(mirror, str)
                or not mirror.startswith("https://")
                or failure_class is not None
            ):
                raise ValueError("downloaded package progress lacks valid acquisition-attempt evidence")
        elif state == "failed":
            if (
                failure_class not in PACKAGE_ACQUISITION_FAILURE_CLASSES
                or not isinstance(batch, int)
                or isinstance(batch, bool)
                or batch < 1
                or not isinstance(attempt, int)
                or isinstance(attempt, bool)
                or attempt < 1
                or not isinstance(mirror, str)
                or not mirror.startswith("https://")
            ):
                raise ValueError("failed package progress lacks valid acquisition-attempt evidence")
        elif any(value is not None for value in (batch, attempt, mirror, failure_class)):
            raise ValueError("unattempted package progress unexpectedly carries acquisition-attempt state")
        progress_filenames.add(filename)
        state_totals[state]["packages"] += 1
        state_totals[state]["bytes"] += size_bytes
        if verified:
            verified_packages += 1
            verified_bytes += size_bytes
        if needs_acquisition:
            pending_packages += 1
            pending_bytes += size_bytes
    if progress_filenames != set(packages_by_filename):
        raise ValueError("repository closure package progress does not cover the frozen package graph")

    expected_summary = {
        "display": f"{len(progress)} resolved / {verified_packages} verified / {pending_packages} pending",
        "resolved": {
            "packages": len(progress),
            "bytes": sum(record["size_bytes"] for record in progress),
        },
        "verified": {"packages": verified_packages, "bytes": verified_bytes},
        "pending": {"packages": pending_packages, "bytes": pending_bytes},
        "states": state_totals,
    }
    if summary != expected_summary:
        raise ValueError("repository closure package progress summary disagrees with package states")
    return expected_summary


def validate_repository_closure_failure(failure: Any, status: str) -> dict[str, Any] | None:
    """Validate A6 structured failure evidence while retaining historical strings."""
    if status == "pass":
        if failure is not None:
            raise ValueError("passing repository closure report contains a failure")
        return None
    if isinstance(failure, str):
        if not failure.strip():
            raise ValueError("historical repository closure failure string is empty")
        return None
    if not isinstance(failure, dict) or set(failure) != {"substage", "cause", "detail", "context"}:
        raise ValueError("failed repository closure report contains malformed A6 failure evidence")
    substage = failure.get("substage")
    cause = failure.get("cause")
    detail = failure.get("detail")
    context = failure.get("context")
    if substage not in REPOSITORY_CLOSURE_SUBSTAGES or cause not in REPOSITORY_CLOSURE_FAILURE_CAUSES:
        raise ValueError("failed repository closure report contains an invalid A6 substage/cause")
    if not isinstance(detail, str) or not detail.strip() or len(detail) > 1000:
        raise ValueError("failed repository closure report contains invalid A6 detail")
    if not isinstance(context, dict):
        raise ValueError("failed repository closure report contains invalid A6 context")
    allowed_context = {"batch", "attempt", "mirror", "mirrorlist_line"}
    if not set(context).issubset(allowed_context):
        raise ValueError("failed repository closure report contains unknown A6 context keys")
    for key in ("batch", "attempt", "mirrorlist_line"):
        if key in context and (
            not isinstance(context[key], int)
            or isinstance(context[key], bool)
            or context[key] < 1
        ):
            raise ValueError(f"failed repository closure report contains invalid A6 {key}")
    if "mirror" in context and (
        not isinstance(context["mirror"], str)
        or not context["mirror"].startswith("https://")
    ):
        raise ValueError("failed repository closure report contains invalid A6 mirror")
    return copy.deepcopy(failure)


def format_repository_closure_failure_reason(failure: dict[str, Any] | None) -> str:
    base = "native repository/package closure failed before buildiso could complete"
    if failure is None:
        return base
    detail = " ".join(failure["detail"].split())
    diagnosis = f"{failure['substage']} ({failure['cause']})"
    return f"native repository/package closure failed at {diagnosis}: {detail[:500]}"


def validate_repository_closure_report(report: Any, run_id: str) -> str:
    if not isinstance(report, dict):
        raise ValueError("repository closure report must be a JSON object")
    required = {
        "schema_version",
        "run_id",
        "captured_at",
        "status",
        "package_targets",
        "profile_sha256",
        "network_pacman_config",
        "mirrorlist",
        "repositories",
        "packages",
        "cache",
        "frozen_pacman_config",
        "local_validation",
        "failure",
    }
    allowed = required | {"repository_anchor", "package_progress", "progress_summary", "buildiso_gate"}
    if not required.issubset(report) or not set(report).issubset(allowed):
        raise ValueError("repository closure report keys differ from the locked evidence shape")
    if report.get("schema_version") != SCHEMA_VERSION or report.get("run_id") != run_id:
        raise ValueError("repository closure report identity mismatch")
    status = report.get("status")
    if status not in {"pass", "fail"}:
        raise ValueError("repository closure report status is invalid")
    if not isinstance(report.get("package_targets"), list) or not report["package_targets"]:
        raise ValueError("repository closure package_targets must be non-empty")
    if not isinstance(report.get("repositories"), list) or not isinstance(report.get("packages"), list):
        raise ValueError("repository closure repositories/packages must be lists")
    progress_summary = validate_package_progress_evidence(report)
    validate_repository_closure_failure(report.get("failure"), status)
    if status == "pass":
        if not report["packages"] or len(report["repositories"]) != 3:
            raise ValueError("passing repository closure report lacks frozen package/repository evidence")
        if (
            progress_summary is None
            or progress_summary["resolved"]["packages"] != len(report["packages"])
            or progress_summary["verified"]["packages"] != len(report["packages"])
            or progress_summary["pending"]["packages"] != 0
        ):
            raise ValueError("passing repository closure report lacks complete per-package progress evidence")
        buildiso_gate = report.get("buildiso_gate")
        expected_buildiso_gate_keys = {
            "status",
            "all_packages_verified",
            "verified_package_count",
            "repository_databases_immutable",
            "local_repository_constructed",
            "local_resolution_matches",
            "cache_outer_owner_restored",
            "cache_read_only",
        }
        if (
            not isinstance(buildiso_gate, dict)
            or set(buildiso_gate) != expected_buildiso_gate_keys
            or buildiso_gate.get("status") != "pass"
            or buildiso_gate.get("all_packages_verified") is not True
            or buildiso_gate.get("verified_package_count") != len(report["packages"])
            or buildiso_gate.get("repository_databases_immutable") is not True
            or buildiso_gate.get("local_repository_constructed") is not True
            or buildiso_gate.get("local_resolution_matches") is not True
            or buildiso_gate.get("cache_outer_owner_restored") is not True
            or buildiso_gate.get("cache_read_only") is not True
        ):
            raise ValueError("passing repository closure report lacks the complete A7 buildiso security gate")
        anchor = report.get("repository_anchor")
        selected_anchor = anchor.get("selected") if isinstance(anchor, dict) else None
        if (
            not isinstance(anchor, dict)
            or anchor.get("status") != "pass"
            or anchor.get("database_sync_locked") is not True
            or not isinstance(anchor.get("attempts"), list)
            or not isinstance(selected_anchor, dict)
            or not isinstance(selected_anchor.get("server"), str)
            or not selected_anchor["server"].startswith("https://")
        ):
            raise ValueError("passing repository closure report did not prove one locked HTTPS repository anchor")
        frozen = report.get("frozen_pacman_config")
        local = report.get("local_validation")
        cache = report.get("cache")
        if (
            not isinstance(frozen, dict)
            or frozen.get("network_repositories_enabled") is not False
            or not isinstance(frozen.get("server"), str)
            or not frozen["server"].startswith("file://")
        ):
            raise ValueError("passing repository closure report did not prove a local-only file:// pacman configuration")
        if (
            not isinstance(local, dict)
            or local.get("resolution_matches") is not True
            or local.get("package_files_verified") is not True
            or local.get("network_repositories_enabled") is not False
        ):
            raise ValueError("passing repository closure report did not prove local-only package resolution and files")
        if (
            not isinstance(cache, dict)
            or cache.get("read_only_for_buildiso") is not True
            or cache.get("outer_owner_restored") is not True
            or cache.get("verified_count") != len(report["packages"])
        ):
            raise ValueError("passing repository closure report did not prove a verified read-only build cache with restored outer ownership")
        cache_audit = cache.get("audit")
        if not isinstance(cache_audit, dict) or cache_audit.get("status") != "pass":
            raise ValueError("passing repository closure report lacks a successful persistent-cache audit")
        reused_filenames = cache_audit.get("reused_filenames")
        pending_filenames = cache_audit.get("pending_filenames")
        if (
            not isinstance(reused_filenames, list)
            or not all(isinstance(filename, str) and filename for filename in reused_filenames)
            or len(set(reused_filenames)) != len(reused_filenames)
            or not isinstance(pending_filenames, list)
            or not all(isinstance(filename, str) and filename for filename in pending_filenames)
            or len(set(pending_filenames)) != len(pending_filenames)
            or set(reused_filenames) & set(pending_filenames)
            or cache_audit.get("resolved_count") != len(report["packages"])
            or cache_audit.get("reused_count") != len(reused_filenames)
            or cache_audit.get("pending_count") != len(pending_filenames)
            or cache.get("reused_count") != len(reused_filenames)
            or not isinstance(cache_audit.get("corrupt_entries_removed"), list)
            or not isinstance(cache_audit.get("stale_partial_entries_removed"), list)
        ):
            raise ValueError("passing repository closure report contains an invalid persistent-cache audit")
        package_filenames: set[str] = set()
        reused_filename_set = set(reused_filenames)
        for package in report["packages"]:
            if not isinstance(package, dict) or not isinstance(package.get("filename"), str) or not package["filename"]:
                raise ValueError("passing repository closure report contains a package without a filename")
            filename = package["filename"]
            if filename in package_filenames:
                raise ValueError("passing repository closure report contains duplicate package filenames")
            package_filenames.add(filename)
            if package.get("cached_before") is not (filename in reused_filename_set):
                raise ValueError("passing repository closure package cached_before state disagrees with cache audit")
        if package_filenames != reused_filename_set | set(pending_filenames):
            raise ValueError("passing repository closure cache audit does not partition the frozen package closure")
        mirror_limit = cache.get("prefetch_mirror_attempt_limit")
        prefetch_mirrors = cache.get("prefetch_mirrors")
        prefetch_attempts = cache.get("prefetch_attempts")
        batch_count = cache.get("prefetch_batch_count")
        completed_batch_count = cache.get("prefetch_completed_batch_count")
        pending_count = cache.get("prefetch_pending_count")
        if (
            not isinstance(mirror_limit, int)
            or isinstance(mirror_limit, bool)
            or mirror_limit <= 0
            or mirror_limit > 16
            or not isinstance(prefetch_mirrors, list)
            or not prefetch_mirrors
            or len(prefetch_mirrors) > mirror_limit
            or not isinstance(prefetch_attempts, list)
            or not isinstance(batch_count, int)
            or isinstance(batch_count, bool)
            or batch_count < 0
            or completed_batch_count != batch_count
            or pending_count != 0
        ):
            raise ValueError("passing repository closure report did not prove bounded complete package prefetch")
        mirror_servers: list[str] = []
        for mirror in prefetch_mirrors:
            if not isinstance(mirror, dict) or not isinstance(mirror.get("server"), str) or not mirror["server"].startswith("https://"):
                raise ValueError("passing repository closure report contains an invalid prefetch mirror")
            mirror_servers.append(mirror["server"])
        if mirror_servers[0] != selected_anchor["server"] or len(set(mirror_servers)) != len(mirror_servers):
            raise ValueError("passing repository closure report did not prove anchor-first distinct package mirrors")
        attempts_by_batch: dict[int, list[dict[str, Any]]] = {}
        for attempt in prefetch_attempts:
            if not isinstance(attempt, dict):
                raise ValueError("passing repository closure report contains an invalid prefetch attempt")
            batch = attempt.get("batch")
            attempt_number = attempt.get("attempt")
            mirror = attempt.get("mirror")
            requested_count = attempt.get("requested_count")
            requested_bytes = attempt.get("requested_bytes")
            requested_filenames = attempt.get("requested_filenames")
            verified_count = attempt.get("verified_count")
            verified_bytes = attempt.get("verified_bytes")
            verified_filenames = attempt.get("verified_filenames")
            attempt_pending_count = attempt.get("pending_count")
            attempt_pending_bytes = attempt.get("pending_bytes")
            attempt_pending_filenames = attempt.get("pending_filenames")
            failure_class = attempt.get("failure_class")
            if (
                not isinstance(batch, int)
                or isinstance(batch, bool)
                or batch < 1
                or batch > batch_count
                or not isinstance(attempt_number, int)
                or isinstance(attempt_number, bool)
                or attempt_number < 1
                or attempt_number > mirror_limit
                or attempt_number > len(mirror_servers)
                or not isinstance(mirror, dict)
                or mirror.get("server") != mirror_servers[attempt_number - 1]
                or not isinstance(requested_count, int)
                or isinstance(requested_count, bool)
                or requested_count < 1
                or not isinstance(requested_bytes, int)
                or isinstance(requested_bytes, bool)
                or requested_bytes < 0
                or not isinstance(requested_filenames, list)
                or len(requested_filenames) != requested_count
                or not all(isinstance(filename, str) and filename for filename in requested_filenames)
                or len(set(requested_filenames)) != len(requested_filenames)
                or not all(filename in package_filenames for filename in requested_filenames)
                or requested_bytes != sum(
                    package["size_bytes"]
                    for package in report["packages"]
                    if package["filename"] in set(requested_filenames)
                )
                or not isinstance(verified_count, int)
                or isinstance(verified_count, bool)
                or verified_count < 0
                or verified_count > requested_count
                or not isinstance(verified_bytes, int)
                or isinstance(verified_bytes, bool)
                or verified_bytes < 0
                or not isinstance(verified_filenames, list)
                or len(verified_filenames) != verified_count
                or not all(isinstance(filename, str) and filename for filename in verified_filenames)
                or len(set(verified_filenames)) != len(verified_filenames)
                or not set(verified_filenames).issubset(set(requested_filenames))
                or verified_bytes != sum(
                    package["size_bytes"]
                    for package in report["packages"]
                    if package["filename"] in set(verified_filenames)
                )
                or not isinstance(attempt_pending_count, int)
                or isinstance(attempt_pending_count, bool)
                or attempt_pending_count < 0
                or not isinstance(attempt_pending_bytes, int)
                or isinstance(attempt_pending_bytes, bool)
                or attempt_pending_bytes < 0
                or not isinstance(attempt_pending_filenames, list)
                or len(attempt_pending_filenames) != attempt_pending_count
                or not all(isinstance(filename, str) and filename for filename in attempt_pending_filenames)
                or len(set(attempt_pending_filenames)) != len(attempt_pending_filenames)
                or not set(attempt_pending_filenames).issubset(set(requested_filenames))
                or attempt_pending_bytes != sum(
                    package["size_bytes"]
                    for package in report["packages"]
                    if package["filename"] in set(attempt_pending_filenames)
                )
                or attempt.get("result") not in {"pass", "fail"}
                or (attempt.get("result") == "pass" and failure_class is not None)
                or (attempt.get("result") == "fail" and failure_class not in PACKAGE_ACQUISITION_FAILURE_CLASSES)
                or not isinstance(attempt.get("removed_unverified"), list)
            ):
                raise ValueError("passing repository closure report contains malformed bounded mirror-attempt evidence")
            attempts_by_batch.setdefault(batch, []).append(attempt)
        for batch in range(1, batch_count + 1):
            batch_attempts = attempts_by_batch.get(batch, [])
            if not batch_attempts:
                raise ValueError("passing repository closure report lacks mirror-attempt evidence for a downloaded batch")
            if [attempt["attempt"] for attempt in batch_attempts] != list(range(1, len(batch_attempts) + 1)):
                raise ValueError("passing repository closure report has non-contiguous mirror attempts")
            if batch_attempts[-1].get("result") != "pass" or batch_attempts[-1].get("pending_count") != 0:
                raise ValueError("passing repository closure report contains an unfinished prefetch batch")
    return status


def capture_repository_closure_record(
    run_dir: Path,
    metadata: dict[str, Any],
    run_json: Path,
    run_id: str,
) -> tuple[str | None, dict[str, Any] | None]:
    path = run_dir / REPOSITORY_CLOSURE_FILE
    if not path.is_file():
        return None, None
    report = json.loads(path.read_text(encoding="utf-8"))
    status = validate_repository_closure_report(report, run_id)
    failure = report.get("failure") if isinstance(report.get("failure"), dict) else None
    metadata["records"]["repository_closure_sha256"] = sha256_file(path)
    write_json(run_json, metadata)
    return status, copy.deepcopy(failure)


def validate_native_cleanup_report(report: Any, run_id: str) -> str:
    if not isinstance(report, dict):
        raise ValueError("native cleanup report must be a JSON object")
    expected_keys = {
        "schema_version",
        "run_id",
        "status",
        "namespace_action_succeeded",
        "native_context_root",
        "mount_references_before",
        "mount_references_after",
        "process_references_before",
        "process_references_after",
        "helper_processes_before",
        "helper_processes_after",
        "seed_loop_devices_before",
        "seed_loop_devices_after",
        "leaked_seed_loop_devices",
        "scratch_removed",
    }
    if set(report) != expected_keys:
        raise ValueError("native cleanup report keys differ from the locked evidence shape")
    if report.get("schema_version") != SCHEMA_VERSION or report.get("run_id") != run_id:
        raise ValueError("native cleanup report identity mismatch")
    status = report.get("status")
    if status not in {"pass", "fail"}:
        raise ValueError("native cleanup report status is invalid")
    for key in (
        "mount_references_before",
        "mount_references_after",
        "process_references_before",
        "process_references_after",
        "helper_processes_before",
        "helper_processes_after",
        "seed_loop_devices_before",
        "seed_loop_devices_after",
        "leaked_seed_loop_devices",
    ):
        if not isinstance(report.get(key), list):
            raise ValueError(f"native cleanup report {key} must be a list")
    if not isinstance(report.get("namespace_action_succeeded"), bool) or not isinstance(report.get("scratch_removed"), bool):
        raise ValueError("native cleanup report boolean fields are invalid")
    if not isinstance(report.get("native_context_root"), str) or not report["native_context_root"].startswith("portusos-build/work/native-runs/"):
        raise ValueError("native cleanup report context root is outside the run-owned native root")
    if status == "pass" and (
        report["mount_references_after"]
        or report["process_references_after"]
        or report["helper_processes_after"]
        or report["leaked_seed_loop_devices"]
        or not report["scratch_removed"]
    ):
        raise ValueError("native cleanup report claims pass with surviving run resources")
    return status


def validate_environment_report(report: Any, run_id: str, config_sha256: str) -> str:
    if not isinstance(report, dict):
        raise ValueError("preflight report must be a JSON object")
    if report.get("schema_version") != SCHEMA_VERSION:
        raise ValueError("preflight report schema_version mismatch")
    if report.get("run_id") != run_id:
        raise ValueError("preflight report run_id mismatch")
    overall = report.get("overall")
    if overall not in {"pass", "warn", "block"}:
        raise ValueError("preflight report has invalid overall result")
    if report.get("native_build_allowed") != (overall != "block"):
        raise ValueError("preflight report native_build_allowed contradicts overall result")
    config = report.get("build_config")
    if not isinstance(config, dict) or config.get("sha256") != config_sha256:
        raise ValueError("preflight report build-config identity mismatch")
    checks = report.get("checks")
    if not isinstance(checks, list) or not checks:
        raise ValueError("preflight report must contain checks")
    ids: set[str] = set()
    counts = {"pass": 0, "warn": 0, "block": 0}
    for check in checks:
        if not isinstance(check, dict):
            raise ValueError("preflight check must be an object")
        check_id = check.get("id")
        result = check.get("result")
        if not isinstance(check_id, str) or not check_id or check_id in ids:
            raise ValueError("preflight check IDs must be non-empty and unique")
        if result not in counts:
            raise ValueError(f"preflight check {check_id} has invalid result")
        ids.add(check_id)
        counts[result] += 1
    if report.get("summary") != counts:
        raise ValueError("preflight report summary does not match check results")
    derived = "block" if counts["block"] else "warn" if counts["warn"] else "pass"
    if overall != derived:
        raise ValueError("preflight report overall result does not match check results")
    return overall


def append_log(log_path: Path, text: str) -> None:
    with log_path.open("a", encoding="utf-8") as handle:
        handle.write(text)
        if text and not text.endswith("\n"):
            handle.write("\n")


def log_header(log_path: Path, title: str, command: Iterable[str] | None = None) -> None:
    append_log(log_path, f"\n===== {utc_now()} {title} =====")
    if command is not None:
        append_log(log_path, "$ " + " ".join(command))


def record_step(
    metadata: dict[str, Any],
    run_json: Path,
    *,
    name: str,
    command: list[str],
    started_at: str,
    finished_at: str,
    exit_code: int,
) -> None:
    metadata["steps"].append(
        {
            "name": name,
            "command": command,
            "started_at": started_at,
            "finished_at": finished_at,
            "exit_code": exit_code,
        }
    )
    write_json(run_json, metadata)


def run_logged(
    repo: Path,
    log_path: Path,
    metadata: dict[str, Any],
    run_json: Path,
    name: str,
    command: list[str],
    *,
    env: dict[str, str] | None = None,
) -> int:
    started = utc_now()
    log_header(log_path, name, command)
    try:
        result = run_capture(repo, command, env=env)
    except FileNotFoundError as error:
        append_log(log_path, str(error))
        finished = utc_now()
        record_step(metadata, run_json, name=name, command=command, started_at=started, finished_at=finished, exit_code=127)
        return 127
    except KeyboardInterrupt:
        append_log(log_path, "step interrupted by terminal SIGINT")
        finished = utc_now()
        record_step(
            metadata,
            run_json,
            name=name,
            command=command,
            started_at=started,
            finished_at=finished,
            exit_code=EXIT_INTERRUPTED,
        )
        return EXIT_INTERRUPTED
    if result.stdout:
        sys.stdout.write(result.stdout)
        append_log(log_path, result.stdout)
    if result.stderr:
        sys.stderr.write(result.stderr)
        append_log(log_path, result.stderr)
    finished = utc_now()
    record_step(metadata, run_json, name=name, command=command, started_at=started, finished_at=finished, exit_code=result.returncode)
    return result.returncode


def run_json_step(
    repo: Path,
    log_path: Path,
    metadata: dict[str, Any],
    run_json: Path,
    name: str,
    command: list[str],
    output_path: Path,
) -> int:
    started = utc_now()
    log_header(log_path, name, command)
    try:
        result = run_capture(repo, command)
    except FileNotFoundError as error:
        append_log(log_path, str(error))
        finished = utc_now()
        record_step(metadata, run_json, name=name, command=command, started_at=started, finished_at=finished, exit_code=127)
        return 127
    if result.stderr:
        sys.stderr.write(result.stderr)
        append_log(log_path, result.stderr)
    if result.stdout:
        append_log(log_path, result.stdout)
    if result.returncode == 0:
        try:
            parsed = json.loads(result.stdout)
        except json.JSONDecodeError as error:
            append_log(log_path, f"invalid JSON output: {error}")
            result = subprocess.CompletedProcess(result.args, 1, result.stdout, result.stderr)
        else:
            write_json(output_path, parsed)
    finished = utc_now()
    record_step(metadata, run_json, name=name, command=command, started_at=started, finished_at=finished, exit_code=result.returncode)
    return result.returncode


def run_streamed_builder(
    repo: Path,
    log_path: Path,
    metadata: dict[str, Any],
    run_json: Path,
    command: list[str],
    env: dict[str, str],
) -> int:
    started = utc_now()
    log_header(log_path, "native-iso-build", command)
    try:
        process = subprocess.Popen(
            command,
            cwd=repo,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
    except FileNotFoundError as error:
        append_log(log_path, str(error))
        finished = utc_now()
        record_step(metadata, run_json, name="native-iso-build", command=command, started_at=started, finished_at=finished, exit_code=127)
        return 127
    assert process.stdout is not None
    try:
        with log_path.open("a", encoding="utf-8") as handle:
            for line in process.stdout:
                sys.stdout.write(line)
                sys.stdout.flush()
                handle.write(line)
                handle.flush()
        exit_code = process.wait()
    except KeyboardInterrupt:
        # Keep the builder in the terminal foreground process group so the
        # kernel-delivered SIGINT reaches cargo, sudo/root helpers, unshare and
        # buildiso regardless of UID. Then reap the top-level builder with a
        # bounded wait; if it ignores the terminal signal, terminate/kill it.
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        raise
    finished = utc_now()
    record_step(metadata, run_json, name="native-iso-build", command=command, started_at=started, finished_at=finished, exit_code=exit_code)
    return exit_code


def finalize_checksums(run_dir: Path, metadata: dict[str, Any], run_json: Path) -> None:
    write_json(run_json, metadata)
    names = [
        "build-run.json",
        "build.log",
        CONFIG_SNAPSHOT_FILE,
        ENVIRONMENT_PREFLIGHT_FILE,
        "inputs.json",
        "contract-report.json",
        "build-plan.json",
        STAGING_EVIDENCE_FILE,
        NATIVE_BUILD_RESULT_FILE,
        NATIVE_CLEANUP_FILE,
        REPOSITORY_CLOSURE_FILE,
    ]
    artifact = metadata.get("artifact")
    if artifact:
        names.append(artifact["relative_path"])
    entries: list[tuple[str, str]] = []
    for relative in names:
        path = run_dir / relative
        if path.is_file():
            entries.append((relative, sha256_file(path)))
    entries.sort(key=lambda item: item[0].encode("utf-8"))
    checksum_path = run_dir / "RUN_SHA256SUMS"
    checksum_path.write_text("".join(f"{digest}  {name}\n" for name, digest in entries), encoding="utf-8")
    metadata["records"]["run_sha256sums"] = "RUN_SHA256SUMS"
    write_json(run_json, metadata)
    # Recompute because build-run.json gained the checksum reference.
    entries = [(name, sha256_file(run_dir / name)) for name, _ in entries if name != "build-run.json"]
    entries.append(("build-run.json", sha256_file(run_json)))
    entries.sort(key=lambda item: item[0].encode("utf-8"))
    checksum_path.write_text("".join(f"{digest}  {name}\n" for name, digest in entries), encoding="utf-8")


def fail_run(metadata: dict[str, Any], run_json: Path, run_dir: Path, stage: str, reason: str, exit_code: int) -> int:
    metadata["status"] = "blocked" if exit_code in {EXIT_UNRESOLVED, EXIT_DIRTY} else "failed"
    metadata["finished_at"] = utc_now()
    metadata["failure"] = {"stage": stage, "reason": reason, "exit_code": exit_code}
    finalize_checksums(run_dir, metadata, run_json)
    print(f"PortusOS build iteration {metadata['status']}: {metadata['run_id']}", file=sys.stderr)
    print(f"metadata: {run_json}", file=sys.stderr)
    print(f"log: {run_dir / 'build.log'}", file=sys.stderr)
    return exit_code


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="portus-build-iteration-") as raw:
        root = Path(raw)
        sample = root / "sample.bin"
        sample.write_bytes(b"PortusOS iteration harness")
        expected = hashlib.sha256(b"PortusOS iteration harness").hexdigest()
        assert sha256_file(sample) == expected
        assert SAFE_LABEL.fullmatch("first-live.1")
        assert not SAFE_LABEL.fullmatch("bad label")
        value = {"schema_version": SCHEMA_VERSION, "status": "running"}
        path = root / "run.json"
        write_json(path, value)
        assert json.loads(path.read_text(encoding="utf-8")) == value
        closure_fixture = {
            "schema_version": SCHEMA_VERSION,
            "run_id": "run-1",
            "captured_at": "2026-08-29T00:00:00Z",
            "status": "pass",
            "package_targets": ["base"],
            "profile_sha256": {"common": "a" * 64, "portus": "b" * 64},
            "network_pacman_config": {"path": "/usr/share/artools/pacman.conf.d/iso-x86_64.conf", "sha256": "c" * 64},
            "mirrorlist": {"path": "/etc/pacman.d/mirrorlist", "sha256": "d" * 64},
            "repository_anchor": {
                "status": "pass",
                "selected": {"server": "https://mirror.example/artix/$repo/os/$arch", "mirrorlist_line": 1},
                "attempts": [{"server": "https://mirror.example/artix/$repo/os/$arch", "mirrorlist_line": 1, "result": "pass", "detail": None}],
                "candidate_count": 1,
                "database_sync_locked": True,
                "pacman_config_path": "/run/portus-build/repository-closure/anchor-pacman.conf",
                "pacman_config_sha256": "e" * 64,
            },
            "repositories": [{"name": name} for name in ("system", "world", "galaxy")],
            "packages": [
                {
                    "repository": "system",
                    "name": "base",
                    "version": "1-1",
                    "filename": "base-1-1-x86_64.pkg.tar.zst",
                    "size_bytes": 100,
                    "cached_before": False,
                }
            ],
            "package_progress": [
                {
                    "repository": "system",
                    "name": "base",
                    "version": "1-1",
                    "filename": "base-1-1-x86_64.pkg.tar.zst",
                    "size_bytes": 100,
                    "state": "downloaded_verified",
                    "verified": True,
                    "needs_acquisition": False,
                    "batch": 1,
                    "attempt": 1,
                    "mirror": "https://mirror.example/artix/$repo/os/$arch",
                    "failure_class": None,
                }
            ],
            "progress_summary": {
                "display": "1 resolved / 1 verified / 0 pending",
                "resolved": {"packages": 1, "bytes": 100},
                "verified": {"packages": 1, "bytes": 100},
                "pending": {"packages": 0, "bytes": 0},
                "states": {
                    "pending": {"packages": 0, "bytes": 0},
                    "reused_verified": {"packages": 0, "bytes": 0},
                    "downloaded_verified": {"packages": 1, "bytes": 100},
                    "corrupt_removed": {"packages": 0, "bytes": 0},
                    "failed": {"packages": 0, "bytes": 0},
                },
            },
            "cache": {
                "path": "portusos-build/cache/artix-packages",
                "audit": {
                    "status": "pass",
                    "resolved_count": 1,
                    "reused_count": 0,
                    "reused_filenames": [],
                    "pending_count": 1,
                    "pending_filenames": ["base-1-1-x86_64.pkg.tar.zst"],
                    "corrupt_entries_removed": [],
                    "stale_partial_entries_removed": [],
                },
                "reused_count": 0,
                "verified_count": 1,
                "prefetch_batch_count": 1,
                "prefetch_completed_batch_count": 1,
                "prefetch_pending_count": 0,
                "prefetch_mirror_attempt_limit": 4,
                "prefetch_mirrors": [
                    {"server": "https://mirror.example/artix/$repo/os/$arch", "mirrorlist_line": 1}
                ],
                "prefetch_attempts": [
                    {
                        "batch": 1,
                        "attempt": 1,
                        "mirror": {"server": "https://mirror.example/artix/$repo/os/$arch", "mirrorlist_line": 1},
                        "requested_count": 1,
                        "requested_bytes": 100,
                        "requested_filenames": ["base-1-1-x86_64.pkg.tar.zst"],
                        "verified_count": 1,
                        "verified_bytes": 100,
                        "verified_filenames": ["base-1-1-x86_64.pkg.tar.zst"],
                        "pending_count": 0,
                        "pending_bytes": 0,
                        "pending_filenames": [],
                        "removed_unverified": [],
                        "result": "pass",
                        "failure_class": None,
                        "detail": None,
                    }
                ],
                "read_only_for_buildiso": True,
                "outer_owner_restored": True,
            },
            "buildiso_gate": {
                "status": "pass",
                "all_packages_verified": True,
                "verified_package_count": 1,
                "repository_databases_immutable": True,
                "local_repository_constructed": True,
                "local_resolution_matches": True,
                "cache_outer_owner_restored": True,
                "cache_read_only": True,
            },
            "frozen_pacman_config": {
                "server": "file:///run/portus-build/repository-closure/repo",
                "network_repositories_enabled": False,
            },
            "local_validation": {
                "resolution_matches": True,
                "package_files_verified": True,
                "network_repositories_enabled": False,
            },
            "failure": None,
        }
        assert validate_repository_closure_report(closure_fixture, "run-1") == "pass"
        invalid_buildiso_gate_fixture = copy.deepcopy(closure_fixture)
        invalid_buildiso_gate_fixture["buildiso_gate"]["cache_read_only"] = False
        try:
            validate_repository_closure_report(invalid_buildiso_gate_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("passing closure evidence must reject an incomplete A7 buildiso gate")
        current_failed_fixture = copy.deepcopy(closure_fixture)
        current_failed_fixture["status"] = "fail"
        current_failed_fixture["failure"] = {
            "substage": "acquisition",
            "cause": "timeout",
            "detail": "Operation too slow while retrieving package payload",
            "context": {
                "batch": 1,
                "attempt": 1,
                "mirror": "https://mirror.example/artix/$repo/os/$arch",
            },
        }
        current_failed_fixture["local_validation"] = None
        current_failed_fixture["package_progress"][0].update(
            {
                "state": "failed",
                "verified": False,
                "needs_acquisition": True,
                "failure_class": "timeout",
            }
        )
        current_failed_fixture["progress_summary"] = {
            "display": "1 resolved / 0 verified / 1 pending",
            "resolved": {"packages": 1, "bytes": 100},
            "verified": {"packages": 0, "bytes": 0},
            "pending": {"packages": 1, "bytes": 100},
            "states": {
                "pending": {"packages": 0, "bytes": 0},
                "reused_verified": {"packages": 0, "bytes": 0},
                "downloaded_verified": {"packages": 0, "bytes": 0},
                "corrupt_removed": {"packages": 0, "bytes": 0},
                "failed": {"packages": 1, "bytes": 100},
            },
        }
        assert validate_repository_closure_report(current_failed_fixture, "run-1") == "fail"
        assert format_repository_closure_failure_reason(current_failed_fixture["failure"]) == (
            "native repository/package closure failed at acquisition (timeout): "
            "Operation too slow while retrieving package payload"
        )
        invalid_a6_fixture = copy.deepcopy(current_failed_fixture)
        invalid_a6_fixture["failure"]["cause"] = "not-a-cause"
        try:
            validate_repository_closure_report(invalid_a6_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("current A6 closure failure must reject an unknown cause")
        current_failed_fixture["progress_summary"]["pending"]["packages"] = 0
        try:
            validate_repository_closure_report(current_failed_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("failed A5 closure evidence must reject a false package-progress summary")
        invalid_failure_class_fixture = copy.deepcopy(closure_fixture)
        invalid_failure_class_fixture["cache"]["prefetch_attempts"][0]["failure_class"] = "timeout"
        try:
            validate_repository_closure_report(invalid_failure_class_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("passing mirror attempt must not carry a failure class")
        all_reused_fixture = copy.deepcopy(closure_fixture)
        all_reused_fixture["packages"][0]["cached_before"] = True
        all_reused_fixture["package_progress"][0].update(
            {
                "state": "reused_verified",
                "batch": None,
                "attempt": None,
                "mirror": None,
            }
        )
        all_reused_fixture["progress_summary"] = {
            "display": "1 resolved / 1 verified / 0 pending",
            "resolved": {"packages": 1, "bytes": 100},
            "verified": {"packages": 1, "bytes": 100},
            "pending": {"packages": 0, "bytes": 0},
            "states": {
                "pending": {"packages": 0, "bytes": 0},
                "reused_verified": {"packages": 1, "bytes": 100},
                "downloaded_verified": {"packages": 0, "bytes": 0},
                "corrupt_removed": {"packages": 0, "bytes": 0},
                "failed": {"packages": 0, "bytes": 0},
            },
        }
        all_reused_fixture["cache"]["audit"].update(
            {
                "reused_count": 1,
                "reused_filenames": ["base-1-1-x86_64.pkg.tar.zst"],
                "pending_count": 0,
                "pending_filenames": [],
            }
        )
        all_reused_fixture["cache"]["reused_count"] = 1
        all_reused_fixture["cache"]["prefetch_batch_count"] = 0
        all_reused_fixture["cache"]["prefetch_completed_batch_count"] = 0
        all_reused_fixture["cache"]["prefetch_attempts"] = []
        assert validate_repository_closure_report(all_reused_fixture, "run-1") == "pass"
        closure_fixture["cache"]["audit"]["pending_filenames"] = []
        try:
            validate_repository_closure_report(closure_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("repository closure evidence must fail closed on an incomplete cache-audit partition")
        closure_fixture["cache"]["audit"]["pending_filenames"] = ["base-1-1-x86_64.pkg.tar.zst"]
        closure_fixture["packages"][0]["cached_before"] = True
        try:
            validate_repository_closure_report(closure_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("repository closure evidence must fail closed when cached_before disagrees with cache audit")
        closure_fixture["packages"][0]["cached_before"] = False
        closure_fixture["repository_anchor"]["database_sync_locked"] = False
        try:
            validate_repository_closure_report(closure_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("repository closure evidence must fail closed on an unlocked repository anchor")
        closure_fixture["repository_anchor"]["database_sync_locked"] = True
        closure_fixture["cache"]["prefetch_attempts"][0]["mirror"]["server"] = "https://other.example/artix/$repo/os/$arch"
        try:
            validate_repository_closure_report(closure_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("repository closure evidence must fail closed on package-mirror sequence drift")
        closure_fixture["cache"]["prefetch_attempts"][0]["mirror"]["server"] = "https://mirror.example/artix/$repo/os/$arch"
        closure_fixture["cache"]["prefetch_attempts"][0]["attempt"] = 2
        try:
            validate_repository_closure_report(closure_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("repository closure evidence must fail closed when an attempt exceeds the mirror list")
        closure_fixture["cache"]["prefetch_attempts"][0]["attempt"] = 1
        historical_failed_fixture = copy.deepcopy(closure_fixture)
        historical_failed_fixture.pop("repository_anchor")
        historical_failed_fixture.pop("package_progress")
        historical_failed_fixture.pop("progress_summary")
        historical_failed_fixture["status"] = "fail"
        historical_failed_fixture["failure"] = "historical pre-anchor closure failure"
        historical_failed_fixture["local_validation"] = None
        assert validate_repository_closure_report(historical_failed_fixture, "run-1") == "fail"
        closure_fixture["local_validation"] = {"resolution_matches": False, "package_files_verified": True}
        try:
            validate_repository_closure_report(closure_fixture, "run-1")
        except ValueError:
            pass
        else:
            raise AssertionError("repository closure evidence must fail closed on resolution mismatch")

        interrupt_metadata: dict[str, Any] = {"steps": []}
        interrupt_json = root / "interrupt-run.json"
        interrupt_log = root / "interrupt.log"
        write_json(interrupt_json, interrupt_metadata)
        original_run_capture = run_capture

        def interrupting_run_capture(
            _repo: Path,
            _args: list[str],
            *,
            env: dict[str, str] | None = None,
        ) -> subprocess.CompletedProcess[str]:
            del env
            raise KeyboardInterrupt

        try:
            globals()["run_capture"] = interrupting_run_capture
            assert (
                run_logged(
                    root,
                    interrupt_log,
                    interrupt_metadata,
                    interrupt_json,
                    "fixture-interrupt",
                    ["fixture"],
                )
                == EXIT_INTERRUPTED
            )
        finally:
            globals()["run_capture"] = original_run_capture
        assert interrupt_metadata["steps"][-1]["exit_code"] == EXIT_INTERRUPTED
        assert "step interrupted by terminal SIGINT" in interrupt_log.read_text(encoding="utf-8")

        preflight_fixture = {
            "schema_version": SCHEMA_VERSION,
            "run_id": "fixture-run",
            "overall": "warn",
            "native_build_allowed": True,
            "build_config": {"sha256": "a" * 64},
            "summary": {"pass": 1, "warn": 1, "block": 0},
            "checks": [
                {"id": "host.os", "result": "pass"},
                {"id": "resource.disk-headroom", "result": "warn"},
            ],
        }
        assert validate_environment_report(preflight_fixture, "fixture-run", "a" * 64) == "warn"
        cleanup_fixture = {
            "schema_version": SCHEMA_VERSION,
            "run_id": "fixture-run",
            "status": "pass",
            "namespace_action_succeeded": True,
            "native_context_root": "portusos-build/work/native-runs/fixture-run/artix-context",
            "mount_references_before": [],
            "mount_references_after": [],
            "process_references_before": [],
            "process_references_after": [],
            "helper_processes_before": [],
            "helper_processes_after": [],
            "seed_loop_devices_before": ["/dev/loop1"],
            "seed_loop_devices_after": ["/dev/loop1"],
            "leaked_seed_loop_devices": [],
            "scratch_removed": True,
        }
        assert validate_native_cleanup_report(cleanup_fixture, "fixture-run") == "pass"
        bad_cleanup = dict(cleanup_fixture)
        bad_cleanup["mount_references_after"] = [{"pid": 1}]
        try:
            validate_native_cleanup_report(bad_cleanup, "fixture-run")
        except ValueError:
            pass
        else:
            raise AssertionError("cleanup evidence must reject surviving mount references")
        clean_env = sanitized_build_env(root, root / "home")
        for secret_name in ("OPENAI_API_KEY", "GITHUB_TOKEN", "CONTROL_PLANE_API_KEY", "AWS_SECRET_ACCESS_KEY"):
            assert secret_name not in clean_env
        assert "PATH" in clean_env and clean_env["HOME"] == str(root / "home")
        config = {
            "schema_version": SCHEMA_VERSION,
            "name": "first-live",
            "build": {
                "mode": "development",
                "preflight": "standard",
                "source_cleanliness": "require_clean",
                "label": "first-live",
            },
            "target": {
                "composition_contract": DEFAULT_COMPOSITION_CONTRACT,
                "iso_profile": DEFAULT_ISO_PROFILE,
                "installer_plan_disk_mib": 81920,
            },
        }
        assert validate_build_config_value(config) == config
        invalid = json.loads(json.dumps(config))
        invalid["build"]["unexpected"] = True
        try:
            validate_build_config_value(invalid)
        except ValueError:
            pass
        else:
            raise AssertionError("build config validator accepted an unknown field")
    print("build_iteration self-test: PASS")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build a config-driven, traceable PortusOS ISO iteration")
    parser.add_argument("--config", help="repository-relative build config JSON")
    parser.add_argument("--check-config", help="validate a build config and exit without creating a build run")
    parser.add_argument("--self-test", action="store_true", help=argparse.SUPPRESS)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()

    repo = Path(__file__).resolve().parents[1]
    os.chdir(repo)
    if args.check_config:
        try:
            config, _, relative, raw, digest = load_build_config(repo, args.check_config)
        except (ValueError, OSError) as error:
            print(f"invalid build config: {error}", file=sys.stderr)
            return EXIT_USAGE
        print(json.dumps({"valid": True, "path": relative, "sha256": digest, "config": config}, indent=2, sort_keys=True))
        return 0
    if not args.config:
        print("--config is required; see BUILD_HARNESS.md", file=sys.stderr)
        return EXIT_USAGE
    try:
        config, _, config_relative, config_bytes, config_sha256 = load_build_config(repo, args.config)
    except (ValueError, OSError) as error:
        print(f"invalid build config: {error}", file=sys.stderr)
        return EXIT_USAGE

    build_config = config["build"]
    target_config = config["target"]
    release_candidate = build_config["mode"] == "release_candidate"
    preflight_mode = build_config["preflight"]
    build_label = build_config["label"] or config["name"]

    try:
        revision = git_text(repo, ["rev-parse", "HEAD"])
        short_revision = git_text(repo, ["rev-parse", "--short=12", "HEAD"])
        branch = git_text(repo, ["rev-parse", "--abbrev-ref", "HEAD"])
        status_text = git_text(repo, ["status", "--porcelain=v1", "--untracked-files=normal"])
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return 1

    source_clean = not bool(status_text)
    label_suffix = f"-{build_label}" if build_label else ""
    mode = "rc" if release_candidate else "dev"
    base_run_id = f"{compact_utc_now()}-{short_revision}-{mode}{label_suffix}"
    builds_root = repo / "portusos-build" / "out" / "builds"
    builds_root.mkdir(parents=True, exist_ok=True)
    run_id = base_run_id
    ordinal = 2
    while (builds_root / run_id).exists():
        run_id = f"{base_run_id}-{ordinal}"
        ordinal += 1
    run_dir = builds_root / run_id
    artifact_dir = run_dir / "artifacts"
    run_dir.mkdir()
    artifact_dir.mkdir()
    log_path = run_dir / "build.log"
    run_json = run_dir / "build-run.json"
    inputs_json = run_dir / "inputs.json"
    contract_report = run_dir / "contract-report.json"
    build_plan = run_dir / "build-plan.json"
    config_snapshot = run_dir / CONFIG_SNAPSHOT_FILE
    environment_preflight = run_dir / ENVIRONMENT_PREFLIGHT_FILE
    config_snapshot.write_bytes(config_bytes)
    (builds_root / "LATEST").write_text(run_id + "\n", encoding="utf-8")

    os_release = read_os_release()
    metadata: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "run_id": run_id,
        "mode": build_config["mode"],
        "label": build_config["label"],
        "status": "running",
        "started_at": utc_now(),
        "finished_at": None,
        "configuration": {
            "schema_version": config["schema_version"],
            "name": config["name"],
            "source_path": config_relative,
            "snapshot_ref": CONFIG_SNAPSHOT_FILE,
            "sha256": config_sha256,
            "mode": build_config["mode"],
            "label": build_config["label"],
            "preflight": preflight_mode,
            "source_cleanliness": build_config["source_cleanliness"],
            "composition_contract": target_config["composition_contract"],
            "iso_profile": target_config["iso_profile"],
            "installer_plan_disk_mib": target_config["installer_plan_disk_mib"],
        },
        "source": {
            "revision": revision,
            "short_revision": short_revision,
            "branch": branch,
            "tree_clean": source_clean,
            "reproducible_source": source_clean,
            "dirty_status": status_text.splitlines() if status_text else [],
        },
        "outer_host": {
            "system": platform.system(),
            "architecture": platform.machine(),
            "kernel_release": platform.release(),
            "distribution_id": os_release.get("ID"),
            "distribution_version": os_release.get("VERSION_ID"),
            "distribution_pretty_name": os_release.get("PRETTY_NAME"),
            "git_version": command_version(repo, ["git", "--version"]),
            "rustc_version": command_version(repo, ["rustc", "--version"]),
            "cargo_version": command_version(repo, ["cargo", "--version"]),
        },
        "preflight": {
            "environment_required": True,
            "environment_result": None,
            "skipped": preflight_mode == "skip",
            "full": preflight_mode == "full",
        },
        "steps": [],
        "artifact": None,
        "failure": None,
        "records": {
            "build_config": CONFIG_SNAPSHOT_FILE,
            "build_config_sha256": config_sha256,
            "environment_preflight": ENVIRONMENT_PREFLIGHT_FILE,
            "log": "build.log",
            "inputs": "inputs.json",
            "contract_report": "contract-report.json",
            "build_plan": "build-plan.json",
            "staging_evidence": STAGING_EVIDENCE_FILE,
            "native_build_result": NATIVE_BUILD_RESULT_FILE,
            "native_cleanup": NATIVE_CLEANUP_FILE,
            "repository_closure": REPOSITORY_CLOSURE_FILE,
            "run_sha256sums": None,
        },
    }
    write_json(run_json, metadata)
    log_header(log_path, f"PortusOS build iteration {run_id}")
    append_log(log_path, f"build config source: {config_relative}")
    append_log(log_path, f"build config SHA-256: {config_sha256}")
    append_log(log_path, "build config resolved value:")
    append_log(log_path, json.dumps(config, indent=2, sort_keys=True))

    try:
        inputs = tracked_input_manifest(repo)
        write_json(
            inputs_json,
            {
                "schema_version": SCHEMA_VERSION,
                "source_revision": revision,
                "source_tree_clean": source_clean,
                "tracked_inputs": inputs,
            },
        )
        metadata["records"]["inputs_sha256"] = sha256_file(inputs_json)
        write_json(run_json, metadata)
    except (RuntimeError, OSError) as error:
        append_log(log_path, f"input manifest failure: {error}")
        return fail_run(metadata, run_json, run_dir, "input-manifest", str(error), 1)

    environment_command = [
        "python",
        "-B",
        "scripts/build_environment_preflight.py",
        "--repo",
        ".",
        "--config",
        str(config_snapshot),
        "--expected-config-sha256",
        config_sha256,
        "--config-source",
        config_relative,
        "--run-id",
        run_id,
        "--output",
        str(environment_preflight),
    ]
    exit_code = run_logged(
        repo,
        log_path,
        metadata,
        run_json,
        "environment-preflight",
        environment_command,
    )
    if environment_preflight.is_file():
        try:
            environment_report = json.loads(environment_preflight.read_text(encoding="utf-8"))
            environment_result = validate_environment_report(environment_report, run_id, config_sha256)
            metadata["preflight"]["environment_result"] = environment_result
            metadata["records"]["environment_preflight_sha256"] = sha256_file(environment_preflight)
            write_json(run_json, metadata)
        except (OSError, json.JSONDecodeError, ValueError) as error:
            append_log(log_path, f"environment preflight report failure: {error}")
            return fail_run(metadata, run_json, run_dir, "environment-preflight", f"invalid environment preflight report: {error}", 1)
    if exit_code != 0:
        reason = "mandatory environment preflight blocked native construction" if exit_code in {EXIT_UNRESOLVED, EXIT_DIRTY} else "mandatory environment preflight failed"
        return fail_run(metadata, run_json, run_dir, "environment-preflight", reason, exit_code)

    if preflight_mode != "skip":
        preflight_commands = [
            ("check", ["bash", "scripts/check.sh"]),
            ("build-contract-tests", ["cargo", "test", "--locked", "-p", "portus-build-contract", "--all-targets"]),
            ("build-skeleton-tests", ["cargo", "test", "--locked", "-p", "portus-build", "--all-targets"]),
        ]
        if preflight_mode == "full":
            preflight_commands.extend(
                [
                    ("linux-integration", ["bash", "scripts/test.sh", "integration"]),
                    ("hardening", ["bash", "scripts/test.sh", "hardening"]),
                ]
            )
        for name, command in preflight_commands:
            exit_code = run_logged(repo, log_path, metadata, run_json, name, command)
            if exit_code != 0:
                return fail_run(metadata, run_json, run_dir, name, f"preflight step {name} failed", exit_code)

    exit_code = run_json_step(
        repo,
        log_path,
        metadata,
        run_json,
        "contract-report",
        ["cargo", "run", "--locked", "-q", "-p", "portus-build-contract", "--", "."],
        contract_report,
    )
    if exit_code != 0:
        return fail_run(metadata, run_json, run_dir, "contract-report", "build contract report generation failed", exit_code)

    exit_code = run_json_step(
        repo,
        log_path,
        metadata,
        run_json,
        "build-plan",
        ["cargo", "run", "--locked", "-q", "-p", "portus-build", "--", "plan", "--disk-size-mib", str(target_config["installer_plan_disk_mib"])],
        build_plan,
    )
    if exit_code != 0:
        return fail_run(metadata, run_json, run_dir, "build-plan", "build plan generation failed", exit_code)

    native_work_root = repo / "portusos-build/work/native-runs" / run_id
    staging_evidence = run_dir / STAGING_EVIDENCE_FILE
    harness_home = repo / "portusos-build/work/harness-homes" / run_id
    stage_env = sanitized_build_env(repo, harness_home)
    stage_command = [
        "python",
        "-B",
        "scripts/artix/stage_first_iso.py",
        "--repo",
        ".",
        "--run-id",
        run_id,
        "--work-root",
        native_work_root.relative_to(repo).as_posix(),
        "--evidence",
        staging_evidence.relative_to(repo).as_posix(),
    ]
    exit_code = run_logged(
        repo,
        log_path,
        metadata,
        run_json,
        "first-iso-staging",
        stage_command,
        env=stage_env,
    )
    if exit_code != 0:
        return fail_run(metadata, run_json, run_dir, "first-iso-staging", "first-ISO payload staging failed", exit_code)
    if not staging_evidence.is_file():
        return fail_run(metadata, run_json, run_dir, "first-iso-staging", "staging completed without evidence manifest", 1)
    metadata["records"]["staging_evidence_sha256"] = sha256_file(staging_evidence)
    write_json(run_json, metadata)

    env = sanitized_build_env(repo, harness_home)
    env["PORTUS_BUILD_RUN_ID"] = run_id
    env["PORTUS_BUILD_RUN_DIR"] = str(run_dir)
    env["PORTUS_BUILD_ARTIFACT_DIR"] = str(artifact_dir)
    env["PORTUS_BUILD_CONFIG"] = str(config_snapshot)
    env["PORTUS_BUILD_CONFIG_SHA256"] = config_sha256
    env["PORTUS_BUILD_COMPOSITION_CONTRACT"] = target_config["composition_contract"]
    env["PORTUS_BUILD_ISO_PROFILE"] = target_config["iso_profile"]
    env["PORTUS_BUILD_INSTALLER_PLAN_DISK_MIB"] = str(target_config["installer_plan_disk_mib"])
    env["PORTUS_BUILD_MODE"] = build_config["mode"]
    env["PORTUS_BUILD_STAGING_MANIFEST"] = str(staging_evidence)
    build_command = ["cargo", "run", "--locked", "-q", "-p", "portus-build", "--", "build-iso"]
    if release_candidate:
        build_command.append("--release-candidate")

    try:
        exit_code = run_streamed_builder(repo, log_path, metadata, run_json, build_command, env)
    except KeyboardInterrupt:
        cleanup_path = run_dir / NATIVE_CLEANUP_FILE
        try:
            capture_repository_closure_record(run_dir, metadata, run_json, run_id)
        except (OSError, json.JSONDecodeError, ValueError) as error:
            append_log(log_path, f"repository closure evidence invalid after interruption: {error}")
        if cleanup_path.is_file():
            try:
                cleanup_report = json.loads(cleanup_path.read_text(encoding="utf-8"))
                validate_native_cleanup_report(cleanup_report, run_id)
                metadata["records"]["native_cleanup_sha256"] = sha256_file(cleanup_path)
                write_json(run_json, metadata)
            except (OSError, json.JSONDecodeError, ValueError) as error:
                append_log(log_path, f"native cleanup evidence invalid after interruption: {error}")
        append_log(log_path, "build interrupted by user; terminal SIGINT was allowed to reach the native tree and the top-level builder was reaped")
        return fail_run(metadata, run_json, run_dir, "native-iso-build", "interrupted", EXIT_INTERRUPTED)

    repository_closure_status: str | None = None
    repository_closure_failure: dict[str, Any] | None = None
    try:
        repository_closure_status, repository_closure_failure = capture_repository_closure_record(
            run_dir,
            metadata,
            run_json,
            run_id,
        )
    except (OSError, json.JSONDecodeError, ValueError) as error:
        append_log(log_path, f"repository closure evidence invalid: {error}")
        return fail_run(metadata, run_json, run_dir, "repository-closure", f"invalid repository closure evidence: {error}", 1)

    cleanup_path = run_dir / NATIVE_CLEANUP_FILE
    cleanup_status: str | None = None
    if cleanup_path.is_file():
        try:
            cleanup_report = json.loads(cleanup_path.read_text(encoding="utf-8"))
            cleanup_status = validate_native_cleanup_report(cleanup_report, run_id)
            metadata["records"]["native_cleanup_sha256"] = sha256_file(cleanup_path)
            write_json(run_json, metadata)
        except (OSError, json.JSONDecodeError, ValueError) as error:
            append_log(log_path, f"native cleanup evidence invalid: {error}")
            return fail_run(metadata, run_json, run_dir, "native-cleanup", f"invalid native cleanup evidence: {error}", 1)

    if exit_code != 0:
        if repository_closure_status == "fail":
            return fail_run(
                metadata,
                run_json,
                run_dir,
                "repository-closure",
                format_repository_closure_failure_reason(repository_closure_failure),
                exit_code,
            )
        reason = (
            "native ISO construction blocked by an unresolved runtime or owner-authorization prerequisite"
            if exit_code == EXIT_UNRESOLVED
            else "native ISO build failed"
        )
        return fail_run(metadata, run_json, run_dir, "native-iso-build", reason, exit_code)

    if repository_closure_status != "pass":
        reason = "successful native builder did not prove a coherent local-only Artix repository/package closure"
        append_log(log_path, reason)
        return fail_run(metadata, run_json, run_dir, "repository-closure", reason, 1)

    if cleanup_status != "pass":
        reason = "successful native builder did not prove zero leaked mounts/process references/seed loop devices and removal of run-scoped Artix scratch"
        append_log(log_path, reason)
        return fail_run(metadata, run_json, run_dir, "native-cleanup", reason, 1)

    native_result = run_dir / NATIVE_BUILD_RESULT_FILE
    if native_result.is_file():
        metadata["records"]["native_build_result_sha256"] = sha256_file(native_result)
        write_json(run_json, metadata)

    artifacts = sorted(path for path in artifact_dir.rglob("*.iso") if path.is_file())
    if len(artifacts) != 1:
        reason = f"successful builder must leave exactly one .iso under {artifact_dir}; found {len(artifacts)}"
        append_log(log_path, reason)
        return fail_run(metadata, run_json, run_dir, "artifact-discovery", reason, 1)

    artifact = artifacts[0]
    relative_artifact = artifact.relative_to(run_dir).as_posix()
    metadata["artifact"] = {
        "relative_path": relative_artifact,
        "filename": artifact.name,
        "sha256": sha256_file(artifact),
        "size_bytes": artifact.stat().st_size,
    }
    metadata["status"] = "succeeded"
    metadata["finished_at"] = utc_now()
    finalize_checksums(run_dir, metadata, run_json)
    print(f"PortusOS build iteration succeeded: {run_id}")
    print(f"artifact: {artifact}")
    print(f"metadata: {run_json}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
