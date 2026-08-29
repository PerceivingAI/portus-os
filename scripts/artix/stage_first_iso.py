#!/usr/bin/env python3
"""Materialize the unprivileged first-ISO payload for the native Artix builder.

This program performs no mount/chroot/root operation. It builds and verifies the
first-party/external payloads, materializes a run-owned artools workspace, and
emits the only manifest accepted by the privileged native namespace helper.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path
from typing import Any, Iterable

SCHEMA_VERSION = 1
CODEX_VERSION = "0.150.1"
CODEX_TAG = "rust-v0.150.1"
CODEX_TARGET = "x86_64-unknown-linux-musl"
CODEX_PACKAGE = "codex-package-x86_64-unknown-linux-musl.tar.gz"
CODEX_PACKAGE_SHA256 = "00aba704f029f6dc0d948be407a756e0c97cc840132fd691353b2c6b0a505b17"
CODEX_SUMS = "codex-package_SHA256SUMS"
CODEX_SUMS_SHA256 = "5580070dd9e1c06a603421111f32aa107fd77de2ab306986c11a26166b78e6fa"
CODEX_PACKAGE_ROOT = f"usr/lib/codex/standalone/releases/{CODEX_VERSION}-{CODEX_TARGET}"
CODEX_SYMLINK_TARGET = f"../../lib/codex/standalone/releases/{CODEX_VERSION}-{CODEX_TARGET}/bin/codex"
PORTUS_BROWSER_REVISION = "c263c3997b4e6f2f7df5922e062a9e949e22f755"
PORTUS_MCP_REVISION = "a963c56a72ca106cbac3b848256bb393fb9dc2da"
TUNNEL_VERSION = "0.0.13"
TUNNEL_ASSET = "tunnel-client-v0.0.13-linux-amd64.zip"
TUNNEL_SHA256 = "e71f37b424126513173d5e3590687c0b5ccf6e8ef3fba900104d1f8c60dad906"

PORTUS_PACKAGES = (
    "portus-os",
    "portusd",
    "portus-privd",
    "portus-apid",
    "portus-api",
    "portus-auth",
    "portus-master",
)
PORTUS_BINARIES = (
    "portus-os",
    "portusd",
    "portus-privd",
    "portus-apid",
    "portus-api",
    "portus-auth",
    "portus-bootstrap",
    "portus-master",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def repo_root_from(value: str) -> Path:
    root = Path(value).resolve()
    if not root.is_dir() or not (root / "Cargo.toml").is_file():
        raise ValueError(f"invalid repository root: {root}")
    return root


def relative_under(repo: Path, value: Path, allowed: Iterable[str], label: str) -> str:
    resolved = value.resolve()
    for prefix in allowed:
        base = (repo / prefix).resolve()
        if resolved == base or resolved.is_relative_to(base):
            return resolved.relative_to(repo).as_posix()
    raise ValueError(f"{label} must remain below one of: {', '.join(allowed)}")


def ensure_empty_dir(path: Path, allowed_root: Path) -> None:
    resolved = path.resolve()
    allowed = allowed_root.resolve()
    if resolved == allowed or not resolved.is_relative_to(allowed):
        raise ValueError(f"refusing to reset unbounded generated path: {resolved}")
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True, exist_ok=False)


def copy_contents(source: Path, target: Path) -> None:
    if not source.is_dir():
        raise ValueError(f"source tree is missing: {source}")
    target.mkdir(parents=True, exist_ok=True)
    for item in sorted(source.iterdir(), key=lambda p: p.name.encode("utf-8")):
        destination = target / item.name
        if item.is_symlink():
            if destination.exists() or destination.is_symlink():
                destination.unlink()
            destination.symlink_to(os.readlink(item))
        elif item.is_dir():
            shutil.copytree(item, destination, symlinks=True, dirs_exist_ok=True)
        elif item.is_file():
            shutil.copy2(item, destination)
        else:
            raise ValueError(f"unsupported source entry: {item}")


def write_executable(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")
    path.chmod(0o755)


def stage_installer(repo: Path, profile_target: Path) -> dict[str, Any]:
    module_source = repo / "portusos-build/installer/modules/portus-storage"
    module_required = (
        "module.desc",
        "main.py",
        "storage_engine.py",
        "portus-storage-preflight.conf",
        "portus-storage.conf",
        "portus-storage-finalize.conf",
    )
    config_source = repo / "portusos-build/installer/config"
    config_required = (
        "settings.conf",
        "portus-storage-input.qml",
        "modules/bootloader.conf",
        "modules/portus-storage-input.conf",
        "modules/unpackfs.conf",
        "modules/initcpio.conf",
        "modules/users.conf",
        "modules/services-openrc.conf",
    )
    launcher_source = repo / "portusos-build/installer/live/portus-install"
    polkit_rule_source = repo / "portusos-build/installer/live/90-portus-installer.rules"
    for name in module_required:
        if not (module_source / name).is_file():
            raise ValueError(f"PortusOS Calamares storage source is missing: {name}")
    for name in config_required:
        if not (config_source / name).is_file():
            raise ValueError(f"PortusOS Calamares configuration is missing: {name}")
    if not launcher_source.is_file():
        raise ValueError("PortusOS Calamares live launcher is missing")
    if not polkit_rule_source.is_file():
        raise ValueError("PortusOS Calamares live polkit rule is missing")

    module_target = profile_target / "portus/live-overlay/usr/lib/calamares/modules/portus-storage"
    module_target.mkdir(parents=True, exist_ok=True)
    for name in ("module.desc", "main.py", "storage_engine.py"):
        shutil.copy2(module_source / name, module_target / name)

    config_target = profile_target / "portus/live-overlay/etc/calamares/modules"
    config_target.mkdir(parents=True, exist_ok=True)
    for name in ("portus-storage-preflight.conf", "portus-storage.conf", "portus-storage-finalize.conf"):
        shutil.copy2(module_source / name, config_target / name)
    for name in (
        "bootloader.conf",
        "portus-storage-input.conf",
        "unpackfs.conf",
        "initcpio.conf",
        "users.conf",
        "services-openrc.conf",
    ):
        shutil.copy2(config_source / "modules" / name, config_target / name)

    settings_target = profile_target / "portus/live-overlay/usr/share/calamares/settings.conf"
    settings_target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(config_source / "settings.conf", settings_target)

    input_view_target = profile_target / "portus/live-overlay/usr/share/portus-installer/portus-storage-input.qml"
    input_view_target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(config_source / "portus-storage-input.qml", input_view_target)

    launcher_target = profile_target / "portus/live-overlay/usr/local/bin/portus-install"
    write_executable(launcher_target, launcher_source.read_text(encoding="utf-8"))
    polkit_rule_target = profile_target / "portus/live-overlay/etc/polkit-1/rules.d/90-portus-installer.rules"
    polkit_rule_target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(polkit_rule_source, polkit_rule_target)

    return {
        "module": "portus-storage",
        "input_view": "notesqml@portus-storage-input",
        "live_only": True,
        "files": {
            **{f"module/{name}": sha256_file(module_source / name) for name in module_required},
            **{f"config/{name}": sha256_file(config_source / name) for name in config_required},
            "live/portus-install": sha256_file(launcher_source),
            "live/90-portus-installer.rules": sha256_file(polkit_rule_source),
        },
    }

def sanitized_child_env(repo: Path, home: Path) -> dict[str, str]:
    home.mkdir(parents=True, exist_ok=True)
    tool_dirs: list[str] = []
    for name in ("cargo", "rustc", "git", "npm", "node", "curl"):
        found = shutil.which(name)
        if found:
            parent = str(Path(found).resolve().parent)
            if parent not in tool_dirs:
                tool_dirs.append(parent)
    for parent in ("/usr/local/sbin", "/usr/local/bin", "/usr/sbin", "/usr/bin", "/sbin", "/bin"):
        if parent not in tool_dirs:
            tool_dirs.append(parent)
    original_home = Path.home()
    cargo_home = repo / "portusos-build/cache/cargo-home"
    cargo_home.mkdir(parents=True, exist_ok=True)
    env = {
        "PATH": os.pathsep.join(tool_dirs),
        "HOME": str(home),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "LANG": os.environ.get("LANG", "C.UTF-8"),
        "LC_ALL": "C.UTF-8",
        "CARGO_HOME": str(cargo_home),
        "CARGO_TERM_COLOR": "never",
        "npm_config_cache": str(repo / "portusos-build/cache/npm"),
        "npm_config_update_notifier": "false",
        "npm_config_fund": "false",
        "npm_config_audit": "false",
    }
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
    # The reference builder's HTTPS proxy has produced invalid TLS chains for
    # npm registry tarballs while direct registry TLS verifies successfully.
    # Keep proxy routing for other build traffic, but always fetch npm registry
    # content directly rather than weakening TLS verification.
    no_proxy_values: list[str] = []
    for key in ("NO_PROXY", "no_proxy"):
        for item in os.environ.get(key, "").split(","):
            item = item.strip()
            if item and item not in no_proxy_values:
                no_proxy_values.append(item)
    if "registry.npmjs.org" not in no_proxy_values:
        no_proxy_values.append("registry.npmjs.org")
    no_proxy = ",".join(no_proxy_values)
    env["NO_PROXY"] = no_proxy
    env["no_proxy"] = no_proxy
    return env


def run_checked(command: list[str], *, cwd: Path, env: dict[str, str], label: str) -> None:
    result = subprocess.run(command, cwd=cwd, env=env, text=True, check=False)
    if result.returncode != 0:
        raise RuntimeError(f"{label} failed with exit {result.returncode}")


def capture_checked(command: list[str], *, cwd: Path, env: dict[str, str], label: str) -> str:
    result = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"{label} failed: {detail or f'exit {result.returncode}'}")
    return result.stdout.strip()


def assert_contract_text(repo: Path) -> None:
    checks = {
        "portusos-build/components/codex.yaml": (
            f'version: "{CODEX_VERSION}"',
            f"release_tag: {CODEX_TAG}",
            f"target: {CODEX_TARGET}",
            f"package_asset: {CODEX_PACKAGE}",
            f"value: {CODEX_PACKAGE_SHA256}",
            f"value: {CODEX_SUMS_SHA256}",
        ),
        "portusos-build/components/portus-browser.yaml": (f"value: {PORTUS_BROWSER_REVISION}",),
        "portusos-build/components/portus-mcp.yaml": (f"value: {PORTUS_MCP_REVISION}",),
        "portusos-build/components/tunnel-client.yaml": (
            f'version: "{TUNNEL_VERSION}"',
            f"linux_amd64_asset: {TUNNEL_ASSET}",
            f"value: {TUNNEL_SHA256}",
        ),
    }
    for relative, required in checks.items():
        text = (repo / relative).read_text(encoding="utf-8")
        for needle in required:
            if needle not in text:
                raise ValueError(f"frozen staging identity differs from {relative}: missing {needle!r}")


def download_verified(url: str, target: Path, expected_sha256: str, *, repo: Path, env: dict[str, str]) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_file() and sha256_file(target) == expected_sha256:
        return
    if target.exists():
        target.unlink()
    partial = target.with_name(target.name + ".part")
    if partial.exists():
        partial.unlink()
    run_checked(
        [
            "curl",
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--output",
            str(partial),
            url,
        ],
        cwd=repo,
        env=env,
        label=f"download {target.name}",
    )
    digest = sha256_file(partial)
    if digest != expected_sha256:
        partial.unlink(missing_ok=True)
        raise RuntimeError(f"SHA-256 mismatch for {target.name}: expected {expected_sha256}, found {digest}")
    partial.replace(target)


def safe_archive_name(name: str) -> None:
    path = Path(name)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"unsafe archive path: {name}")


def extract_tar_safe(archive: Path, target: Path) -> None:
    target.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive, "r:gz") as handle:
        for member in handle.getmembers():
            safe_archive_name(member.name)
            if member.issym() or member.islnk():
                safe_archive_name(member.linkname)
        handle.extractall(target, filter="data")


def extract_zip_safe(archive: Path, target: Path) -> None:
    target.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive) as handle:
        for info in handle.infolist():
            safe_archive_name(info.filename)
            mode = (info.external_attr >> 16) & 0o170000
            if mode == stat.S_IFLNK:
                raise ValueError(f"zip symlink is not accepted: {info.filename}")
        handle.extractall(target)


def git_exact_checkout(remote: str, revision: str, cache: Path, *, repo: Path, env: dict[str, str]) -> None:
    if not (cache / ".git").is_dir():
        if cache.exists():
            shutil.rmtree(cache)
        cache.parent.mkdir(parents=True, exist_ok=True)
        run_checked(["git", "clone", "--no-checkout", remote, str(cache)], cwd=repo, env=env, label=f"clone {remote}")
    object_check = subprocess.run(
        ["git", "cat-file", "-e", f"{revision}^{{commit}}"],
        cwd=cache,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if object_check.returncode != 0:
        run_checked(["git", "fetch", "--force", "origin", revision], cwd=cache, env=env, label=f"fetch {revision}")
    run_checked(["git", "checkout", "--detach", "--force", revision], cwd=cache, env=env, label=f"checkout {revision}")
    run_checked(["git", "clean", "-ffdx"], cwd=cache, env=env, label=f"clean {cache.name}")
    head = capture_checked(["git", "rev-parse", "HEAD"], cwd=cache, env=env, label="read source revision")
    if head != revision:
        raise RuntimeError(f"source revision mismatch for {cache}: expected {revision}, found {head}")
    dirty = capture_checked(["git", "status", "--porcelain=v1"], cwd=cache, env=env, label="source cleanliness check")
    if dirty:
        raise RuntimeError(f"source cache is dirty after exact checkout: {cache}")


def build_portus(repo: Path, root_overlay: Path, env: dict[str, str]) -> dict[str, str]:
    command = ["cargo", "build", "--locked", "--release"]
    for package in PORTUS_PACKAGES:
        command.extend(["-p", package])
    run_checked(command, cwd=repo, env=env, label="Portus release build")
    binary_dir = repo / "target/release"
    for binary in PORTUS_BINARIES:
        if not (binary_dir / binary).is_file():
            raise RuntimeError(f"expected release binary is missing: {binary}")
    run_checked(
        [
            "cargo",
            "run",
            "--locked",
            "-q",
            "-p",
            "portus-build",
            "--",
            "stage-portus",
            "--binary-dir",
            str(binary_dir),
            "--target-root",
            str(root_overlay),
        ],
        cwd=repo,
        env=env,
        label="P16 Portus staging",
    )
    return {binary: sha256_file(root_overlay / "usr/bin" / binary) for binary in PORTUS_BINARIES}


def stage_codex(repo: Path, root_overlay: Path, env: dict[str, str], scratch: Path) -> dict[str, Any]:
    cache = repo / f"portusos-build/cache/codex/{CODEX_VERSION}"
    package = cache / CODEX_PACKAGE
    sums = cache / CODEX_SUMS
    download_verified(
        f"https://github.com/openai/codex/releases/download/{CODEX_TAG}/{CODEX_PACKAGE}",
        package,
        CODEX_PACKAGE_SHA256,
        repo=repo,
        env=env,
    )
    download_verified(
        f"https://github.com/openai/codex/releases/download/{CODEX_TAG}/{CODEX_SUMS}",
        sums,
        CODEX_SUMS_SHA256,
        repo=repo,
        env=env,
    )
    sum_text = sums.read_text(encoding="utf-8", errors="strict")
    if CODEX_PACKAGE_SHA256 not in sum_text or CODEX_PACKAGE not in sum_text:
        raise RuntimeError("Codex checksum manifest does not bind the selected package")
    extracted = scratch / "codex-extracted"
    extract_tar_safe(package, extracted)
    candidates = [p for p in extracted.rglob("codex") if p.is_file() and p.parent.name == "bin"]
    if len(candidates) != 1:
        raise RuntimeError(f"Codex package must contain exactly one bin/codex; found {len(candidates)}")
    package_root = candidates[0].parent.parent
    required = [
        package_root / "bin/codex",
        package_root / "bin/codex-code-mode-host",
        package_root / "codex-package.json",
        package_root / "codex-path/rg",
        package_root / "codex-resources/bwrap",
        package_root / "codex-resources/zsh/bin/zsh",
    ]
    missing = [str(item.relative_to(package_root)) for item in required if not item.exists()]
    if missing:
        raise RuntimeError(f"Codex standalone package is incomplete: {', '.join(missing)}")
    target = root_overlay / CODEX_PACKAGE_ROOT
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(package_root, target, symlinks=True)
    visible = root_overlay / "usr/local/bin/codex"
    visible.parent.mkdir(parents=True, exist_ok=True)
    if visible.exists() or visible.is_symlink():
        visible.unlink()
    visible.symlink_to(CODEX_SYMLINK_TARGET)
    return {
        "version": CODEX_VERSION,
        "package_sha256": CODEX_PACKAGE_SHA256,
        "checksum_manifest_sha256": CODEX_SUMS_SHA256,
        "target_root": "/" + CODEX_PACKAGE_ROOT,
        "codex_sha256": sha256_file(target / "bin/codex"),
    }


def npm_pnpm(env: dict[str, str], cwd: Path, args: list[str], label: str) -> None:
    run_checked(
        ["npm", "exec", "--yes", "--package=pnpm@9.15.4", "--", "pnpm", *args],
        cwd=cwd,
        env=env,
        label=label,
    )


def copy_component_payload(source: Path, target: Path) -> None:
    def ignored(_path: str, names: list[str]) -> set[str]:
        return {name for name in names if name in {".git", ".portus-mcp", ".DS_Store"}}

    shutil.copytree(source, target, symlinks=True, ignore=ignored)


def stage_portus_browser(repo: Path, root_overlay: Path, env: dict[str, str]) -> dict[str, Any]:
    cache = repo / "portusos-build/cache/portus-browser-build"
    git_exact_checkout("https://github.com/PerceivingAI/portus-browser.git", PORTUS_BROWSER_REVISION, cache, repo=repo, env=env)
    package = json.loads((cache / "package.json").read_text(encoding="utf-8"))
    if package.get("packageManager") != "pnpm@9.15.4":
        raise RuntimeError("PortusBrowser packageManager differs from pinned pnpm@9.15.4")
    npm_pnpm(env, cache, ["install", "--frozen-lockfile"], "PortusBrowser dependency install")
    npm_pnpm(env, cache, ["-r", "--if-present", "build"], "PortusBrowser build")
    expected = [
        cache / "apps/portus-browser-cli/dist/index.js",
        cache / "apps/portus-broker/bin/portus-broker.js",
        cache / "apps/portus-native-host/bin/portus-native-host.js",
        cache / "apps/portus-extension/manifest.json",
        cache / "installers/dev/dist/index.js",
    ]
    missing = [str(path.relative_to(cache)) for path in expected if not path.is_file()]
    if missing:
        raise RuntimeError(f"PortusBrowser build output is incomplete: {', '.join(missing)}")
    target = root_overlay / "opt/portus/portus-browser"
    target.parent.mkdir(parents=True, exist_ok=True)
    copy_component_payload(cache, target)
    wrappers = {
        "portus-browser": "/opt/portus/portus-browser/apps/portus-browser-cli/dist/index.js",
        "portus-broker": "/opt/portus/portus-browser/apps/portus-broker/bin/portus-broker.js",
        "portus-native-host": "/opt/portus/portus-browser/apps/portus-native-host/bin/portus-native-host.js",
        "portus-browser-dev-install": "/opt/portus/portus-browser/installers/dev/dist/index.js",
    }
    for name, script in wrappers.items():
        write_executable(root_overlay / "usr/bin" / name, f'#!/bin/sh\nexec /usr/bin/node "{script}" "$@"\n')
    return {
        "revision": PORTUS_BROWSER_REVISION,
        "pnpm": "9.15.4",
        "lock_sha256": sha256_file(cache / "pnpm-lock.yaml"),
        "cli_sha256": sha256_file(expected[0]),
        "extension_manifest_sha256": sha256_file(expected[3]),
        "install_root": "/opt/portus/portus-browser",
    }


def stage_portus_mcp(repo: Path, root_overlay: Path, env: dict[str, str]) -> dict[str, Any]:
    cache = repo / "portusos-build/cache/portus-mcp-build"
    git_exact_checkout("https://github.com/PerceivingAI/portus-mcp.git", PORTUS_MCP_REVISION, cache, repo=repo, env=env)
    run_checked(["npm", "ci", "--include=dev"], cwd=cache, env=env, label="Portus MCP dependency install")
    tsx = cache / "node_modules/tsx/dist/cli.mjs"
    if not tsx.is_file():
        raise RuntimeError("Portus MCP dependency payload is incomplete: node_modules/tsx/dist/cli.mjs missing")
    target = root_overlay / "opt/portus/portus-mcp"
    target.parent.mkdir(parents=True, exist_ok=True)
    copy_component_payload(cache, target)
    return {
        "revision": PORTUS_MCP_REVISION,
        "package_lock_sha256": sha256_file(cache / "package-lock.json"),
        "tsx_sha256": sha256_file(tsx),
        "install_root": "/opt/portus/portus-mcp",
    }


def stage_tunnel_client(repo: Path, root_overlay: Path, env: dict[str, str], scratch: Path) -> dict[str, Any]:
    cache = repo / f"portusos-build/cache/tunnel-client/{TUNNEL_VERSION}"
    asset = cache / TUNNEL_ASSET
    download_verified(
        f"https://github.com/openai/tunnel-client/releases/download/v{TUNNEL_VERSION}/{TUNNEL_ASSET}",
        asset,
        TUNNEL_SHA256,
        repo=repo,
        env=env,
    )
    extracted = scratch / "tunnel-client-extracted"
    extract_zip_safe(asset, extracted)
    candidates = [p for p in extracted.rglob("tunnel-client") if p.is_file()]
    if len(candidates) != 1:
        raise RuntimeError(f"tunnel-client archive must contain exactly one executable; found {len(candidates)}")
    target = root_overlay / "usr/local/bin/tunnel-client"
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(candidates[0], target)
    target.chmod(0o755)
    return {
        "version": TUNNEL_VERSION,
        "asset_sha256": TUNNEL_SHA256,
        "binary_sha256": sha256_file(target),
        "target": "/usr/local/bin/tunnel-client",
    }


def materialize(repo: Path, run_id: str, work_root: Path, evidence_path: Path) -> dict[str, Any]:
    assert_contract_text(repo)
    work_parent = repo / "portusos-build/work/native-runs"
    relative_under(repo, work_root, ("portusos-build/work",), "native work root")
    relative_under(repo, evidence_path, ("portusos-build/out",), "staging evidence")
    ensure_empty_dir(work_root, work_parent)
    workspace = work_root / "workspace"
    profile_target = workspace / "iso-profiles"
    chroots = work_root / "chroots"
    iso_out = work_root / "iso-out"
    root_overlay = profile_target / "portus/root-overlay"
    scratch = work_root / "scratch"
    home = work_root / "home"
    for path in (chroots, iso_out, scratch, home):
        path.mkdir(parents=True, exist_ok=True)

    profile_source = repo / "portusos-build/iso/artools-profile/workspace"
    copy_contents(profile_source, profile_target)
    copy_contents(repo / "portusos-build/rootfs/overlay", root_overlay)
    installer = stage_installer(repo, profile_target)

    env = sanitized_child_env(repo, home)
    portus = build_portus(repo, root_overlay, env)
    codex = stage_codex(repo, root_overlay, env, scratch)
    browser = stage_portus_browser(repo, root_overlay, env)
    mcp = stage_portus_mcp(repo, root_overlay, env)
    tunnel = stage_tunnel_client(repo, root_overlay, env, scratch)

    revision = capture_checked(["git", "rev-parse", "HEAD"], cwd=repo, env=env, label="repository revision")
    dirty = capture_checked(["git", "status", "--porcelain=v1"], cwd=repo, env=env, label="repository cleanliness")
    if dirty:
        raise RuntimeError("source tree became dirty during staging")

    artifact_dir = evidence_path.parent / "artifacts"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    manifest = {
        "schema_version": SCHEMA_VERSION,
        "run_id": run_id,
        "source_revision": revision,
        "paths": {
            "native_work_root": relative_under(repo, work_root, ("portusos-build/work",), "native work root"),
            "workspace": relative_under(repo, workspace, ("portusos-build/work",), "workspace"),
            "chroots": relative_under(repo, chroots, ("portusos-build/work",), "chroots"),
            "iso_out": relative_under(repo, iso_out, ("portusos-build/work",), "ISO output"),
            "artifact_dir": relative_under(repo, artifact_dir, ("portusos-build/out",), "artifact directory"),
        },
        "artools": {
            "profile": "portus",
            "stability": "stable",
            "architecture": "x86_64",
            "init": "openrc",
            "workspace_profiles_dir": "iso-profiles",
            "stable_pacman_config": "/usr/share/artools/pacman.conf.d/iso-x86_64.conf",
            "output_filename_prefix": "artix-portus-openrc-",
            "live_boot_kernel_package": "linux-lts",
            "output_filename_suffix": "-x86_64.iso",
        },
        "components": {
            "portus": portus,
            "codex": codex,
            "portus_browser": browser,
            "portus_mcp": mcp,
            "tunnel_client": tunnel,
            "installer": installer,
        },
    }
    evidence_path.parent.mkdir(parents=True, exist_ok=True)
    evidence_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    shutil.rmtree(scratch)
    return manifest


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="portus-stage-self-test-") as raw:
        root = Path(raw)
        repo = root / "repo"
        (repo / "portusos-build/work/native-runs").mkdir(parents=True)
        (repo / "portusos-build/out/builds/run").mkdir(parents=True)
        bounded = repo / "portusos-build/work/native-runs/run"
        assert relative_under(repo, bounded, ("portusos-build/work",), "fixture") == "portusos-build/work/native-runs/run"
        try:
            relative_under(repo, repo / "outside", ("portusos-build/work",), "fixture")
        except ValueError:
            pass
        else:
            raise AssertionError("unbounded path must be rejected")
        env = sanitized_child_env(repo, root / "home")
        for secret_name in ("OPENAI_API_KEY", "GITHUB_TOKEN", "CONTROL_PLANE_API_KEY", "AWS_SECRET_ACCESS_KEY"):
            if secret_name in env:
                raise AssertionError(f"secret-like variable leaked into sanitized environment: {secret_name}")
        for key in ("NO_PROXY", "no_proxy"):
            if "registry.npmjs.org" not in env.get(key, "").split(","):
                raise AssertionError(f"sanitized environment does not bypass the broken npm-registry proxy via {key}")
        archive = root / "bad.tar.gz"
        payload = root / "payload"
        payload.write_text("fixture", encoding="utf-8")
        with tarfile.open(archive, "w:gz") as handle:
            info = tarfile.TarInfo("../escape")
            data = b"fixture"
            info.size = len(data)
            import io
            handle.addfile(info, io.BytesIO(data))
        source_repo = Path(__file__).resolve().parents[2]
        installer_profile = root / "installer-profile"
        installer = stage_installer(source_repo, installer_profile)
        assert installer["module"] == "portus-storage"
        assert installer["live_only"] is True
        for relative in (
            "portus/live-overlay/usr/lib/calamares/modules/portus-storage/module.desc",
            "portus/live-overlay/usr/lib/calamares/modules/portus-storage/main.py",
            "portus/live-overlay/usr/lib/calamares/modules/portus-storage/storage_engine.py",
            "portus/live-overlay/etc/calamares/modules/portus-storage-preflight.conf",
            "portus/live-overlay/etc/calamares/modules/portus-storage.conf",
            "portus/live-overlay/etc/calamares/modules/portus-storage-finalize.conf",
            "portus/live-overlay/etc/calamares/modules/bootloader.conf",
            "portus/live-overlay/etc/calamares/modules/portus-storage-input.conf",
            "portus/live-overlay/etc/calamares/modules/unpackfs.conf",
            "portus/live-overlay/etc/calamares/modules/initcpio.conf",
            "portus/live-overlay/etc/calamares/modules/users.conf",
            "portus/live-overlay/etc/calamares/modules/services-openrc.conf",
            "portus/live-overlay/usr/share/calamares/settings.conf",
            "portus/live-overlay/usr/share/portus-installer/portus-storage-input.qml",
            "portus/live-overlay/usr/local/bin/portus-install",
            "portus/live-overlay/etc/polkit-1/rules.d/90-portus-installer.rules",
        ):
            if not (installer_profile / relative).is_file():
                raise AssertionError(f"installer staging omitted live-only file: {relative}")
        if (installer_profile / "portus/root-overlay/usr/lib/calamares/modules/portus-storage").exists():
            raise AssertionError("Calamares custom module must not be staged into installed rootfs")
        polkit_rule = (
            installer_profile / "portus/live-overlay/etc/polkit-1/rules.d/90-portus-installer.rules"
        ).read_text(encoding="utf-8")
        if "io.calamares.calamares.pkexec.run" not in polkit_rule or 'subject.isInGroup("wheel")' not in polkit_rule:
            raise AssertionError("PortusOS live Calamares polkit authorization is not narrowly bound")
        if (installer_profile / "portus/root-overlay/etc/polkit-1/rules.d/90-portus-installer.rules").exists():
            raise AssertionError("PortusOS live installer polkit rule must not enter installed rootfs")
        launcher = installer_profile / "portus/live-overlay/usr/local/bin/portus-install"
        if launcher.stat().st_mode & 0o111 == 0:
            raise AssertionError("PortusOS installer launcher must be executable")
        settings_text = (installer_profile / "portus/live-overlay/usr/share/calamares/settings.conf").read_text(
            encoding="utf-8"
        )
        for required_text in (
            "notesqml@portus-storage-input",
            "portus-storage@preflight",
            "portus-storage@prepare",
            "portus-storage@finalize",
            "services-openrc",
        ):
            if required_text not in settings_text:
                raise AssertionError(f"PortusOS Calamares settings omitted {required_text}")
        for forbidden_text in ("\n      - partition\n", "luksbootkeyfile"):
            if forbidden_text in settings_text:
                raise AssertionError(f"PortusOS Calamares settings contain forbidden stock storage path: {forbidden_text}")
        bootloader_text = (
            installer_profile / "portus/live-overlay/etc/calamares/modules/bootloader.conf"
        ).read_text(encoding="utf-8")
        if 'efiBootloaderId: "PortusOS"' not in bootloader_text or "installEFIFallback: true" not in bootloader_text:
            raise AssertionError("PortusOS Calamares bootloader override is incomplete")
        try:
            extract_tar_safe(archive, root / "extract")
        except ValueError:
            pass
        else:
            raise AssertionError("path traversal archive must be rejected")
    print("first ISO staging self-test: PASS")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Materialize frozen first-ISO payloads without privilege")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--repo", default=".")
    parser.add_argument("--run-id")
    parser.add_argument("--work-root")
    parser.add_argument("--evidence")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    if not args.run_id or not args.work_root or not args.evidence:
        print("--run-id, --work-root and --evidence are required", file=sys.stderr)
        return 64
    try:
        repo = repo_root_from(args.repo)
        work_root = (repo / args.work_root).resolve() if not Path(args.work_root).is_absolute() else Path(args.work_root).resolve()
        evidence = (repo / args.evidence).resolve() if not Path(args.evidence).is_absolute() else Path(args.evidence).resolve()
        manifest = materialize(repo, args.run_id, work_root, evidence)
        print(json.dumps(manifest, indent=2, sort_keys=True))
        return 0
    except (OSError, ValueError, RuntimeError, json.JSONDecodeError, tarfile.TarError, zipfile.BadZipFile) as error:
        print(f"first ISO staging failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
