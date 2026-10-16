#Requires -Version 7.2
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$template = Get-Content -LiteralPath "$root\installer\SpeakTypeCloud.iss" -Raw
foreach ($required in @('PrivilegesRequired=lowest', 'AppMutex=Local\SpeakTypeCloud_SingleInstance',
    'DefaultDirName={localappdata}\Programs\SpeakType Cloud Slint',
    'Name: "{autodesktop}', 'Name: "{group}', 'slint-royalty-free-2.0.pdf',
    'function PrepareToInstall', 'LegacyUninstallKey', 'RegKeyExists',
    'function InitializeUninstall', 'StartupMatchesInstalledApp')) {
    if (-not $template.Contains($required)) { throw "Inno contract missing: $required" }
}
foreach ($forbidden in @('[InstallDelete]', '[UninstallDelete]', 'DelTree(', 'UninstallString', 'Exec(')) {
    if ($template.Contains($forbidden)) { throw "Unsafe migration directive: $forbidden" }
}
Write-Host 'Inno conservative migration and data preservation contracts passed.'
