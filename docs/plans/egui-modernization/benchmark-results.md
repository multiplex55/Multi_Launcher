# M0-B original baseline measurements

## M2-A comparison input audit —2026-10-13

Before migrated measurements, parent independently reparsed all five original TRACK_C_RESULT logs and verified exact equality with the retained84-row cpu-original-results.json. Every row has five warmups/twenty raw samples,zero exclusions,Full mode/telemetry enabled. Stored p50/p95 values match nearest-rank quantiles recomputed from all arrays. Sourcef45e3773 is the corrected test runner; original production remains72a01bd5. No original receipt or sample was changed.

The81 M2-A cases are list6/grid30/Notes42/query3; startup3 is excluded. `(fixture,count,settings,cold_type,viewport,scale_milli)` uniquely identifies all84 original cases. Join future current cases by these fields, check fixture signatures/settings/work/oracles explicitly, and map changed owner labels to actual run_ui/lifecycle paths rather than joining on obsolete API names. Preserve every sample/outlier. Current measurements have not run yet; source writer owns cache corrections and parent owns serial matched comparisons after source acceptance.

Verified input SHA256:

- cpu-original-results.json:4BFB209811B0FF4A72B59AB11A297DA6F560AF00667C64DB924110000E89AAFC
- owner-list.log:25304122FDE2C7FB4026438FF8C7FDC07DE03824289749D2CA0B004DD6286A43
- track_c_benchmark_root_grid_frame_geometry.log:8AA6EE32EF9FABFE62C86C204D7D1A266CA2FBA9F8EFF9E9B208EC31616551F8
- track_c_benchmark_quick_notes_dialog_geometry.log:5BB83EF80401F861553C9604F00C2B435DC57AE47CCBC13308F3061E09C856DC
- track_c_benchmark_root_query_to_usable_cpu_frame.log:A45DC1CA437C79F32CC737122E4B4419D9E33FCDF922661751084B1677C04E95
- track_c_benchmark_startup_action_catalog_owner.log:561EBD3310265FDE0861F9C7731784938165477A32C0C1F7799253233A278EF6

Report p50/p95 deltas,absolute costs,raw spread and sample counts. Any resampling confidence interval describes only noise within these arrays; it cannot establish between-run variance or hardware/display latency. Repeat comparable cases when an apparent slowdown needs classification; do not accept an arbitrary percentage allowance or infer no regression from overlapping intervals alone. QA capture feature/request must be absent from performance runs.

Preparation host recheck2026-10-13: scoped read-only CIM queries report Intel Core i7-7700K4cores/8logical processors,max clock4200MHz;Windows10Home10.0.19045;NVIDIA GTX1080 driver32.0.15.6094,controller-reported2560x1440@165Hz;Parsec Virtual Display Adapter0.45.0.0 reports no current resolution/refresh. powercfg reports Balanced. CPU/OS/driver/power fields match original recorded context. Initial sandbox CIM calls returned Access denied; scoped read retry succeeded. Controller properties do not identify the app's actual GL adapter,monitor topology or OS scaling. Native QA receipt PPP1/zoom1 remains separate measured app-renderer metadata. This is preparation context,not a benchmark result or GPU/display-timing measurement; record actual run attribution again with measured owners.

Status: **PARTIAL.** All five original CPU owners passed (84 cases, five warmups/twenty measured samples per case); native visual/GPU/display latency acceptance remains unproven. Original profile build timings are in progress.

## Provenance and run constraints

- Application code baseline: `72a01bd5451060d3f6b640b83acd081d7db76e77`.
- Fixture manifest build HEAD: `050a922faab0c65ec765302c1c2b81843561c069`. The later M0-A corrections touch test-only code in `src/gui/watch.rs` and `src/plugin.rs`; production source, manifest and lockfile remain unchanged from the code baseline.
- Track C runner: test-only correction HEAD prefix `f45e3773`, with `ML_TRACK_C_SOURCE_SHA=f45e3773`; the production source baseline remains `72a01bd5451060d3f6b640b83acd081d7db76e77`. The root-list run used `--success-output final`; these test-only edits do not change application behavior.
- Host/toolchain: Windows MSVC, Rust/Cargo 1.99.0 direct stable binaries; record CPU model, power mode, Windows build, display/GPU/driver, scale and monitor for each actual run.
- Separate debug/fast-dev/release build-time receipts were not collected. If collected later, run these sequentially with the direct stable toolchain binaries from M0-A after other Cargo work stops:

  ```powershell
  Measure-Command { cargo build --locked --offline --bin multi_launcher }
  Measure-Command { cargo build --locked --offline --profile fast-dev --bin multi_launcher }
  Measure-Command { cargo build --locked --offline --release --bin multi_launcher }
  ```

  Record each exit code, wall time, actual Git HEAD and the matching executable hash (`target/debug/multi_launcher.exe`, `target/fast-dev/multi_launcher.exe`, `target/release/multi_launcher.exe`). These are build-profile measurements from the present cache state; do not run `cargo clean` or delete artifacts to manufacture a clean-build result. The two launches recorded below used the current-source `target/fast-dev/multi_launcher.exe` app target produced during Nextest compilation, not a Nextest test harness. Its exact SHA is recorded, but no dedicated `cargo build --bin` receipt exists. Do not launch an unverified test harness or any Oct 10 artifact.
- The original no-fail-fast diagnostics completed with three stale fixture failures. The focused baseline fixture correction subsequently passed its four targeted tests; the original full-suite log remains unchanged. The root-list Track C owner has now completed successfully; its raw output is preserved in `target/egui-modernization/baseline/owner-list.log`. The grid owner is running while notes, query and startup are queued, so no cross-owner conclusion is recorded here.
- Fixture: the parent compiled and ran `prepare_native_fixture.rs` using `build_fixture_helper.ps1`. Serializer/load round-trips and manifest generation completed for two unique directories, `run-original-001` and `run-original-002`, each with 1,000 actions and 100 notes. Both manifests record action SHA-256 `fd19a0d9e47ecc6ff10908381a6b92a2f3b9701066e5f592d41df3811dbca4c2`; settings SHA-256 differs because the first run used F12 and the second uses Ctrl+Alt+F10. Use a new run directory for future repeats; preserve both manifests and logs.

## Native launch receipts

| Run | Start / process | Executable SHA-256 | Fixture signature / counts | Observation |
|---|---|---|---|---|
| `run-original-001` | 2026-10-12 17:19:07.3223533 -04:00; PID 30408; sandboxed | `502d5da6422c80161c2d2b24df8856a9be14424f7234cf1677b3b79830b202cb` | 1,000 actions / 100 notes; settings SHA-256 `8bd4ae9a86baeb749931c36f399957074a2b7998f88a280864e31e5def145b7e` | F12 registration failed and desktop COM restore returned access denied (`0x80070005`). Parent gracefully terminated the exact PID. No screenshot or valid sample; do not classify this fixture/environment failure as a production regression. |
| `run-original-002` | 2026-10-12 17:22:44.8982728 -04:00; PID 22572; unsandboxed | Same executable SHA as run 001 | 1,000 actions / 100 notes; settings SHA-256 `9bc50953ec3613655737298c34d864579b01063884b240f512cbf7d599f22dcc` | App log reports Ctrl+Alt+F10 registered and the UI tool returned the root window. Capture approval timed out; after refreshing selection, the FrameArrived retry timed out waiting on its channel. PID 22572 was later gracefully terminated after validating its identity. No screenshot, input interaction, visual pass or measured sample. |

Both manifests, process receipts and app logs are under ignored `target/egui-modernization/native-baseline/<run-id>/`. The app logs contain internal `startup.first_usable_frame` instrumentation, but no validated displayed frame or repeated sample exists; those lines are not accepted as visual-frame or latency measurements.

## Track C CPU owners

Run each owner in its own serial Nextest invocation, with process-scoped env set before `cargo nextest` and restored in PowerShell `finally`:

```powershell
$env:MULTI_LAUNCHER_PERF = '1'
$env:ML_SKIP_CLIPBOARD_SYNC = '1'
$env:ML_TRACK_C_BENCH_MODE = 'full'
  $env:ML_TRACK_C_SOURCE_SHA = 'f45e3773'
$env:ML_TRACK_C_PROFILE = 'fast-dev'
cargo nextest run --success-output final --locked --offline --profile default --cargo-profile fast-dev --lib --run-ignored ignored-only -E 'test(track_c_benchmark_root_list_frame_geometry)'
```

Repeat the serial command, changing only the exact owner filter for `track_c_benchmark_root_grid_frame_geometry`, `track_c_benchmark_quick_notes_dialog_geometry`, and `track_c_benchmark_root_query_to_usable_cpu_frame`. Run startup separately with `--bin multi_launcher` and filter `test(track_c_benchmark_startup_action_catalog_owner)`. Preserve all per-sample arrays and oracles; do not downsize full mode. These headless owners measure CPU/layout work and cannot establish actual pixels, GPU composition, native hotkey-to-visible latency, focus or display input-to-frame latency.

Full Track C fixture sizes are 100/1,000/10,000 actions and 100/1,000/5,000 notes, with five warmups and twenty measured samples per owner. The fixture source also defines 20,000 combined actions and index-tree sizes 16/1,000/10,000 where the corresponding owner uses them. Report only counts actually exercised. A failing oracle invalidates that owner's timing; keep the failure log and mark its measurement `FAIL`, not as a performance sample.

## Native measurements and capture plan

Use an app binary whose source/build identity and executable hash are verified against the isolated fixture manifest in `visual-qa.md`. The two recorded attempts used the current-source `target/fast-dev/multi_launcher.exe`; no separate app-build receipt was collected. The following are separate data points, not substitutes for one another:

| Metric | Method | Status / result |
|---|---|---|
| Hidden idle CPU and repaint/paint activity | Repeated fixed observation window; record process CPU and available app/frame counters | `NOT MEASURED` |
| Visible idle CPU and repaint/paint activity | Same window, no pointer/key activity, root visible | `NOT MEASURED` |
| Hotkey-to-visible and focus readiness | Invoke collision-checked `Ctrl+Alt+F10`; observe actual window and usable input | `NOT MEASURED`; run 002 only logged registration, with no invocation or focus observation |
| First CPU-usable root frame | Existing Track C query/frame receipts | `NOT RUN` |
| First actually displayed/usable frame | Visible Windows observation; not inferred from CPU receipt | `NOT MEASURED`; capture and FrameArrived wait timed out in run 002 |
| Query update/input response | Fixed synthetic queries; preserve query and result identity | `NOT MEASURED` |
| List/grid scroll | Same fixture/count, viewport and scale; separate small and large results | `NOT MEASURED` |
| Notes reflow | Generated notes only, identical width/font/scale; cold and warm paths distinct | `NOT MEASURED` |
| Root and notes geometry cold/warm | Track C owner sample arrays and oracle receipts | `PARTIAL`; root-list owner passed, grid is running, and notes/query/startup are queued |
| Memory/GPU footprint | Capture available process/renderer counters with tool and sampling interval identified | `NOT MEASURED` |
| Debug/release build time | Separate builds after all other Cargo work; command/profile/host and elapsed wall time | `NOT MEASURED` |

For each native series, record warmup and sample count, every retained sample, start/end timestamps, host load/power profile, renderer/backend and actual profile. Use five warmups/twenty samples where practical; for slow launches record the actual smaller count and do not report a p95 from one sample. Preserve outliers and explain exclusions. Screenshots are visual evidence only.

## Result ledger

| Run ID | Owner/scenario | Source/build HEAD | Binary SHA-256 | Fixture signature/counts | Warmups / samples | Result / evidence |
|---|---|---|---|---|---|---|
| `owner-list.log` | Track C root-list geometry | Test-only HEAD prefix / source env `f45e3773`; `fast-dev` | Test binary receipt under `target/egui-modernization/baseline/bin` | Root-list fixtures, 100/1,000/10,000 actions | 5 warmups / 20 samples per case | `PASS`; six cold/warm cases completed; raw samples retained, no aggregate comparison recorded |
| — | Track C root grid / notes / query / startup | — | — | — | — | Grid `IN PROGRESS`; notes, query and startup `QUEUED` in parent-owned serial run |
| `run-original-001` | Native startup/hotkey/window restore attempt | `050a922faab0c65ec765302c1c2b81843561c069` | `502d5da6422c80161c2d2b24df8856a9be14424f7234cf1677b3b79830b202cb` | 1,000 actions / 100 notes; settings SHA above | — | `NOT MEASURED`; F12 registration and sandbox COM restore failed before visual observation |
| `run-original-002` | Native root discovery/capture attempt | `050a922faab0c65ec765302c1c2b81843561c069` | Same executable SHA; settings SHA above | 1,000 actions / 100 notes | — | `NOT MEASURED`; root window returned, capture and FrameArrived wait timed out |
| — | Hidden/visible idle, input, scroll, notes, memory/GPU | — | — | — | — | `NOT MEASURED`; no valid observation series |
| — | Debug/release build time | — | — | — | — | `NOT RUN`; no M0-B build timing recorded |

Store private raw logs and screenshots under ignored `target/egui-modernization/native-baseline/<run-id>/`. Do not put absolute user paths, dashboard contents, personal configuration, or unsanitized screenshots in the tracked result ledger. No historical Track C numbers are copied into this original-source baseline.

## Captured root-list CPU baseline

Full root-list owner passed (97.792 s; six cases). Production source is 72a01bd5; test runner includes reviewed test-only correction f45e3773. Each case retains five warmups and twenty measured samples, with no exclusions. Raw arrays and oracle receipts: ignored target/egui-modernization/baseline/owner-list.log.

| Rows | Cold p50 / p95 ms | Warm p50 / p95 ms |
|---|---|---|
| 100 | 5.4931 / 7.6679 | 0.9283 / 1.3954 |
| 1,000 | 52.5765 / 60.8025 | 0.8957 / 1.3429 |
| 10,000 | 526.4549 / 790.9059 | 1.0012 / 1.6860 |

These unoptimized fast-dev CPU values are an initial baseline, not shipping/GPU/display latency. Host: i7-7700K (4 cores/8 threads), Windows 10 Home build19045, GTX1080 driver32.0.15.6094, Balanced power. Full host receipt is local target/egui-modernization/baseline/host.json. Metadata queries and binary preservation occurred during the serial owner series; retain outliers and repeat matched comparisons before performance acceptance.

## Captured grid CPU baseline

Full grid owner passed in 556.405 s: 30 cases (100/1,000/10,000 actions, columns1/2/3/5/6, cold/warm), each with five warmups and twenty samples. Warm p50 values range 0.73–2.09 ms in fast-dev. Raw arrays, independent oracles and exact per-case metrics are retained in target/egui-modernization/baseline/track_c_benchmark_root_grid_frame_geometry.log; parsed local summary is grid-summary.json beside it. Notes is running; query and startup remain queued. No native or cross-version parity claim follows from these CPU passes.

## Completed original CPU owner series

All five full-mode owners passed, with production source72a01bd5 and test-only runner/source prefix f45e3773. The table supersedes earlier running/queued preparation entries. No samples were excluded; all independent owner assertions passed.

| Owner | Cases | Test wall time s | Evidence log under local baseline directory |
|---|---|---|---|
| Root list | 6 | 97.792 | owner-list.log |
| Root grid | 30 | 556.405 | track_c_benchmark_root_grid_frame_geometry.log |
| Notes (including width transitions) | 42 | 896.719 | track_c_benchmark_quick_notes_dialog_geometry.log |
| Query to usable CPU frame | 3 | 22.972 | track_c_benchmark_root_query_to_usable_cpu_frame.log |
| Startup catalog/index owner | 3 | 41.563 | track_c_benchmark_startup_action_catalog_owner.log |

[Sanitized per-case summary](receipts/cpu-original-summary.json) retains fixture identity, profile, sizes, warmups/sample counts and quantiles. Full arrays, signatures, structural/revision/work receipts remain in ignored target/egui-modernization/baseline logs and cpu-original-results.json. Startup uses16/1,000/10,000 synthetic index entries; its CPU medians are1.739/95.360/926.490 ms. Query uses100/1,000/10,000 actions; medians0.608/1.110/5.614 ms. Startup CPU/catalog timing is not process-to-display latency.

## Original profile build receipts

All three serial builds passed with the existing cache state; no clean was run. These are single observed build times, not clean-build benchmarks or statistical comparisons. The series began at documentation HEAD `de6bcd22`; the later CPU-receipt commit changed documentation only. Source, manifest, lockfile and build script remained byte-identical to `f45e3773` throughout.

| Profile | Wall seconds | Executable SHA-256 |
|---|---:|---|
| fast-dev | 124.450 | 49873783D3BB1C6A57488D432A3EFAAD0088B310936EA2E9B2F55F638CEEFB35 |
| dev | 261.312 | 1C305F28494C496147E3BA55378B39F26AFEA378FC7803FCEEFBE0D1F2D5FA0A |
| release | 419.488 | 4266A55233B5B94EB4C2276559EB16CF954306EB0B2FBA49C78E0F51507F2715 |

[Exact commands and receipts](receipts/original-build-times.json). Original executables and startup/library test binaries are preserved locally under ignored `target/egui-modernization/baseline/bin/`. No native latency or GPU result follows from successful builds.

## Current canonical-release Full ROOT control

Matched originalf45e3773 against production3a10f1de using the same direct libtest runner, canonical release and existing Full owner. Both one-shot invocations passed correctness; each retained six cases/five warmups/twenty samples, with zero exclusions. Performance acceptance remains **FAIL**.

| Rows | Original cold p50 ms | Current cold p50 ms | Original warm p50 ms | Current warm p50 ms |
| --- | ---: | ---: | ---: | ---: |
| 100 | 1.3030 | 1.9867 | 0.1636 | 0.2740 |
| 1,000 | 11.5831 | 21.0992 | 0.1745 | 0.2989 |
| 10,000 | 126.1726 | 235.4586 | 0.2057 | 0.3284 |

[Full results, p95/spread, artifact bindings and limitations](receipts/m2a-release-full-root-control.md). All240 measurements and aligned arrays remain available. Original26/current25 painted rows, owner-boundary labels and emoji-icon font differences are disclosed; no cross-version output identity was forced. These CPU-owner results do not establish native/display/GPU or Full81 acceptance. Historical baseline results above remain unchanged.


## Provisional paragraph-capacity production comparison

Fresh matched originalf45/currentfcd4 canonical-release Full ROOT pair: both one-shot invocations passed correctness; all12records/240values and aligned work/identity vectors retained, zero exclusions. Performance remains **FAIL**.

| Rows | Original cold p50 ms | Current cold p50 ms | Original warm p50 ms | Current warm p50 ms |
| --- | ---: | ---: | ---: | ---: |
| 100 | 1.2654 | 1.8155 | 0.1545 | 0.2627 |
| 1,000 | 11.6476 | 19.6725 | 0.1775 | 0.2984 |
| 10,000 | 126.1575 | 212.1170 | 0.2122 | 0.3310 |

[Actual p95/ranges, invocation/artifact bindings and limitations](receipts/m2a-production-paragraph-capacity.md). The10,000cold ranges are disjoint. Original26/current25 painted rows and version-specific owner boundaries remain disclosed. Historical3a10 differences are unpaired and do not quantify a causal reserve benefit. These CPU results do not waive native/display/GPU, fast-dev, remaining owners or Track B prerequisites.

## M2-A exact-measurement candidate control experiment (not Full qualification)

Accepted source/private11fd6663, releaseartifact405a5cb4 and completeTraceparity0aded75e precede exactly one fresh baseline48B2/5D87 then candidateF62C/EBBB observer-control body. Bothnative0;1000cold/nineprep/two warmups+five samples perstate, all28records/N55 retained. Disabled owner p50/p95ms25.3959/34.2486→16.4733/19.6152, ranges24.4816–34.2486 vs15.2652–19.6152 disjoint; every corresponding candidate sample lower. Enabled supporting p50/p95ms31.6263/33.5289→16.3410/25.7514. Full ordered samples, bindings, record checks and limitations are in `receipts/m2a-size-only-layout.md`.

This is descriptive single fixed-arm-order diagnostic evidence, not a causalpercentage, old0.27comparison, production adoption or performance gate waiver. Actual production Full ROOT remains FAIL. Evidence supports separately reviewed live port and then actual original/current Full qualification; no theme rollout yet.


## Actual original/current Full ROOT pair — performance FAIL

Exactly one original f45e377/990081 invocation then one actual production79653eca/F26D invocation completed native0, wall28.903s/38.175s. Both correctness assertions pass. Full owner and recipe unchanged:6cases,5warmups/20samples each, all12records/240values and7aligned20-value vectors, zeroexclusions. Parent independently audited actual12rawrecords/fields/quantiles/logbindings/sourceprofile/work/within-versionidentities. Independent result review pending.

| Case | Original p50 / p95 ms | Current p50 / p95 ms | Original range ms | Current range ms |
| --- | ---: | ---: | ---: | ---: |
| 100 cold | 1.3181 / 1.6105 | 1.2828 / 1.7002 | 1.1506–2.1009 | 1.1970–1.8628 |
| 100 warm | 0.1567 / 0.2203 | 0.2694 / 0.3546 | 0.1018–0.2611 | 0.2606–0.3546 |
| 1,000 cold | 11.2742 / 16.5789 | 15.8411 / 17.7901 | 10.5613–16.7666 | 15.4875–27.1029 |
| 1,000 warm | 0.1832 / 0.2370 | 0.2847 / 0.3849 | 0.1590–0.2748 | 0.2611–0.6334 |
| 10,000 cold | 127.5695 / 149.0629 | 166.2805 / 179.1976 | 118.1821–179.8763 | 158.4539–182.6321 |
| 10,000 warm | 0.2003 / 0.2771 | 0.3171 / 0.3891 | 0.1916–0.3553 | 0.3048–0.4530 |

Current is slower in5/6medians;10kcold+30.345% and10kwarm+58.313% descriptive in this pair. Full10kcoldranges overlap; do not claim disjoint ranges or trim current1kcold27.1029ms/any other sample. Fixed original-first armorder, nonexclusive local Get-Process compiler snapshot/CIMaccessdenied, original26/current25paint and distinct oldContext.run/currentlogic-ui-finalizer owner boundaries remain explicit. No cross-version outputidentityequality required. Priorparagraph-only/newsource differences are unpaired and cannot establish causalportbenefit. M2-A remains FAIL; no TrackB/performancewaiver/automaticoptimization or repeats.

Artifacts: matchedindex5e41fec2f480d5282ccd66b9d4511402ebec72e830690430c7df590a688c8f9b; originalparsedf642e79e8213a0397cd64498043beead235706b9926c97b67369db8c8247814c; currentparsedaeffbb924bcc09408afcd46c58f875ad0db2339858a1cbaa5991a2d47c0833e0. Directrunnerbcf3c0733e7780e7f43d1afd182ec873c18200c9407942d1fd7d2724a73d9dfe. Rawlogs/preflight/native/results preserved under release-full-root-size-only-001/body-runs. No source/build/body repeats.

Independent actual-result review accepted the Full ROOT measurement evidence without integrity findings and retained performance FAIL. No repeat/exclusion/automatic optimization or Track B acceptance. Source79653eca remains provisional.


Current production warm-owner diagnostic (onebody,2warmups/5samples) accepted: ownerp50/p95us265.2/368.6,UI231.8/317.5,logic12.2/17.1,finalizer0.9/1.8,residual19.7/32.2. Nestedrowmetric109.5/132.3 is included inUI; do notaddmedians. SameFull100fixture/0rebuild0measure25paint1diagnosticpass. No originalphasecomparison/overhead/productionparity claimed; FullperformanceFAIL remains. See receipts/m2a-production-warm-owner.md for allorderedvalues/bindings/limits.
