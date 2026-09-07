[CmdletBinding()]
param([switch]$SkipBuild, [switch]$IncludeSymbols)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$stage = Join-Path $root "dist\package-staging-$PID-$([Guid]::NewGuid().ToString('N'))"
$zip = "$root\dist\SpeakTypeCloud-portable.zip"
$temporaryZip = Join-Path $root "dist\package-$PID-$([Guid]::NewGuid().ToString('N')).zip"
$backupZip = "$temporaryZip.backup"
$mutex = [Threading.Mutex]::new($false, 'Local\SpeakTypeCloudPortablePackage')
$ownsMutex = $false
try {
    try { $ownsMutex = $mutex.WaitOne(0) }
    catch [Threading.AbandonedMutexException] { $ownsMutex = $true }
    if (-not $ownsMutex) { throw 'Another portable packaging process is already running.' }

    # Never rebuild an executable that has already been signed by the release job.
    if (-not $SkipBuild) { & (Join-Path $PSScriptRoot 'build-release.ps1') }
    $exe = Join-Path $root 'dist\release\SpeakTypeCloud.exe'
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'Staged release executable is missing' }
    New-Item -ItemType Directory -Force -Path "$stage\docs" | Out-Null
    Copy-Item -LiteralPath $exe -Destination "$stage\SpeakTypeCloud.exe"
    Copy-Item -LiteralPath "$root\LICENSE" -Destination "$stage\LICENSE"
    foreach ($doc in @('README.md', 'SECURITY.md', 'API_PROVIDERS.md')) {
        Copy-Item -LiteralPath (Join-Path $root $doc) -Destination "$stage\docs"
    }
    $icon = Join-Path $root 'dist\release\SpeakTypeCloud.ico'
    if (Test-Path -LiteralPath $icon) { Copy-Item -LiteralPath $icon -Destination "$stage\SpeakTypeCloud.ico" }
    if ($IncludeSymbols) {
        $pdb = Join-Path $root 'dist\release\SpeakTypeCloud.pdb'
        if (-not (Test-Path -LiteralPath $pdb -PathType Leaf)) { throw 'Debug symbols were requested but the PDB is missing' }
        Copy-Item -LiteralPath $pdb -Destination "$stage\SpeakTypeCloud.pdb"
    }
    @'
1. Extract the entire ZIP and run SpeakTypeCloud.exe.
2. Save your provider API key using the API Key button in the app.
3. Focus a text field, hold Ctrl+Shift+Space, speak, then release.
4. Configuration, history and credentials still use the Windows user profile.
   This is a no-install distribution, not a fully self-contained data profile.
5. See docs/README.md and docs/SECURITY.md for privacy and update limitations.
'@ | Set-Content -LiteralPath "$stage\QUICKSTART.txt" -Encoding utf8

    $forbidden = @(Get-ChildItem -LiteralPath $stage -Recurse -File | Where-Object {
        $_.Name -eq 'config.toml' -or $_.Name -like '.env*' -or
        $_.Name -like 'history*.jsonl' -or $_.Extension -in '.wav', '.log', '.pfx', '.p12'
    })
    if ($forbidden.Count) { throw 'Portable staging contains prohibited runtime data or secrets' }
    Compress-Archive -Path "$stage\*" -DestinationPath $temporaryZip
    $archive = [IO.Compression.ZipFile]::OpenRead($temporaryZip)
    try {
        $names = @($archive.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
        foreach ($required in @('SpeakTypeCloud.exe', 'LICENSE', 'QUICKSTART.txt', 'docs/README.md')) {
            if ($names -cnotcontains $required) { throw "Portable archive is missing: $required" }
        }
    } finally { $archive.Dispose() }
    if (Test-Path -LiteralPath $zip -PathType Leaf) { [IO.File]::Replace($temporaryZip, $zip, $backupZip, $true) }
    else { [IO.File]::Move($temporaryZip, $zip) }
    Write-Host "Packaged: $zip"
} finally {
    foreach ($temporaryPath in @($stage, $temporaryZip, $backupZip)) {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath -Recurse -Force -ErrorAction SilentlyContinue
            if (Test-Path -LiteralPath $temporaryPath) { Write-Warning "Temporary packaging path could not be removed: $temporaryPath" }
        }
    }
    if ($ownsMutex) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
