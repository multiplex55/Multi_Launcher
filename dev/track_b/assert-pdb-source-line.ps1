#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [Parameter(Mandatory = $true)][string]$PdbPath,
    [Parameter(Mandatory = $true)][string]$SourcePath,
    [Parameter(Mandatory = $true)][ValidateRange(1, 2147483647)][int]$ExpectedLine,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ExpectedExecutableSha256,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ExpectedPdbSha256,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ExpectedSourceSha256,
    [Parameter(Mandatory = $true)][string]$ResultPath
)

$ErrorActionPreference = 'Stop'
if (!$IsWindows) { throw 'PDB line verification requires Windows.' }
if (![Environment]::Is64BitProcess) { throw 'PDB line verification requires 64-bit PowerShell.' }

if ($null -eq ('TrackBPdbLineNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct TrackBSrcCodeInfoW
{
    public uint SizeOfStruct;
    public IntPtr Key;
    public ulong ModBase;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 261)] public string Obj;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 261)] public string FileName;
    public uint LineNumber;
    public ulong Address;
}

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct TrackBImageHlpModuleW64
{
    public uint SizeOfStruct;
    public ulong BaseOfImage;
    public uint ImageSize;
    public uint TimeDateStamp;
    public uint CheckSum;
    public uint NumSyms;
    public uint SymType;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string ModuleName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string ImageName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string LoadedImageName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string LoadedPdbName;
    public uint CVSig;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 780)] public string CVData;
    public uint PdbSig;
    public Guid PdbSig70;
    public uint PdbAge;
    [MarshalAs(UnmanagedType.Bool)] public bool PdbUnmatched;
    [MarshalAs(UnmanagedType.Bool)] public bool DbgUnmatched;
    [MarshalAs(UnmanagedType.Bool)] public bool LineNumbers;
    [MarshalAs(UnmanagedType.Bool)] public bool GlobalSymbols;
    [MarshalAs(UnmanagedType.Bool)] public bool TypeInfo;
    [MarshalAs(UnmanagedType.Bool)] public bool SourceIndexed;
    [MarshalAs(UnmanagedType.Bool)] public bool Publics;
    public uint MachineType;
    public uint Reserved;
}

[StructLayout(LayoutKind.Sequential)]
public struct TrackBImageHlpLineW64
{
    public uint SizeOfStruct;
    public IntPtr Key;
    public uint LineNumber;
    public IntPtr FileName;
    public ulong Address;
}

public sealed class TrackBPdbLineMatch
{
    public string PdbSourcePath;
    public uint PdbLine;
    public ulong Address;
    public uint Displacement;
    public string ResolvedSourcePath;
    public uint ResolvedLine;
    public ulong ResolvedAddress;
}

public sealed class TrackBPdbLineLookup
{
    public string DbgHelpPath;
    public ulong ModuleBase;
    public string LoadedImageName;
    public string LoadedPdbName;
    public uint SymType;
    public Guid PdbSig70;
    public uint PdbAge;
    public bool PdbUnmatched;
    public bool LineNumbers;
    public bool EnumerationSucceeded;
    public bool SymCleanupSucceeded;
    public bool DbgHelpLibraryReleased;
    public TrackBPdbLineMatch[] Matches;
}

public static class TrackBPdbLineNative
{
    [UnmanagedFunctionPointer(CallingConvention.Winapi, CharSet = CharSet.Unicode)]
    private delegate bool EnumLinesCallback(ref TrackBSrcCodeInfoW lineInfo, IntPtr context);

    private static readonly EnumLinesCallback LineCallback = OnLine;
    private static List<TrackBSrcCodeInfoW> currentMatches;
    private static string currentSourcePath;
    private static uint currentLine;

    [DllImport("kernel32.dll", EntryPoint = "LoadLibraryExW", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    private static extern IntPtr LoadLibraryExW(string fileName, IntPtr file, uint flags);

    [DllImport("kernel32.dll", EntryPoint = "GetModuleFileNameW", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    private static extern uint GetModuleFileNameW(IntPtr module, StringBuilder fileName, uint size);

    [DllImport("kernel32.dll", EntryPoint = "FreeLibrary", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool FreeLibrary(IntPtr module);

    [DllImport("kernel32.dll", EntryPoint = "GetCurrentProcess", ExactSpelling = true)]
    private static extern IntPtr GetCurrentProcess();

    [DllImport("dbghelp.dll", EntryPoint = "SymInitializeW", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SymInitializeW(IntPtr process, string searchPath, bool invadeProcess);

    [DllImport("dbghelp.dll", EntryPoint = "SymSetSearchPathW", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SymSetSearchPathW(IntPtr process, string searchPath);

    [DllImport("dbghelp.dll", EntryPoint = "SymSetOptions", ExactSpelling = true)]
    private static extern uint SymSetOptions(uint options);

    [DllImport("dbghelp.dll", EntryPoint = "SymLoadModuleExW", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    private static extern ulong SymLoadModuleExW(
        IntPtr process, IntPtr file, string imageName, string moduleName,
        ulong baseOfDll, uint dllSize, IntPtr data, uint flags);

    [DllImport("dbghelp.dll", EntryPoint = "SymGetModuleInfoW64", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SymGetModuleInfoW64(IntPtr process, ulong address, ref TrackBImageHlpModuleW64 moduleInfo);

    [DllImport("dbghelp.dll", EntryPoint = "SymEnumLinesW", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SymEnumLinesW(
        IntPtr process, ulong baseAddress, string obj, string file,
        EnumLinesCallback callback, IntPtr context);

    [DllImport("dbghelp.dll", EntryPoint = "SymGetLineFromAddrW64", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SymGetLineFromAddrW64(
        IntPtr process, ulong address, out uint displacement, ref TrackBImageHlpLineW64 line);

    [DllImport("dbghelp.dll", EntryPoint = "SymCleanup", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SymCleanup(IntPtr process);

    private static bool OnLine(ref TrackBSrcCodeInfoW lineInfo, IntPtr context)
    {
        if (lineInfo.LineNumber == currentLine && SamePath(lineInfo.FileName, currentSourcePath))
            currentMatches.Add(lineInfo);
        return true;
    }

    private static bool SamePath(string left, string right)
    {
        try
        {
            string normalizedLeft = Path.GetFullPath(left).Replace('/', '\\');
            string normalizedRight = Path.GetFullPath(right).Replace('/', '\\');
            if (normalizedLeft.StartsWith("\\\\?\\", StringComparison.Ordinal)) normalizedLeft = normalizedLeft.Substring(4);
            if (normalizedRight.StartsWith("\\\\?\\", StringComparison.Ordinal)) normalizedRight = normalizedRight.Substring(4);
            return String.Equals(normalizedLeft, normalizedRight, StringComparison.OrdinalIgnoreCase);
        }
        catch { return false; }
    }

    private static Exception LastError(string api)
    {
        return new Win32Exception(Marshal.GetLastWin32Error(), api + " failed");
    }

    private static Exception Combine(Exception existing, Exception next)
    {
        return existing == null ? next : new AggregateException(existing, next);
    }

    public static TrackBPdbLineLookup Lookup(string imagePath, string pdbPath, string sourcePath, uint lineNumber)
    {
        if (IntPtr.Size != 8) throw new PlatformNotSupportedException("64-bit DbgHelp structure layout is required.");
        imagePath = Path.GetFullPath(imagePath);
        pdbPath = Path.GetFullPath(pdbPath);
        sourcePath = Path.GetFullPath(sourcePath);
        string symbolDirectory = Path.GetDirectoryName(pdbPath);
        string systemDbgHelp = Path.Combine(Environment.SystemDirectory, "dbghelp.dll");
        const uint LOAD_LIBRARY_SEARCH_SYSTEM32 = 0x00000800;
        IntPtr dbgHelpModule = LoadLibraryExW(systemDbgHelp, IntPtr.Zero, LOAD_LIBRARY_SEARCH_SYSTEM32);
        if (dbgHelpModule == IntPtr.Zero) throw LastError("LoadLibraryExW(System32 DbgHelp)");

        bool initialized = false;
        IntPtr process = GetCurrentProcess();
        TrackBPdbLineLookup lookupResult = null;
        Exception failure = null;
        try
        {
            StringBuilder actualDbgHelpPath = new StringBuilder(32768);
            uint copied = GetModuleFileNameW(dbgHelpModule, actualDbgHelpPath, (uint)actualDbgHelpPath.Capacity);
            if (copied == 0 || copied >= actualDbgHelpPath.Capacity) throw LastError("GetModuleFileNameW(DbgHelp)");

            if (!SymInitializeW(process, symbolDirectory, false)) throw LastError("SymInitializeW");
            initialized = true;
            if (!SymSetSearchPathW(process, symbolDirectory)) throw LastError("SymSetSearchPathW(local PDB directory)");

            // LOAD_LINES + EXACT_SYMBOLS; ignore process-wide NT symbol paths,
            // fail on critical errors, and suppress UI prompts. No server path
            // or network symbol provider is supplied.
            SymSetOptions(0x00000010u | 0x00000200u | 0x00000400u | 0x00001000u | 0x00080000u);
            ulong moduleBase = SymLoadModuleExW(process, IntPtr.Zero, imagePath, null, 0, 0, IntPtr.Zero, 0);
            if (moduleBase == 0) throw LastError("SymLoadModuleExW");

            TrackBImageHlpModuleW64 moduleInfo = new TrackBImageHlpModuleW64();
            moduleInfo.SizeOfStruct = (uint)Marshal.SizeOf(typeof(TrackBImageHlpModuleW64));
            if (!SymGetModuleInfoW64(process, moduleBase, ref moduleInfo)) throw LastError("SymGetModuleInfoW64");

            currentMatches = new List<TrackBSrcCodeInfoW>();
            currentSourcePath = sourcePath;
            currentLine = lineNumber;
            bool enumerationSucceeded = SymEnumLinesW(
                process, moduleBase, null, "*" + Path.GetFileName(sourcePath), LineCallback, IntPtr.Zero);
            int enumError = Marshal.GetLastWin32Error();
            if (!enumerationSucceeded) throw new Win32Exception(enumError, "SymEnumLinesW failed");
            if (currentMatches.Count == 0)
                throw new InvalidOperationException("The loaded PDB has no line entry for the exact source path and requested line.");

            List<TrackBPdbLineMatch> resolved = new List<TrackBPdbLineMatch>();
            foreach (TrackBSrcCodeInfoW match in currentMatches)
            {
                TrackBImageHlpLineW64 lineInfo = new TrackBImageHlpLineW64();
                lineInfo.SizeOfStruct = (uint)Marshal.SizeOf(typeof(TrackBImageHlpLineW64));
                uint displacement;
                if (!SymGetLineFromAddrW64(process, match.Address, out displacement, ref lineInfo))
                    throw LastError("SymGetLineFromAddrW64");
                string resolvedPath = Marshal.PtrToStringUni(lineInfo.FileName);
                resolved.Add(new TrackBPdbLineMatch {
                    PdbSourcePath = match.FileName,
                    PdbLine = match.LineNumber,
                    Address = match.Address,
                    Displacement = displacement,
                    ResolvedSourcePath = resolvedPath,
                    ResolvedLine = lineInfo.LineNumber,
                    ResolvedAddress = lineInfo.Address
                });
            }

            lookupResult = new TrackBPdbLineLookup {
                DbgHelpPath = actualDbgHelpPath.ToString(),
                ModuleBase = moduleInfo.BaseOfImage,
                LoadedImageName = moduleInfo.LoadedImageName,
                LoadedPdbName = moduleInfo.LoadedPdbName,
                SymType = moduleInfo.SymType,
                PdbSig70 = moduleInfo.PdbSig70,
                PdbAge = moduleInfo.PdbAge,
                PdbUnmatched = moduleInfo.PdbUnmatched,
                LineNumbers = moduleInfo.LineNumbers,
                EnumerationSucceeded = enumerationSucceeded,
                Matches = resolved.ToArray()
            };
        }
        catch (Exception error)
        {
            failure = error;
        }
        finally
        {
            currentMatches = null;
            currentSourcePath = null;
            currentLine = 0;
            try
            {
                if (initialized)
                {
                    if (!SymCleanup(process)) failure = Combine(failure, LastError("SymCleanup"));
                    else if (lookupResult != null) lookupResult.SymCleanupSucceeded = true;
                }
            }
            catch (Exception error)
            {
                failure = Combine(failure, error);
            }
            finally
            {
                try
                {
                    if (!FreeLibrary(dbgHelpModule)) failure = Combine(failure, LastError("FreeLibrary(DbgHelp)"));
                    else if (lookupResult != null) lookupResult.DbgHelpLibraryReleased = true;
                }
                catch (Exception error)
                {
                    failure = Combine(failure, error);
                }
            }
        }
        if (failure != null)
            throw new InvalidOperationException("DbgHelp lookup or cleanup failed: " + failure.ToString(), failure);
        return lookupResult;
    }
}
'@ | Out-Null
}

function Read-At {
    param([Parameter(Mandatory)][IO.BinaryReader]$Reader, [Parameter(Mandatory)][long]$Offset, [Parameter(Mandatory)][int]$Count)
    if ($Offset -lt 0 -or $Count -lt 0 -or $Offset + $Count -gt $Reader.BaseStream.Length) {
        throw 'Executable PE structure points outside the file.'
    }
    $null = $Reader.BaseStream.Seek($Offset, [IO.SeekOrigin]::Begin)
    $bytes = $Reader.ReadBytes($Count)
    if ($bytes.Length -ne $Count) { throw 'Executable PE structure is truncated.' }
    return ,$bytes
}

function Get-PeCodeViewIdentity {
    param([Parameter(Mandatory)][string]$Path)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $reader = [IO.BinaryReader]::new($stream)
    try {
        $peOffset = [BitConverter]::ToUInt32((Read-At $reader 0x3c 4), 0)
        if ([Text.Encoding]::ASCII.GetString((Read-At $reader $peOffset 4)) -cne "PE`0`0") {
            throw 'Executable has no valid PE signature.'
        }
        $sectionCount = [BitConverter]::ToUInt16((Read-At $reader ($peOffset + 6) 2), 0)
        $optionalSize = [BitConverter]::ToUInt16((Read-At $reader ($peOffset + 20) 2), 0)
        $optionalOffset = [long]$peOffset + 24
        $magic = [BitConverter]::ToUInt16((Read-At $reader $optionalOffset 2), 0)
        $directoryOffset = switch ($magic) {
            0x10b { 96 }
            0x20b { 112 }
            default { throw ('Unsupported PE optional-header magic: 0x{0:x}' -f $magic) }
        }
        $directoryCountOffset = if ($magic -eq 0x10b) { 92 } else { 108 }
        $directoryCount = [BitConverter]::ToUInt32((Read-At $reader ($optionalOffset + $directoryCountOffset) 4), 0)
        if ($directoryCount -le 6 -or ($directoryOffset + (7 * 8)) -gt $optionalSize) {
            throw 'Executable has no PE debug data directory.'
        }
        $debugDirectoryEntry = Read-At $reader ($optionalOffset + $directoryOffset + (6 * 8)) 8
        $debugRva = [BitConverter]::ToUInt32($debugDirectoryEntry, 0)
        $debugSize = [BitConverter]::ToUInt32($debugDirectoryEntry, 4)
        if ($debugRva -eq 0 -or $debugSize -lt 28) { throw 'Executable PE debug directory is absent or truncated.' }

        $sectionOffset = $optionalOffset + $optionalSize
        $rawDebugOffset = $null
        for ($index = 0; $index -lt $sectionCount; $index++) {
            $section = Read-At $reader ($sectionOffset + (40 * $index)) 40
            $virtualSize = [BitConverter]::ToUInt32($section, 8)
            $virtualAddress = [BitConverter]::ToUInt32($section, 12)
            $rawSize = [BitConverter]::ToUInt32($section, 16)
            $rawPointer = [BitConverter]::ToUInt32($section, 20)
            $span = [Math]::Max([UInt64]$virtualSize, [UInt64]$rawSize)
            if ([UInt64]$debugRva -ge [UInt64]$virtualAddress -and
                [UInt64]$debugRva -lt ([UInt64]$virtualAddress + $span)) {
                $delta = [UInt64]$debugRva - [UInt64]$virtualAddress
                if ($delta -ge $rawSize) { throw 'PE debug directory is not backed by raw section data.' }
                $rawDebugOffset = [long]$rawPointer + [long]$delta
                break
            }
        }
        if ($null -eq $rawDebugOffset) { throw 'PE debug directory RVA does not map to a section.' }

        $entryCount = [Math]::Floor([double]$debugSize / 28.0)
        for ($entryIndex = 0; $entryIndex -lt $entryCount; $entryIndex++) {
            $entryOffset = $rawDebugOffset + (28 * $entryIndex)
            $entry = Read-At $reader $entryOffset 28
            $type = [BitConverter]::ToUInt32($entry, 12)
            if ($type -ne 2) { continue }
            $dataSize = [BitConverter]::ToUInt32($entry, 16)
            $dataPointer = [BitConverter]::ToUInt32($entry, 24)
            if ($dataSize -lt 24) { throw 'PE CodeView record is truncated.' }
            $record = Read-At $reader $dataPointer ([int][Math]::Min([UInt64]$dataSize, [UInt64]4096))
            if ([Text.Encoding]::ASCII.GetString($record, 0, 4) -cne 'RSDS') { continue }
            $guidBytes = [byte[]]::new(16)
            [Array]::Copy($record, 4, $guidBytes, 0, 16)
            $guid = [Guid]::new($guidBytes)
            $age = [BitConverter]::ToUInt32($record, 20)
            $pathEnd = [Array]::IndexOf($record, [byte]0, 24)
            if ($pathEnd -lt 0) { $pathEnd = $record.Length }
            $codeViewPath = [Text.Encoding]::UTF8.GetString($record, 24, $pathEnd - 24)
            return [pscustomobject]@{ Guid = $guid.ToString('D').ToLowerInvariant(); Age = $age; Path = $codeViewPath }
        }
        throw 'Executable has no RSDS CodeView identity.'
    }
    finally {
        $reader.Dispose()
        $stream.Dispose()
    }
}

function Get-FileIdentity {
    param([Parameter(Mandatory)][string]$Path)
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Required input is not a file: $Path" }
    $file = Get-Item -LiteralPath $Path -ErrorAction Stop
    [pscustomobject]@{
        Path = [IO.Path]::GetFullPath($Path)
        LengthBytes = [Int64]$file.Length
        Sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function ConvertTo-ComparablePath {
    param([Parameter(Mandatory)][string]$Path)
    $fullPath = [IO.Path]::GetFullPath($Path).Replace('/', '\')
    if ($fullPath.StartsWith('\\?\', [StringComparison]::Ordinal)) { $fullPath = $fullPath.Substring(4) }
    return $fullPath.TrimEnd('\')
}

$resolvedExecutable = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $ExecutablePath -ErrorAction Stop).ProviderPath)
$resolvedPdb = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $PdbPath -ErrorAction Stop).ProviderPath)
$resolvedSource = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $SourcePath -ErrorAction Stop).ProviderPath)
$resultFile = [IO.Path]::GetFullPath($ResultPath)
$resultDirectory = [IO.Path]::GetDirectoryName($resultFile)
if (![IO.Directory]::Exists($resultDirectory)) { [IO.Directory]::CreateDirectory($resultDirectory) | Out-Null }

$result = [ordered]@{
    SchemaVersion = 1
    Status = 'FAIL'
    StartedUtc = [DateTimeOffset]::UtcNow.ToString('o')
    EndedUtc = $null
    Error = $null
    Executable = $null
    Pdb = $null
    Source = $null
    ExpectedLine = $ExpectedLine
    PeCodeView = $null
    DbgHelp = $null
    DbgHelpCleanupVerified = $false
    LineMatches = @()
    LocalOnlySymbolSearch = $true
}

try {
    if ($ExpectedExecutableSha256 -cne (Get-FileHash -LiteralPath $resolvedExecutable -Algorithm SHA256).Hash.ToLowerInvariant()) {
        throw 'Executable SHA-256 differs from its accepted artifact binding.'
    }
    if ($ExpectedPdbSha256 -cne (Get-FileHash -LiteralPath $resolvedPdb -Algorithm SHA256).Hash.ToLowerInvariant()) {
        throw 'PDB SHA-256 differs from its accepted artifact binding.'
    }
    if ($ExpectedSourceSha256 -cne (Get-FileHash -LiteralPath $resolvedSource -Algorithm SHA256).Hash.ToLowerInvariant()) {
        throw 'Source SHA-256 differs from the restored accepted source binding.'
    }

    $executableBefore = Get-FileIdentity $resolvedExecutable
    $pdbBefore = Get-FileIdentity $resolvedPdb
    $sourceBefore = Get-FileIdentity $resolvedSource
    $codeView = Get-PeCodeViewIdentity $resolvedExecutable
    $result.Executable = $executableBefore
    $result.Pdb = $pdbBefore
    $result.Source = $sourceBefore
    $result.PeCodeView = $codeView

    Push-Location -LiteralPath (Get-Location).Path
    try {
        $lookup = [TrackBPdbLineNative]::Lookup($resolvedExecutable, $resolvedPdb, $resolvedSource, [uint32]$ExpectedLine)
    }
    finally { Pop-Location }

    $expectedLoadedImage = ConvertTo-ComparablePath $resolvedExecutable
    $actualLoadedImage = ConvertTo-ComparablePath ([string]$lookup.LoadedImageName)
    if (!$expectedLoadedImage.Equals($actualLoadedImage, [StringComparison]::OrdinalIgnoreCase)) {
        throw "DbgHelp loaded a different executable image: $($lookup.LoadedImageName)"
    }
    $expectedLoadedPdb = ConvertTo-ComparablePath $resolvedPdb
    $actualLoadedPdb = ConvertTo-ComparablePath ([string]$lookup.LoadedPdbName)
    if (!$expectedLoadedPdb.Equals($actualLoadedPdb, [StringComparison]::OrdinalIgnoreCase)) {
        throw "DbgHelp loaded a different PDB path: $($lookup.LoadedPdbName)"
    }
    if ([uint32]$lookup.SymType -ne 3) { throw "DbgHelp symbol type $($lookup.SymType) is not SymPdb." }
    if ($lookup.PdbUnmatched) { throw 'DbgHelp reported the loaded PDB as unmatched.' }
    if (!$lookup.LineNumbers) { throw 'DbgHelp reported that line information is unavailable.' }
    if ([string]$lookup.PdbSig70 -ine [string]$codeView.Guid -or [uint32]$lookup.PdbAge -ne [uint32]$codeView.Age) {
        throw 'PE CodeView GUID/age differs from the matching loaded PDB identity.'
    }

    $lineResults = @(
        foreach ($match in $lookup.Matches) {
            $pdbPath = ConvertTo-ComparablePath ([string]$match.PdbSourcePath)
            $resolvedPath = ConvertTo-ComparablePath ([string]$match.ResolvedSourcePath)
            $expectedPath = ConvertTo-ComparablePath $resolvedSource
            if (!$pdbPath.Equals($expectedPath, [StringComparison]::OrdinalIgnoreCase) -or
                !$resolvedPath.Equals($expectedPath, [StringComparison]::OrdinalIgnoreCase) -or
                [uint32]$match.PdbLine -ne [uint32]$ExpectedLine -or
                [uint32]$match.ResolvedLine -ne [uint32]$ExpectedLine) {
                throw 'The PDB line entry and SymGetLineFromAddrW64 resolution do not identify the exact expected source path and line.'
            }
            [pscustomobject][ordered]@{
                PdbSourcePath = [string]$match.PdbSourcePath
                PdbLine = [uint32]$match.PdbLine
                Address = ('0x{0:x}' -f [UInt64]$match.Address)
                Displacement = [uint32]$match.Displacement
                ResolvedSourcePath = [string]$match.ResolvedSourcePath
                ResolvedLine = [uint32]$match.ResolvedLine
                ResolvedAddress = ('0x{0:x}' -f [UInt64]$match.ResolvedAddress)
            }
        }
    )
    $result.DbgHelp = [pscustomobject][ordered]@{
        Path = [string]$lookup.DbgHelpPath
        LoadedImageName = [string]$lookup.LoadedImageName
        LoadedPdbName = [string]$lookup.LoadedPdbName
        SymbolType = [uint32]$lookup.SymType
        PdbGuid = ([Guid]$lookup.PdbSig70).ToString('D').ToLowerInvariant()
        PdbAge = [uint32]$lookup.PdbAge
        PdbUnmatched = [bool]$lookup.PdbUnmatched
        LineNumbers = [bool]$lookup.LineNumbers
        EnumerationSucceeded = [bool]$lookup.EnumerationSucceeded
        SymCleanupSucceeded = [bool]$lookup.SymCleanupSucceeded
        DbgHelpLibraryReleased = [bool]$lookup.DbgHelpLibraryReleased
    }
    if (!$lookup.SymCleanupSucceeded -or !$lookup.DbgHelpLibraryReleased) {
        throw 'DbgHelp session cleanup and library release were not both verified.'
    }
    $result.DbgHelpCleanupVerified = $true
    $result.LineMatches = $lineResults

    $result.ExecutableAfter = Get-FileIdentity $resolvedExecutable
    $result.PdbAfter = Get-FileIdentity $resolvedPdb
    $result.SourceAfter = Get-FileIdentity $resolvedSource
    foreach ($pair in @(
        @($result.Executable, $result.ExecutableAfter),
        @($result.Pdb, $result.PdbAfter),
        @($result.Source, $result.SourceAfter)
    )) {
        if ($pair[0].LengthBytes -ne $pair[1].LengthBytes -or $pair[0].Sha256 -cne $pair[1].Sha256) {
            throw "Input identity changed during PDB source-line verification: $($pair[0].Path)"
        }
    }
    if ($result.ExecutableAfter.Sha256 -cne $ExpectedExecutableSha256.ToLowerInvariant() -or
        $result.PdbAfter.Sha256 -cne $ExpectedPdbSha256.ToLowerInvariant() -or
        $result.SourceAfter.Sha256 -cne $ExpectedSourceSha256.ToLowerInvariant()) {
        throw 'An executable, PDB, or source binding changed during PDB source-line verification.'
    }
    $result.Status = 'PASS'
}
catch {
    $result.Error = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
}
finally {
    $result.EndedUtc = [DateTimeOffset]::UtcNow.ToString('o')
}

$json = ConvertTo-Json -InputObject $result -Depth 10
try {
    [IO.File]::WriteAllText($resultFile, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}
catch {
    Write-Error "Could not persist PDB source-line result at '$resultFile': $($_.Exception.Message)"
    exit 1
}
Write-Output $json
if ($result.Status -cne 'PASS') { exit 1 }
