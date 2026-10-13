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

### Bounded phase diagnostic

`nextest-root-geometry-diagnostic-100-1000-fast-dev.log`: ignored diagnostic passed1/1 in33.965s after a1m06s fast-dev build,5,232 skipped. This supplements the full benchmark: counts100/1,000,2 warmups and5 samples for each cold/warm case. Source label identifies df27 production base plus dirty diagnostic render.rs SHA256 `aaa3362ae962d63248c07f44382583ab04c328d01858c1def232b271a2499835`. No production fix yet. Parent inspected the log and independently parsed all20 diagnostic records.

| Actions / case | Whole-owner p50 ms | RootGeometryCold p50 ms |
| --- | ---: | ---: |
| 100 cold | 58.8387 | 56.4948 |
| 100 warm | 2.2614 | 0 |
| 1,000 cold | 751.6604 | 749.3544 |
| 1,000 warm | 2.3268 | 0 |

Every sample had one adapter pass and25 painted rows. Cold samples had one rebuild, exactly count measured rows and phase work, no abandoned/error phase; warm had zero geometry calls/work/rebuilds. Geometry accounts for nearly all cold time, while the remainder is about2.3ms. The diagnostic distinguishes stored previous rebuild duration from zero current-frame warm geometry work. Validation and texture disposal follow timing. Next authorized step is aggregate metric/job/layout/vector stage measurement and equivalent supported job variants; no production change until attribution is reported.

### Text-stage and adapter diagnostics accepted

Final diagnostic source render.rs SHA256 `D3590ED37F65FAB4677378DD59ECA89F4662E9DCC3813F0D61C6B75514726B9A`, production base df27 unchanged. `nextest-root-text-stage-diagnostic-v2.log` passed1/1 in61.420s after1m05s build,5,234 skipped; earlier v1 remains retained. Counts100/1,000,2 warmups and3 samples for each of four variants. Parent inspected actual log completion and fresh-state isolation; independent reviewer accepted measurement validity and source without findings. Parent final fmt/diff checks passed, and raw source hash matches the receipts. All additions are test-only; existing benchmark timer remains unchanged.

At1,000 rows, medians: current WidgetText735.98ms; manually equivalent current path756.47ms, including font metrics6.07ms, job construction1.19ms, layout748.10ms and vector bookkeeping0.34ms. Display strings about0.56ms. Metrics-once/per-row access729.72ms; metrics-once/batched access726.15ms, retaining an833.18ms outlier. Three samples are diagnostic evidence, not statistical acceptance. Repeated invariant metric/context resolution is too small to explain or fix the regression. No production optimization was applied.

Each variant starts from a fresh equivalent prepared state, preventing earlier variants from warming its galley cache. Separate preflight asserts exact supported LayoutJobs and galley dimensions for Wrap/Extend/Truncate; timed results compare every cached row geometry vector and width progression. These controls preserve the current semantics rather than replacing Text with RichText.

`nextest-root-test-adapter-overhead-diagnostic-v1.log` passed1/1. Five samples,2 warmups,1,000 operations each: mock Frame construction about20ns/op, warmed renderer guard157–160ns/op, combined about176ns/op. Correct conversion is **0.000176ms per operation**, or0.176ms per1,000-operation batch. The writer's interim prose used the latter as a per-frame figure; raw labels were correct and the prose error was acknowledged. This bounds only these test infrastructure components, not all lifecycle/native-handle/finalizer work. It cannot explain the observed warm increase. `black_box` is used; no timer subtraction is applied.

Next source-backed candidate: epaint's shape_text invokes HarfRust with default ShapeOptions; locked HarfRust0.12 builds a ShapePlan when none is supplied. Existing ShaperData/buffer caching does not establish plan reuse. A bounded direct plan-construction/reuse experiment is required before any production patch. Planner compared cached assets: Ubuntu-Light/Hack/NotoEmoji bytes match old/current; emoji-icon and tweaks differ. Font swapping is unsupported by this evidence.

Primary upstream context: [egui's OpenType shaping change](https://github.com/emilk/egui/pull/8031) documents the new engine and existing data/buffer reuse; [HarfRust changelog](https://github.com/harfbuzz/harfrust/blob/main/CHANGELOG.md) documents public shape-plan caching support. [Upstream text-performance issue](https://github.com/emilk/egui/issues/8093) discusses cold rasterization during zoom; that is not proof of this fixed-scale/warm-glyph cause. Do not apply scale binning, disable hinting or change profiles based on that separate case.

### Same-host preserved-binary control

Fresh supplementary Small-mode control under `target/egui-modernization/m2a/same-host-control-001/`: original and df27 saved libtest binaries ran serially, without Cargo rebuilding either. Both native exit codes and the enclosing PowerShell exit were0. Original binary SHA256 `64004589367AD7B7D93FAB5DF95655572327788F0BC6D47D0B479BFC72F5F447`; df27 binary SHA256 `D4FFD2FE04ABD0211B6EED30DCF49DFB65FE45B1B8EFA1B3D6A8B121629E8DE1`, matching its preserved full-series binary. Each `.run.json` records command, source attribution, environment, hash and wall time. The implementer held compilation/execution throughout; parent performed a bounded read-only source/log observation during the candidate run.

Both used100 actions, fixture signature `c3745052c65797f2`,960x640,scale1000,fast-dev,telemetry enabled,five warmups,twenty samples and zero exclusions. Parent independently recomputed nearest-rank p50/p95 for all four records. Cold p50: original4.9064ms versus candidate60.1619ms; warm p50: original0.9131ms versus candidate2.1928ms. Cold p95:5.3386ms versus73.2334ms; warm p95:1.1643ms versus2.9426ms. Outliers retained. Process wall times3.6490s and17.6853s include startup/oracles and differ from sample timers.

The original remains near its earlier measured cost on this host while the candidate remains substantially slower. Broad host slowdown cannot explain the migration regression. This cheap control supplements, and does not replace, Full-mode acceptance. No production fix or performance acceptance is claimed. The standalone shape-plan probe is released after both controls finished.

### Standalone ShapePlan control accepted

Ignored probe scope: `target/egui-modernization/m2a/shape-plan-diagnostic/`. Source SHA256 `42195CB308C98FDA29FF0E601B8F1FB9D92C72FCFA04B435F718CCA95975F81E`; executable SHA256 `1C2A5B6B1B9846D5FF4BC551C3C0F925D091109A5068C9E2AD8985CB05E02F94`. Direct stable1.99 rustc builds against the existing locked HarfRust0.12 rlib with fast-dev-equivalent compiler options; root Cargo graph/registry/production source unchanged. `results/hash-receipt-001.txt` records compiler,rlib,font,input,source,executable and output hashes. Failed compile attempts001/002 remain; build003 succeeds. Actual commands and run logs are preserved separately.

Inputs100/1,000 use the actual synthetic action formulas, explicitly projected to joined ASCII primary-font text. Each row is one shaping call; this does **not** reproduce epaint fallback segmentation. Ubuntu-Light font SHA256 `80307B8DA7649AA4EE4D484B232140E3CE1EC0CA093073D3C53C8F5A5CED7A70`; direction LTR,script Latin,language None,empty features/default variation,begin/end flags. Two warmups/five samples. All strategies match every glyph ID,cluster,flag,advance and offset outside timing; zero missing glyphs. Independent reviewer accepted the limited control without findings; parent independently recomputed every summary p50/p95 in both logs.

At1,000 projected runs: implicit plans283.9267ms p50; supplied-plan shaping183.4496ms; supplied-plan lookup+shape189.3698ms; initial-plan construction+lookup+shape190.2629ms. Plan creation alone0.0865ms;1,000 key lookups0.1274ms. Realistic probe saving93.6638ms is about12.9% of the previously observed727ms layout cost, but different run segmentation prevents attributing that percentage to production. Complete arrays/outliers retained. This justifies actual epaint-run instrumentation, not a production cache patch or performance acceptance.

Receipt limitation: `results/execution-receipt-001.txt` SHA256 `3F6F9E452383B847E396002125F7A2640AF5F32DA4F828E7359B5B1C3B54ACE2` marks original build/run tool-return exit codes and wall times unavailable: the writer did not preserve those envelopes. Build003 emitted the executable and both run logs end COMPLETE/pass; those observable facts do not independently establish process exit codes. No rerun was made solely to replace missing bookkeeping. Future diagnostic invocations must persist native exits/wall times directly.

Next: planner prepares the smallest isolated actual-run probe; implementer is sole writer for the separately bounded warm-frame diagnostic in render.rs tests, with production untouched. Full series remains paused pending an evidence-backed correction.

### Warm-frame phase attribution

Test-only render.rs source SHA256 `0A73101E0A429931C81D93476053821CC35551CC3B62A788496E89B427FDA827` adds optional clocks to the shared test lifecycle adapter. Ordinary adapters pass None and construct no clocks; owner order,forced discard,callback and retained-pass finalization remain shared. Independent source review accepted without findings, pending actual scoped test results. No production change.

`target/egui-modernization/m2a/warm-owner/warm-owner-diagnostic-001.log`: Nextest1/1 PASS,2.212s; native exit0, enclosing invocation76.0031s including fast-dev build.100-row fixture,2 warmups/5 samples, fresh equivalent nine-frame-prepared state per invocation. Every sample:zero geometry rebuilds/measured rows,25 painted rows,one pass and stable geometry/structural/viewport identities. Parent inspected actual completion and independently checked all five phase sums/work counters.

Median whole owner2.1010ms; logic0.0553ms; UI1.9505ms; finalization0.0053ms; framework/adapter/timing residual0.0900ms. Existing LauncherRowsBuilt median1.4295ms (range1.4007–1.7724ms),one call/25 widgets each. **Rows timing is inclusive inside UI**, not additive. Samples retained, including2.4913ms owner maximum. Metric snapshots,validation,logging and texture disposal follow owner timing. This localizes most warm cost to visible UI/results rather than logic/finalization, but does not yet attribute that cost to a specific upstream operation.

Initial forced-discard test command accidentally retained ignored-only mode and selected zero tests; Nextest exit4 and log/receipt preserved, not counted as a pass. Corrected non-ignored exact run pending. Direct rustfmt check and parent diff check pass; source hash matches emitted attribution. Source acceptance/checkpoint remains pending corrected lifecycle evidence.

Final shared-input refinement supersedes that pending state: render.rs SHA256 `2694C4F5445D3C76AFF84CC2226ECE677B1CC6ECC2BB25647F647C7CBC14211B`. Timed/ordinary paths share RawInput setup as well as lifecycle. Final warm-owner-diagnostic-002.log PASS1/1,2.206s, native exit0,wall51.4229s including47.37s compile; forced-discard-lifecycle-003.log PASS1/1,0.100s,native exit0,wall2.1830s. execution-receipt-003.txt binds exact commands/source. Earlier forced-discard002 also passed before the refinement; all previous logs remain.

Final five-sample medians: owner2.2159ms,logic0.0554ms,UI2.0173ms,finalizer0.0080ms,residual0.1211ms,RowsBuilt1.4491ms inclusive within UI. Owner range2.1691–2.3168ms;RowsBuilt1.4308–1.4736ms. Parent independently verified all phase sums and zero-rebuild/zero-measure/25-painted/one-pass counters, source hash and actual log completions. Independent final review accepted without findings. Parent cargo fmt --all -- --check and diff checks pass. No production/all-target changes; compiled focused lib-test target and directly affected discard test suffice for this diagnostic-only step. Next accepted packet is isolated actual epaint attribution, including the existing warm cases; no production fix/performance acceptance yet.

### Actual-layout archive Stage1 stock control

Ignored archive base `05fc095082a3d41bfd95890ea57584a1db8f6acc`, under `target/egui-modernization/m2a/actual-layout-probe/`. Git tar SHA256 `DEF3DC20C8081849C9BE9C9D992725FE3AD74EA17486A3F7B384F2BFF7556F3D`; accepted warm libtest preserved before target reuse, SHA256 `BD8DFF7A2568C7A273BE086222A91A8782302E6887FD866ED147EB0216A00B5D`. Cached epaint0.36.2 copied exactly,44-file SHA manifest. Only archived manifest adds its path patch; restoring registry source/checksum fields makes archived lock equal the accepted lock after EOL normalization. Versions/dependency lists unchanged. Resolved features bytemuck/default_fonts/epaint_default_fonts; prior vendor patches retained.

First unfiltered offline metadata exited101 requesting uncached non-Windows accesskit_atspi_common0.18.1; no fetch occurred. Windows-filtered locked/offline metadata exits0. Cached epaint crate SHA256 matches the baseline lock checksum; registry lacks .cargo-checksum.json, so that absent file is not claimed as evidence. Archive patch/source/metadata commands and hashes are in receipts/.

Archived render.rs raw SHA256 `159A423BBDE151636648E37F310C28DFAF57C3394333D397076F645C258A70F9` initially failed a no-filters Git identity check. Parent independently read Git blob bytes and both files: accepted Git/live source989170 bytes,23754 LF and zero CRLF,SHA2694…; archive1012924 bytes,23754 CRLF,SHA159A…. CRLF-to-LF normalization gives exact byte equality; normal Git filtering matches accepted blob0761fd4f11f5f5361b71e430ffe6cda775d0e663. This is EOL conversion, not source drift. No normalization or rerun applied.

Uninstrumented existing geometry diagnostic passed1/1,about33.85s,native exit0,wall223.851s including build. Same100/1,000 fixtures,2 warmups/5 samples each cold/warm; all20 records retained and independently parsed by parent. Cold owner/geometry medians:100 rows57.8276/55.6211ms;1,000 rows727.6592/725.3684ms. Warm owners2.1300/2.1677ms,geometry0. All samples25 painted/one pass; cold one rebuild and exactly count measured rows/phase work; warm zero. Existing eager assertions pass. Costs reproduce the accepted diagnostic's large cold/warm regression at the same scale; no performance acceptance. Independent Stage1 review accepted without findings; final source/run/analysis receipts saved and writer released. Stderr SHA25659C1B389C546258F4EA77517A55744FC4D2546497B85C865268BDD598637980F; stdout empty. Fast-dev build3m08s; Nextest test33.85s. Ready for reviewed Stage2 packet; no instrumentation or production fix yet.

### Actual-layout archive Stage2 attribution accepted

Final source manifest SHA256 `3f404658df3be1bbda385ea2dbbd8279bc9b38cb8394d0e04ea71e29aa6c0a33`, six archive-only files: root render.rs and epaint text/{layout_diagnostics.rs,mod.rs,fonts.rs,font.rs,text_layout.rs}. Thread-local state, no observation borrows across layout operations. Source and final measurement independently accepted without findings. Run004 native exit0,wall163.601s including build,exact ignored track_c_diagnostic_root_list_actual_layout_attribution. Logs/command/result/analysis/manifest retained in actual-layout-probe/receipts/. No live source/dependency changes.

Earlier compile failure001 (GlyphFlags accessor) and runtime failure003 (warm assertion) remain. Review corrected warm measured rows to zero, warmed each selected variant path, and renamed allocation-cache miss/inclusive allocation-time fields: those do not prove raster execution. No meaningful assertions removed. Final rustfmt passes. Parent independently parsed all60 records:three modes×two counts×cold/warm×five samples, two matching warmups. Existing independent eager geometry and visible identity/work assertions pass.

At1,000 cold rows, owner medians: stock-disabled724.1362ms;stock-observed733.4621ms;explicit per-call plan743.5867ms. Observed-versus-disabled overhead about9.326ms (1.29%) in this bounded five-sample experiment. Implicit plan+shape548.9095ms; explicit plan construction333.9284ms and shape218.6790ms. Explicit per-call planning gives no owner speedup; its purpose is attribution. Segment28.2057ms;glyph emission65.0587ms. Layout-section/galley-miss timings include these nested stages and cannot be added to them.

Actual cold work4,047 face runs/103,830 glyphs,975 galley misses/55 hits;zero measured glyph-allocation cache misses,atlas recreations or evictions. Untimed parity compares all4,047 ordered runs,2,562 non-ASCII runs and complete glyph ID/cluster/flag/advance/offset records using stable actual font-name/asset identities; hash2e54d32e325e40b8 matches. Process-local face keys differ as expected and are excluded from parity identity. These untimed owner durations are not performance samples.

Warm1,000-row owner medians2.2513/2.2808/2.1806ms (disabled/observed/explicit). Thirty galley hits,no misses,no shaping,zero geometry rebuilds/measured rows. Thus plan reuse can address substantial cold cost but cannot explain/fix the warm UI regression. No cache or production patch is accepted. Next bounded decisions: matched canonical-release Small comparison to separate shipping-profile cost from opt0 development cost; then actual-owner bounded per-face plan-reuse prototype if justified. Existing fast-dev failure remains explicit and is not waived by release evidence.

Final Stage2 executable preserved as receipts/stage2-actual-layout-004-multi_launcher-2b9e6d16eb4af014.exe,138736640 bytes,SHA256 `bfa6b9a14f10fc272ef6cbb515d4080f1af777179af5693f0387aa407b693c4e`. Corrected provenance-002.txt binds matching Cargo fingerprint/dep-info within the run interval,rustc/Cargo1.99 versions/compiler hashes and all four default font asset SHA256s. Earlier malformed provenance serialization is retained and not used. Parent independently verified all six current source hashes against manifest004. Source writer released; no source changes/builds/tests for the receipt follow-up.
