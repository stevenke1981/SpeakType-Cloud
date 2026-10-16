#Requires -Version 7.2
param([string]$TestRoot = [IO.Path]::GetTempPath())
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'release-common.ps1')
$script:passed = 0
function Assert-True([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Assert-Throws([scriptblock]$Action) {
    $threw = $false
    try { & $Action | Out-Null } catch { $threw = $true }
    Assert-True $threw 'Expected failure, but the operation succeeded'
}
function Test-Case([string]$Name, [scriptblock]$Action) {
    & $Action
    $script:passed++
    Write-Host "PASS: $Name"
}
$testRootFull = [IO.Path]::GetFullPath($TestRoot)
if (-not (Test-Path -LiteralPath $testRootFull -PathType Container)) { throw 'TestRoot must be an existing directory' }
$temp = Join-Path $testRootFull "SpeakTypeCloud-release-tests-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    Test-Case 'SemVer accepts stable, prerelease and build identifiers' {
        foreach ($version in @('0.1.0', '1.2.3', '1.2.3-rc.1', '1.2.3+build.9', '1.2.3-beta.2+build.9')) { Assert-ReleaseVersion $version }
    }
    Test-Case 'SemVer rejects unsafe and malformed values' {
        foreach ($version in @('', 'v1.2.3', '01.2.3', '1.2', '1.2.3-01', '1.2.3-', '1.2.3/other', "1.2.3`ncommand", "1.2.3`n")) { Assert-Throws { Assert-ReleaseVersion $version } }
    }
    Test-Case 'Only the package section supplies the version' {
        $manifest = Join-Path $temp 'Cargo.toml'
        "[package]`nname = `"example`"`nversion = `"1.2.3`"`n[dependencies]`nversion = `"9.9.9`"`n" | Set-Content -LiteralPath $manifest
        Assert-True ((Get-ReleaseVersion $manifest) -ceq '1.2.3') 'Wrong package version'
        "[package]`nname = `"example`"`n[dependencies]`nversion = `"9.9.9`"`n" | Set-Content -LiteralPath $manifest
        Assert-Throws { Get-ReleaseVersion $manifest }
    }
    Test-Case 'Default branches main and master can publish' {
        foreach ($branch in @('main', 'master')) {
            $decision = Get-ReleaseDecision '1.2.3' 'push' "refs/heads/$branch" $branch
            Assert-True ($decision.Publish -and -not $decision.Signed) 'Default-branch release must be eligible and unsigned without a pin'
        }
    }
    Test-Case 'Pull requests and development branches cannot publish or sign' {
        $pin = 'a' * 64
        $pr = Get-ReleaseDecision '1.2.3' 'pull_request' 'refs/heads/main' 'main' $pin
        $branch = Get-ReleaseDecision '1.2.3' 'push' 'refs/heads/develop' 'main' $pin
        Assert-True (-not $pr.Publish -and -not $pr.Signed -and -not $branch.Publish -and -not $branch.Signed) 'Untrusted context became release eligible'
    }
    Test-Case 'Matching version tags are eligible and mismatched tags fail' {
        Assert-True ((Get-ReleaseDecision '1.2.3' 'push' 'refs/tags/v1.2.3' 'main').Publish) 'Matching tag was rejected'
        Assert-Throws { Get-ReleaseDecision '1.2.3' 'push' 'refs/tags/v9.9.9' 'main' }
    }
    Test-Case 'Manual runs publish only from trusted refs' {
        Assert-True ((Get-ReleaseDecision '1.2.3' 'workflow_dispatch' 'refs/heads/main' 'main').Publish) 'Default branch manual run was rejected'
        Assert-True (-not (Get-ReleaseDecision '1.2.3' 'workflow_dispatch' 'refs/heads/dev' 'main').Publish) 'Development manual run became eligible'
    }
    Test-Case 'Signing pins are validated without unsigned fallback' {
        Assert-True ((Get-ReleaseDecision '1.2.3' 'push' 'refs/heads/main' 'main' ('a' * 64)).Signed) 'Valid pin was rejected'
        Assert-Throws { Get-ReleaseDecision '1.2.3' 'push' 'refs/heads/main' 'main' 'bad-pin' }
    }
    Test-Case 'Draft lookup uses the release collection rather than a published tag' {
        $found = Find-ReleaseByTag 'v1.2.3' {
            param($path)
            if ($path.StartsWith('releases/tags/')) { return $null }
            return ,@([pscustomobject]@{ tag_name = 'v1.2.3'; draft = $true; id = 42 })
        }
        Assert-True ($found.id -eq 42 -and $found.draft) 'Draft release was not found'
    }
    Test-Case 'Draft lookup paginates without treating the first page as complete' {
        $found = Find-ReleaseByTag 'v1.2.3' {
            param($path)
            if ($path.StartsWith('releases/tags/')) { return $null }
            if ($path.EndsWith('page=1')) {
                return ,@(1..100 | ForEach-Object { [pscustomobject]@{ tag_name = "v0.0.$_"; id = $_; draft = $false } })
            }
            return ,@([pscustomobject]@{ tag_name = 'v1.2.3'; id = 101; draft = $true })
        }
        Assert-True ($found.id -eq 101) 'Later-page draft was missed'
    }
    Test-Case 'Empty release collections differ from unavailable collections' {
        $found = Find-ReleaseByTag 'v1.2.3' {
            param($path)
            if ($path.StartsWith('releases/tags/')) { return $null }
            return ,@()
        }
        Assert-True ($null -eq $found) 'Empty collection unexpectedly produced a release'
        Assert-Throws { Find-ReleaseByTag 'v1.2.3' { return $null } }
    }
    Test-Case 'Duplicate drafts are rejected instead of overwritten' {
        Assert-Throws {
            Find-ReleaseByTag 'v1.2.3' {
                param($path)
                if ($path.StartsWith('releases/tags/')) { return $null }
                return ,@(
                    [pscustomobject]@{ tag_name = 'v1.2.3'; id = 1; draft = $true },
                    [pscustomobject]@{ tag_name = 'v1.2.3'; id = 2; draft = $true }
                )
            }
        }
    }
    $assets = Join-Path $temp 'assets'
    New-Item -ItemType Directory -Path $assets | Out-Null
    'original content' | Set-Content -LiteralPath (Join-Path $assets 'one.exe')
    Test-Case 'Checksums round trip successfully' { Write-ReleaseChecksums $assets; Test-ReleaseChecksums $assets }
    Test-Case 'Tampered bytes fail checksum verification' {
        'tampered content' | Set-Content -LiteralPath (Join-Path $assets 'one.exe')
        Assert-Throws { Test-ReleaseChecksums $assets }
        Write-ReleaseChecksums $assets
    }
    Test-Case 'Unlisted assets fail verification' {
        'extra' | Set-Content -LiteralPath (Join-Path $assets 'extra.exe')
        Assert-Throws { Test-ReleaseChecksums $assets }
        Remove-Item -LiteralPath (Join-Path $assets 'extra.exe')
    }
    Test-Case 'Missing assets fail verification' {
        Move-Item -LiteralPath (Join-Path $assets 'one.exe') -Destination (Join-Path $temp 'one.exe')
        Assert-Throws { Test-ReleaseChecksums $assets }
        Move-Item -LiteralPath (Join-Path $temp 'one.exe') -Destination (Join-Path $assets 'one.exe')
    }
    Test-Case 'Duplicate checksum entries are rejected' {
        $line = Get-Content -LiteralPath (Join-Path $assets 'SHA256SUMS.txt')
        $line | Add-Content -LiteralPath (Join-Path $assets 'SHA256SUMS.txt')
        Assert-Throws { Test-ReleaseChecksums $assets }
        Write-ReleaseChecksums $assets
    }
    Test-Case 'Checksum paths cannot escape the asset directory' {
        (('a' * 64) + '  ../escape.exe') | Set-Content -LiteralPath (Join-Path $assets 'SHA256SUMS.txt')
        Assert-Throws { Test-ReleaseChecksums $assets }
        Write-ReleaseChecksums $assets
    }
    Test-Case 'Empty release files and nested directories are rejected' {
        [IO.File]::WriteAllText((Join-Path $assets 'empty.exe'), '')
        Assert-Throws { Write-ReleaseChecksums $assets }
        Remove-Item -LiteralPath (Join-Path $assets 'empty.exe')
        New-Item -ItemType Directory -Path (Join-Path $assets 'nested') | Out-Null
        Assert-Throws { Write-ReleaseChecksums $assets }
        Remove-Item -LiteralPath (Join-Path $assets 'nested')
    }
    Test-Case 'Unsigned files cannot produce an updater manifest' {
        $installer = Join-Path $temp 'SpeakTypeCloud-Setup-1.2.3.exe'
        'Not an executable; offline negative test fixture.' | Set-Content -LiteralPath $installer
        Assert-Throws { & (Join-Path $PSScriptRoot 'new-update-manifest.ps1') -InstallerPath $installer -Version '1.2.3' -ExpectedSignerCertSha256 '' }
        Assert-Throws { & (Join-Path $PSScriptRoot 'new-update-manifest.ps1') -InstallerPath $installer -Version '1.2.3' -ExpectedSignerCertSha256 ('a' * 64) }
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $temp 'update-manifest.json'))) 'An unsigned updater manifest was created'
    }
    $fixture = Join-Path $temp 'fixture'
    New-Item -ItemType Directory -Force -Path "$fixture\scripts", "$fixture\target\release" | Out-Null
    foreach ($name in @('build-release.ps1', 'package-portable.ps1')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination "$fixture\scripts" }
    New-Item -ItemType Directory -Force -Path "$fixture\docs\licenses" | Out-Null
    'Synthetic license test fixture' | Set-Content -LiteralPath "$fixture\docs\licenses\slint-royalty-free-2.0.pdf"
    foreach ($name in @('LICENSE', 'THIRD_PARTY_NOTICES.md', 'README.md', 'SECURITY.md', 'API_PROVIDERS.md', 'config.toml', '.env')) { 'Offline fixture only.' | Set-Content -LiteralPath (Join-Path $fixture $name) }
    'Not executable; packaging fixture only.' | Set-Content -LiteralPath "$fixture\target\release\speaktype-cloud.exe"
    'Debug fixture.' | Set-Content -LiteralPath "$fixture\target\release\speaktype_cloud.pdb"
    Test-Case 'SkipBuild stages the existing executable and PDB' {
        & "$fixture\scripts\build-release.ps1" -SkipBuild
        Assert-True (Test-Path -LiteralPath "$fixture\dist\release\SpeakTypeCloud.pdb") 'PDB was not staged'
    }
    Test-Case 'Portable ZIP does not require symbols or include runtime files' {
        Remove-Item -LiteralPath "$fixture\target\release\speaktype_cloud.pdb"
        & "$fixture\scripts\build-release.ps1" -SkipBuild
        Assert-True (-not (Test-Path -LiteralPath "$fixture\dist\release\SpeakTypeCloud.pdb")) 'Stale PDB was retained'
        & "$fixture\scripts\package-portable.ps1" -SkipBuild
        $archive = [IO.Compression.ZipFile]::OpenRead("$fixture\dist\SpeakTypeCloud-portable.zip")
        try {
            $names = @($archive.Entries | ForEach-Object FullName)
            Assert-True ($names -contains 'LICENSE' -and $names -contains 'SpeakTypeCloud.exe') 'Required ZIP content missing'
            Assert-True ($names -notcontains 'SpeakTypeCloud.pdb' -and $names -notcontains 'config.toml' -and $names -notcontains '.env') 'Symbols or runtime files leaked into ZIP'
        } finally { $archive.Dispose() }
    }
    Test-Case 'Failed symbol packaging preserves the previous ZIP' {
        $before = (Get-FileHash -LiteralPath "$fixture\dist\SpeakTypeCloud-portable.zip").Hash
        Assert-Throws { & "$fixture\scripts\package-portable.ps1" -SkipBuild -IncludeSymbols }
        Assert-True ((Get-FileHash -LiteralPath "$fixture\dist\SpeakTypeCloud-portable.zip").Hash -ceq $before) 'Failed packaging replaced the previous ZIP'
    }
    Test-Case 'Explicit symbol packaging includes a supplied PDB' {
        'Debug fixture.' | Set-Content -LiteralPath "$fixture\dist\release\SpeakTypeCloud.pdb"
        & "$fixture\scripts\package-portable.ps1" -SkipBuild -IncludeSymbols
        $archive = [IO.Compression.ZipFile]::OpenRead("$fixture\dist\SpeakTypeCloud-portable.zip")
        try { Assert-True (@($archive.Entries | ForEach-Object FullName) -contains 'SpeakTypeCloud.pdb') 'Requested symbols missing from ZIP' }
        finally { $archive.Dispose() }
    }
    Write-Host "Release behavior tests passed: $script:passed"
} finally {
    Remove-Item -LiteralPath $temp -Recurse -Force
}
