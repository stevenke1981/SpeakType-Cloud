#Requires -Version 7.2
# Pure helpers shared by release scripts and offline regression tests.
Set-StrictMode -Version Latest

function Assert-ReleaseVersion([string]$Version) {
    $identifier = '(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
    $pattern = '\A(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-' + $identifier + '(?:\.' + $identifier + ')*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?\z'
    if ($Version -cnotmatch $pattern) { throw "Invalid SemVer release version: $Version" }
}

function Get-ReleaseVersion([string]$ManifestPath) {
    $text = Get-Content -LiteralPath $ManifestPath -Raw
    $section = [regex]::Match($text, '(?ms)^\[package\]\s*\r?\n(?<body>.*?)(?=^\[|\z)')
    $match = [regex]::Match($section.Groups['body'].Value, '(?m)^version\s*=\s*"([^"]+)"\s*(?:#.*)?$')
    if (-not $section.Success -or -not $match.Success) { throw 'Cannot read [package].version from Cargo.toml' }
    $version = $match.Groups[1].Value
    Assert-ReleaseVersion $version
    return $version
}

function Get-ReleaseDecision {
    param([string]$Version, [string]$EventName, [string]$Ref, [string]$DefaultBranch, [string]$SignerPin = '')
    Assert-ReleaseVersion $Version
    $tag = "v$Version"
    $tagEvent = $Ref.StartsWith('refs/tags/')
    if ($tagEvent -and $Ref -cne "refs/tags/$tag") { throw "Tag must equal v$Version from Cargo.toml" }
    $trustedRef = $tagEvent -or ($DefaultBranch -and $Ref -ceq "refs/heads/$DefaultBranch")
    $publish = ($EventName -in @('push', 'workflow_dispatch')) -and $trustedRef
    $signed = $publish -and -not [string]::IsNullOrWhiteSpace($SignerPin)
    if ($signed -and $SignerPin -cnotmatch '^[0-9a-fA-F]{64}$') { throw 'UPDATE_SIGNER_CERT_SHA256 must contain 64 hexadecimal characters' }
    return [pscustomobject]@{ Version = $Version; Tag = $tag; Publish = [bool]$publish; Signed = [bool]$signed }
}

function Assert-SignedReleaseFile([string]$Path, [string]$ExpectedSignerCertSha256) {
    if ($ExpectedSignerCertSha256 -cnotmatch '^[0-9a-fA-F]{64}$') { throw 'A SHA-256 signer certificate pin is required' }
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or -not $signature.SignerCertificate) { throw 'Release file does not have a valid Authenticode signature' }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $actual = [Convert]::ToHexString($sha.ComputeHash($signature.SignerCertificate.RawData)).ToLowerInvariant() }
    finally { $sha.Dispose() }
    if ($actual -cne $ExpectedSignerCertSha256.ToLowerInvariant()) { throw 'Release signer certificate does not match the pinned identity' }
}

function Get-ReleaseFiles([string]$Directory) {
    $items = @(Get-ChildItem -LiteralPath $Directory -Force)
    foreach ($item in $items) {
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Release assets must be regular top-level files' }
        if ($item.Name -cnotmatch '^[A-Za-z0-9][A-Za-z0-9._+-]*$') { throw "Unsafe release filename: $($item.Name)" }
        if ($item.Length -eq 0) { throw "Empty release asset: $($item.Name)" }
    }
    return @($items | Sort-Object Name)
}

function Write-ReleaseChecksums([string]$Directory) {
    $files = @(Get-ReleaseFiles $Directory | Where-Object Name -ne 'SHA256SUMS.txt')
    if ($files.Count -eq 0) { throw 'No release assets to hash' }
    $lines = @($files | ForEach-Object {
        '{0}  {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
    })
    [IO.File]::WriteAllText((Join-Path $Directory 'SHA256SUMS.txt'), ($lines -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
}

function Test-ReleaseChecksums([string]$Directory) {
    $files = @(Get-ReleaseFiles $Directory | Where-Object Name -ne 'SHA256SUMS.txt')
    $listed = @{}
    foreach ($line in Get-Content -LiteralPath (Join-Path $Directory 'SHA256SUMS.txt')) {
        if ($line -cnotmatch '^([0-9a-f]{64})  ([A-Za-z0-9][A-Za-z0-9._+-]*)$') { throw 'Malformed SHA256SUMS.txt entry' }
        $expected = $Matches[1]
        $name = $Matches[2]
        if ($name -eq 'SHA256SUMS.txt' -or $listed.ContainsKey($name)) { throw 'Duplicate or self-referencing checksum entry' }
        $listed[$name] = $true
        $path = Join-Path $Directory $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing release asset: $name" }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) { throw "Checksum mismatch: $name" }
    }
    if ($files.Count -eq 0 -or $files.Count -ne $listed.Count) { throw 'Release files and checksum entries do not match' }
    foreach ($file in $files) {
        if (-not $listed.ContainsKey($file.Name)) { throw "Unlisted release asset: $($file.Name)" }
    }
}
