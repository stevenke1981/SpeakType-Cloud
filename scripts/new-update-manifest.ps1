#Requires -Version 7.2
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$InstallerPath,
    [string]$Version = '',
    [string]$Repository = 'stevenke1981/SpeakType-Cloud',
    [string]$OutputPath = '',
    [string]$ExpectedSignerCertSha256 = $env:SPEAKTYPE_UPDATE_SIGNER_CERT_SHA256
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'release-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
$InstallerPath = [IO.Path]::GetFullPath($InstallerPath)
if (-not (Test-Path -LiteralPath $InstallerPath -PathType Leaf)) { throw "Installer not found: $InstallerPath" }
if ([string]::IsNullOrWhiteSpace($Version)) { $Version = Get-ReleaseVersion (Join-Path $root 'Cargo.toml') }
Assert-ReleaseVersion $Version
if ($Repository -cnotmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw 'Invalid GitHub repository' }
if ([IO.Path]::GetFileName($InstallerPath) -cne "SpeakTypeCloud-Setup-$Version.exe") { throw 'Installer filename must match the release version' }
# Enforce this even for manual script calls, not only in the workflow.
Assert-SignedReleaseFile -Path $InstallerPath -ExpectedSignerCertSha256 $ExpectedSignerCertSha256
if ([string]::IsNullOrWhiteSpace($OutputPath)) { $OutputPath = Join-Path (Split-Path -Parent $InstallerPath) 'update-manifest.json' }
$hash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLowerInvariant()
$fileName = [Uri]::EscapeDataString([IO.Path]::GetFileName($InstallerPath))
$manifest = [ordered]@{
    schema_version = 1
    version = $Version
    installer_url = "https://github.com/$Repository/releases/download/v$Version/$fileName"
    sha256 = $hash
}
$OutputPath = [IO.Path]::GetFullPath($OutputPath)
[IO.Directory]::CreateDirectory((Split-Path -Parent $OutputPath)) | Out-Null
[IO.File]::WriteAllText($OutputPath, ($manifest | ConvertTo-Json) + "`n", [Text.UTF8Encoding]::new($false))
Write-Host "Created signed update manifest: $OutputPath"
