[CmdletBinding()]
param(
    [string]$MakensisPath = "",
    [switch]$SkipBuild,
    [switch]$ValidateOnly
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$template = Join-Path $root "installer\SpeakTypeCloud.nsi"
$cargoToml = Join-Path $root "Cargo.toml"
foreach ($required in @($template, $cargoToml)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) { throw "Required file not found: $required" }
}

$templateText = Get-Content -LiteralPath $template -Raw
foreach ($requiredDirective in @("RequestExecutionLevel user", "WriteUninstaller", "CreateShortcut", "UninstallString")) {
    if (-not $templateText.Contains($requiredDirective)) { throw "Installer template is missing: $requiredDirective" }
}
if ($ValidateOnly) {
    Write-Host "Installer template validation passed."
    return
}

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot "build-release.ps1")
}

$appExe = Join-Path $root "dist\release\SpeakTypeCloud.exe"
if (-not (Test-Path -LiteralPath $appExe -PathType Leaf)) {
    throw "Release executable not found: $appExe. Run scripts\build-release.ps1 first."
}
$iconFile = Join-Path $root "dist\release\SpeakTypeCloud.ico"
& (Join-Path $PSScriptRoot "new-app-icon.ps1") -OutputPath $iconFile

$versionMatch = [regex]::Match((Get-Content -LiteralPath $cargoToml -Raw), '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"')
if (-not $versionMatch.Success) { throw "Unable to read package version from Cargo.toml" }
$version = $versionMatch.Groups[1].Value
$outputDir = Join-Path $root "dist\installer"
[System.IO.Directory]::CreateDirectory($outputDir) | Out-Null
$outputFile = Join-Path $outputDir "SpeakTypeCloud-Setup-$version.exe"

if ([string]::IsNullOrWhiteSpace($MakensisPath)) {
    $command = Get-Command makensis.exe -ErrorAction SilentlyContinue
    if ($command) { $MakensisPath = $command.Source }
}
if ([string]::IsNullOrWhiteSpace($MakensisPath)) {
    $candidates = @(
        (Join-Path $env:ProgramFiles "NSIS\makensis.exe"),
        (Join-Path ${env:ProgramFiles(x86)} "NSIS\makensis.exe")
    )
    $MakensisPath = $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } | Select-Object -First 1
}
if ([string]::IsNullOrWhiteSpace($MakensisPath) -or -not (Test-Path -LiteralPath $MakensisPath -PathType Leaf)) {
    throw "NSIS makensis.exe was not found. Install it with: winget install --id NSIS.NSIS -e (this script never runs the installer it builds)."
}

& $MakensisPath "/DAPP_EXE=$appExe" "/DICON_FILE=$iconFile" "/DOUT_FILE=$outputFile" "/DVERSION=$version" $template
if ($LASTEXITCODE -ne 0) { throw "makensis.exe failed with exit code $LASTEXITCODE" }
if (-not (Test-Path -LiteralPath $outputFile -PathType Leaf)) { throw "NSIS reported success but did not create: $outputFile" }
Write-Host "Created installer (not executed): $outputFile"

