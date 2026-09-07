#Requires -Version 7.2
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
function Assert-True([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
$required = @(
    'installer\SpeakTypeCloud.nsi', 'installer\cyclonedx-minimal.schema.json',
    'installer\release-provenance.json', 'scripts\build-installer.ps1',
    'scripts\generate-sbom.ps1', 'scripts\install-nsis-build-tool.ps1',
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
$nsi = Get-Content -LiteralPath (Join-Path $root 'installer\SpeakTypeCloud.nsi') -Raw
foreach ($needle in @('WriteUninstaller', 'CreateShortcut', 'UninstallString', 'RequestExecutionLevel user')) { Assert-True $nsi.Contains($needle) "NSIS template missing: $needle" }
Assert-True $nsi.Contains('DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "SpeakType Cloud"') 'Uninstaller must remove only the app-owned startup value'
Assert-True (-not [regex]::IsMatch($nsi, '(?m)^SetShellVarContext\s')) 'SetShellVarContext is not valid at NSIS top level'
Assert-True ([regex]::Matches($nsi, '(?m)^\s{2}SetShellVarContext current\r?$').Count -eq 2) 'Installer and uninstaller must use the current-user shell context'
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
Assert-True $workflow.Contains('install-nsis-build-tool.ps1') 'Use the hash-pinned NSIS installer'
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
Assert-True ($provenance.build_tools[0].version -eq '3.12.0') 'Release provenance must pin NSIS 3.12.0'
$packages = @($provenance.build_tools[0].packages)
Assert-True ($packages.Count -eq 2) 'NSIS and its install dependency must both be pinned'
foreach ($package in $packages) {
    Assert-True ($package.expected_sha256 -match '^[0-9a-f]{64}$' -and $package.audited_actual_sha256 -match '^[0-9a-f]{64}$') 'Invalid NSIS package hash'
    Assert-True ($package.expected_sha256 -eq $package.audited_actual_sha256) 'NSIS expected and audited hashes differ'
    Assert-True ($package.package_url -match '^https://community\.chocolatey\.org/api/v2/package/[^/]+/3\.12\.0$') 'NSIS URL must pin 3.12.0'
}
$nsisScript = Get-Content -LiteralPath (Join-Path $root 'scripts\install-nsis-build-tool.ps1') -Raw
Assert-True ($nsisScript.IndexOf('Get-FileHash') -ge 0 -and $nsisScript.IndexOf('choco install') -gt $nsisScript.IndexOf('Get-FileHash')) 'Verify NSIS hashes before installation'
$stage = Get-Content -LiteralPath (Join-Path $root 'scripts\stage-release.ps1') -Raw
Assert-True $stage.Contains('release-provenance.json') 'Release assets must include build tool provenance'
Assert-True $stage.Contains('if ($Signed)') 'Updater manifest must be included only in signed releases'
Write-Host 'Release engineering static tests passed.'
