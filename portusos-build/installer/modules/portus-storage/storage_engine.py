# SPDX-License-Identifier: Apache-2.0
"""Bounded first-ISO PortusOS storage implementation for Calamares.

This module deliberately owns only the verified stock-Calamares gap:
GPT layout, LUKS2, LVM, target mounts, and dynamic boot/storage config.
It never acquires credentials and never logs or persists them.
"""
from __future__ import annotations

from dataclasses import dataclass
from decimal import Decimal, ROUND_CEILING
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
from typing import Any

ESP_TYPE = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"
LINUX_FS_TYPE = "0fc63daf-8483-4772-8e79-3d69d8477de4"
LINUX_LUKS_TYPE = "ca7d7ccb-63ed-4c53-861c-1742536059cc"
MIB = 1024 * 1024
GIB = 1024 * MIB
MINIMUM_DISK_BYTES = 40 * GIB


class StorageError(RuntimeError):
    pass


@dataclass(frozen=True)
class StorageSpec:
    esp_mib: int = 512
    boot_mib: int = 2048
    swap_mib: int = 4096
    reserve_percent: int = 5
    vg: str = "portus"
    mapper: str = "portus-crypt"
    root_lv: str = "root"
    swap_lv: str = "swap"
    luks_type: str = "luks2"
    cipher: str = "aes-xts-plain64"
    key_bits: int = 512
    pbkdf: str = "argon2id"
    target_time_ms: int = 2000
    memory_kib: int = 262144

    def canonical(self, target: str, target_bytes: int) -> dict[str, Any]:
        return {
            "target": target,
            "target_bytes": target_bytes,
            "partition_table": "gpt",
            "esp_mib": self.esp_mib,
            "boot_mib": self.boot_mib,
            "luks": {
                "type": self.luks_type,
                "cipher": self.cipher,
                "key_bits": self.key_bits,
                "pbkdf": self.pbkdf,
                "target_time_ms": self.target_time_ms,
                "memory_kib": self.memory_kib,
            },
            "lvm": {
                "vg": self.vg,
                "root_lv": self.root_lv,
                "swap_lv": self.swap_lv,
                "swap_mib": self.swap_mib,
                "reserve_percent": self.reserve_percent,
            },
        }


SPEC = StorageSpec()


def canonical_target(value: str) -> str:
    if not value or not value.startswith("/dev/"):
        raise StorageError("installer target must be an explicit /dev block device")
    resolved = os.path.realpath(value)
    if not resolved.startswith("/dev/") or not re.fullmatch(r"/dev/[A-Za-z0-9._+-]+", resolved):
        raise StorageError("installer target resolves outside the bounded /dev device namespace")
    return resolved


def partition_path(target: str, number: int) -> str:
    if number < 1:
        raise StorageError("partition number must be positive")
    return f"{target}p{number}" if target[-1:].isdigit() else f"{target}{number}"


def plan_hash(target: str, target_bytes: int, spec: StorageSpec = SPEC) -> str:
    if target_bytes < MINIMUM_DISK_BYTES:
        raise StorageError("target disk is smaller than the locked 40 GiB first-ISO minimum")
    payload = json.dumps(
        spec.canonical(canonical_target(target), target_bytes),
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    return hashlib.sha256(payload).hexdigest()


def sfdisk_script(spec: StorageSpec = SPEC) -> str:
    return (
        "label: gpt\n"
        "unit: MiB\n"
        f"start=1,size={spec.esp_mib},type={ESP_TYPE},name=PORTUS-ESP\n"
        f"size={spec.boot_mib},type={LINUX_FS_TYPE},name=PORTUS-BOOT\n"
        f"type={LINUX_LUKS_TYPE},name=PORTUS-LUKS\n"
    )


def validate_secrets(owner: str, recovery: str) -> None:
    if not owner:
        raise StorageError("owner LUKS passphrase is required")
    if not recovery:
        raise StorageError("recovery credential is required")
    if owner == recovery:
        raise StorageError("owner and recovery credentials must be independent")


class Runner:
    """No-shell command runner with secret-safe failure reporting."""

    def run(self, args: list[str], *, input_bytes: bytes | None = None, pass_fds: tuple[int, ...] = ()) -> None:
        result = subprocess.run(
            args,
            input=input_bytes,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
            pass_fds=pass_fds,
        )
        if result.returncode != 0:
            raise StorageError(f"storage command {Path(args[0]).name} failed with exit {result.returncode}")

    def capture(self, args: list[str]) -> str:
        result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, check=False)
        if result.returncode != 0:
            raise StorageError(f"storage inspection {Path(args[0]).name} failed with exit {result.returncode}")
        return result.stdout.strip()


def _require_block_device(target: str) -> None:
    try:
        mode = os.stat(target).st_mode
    except OSError as exc:
        raise StorageError("installer target is not accessible") from exc
    if not stat.S_ISBLK(mode):
        raise StorageError("installer target is not a block device")


def _require_writable(target: str, runner: Runner) -> None:
    value = runner.capture(["blockdev", "--getro", target]).strip()
    if value != "0":
        raise StorageError("installer target is read-only")


def _target_size_bytes(target: str, runner: Runner) -> int:
    raw = runner.capture(["blockdev", "--getsize64", target]).strip()
    try:
        size = int(raw)
    except ValueError as exc:
        raise StorageError("unable to determine target disk size") from exc
    if size < MINIMUM_DISK_BYTES:
        raise StorageError("target disk is smaller than the locked 40 GiB first-ISO minimum")
    return size


def _require_unmounted(target: str, runner: Runner) -> None:
    output = runner.capture(["lsblk", "-nrpo", "NAME,MOUNTPOINTS", target])
    for line in output.splitlines():
        fields = line.split(None, 1)
        if len(fields) > 1 and fields[1].strip():
            raise StorageError("installer target or a child device is mounted")


def _require_names_free(runner: Runner, spec: StorageSpec) -> None:
    if Path(f"/dev/mapper/{spec.mapper}").exists():
        raise StorageError("PortusOS LUKS mapper name is already active")
    result = subprocess.run(["vgs", "--noheadings", "-o", "vg_name", spec.vg], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    if result.returncode == 0:
        raise StorageError("PortusOS volume-group name already exists")


def _memfd_secret(name: str, secret: str) -> int:
    if not hasattr(os, "memfd_create"):
        raise StorageError("anonymous Linux memfd support is required for recovery-key provisioning")
    fd = os.memfd_create(name, flags=0)
    os.write(fd, secret.encode("utf-8"))
    os.lseek(fd, 0, os.SEEK_SET)
    return fd


def _query_vg_extents(runner: Runner, spec: StorageSpec) -> tuple[int, int, int]:
    raw = runner.capture([
        "vgs", "--noheadings", "--separator", ":", "--units", "b", "--nosuffix",
        "-o", "vg_extent_size,vg_extent_count,vg_free_count", spec.vg,
    ])
    fields = [item.strip() for item in raw.split(":")]
    if len(fields) != 3:
        raise StorageError("unexpected LVM extent report")
    extent_bytes = int(Decimal(fields[0]).to_integral_value(rounding=ROUND_CEILING))
    total = int(fields[1])
    free = int(fields[2])
    if extent_bytes <= 0 or total <= 0 or free <= 0:
        raise StorageError("invalid LVM extent report")
    return extent_bytes, total, free


def _uuid(runner: Runner, device: str) -> str:
    value = runner.capture(["blkid", "-s", "UUID", "-o", "value", device]).strip()
    if not value:
        raise StorageError("filesystem UUID discovery returned empty data")
    return value


def _cleanup_owned(
    root_mount: Path,
    *,
    mounted_root: bool,
    mounted_boot: bool,
    mounted_esp: bool,
    vg_created: bool,
    mapper_open: bool,
    spec: StorageSpec,
) -> None:
    """Best-effort cleanup of resources created by this invocation only."""
    commands: list[list[str]] = []
    if mounted_esp:
        commands.append(["umount", str(root_mount / "boot/efi")])
    if mounted_boot:
        commands.append(["umount", str(root_mount / "boot")])
    if mounted_root:
        commands.append(["umount", str(root_mount)])
    if vg_created:
        commands.append(["vgchange", "-an", spec.vg])
    if mapper_open:
        commands.append(["cryptsetup", "close", spec.mapper])
    for args in commands:
        subprocess.run(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)


def preflight_storage(
    target_value: str,
    owner_secret: str,
    recovery_secret: str,
    *,
    runner: Runner | None = None,
    spec: StorageSpec = SPEC,
) -> dict[str, Any]:
    """Validate the confirmed target/credentials without modifying storage."""
    runner = runner or Runner()
    target = canonical_target(target_value)
    validate_secrets(owner_secret, recovery_secret)
    _require_block_device(target)
    _require_writable(target, runner)
    target_bytes = _target_size_bytes(target, runner)
    _require_unmounted(target, runner)
    _require_names_free(runner, spec)
    return {
        "schema_version": 1,
        "target": target,
        "target_bytes": target_bytes,
        "plan_hash": plan_hash(target, target_bytes, spec),
    }


def prepare_storage(
    target_value: str,
    owner_secret: str,
    recovery_secret: str,
    confirmed_plan_hash: str,
    root_mount_point: str,
    *,
    runner: Runner | None = None,
    spec: StorageSpec = SPEC,
) -> dict[str, Any]:
    """Destroy and build only the explicitly confirmed target disk."""
    runner = runner or Runner()
    target = canonical_target(target_value)
    validate_secrets(owner_secret, recovery_secret)
    _require_block_device(target)
    target_bytes = _target_size_bytes(target, runner)
    _require_writable(target, runner)
    expected_hash = plan_hash(target, target_bytes, spec)
    if confirmed_plan_hash != expected_hash:
        raise StorageError("storage plan confirmation hash does not match the selected target and locked plan")
    _require_unmounted(target, runner)
    _require_names_free(runner, spec)

    root_mount = Path(root_mount_point).resolve(strict=False)
    if root_mount == Path("/tmp") or not root_mount.is_relative_to(Path("/tmp")):
        raise StorageError("Calamares target mount point must remain below /tmp")

    mounted_root = False
    mounted_boot = False
    mounted_esp = False
    mapper_open = False
    vg_created = False
    p1, p2, p3 = (partition_path(target, n) for n in (1, 2, 3))
    owner_bytes = owner_secret.encode("utf-8")

    try:
        runner.run(["wipefs", "--all", "--force", target])
        runner.run(
            ["sfdisk", "--wipe", "always", "--wipe-partitions", "always", target],
            input_bytes=sfdisk_script(spec).encode(),
        )
        runner.run(["blockdev", "--rereadpt", target])
        runner.run(["udevadm", "settle"])
        runner.run(["mkfs.fat", "-F", "32", "-n", "PORTUS-ESP", p1])
        runner.run(["mkfs.ext4", "-F", "-L", "PORTUS-BOOT", p2])

        runner.run(
            [
                "cryptsetup",
                "luksFormat",
                "--batch-mode",
                "--type",
                spec.luks_type,
                "--cipher",
                spec.cipher,
                "--key-size",
                str(spec.key_bits),
                "--pbkdf",
                spec.pbkdf,
                "--iter-time",
                str(spec.target_time_ms),
                "--pbkdf-memory",
                str(spec.memory_kib),
                "--key-file",
                "-",
                p3,
            ],
            input_bytes=owner_bytes,
        )

        recovery_fd = _memfd_secret("portus-recovery-key", recovery_secret)
        try:
            runner.run(
                [
                    "cryptsetup",
                    "luksAddKey",
                    p3,
                    "--key-file",
                    "-",
                    "--new-keyfile",
                    f"/proc/self/fd/{recovery_fd}",
                    "--pbkdf",
                    spec.pbkdf,
                    "--iter-time",
                    str(spec.target_time_ms),
                    "--pbkdf-memory",
                    str(spec.memory_kib),
                ],
                input_bytes=owner_bytes,
                pass_fds=(recovery_fd,),
            )
        finally:
            os.close(recovery_fd)

        runner.run(
            ["cryptsetup", "open", "--type", "luks", "--key-file", "-", p3, spec.mapper],
            input_bytes=owner_bytes,
        )
        mapper_open = True
        mapper = f"/dev/mapper/{spec.mapper}"
        runner.run(["pvcreate", "--yes", mapper])
        runner.run(["vgcreate", spec.vg, mapper])
        vg_created = True
        extent_bytes, total_extents, free_extents = _query_vg_extents(runner, spec)
        reserve_extents = max(1, (total_extents * spec.reserve_percent + 99) // 100)
        swap_extents = (spec.swap_mib * MIB + extent_bytes - 1) // extent_bytes
        root_extents = free_extents - reserve_extents - swap_extents
        if root_extents <= 0:
            raise StorageError("target disk cannot satisfy the locked PortusOS LVM plan")
        runner.run(["lvcreate", "-l", str(root_extents), "-n", spec.root_lv, spec.vg])
        runner.run(["lvcreate", "-l", str(swap_extents), "-n", spec.swap_lv, spec.vg])

        root_dev = f"/dev/{spec.vg}/{spec.root_lv}"
        swap_dev = f"/dev/{spec.vg}/{spec.swap_lv}"
        runner.run(["mkfs.ext4", "-F", "-L", "PORTUS-ROOT", root_dev])
        runner.run(["mkswap", "-L", "PORTUS-SWAP", swap_dev])

        root_mount.mkdir(parents=True, exist_ok=True)
        runner.run(["mount", root_dev, str(root_mount)])
        mounted_root = True
        (root_mount / "boot").mkdir(parents=True, exist_ok=True)
        runner.run(["mount", p2, str(root_mount / "boot")])
        mounted_boot = True
        (root_mount / "boot/efi").mkdir(parents=True, exist_ok=True)
        runner.run(["mount", p1, str(root_mount / "boot/efi")])
        mounted_esp = True

        luks_uuid = runner.capture(["cryptsetup", "luksUUID", p3]).strip()
        if not luks_uuid:
            raise StorageError("LUKS UUID discovery returned empty data")
        state = {
            "schema_version": 1,
            "target": target,
            "target_bytes": target_bytes,
            "plan_hash": expected_hash,
            "root_mount_point": str(root_mount),
            "luks_device": p3,
            "luks_uuid": luks_uuid,
            "luks_mapper": spec.mapper,
            "vg": spec.vg,
            "reserve_extents": reserve_extents,
            "total_extents": total_extents,
            "partitions": [
                {
                    "device": root_dev,
                    "mountPoint": "/",
                    "fs": "ext4",
                    "fsName": "ext4",
                    "uuid": _uuid(runner, root_dev),
                    "claimed": True,
                },
                {
                    "device": p2,
                    "mountPoint": "/boot",
                    "fs": "ext4",
                    "fsName": "ext4",
                    "uuid": _uuid(runner, p2),
                    "claimed": True,
                },
                {
                    "device": p1,
                    "mountPoint": "/boot/efi",
                    "fs": "fat32",
                    "fsName": "fat32",
                    "uuid": _uuid(runner, p1),
                    "claimed": True,
                },
                {
                    "device": swap_dev,
                    "mountPoint": "",
                    "fs": "linuxswap",
                    "fsName": "linuxswap",
                    "uuid": _uuid(runner, swap_dev),
                    "claimed": True,
                },
            ],
            "boot_loader": {"installPath": target},
        }
        return state
    except Exception:
        _cleanup_owned(
            root_mount,
            mounted_root=mounted_root,
            mounted_boot=mounted_boot,
            mounted_esp=mounted_esp,
            vg_created=vg_created,
            mapper_open=mapper_open,
            spec=spec,
        )
        raise

def _write(path: Path, text: str, mode: int = 0o644) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    path.chmod(mode)


def _replace_shell_assignment(path: Path, key: str, literal: str) -> None:
    text = path.read_text(encoding="utf-8") if path.exists() else ""
    line = f"{key}={literal}"
    pattern = re.compile(rf"^\s*{re.escape(key)}=.*$", re.MULTILINE)
    text = pattern.sub(line, text) if pattern.search(text) else text.rstrip() + "\n" + line + "\n"
    _write(path, text)


def finalize_target(state: dict[str, Any], *, spec: StorageSpec = SPEC) -> None:
    """Write only non-secret dynamic storage/boot configuration into the target."""
    if state.get("schema_version") != 1 or state.get("vg") != spec.vg or state.get("luks_mapper") != spec.mapper:
        raise StorageError("invalid PortusOS storage state supplied to finalization")
    root = Path(str(state.get("root_mount_point", "")))
    if not root.is_absolute() or not root.is_dir():
        raise StorageError("target root is unavailable for PortusOS storage finalization")
    partitions = {p["mountPoint"]: p for p in state.get("partitions", [])}
    for mountpoint in ("/", "/boot", "/boot/efi", ""):
        if mountpoint not in partitions:
            raise StorageError("storage state is missing a required filesystem")
    luks_uuid = str(state.get("luks_uuid", ""))
    if not luks_uuid:
        raise StorageError("storage state is missing the LUKS UUID")

    _write(root / "etc/crypttab", f"{spec.mapper} UUID={luks_uuid} none luks\n")
    _write(
        root / "etc/fstab",
        "# PortusOS first-ISO storage\n"
        f"UUID={partitions['/']['uuid']} / ext4 defaults,relatime 0 1\n"
        f"UUID={partitions['/boot']['uuid']} /boot ext4 defaults,relatime 0 2\n"
        f"UUID={partitions['/boot/efi']['uuid']} /boot/efi vfat defaults,umask=0077 0 2\n"
        f"UUID={partitions['']['uuid']} none swap defaults 0 0\n",
    )
    mkinit = root / "etc/mkinitcpio.conf"
    _replace_shell_assignment(
        mkinit,
        "HOOKS",
        "(base udev autodetect microcode modconf kms keyboard keymap block encrypt lvm2 filesystems fsck)",
    )
    for kernel in ("linux-lts", "linux"):
        preset = root / f"etc/mkinitcpio.d/{kernel}.preset"
        if not preset.exists():
            raise StorageError(f"required mkinitcpio preset is missing for {kernel}")
        _replace_shell_assignment(preset, "PRESETS", "('default' 'fallback')")

    grub = root / "etc/default/grub"
    _replace_shell_assignment(grub, "GRUB_DEFAULT", '"0"')
    _replace_shell_assignment(grub, "GRUB_TIMEOUT", '"5"')
    _replace_shell_assignment(
        grub,
        "GRUB_CMDLINE_LINUX",
        f'"cryptdevice=UUID={luks_uuid}:{spec.mapper} root=/dev/{spec.vg}/{spec.root_lv}"',
    )


def self_test() -> None:
    assert partition_path("/dev/sda", 3) == "/dev/sda3"
    assert partition_path("/dev/nvme0n1", 3) == "/dev/nvme0n1p3"
    script = sfdisk_script()
    assert "size=512" in script and "size=2048" in script and LINUX_LUKS_TYPE in script
    assert len(plan_hash("/dev/sda", 40 * GIB)) == 64
    try:
        validate_secrets("abcdefgh", "abcdefgh")
    except StorageError:
        pass
    else:
        raise AssertionError("equal credentials must be rejected")
