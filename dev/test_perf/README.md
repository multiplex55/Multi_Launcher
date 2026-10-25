# Targeted Nextest profiling

`profile.ps1` is an opt-in helper for bounded build, inventory, and execution
receipts. `SelectedBuild` and `SelectedRun` require `-TargetKind Library` or
`-TargetKind Integration -Target <name>`. `Inventory` accepts either selection
or `-TargetKind Workspace`. `FullRun` requires `-TargetKind Workspace` and uses
Cargo's unfiltered default-feature target set. No command enables feature-gated binaries implicitly.
Native arguments are passed as arrays and Windows-quoted only at the existing
owned-process runner boundary, so paths, filters, spaces, and embedded quotes
remain intact.

The profile stores each run outside the worktree under
`$env:TEMP\Multi_Launcher\test-perf` by default. `receipt.json` is schema
version 1 and records the source SHA, a Rust/build/test-tool source fingerprint,
dirty files and dirty-source status, selected Cargo target/filter,
requested profile, default-feature choice, Cargo/rustc/Nextest versions, host,
phase wall times, process exit/timeout/cleanup, inventory and per-case results.
Raw Nextest stdout and stderr are kept beside the receipt. Deadline and process
tree cleanup are delegated to `dev/run_owned_process.ps1`; each phase retains
its owned-process identities and post-cleanup states. The helper does not
delete or modify Cargo caches. A cold or newly selected profile uses its own
Cargo cache; allow for that first population before comparing warm runs.

Use `-CacheState preparation`, `warm-noop`, or `warm-edit` to label known cache
conditions; the default is `unknown`. Phase wall times include containment
inventory/cleanup overhead and are not pure compiler or linker durations.
Per-case durations come from Nextest. Track B remains useful for build-only
samples inside an outer owned invocation, as used in this task's comparisons.

PowerShell examples from the repository root:

```powershell
# Interactive exact library case: target build/list, then native Nextest progress.
& .\dev\test_perf\run.ps1 -TargetKind Library `
  -Filter 'test(=gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames)'

# Interactive exact integration case.
& .\dev\test_perf\run.ps1 -TargetKind Integration -Target plugin_queries `
  -Filter 'test(help_plugin::search_returns_help_action)'

# Build the library test binary and save reusable Nextest binary metadata.
& .\dev\test_perf\profile.ps1 -Mode SelectedBuild -TargetKind Library -CargoProfile fast-dev

# Profile one exact library case; execution consumes the just-built metadata.
& .\dev\test_perf\profile.ps1 -Mode SelectedRun -TargetKind Library `
  -Filter 'test(=gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames)' `
  -CargoProfile fast-dev -TimeoutSeconds 300

# Select one integration target explicitly.
& .\dev\test_perf\profile.ps1 -Mode SelectedRun -TargetKind Integration -Target plugin_queries `
  -Filter 'test(help_plugin::search_returns_help_action)' -CargoProfile fast-dev

# Inventory one selected target without running its cases.
& .\dev\test_perf\profile.ps1 -Mode Inventory -TargetKind Integration -Target plugin_queries

# Inventory all workspace default-feature test cases.
& .\dev\test_perf\profile.ps1 -Mode Inventory -TargetKind Workspace

# Run the entire unfiltered workspace once with a bounded deadline.
& .\dev\test_perf\profile.ps1 -Mode FullRun -TargetKind Workspace -TimeoutSeconds 900

# Summarize the newest receipt; use -ReceiptPath to read one or more exact files.
& .\dev\test_perf\summarize.ps1
```

`SelectedRun` separates binary build metadata, selected test inventory, and
execution. `Inventory` and `FullRun` build binary metadata once, then reuse it
for listing without another build. `FullRun` has no case filter, passes
`--workspace` and `--no-fail-fast`, and is bounded by the supplied timeout. A
no-match selection is surfaced with status `NO_MATCHES` and exit code 4. Missing
Nextest or an unsupported reporter remains a failed receipt.

`run.ps1` is the interactive path: it prints the selected-target build phase,
checks a supplied filter for runnable cases, then invokes Nextest directly so
its native live progress is preserved. A no-match filter returns exit code 4;
otherwise the native Nextest exit code remains in `$LASTEXITCODE`.

The default run reporter is Nextest's experimental `libtest-json-plus` version
`0.1`. The child process alone receives
`NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1`; the caller's environment is unchanged.
Per-case events are stored as structured rows and raw JSONL. The message format
is experimental, so receipts also retain the raw output and parser errors.
Use `-MessageFormat human` when the installed Nextest lacks that reporter.

The receipt workflow writes native stdout/stderr to its external run directory
and replays it after the owned-process invocation finishes. For ordinary
interactive runs, direct target-first commands preserve Nextest's native live
terminal progress and standard exit status. Filtering cases does not limit
compilation by itself; always select a Cargo target first. A new profile
such as `fast-dev` has a separate cache that must be populated before its warm
times can be compared.

```powershell
cargo check --lib
cargo nextest run --lib -E 'test(=gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames)'
cargo nextest run --test plugin_queries -E 'test(help_plugin::search_returns_help_action)'
cargo nextest run --test domain -E 'test(indexer_)' --cargo-profile fast-dev
```

Run the synthetic process/parser checks without invoking Cargo:

```powershell
& .\dev\test_perf\test_profile.ps1
```
