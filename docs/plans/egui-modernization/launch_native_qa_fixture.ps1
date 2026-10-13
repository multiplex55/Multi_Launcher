param(
    [Parameter(Mandatory)][ValidatePattern('^run-[a-zA-Z0-9_-]+$')][string]$RunLeaf,
    [Parameter(Mandatory)][ValidatePattern('^build-[a-f0-9]+$')][string]$BuildLeaf
)

$ErrorActionPreference = 'Stop'
$taskRepository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$taskRun = Join-Path $taskRepository ('target/egui-modernization/native-baseline/' + $RunLeaf)
$taskBuild = Join-Path $taskRepository ('target/egui-modernization/qa-framebuffer/' + $BuildLeaf)
$taskBuildReceipt = Get-Content -LiteralPath (Join-Path $taskBuild 'build-receipt.json') -Raw | ConvertFrom-Json
$taskExecutable = Join-Path $taskBuild 'multi_launcher.exe'
$taskExeHash = (Get-FileHash -LiteralPath $taskExecutable -Algorithm SHA256).Hash
if ($taskExeHash -ne $taskBuildReceipt.executable_sha256) { throw 'Executable hash mismatch' }
$taskRequest = Join-Path $taskRun 'capture-request.json'
$taskProcessReceipt = Join-Path $taskRun 'process-receipt.json'
if (!(Test-Path -LiteralPath $taskRequest) -or (Test-Path -LiteralPath $taskProcessReceipt)) {
    throw 'Request missing or fixture already launched'
}
$taskPreSettingsCopy = Join-Path $taskRun 'prelaunch-settings.json'
if (Test-Path -LiteralPath $taskPreSettingsCopy) { throw 'Prelaunch settings snapshot already exists' }
Copy-Item -LiteralPath (Join-Path $taskRun 'settings.json') -Destination $taskPreSettingsCopy -ErrorAction Stop
$taskPreSettings = (Get-FileHash -LiteralPath $taskPreSettingsCopy -Algorithm SHA256).Hash
if ((Get-FileHash -LiteralPath (Join-Path $taskRun 'settings.json') -Algorithm SHA256).Hash -ne $taskPreSettings) {
    throw 'Settings changed while taking the prelaunch snapshot'
}
$taskEnvironment = @{
    ML_NOTES_DIR = (Join-Path $taskRun 'notes')
    ML_NOTE_TEMPLATES_DIR = (Join-Path $taskRun 'templates')
    ML_SKIP_CLIPBOARD_SYNC = '1'
    ML_QA_FRAMEBUFFER_REQUEST_MANIFEST = $taskRequest
    MULTI_LAUNCHER_PERF = $null
    ML_TRACK_C_BENCH_MODE = $null
}
$taskPrior = @{}
foreach ($taskName in $taskEnvironment.Keys) {
    $taskPrior[$taskName] = [Environment]::GetEnvironmentVariable($taskName, 'Process')
}
try {
    foreach ($taskName in $taskEnvironment.Keys) {
        if ($null -eq $taskEnvironment[$taskName]) {
            if (Test-Path -LiteralPath ('Env:\' + $taskName)) { Remove-Item -LiteralPath ('Env:\' + $taskName) }
        } else {
            [Environment]::SetEnvironmentVariable($taskName, $taskEnvironment[$taskName], 'Process')
        }
    }
    if ((Test-Path Env:\MULTI_LAUNCHER_PERF) -or (Test-Path Env:\ML_TRACK_C_BENCH_MODE)) {
        throw 'Benchmark environment present'
    }
    $taskProcess = Start-Process -FilePath $taskExecutable -WorkingDirectory $taskRun -PassThru -WindowStyle Normal `
        -RedirectStandardOutput (Join-Path $taskRun 'app-stdout.log') -RedirectStandardError (Join-Path $taskRun 'app-stderr.log')
    [ordered]@{
        pid = $taskProcess.Id
        started_at = $taskProcess.StartTime.ToString('o')
        executable = $taskExecutable
        executable_sha256 = $taskExeHash
        source_commit = $taskBuildReceipt.source_commit
        source_kind = $taskBuildReceipt.source_kind
        instrumentation_patch_sha256 = $taskBuildReceipt.instrumentation_patch_sha256
        build_receipt_sha256 = (Get-FileHash -LiteralPath (Join-Path $taskBuild 'build-receipt.json') -Algorithm SHA256).Hash
        cwd = $taskRun
        prelaunch_settings_sha256 = $taskPreSettings
        prelaunch_settings_file = 'prelaunch-settings.json'
        request_sha256 = (Get-FileHash -LiteralPath $taskRequest -Algorithm SHA256).Hash
        window_style = 'Normal'
        performance_variable_present = $false
        track_c_variable_present = $false
        environment_check = 'Test-Path Env: immediately before launch'
        launch_script_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash
    } | ConvertTo-Json -Depth 5 | Tee-Object -FilePath $taskProcessReceipt
} finally {
    foreach ($taskName in $taskPrior.Keys) {
        if ($null -eq $taskPrior[$taskName]) {
            if (Test-Path -LiteralPath ('Env:\' + $taskName)) { Remove-Item -LiteralPath ('Env:\' + $taskName) }
        } else {
            [Environment]::SetEnvironmentVariable($taskName, $taskPrior[$taskName], 'Process')
        }
    }
}
