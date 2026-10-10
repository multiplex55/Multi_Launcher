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

# Cargo workflows

Run `track_b/cargo.ps1` from any directory to use one of its fixed presets.
The helper resolves this repository and `Cargo.toml` relative to its own path,
passes native arguments as an array, and leaves the caller's location and
environment unchanged.

```powershell
& .\dev\track_b\cargo.ps1 -Preset release
& .\dev\track_b\cargo.ps1 -Preset debug
& .\dev\track_b\cargo.ps1 -Preset lib-test -CargoArguments @('-E', 'test(history_prepare_)')
& .\dev\track_b\cargo.ps1 -Preset integration-test -TestTarget history
& .\dev\track_b\cargo.ps1 -Preset release -PrintCommand
```

The library-test preset runs the selected `--lib` suite; the integration-test
preset requires a target such as `history`. Forward Nextest filters and other
ordinary options through `-CargoArguments` as a string array, so a filter with
spaces stays one native argument. `-PrintCommand` reports the executable,
argument array, and fixed working directory without execution. The helper
rejects forwarded package, target, workspace, manifest, and Cargo compiler-
profile selectors; use Cargo directly for those advanced workflows. Nextest
runner profiles remain available through `-CargoArguments @('-P', 'default')`;
`--cargo-profile` is blocked because it changes the compiler profile.

For a quick library check, run `cargo check --lib`. To deliberately build all
package binaries, use `cargo build --release` directly; the release preset
selects only `multi_launcher`.

## Opt-in reduced-symbol development profile

`fast-dev` is accepted as an opt-in for repeated launcher builds and selected
Nextest compilation. It inherits the `dev` profile and stores line tables instead
of full debug symbols. Its artifacts use a separate `target/fast-dev` cache;
the default profile remains under `target/debug`.

~~~powershell
cargo build --profile fast-dev --bin multi_launcher
cargo nextest run --lib --cargo-profile fast-dev -E 'test(history_prepare_)'
~~~

The benchmark changed one exact `color.rs` formatting fixture on one Windows
host. These are separate first-edited-sample and exact-fixture-replay phases,
timed as whole commands after original-source artifact preparation:

| Phase | Command | `dev` seconds | `fast-dev` seconds |
| --- | --- | ---: | ---: |
| First edited sample | Launcher build | 144.058 | 93.992 |
| First edited sample | Library test build (`--no-run`) | 196.956 | 175.760 |
| First edited sample | `cargo check --lib` | 37.153 | 17.655 |
| Exact-fixture replay | Launcher build | 43.251 | 30.412 |
| Exact-fixture replay | Library test build (`--no-run`) | 40.201 | 36.489 |
| Exact-fixture replay | `cargo check --lib` | 12.629 | 13.493 |

The default `dev` cache already existed; `fast-dev` used a newly populated
profile cache. The baseline first sample was collected before a harness parser
interruption and imported into the resumed run; these samples were not one
uninterrupted paired batch. Separate profile-population times are excluded from
the edit table and are not a fair cold-cache comparison because the starting
cache histories differed. Results apply only to this host and fixture.

Three no-op medians (`dev` / `fast-dev`) were slower with `fast-dev`: launcher
0.881s / 1.350s, library test build 1.641s / 1.720s, and `cargo check --lib`
0.866s / 1.345s. The check result is mixed too: the first edited check was
faster with `fast-dev`, while exact-fixture replay was slower. Keep plain
`cargo check --lib` as the quick-check recommendation; opt into `fast-dev` for
launcher or selected-test compilation when the measured replay behavior suits
your workflow. See the [detailed Track B measurements](../docs/performance/track_b_results.md).

Line tables support source-line locations but omit much of the information
needed to inspect variables. Use the default profile when full debug information
is needed:

~~~powershell
cargo build --bin multi_launcher
cargo nextest run --lib -E 'test(history_prepare_)'
~~~

Timings are process wall time, not isolated linker time.
