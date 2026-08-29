Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    python -B scripts/build_iteration.py @args
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
