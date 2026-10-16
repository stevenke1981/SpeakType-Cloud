#Requires -Version 7.2
param([string]$TestRoot = [IO.Path]::GetTempPath())
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    & (Join-Path $PSScriptRoot 'test-release.ps1')
    & (Join-Path $PSScriptRoot 'test-release-behavior.ps1') -TestRoot $TestRoot

    & cargo fmt --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt --check failed with exit code $LASTEXITCODE" }
    & cargo clippy --locked --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed with exit code $LASTEXITCODE" }
    & cargo test --locked --all-targets
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed with exit code $LASTEXITCODE" }
    & cargo test --locked --all-targets --no-default-features --features legacy-ui
    if ($LASTEXITCODE -ne 0) { throw "legacy comparison tests failed with exit code $LASTEXITCODE" }
    & cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}
