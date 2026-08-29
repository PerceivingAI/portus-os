#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

category=${1:-}

case "$category" in
    unit)
        cargo test --locked --workspace --lib
        ;;
    protocol)
        cargo test --locked -p portus-protocol --all-targets
        ;;
    state)
        cargo test --locked -p portus-state --all-targets
        ;;
    integration)
        if [ "$(uname -s)" != "Linux" ]; then
            printf '%s\n' "test category 'integration' currently requires Linux/Artix" >&2
            exit 78
        fi
        cargo test --locked -p portus-client -p portus-index -p portusd -p portus-os --all-targets
        ;;
    security-negative)
        cargo test --locked -p portus-policy -p portus-privd -p portus-protected-api -p portus-apid -p portus-api -p portus-auth --all-targets
        ;;
    build-contract)
        cargo test --locked -p portus-build-contract --all-targets
        cargo run --locked -q -p portus-build-contract -- .
        ;;
    build-skeleton)
        python -B scripts/build_iteration.py --self-test
        python -B scripts/build_environment_preflight.py --self-test
        python -B scripts/artix/stage_first_iso.py --self-test
        python -B scripts/artix/test_portus_storage.py -v
        python -B scripts/build_iteration.py --check-config portusos-build/configs/first-live.json >/dev/null
        cargo test --locked -p portus-build --all-targets
        cargo run --locked -q -p portus-build -- validate
        cargo run --locked -q -p portus-build -- plan --disk-size-mib 40960
        set +e
        cargo run --locked -q -p portus-build -- build-iso
        status=$?
        set -e
        if [ "$status" -ne 78 ]; then
            printf '%s\n' 'build-iso without a run-owned staging manifest must fail closed with exit 78' >&2
            exit 1
        fi
        ;;
    hardening)
        cargo test --locked -p portus-client -p portus-protocol -p portus-state -p portus-provider -p portus-index -p portus-task -p portus-health -p portus-audit -p portus-policy -p portus-privd -p portus-protected-api -p portus-apid -p portus-api -p portus-auth -p portus-artifact -p portus-browser-integration -p portus-visual -p portus-install -p portusd -p portus-os --all-targets
        ;;
    graphical-vm)
        cargo test --locked -p portus-build validation_harness
        cargo run --locked -q -p portus-build -- validation-harness-check
        set +e
        cargo run --locked -q -p portus-build -- validation-vm-run
        status=$?
        set -e
        if [ "$status" -ne 78 ]; then
            printf '%s\n' 'validation-vm-run must fail closed with exit 78 until Track V provides the VMware adapter' >&2
            exit 1
        fi
        ;;
    oss)
        python3 -B scripts/oss/test_oss.py -v
        python3 -B scripts/release/test_signing.py -v
        python3 -B scripts/oss/dependency_inventory.py --check
        cargo audit -D warnings
        cargo deny check advisories bans licenses sources
        python3 -B scripts/oss/audit_repo.py --scope current --strict
        python3 -B scripts/oss/audit_repo.py --scope history --strict
        ;;
    all)
        cargo test --locked --workspace --all-targets
        ;;
    "")
        printf '%s\n' "usage: scripts/test.sh {unit|protocol|state|integration|security-negative|build-contract|build-skeleton|hardening|graphical-vm|oss|all}" >&2
        exit 64
        ;;
    *)
        printf '%s\n' "unknown test category: $category" >&2
        exit 64
        ;;
esac
