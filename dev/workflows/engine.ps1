# Shared resolver and bounded workflow action boundary. Dot-source this file from
# run.ps1 or the focused self-test; child processes always go through measure.ps1.

$script:WorkflowRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..') -ErrorAction Stop).ProviderPath
$script:WorkflowCatalogPath = Join-Path $PSScriptRoot 'catalog.json'
$script:WorkflowCargoPath = Join-Path $script:WorkflowRoot 'dev\track_b\cargo.ps1'
$script:WorkflowMeasurePath = Join-Path $script:WorkflowRoot 'dev\track_b\measure.ps1'
$script:WorkflowCommonPath = Join-Path $script:WorkflowRoot 'dev\test_perf\common.ps1'
. $script:WorkflowCommonPath

function Get-WorkflowCatalog {
    $catalog = Get-Content -LiteralPath $script:WorkflowCatalogPath -Raw -ErrorAction Stop |
        ConvertFrom-Json -AsHashtable -ErrorAction Stop
    if ($catalog.schemaVersion -ne 1 -or $catalog.package -cne 'multi_launcher') {
        throw 'Workflow catalog schema or package identity is not supported.'
    }
    return $catalog
}

function Get-WorkflowCatalogSha256 {
    return (Get-FileHash -LiteralPath $script:WorkflowCatalogPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
}

function Get-WorkflowSourceFingerprint {
    $files = [System.Collections.Generic.List[string]]::new()
    foreach ($directory in @('src', 'tests')) {
        $directoryPath = Join-Path $script:WorkflowRoot $directory
        if (!(Test-Path -LiteralPath $directoryPath -PathType Container)) { throw "Workflow source directory is missing: $directory" }
        foreach ($file in Get-ChildItem -LiteralPath $directoryPath -Filter '*.rs' -File -Recurse -ErrorAction Stop) {
            $files.Add($file.FullName)
        }
    }
    foreach ($relativePath in @(
        'Cargo.toml', 'Cargo.lock', 'build.rs', '.cargo/config.toml',
        'dev/track_b/cargo.ps1', 'dev/track_b/measure.ps1',
        'dev/workflows/owned_process.ps1', 'dev/workflows/engine.ps1',
        'dev/workflows/run.ps1', 'dev/workflows/catalog.json',
        'dev/test_perf/common.ps1'
    )) {
        $path = Join-Path $script:WorkflowRoot $relativePath
        if (Test-Path -LiteralPath $path -PathType Leaf) { $files.Add($path) }
    }

    $entries = [System.Collections.Generic.List[string]]::new()
    foreach ($path in @($files | Sort-Object -Unique)) {
        $relativePath = [IO.Path]::GetRelativePath($script:WorkflowRoot, $path).Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
        $entries.Add("$relativePath=$hash")
    }
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($entries -join "`n"))
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}

function Get-WorkflowCargoPath {
    $application = Get-Command -Name 'cargo.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1
    if ($null -eq $application) { throw 'cargo.exe was not found as a native application on PATH.' }
    $path = if ($application.Source) { $application.Source } else { $application.Path }
    return [IO.Path]::GetFullPath($path)
}

function Get-WorkflowNextestIdentity {
    $application = Get-Command -Name 'cargo-nextest.exe' -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $application) { throw 'cargo-nextest.exe was not found on PATH; fast-dev-tests requires Cargo Nextest.' }
    $path = if ($application.Source) { $application.Source } else { $application.Path }
    $resolvedPath = [IO.Path]::GetFullPath($path)
    return [pscustomobject][ordered]@{
        Path = $resolvedPath
        Sha256 = (Get-FileHash -LiteralPath $resolvedPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
    }
}

function Get-WorkflowCommand {
    param(
        [Parameter(Mandatory)][string]$Preset,
        [string]$WorkflowPhase,
        [string]$TargetKind,
        [string]$TestTarget,
        [string]$FilterExpression,
        [string]$BinariesMetadataPath,
        [string]$RunnerProfile,
        [string[]]$TestArguments = @()
    )

    $wrapperArguments = @{ Preset = $Preset; PrintCommand = $true }
    if ($WorkflowPhase) { $wrapperArguments.WorkflowPhase = $WorkflowPhase }
    if ($TargetKind) { $wrapperArguments.TargetKind = $TargetKind }
    if ($TestTarget) { $wrapperArguments.TestTarget = $TestTarget }
    if ($FilterExpression) { $wrapperArguments.FilterExpression = $FilterExpression }
    if ($BinariesMetadataPath) { $wrapperArguments.BinariesMetadataPath = $BinariesMetadataPath }
    if ($RunnerProfile) { $wrapperArguments.RunnerProfile = $RunnerProfile }
    if ($TestArguments.Count -gt 0) { $wrapperArguments.TestArguments = @($TestArguments) }

    $json = & $script:WorkflowCargoPath @wrapperArguments
    $record = $json | ConvertFrom-Json -ErrorAction Stop
    if ($record.Started -ne $false -or [string]::IsNullOrWhiteSpace([string]$record.Executable) -or $null -eq $record.ArgumentList) {
        throw "Cargo command resolver returned an invalid or executing command for preset '$Preset'."
    }
    return [pscustomobject][ordered]@{
        Executable = [IO.Path]::GetFullPath([string]$record.Executable)
        ArgumentList = @($record.ArgumentList | ForEach-Object { [string]$_ })
        WorkingDirectory = [IO.Path]::GetFullPath([string]$record.WorkingDirectory)
        Started = $false
    }
}

function Get-WorkflowFilterExpression {
    param([Parameter(Mandatory)][string]$ExactCase)
    if ([string]::IsNullOrWhiteSpace($ExactCase) -or $ExactCase.Contains("`n") -or $ExactCase.Contains("`r")) {
        throw 'Catalog exact-case filters must be non-empty single-line test names.'
    }
    $escaped = [regex]::Escape($ExactCase).Replace('/', '\/')
    return "test(/^$escaped`$/)"
}

function Get-WorkflowSelection {
    param([string]$SelectionName, [string]$TargetKind)
    $catalog = Get-WorkflowCatalog
    if (![string]::IsNullOrWhiteSpace($SelectionName)) {
        $selection = @($catalog.selections | Where-Object { [string]$_.name -ceq $SelectionName })
        if ($selection.Count -ne 1) { throw "Unknown or ambiguous workflow selection: $SelectionName" }
        if ($TargetKind -and $selection[0].targetKind -cne $TargetKind) {
            throw "Selection '$SelectionName' targets '$($selection[0].targetKind)', not '$TargetKind'."
        }
        return @($selection[0])
    }
    if ([string]::IsNullOrWhiteSpace($TargetKind)) { throw 'A selection name or target kind is required.' }
    return @($catalog.selections | Where-Object { [string]$_.targetKind -ceq $TargetKind })
}

function New-WorkflowPhaseTemplate {
    param(
        [Parameter(Mandatory)][string]$Preset,
        [Parameter(Mandatory)][string]$Phase,
        [Parameter(Mandatory)]$Selection,
        [string]$BinariesMetadataPath,
        [string]$FilterExpression,
        [string]$RunnerProfile,
        [string[]]$TestArguments = @()
    )
    $testTarget = if ($Selection.targetKind -eq 'integration') { [string]$Selection.targetName } else { $null }
    return Get-WorkflowCommand -Preset $Preset -WorkflowPhase $Phase -TargetKind ([string]$Selection.targetKind) `
        -TestTarget $testTarget -FilterExpression $FilterExpression -BinariesMetadataPath $BinariesMetadataPath `
        -RunnerProfile $RunnerProfile -TestArguments $TestArguments
}

function New-WorkflowRequest {
    param(
        [Parameter(Mandatory)][ValidateSet('Preview', 'Discover', 'Run')][string]$Action,
        [Parameter(Mandatory)][string]$Mode,
        [string]$SelectionName,
        [ValidateSet('lib', 'integration')][string]$TargetKind,
        [string]$DiscoveryReceipt,
        [string]$RunnerProfile,
        [string[]]$TestArguments = @()
    )

    $catalog = Get-WorkflowCatalog
    $modeRecord = @($catalog.modes | Where-Object { [string]$_.name -ceq $Mode })
    if ($modeRecord.Count -ne 1) { throw "Unknown or ambiguous workflow mode: $Mode" }
    $modeRecord = $modeRecord[0]
    if ($null -eq $TestArguments) { $TestArguments = @() }
    foreach ($argument in $TestArguments) { if ($null -eq $argument) { throw 'TestArguments cannot contain null elements.' } }
    if ($RunnerProfile -and $modeRecord.kind -ne 'tests') { throw '-RunnerProfile is valid only for fast-dev-tests.' }
    if ($RunnerProfile -and $RunnerProfile -notmatch '^[A-Za-z0-9_][A-Za-z0-9_.-]*$') { throw 'Runner profile name is malformed.' }

    $request = [pscustomobject][ordered]@{
        SchemaVersion = 1
        Action = $Action
        Mode = [string]$modeRecord.name
        Package = [string]$catalog.package
        TargetKind = $null
        TargetName = $null
        Features = [pscustomobject][ordered]@{ DefaultFeatures = [bool]$catalog.features.defaultFeatures; Explicit = @($catalog.features.explicit) }
        CompilerProfile = [string]$modeRecord.compilerProfile
        RunnerProfile = if ($RunnerProfile) { $RunnerProfile } elseif ($modeRecord.runnerProfile) { [string]$modeRecord.runnerProfile } else { 'default' }
        BudgetSeconds = $modeRecord.budgets
        SelectionName = $null
        ExactCase = $null
        FilterExpression = $null
        SelectedIdentity = $null
        BinariesMetadataPath = $null
        DiscoveryReceipt = $null
        CatalogSha256 = Get-WorkflowCatalogSha256
        SourceFingerprintSha256 = Get-WorkflowSourceFingerprint
        CargoExecutable = $null
        NextestTool = $null
        Phases = @()
        Status = 'RESOLVED'
        ResolutionError = $null
    }

    if ($Action -eq 'Discover' -and $modeRecord.kind -ne 'tests') { throw 'Discover requires the fast-dev-tests mode.' }
    if ($Action -eq 'Discover') {
        if ($SelectionName) { throw 'Discover accepts a target kind and discovers its catalog selections; it does not accept -SelectionName.' }
        if (!$TargetKind) { throw 'Discover requires -TargetKind lib or integration.' }
        if ($DiscoveryReceipt) { throw '-DiscoveryReceipt is not valid with Discover.' }
        if ($TestArguments.Count -gt 0) { throw 'Discover does not accept test-binary arguments.' }
        $selections = @(Get-WorkflowSelection -TargetKind $TargetKind)
        if ($selections.Count -eq 0) { throw "The catalog has no selections for target kind '$TargetKind'." }
        $request.TargetKind = $TargetKind
        $request.TargetName = if ($TargetKind -eq 'lib') { 'multi_launcher' } else { 'pure' }
        $request.CargoExecutable = Get-WorkflowCargoPath
        $request.NextestTool = Get-WorkflowNextestIdentity
        $request | Add-Member -NotePropertyName Selections -NotePropertyValue @($selections) -Force
        $request | Add-Member -NotePropertyName PrepareCommand -NotePropertyValue (New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'prepare' -Selection $selections[0]) -Force
        $placeholder = Join-Path ([IO.Path]::GetTempPath()) 'multi-launcher-binaries-metadata-pending.json'
        $inventories = foreach ($selection in $selections) {
            $filter = Get-WorkflowFilterExpression -ExactCase ([string]$selection.exactCase)
            [pscustomobject][ordered]@{
                SelectionName = [string]$selection.name
                FilterExpression = $filter
                Command = New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'inventory' -Selection $selection `
                    -BinariesMetadataPath $placeholder -FilterExpression $filter
            }
        }
        $request | Add-Member -NotePropertyName InventoryTemplates -NotePropertyValue @($inventories) -Force
        $request.Phases = @([pscustomobject][ordered]@{ Name = 'target-prepare'; Stage = 'compile'; TimeoutSeconds = [int]$modeRecord.budgets.preparationSeconds; Command = $request.PrepareCommand })
        return $request
    }

    if ($modeRecord.kind -eq 'build') {
        if ($SelectionName -or $TargetKind -or $DiscoveryReceipt -or $TestArguments.Count -gt 0) {
            throw "Build mode '$Mode' does not accept a test selection, discovery receipt, target kind, or test arguments."
        }
        $request.CargoExecutable = Get-WorkflowCargoPath
        $command = Get-WorkflowCommand -Preset ([string]$modeRecord.cargoPreset)
        $request.TargetKind = if ($modeRecord.cargoPreset -eq 'quick-check') { 'lib' } else { 'binary' }
        $request.TargetName = 'multi_launcher'
        $request.Phases = @([pscustomobject][ordered]@{
            Name = 'build'
            Stage = 'compile'
            TimeoutSeconds = [int]$modeRecord.budgets.executionSeconds
            Command = $command
        })
        if ($Action -eq 'Discover') { throw 'Build modes cannot discover test selections.' }
        return $request
    }

    if ($modeRecord.name -cne 'fast-dev-tests') { throw "Unsupported test mode: $Mode" }
    if ($Action -eq 'Discover') { throw 'Internal discovery resolution was not completed.' }
    if (!$SelectionName -or $TargetKind) { throw 'Preview and Run for test mode require exactly one -SelectionName.' }
    if ($TestArguments.Count -gt 0 -and $Action -ne 'Run' -and $Action -ne 'Preview') { throw 'Test arguments are valid only for test Preview or Run.' }
    $selection = @(Get-WorkflowSelection -SelectionName $SelectionName)
    if ($selection.Count -ne 1) { throw "Selection '$SelectionName' did not resolve to exactly one catalog entry." }
    $selection = $selection[0]
    $request.TargetKind = [string]$selection.targetKind
    $request.TargetName = [string]$selection.targetName
    $request.SelectionName = [string]$selection.name
    $request.ExactCase = [string]$selection.exactCase
    $request.FilterExpression = Get-WorkflowFilterExpression -ExactCase $request.ExactCase
    $request.CargoExecutable = Get-WorkflowCargoPath
    $request.NextestTool = Get-WorkflowNextestIdentity
    $request | Add-Member -NotePropertyName Selection -NotePropertyValue $selection -Force
    $request | Add-Member -NotePropertyName TestArguments -NotePropertyValue @($TestArguments) -Force
    $placeholder = Join-Path ([IO.Path]::GetTempPath()) 'multi-launcher-binaries-metadata-pending.json'
    $request | Add-Member -NotePropertyName PrepareCommand -NotePropertyValue (New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'prepare' -Selection $selection) -Force
    $request | Add-Member -NotePropertyName InventoryCommand -NotePropertyValue (New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'inventory' -Selection $selection `
        -BinariesMetadataPath $placeholder -FilterExpression $request.FilterExpression) -Force
    $request | Add-Member -NotePropertyName RunCommand -NotePropertyValue (New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'run' -Selection $selection `
        -BinariesMetadataPath $placeholder -FilterExpression $request.FilterExpression -RunnerProfile $RunnerProfile -TestArguments $TestArguments) -Force
    $request.Phases = @(
        [pscustomobject][ordered]@{ Name = 'nextest-version'; Stage = 'provenance'; TimeoutSeconds = 20; Command = [pscustomobject][ordered]@{ Executable = $request.CargoExecutable; ArgumentList = @('nextest', '--version'); WorkingDirectory = $script:WorkflowRoot; Started = $false } },
        [pscustomobject][ordered]@{ Name = 'target-prepare'; Stage = 'compile'; TimeoutSeconds = [int]$modeRecord.budgets.preparationSeconds; Command = $request.PrepareCommand },
        [pscustomobject][ordered]@{ Name = 'selected-inventory'; Stage = 'provenance'; TimeoutSeconds = [int]$modeRecord.budgets.inventorySeconds; Command = $request.InventoryCommand },
        [pscustomobject][ordered]@{ Name = 'selected-run'; Stage = 'test'; TimeoutSeconds = [int]$modeRecord.budgets.executionSeconds; Command = $request.RunCommand }
    )

    if ($Action -eq 'Preview') {
        $validation = Test-WorkflowDiscoveryReceipt -Path $DiscoveryReceipt -Request $request
        if ($validation.Status -ne 'PASS') {
            $request.Status = $validation.Status
            $request.ResolutionError = $validation.Error
            return $request
        }
        Set-WorkflowRequestMetadataPath -Request $request -Path $validation.BinariesMetadataPath
        $request.SelectedIdentity = $validation.SelectedIdentity
        $request.DiscoveryReceipt = [IO.Path]::GetFullPath($DiscoveryReceipt)
    }
    elseif ($DiscoveryReceipt) { throw '-DiscoveryReceipt is used only by test Preview; Run always creates fresh target metadata.' }
    return $request
}

function New-WorkflowRequestFromCli {
    param(
        [Parameter(Mandatory)][ValidateSet('Preview', 'Discover', 'Run')][string]$Action,
        [Parameter(Mandatory)][string]$Mode,
        [string]$SelectionName,
        [string]$TargetKind,
        [string]$DiscoveryReceipt,
        [string]$RunnerProfile,
        [string[]]$TestArguments = @()
    )

    $requestArguments = @{ Action = $Action; Mode = $Mode }
    if (![string]::IsNullOrWhiteSpace($SelectionName)) { $requestArguments.SelectionName = $SelectionName }
    if (![string]::IsNullOrWhiteSpace($TargetKind)) { $requestArguments.TargetKind = $TargetKind }
    if (![string]::IsNullOrWhiteSpace($DiscoveryReceipt)) { $requestArguments.DiscoveryReceipt = $DiscoveryReceipt }
    if (![string]::IsNullOrWhiteSpace($RunnerProfile)) { $requestArguments.RunnerProfile = $RunnerProfile }
    if ($PSBoundParameters.ContainsKey('TestArguments')) { $requestArguments.TestArguments = @($TestArguments) }
    return New-WorkflowRequest @requestArguments
}

function New-WorkflowToolReceipt {
    param([Parameter(Mandatory)]$Tool)
    return [ordered]@{
        path = [string]$Tool.Path
        sha256 = [string]$Tool.Sha256
    }
}

function Set-WorkflowRequestMetadataPath {
    param([Parameter(Mandatory)]$Request, [Parameter(Mandatory)][string]$Path)
    $metadataPath = [IO.Path]::GetFullPath($Path)
    $selection = $Request.Selection
    $mode = Get-WorkflowCatalog | ForEach-Object { @($_.modes | Where-Object { $_.name -ceq $Request.Mode })[0] }
    $filter = [string]$Request.FilterExpression
    $runnerProfile = if ($Request.RunnerProfile -and $Request.RunnerProfile -cne 'default') { [string]$Request.RunnerProfile } else { $null }
    $Request.InventoryCommand = New-WorkflowPhaseTemplate -Preset ([string]$mode.cargoPreset) -Phase 'inventory' -Selection $selection `
        -BinariesMetadataPath $metadataPath -FilterExpression $filter
    $Request.RunCommand = New-WorkflowPhaseTemplate -Preset ([string]$mode.cargoPreset) -Phase 'run' -Selection $selection `
        -BinariesMetadataPath $metadataPath -FilterExpression $filter -RunnerProfile $runnerProfile -TestArguments @($Request.TestArguments)
    $Request.BinariesMetadataPath = $metadataPath
    $Request.Phases[2].Command = $Request.InventoryCommand
    $Request.Phases[3].Command = $Request.RunCommand
}

function Test-WorkflowDiscoveryReceipt {
    param([string]$Path, [Parameter(Mandatory)]$Request)
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return [pscustomobject]@{ Status = 'DISCOVERY_REQUIRED'; Error = 'A test preview requires -DiscoveryReceipt from a successful Discover action.' }
    }
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) {
        return [pscustomobject]@{ Status = 'DISCOVERY_REQUIRED'; Error = "Discovery receipt is missing: $Path" }
    }
    try { $receipt = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop }
    catch { return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = "Discovery receipt is malformed: $($_.Exception.Message)" } }

    if ($receipt.schemaVersion -ne 1 -or $receipt.status -cne 'PASS') {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Discovery receipt is not a successful supported version.' }
    }
    if ($receipt.catalogSha256 -cne $Request.CatalogSha256 -or $receipt.sourceFingerprintSha256 -cne $Request.SourceFingerprintSha256) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Discovery receipt does not match the current catalog or source fingerprint; run Discover again.' }
    }
    if ($receipt.targetKind -cne $Request.TargetKind) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Discovery receipt target kind does not match this selection.' }
    }
    if ($receipt.cargoExecutable -cne $Request.CargoExecutable -or $receipt.cargoExecutableSha256 -cne (Get-FileHash -LiteralPath $Request.CargoExecutable -Algorithm SHA256).Hash.ToLowerInvariant()) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Cargo executable identity changed after discovery.' }
    }
    if ($receipt.nextestTool.path -cne $Request.NextestTool.Path -or $receipt.nextestTool.sha256 -cne $Request.NextestTool.Sha256) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Cargo Nextest tool identity changed after discovery.' }
    }
    $metadataPath = [string]$receipt.binariesMetadataPath
    if ([string]::IsNullOrWhiteSpace($metadataPath) -or !(Test-Path -LiteralPath $metadataPath -PathType Leaf)) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Prepared binary metadata is missing; run Discover again.' }
    }
    $metadataHash = (Get-FileHash -LiteralPath $metadataPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
    if ($metadataHash -cne [string]$receipt.binariesMetadataSha256) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Prepared binary metadata changed after discovery.' }
    }
    $discovered = @($receipt.selections | Where-Object { [string]$_.name -ceq $Request.SelectionName })
    if ($discovered.Count -ne 1 -or $discovered[0].status -cne 'PASS' -or [int]$discovered[0].selectedCaseCount -ne 1) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = "Discovery receipt does not contain exactly one validated case for '$($Request.SelectionName)'." }
    }
    $identity = $discovered[0].selectedIdentity
    if ($null -eq $identity -or $identity.packageName -cne $Request.Package -or $identity.binaryName -cne $Request.TargetName -or
        $identity.kind -cne [string]$Request.Selection.nextestKind -or $identity.testName -cne $Request.ExactCase -or [string]::IsNullOrWhiteSpace([string]$identity.binaryId)) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Discovered package, target, kind, binary, or exact case no longer matches the catalog.' }
    }
    return [pscustomobject]@{ Status = 'PASS'; Error = $null; BinariesMetadataPath = $metadataPath; SelectedIdentity = $identity }
}

function Test-WorkflowPathWithin {
    param([string]$Candidate, [string]$Directory)
    $candidatePath = [IO.Path]::GetFullPath($Candidate).TrimEnd([char[]]@('\', '/'))
    $directoryPath = [IO.Path]::GetFullPath($Directory).TrimEnd([char[]]@('\', '/'))
    if ([string]::IsNullOrEmpty($directoryPath)) { $directoryPath = [IO.Path]::GetPathRoot([IO.Path]::GetFullPath($Directory)) }
    $prefix = $directoryPath + [IO.Path]::DirectorySeparatorChar
    return $candidatePath.Equals($directoryPath, [StringComparison]::OrdinalIgnoreCase) -or
        $candidatePath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

function New-WorkflowEvidenceDirectory {
    param([string]$EvidenceDirectory)
    $root = if ([string]::IsNullOrWhiteSpace($EvidenceDirectory)) { Join-Path ([IO.Path]::GetTempPath()) 'Multi_Launcher\workflows' } else { [IO.Path]::GetFullPath($EvidenceDirectory) }
    if (Test-WorkflowPathWithin -Candidate $root -Directory $script:WorkflowRoot) { throw "Workflow evidence must be outside the source worktree: $root" }
    [IO.Directory]::CreateDirectory($root) | Out-Null
    $runPath = Join-Path $root ('workflow_' + [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '_' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($runPath) | Out-Null
    return $runPath
}

function Get-WorkflowPhaseSummary {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)]$Command, [Parameter(Mandatory)]$Measurement)
    return [pscustomobject][ordered]@{
        Name = $Name
        Executable = [string]$Command.Executable
        ArgumentList = @($Command.ArgumentList)
        WorkingDirectory = [string]$Command.WorkingDirectory
        DeadlineUtc = $Measurement.DeadlineUtc
        TimeoutSeconds = $Measurement.TimeoutSeconds
        TimedOut = [bool]$Measurement.TimedOut
        ExitCode = $Measurement.ExitCode
        Succeeded = [bool]$Measurement.Succeeded
        ExecutionReason = $Measurement.ExecutionReason
        ProcessStarted = [bool]$Measurement.ProcessStarted
        CleanupVerified = [bool]$Measurement.CleanupVerified
        CleanupErrors = @($Measurement.CleanupErrors)
        OutputDrainVerified = [bool]$Measurement.OutputDrainVerified
        OutputBytes = $Measurement.OutputBytes
        OwnedProcesses = @($Measurement.OwnedProcesses)
        PostCleanup = @($Measurement.PostCleanup)
        RecordingErrors = @($Measurement.RecordingErrors)
        EvidenceDirectory = [string]$Measurement.EvidenceDirectory
        MeasurementPath = [string]$Measurement.ManifestPath
        StdoutPath = [string]$Measurement.StdoutPath
        StderrPath = [string]$Measurement.StderrPath
    }
}

function Invoke-WorkflowCommand {
    param(
        [Parameter(Mandatory)]$Request,
        [Parameter(Mandatory)][string]$EvidenceDirectory,
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][int]$TimeoutSeconds,
        [Parameter(Mandatory)]$Command,
        [scriptblock]$ProcessInvoker
    )
    if ($null -eq $ProcessInvoker) {
        $measureArguments = @{
            Executable = [string]$Command.Executable
            ArgumentList = @($Command.ArgumentList)
            ScenarioId = ('workflow-{0}-{1}-{2}' -f $Request.Mode, $Name, [guid]::NewGuid().ToString('N'))
            EvidenceDirectory = $EvidenceDirectory
            WorkingDirectory = [string]$Command.WorkingDirectory
            SourceRoot = $script:WorkflowRoot
            Profile = [string]$Request.CompilerProfile
            Stage = $Stage
            TimeoutSeconds = $TimeoutSeconds
            CleanupTimeoutSeconds = 5
        }
        $measurement = & $script:WorkflowMeasurePath @measureArguments
    }
    else {
        $measurement = & $ProcessInvoker @{
            Request = $Request
            Name = $Name
            Stage = $Stage
            TimeoutSeconds = $TimeoutSeconds
            Command = $Command
            EvidenceDirectory = $EvidenceDirectory
        }
    }
    if ($null -eq $measurement) { throw "Process owner returned no result for phase '$Name'." }
    return $measurement
}

function Get-WorkflowFailureExitCode {
    param($Measurement)
    if ($Measurement.TimedOut) { return 124 }
    if ($null -ne $Measurement.ExitCode -and [int]$Measurement.ExitCode -ne 0) { return [int]$Measurement.ExitCode }
    return 1
}

function Assert-WorkflowBinaryMetadata {
    param([Parameter(Mandatory)][string]$Path)
    $metadata = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
    $binaries = Get-TestPerfField $metadata 'rust-binaries'
    if ($null -eq $binaries -or @($binaries).Count -eq 0) { throw 'Nextest binary metadata is missing rust-binaries.' }
    return $metadata
}

function Get-WorkflowSelectedIdentity {
    param([Parameter(Mandatory)][string]$InventoryPath, [Parameter(Mandatory)]$Selection)
    $cases = @(Convert-TestPerfInventory -Path $InventoryPath)
    $selectedCount = [int](Get-TestPerfSelectedCaseCount -Cases $cases)
    $matches = @($cases | Where-Object { !$_.Ignored -and $_.FilterStatus -eq 'matches' })
    if ($selectedCount -ne 1 -or $matches.Count -ne 1) {
        throw "Exact selection '$($Selection.name)' matched $selectedCount runnable cases; exactly one is required."
    }
    $case = $matches[0]
    if ($case.TestName -cne [string]$Selection.exactCase -or $case.PackageName -cne [string]$Selection.package -or
        $case.BinaryName -cne [string]$Selection.targetName -or $case.Kind -cne [string]$Selection.nextestKind -or
        [string]::IsNullOrWhiteSpace([string]$case.BinaryId)) {
        throw "Nextest identity for '$($Selection.name)' does not match its package, target, kind, and exact-case catalog entry."
    }
    return [pscustomobject][ordered]@{
        BinaryId = [string]$case.BinaryId
        PackageName = [string]$case.PackageName
        BinaryName = [string]$case.BinaryName
        Kind = [string]$case.Kind
        TestName = [string]$case.TestName
    }
}

function Write-WorkflowJson {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)]$Value)
    $json = ConvertTo-Json -InputObject $Value -Depth 18 -ErrorAction Stop
    Set-Content -LiteralPath $Path -Value $json -Encoding utf8 -ErrorAction Stop
}

function Invoke-WorkflowAction {
    param(
        [Parameter(Mandatory)]$Request,
        [string]$EvidenceDirectory,
        [scriptblock]$ProcessInvoker
    )
    if ($Request.Action -eq 'Preview') {
        if ($Request.Status -ne 'RESOLVED') {
            return [pscustomobject][ordered]@{ Status = [string]$Request.Status; ExitCode = 3; Error = [string]$Request.ResolutionError; Request = $Request; EvidenceDirectory = $null }
        }
        return [pscustomobject][ordered]@{ Status = 'PREVIEW'; ExitCode = 0; Error = $null; Request = $Request; EvidenceDirectory = $null }
    }

    $runDirectory = New-WorkflowEvidenceDirectory -EvidenceDirectory $EvidenceDirectory
    $phases = [System.Collections.Generic.List[object]]::new()
    $result = [pscustomobject][ordered]@{
        SchemaVersion = 1
        Status = 'RUNNING'
        ExitCode = 1
        Action = [string]$Request.Action
        Mode = [string]$Request.Mode
        SelectionName = $Request.SelectionName
        Request = $Request
        Phases = @()
        Error = $null
        DiscoveryReceiptPath = $null
        EvidenceDirectory = $runDirectory
    }
    $failureSeen = $false
    $firstFailure = $null

    try {
        if ($Request.Action -eq 'Run' -and $Request.TargetKind -ne 'lib' -and $Request.TargetKind -ne 'integration') {
            $phase = $Request.Phases[0]
            $measurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name $phase.Name -Stage $phase.Stage `
                -TimeoutSeconds $phase.TimeoutSeconds -Command $phase.Command -ProcessInvoker $ProcessInvoker
            $summary = Get-WorkflowPhaseSummary -Name $phase.Name -Command $phase.Command -Measurement $measurement
            $phases.Add($summary)
            if (!$measurement.Succeeded) {
                $firstFailure = "Build phase '$($phase.Name)' failed ($($measurement.ExecutionReason), exit $($measurement.ExitCode))."
                $result.ExitCode = Get-WorkflowFailureExitCode $measurement
                throw $firstFailure
            }
            $result.Status = 'PASS'
            $result.ExitCode = 0
        }
        elseif ($Request.Action -eq 'Discover' -or ($Request.Action -eq 'Run' -and $Request.Mode -eq 'fast-dev-tests')) {
            $versionCommand = if ($Request.Action -eq 'Discover') { [pscustomobject]@{ Executable = $Request.CargoExecutable; ArgumentList = @('nextest', '--version'); WorkingDirectory = $script:WorkflowRoot } } else { $Request.Phases[0].Command }
            $versionMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name 'nextest-version' -Stage 'provenance' `
                -TimeoutSeconds 20 -Command $versionCommand -ProcessInvoker $ProcessInvoker
            $versionSummary = Get-WorkflowPhaseSummary -Name 'nextest-version' -Command $versionCommand -Measurement $versionMeasurement
            $phases.Add($versionSummary)
            if (!$versionMeasurement.Succeeded) {
                $firstFailure = "Cargo Nextest capability check failed ($($versionMeasurement.ExecutionReason), exit $($versionMeasurement.ExitCode))."
                $result.ExitCode = Get-WorkflowFailureExitCode $versionMeasurement
                throw $firstFailure
            }

            $prepareCommand = if ($Request.Action -eq 'Discover') { $Request.PrepareCommand } else { $Request.PrepareCommand }
            $prepareTimeout = [int]$Request.BudgetSeconds.preparationSeconds
            $prepareMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name 'target-prepare' -Stage 'compile' `
                -TimeoutSeconds $prepareTimeout -Command $prepareCommand -ProcessInvoker $ProcessInvoker
            $prepareSummary = Get-WorkflowPhaseSummary -Name 'target-prepare' -Command $prepareCommand -Measurement $prepareMeasurement
            $phases.Add($prepareSummary)
            if (!$prepareMeasurement.Succeeded) {
                $firstFailure = "Target preparation failed ($($prepareMeasurement.ExecutionReason), exit $($prepareMeasurement.ExitCode))."
                $result.ExitCode = Get-WorkflowFailureExitCode $prepareMeasurement
                throw $firstFailure
            }
            $metadataPath = [string]$prepareMeasurement.StdoutPath
            $null = Assert-WorkflowBinaryMetadata -Path $metadataPath

            if ($Request.Action -eq 'Discover') {
                $metadataHash = (Get-FileHash -LiteralPath $metadataPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
                $discoverySelections = [System.Collections.Generic.List[object]]::new()
                foreach ($inventoryTemplate in $Request.InventoryTemplates) {
                    $selection = @($Request.Selections | Where-Object { [string]$_.name -ceq [string]$inventoryTemplate.SelectionName })[0]
                    $command = New-WorkflowPhaseTemplate -Preset 'fast-dev-tests' -Phase 'inventory' -Selection $selection `
                        -BinariesMetadataPath $metadataPath -FilterExpression ([string]$inventoryTemplate.FilterExpression)
                    $measurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name ("inventory-{0}" -f $selection.name) `
                        -Stage 'provenance' -TimeoutSeconds ([int]$selection.budgets.inventorySeconds) -Command $command -ProcessInvoker $ProcessInvoker
                    $summary = Get-WorkflowPhaseSummary -Name ("inventory-{0}" -f $selection.name) -Command $command -Measurement $measurement
                    $phases.Add($summary)
                    $selectionRecord = [pscustomobject][ordered]@{
                        Name = [string]$selection.name
                        FilterExpression = [string]$inventoryTemplate.FilterExpression
                        Status = 'FAIL'
                        SelectedCaseCount = 0
                        SelectedIdentity = $null
                        InventoryMeasurementPath = [string]$measurement.ManifestPath
                        Error = $null
                    }
                    if (!$measurement.Succeeded) {
                        $selectionRecord.Error = "Inventory process failed ($($measurement.ExecutionReason), exit $($measurement.ExitCode))."
                        if (!$failureSeen) { $failureSeen = $true; $firstFailure = $selectionRecord.Error; $result.ExitCode = Get-WorkflowFailureExitCode $measurement }
                    }
                    else {
                        try {
                            $identity = Get-WorkflowSelectedIdentity -InventoryPath ([string]$measurement.StdoutPath) -Selection $selection
                            $selectionRecord.Status = 'PASS'
                            $selectionRecord.SelectedCaseCount = 1
                            $selectionRecord.SelectedIdentity = $identity
                        }
                        catch {
                            $selectionRecord.Error = $_.Exception.Message
                            if (!$failureSeen) { $failureSeen = $true; $firstFailure = $selectionRecord.Error; $result.ExitCode = 1 }
                        }
                    }
                    $discoverySelections.Add($selectionRecord)
                }
                $finalFingerprint = Get-WorkflowSourceFingerprint
                if ($finalFingerprint -cne $Request.SourceFingerprintSha256 -and !$failureSeen) {
                    $failureSeen = $true
                    $firstFailure = 'Source inputs changed during target discovery; the receipt is not reusable.'
                    $result.ExitCode = 1
                }
                $allSelectionsPass = @($discoverySelections | Where-Object Status -ne 'PASS').Count -eq 0
                $discoveryStatus = if ($allSelectionsPass -and !$failureSeen) { 'PASS' } else { 'FAIL' }
                $toolVersion = if (Test-Path -LiteralPath $versionSummary.StdoutPath -PathType Leaf) { (Get-Content -LiteralPath $versionSummary.StdoutPath -Raw).Trim() } else { $null }
                $discovery = [pscustomobject][ordered]@{
                    schemaVersion = 1
                    status = $discoveryStatus
                    targetKind = [string]$Request.TargetKind
                    targetName = [string]$Request.TargetName
                    package = [string]$Request.Package
                    mode = [string]$Request.Mode
                    compilerProfile = [string]$Request.CompilerProfile
                    runnerProfile = [string]$Request.RunnerProfile
                    features = $Request.Features
                    catalogSha256 = [string]$Request.CatalogSha256
                    sourceFingerprintSha256 = [string]$Request.SourceFingerprintSha256
                    cargoExecutable = [string]$Request.CargoExecutable
                    cargoExecutableSha256 = (Get-FileHash -LiteralPath $Request.CargoExecutable -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
                    nextestTool = New-WorkflowToolReceipt -Tool $Request.NextestTool
                    nextestVersion = $toolVersion
                    preparationMeasurementPath = [string]$prepareMeasurement.ManifestPath
                    binariesMetadataPath = $metadataPath
                    binariesMetadataSha256 = $metadataHash
                    selections = @($discoverySelections)
                    phases = @($phases)
                    error = $firstFailure
                    evidenceDirectory = $runDirectory
                }
                $receiptPath = Join-Path $runDirectory 'discovery.json'
                Write-WorkflowJson -Path $receiptPath -Value $discovery
                $result.DiscoveryReceiptPath = $receiptPath
                $result.Status = $discoveryStatus
                $result.ExitCode = if ($discoveryStatus -eq 'PASS') { 0 } else { if ($result.ExitCode -eq 0) { 1 } else { $result.ExitCode } }
                $result.Error = $firstFailure
                $result | Add-Member -NotePropertyName Discovery -NotePropertyValue $discovery -Force
            }
            else {
                Set-WorkflowRequestMetadataPath -Request $Request -Path $metadataPath
                $inventoryMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name 'selected-inventory' -Stage 'provenance' `
                    -TimeoutSeconds ([int]$Request.BudgetSeconds.inventorySeconds) -Command $Request.InventoryCommand -ProcessInvoker $ProcessInvoker
                $inventorySummary = Get-WorkflowPhaseSummary -Name 'selected-inventory' -Command $Request.InventoryCommand -Measurement $inventoryMeasurement
                $phases.Add($inventorySummary)
                if (!$inventoryMeasurement.Succeeded) {
                    $firstFailure = "Selected inventory failed ($($inventoryMeasurement.ExecutionReason), exit $($inventoryMeasurement.ExitCode))."
                    $result.ExitCode = Get-WorkflowFailureExitCode $inventoryMeasurement
                    throw $firstFailure
                }
                $identity = Get-WorkflowSelectedIdentity -InventoryPath ([string]$inventoryMeasurement.StdoutPath) -Selection $Request.Selection
                $Request.SelectedIdentity = $identity
                $runPhase = $Request.Phases[3]
                $runMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name 'selected-run' -Stage 'test' `
                    -TimeoutSeconds ([int]$Request.BudgetSeconds.executionSeconds) -Command $Request.RunCommand -ProcessInvoker $ProcessInvoker
                $runSummary = Get-WorkflowPhaseSummary -Name 'selected-run' -Command $Request.RunCommand -Measurement $runMeasurement
                $phases.Add($runSummary)
                if (!$runMeasurement.Succeeded) {
                    $firstFailure = "Selected test execution failed ($($runMeasurement.ExecutionReason), exit $($runMeasurement.ExitCode))."
                    $result.ExitCode = Get-WorkflowFailureExitCode $runMeasurement
                    throw $firstFailure
                }
                if ((Get-WorkflowSourceFingerprint) -cne $Request.SourceFingerprintSha256) {
                    $firstFailure = 'Source inputs changed during selected execution; provenance is uncertain.'
                    $result.ExitCode = 1
                    throw $firstFailure
                }
                $result.Status = 'PASS'
                $result.ExitCode = 0
                $result | Add-Member -NotePropertyName SelectedIdentity -NotePropertyValue $identity -Force
            }
        }
        else { throw "Unsupported workflow action/mode combination: $($Request.Action) / $($Request.Mode)." }
    }
    catch {
        if (!$firstFailure) { $firstFailure = $_.Exception.Message }
        if ($result.ExitCode -eq 0) { $result.ExitCode = 1 }
        if ($Request.Action -ne 'Discover' -or !$result.DiscoveryReceiptPath) { $result.Status = 'FAIL' }
        $result.Error = $firstFailure
    }

    $result.Phases = @($phases)
    $resultPath = Join-Path $runDirectory 'workflow.json'
    $result | Add-Member -NotePropertyName ReceiptPath -NotePropertyValue $resultPath -Force
    try { Write-WorkflowJson -Path $resultPath -Value $result }
    catch {
        if ($result.Status -eq 'PASS') { $result.Status = 'FAIL'; $result.ExitCode = 1 }
        if (!$result.Error) { $result.Error = "Workflow receipt could not be written: $($_.Exception.Message)" }
    }
    return $result
}
