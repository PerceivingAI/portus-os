#!/usr/bin/env python3
"""Manage the verified isolated Artix build context for PortusOS L2 work."""

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
import tarfile
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

EX_USAGE = 64
EX_UNAVAILABLE = 78
CONFIG_PATH = Path("portusos-build/artix/bootstrap.json")
PACKAGE_CACHE_PATH = Path("portusos-build/cache/artix-packages")
REPOSITORY_CLOSURE_FILE = "repository-closure.json"
PACMAN_PRINT_FORMAT = "%r|%n|%v|%f|%h|%s|%a"
PACKAGE_TARGET_RE = re.compile(r"^[A-Za-z0-9@._+:-]+$")
FROZEN_REPOSITORIES = ("system", "world", "galaxy")
PACKAGE_PREFETCH_BATCH_LIMIT_BYTES = 192 * 1024 * 1024
PACKAGE_PREFETCH_MAX_MIRROR_ATTEMPTS = 4
PACKAGE_PROGRESS_STATES = (
    "pending",
    "reused_verified",
    "downloaded_verified",
    "corrupt_removed",
    "failed",
)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def load_config(repo: Path) -> dict[str, Any]:
    value = json.loads((repo / CONFIG_PATH).read_text(encoding="utf-8"))
    expected_top = {"schema_version", "method", "seed", "context", "bootstrap_packages"}
    if set(value) != expected_top or value["schema_version"] != 1 or value["method"] != "verified-live-squashfs-overlay":
        raise ValueError("unsupported Artix bootstrap config shape")
    seed = value["seed"]
    if set(seed) != {"url", "cache_path", "sha256", "size_bytes", "volume_id", "rootfs"}:
        raise ValueError("unsupported Artix seed config shape")
    rootfs = seed["rootfs"]
    if set(rootfs) != {"iso_path", "offset_bytes", "size_bytes", "filesystem", "compression"}:
        raise ValueError("unsupported Artix rootfs config shape")
    context = value["context"]
    if set(context) != {"work_root", "lower_mount", "upper_dir", "overlay_work_dir", "root_mount", "repo_mount", "state_path", "facts_path"}:
        raise ValueError("unsupported Artix context config shape")
    if rootfs["iso_path"] != "/LIVEOS/ROOTFS.IMG" or rootfs["filesystem"] != "squashfs" or rootfs["compression"] != "zstd":
        raise ValueError("Artix rootfs identity differs from verified seed evidence")
    if not isinstance(value["bootstrap_packages"], list) or not value["bootstrap_packages"]:
        raise ValueError("bootstrap_packages must be a non-empty list")
    for field in ("cache_path",):
        candidate = Path(seed[field])
        if candidate.is_absolute() or ".." in candidate.parts:
            raise ValueError(f"{field} must be repository-relative")
    for field in ("work_root", "state_path", "facts_path"):
        candidate = Path(context[field])
        if candidate.is_absolute() or ".." in candidate.parts or candidate.parts[:2] != ("portusos-build", "work"):
            raise ValueError(f"{field} must remain below portusos-build/work")
    if context["repo_mount"] != "/workspace/portus-os":
        raise ValueError("repo_mount differs from the locked isolated-context path")
    return value


def paths(repo: Path, config: dict[str, Any]) -> dict[str, Path]:
    context = config["context"]
    work_root = repo / context["work_root"]
    return {
        "seed": repo / config["seed"]["cache_path"],
        "work_root": work_root,
        "lower": work_root / context["lower_mount"],
        "upper": work_root / context["upper_dir"],
        "overlay_work": work_root / context["overlay_work_dir"],
        "root": work_root / context["root_mount"],
        "state": repo / context["state_path"],
        "facts": repo / context["facts_path"],
    }


def native_context_config(repo: Path, config: dict[str, Any], native_root: Path) -> dict[str, Any]:
    """Derive a run-owned Artix mount context from the persistent prepared upper state."""
    resolved_repo = repo.resolve()
    resolved_native_root = native_root.resolve()
    work_root = (resolved_repo / "portusos-build/work").resolve()
    if not resolved_native_root.is_relative_to(work_root):
        raise ValueError("native Artix context root must remain below portusos-build/work")
    derived = copy.deepcopy(config)
    context_root = resolved_native_root / "artix-context"
    derived["context"]["work_root"] = context_root.relative_to(resolved_repo).as_posix()
    derived["context"]["state_path"] = (resolved_native_root / "artix-context-state.json").relative_to(resolved_repo).as_posix()
    derived["context"]["facts_path"] = (resolved_native_root / "artix-context-facts.tsv").relative_to(resolved_repo).as_posix()
    return derived


def snapshot_prepared_upper(repo: Path, prepared_config: dict[str, Any], native_config: dict[str, Any]) -> None:
    """Clone the prepared Artix upper into a fresh run-owned OverlayFS upper directory."""
    prepared = paths(repo, prepared_config)
    native = paths(repo, native_config)
    if not prepared["upper"].is_dir():
        raise RuntimeError("prepared Artix upper state is missing; run private prepare first")
    if native["work_root"].exists():
        raise RuntimeError(f"run-owned Artix context already exists: {native['work_root']}")
    native["upper"].mkdir(parents=True, exist_ok=False)
    # Preserve ownership, modes, timestamps, symlinks, xattrs and capabilities.  A
    # dedicated upper/work pair is required because OverlayFS rejects an upper
    # that remains mounted in any other mount namespace.
    run(["cp", "-a", f"{prepared['upper']}/.", str(native["upper"])])
    native["lower"].mkdir(parents=True, exist_ok=True)
    native["overlay_work"].mkdir(parents=True, exist_ok=True)
    native["root"].mkdir(parents=True, exist_ok=True)


def run(command: list[str], *, capture: bool = False, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, text=True, capture_output=capture, check=check)


def validate_artools_unattended_contract(buildiso_text: str, basestrap_text: str) -> None:
    """Fail closed unless the selected artools path is noninteractive by construction."""
    match = re.search(r"(?m)^basestrap_args=\(([^)]*)\)\s*$", buildiso_text)
    if match is None:
        raise RuntimeError("buildiso does not expose the expected basestrap_args contract")
    option_chars = "".join(token[1:] for token in match.group(1).split() if token.startswith("-"))
    if "i" in option_chars:
        raise RuntimeError("buildiso enables basestrap interactive mode")
    if "c" not in option_chars:
        raise RuntimeError("buildiso no longer shares the host package cache with basestrap")
    if 'basestrap_args+=(-C "${pacman_conf}")' not in buildiso_text:
        raise RuntimeError("buildiso no longer routes basestrap through the selected pacman configuration")
    if 'basestrap "${basestrap_args[@]}"' not in buildiso_text:
        raise RuntimeError("buildiso no longer invokes basestrap through the locked argument array")
    for marker in (
        "i) interactive=1 ;;",
        "if (( ! interactive )); then",
        "pacman_args+=(--noconfirm)",
    ):
        if marker not in basestrap_text:
            raise RuntimeError(f"basestrap unattended contract is missing: {marker}")


def verify_artools_unattended_contract(root: Path) -> None:
    buildiso = root / "usr/bin/buildiso"
    basestrap = next(
        (candidate for candidate in (root / "usr/sbin/basestrap", root / "usr/bin/basestrap") if candidate.is_file()),
        None,
    )
    if not buildiso.is_file() or basestrap is None:
        raise RuntimeError("Artix build root is missing buildiso/basestrap for unattended-contract verification")
    validate_artools_unattended_contract(
        buildiso.read_text(encoding="utf-8", errors="replace"),
        basestrap.read_text(encoding="utf-8", errors="replace"),
    )


ARTOOLS_SINGLE_KERNEL_BOOT_BLOCK = '''        cp "${work_dir}"/rootfs/boot/vmlinuz* "${iso_root}"/boot/vmlinuz-"${arch}"

        local bootfs="${work_dir}/bootfs"

        mount_overlayfs "${bootfs}" "${work_dir}"

        if "${use_dracut}"; then
'''

PORTUS_LIVE_KERNEL_BOOT_BLOCK = '''        # PortusOS installs two kernels, while artools 0.39.1 assumes exactly one
        # rootfs vmlinuz.  The live medium boots the normal/default kernel only;
        # both kernels remain untouched in rootfs for the installed system.
        local live_kernel_pkg='@LIVE_KERNEL_PACKAGE@'
        local live_kernel_image="${work_dir}/rootfs/boot/vmlinuz-${live_kernel_pkg}"
        [[ -f "${live_kernel_image}" ]] || die "Portus live kernel missing: %s" "${live_kernel_image}"
        cp "${live_kernel_image}" "${iso_root}/boot/vmlinuz-${arch}"

        local bootfs="${work_dir}/bootfs"

        mount_overlayfs "${bootfs}" "${work_dir}"

        # artools' mkinitcpio helper reads /usr/src/linux/version. Resolve that
        # value from the selected kernel package inside the temporary bootfs so
        # the live initramfs matches vmlinuz-linux-lts without changing rootfs.
        local live_kernel_version='' pkgbase live_kernel_matches=0
        for pkgbase in "${bootfs}"/usr/lib/modules/*/pkgbase; do
            [[ -f "${pkgbase}" ]] || continue
            if [[ "$(<"${pkgbase}")" == "${live_kernel_pkg}" ]]; then
                live_kernel_version=${pkgbase%/pkgbase}
                live_kernel_version=${live_kernel_version##*/}
                live_kernel_matches=$((live_kernel_matches + 1))
            fi
        done
        [[ "${live_kernel_matches}" -eq 1 ]] || die "Portus live kernel module selection for %s is ambiguous: %s matches" "${live_kernel_pkg}" "${live_kernel_matches}"
        mkdir -p "${bootfs}/usr/src/linux"
        printf '%s\n' "${live_kernel_version}" > "${bootfs}/usr/src/linux/version"

        if "${use_dracut}"; then
'''


def patch_artools_buildiso_text(buildiso_text: str, live_kernel_package: str) -> str:
    """Adapt the verified artools 0.39.1 single-kernel boot path for PortusOS."""
    if live_kernel_package != "linux-lts":
        raise RuntimeError(f"unsupported Portus live boot kernel package: {live_kernel_package}")
    occurrences = buildiso_text.count(ARTOOLS_SINGLE_KERNEL_BOOT_BLOCK)
    if occurrences != 1:
        raise RuntimeError(
            "artools make_bootfs single-kernel contract changed; "
            f"expected one verified compatibility seam, found {occurrences}"
        )
    replacement = PORTUS_LIVE_KERNEL_BOOT_BLOCK.replace("@LIVE_KERNEL_PACKAGE@", live_kernel_package)
    patched = buildiso_text.replace(ARTOOLS_SINGLE_KERNEL_BOOT_BLOCK, replacement, 1)
    if 'rootfs/boot/vmlinuz* "${iso_root}"/boot/vmlinuz-' in patched:
        raise RuntimeError("artools compatibility patch left the ambiguous kernel wildcard in make_bootfs")
    return patched


def apply_artools_live_kernel_compatibility(root: Path, live_kernel_package: str) -> dict[str, str]:
    """Patch only the run-owned Artix build root; never mutate the prepared canonical upper."""
    buildiso = root / "usr/bin/buildiso"
    if not buildiso.is_file():
        raise RuntimeError("Artix build root is missing buildiso for live-kernel compatibility")
    original = buildiso.read_text(encoding="utf-8", errors="strict")
    patched = patch_artools_buildiso_text(original, live_kernel_package)
    buildiso.write_text(patched, encoding="utf-8")
    return {
        "kind": "artools-0.39.1-dual-kernel-live-boot",
        "live_boot_kernel_package": live_kernel_package,
        "source_sha256": hashlib.sha256(original.encode("utf-8")).hexdigest(),
        "patched_sha256": hashlib.sha256(patched.encode("utf-8")).hexdigest(),
    }


def run_unattended(command: list[str]) -> subprocess.CompletedProcess[str]:
    """Run a native builder with no readable stdin; unexpected prompts cannot depend on an operator."""
    return subprocess.run(command, text=True, check=True, stdin=subprocess.DEVNULL)


def verify_seed(repo: Path, config: dict[str, Any], *, quiet: bool = False) -> dict[str, Any]:
    p = paths(repo, config)["seed"]
    seed = config["seed"]
    if not p.is_file():
        raise RuntimeError(f"verified Artix seed is missing: {p}")
    size = p.stat().st_size
    if size != seed["size_bytes"]:
        raise RuntimeError(f"Artix seed size mismatch: expected {seed['size_bytes']}, found {size}")
    digest = sha256_file(p)
    if digest != seed["sha256"]:
        raise RuntimeError(f"Artix seed SHA-256 mismatch: expected {seed['sha256']}, found {digest}")
    evidence = {"path": str(p), "size_bytes": size, "sha256": digest, "verified": True}
    if not quiet:
        print(json.dumps(evidence, indent=2, sort_keys=True))
    return evidence


def require_linux_x86_64() -> None:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("Artix context requires an x86_64 Linux outer host")


def require_root(action: str) -> None:
    if os.geteuid() != 0:
        raise PermissionError(f"{action} requires root for bounded mount/chroot operations")


def is_mountpoint(path: Path) -> bool:
    result = run(["findmnt", "-n", "-T", str(path)], capture=True, check=False)
    if result.returncode != 0:
        return False
    target = result.stdout.strip().split()[0] if result.stdout.strip() else ""
    try:
        return Path(target).resolve() == path.resolve()
    except OSError:
        return target == str(path)


def ensure_dirs(p: dict[str, Path]) -> None:
    for key in ("work_root", "lower", "upper", "overlay_work", "root"):
        p[key].mkdir(parents=True, exist_ok=True)
    p["state"].parent.mkdir(parents=True, exist_ok=True)
    p["facts"].parent.mkdir(parents=True, exist_ok=True)


def mount_context(repo: Path, config: dict[str, Any]) -> None:
    require_root("Artix context mount")
    verify_seed(repo, config, quiet=True)
    p = paths(repo, config)
    ensure_dirs(p)
    rootfs = config["seed"]["rootfs"]
    if not is_mountpoint(p["lower"]):
        run([
            "mount", "-t", "squashfs",
            "-o", f"loop,ro,offset={rootfs['offset_bytes']},sizelimit={rootfs['size_bytes']}",
            str(p["seed"]), str(p["lower"]),
        ])
    if not is_mountpoint(p["root"]):
        run([
            "mount", "-t", "overlay", "overlay",
            "-o", f"lowerdir={p['lower']},upperdir={p['upper']},workdir={p['overlay_work']}",
            str(p["root"]),
        ])
    for directory in ("proc", "sys", "dev", "workspace"):
        (p["root"] / directory).mkdir(parents=True, exist_ok=True)
    repo_target = p["root"] / config["context"]["repo_mount"].lstrip("/")
    repo_target.mkdir(parents=True, exist_ok=True)
    if not is_mountpoint(repo_target):
        run(["mount", "--bind", str(repo), str(repo_target)])
        run(["mount", "-o", "remount,bind,ro", str(repo_target)])
    if not is_mountpoint(p["root"] / "proc"):
        run(["mount", "-t", "proc", "proc", str(p["root"] / "proc")])
    for source, target_name in (("/sys", "sys"), ("/dev", "dev")):
        target = p["root"] / target_name
        if not is_mountpoint(target):
            run(["mount", "--rbind", source, str(target)])
            run(["mount", "--make-rslave", str(target)])
    resolv = p["root"] / "etc/resolv.conf"
    try:
        if resolv.is_symlink() or resolv.exists():
            resolv.unlink()
        shutil.copyfile("/etc/resolv.conf", resolv)
    except OSError as error:
        raise RuntimeError(f"failed to prepare isolated resolv.conf: {error}") from error


def chroot(repo: Path, config: dict[str, Any], command: list[str], *, capture: bool = False) -> subprocess.CompletedProcess[str]:
    require_root("Artix chroot execution")
    p = paths(repo, config)
    if not is_mountpoint(p["root"]):
        raise RuntimeError("Artix context is not mounted")
    return run(["chroot", str(p["root"]), *command], capture=capture)


def parse_package_candidates(facts: str) -> dict[str, dict[str, str]]:
    candidates: dict[str, dict[str, str]] = {}
    in_section = False
    for line in facts.splitlines():
        if line == "[package-candidates]":
            in_section = True
            continue
        if in_section and line.startswith("["):
            break
        if not in_section or not line or line.startswith("package\t"):
            continue
        fields = line.split("\t")
        if len(fields) != 7:
            continue
        package, availability, repository, version, architecture, licenses, installed = fields
        candidates[package] = {
            "availability": availability,
            "repository": repository,
            "version": version,
            "architecture": architecture,
            "licenses": licenses,
            "installed": installed,
        }
    return candidates


def capture_artools_evidence(repo: Path, config: dict[str, Any]) -> dict[str, Any]:
    help_result = chroot(repo, config, ["/usr/bin/buildiso", "--help"], capture=True)
    help_text = (help_result.stdout + help_result.stderr).strip()
    package_files: dict[str, list[str]] = {}
    for package in ("artools-base", "artools-iso", "artools-pkg"):
        result = chroot(repo, config, ["/usr/bin/pacman", "-Ql", package], capture=True)
        files: list[str] = []
        for line in result.stdout.splitlines():
            prefix = package + " "
            if line.startswith(prefix):
                files.append(line[len(prefix):])
        package_files[package] = files
    command_paths: dict[str, str | None] = {}
    for command in ("buildiso", "buildpkg", "basestrap", "artix-chroot"):
        result = chroot(repo, config, ["/bin/sh", "-c", f"command -v {command} || true"], capture=True)
        command_paths[command] = result.stdout.strip() or None
    return {
        "buildiso_help": help_text,
        "command_paths": command_paths,
        "package_files": package_files,
    }


def read_artix_os_release(repo: Path, config: dict[str, Any]) -> dict[str, str]:
    result = chroot(repo, config, ["/bin/sh", "-c", "cat /etc/os-release"], capture=True)
    values: dict[str, str] = {}
    for line in result.stdout.splitlines():
        if "=" not in line or line.startswith("#"):
            continue
        key, raw = line.split("=", 1)
        values[key] = raw.strip().strip('"')
    return values


def initialize_pacman_keyring(repo: Path, config: dict[str, Any]) -> list[str]:
    """Initialize the isolated pacman keyring before signed package installation."""
    p = paths(repo, config)
    chroot(repo, config, ["/usr/bin/pacman-key", "--init"])
    populations = ["artix"]
    if (p["root"] / "usr/share/pacman/keyrings/archlinux.gpg").is_file():
        populations.append("archlinux")
    for population in populations:
        chroot(repo, config, ["/usr/bin/pacman-key", "--populate", population])
    chroot(repo, config, ["/usr/bin/pacman-key", "--updatedb"])
    return populations


def private_namespace_command(repo: Path, action: str, *, manifest: Path | None = None) -> list[str]:
    unshare = shutil.which("unshare")
    if not unshare:
        raise RuntimeError("private Artix mount namespaces require the util-linux unshare command")
    command = [
        unshare,
        "--mount",
        "--fork",
        sys.executable,
        "-B",
        str(repo / "scripts/artix/context.py"),
        action,
    ]
    if manifest is not None:
        command.extend(["--manifest", str(manifest)])
    return command


def run_private_namespace(repo: Path, config: dict[str, Any], action: str, *, manifest: Path | None = None) -> None:
    require_linux_x86_64()
    require_root("private Artix namespace execution")
    p = paths(repo, config)
    if is_mountpoint(p["root"]) or is_mountpoint(p["lower"]):
        raise RuntimeError(
            "legacy global Artix mounts are still active; clean them before starting a private namespace"
        )
    result = run(private_namespace_command(repo, action, manifest=manifest), check=False)
    if result.returncode != 0:
        raise RuntimeError(f"private Artix namespace action {action} failed with exit {result.returncode}")
    if is_mountpoint(p["root"]) or is_mountpoint(p["lower"]):
        raise RuntimeError("private Artix namespace leaked a mount into the outer VM")


def make_namespace_private() -> None:
    require_root("private mount propagation setup")
    run(["mount", "--make-rprivate", "/"])


def prepare(repo: Path, config: dict[str, Any]) -> None:
    run_private_namespace(repo, config, "_prepare-inner")


def prepare_inner(repo: Path, config: dict[str, Any]) -> None:
    make_namespace_private()
    prepare_mounted(repo, config)


def prepare_mounted(repo: Path, config: dict[str, Any]) -> None:
    require_linux_x86_64()
    require_root("Artix context prepare")
    seed_evidence = verify_seed(repo, config, quiet=True)
    p = paths(repo, config)
    if p["state"].exists():
        p["state"].unlink()
    mounted = False
    try:
        mount_context(repo, config)
        mounted = True
        release = read_artix_os_release(repo, config)
        arch = chroot(repo, config, ["/usr/bin/uname", "-m"], capture=True).stdout.strip()
        if release.get("ID") != "artix" or arch != "x86_64":
            raise RuntimeError(f"isolated context identity mismatch: ID={release.get('ID')} arch={arch}")

        keyring_populations = initialize_pacman_keyring(repo, config)
        chroot(repo, config, ["/usr/bin/pacman", "-Sy", "--noconfirm"])
        chroot(repo, config, ["/usr/bin/pacman", "-S", "--needed", "--noconfirm", *config["bootstrap_packages"]])
        repo_mount = config["context"]["repo_mount"]
        chroot(repo, config, ["/usr/bin/git", "config", "--global", "--add", "safe.directory", repo_mount])
        pacman_result = chroot(repo, config, ["/usr/bin/pacman", "--version"], capture=True)
        pacman_version = (pacman_result.stdout + pacman_result.stderr).strip()
        pacman_key_result = chroot(repo, config, ["/usr/bin/pacman-key", "--version"], capture=True)
        pacman_key_version = (pacman_key_result.stdout + pacman_key_result.stderr).strip()
        buildiso_result = chroot(repo, config, ["/usr/bin/buildiso", "--version"], capture=True)
        buildiso_version = (buildiso_result.stdout + buildiso_result.stderr).strip()
        package_versions: dict[str, str] = {}
        evidence_packages = [*config["bootstrap_packages"], "artix-keyring", "artix-mirrorlist", "openrc"]
        for package in evidence_packages:
            package_versions[package] = chroot(repo, config, ["/usr/bin/pacman", "-Q", package], capture=True).stdout.strip()
        collector = repo_mount + "/scripts/artix/collect-l0-l2-facts.sh"
        facts = chroot(repo, config, ["/bin/sh", collector], capture=True).stdout
        package_candidates = parse_package_candidates(facts)
        artools_evidence = capture_artools_evidence(repo, config)
        p = paths(repo, config)
        p["facts"].write_text(facts, encoding="utf-8")
        state = {
            "schema_version": 1,
            "captured_at": utc_now(),
            "ready": True,
            "method": config["method"],
            "seed": seed_evidence,
            "rootfs": config["seed"]["rootfs"],
            "context_root": str(p["root"]),
            "os_id": release.get("ID"),
            "os_pretty_name": release.get("PRETTY_NAME"),
            "architecture": arch,
            "pacman_version": pacman_version,
            "pacman_key_version": pacman_key_version,
            "keyring_initialized": True,
            "keyrings_populated": keyring_populations,
            "buildiso_version": buildiso_version,
            "repository_sync_verified": True,
            "bootstrap_packages": package_versions,
            "facts_path": str(p["facts"].relative_to(repo)),
            "package_candidates": package_candidates,
            "artools_evidence": artools_evidence,
        }
        p["state"].write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        uid = os.environ.get("SUDO_UID")
        gid = os.environ.get("SUDO_GID")
        if uid and gid:
            for item in (p["state"], p["facts"]):
                os.chown(item, int(uid), int(gid))
        print(json.dumps(state, indent=2, sort_keys=True))
    except Exception:
        if mounted:
            try:
                teardown(repo, config)
            except Exception as cleanup_error:
                print(f"warning: failed to clean up Artix mounts after prepare failure: {cleanup_error}", file=sys.stderr)
        raise

def parse_artools_package_targets(common_text: str, profile_text: str) -> list[str]:
    """Extract package targets only from the locked artools package sections."""
    targets: set[str] = set()

    active_common = False
    for raw in common_text.splitlines():
        if raw and not raw[0].isspace() and raw.rstrip().endswith(":"):
            active_common = raw.split(":", 1)[0].startswith("packages-")
            continue
        if active_common:
            match = re.match(r"^\s*-\s+([^\s#]+)\s*(?:#.*)?$", raw)
            if match:
                targets.add(match.group(1))

    active_profile = False
    for raw in profile_text.splitlines():
        if raw and not raw[0].isspace() and raw.rstrip().endswith(":"):
            active_profile = raw.split(":", 1)[0] in {"rootfs", "livefs"}
            continue
        if active_profile:
            match = re.match(r"^\s*-\s+([^\s#]+)\s*(?:#.*)?$", raw)
            if match:
                targets.add(match.group(1))

    if not targets:
        raise RuntimeError("artools package profile produced an empty package target set")
    invalid = sorted(target for target in targets if not PACKAGE_TARGET_RE.fullmatch(target))
    if invalid:
        raise RuntimeError(f"artools package profile contains invalid package targets: {', '.join(invalid)}")
    return sorted(targets, key=lambda value: value.encode("utf-8"))


def parse_pacman_print_rows(text: str) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str]] = set()
    for raw in text.splitlines():
        if not raw.strip():
            continue
        fields = raw.split("|")
        if len(fields) != 7:
            raise RuntimeError(f"unexpected pacman closure row: {raw!r}")
        repository, name, version, filename, sha256, size_text, architecture = fields
        if repository not in FROZEN_REPOSITORIES:
            raise RuntimeError(f"package closure escaped locked repositories: {repository}/{name}")
        if not PACKAGE_TARGET_RE.fullmatch(name) or not version or not filename:
            raise RuntimeError(f"invalid package closure identity: {raw!r}")
        if not re.fullmatch(r"[0-9a-f]{64}", sha256):
            raise RuntimeError(f"package closure lacks a valid SHA-256: {repository}/{name}")
        try:
            size_bytes = int(size_text)
        except ValueError as error:
            raise RuntimeError(f"package closure size is invalid: {repository}/{name}") from error
        if size_bytes < 0:
            raise RuntimeError(f"package closure size is negative: {repository}/{name}")
        identity = (repository, name, version)
        if identity in seen:
            continue
        seen.add(identity)
        rows.append(
            {
                "repository": repository,
                "name": name,
                "version": version,
                "filename": filename,
                "sha256": sha256,
                "size_bytes": size_bytes,
                "architecture": architecture,
            }
        )
    if not rows:
        raise RuntimeError("pacman resolved an empty repository/package closure")
    rows.sort(key=lambda row: (row["repository"], row["name"], row["version"]))
    return rows


def closure_identity(packages: list[dict[str, Any]]) -> list[tuple[str, str, str, str, str]]:
    return [
        (entry["repository"], entry["name"], entry["version"], entry["filename"], entry["sha256"])
        for entry in packages
    ]


def require_same_package_closure(expected: list[dict[str, Any]], actual: list[dict[str, Any]]) -> None:
    if closure_identity(expected) != closure_identity(actual):
        raise RuntimeError("frozen local repository resolves a different package closure than the fresh network database")


def verify_cached_package_files(cache: Path, packages: list[dict[str, Any]]) -> None:
    for package in packages:
        cached = cache / package["filename"]
        if not cached.is_file():
            raise RuntimeError(f"prefetched package file is missing: {package['filename']}")
        actual_sha256 = sha256_file(cached)
        if actual_sha256 != package["sha256"]:
            raise RuntimeError(
                f"prefetched package SHA-256 mismatch for {package['filename']}: {actual_sha256}"
            )


def plan_package_prefetch_batches(
    packages: list[dict[str, Any]],
    max_bytes: int = PACKAGE_PREFETCH_BATCH_LIMIT_BYTES,
) -> list[list[dict[str, Any]]]:
    """Partition exact package identities into deterministic byte-bounded batches."""
    if max_bytes <= 0:
        raise ValueError("package prefetch batch limit must be positive")
    ordered = sorted(
        packages,
        key=lambda entry: (
            entry["repository"],
            entry["name"],
            entry["version"],
            entry["filename"],
        ),
    )
    batches: list[list[dict[str, Any]]] = []
    current: list[dict[str, Any]] = []
    current_bytes = 0
    for package in ordered:
        size_bytes = package["size_bytes"]
        if not isinstance(size_bytes, int) or size_bytes < 0:
            raise RuntimeError(f"package has invalid download size for batching: {package.get('name', '<unknown>')}")
        if current and current_bytes + size_bytes > max_bytes:
            batches.append(current)
            current = []
            current_bytes = 0
        current.append(package)
        current_bytes += size_bytes
        if size_bytes > max_bytes:
            batches.append(current)
            current = []
            current_bytes = 0
    if current:
        batches.append(current)
    return batches


def exact_package_sync_targets(packages: list[dict[str, Any]]) -> list[str]:
    return [f"{package['repository']}/{package['name']}" for package in packages]


def prefetch_pacman_command(
    config_path: str,
    db_path: str,
    cache_path: str,
    packages: list[dict[str, Any]],
) -> list[str]:
    """Build the pacman 7.1 download-only command for already-resolved identities."""
    if not packages:
        raise ValueError("package prefetch command requires at least one exact package identity")
    return [
        "/usr/bin/pacman",
        "-Sw",
        "--noconfirm",
        "--config",
        config_path,
        "--dbpath",
        db_path,
        "--cachedir",
        cache_path,
        "--nodeps",
        "--nodeps",
        *exact_package_sync_targets(packages),
    ]


def ordered_prefetch_mirrors(
    candidates: list[dict[str, Any]],
    selected_anchor: dict[str, Any],
    max_attempts: int = PACKAGE_PREFETCH_MAX_MIRROR_ATTEMPTS,
) -> list[dict[str, Any]]:
    """Return the anchor first, followed by bounded distinct mirrorlist fallbacks."""
    if max_attempts <= 0:
        raise ValueError("package prefetch mirror-attempt limit must be positive")
    anchor_server = selected_anchor.get("server") if isinstance(selected_anchor, dict) else None
    if not isinstance(anchor_server, str):
        raise RuntimeError("selected Artix repository anchor lacks a server template")
    matching_anchor = next((candidate for candidate in candidates if candidate.get("server") == anchor_server), None)
    if matching_anchor is None:
        raise RuntimeError("selected Artix repository anchor is absent from parsed mirror candidates")
    ordered = [copy.deepcopy(matching_anchor)]
    ordered.extend(copy.deepcopy(candidate) for candidate in candidates if candidate.get("server") != anchor_server)
    return ordered[:max_attempts]


def remove_unverified_attempt_files(cache: Path, packages: list[dict[str, Any]]) -> list[str]:
    """Remove corrupt complete archives and partial files before mirror failover."""
    removed: list[str] = []
    for package in packages:
        cached = cache / package["filename"]
        if cached.is_file() and sha256_file(cached) != package["sha256"]:
            cached.unlink()
            removed.append(cached.name)
            signature = cache / f"{package['filename']}.sig"
            if signature.exists():
                signature.unlink()
                removed.append(signature.name)
        partial = cache / f"{package['filename']}.part"
        if partial.exists():
            partial.unlink()
            removed.append(partial.name)
        signature_partial = cache / f"{package['filename']}.sig.part"
        if signature_partial.exists():
            signature_partial.unlink()
            removed.append(signature_partial.name)
    return sorted(removed)


def acquire_batch_with_mirror_failover(
    cache: Path,
    batch: list[dict[str, Any]],
    mirrors: list[dict[str, Any]],
    fetch_attempt: Any,
    progress: Any | None = None,
) -> tuple[set[str], list[dict[str, Any]]]:
    """Acquire one frozen batch across bounded mirrors without changing repository DB state."""
    if not mirrors:
        raise RuntimeError("package prefetch has no eligible mirrors")
    pending_by_filename = {package["filename"]: package for package in batch}
    verified: set[str] = set()
    attempts: list[dict[str, Any]] = []
    last_error: BaseException | None = None
    clean_retry_packages: list[dict[str, Any]] | None = None

    for attempt_index, mirror in enumerate(mirrors, start=1):
        if clean_retry_packages is not None:
            attempt_packages = clean_retry_packages
            clean_retry_packages = None
        elif pending_by_filename:
            attempt_packages = [
                package for package in batch if package["filename"] in pending_by_filename
            ]
        else:
            break
        attempt_error: BaseException | None = None
        try:
            fetch_attempt(attempt_index, mirror, attempt_packages)
        except BaseException as error:
            attempt_error = error
            last_error = error

        verified_attempt = verified_cached_filenames(cache, attempt_packages)
        verified.update(verified_attempt)
        for filename in verified_attempt:
            pending_by_filename.pop(filename, None)
        pending_packages = [
            package for package in batch if package["filename"] in pending_by_filename
        ]
        removed_unverified = remove_unverified_attempt_files(cache, pending_packages)
        detail = None
        if attempt_error is not None:
            detail = concise_process_failure(attempt_error) if isinstance(attempt_error, subprocess.CalledProcessError) else str(attempt_error)[:500]
        elif pending_by_filename:
            detail = "attempt completed without producing SHA-256-valid files for every requested identity"
        failure_class = classify_package_acquisition_failure(
            attempt_error,
            set(pending_by_filename),
            detail,
        )
        verified_filenames = sorted(verified_attempt)
        pending_filenames = sorted(pending_by_filename)
        requested_filenames = [package["filename"] for package in attempt_packages]
        requested_bytes = sum(package["size_bytes"] for package in attempt_packages)
        verified_bytes = sum(
            package["size_bytes"]
            for package in attempt_packages
            if package["filename"] in verified_attempt
        )
        pending_bytes = sum(package["size_bytes"] for package in pending_by_filename.values())
        attempt_record = {
            "attempt": attempt_index,
            "mirror": copy.deepcopy(mirror),
            "requested_count": len(attempt_packages),
            "requested_bytes": requested_bytes,
            "requested_filenames": requested_filenames,
            "verified_count": len(verified_attempt),
            "verified_bytes": verified_bytes,
            "verified_filenames": verified_filenames,
            "pending_count": len(pending_by_filename),
            "pending_bytes": pending_bytes,
            "pending_filenames": pending_filenames,
            "removed_unverified": removed_unverified,
            "result": "pass" if attempt_error is None and not pending_by_filename else "fail",
            "failure_class": failure_class,
            "detail": detail,
        }
        attempts.append(attempt_record)
        if progress is not None:
            progress(copy.deepcopy(attempt_record), copy.deepcopy(verified), set(pending_by_filename))

        if isinstance(attempt_error, (KeyboardInterrupt, SystemExit)):
            raise attempt_error
        if attempt_error is None and not pending_by_filename:
            return verified, attempts
        if attempt_error is not None and not pending_by_filename:
            clean_retry_packages = list(attempt_packages)

    pending_names = [package["filename"] for package in batch if package["filename"] in pending_by_filename]
    if not pending_names and last_error is not None:
        message = (
            f"package prefetch exhausted {len(attempts)}/{len(mirrors)} mirror attempts; all requested archives "
            "matched the frozen SHA-256 identities but pacman never completed a clean transaction"
        )
    else:
        message = (
            f"package prefetch exhausted {len(attempts)}/{len(mirrors)} mirror attempts with "
            f"{len(pending_names)} unresolved package files: " + ", ".join(pending_names)
        )
    if last_error is not None:
        raise RuntimeError(message) from last_error
    raise RuntimeError(message)


def acquire_prefetch_batches(
    cache: Path,
    batches: list[list[dict[str, Any]]],
    fetch_batch: Any,
    progress: Any | None = None,
) -> set[str]:
    """Fetch deterministic batches and retain only hash-verified completed identities."""
    pending = {
        package["filename"]
        for batch in batches
        for package in batch
    }
    verified: set[str] = set()
    for batch_index, batch in enumerate(batches, start=1):
        batch_error: BaseException | None = None
        try:
            fetch_batch(batch_index, batch)
        except BaseException as error:
            batch_error = error

        verified_batch = verified_cached_filenames(cache, batch)
        verified.update(verified_batch)
        pending.difference_update(verified_batch)
        missing_batch = [
            package["filename"]
            for package in batch
            if package["filename"] not in verified_batch
        ]
        batch_complete = batch_error is None and not missing_batch
        if progress is not None:
            progress(
                batch_index,
                len(batches),
                batch,
                verified_batch,
                pending,
                batch_error,
                batch_complete,
            )

        if batch_error is not None:
            if isinstance(batch_error, (KeyboardInterrupt, SystemExit)):
                raise batch_error
            raise RuntimeError(
                f"package prefetch batch {batch_index}/{len(batches)} failed; "
                f"{len(verified_batch)}/{len(batch)} batch package files verified before failure"
            ) from batch_error

        if missing_batch:
            raise RuntimeError(
                f"package prefetch batch {batch_index}/{len(batches)} completed without verified files: "
                + ", ".join(missing_batch)
            )

    if pending:
        raise RuntimeError(f"package prefetch completed with {len(pending)} unresolved package files")
    return verified


def remove_cache_entry(path: Path, label: str) -> bool:
    """Remove one cache file/symlink, but never recursively remove an unexpected directory."""
    if path.is_symlink() or path.is_file():
        path.unlink()
        return True
    if path.exists():
        raise RuntimeError(f"unexpected directory or special entry at {label}: {path}")
    return False


def audit_persistent_package_cache(cache: Path, packages: list[dict[str, Any]]) -> dict[str, Any]:
    """Reconcile the persistent package cache against one frozen package closure."""
    reused: list[str] = []
    pending: list[str] = []
    corrupt_removed: list[str] = []
    stale_partial_removed: list[str] = []
    expected_filenames: set[str] = set()

    for package in sorted(packages, key=lambda entry: entry["filename"]):
        filename = package["filename"]
        if filename in expected_filenames:
            raise RuntimeError(f"duplicate package filename in frozen closure: {filename}")
        expected_filenames.add(filename)
        cached = cache / filename
        valid = False
        if cached.is_symlink() or cached.exists():
            if cached.is_symlink() or not cached.is_file():
                if remove_cache_entry(cached, "package cache archive"):
                    corrupt_removed.append(filename)
                signature = cache / f"{filename}.sig"
                if remove_cache_entry(signature, "package cache signature"):
                    corrupt_removed.append(signature.name)
            elif sha256_file(cached) == package["sha256"]:
                valid = True
                reused.append(filename)
            else:
                if remove_cache_entry(cached, "package cache archive"):
                    corrupt_removed.append(filename)
                signature = cache / f"{filename}.sig"
                if remove_cache_entry(signature, "package cache signature"):
                    corrupt_removed.append(signature.name)

        for partial_name in (f"{filename}.part", f"{filename}.sig.part"):
            partial = cache / partial_name
            if remove_cache_entry(partial, "stale package cache partial"):
                stale_partial_removed.append(partial_name)

        if not valid:
            pending.append(filename)

    reused_set = set(reused)
    pending_set = set(pending)
    if reused_set & pending_set or reused_set | pending_set != expected_filenames:
        raise RuntimeError("persistent package cache audit did not partition the frozen closure")

    return {
        "status": "pass",
        "resolved_count": len(packages),
        "reused_count": len(reused),
        "reused_filenames": reused,
        "pending_count": len(pending),
        "pending_filenames": pending,
        "corrupt_entries_removed": sorted(corrupt_removed),
        "stale_partial_entries_removed": sorted(stale_partial_removed),
    }


def initialize_package_progress(packages: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Create deterministic per-package progress records for one frozen closure."""
    progress: list[dict[str, Any]] = []
    seen: set[str] = set()
    for package in sorted(packages, key=lambda entry: entry["filename"]):
        filename = package["filename"]
        if filename in seen:
            raise RuntimeError(f"duplicate package filename in progress closure: {filename}")
        seen.add(filename)
        progress.append(
            {
                "repository": package["repository"],
                "name": package["name"],
                "version": package["version"],
                "filename": filename,
                "size_bytes": package["size_bytes"],
                "state": "pending",
                "verified": False,
                "needs_acquisition": True,
                "batch": None,
                "attempt": None,
                "mirror": None,
                "failure_class": None,
            }
        )
    return progress


def summarize_package_progress(progress: list[dict[str, Any]]) -> dict[str, Any]:
    """Summarize package state without losing unresolved-state distinctions."""
    states = {
        state: {"packages": 0, "bytes": 0}
        for state in PACKAGE_PROGRESS_STATES
    }
    resolved_packages = 0
    resolved_bytes = 0
    verified_packages = 0
    verified_bytes = 0
    pending_packages = 0
    pending_bytes = 0
    seen: set[str] = set()
    for record in progress:
        filename = record.get("filename")
        state = record.get("state")
        size_bytes = record.get("size_bytes")
        if not isinstance(filename, str) or not filename or filename in seen:
            raise RuntimeError("package progress contains an invalid or duplicate filename")
        if state not in PACKAGE_PROGRESS_STATES:
            raise RuntimeError(f"package progress contains invalid state for {filename}: {state!r}")
        if not isinstance(size_bytes, int) or isinstance(size_bytes, bool) or size_bytes < 0:
            raise RuntimeError(f"package progress contains invalid size for {filename}")
        if not isinstance(record.get("verified"), bool) or not isinstance(record.get("needs_acquisition"), bool):
            raise RuntimeError(f"package progress contains invalid verification flags for {filename}")
        seen.add(filename)
        resolved_packages += 1
        resolved_bytes += size_bytes
        states[state]["packages"] += 1
        states[state]["bytes"] += size_bytes
        if record["verified"]:
            verified_packages += 1
            verified_bytes += size_bytes
        if record["needs_acquisition"]:
            pending_packages += 1
            pending_bytes += size_bytes
    return {
        "display": f"{resolved_packages} resolved / {verified_packages} verified / {pending_packages} pending",
        "resolved": {"packages": resolved_packages, "bytes": resolved_bytes},
        "verified": {"packages": verified_packages, "bytes": verified_bytes},
        "pending": {"packages": pending_packages, "bytes": pending_bytes},
        "states": states,
    }


def reconcile_package_progress_from_cache_audit(
    progress: list[dict[str, Any]],
    audit: dict[str, Any],
) -> None:
    """Apply the A4 cache audit to the current per-package state records."""
    reused = set(audit["reused_filenames"])
    pending = set(audit["pending_filenames"])
    corrupt = set(audit["corrupt_entries_removed"])
    progress_filenames = {record["filename"] for record in progress}
    if reused | pending != progress_filenames or reused & pending:
        raise RuntimeError("cache audit cannot be reconciled with package progress")
    corrupt_archives = corrupt & progress_filenames
    for record in progress:
        filename = record["filename"]
        record.update(
            {
                "batch": None,
                "attempt": None,
                "mirror": None,
                "failure_class": None,
            }
        )
        if filename in reused:
            record.update(
                {
                    "state": "reused_verified",
                    "verified": True,
                    "needs_acquisition": False,
                }
            )
        elif filename in corrupt_archives:
            record.update(
                {
                    "state": "corrupt_removed",
                    "verified": False,
                    "needs_acquisition": True,
                }
            )
        else:
            record.update(
                {
                    "state": "pending",
                    "verified": False,
                    "needs_acquisition": True,
                }
            )


def record_package_progress_attempt(
    progress: list[dict[str, Any]],
    batch_index: int,
    attempt_record: dict[str, Any],
) -> None:
    """Record one mirror attempt against the package state ledger."""
    by_filename = {record["filename"]: record for record in progress}
    verified = set(attempt_record["verified_filenames"])
    pending = set(attempt_record["pending_filenames"])
    mirror = attempt_record["mirror"].get("server")
    attempt_number = attempt_record["attempt"]
    failure_class = attempt_record.get("failure_class")
    for filename in verified:
        if filename not in by_filename:
            raise RuntimeError(f"mirror attempt verified unknown package identity: {filename}")
        by_filename[filename].update(
            {
                "state": "downloaded_verified",
                "verified": True,
                "needs_acquisition": False,
                "batch": batch_index,
                "attempt": attempt_number,
                "mirror": mirror,
                "failure_class": None,
            }
        )
    for filename in pending:
        if filename not in by_filename:
            raise RuntimeError(f"mirror attempt left unknown package identity pending: {filename}")
        by_filename[filename].update(
            {
                "state": "failed" if attempt_record.get("result") == "fail" else "pending",
                "verified": False,
                "needs_acquisition": True,
                "batch": batch_index,
                "attempt": attempt_number,
                "mirror": mirror,
                "failure_class": failure_class,
            }
        )


def verified_cached_filenames(cache: Path, packages: list[dict[str, Any]]) -> set[str]:
    verified: set[str] = set()
    for package in packages:
        cached = cache / package["filename"]
        if cached.is_file() and sha256_file(cached) == package["sha256"]:
            verified.add(package["filename"])
    return verified


def parse_artix_mirror_servers(text: str) -> list[dict[str, Any]]:
    """Return active HTTPS Artix mirror templates in mirrorlist order."""
    candidates: list[dict[str, Any]] = []
    seen: set[str] = set()
    for line_number, raw in enumerate(text.splitlines(), start=1):
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = re.fullmatch(r"Server\s*=\s*(\S+)", stripped)
        if match is None:
            continue
        server = match.group(1)
        if not server.startswith("https://"):
            continue
        if "$repo" not in server or "$arch" not in server:
            raise RuntimeError(f"active HTTPS Artix mirror lacks $repo/$arch placeholders at line {line_number}")
        if server in seen:
            continue
        seen.add(server)
        candidates.append({"server": server, "mirrorlist_line": line_number})
    if not candidates:
        raise RuntimeError("Artix mirrorlist contains no active HTTPS repository mirrors")
    return candidates


def select_first_healthy_mirror(candidates: list[dict[str, Any]], probe: Any) -> tuple[dict[str, Any] | None, list[dict[str, Any]]]:
    """Select the first mirror whose probe proves all locked repositories usable."""
    attempts: list[dict[str, Any]] = []
    for candidate in candidates:
        ok, detail = probe(candidate)
        attempt = copy.deepcopy(candidate)
        attempt["result"] = "pass" if ok else "fail"
        attempt["detail"] = detail
        attempts.append(attempt)
        if ok:
            return copy.deepcopy(candidate), attempts
    return None, attempts


def render_anchor_pacman_config(original: str, server_template: str) -> str:
    if not server_template.startswith("https://") or "$repo" not in server_template or "$arch" not in server_template:
        raise RuntimeError("Artix repository anchor must be an HTTPS mirror template containing $repo and $arch")
    return render_frozen_pacman_config(original, server_template)


def classify_package_acquisition_failure(
    error: BaseException | None,
    pending_filenames: set[str],
    detail: str | None,
) -> str | None:
    """Return a stable failure class for one package-mirror attempt."""
    if error is None:
        return "verification_incomplete" if pending_filenames else None
    if isinstance(error, (KeyboardInterrupt, SystemExit)):
        return "interrupted"
    text = " ".join(
        part
        for part in (
            detail,
            getattr(error, "stderr", None),
            getattr(error, "stdout", None),
            str(error),
        )
        if isinstance(part, str) and part
    ).lower()
    if "operation too slow" in text or "timed out" in text or "timeout" in text:
        return "timeout"
    if "404" in text or "not found" in text:
        return "http_not_found"
    if "tls" in text or "ssl" in text or "unexpected eof" in text or "certificate" in text:
        return "tls"
    if "could not resolve" in text or "name resolution" in text or "name or service not known" in text:
        return "dns"
    if "connection refused" in text or "failed to connect" in text or "could not connect" in text:
        return "connection"
    if "sha-256" in text or "checksum" in text or "signature" in text or "corrupt" in text:
        return "integrity"
    if isinstance(error, subprocess.CalledProcessError):
        return "process_failed"
    return "runtime_error"


def concise_process_failure(error: subprocess.CalledProcessError) -> str:
    text = (error.stderr or error.stdout or str(error)).strip()
    if not text:
        text = f"exit {error.returncode}"
    return text.splitlines()[-1][:500]


def render_frozen_pacman_config(original: str, local_server: str) -> str:
    """Retain locked pacman options but replace repository sources with one server."""
    lines = original.splitlines()
    first_repo = None
    enabled_repositories: list[str] = []
    for index, raw in enumerate(lines):
        stripped = raw.strip()
        if stripped.startswith("[") and stripped.endswith("]") and not stripped.startswith("[#"):
            name = stripped[1:-1]
            if name != "options":
                if first_repo is None:
                    first_repo = index
                enabled_repositories.append(name)
    if tuple(enabled_repositories) != FROZEN_REPOSITORIES:
        raise RuntimeError(
            "stable artools pacman repositories drifted from system/world/galaxy: "
            + ",".join(enabled_repositories)
        )
    if first_repo is None:
        raise RuntimeError("stable artools pacman configuration has no enabled repositories")
    prefix = "\n".join(lines[:first_repo]).rstrip() + "\n\n"
    repositories = "\n\n".join(f"[{name}]\nServer = {local_server}" for name in FROZEN_REPOSITORIES)
    return prefix + repositories + "\n"


def repository_closure_evidence_path(manifest_file: Path) -> Path:
    return manifest_file.parent / REPOSITORY_CLOSURE_FILE


def write_repository_closure_evidence(manifest_file: Path, value: dict[str, Any]) -> None:
    path = repository_closure_evidence_path(manifest_file)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    uid = os.environ.get("SUDO_UID")
    gid = os.environ.get("SUDO_GID")
    if uid and gid:
        os.chown(path, int(uid), int(gid))


def freeze_resolved_package_evidence(
    manifest_file: Path,
    evidence: dict[str, Any],
    packages: list[dict[str, Any]],
) -> None:
    """Persist the exact resolved graph before any network acquisition can fail."""
    evidence["packages"] = copy.deepcopy(packages)
    evidence["package_progress"] = initialize_package_progress(packages)
    evidence["progress_summary"] = summarize_package_progress(evidence["package_progress"])
    write_repository_closure_evidence(manifest_file, evidence)


def prepare_repository_closure(
    repo: Path,
    native_config: dict[str, Any],
    manifest: dict[str, Any],
    manifest_file: Path,
) -> dict[str, Any]:
    """Freeze one coherent Artix repository/package universe before buildiso starts."""
    p = paths(repo, native_config)
    resolved = manifest["_resolved"]
    workspace = resolved["workspace"]
    common = workspace / "iso-profiles/common/common.yaml"
    profile = workspace / "iso-profiles/portus/profile.yaml"
    targets = parse_artools_package_targets(
        common.read_text(encoding="utf-8"),
        profile.read_text(encoding="utf-8"),
    )
    pacman_config = manifest["artools"]["stable_pacman_config"]
    pacman_config_host = p["root"] / pacman_config.lstrip("/")
    mirrorlist_host = p["root"] / "etc/pacman.d/mirrorlist"
    if not pacman_config_host.is_file() or not mirrorlist_host.is_file():
        raise RuntimeError("native Artix context is missing the locked pacman config or mirrorlist")

    cache_host = repo / PACKAGE_CACHE_PATH
    cache_host.mkdir(parents=True, exist_ok=True)
    cache_target = p["root"] / "var/cache/pacman/pkg"
    cache_target.mkdir(parents=True, exist_ok=True)
    if is_mountpoint(cache_target):
        raise RuntimeError("native Artix package cache target is already mounted before closure setup")
    run(["mount", "--bind", str(cache_host), str(cache_target)])
    run(["mount", "--make-private", str(cache_target)])
    outer_cache_uid = repo.stat().st_uid
    outer_cache_gid = repo.stat().st_gid
    chroot(repo, native_config, ["/usr/bin/chown", "-R", "alpm:alpm", "/var/cache/pacman/pkg"])
    chroot(repo, native_config, ["/usr/bin/chmod", "0755", "/var/cache/pacman/pkg"])

    closure_host = resolved["native_root"] / "repository-closure"
    if closure_host.exists():
        raise RuntimeError(f"run-owned repository closure already exists: {closure_host}")
    closure_host.mkdir(parents=True, exist_ok=False)
    closure_db = closure_host / "pacman-db"
    validation_db = closure_host / "validation-db"
    frozen_repo = closure_host / "repo"
    for directory in (closure_db / "local", closure_db / "sync", validation_db / "local", validation_db / "sync", frozen_repo):
        directory.mkdir(parents=True, exist_ok=True)

    closure_inside = "/run/portus-build/repository-closure"
    closure_db_inside = f"{closure_inside}/pacman-db"
    validation_db_inside = f"{closure_inside}/validation-db"
    local_server = f"file://{closure_inside}/repo"
    original_config = pacman_config_host.read_text(encoding="utf-8", errors="strict")
    evidence: dict[str, Any] = {
        "schema_version": 1,
        "run_id": manifest["run_id"],
        "captured_at": utc_now(),
        "status": "fail",
        "package_targets": targets,
        "profile_sha256": {
            "common": sha256_file(common),
            "portus": sha256_file(profile),
        },
        "network_pacman_config": {
            "path": pacman_config,
            "sha256": hashlib.sha256(original_config.encode("utf-8")).hexdigest(),
        },
        "mirrorlist": {
            "path": "/etc/pacman.d/mirrorlist",
            "sha256": sha256_file(mirrorlist_host),
        },
        "repository_anchor": {
            "status": "pending",
            "selected": None,
            "attempts": [],
            "candidate_count": 0,
            "database_sync_locked": False,
        },
        "repositories": [],
        "packages": [],
        "package_progress": [],
        "progress_summary": summarize_package_progress([]),
        "cache": {
            "path": PACKAGE_CACHE_PATH.as_posix(),
            "audit": None,
            "reused_count": 0,
            "downloaded_or_recovered_count": 0,
            "corrupt_entries_removed": [],
            "stale_partial_entries_removed": [],
            "verified_count": 0,
            "read_only_for_buildiso": False,
            "outer_owner_restored": False,
        },
        "frozen_pacman_config": None,
        "local_validation": None,
        "failure": None,
    }
    try:
        mirror_candidates = parse_artix_mirror_servers(mirrorlist_host.read_text(encoding="utf-8", errors="strict"))
        evidence["repository_anchor"]["candidate_count"] = len(mirror_candidates)
        anchor_config_host = closure_host / "anchor-pacman.conf"
        anchor_config_inside = f"{closure_inside}/anchor-pacman.conf"
        chroot(repo, native_config, ["/usr/bin/chown", "-R", "alpm:alpm", closure_db_inside])

        def probe_anchor(candidate: dict[str, Any]) -> tuple[bool, str | None]:
            sync_dir = closure_db / "sync"
            lock_file = closure_db / "db.lck"
            if lock_file.exists():
                lock_file.unlink()
            for item in tuple(sync_dir.iterdir()):
                if item.is_dir() and not item.is_symlink():
                    shutil.rmtree(item)
                else:
                    item.unlink()
            anchor_config = render_anchor_pacman_config(original_config, candidate["server"])
            anchor_config_host.write_text(anchor_config, encoding="utf-8")
            try:
                chroot(
                    repo,
                    native_config,
                    [
                        "/usr/bin/pacman",
                        "-Syy",
                        "--noconfirm",
                        "--config",
                        anchor_config_inside,
                        "--dbpath",
                        closure_db_inside,
                    ],
                    capture=True,
                )
            except subprocess.CalledProcessError as error:
                return False, concise_process_failure(error)
            missing = [
                repository
                for repository in FROZEN_REPOSITORIES
                if not (sync_dir / f"{repository}.db").is_file()
                or (sync_dir / f"{repository}.db").stat().st_size <= 0
            ]
            if missing:
                return False, "missing or empty repository databases: " + ", ".join(missing)
            return True, None

        selected_anchor, anchor_attempts = select_first_healthy_mirror(mirror_candidates, probe_anchor)
        evidence["repository_anchor"]["attempts"] = anchor_attempts
        if selected_anchor is None:
            evidence["repository_anchor"]["status"] = "fail"
            write_repository_closure_evidence(manifest_file, evidence)
            raise RuntimeError("no healthy Artix HTTPS anchor supplied system/world/galaxy repository databases")

        anchor_config = anchor_config_host.read_text(encoding="utf-8", errors="strict")
        evidence["repository_anchor"].update(
            {
                "status": "selected",
                "selected": selected_anchor,
                "pacman_config_path": anchor_config_inside,
                "pacman_config_sha256": hashlib.sha256(anchor_config.encode("utf-8")).hexdigest(),
            }
        )
        repository_records: list[dict[str, Any]] = []
        sync_dir = closure_db / "sync"
        for repository in FROZEN_REPOSITORIES:
            source_db = sync_dir / f"{repository}.db"
            destination = frozen_repo / f"{repository}.db"
            shutil.copy2(source_db, destination)
            repository_records.append(
                {
                    "name": repository,
                    "database_sha256": sha256_file(source_db),
                    "database_size_bytes": source_db.stat().st_size,
                }
            )
        evidence["repositories"] = repository_records
        for item in sync_dir.iterdir():
            if item.is_file() or item.is_symlink():
                item.chmod(0o444)
        sync_dir.chmod(0o555)
        evidence["repository_anchor"]["database_sync_locked"] = True
        evidence["repository_anchor"]["status"] = "pass"
        write_repository_closure_evidence(manifest_file, evidence)

        resolve_command = [
            "/usr/bin/pacman",
            "-Sp",
            "--config",
            anchor_config_inside,
            "--dbpath",
            closure_db_inside,
            "--print-format",
            PACMAN_PRINT_FORMAT,
            *targets,
        ]
        resolved_result = chroot(repo, native_config, resolve_command, capture=True)
        packages = parse_pacman_print_rows(resolved_result.stdout)
        resolved_names = {entry["name"] for entry in packages}
        missing_targets = sorted(set(targets) - resolved_names)
        if missing_targets:
            raise RuntimeError("package closure omitted explicit targets: " + ", ".join(missing_targets))

        freeze_resolved_package_evidence(manifest_file, evidence, packages)

        cache_audit = audit_persistent_package_cache(cache_host, packages)
        reconcile_package_progress_from_cache_audit(evidence["package_progress"], cache_audit)
        evidence["progress_summary"] = summarize_package_progress(evidence["package_progress"])
        cache_valid_before = set(cache_audit["reused_filenames"])
        reused_count = cache_audit["reused_count"]
        pending_filenames = set(cache_audit["pending_filenames"])
        pending_packages = [
            package for package in packages if package["filename"] in pending_filenames
        ]
        if len(pending_packages) != cache_audit["pending_count"]:
            raise RuntimeError("persistent package cache audit pending identities do not match frozen closure")
        prefetch_batches = plan_package_prefetch_batches(pending_packages)
        prefetch_mirrors = ordered_prefetch_mirrors(mirror_candidates, selected_anchor)
        prefetch_config_host = closure_host / "prefetch-pacman.conf"
        prefetch_config_inside = f"{closure_inside}/prefetch-pacman.conf"
        prefetched_verified: set[str] = set()
        evidence["cache"].update(
            {
                "audit": copy.deepcopy(cache_audit),
                "reused_count": reused_count,
                "corrupt_entries_removed": copy.deepcopy(cache_audit["corrupt_entries_removed"]),
                "stale_partial_entries_removed": copy.deepcopy(cache_audit["stale_partial_entries_removed"]),
                "prefetch_batch_limit_bytes": PACKAGE_PREFETCH_BATCH_LIMIT_BYTES,
                "prefetch_batch_count": len(prefetch_batches),
                "prefetch_completed_batch_count": 0,
                "prefetch_pending_count": len(pending_packages),
                "prefetch_mirror_attempt_limit": PACKAGE_PREFETCH_MAX_MIRROR_ATTEMPTS,
                "prefetch_mirrors": copy.deepcopy(prefetch_mirrors),
                "prefetch_attempts": [],
            }
        )
        write_repository_closure_evidence(manifest_file, evidence)

        def fetch_prefetch_batch(batch_index: int, batch: list[dict[str, Any]]) -> None:
            def fetch_mirror_attempt(
                _attempt_index: int,
                mirror: dict[str, Any],
                attempt_packages: list[dict[str, Any]],
            ) -> None:
                prefetch_config = render_anchor_pacman_config(original_config, mirror["server"])
                prefetch_config_host.write_text(prefetch_config, encoding="utf-8")
                chroot(
                    repo,
                    native_config,
                    prefetch_pacman_command(
                        prefetch_config_inside,
                        closure_db_inside,
                        "/var/cache/pacman/pkg",
                        attempt_packages,
                    ),
                )

            def record_mirror_attempt(
                attempt_record: dict[str, Any],
                _verified: set[str],
                _pending: set[str],
            ) -> None:
                evidence["cache"]["prefetch_attempts"].append(
                    {"batch": batch_index, **attempt_record}
                )
                record_package_progress_attempt(
                    evidence["package_progress"],
                    batch_index,
                    attempt_record,
                )
                evidence["progress_summary"] = summarize_package_progress(evidence["package_progress"])
                write_repository_closure_evidence(manifest_file, evidence)

            acquire_batch_with_mirror_failover(
                cache_host,
                batch,
                prefetch_mirrors,
                fetch_mirror_attempt,
                record_mirror_attempt,
            )

        def record_prefetch_progress(
            batch_index: int,
            _batch_count: int,
            _batch: list[dict[str, Any]],
            verified_batch: set[str],
            pending_filenames: set[str],
            _batch_error: BaseException | None,
            batch_complete: bool,
        ) -> None:
            prefetched_verified.update(verified_batch)
            evidence["cache"]["downloaded_or_recovered_count"] = len(prefetched_verified)
            evidence["cache"]["verified_count"] = reused_count + len(prefetched_verified)
            evidence["cache"]["prefetch_pending_count"] = len(pending_filenames)
            evidence["progress_summary"] = summarize_package_progress(evidence["package_progress"])
            if batch_complete:
                evidence["cache"]["prefetch_completed_batch_count"] = batch_index
            write_repository_closure_evidence(manifest_file, evidence)

        acquire_prefetch_batches(
            cache_host,
            prefetch_batches,
            fetch_prefetch_batch,
            record_prefetch_progress,
        )
        verify_cached_package_files(cache_host, packages)
        for package in packages:
            link = frozen_repo / package["filename"]
            link.symlink_to(Path("/var/cache/pacman/pkg") / package["filename"])
            signature = cache_host / f"{package['filename']}.sig"
            if signature.is_file():
                (frozen_repo / signature.name).symlink_to(Path("/var/cache/pacman/pkg") / signature.name)
            package["cached_before"] = package["filename"] in cache_valid_before
        evidence["packages"] = packages
        evidence["cache"].update(
            {
                "reused_count": reused_count,
                "downloaded_or_recovered_count": len(packages) - reused_count,
                "corrupt_entries_removed": copy.deepcopy(cache_audit["corrupt_entries_removed"]),
                "stale_partial_entries_removed": copy.deepcopy(cache_audit["stale_partial_entries_removed"]),
                "verified_count": len(packages),
                "prefetch_pending_count": 0,
                "read_only_for_buildiso": False,
                "outer_owner_restored": False,
            }
        )

        frozen_config = render_frozen_pacman_config(original_config, local_server)
        pacman_config_host.write_text(frozen_config, encoding="utf-8")
        evidence["frozen_pacman_config"] = {
            "path": pacman_config,
            "sha256": hashlib.sha256(frozen_config.encode("utf-8")).hexdigest(),
            "server": local_server,
            "network_repositories_enabled": False,
        }

        chroot(
            repo,
            native_config,
            ["/usr/bin/chown", "-R", "alpm:alpm", validation_db_inside],
        )
        chroot(
            repo,
            native_config,
            [
                "/usr/bin/pacman",
                "-Syy",
                "--noconfirm",
                "--config",
                pacman_config,
                "--dbpath",
                validation_db_inside,
            ],
        )
        validation_result = chroot(
            repo,
            native_config,
            [
                "/usr/bin/pacman",
                "-Sp",
                "--config",
                pacman_config,
                "--dbpath",
                validation_db_inside,
                "--print-format",
                PACMAN_PRINT_FORMAT,
                *targets,
            ],
            capture=True,
        )
        validation_packages = parse_pacman_print_rows(validation_result.stdout)
        require_same_package_closure(packages, validation_packages)
        chroot(
            repo,
            native_config,
            [
                "/usr/bin/pacman",
                "-Sw",
                "--noconfirm",
                "--config",
                pacman_config,
                "--dbpath",
                validation_db_inside,
                "--cachedir",
                "/var/cache/pacman/pkg",
                *targets,
            ],
        )
        evidence["local_validation"] = {
            "resolved_package_count": len(validation_packages),
            "resolution_matches": True,
            "package_files_verified": True,
            "network_repositories_enabled": False,
        }
        run(["chown", "-R", f"{outer_cache_uid}:{outer_cache_gid}", str(cache_host)])
        evidence["cache"]["outer_owner_restored"] = True
        run(["mount", "-o", "remount,bind,ro", str(cache_target)])
        evidence["cache"]["read_only_for_buildiso"] = True
        evidence["status"] = "pass"
        evidence["failure"] = None
        write_repository_closure_evidence(manifest_file, evidence)
        return evidence
    except BaseException as error:
        evidence["failure"] = str(error)
        evidence["progress_summary"] = summarize_package_progress(evidence.get("package_progress", []))
        write_repository_closure_evidence(manifest_file, evidence)
        raise
    finally:
        # The reusable cache belongs to the unprivileged build owner outside the
        # private namespace.  Artix's DownloadUser owns it only while prefetching.
        # Restore ownership even on closure failure so recovery never requires
        # retaining an alpm-owned generated tree on the outer build host.
        try:
            run(["chown", "-R", f"{outer_cache_uid}:{outer_cache_gid}", str(cache_host)])
        except Exception as ownership_error:
            print(f"warning: failed to restore Artix package-cache ownership: {ownership_error}", file=sys.stderr)


def parse_pacman_desc(text: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index]
        if line.startswith("%") and line.endswith("%"):
            key = line.strip("%")
            index += 1
            values: list[str] = []
            while index < len(lines) and lines[index] != "":
                values.append(lines[index])
                index += 1
            fields[key] = "\n".join(values)
        index += 1
    return fields


def inspect_packages(repo: Path, config: dict[str, Any], names: list[str]) -> int:
    """Read synchronized Artix package DBs directly; no chroot/root required."""
    require_linux_x86_64()
    p = paths(repo, config)
    if not is_mountpoint(p["root"]):
        raise RuntimeError("Artix context is not mounted")
    sync_dir = p["root"] / "var/lib/pacman/sync"
    if not sync_dir.is_dir():
        raise RuntimeError("Artix synchronized package database directory is missing")
    wanted = set(names)
    found: dict[str, dict[str, str]] = {}
    for db_path in sorted(sync_dir.glob("*.db")):
        repository = db_path.stem
        try:
            with tarfile.open(db_path, mode="r:*") as archive:
                for member in archive.getmembers():
                    if not member.isfile() or not member.name.endswith("/desc"):
                        continue
                    handle = archive.extractfile(member)
                    if handle is None:
                        continue
                    fields = parse_pacman_desc(handle.read().decode("utf-8", errors="replace"))
                    name = fields.get("NAME")
                    if name not in wanted:
                        continue
                    found[name] = {
                        "repository": repository,
                        "version": fields.get("VERSION", ""),
                        "architecture": fields.get("ARCH", ""),
                        "description": fields.get("DESC", ""),
                        "licenses": fields.get("LICENSE", ""),
                        "filename": fields.get("FILENAME", ""),
                        "sha256": fields.get("SHA256SUM", ""),
                        "compressed_size": fields.get("CSIZE", ""),
                    }
        except (OSError, tarfile.TarError):
            continue
    value = {
        "schema_version": 1,
        "captured_at": utc_now(),
        "requested": names,
        "found": found,
        "missing": [name for name in names if name not in found],
    }
    print(json.dumps(value, indent=2, sort_keys=True))
    return 0


def inspect_browser_archive(repo: Path) -> int:
    """Verify and inspect only browser executable/desktop integration members."""
    candidate = repo / "portusos-build/out/l2-inspect/ungoogled-chromium.pkg.tar.zst"
    expected = "754d33d4bebe9effed173232a9b3f6ae409d873f076f1f5262995ebd15c93f46"
    if not candidate.is_file():
        raise RuntimeError(f"browser inspection archive is missing: {candidate}")
    digest = sha256_file(candidate)
    if digest != expected:
        raise RuntimeError(f"browser inspection archive SHA-256 mismatch: {digest}")
    with tarfile.open(candidate, mode="r:*") as archive:
        members = [
            member.name
            for member in archive.getmembers()
            if member.name.startswith("usr/share/applications/")
            or member.name in {"usr/bin/chromium", "usr/bin/ungoogled-chromium"}
            or member.name.endswith("/chromium")
            or member.name.endswith("/ungoogled-chromium")
        ]
    print(json.dumps({"sha256": digest, "members": members}, indent=2, sort_keys=True))
    return 0


def inspect_archives(repo: Path, names: list[str]) -> int:
    """List verified inspection-package archive members from the bounded out directory."""
    base = repo / "portusos-build/out/l2-inspect"
    output: dict[str, list[str]] = {}
    for name in names:
        candidate_name = Path(name)
        if candidate_name.name != name or name in {"", ".", ".."}:
            raise ValueError("inspect-archives accepts archive basenames only")
        candidate = base / name
        if not candidate.is_file():
            raise RuntimeError(f"inspection archive is missing: {candidate}")
        with tarfile.open(candidate, mode="r:*") as archive:
            output[name] = [member.name for member in archive.getmembers()]
    print(json.dumps(output, indent=2, sort_keys=True))
    return 0


def inspect_artools(repo: Path, config: dict[str, Any]) -> int:
    """Capture selected installed artools implementation files without mutation/root."""
    require_linux_x86_64()
    p = paths(repo, config)
    if not is_mountpoint(p["root"]):
        raise RuntimeError("Artix context is not mounted")
    allowed = (
        "/usr/bin/buildiso",
        "/etc/artools/artools-iso.conf",
        "/usr/share/artools/lib/iso/config.sh",
        "/usr/share/artools/lib/iso/profile.sh",
        "/usr/share/artools/lib/iso/iso.sh",
        "/usr/share/artools/lib/iso/services.sh",
        "/usr/share/artools/pacman.conf.d/iso-x86_64.conf",
    )
    files: dict[str, str | None] = {}
    for absolute in allowed:
        candidate = p["root"] / absolute.lstrip("/")
        if candidate.is_file():
            files[absolute] = candidate.read_text(encoding="utf-8", errors="replace")
        else:
            files[absolute] = None
    output = repo / "portusos-build/out/l2-inspect/artools-inspect.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    value = {
        "schema_version": 1,
        "captured_at": utc_now(),
        "context_state_path": config["context"]["state_path"],
        "files": files,
    }
    output.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    uid = os.environ.get("SUDO_UID")
    gid = os.environ.get("SUDO_GID")
    if uid and gid:
        os.chown(output, int(uid), int(gid))
    print(json.dumps(value, indent=2, sort_keys=True))
    return 0


def status(repo: Path, config: dict[str, Any]) -> int:
    require_linux_x86_64()
    seed_ok = False
    seed_error = None
    try:
        verify_seed(repo, config, quiet=True)
        seed_ok = True
    except RuntimeError as error:
        seed_error = str(error)
    p = paths(repo, config)
    state = None
    if p["state"].is_file():
        try:
            state = json.loads(p["state"].read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            state = None
    value = {
        "seed_verified": seed_ok,
        "seed_error": seed_error,
        "lower_mounted": is_mountpoint(p["lower"]) if p["lower"].exists() else False,
        "root_mounted": is_mountpoint(p["root"]) if p["root"].exists() else False,
        "state_present": state is not None,
        "state": state,
    }
    print(json.dumps(value, indent=2, sort_keys=True))
    return 0 if seed_ok else EX_UNAVAILABLE


def manifest_path(repo: Path, relative: str, allowed_root: str, label: str) -> Path:
    candidate = Path(relative)
    if candidate.is_absolute() or ".." in candidate.parts:
        raise ValueError(f"{label} must be a bounded repository-relative path")
    resolved = (repo / candidate).resolve()
    base = (repo / allowed_root).resolve()
    if resolved != base and not resolved.is_relative_to(base):
        raise ValueError(f"{label} must remain below {allowed_root}")
    return resolved


def load_native_manifest(repo: Path, manifest_file: Path) -> dict[str, Any]:
    resolved_manifest = manifest_file.resolve()
    out_root = (repo / "portusos-build/out").resolve()
    if not resolved_manifest.is_relative_to(out_root):
        raise ValueError("native staging manifest must remain below portusos-build/out")
    manifest = json.loads(resolved_manifest.read_text(encoding="utf-8"))
    if manifest.get("schema_version") != 1:
        raise ValueError("unsupported native staging manifest schema")
    run_id = manifest.get("run_id")
    if not isinstance(run_id, str) or not run_id or any(ch in run_id for ch in "/\\\n\r"):
        raise ValueError("invalid native staging run id")
    paths_value = manifest.get("paths")
    if not isinstance(paths_value, dict):
        raise ValueError("native staging manifest paths are missing")
    native_root = manifest_path(repo, paths_value.get("native_work_root", ""), "portusos-build/work", "native work root")
    workspace = manifest_path(repo, paths_value.get("workspace", ""), "portusos-build/work", "workspace")
    chroots = manifest_path(repo, paths_value.get("chroots", ""), "portusos-build/work", "chroots")
    iso_out = manifest_path(repo, paths_value.get("iso_out", ""), "portusos-build/work", "ISO output")
    artifact_dir = manifest_path(repo, paths_value.get("artifact_dir", ""), "portusos-build/out", "artifact directory")
    for child, label in ((workspace, "workspace"), (chroots, "chroots"), (iso_out, "ISO output")):
        if child != native_root and not child.is_relative_to(native_root):
            raise ValueError(f"{label} must remain below the native work root")
    if not (workspace / "iso-profiles/common/common.yaml").is_file() or not (workspace / "iso-profiles/portus/profile.yaml").is_file():
        raise ValueError("run-owned artools profile workspace is incomplete")
    root_overlay = workspace / "iso-profiles/portus/root-overlay"
    for required in (
        "usr/bin/portus-os",
        "usr/local/bin/codex",
        "usr/local/bin/tunnel-client",
        "opt/portus/portus-browser/apps/portus-extension/manifest.json",
        "opt/portus/portus-mcp/node_modules/tsx/dist/cli.mjs",
    ):
        if not (root_overlay / required).exists() and not (root_overlay / required).is_symlink():
            raise ValueError(f"staged first-ISO payload is incomplete: {required}")
    artools = manifest.get("artools")
    expected = {
        "profile": "portus",
        "stability": "stable",
        "architecture": "x86_64",
        "init": "openrc",
        "workspace_profiles_dir": "iso-profiles",
        "stable_pacman_config": "/usr/share/artools/pacman.conf.d/iso-x86_64.conf",
        "output_filename_prefix": "artix-portus-openrc-",
        "live_boot_kernel_package": "linux-lts",
        "output_filename_suffix": "-x86_64.iso",
    }
    if artools != expected:
        raise ValueError("native staging manifest artools mapping differs from the locked adapter")
    revision = run(["git", "rev-parse", "HEAD"], capture=True).stdout.strip()
    if manifest.get("source_revision") != revision:
        raise ValueError("native staging manifest source revision differs from the checked-out repository")
    artifact_dir.mkdir(parents=True, exist_ok=True)
    if any(artifact_dir.rglob("*.iso")):
        raise ValueError("artifact directory must not contain an ISO before native construction")
    manifest["_resolved"] = {
        "native_root": native_root,
        "workspace": workspace,
        "chroots": chroots,
        "iso_out": iso_out,
        "artifact_dir": artifact_dir,
    }
    return manifest


def mount_namespace_references(markers: list[Path]) -> list[dict[str, Any]]:
    """Find processes whose mount tables still reference any supplied path."""
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
        references.append(
            {
                "pid": int(entry.name),
                "comm": comm,
                "mount_namespace": namespace,
                "markers": matched,
            }
        )
    return references


def native_helper_processes(run_id: str) -> list[str]:
    """Return surviving private-namespace helper processes for one build run."""
    references: list[str] = []
    run_marker = run_id.encode("utf-8")
    helper_marker = b"_build-iso-inner"
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit() or int(entry.name) == os.getpid():
            continue
        try:
            cmdline = (entry / "cmdline").read_bytes()
        except OSError:
            continue
        if helper_marker not in cmdline or run_marker not in cmdline:
            continue
        try:
            comm = (entry / "comm").read_text(encoding="utf-8").strip()
        except OSError:
            comm = "?"
        references.append(f"pid={entry.name} comm={comm} helper=_build-iso-inner")
    return references


def loop_devices_for_backing_file(backing_file: Path) -> list[str]:
    """Return loop devices whose kernel backing-file record points at backing_file."""
    expected = os.path.realpath(backing_file)
    devices: list[str] = []
    sys_block = Path("/sys/block")
    if not sys_block.is_dir():
        return devices
    for entry in sorted(sys_block.glob("loop*")):
        backing = entry / "loop/backing_file"
        try:
            value = backing.read_text(encoding="utf-8", errors="replace").strip()
        except OSError:
            continue
        if not value:
            continue
        candidate = value if value.startswith("/") else "/" + value
        if os.path.realpath(candidate) == expected:
            devices.append(f"/dev/{entry.name}")
    return devices


def write_native_cleanup_evidence(
    manifest_file: Path,
    manifest: dict[str, Any],
    native_context_root: Path,
    *,
    namespace_action_succeeded: bool,
    mount_references_before: list[dict[str, Any]],
    mount_references_after: list[dict[str, Any]],
    process_references_before: list[str],
    process_references_after: list[str],
    helper_processes_before: list[str],
    helper_processes_after: list[str],
    seed_loop_devices_before: list[str],
    seed_loop_devices_after: list[str],
    leaked_seed_loop_devices: list[str],
    scratch_removed: bool,
    status: str,
) -> None:
    result_path = manifest_file.parent / "native-cleanup.json"
    value = {
        "schema_version": 1,
        "run_id": manifest["run_id"],
        "status": status,
        "namespace_action_succeeded": namespace_action_succeeded,
        "native_context_root": native_context_root.relative_to(repo_root()).as_posix(),
        "mount_references_before": mount_references_before,
        "mount_references_after": mount_references_after,
        "process_references_before": process_references_before,
        "process_references_after": process_references_after,
        "helper_processes_before": helper_processes_before,
        "helper_processes_after": helper_processes_after,
        "seed_loop_devices_before": seed_loop_devices_before,
        "seed_loop_devices_after": seed_loop_devices_after,
        "leaked_seed_loop_devices": leaked_seed_loop_devices,
        "scratch_removed": scratch_removed,
    }
    result_path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    uid = os.environ.get("SUDO_UID")
    gid = os.environ.get("SUDO_GID")
    if uid and gid:
        os.chown(result_path, int(uid), int(gid))


def build_iso(repo: Path, config: dict[str, Any], manifest_file: Path) -> None:
    manifest = load_native_manifest(repo, manifest_file)
    native_context_root = manifest["_resolved"]["native_root"] / "artix-context"
    seed = paths(repo, config)["seed"]
    mount_refs_before = mount_namespace_references([native_context_root])
    process_refs_before = process_references(native_context_root)
    helper_processes_before = native_helper_processes(manifest["run_id"])
    seed_loops_before = loop_devices_for_backing_file(seed)
    if mount_refs_before or process_refs_before or helper_processes_before:
        write_native_cleanup_evidence(
            manifest_file,
            manifest,
            native_context_root,
            namespace_action_succeeded=False,
            mount_references_before=mount_refs_before,
            mount_references_after=mount_refs_before,
            process_references_before=process_refs_before,
            process_references_after=process_refs_before,
            helper_processes_before=helper_processes_before,
            helper_processes_after=helper_processes_before,
            seed_loop_devices_before=seed_loops_before,
            seed_loop_devices_after=seed_loops_before,
            leaked_seed_loop_devices=[],
            scratch_removed=not native_context_root.exists(),
            status="fail",
        )
        raise RuntimeError("current native run already has mount/process references before namespace execution")

    action_error: BaseException | None = None
    try:
        run_private_namespace(repo, config, "_build-iso-inner", manifest=manifest_file)
    except BaseException as error:  # cleanup evidence is required on normal failure and SIGINT/KeyboardInterrupt paths
        action_error = error

    mount_refs_after = mount_namespace_references([native_context_root])
    process_refs_after = process_references(native_context_root)
    helper_processes_after = native_helper_processes(manifest["run_id"])
    seed_loops_after = loop_devices_for_backing_file(seed)
    leaked_seed_loops = sorted(set(seed_loops_after) - set(seed_loops_before))
    cleanup_ok = not mount_refs_after and not process_refs_after and not helper_processes_after and not leaked_seed_loops
    scratch_removed = False
    cleanup_error: OSError | None = None
    if cleanup_ok:
        try:
            if native_context_root.exists():
                shutil.rmtree(native_context_root)
            scratch_removed = not native_context_root.exists()
        except OSError as error:
            cleanup_error = error
            cleanup_ok = False

    write_native_cleanup_evidence(
        manifest_file,
        manifest,
        native_context_root,
        namespace_action_succeeded=action_error is None,
        mount_references_before=mount_refs_before,
        mount_references_after=mount_refs_after,
        process_references_before=process_refs_before,
        process_references_after=process_refs_after,
        helper_processes_before=helper_processes_before,
        helper_processes_after=helper_processes_after,
        seed_loop_devices_before=seed_loops_before,
        seed_loop_devices_after=seed_loops_after,
        leaked_seed_loop_devices=leaked_seed_loops,
        scratch_removed=scratch_removed,
        status="pass" if cleanup_ok and scratch_removed else "fail",
    )

    if not cleanup_ok or not scratch_removed:
        detail = "native cleanup verification failed"
        if mount_refs_after:
            detail += f"; mount references={len(mount_refs_after)}"
        if process_refs_after:
            detail += f"; process references={len(process_refs_after)}"
        if helper_processes_after:
            detail += f"; surviving namespace helpers={len(helper_processes_after)}"
        if leaked_seed_loops:
            detail += f"; leaked seed loops={','.join(leaked_seed_loops)}"
        if cleanup_error is not None:
            detail += f"; scratch removal failed: {cleanup_error}"
        if action_error is not None:
            detail += f"; namespace action also failed: {action_error}"
        raise RuntimeError(detail)
    if action_error is not None:
        raise action_error


def build_iso_inner(repo: Path, config: dict[str, Any], manifest_file: Path) -> None:
    make_namespace_private()
    manifest = load_native_manifest(repo, manifest_file)
    resolved = manifest["_resolved"]
    native_config = native_context_config(repo, config, resolved["native_root"])
    snapshot_prepared_upper(repo, config, native_config)
    mount_context(repo, native_config)
    p = paths(repo, native_config)
    buildiso = p["root"] / "usr/bin/buildiso"
    if not buildiso.is_file():
        raise RuntimeError("prepared Artix upper state does not provide /usr/bin/buildiso; run private prepare first")
    verify_artools_unattended_contract(p["root"])
    bind_target = p["root"] / "run/portus-build"
    bind_target.mkdir(parents=True, exist_ok=True)
    run(["mount", "--bind", str(resolved["native_root"]), str(bind_target)])
    run(["mount", "--make-private", str(bind_target)])
    repository_closure = prepare_repository_closure(repo, native_config, manifest, manifest_file)
    artools_compatibility = apply_artools_live_kernel_compatibility(
        p["root"], manifest["artools"]["live_boot_kernel_package"]
    )
    command = [
        "chroot",
        str(p["root"]),
        "/usr/bin/env",
        "-i",
        "HOME=/root",
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "WORKSPACE_DIR=/run/portus-build/workspace",
        "/usr/bin/buildiso",
        "-p",
        "portus",
        "-R",
        "stable",
        "-a",
        "x86_64",
        "-i",
        "openrc",
        "-r",
        "/run/portus-build/chroots",
        "-t",
        "/run/portus-build/iso-out",
    ]
    run_unattended(command)
    artifacts = sorted(path for path in resolved["iso_out"].rglob("*.iso") if path.is_file())
    if len(artifacts) != 1:
        raise RuntimeError(f"native artools build must produce exactly one ISO; found {len(artifacts)}")
    artifact = artifacts[0]
    name = artifact.name
    if not name.startswith("artix-portus-openrc-") or not name.endswith("-x86_64.iso"):
        raise RuntimeError(f"unexpected native ISO filename: {name}")
    destination = resolved["artifact_dir"] / name
    if destination.exists():
        raise RuntimeError(f"refusing to overwrite existing artifact: {destination}")
    shutil.move(str(artifact), destination)
    uid = os.environ.get("SUDO_UID")
    gid = os.environ.get("SUDO_GID")
    if uid and gid:
        os.chown(destination, int(uid), int(gid))
    result_path = manifest_file.parent / "native-build-result.json"
    result_value = {
        "schema_version": 1,
        "run_id": manifest["run_id"],
        "namespace": "private-mount",
        "buildiso_command": command[2:],
        "unattended": True,
        "stdin": "devnull",
        "artools_compatibility": artools_compatibility,
        "repository_closure_sha256": sha256_file(repository_closure_evidence_path(manifest_file)),
        "repository_package_count": len(repository_closure["packages"]),
        "artifact": destination.relative_to(repo).as_posix(),
        "artifact_sha256": sha256_file(destination),
        "artifact_size_bytes": destination.stat().st_size,
    }
    result_path.write_text(json.dumps(result_value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if uid and gid:
        os.chown(result_path, int(uid), int(gid))
    print(json.dumps(result_value, indent=2, sort_keys=True))


def path_is_at_or_below(candidate: str, root: Path) -> bool:
    root_text = str(root)
    return candidate == root_text or candidate.startswith(root_text + os.sep)


def process_references(path: Path) -> list[str]:
    """Return process root/cwd/fd references into an isolated mount path."""
    references: list[str] = []
    proc = Path("/proc")
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            comm = (entry / "comm").read_text(encoding="utf-8").strip()
        except OSError:
            comm = "?"
        for field in ("cwd", "root"):
            try:
                target = os.readlink(entry / field)
            except OSError:
                continue
            if path_is_at_or_below(target, path):
                references.append(f"pid={entry.name} comm={comm} {field}={target}")
        fd_dir = entry / "fd"
        try:
            descriptors = list(fd_dir.iterdir())
        except OSError:
            continue
        for descriptor in descriptors:
            try:
                target = os.readlink(descriptor)
            except OSError:
                continue
            if path_is_at_or_below(target, path):
                references.append(f"pid={entry.name} comm={comm} fd={descriptor.name} target={target}")
    return references


def umount_recursive(path: Path) -> None:
    if not path.exists() or not is_mountpoint(path):
        return
    result = run(["umount", "-R", str(path)], capture=True, check=False)
    if result.returncode == 0:
        return
    detail = (result.stderr or result.stdout).strip()
    if "target is busy" not in detail.lower():
        raise RuntimeError(f"failed to unmount isolated path {path}: {detail or f'exit {result.returncode}'}")

    references = process_references(path)
    for _ in range(3):
        if not references:
            break
        time.sleep(0.25)
        retry = run(["umount", "-R", str(path)], capture=True, check=False)
        if retry.returncode == 0:
            return
        retry_detail = (retry.stderr or retry.stdout).strip()
        if "target is busy" not in retry_detail.lower():
            raise RuntimeError(f"failed to unmount isolated path {path}: {retry_detail or f'exit {retry.returncode}'}")
        references = process_references(path)

    if references:
        preview = "; ".join(references[:8])
        if len(references) > 8:
            preview += f"; ... {len(references) - 8} more"
        raise RuntimeError(f"refusing lazy detach for busy isolated path {path}; active process references: {preview}")

    # Children were already handled by the ordinary recursive attempt and by the
    # teardown ordering. Detach only this proven-unused mountpoint here; using
    # recursive lazy mode can leave the parent/child pair visible on some
    # util-linux/libmount combinations after a partial recursive teardown.
    fallback = run(["umount", "-l", str(path)], capture=True, check=False)
    if fallback.returncode != 0 or is_mountpoint(path):
        fallback_detail = (fallback.stderr or fallback.stdout).strip()
        raise RuntimeError(
            f"failed to lazily detach proven-unused isolated path {path}: "
            f"{fallback_detail or f'exit {fallback.returncode}'}"
        )
    print(
        f"warning: lazily detached busy isolated mount {path} after proving no process root/cwd/fd references",
        file=sys.stderr,
    )


def teardown(repo: Path, config: dict[str, Any]) -> None:
    require_root("Artix context teardown")
    p = paths(repo, config)
    repo_target = p["root"] / config["context"]["repo_mount"].lstrip("/")
    errors: list[str] = []
    for target in (repo_target, p["root"] / "proc", p["root"] / "dev", p["root"] / "sys", p["root"], p["lower"]):
        try:
            umount_recursive(target)
        except RuntimeError as error:
            errors.append(str(error))
    if errors:
        raise RuntimeError("Artix context teardown incomplete: " + " | ".join(errors))
    print("Artix context unmounted; upper/work state retained for reproducible reuse")


def self_test() -> int:
    repo = repo_root()
    config = load_config(repo)
    assert config["seed"]["sha256"] == "6f75cf428b6b2df2d387eceff0d08fb0d93eeff39ec30e0957fbca7851bdf5c6"
    assert config["seed"]["rootfs"]["offset_bytes"] == 234659840
    assert config["seed"]["rootfs"]["size_bytes"] == 997089280
    sample_facts = "[package-candidates]\npackage\tavailability\trepository\tversion\tarchitecture\tlicenses\tinstalled\nchromium\tavailable\textra\t1.2-3\tx86_64\tBSD\tabsent\n[notes]\n"
    parsed = parse_package_candidates(sample_facts)
    assert parsed["chromium"]["repository"] == "extra" and parsed["chromium"]["version"] == "1.2-3"
    assert paths(repo, config)["work_root"].is_relative_to(repo / "portusos-build/work")
    assert native_helper_processes("definitely-not-a-real-portus-run") == []
    package_targets = parse_artools_package_targets(
        "packages-base:\n  - base\npackages-boot:\n  - grub\n  - memtest86+\n",
        "live-session:\n  services:\n    - dbus\nrootfs:\n  packages:\n    - linux-lts\nlivefs:\n  packages:\n    - calamares\n",
    )
    assert package_targets == ["base", "calamares", "grub", "linux-lts", "memtest86+"]
    closure_rows = parse_pacman_print_rows(
        "system|base|3-6.5|base-3-6.5-any.pkg.tar.zst|" + "a" * 64 + "|1024|any\n"
        "world|calamares|3.4.2-4|calamares-3.4.2-4-x86_64.pkg.tar.zst|" + "b" * 64 + "|2048|x86_64\n"
    )
    assert [row["name"] for row in closure_rows] == ["base", "calamares"]
    require_same_package_closure(closure_rows, copy.deepcopy(closure_rows))
    with tempfile.TemporaryDirectory(prefix="portus-artix-evidence-self-test-") as raw_evidence:
        evidence_root = Path(raw_evidence)
        manifest_file = evidence_root / "staging-evidence.json"
        manifest_file.write_text("{}\n", encoding="utf-8")
        frozen_evidence = {
            "schema_version": 1,
            "run_id": "self-test",
            "status": "fail",
            "packages": [],
            "failure": None,
        }
        freeze_resolved_package_evidence(manifest_file, frozen_evidence, closure_rows)
        closure_rows[0]["name"] = "mutated-after-freeze"
        frozen_evidence["failure"] = "simulated post-resolution acquisition failure"
        write_repository_closure_evidence(manifest_file, frozen_evidence)
        persisted = json.loads(repository_closure_evidence_path(manifest_file).read_text(encoding="utf-8"))
        assert [row["name"] for row in persisted["packages"]] == ["base", "calamares"]
        required_identity_fields = {"repository", "name", "version", "filename", "sha256", "size_bytes"}
        assert all(required_identity_fields.issubset(row) for row in persisted["packages"])
        assert persisted["packages"][0]["size_bytes"] == 1024
        assert persisted["failure"] == "simulated post-resolution acquisition failure"
        assert persisted["progress_summary"]["display"] == "2 resolved / 0 verified / 2 pending"
        assert persisted["progress_summary"]["resolved"] == {"packages": 2, "bytes": 3072}
        assert [record["state"] for record in persisted["package_progress"]] == ["pending", "pending"]
    closure_rows[0]["name"] = "base"
    drifted_rows = copy.deepcopy(closure_rows)
    drifted_rows[1]["version"] = "3.4.3-1"
    try:
        require_same_package_closure(closure_rows, drifted_rows)
    except RuntimeError:
        pass
    else:
        raise AssertionError("repository/package identity drift must fail closed")
    with tempfile.TemporaryDirectory(prefix="portus-artix-cache-self-test-") as raw_cache:
        cache = Path(raw_cache)
        fixture_bytes = b"verified package cache fixture"
        fixture = cache / "fixture-1-any.pkg.tar.zst"
        fixture_row = {
            "filename": fixture.name,
            "sha256": hashlib.sha256(fixture_bytes).hexdigest(),
        }
        try:
            verify_cached_package_files(cache, [fixture_row])
        except RuntimeError:
            pass
        else:
            raise AssertionError("missing package cache entry must fail closed")
        fixture.write_bytes(fixture_bytes)
        verify_cached_package_files(cache, [fixture_row])
        fixture.write_bytes(b"corrupt")
        try:
            verify_cached_package_files(cache, [fixture_row])
        except RuntimeError:
            pass
        else:
            raise AssertionError("corrupt package cache entry must fail closed")
    with tempfile.TemporaryDirectory(prefix="portus-artix-cache-audit-self-test-") as raw_cache_audit:
        cache_audit_root = Path(raw_cache_audit)
        reused_bytes = b"reused-package"
        corrupt_bytes = b"expected-corrupt-package"
        missing_bytes = b"missing-package"
        audit_packages = [
            {
                "repository": "system",
                "name": "reused",
                "version": "1-1",
                "filename": "reused-1-1-any.pkg.tar.zst",
                "sha256": hashlib.sha256(reused_bytes).hexdigest(),
                "size_bytes": len(reused_bytes),
            },
            {
                "repository": "world",
                "name": "corrupt",
                "version": "1-1",
                "filename": "corrupt-1-1-any.pkg.tar.zst",
                "sha256": hashlib.sha256(corrupt_bytes).hexdigest(),
                "size_bytes": len(corrupt_bytes),
            },
            {
                "repository": "galaxy",
                "name": "missing",
                "version": "1-1",
                "filename": "missing-1-1-any.pkg.tar.zst",
                "sha256": hashlib.sha256(missing_bytes).hexdigest(),
                "size_bytes": len(missing_bytes),
            },
        ]
        (cache_audit_root / audit_packages[0]["filename"]).write_bytes(reused_bytes)
        (cache_audit_root / audit_packages[1]["filename"]).write_bytes(b"wrong-bytes")
        (cache_audit_root / f"{audit_packages[1]['filename']}.sig").write_bytes(b"stale-signature")
        (cache_audit_root / f"{audit_packages[2]['filename']}.part").write_bytes(b"stale-partial")
        (cache_audit_root / f"{audit_packages[2]['filename']}.sig.part").write_bytes(b"stale-signature-partial")
        cache_audit = audit_persistent_package_cache(cache_audit_root, audit_packages)
        assert cache_audit["status"] == "pass"
        assert cache_audit["resolved_count"] == 3
        assert cache_audit["reused_count"] == 1
        assert cache_audit["reused_filenames"] == [audit_packages[0]["filename"]]
        assert cache_audit["pending_count"] == 2
        assert cache_audit["pending_filenames"] == [
            audit_packages[1]["filename"],
            audit_packages[2]["filename"],
        ]
        assert cache_audit["corrupt_entries_removed"] == [
            audit_packages[1]["filename"],
            f"{audit_packages[1]['filename']}.sig",
        ]
        assert cache_audit["stale_partial_entries_removed"] == [
            f"{audit_packages[2]['filename']}.part",
            f"{audit_packages[2]['filename']}.sig.part",
        ]
        assert (cache_audit_root / audit_packages[0]["filename"]).is_file()
        assert not (cache_audit_root / audit_packages[1]["filename"]).exists()
        assert not (cache_audit_root / f"{audit_packages[1]['filename']}.sig").exists()
        assert not (cache_audit_root / f"{audit_packages[2]['filename']}.part").exists()
        audit_progress = initialize_package_progress(audit_packages)
        reconcile_package_progress_from_cache_audit(audit_progress, cache_audit)
        audit_summary = summarize_package_progress(audit_progress)
        audit_states = {record["name"]: record["state"] for record in audit_progress}
        assert audit_states == {
            "reused": "reused_verified",
            "corrupt": "corrupt_removed",
            "missing": "pending",
        }
        assert audit_summary["display"] == "3 resolved / 1 verified / 2 pending"
        assert audit_summary["verified"]["bytes"] == len(reused_bytes)
        assert audit_summary["pending"]["bytes"] == len(corrupt_bytes) + len(missing_bytes)
        simulated_attempt = {
            "attempt": 1,
            "mirror": {"server": "https://mirror.example/artix/$repo/os/$arch", "mirrorlist_line": 1},
            "verified_filenames": [audit_packages[1]["filename"]],
            "pending_filenames": [audit_packages[2]["filename"]],
            "result": "fail",
            "failure_class": "timeout",
        }
        record_package_progress_attempt(audit_progress, 1, simulated_attempt)
        attempt_states = {record["name"]: record for record in audit_progress}
        assert attempt_states["corrupt"]["state"] == "downloaded_verified"
        assert attempt_states["missing"]["state"] == "failed"
        assert attempt_states["missing"]["failure_class"] == "timeout"
        attempt_summary = summarize_package_progress(audit_progress)
        assert attempt_summary["display"] == "3 resolved / 2 verified / 1 pending"

        duplicate_audit_packages = [audit_packages[0], copy.deepcopy(audit_packages[0])]
        try:
            audit_persistent_package_cache(cache_audit_root, duplicate_audit_packages)
        except RuntimeError as error:
            assert "duplicate package filename" in str(error)
        else:
            raise AssertionError("duplicate frozen package filenames must fail cache audit closed")

    with tempfile.TemporaryDirectory(prefix="portus-artix-cache-audit-dir-self-test-") as raw_cache_dir:
        cache_dir_root = Path(raw_cache_dir)
        directory_package = {
            "repository": "system",
            "name": "directory",
            "version": "1-1",
            "filename": "directory-1-1-any.pkg.tar.zst",
            "sha256": "0" * 64,
            "size_bytes": 1,
        }
        (cache_dir_root / directory_package["filename"]).mkdir()
        try:
            audit_persistent_package_cache(cache_dir_root, [directory_package])
        except RuntimeError as error:
            assert "unexpected directory or special entry" in str(error)
        else:
            raise AssertionError("unexpected directory in package cache must fail audit closed")

    batch_fixture = [
        {
            "repository": "world",
            "name": "delta",
            "version": "1-1",
            "filename": "delta.pkg.tar.zst",
            "sha256": "d" * 64,
            "size_bytes": 60,
        },
        {
            "repository": "system",
            "name": "alpha",
            "version": "1-1",
            "filename": "alpha.pkg.tar.zst",
            "sha256": "a" * 64,
            "size_bytes": 40,
        },
        {
            "repository": "world",
            "name": "charlie",
            "version": "1-1",
            "filename": "charlie.pkg.tar.zst",
            "sha256": "c" * 64,
            "size_bytes": 80,
        },
        {
            "repository": "system",
            "name": "bravo",
            "version": "1-1",
            "filename": "bravo.pkg.tar.zst",
            "sha256": "b" * 64,
            "size_bytes": 250,
        },
    ]
    batch_plan = plan_package_prefetch_batches(batch_fixture, max_bytes=100)
    assert [[entry["name"] for entry in batch] for batch in batch_plan] == [
        ["alpha"],
        ["bravo"],
        ["charlie"],
        ["delta"],
    ]
    assert sum(entry["size_bytes"] for entry in batch_plan[1]) == 250
    assert all(
        sum(entry["size_bytes"] for entry in batch) <= 100 or len(batch) == 1
        for batch in batch_plan
    )
    compact_batch_plan = plan_package_prefetch_batches(
        [
            {**batch_fixture[0], "repository": "system", "name": "a", "size_bytes": 40},
            {**batch_fixture[1], "repository": "system", "name": "b", "size_bytes": 50},
            {**batch_fixture[2], "repository": "system", "name": "c", "size_bytes": 20},
        ],
        max_bytes=100,
    )
    assert [[entry["name"] for entry in batch] for batch in compact_batch_plan] == [["a", "b"], ["c"]]
    assert exact_package_sync_targets(compact_batch_plan[0]) == ["system/a", "system/b"]
    timeout_error = subprocess.CalledProcessError(
        1,
        ["pacman"],
        stderr="Operation too slow. Less than 1 bytes/sec transferred the last 10 seconds",
    )
    assert classify_package_acquisition_failure(timeout_error, {"a"}, concise_process_failure(timeout_error)) == "timeout"
    tls_error = subprocess.CalledProcessError(1, ["pacman"], stderr="OpenSSL SSL_read: unexpected eof while reading")
    assert classify_package_acquisition_failure(tls_error, {"a"}, concise_process_failure(tls_error)) == "tls"
    not_found_error = subprocess.CalledProcessError(1, ["pacman"], stderr="failed retrieving file: 404 Not Found")
    assert classify_package_acquisition_failure(not_found_error, {"a"}, concise_process_failure(not_found_error)) == "http_not_found"
    assert classify_package_acquisition_failure(None, {"a"}, None) == "verification_incomplete"
    assert classify_package_acquisition_failure(None, set(), None) is None
    prefetch_command_fixture = prefetch_pacman_command(
        "/run/prefetch.conf",
        "/run/pacman-db",
        "/var/cache/pacman/pkg",
        compact_batch_plan[0],
    )
    assert prefetch_command_fixture[:2] == ["/usr/bin/pacman", "-Sw"]
    assert prefetch_command_fixture.count("--nodeps") == 2
    assert not any(argument in {"-y", "-Sy", "-Syy", "--refresh"} for argument in prefetch_command_fixture)
    assert prefetch_command_fixture[-2:] == ["system/a", "system/b"]
    try:
        prefetch_pacman_command("/run/prefetch.conf", "/run/pacman-db", "/cache", [])
    except ValueError:
        pass
    else:
        raise AssertionError("empty exact-package prefetch command must fail closed")
    try:
        plan_package_prefetch_batches(batch_fixture, max_bytes=0)
    except ValueError:
        pass
    else:
        raise AssertionError("non-positive prefetch byte limit must fail closed")

    with tempfile.TemporaryDirectory(prefix="portus-artix-batch-acquire-self-test-") as raw_batch_cache:
        batch_cache = Path(raw_batch_cache)
        alpha_bytes = b"alpha-package"
        beta_bytes = b"beta-package"
        acquire_packages = [
            {
                "repository": "system",
                "name": "alpha",
                "version": "1-1",
                "filename": "alpha.pkg.tar.zst",
                "sha256": hashlib.sha256(alpha_bytes).hexdigest(),
                "size_bytes": len(alpha_bytes),
            },
            {
                "repository": "system",
                "name": "beta",
                "version": "1-1",
                "filename": "beta.pkg.tar.zst",
                "sha256": hashlib.sha256(beta_bytes).hexdigest(),
                "size_bytes": len(beta_bytes),
            },
        ]
        acquire_batches = plan_package_prefetch_batches(acquire_packages, max_bytes=1024)
        acquire_progress: list[tuple[int, int, bool, int, int]] = []

        def successful_fixture_fetch(_index: int, batch: list[dict[str, Any]]) -> None:
            for package in batch:
                payload = alpha_bytes if package["name"] == "alpha" else beta_bytes
                (batch_cache / package["filename"]).write_bytes(payload)

        def record_fixture_progress(
            index: int,
            count: int,
            _batch: list[dict[str, Any]],
            verified_batch: set[str],
            pending: set[str],
            _error: BaseException | None,
            complete: bool,
        ) -> None:
            acquire_progress.append((index, count, complete, len(verified_batch), len(pending)))

        acquired = acquire_prefetch_batches(
            batch_cache,
            acquire_batches,
            successful_fixture_fetch,
            record_fixture_progress,
        )
        assert acquired == {"alpha.pkg.tar.zst", "beta.pkg.tar.zst"}
        assert acquire_progress == [(1, 1, True, 2, 0)]

        for path in batch_cache.iterdir():
            path.unlink()
        failed_progress: list[tuple[bool, int, int]] = []

        def partial_fixture_fetch(_index: int, batch: list[dict[str, Any]]) -> None:
            first = batch[0]
            (batch_cache / first["filename"]).write_bytes(alpha_bytes)
            raise RuntimeError("simulated network failure")

        def record_failed_progress(
            _index: int,
            _count: int,
            _batch: list[dict[str, Any]],
            verified_batch: set[str],
            pending: set[str],
            _error: BaseException | None,
            complete: bool,
        ) -> None:
            failed_progress.append((complete, len(verified_batch), len(pending)))

        try:
            acquire_prefetch_batches(
                batch_cache,
                acquire_batches,
                partial_fixture_fetch,
                record_failed_progress,
            )
        except RuntimeError as error:
            assert "1/2 batch package files verified before failure" in str(error)
        else:
            raise AssertionError("failed prefetch batch must fail closed")
        assert failed_progress == [(False, 1, 1)]
        assert verified_cached_filenames(batch_cache, acquire_packages) == {"alpha.pkg.tar.zst"}

        for path in tuple(batch_cache.iterdir()):
            path.unlink()
        failover_mirrors = [
            {"server": "https://anchor.example/artix/$repo/os/$arch", "mirrorlist_line": 1},
            {"server": "https://fallback.example/artix/$repo/os/$arch", "mirrorlist_line": 2},
            {"server": "https://unused.example/artix/$repo/os/$arch", "mirrorlist_line": 3},
        ]
        failover_calls: list[tuple[int, str, list[str]]] = []
        failover_progress: list[dict[str, Any]] = []

        def failover_fixture_fetch(
            attempt: int,
            mirror: dict[str, Any],
            attempt_packages: list[dict[str, Any]],
        ) -> None:
            names = [package["name"] for package in attempt_packages]
            failover_calls.append((attempt, mirror["server"], names))
            if attempt == 1:
                (batch_cache / acquire_packages[0]["filename"]).write_bytes(alpha_bytes)
                (batch_cache / acquire_packages[1]["filename"]).write_bytes(b"corrupt-beta")
                (batch_cache / f"{acquire_packages[1]['filename']}.part").write_bytes(b"partial-beta")
                raise RuntimeError("simulated anchor transfer failure")
            assert names == ["beta"]
            (batch_cache / acquire_packages[1]["filename"]).write_bytes(beta_bytes)

        failover_verified, failover_attempts = acquire_batch_with_mirror_failover(
            batch_cache,
            acquire_packages,
            failover_mirrors,
            failover_fixture_fetch,
            lambda record, _verified, _pending: failover_progress.append(record),
        )
        assert failover_verified == {"alpha.pkg.tar.zst", "beta.pkg.tar.zst"}
        assert len(failover_attempts) == 2
        assert [call[2] for call in failover_calls] == [["alpha", "beta"], ["beta"]]
        assert failover_attempts[0]["result"] == "fail"
        assert failover_attempts[0]["failure_class"] == "runtime_error"
        assert failover_attempts[0]["requested_filenames"] == ["alpha.pkg.tar.zst", "beta.pkg.tar.zst"]
        assert failover_attempts[0]["verified_filenames"] == ["alpha.pkg.tar.zst"]
        assert failover_attempts[0]["pending_filenames"] == ["beta.pkg.tar.zst"]
        assert failover_attempts[0]["requested_bytes"] == len(alpha_bytes) + len(beta_bytes)
        assert failover_attempts[0]["verified_bytes"] == len(alpha_bytes)
        assert failover_attempts[0]["pending_bytes"] == len(beta_bytes)
        assert "beta.pkg.tar.zst" in failover_attempts[0]["removed_unverified"]
        assert "beta.pkg.tar.zst.part" in failover_attempts[0]["removed_unverified"]
        assert failover_attempts[1]["result"] == "pass"
        assert len(failover_progress) == 2
        failover_package_progress = initialize_package_progress(acquire_packages)
        for attempt_record in failover_attempts:
            record_package_progress_attempt(failover_package_progress, 1, attempt_record)
        failover_summary = summarize_package_progress(failover_package_progress)
        assert failover_summary["display"] == "2 resolved / 2 verified / 0 pending"
        assert all(record["state"] == "downloaded_verified" for record in failover_package_progress)
        verify_cached_package_files(batch_cache, acquire_packages)

        for path in tuple(batch_cache.iterdir()):
            path.unlink()
        exhausted_calls: list[str] = []

        def exhausted_fixture_fetch(
            _attempt: int,
            mirror: dict[str, Any],
            _attempt_packages: list[dict[str, Any]],
        ) -> None:
            exhausted_calls.append(mirror["server"])
            raise RuntimeError("simulated mirror failure")

        try:
            acquire_batch_with_mirror_failover(
                batch_cache,
                acquire_packages,
                failover_mirrors[:2],
                exhausted_fixture_fetch,
            )
        except RuntimeError as error:
            assert "exhausted 2/2 mirror attempts" in str(error)
        else:
            raise AssertionError("exhausted package mirrors must fail closed")
        assert exhausted_calls == [mirror["server"] for mirror in failover_mirrors[:2]]

        for path in tuple(batch_cache.iterdir()):
            path.unlink()

        def nonzero_complete_fixture_fetch(
            _attempt: int,
            _mirror: dict[str, Any],
            attempt_packages: list[dict[str, Any]],
        ) -> None:
            for package in attempt_packages:
                payload = alpha_bytes if package["name"] == "alpha" else beta_bytes
                (batch_cache / package["filename"]).write_bytes(payload)
            raise RuntimeError("simulated pacman nonzero after complete bytes")

        clean_retry_calls: list[list[str]] = []

        def nonzero_then_clean_fixture_fetch(
            attempt: int,
            _mirror: dict[str, Any],
            attempt_packages: list[dict[str, Any]],
        ) -> None:
            clean_retry_calls.append([package["name"] for package in attempt_packages])
            if attempt == 1:
                nonzero_complete_fixture_fetch(attempt, _mirror, attempt_packages)

        retried_verified, retried_attempts = acquire_batch_with_mirror_failover(
            batch_cache,
            acquire_packages,
            failover_mirrors[:2],
            nonzero_then_clean_fixture_fetch,
        )
        assert retried_verified == {"alpha.pkg.tar.zst", "beta.pkg.tar.zst"}
        assert clean_retry_calls == [["alpha", "beta"], ["alpha", "beta"]]
        assert [attempt["result"] for attempt in retried_attempts] == ["fail", "pass"]

        for path in tuple(batch_cache.iterdir()):
            path.unlink()
        try:
            acquire_batch_with_mirror_failover(
                batch_cache,
                acquire_packages,
                failover_mirrors[:1],
                nonzero_complete_fixture_fetch,
            )
        except RuntimeError as error:
            assert "pacman never completed a clean transaction" in str(error)
        else:
            raise AssertionError("nonzero pacman attempt must not become PASS from SHA evidence alone")

    mirror_fixture = (
        "# disabled\n"
        "#Server = https://disabled.example/$repo/os/$arch\n"
        "Server = http://legacy.example/$repo/os/$arch\n"
        "Server = https://first.example/artix/$repo/os/$arch\n"
        "Server = https://second.example/artix/$repo/os/$arch\n"
        "Server = https://first.example/artix/$repo/os/$arch\n"
    )
    mirror_candidates = parse_artix_mirror_servers(mirror_fixture)
    assert mirror_candidates == [
        {"server": "https://first.example/artix/$repo/os/$arch", "mirrorlist_line": 4},
        {"server": "https://second.example/artix/$repo/os/$arch", "mirrorlist_line": 5},
    ]
    ordered_mirrors = ordered_prefetch_mirrors(
        mirror_candidates,
        mirror_candidates[1],
        max_attempts=2,
    )
    assert [mirror["server"] for mirror in ordered_mirrors] == [
        "https://second.example/artix/$repo/os/$arch",
        "https://first.example/artix/$repo/os/$arch",
    ]
    try:
        ordered_prefetch_mirrors(mirror_candidates, mirror_candidates[0], max_attempts=0)
    except ValueError:
        pass
    else:
        raise AssertionError("non-positive mirror failover limit must fail closed")
    try:
        ordered_prefetch_mirrors(
            mirror_candidates,
            {"server": "https://missing.example/artix/$repo/os/$arch"},
            max_attempts=2,
        )
    except RuntimeError:
        pass
    else:
        raise AssertionError("selected anchor absent from mirrorlist candidates must fail closed")
    selected_mirror, mirror_attempts = select_first_healthy_mirror(
        mirror_candidates,
        lambda candidate: (
            True,
            None,
        )
        if candidate["server"].startswith("https://second.")
        else (False, "simulated unhealthy"),
    )
    assert selected_mirror == mirror_candidates[1]
    assert [attempt["result"] for attempt in mirror_attempts] == ["fail", "pass"]
    assert mirror_attempts[1]["detail"] is None
    no_mirror, failed_mirror_attempts = select_first_healthy_mirror(
        mirror_candidates,
        lambda candidate: (False, f"simulated failure for {candidate['server']}"),
    )
    assert no_mirror is None
    assert len(failed_mirror_attempts) == len(mirror_candidates)
    try:
        parse_artix_mirror_servers("Server = https://invalid.example/artix/system/os/x86_64\n")
    except RuntimeError:
        pass
    else:
        raise AssertionError("active HTTPS mirror without $repo/$arch placeholders must fail closed")
    anchor_fixture = render_anchor_pacman_config(
        "[options]\nSigLevel = Required DatabaseOptional\n\n[system]\nInclude = /etc/pacman.d/mirrorlist\n\n[world]\nInclude = /etc/pacman.d/mirrorlist\n\n[galaxy]\nInclude = /etc/pacman.d/mirrorlist\n",
        "https://anchor.example/artix/$repo/os/$arch",
    )
    assert "Include = /etc/pacman.d/mirrorlist" not in anchor_fixture
    assert anchor_fixture.count("Server = https://anchor.example/artix/$repo/os/$arch") == 3
    frozen_fixture = render_frozen_pacman_config(
        "[options]\nSigLevel = Required DatabaseOptional\n\n[system]\nInclude = /etc/pacman.d/mirrorlist\n\n[world]\nInclude = /etc/pacman.d/mirrorlist\n\n[galaxy]\nInclude = /etc/pacman.d/mirrorlist\n",
        "file:///run/portus-build/repository-closure/repo",
    )
    assert "Include = /etc/pacman.d/mirrorlist" not in frozen_fixture
    assert frozen_fixture.count("Server = file:///run/portus-build/repository-closure/repo") == 3
    buildiso_fixture = (
        'basestrap_args=(-GMc)\n'
        'basestrap_args+=(-C "${pacman_conf}")\n'
        'basestrap "${basestrap_args[@]}" "${rootfs}"\n'
    )
    validate_artools_unattended_contract(
        buildiso_fixture,
        'i) interactive=1 ;;\nif (( ! interactive )); then\n  pacman_args+=(--noconfirm)\nfi\n',
    )
    patched_fixture = patch_artools_buildiso_text(
        "prefix\n" + ARTOOLS_SINGLE_KERNEL_BOOT_BLOCK + "suffix\n", "linux-lts"
    )
    assert "local live_kernel_pkg='linux-lts'" in patched_fixture
    assert 'cp "${live_kernel_image}" "${iso_root}/boot/vmlinuz-${arch}"' in patched_fixture
    assert 'rootfs/boot/vmlinuz* "${iso_root}"/boot/vmlinuz-' not in patched_fixture
    assert 'pkgbase in "${bootfs}"/usr/lib/modules/*/pkgbase' in patched_fixture
    assert '> "${bootfs}/usr/src/linux/version"' in patched_fixture
    try:
        patch_artools_buildiso_text("upstream changed", "linux-lts")
    except RuntimeError:
        pass
    else:
        raise AssertionError("changed artools make_bootfs contract must fail closed")
    try:
        patch_artools_buildiso_text("prefix\n" + ARTOOLS_SINGLE_KERNEL_BOOT_BLOCK + "suffix\n", "linux")
    except RuntimeError:
        pass
    else:
        raise AssertionError("alternate kernel must not become the live default")
    try:
        validate_artools_unattended_contract(
            'basestrap_args=(-GMci)\nbasestrap_args+=(-C "${pacman_conf}")\nbasestrap "${basestrap_args[@]}" "${rootfs}"\n',
            'i) interactive=1 ;;\nif (( ! interactive )); then\n  pacman_args+=(--noconfirm)\nfi\n',
        )
    except RuntimeError:
        pass
    else:
        raise AssertionError("interactive basestrap mode must be rejected")
    try:
        validate_artools_unattended_contract(
            'basestrap_args=(-GM)\nbasestrap_args+=(-C "${pacman_conf}")\nbasestrap "${basestrap_args[@]}" "${rootfs}"\n',
            'i) interactive=1 ;;\nif (( ! interactive )); then\n  pacman_args+=(--noconfirm)\nfi\n',
        )
    except RuntimeError:
        pass
    else:
        raise AssertionError("artools must keep basestrap host-cache sharing for repository closure")
    sample_native_root = repo / "portusos-build/work/native-runs/self-test"
    derived = native_context_config(repo, config, sample_native_root)
    derived_paths = paths(repo, derived)
    assert derived_paths["work_root"] == sample_native_root / "artix-context"
    assert derived_paths["upper"] != paths(repo, config)["upper"]
    assert derived_paths["overlay_work"] != paths(repo, config)["overlay_work"]
    try:
        native_context_config(repo, config, repo / "outside")
    except ValueError:
        pass
    else:
        raise AssertionError("native Artix context must remain below portusos-build/work")
    sample_root = Path("/tmp/portus-artix/root/dev")
    assert path_is_at_or_below("/tmp/portus-artix/root/dev", sample_root)
    assert path_is_at_or_below("/tmp/portus-artix/root/dev/null", sample_root)
    assert not path_is_at_or_below("/dev/null", sample_root)
    assert not path_is_at_or_below("/tmp/portus-artix/root/device", sample_root)
    command = private_namespace_command(repo, "_prepare-inner")
    assert "--mount" in command and "--fork" in command
    try:
        manifest_path(repo, "../escape", "portusos-build/work", "fixture")
    except ValueError:
        pass
    else:
        raise AssertionError("native manifest path traversal must be rejected")
    print("artix context self-test: PASS")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Manage the verified PortusOS isolated Artix build context")
    parser.add_argument(
        "action",
        choices=[
            "verify-seed",
            "status",
            "inspect-artools",
            "inspect-packages",
            "inspect-archives",
            "inspect-browser",
            "prepare",
            "teardown",
            "build-iso",
            "_prepare-inner",
            "_build-iso-inner",
            "self-test",
        ],
    )
    parser.add_argument("packages", nargs="*", help="package names for inspect-packages")
    parser.add_argument("--manifest", help="run-owned staging manifest for build-iso")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = repo_root()
    try:
        config = load_config(repo)
        if args.action == "self-test":
            return self_test()
        if args.action == "verify-seed":
            require_linux_x86_64()
            verify_seed(repo, config)
            return 0
        if args.action == "status":
            return status(repo, config)
        if args.action == "inspect-artools":
            return inspect_artools(repo, config)
        if args.action == "inspect-packages":
            if not args.packages:
                raise ValueError("inspect-packages requires at least one package name")
            return inspect_packages(repo, config, args.packages)
        if args.action == "inspect-archives":
            if not args.packages:
                raise ValueError("inspect-archives requires at least one archive basename")
            return inspect_archives(repo, args.packages)
        if args.action == "inspect-browser":
            return inspect_browser_archive(repo)
        if args.action == "prepare":
            prepare(repo, config)
            return 0
        if args.action == "_prepare-inner":
            prepare_inner(repo, config)
            return 0
        if args.action == "teardown":
            teardown(repo, config)
            return 0
        if args.action in {"build-iso", "_build-iso-inner"}:
            if not args.manifest:
                raise ValueError(f"{args.action} requires --manifest")
            manifest_file = Path(args.manifest)
            if args.action == "build-iso":
                build_iso(repo, config, manifest_file)
            else:
                build_iso_inner(repo, config, manifest_file)
            return 0
    except PermissionError as error:
        print(error, file=sys.stderr)
        print(f"Run the bounded privileged action explicitly: sudo python -B scripts/artix/context.py {args.action}", file=sys.stderr)
        return EX_UNAVAILABLE
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"Artix context {args.action} failed: {error}", file=sys.stderr)
        return EX_UNAVAILABLE
    return EX_USAGE


if __name__ == "__main__":
    raise SystemExit(main())
