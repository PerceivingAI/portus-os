Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    cargo check --locked --workspace --all-targets
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    cargo clippy --locked --workspace --all-targets -- -D warnings
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
