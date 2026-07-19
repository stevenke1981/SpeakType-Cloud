$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$stage = Join-Path $root "dist\package-staging-$PID-$([Guid]::NewGuid().ToString('N'))"
$zip = "$root\dist\SpeakTypeCloud-portable.zip"
$temporaryZip = Join-Path $root "dist\package-$PID-$([Guid]::NewGuid().ToString('N')).zip"
$backupZip = "$temporaryZip.backup"
$mutex = [Threading.Mutex]::new($false, "Local\SpeakTypeCloudPortablePackage")
$ownsMutex = $false
try {
    try {
        $ownsMutex = $mutex.WaitOne(0)
    }
    catch [Threading.AbandonedMutexException] {
        $ownsMutex = $true
    }
    if (-not $ownsMutex) { throw "Another portable packaging process is already running." }

    & "$PSScriptRoot\build-release.ps1"
    New-Item -ItemType Directory -Force -Path "$stage\docs" | Out-Null
    Copy-Item "$root\dist\release\SpeakTypeCloud.exe" "$stage\SpeakTypeCloud.exe"
    Copy-Item "$root\dist\release\SpeakTypeCloud.pdb" "$stage\SpeakTypeCloud.pdb"
    Copy-Item "$root\README.md","$root\SECURITY.md","$root\API_PROVIDERS.md" "$stage\docs"
    @"
1. Run SpeakTypeCloud.exe.
2. Save an OpenAI, xAI, or OpenRouter API key from the app's Settings page.
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

    Compress-Archive -Path "$stage\*" -DestinationPath $temporaryZip
    $archive = [IO.Compression.ZipFile]::OpenRead($temporaryZip)
    try {
        if ($archive.Entries.Count -eq 0) { throw "Portable archive is empty." }
    }
    finally {
        $archive.Dispose()
    }

    if (Test-Path -LiteralPath $zip -PathType Leaf) {
        [IO.File]::Replace($temporaryZip, $zip, $backupZip, $true)
    }
    else {
        [IO.File]::Move($temporaryZip, $zip)
    }
    Write-Host "Packaged: $zip"
}
finally {
    foreach ($temporaryPath in @($stage, $temporaryZip, $backupZip)) {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath -Recurse -Force -ErrorAction SilentlyContinue
            if (Test-Path -LiteralPath $temporaryPath) {
                Write-Warning "Temporary packaging path could not be removed: $temporaryPath"
            }
        }
    }
    if ($ownsMutex) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
