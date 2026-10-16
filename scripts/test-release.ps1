#Requires -Version 7.2
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
function Assert-True([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
$required = @(
    'installer\SpeakTypeCloud.iss', 'installer\cyclonedx-minimal.schema.json',
    'installer\release-provenance.json', 'scripts\build-installer.ps1',
    'scripts\generate-sbom.ps1', 'scripts\install-inno-build-tool.ps1',
    'scripts\sign-artifacts.ps1', 'scripts\new-app-icon.ps1',
    'scripts\release-common.ps1', 'scripts\release-metadata.ps1',
    'scripts\stage-release.ps1', 'scripts\publish-release.ps1', 'scripts\test-release-behavior.ps1'
)
foreach ($relative in $required) { Assert-True (Test-Path -LiteralPath (Join-Path $root $relative)) "Missing release file: $relative" }
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $root 'scripts') -Filter '*.ps1' -File) {
    $tokens = $null
    $errors = $null
    [void][Management.Automation.Language.Parser]::ParseFile($file.FullName, [ref]$tokens, [ref]$errors)
    Assert-True ($errors.Count -eq 0) "PowerShell syntax errors in $($file.Name): $errors"
}
& (Join-Path $root 'scripts\test-inno-contract.ps1')
& (Join-Path $root 'scripts\build-installer.ps1') -ValidateOnly
$workflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\windows.yml') -Raw
foreach ($needle in @('check.ps1', 'build-installer.ps1 -SkipBuild', 'package-portable.ps1 -SkipBuild', 'generate-sbom.ps1', 'stage-release.ps1', 'publish-release.ps1', 'upload-artifact', 'download-artifact', 'workflow_dispatch:', 'needs: build', 'digest-mismatch: error', 'if-no-files-found: error')) {
    Assert-True $workflow.Contains($needle) "Workflow missing gate: $needle"
}
Assert-True ([regex]::IsMatch($workflow, '(?m)^permissions:\r?\n  contents: read\r?$')) 'Workflow must default to read-only permissions'
$publishStart = $workflow.IndexOf("`n  publish:")
Assert-True ($publishStart -gt 0) 'Separate publishing job is required'
$buildBlock = $workflow.Substring(0, $publishStart)
Assert-True (-not $buildBlock.Contains('contents: write')) 'Build job must not have repository write permission'
Assert-True $workflow.Substring($publishStart).Contains('contents: write') 'Publishing job requires contents write permission'
Assert-True ($workflow.IndexOf('Sign installer') -lt $workflow.IndexOf('Generate manifest only after installer signing')) 'Installer must be signed before its manifest is generated'
Assert-True $workflow.Contains("if: steps.metadata.outputs.signed == 'true'") 'Signing must be restricted to trusted release contexts'
Assert-True $workflow.Contains('install-inno-build-tool.ps1') 'Use the hash-pinned Inno installer'
Assert-True (-not $workflow.Contains('choco install nsis')) 'Do not install unverified NSIS directly from a feed'
Assert-True (-not $workflow.Contains('pull_request_target:')) 'Do not execute contributor code with privileged PR events'
Assert-True (-not (Test-Path -LiteralPath (Join-Path $root '.github\workflows\ci.yml'))) 'The duplicate full-build workflow must stay consolidated'
foreach ($yaml in Get-ChildItem -LiteralPath (Join-Path $root '.github\workflows') -File) {
    foreach ($line in Get-Content -LiteralPath $yaml.FullName | Where-Object { $_ -match '^\s*-?\s*uses:' }) {
        Assert-True ($line -match '@[0-9a-f]{40}(?:\s+#\s+.+)?$') "Action must be pinned to a full SHA: $line"
    }
}
$provenance = Get-Content -LiteralPath (Join-Path $root 'installer\release-provenance.json') -Raw | ConvertFrom-Json
Assert-True ($provenance.schema_version -eq 1) 'Invalid provenance schema'
$tool = $provenance.build_tools[0]
Assert-True ($tool.name -eq 'Inno Setup' -and $tool.version -eq '7.1.0') 'Pin audited Inno Setup 7.1.0'
Assert-True ($tool.expected_sha256 -match '^[0-9a-f]{64}$' -and $tool.expected_sha256 -eq $tool.audited_actual_sha256) 'Invalid Inno provenance hashes'
Assert-True ($tool.package_url -eq 'https://github.com/jrsoftware/issrc/releases/download/is-7_1_0/innosetup-7.1.0-x64.exe') 'Only audited official Inno package allowed'
$innoScript = Get-Content -LiteralPath (Join-Path $root 'scripts\install-inno-build-tool.ps1') -Raw
Assert-True ($innoScript.IndexOf('Get-FileHash') -ge 0 -and $innoScript.IndexOf('Start-Process') -gt $innoScript.IndexOf('Get-FileHash')) 'Verify compiler hash before execution'
$stage = Get-Content -LiteralPath (Join-Path $root 'scripts\stage-release.ps1') -Raw
Assert-True $stage.Contains('release-provenance.json') 'Release assets must include build tool provenance'
Assert-True $stage.Contains('if ($Signed)') 'Updater manifest must be included only in signed releases'
Write-Host 'Release engineering static tests passed.'
