#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [Parameter(Mandatory = $true)][string]$IcoPath
)

$ErrorActionPreference = 'Stop'
if (!$IsWindows) { throw 'Icon resource verification requires Windows.' }
if (![BitConverter]::IsLittleEndian) { throw 'ICO parsing requires a little-endian host.' }

if ($null -eq ('TrackBIconResourceNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class TrackBIconResourceNative
{
    [DllImport("kernel32.dll", EntryPoint = "LoadLibraryExW", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    public static extern IntPtr LoadLibraryExW(string fileName, IntPtr file, uint flags);

    [DllImport("kernel32.dll", EntryPoint = "FindResourceW", ExactSpelling = true, SetLastError = true)]
    public static extern IntPtr FindResourceW(IntPtr module, IntPtr name, IntPtr type);

    [DllImport("kernel32.dll", EntryPoint = "LoadResource", ExactSpelling = true, SetLastError = true)]
    public static extern IntPtr LoadResource(IntPtr module, IntPtr resource);

    [DllImport("kernel32.dll", EntryPoint = "SizeofResource", ExactSpelling = true, SetLastError = true)]
    public static extern uint SizeofResource(IntPtr module, IntPtr resource);

    [DllImport("kernel32.dll", EntryPoint = "LockResource", ExactSpelling = true, SetLastError = true)]
    public static extern IntPtr LockResource(IntPtr resourceData);

    [DllImport("kernel32.dll", EntryPoint = "FreeLibrary", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool FreeLibrary(IntPtr module);
}
'@ | Out-Null
}

function Read-UInt16At {
    param([byte[]]$Data, [int]$Offset)
    if ($Offset -lt 0 -or $Offset + 2 -gt $Data.Length) { throw 'Truncated icon directory.' }
    return [BitConverter]::ToUInt16($Data, $Offset)
}

function Read-UInt32At {
    param([byte[]]$Data, [int]$Offset)
    if ($Offset -lt 0 -or $Offset + 4 -gt $Data.Length) { throw 'Truncated icon directory.' }
    return [BitConverter]::ToUInt32($Data, $Offset)
}

function Get-Win32Failure {
    param([string]$Api)
    $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    $detail = ([ComponentModel.Win32Exception]::new($code)).Message
    return "$Api failed (Win32 $code): $detail"
}

function Get-Sha256Hex {
    param([byte[]]$Data)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString($sha256.ComputeHash($Data)).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
}

function Get-ResourceBytes {
    param([IntPtr]$Module, [UInt16]$ResourceId, [UInt16]$ResourceType, [string]$Label)
    $namePointer = [IntPtr]::new([Int64]$ResourceId)
    $typePointer = [IntPtr]::new([Int64]$ResourceType)
    $resource = [TrackBIconResourceNative]::FindResourceW($Module, $namePointer, $typePointer)
    if ($resource -eq [IntPtr]::Zero) { throw (Get-Win32Failure "FindResourceW($Label)") }

    $size = [TrackBIconResourceNative]::SizeofResource($Module, $resource)
    if ($size -eq 0) { throw (Get-Win32Failure "SizeofResource($Label)") }
    if ($size -gt [Int32]::MaxValue) { throw "$Label is too large to validate safely." }

    $loaded = [TrackBIconResourceNative]::LoadResource($Module, $resource)
    if ($loaded -eq [IntPtr]::Zero) { throw (Get-Win32Failure "LoadResource($Label)") }
    $locked = [TrackBIconResourceNative]::LockResource($loaded)
    if ($locked -eq [IntPtr]::Zero) { throw (Get-Win32Failure "LockResource($Label)") }

    $bytes = [byte[]]::new([Int32]$size)
    [Runtime.InteropServices.Marshal]::Copy($locked, $bytes, 0, $bytes.Length)
    return [pscustomobject]@{ Size = $size; Bytes = $bytes }
}

$executable = (Resolve-Path -LiteralPath $ExecutablePath -ErrorAction Stop).ProviderPath
$ico = (Resolve-Path -LiteralPath $IcoPath -ErrorAction Stop).ProviderPath
if (!(Test-Path -LiteralPath $executable -PathType Leaf)) { throw "Executable is not a file: $executable" }
if (!(Test-Path -LiteralPath $ico -PathType Leaf)) { throw "ICO is not a file: $ico" }

$icoBytes = [IO.File]::ReadAllBytes($ico)
if ($icoBytes.Length -lt 6) { throw 'ICO header is truncated.' }
$icoReserved = Read-UInt16At $icoBytes 0
$icoType = Read-UInt16At $icoBytes 2
$icoCount = Read-UInt16At $icoBytes 4
if ($icoReserved -ne 0 -or $icoType -ne 1 -or $icoCount -eq 0) { throw 'ICO header must have reserved=0, type=1, and at least one entry.' }
$icoDirectoryEnd = 6 + (16 * [int]$icoCount)
if ($icoBytes.Length -lt $icoDirectoryEnd) { throw 'ICO directory is truncated.' }

$icoEntries = @(
    for ($index = 0; $index -lt $icoCount; $index++) {
        $offset = 6 + (16 * $index)
        $length = Read-UInt32At $icoBytes ($offset + 8)
        $imageOffset = Read-UInt32At $icoBytes ($offset + 12)
        $imageEnd = [UInt64]$imageOffset + [UInt64]$length
        if ($icoBytes[$offset + 3] -ne 0) { throw "ICO entry $index has a nonzero reserved byte." }
        if ($length -eq 0 -or $imageOffset -lt $icoDirectoryEnd -or $imageEnd -gt [UInt64]$icoBytes.Length) {
            throw "ICO entry $index has an invalid payload length or offset."
        }
        [pscustomobject]@{
            Width = $icoBytes[$offset]
            Height = $icoBytes[$offset + 1]
            ColorCount = $icoBytes[$offset + 2]
            Reserved = $icoBytes[$offset + 3]
            Planes = Read-UInt16At $icoBytes ($offset + 4)
            BitCount = Read-UInt16At $icoBytes ($offset + 6)
            Length = $length
            Offset = $imageOffset
        }
    }
)

$loadAsDataFile = [UInt32]0x2
$loadAsImageResource = [UInt32]0x20
$module = [TrackBIconResourceNative]::LoadLibraryExW($executable, [IntPtr]::Zero, ($loadAsDataFile -bor $loadAsImageResource))
if ($module -eq [IntPtr]::Zero) { throw (Get-Win32Failure 'LoadLibraryExW') }

try {
    $group = Get-ResourceBytes -Module $module -ResourceId 1 -ResourceType 14 -Label 'RT_GROUP_ICON #1'
    $groupBytes = $group.Bytes
    if ($groupBytes.Length -lt 6) { throw 'RT_GROUP_ICON #1 header is truncated.' }
    $groupReserved = Read-UInt16At $groupBytes 0
    $groupType = Read-UInt16At $groupBytes 2
    $groupCount = Read-UInt16At $groupBytes 4
    if ($groupReserved -ne 0 -or $groupType -ne 1 -or $groupCount -eq 0) {
        throw 'RT_GROUP_ICON #1 must have reserved=0, type=1, and at least one entry.'
    }
    $expectedGroupLength = 6 + (14 * [int]$groupCount)
    if ($groupBytes.Length -ne $expectedGroupLength) { throw 'RT_GROUP_ICON #1 has an invalid directory length.' }
    if ($groupCount -ne $icoCount) { throw "ICO entry count ($icoCount) differs from RT_GROUP_ICON count ($groupCount)." }

    $payloadResults = @(
        for ($index = 0; $index -lt $groupCount; $index++) {
            $offset = 6 + (14 * $index)
            $icoEntry = $icoEntries[$index]
            $groupEntry = [pscustomobject]@{
                Width = $groupBytes[$offset]
                Height = $groupBytes[$offset + 1]
                ColorCount = $groupBytes[$offset + 2]
                Reserved = $groupBytes[$offset + 3]
                Planes = Read-UInt16At $groupBytes ($offset + 4)
                BitCount = Read-UInt16At $groupBytes ($offset + 6)
                Length = Read-UInt32At $groupBytes ($offset + 8)
                ResourceId = Read-UInt16At $groupBytes ($offset + 12)
            }
            if ($groupEntry.Reserved -ne 0 -or $groupEntry.ResourceId -eq 0) {
                throw "RT_GROUP_ICON entry $index has a nonzero reserved byte or invalid resource ID."
            }
            foreach ($field in @('Width', 'Height', 'ColorCount', 'Reserved', 'Planes', 'BitCount', 'Length')) {
                if ($groupEntry.$field -ne $icoEntry.$field) { throw "Icon entry $index metadata differs at $field." }
            }

            $iconResource = Get-ResourceBytes -Module $module -ResourceId $groupEntry.ResourceId -ResourceType 3 -Label "RT_ICON #$($groupEntry.ResourceId)"
            if ($iconResource.Size -ne $groupEntry.Length) { throw "RT_ICON #$($groupEntry.ResourceId) length differs from the group directory." }
            $icoPayload = [byte[]]::new([Int32]$icoEntry.Length)
            [Array]::Copy($icoBytes, [Int32]$icoEntry.Offset, $icoPayload, 0, $icoPayload.Length)
            for ($byteIndex = 0; $byteIndex -lt $icoPayload.Length; $byteIndex++) {
                if ($iconResource.Bytes[$byteIndex] -ne $icoPayload[$byteIndex]) {
                    throw "RT_ICON #$($groupEntry.ResourceId) payload differs from ICO entry $index at byte $byteIndex."
                }
            }
            [pscustomobject]@{
                ResourceId = $groupEntry.ResourceId
                Width = $groupEntry.Width
                Height = $groupEntry.Height
                Bytes = $iconResource.Size
                Sha256 = Get-Sha256Hex $iconResource.Bytes
            }
        }
    )

    [pscustomobject][ordered]@{
        Verified = $true
        ExecutablePath = $executable
        ExecutableSha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
        IcoPath = $ico
        IcoSha256 = Get-Sha256Hex $icoBytes
        GroupResourceId = 1
        GroupResourceSha256 = Get-Sha256Hex $groupBytes
        EntryCount = $groupCount
        IconPayloads = @($payloadResults)
    }
}
finally {
    if (![TrackBIconResourceNative]::FreeLibrary($module)) { throw (Get-Win32Failure 'FreeLibrary') }
}
