#Requires -Version 7.2
[CmdletBinding()]
param([string]$ToolDirectory = '')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$metadata = Get-Content -LiteralPath "$root\installer\release-provenance.json" -Raw | ConvertFrom-Json
$tool = @($metadata.build_tools | Where-Object name -eq 'Inno Setup')
if ($tool.Count -ne 1 -or $tool[0].version -ne '7.1.0') { throw 'Unrecognized compiler provenance' }
$tool = $tool[0]
if ($tool.package_url -ne 'https://github.com/jrsoftware/issrc/releases/download/is-7_1_0/innosetup-7.1.0-x64.exe' -or $tool.expected_sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Unpinned compiler download' }
if (-not $ToolDirectory) {
    $base = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
    $ToolDirectory = Join-Path $base 'SpeakTypeCloud-Inno-7.1.0'
}
$ToolDirectory = [IO.Path]::GetFullPath($ToolDirectory)
[IO.Directory]::CreateDirectory($ToolDirectory) | Out-Null
$download = Join-Path $ToolDirectory 'innosetup-7.1.0-x64.exe'
Invoke-WebRequest -Uri $tool.package_url -OutFile $download -MaximumRedirection 5
$hash = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash.ToLowerInvariant()
if ($hash -ne $tool.expected_sha256) { throw 'Inno SHA-256 mismatch; refusing execution' }
$signature = Get-AuthenticodeSignature -LiteralPath $download
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notlike '*CN=Pyrsys B.V.*') { throw 'Inno publisher signature invalid' }
$compiler = Join-Path $ToolDirectory 'compiler'
$process = Start-Process -FilePath $download -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', '/NOICONS', "/DIR=`"$compiler`"") -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Inno tool install failed: $($process.ExitCode)" }
$iscc = Join-Path $compiler 'ISCC.exe'
$version = & $iscc --version
if ($LASTEXITCODE -ne 0 -or "$version".Trim() -ne '7.1.0') { throw 'Installed compiler version mismatch' }
$env:SPEAKTYPE_ISCC = $iscc
if ($env:GITHUB_ENV) { "SPEAKTYPE_ISCC=$iscc" | Add-Content -LiteralPath $env:GITHUB_ENV -Encoding utf8 }
Write-Host "Verified Inno Setup $version ($hash)"
