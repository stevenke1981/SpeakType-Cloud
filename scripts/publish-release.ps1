#Requires -Version 7.2
[CmdletBinding()]
param([Parameter(Mandatory)][string]$AssetDirectory)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'release-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
$version = Get-ReleaseVersion (Join-Path $root 'Cargo.toml')
$decision = Get-ReleaseDecision -Version $version -EventName $env:GITHUB_EVENT_NAME -Ref $env:GITHUB_REF -DefaultBranch $env:DEFAULT_BRANCH
if (-not $decision.Publish) { throw 'Publishing is allowed only from the default branch or a matching version tag' }
$repository = $env:GITHUB_REPOSITORY
$sourceSha = $env:GITHUB_SHA
if ($repository -cnotmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' -or $sourceSha -cnotmatch '^[0-9a-f]{40}$' -or -not $env:GH_TOKEN) { throw 'Invalid GitHub release environment' }
$tag = $decision.Tag
$AssetDirectory = [IO.Path]::GetFullPath($AssetDirectory)
Test-ReleaseChecksums $AssetDirectory
$info = Get-Content -LiteralPath (Join-Path $AssetDirectory 'build-info.json') -Raw | ConvertFrom-Json
if ($info.schema_version -ne 1 -or $info.version -cne $version -or $info.source_sha -cne $sourceSha -or $info.platform -cne 'windows-x64' -or $info.signed -isnot [bool]) { throw 'Release provenance does not match this workflow commit' }
$allowed = @('SpeakTypeCloud-portable.zip', "SpeakTypeCloud-Setup-$version.exe", 'SpeakTypeCloud.cdx.json', 'release-provenance.json', 'build-info.json', 'SHA256SUMS.txt')
if ($info.signed) {
    $allowed += 'update-manifest.json'
    Assert-SignedReleaseFile (Join-Path $AssetDirectory "SpeakTypeCloud-Setup-$version.exe") $info.signer_cert_sha256
    $manifest = Get-Content -LiteralPath (Join-Path $AssetDirectory 'update-manifest.json') -Raw | ConvertFrom-Json
    $encodedInstaller = [Uri]::EscapeDataString("SpeakTypeCloud-Setup-$version.exe")
    $expectedUrl = "https://github.com/$repository/releases/download/$tag/$encodedInstaller"
    $installerHash = (Get-FileHash -LiteralPath (Join-Path $AssetDirectory "SpeakTypeCloud-Setup-$version.exe") -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($manifest.schema_version -ne 1 -or $manifest.version -cne $version -or $manifest.installer_url -cne $expectedUrl -or $manifest.sha256 -cne $installerHash) { throw 'Update manifest does not match the signed installer' }
}
$files = @(Get-ReleaseFiles $AssetDirectory)
if ($files.Count -ne $allowed.Count) { throw 'Unexpected release asset count' }
foreach ($file in $files) { if ($allowed -cnotcontains $file.Name) { throw "Unexpected release asset: $($file.Name)" } }

function Read-GitHub([string]$RelativePath) {
    $response = Invoke-WebRequest -Uri "https://api.github.com/repos/$repository/$RelativePath" -Headers @{
        Authorization = "Bearer $env:GH_TOKEN"
        Accept = 'application/vnd.github+json'
        'X-GitHub-Api-Version' = '2022-11-28'
    } -TimeoutSec 60 -MaximumRedirection 0 -SkipHttpErrorCheck
    if ($response.StatusCode -eq 404) { return $null }
    if ($response.StatusCode -ne 200) { throw "GitHub read failed with HTTP $($response.StatusCode); refusing to assume the release is absent" }
    return ($response.Content | ConvertFrom-Json)
}
function Invoke-Gh([string[]]$Arguments) {
    & gh @Arguments
    if ($LASTEXITCODE -ne 0) { throw "GitHub CLI failed with exit code $LASTEXITCODE" }
}
function Write-Summary([string]$Text) {
    Write-Host $Text
    if ($env:GITHUB_STEP_SUMMARY) { $Text | Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Encoding utf8 }
}

$encodedTag = [Uri]::EscapeDataString($tag)
$existing = Read-GitHub "releases/tags/$encodedTag"
if ($existing -and -not $existing.draft) {
    Write-Summary "Release $tag already exists and was not modified. Bump [package].version and update Cargo.lock to publish new code. $($existing.html_url)"
    return
}
if ($existing -and $existing.target_commitish -cne $sourceSha) { throw 'An existing draft belongs to a different source commit; refusing to overwrite it' }
$tagRef = Read-GitHub "git/ref/tags/$encodedTag"
if ($tagRef) {
    $object = $tagRef.object
    for ($depth = 0; $object.type -eq 'tag' -and $depth -lt 5; $depth++) {
        $annotated = Read-GitHub "git/tags/$($object.sha)"
        if (-not $annotated) { throw 'Cannot resolve annotated release tag' }
        $object = $annotated.object
    }
    if ($object.type -ne 'commit' -or $object.sha -cne $sourceSha) { throw 'Existing version tag points at a different commit; it will not be moved' }
}
$prerelease = (($version -split '\+', 2)[0]).Contains('-')
$makeLatest = -not $prerelease
$latest = Read-GitHub 'releases/latest'
if ($latest) {
    if ($latest.tag_name -cmatch '^v([0-9]+\.[0-9]+\.[0-9]+)$') {
        $latestVersion = [version]$Matches[1]
        $coreVersion = [version](($version -split '[-+]')[0])
        if ($coreVersion -lt $latestVersion) { $makeLatest = $false }
    } else { $makeLatest = $false }
    # Never break the signed updater's /latest/download/update-manifest.json endpoint.
    if (-not $info.signed -and @($latest.assets | Where-Object name -eq 'update-manifest.json').Count) { $makeLatest = $false }
}
$signingNotice = if ($info.signed) {
    'Authenticode signed. The updater manifest is included. Installation still requires user confirmation.'
} else {
    'UNSIGNED build: manual download/install only. Windows may warn about an unknown publisher. No update-manifest.json is supplied and in-app automatic installation is disabled. Do not disable Windows security protections.'
}
$notesPath = Join-Path $env:RUNNER_TEMP "SpeakTypeCloud-release-notes-$env:GITHUB_RUN_ID.md"
$notes = @"
## SpeakType Cloud $version - Windows x64

$signingNotice

- Installer: SpeakTypeCloud-Setup-$version.exe
- No-install ZIP: SpeakTypeCloud-portable.zip (user data still resides in the Windows user profile).
- Verify downloaded files against SHA256SUMS.txt.
- Source commit: $sourceSha
- Build details: build-info.json; dependency inventory: SpeakTypeCloud.cdx.json.
- CI validates offline tests and packaging; real microphone, provider and Windows 10/11 UI smoke tests are separate acceptance checks.
"@
[IO.File]::WriteAllText($notesPath, $notes, [Text.UTF8Encoding]::new($false))
try {
    $paths = @($files | ForEach-Object FullName)
    if ($existing) {
        # Only this commit's unpublished draft can be resumed; never clobber a public release.
        Invoke-Gh (@('release', 'upload', $tag, '--repo', $repository, '--clobber') + $paths)
    } else {
        $arguments = @('release', 'create', $tag, '--repo', $repository, '--target', $sourceSha, '--title', "SpeakType Cloud $tag", '--notes-file', $notesPath, '--generate-notes', '--draft')
        if ($prerelease) { $arguments += '--prerelease' }
        Invoke-Gh ($arguments + $paths)
    }
    $uploaded = Read-GitHub "releases/tags/$encodedTag"
    if (-not $uploaded -or -not $uploaded.draft) { throw 'Expected an unpublished draft before final verification' }
    if (@($uploaded.assets).Count -ne $files.Count) { throw 'Uploaded asset count does not match the verified local set' }
    foreach ($file in $files) {
        $remote = @($uploaded.assets | Where-Object name -CEQ $file.Name)
        if ($remote.Count -ne 1 -or $remote[0].size -ne $file.Length -or $remote[0].state -ne 'uploaded') { throw "Uploaded asset verification failed: $($file.Name)" }
        if ($remote[0].PSObject.Properties['digest'] -and $remote[0].digest) {
            $expectedDigest = 'sha256:' + (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($remote[0].digest -cne $expectedDigest) { throw "Remote digest mismatch: $($file.Name)" }
        }
    }
    Invoke-Gh @('release', 'edit', $tag, '--repo', $repository, '--draft=false', "--latest=$($makeLatest.ToString().ToLowerInvariant())")
    Write-Summary "Published $tag from $sourceSha. Signed=$($info.signed); Latest=$makeLatest. https://github.com/$repository/releases/tag/$tag"
} finally {
    if (Test-Path -LiteralPath $notesPath) { Remove-Item -LiteralPath $notesPath -Force }
}
