#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import sys
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
ENGINE_PATH = ROOT / "portusos-build/installer/modules/portus-storage/storage_engine.py"
MODULE_DIR = ENGINE_PATH.parent
ADAPTER_PATH = MODULE_DIR / "main.py"
spec = importlib.util.spec_from_file_location("portus_storage_engine", ENGINE_PATH)
assert spec and spec.loader
engine = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = engine
spec.loader.exec_module(engine)


class FakeRunner:
    def __init__(self):
        self.calls = []

    def run(self, args, *, input_bytes=None, pass_fds=()):
        self.calls.append((list(args), input_bytes, tuple(pass_fds)))

    def capture(self, args):
        self.calls.append((list(args), None, ()))
        if args[:2] == ["blockdev", "--getro"]:
            return "0"
        if args[:2] == ["blockdev", "--getsize64"]:
            return str(80 * engine.GIB)
        if args[0] == "vgs":
            return "4194304:10000:10000"
        if args[:2] == ["cryptsetup", "luksUUID"]:
            return "11111111-2222-3333-4444-555555555555"
        if args[0] == "blkid":
            device = args[-1]
            return {
                "/dev/portus/root": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
                "/dev/portus/swap": "bbbbbbbb-cccc-dddd-eeee-ffffffffffff",
                "/dev/sdz1": "ESP-UUID",
                "/dev/sdz2": "cccccccc-dddd-eeee-ffff-000000000000",
            }[device]
        if args[0] == "lsblk":
            return "/dev/sdz"
        raise AssertionError(f"unexpected capture: {args}")


class PortusStorageTests(unittest.TestCase):
    def test_calamares_adapter_loads_sibling_engine_without_sys_path_injection(self):
        import types

        self.assertNotIn(str(MODULE_DIR), sys.path)
        fake_libcalamares = types.ModuleType("libcalamares")
        fake_libcalamares.job = types.SimpleNamespace(working_path=str(MODULE_DIR), configuration={})
        fake_libcalamares.globalstorage = types.SimpleNamespace()
        adapter_name = "portus_storage_adapter_test"
        adapter_spec = importlib.util.spec_from_file_location(adapter_name, ADAPTER_PATH)
        self.assertIsNotNone(adapter_spec)
        self.assertIsNotNone(adapter_spec.loader)
        adapter = importlib.util.module_from_spec(adapter_spec)
        with mock.patch.dict(sys.modules, {"libcalamares": fake_libcalamares}):
            adapter_spec.loader.exec_module(adapter)
        self.assertEqual(adapter.StorageError.__name__, "StorageError")
        self.assertTrue(callable(adapter.prepare_storage))
        self.assertTrue(callable(adapter.finalize_target))
        self.assertTrue(callable(adapter.preflight_storage))

        class GlobalStorage:
            def __init__(self, values):
                self.values = dict(values)

            def value(self, key):
                return self.values.get(key)

            def insert(self, key, value):
                self.values[key] = value

            def remove(self, key):
                self.values.pop(key, None)

        fake_libcalamares.job.configuration = {"mode": "preflight"}
        fake_libcalamares.globalstorage = GlobalStorage({"firmwareType": "efi"})
        with mock.patch.object(adapter, "preflight_storage") as preflight:
            result = adapter.run()
        self.assertIsInstance(result, tuple)
        self.assertIn("not explicitly confirmed", result[1])
        preflight.assert_not_called()

        fake_libcalamares.globalstorage = GlobalStorage(
            {
                "firmwareType": "efi",
                "portusStorageInputArmed": True,
                "portusTargetDevice": "/dev/sdz",
                "portusOwnerLuksPassphrase": "owner",
                "portusRecoveryCredential": "recovery",
            }
        )
        with mock.patch.object(
            adapter,
            "preflight_storage",
            return_value={"target": "/dev/sdz", "plan_hash": "a" * 64},
        ) as preflight:
            self.assertIsNone(adapter.run())
        preflight.assert_called_once_with("/dev/sdz", "owner", "recovery")
        self.assertEqual(fake_libcalamares.globalstorage.values["portusStoragePlanHash"], "a" * 64)

        failing_storage = GlobalStorage(
            {
                "firmwareType": "efi",
                "portusStorageInputArmed": True,
                "portusTargetDevice": "/dev/sdz",
                "portusStoragePlanHash": "a" * 64,
                "portusOwnerLuksPassphrase": "owner",
                "portusRecoveryCredential": "recovery",
            }
        )
        fake_libcalamares.globalstorage = failing_storage
        with mock.patch.object(adapter, "preflight_storage", side_effect=adapter.StorageError("fixture failure")):
            result = adapter.run()
        self.assertIsInstance(result, tuple)
        for key in (
            "portusStorageInputArmed",
            "portusTargetDevice",
            "portusStoragePlanHash",
            "portusOwnerLuksPassphrase",
            "portusRecoveryCredential",
        ):
            self.assertNotIn(key, failing_storage.values)

        firmware_failure = GlobalStorage(
            {
                "firmwareType": "bios",
                "portusStorageInputArmed": True,
                "portusTargetDevice": "/dev/sdz",
                "portusStoragePlanHash": "a" * 64,
                "portusOwnerLuksPassphrase": "owner",
                "portusRecoveryCredential": "recovery",
            }
        )
        fake_libcalamares.globalstorage = firmware_failure
        with mock.patch.object(adapter, "preflight_storage") as preflight:
            result = adapter.run()
        self.assertIsInstance(result, tuple)
        self.assertIn("requires UEFI firmware mode", result[1])
        preflight.assert_not_called()
        for key in (
            "portusStorageInputArmed",
            "portusTargetDevice",
            "portusStoragePlanHash",
            "portusOwnerLuksPassphrase",
            "portusRecoveryCredential",
        ):
            self.assertNotIn(key, firmware_failure.values)

        fake_libcalamares.job.configuration = {"mode": "prepare"}
        failing_storage = GlobalStorage(
            {
                "firmwareType": "efi",
                "portusStorageInputArmed": True,
                "portusTargetDevice": "/dev/sdz",
                "portusStoragePlanHash": "a" * 64,
                "portusOwnerLuksPassphrase": "owner",
                "portusRecoveryCredential": "recovery",
            }
        )
        fake_libcalamares.globalstorage = failing_storage
        with mock.patch.object(adapter, "prepare_storage", side_effect=adapter.StorageError("fixture failure")):
            result = adapter.run()
        self.assertIsInstance(result, tuple)
        for key in (
            "portusStorageInputArmed",
            "portusTargetDevice",
            "portusStoragePlanHash",
            "portusOwnerLuksPassphrase",
            "portusRecoveryCredential",
        ):
            self.assertNotIn(key, failing_storage.values)

    def test_locked_plan_and_partition_naming(self):
        self.assertEqual(engine.partition_path("/dev/sda", 3), "/dev/sda3")
        self.assertEqual(engine.partition_path("/dev/nvme0n1", 3), "/dev/nvme0n1p3")
        text = engine.sfdisk_script()
        self.assertIn("size=512", text)
        self.assertIn("size=2048", text)
        self.assertIn(engine.LINUX_LUKS_TYPE, text)
        self.assertEqual(len(engine.plan_hash("/dev/sda", 40 * engine.GIB)), 64)
        with self.assertRaises(engine.StorageError):
            engine.plan_hash("/dev/sda", 40 * engine.GIB - 1)
        self.assertNotEqual(
            engine.plan_hash("/dev/sda", 40 * engine.GIB),
            engine.plan_hash("/dev/sda", 80 * engine.GIB),
        )

    def test_credentials_are_required_and_independent_without_invented_strength_policy(self):
        with self.assertRaises(engine.StorageError):
            engine.validate_secrets("", "recovery")
        with self.assertRaises(engine.StorageError):
            engine.validate_secrets("owner", "")
        with self.assertRaises(engine.StorageError):
            engine.validate_secrets("same", "same")
        engine.validate_secrets("owner", "independent-recovery")

    def test_small_target_is_rejected_before_destructive_commands(self):
        class SmallRunner(FakeRunner):
            def capture(self, args):
                self.calls.append((list(args), None, ()))
                if args[:2] == ["blockdev", "--getsize64"]:
                    return str(40 * engine.GIB - 1)
                return super().capture(args)

        runner = SmallRunner()
        with mock.patch.object(engine, "_require_block_device"):
            with self.assertRaises(engine.StorageError):
                engine.prepare_storage(
                    "/dev/sdz",
                    "owner",
                    "recovery",
                    "irrelevant",
                    "/tmp/calamares-root-test",
                    runner=runner,
                )
        destructive = {"wipefs", "sfdisk", "cryptsetup", "pvcreate", "vgcreate", "lvcreate", "mkfs.ext4", "mkfs.fat"}
        self.assertFalse(any(args and args[0] in destructive for args, _, _ in runner.calls))

    def test_preflight_is_non_destructive_and_hashes_current_size(self):
        runner = FakeRunner()
        with mock.patch.object(engine, "_require_block_device"), mock.patch.object(
            engine, "_require_names_free"
        ):
            proof = engine.preflight_storage("/dev/sdz", "owner", "recovery", runner=runner)
        self.assertEqual(proof["target"], "/dev/sdz")
        self.assertEqual(proof["target_bytes"], 80 * engine.GIB)
        self.assertEqual(proof["plan_hash"], engine.plan_hash("/dev/sdz", 80 * engine.GIB))
        destructive = {
            "wipefs",
            "sfdisk",
            "cryptsetup",
            "pvcreate",
            "vgcreate",
            "lvcreate",
            "mkfs.ext4",
            "mkfs.fat",
        }
        self.assertFalse(any(args and args[0] in destructive for args, _, _ in runner.calls))

    def test_read_only_target_is_rejected_before_destructive_commands(self):
        class ReadOnlyRunner(FakeRunner):
            def capture(self, args):
                if args[:2] == ["blockdev", "--getro"]:
                    self.calls.append((list(args), None, ()))
                    return "1"
                return super().capture(args)

        runner = ReadOnlyRunner()
        with mock.patch.object(engine, "_require_block_device"):
            with self.assertRaisesRegex(engine.StorageError, "read-only"):
                engine.preflight_storage("/dev/sdz", "owner", "recovery", runner=runner)
        self.assertFalse(any(args and args[0] == "wipefs" for args, _, _ in runner.calls))

    def test_storage_input_qml_is_static_masked_and_fail_closed(self):
        qml = (ROOT / "portusos-build/installer/config/portus-storage-input.qml").read_text(encoding="utf-8")
        self.assertIn("Qt.labs.folderlistmodel", qml)
        self.assertIn('folder: "file:///sys/block"', qml)
        self.assertIn('nameFilters: ["sd*", "vd*", "xvd*", "nvme*n*", "mmcblk*", "hd*"]', qml)
        self.assertGreaterEqual(qml.count("echoMode: TextInput.Password"), 4)
        self.assertIn('Global.insert("portusTargetDevice", selectedTarget())', qml)
        self.assertIn('Global.insert("portusStorageInputArmed", true)', qml)
        self.assertNotIn('Global.insert("portusStoragePlanHash"', qml)
        self.assertIn("function onActivate()", qml)
        self.assertIn("function onLeave()", qml)
        self.assertIn("ViewManager.next()", qml)
        self.assertNotIn("XMLHttpRequest", qml)
        self.assertNotIn("/tmp/portus-storage-input", qml)

    def test_preexisting_name_failure_does_not_run_owned_cleanup(self):
        runner = FakeRunner()
        expected_hash = engine.plan_hash("/dev/sdz", 80 * engine.GIB)
        with mock.patch.object(engine, "_require_block_device"), mock.patch.object(
            engine, "_require_unmounted"
        ), mock.patch.object(engine, "_require_names_free", side_effect=engine.StorageError("name busy")), mock.patch.object(
            engine, "_cleanup_owned"
        ) as cleanup:
            with self.assertRaises(engine.StorageError):
                engine.prepare_storage(
                    "/dev/sdz",
                    "owner",
                    "recovery",
                    expected_hash,
                    "/tmp/calamares-root-test",
                    runner=runner,
                )
        cleanup.assert_not_called()

    def test_prepare_never_places_secrets_in_argv_or_public_state(self):
        runner = FakeRunner()
        owner = "owner-passphrase"
        recovery = "recovery-credential-0123456789"
        with tempfile.TemporaryDirectory(prefix="portus-storage-test-") as raw:
            target_root = Path(raw) / "target"
            with mock.patch.object(engine, "_require_block_device"), mock.patch.object(
                engine, "_require_unmounted"
            ), mock.patch.object(engine, "_require_names_free"):
                state = engine.prepare_storage(
                    "/dev/sdz",
                    owner,
                    recovery,
                    engine.plan_hash("/dev/sdz", 80 * engine.GIB),
                    str(target_root),
                    runner=runner,
                )
        serialized = repr(state)
        self.assertNotIn(owner, serialized)
        self.assertNotIn(recovery, serialized)
        for args, _, _ in runner.calls:
            joined = " ".join(args)
            self.assertNotIn(owner, joined)
            self.assertNotIn(recovery, joined)
        recovery_calls = [args for args, _, _ in runner.calls if args[:2] == ["cryptsetup", "luksAddKey"]]
        self.assertEqual(len(recovery_calls), 1)
        self.assertIn("--pbkdf", recovery_calls[0])
        self.assertIn("argon2id", recovery_calls[0])
        self.assertIn("--pbkdf-memory", recovery_calls[0])
        self.assertEqual(state["target_bytes"], 80 * engine.GIB)
        self.assertEqual(state["vg"], "portus")
        self.assertGreater(state["reserve_extents"], 0)
        self.assertEqual(state["boot_loader"], {"installPath": "/dev/sdz"})

    def test_finalize_writes_locked_nonsecret_boot_storage_configuration(self):
        with tempfile.TemporaryDirectory(prefix="portus-storage-finalize-") as raw:
            root = Path(raw)
            (root / "etc/mkinitcpio.d").mkdir(parents=True)
            (root / "etc/default").mkdir(parents=True)
            (root / "etc/mkinitcpio.conf").write_text("HOOKS=(base udev filesystems)\n", encoding="utf-8")
            for kernel in ("linux-lts", "linux"):
                (root / f"etc/mkinitcpio.d/{kernel}.preset").write_text("PRESETS=('default')\n", encoding="utf-8")
            (root / "etc/default/grub").write_text("GRUB_TIMEOUT=1\n", encoding="utf-8")
            state = {
                "schema_version": 1,
                "root_mount_point": str(root),
                "luks_uuid": "11111111-2222-3333-4444-555555555555",
                "luks_mapper": "portus-crypt",
                "vg": "portus",
                "partitions": [
                    {"mountPoint": "/", "uuid": "root-uuid"},
                    {"mountPoint": "/boot", "uuid": "boot-uuid"},
                    {"mountPoint": "/boot/efi", "uuid": "esp-uuid"},
                    {"mountPoint": "", "uuid": "swap-uuid"},
                ],
            }
            engine.finalize_target(state)
            fstab = (root / "etc/fstab").read_text(encoding="utf-8")
            crypttab = (root / "etc/crypttab").read_text(encoding="utf-8")
            mkinit = (root / "etc/mkinitcpio.conf").read_text(encoding="utf-8")
            grub = (root / "etc/default/grub").read_text(encoding="utf-8")
            self.assertIn("UUID=root-uuid / ext4", fstab)
            self.assertIn("portus-crypt UUID=11111111-2222-3333-4444-555555555555 none luks", crypttab)
            self.assertIn("HOOKS=(base udev autodetect microcode modconf kms keyboard keymap block encrypt lvm2 filesystems fsck)", mkinit)
            self.assertIn("cryptdevice=UUID=11111111-2222-3333-4444-555555555555:portus-crypt", grub)
            for kernel in ("linux-lts", "linux"):
                preset = (root / f"etc/mkinitcpio.d/{kernel}.preset").read_text(encoding="utf-8")
                self.assertIn("PRESETS=('default' 'fallback')", preset)


if __name__ == "__main__":
    unittest.main()
