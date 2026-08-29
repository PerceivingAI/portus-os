param(
    [Parameter(Position = 0)]
    [string]$Category
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    switch ($Category) {
        'unit' {
            cargo test --locked --workspace --lib
            exit $LASTEXITCODE
        }
        'protocol' {
            cargo test --locked -p portus-protocol --all-targets
            exit $LASTEXITCODE
        }
        'state' {
            cargo test --locked -p portus-state --all-targets
            exit $LASTEXITCODE
        }
        'integration' {
            [Console]::Error.WriteLine("test category 'integration' currently requires Linux/Artix")
            exit 78
        }
        'security-negative' {
            cargo test --locked -p portus-policy -p portus-privd -p portus-protected-api -p portus-apid -p portus-api -p portus-auth --all-targets
            exit $LASTEXITCODE
        }
        'build-contract' {
            cargo test --locked -p portus-build-contract --all-targets
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            cargo run --locked -q -p portus-build-contract -- .
            exit $LASTEXITCODE
        }
        'build-skeleton' {
            cargo test --locked -p portus-build --all-targets
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            cargo run --locked -q -p portus-build -- validate
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            cargo run --locked -q -p portus-build -- plan --disk-size-mib 40960
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            cargo run --locked -q -p portus-build -- build-iso
            if ($LASTEXITCODE -ne 78) {
                [Console]::Error.WriteLine("build-iso must fail closed with exit 78 until L2 resolves the native adapter")
                exit 1
            }
            exit 0
        }
        'hardening' {
            cargo test --locked -p portus-client -p portus-protocol -p portus-state -p portus-provider -p portus-index -p portus-task -p portus-health -p portus-audit -p portus-policy -p portus-privd -p portus-protected-api -p portus-apid -p portus-api -p portus-auth -p portus-artifact -p portus-browser-integration -p portus-visual -p portus-install -p portusd -p portus-os --all-targets
            exit $LASTEXITCODE
        }
        'graphical-vm' {
            cargo test --locked -p portus-build validation_harness
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            cargo run --locked -q -p portus-build -- validation-harness-check
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            cargo run --locked -q -p portus-build -- validation-vm-run
            if ($LASTEXITCODE -ne 78) {
                [Console]::Error.WriteLine("validation-vm-run must fail closed with exit 78 until Track V provides the VMware adapter")
                exit 1
            }
            exit 0
        }
        'oss' {
            python -B scripts/oss/test_oss.py
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            python -B scripts/release/test_signing.py
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            python -B scripts/oss/dependency_inventory.py --check
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            cargo audit -D warnings
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            cargo deny check advisories bans licenses sources
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            python -B scripts/oss/audit_repo.py --scope current --strict
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            python -B scripts/oss/audit_repo.py --scope history --strict
            exit $LASTEXITCODE
        }
        'all' {
            cargo test --locked --workspace --all-targets
            exit $LASTEXITCODE
        }
        default {
            [Console]::Error.WriteLine('usage: scripts/test.ps1 {unit|protocol|state|integration|security-negative|build-contract|build-skeleton|hardening|graphical-vm|oss|all}')
            exit 64
        }
    }
}
finally {
    Pop-Location
}
