[CmdletBinding()]
param(
    [string]$OutputPath = "",
    [string]$CargoPath = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $root "dist\sbom\SpeakTypeCloud.cdx.json"
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$schemaPath = Join-Path $root "installer\cyclonedx-minimal.schema.json"
if (-not (Test-Path -LiteralPath $schemaPath -PathType Leaf)) { throw "SBOM schema not found: $schemaPath" }

if ([string]::IsNullOrWhiteSpace($CargoPath)) {
    $cargoCommand = Get-Command cargo.exe -ErrorAction SilentlyContinue
    if ($cargoCommand) { $CargoPath = $cargoCommand.Source }
}
if ([string]::IsNullOrWhiteSpace($CargoPath)) {
    $fallback = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    if (Test-Path -LiteralPath $fallback -PathType Leaf) { $CargoPath = $fallback }
}
if ([string]::IsNullOrWhiteSpace($CargoPath) -or -not (Test-Path -LiteralPath $CargoPath -PathType Leaf)) {
    throw "cargo.exe was not found. Install Rust or pass -CargoPath explicitly."
}

$metadataText = & $CargoPath metadata --locked --format-version 1 --manifest-path (Join-Path $root "Cargo.toml")
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed with exit code $LASTEXITCODE" }
$metadata = ($metadataText -join [Environment]::NewLine) | ConvertFrom-Json -Depth 100
if (-not $metadata.resolve -or -not $metadata.resolve.root) { throw "Cargo metadata did not contain a resolved root package" }

function New-PackageUrl($Package) {
    $name = [Uri]::EscapeDataString([string]$Package.name)
    $version = [Uri]::EscapeDataString([string]$Package.version)
    return "pkg:cargo/$name@$version"
}

function New-Component($Package, [string]$Type) {
    $purl = New-PackageUrl $Package
    $component = [ordered]@{
        type = $Type
        name = [string]$Package.name
        version = [string]$Package.version
        "bom-ref" = [string]$Package.id
        purl = $purl
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Package.license)) {
        $component.licenses = @([ordered]@{ expression = [string]$Package.license })
    }
    return $component
}

$packageById = @{}
foreach ($package in $metadata.packages) { $packageById[[string]$package.id] = $package }
$rootId = [string]$metadata.resolve.root
$rootPackage = $packageById[$rootId]
if (-not $rootPackage) { throw "Cargo metadata root package was not present in packages" }

$components = @(
    $metadata.packages |
        Where-Object { [string]$_.id -ne $rootId } |
        Sort-Object @{ Expression = { [string]$_.name } }, @{ Expression = { [string]$_.version } }, @{ Expression = { [string]$_.id } } |
        ForEach-Object { New-Component $_ "library" }
)

$dependencies = @(
    $metadata.resolve.nodes |
        Sort-Object @{ Expression = { [string]$_.id } } |
        ForEach-Object {
            [ordered]@{
                ref = [string]$_.id
                dependsOn = @($_.dependencies | ForEach-Object { [string]$_ } | Sort-Object -Unique)
            }
        }
)

$bom = [ordered]@{
    bomFormat = "CycloneDX"
    specVersion = "1.5"
    version = 1
    metadata = [ordered]@{
        component = New-Component $rootPackage "application"
    }
    components = $components
    dependencies = $dependencies
}
$json = $bom | ConvertTo-Json -Depth 100

$testJson = Get-Command Test-Json -ErrorAction SilentlyContinue
if (-not $testJson) { throw "Test-Json is required to validate the generated CycloneDX JSON" }
$valid = $json | Test-Json -SchemaFile $schemaPath
if (-not $valid) { throw "Generated SBOM failed required-field schema validation" }

$knownRefs = @{}
$knownRefs[$rootId] = $true
foreach ($component in $components) { $knownRefs[[string]$component."bom-ref"] = $true }
foreach ($dependency in $dependencies) {
    if (-not $knownRefs.ContainsKey([string]$dependency.ref)) { throw "SBOM dependency ref is unknown: $($dependency.ref)" }
    foreach ($child in $dependency.dependsOn) {
        if (-not $knownRefs.ContainsKey([string]$child)) { throw "SBOM dependency target is unknown: $child" }
    }
}

$parent = Split-Path -Parent $OutputPath
[System.IO.Directory]::CreateDirectory($parent) | Out-Null
[System.IO.File]::WriteAllText($OutputPath, $json + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
Write-Host "Created reproducible CycloneDX SBOM: $OutputPath"

