Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    cargo build --locked --workspace
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
