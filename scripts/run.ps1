$ErrorActionPreference = "Stop"
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    & cargo run
    if ($LASTEXITCODE -ne 0) { throw "cargo run failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}
