# Track B baseline

Initial branch: `cargo-build-perf`; initial HEAD: `6a6be519685652a282f7d0a411008601f158d075` (`6a6be519 md`). Initial Git status was clean. Provenance was captured before creating `build-optimization` at that exact HEAD. Origin identifies GitHub multiplex55/Multi_Launcher; no remote operations, push or merge occurred.

**B0 frozen:** BL-01 through BL-10 completed, source restored, roster normalized and baseline PE icon verified. No production compiler/profile/build-script changes were made before this freeze.

## Verified environment and source

- Windows 10 Home 10.0.19045; stable-x86_64-pc-windows-msvc.
- Intel Core i7-7700K, 4 physical / 8 logical processors; 34,318,487,552 bytes RAM (approximately 32 GiB).
- G: NTFS, 4 TB, approximately 3.19 TB free at inventory; WDC WD40EZRZ HDD. Balanced power scheme; Defender/realtime protection enabled and unchanged.
- rustc 1.97.1, commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6; cargo 1.97.1 (`c980f4866`); nextest 0.9.135 (`610eefb88762529a316373f4a50f5fd9194c3c35`).
- Actual owned build linker: MSVC 14.44.35207, Visual Studio 2022 Community HostX64/x64/link.exe. LLVM version does not identify the linker. Bundled rust-lld exists; no linker executable was found on shell PATH. sccache unavailable.
- No inspected repo/ancestor/default Cargo-home config or relevant flags, wrappers, jobs, target-directory or profile environment overrides at initial inventory.
- Baseline has no explicit Cargo profiles. Observed release flags include opt-level=3, embed-bitcode=no, strip=debuginfo. Canonical release remains authoritative.
- Cargo metadata: 78 targets: 1 library, 4 binaries, 67 integration tests, 5 benchmarks, 1 build script. Binaries: multi_launcher, coordinate_tool_smoke, passive_overlay_smoke, radial_acceptance. Integrations: 65 standalone targets plus domain/plugin_queries aggregators (24/35 modules). Benchmarks: search, omni_search, todo_widget_filtering, radial_runtime, macros_search.
- Existing Track A runtime changes were confirmed through source/history inspection. This is preservation provenance, not newly executed runtime qualification.

## Controlled protocol

Owned scratch worktree: `target/track-b/baseline`; initial source SHA above. Raw evidence is outside that source root under ignored `target/track-b`. Serial builds reuse the existing primary `target` dependency cache; no clean, cache deletion or concurrent build. First scratch-path/profile/target population is labelled separately from warmed edits and no-ops.

Small fixture changes only the RGB formatting expression in `src/color.rs`; large fixture changes only RADIAL_DESIGNER_WINDOW_TITLE in `src/gui/mod.rs`. Exact original bytes are restored in finally and hash/clean status checked. Native validation requires original source. Same source and target paths, toolchain, command, flags and fixture bytes must be used for final comparisons.

Git autocrlf=true: active source has LF, scratch checkout has CRLF. Actual scratch original SHA256 values:

| File | SHA256 |
| --- | --- |
| src/color.rs | 407D4A9B551258C39E563E5CB48753CED7CD9ED357E2CE78109112552193EE9C |
| src/gui/mod.rs | B2379F2ED9D2A45A8ECE046F0659C626DE72004DC6B985870C4F96F7156C5975 |
| Resources/Green_MultiLauncher.ico | 73A4231F0021FD3F2B039B3B701307706ED10B721C4A4031492D6579B041C34B |

Branch trials use owned refs codex/track-b-baseline-a and codex/track-b-fixture-b (color-only local fixture commit), canonicalize captured checkout bytes before timing, and restore baseline A. The active implementation branch is unaffected.

## Measured baseline

All completed commands below exited 0. Times are seconds; preparation/restoration costs are excluded from edited medians.

| Scenario | Command/workload | Wall time |
| --- | --- | --- |
| Scratch-path transition | cargo build --release --bin multi_launcher --timings -vv | 279.014845 |
| BL-01 unchanged launcher, n=3 | same command, Fresh | 1.058698 / 1.017300 / 0.997221; median 1.017300 |
| BL-02 original to small edit, n=2 | same explicit launcher command | 249.803047 / 239.562765; median 244.682906 |
| BL-03 original to large GUI edit, n=2 | same explicit launcher command | 237.433298 / 243.186698; median 240.309998 |
| All-bin population | cargo build --release --timings -vv; library/launcher fresh | 100.259540 |
| BL-04 original to same small edit, n=2 | default all-bin release | 317.662238 / 328.656558; median 323.159398 |
| BL-05 library target preparation | cargo nextest run --lib -E 'test(history_prepare_)' --no-run | 210.186021 |
| BL-05 cached selected invocation | same without --no-run | 2.009562; runner 0.111; 8 passed, 5111 skipped |
| BL-06 history preparation / cached invocation | cargo nextest run --test history [--no-run] | 215.092202 / 2.077256; runner 0.246; 5 passed |
| BL-07 domain preparation / cached invocation | cargo nextest run --test domain [--no-run] | 20.862317 / 2.796195; runner 1.001; 102 passed |
| BL-08 broad compile/discovery | cargo nextest list --message-format json | 943.248524; 6389 declared cases; no full-suite execution |
| BL-09 check target population | cargo check --lib --timings | 76.682837 |
| BL-09 subsequent warm checks, n=2 | same command | 1.078176 / 0.918521 |
| BL-10 A→B switch, n=2 | explicit launcher release | 249.333422 / 237.573284; median243.453353 |
| BL-10 B→A switch, n=2 | same command | 241.676204 / 240.233264; median240.954734 |
| BL-11 cold build | isolated empty target | NOT RUN; warmed edit bottleneck established without deleting caches |

The all-bin edited median exceeds explicit launcher by 78.476492s. Extra targets are the intended workload difference; sequential trials and differing library durations prevent exact attribution of every second. Compiler configuration did not change. The all-bin 5m23s median most closely resembles the approximate six-minute report among completed workloads; the user's exact original command remains unknown. Small and large module edits invalidate the same library, so file size is not an independent compilation boundary.

Targeted preparations have different preceding cache states and must not be compared as grouping gains. First check population is not part of a no-op median. Final profile comparisons require matched source edits, not comparisons to population or cached execution.

## Attribution and limitations

Transition Cargo timing: library253.78s (frontend68.97/codegen184.81), launcher21.94s, build-script compilation2.03/run0.22. Small-edit sample1: library231.84s (60.20/171.64), launcher16.88s, resource0.06. Large sample1: library218.95s (60.57/158.38), launcher17.37s. All-bin sample1: library219.86s (60.15/159.71), then parallel aggregates radial_acceptance96.60s, launcher20.44s, coordinate6.15s, passive2.45s. Overlapping binary durations must not be summed. Cargo codegen/binary aggregates include linking; exact release linker duration is NOT MEASURED.

Broad discovery observed 8rustc/4link and 22,357,471,232 combined working-set bytes at one snapshot, not a measured peak. Proper ancestry includes batch→rustup→cargo→nextest→cargo→rustc/link. An initial filter omitted rustup and failed attribution; it does not establish termination or reparenting. Verified owned linkers had multi-minute lifetime lower bounds. Sparse paging/HDD samples suggest reassessing jobs/linker after reduced-symbol profiles; they do not establish sustained thrashing or causation. Correct ancestry snapshot: `target/track-b/broad-owned-resource-snapshot.json`.

The domain run generated an initially absent `clipboard_modifiers.json` in the owned scratch tree. All 23 Cargo/Nextest measurements succeeded, but the batch process exited1 at final clean-tree assertion. Parent preserved the file, hash and provenance, verified all manifests/expected scenarios and moved only that owned file to evidence, then verified clean restoration. See `batch-cleanup-verification.json` and `generated-test-data/` under target/track-b. The old dependent queue failed its prerequisite and performed no branch work; the new verified queue completed all branch trials. This generated file may explain incidental broad package invalidation under baseline build.rs, but causation was not proven by fingerprint logs.

## Evidence and harness

`dev/track_b/measure.ps1` captures exact arguments, source SHA/dirty paths, fixture hashes, scoped environment/cache history, exit status, unique stdout/stderr/manifests and changed/new Cargo timing HTML. Callers reject failed records. Self-tests passed for exit0/7, argument spacing, env/cwd restoration, unique evidence, stale timing suppression and null target override; independent review approved corrected override handling.

Raw initial evidence: baseline-populate, baseline-noop-{1,2,3}, baseline-processes.json. Subsequent immutable manifests are indexed by `target/track-b/batch-summary.jsonl`; HTML snapshots live in each run. Broad JSON: `runs/BL-08-broad-list_20261010T123737746Z_6f2b549f42be4108adcf625c6a507489/stdout.bin`. Normalized inventory:72suites (1lib/4bin/67test),6389cases,17ignored,6372matched/17unmatched,2empty suites,zero duplicate logical identities; baseline-nextest-roster-final{,-summary}.json retains stable cases independently of artifact paths. Baseline PE/icon comparison passed after branch restoration: group1, one32x32entry,4264-byte payload hash1c3aea13e6652ff273d2d995c68df5db23805476ad96d4cd751461b3b0d89d3b matches original ICO; see baseline-icon-resource.json. Native startup is NOT RUN. Full suite execution and exact peak/link telemetry are NOT MEASURED.

Branch preparations270.213662/255.819132s are excluded from measured switch medians. All four measured switches exited0; branch queue terminated0 and exact original scratch bytes/Git clean status were verified. Owned fixture commit db027294 only changes the color expression; no branch was overwritten. B0 independent review confirmed timings/roster/ownership and resolved over-attribution wording and roster no-overwrite safeguard; no outstanding findings.
