# Owned Windows process runs

Use `run_owned_process.ps1` for bounded test executables and native probes.
Requires Windows, PowerShell 7, and access to process inventory and Application
events (elevation may be needed). Use a fresh evidence directory for each run.

```powershell
& .\dev\run_owned_process.ps1 `
  -Executable (Resolve-Path '.\target\debug\deps\multi_launcher-<hash>.exe').Path `
  -Arguments @('--exact', 'mkmacro::ocr::tests::windows_ocr_repeated_operations_survive_worker_and_backend_teardown', '--ignored', '--nocapture') `
  -TimeoutSeconds 20 -EvidenceDirectory "$env:TEMP\ocr-native-run-001"
```

The native OCR regression requires an installed English OCR pack and a working
user-profile recognizer. Compile with a separately contained `cargo test --lib
--no-run` first. Launch the compiled test binary directly when its exact PID
must be recorded; 200 ms descendant sampling can miss short-lived test children
of a runner such as Nextest.

`Start-Process` joins the argument array into a Windows command line. Include
literal double quotes around arguments containing spaces or expressions that
need quoting, for example `@('nextest', 'run', '--lib', '-E', '"test(/^ocr::job::tests::/)"')`.
This script does not reinterpret arguments through another shell.

Windows start hidden by default. Use `-WindowStyle Normal` only for explicitly
requested interactive verification. Timeout, an owned crash reporter, or an
inventory failure triggers cleanup of the launched process tree and verified
recorded identities. A reporter whose target identity cannot be verified is
logged as uncertain and left alone. No processes are killed by name.

Evidence includes stdout/stderr, PID/creation/parent records, relevant Application
Error/WER events, termination/errors, post-cleanup identity checks, and
`result.json`. Event reporting may be delayed or unavailable; an empty event log
does not disprove a native crash. Unknown/alive cleanup states produce failure.
Exit 0 means successful execution and verified recorded-process cleanup; exit 1
means failure, with the executable's actual exit code preserved in `result.json`.
Log-storage failures are reported but cannot guarantee durable evidence.

Run `& .\dev\test_owned_process.ps1` to check a controlled sleeping parent/child
timeout, exit 7, and inventory-query failure with handle-based cleanup. It creates
only temporary fixture scripts/processes and retains each run's evidence; it
does not launch Multi Launcher.
