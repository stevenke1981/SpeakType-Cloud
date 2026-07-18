[CmdletBinding()]
param(
    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $root "dist\release\SpeakTypeCloud.ico"
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$parent = Split-Path -Parent $OutputPath
[System.IO.Directory]::CreateDirectory($parent) | Out-Null

$width = 32
$height = 32
$xorBytes = $width * $height * 4
$andBytes = [int](($width * $height) / 8)
$imageBytes = 40 + $xorBytes + $andBytes
$stream = [System.IO.MemoryStream]::new()
$writer = [System.IO.BinaryWriter]::new($stream)
try {
    # ICONDIR and ICONDIRENTRY.
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]1)
    $writer.Write([byte]$width)
    $writer.Write([byte]$height)
    $writer.Write([byte]0)
    $writer.Write([byte]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]32)
    $writer.Write([uint32]$imageBytes)
    $writer.Write([uint32]22)

    # BITMAPINFOHEADER. ICO stores the XOR and AND bitmap heights together.
    $writer.Write([uint32]40)
    $writer.Write([int32]$width)
    $writer.Write([int32]($height * 2))
    $writer.Write([uint16]1)
    $writer.Write([uint16]32)
    $writer.Write([uint32]0)
    $writer.Write([uint32]($xorBytes + $andBytes))
    $writer.Write([int32]0)
    $writer.Write([int32]0)
    $writer.Write([uint32]0)
    $writer.Write([uint32]0)

    # Deterministic blue disc with a white microphone waveform.
    for ($sourceY = $height - 1; $sourceY -ge 0; $sourceY--) {
        for ($x = 0; $x -lt $width; $x++) {
            $dx = $x - 15.5
            $dy = $sourceY - 15.5
            $insideDisc = (($dx * $dx) + ($dy * $dy)) -le (14.5 * 14.5)
            $waveHeight = @(3, 5, 8, 11, 8, 5, 3)
            $waveIndex = [int][Math]::Floor(($x - 5) / 3)
            $insideWave = $waveIndex -ge 0 -and $waveIndex -lt $waveHeight.Count -and
                [Math]::Abs($sourceY - 15.5) -le ($waveHeight[$waveIndex] / 2)

            if ($insideDisc -and $insideWave) {
                $writer.Write([byte]255); $writer.Write([byte]255); $writer.Write([byte]255); $writer.Write([byte]255)
            }
            elseif ($insideDisc) {
                $writer.Write([byte]213); $writer.Write([byte]111); $writer.Write([byte]37); $writer.Write([byte]255)
            }
            else {
                $writer.Write([byte]0); $writer.Write([byte]0); $writer.Write([byte]0); $writer.Write([byte]0)
            }
        }
    }
    for ($i = 0; $i -lt $andBytes; $i++) { $writer.Write([byte]0) }
    $writer.Flush()
    [System.IO.File]::WriteAllBytes($OutputPath, $stream.ToArray())
}
finally {
    $writer.Dispose()
    $stream.Dispose()
}

Write-Host "Created app icon: $OutputPath"

