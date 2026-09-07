#Requires -Version 7.2
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Version, [switch]$Signed)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'release-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
Assert-ReleaseVersion $Version
if ($Version -cne (Get-ReleaseVersion (Join-Path $root 'Cargo.toml'))) { throw 'Release version differs from Cargo.toml' }
if ($env:GITHUB_SHA -cnotmatch '^[0-9a-f]{40}$') { throw 'A full source commit SHA is required' }
$destination = Join-Path $root 'dist\publish'
if (Test-Path -LiteralPath $destination) { throw 'dist/publish already exists; use a clean workspace for release staging' }
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$installer = Join-Path $root "dist\installer\SpeakTypeCloud-Setup-$Version.exe"
$inputs = @(
    $installer,
    (Join-Path $root 'dist\SpeakTypeCloud-portable.zip'),
    (Join-Path $root 'dist\sbom\SpeakTypeCloud.cdx.json'),
    (Join-Path $root 'installer\release-provenance.json')
)
if ($Signed) {
    Assert-SignedReleaseFile $installer $env:SPEAKTYPE_UPDATE_SIGNER_CERT_SHA256
    $inputs += Join-Path $root 'dist\installer\update-manifest.json'
}
foreach ($source in $inputs) {
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing release input: $source" }
    Copy-Item -LiteralPath $source -Destination $destination
}
$rustVersion = & rustc --version
if ($LASTEXITCODE -ne 0) { throw 'rustc --version failed' }
$info = [ordered]@{
    schema_version = 1
    version = $Version
    source_sha = $env:GITHUB_SHA
    platform = 'windows-x64'
    rust = [string]$rustVersion
    signed = [bool]$Signed
    signer_cert_sha256 = $(if ($Signed) { $env:SPEAKTYPE_UPDATE_SIGNER_CERT_SHA256.ToLowerInvariant() } else { $null })
    workflow_run = "$env:GITHUB_SERVER_URL/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID"
}
[IO.File]::WriteAllText((Join-Path $destination 'build-info.json'), ($info | ConvertTo-Json) + "`n", [Text.UTF8Encoding]::new($false))
Write-ReleaseChecksums $destination
Test-ReleaseChecksums $destination
Write-Host "Verified release assets: $destination"
