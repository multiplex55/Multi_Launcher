$ErrorActionPreference = "Stop"

if ($args.Count -lt 1) {
    [Console]::Error.WriteLine("Nextest wrapper requires a test executable.")
    exit 2
}

$minimumStack = 4 * 1024 * 1024
$configuredStack = 0L
# Parse with invariant integer rules, then canonicalize the string so whitespace
# or the current locale cannot make Rust ignore a valid numeric value.
$validStack = [long]::TryParse(
    $env:RUST_MIN_STACK,
    [System.Globalization.NumberStyles]::Integer,
    [System.Globalization.CultureInfo]::InvariantCulture,
    [ref]$configuredStack
)
if (-not $validStack -or $configuredStack -lt $minimumStack) {
    $configuredStack = $minimumStack
}
$env:RUST_MIN_STACK = $configuredStack.ToString([System.Globalization.CultureInfo]::InvariantCulture)

$testExecutable = $args[0]
$testArguments = @()
if ($args.Count -gt 1) {
    $testArguments = $args[1..($args.Count - 1)]
}

& $testExecutable @testArguments
exit $LASTEXITCODE
