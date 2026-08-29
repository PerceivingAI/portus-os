#!/usr/bin/env python3
"""Mandatory PortusOS native-build environment preflight.

This checker is intentionally cheap, non-interactive, and non-secret-bearing. It
proves that the outer build host and currently selected build inputs are ready
before the harness enters expensive native ISO construction. The exact Artix
context probe is expected to evolve in the same change that resolves the L2
adapter contract; until then the Artix-specific checks fail closed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from build_iteration import validate_build_config_value

SCHEMA_VERSION = 1
EXIT_DIRTY = 65
EXIT_BLOCKED = 78
GIB = 1024**3
RECOMMENDED_FREE_GIB = 30
ADAPTER_PATH = "portusos-build/iso/artools-profile/adapter.yaml"
ARTIX_BOOTSTRAP_PATH = "portusos-build/artix/bootstrap.json"
PACKAGES_PATH = "portusos-build/packages/packages.yaml"
CODEX_PATH = "portusos-build/components/codex.yaml"
PORTUS_BROWSER_PATH = "portusos-build/components/portus-browser.yaml"
PORTUS_MCP_PATH = "portusos-build/components/portus-mcp.yaml"
TUNNEL_CLIENT_PATH = "portusos-build/components/tunnel-client.yaml"
GENERATED_ROOTS = (
    "portusos-build/work",
    "portusos-build/cache",
    "portusos-build/out",
)


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def run_capture(args: list[str], *, cwd: Path) -> subprocess.CompletedProcess[str] | None:
    try:
        return subprocess.run(
            args,
            cwd=cwd,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except FileNotFoundError:
        return None


def first_line(result: subprocess.CompletedProcess[str] | None) -> str | None:
    if result is None or result.returncode != 0:
        return None
    text = (result.stdout or result.stderr).strip()
    return text.splitlines()[0] if text else None


def parse_version(text: str | None) -> tuple[int, ...] | None:
    if not text:
        return None
    match = re.search(r"(?<!\d)(\d+)\.(\d+)(?:\.(\d+))?", text)
    if not match:
        return None
    return tuple(int(part) for part in match.groups(default="0"))


def version_at_least(actual: tuple[int, ...], minimum: tuple[int, ...]) -> bool:
    width = max(len(actual), len(minimum))
    return actual + (0,) * (width - len(actual)) >= minimum + (0,) * (width - len(minimum))


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def yaml_scalar(text: str, key: str) -> str | None:
    match = re.search(rf"(?m)^\s*{re.escape(key)}:\s*([^#\n]+?)\s*$", text)
    return match.group(1).strip().strip('"\'') if match else None


def nested_resolution(text: str, section: str) -> str | None:
    match = re.search(
        rf"(?ms)^\s*{re.escape(section)}:\s*\n(?:(?:\s{{2,}}.*\n))*?\s{{2,}}resolution:\s*([^#\n]+?)\s*$",
        text,
    )
    return match.group(1).strip().strip('"\'') if match else None


def add_check(
    checks: list[dict[str, Any]],
    check_id: str,
    requirement_class: str,
    result: str,
    observed: str | None,
    reason: str,
    remediation: str | None = None,
) -> None:
    checks.append(
        {
            "id": check_id,
            "requirement_class": requirement_class,
            "result": result,
            "observed": observed,
            "reason": reason,
            "remediation": remediation,
        }
    )


def check_command(
    checks: list[dict[str, Any]],
    command: str,
    requirement_class: str,
    *,
    required: bool = True,
    version_args: list[str] | None = None,
    cwd: Path,
) -> str | None:
    path = shutil.which(command)
    check_id = f"command.{command.replace('-', '_')}"
    if path is None:
        add_check(
            checks,
            check_id,
            requirement_class,
            "block" if required else "warn",
            None,
            f"required command {command} is not installed or not on PATH" if required else f"optional/currently-deferred command {command} is not installed or not on PATH",
            f"install/provide {command} using the outer-host build requirements before native construction" if required else f"provide {command} if the selected adapter/test path requires it",
        )
        return None
    observed = path
    if version_args is not None:
        version = first_line(run_capture([command, *version_args], cwd=cwd))
        if version:
            observed = f"{path} :: {version}"
    add_check(checks, check_id, requirement_class, "pass", observed, f"{command} is available")
    return path


def scan_process_mountinfo(markers: list[Path]) -> list[dict[str, Any]]:
    """Return process mount tables containing any exact build-path marker."""
    marker_text = [str(path.resolve()) for path in markers]
    references: list[dict[str, Any]] = []
    proc = Path("/proc")
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            mountinfo = (entry / "mountinfo").read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        matched = [marker for marker in marker_text if marker in mountinfo]
        if not matched:
            continue
        try:
            comm = (entry / "comm").read_text(encoding="utf-8").strip()
        except OSError:
            comm = "?"
        try:
            namespace = os.readlink(entry / "ns/mnt")
        except OSError:
            namespace = None
        references.append({"pid": int(entry.name), "comm": comm, "mount_namespace": namespace, "markers": matched})
    return references


def check_writable_roots(repo: Path, checks: list[dict[str, Any]]) -> None:
    failures: list[str] = []
    observed: list[str] = []
    for relative in GENERATED_ROOTS:
        root = repo / relative
        try:
            root.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(prefix=".portus-preflight-", dir=root, delete=True) as handle:
                handle.write(b"portus")
                handle.flush()
            observed.append(relative)
        except OSError as error:
            failures.append(f"{relative}: {error}")
    if failures:
        add_check(
            checks,
            "host.generated-roots",
            "outer-host",
            "block",
            "; ".join(failures),
            "one or more bounded generated roots cannot be created/written",
            "fix ownership/permissions for portusos-build/work, cache, and out; do not redirect build state elsewhere",
        )
    else:
        add_check(
            checks,
            "host.generated-roots",
            "outer-host",
            "pass",
            ", ".join(observed),
            "all bounded generated roots are writable by the build user",
        )


def read_os_release() -> dict[str, str]:
    path = Path("/etc/os-release")
    values: dict[str, str] = {}
    if not path.is_file():
        return values
    for raw in read_text(path).splitlines():
        if not raw or raw.startswith("#") or "=" not in raw:
            continue
        key, value = raw.split("=", 1)
        values[key] = value.strip().strip('"')
    return values


def check_pacman_contamination(checks: list[dict[str, Any]], os_release: dict[str, str]) -> None:
    distro = os_release.get("ID", "unknown").lower()
    if distro == "artix":
        add_check(
            checks,
            "host.artix-repository-contamination",
            "contamination",
            "pass",
            "outer host identifies as Artix",
            "Artix repository markers are not cross-distribution contamination on an Artix outer host",
        )
        return
    paths = [Path("/etc/pacman.conf")]
    pacman_d = Path("/etc/pacman.d")
    if pacman_d.is_dir():
        paths.extend(sorted(path for path in pacman_d.iterdir() if path.is_file()))
    markers = ("artixlinux.org", "artix-mirrorlist", "mirrorlist-artix", "packages.artixlinux.org")
    hits: list[str] = []
    for path in paths:
        try:
            lower = read_text(path).lower()
        except OSError:
            continue
        if any(marker in lower for marker in markers):
            hits.append(str(path))
    if hits:
        add_check(
            checks,
            "host.artix-repository-contamination",
            "contamination",
            "block",
            ", ".join(hits),
            "non-Artix outer host contains explicit Artix repository/mirror configuration",
            "remove Artix repository configuration from the outer host and keep Artix repositories inside the isolated build context",
        )
    else:
        add_check(
            checks,
            "host.artix-repository-contamination",
            "contamination",
            "pass",
            distro,
            "no explicit Artix repository markers were found in the outer host pacman configuration",
        )


def build_checks(repo: Path, config: dict[str, Any], config_path: Path, config_sha256: str, run_id: str) -> list[dict[str, Any]]:
    checks: list[dict[str, Any]] = []
    build = config["build"]
    target = config["target"]

    system = platform.system()
    add_check(
        checks,
        "host.os",
        "outer-host",
        "pass" if system == "Linux" else "block",
        system,
        "outer build host is Linux" if system == "Linux" else "native PortusOS ISO construction is supported only on Linux",
        None if system == "Linux" else "run the build harness on a native x86_64 Linux outer host",
    )
    machine = platform.machine()
    add_check(
        checks,
        "host.architecture",
        "outer-host",
        "pass" if machine == "x86_64" else "block",
        machine,
        "outer build host architecture is x86_64" if machine == "x86_64" else "first-ISO native construction requires x86_64",
        None if machine == "x86_64" else "use an x86_64 Linux build host",
    )

    if hasattr(os, "geteuid") and os.geteuid() == 0:
        add_check(
            checks,
            "host.execution-user",
            "privilege",
            "block",
            "uid=0",
            "the whole build harness must not run as root merely for convenience",
            "run the harness as the normal build user; allow the adapter to request only narrow privileged operations when required",
        )
    else:
        uid = str(os.geteuid()) if hasattr(os, "geteuid") else "non-Windows runtime"
        add_check(checks, "host.execution-user", "privilege", "pass", f"uid={uid}", "harness is not running as root")

    python_version = platform.python_version()
    python_ok = sys.version_info >= (3, 10)
    add_check(
        checks,
        "command.python",
        "bootstrap",
        "pass" if python_ok else "block",
        f"{sys.executable} :: Python {python_version}",
        "Python satisfies the 3.10 minimum" if python_ok else "Python is older than the 3.10 harness minimum",
        None if python_ok else "install/use Python 3.10 or newer",
    )

    check_command(checks, "git", "bootstrap", version_args=["--version"], cwd=repo)
    check_command(checks, "sh", "bootstrap", cwd=repo)
    check_command(checks, "bash", "bootstrap", version_args=["--version"], cwd=repo)
    rustc_path = check_command(checks, "rustc", "outer-host", version_args=["--version"], cwd=repo)
    check_command(checks, "cargo", "outer-host", version_args=["--version"], cwd=repo)
    check_command(checks, "stat", "outer-host", cwd=repo)
    check_command(checks, "sha256sum", "outer-host", version_args=["--version"], cwd=repo)
    check_command(checks, "curl", "input-availability", version_args=["--version"], cwd=repo)
    check_command(checks, "node", "outer-host", version_args=["--version"], cwd=repo)
    check_command(checks, "npm", "outer-host", version_args=["--version"], cwd=repo)
    check_command(checks, "unshare", "privilege", version_args=["--version"], cwd=repo)
    check_command(checks, "sudo", "privilege", required=False, version_args=["--version"], cwd=repo)

    rust_minimum: tuple[int, ...] | None = None
    cargo_toml = repo / "Cargo.toml"
    try:
        workspace = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
        rust_minimum_text = workspace["workspace"]["package"]["rust-version"]
        rust_minimum = parse_version(str(rust_minimum_text))
    except (OSError, KeyError, tomllib.TOMLDecodeError):
        rust_minimum_text = None
    rust_line = first_line(run_capture(["rustc", "--version"], cwd=repo)) if rustc_path else None
    rust_actual = parse_version(rust_line)
    if rust_minimum is None:
        add_check(
            checks,
            "host.rust-minimum",
            "outer-host",
            "block",
            rust_line,
            "could not determine the repository Rust minimum from Cargo.toml",
            "restore a valid workspace.package.rust-version declaration",
        )
    elif rust_actual is None or not version_at_least(rust_actual, rust_minimum):
        add_check(
            checks,
            "host.rust-minimum",
            "outer-host",
            "block",
            rust_line,
            f"Rust does not satisfy repository minimum {rust_minimum_text}",
            f"install/use rustc and Cargo {rust_minimum_text} or newer",
        )
    else:
        add_check(
            checks,
            "host.rust-minimum",
            "outer-host",
            "pass",
            f"{rust_line}; minimum={rust_minimum_text}",
            "Rust satisfies the repository minimum",
        )

    test_tools_required = build["preflight"] != "skip"
    check_command(checks, "rustfmt", "outer-host", required=test_tools_required, version_args=["--version"], cwd=repo)
    clippy = run_capture(["cargo", "clippy", "--version"], cwd=repo) if shutil.which("cargo") else None
    clippy_line = first_line(clippy)
    if clippy_line:
        add_check(checks, "command.cargo_clippy", "outer-host", "pass", clippy_line, "Cargo Clippy is available")
    else:
        add_check(
            checks,
            "command.cargo_clippy",
            "outer-host",
            "block" if test_tools_required else "warn",
            None,
            "Cargo Clippy is required by standard/full repository preflight" if test_tools_required else "Cargo Clippy is absent but test preflight is explicitly skipped",
            "install the Clippy component for the selected Rust toolchain" if test_tools_required else "install Clippy before using standard/full preflight",
        )

    required_paths = [
        target["composition_contract"],
        target["iso_profile"],
        ADAPTER_PATH,
        ARTIX_BOOTSTRAP_PATH,
        "scripts/artix/context.py",
        "scripts/artix/stage_first_iso.py",
        PACKAGES_PATH,
        CODEX_PATH,
        PORTUS_BROWSER_PATH,
        PORTUS_MCP_PATH,
        TUNNEL_CLIENT_PATH,
        "Cargo.toml",
        "Cargo.lock",
        "scripts/check.sh",
    ]
    missing = [relative for relative in required_paths if not (repo / relative).is_file()]
    add_check(
        checks,
        "source.required-paths",
        "bootstrap",
        "block" if missing else "pass",
        ", ".join(missing) if missing else f"{len(required_paths)} required source paths present",
        "selected build source paths are missing" if missing else "selected build source paths exist",
        "restore the missing tracked build source paths" if missing else None,
    )

    status = run_capture(["git", "status", "--porcelain=v1", "--untracked-files=normal"], cwd=repo)
    dirty_lines = status.stdout.splitlines() if status is not None and status.returncode == 0 else ["git status unavailable"]
    if build["source_cleanliness"] == "require_clean" and dirty_lines:
        add_check(
            checks,
            "source.cleanliness",
            "bootstrap",
            "block",
            f"{len(dirty_lines)} dirty/untracked entries",
            "selected build config requires a clean Git tree",
            "commit/stash/remove the changes, or use an explicitly development-only config with source_cleanliness=allow_dirty",
        )
    elif dirty_lines:
        add_check(
            checks,
            "source.cleanliness",
            "bootstrap",
            "warn",
            f"{len(dirty_lines)} dirty/untracked entries",
            "development config explicitly permits a non-reproducible dirty build",
            "commit the changes before producing an iteration intended for reproducible comparison",
        )
    else:
        add_check(checks, "source.cleanliness", "bootstrap", "pass", "clean", "source cleanliness satisfies the selected config")

    add_check(
        checks,
        "config.identity",
        "bootstrap",
        "pass",
        f"{config_path} sha256={config_sha256}",
        "build config parsed successfully and its exact identity is known",
    )

    check_writable_roots(repo, checks)

    usage = shutil.disk_usage(repo)
    free_gib = usage.free / GIB
    if free_gib < RECOMMENDED_FREE_GIB:
        add_check(
            checks,
            "resource.disk-headroom",
            "resource",
            "warn",
            f"{free_gib:.1f} GiB free",
            f"free space is below the provisional {RECOMMENDED_FREE_GIB}-GiB recommended headroom; no measured hard floor is locked yet",
            "reclaim/expand build storage before repeated cold builds if possible; replace this warning threshold with measured evidence after the first real artools builds",
        )
    else:
        add_check(
            checks,
            "resource.disk-headroom",
            "resource",
            "pass",
            f"{free_gib:.1f} GiB free",
            f"free space meets the provisional {RECOMMENDED_FREE_GIB}-GiB recommended headroom",
        )

    cpu_count = os.cpu_count()
    add_check(checks, "resource.cpu", "resource", "pass", str(cpu_count) if cpu_count is not None else "unknown", "logical CPU count measured; no hard minimum is locked yet")
    memory_observed = "unknown"
    try:
        match = re.search(r"(?m)^MemTotal:\s+(\d+)\s+kB$", read_text(Path("/proc/meminfo")))
        if match:
            memory_observed = f"{int(match.group(1)) / 1024 / 1024:.1f} GiB"
    except OSError:
        pass
    add_check(checks, "resource.memory", "resource", "pass", memory_observed, "memory capacity measured where available; no hard minimum is locked yet")

    os_release = read_os_release()
    check_pacman_contamination(checks, os_release)

    bootstrap_path = repo / ARTIX_BOOTSTRAP_PATH
    bootstrap: dict[str, Any] | None = None
    try:
        bootstrap = json.loads(bootstrap_path.read_text(encoding="utf-8"))
        seed = bootstrap["seed"]
        context_config = bootstrap["context"]
        seed_path = repo / seed["cache_path"]
        seed_ok = (
            seed_path.is_file()
            and seed_path.stat().st_size == seed["size_bytes"]
            and sha256_file(seed_path) == seed["sha256"]
        )
        add_check(
            checks,
            "artix.seed",
            "isolated-artix",
            "pass" if seed_ok else "block",
            f"{seed_path} sha256={seed['sha256']}" if seed_ok else str(seed_path),
            "verified official Artix OpenRC live seed is present" if seed_ok else "verified Artix OpenRC live seed is missing or differs from the locked identity",
            None if seed_ok else "run python -B scripts/artix/context.py verify-seed after acquiring the locked seed; never bootstrap from an unverified ISO",
        )

        state_path = repo / context_config["state_path"]
        state: dict[str, Any] | None = None
        if state_path.is_file():
            try:
                state = json.loads(state_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                state = None
        context_root = repo / context_config["work_root"] / context_config["root_mount"]
        mount_result = run_capture(["findmnt", "-n", "-T", str(context_root)], cwd=repo)
        mounted = False
        if mount_result is not None and mount_result.returncode == 0 and mount_result.stdout.strip():
            target = mount_result.stdout.strip().split()[0]
            try:
                mounted = Path(target).resolve() == context_root.resolve()
            except OSError:
                mounted = target == str(context_root)
        state_identity_ok = bool(
            state
            and state.get("ready") is True
            and state.get("method") == bootstrap.get("method")
            and state.get("os_id") == "artix"
            and state.get("architecture") == "x86_64"
            and isinstance(state.get("seed"), dict)
            and state["seed"].get("sha256") == seed["sha256"]
        )
        upper_path = repo / context_config["work_root"] / context_config["upper_dir"]
        persistent_state_ok = upper_path.is_dir() and state_identity_ok
        context_ok = seed_ok and persistent_state_ok and not mounted
        if mounted:
            context_remediation = "remove the legacy global Artix mount tree before building; private namespace execution must start from a clean host mount table"
        else:
            context_remediation = "run the bounded privileged bootstrap once: sudo python -B scripts/artix/context.py prepare"
        add_check(
            checks,
            "artix.context",
            "isolated-artix",
            "pass" if context_ok else "block",
            f"state={state_path.relative_to(repo)} persistent_upper={upper_path.is_dir()} globally_mounted={mounted}",
            "verified Artix/x86_64 persistent build state is ready and no Artix mounts are exposed in the outer VM" if context_ok else "isolated Artix persistent state is missing, invalid, or still globally mounted",
            None if context_ok else context_remediation,
        )
        canonical_work_root = repo / context_config["work_root"]
        canonical_upper = canonical_work_root / context_config["upper_dir"]
        canonical_overlay_work = canonical_work_root / context_config["overlay_work_dir"]
        historical_refs = scan_process_mountinfo([canonical_upper, canonical_overlay_work])
        add_check(
            checks,
            "artix.historical-mount-namespaces",
            "isolated-artix",
            "warn" if historical_refs else "pass",
            f"{len(historical_refs)} process mount tables reference canonical prepared upper/work" if historical_refs else "no cross-namespace references to canonical prepared upper/work",
            "historical Artix OverlayFS state remains inherited in other process mount namespaces; native builds remain isolated because they clone the prepared upper into a unique run-owned upper/work pair" if historical_refs else "canonical prepared upper/work is not referenced by another visible process mount table",
            "a normal VM reboot will retire historical inherited namespaces; do not kill unrelated system services or reuse the canonical upper directly" if historical_refs else None,
        )

        native_runs_root = repo / "portusos-build/work/native-runs"
        leaked_run_refs = scan_process_mountinfo([native_runs_root])
        add_check(
            checks,
            "artix.native-run-mount-leaks",
            "isolated-artix",
            "block" if leaked_run_refs else "pass",
            f"{len(leaked_run_refs)} process mount tables reference prior native-runs contexts" if leaked_run_refs else str(native_runs_root.relative_to(repo)),
            "a prior run-scoped Artix mount namespace is still alive; new builds must not proceed while a previous native run leaked mount state" if leaked_run_refs else "no prior run-scoped Artix mount namespace survives",
            "stop/reap the leaked prior native helper or reboot the build VM after preserving its run evidence; do not delete a path that is still mounted in another namespace" if leaked_run_refs else None,
        )

        current_native_context = repo / "portusos-build/work/native-runs" / run_id / "artix-context"
        current_refs = scan_process_mountinfo([current_native_context])
        add_check(
            checks,
            "artix.current-run-mount-cleanliness",
            "isolated-artix",
            "block" if current_refs else "pass",
            f"{len(current_refs)} process mount tables reference {current_native_context.relative_to(repo)}" if current_refs else str(current_native_context.relative_to(repo)),
            "the current run-scoped Artix context is already referenced before native construction" if current_refs else "the current run-scoped Artix context has no pre-existing mount-namespace references",
            "stop/reap the prior current-run native helper and preserve its evidence before retrying; never reuse a still-mounted run context" if current_refs else None,
        )

        package_evidence = state.get("bootstrap_packages", {}) if isinstance(state, dict) else {}
        package_manager_ok = bool(
            context_ok
            and state.get("repository_sync_verified") is True
            and state.get("pacman_version")
            and state.get("pacman_key_version")
            and state.get("keyring_initialized") is True
            and "artix" in state.get("keyrings_populated", [])
            and package_evidence.get("artix-keyring")
            and package_evidence.get("artix-mirrorlist")
            and package_evidence.get("openrc")
        )
        add_check(
            checks,
            "artix.package-manager-keyring",
            "isolated-artix",
            "pass" if package_manager_ok else "block",
            state.get("pacman_version") if isinstance(state, dict) else None,
            "Artix pacman repository sync/keyring/mirror baseline was verified inside the isolated context" if package_manager_ok else "Artix pacman/keyring/repository evidence is not ready",
            None if package_manager_ok else "prepare/refresh the private isolated context with sudo python -B scripts/artix/context.py prepare",
        )
        artools_ok = bool(
            context_ok
            and state.get("buildiso_version")
            and package_evidence.get("artools-base")
            and package_evidence.get("artools-iso")
            and package_evidence.get("artools-pkg")
        )
        add_check(
            checks,
            "artix.artools",
            "isolated-artix",
            "pass" if artools_ok else "block",
            state.get("buildiso_version") if isinstance(state, dict) else None,
            "artools/buildiso is installed and version-recorded inside the isolated Artix context" if artools_ok else "artools/buildiso evidence is not ready inside the isolated Artix context",
            None if artools_ok else "prepare/refresh the private isolated context with sudo python -B scripts/artix/context.py prepare",
        )
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        add_check(
            checks,
            "artix.seed",
            "isolated-artix",
            "block",
            str(bootstrap_path),
            f"Artix bootstrap contract could not be validated: {error}",
            "restore the tracked portusos-build/artix/bootstrap.json contract",
        )
        for check_id in ("artix.context", "artix.historical-mount-namespaces", "artix.native-run-mount-leaks", "artix.current-run-mount-cleanliness", "artix.package-manager-keyring", "artix.artools"):
            add_check(checks, check_id, "isolated-artix", "block", None, "Artix bootstrap contract is unavailable", "restore and validate the Artix bootstrap contract")

    adapter_file = repo / ADAPTER_PATH
    adapter_text = read_text(adapter_file) if adapter_file.is_file() else ""
    adapter_resolution = yaml_scalar(adapter_text, "mapping_resolution")
    add_check(
        checks,
        "artix.adapter-mapping",
        "isolated-artix",
        "pass" if adapter_resolution == "locked" else "block",
        adapter_resolution,
        "native artools adapter mapping is locked" if adapter_resolution == "locked" else "native artools profile/build command mapping remains unresolved",
        None if adapter_resolution == "locked" else "use the prepared isolated context and L2 facts to resolve exact artools profile/repository/build syntax, then lock adapter.yaml",
    )

    packages_text = read_text(repo / PACKAGES_PATH) if (repo / PACKAGES_PATH).is_file() else ""
    unresolved_artix_count = len(re.findall(r"(?m)^\s*resolution:\s*linux-verified\s*$", packages_text))
    add_check(
        checks,
        "input.artix-packages",
        "input-availability",
        "block" if unresolved_artix_count else "pass",
        f"{unresolved_artix_count} linux-verified package resolution entries" if unresolved_artix_count else "all selected package identities resolved",
        "required first-ISO Artix package identities are not fully resolved" if unresolved_artix_count else "selected Artix package identities are resolved",
        "resolve exact first-ISO package identities from the isolated Artix repositories and generate/verify the package lock" if unresolved_artix_count else None,
    )

    codex_text = read_text(repo / CODEX_PATH) if (repo / CODEX_PATH).is_file() else ""
    codex_pin = nested_resolution(codex_text, "pin")
    add_check(
        checks,
        "input.codex-pin",
        "input-availability",
        "pass" if codex_pin == "locked" else "block",
        codex_pin,
        "Codex artifact identity is locked" if codex_pin == "locked" else "Codex first-ISO artifact/version is not yet locked",
        None if codex_pin == "locked" else "complete the L3 Artix compatibility/pin/digest freeze before native first-live ISO construction",
    )

    browser_text = read_text(repo / PORTUS_BROWSER_PATH) if (repo / PORTUS_BROWSER_PATH).is_file() else ""
    browser_revision = nested_resolution(browser_text, "revision")
    add_check(
        checks,
        "input.portus-browser-pin",
        "input-availability",
        "pass" if browser_revision == "locked" else "block",
        browser_revision,
        "PortusBrowser source revision is locked" if browser_revision == "locked" else "PortusBrowser source revision is not yet frozen",
        None if browser_revision == "locked" else "freeze a clean immutable PortusBrowser source revision before native first-live ISO construction",
    )

    mcp_text = read_text(repo / PORTUS_MCP_PATH) if (repo / PORTUS_MCP_PATH).is_file() else ""
    mcp_revision = nested_resolution(mcp_text, "revision")
    tunnel_text = read_text(repo / TUNNEL_CLIENT_PATH) if (repo / TUNNEL_CLIENT_PATH).is_file() else ""
    tunnel_release = nested_resolution(tunnel_text, "release")
    locked_external = mcp_revision == "locked" and tunnel_release == "locked"
    add_check(
        checks,
        "input.portus-mcp-tunnel-identities",
        "input-availability",
        "pass" if locked_external else "block",
        f"portus-mcp={mcp_revision}; tunnel-client={tunnel_release}",
        "Portus MCP and tunnel-client source/release identities are locked" if locked_external else "Portus MCP and/or tunnel-client identity is not locked",
        None if locked_external else "lock both canonical first-ISO connection-stack identities before native construction",
    )
    context_manager = repo / "scripts/artix/context.py"
    context_text = read_text(context_manager) if context_manager.is_file() else ""
    closure_markers = (
        "prepare_repository_closure(",
        "repository-closure.json",
        '"-Syy"',
        '"-Sw"',
        "PACKAGE_PREFETCH_BATCH_LIMIT_BYTES",
        "PACKAGE_PREFETCH_MAX_MIRROR_ATTEMPTS",
        "prefetch_pacman_command(",
        "acquire_batch_with_mirror_failover(",
        "audit_persistent_package_cache(",
        "summarize_package_progress(",
        '"failure_class": failure_class',
        'local_server = f"file://{closure_inside}/repo"',
    )
    closure_gate = all(marker in context_text for marker in closure_markers)
    add_check(
        checks,
        "input.network-cache-closure",
        "input-availability",
        "pass" if closure_gate else "block",
        "native-run repository/package closure gate implemented" if closure_gate else "native-run repository/package closure gate missing",
        "the native handoff anchors repository metadata, resolves the exact package closure, audits resumable cache state, acquires pending identities in bounded byte batches with bounded mirror failover, records per-package/byte progress and attempt failure classes, freezes buildiso onto a local-only run snapshot, and emits repository-closure.json"
        if closure_gate
        else "native construction could pair stale pacman metadata with newer rolling Artix mirrors",
        None
        if closure_gate
        else "restore the native-run repository/package closure gate before starting buildiso",
    )

    return checks


def summarize(checks: list[dict[str, Any]]) -> tuple[str, dict[str, int]]:
    counts = {"pass": 0, "warn": 0, "block": 0}
    for check in checks:
        counts[check["result"]] += 1
    overall = "block" if counts["block"] else "warn" if counts["warn"] else "pass"
    return overall, counts


def write_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def self_test() -> int:
    assert parse_version("rustc 1.85.0") == (1, 85, 0)
    assert version_at_least((1, 97, 1), (1, 85, 0))
    assert not version_at_least((1, 84, 9), (1, 85, 0))
    checks: list[dict[str, Any]] = []
    add_check(checks, "a", "outer-host", "pass", "ok", "ok")
    add_check(checks, "b", "resource", "warn", "low", "warning")
    overall, counts = summarize(checks)
    assert overall == "warn" and counts == {"pass": 1, "warn": 1, "block": 0}
    add_check(checks, "c", "isolated-artix", "block", None, "blocked")
    overall, counts = summarize(checks)
    assert overall == "block" and counts["block"] == 1
    assert scan_process_mountinfo([Path("/definitely/not/a/real/portus/path")]) == []
    print("build_environment_preflight self-test: PASS")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Check mandatory PortusOS native-build environment requirements")
    parser.add_argument("--repo", default=".", help="repository root (default: current directory)")
    parser.add_argument("--config", help="build config or run-owned build-config.json snapshot")
    parser.add_argument("--expected-config-sha256", help="optional expected SHA-256 for the supplied config bytes")
    parser.add_argument("--config-source", help="original repository config path for evidence display")
    parser.add_argument("--run-id", help="build run ID; standalone checks use a generated ID")
    parser.add_argument("--output", help="write machine-readable preflight JSON here")
    parser.add_argument("--self-test", action="store_true", help=argparse.SUPPRESS)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    if not args.config or not args.output:
        print("--config and --output are required", file=sys.stderr)
        return 64

    repo = Path(args.repo).resolve()
    config_path = Path(args.config)
    if not config_path.is_absolute():
        config_path = (repo / config_path).resolve()
    output_path = Path(args.output)
    if not output_path.is_absolute():
        output_path = (repo / output_path).resolve()

    try:
        raw = config_path.read_bytes()
        config_sha256 = sha256_bytes(raw)
        if args.expected_config_sha256 and config_sha256 != args.expected_config_sha256:
            raise ValueError("build config SHA-256 differs from harness-supplied expected identity")
        value = json.loads(raw.decode("utf-8-sig"))
        config = validate_build_config_value(value)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
        print(f"environment preflight configuration error: {error}", file=sys.stderr)
        return 64

    run_id = args.run_id or f"standalone-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}"
    checks = build_checks(repo, config, config_path, config_sha256, run_id)
    overall, counts = summarize(checks)
    report = {
        "schema_version": SCHEMA_VERSION,
        "run_id": run_id,
        "checked_at": utc_now(),
        "overall": overall,
        "native_build_allowed": overall != "block",
        "build_config": {
            "name": config["name"],
            "source_path": args.config_source or str(config_path),
            "sha256": config_sha256,
        },
        "summary": counts,
        "checks": checks,
    }
    write_report(output_path, report)

    print(f"mandatory environment preflight: {overall.upper()} (pass={counts['pass']} warn={counts['warn']} block={counts['block']})")
    for check in checks:
        observed = f" :: {check['observed']}" if check["observed"] is not None else ""
        print(f"{check['result'].upper():5} {check['id']}{observed}")
        if check["result"] != "pass":
            print(f"      {check['reason']}")
            if check["remediation"]:
                print(f"      remediation: {check['remediation']}")
    print(f"preflight record: {output_path}")
    if overall != "block":
        return 0
    if any(check["id"] == "source.cleanliness" and check["result"] == "block" for check in checks):
        return EXIT_DIRTY
    return EXIT_BLOCKED


if __name__ == "__main__":
    raise SystemExit(main())
