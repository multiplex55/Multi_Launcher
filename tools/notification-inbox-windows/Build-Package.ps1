[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Launcher,
    [Parameter(Mandatory)] [string] $OutputDirectory,
    [Parameter(Mandatory)] [string] $SdkBin,
    [Parameter(Mandatory)] [string] $PublisherSubject,
    [Parameter(Mandatory)] [version] $PackageVersion
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($PackageVersion.Major -lt 1 -or $PackageVersion.Revision -ne 0 -or
    $PackageVersion.Major -gt 65535 -or $PackageVersion.Minor -gt 65535 -or $PackageVersion.Build -gt 65535 -or
    $PackageVersion.Build -lt 0) {
    throw 'Use a four-component package version from 1.0.0.0 with a final component of zero.'
}
if ([string]::IsNullOrWhiteSpace($PublisherSubject)) {
    throw 'Specify the full subject of the Windows code-signing certificate.'
}
$launcherExecutable = (Resolve-Path -LiteralPath $Launcher).Path
if ([IO.Path]::GetExtension($launcherExecutable) -ne '.exe') {
    throw 'Pass the launcher executable from an ordinary Windows build.'
}
$tools = (Resolve-Path -LiteralPath $SdkBin).Path
$makeAppx = (Resolve-Path -LiteralPath (Join-Path $tools 'makeappx.exe')).Path
$manifestTool = (Resolve-Path -LiteralPath (Join-Path $tools 'mt.exe')).Path
$output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
if (Test-Path -LiteralPath $output) {
    throw 'Choose a new output directory. Existing launchers and packages are preserved.'
}
$null = New-Item -Path $output -ItemType Directory
$external = Join-Path $output 'External'
$package = Join-Path $output 'Package'
$assets = Join-Path $package 'Assets'
$null = New-Item -Path $external, $package, $assets -ItemType Directory

$embeddedName = 'MultiLauncher.NotificationInbox'
$applicationId = 'MultiLauncher'
$copiedLauncher = Join-Path $external 'multi_launcher.exe'
Copy-Item -LiteralPath $launcherExecutable -Destination $copiedLauncher
$packagePath = Join-Path $package 'AppxManifest.xml'
[xml] $packageManifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'AppxManifest.xml') -Raw
$packageManifest.Package.Identity.SetAttribute('Publisher', $PublisherSubject)
$packageManifest.Package.Identity.SetAttribute('Version', $PackageVersion.ToString())
$packageManifest.Save($packagePath)

# Preserve any linker/launcher manifest already present, then add only the sparse identity.
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class SparseInboxManifestResource {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr FindResource(IntPtr image, IntPtr name, IntPtr kind);
    [DllImport("kernel32.dll")]
    private static extern bool FreeLibrary(IntPtr image);

    public static bool Exists(string path) {
        // Read resources without loading or running executable code.
        IntPtr image = LoadLibraryEx(path, IntPtr.Zero, 0x22);
        if (image == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            if (FindResource(image, new IntPtr(1), new IntPtr(24)) != IntPtr.Zero) return true;
            int error = Marshal.GetLastWin32Error();
            if (error == 1812 || error == 1813 || error == 1814) return false;
            throw new Win32Exception(error);
        } finally {
            FreeLibrary(image);
        }
    }
}
'@
if ([SparseInboxManifestResource]::Exists($copiedLauncher)) {
    $existing = Join-Path $output 'existing-executable.manifest'
    & $manifestTool -nologo "-inputresource:$copiedLauncher;#1" "-out:$existing"
    if ($LASTEXITCODE -ne 0) {
        throw 'Could not extract the existing executable manifest.'
    }
    [xml] $assembly = Get-Content -LiteralPath $existing -Raw
} else {
    [xml] $assembly = @'
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <assemblyIdentity name="MultiLauncher.NotificationInbox" version="1.0.0.0" />
</assembly>
'@
}
if ($assembly.DocumentElement.LocalName -ne 'assembly' -or
    $assembly.SelectSingleNode("/*[local-name()='assembly']/*[local-name()='msix']")) {
    throw 'The input launcher must have a Win32 assembly manifest without an existing package identity.'
}
$identity = $assembly.CreateElement('msix', 'urn:schemas-microsoft-com:msix.v1')
$identity.SetAttribute('publisher', $PublisherSubject)
$identity.SetAttribute('packageName', $embeddedName)
$identity.SetAttribute('applicationId', $applicationId)
$null = $assembly.DocumentElement.AppendChild($identity)
$updated = Join-Path $output 'launcher-with-identity.manifest'
$assembly.Save($updated)
& $manifestTool -nologo -manifest $updated "-outputresource:$copiedLauncher;#1"
if ($LASTEXITCODE -ne 0) {
    throw 'Could not embed the sparse package identity in the copied launcher.'
}

Add-Type -AssemblyName System.Drawing
$logoPath = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../Resources/Green_MultiLauncher.png')).Path
$image = [System.Drawing.Image]::FromFile($logoPath)
try {
    $sizes = @{StoreLogo=50; Square44x44Logo=44; Square150x150Logo=150}
    foreach ($name in $sizes.Keys) {
        $size = $sizes[$name]
        $bitmap = [System.Drawing.Bitmap]::new($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $canvas = [System.Drawing.Graphics]::FromImage($bitmap)
            try {
                $canvas.Clear([System.Drawing.Color]::Transparent)
                $canvas.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $canvas.DrawImage($image, 0, 0, $size, $size)
            } finally {
                $canvas.Dispose()
            }
            $bitmap.Save((Join-Path $assets "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $bitmap.Dispose()
        }
    }
} finally {
    $image.Dispose()
}

$unsigned = Join-Path $output 'MultiLauncher.NotificationInbox.unsigned.msix'
# Ordinary pack validation expects the executable inside the package. Sparse packages reference External instead.
& $makeAppx pack /nv /d $package /p $unsigned
if ($LASTEXITCODE -ne 0) {
    throw 'Could not build the unsigned sparse MSIX.'
}
Write-Output $unsigned
