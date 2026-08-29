#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

bash -n scripts/artix/collect-l0-l2-facts.sh
sh -n portusos-build/build-iso.sh
python -B scripts/build_iteration.py --self-test >/dev/null
python -B scripts/build_environment_preflight.py --self-test >/dev/null
python -B scripts/artix/context.py self-test >/dev/null
python -B scripts/artix/stage_first_iso.py --self-test >/dev/null
python -B scripts/artix/test_portus_storage.py >/dev/null
python -B scripts/build_iteration.py --check-config portusos-build/configs/first-live.json >/dev/null
bash -n portusos-build/rootfs/overlay/usr/local/bin/portus-mcp-local
bash -n portusos-build/rootfs/overlay/usr/local/bin/portus-tunnel-setup
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
