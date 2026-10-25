# Shared, secret-safe provenance and comparison helpers for Track B.

function Get-TrackBTextSha256 {
    param([AllowEmptyString()][string]$Text)
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}

function ConvertTo-TrackBCanonicalValue {
    param($Value)
    if ($null -eq $Value -or $Value -is [string] -or $Value.GetType().IsValueType) { return $Value }
    if ($Value -is [System.Collections.IDictionary]) {
        $result = [ordered]@{}
        foreach ($key in @($Value.Keys | Sort-Object { [string]$_ })) {
            $result[[string]$key] = ConvertTo-TrackBCanonicalValue -Value $Value[$key]
        }
        return $result
    }
    if ($Value -is [pscustomobject]) {
        $result = [ordered]@{}
        foreach ($property in @($Value.PSObject.Properties | Sort-Object Name)) {
            $result[$property.Name] = ConvertTo-TrackBCanonicalValue -Value $property.Value
        }
        return $result
    }
    if ($Value -is [System.Collections.IEnumerable]) {
        $items = [System.Collections.Generic.List[object]]::new()
        foreach ($item in $Value) { $items.Add((ConvertTo-TrackBCanonicalValue -Value $item)) }
        return ,$items.ToArray()
    }
    return $Value
}

function Get-TrackBEnvironmentSnapshot {
    param([hashtable]$Overrides = @{})
    $fixedNames = @(
        'CARGO_HOME', 'CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET', 'CARGO_INCREMENTAL',
        'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_BUILD_JOBS', 'CARGO_NET_OFFLINE',
        'CARGO_REGISTRIES_CRATES_IO_PROTOCOL', 'RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER',
        'RUSTC_WORKSPACE_WRAPPER', 'RUSTDOC', 'CC', 'CXX', 'AR', 'LINKER'
    )
    $inheritedNames = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process).Keys |
        ForEach-Object { [string]$_ } | Where-Object {
            $_ -match '^CARGO_PROFILE_[A-Za-z0-9_]+$' -or
            $_ -match '^CARGO_TARGET_[A-Za-z0-9_]+_(?:RUSTFLAGS|LINKER)$'
        }
    $overrideNames = @($Overrides.Keys | ForEach-Object { [string]$_ } | Where-Object {
        $_ -match '^CARGO_PROFILE_[A-Za-z0-9_]+$' -or
        $_ -match '^CARGO_TARGET_[A-Za-z0-9_]+_(?:RUSTFLAGS|LINKER)$'
    })
    $names = @($fixedNames + $inheritedNames + $overrideNames + @($Overrides.Keys | ForEach-Object { [string]$_ }) | Sort-Object -Unique)
    $allowed = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($name in $fixedNames) { $null = $allowed.Add($name) }
    foreach ($name in $inheritedNames) { $null = $allowed.Add($name) }
    foreach ($name in $overrideNames) { $null = $allowed.Add($name) }

    $records = [System.Collections.Generic.List[object]]::new()
    $unknownOverrides = [System.Collections.Generic.List[string]]::new()
    foreach ($name in $names) {
        if ([string]::IsNullOrWhiteSpace($name) -or $name.Contains('=') -or $name.Contains([char]0)) { continue }
        if (!$allowed.Contains($name)) {
            $unknownOverrides.Add($name)
            $records.Add([pscustomobject][ordered]@{ Name = $name; State = 'explicit-override-not-fingerprinted'; ValueSha256 = $null })
            continue
        }
        $overrideFound = $false
        $overrideKey = $null
        foreach ($key in $Overrides.Keys) {
            if ([string]::Equals([string]$key, $name, [StringComparison]::OrdinalIgnoreCase)) {
                $overrideFound = $true
                $overrideKey = $key
                break
            }
        }
        $isPresent = $false
        $value = $null
        if ($overrideFound) {
            $value = $Overrides[$overrideKey]
            $isPresent = $null -ne $value
        }
        else {
            $isPresent = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process).Contains($name)
            if ($isPresent) { $value = [Environment]::GetEnvironmentVariable($name, 'Process') }
        }
        $state = if (!$isPresent) { 'absent' } elseif ([string]$value -ceq '') { 'empty' } else { 'set' }
        $records.Add([pscustomobject][ordered]@{
            Name = $name.ToUpperInvariant()
            State = $state
            ValueSha256 = if ($state -eq 'set') { Get-TrackBTextSha256 -Text ([string]$value) } else { $null }
        })
    }
    $json = ConvertTo-Json -InputObject @($records) -Depth 5 -Compress
    return [pscustomobject][ordered]@{
        Values = @($records)
        Sha256 = Get-TrackBTextSha256 -Text $json
        UnknownOverrides = @($unknownOverrides | Sort-Object -Unique)
        Complete = $unknownOverrides.Count -eq 0
    }
}

function Get-TrackBConfigurationSnapshot {
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$ArgumentList,
        [string]$CargoHome
    )
    $root = [IO.Path]::GetFullPath($SourceRoot)
    $files = [System.Collections.Generic.List[object]]::new()
    $complete = $true
    $knownInputs = @('Cargo.toml', 'Cargo.lock', 'build.rs', 'rust-toolchain', 'rust-toolchain.toml', '.cargo/config', '.cargo/config.toml')
    foreach ($relative in $knownInputs) {
        $path = Join-Path $root $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $item = Get-Item -LiteralPath $path -ErrorAction Stop
            if ($item.Length -gt 8388608) {
                $complete = $false
                $files.Add([pscustomobject][ordered]@{ Name = $relative.Replace('\', '/'); State = 'too-large'; Sha256 = $null })
            }
            else {
                $files.Add([pscustomobject][ordered]@{ Name = $relative.Replace('\', '/'); State = 'present'; Sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant() })
            }
        }
        else { $files.Add([pscustomobject][ordered]@{ Name = $relative.Replace('\', '/'); State = 'absent'; Sha256 = $null }) }
    }

    $current = [IO.Path]::GetFullPath($WorkingDirectory)
    $ancestorCount = 0
    while ($current) {
        foreach ($name in @('config.toml', 'config')) {
            $path = Join-Path (Join-Path $current '.cargo') $name
            $label = 'ancestor/' + [IO.Path]::GetRelativePath($root, $path).Replace('\', '/')
            if (Test-Path -LiteralPath $path -PathType Leaf) {
                $item = Get-Item -LiteralPath $path -ErrorAction Stop
                if ($item.Length -gt 8388608) {
                    $complete = $false
                    $files.Add([pscustomobject][ordered]@{ Name = $label; State = 'too-large'; Sha256 = $null })
                }
                else { $files.Add([pscustomobject][ordered]@{ Name = $label; State = 'present'; Sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant() }) }
            }
            else { $files.Add([pscustomobject][ordered]@{ Name = $label; State = 'absent'; Sha256 = $null }) }
        }
        $parent = [IO.Directory]::GetParent($current)
        if ($null -eq $parent) { break }
        $current = $parent.FullName
        $ancestorCount++
        if ($ancestorCount -ge 16) { $complete = $false; break }
    }

    $resolvedCargoHome = if (![string]::IsNullOrWhiteSpace($CargoHome)) { $CargoHome } else { Join-Path ([Environment]::GetFolderPath('UserProfile')) '.cargo' }
    foreach ($name in @('config.toml', 'config')) {
        $path = Join-Path $resolvedCargoHome $name
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $item = Get-Item -LiteralPath $path -ErrorAction Stop
            if ($item.Length -gt 8388608) {
                $complete = $false
                $files.Add([pscustomobject][ordered]@{ Name = "cargo-home/$name"; State = 'too-large'; Sha256 = $null })
            }
            else { $files.Add([pscustomobject][ordered]@{ Name = "cargo-home/$name"; State = 'present'; Sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant() }) }
        }
        else { $files.Add([pscustomobject][ordered]@{ Name = "cargo-home/$name"; State = 'absent'; Sha256 = $null }) }
    }

    $inlineIndex = 0
    for ($index = 0; $index -lt $ArgumentList.Count; $index++) {
        $argument = [string]$ArgumentList[$index]
        $configValue = $null
        if ($argument -ceq '--config') {
            if ($index + 1 -ge $ArgumentList.Count) { $complete = $false; continue }
            $configValue = [string]$ArgumentList[++$index]
        }
        elseif ($argument.StartsWith('--config=', [StringComparison]::Ordinal)) { $configValue = $argument.Substring(9) }
        if ($null -eq $configValue) { continue }
        $inlineIndex++
        $resolved = $null
        try {
            $candidate = if ([IO.Path]::IsPathRooted($configValue)) { [IO.Path]::GetFullPath($configValue) } else { [IO.Path]::GetFullPath((Join-Path $WorkingDirectory $configValue)) }
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { $resolved = $candidate }
        }
        catch { $resolved = $null }
        if ($resolved) {
            $item = Get-Item -LiteralPath $resolved -ErrorAction Stop
            if ($item.Length -gt 8388608) { $complete = $false; $files.Add([pscustomobject][ordered]@{ Name = "command-config/$inlineIndex"; State = 'too-large'; Sha256 = $null }) }
            else { $files.Add([pscustomobject][ordered]@{ Name = "command-config/$inlineIndex"; State = 'present'; Sha256 = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant() }) }
        }
        else {
            # Inline Cargo config is never copied to the receipt; only its digest is retained.
            $files.Add([pscustomobject][ordered]@{ Name = "command-config/$inlineIndex"; State = 'inline-hash'; Sha256 = Get-TrackBTextSha256 -Text $configValue })
        }
    }

    $ordered = @($files | Sort-Object Name, State)
    $json = ConvertTo-Json -InputObject $ordered -Depth 6 -Compress
    return [pscustomobject][ordered]@{
        Files = $ordered
        Sha256 = Get-TrackBTextSha256 -Text $json
        Complete = $complete
        InputLimitBytes = 8388608
        AncestorLimit = 16
    }
}

function Get-TrackBSourceIdentity {
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [Parameter(Mandatory)][string]$SourceSha,
        [Parameter(Mandatory)][string]$GitPath,
        [string]$BaselineSha
    )
    $byPath = [System.Collections.Generic.Dictionary[string, object]]::new([StringComparer]::OrdinalIgnoreCase)
    $complete = $true
    $changeMapStatus = 'unavailable-no-base'
    try {
        if (![string]::IsNullOrWhiteSpace($BaselineSha)) {
            if ($BaselineSha -notmatch '^(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})$') { throw 'Comparison baseline is not a full Git object ID.' }
            $diff = Invoke-TextProcess -ExecutablePath $GitPath -Arguments @('-C', $SourceRoot, 'diff', '--name-only', '--no-renames', '-z', "$BaselineSha..$SourceSha", '--') -WorkingDirectory $SourceRoot -TimeoutSeconds 10
            if ($diff.ExitCode -ne 0) { throw 'Bounded baseline-to-source Git diff failed.' }
            foreach ($path in $diff.Stdout.Split([char]0, [StringSplitOptions]::RemoveEmptyEntries)) {
                $normalized = ([string]$path).Replace('\', '/')
                $byPath[$normalized] = [pscustomobject][ordered]@{ Path = $normalized; Status = 'committed'; State = 'committed-change'; Sha256 = $null }
            }
            $changeMapStatus = 'known'
        }
        $raw = Invoke-TextProcess -ExecutablePath $GitPath -Arguments @('-C', $SourceRoot, 'status', '--porcelain=v1', '-z', '--untracked-files=all') -WorkingDirectory $SourceRoot -TimeoutSeconds 10
        if ($raw.ExitCode -ne 0) { throw 'git status did not complete successfully.' }
        $records = @($raw.Stdout.Split([char]0, [StringSplitOptions]::RemoveEmptyEntries))
        if ($records.Count -gt 2000) { throw 'Changed source file count exceeded the fingerprint bound.' }
        foreach ($record in $records) {
            if ($record.Length -lt 4) { $complete = $false; continue }
            $status = $record.Substring(0, 2)
            $relative = $record.Substring(3).Replace('\', '/')
            if ($status.Contains('R') -or $status.Contains('C')) { $complete = $false; continue }
            $path = [IO.Path]::GetFullPath((Join-Path $SourceRoot $relative))
            if (!(Test-Path -LiteralPath $path -PathType Leaf)) {
                $byPath[$relative] = [pscustomobject][ordered]@{ Path = $relative; Status = $status; State = 'deleted'; Sha256 = $null }
                continue
            }
            $item = Get-Item -LiteralPath $path -Force -ErrorAction Stop
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $item.Length -gt 1073741824) { $complete = $false; continue }
            $byPath[$relative] = [pscustomobject][ordered]@{ Path = $relative; Status = $status; State = 'present'; Sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant() }
        }
    }
    catch { $complete = $false }
    $ordered = @($byPath.Values | Sort-Object Path, Status)
    $sourceBytes = ConvertTo-Json -InputObject ([ordered]@{ Commit = $SourceSha; Baseline = $BaselineSha; Changed = $ordered }) -Depth 6 -Compress
    return [pscustomobject][ordered]@{
        Commit = $SourceSha
        BaselineSha = $BaselineSha
        ChangedFiles = $ordered
        ChangeMapStatus = $changeMapStatus
        Status = if ($complete) { 'known' } else { 'incomplete' }
        Sha256 = if ($complete) { Get-TrackBTextSha256 -Text $sourceBytes } else { $null }
    }
}

function Get-TrackBToolIdentity {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string[]]$VersionArguments, [Parameter(Mandatory)][scriptblock]$ProcessProbe)
    $tool = [ordered]@{ Status = 'unavailable'; Path = $Path; Sha256 = $null; Version = $null; ProbeTimeoutSeconds = 10; Error = $null }
    try {
        if (!(Test-Path -LiteralPath $Path -PathType Leaf)) { throw 'Resolved executable path does not exist.' }
        $tool.Sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
        $probe = & $ProcessProbe $Path ([string[]]$VersionArguments) 10
        if ($probe.ExitCode -ne 0 -or [string]::IsNullOrWhiteSpace([string]$probe.Stdout)) { throw 'Bounded version command returned no usable version.' }
        $version = ([string]$probe.Stdout).Trim()
        if ($version.Length -gt 8192) { throw 'Version output exceeded the retained bound.' }
        $tool.Version = $version
        $tool.Status = 'available'
    }
    catch { $tool.Error = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message }
    return [pscustomobject]$tool
}

function New-TrackBComparisonContext {
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [Parameter(Mandatory)][string]$SourceSha,
        [string]$BaselineSha,
        [Parameter(Mandatory)][string]$GitPath,
        [Parameter(Mandatory)][string]$InvokedExecutablePath,
        [string]$CargoPath,
        [string]$RustcPath,
        [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$ArgumentList,
        [Parameter(Mandatory)][string]$TargetDirectory,
        [Parameter(Mandatory)][string]$TargetDirectorySource,
        [Parameter(Mandatory)][string]$Profile,
        [Parameter(Mandatory)][string]$WorkKind,
        [Parameter(Mandatory)][string]$CacheState,
        [string]$ControlledChangeId,
        [string[]]$ControlledChangePaths = @(),
        [string[]]$FingerprintFlags = @(),
        [string[]]$TargetFlags = @(),
        [object]$Selection = @{},
        [hashtable]$EnvironmentOverrides = @{},
        [Parameter(Mandatory)][scriptblock]$ProcessProbe
    )
    $environment = Get-TrackBEnvironmentSnapshot -Overrides $EnvironmentOverrides
    $cargoHome = $null
    $cargoHomeItem = @($environment.Values | Where-Object Name -ceq 'CARGO_HOME')
    if ($cargoHomeItem.Count -eq 1 -and $cargoHomeItem[0].State -eq 'set') {
        foreach ($key in $EnvironmentOverrides.Keys) { if ([string]$key -ieq 'CARGO_HOME') { $cargoHome = [string]$EnvironmentOverrides[$key]; break } }
        if (!$cargoHome) { $cargoHome = [Environment]::GetEnvironmentVariable('CARGO_HOME', 'Process') }
    }
    $configuration = Get-TrackBConfigurationSnapshot -SourceRoot $SourceRoot -WorkingDirectory $WorkingDirectory `
        -ArgumentList $ArgumentList -CargoHome $cargoHome
    if (@($environment.Values | Where-Object { $_.Name -ceq 'CARGO_HOME' -and $_.State -eq 'empty' }).Count -gt 0) {
        $configuration.Complete = $false
    }
    $source = Get-TrackBSourceIdentity -SourceRoot $SourceRoot -SourceSha $SourceSha -GitPath $GitPath -BaselineSha $BaselineSha
    $invokedExecutable = [pscustomobject][ordered]@{
        Path = [IO.Path]::GetFullPath($InvokedExecutablePath)
        Sha256 = if (Test-Path -LiteralPath $InvokedExecutablePath -PathType Leaf) { (Get-FileHash -LiteralPath $InvokedExecutablePath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant() } else { $null }
        VersionProbe = if ($CargoPath) { 'cargo --version through checked owner' } else { 'not-run for generic executable' }
    }
    $cargo = if ($CargoPath) { Get-TrackBToolIdentity -Path $CargoPath -VersionArguments @('--version') -ProcessProbe $ProcessProbe } else {
        [pscustomobject][ordered]@{ Status = 'not-applicable'; Path = $null; Sha256 = $null; Version = $null; ProbeTimeoutSeconds = $null; Error = 'The measured executable is not an explicit Cargo compile command.' }
    }
    $rustc = if ($CargoPath -and $RustcPath) { Get-TrackBToolIdentity -Path $RustcPath -VersionArguments @('--version', '--verbose') -ProcessProbe $ProcessProbe } else {
        [pscustomobject][ordered]@{ Status = 'not-applicable'; Path = $null; Sha256 = $null; Version = $null; ProbeTimeoutSeconds = $null; Error = if ($CargoPath) { 'Rust compiler executable could not be resolved from the effective selection.' } else { 'Not probed for a generic executable.' } }
    }
    $hostNameHash = Get-TrackBTextSha256 -Text ([Environment]::MachineName.ToUpperInvariant())
    $comparisonHost = [pscustomobject][ordered]@{
        MachineNameSha256 = $hostNameHash
        OSDescription = [Runtime.InteropServices.RuntimeInformation]::OSDescription
        OSArchitecture = [string][Runtime.InteropServices.RuntimeInformation]::OSArchitecture
        ProcessArchitecture = [string][Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture
        PowerShellVersion = [string]$PSVersionTable.PSVersion
    }
    $argumentHash = Get-TrackBTextSha256 -Text (ConvertTo-Json -InputObject @($ArgumentList) -Depth 4 -Compress)
    $selectionSnapshot = ConvertTo-TrackBCanonicalValue -Value $Selection
    $fingerprintInput = [ordered]@{
        Host = $comparisonHost
        InvokedExecutable = $invokedExecutable
        Cargo = $cargo
        Rustc = $rustc
        WorkingDirectory = [IO.Path]::GetFullPath($WorkingDirectory)
        TargetDirectory = [IO.Path]::GetFullPath($TargetDirectory)
        TargetDirectorySource = $TargetDirectorySource
        Selection = $selectionSnapshot
        Profile = $Profile
        ArgumentsSha256 = $argumentHash
        FingerprintFlags = @($FingerprintFlags)
        TargetFlags = @($TargetFlags)
        EnvironmentSha256 = $environment.Sha256
        ConfigurationSha256 = $configuration.Sha256
    }
    $compatibilityComplete = $environment.Complete -and $configuration.Complete -and $source.Status -eq 'known' -and
        $CargoPath -and $cargo.Status -eq 'available' -and $rustc.Status -eq 'available'
    $context = [pscustomobject][ordered]@{
        SchemaVersion = 1
        ProvenanceStatus = if ($compatibilityComplete) { 'complete' } else { 'incomplete' }
        CompatibilityFingerprintSha256 = Get-TrackBTextSha256 -Text (ConvertTo-Json -InputObject $fingerprintInput -Depth 12 -Compress)
        CompatibilityCompleteness = $compatibilityComplete
        Host = $comparisonHost
        Tools = [pscustomobject][ordered]@{ InvokedExecutable = $invokedExecutable; Cargo = $cargo; Rustc = $rustc }
        Selection = [pscustomobject]$selectionSnapshot
        Invocation = [pscustomobject][ordered]@{ WorkingDirectory = [IO.Path]::GetFullPath($WorkingDirectory); ArgumentsSha256 = $argumentHash; FingerprintFlags = @($FingerprintFlags); TargetFlags = @($TargetFlags) }
        Environment = $environment
        Configuration = $configuration
        Source = $source
        Scenario = [pscustomobject][ordered]@{
            WorkKind = $WorkKind
            CacheState = $CacheState
            ControlledChangeId = $ControlledChangeId
            ControlledChangePaths = @($ControlledChangePaths | ForEach-Object { ([string]$_).Replace('\', '/') } | Sort-Object -Unique)
        }
        EffectiveTargetDirectory = [pscustomobject][ordered]@{ Path = [IO.Path]::GetFullPath($TargetDirectory); Source = $TargetDirectorySource }
        Profile = $Profile
        TimingInterpretation = 'Whole-command wall time; Cargo timing artifact remains a separate freshness receipt and linker time is not inferred.'
        Unknowns = @(
            if (!$environment.Complete) { 'One or more explicit environment overrides are outside the fingerprint allowlist.' }
            if (!$configuration.Complete) { 'One or more configuration/build inputs exceeded bounded discovery.' }
            if ($source.Status -ne 'known') { 'Changed-source fingerprint could not be completed.' }
            if ($CargoPath -and $cargo.Status -ne 'available') { 'Cargo version identity is unavailable.' }
            if ($CargoPath -and $rustc.Status -ne 'available') { 'Rust compiler version identity is unavailable.' }
            if (!$CargoPath) { 'Generic measured command is not probed as Cargo; compiler provenance is unavailable.' }
        )
    }
    return $context
}

function Get-TrackBRecordDisposition {
    param($Record)
    if ($null -eq $Record) { return 'missing' }
    if ($Record.TimedOut) { return 'interrupted-timeout' }
    if ($Record.ProcessStarted -and !$Record.ExecutionCompleted) { return 'interrupted' }
    if (!$Record.Succeeded -or $null -ne $Record.ExitCode -and [int]$Record.ExitCode -ne 0) { return 'failed' }
    if ($null -eq $Record.ComparisonContext -or $Record.ComparisonContext.SchemaVersion -ne 1) { return 'legacy-unknown' }
    if (!$Record.ComparisonContext.CompatibilityCompleteness -or !$Record.ComparisonContext.Source.Sha256) { return 'unknown-provenance' }
    return 'successful'
}

function Get-TrackBComparisonPropertySummary {
    param([Parameter(Mandatory)][string]$Property, $Value)
    if ($null -eq $Value) { return $null }
    switch ($Property) {
        'Host' {
            return [pscustomobject][ordered]@{
                MachineNameSha256 = $Value.MachineNameSha256
                OSDescription = $Value.OSDescription
                OSArchitecture = $Value.OSArchitecture
                ProcessArchitecture = $Value.ProcessArchitecture
                PowerShellVersion = $Value.PowerShellVersion
            }
        }
        'Tools' {
            $summary = [ordered]@{}
            foreach ($name in @('InvokedExecutable', 'Cargo', 'Rustc')) {
                $toolProperty = $Value.PSObject.Properties[$name]
                if ($null -eq $toolProperty -or $null -eq $toolProperty.Value) { continue }
                $tool = $toolProperty.Value
                $summary[$name] = [pscustomobject][ordered]@{
                    Status = $tool.Status
                    Path = $tool.Path
                    Sha256 = $tool.Sha256
                    Version = $tool.Version
                    VersionProbe = $tool.VersionProbe
                }
            }
            return [pscustomobject]$summary
        }
        'Selection' {
            $summary = [ordered]@{}
            foreach ($name in @('Mode', 'ModeKind', 'Package', 'TargetKind', 'TargetName', 'Features', 'CompilerProfile', 'RunnerProfile', 'SelectionName', 'ExactCase', 'SelectedIdentity')) {
                $selectionProperty = $Value.PSObject.Properties[$name]
                if ($null -ne $selectionProperty) { $summary[$name] = $selectionProperty.Value }
            }
            return [pscustomobject]$summary
        }
        'Profile' { return [string]$Value }
        'EffectiveTargetDirectory' {
            return [pscustomobject][ordered]@{ Path = $Value.Path; Source = $Value.Source }
        }
        'Environment' {
            return @($Value.Values | ForEach-Object {
                [pscustomobject][ordered]@{ Name = $_.Name; State = $_.State; ValueSha256 = $_.ValueSha256 }
            } | Sort-Object Name)
        }
        'Configuration' {
            return [pscustomobject][ordered]@{
                Sha256 = $Value.Sha256
                Files = @($Value.Files | ForEach-Object {
                    [pscustomobject][ordered]@{ Name = $_.Name; State = $_.State; Sha256 = $_.Sha256 }
                } | Sort-Object Name, State)
            }
        }
        'Invocation' {
            return [pscustomobject][ordered]@{
                WorkingDirectory = $Value.WorkingDirectory
                ArgumentsSha256 = $Value.ArgumentsSha256
                FingerprintFlags = @($Value.FingerprintFlags)
                TargetFlags = @($Value.TargetFlags)
            }
        }
        default { return $null }
    }
}

function Get-TrackBSourceSnapshotMap {
    param([object]$Source)
    $files = [System.Collections.Generic.Dictionary[string, object]]::new([StringComparer]::Ordinal)
    if ($null -eq $Source -or [string]$Source.Status -ne 'known' -or $null -eq $Source.PSObject.Properties['ChangedFiles']) {
        return [pscustomobject]@{ Complete = $false; Files = $files }
    }
    foreach ($entry in @($Source.ChangedFiles)) {
        if ($null -eq $entry -or $null -eq $entry.PSObject.Properties['Path'] -or
            $null -eq $entry.PSObject.Properties['State'] -or $null -eq $entry.PSObject.Properties['Sha256'] -or
            [string]::IsNullOrWhiteSpace([string]$entry.State)) {
            return [pscustomobject]@{ Complete = $false; Files = $files }
        }
        $path = ([string]$entry.Path).Replace('\', '/')
        if ([string]::IsNullOrWhiteSpace($path) -or $files.ContainsKey($path)) {
            return [pscustomobject]@{ Complete = $false; Files = $files }
        }
        $files.Add($path, [pscustomobject][ordered]@{ State = [string]$entry.State; Sha256 = $entry.Sha256 })
    }
    return [pscustomobject]@{ Complete = $true; Files = $files }
}

function Get-TrackBSourceSnapshotDifference {
    param([object]$LeftSource, [object]$RightSource)
    $left = Get-TrackBSourceSnapshotMap -Source $LeftSource
    $right = Get-TrackBSourceSnapshotMap -Source $RightSource
    if (!$left.Complete -or !$right.Complete) {
        return [pscustomobject]@{ Complete = $false; Paths = @() }
    }
    $paths = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($path in $left.Files.Keys) { $null = $paths.Add($path) }
    foreach ($path in $right.Files.Keys) { $null = $paths.Add($path) }
    $changed = [System.Collections.Generic.List[string]]::new()
    foreach ($path in $paths) {
        $leftFile = if ($left.Files.ContainsKey($path)) { $left.Files[$path] } else { $null }
        $rightFile = if ($right.Files.ContainsKey($path)) { $right.Files[$path] } else { $null }
        $leftIdentity = ConvertTo-Json -InputObject $leftFile -Depth 4 -Compress
        $rightIdentity = ConvertTo-Json -InputObject $rightFile -Depth 4 -Compress
        if ($leftIdentity -cne $rightIdentity) { $changed.Add($path) }
    }
    return [pscustomobject]@{ Complete = $true; Paths = @($changed | Sort-Object -Unique) }
}

function Compare-TrackBMeasurements {
    param([Parameter(Mandatory)][string[]]$MeasurementPath)
    if ($MeasurementPath.Count -lt 2 -or $MeasurementPath.Count -gt 50) { throw 'Comparison requires between two and fifty explicit measurement paths.' }
    $samples = [System.Collections.Generic.List[object]]::new()
    foreach ($suppliedPath in $MeasurementPath) {
        $path = [IO.Path]::GetFullPath($suppliedPath)
        if (!(Test-Path -LiteralPath $path -PathType Leaf)) {
            $samples.Add([pscustomobject][ordered]@{ Path = $path; Record = $null; Disposition = 'missing'; Error = 'Measurement file is missing.' })
            continue
        }
        try {
            $record = Get-Content -LiteralPath $path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
            $samples.Add([pscustomobject][ordered]@{ Path = $path; Record = $record; Disposition = (Get-TrackBRecordDisposition -Record $record); Error = $null })
        }
        catch {
            $samples.Add([pscustomobject][ordered]@{ Path = $path; Record = $null; Disposition = 'malformed'; Error = ('{0}: {1}' -f $_.Exception.GetType().Name, $_.Exception.Message) })
        }
    }

    $reasons = [System.Collections.Generic.List[string]]::new()
    $eligible = @($samples | Where-Object Disposition -eq 'successful')
    $compatible = $eligible.Count -eq $samples.Count
    if ($samples.Count -ne $eligible.Count) { $reasons.Add('One or more samples are missing, malformed, failed, interrupted, legacy, or have incomplete provenance.') }
    if ($compatible -and $eligible.Count -ge 2) {
        $fingerprint = [string]$eligible[0].Record.ComparisonContext.CompatibilityFingerprintSha256
        foreach ($sample in $eligible | Select-Object -Skip 1) {
            if ([string]$sample.Record.ComparisonContext.CompatibilityFingerprintSha256 -cne $fingerprint) {
                $compatible = $false
                $reasons.Add("Compatibility fingerprint differs: $($sample.Path)")
            }
        }
        $sourceFingerprints = @($eligible | ForEach-Object { [string]$_.Record.ComparisonContext.Source.Sha256 } | Sort-Object -Unique)
        if ($sourceFingerprints.Count -gt 1) {
            $changeIds = @($eligible | ForEach-Object { [string]$_.Record.ComparisonContext.Scenario.ControlledChangeId } | Sort-Object -Unique)
            $declaredPaths = @($eligible | ForEach-Object { (@($_.Record.ComparisonContext.Scenario.ControlledChangePaths) -join '|') } | Sort-Object -Unique)
            $editedKinds = @($eligible | Where-Object { $_.Record.ComparisonContext.Scenario.WorkKind -eq 'edited-source' })
            if ($changeIds.Count -ne 1 -or [string]::IsNullOrWhiteSpace($changeIds[0]) -or $declaredPaths.Count -ne 1 -or $editedKinds.Count -eq 0) {
                $compatible = $false
                $reasons.Add('Source identity differs without one shared explicit controlled-change id and path set.')
            }
            else {
                $sourceRecords = @($eligible | ForEach-Object { $_.Record.ComparisonContext.Source })
                $commitIds = @($sourceRecords | ForEach-Object { [string]$_.Commit } | Sort-Object -Unique)
                $baselineIds = @($sourceRecords | ForEach-Object { [string]$_.BaselineSha } | Sort-Object -Unique)
                $declared = @($declaredPaths[0] -split '\|' | Where-Object { $_ } | Sort-Object -Unique)
                $actualPaths = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
                $sourceMapsComplete = $true
                if ($commitIds.Count -eq 1 -and ![string]::IsNullOrWhiteSpace($commitIds[0])) {
                    $baselineSource = $eligible[0].Record.ComparisonContext.Source
                    foreach ($sample in $eligible | Select-Object -Skip 1) {
                        $sourceDifference = Get-TrackBSourceSnapshotDifference -LeftSource $baselineSource -RightSource $sample.Record.ComparisonContext.Source
                        if (!$sourceDifference.Complete) { $sourceMapsComplete = $false; break }
                        foreach ($path in $sourceDifference.Paths) { $null = $actualPaths.Add([string]$path) }
                    }
                }
                else {
                    $sourceMapKnown = @($sourceRecords | Where-Object { $_.ChangeMapStatus -eq 'known' }).Count -eq $sourceRecords.Count
                    $sourceMapsComplete = @($sourceRecords | Where-Object { $_.Status -ne 'known' -or $null -eq $_.PSObject.Properties['ChangedFiles'] }).Count -eq 0
                    foreach ($sourceRecord in $sourceRecords) {
                        foreach ($entry in @($sourceRecord.ChangedFiles)) { if ($null -ne $entry.Path) { $null = $actualPaths.Add([string]$entry.Path) } }
                    }
                    if (!$sourceMapKnown -or $baselineIds.Count -ne 1 -or [string]::IsNullOrWhiteSpace($baselineIds[0])) { $sourceMapsComplete = $false }
                }
                $actualPathArray = @($actualPaths | Sort-Object -Unique)
                $pathSetsMatch = $actualPathArray.Count -eq $declared.Count
                if ($pathSetsMatch) {
                    for ($pathIndex = 0; $pathIndex -lt $actualPathArray.Count; $pathIndex++) {
                        if ($actualPathArray[$pathIndex] -cne $declared[$pathIndex]) { $pathSetsMatch = $false; break }
                    }
                }
                if (!$sourceMapsComplete -or !$pathSetsMatch) {
                    $compatible = $false
                    $reasons.Add('Source differences are not confined to the declared controlled-change paths under a shared verifiable base.')
                }
                else { $reasons.Add("Source changed under controlled change '$($changeIds[0])' for $($declaredPaths[0]).") }
            }
        }
        if ($eligible.Count -gt 0) {
            $workKinds = @($eligible | ForEach-Object { [string]$_.Record.ComparisonContext.Scenario.WorkKind } | Sort-Object -Unique)
            if ($workKinds.Count -gt 1) { $reasons.Add('Work classifications differ; timings are observational and not an improvement claim.') }
            $cacheStates = @($eligible | ForEach-Object { [string]$_.Record.ComparisonContext.Scenario.CacheState } | Sort-Object -Unique)
            if ($cacheStates -contains 'unknown') { $reasons.Add('Cache state is explicitly unknown for at least one sample.') }
        }
    }
    if (!$compatible -and $reasons.Count -eq 0) { $reasons.Add('Samples do not form a complete compatible group.') }

    $timings = @($eligible | ForEach-Object { if ($null -ne $_.Record.WallSeconds) { [double]$_.Record.WallSeconds } })
    $delta = $null
    $percent = $null
    if ($compatible -and $timings.Count -ge 2) {
        $delta = [Math]::Round($timings[-1] - $timings[0], 6)
        if ($timings[0] -gt 0) { $percent = [Math]::Round(($delta / $timings[0]) * 100.0, 3) }
    }
    $mean = if ($timings.Count -gt 0) { ($timings | Measure-Object -Average).Average } else { $null }
    $cv = $null
    $noisy = $false
    if ($timings.Count -ge 3 -and $mean -gt 0) {
        $squares = @($timings | ForEach-Object { [Math]::Pow(([double]$_ - $mean), 2) })
        $stddev = [Math]::Sqrt(($squares | Measure-Object -Average).Average)
        $cv = [Math]::Round(($stddev / $mean) * 100.0, 3)
        $noisy = $cv -gt 10.0
    }
    $differences = [System.Collections.Generic.List[string]]::new()
    $differenceDetails = [System.Collections.Generic.List[object]]::new()
    $detailBaseline = @($samples | Where-Object { $null -ne $_.Record -and $null -ne $_.Record.ComparisonContext } | Select-Object -First 1)[0]
    if ($null -ne $detailBaseline) {
        for ($sampleIndex = 0; $sampleIndex -lt $samples.Count; $sampleIndex++) {
            $sample = $samples[$sampleIndex]
            if ($sample.Path -ceq $detailBaseline.Path -or $null -eq $sample.Record -or $null -eq $sample.Record.ComparisonContext) { continue }
            $left = $detailBaseline.Record.ComparisonContext
            $right = $sample.Record.ComparisonContext
            foreach ($property in @('Host', 'Tools', 'Selection', 'Profile', 'EffectiveTargetDirectory', 'Environment', 'Configuration', 'Invocation')) {
                $leftSummary = Get-TrackBComparisonPropertySummary -Property $property -Value $left.$property
                $rightSummary = Get-TrackBComparisonPropertySummary -Property $property -Value $right.$property
                $leftJson = ConvertTo-Json -InputObject $leftSummary -Depth 12 -Compress
                $rightJson = ConvertTo-Json -InputObject $rightSummary -Depth 12 -Compress
                if ($leftJson -cne $rightJson) {
                    if (!$differences.Contains($property)) { $differences.Add($property) }
                    $differenceDetails.Add([pscustomobject][ordered]@{
                        Property = $property
                        ComparedToSamplePath = $detailBaseline.Path
                        SampleIndex = $sampleIndex + 1
                        SamplePath = $sample.Path
                        From = Get-TrackBComparisonPropertySummary -Property $property -Value $left.$property
                        To = Get-TrackBComparisonPropertySummary -Property $property -Value $right.$property
                    })
                }
            }
        }
    }
    $record = [pscustomobject][ordered]@{
        SchemaVersion = 1
        Status = if ($compatible) { 'COMPARABLE' } else { 'INCOMPATIBLE_OR_INCOMPLETE' }
        Samples = @($samples | ForEach-Object {
            [pscustomobject][ordered]@{
                Path = $_.Path; Disposition = $_.Disposition; Error = $_.Error
                Scenario = if ($_.Record.ComparisonContext) { $_.Record.ComparisonContext.Scenario } else { $null }
                Source = if ($_.Record.ComparisonContext) { $_.Record.ComparisonContext.Source } else { $null }
                WallSeconds = $_.Record.WallSeconds
                TimingFreshness = if ($_.Record.CargoTimings) { $_.Record.CargoTimings.Disposition } else { 'unavailable' }
                TimingSnapshotPath = if ($_.Record.CargoTimings) { $_.Record.CargoTimings.SnapshotPath } else { $null }
                LinkerDuration = 'NOT_MEASURED_BY_PROCESS_WALL_TIME'
            }
        })
        Reasons = @($reasons)
        ConfigurationDifferences = @($differences)
        ConfigurationDifferenceDetails = @($differenceDetails)
        WallSeconds = @($timings)
        LastMinusFirstSeconds = $delta
        LastMinusFirstPercent = $percent
        Variability = [pscustomobject][ordered]@{ SampleCount = $timings.Count; CoefficientOfVariationPercent = $cv; Noisy = $noisy; Assessment = if ($timings.Count -lt 3) { 'insufficient repeats for noise assessment' } else { 'population variation only; not a confidence interval' } }
        ImprovementClaim = $false
        ImprovementClaimReason = if (!$compatible) { 'Incompatible or incomplete samples do not support an improvement claim.' } elseif ($noisy) { 'Observed variation is noisy; no improvement claim is made.' } else { 'Timing is descriptive only; work/cache evidence does not establish a speedup cause.' }
    }
    return $record
}

function Get-TrackBEvidenceRetentionReport {
    param(
        [Parameter(Mandatory)][string]$OwnedEvidenceRoot,
        [ValidateRange(1, 100000)][int]$MaximumFiles = 5000,
        [ValidateRange(1, 100000)][int]$MaximumEntries = 10000,
        [ValidateRange(1, 1099511627776)][long]$MaximumBytes = 5368709120
    )
    $root = [IO.Path]::GetFullPath($OwnedEvidenceRoot)
    if (!(Test-Path -LiteralPath $root -PathType Container)) { throw 'Owned evidence root must already exist as a directory.' }
    $rootItem = Get-Item -LiteralPath $root -Force -ErrorAction Stop
    if (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Owned evidence root cannot be a reparse point.' }
    $files = 0
    $entries = 0
    $bytes = [long]0
    $unknown = $false
    $truncated = $false
    $truncationReason = $null
    $pending = [System.Collections.Generic.Queue[string]]::new()
    $pending.Enqueue($root)
    while ($pending.Count -gt 0 -and !$unknown -and !$truncated) {
        $directory = $pending.Dequeue()
        foreach ($entryPath in [IO.Directory]::EnumerateFileSystemEntries($directory)) {
            $entries++
            if ($entries -gt $MaximumEntries) { $truncated = $true; $truncationReason = 'entry-limit'; break }
            $attributes = [IO.File]::GetAttributes($entryPath)
            if (($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { $unknown = $true; break }
            if (($attributes -band [IO.FileAttributes]::Directory) -ne 0) { $pending.Enqueue($entryPath); continue }
            $files++
            $bytes += [long]([IO.FileInfo]::new($entryPath).Length)
            if ($files -gt $MaximumFiles) { $truncated = $true; $truncationReason = 'file-limit'; break }
            if ($bytes -gt $MaximumBytes) { $truncated = $true; $truncationReason = 'byte-limit'; break }
        }
    }
    return [pscustomobject][ordered]@{
        SchemaVersion = 1
        Root = $root
        FileCount = $files
        EntryCount = $entries
        ByteCount = $bytes
        MaximumFiles = $MaximumFiles
        MaximumEntries = $MaximumEntries
        MaximumBytes = $MaximumBytes
        LimitExceeded = $unknown -or $truncated -or $files -gt $MaximumFiles -or $bytes -gt $MaximumBytes
        Truncated = $truncated
        TruncationReason = $truncationReason
        CensusComplete = !$unknown -and !$truncated
        Action = 'report-only; no files are deleted or rewritten'
        ProtectedFailureAndReplayEvidence = 'preserved; this helper never deletes evidence'
    }
}
