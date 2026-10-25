# Owned Windows process runs

For target-first interactive Nextest runs, bounded profiling and
machine-readable receipts, see the [test performance workflow](test_perf/README.md).

```powershell
& .\dev\test_perf\run.ps1 -TargetKind Integration -Target pure -CargoProfile fast-dev
```

Select a Cargo target before filtering cases: a case filter alone may still
compile unrelated test targets. The helper prints build/selection/execution
phases and preserves Nextest's native progress and exit status. Profile caches
must be prepared separately; this task's paired warm no-op measurements found
`dev` and `fast-dev` effectively tied, so defaults remain unchanged.

For opt-in benchmark probes and bounded Job Object execution, see the
[performance harness](perf_harness/README.md).

Use `run_owned_process.ps1` for bounded native executables. Requires Windows
and PowerShell 7. The runner creates the process suspended, verifies assignment
to a kill-on-close Job Object, and only then resumes it; it does not scan or kill
processes by name. It does not query Application event providers. Use a
fresh evidence directory for each run.

```powershell
& .\dev\run_owned_process.ps1 `
  -Executable (Resolve-Path '.\target\debug\deps\multi_launcher-<hash>.exe').Path `
  -Arguments @('--exact', 'mkmacro::ocr::tests::windows_ocr_repeated_operations_survive_worker_and_backend_teardown', '--ignored', '--nocapture') `
  -TimeoutSeconds 20 -EvidenceDirectory "$env:TEMP\ocr-native-run-001"
```

The native OCR regression requires an installed English OCR pack and a working
user-profile recognizer. Compile with a separately contained `cargo test --lib
--no-run` first. The runner records process identities from its verified Job
Object, including short-lived descendants while they remain active.

`-ArgumentList` accepts raw native arguments and keeps spaces, embedded quotes,
and filter punctuation in their original argument. The legacy `-Arguments`
parameter remains pre-encoded for existing callers such as `dev/test_perf`; do
not pass raw strings to that compatibility parameter when they require quoting.

Windows start hidden by default. Use `-WindowStyle Normal` only for explicitly
requested interactive verification. Timeout or an ownership-monitor failure
triggers cleanup of the verified Job Object and recorded identities. No
processes are killed by name.

The workflow facade keeps auxiliary-process handling disabled by default. It
opts into `verified-msvc-vctip` only for its fixed Cargo compiler and target-
preparation phases after a bounded preflight verifies the installed MSVC
`vctip.exe` signature and held file identity. The checked owner still requires
the live PID, creation identity, canonical image and exact Job membership to
match; all survivors must be verified, and the existing five-second grace and
final Job-empty, output, recording and cleanup checks still apply. Unknown or
mixed descendants remain failures. Run
`& .\dev\workflows\test_msvc_auxiliary.ps1` for its focused lifecycle fixtures.

Evidence includes stdout/stderr, PID/creation records, execution/cleanup errors,
post-cleanup identity checks, and `result.json`. For compatibility,
`windows-events.json` remains an empty JSON array; `EventCollectionStatus` is
`NotCollected` with an explanatory `EventCollectionReason`. This means events
were not queried, not that no crash occurred. Optional event providers cannot
delay process-result publication. Unknown/alive cleanup states produce failure.
Exit 0 means successful execution and verified recorded-process cleanup; exit 1
means failure, with the executable's actual exit code preserved in `result.json`.
Log-storage failures are reported but cannot guarantee durable evidence.

Run `& .\dev\test_owned_process.ps1` to check a controlled sleeping parent/child
timeout, exit 7, and inventory-query failure with handle-based cleanup. It creates
only temporary fixture scripts/processes and retains each run's evidence; it
does not launch Multi Launcher.

# Cargo workflows

Run `track_b/cargo.ps1` from any directory to use one of its fixed presets.
The optional Track B PowerShell helpers require PowerShell 7; test commands also
require Cargo Nextest. They do not require sccache, an alternative linker, or
editor configuration.
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

## Bounded workflow facade

`workflows/run.ps1` resolves a fixed catalog request and routes each native
command through `track_b/measure.ps1` and the checked owned-process runner. Its
build modes are `quick-check`, `full-debug`, `fast-dev-build`, `iteration`, and
`production`; `fast-dev-tests` is the selected-test mode. The production mode
uses the existing release profile. All modes keep Cargo's default features and
the facade does not expand a selection to the whole suite.

```powershell
& .\dev\workflows\run.ps1 -Action Preview -Mode quick-check
& .\dev\workflows\run.ps1 -Action Discover -Mode fast-dev-tests -TargetKind lib
& .\dev\workflows\run.ps1 -Action Discover -Mode fast-dev-tests -TargetKind integration
& .\dev\workflows\run.ps1 -Action Preview -Mode fast-dev-tests -SelectionName tables `
    -DiscoveryReceipt '<discovery.json path from the Discover result>'
& .\dev\workflows\run.ps1 -Action Run -Mode fast-dev-tests -SelectionName tables
& .\dev\workflows\run.ps1 -Action Replay -ReplayReceipt '<original failed workflow.json path>'
```

Build previews print the resolved executable and native argument array without
launching it. Test previews require a successful discovery receipt whose
catalog, source fingerprint, Cargo/Nextest tools, metadata file, target, and
exact case still match; missing or stale receipts require another Discover.
Discover prepares only the requested library or `pure` integration target,
then inventories its catalog cases without running them. Run prepares the same
target, validates that the exact catalog case matches one runnable test, and
executes only that case. Target preparation has an 1800-second deadline;
inventory and selected test execution each have a 600-second deadline. Every
phase publishes its arguments, exit/timeout, output-drain, owned-process and
cleanup evidence under an external evidence directory.

Each selected run writes `results.json` with terminal pass/fail/timeout counts,
ignored and filtered cases, unexecuted or started-only cases, per-case durations,
and the slowest cases. Incomplete output records observed timeout events while
leaving the total timeout count unknown (`TimedOut: null`), rather than claiming
an incomplete stream had zero timeouts. A generic failed case also leaves that
total unknown because its failure is not a reliable timeout classification.
Suite `started` and `ok`/`failed` terminal events are recorded separately from
case counts; a missing suite terminal makes the results incomplete. Preparation
time is reported by its own phase receipt and does not contribute to test-case
duration. Unknown identities or uncertain cleanup also make the results
incomplete and the workflow fails closed.

Replay accepts only an original failed `Run` receipt. It preserves the recorded
runner profile and test arguments, prepares the current target again, verifies
the exact binary-qualified failed case in a fresh inventory, then intersects
that identity with Nextest's portable recording. The receipt records whether
source inputs changed since the original failure; catalog, tool, archive, or
identity mismatches are rejected. The original failure receipt and archive are
left unchanged.

The test catalog is `workflows/catalog.json`. Each test entry records its
package, Cargo target, exact case, source, default-feature set, host/resource
notes, and native-selection metadata. Those pure-test entries remain separate
from the Phase 2 smoke selections below. `fast-dev-tests` uses `--cargo-profile fast-dev` for
compilation; an optional `-RunnerProfile` maps separately to Nextest `-P`.
Forward test-binary options as a string array with `-TestArguments`, for
example `-TestArguments @('--exact', 'table_utility::tests::table_parse_csv_quotes_escaped_quotes_and_embedded_newlines_preserve_strings')`;
the facade places those arguments after the native `--` separator. Structured
workflow results require captured test output, so `--nocapture` and
`--no-capture` are rejected before launch. Use capture-preserving filters such
as `--exact` when narrowing a selected test.

Run the focused reducer and facade fixtures with
`& .\dev\workflows\test_results.ps1` and
`& .\dev\workflows\test_run.ps1`. These synthetic fixtures cover structured
result reduction, duplicate/ambiguous identities, retry counts, slow-case
ordering, exact replay filters, catalog/source agreement, argument arrays,
preview and execution resolution, stale/empty discovery, replay provenance,
and bounded failure propagation without compiling the application.

The same catalog also declares two checked Phase 2 Settings smoke selections.
Preview or run the typed-fixture-only selection with
`-SmokeSelectionName phase2-settings-fixture`, or select the native workflow
with `-SmokeSelectionName phase2-settings-native`:

```powershell
& .\dev\workflows\run.ps1 -Action Preview -SmokeSelectionName phase2-settings-fixture
& .\dev\workflows\run.ps1 -Action Run -SmokeSelectionName phase2-settings-fixture `
    -EvidenceDirectory (Join-Path $env:TEMP ('multi-launcher-settings-fixture-' + [guid]::NewGuid().ToString('N')))
& .\dev\workflows\run.ps1 -Action Run -SmokeSelectionName phase2-settings-native `
    -EvidenceDirectory (Join-Path $env:TEMP ('multi-launcher-settings-native-' + [guid]::NewGuid().ToString('N')))
```

Both selections use the fixed existing Phase 2 Settings scenario and explicit
Cargo features. The facade measures metadata, fixture-test preparation and
execution, fixture generation/verification, and (for the native selection) the
application build through the existing checked command owner. The native run
allows at most three ROOT/F11 captures and gives post-build interaction,
process shutdown, tree verification, and output draining one 300-second
deadline with a 30-second shutdown reserve. It reports native acceptance only
when linked capture evidence and owned-process cleanup are verified. An
unavailable interactive desktop is recorded as `NOT_RUN_ENVIRONMENT`, never as
a native pass. The fixture profile disables Paste Sequences' Next and Cancel
shortcuts before application construction. These smoke requests do not widen
the existing pure-test selections or change production settings.

Native capture and exact-failure replay were verified in a bounded isolated
Rust library fixture (this does not substitute for application or GUI tests).
The original run failed its one selected case with a complete structured result
and no parse errors. Its Nextest run ID was
`e3fe9c18-fb77-4517-9296-e7204e6e2c3e`; replay run ID was
`c27799e2-7f86-4556-92cc-fe0f23135d77`. Nextest exported a 12,016-byte archive with SHA-256
`c9ba7cd1493443181c0daa24b9a8832d6312cd864ebaba588017812f4d243cd5`. After the
fixture source was corrected, replay selected the same binary-qualified case
with the same test arguments and passed. The replay disclosed the source change,
used a short private staged copy with the same archive hash, and removed that
copy after execution. Original receipt, result, stdout, stderr and archive
hashes remained unchanged. The checked owner and trusted MSVC helper cleanup
verified all 270 recorded identity observations absent; the generated fixture
workspace was removed and recorded in
`supplemental-fixture-workspace-cleanup.json`.

The earlier cold-preparation, capture-format, long-export-path and environment
restoration failures remain in separate evidence directories as history. The
focused environment regression is
`& .\dev\track_b\test_measure_environment.ps1`; it checks absent, empty and
null-override restoration. Structured workflows continue to reject
`--nocapture`/`--no-capture` and publish archives through short private staging.

For a quick library check, run `cargo check --lib`. To deliberately build all
package binaries, use `cargo build --release` directly; the release preset
selects only `multi_launcher`.

## Build measurement and comparison

For repeatable build observations, run the fixed workflow with explicit work
and cache labels. Leave either label `unknown` unless its state was actually
established; the report does not turn an unknown or warm-cache timing into an
optimization claim.

```powershell
$evidence = Join-Path $env:TEMP ('multi-launcher-quick-check-pair-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($evidence) | Out-Null
$first = Join-Path $evidence 'sample-1'
$second = Join-Path $evidence 'sample-2'
& .\dev\workflows\run.ps1 -Action Run -Mode quick-check `
    -WorkKind unknown -CacheState unknown -EvidenceDirectory $first
& .\dev\workflows\run.ps1 -Action Run -Mode quick-check `
    -WorkKind unknown -CacheState unknown -EvidenceDirectory $second
$measurements = foreach ($sample in @($first, $second)) {
    $workflow = Get-Content -Raw (Get-ChildItem $sample -Filter workflow.json -Recurse -File |
        Select-Object -First 1 -ExpandProperty FullName) | ConvertFrom-Json
    $workflow.Phases[0].MeasurementPath
}
& .\dev\track_b\compare_measurements.ps1 -MeasurementPath $measurements `
    -OwnedEvidenceRoot $evidence -MaximumEvidenceFiles 5000 `
    -MaximumEvidenceEntries 10000 -MaximumEvidenceBytes 5368709120 `
    -OutputPath (Join-Path $evidence 'comparison.json')
```

Quick-check executes `cargo check --lib` with a compile deadline of 1,800
seconds. Each measurement records whole-command wall time, source identity,
host and tool identity, selected target/features/profile, effective target
directory, allowlisted environment state and hashes, and Cargo configuration
file labels and hashes. The environment allowlist includes inherited
`CARGO_BUILD_RUSTFLAGS` and target-specific `CARGO_TARGET_<TRIPLE>_RUSTFLAGS`
and `CARGO_TARGET_<TRIPLE>_LINKER` settings. Cargo and Rustc version probes run
through the checked owner only for an explicit Cargo compile command. Generic
measured executables are never invoked with guessed version arguments. Raw environment values and
configuration file contents are not retained; an explicit environment override
outside the allowlist makes provenance incomplete. The comparison details
show sanitized before/after values, including environment names/states/hashes
and configuration file labels/hashes.

The comparison helper rejects missing, malformed, legacy, failed, timed-out or
incomplete-provenance samples. It keeps Cargo timing-artifact freshness separate
from wall time, and never infers linker duration from process time. Evidence
retention is an explicit report-only census bounded by file count, directory
entry count and total bytes; it reports truncation or excess without deleting
or rewriting evidence. Comparison output must be a new path and all input
measurements must be inside the explicitly selected evidence root when a
retention report is requested.

The M2-B representative pair used the same quick-check request and unchanged
source, with work/cache labels left unknown. Its measured wall times were
0.895 and 0.867 seconds; the cache cause was not established. The comparison
was compatible but explicitly made no improvement claim; these timings are
workflow evidence, not a speedup result. The retained comparison receipt
records the full provenance and report-only census.

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

## Release-like local iteration profile

The `iteration` profile is accepted for opt-in local launcher builds. It writes to
`target/iteration`, separate from canonical release artifacts in `target/release`:

~~~powershell
cargo build --profile iteration --bin multi_launcher
cargo build --release --bin multi_launcher
~~~

It inherits `release` with `opt-level = 2`, `codegen-units = 64`,
`incremental = true`, and `lto = "off"`. These choices aim to favor repeated
local builds: lower optimization and more codegen units can reduce compile work,
while incremental mode can reuse work between edits. They can also reduce
runtime optimization or increase generated artifacts. Explicit `lto = "off"`
disables local ThinLTO that the release profile's default `lto = false` may
allow. This profile is not distribution-equivalent; use `cargo build --release`
for canonical release artifacts.

On the same Windows host and exact small-edit fixture, first edited builds took
235.618s release / 26.147s iteration; replay after original-artifact preparation
took 244.347s / 27.977s. The large-module screening pair took 238.327s / 28.489s
(one sample each). These gains belong to the combined profile settings, without
isolating individual parameters or establishing runtime-performance equivalence.

No-op medians were slower: 0.886s release / 1.389s iteration. Initial population
and original-source preparations are separate from edited samples. Iteration
also adds a separate cache (3,988,730,009 logical bytes after this sequence);
cache histories differ, so this is not a normalized storage comparison. Its PDB
was larger: 21,491,712 bytes versus release's 15,069,184 bytes. Both restored
binaries passed icon-resource and bounded responsive-startup/shutdown checks.
See the detailed Track B report for the measured source SHA and limitations.

## Profile boundaries and environment

Use plain `cargo check --lib` for a quick type check, `fast-dev` for opt-in
reduced-symbol launcher/selected-test builds, `iteration` for release-like local
launcher builds, and `cargo build --release --bin multi_launcher` for production.
Default `cargo build --bin multi_launcher` and target-selected Nextest commands
retain full development debug information under `target/debug`. No additional
debugging profile is needed. Manual debugger variable/breakpoint inspection was
NOT RUN; passing assertions and source-line backtraces are separate evidence.

`iteration` inherits release's safety-check defaults, so choose dev/test when
debug assertions are needed. Each named profile has a separate artifact cache.
The fixed workflow helper intentionally leaves named-profile choices to direct
Cargo commands.

Cargo environment variables and configuration can override manifest profile
settings. `CARGO_PROFILE_*`, `CARGO_INCREMENTAL`, Rust flags and Cargo
configuration can change the effective build. Track B measurements used guarded
settings and retained actual compiler invocations; the manifest and workflow
helper do not prevent intentional user overrides. Reproduce measurements in an
equivalent environment. `lto = "off"` differs from default `false`, which can
permit local ThinLTO; no canonical release setting was changed.

## Targeted Nextest recipes

Start with `cargo check --lib` when type checking is sufficient. Then select the
affected Cargo target before filtering cases; broaden to related integration
targets when the change crosses those boundaries.

~~~powershell
cargo nextest run --lib -E 'test(history_prepare_)'
cargo nextest run --test history
cargo nextest run --test domain -E 'test(indexer_)'
cargo nextest run --test plugin_queries -E 'test(/^help_plugin::search_returns_help_action$/)'
cargo nextest run --test multi_manager_plugin
cargo nextest run --test pure -E 'test(hotkey_events::)'
cargo nextest run --test pure -E 'test(quit_hotkey::)'
~~~

The current Windows roster selects respectively 8, 5, 4, 1, 8, 4 and 1 cases.
The final two recipes select the moved hotkey and quit cases under the `pure`
target; together with its existing five cases, that target now contains ten.
These are explicit selections, not a whole-application acceptance suite. Add
`--cargo-profile fast-dev` when its reduced-symbol tradeoff suits the work.
The existing helper supports the same target boundary via `lib-test` or
`integration-test -TestTarget <name>`; filters are native argument-array values.

`--lib` and `--test` choose Cargo targets; `-E` chooses cases within the built
targets. A bare case filter can still compile unrelated executables. Integration
targets may retain normal binary prerequisites. Use unrestricted `cargo nextest
run` deliberately for broad coverage, and `cargo build --features
dev-smoke-binaries --bin coordinate_tool_smoke` when that auxiliary binary is
relevant. The five original recipes above remain unchanged. Separately, M2-B
migrated the three watcher cases from the obsolete `watchers` integration
target into the library and added one unwind guard case; no watcher behavior
was dropped.

The real Actions, Folders and Bookmarks file-watcher cases live under
`gui::watch::tests` in the library target. They save files through the existing
production save functions and wait up to three seconds for the app-owned OS
watcher event. Their fixture drops the app, waits up to five seconds for the
captured callback closures to release, then restores process paths and removes
the temporary root. The release receipt covers callback closures; it does not
claim that Windows `notify` backend threads have joined.

Each case uses its own temporary files, so this roster has no demonstrated
shared exclusive resource and needs no Nextest group. Keep default concurrency;
the fixture's existing process-local workspace lock protects its CWD and
environment changes.

~~~powershell
cargo nextest run --lib -E 'test(=gui::watch::tests::actions_watcher_sends_event) | test(=gui::watch::tests::folders_watcher_sends_event) | test(=gui::watch::tests::bookmarks_watcher_sends_event) | test(=gui::watch::tests::real_watcher_fixture_unwind_releases_callbacks_before_workspace_cleanup)' --success-output immediate
~~~
