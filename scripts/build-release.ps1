$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed with exit code $LASTEXITCODE" }

    New-Item -ItemType Directory -Force -Path "$root\dist\release" | Out-Null
    Copy-Item "$root\target\release\speaktype-cloud.exe" "$root\dist\release\SpeakTypeCloud.exe" -Force
    Write-Host "Built: $root\dist\release\SpeakTypeCloud.exe"
} finally {
    Pop-Location
}
