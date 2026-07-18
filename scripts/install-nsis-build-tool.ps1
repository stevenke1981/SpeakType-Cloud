[CmdletBinding()]
param(
    [string]$PackageDirectory = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$provenancePath = Join-Path $root "installer\release-provenance.json"
if (-not (Test-Path -LiteralPath $provenancePath -PathType Leaf)) {
    throw "Release provenance not found: $provenancePath"
}
$provenance = Get-Content -LiteralPath $provenancePath -Raw | ConvertFrom-Json
$tool = @($provenance.build_tools | Where-Object { $_.name -eq "NSIS Chocolatey package" })
if ($tool.Count -ne 1) { throw "Expected exactly one NSIS provenance entry" }
$tool = $tool[0]
if ($tool.version -ne "3.12.0") { throw "Only the audited NSIS 3.12.0 package is allowed" }
$packages = @($tool.packages)
if ($packages.Count -ne 2) { throw "NSIS provenance must contain the meta and install packages" }

if ([string]::IsNullOrWhiteSpace($PackageDirectory)) {
    $tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
    $PackageDirectory = Join-Path $tempRoot "SpeakTypeCloud-nsis-3.12.0"
}
$PackageDirectory = [System.IO.Path]::GetFullPath($PackageDirectory)
[System.IO.Directory]::CreateDirectory($PackageDirectory) | Out-Null

foreach ($package in $packages) {
    if ($package.id -notmatch '^nsis(?:\.install)?$') { throw "Unexpected NSIS package id: $($package.id)" }
    if ($package.package_url -notmatch '^https://community\.chocolatey\.org/api/v2/package/[^/]+/3\.12\.0$') {
        throw "NSIS package URL is not pinned to the audited Chocolatey version"
    }
    if ($package.expected_sha256 -notmatch '^[0-9a-f]{64}$') { throw "Invalid expected SHA-256 for $($package.id)" }

    $packagePath = Join-Path $PackageDirectory "$($package.id).3.12.0.nupkg"
    Invoke-WebRequest -Uri $package.package_url -OutFile $packagePath -MaximumRedirection 5
    $actualSha256 = (Get-FileHash -LiteralPath $packagePath -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Host "Verified download candidate $($package.id) 3.12.0: expected=$($package.expected_sha256) actual=$actualSha256"
    if ($actualSha256 -ne $package.expected_sha256) {
        throw "SHA-256 mismatch for $($package.id) 3.12.0; refusing to install"
    }
}

& choco install nsis "--version=$($tool.version)" "--source=$PackageDirectory" --no-progress -y
if ($LASTEXITCODE -ne 0) { throw "Chocolatey local NSIS install failed with exit code $LASTEXITCODE" }
Write-Host "Installed hash-pinned NSIS $($tool.version) from local verified nupkg files."

