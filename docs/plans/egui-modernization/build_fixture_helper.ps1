param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$LauncherRlib,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$SerdeJsonRlib,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Sha2Rlib,

    [string]$ToolchainBin = (Join-Path $env:USERPROFILE '.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin')
)

$ErrorActionPreference = 'Stop'
$taskRepository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$taskDependencies = [IO.Path]::GetFullPath((Join-Path $taskRepository 'target/fast-dev/deps'))
$taskRustc = Join-Path $ToolchainBin 'rustc.exe'
$taskSource = Join-Path $PSScriptRoot 'prepare_native_fixture.rs'
$taskOutput = Join-Path $taskRepository 'target/egui-modernization/prepare_native_fixture.exe'
if (-not (Test-Path -LiteralPath $taskRustc -PathType Leaf)) {
    throw "rustc executable does not exist: $taskRustc"
}
if (-not (Test-Path -LiteralPath $taskDependencies -PathType Container)) {
    throw "Fast-dev dependency directory does not exist: $taskDependencies"
}

$taskExternArgs = @()
$taskResolvedLibraries = [ordered]@{}
$taskNativeArgs = @()
$taskLibraryInputs = @(
    [pscustomobject]@{ Crate = 'multi_launcher'; Path = $LauncherRlib },
    [pscustomobject]@{ Crate = 'serde_json'; Path = $SerdeJsonRlib },
    [pscustomobject]@{ Crate = 'sha2'; Path = $Sha2Rlib }
)
foreach ($taskInput in $taskLibraryInputs) {
    $taskLibraryPath = [IO.Path]::GetFullPath($taskInput.Path)
    if (-not (Test-Path -LiteralPath $taskLibraryPath -PathType Leaf)) {
        throw "Explicit $($taskInput.Crate) rlib does not exist: $taskLibraryPath"
    }

    $taskLibrary = Get-Item -LiteralPath $taskLibraryPath
    $taskExpectedPrefix = "lib$($taskInput.Crate)-"
    if (-not $taskLibrary.Name.StartsWith($taskExpectedPrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
        $taskLibrary.Extension -ne '.rlib') {
        throw "Expected a $($taskInput.Crate) rlib, received: $taskLibraryPath"
    }
    if (-not [System.String]::Equals(
        [IO.Path]::GetFullPath($taskLibrary.DirectoryName),
        $taskDependencies,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw "The explicit $($taskInput.Crate) rlib must be in target/fast-dev/deps: $taskLibraryPath"
    }

    $taskLibraryHash = (Get-FileHash -LiteralPath $taskLibrary.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $taskExternArgs += @('--extern', "$($taskInput.Crate)=$($taskLibrary.FullName)")
    $taskResolvedLibraries[$taskInput.Crate] = [ordered]@{
        path = "target/fast-dev/deps/$($taskLibrary.Name)"
        file = $taskLibrary.Name
        sha256 = $taskLibraryHash
    }
    Write-Output "fixture helper dependency: $($taskInput.Crate) $($taskLibrary.Name) sha256=$taskLibraryHash"
}

$taskCargoCache = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
foreach ($taskRegistry in Get-ChildItem (Join-Path $taskCargoCache 'registry/src') -Directory) {
    foreach ($taskWindows in Get-ChildItem $taskRegistry.FullName -Directory -Filter 'windows_x86_64_msvc-*') {
        $taskNativeDirectory = Join-Path $taskWindows.FullName 'lib'
        if (Test-Path $taskNativeDirectory) { $taskNativeArgs += @('-L', "native=$taskNativeDirectory") }
    }
}
foreach ($taskBuild in Get-ChildItem (Join-Path $taskRepository 'target/fast-dev/build') -Directory) {
    $taskNativeDirectory = Join-Path $taskBuild.FullName 'out'
    if ((Test-Path $taskNativeDirectory) -and (Get-ChildItem $taskNativeDirectory -Filter '*.lib')) {
        $taskNativeArgs += @('-L', "native=$taskNativeDirectory")
    }
}

New-Item -ItemType Directory -Path (Split-Path $taskOutput) -Force | Out-Null
$taskRustcVersion = (& $taskRustc --version)
if ($LASTEXITCODE -ne 0) { throw "Could not query rustc version (exit $LASTEXITCODE)" }
$taskRustcVersion = $taskRustcVersion.Trim()

& $taskRustc --edition=2024 --crate-name prepare_native_fixture $taskSource `
    @taskExternArgs -L "dependency=$taskDependencies" @taskNativeArgs -o $taskOutput
if ($LASTEXITCODE -ne 0) { throw "Fixture helper compilation failed with exit $LASTEXITCODE" }
foreach ($taskInput in $taskLibraryInputs) {
    $taskLibraryPath = [IO.Path]::GetFullPath($taskInput.Path)
    $taskActualHash = (Get-FileHash -LiteralPath $taskLibraryPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($taskActualHash -ne $taskResolvedLibraries[$taskInput.Crate].sha256) {
        throw "Selected $($taskInput.Crate) rlib changed while compiling the fixture helper"
    }
}

$taskHelperHash = (Get-FileHash -LiteralPath $taskOutput -Algorithm SHA256).Hash.ToLowerInvariant()
$taskSourceHash = (Get-FileHash -LiteralPath $taskSource -Algorithm SHA256).Hash.ToLowerInvariant()
$taskReceiptPath = Join-Path (Split-Path $taskOutput) "prepare_native_fixture.$taskHelperHash.build-receipt.json"
$taskBuildReceipt = [ordered]@{
    schema = 1
    artifact_selection = 'Explicit rlib paths supplied from the parent-verified Cargo JSON build receipt.'
    profile = 'fast-dev'
    target = 'x86_64-pc-windows-msvc'
    edition = '2024'
    rustc_version = $taskRustcVersion
    helper_source = 'docs/plans/egui-modernization/prepare_native_fixture.rs'
    helper_source_sha256 = $taskSourceHash
    helper_executable_sha256 = $taskHelperHash
    libraries = $taskResolvedLibraries
}
$taskReceiptJson = ConvertTo-Json -InputObject $taskBuildReceipt -Depth 6
$taskUtf8NoBom = [System.Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText($taskReceiptPath, $taskReceiptJson, $taskUtf8NoBom)

Write-Output "fixture helper built: $taskOutput"
Write-Output "fixture helper sha256: $taskHelperHash"
Write-Output "fixture helper build receipt: $taskReceiptPath"
