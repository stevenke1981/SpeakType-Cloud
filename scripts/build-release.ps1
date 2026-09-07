[CmdletBinding()]
param([switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (-not $SkipBuild) {
        & cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw "cargo build --release --locked failed with exit code $LASTEXITCODE" }
    }
    $source = Join-Path $root 'target\release\speaktype-cloud.exe'
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw 'Release executable is missing; run a release build first' }
    $destination = Join-Path $root 'dist\release'
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    Copy-Item -LiteralPath $source -Destination (Join-Path $destination 'SpeakTypeCloud.exe') -Force
    $pdb = Join-Path $root 'target\release\speaktype_cloud.pdb'
    $stagedPdb = Join-Path $destination 'SpeakTypeCloud.pdb'
    if (Test-Path -LiteralPath $pdb -PathType Leaf) {
        Copy-Item -LiteralPath $pdb -Destination $stagedPdb -Force
    } elseif (Test-Path -LiteralPath $stagedPdb) {
        Remove-Item -LiteralPath $stagedPdb -Force
    }
    Write-Host "Staged: $destination\SpeakTypeCloud.exe"
} finally {
    Pop-Location
}
