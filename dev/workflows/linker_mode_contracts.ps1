function Invoke-LinkerModeContractTests {
    $manifestPath = [IO.Path]::GetFullPath((Join-Path $script:WorkflowRoot 'Cargo.toml'))
    $cargoWrapper = Join-Path $script:WorkflowRoot 'dev\track_b\cargo.ps1'
    $linkerFixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('Multi Launcher\linker mode contracts-' + [guid]::NewGuid().ToString('N'))
    $linkerFixtureRepository = Join-Path $linkerFixtureRoot 'repository'
    $linkerFixtureHome = Join-Path $linkerFixtureRoot 'cargo home'
    $linkerFixtureDirectory = Join-Path $linkerFixtureRoot 'toolchain with spaces\bin'
    [IO.Directory]::CreateDirectory($linkerFixtureRepository) | Out-Null
    [IO.Directory]::CreateDirectory($linkerFixtureHome) | Out-Null
    [IO.Directory]::CreateDirectory($linkerFixtureDirectory) | Out-Null
    $fixtureLinkerPath = Join-Path $linkerFixtureDirectory 'rust-lld.exe'
    [IO.File]::WriteAllBytes($fixtureLinkerPath, [byte[]]@(0x4d, 0x5a))
    $environmentVariables = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process)
    $cargoHomeWasPresent = $environmentVariables.Contains('CARGO_HOME')
    $cargoHomeOriginal = [string]$environmentVariables['CARGO_HOME']
    $linkerOverrideName = 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER'
    $linkerOverrideWasPresent = $environmentVariables.Contains($linkerOverrideName)
    $linkerOverrideOriginal = [string]$environmentVariables[$linkerOverrideName]
    try {
        $expectedFixtureLinker = [IO.Path]::GetFullPath($fixtureLinkerPath).Replace('\', '\\')
        $fixtureArguments = @(New-BundledLldCargoArguments -Preset fast-dev-build -LinkerPath $fixtureLinkerPath)
        Assert-WorkflowSequence $fixtureArguments @(
            '--config', ('target.x86_64-pc-windows-msvc.linker="' + $expectedFixtureLinker + '"'),
            '--config', 'target.x86_64-pc-windows-msvc.rustflags=["-Clinker-flavor=lld-link"]'
        ) 'Bundled LLD TOML quoting and native argument boundaries'
        Assert-WorkflowThrows { New-BundledLldCargoArguments -Preset fast-dev-tests -WorkflowPhase inventory -LinkerPath $fixtureLinkerPath } 'Linker helper rejects metadata-only phase'
        Assert-WorkflowThrows { New-BundledLldCargoArguments -Preset fast-dev-build -LinkerPath 'relative\rust-lld.exe' } 'Linker helper rejects relative tool paths'
        Assert-WorkflowThrows { New-BundledLldCargoArguments -Preset fast-dev-build -LinkerPath (Join-Path $linkerFixtureDirectory 'lld.exe') } 'Linker helper requires rust-lld.exe'

        [Environment]::SetEnvironmentVariable('CARGO_HOME', $linkerFixtureHome, [EnvironmentVariableTarget]::Process)
        $fixtureCargoConfig = Join-Path $linkerFixtureHome 'config.toml'
        $configPolicyCases = @(
            @{ Label = 'Benign build and network settings'; Lines = @('[build]', 'jobs = 2', '[net]', 'retry = 2'); ShouldThrow = $false },
            @{ Label = 'Quoted target table linker'; Lines = @("[target.'x86_64-pc-windows-msvc']", '"linker" = "other-linker"'); ShouldThrow = $true; Message = 'target linker/rustflags' },
            @{ Label = 'Root inline target table'; Lines = @('target = { "x86_64-pc-windows-msvc" = { linker = "other-linker" } }'); ShouldThrow = $true; Message = 'root-level inline or dotted Cargo target' },
            @{ Label = 'Root dotted target table'; Lines = @('target.x86_64-pc-windows-msvc = { linker = "other-linker" }'); ShouldThrow = $true; Message = 'root-level inline or dotted Cargo target' },
            @{ Label = 'Quoted dotted target linker'; Lines = @('"target"."x86_64-pc-windows-msvc"."linker" = "other-linker"'); ShouldThrow = $true; Message = 'root-level inline or dotted Cargo target' },
            @{ Label = 'Dev profile table'; Lines = @('[profile.dev]', 'debug = 0'); ShouldThrow = $true; Message = 'dev/fast-dev profile' },
            @{ Label = 'Quoted fast-dev profile table'; Lines = @('["profile"."fast-dev"]', '"opt-level" = 3'); ShouldThrow = $true; Message = 'dev/fast-dev profile' },
            @{ Label = 'Escaped target table key'; Lines = @('["\u0074arget"."x86_64-pc-windows-msvc"]', 'linker = "other-linker"'); ShouldThrow = $true; Message = 'escaped Cargo table-key' },
            @{ Label = 'Root inline profile table'; Lines = @('profile = { dev = { debug = 0 } }'); ShouldThrow = $true; Message = 'root-level inline or dotted Cargo dev/fast-dev profile' },
            @{ Label = 'Quoted dotted profile key'; Lines = @('"profile"."fast-dev"."opt-level" = 3'); ShouldThrow = $true; Message = 'root-level inline or dotted Cargo dev/fast-dev profile' }
        )
        foreach ($policyCase in $configPolicyCases) {
            [IO.File]::WriteAllLines($fixtureCargoConfig, [string[]]$policyCase.Lines, [Text.UTF8Encoding]::new($false))
            if ($policyCase.ShouldThrow) {
                Assert-WorkflowThrowsContaining {
                    Assert-NoConflictingBundledLldConfiguration -RepositoryRoot $linkerFixtureRepository -CargoExecutable $cargoWrapper
                } $policyCase.Message $policyCase.Label
            } else {
                Assert-NoConflictingBundledLldConfiguration -RepositoryRoot $linkerFixtureRepository -CargoExecutable $cargoWrapper
            }
        }
        Remove-Item -LiteralPath $fixtureCargoConfig -Force
        [Environment]::SetEnvironmentVariable($linkerOverrideName, 'other-linker.exe', [EnvironmentVariableTarget]::Process)
        Assert-WorkflowThrows {
            Assert-NoConflictingBundledLldConfiguration -RepositoryRoot $linkerFixtureRepository -CargoExecutable $cargoWrapper
        } 'Bundled LLD rejects process linker overrides'
    } finally {
        if ($linkerOverrideWasPresent) {
            [Environment]::SetEnvironmentVariable($linkerOverrideName, $linkerOverrideOriginal, [EnvironmentVariableTarget]::Process)
        } else {
            Remove-Item -LiteralPath ('Env:{0}' -f $linkerOverrideName) -ErrorAction Stop
        }
        if ($cargoHomeWasPresent) {
            [Environment]::SetEnvironmentVariable('CARGO_HOME', $cargoHomeOriginal, [EnvironmentVariableTarget]::Process)
        } else {
            Remove-Item -LiteralPath 'Env:CARGO_HOME' -ErrorAction Stop
        }
        [IO.Directory]::Delete($linkerFixtureRoot, $true)
    }
    $restoredFixtureEnvironment = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process)
    Assert-WorkflowEqual $restoredFixtureEnvironment.Contains('CARGO_HOME') $cargoHomeWasPresent 'Linker test restores CARGO_HOME presence'
    if ($cargoHomeWasPresent) {
        Assert-WorkflowEqual ([string]$restoredFixtureEnvironment['CARGO_HOME']) $cargoHomeOriginal 'Linker test restores CARGO_HOME value'
    }
    Assert-WorkflowEqual $restoredFixtureEnvironment.Contains($linkerOverrideName) $linkerOverrideWasPresent 'Linker test restores linker override presence'
    if ($linkerOverrideWasPresent) {
        Assert-WorkflowEqual ([string]$restoredFixtureEnvironment[$linkerOverrideName]) $linkerOverrideOriginal 'Linker test restores linker override value'
    }

    $defaultFastBuild = (& $cargoWrapper -Preset fast-dev-build -PrintCommand) | ConvertFrom-Json
    Assert-WorkflowEqual $defaultFastBuild.Started $false 'Default fast-dev build preview state'
    Assert-WorkflowSequence @($defaultFastBuild.ArgumentList) @('build', '--profile', 'fast-dev', '--bin', 'multi_launcher', '--manifest-path', $manifestPath) 'Default fast-dev build compatibility'
    $defaultFastPrepare = (& $cargoWrapper -Preset fast-dev-tests -WorkflowPhase prepare -TargetKind lib -PrintCommand) | ConvertFrom-Json
    Assert-WorkflowEqual $defaultFastPrepare.Started $false 'Default fast-dev test preparation preview state'
    Assert-WorkflowSequence @($defaultFastPrepare.ArgumentList) @(
        'nextest', 'list', '--list-type', 'binaries-only', '--message-format', 'json', '--cargo-profile', 'fast-dev',
        '-p', 'multi_launcher', '--lib', '--manifest-path', $manifestPath
    ) 'Default fast-dev test preparation compatibility'
    Assert-WorkflowThrows { & $cargoWrapper -Preset release -LinkerMode bundled-lld -PrintCommand } 'Bundled LLD rejected for release'
    Assert-WorkflowThrows { & $cargoWrapper -Preset FAST-DEV-BUILD -LinkerMode bundled-lld -PrintCommand } 'Case-variant unsupported preset is rejected instead of defaulting'
        Assert-WorkflowThrows { & $cargoWrapper -Preset fast-dev-tests -WorkflowPhase PREPARE -TargetKind lib -LinkerMode bundled-lld -PrintCommand } 'Case-variant unsupported phase is rejected instead of defaulting'
    Assert-WorkflowThrows {
        & $cargoWrapper -Preset fast-dev-build -LinkerMode bundled-lld -CargoArguments @('--config', 'build.target="x86_64-pc-windows-msvc"') -PrintCommand
    } 'Bundled LLD owns Cargo configuration arguments'
    Assert-WorkflowThrows {
        & $cargoWrapper -Preset fast-dev-tests -WorkflowPhase inventory -TargetKind lib -BinariesMetadataPath (Join-Path $linkerFixtureRoot 'metadata.json') -LinkerMode bundled-lld -PrintCommand
    } 'Bundled LLD rejected for inventory'
    Assert-WorkflowThrows {
        & $cargoWrapper -Preset fast-dev-tests -WorkflowPhase run -TargetKind lib -FilterExpression 'test' `
            -BinariesMetadataPath (Join-Path $linkerFixtureRoot 'metadata.json') -LinkerMode bundled-lld -PrintCommand
    } 'Bundled LLD rejected for test execution'

    $linkerEnvironmentBefore = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process)
    $candidateFastBuild = (& $cargoWrapper -Preset fast-dev-build -LinkerMode BUNDLED-LLD -PrintCommand) | ConvertFrom-Json
    Assert-WorkflowEqual $candidateFastBuild.Started $false 'Bundled LLD build remains a preview'
    Assert-WorkflowEqual $candidateFastBuild.ArgumentList[0] 'build' 'Bundled LLD build remains an explicit Cargo build command'
    Assert-WorkflowEqual $candidateFastBuild.ArgumentList[1] '--config' 'Bundled LLD linker config follows the Cargo subcommand'
    $linkerSettingPrefix = 'target.x86_64-pc-windows-msvc.linker="'
    $linkerSetting = [string]$candidateFastBuild.ArgumentList[2]
    if (!$linkerSetting.StartsWith($linkerSettingPrefix, [StringComparison]::Ordinal) -or !$linkerSetting.EndsWith('"', [StringComparison]::Ordinal)) {
        throw "Bundled LLD command did not include a quoted linker value: $linkerSetting"
    }
    $tomlLinkerPath = $linkerSetting.Substring($linkerSettingPrefix.Length, $linkerSetting.Length - $linkerSettingPrefix.Length - 1)
    $candidateLinkerPath = $tomlLinkerPath.Replace('\\', '\')
    if (![IO.Path]::IsPathRooted($candidateLinkerPath) -or [IO.Path]::GetFileName($candidateLinkerPath) -cne 'rust-lld.exe' -or
        !(Test-Path -LiteralPath $candidateLinkerPath -PathType Leaf)) {
        throw "Bundled LLD command path does not resolve to the active rust-lld.exe: $candidateLinkerPath"
    }
    Assert-WorkflowSequence @($candidateFastBuild.ArgumentList[1..4]) @(
        '--config', $candidateFastBuild.ArgumentList[2], '--config', 'target.x86_64-pc-windows-msvc.rustflags=["-Clinker-flavor=lld-link"]'
    ) 'Bundled LLD config arguments stay separate'
    if (!(Test-VerifiedMsvcCargoCommand -Stage 'compile' -Executable $candidateFastBuild.Executable -ArgumentList @($candidateFastBuild.ArgumentList))) {
        throw 'Bundled LLD application command is not accepted by the existing verified MSVC auxiliary-process classifier.'
    }
    Assert-WorkflowSequence @($candidateFastBuild.ArgumentList[5..($candidateFastBuild.ArgumentList.Count - 1)]) @(
        '-p', 'multi_launcher', '--profile', 'fast-dev', '--bin', 'multi_launcher', '--manifest-path', $manifestPath
    ) 'Bundled LLD fast-dev build command'
    $candidateFastPrepare = (& $cargoWrapper -Preset fast-dev-tests -WorkflowPhase prepare -TargetKind lib -LinkerMode bundled-lld -PrintCommand) | ConvertFrom-Json
    Assert-WorkflowEqual $candidateFastPrepare.Started $false 'Bundled LLD test preparation remains a preview'
    Assert-WorkflowSequence @($candidateFastPrepare.ArgumentList[0..1]) @('nextest', 'list') 'Nextest owns its linker config forwarding boundary'
    Assert-WorkflowEqual $candidateFastPrepare.ArgumentList[3] $candidateFastBuild.ArgumentList[2] 'Nextest preparation uses the same resolved linker path as build mode'
    Assert-WorkflowSequence @($candidateFastPrepare.ArgumentList[2..5]) @(
        '--config', $candidateFastPrepare.ArgumentList[3], '--config', 'target.x86_64-pc-windows-msvc.rustflags=["-Clinker-flavor=lld-link"]'
    ) 'Bundled LLD Nextest config arguments stay separate'
    Assert-WorkflowSequence @($candidateFastPrepare.ArgumentList[6..($candidateFastPrepare.ArgumentList.Count - 1)]) @(
        '--list-type', 'binaries-only', '--message-format', 'json', '--cargo-profile', 'fast-dev',
        '-p', 'multi_launcher', '--lib', '--manifest-path', $manifestPath
    ) 'Bundled LLD fast-dev test preparation command'
    $coreCandidateFastPrepare = (& $cargoWrapper -Preset fast-dev-tests -WorkflowPhase prepare -TargetKind lib `
        -Packages @('multi_launcher_table_core') -LinkerMode bundled-lld -PrintCommand) | ConvertFrom-Json
    Assert-WorkflowSequence @($coreCandidateFastPrepare.ArgumentList[6..($coreCandidateFastPrepare.ArgumentList.Count - 1)]) @(
        '--list-type', 'binaries-only', '--message-format', 'json', '--cargo-profile', 'fast-dev',
        '-p', 'multi_launcher_table_core', '--lib', '--manifest-path', $manifestPath
    ) 'Bundled LLD preserves selected root-core package'
    $integrationCandidateFastPrepare = (& $cargoWrapper -Preset fast-dev-tests -WorkflowPhase prepare -TargetKind integration `
        -TestTarget 'history' -LinkerMode bundled-lld -PrintCommand) | ConvertFrom-Json
    Assert-WorkflowSequence @($integrationCandidateFastPrepare.ArgumentList[6..($integrationCandidateFastPrepare.ArgumentList.Count - 1)]) @(
        '--list-type', 'binaries-only', '--message-format', 'json', '--cargo-profile', 'fast-dev',
        '-p', 'multi_launcher', '--test', 'history', '--manifest-path', $manifestPath
    ) 'Bundled LLD preserves selected integration target'
    $linkerEnvironmentAfter = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process)
    foreach ($name in @('CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTFLAGS')) {
        Assert-WorkflowEqual $linkerEnvironmentAfter.Contains($name) $linkerEnvironmentBefore.Contains($name) "Bundled LLD preserves $name presence"
        if ($linkerEnvironmentBefore.Contains($name)) {
            Assert-WorkflowEqual ([string]$linkerEnvironmentAfter[$name]) ([string]$linkerEnvironmentBefore[$name]) "Bundled LLD preserves $name value"
        }
    }
}
