[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string[]]$Path,
    [string]$Thumbprint = "",
    [string]$PfxPath = "",
    [securestring]$PfxPassword,
    [string]$PasswordEnv = "",
    [ValidateSet("Fail", "Skip")]
    [string]$MissingCredentialPolicy = "Fail",
    [string]$TimestampUrl = "https://timestamp.digicert.com",
    [string]$SignToolPath = "",
    [string]$ExpectedSignerCertSha256 = "",
    [switch]$VerifyOnly,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Resolve-SignTool([string]$RequestedPath) {
    if (-not [string]::IsNullOrWhiteSpace($RequestedPath)) {
        if (Test-Path -LiteralPath $RequestedPath -PathType Leaf) { return [System.IO.Path]::GetFullPath($RequestedPath) }
        return $null
    }
    $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    $kitsRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    if (Test-Path -LiteralPath $kitsRoot -PathType Container) {
        $candidate = Get-ChildItem -LiteralPath $kitsRoot -Filter signtool.exe -File -Recurse |
            Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
            Sort-Object @{ Expression = { try { [version]$_.Directory.Parent.Name } catch { [version]"0.0" } }; Descending = $true } |
            Select-Object -First 1
        if ($candidate) { return $candidate.FullName }
    }
    return $null
}

function Stop-Or-Skip([string]$Reason) {
    if ($MissingCredentialPolicy -eq "Skip") {
        Write-Warning "Signing skipped: $Reason"
        return $false
    }
    throw $Reason
}

function Assert-ExpectedSigner([string]$Artifact, [string]$Expected) {
    if ([string]::IsNullOrWhiteSpace($Expected)) { return }
    $normalized = ($Expected -replace '\s', '').ToLowerInvariant()
    if ($normalized -notmatch '^[0-9a-f]{64}$') { throw "Expected signer certificate SHA-256 must contain exactly 64 hexadecimal characters." }
    $signature = Get-AuthenticodeSignature -LiteralPath $Artifact
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or -not $signature.SignerCertificate) {
        throw "Authenticode signer identity cannot be validated for '$Artifact'."
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $actual = (($sha.ComputeHash($signature.SignerCertificate.RawData) | ForEach-Object { $_.ToString('x2') }) -join '')
    }
    finally {
        $sha.Dispose()
    }
    if ($actual -ne $normalized) { throw "Authenticode signer certificate SHA-256 does not match the expected trust root for '$Artifact'." }
}

$resolvedPaths = @()
foreach ($item in $Path) {
    $resolved = [System.IO.Path]::GetFullPath($item)
    if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) { throw "Artifact not found: $resolved" }
    $resolvedPaths += $resolved
}

$tool = Resolve-SignTool $SignToolPath
if (-not $tool) {
    if (-not (Stop-Or-Skip "signtool.exe was not found. Install the Windows SDK or pass -SignToolPath.")) { return }
}

if ($VerifyOnly) {
    foreach ($artifact in $resolvedPaths) {
        & $tool verify /pa /all $artifact
        if ($LASTEXITCODE -ne 0) { throw "Authenticode verification failed for '$artifact' with exit code $LASTEXITCODE" }
        Assert-ExpectedSigner $artifact $ExpectedSignerCertSha256
    }
    Write-Host "Authenticode verification passed for $($resolvedPaths.Count) artifact(s)."
    return
}

if ([string]::IsNullOrWhiteSpace($Thumbprint) -and [string]::IsNullOrWhiteSpace($PfxPath)) {
    if (-not (Stop-Or-Skip "Provide exactly one signing identity: -Thumbprint or -PfxPath.")) { return }
}
if (-not [string]::IsNullOrWhiteSpace($Thumbprint) -and -not [string]::IsNullOrWhiteSpace($PfxPath)) {
    throw "Provide only one signing identity: -Thumbprint or -PfxPath."
}

$securePassword = $PfxPassword
if (-not [string]::IsNullOrWhiteSpace($PfxPath)) {
    $PfxPath = [System.IO.Path]::GetFullPath($PfxPath)
    if (-not (Test-Path -LiteralPath $PfxPath -PathType Leaf)) { throw "PFX file not found: $PfxPath" }
    if (-not $securePassword -and -not [string]::IsNullOrWhiteSpace($PasswordEnv)) {
        $environmentValue = [Environment]::GetEnvironmentVariable($PasswordEnv)
        if (-not [string]::IsNullOrEmpty($environmentValue)) {
            $securePassword = ConvertTo-SecureString $environmentValue -AsPlainText -Force
            $environmentValue = $null
        }
    }
    if (-not $securePassword -and $env:CI -ne "true") {
        $securePassword = Read-Host "PFX password" -AsSecureString
    }
    if (-not $securePassword) {
        if (-not (Stop-Or-Skip "PFX password was not supplied as SecureString, environment variable, or interactive input.")) { return }
    }
}

if ($DryRun) {
    Write-Host "Signing dry run passed for $($resolvedPaths.Count) artifact(s); no files were modified."
    return
}

foreach ($artifact in $resolvedPaths) {
    $arguments = @("sign", "/fd", "SHA256", "/tr", $TimestampUrl, "/td", "SHA256")
    $passwordPointer = [IntPtr]::Zero
    $plainPassword = $null
    try {
        if (-not [string]::IsNullOrWhiteSpace($Thumbprint)) {
            $arguments += @("/sha1", ($Thumbprint -replace '\s', ''))
        }
        else {
            $arguments += @("/f", $PfxPath)
            $passwordPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($securePassword)
            $plainPassword = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($passwordPointer)
            $arguments += @("/p", $plainPassword)
        }
        $arguments += $artifact
        & $tool @arguments
        if ($LASTEXITCODE -ne 0) { throw "Authenticode signing failed for '$artifact' with exit code $LASTEXITCODE" }
    }
    finally {
        $arguments = $null
        $plainPassword = $null
        if ($passwordPointer -ne [IntPtr]::Zero) {
            [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordPointer)
        }
    }

    & $tool verify /pa /all $artifact
    if ($LASTEXITCODE -ne 0) { throw "Post-sign Authenticode verification failed for '$artifact' with exit code $LASTEXITCODE" }
    Assert-ExpectedSigner $artifact $ExpectedSignerCertSha256
}
Write-Host "Signed and verified $($resolvedPaths.Count) artifact(s)."
