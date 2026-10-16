#Requires -Version 7.2
[CmdletBinding()]
param([Parameter(Mandatory)][string]$IsccPath, [Parameter(Mandatory)][string]$EvidenceRoot,
    [string]$FixtureId = ('qa-' + [Guid]::NewGuid().ToString('N').Substring(0, 10)), [switch]$KeepInstalled)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if ($FixtureId -cnotmatch '^qa-[a-z0-9]{10}$') { throw 'Use a unique QA fixture id' }
$evidence = [IO.Path]::GetFullPath((Join-Path $EvidenceRoot $FixtureId))
if (Test-Path -LiteralPath $evidence) { throw 'Refusing to reuse existing fixture' }
[IO.Directory]::CreateDirectory($evidence) | Out-Null
$app = Join-Path $evidence 'Application With Spaces'
$legacy = Join-Path $evidence 'Legacy Custom Path'
$profile = Join-Path $evidence 'Synthetic Profile'
[IO.Directory]::CreateDirectory($legacy) | Out-Null
[IO.Directory]::CreateDirectory($profile) | Out-Null
foreach ($name in @('config.toml', 'history.jsonl', 'synthetic.wav', 'fake-credential.txt')) {
    [IO.File]::WriteAllText((Join-Path $profile $name), "SYNTHETIC acceptance sentinel $FixtureId $name")
}
$before = @(Get-ChildItem -LiteralPath $profile -File | Get-FileHash -Algorithm SHA256 | Select-Object Path,Hash)
$legacyKey = "Software\SpeakTypeCloud.Fixture.$FixtureId"
$legacyUninstallKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\SpeakTypeCloud.LegacyFixture.$FixtureId"
$ownKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\SpeakTypeCloud.Fixture.${FixtureId}_is1"
$reg32 = [Microsoft.Win32.RegistryKey]::OpenBaseKey('CurrentUser', 'Registry32')
$reg64 = [Microsoft.Win32.RegistryKey]::OpenBaseKey('CurrentUser', 'Registry64')
foreach ($key in @($legacyKey, $legacyUninstallKey, $ownKey)) {
    foreach ($view in @($reg32, $reg64)) {
        $existing = $view.OpenSubKey($key)
        if ($existing) { $existing.Dispose(); throw 'Fixture registry already exists; refusing reuse' }
    }
}
& "$PSScriptRoot\build-installer.ps1" -SkipBuild -IsccPath $IsccPath -FixtureId $FixtureId
$installer = Join-Path $root "dist\installer-fixtures\SpeakTypeCloud-Fixture-$FixtureId.exe"
function Invoke-FixtureSetup([string]$Label) {
    $process = Start-Process -FilePath $installer -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',
        '/CURRENTUSER','/TASKS=desktopicon',"/DIR=`"$app`"", "/LOG=`"$evidence\$Label.log`"") -WindowStyle Hidden -Wait -PassThru
    return $process.ExitCode
}
function Assert-Sentinels {
    foreach ($entry in $before) {
        if ((Get-FileHash -LiteralPath $entry.Path -Algorithm SHA256).Hash -ne $entry.Hash) { throw 'Synthetic user data changed' }
    }
}
$results = [Collections.Generic.List[string]]::new()
# Synthetic legacy metadata proves safe refusal; it does not claim an actual NSIS upgrade.
[IO.File]::WriteAllText("$legacy\SpeakTypeCloud.exe", 'Synthetic legacy executable marker')
[IO.File]::WriteAllText("$legacy\Uninstall.exe", 'Synthetic legacy uninstaller marker; never execute')
$key = $reg32.CreateSubKey($legacyKey); $key.SetValue('InstallDir', $legacy); $key.Dispose()
$key = $reg32.CreateSubKey($legacyUninstallKey); $key.SetValue('DisplayName', "Legacy Fixture $FixtureId"); $key.Dispose()
try {
    $code = Invoke-FixtureSetup 'legacy-registration-block'
    if ($code -eq 0 -or (Test-Path -LiteralPath "$app\SpeakTypeCloud.exe")) { throw 'Legacy registration failed to block installation' }
    $results.Add("PASS legacy registration block: exit=$code")
    $reg32.DeleteSubKey($legacyUninstallKey)
    $code = Invoke-FixtureSetup 'legacy-remnants-block'
    if ($code -eq 0 -or (Test-Path -LiteralPath "$app\SpeakTypeCloud.exe")) { throw 'Legacy remnants failed to block installation' }
    $results.Add("PASS remembered custom-path remnants block: exit=$code")
    # Dispose only this test's two synthetic markers and test registration.
    Remove-Item -LiteralPath "$legacy\SpeakTypeCloud.exe", "$legacy\Uninstall.exe"
    $reg32.DeleteSubKey($legacyKey)
    $code = Invoke-FixtureSetup 'fresh-install'
    if ($code -ne 0) { throw "Fresh installation failed: $code" }
    foreach ($file in @('SpeakTypeCloud.exe','unins000.exe','THIRD_PARTY_NOTICES.md','licenses\slint-royalty-free-2.0.pdf')) {
        if (-not (Test-Path -LiteralPath (Join-Path $app $file))) { throw "Installed file missing: $file" }
    }
    if ((Get-FileHash "$app\SpeakTypeCloud.exe").Hash -ne (Get-FileHash "$root\dist\release\SpeakTypeCloud.exe").Hash) { throw 'Installed executable hash mismatch' }
    $registration = $reg64.OpenSubKey($ownKey)
    if (-not $registration -or $registration.GetValue('InstallLocation').TrimEnd('\') -ne $app) { throw 'Inno registration missing or wrong path' }
    $registration.Dispose()
    $shell = New-Object -ComObject WScript.Shell
    $desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) "SpeakType Cloud Fixture $FixtureId.lnk"
    $start = Join-Path ([Environment]::GetFolderPath('Programs')) "SpeakType Cloud Fixture $FixtureId\SpeakType Cloud Fixture $FixtureId.lnk"
    foreach ($path in @($desktop, $start)) {
        if (-not (Test-Path -LiteralPath $path)) { throw "Shortcut missing: $path" }
        $link = $shell.CreateShortcut($path)
        if ($link.TargetPath -ne "$app\SpeakTypeCloud.exe" -or $link.Arguments -ne '--demo') { throw 'Fixture shortcut must launch only isolated demo' }
    }
    $results.Add('PASS fresh install, installed hash, license files, current-user shortcuts and registration')
    $code = Invoke-FixtureSetup 'repair-install'
    if ($code -ne 0) { throw "Repair/re-run failed: $code" }
    $results.Add('PASS repair/re-run')
    Assert-Sentinels
    if (-not $KeepInstalled) {
        $process = Start-Process -FilePath "$app\unins000.exe" -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',"/LOG=`"$evidence\uninstall.log`"") -WindowStyle Hidden -Wait -PassThru
        if ($process.ExitCode -ne 0 -or (Test-Path -LiteralPath "$app\SpeakTypeCloud.exe")) { throw 'Normal uninstall failed' }
        if ($reg64.OpenSubKey($ownKey)) { throw 'Uninstall registration remains' }
        foreach ($path in @($desktop, $start)) { if (Test-Path -LiteralPath $path) { throw 'Shortcut remains after uninstall' } }
        Assert-Sentinels
        $results.Add('PASS normal uninstall; synthetic profile hashes preserved')
    } else { $results.Add('PENDING native launch and normal uninstall; fixture intentionally kept installed') }
    [ordered]@{fixture=$FixtureId;app=$app;installer=$installer;results=$results.ToArray();sentinels=$before} |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$evidence\evidence.json" -Encoding utf8
    $results | Write-Host
    Write-Host "Evidence: $evidence"
} finally {
    # Only isolated registry keys created above. Preserve installed fixture for diagnosis on failure.
    $reg32.DeleteSubKey($legacyUninstallKey, $false)
    $reg32.DeleteSubKey($legacyKey, $false)
    $reg32.Dispose(); $reg64.Dispose()
}
