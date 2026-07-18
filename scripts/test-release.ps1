$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

$required = @(
    "installer\SpeakTypeCloud.nsi",
    "installer\cyclonedx-minimal.schema.json",
    "installer\release-provenance.json",
    "scripts\build-installer.ps1",
    "scripts\generate-sbom.ps1",
    "scripts\install-nsis-build-tool.ps1",
    "scripts\sign-artifacts.ps1",
    "scripts\new-app-icon.ps1"
)
foreach ($relative in $required) {
    Assert-True (Test-Path -LiteralPath (Join-Path $root $relative)) "Missing release file: $relative"
}

$parseTargets = Get-ChildItem -LiteralPath (Join-Path $root "scripts") -Filter "*.ps1" -File
foreach ($file in $parseTargets) {
    $tokens = $null
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile(
        $file.FullName,
        [ref]$tokens,
        [ref]$errors
    )
    Assert-True ($errors.Count -eq 0) "PowerShell syntax errors in $($file.Name): $errors"
}

$nsi = Get-Content -LiteralPath (Join-Path $root "installer\SpeakTypeCloud.nsi") -Raw
foreach ($needle in @("WriteUninstaller", "CreateShortcut", "UninstallString", "RequestExecutionLevel user")) {
    Assert-True $nsi.Contains($needle) "NSIS template missing: $needle"
}
Assert-True $nsi.Contains('DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "SpeakType Cloud"') "NSIS uninstaller must remove only the app-owned startup value"
Assert-True (-not [regex]::IsMatch($nsi, '(?m)^SetShellVarContext\s')) "SetShellVarContext is not valid at NSIS top level"
Assert-True ([regex]::Matches($nsi, '(?m)^\s{2}SetShellVarContext current\r?$').Count -eq 2) "Installer and uninstaller must both use the current-user shell context"

$workflow = Get-Content -LiteralPath (Join-Path $root ".github\workflows\windows.yml") -Raw
foreach ($needle in @("build-installer.ps1", "generate-sbom.ps1", "upload-artifact")) {
    Assert-True $workflow.Contains($needle) "Windows workflow missing release step: $needle"
}
Assert-True ([regex]::IsMatch($workflow, '(?m)^permissions:\r?\n  contents: read\r?$')) "Workflow must default to read-only contents permission"
Assert-True (-not $workflow.Contains("Select-Object -Single")) "Workflow must validate installer count explicitly"
Assert-True $workflow.Contains("install-nsis-build-tool.ps1") "Workflow must use the hash-pinned NSIS installer"
Assert-True (-not $workflow.Contains("choco install nsis")) "Workflow must not install NSIS directly from a feed"
Assert-True $workflow.Contains("release-provenance.json") "Workflow artifacts must include release provenance"
$unsignedStart = $workflow.IndexOf("unsigned-release-artifacts:")
$signedStart = $workflow.IndexOf("signed-tag-artifacts:")
Assert-True ($unsignedStart -ge 0 -and $signedStart -gt $unsignedStart) "Workflow release jobs are missing or out of order"
$unsignedBlock = $workflow.Substring($unsignedStart, $signedStart - $unsignedStart)
Assert-True (-not $unsignedBlock.Contains("update-manifest.json")) "Unsigned CI artifacts must never contain an auto-update manifest"
Assert-True ($workflow.IndexOf("Sign installer") -lt $workflow.IndexOf("Generate manifest only after installer signing")) "Signed workflow must sign the installer before generating its manifest"
$provenance = Get-Content -LiteralPath (Join-Path $root "installer\release-provenance.json") -Raw | ConvertFrom-Json
Assert-True ($provenance.schema_version -eq 1) "Release provenance schema version is invalid"
$nsisPackages = @($provenance.build_tools[0].packages)
Assert-True ($provenance.build_tools[0].version -eq "3.12.0") "Release provenance must pin NSIS 3.12.0"
Assert-True ($nsisPackages.Count -eq 2) "Release provenance must pin the NSIS package and its install dependency"
foreach ($package in $nsisPackages) {
    Assert-True ($package.expected_sha256 -match '^[0-9a-f]{64}$') "NSIS expected SHA-256 is invalid"
    Assert-True ($package.audited_actual_sha256 -match '^[0-9a-f]{64}$') "NSIS audited actual SHA-256 is invalid"
    Assert-True ($package.expected_sha256 -eq $package.audited_actual_sha256) "NSIS expected and audited SHA-256 differ"
    Assert-True ($package.package_url -match '^https://community\.chocolatey\.org/api/v2/package/[^/]+/3\.12\.0$') "NSIS package URL is not fixed to version 3.12.0"
}
$nsisInstallScript = Get-Content -LiteralPath (Join-Path $root "scripts\install-nsis-build-tool.ps1") -Raw
$hashCheckIndex = $nsisInstallScript.IndexOf("Get-FileHash")
$installIndex = $nsisInstallScript.IndexOf("choco install")
Assert-True ($hashCheckIndex -ge 0 -and $installIndex -gt $hashCheckIndex) "NSIS hash verification must precede installation"
$usesLines = @($workflow -split "`r?`n" | Where-Object { $_ -match '^\s*-?\s*uses:' })
foreach ($line in $usesLines) {
    Assert-True ($line -match '@[0-9a-f]{40}(?:\s+#\s+.+)?$') "GitHub Action is not pinned to a full commit SHA: $line"
}

Write-Host "Release engineering static tests passed."
