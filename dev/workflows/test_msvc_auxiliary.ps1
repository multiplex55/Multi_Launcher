#requires -Version 7.0
[CmdletBinding()]
param([string]$EvidenceDirectory = (Join-Path $env:TEMP ('msvc-auxiliary-fixtures-' + [guid]::NewGuid().ToString('N'))))

$ErrorActionPreference = 'Stop'
$evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
[IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null
. (Join-Path $PSScriptRoot 'owned_process.ps1')

$pwshCommand = Get-Command pwsh.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1
$pwshPath = if ($pwshCommand.Source) { $pwshCommand.Source } else { $pwshCommand.Path }
$cargoPath = 'C:\toolchain\cargo.exe'
$compilerPolicyCases = @(
    @{ Stage = 'compile'; Executable = $cargoPath; Arguments = @('build', '--bin', 'multi_launcher'); Expected = $true; Label = 'Cargo build' },
    @{ Stage = 'compile'; Executable = $cargoPath; Arguments = @('check', '--lib'); Expected = $true; Label = 'Cargo check' },
    @{ Stage = 'compile'; Executable = $cargoPath; Arguments = @('clippy', '--lib'); Expected = $true; Label = 'Cargo clippy' },
    @{ Stage = 'compile'; Executable = $cargoPath; Arguments = @('nextest', 'list', '--list-type', 'binaries-only'); Expected = $true; Label = 'Cargo Nextest list' },
    @{ Stage = 'compile'; Executable = $cargoPath; Arguments = @('test', '--no-run', '--bin', 'jump_mode_fixture'); Expected = $false; Label = 'Cargo test preparation' },
    @{ Stage = 'compile'; Executable = $cargoPath; Arguments = @('run', '--bin', 'jump_mode_fixture'); Expected = $false; Label = 'Cargo run' },
    @{ Stage = 'test'; Executable = $cargoPath; Arguments = @('clippy', '--lib'); Expected = $false; Label = 'Non-compile Cargo clippy' },
    @{ Stage = 'compile'; Executable = $pwshPath; Arguments = @('clippy', '--lib'); Expected = $false; Label = 'Non-Cargo compiler command' }
)
foreach ($case in $compilerPolicyCases) {
    $actual = Test-VerifiedMsvcCargoCommand -Stage $case.Stage -Executable $case.Executable -ArgumentList $case.Arguments
    if ([bool]$actual -ne [bool]$case.Expected) { throw "Verified MSVC compiler policy routing mismatch for $($case.Label)." }
}

$fixturePath = Join-Path $evidenceRoot 'owned compiler fixture.ps1'
$results = [System.Collections.Generic.List[object]]::new()
$script:fakeJobMemberPid = $null
$script:testImageMode = 'trusted'
$script:trustedDescriptor = $null
$script:readyToInspect = $null
$script:trustedChildPid = $null
$script:fixtureRootPid = $null
$script:injectOutputByteFailure = $false
$script:injectUnknownPostCleanup = $false
$script:originalTestIdentityAbsent = (Get-Command Test-CheckedOwnedIdentityAbsent).ScriptBlock

function Test-CheckedOwnedIdentityAbsent {
    param([Parameter(Mandatory)][object]$Identity)
    if ($script:injectUnknownPostCleanup -and [int]$Identity.ProcessId -ne [int]$script:fixtureRootPid) { return 'Unknown' }
    return (& $script:originalTestIdentityAbsent @PSBoundParameters)
}

@'
param([string]$Mode, [string]$ReadyFile, [int]$ChildCount = 1)
$ErrorActionPreference = 'Stop'
if ($Mode -eq 'child') {
    [Console]::Out.WriteLine('auxiliary child stdout marker')
    [Console]::Out.Flush()
    [Console]::Error.WriteLine('auxiliary child stderr marker')
    [Console]::Error.Flush()
    Start-Sleep -Seconds 30
    exit 0
}
if ($Mode -in @('short-parent', 'mixed-parent', 'failed-parent', 'tree')) {
    $children = [System.Collections.Generic.List[object]]::new()
    for ($index = 0; $index -lt $ChildCount; $index++) {
        $info = [Diagnostics.ProcessStartInfo]::new((Get-Command pwsh.exe -CommandType Application | Select-Object -First 1).Source)
        $info.UseShellExecute = $false
        $info.CreateNoWindow = $true
        foreach ($argument in @('-NoProfile', '-File', $PSCommandPath, '-Mode', 'child')) { $info.ArgumentList.Add($argument) }
        $child = [Diagnostics.Process]::Start($info)
        $children.Add([pscustomobject]@{ ProcessId = $child.Id; CreationDate = $child.StartTime.ToUniversalTime().ToString('o') })
        $child.Dispose()
    }
    $root = [Diagnostics.Process]::GetCurrentProcess()
    [pscustomobject]@{
        ParentPid = $PID
        ParentCreationDate = $root.StartTime.ToUniversalTime().ToString('o')
        Children = @($children)
    } | ConvertTo-Json -Depth 5 -Compress | Set-Content -LiteralPath $ReadyFile -Encoding utf8NoBOM
    [Console]::Out.WriteLine('compiler parent output before helper retirement')
    [Console]::Out.Flush()
    if ($Mode -eq 'tree') { Start-Sleep -Seconds 30; exit 0 }
    if ($Mode -eq 'failed-parent') { exit 7 }
    exit 0
}
throw "Unknown auxiliary fixture mode: $Mode"
'@ | Set-Content -LiteralPath $fixturePath -Encoding utf8NoBOM

$lease = [IO.File]::Open($pwshPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
try {
    $fileIdentity = [MultiLauncher.Dev.OwnedProcessNative]::GetOpenFileDescriptor($lease.SafeFileHandle)
    $script:trustedDescriptor = [pscustomobject][ordered]@{
        CanonicalPath = [string]$fileIdentity.CanonicalPath
        FileId = [string]$fileIdentity.FileId
        Sha256 = 'a' * 64
        SignatureStatus = 'Valid'
        SignerThumbprint = 'A' * 40
        SignerCertificateSha256 = 'b' * 64
        SignerOrganization = 'Microsoft Corporation'
        VerificationSource = 'SYNTHETIC TEST CONTRACT; not actual trust evidence'
    }
    $imageLease = [pscustomobject]@{ Descriptor = $script:trustedDescriptor; Lease = $lease }

    function Get-CheckedOwnedProcessIds {
        param([Parameter(Mandatory)][MultiLauncher.Dev.OwnedChild]$Owner)
        $script:fixtureRootPid = [uint32]$Owner.ProcessId
        $ids = @([MultiLauncher.Dev.OwnedProcessNative]::GetProcessIds($Owner))
        if ($null -ne $script:fakeJobMemberPid) { $ids += [uint32]$script:fakeJobMemberPid }
        return $ids
    }

    function Get-TestOwnedProcessImage {
        param([MultiLauncher.Dev.OwnedChild]$Owner, [uint32]$ProcessId)
        if ($script:testImageMode -eq 'identity-error') { throw 'injected live image identity read failure' }
        $actual = [MultiLauncher.Dev.OwnedProcessNative]::GetOwnedProcessImage($Owner, $ProcessId)
        $fileId = [string]$script:trustedDescriptor.FileId
        $path = [string]$script:trustedDescriptor.CanonicalPath
        if ($script:testImageMode -eq 'same-name-untrusted') { $fileId = 'UNTRUSTED-SAME-NAME-FILE-ID' }
        if ($script:testImageMode -eq 'mixed') {
            if ($null -eq $script:trustedChildPid -and (Test-Path -LiteralPath $script:readyToInspect)) {
                $ready = Get-Content -LiteralPath $script:readyToInspect -Raw | ConvertFrom-Json
                $script:trustedChildPid = [uint32]$ready.Children[0].ProcessId
            }
            if ([uint32]$ProcessId -ne [uint32]$script:trustedChildPid) { $fileId = 'UNTRUSTED-MIXED-MEMBER' }
        }
        return [pscustomobject]@{
            ProcessId = $actual.ProcessId
            CreationUtc = $actual.CreationUtc
            InOwnedJob = $actual.InOwnedJob
            Running = $actual.Running
            CanonicalPath = $path
            FileId = $fileId
            ActualCanonicalPath = [string]$actual.CanonicalPath
            ActualFileId = [string]$actual.FileId
        }
    }

    function Invoke-AuxiliaryFixture {
        param(
            [string]$Name,
            [string]$Mode,
            [int]$ChildCount = 1,
            [int]$TimeoutSeconds = 8,
            [int]$CleanupTimeoutSeconds = 1,
            [switch]$UsePolicy,
            [switch]$AddUnknownJobMember,
            [switch]$UseTestInspector,
            [switch]$InjectOutputByteFailure,
            [switch]$InjectUnknownPostCleanup
        )
        $directory = Join-Path $evidenceRoot $Name
        [IO.Directory]::CreateDirectory($directory) | Out-Null
        $ready = Join-Path $directory 'fixture-identities.json'
        $script:readyToInspect = $ready
        $script:trustedChildPid = $null
        $script:fixtureRootPid = $null
        $script:injectOutputByteFailure = $InjectOutputByteFailure.IsPresent
        $script:injectUnknownPostCleanup = $InjectUnknownPostCleanup.IsPresent
        $observationsPath = Join-Path $directory 'actual-process-images.jsonl'
        [IO.File]::WriteAllText($observationsPath, '', [Text.UTF8Encoding]::new($false))
        $injectionState = [pscustomobject]@{ OutputByteFailureInjected = $false }
        $inspector = {
            param($owner, $processId)
            if ($InjectOutputByteFailure -and !$injectionState.OutputByteFailureInjected) {
                if ($null -eq $owner.StdoutDrain) { throw 'Output byte failure fixture reached image classification before drain state existed.' }
                $flags = [Reflection.BindingFlags]::Instance -bor [Reflection.BindingFlags]::NonPublic
                $addRead = $owner.StdoutDrain.GetType().GetMethod('AddRead', $flags)
                if ($null -eq $addRead) { throw 'Output byte failure fixture could not resolve the test-only drain counter seam.' }
                [void]$addRead.Invoke($owner.StdoutDrain, [object[]]@([long]1))
                $injectionState.OutputByteFailureInjected = $true
            }
            $inspected = Get-TestOwnedProcessImage $owner ([uint32]$processId)
            $observation = [pscustomobject][ordered]@{
                ProcessId = [uint32]$inspected.ProcessId
                CreationUtc = [string]$inspected.CreationUtc
                InOwnedJob = [bool]$inspected.InOwnedJob
                Running = [bool]$inspected.Running
                ActualCanonicalPath = [string]$inspected.ActualCanonicalPath
                ActualFileId = [string]$inspected.ActualFileId
            }
            [IO.File]::AppendAllText($observationsPath, (ConvertTo-Json -InputObject $observation -Compress) + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
            return $inspected
        }.GetNewClosure()
        $script:fakeJobMemberPid = if ($AddUnknownJobMember) { [uint32]$PID } else { $null }
        $scriptBlock = {
            $arguments = @('-NoLogo', '-NoProfile', '-File', $fixturePath, '-Mode', $Mode, '-ReadyFile', $ready, '-ChildCount', [string]$ChildCount)
            $parameters = @{
                Executable = $pwshPath
                ArgumentList = $arguments
                WorkingDirectory = $directory
                Stage = 'compile'
                TimeoutSeconds = $TimeoutSeconds
                CleanupTimeoutSeconds = $CleanupTimeoutSeconds
                StdoutPath = (Join-Path $directory 'stdout.log')
                StderrPath = (Join-Path $directory 'stderr.log')
            }
            if ($UsePolicy) {
                $parameters.AuxiliaryProcessPolicy = 'verified-msvc-vctip'
                $parameters.VerifiedMsvcVctipImages = @($imageLease)
                if ($UseTestInspector) { $parameters.TestOwnedProcessImageInspector = $inspector }
            }
            $result = Invoke-CheckedOwnedProcess @parameters
            $json = ConvertTo-Json -InputObject $result -Depth 12 -ErrorAction Stop
            [IO.File]::WriteAllText((Join-Path $directory 'owner-result.json'), $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
            return $result
        }.GetNewClosure()
        try { $result = & $scriptBlock }
        finally {
            $script:fakeJobMemberPid = $null
            $script:injectOutputByteFailure = $false
            $script:injectUnknownPostCleanup = $false
        }
        $results.Add([pscustomobject]@{ Name = $Name; Result = $result; EvidencePath = (Join-Path $directory 'owner-result.json'); ReadyPath = $ready })
        return $result
    }

    function Assert-AuxiliaryCleanup {
        param([object]$Result, [string]$Name)
        if (!$Result.CleanupVerified -or !$Result.OutputDrainVerified -or
            @($Result.PostCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -gt 0) {
            throw "$Name did not verify Job empty, drained output, and absent owned identities."
        }
    }

    $script:testImageMode = 'trusted'
    $positive = Invoke-AuxiliaryFixture -Name 'synthetic-trusted-helper' -Mode short-parent -UsePolicy -UseTestInspector
    Assert-AuxiliaryCleanup $positive 'Synthetic trusted helper'
    if (!$positive.Succeeded -or !$positive.ForcedJobRetirementAttempted -or
        $positive.AuxiliaryDisposition -cne 'verified-helpers-retired-and-all-final-checks-verified' -or
        @($positive.AuxiliaryProcessClassifications).Count -lt 1 -or
        @($positive.AuxiliaryProcessClassifications | Where-Object { $_.Disposition -cne 'retired-and-all-final-checks-verified' }).Count -ne 0 -or
        $positive.StdoutBytesRead -ne $positive.StdoutBytesRecorded -or
        $positive.StderrBytesRead -ne $positive.StderrBytesRecorded) {
        throw 'The synthetic verified helper did not remain pending until all retirement, output, and identity checks passed.'
    }
    $positiveReadyDocument = [Text.Json.JsonDocument]::Parse([IO.File]::ReadAllText((Join-Path $evidenceRoot 'synthetic-trusted-helper\fixture-identities.json')))
    try {
        $positiveReadyChild = $positiveReadyDocument.RootElement.GetProperty('Children')[0]
        $positiveReadyChildPid = $positiveReadyChild.GetProperty('ProcessId').GetUInt32()
        $positiveReadyChildCreation = $positiveReadyChild.GetProperty('CreationDate').GetString()
    }
    finally { $positiveReadyDocument.Dispose() }
    $positiveMatchingChild = @($positive.AuxiliaryProcessClassifications | Where-Object {
        [uint32]$_.ProcessId -eq $positiveReadyChildPid -and
        [string]$_.CreationDate -ceq $positiveReadyChildCreation -and
        $_.Disposition -ceq 'retired-and-all-final-checks-verified'
    })
    if ($positiveMatchingChild.Count -ne 1) {
        throw 'The explicitly launched readiness child PID and creation identity were not among the verified retired members.'
    }

    $script:testImageMode = 'trusted'
    $outputByteFailure = Invoke-AuxiliaryFixture -Name 'eligible-retirement-output-byte-failure' -Mode short-parent `
        -UsePolicy -UseTestInspector -InjectOutputByteFailure
    if ($outputByteFailure.Succeeded -or !$outputByteFailure.ForcedJobRetirementAttempted -or
        $outputByteFailure.AuxiliaryDisposition -cne 'verified-helpers-retired-but-final-checks-failed' -or
        $outputByteFailure.StdoutBytesRead -ne ($outputByteFailure.StdoutBytesRecorded + 1) -or
        @($outputByteFailure.RecordingErrors | Where-Object { $_ -like '*Standard output bytes were read but not recorded*' }).Count -ne 1 -or
        @($outputByteFailure.AuxiliaryProcessClassifications | Where-Object { $_.Disposition -ne 'retirement-attempted-but-final-checks-failed' }).Count -ne 0) {
        throw 'A byte-count recording failure after eligible helper classification did not remain an explicit final failure.'
    }

    $cleanupIdentityFailure = Invoke-AuxiliaryFixture -Name 'eligible-retirement-unknown-postcleanup-identity' -Mode short-parent `
        -UsePolicy -UseTestInspector -InjectUnknownPostCleanup
    if ($cleanupIdentityFailure.Succeeded -or !$cleanupIdentityFailure.ForcedJobRetirementAttempted -or
        $cleanupIdentityFailure.AuxiliaryDisposition -cne 'verified-helpers-retired-but-final-checks-failed' -or
        @($cleanupIdentityFailure.PostCleanup | Where-Object State -eq 'Unknown').Count -lt 1 -or
        @($cleanupIdentityFailure.AuxiliaryProcessClassifications | Where-Object { $_.Disposition -ne 'retirement-attempted-but-final-checks-failed' }).Count -ne 0) {
        throw 'Uncertain owned identity cleanup after eligible helper classification did not remain an explicit final failure.'
    }

    $script:testImageMode = 'same-name-untrusted'
    $sameName = Invoke-AuxiliaryFixture -Name 'same-name-untrusted-file-id' -Mode short-parent -UsePolicy -UseTestInspector
    Assert-AuxiliaryCleanup $sameName 'Same-name untrusted helper'
    if ($sameName.Succeeded -or $sameName.ForcedJobRetirementAttempted -or $sameName.Reason -notlike 'auxiliary process classification failure:*') {
        throw 'A same-name process with a different file identity was authorized for retirement.'
    }

    $script:testImageMode = 'mixed'
    $mixed = Invoke-AuxiliaryFixture -Name 'mixed-owned-survivors' -Mode mixed-parent -ChildCount 2 -UsePolicy -UseTestInspector
    Assert-AuxiliaryCleanup $mixed 'Mixed survivor members'
    if ($mixed.Succeeded -or $mixed.ForcedJobRetirementAttempted -or $mixed.Reason -notlike 'auxiliary process classification failure:*') {
        throw 'A mixed trusted/untrusted Job membership was authorized for retirement.'
    }

    $script:testImageMode = 'trusted'
    $unknown = Invoke-AuxiliaryFixture -Name 'unknown-job-membership' -Mode short-parent -UsePolicy -UseTestInspector -AddUnknownJobMember
    Assert-AuxiliaryCleanup $unknown 'Unknown Job member'
    if ($unknown.Succeeded -or $unknown.ForcedJobRetirementAttempted -or $unknown.Reason -notlike 'auxiliary process classification failure:*') {
        throw 'A process outside the checked Job was accepted as a trusted helper.'
    }

    $script:testImageMode = 'identity-error'
    $identityFailure = Invoke-AuxiliaryFixture -Name 'image-identity-failure' -Mode short-parent -UsePolicy -UseTestInspector
    Assert-AuxiliaryCleanup $identityFailure 'Image identity failure'
    if ($identityFailure.Succeeded -or $identityFailure.ForcedJobRetirementAttempted -or $identityFailure.Reason -notlike 'auxiliary process classification failure:*') {
        throw 'A failed live image identity query was treated as a verified helper.'
    }

    $script:testImageMode = 'trusted'
    $nonzero = Invoke-AuxiliaryFixture -Name 'nonzero-root' -Mode failed-parent -UsePolicy -UseTestInspector
    Assert-AuxiliaryCleanup $nonzero 'Nonzero compiler root'
    if ($nonzero.Succeeded -or $nonzero.ExitCode -ne 7 -or $nonzero.ForcedJobRetirementAttempted -or
        $nonzero.Reason -cne 'owned descendants survived a nonzero root exit') {
        throw 'A nonzero compiler root was eligible for the helper-only retirement policy.'
    }

    $timedOut = Invoke-AuxiliaryFixture -Name 'timed-out-compiler-root' -Mode tree -TimeoutSeconds 2 -CleanupTimeoutSeconds 1 -UsePolicy -UseTestInspector
    Assert-AuxiliaryCleanup $timedOut 'Timed-out compiler root'
    if ($timedOut.Succeeded -or !$timedOut.TimedOut -or $timedOut.ForcedJobRetirementAttempted) {
        throw 'A timed-out compiler root was eligible for helper-only retirement.'
    }

    $unrelated = Invoke-AuxiliaryFixture -Name 'default-policy-unrelated-child' -Mode short-parent -CleanupTimeoutSeconds 1
    Assert-AuxiliaryCleanup $unrelated 'Default policy unrelated child'
    if ($unrelated.Succeeded -or $unrelated.ForcedJobRetirementAttempted -or $unrelated.Reason -cne 'owned descendants survived the root process') {
        throw 'The default policy changed for an unrelated descendant.'
    }

    $preflightDir = Join-Path $evidenceRoot 'bounded-preflight-timeout'
    [IO.Directory]::CreateDirectory($preflightDir) | Out-Null
    $sleepScript = Join-Path $preflightDir 'trust-check-sleeper.ps1'
    "Start-Sleep -Seconds 30" | Set-Content -LiteralPath $sleepScript -Encoding utf8NoBOM
    $preflightTimeout = Invoke-CheckedOwnedProcess -Executable $pwshPath `
        -ArgumentList @('-NoProfile', '-File', $sleepScript) -WorkingDirectory $preflightDir `
        -Stage 'provenance' -TimeoutSeconds 1 -CleanupTimeoutSeconds 2 `
        -StdoutPath (Join-Path $preflightDir 'stdout.log') -StderrPath (Join-Path $preflightDir 'stderr.log')
    if ($preflightTimeout.Succeeded -or !$preflightTimeout.TimedOut -or !$preflightTimeout.CleanupVerified -or
        @($preflightTimeout.PostCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -gt 0) {
        throw 'A hanging trust-helper analogue escaped its finite owner deadline or cleanup.'
    }
    [IO.File]::WriteAllText((Join-Path $preflightDir 'owner-result.json'), (ConvertTo-Json -InputObject $preflightTimeout -Depth 12), [Text.UTF8Encoding]::new($false))

    $leaseDir = Join-Path $evidenceRoot 'write-delete-denial'
    [IO.Directory]::CreateDirectory($leaseDir) | Out-Null
    $leasePath = Join-Path $leaseDir 'leased-image.bin'
    [IO.File]::WriteAllBytes($leasePath, [byte[]](1, 2, 3, 4))
    $held = [IO.File]::Open($leasePath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $writeDenied = $false
    $deleteDenied = $false
    try {
        try { $writer = [IO.File]::Open($leasePath, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite); $writer.Dispose() }
        catch [IO.IOException] { $writeDenied = $true }
        try { [IO.File]::Delete($leasePath) }
        catch [IO.IOException] { $deleteDenied = $true }
    }
    finally { $held.Dispose() }
    if (!$writeDenied -or !$deleteDenied -or !(Test-Path -LiteralPath $leasePath)) {
        throw 'The held read-only image lease did not block replacement writes and deletion.'
    }
    Remove-Item -LiteralPath $leasePath -Force
    [IO.File]::WriteAllBytes($leasePath, [byte[]](4, 3, 2, 1))
    $changedBytesHash = (Get-FileHash -LiteralPath $leasePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($changedBytesHash -ceq ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([byte[]](1, 2, 3, 4))).ToLowerInvariant())) {
        throw 'The replacement fixture did not produce changed image bytes.'
    }
    $changedStream = [IO.File]::Open($leasePath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try { $changedFileId = [MultiLauncher.Dev.OwnedProcessNative]::GetOpenFileDescriptor($changedStream.SafeFileHandle).FileId }
    finally { $changedStream.Dispose() }
    [IO.File]::WriteAllText((Join-Path $leaseDir 'lease-check.json'), (ConvertTo-Json -InputObject @{
        WriteDeniedWhileLeased = $writeDenied; DeleteDeniedWhileLeased = $deleteDenied
        ChangedFileId = $changedFileId
        ChangedSha256 = $changedBytesHash
    } -Depth 4), [Text.UTF8Encoding]::new($false))

    $summary = [pscustomobject][ordered]@{
        Result = 'PASS'
        EvidenceDirectory = $evidenceRoot
        EvidenceRole = 'Synthetic owner decision fixtures; only preflight-1 proves actual Authenticode trust.'
        CaseCount = $results.Count + 2
        Cases = @($results | ForEach-Object { [pscustomobject]@{ Name = $_.Name; Result = $_.Result.Reason; Succeeded = $_.Result.Succeeded; ForcedJobRetirementAttempted = $_.Result.ForcedJobRetirementAttempted; AuxiliaryDisposition = $_.Result.AuxiliaryDisposition; ReceiptPath = $_.EvidencePath } }) + @(
            [pscustomobject]@{ Name = 'bounded-preflight-timeout-analogue'; Result = $preflightTimeout.Reason; Succeeded = $preflightTimeout.Succeeded; ReceiptPath = (Join-Path $preflightDir 'owner-result.json') },
            [pscustomobject]@{ Name = 'held-image-write-delete-denial-and-replacement'; Result = 'PASS'; WriteDenied = $writeDenied; DeleteDenied = $deleteDenied; ReceiptPath = (Join-Path $leaseDir 'lease-check.json') }
        )
    }
    [IO.File]::WriteAllText((Join-Path $evidenceRoot 'suite-summary.json'), (ConvertTo-Json -InputObject $summary -Depth 14), [Text.UTF8Encoding]::new($false))
    $summary
}
finally {
    if ($lease) { $lease.Dispose() }
}
