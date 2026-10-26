# Shared, narrowly scoped argument and preflight helpers for cargo.ps1's
# opt-in bundled Windows linker mode. This file does not launch Cargo.

function New-BundledLldCargoArguments {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateSet('fast-dev-build', 'fast-dev-tests')][string]$Preset,
        [string]$WorkflowPhase,
        [Parameter(Mandatory)][string]$LinkerPath
    )

    if ($Preset -eq 'fast-dev-tests' -and $WorkflowPhase -cne 'prepare') {
        throw 'The bundled LLD mode is valid only for the fast-dev-tests prepare phase.'
    }
    if ($Preset -eq 'fast-dev-build' -and ![string]::IsNullOrWhiteSpace($WorkflowPhase)) {
        throw 'The bundled LLD mode does not accept workflow phases for fast-dev-build.'
    }
    if (![IO.Path]::IsPathRooted($LinkerPath) -or [IO.Path]::GetFileName($LinkerPath) -cne 'rust-lld.exe') {
        throw 'The bundled LLD mode requires an absolute rust-lld.exe path resolved from the active Rust sysroot.'
    }

    $resolvedLinkerPath = [IO.Path]::GetFullPath($LinkerPath)
    if (!(Test-Path -LiteralPath $resolvedLinkerPath -PathType Leaf)) {
        throw "The active sysroot's bundled rust-lld.exe was not found: $resolvedLinkerPath"
    }
    foreach ($character in $resolvedLinkerPath.ToCharArray()) {
        if ([char]::IsControl($character) -or $character -eq '"') {
            throw 'The bundled linker path contains a character that cannot be represented safely in Cargo TOML.'
        }
    }

    # Cargo parses each --config value as TOML. Keep paths with spaces in one
    # argument and escape Windows backslashes for a TOML basic string.
    $tomlLinkerPath = $resolvedLinkerPath.Replace('\', '\\')
    $linkerOverride = 'target.x86_64-pc-windows-msvc.linker="' + $tomlLinkerPath + '"'
    $rustflagsOverride = 'target.x86_64-pc-windows-msvc.rustflags=["-Clinker-flavor=lld-link"]'
    return @('--config', $linkerOverride, '--config', $rustflagsOverride)
}

function Get-CargoLinkerConfigFiles {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$RepositoryRoot,
        [Parameter(Mandatory)][string]$CargoExecutable
    )

    $directories = [System.Collections.Generic.List[string]]::new()
    $cursor = [IO.DirectoryInfo]([IO.Path]::GetFullPath($RepositoryRoot))
    while ($null -ne $cursor) {
        $directories.Add((Join-Path $cursor.FullName '.cargo'))
        $cursor = $cursor.Parent
    }

    $cargoHome = [Environment]::GetEnvironmentVariable('CARGO_HOME', [EnvironmentVariableTarget]::Process)
    if ([string]::IsNullOrWhiteSpace($cargoHome)) {
        $userHome = [Environment]::GetEnvironmentVariable('HOME', [EnvironmentVariableTarget]::Process)
        if ([string]::IsNullOrWhiteSpace($userHome)) {
            $userHome = [Environment]::GetEnvironmentVariable('USERPROFILE', [EnvironmentVariableTarget]::Process)
        }
        if (![string]::IsNullOrWhiteSpace($userHome)) {
            $cargoHome = Join-Path $userHome '.cargo'
        } else {
            $cargoHome = Split-Path -Parent (Split-Path -Parent $CargoExecutable)
        }
    }
    if (![string]::IsNullOrWhiteSpace($cargoHome)) {
        $directories.Add([IO.Path]::GetFullPath($cargoHome))
    }

    $files = [System.Collections.Generic.List[string]]::new()
    foreach ($directory in @($directories | Select-Object -Unique)) {
        foreach ($name in @('config.toml', 'config')) {
            $path = Join-Path $directory $name
            if (Test-Path -LiteralPath $path -PathType Leaf) {
                $files.Add([IO.Path]::GetFullPath($path))
            }
        }
    }
    return @($files | Select-Object -Unique)
}

function ConvertTo-CargoConfigPathShape {
    param([Parameter(Mandatory)][string]$Text)

    $unquoted = [regex]::Replace($Text.Trim(), '["'']', '')
    $normalized = [regex]::Replace($unquoted, '\s*\.\s*', '.')
    return $normalized.Trim().ToLowerInvariant()
}

function Assert-NoConflictingBundledLldConfiguration {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$RepositoryRoot,
        [Parameter(Mandatory)][string]$CargoExecutable
    )

    $target = 'x86_64-pc-windows-msvc'
    $ordinaryCargoHint = ' Use ordinary Cargo mode by omitting -LinkerMode bundled-lld.'
    $processEnvironment = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process)
    $targetOverride = [string]$processEnvironment['CARGO_BUILD_TARGET']
    if ($processEnvironment.Contains('CARGO_BUILD_TARGET') -and ![string]::IsNullOrWhiteSpace($targetOverride) -and $targetOverride -cne $target) {
        throw ("The bundled LLD mode supports only the native target '$target'; CARGO_BUILD_TARGET selects '$targetOverride'." + $ordinaryCargoHint)
    }

    $overrides = @(
        'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER',
        'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS',
        'CARGO_BUILD_RUSTFLAGS',
        'CARGO_BUILD_RUSTC',
        'CARGO_BUILD_RUSTC_WRAPPER',
        'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER',
        'CARGO_ENCODED_RUSTFLAGS',
        'RUSTFLAGS',
        'RUSTC',
        'RUSTC_WRAPPER',
        'RUSTC_WORKSPACE_WRAPPER'
    )
    foreach ($name in $overrides) {
        if ($processEnvironment.Contains($name)) {
            throw "The bundled LLD mode refuses the process override '$name'; remove the override or use the default Cargo mode."
        }
    }
    foreach ($name in @($processEnvironment.Keys | Where-Object { [string]$_ -imatch '^CARGO_PROFILE_(?:FAST_DEV|DEV)_.+$' })) {
        throw "The bundled LLD mode refuses the profile override '$name'; remove the override or use the default Cargo mode."
    }

    # This deliberately recognizes a small TOML key/table subset. Ordinary
    # settings such as [build].jobs and [net].retry are allowed. Target and
    # dev/fast-dev profile shapes that can change this candidate are refused,
    # including inline and dotted forms that a line-only parser cannot safely
    # merge with Cargo's precedence rules. Escaped key/table spellings are
    # explicitly unsupported in this opt-in mode rather than decoded here.
    foreach ($path in Get-CargoLinkerConfigFiles -RepositoryRoot $RepositoryRoot -CargoExecutable $CargoExecutable) {
        $section = ''
        $lineNumber = 0
        foreach ($line in [IO.File]::ReadAllLines($path)) {
            $lineNumber++
            $trimmed = $line.Trim()
            if ([string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith('#', [StringComparison]::Ordinal)) { continue }

            $table = [regex]::Match($trimmed, '^\[(?<section>[^\]]+)\]\s*(?:#.*)?$')
            if ($trimmed.StartsWith('[', [StringComparison]::Ordinal)) {
                if (!$table.Success) {
                    $tableShape = ConvertTo-CargoConfigPathShape -Text $trimmed
                    if ($tableShape -match '(?:^|[^a-z0-9_-])(?:target|profile|build)(?:[^a-z0-9_-]|$)') {
                        throw ("The bundled LLD mode cannot safely classify a relevant Cargo table at '$path' line $lineNumber; remove the unsupported table shape." + $ordinaryCargoHint)
                    }
                    $section = ''
                    continue
                }

                $rawSection = $table.Groups['section'].Value
                if ($rawSection.Contains('\')) {
                    throw ("The bundled LLD mode refuses escaped Cargo table-key spellings in '$path' line $lineNumber (section [$rawSection]); use unescaped keys." + $ordinaryCargoHint)
                }
                $section = ConvertTo-CargoConfigPathShape -Text $rawSection
                if ($section -ceq 'target' -or $section -match '^target\..*\.(?:linker|rustflags)(?:\.|$)') {
                    throw ("The bundled LLD mode refuses an unsupported Cargo target table in '$path' line $lineNumber (section [$section])." + $ordinaryCargoHint)
                }
                if ($section -ceq 'profile' -or $section -match '^profile\.(?:dev|fast-dev)(?:\.|$)') {
                    throw ("The bundled LLD mode refuses Cargo dev/fast-dev profile configuration in '$path' line $lineNumber (section [$section])." + $ordinaryCargoHint)
                }
                if ($section -match '^build\.(?:target|rustflags|rustc|rustc-wrapper|rustc-workspace-wrapper)(?:\.|$)') {
                    throw ("The bundled LLD mode refuses an unsupported Cargo build table in '$path' line $lineNumber (section [$section])." + $ordinaryCargoHint)
                }
                continue
            }

            $equalsIndex = $trimmed.IndexOf('=')
            if ($equalsIndex -lt 0) {
                $lineShape = ConvertTo-CargoConfigPathShape -Text $trimmed
                if ($section -match '^(?:target|build|profile)(?:\.|$)' -or
                    $lineShape -match '(?:^|\.)(?:target|profile|linker|rustflags|rustc|rustc-wrapper|rustc-workspace-wrapper)(?:\.|$)') {
                    throw ("The bundled LLD mode cannot safely classify a relevant Cargo setting in '$path' line $lineNumber (section [$section])." + $ordinaryCargoHint)
                }
                continue
            }

            $rawKey = $trimmed.Substring(0, $equalsIndex).Trim()
            if ($rawKey.Contains('\')) {
                throw ("The bundled LLD mode refuses escaped Cargo key spellings in '$path' line $lineNumber (key '$rawKey'); use unescaped keys." + $ordinaryCargoHint)
            }
            $key = ConvertTo-CargoConfigPathShape -Text $rawKey
            $relevantKeyShape = $key -match '^(?:[a-z0-9_-]+\.)*[a-z0-9_-]+$'
            $keyPath = if ([string]::IsNullOrWhiteSpace($section)) { $key } else { "$section.$key" }
            if (!$relevantKeyShape -and $keyPath -match '(?:^|\.)(?:target|profile|build|linker|rustflags|rustc|rustc-wrapper|rustc-workspace-wrapper)(?:\.|$)') {
                throw ("The bundled LLD mode cannot safely classify a relevant Cargo key in '$path' line $lineNumber (section [$section], key '$key')." + $ordinaryCargoHint)
            }

            if ([string]::IsNullOrWhiteSpace($section)) {
                if ($key -ceq 'target' -or $key.StartsWith('target.', [StringComparison]::Ordinal)) {
                    throw ("The bundled LLD mode refuses root-level inline or dotted Cargo target configuration in '$path' line $lineNumber (key '$key')." + $ordinaryCargoHint)
                }
                if ($key -ceq 'profile' -or $key -match '^profile\.(?:dev|fast-dev)(?:\.|$)') {
                    throw ("The bundled LLD mode refuses root-level inline or dotted Cargo dev/fast-dev profile configuration in '$path' line $lineNumber (key '$key')." + $ordinaryCargoHint)
                }
                if ($key -match '^build\.(?:target|rustflags|rustc|rustc-wrapper|rustc-workspace-wrapper)(?:\.|$)') {
                    throw ("The bundled LLD mode refuses a conflicting Cargo build setting in '$path' line $lineNumber (key '$key')." + $ordinaryCargoHint)
                }
            }

            if ($section -ceq 'profile' -and $key -match '^(?:dev|fast-dev)(?:\.|$)') {
                throw ("The bundled LLD mode refuses an inline Cargo dev/fast-dev profile setting in '$path' line $lineNumber (key '$key')." + $ordinaryCargoHint)
            }
            if ($section -ceq 'build' -and $key -match '^(?:target|rustflags|rustc|rustc-wrapper|rustc-workspace-wrapper)(?:\.|$)') {
                throw ("The bundled LLD mode refuses an existing Cargo build target/compiler setting in '$path' line $lineNumber (key '$key')." + $ordinaryCargoHint)
            }
            if ($section.StartsWith('target.', [StringComparison]::Ordinal) -and $key -match '^(?:linker|rustflags)(?:\.|$)') {
                throw ("The bundled LLD mode refuses an existing Cargo target linker/rustflags setting in '$path' line $lineNumber (section [$section], key '$key')." + $ordinaryCargoHint)
            }
            if ([string]::IsNullOrWhiteSpace($section) -and $key -ceq 'build' -and $trimmed.Substring($equalsIndex + 1).TrimStart().StartsWith('{', [StringComparison]::Ordinal)) {
                throw ("The bundled LLD mode cannot safely classify a root-level inline Cargo build table in '$path' line $lineNumber." + $ordinaryCargoHint)
            }
        }
    }
}

function Resolve-ActiveSysrootBundledLld {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$RepositoryRoot)

    $rustcApplication = Get-Command -Name 'rustc.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1
    if ($null -eq $rustcApplication) { throw 'rustc.exe was not found as a native application on PATH.' }
    $rustcPath = [IO.Path]::GetFullPath($(if ($rustcApplication.Source) { $rustcApplication.Source } else { $rustcApplication.Path }))
    if ([IO.Path]::GetExtension($rustcPath) -in @('.ps1', '.cmd', '.bat')) {
        throw "The active rustc resolver must be a native executable: $rustcPath"
    }

    $probeRoot = Join-Path ([IO.Path]::GetTempPath()) 'MultiLauncher\cargo-linker-mode\sysroot-probes'
    $probeDirectory = Join-Path $probeRoot ('probe-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($probeDirectory) | Out-Null
    . (Join-Path $RepositoryRoot 'dev\workflows\owned_process.ps1')

    $versionStdoutPath = Join-Path $probeDirectory 'rustc-vv-stdout.bin'
    $versionStderrPath = Join-Path $probeDirectory 'rustc-vv-stderr.bin'
    $versionProbe = Invoke-CheckedOwnedProcess -Executable $rustcPath -ArgumentList @('-vV') `
        -WorkingDirectory $RepositoryRoot -Stage 'provenance' -TimeoutSeconds 15 -CleanupTimeoutSeconds 5 `
        -StdoutPath $versionStdoutPath -StderrPath $versionStderrPath
    $versionReceiptPath = Join-Path $probeDirectory 'rustc-vv-owner.json'
    [IO.File]::WriteAllText($versionReceiptPath, (ConvertTo-Json -InputObject $versionProbe -Depth 12))
    if (!$versionProbe.Succeeded -or !$versionProbe.CleanupVerified -or !$versionProbe.OutputDrainVerified) {
        throw "The bounded rustc host probe failed ($($versionProbe.Reason), exit $($versionProbe.ExitCode)); receipt: $versionReceiptPath"
    }

    $sysrootStdoutPath = Join-Path $probeDirectory 'sysroot-stdout.bin'
    $sysrootStderrPath = Join-Path $probeDirectory 'sysroot-stderr.bin'
    $sysrootProbe = Invoke-CheckedOwnedProcess -Executable $rustcPath -ArgumentList @('--print', 'sysroot') `
        -WorkingDirectory $RepositoryRoot -Stage 'provenance' -TimeoutSeconds 15 -CleanupTimeoutSeconds 5 `
        -StdoutPath $sysrootStdoutPath -StderrPath $sysrootStderrPath
    $sysrootReceiptPath = Join-Path $probeDirectory 'rustc-sysroot-owner.json'
    [IO.File]::WriteAllText($sysrootReceiptPath, (ConvertTo-Json -InputObject $sysrootProbe -Depth 12))
    if (!$sysrootProbe.Succeeded -or !$sysrootProbe.CleanupVerified -or !$sysrootProbe.OutputDrainVerified) {
        throw "The bounded rustc sysroot probe failed ($($sysrootProbe.Reason), exit $($sysrootProbe.ExitCode)); receipt: $sysrootReceiptPath"
    }

    $rustcVersionOutput = [IO.File]::ReadAllText($versionStdoutPath)
    $hostMatch = [regex]::Match($rustcVersionOutput, '(?m)^host:\s*(?<host>\S+)\s*$')
    if (!$hostMatch.Success -or $hostMatch.Groups['host'].Value -cne 'x86_64-pc-windows-msvc') {
        $reportedHost = if ($hostMatch.Success) { $hostMatch.Groups['host'].Value } else { 'missing' }
        throw "The active rustc host is '$reportedHost'; bundled Windows LLD mode requires x86_64-pc-windows-msvc. Receipt: $versionReceiptPath"
    }

    $sysrootOutput = [IO.File]::ReadAllText($sysrootStdoutPath).Trim()
    if ([string]::IsNullOrWhiteSpace($sysrootOutput) -or !(Test-Path -LiteralPath $sysrootOutput -PathType Container)) {
        throw "rustc --print sysroot did not identify an existing active sysroot directory; receipt: $sysrootReceiptPath"
    }
    $sysroot = [IO.Path]::GetFullPath($sysrootOutput)
    $targetLibDirectory = Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-msvc\lib'
    if (!(Test-Path -LiteralPath $targetLibDirectory -PathType Container)) {
        throw "The active Rust sysroot does not contain the native target libraries: $targetLibDirectory; receipt: $sysrootReceiptPath"
    }

    $linkerPath = Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe'
    if (!(Test-Path -LiteralPath $linkerPath -PathType Leaf)) {
        throw "The active Rust sysroot does not contain its bundled Windows linker: $linkerPath; receipt: $sysrootReceiptPath"
    }
    return [IO.Path]::GetFullPath($linkerPath)
}
