# M2-A cache and CPU comparison evidence

Status: source accepted and released; performance comparison pending. Source behavior checks below do not establish performance acceptance. Checkpoints remain local.

## Scoped source verification

Sole writer owns `src/gui/render.rs` and `src/gui/notes_dialog.rs`. Parent inspected the actual logs under ignored `target/egui-modernization/m2a/`:

- `nextest-focused-cache-oracles-final.log`: 14 passed, 5,217 skipped, 9.877 seconds. Includes effective layout/cache behavior, consumer font activation, root eager list/grid comparisons, both Notes geometry oracles and retained C2-D/C2-E/C2-F recipes.
- `cargo-check-all-targets-final.log`: all-target check passed in 1m11s, with warnings retained. A later query fixture correction will require its relevant verification before source release.
- `nextest-root-query-oracle-final.log`: query oracle passed 1/1, 5,230 skipped, 0.263 seconds after a 1m01s test build. Parent inspected this log after the visibility correction.
- Independent reviewer accepted the final cache/query source snapshot without findings. Source acceptance still requires the final output-disposal correction and its scoped checks described below.
- Earlier failed runs remain retained. They are not passing evidence. Final formatting and source release remain pending.
- Initial wrapper test compilation (`nextest-frame-output-wrapper.log`) exposed an incorrect `crate::egui` path in the new helper; the established dependency path is `eframe::egui`. This is compile feedback, not a runtime test pass. The writer is correcting it; the failure log remains retained.
- Corrected `nextest-frame-output-wrapper-fixed.log`: focused pending-texture disposal test passed 1/1, 5,231 skipped, 0.045 seconds after a 2m45s build. Parent inspected the actual log. Independent reviewer accepted the wrapper and all four integrations without findings; final scoped group/check/format release remains pending.
- Parent observed a subsequent 16/16 group pass (run ID `15edae5f-ba91-4458-90f8-a46518045e3c`, 10.359 seconds, 5,216 skipped), including query and wrapper tests. Its `nextest-cache-oracles-wrapper-final.log` was then overwritten by the writer's C2-D/C2-E wrapper rerun. The observation remains recorded here, but the old raw log is no longer retained. Fresh filenames are required for further attempts; acceptance will use the completed final rerun's actual receipt.
- Final `nextest-c2d-c2e-virtualization-wrapper-final.log`: 20/20 passed, 5,212 skipped, 10.593 seconds after a 1m00s build, run ID `26fdd55a-e769-4b49-9070-71cc01a3b597`. Parent inspected the actual log. Adds retained tall-cell spill, list/grid bounded absolute-ID/click/selection behavior and offscreen context-menu ownership to the cache, eager recipe, query and disposal checks. C2-D/C2-E owner outputs now use the same wrapper; their existing timing/assertions remain intact. Parent scoped diff check passed. Final all-target/format and source release remain pending.
- Final `cargo-check-all-targets-wrapper-final.log`: all-target check passed in 1m21s, with warnings retained; parent inspected completion. Formatting receipt and source release are the remaining source-checkpoint prerequisites.
- Final source release: writer reports `cargo fmt --all -- --check` passed (`cargo-fmt-all-final-check.log`), scoped diff check passed, and no benchmark/commit was run. Source changes are limited to `render.rs`, `notes_dialog.rs` and test-only `track_c_workloads.rs`. Independent source review, parent diff inspection, final 20-case group and all-target check establish this source checkpoint. Earlier pending statements above describe intermediate states, superseded by this release.

## Benchmark envelope audit

Planner compared original production `72a01bd5`, original corrected fixture runner `f45e3773` and the current migration adapter. This was read-only source inspection, without benchmarks or builds.

`measure_owner` is byte-identical to the original corrected runner. State construction and warmup precede timing; timing starts immediately before `run_owner` and stops immediately afterward. Validation and output destruction follow elapsed-time collection.

Original list/grid setup initialized logical visibility false, but the old root rendering method still painted. Its existing acceptance assertions required positive geometry work, available width, visible IDs and eager agreement. Migrated lifecycle correctly skips hidden root UI. Explicitly setting visible state before setup frames restores the original intended visible workload outside timing; it does not add a timed show transition. Notes directly measures its dialog UI and needs no root visibility correction.

Query setup now warms the real visible root, preserves geometry/width/focus assertions, then restores logical visibility and `last_visible` to hidden before the measured owner. The existing invocation arm asserts hidden and remains inside timing. Exact query oracle and independent source review passed.

Historical receipt labels remain unchanged. Current root ownership maps from `Context::run + render_root_frame` to the test adapter's `run_ui + begin_root_frame_logic + render_root_ui + finish_ui_frame_receipt`; Notes maps to `run_ui + NotesDialog::ui`. Current root measurements also include test frame construction and immediate-renderer guard setup/restoration absent from the original adapter. Results must be described as harness-inclusive CPU ownership. Separately bound adapter overhead if needed to attribute a regression; do not subtract an assumed constant or apply an arbitrary allowance.

Final read-only output audit found that all four timed owners returned `FullOutput`, while the unchanged generic runner validates by shared reference and then drops normally. Epaint 0.36 debug-asserts on unused texture deltas; warming makes empty deltas plausible but does not guarantee them. No particular measured case had failed. The narrow `#[cfg(test)] TestFrameOutput` wrapper now retains the complete output through validation and disposes unused deltas on normal drop after elapsed-time capture. List/grid/Notes benchmark callbacks and the query owner use it; `measure_owner` remains unchanged. A real first-text-pass test checks pending deltas remain observable before disposal. This correction's final checks and review remain pending; no guard suppression or disposal inside timing.

## Pending comparison

After accepted source release/checkpoint, run four existing ignored owners serially: list 6, grid 30, Notes 42, query 3. Preserve original full fixtures, five warmups, twenty raw samples, settings, viewport and scale. Use the actual accepted source SHA and fast-dev profile; omit framebuffer QA and its request manifest. Startup's three catalog cases are outside M2-A.

Original arrays and all five owner logs were independently reparsed before this milestone; all 84 rows matched the stored original results, including nearest-rank p50/p95. Join the 81 affected comparisons by fixture/count/settings/cold type/viewport/scale, not obsolete owner label. Preserve outliers and investigate meaningful reproducible regressions. No current performance result is claimed yet.

## First measured source checkpoint — performance regression

Accepted source checkpoint: `df27da179ebc2b29e5c54801293163181ffe2920`. The full root-list owner completed and passed its independent oracles: six matched cases, five warmups and twenty samples each, zero exclusions. Nextest test wall time was 1,109.357 seconds, versus original 97.792 seconds. The fast-dev build took 3m29s; enclosing invocation wall time including build was 1,320.261 seconds. These total durations are distinct from the sample timers.

| Actions | Original cold p50 ms | Current cold p50 ms | Original warm p50 ms | Current warm p50 ms |
| --- | ---: | ---: | ---: | ---: |
| 100 | 5.4931 | 59.2506 | 0.9283 | 2.2208 |
| 1,000 | 52.5765 | 731.0243 | 0.8957 | 2.2087 |
| 10,000 | 526.4549 | 7,596.9763 | 1.0012 | 2.3015 |

This is not performance acceptance. Cold medians increased about 10.8–14.4 times and warm medians about 2.3–2.5 times. The twenty samples are consistent enough to require root-cause investigation; do not explain this away using an allowance or infer native/display latency. Grid/Notes/query comparisons remain queued while this demonstrated issue is investigated.

Parent independently parsed all six raw records, verified original fixture signatures and full protocol, recomputed nearest-rank quantiles and retained every sample. Cold work remains one rebuild and exactly the action count measured; warm work remains zero rebuild/measured rows. Actual painted rows are 25 versus original 26, and cross-version structural/viewport identities differ; independent eager geometry assertions passed. These facts cannot explain an order-of-magnitude cold timing increase by themselves.

Local artifacts: `target/egui-modernization/m2a/owner-list-df27da17.log`, SHA256 `B12D3CDB15FB406DF3464D997D8F8F15390D34316E674E4051E8E878E8BA92A8`; `.run.json` records native owner exit0 and source/profile/environment/command. The enclosing PowerShell returned1 after successful receipt creation, consistent with its final silent removal of an already absent process environment variable; future wrappers must check presence and return explicit success. This bookkeeping result does not change the independently observed Nextest pass or samples.

Preserved test binary and parsed analysis are under `target/egui-modernization/m2a/df27da17-list-analysis/`. Executable SHA256 `D4FFD2FE04ABD0211B6EED30DCF49DFB65FE45B1B8EFA1B3D6A8B121629E8DE1`; copy hash verified. No original artifact was overwritten. QA feature/request were excluded, all fixture processes were closed, and no other build/test was run concurrently. Parent made two bounded process-progress observations; read-only planner source inspection occurred during the series. Retain this context when repeating measurements.

Read-only diagnostic inspection found no extra visible-only domain workload: inert provider, worker suppression, seeded generations and visibility transition avoidance remain intact. Actual migration costs include the mock frame/native-handle path, lifecycle receipt/finalization and changed released-egui text layout. In0.27 `WidgetText::from(String)` produced RichText; in0.36 Text resolves line height through a font access before another access for layout. This is a source-backed candidate, not measured attribution. Next work is a bounded phase/text-layout diagnostic and root-cause correction with existing eager invariants retained. No styling or renderer changes are authorized by this failure.
