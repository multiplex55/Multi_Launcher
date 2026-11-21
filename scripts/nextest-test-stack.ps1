$ErrorActionPreference = "Stop"

if ($args.Count -lt 1) {
    [Console]::Error.WriteLine("Nextest wrapper requires a test executable.")
    exit 2
}

$minimumStack = 4 * 1024 * 1024
$configuredStack = 0L
# Preserve an explicit larger caller setting, but raise unset/invalid/smaller values.
if (-not [long]::TryParse($env:RUST_MIN_STACK, [ref]$configuredStack) -or
    $configuredStack -lt $minimumStack) {
    $env:RUST_MIN_STACK = [string]$minimumStack
}

$testExecutable = $args[0]
$testArguments = @()
if ($args.Count -gt 1) {
    $testArguments = $args[1..($args.Count - 1)]
}

& $testExecutable @testArguments
exit $LASTEXITCODE
