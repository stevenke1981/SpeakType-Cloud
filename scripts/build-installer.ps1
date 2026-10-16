#Requires -Version 7.2
[CmdletBinding()]
param([string]$IsccPath = '', [switch]$SkipBuild, [switch]$ValidateOnly, [string]$FixtureId = '')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'release-common.ps1')
$template = Join-Path $root 'installer\SpeakTypeCloud.iss'
& (Join-Path $PSScriptRoot 'test-inno-contract.ps1')
if ($ValidateOnly) { return }
if ($FixtureId -and $FixtureId -cnotmatch '^[a-z0-9-]{1,32}$') { throw 'Invalid isolated fixture id' }
if (-not $SkipBuild) { & (Join-Path $PSScriptRoot 'build-release.ps1') }
$appExe = Join-Path $root 'dist\release\SpeakTypeCloud.exe'
if (-not (Test-Path -LiteralPath $appExe -PathType Leaf)) { throw 'Staged application is missing' }
& (Join-Path $PSScriptRoot 'check-runtime.ps1') -Executable $appExe | Write-Host
& (Join-Path $PSScriptRoot 'new-app-icon.ps1') -OutputPath "$root\dist\release\SpeakTypeCloud.ico"
if (-not $IsccPath) { $IsccPath = $env:SPEAKTYPE_ISCC }
if (-not $IsccPath) {
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { $IsccPath = $command.Source }
}
if (-not $IsccPath -or -not (Test-Path -LiteralPath $IsccPath -PathType Leaf)) {
    throw 'Inno compiler not found. Run scripts/install-inno-build-tool.ps1 or supply -IsccPath.'
}
$compilerVersion = & $IsccPath --version
if ($LASTEXITCODE -ne 0 -or "$compilerVersion".Trim() -ne '7.1.0') { throw 'Expected audited Inno Setup 7.1.0' }
$version = Get-ReleaseVersion "$root\Cargo.toml"
$output = Join-Path $root 'dist\installer'
$name = "SpeakTypeCloud-Setup-$version"
if ($FixtureId) { $output = Join-Path $root 'dist\installer-fixtures'; $name = "SpeakTypeCloud-Fixture-$FixtureId" }
[IO.Directory]::CreateDirectory($output) | Out-Null
$arguments = @('--no-ide-signtools', '--messages-jsonl', "--output-dir=$output", "--output-filename=$name",
    "--define=AppExe=$appExe", "--define=RepoRoot=$root", "--define=Version=$version")
if ($FixtureId) { $arguments += "--define=FixtureId=$FixtureId" }
$messages = & $IsccPath @arguments $template 2>&1
$code = $LASTEXITCODE
$messages | ForEach-Object { Write-Host $_ }
if ($code -ne 0) { throw "Inno compilation failed: $code" }
foreach ($line in $messages) {
    if ("$line".TrimStart().StartsWith('{')) {
        $record = "$line" | ConvertFrom-Json -AsHashtable
        if ($record['type'] -eq 'warning' -or $record['severity'] -eq 'warning') { throw "Inno compiler warning: $line" }
    }
}
if (-not (Test-Path -LiteralPath "$output\$name.exe" -PathType Leaf)) { throw 'Inno output missing' }
Write-Host "Created installer (not executed): $output\$name.exe"
