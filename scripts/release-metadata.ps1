#Requires -Version 7.2
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'release-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
$version = Get-ReleaseVersion (Join-Path $root 'Cargo.toml')
$decision = Get-ReleaseDecision -Version $version -EventName $env:GITHUB_EVENT_NAME -Ref $env:GITHUB_REF -DefaultBranch $env:DEFAULT_BRANCH -SignerPin $env:UPDATE_SIGNER_CERT_SHA256
if (-not $env:GITHUB_OUTPUT -or -not $env:GITHUB_ENV) { throw 'This script must run inside GitHub Actions' }
@("version=$version", "tag=$($decision.Tag)", "publish=$($decision.Publish.ToString().ToLowerInvariant())", "signed=$($decision.Signed.ToString().ToLowerInvariant())") | Add-Content -LiteralPath $env:GITHUB_OUTPUT -Encoding utf8
$pin = if ($decision.Signed) { $env:UPDATE_SIGNER_CERT_SHA256.ToLowerInvariant() } else { '' }
"SPEAKTYPE_UPDATE_SIGNER_CERT_SHA256=$pin" | Add-Content -LiteralPath $env:GITHUB_ENV -Encoding utf8
Write-Host "Version=$version; publish=$($decision.Publish); signed=$($decision.Signed)"
