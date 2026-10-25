#requires -Version 7.0
[CmdletBinding()]
param([Parameter(Mandatory)][string[]]$Paths)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot 'owned_process.ps1')

$records = [System.Collections.Generic.List[object]]::new()
$expectedRoot = [IO.Path]::GetFullPath((Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Microsoft Visual Studio')).TrimEnd('\')
$allowedPath = '^\\\\\?\\[A-Za-z]:\\Program Files\\Microsoft Visual Studio\\20[0-9]{2}\\(Community|Professional|Enterprise|BuildTools)\\VC\\Tools\\MSVC\\[0-9]+\.[0-9]+\.[0-9]+\\bin\\Hostx64\\x64\\vctip\.exe$'

foreach ($path in $Paths) {
    $record = [ordered]@{ Path = $path; VerificationSource = 'Windows Authenticode plus held canonical file identity'; SignatureStatus = 'Unknown'; Error = $null }
    $stream = $null
    try {
        if ($path -notmatch '(?i)\\Microsoft Visual Studio\\20[0-9]{2}\\(Community|Professional|Enterprise|BuildTools)\\VC\\Tools\\MSVC\\[0-9]+\.[0-9]+\.[0-9]+\\bin\\Hostx64\\x64\\vctip\.exe$') {
            throw 'Candidate is outside the installed Visual Studio MSVC Hostx64 x64 tool path.'
        }
        $stream = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        $file = [MultiLauncher.Dev.OwnedProcessNative]::GetOpenFileDescriptor($stream.SafeFileHandle)
        if (!$file.CanonicalPath.StartsWith('\\?\', [StringComparison]::OrdinalIgnoreCase)) {
            throw 'The executable handle did not yield a canonical extended path.'
        }
        $canonicalWithoutPrefix = $file.CanonicalPath.Substring(4)
        if (!$canonicalWithoutPrefix.StartsWith($expectedRoot, [StringComparison]::OrdinalIgnoreCase) -or
            $file.CanonicalPath -notmatch $allowedPath) {
            throw 'The canonical executable path is not an installed Visual Studio MSVC VCTIP image.'
        }
        $stream.Position = 0
        $hash = (Get-FileHash -InputStream $stream -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
        $stream.Position = 0
        $signature = Get-AuthenticodeSignature -LiteralPath $path -ErrorAction Stop
        $record.SignatureStatus = [string]$signature.Status
        $record.SignatureStatusMessage = [string]$signature.StatusMessage
        if ($null -ne $signature.SignerCertificate) {
            $certificate = $signature.SignerCertificate
            $decodedSubject = $certificate.SubjectName.Decode([Security.Cryptography.X509Certificates.X500DistinguishedNameFlags]::UseNewLines)
            $org = [regex]::Match($decodedSubject, '(?im)^O\s*=\s*Microsoft Corporation\s*$')
            $certificateHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($certificate.RawData)).ToLowerInvariant()
            $record.SignerThumbprint = [string]$certificate.Thumbprint
            $record.SignerSubject = [string]$certificate.Subject
            $record.SignerOrganization = if ($org.Success) { 'Microsoft Corporation' } else { $null }
            $record.SignerCertificateSha256 = $certificateHash
        }
        $record.CanonicalPath = [string]$file.CanonicalPath
        $record.FileId = [string]$file.FileId
        $record.Sha256 = $hash
        $record.ValidMicrosoftSignature = [string]$signature.Status -ceq 'Valid' -and
            $record.SignerOrganization -ceq 'Microsoft Corporation' -and
            $record.SignerThumbprint -match '^[0-9A-Fa-f]{40,64}$' -and
            $record.SignerCertificateSha256 -match '^[0-9a-f]{64}$'
    }
    catch {
        $record.Error = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
    }
    finally {
        if ($stream) { $stream.Dispose() }
    }
    $records.Add([pscustomobject]$record)
}

$result = [pscustomobject][ordered]@{
    SchemaVersion = 1
    Policy = 'verified-msvc-vctip'
    CompletedUtc = [DateTimeOffset]::UtcNow.ToString('o')
    Candidates = @($records)
}
[Console]::Out.WriteLine((ConvertTo-Json -InputObject $result -Depth 8 -Compress))
if (@($records | Where-Object { $_.ValidMicrosoftSignature }).Count -eq 0) { exit 2 }
