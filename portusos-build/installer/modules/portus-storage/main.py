# SPDX-License-Identifier: Apache-2.0
"""Calamares adapter for the bounded PortusOS storage engine."""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys

import libcalamares


def _load_storage_engine():
    """Load the sibling engine from Calamares' explicit Python-job working path.

    Calamares 3.4.2 evaluates main.py by absolute filename and does not add the
    module directory to sys.path, so a normal sibling import is not reliable.
    """
    working_path = Path(str(libcalamares.job.working_path)).resolve()
    engine_path = working_path / "storage_engine.py"
    if not engine_path.is_file() or engine_path.parent != working_path:
        raise RuntimeError("PortusOS storage engine is missing from the Calamares module directory")
    module_name = "_portus_storage_engine"
    module_spec = importlib.util.spec_from_file_location(module_name, engine_path)
    if module_spec is None or module_spec.loader is None:
        raise RuntimeError("PortusOS storage engine cannot be loaded")
    module = importlib.util.module_from_spec(module_spec)
    sys.modules[module_name] = module
    try:
        module_spec.loader.exec_module(module)
    except Exception:
        sys.modules.pop(module_name, None)
        raise
    return module


_engine = _load_storage_engine()
StorageError = _engine.StorageError
finalize_target = _engine.finalize_target
prepare_storage = _engine.prepare_storage
preflight_storage = _engine.preflight_storage


def pretty_name():
    return "Configure PortusOS encrypted storage"


def pretty_status_message():
    return "Configuring PortusOS encrypted storage…"


def _text(value):
    return "" if value is None else str(value)


def _clear_runtime_inputs(storage, *keys):
    for key in keys:
        storage.remove(key)


def run():
    config = libcalamares.job.configuration
    storage = libcalamares.globalstorage
    mode = _text(config.get("mode"))
    state_key = _text(config.get("stateKey", "portusStorageState"))

    try:
        if mode == "preflight":
            target_key = _text(config.get("targetDeviceKey", "portusTargetDevice"))
            owner_key = _text(config.get("ownerSecretKey", "portusOwnerLuksPassphrase"))
            recovery_key = _text(config.get("recoverySecretKey", "portusRecoveryCredential"))
            hash_key = _text(config.get("planHashKey", "portusStoragePlanHash"))
            armed_key = _text(config.get("armedKey", "portusStorageInputArmed"))
            try:
                if _text(storage.value("firmwareType")) != "efi":
                    raise StorageError("PortusOS first-ISO installation requires UEFI firmware mode")
                if storage.value(armed_key) is not True:
                    raise StorageError(
                        "storage inputs were not explicitly confirmed; return to Storage & Recovery and use its Continue button"
                    )
                proof = preflight_storage(
                    _text(storage.value(target_key)),
                    _text(storage.value(owner_key)),
                    _text(storage.value(recovery_key)),
                )
            except Exception:
                _clear_runtime_inputs(storage, owner_key, recovery_key, hash_key, target_key, armed_key)
                raise
            storage.insert(target_key, proof["target"])
            storage.insert(hash_key, proof["plan_hash"])
            return None

        if mode == "prepare":
            root_mount = _text(config.get("rootMountPoint", "/tmp/calamares-root"))
            target_key = _text(config.get("targetDeviceKey", "portusTargetDevice"))
            owner_key = _text(config.get("ownerSecretKey", "portusOwnerLuksPassphrase"))
            recovery_key = _text(config.get("recoverySecretKey", "portusRecoveryCredential"))
            hash_key = _text(config.get("planHashKey", "portusStoragePlanHash"))
            armed_key = _text(config.get("armedKey", "portusStorageInputArmed"))
            try:
                if _text(storage.value("firmwareType")) != "efi":
                    raise StorageError("PortusOS first-ISO installation requires UEFI firmware mode")
                if storage.value(armed_key) is not True:
                    raise StorageError(
                        "storage inputs were not explicitly confirmed; return to Storage & Recovery and use its Continue button"
                    )
                state = prepare_storage(
                    _text(storage.value(target_key)),
                    _text(storage.value(owner_key)),
                    _text(storage.value(recovery_key)),
                    _text(storage.value(hash_key)),
                    root_mount,
                )
            finally:
                _clear_runtime_inputs(storage, owner_key, recovery_key, hash_key, target_key, armed_key)
            storage.insert("rootMountPoint", state["root_mount_point"])
            storage.insert("efiSystemPartition", "/boot/efi")
            storage.insert("partitions", state["partitions"])
            storage.insert("bootLoader", state["boot_loader"])
            storage.insert(state_key, state)
            # Runtime input keys were cleared in the prepare finally block;
            # public storage identity is retained only in portusStorageState.
            return None

        if mode == "finalize":
            state = storage.value(state_key)
            if not isinstance(state, dict):
                raise StorageError("PortusOS storage state is unavailable for finalization")
            finalize_target(state)
            return None

        raise StorageError("unknown PortusOS storage module mode")
    except StorageError as exc:
        return ("PortusOS storage configuration failed", str(exc))
    except Exception:
        # Do not echo arbitrary exception values; they may originate in secret-bearing APIs.
        return ("PortusOS storage configuration failed", "unexpected storage implementation failure")
