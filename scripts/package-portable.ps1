$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
& "$PSScriptRoot\build-release.ps1"
$stage = "$root\dist\SpeakTypeCloud-portable"
$zip = "$root\dist\SpeakTypeCloud-portable.zip"
if (Test-Path -LiteralPath $stage) {
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction Stop
}
if (Test-Path -LiteralPath $zip) {
    Remove-Item -LiteralPath $zip -Force -ErrorAction Stop
}
New-Item -ItemType Directory -Force -Path "$stage\docs" | Out-Null
Copy-Item "$root\dist\release\SpeakTypeCloud.exe" "$stage\SpeakTypeCloud.exe"
Copy-Item "$root\README.md","$root\SECURITY.md","$root\API_PROVIDERS.md" "$stage\docs"
@"
1. Set OPENAI_API_KEY or XAI_API_KEY as a User environment variable.
2. Run SpeakTypeCloud.exe.
3. Hold Ctrl+Shift+Space while speaking, then release.
"@ | Set-Content "$stage\QUICKSTART.txt" -Encoding UTF8

$forbidden = Get-ChildItem -LiteralPath $stage -Recurse -File | Where-Object {
    $_.Name -eq "config.toml" -or
    $_.Name -eq ".env" -or
    $_.Name -like "history*.jsonl" -or
    $_.Extension -in ".wav", ".log"
}
if ($forbidden) {
    throw "Portable staging contains prohibited runtime data: $($forbidden.FullName -join ', ')"
}

Compress-Archive -Path "$stage\*" -DestinationPath $zip
Write-Host "Packaged: $zip"
