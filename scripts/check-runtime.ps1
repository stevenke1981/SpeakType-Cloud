#Requires -Version 7.2
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Executable)
$ErrorActionPreference = 'Stop'
$bytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Executable))
function Read-U16([int]$Offset) { [BitConverter]::ToUInt16($bytes, $Offset) }
function Read-U32([int]$Offset) { [BitConverter]::ToUInt32($bytes, $Offset) }
if ($bytes.Length -lt 256 -or (Read-U16 0) -ne 0x5a4d) { throw 'Invalid DOS executable header' }
$pe = [int](Read-U32 60)
if ((Read-U32 $pe) -ne 0x4550 -or (Read-U16 ($pe + 4)) -ne 0x8664) { throw 'Expected Windows x64 PE' }
$count = Read-U16 ($pe + 6)
$optional = $pe + 24
if ((Read-U16 $optional) -ne 0x20b) { throw 'Expected PE32+' }
$sectionsOffset = $optional + (Read-U16 ($pe + 20))
$sections = @(for ($i=0; $i -lt $count; $i++) {
    $offset = $sectionsOffset + 40 * $i
    @{rva=(Read-U32 ($offset+12));size=[Math]::Max((Read-U32 ($offset+8)),(Read-U32 ($offset+16)));file=(Read-U32 ($offset+20))}
})
function Resolve-Rva([uint32]$Rva) {
    foreach ($section in $sections) {
        if ($Rva -ge $section.rva -and $Rva -lt $section.rva + $section.size) { return [int]($section.file + $Rva - $section.rva) }
    }
    throw "Unmapped PE RVA: $Rva"
}
function Read-AsciiZ([int]$Offset) {
    $end=$Offset
    while ($end -lt $bytes.Length -and $end - $Offset -lt 512 -and $bytes[$end] -ne 0) { $end++ }
    if ($end -ge $bytes.Length -or $end - $Offset -ge 512) { throw 'Invalid import name' }
    [Text.Encoding]::ASCII.GetString($bytes,$Offset,$end-$Offset).ToLowerInvariant()
}
$imports = [Collections.Generic.HashSet[string]]::new()
$rva=Read-U32 ($optional+120)
if ($rva -ne 0) {
    $offset=Resolve-Rva $rva
    for ($i=0;$i -lt 256;$i++) {
        $name=Read-U32 ($offset+12)
        if ($name -eq 0) { break }
        [void]$imports.Add((Read-AsciiZ (Resolve-Rva $name)))
        $offset+=20
    }
}
# Check delayed DLL imports too; a portable build must not hide runtime dependencies there.
$delay=Read-U32 ($optional+112+13*8)
if ($delay -ne 0) {
    $offset=Resolve-Rva $delay
    for ($i=0;$i -lt 256;$i++) {
        $name=Read-U32 ($offset+4)
        if ($name -eq 0) { break }
        if (((Read-U32 $offset) -band 1) -eq 0) { throw 'Unsupported VA-based delay import descriptor' }
        [void]$imports.Add((Read-AsciiZ (Resolve-Rva $name)))
        $offset+=32
    }
}
$system = @('kernel32.dll','user32.dll','advapi32.dll','bcrypt.dll','bcryptprimitives.dll','combase.dll',
    'comctl32.dll','dwmapi.dll','dwrite.dll','gdi32.dll','imm32.dll','ntdll.dll','ole32.dll','oleaut32.dll',
    'shell32.dll','shlwapi.dll','uiautomationcore.dll','uxtheme.dll','ws2_32.dll','ucrtbase.dll',
    'winmm.dll','version.dll','secur32.dll','crypt32.dll','d3d11.dll','dxgi.dll','d2d1.dll','dcomp.dll',
    'propsys.dll','setupapi.dll','cfgmgr32.dll','msvcrt.dll')
foreach ($name in $imports) {
    if ($name -notin $system -and $name -notmatch '^api-ms-win-(core|crt)-[a-z0-9-]+\.dll$') {
        throw "External DLL dependency must be bundled or statically linked: $name"
    }
}
[ordered]@{executable=[IO.Path]::GetFileName($Executable);architecture='x64';runtime='Windows 10/11 system DLLs only';imports=@($imports|Sort-Object)} | ConvertTo-Json -Depth 3
