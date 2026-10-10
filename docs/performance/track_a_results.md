# Track A runtime results

Status: implementation in progress. M1-A/B/C and M2-A/B/C are verified; M3-A launcher list virtualization is in progress. The full note comparison below establishes the idle fast path and retains the slower large-draft result.

See `track_a_baseline.md` for the authoritative source and host, and `track_a_checkpoints.md` for checkpoint state, test evidence, review, commit and push outcomes.

## Reporting rules

- Compare the same synthetic fixture, profile, host, warmup and sample protocol.
- Separate production-operation timings from fixture setup, count evidence from wall time, and API submissions from GPU work.
- PASS means the stated command/check actually succeeded. Native checks not performed are NOT RUN.
- Optional M6 work requires measurement evidence; absent evidence does not justify speculative caching.
- Preserve all behavior/data formats and exclude Track B.

## Checkpoint evidence

M0-A/M0-B verified; M0-C baseline captured before optimization.

### M0-A — instrumentation gate

PASS: `cargo nextest run --lib -E 'test(performance::tests)'` on final source: 3 passed, 5,028 skipped. A preceding pass was repeated because final source edits overlapped its compilation. Existing render-test warnings remain. PASS: changed-file `rustfmt --check --edition 2024` and `git diff --check`.

Review: planner owner mapping and parent actual diff inspection; removed unnecessary telemetry forwarding APIs, corrected actual widget construction counts, and made failed index scan outcomes terminal. Fixed atomic storage and opt-in probes cover the required families; definitions are in `track_a_metrics.md`.

No runtime optimization or measured speedup in this checkpoint. Native startup/idle interaction smoke: NOT RUN (headless verification only). Native per-operation counters compile on this Windows host; compilation is not a native behavior pass.

M0-A commit: `193b300c8fd2704e565a38e0928122201c7ada1b`. Ordinary `git push -u origin performance-optimization` FAILED: credential helper authentication prompt was canceled and Git could not read a GitHub username in the noninteractive execution context. No remote success is claimed. Read-only remote access had succeeded earlier. This attempt predates the local-only instruction below.

User steering: checkpoint commits are LOCAL ONLY from this point forward. Do not retry pushes or create an upstream. The user explicitly replaced the original push cadence on October 9, 2026. Preserve the recorded first failed attempt as historical evidence.

### M0-B — deterministic workload gate

PASS: `cargo nextest run --lib -E 'test(track_a_fixture)'` (1 test). Small-mode, serial opt-in owner harnesses passed for notes, history, root list/grid, Quick Notes, changed/unchanged Actions reload and actual index traversal. The five passing owners were retained when the isolated Quick Notes setup correction was verified with its exact test filter. PASS: unprofiled `gui::note_panel::tests::backlink_rows_ignore_fenced_code_links` (1 test), changed-file rustfmt and `git diff --check`.

Independent review resolved unrelated native hotkey startup, dashboard background-work overlap and missing actual rendered-row identity receipts. Test-only inert plugin construction, explicit disabled plugins/hotkeys and startup completion barriers isolate the workload. Disabled gesture state retains an idle watcher on its absolute temporary target; the fixture never creates or changes that target, and each owner runs in its own Nextest process. No native behavior pass is claimed.

Fixture smoke corrections preserved production pin identity semantics and installed Quick Notes synthetic cache after app initialization. Metrics distinguish iterator-next calls from completed scans. Small reload smoke observed 20 logical scans/40 next calls for 16 indexed files on changed payloads, and zero scans on unchanged payloads. These are functional smoke counts, not the frozen full-size baseline or a speedup claim. Full serial measurements follow in M0-C.

M0-B local commit: `d003b4a35ef3fafd75438493199cb20463378baf`. No push attempted, per user instruction.

### M0-C — frozen G0 baseline

PASS: full serial opt-in workload command (6 owner tests, 5,032 skipped, 221.702 seconds), producing 39 bounded summaries. Repeat small root owner PASS (1 test, 5,037 skipped, 1.017 seconds); fixture/rendered identity signatures matched exactly. List p95 varied from 1.954 ms to 3.256 ms despite nearby p50 values; retain this visible tail noise rather than selecting a cleaner repeat.

The baseline source is M0-B `d003b4a35ef3fafd75438493199cb20463378baf`. Exact timings/counters/signatures and limitations are frozen in `track_a_baseline.md`, `track_a_baseline.json` and `track_a_baseline_repeat.json`. Native display/CPU/GDI measurements remain NOT MEASURED; native interaction is NOT RUN. No runtime optimization or speedup is claimed yet.

Selected pre-change p95 values: unchanged 5,000-note check 31.861 ms; 10,000-history mixed count 8 preparation 39.308 ms; 10,000-result list/grid 157.140/142.395 ms; 5,000 Quick Notes empty filter 2,034.023 ms; changed custom/indexed 10,000 Actions reload 1,006.727 ms. These debug-test CPU/event values are comparison inputs, not release-user latency.

M0-C local commit: `e9695fe4edda98eb12f0e5ca4f8f5b12465e6bcc`. No push attempted.

### M1-A — note publication contract

Published notes and revision now share the cache lock; the new fallible versioned snapshot returns a consistent pair. Reload holds the existing mutation transaction across disk load/publication, preventing an older reload from overwriting a newer save. Read-side slug scratch mutations were removed; real slug-generating writers retain their reseeding. Post-commit cache errors identify the already committed disk state. The historical snapshot wrapper remains compatible.

PASS: 16 focused note unit tests covering exact revision changes/no-ops, failed writes/rollback, aliases/title/body/tags/backlinks, append/delete and deterministic reload/save + reader/writer races. PASS: 4 selected notes_plugin alias/delete/backlink/wrap-links integration tests. Scoped filters are recorded in the implementation handoff notes. Independent review found one test guard cleanup defect after an equal initial install; fixed and protected by a new regression. No other substantive findings.

This correctness checkpoint retains the existing heavy refresh algorithm. Timing comparison follows M1-B/M1-C; no speedup is claimed here.

M1-A local commit: `a47461ab7f6f312858ae98a62fe76cf360061645`.

### M1-B — cheap note refresh gate

Heavy refresh now checks persisted note/todo revisions, the applied backlinks setting and pending local work before taking a note snapshot. Local edits retain the 250ms debounce; persisted/settings changes remain immediate. Removed the redundant full-cache alias hash and panel-body hash from this check while preserving the separate markdown-analysis content hash. Link-menu projections capture their lightweight targets and revision together.

Snapshot failure retains last-good rows/applied keys and a bounded retry record. Review identified force-only work lost after failure and a cooldown test that manually bypassed expiry; pending retry now keeps the work actionable and the test expires its actual deadline. A further review finding showed an old retry could override a newer draft's debounce; new content invalidates that obsolete retry and retains ordinary dirty work. Late save/overwrite failures schedule repaint, including immediate repaint for already expired retries. Independent findings are resolved; parent inspected the final zero-delay correction.

PASS: 21 focused NotePanel library tests with performance counters enabled, plus the small ignored note owner workload (1 test). Over 20 measured idle checks, NoteSnapshot, NoteAliasHash and NoteHeavyRecompute calls were all zero. The draft scenario retained 20 snapshots/recomputes after debounce. PASS: changed-file rustfmt and git diff checks. Three existing render test warnings remain. Full-size comparison is reserved for M1-C; no timing improvement is claimed from this small smoke. Native checks remain NOT RUN.

Final targeted Nextest run `7b606620-c99f-4e0f-9445-48fe57d94b87`: 21 passed, 5,031 skipped, 0.350s. Small owner run `5fcbe3d9-dbe7-46a2-bdf0-9fcdac410327`: 1 passed, 5,051 skipped, 0.902s. Small idle fixture signature `40142a5ec2bdddbe`; its timing is smoke evidence only. Exact selection: `cargo nextest run --lib -E 'test(backlink_) | test(link_menu_) | test(empty_link_menu_targets_are_cached) | test(unchanged_heavy_check_skips_note_snapshot_alias_hash_and_recompute) | test(pending_external_mutation_without_edit_time) | test(todo_revision_change_bypasses_local_edit_debounce) | test(persisted_note_revision_bypasses_local_edit_debounce) | test(changed_note_revision_and_setting_bypass_retry_deadline) | test(failed_heavy_snapshot_retains_rows) | test(new_draft_after_failed_refresh_uses_normal_edit_debounce) | test(edits_do_not_trigger_heavy_recompute_every_frame) | test(programmatic_content_replacement) | test(rendered_checkbox_toggle) | test(save_recomputes_derived_and_updates_links) | test(save_invalidates_backlink_rows_when_slug_changes)'`, with `MULTI_LAUNCHER_PERF=1`.

M1-B local commit: `9793d385a6165a6032165398c85b1d0b46b4c737`. No push attempted.

### M1-C — notes comparison and semantic gate

Full serial note owner PASS on exact M1-B source `9793d385a6165a6032165398c85b1d0b46b4c737`: 1 test, 5,051 skipped, 54.08s test duration. Six summaries, each 5 warmups/20 samples, match the frozen baseline's fixture and derived-output signatures exactly. Exact nanoseconds and bounded counters are retained in `track_a_notes_g1.json`; raw local log is `target/performance/track-a-m1c-notes.log`. Command: `MULTI_LAUNCHER_PERF=1`, `ML_TRACK_A_BENCH_MODE=full`, `cargo nextest run --lib --test-threads 1 --run-ignored ignored-only -E 'test(track_a_benchmark_note_refresh_check_owner)' --success-output immediate-final --no-output-indent`. Identical immediate/final summary lines were deduplicated.

| Notes / operation | Baseline p50 / p95 (ms) | G1 p50 / p95 (ms) |
| --- | --- | --- |
| 100 idle check | 0.7434 / 0.7986 | 0.0004 / 0.0005 |
| 1,000 idle check | 5.5586 / 6.5147 | 0.0006 / 0.0007 |
| 5,000 idle check | 29.2544 / 31.8605 | 0.0011 / 0.0012 |
| 100 draft after debounce | 29.0021 / 41.8829 | 28.3448 / 30.7176 |
| 1,000 draft after debounce | 331.5214 / 368.7641 | 310.1108 / 450.7394 |
| 5,000 draft after debounce | 1,512.3676 / 1,529.3767 | 1,635.8284 / 2,085.8004 |

Every idle scenario has zero snapshot calls/estimated clone bytes, zero alias hashes and zero heavy recomputes. At 5,000 notes this removes 20 full snapshots/572,131,520 estimated bytes, 100,000 hashed alias pairs and 401,743,700ns aggregate snapshot lock-held work across the measured checks. Lock acquisition wait also becomes zero. Timer resolution and enabled telemetry overhead matter at sub-microsecond scales; these are headless debug-test CPU measurements, not GPU/input latency.

Draft checks still intentionally rebuild derived data. All sizes retain 20 meaningful rebuilds; snapshots fall from 40 to 20 and alias hashes from 40 to 0. The 5,000-note draft estimated clone bytes fall from 1,144,263,040 to 572,131,520, with snapshot lock-held time 677,121,400ns→416,953,100ns. However medium/large draft p95 worsened in this capture; the first result is retained without selecting favorable reruns. Large heavy-recompute aggregate time rose 29.240s→32.730s while its call count/output stayed identical. There is no blanket draft speedup claim or fabricated attribution to host noise.

Bounded independent comparison found the same todo load and three backlink passes inside the heavy timer. Staging new output retains the previous 294 related-note rows through the mentions pass, approximately 0.1–0.2MB extra live row memory for this fixture versus a 28.6MB full note snapshot. This real difference does not establish the cause of the observed timing loss. Host/allocator/cache variation is plausible inference only; cause remains unresolved and the original capture is retained. No concrete source defect justified broader optimization. The current power scheme was rechecked as Balanced.

PASS: 30 selected library tests on final C test source (5,025 skipped, Nextest run `71f49eb5-3bf2-4288-acc9-c5e468598df9`, 0.553s). New regressions retain the same unsaved panel through external note create/edit/rename/delete and alias collision transitions, exercise both RelatedNotes and Mentions, mutate persisted todo text/reference via its owner, and retain populated menu targets/results/applied keys after failure then recover. The test-only failure seam is panel-local. Independent review corrected a mixed-link fixture that was classified as RelatedNotes before reaching its entity-reference Mentions path; separate mention-only data now exercises both existing categories. Review has no unresolved C findings. A publication/last-good/race evidence carries forward.

PASS: `cargo nextest run --test notes_plugin -E 'test(note_alias_supports) | test(launcher_app_delete_note_accepts_alias) | test(note_link_dedupes_backlinks) | test(note_meta_wrap_links_integration)'` (4 passed, 19 skipped, 0.200s; run `d68fff9b-ceae-4da0-b6ae-81be63fc167d`). PASS: `cargo nextest run --test note_panel_auto_save -E 'test(note_panel_auto_saves_on_close)'` (1 passed, 0.174s; run `5b66cdc6-6b85-42ea-831c-a73ab4f9b3c6`). Changed-file rustfmt and diff checks pass; test-generated default config was removed. C source changes are tests/seam only, so measured production source remains the exact B commit above.

M1-C local commit: `0ebd6fbce9d7f5a2418dde9b235275484d80ab55`. No push attempted. The 30-test filter includes the three C regressions, existing idle/derived/debounce/persisted revision, programmatic edit/checkbox/save/discard, visibility/settings/split-view and link-menu tests, plus exact case-insensitive alias and duplicate-alias owner tests. Names and selection are retained in `track_a_handoff_notes.md`.

### M2-A — captured history resolution inputs

History preparation now captures one shallow dashboard snapshot and one current unsorted plugin command catalog, then passes borrowed inputs to a private in-memory resolver. Existing precedence, first registration-order duplicate matching, exact arguments and history-versus-pin fallback presentation remain unchanged. Plugin commands have no reliable catalog revision, so this uses a fresh catalog per preparation rather than retaining potentially stale commands across frames. Eager history traversal is intentionally retained until M2-B.

Independent scoped review found no concrete issue. PASS: `cargo nextest run --lib -E 'test(history_resolution_) | test(history_pins_keep_opaque_clipboard_literals_and_resolve_snippets_by_alias) | test(failed_pin_reload_retains_last_good_then_recovers)'` (7 passed, 5,053 skipped), covering catalog/enablement changes, duplicate argument identity, precedence and captured snapshot consistency. PASS: `cargo nextest run --test plugin_routing -E 'test(data_prefix_routes_only_when_the_builtin_plugin_is_enabled) | test(ocr_prefix_and_inventory_respect_plugin_and_search_capability_enablement)'` (2 passed, 15 skipped).

PASS: small serial opt-in `track_a_benchmark_history_prepare_owner` (1 passed, 5,059 skipped). All four 100-entry fixture/output signatures match the frozen baseline: fixture `fa44f92296bc978a`; outputs mixed8 `ba0b7576ea7d372a`, pins-only `4b77c403d753452a`, rare filter `992bfbcf088924e3`, renamed/missing count50 `30cf9432c6fcaba2`. Each scenario recorded 20 prepares and20 catalog builds (one command enumerated per preparation in this fixture). Small p50/p95/max timings in microseconds were 222.2/392.6/554.2,142.4/145.9/148.8,257.5/351.6/398.3,195.8/207.8/214.2 respectively; smoke evidence only. Full comparison follows M2-B/C. Changed-file rustfmt and diff checks pass. Local checkpoint commit follows; no push will be attempted.

M2-A local commit: `34d85982b83f83328e53a272a7efb9dd16991ba5`. No push attempted.

### M2-B — bounded matching history preparation

Preparation now retains only matching requested output, skips ordinary history entirely for count zero/pins-only/pin-filled output, and otherwise traverses the borrowed deque under one read guard. Snapshot/catalog acquisition and callbacks remain outside that guard. Mixed pins retain stable descending timestamp order, pins-only cached order, and every pin identity suppresses ordinary duplicates even if filtered out. Borrowed identity keys preserve None versus empty arguments. Sparse filters can scan many in-memory candidates; no constant-time claim is made.

PASS: `cargo nextest run --lib -E 'test(history_prepare_) | test(history_resolution_) | test(history_pins_keep_opaque_clipboard_literals_and_resolve_snippets_by_alias) | test(failed_pin_reload_retains_last_good_then_recovers)'` (13 passed, 5,053 skipped). PASS: final small serial opt-in history owner (1 passed, 5,065 skipped). The library run preceded a benchmark-assertion-only correction; the final owner compiled and verified that correction. Changed-file rustfmt/diff checks pass. Independent source review is clear. Parent review corrected direct allocation from an unclamped configured count (now tested with usize::MAX/small input) and a benchmark expectation that missed mixed pin-filled output's zero-read path.

Small owner outputs match all four baseline signatures. Each scenario records20 prepares, zero full input records copied and20 catalog builds. Resolutions per 20 measured preparations: mixed8=160, pins-only8=160, rare=1,540, renamed/missing count50=1,000. Actual test-only boundary observations include five warmups: mixed cases needing ordinary rows use 25 reads, pins-only zero; separate unit cases prove count 0/pin-filled zero and mixed underfilled one. Remaining output Action/string construction is not claimed eliminated.

Small p50/p95/max microseconds were 16.9/24.8/25.3,10.3/10.9/22.1,198.5/203.9/206.2,76.6/99.0/106.3 for mixed8, pins-only, rare, renamed/missing respectively. These are headless debug-test smoke timings; full exact-source comparison follows C. Local checkpoint commit follows; no push will be attempted.

M2-B local commit: `c396ef3f0b45c7f2354ab5657984378931336866`. No push attempted.

### M2-C — history comparison and parity gate

Full serial history owner PASS on exact B commit `c396ef3f0b45c7f2354ab5657984378931336866`: 1 test, 5,065 skipped, 0.62s test duration (Nextest summary 0.648s, run `394ef417-7c70-4804-87f6-23897527a847`). All 12 summaries have 20 samples/5 warmups and fixture/output signatures identical to the frozen baseline. Exact p50/p95/max and counters are retained in `track_a_history_g1.json`; ignored raw log is `target/performance/track-a-m2c-history.log`. Command uses `MULTI_LAUNCHER_PERF=1`, `ML_TRACK_A_BENCH_MODE=full`, serial `--lib --run-ignored ignored-only -E 'test(track_a_benchmark_history_prepare_owner)' --success-output immediate-final --no-output-indent`. Duplicate immediate/final lines were deduplicated; power scheme rechecked Balanced.

| Entries / mode | Baseline p50 / p95 (ms) | G1 p50 / p95 (ms) |
| --- | --- | --- |
| 100 mixed8 | 0.2563 / 0.2680 | 0.0164 / 0.0181 |
| 100 pins-only8 | 0.0652 / 0.0660 | 0.0106 / 0.0125 |
| 100 rare filter8 | 0.2989 / 0.3754 | 0.1650 / 0.1670 |
| 100 renamed/missing50 | 0.2452 / 0.2751 | 0.0820 / 0.1126 |
| 1,000 mixed8 | 3.2043 / 5.4554 | 0.0160 / 0.0162 |
| 1,000 pins-only8 | 0.7097 / 1.8548 | 0.0106 / 0.0115 |
| 1,000 rare filter8 | 3.5957 / 4.1005 | 1.6777 / 2.2287 |
| 1,000 renamed/missing50 | 3.1665 / 6.5005 | 0.1231 / 0.1328 |
| 10,000 mixed8 | 28.3778 / 39.3082 | 0.0173 / 0.0181 |
| 10,000 pins-only8 | 7.6683 / 12.2372 | 0.0102 / 0.0108 |
| 10,000 rare filter8 | 122.0601 / 143.3292 | 19.1359 / 23.2510 |
| 10,000 renamed/missing50 | 112.5275 / 131.4196 | 0.0942 / 0.1699 |

At 10,000 entries, each measured scenario previously copied 200,000 full input records across 20 preparations; now zero. Catalog builds fall from 100,020/11,540/103,860/103,860 to 20 each. Candidate resolutions fall from 150,020→160 mixed8, 15,400→160 pins-only8, remain 153,860 for the rare filter, and 153,860→1,000 renamed/missing50. The large rare case still scans many in-memory candidates under the read guard; it does not acquire a plugin catalog per candidate, but is not constant time. Large count50 is pin-filled and correctly avoids ordinary-history reads. These are debug-test headless CPU timings, with remaining output Action/string construction and one fresh catalog per preparation explicitly retained. All sizes improved p50/p95 in this capture; no claim about release input/GPU latency is made.

PASS: `cargo nextest run --lib -E 'test(history_prepare_matches_eager_reference) | test(history_prepare_observes_note_snippet_changes_and_pin_reload)'` (2 passed, 5,066 skipped, 0.072s). A test-helper reference lifetime compiler error was corrected before this final pass. The oracle is test-only and compares ordered full Action/optional args/query/timestamp/pin/missing fields against former eager preparation; it never runs in production or timed workloads. Reused-widget transitions cover note alias rename/deletion, snippet deletion, persisted pin/unpin publication and opaque clipboard compatibility. Independent scoped review has no unresolved findings.

PASS: `cargo nextest run --test history` (5 passed, 0 skipped, 0.223s test duration; build3m03s). Changed-file rustfmt and diff checks pass. C source changes are test-only; measured production source remains the exact B commit above. No push will be attempted.

M2-C local commit: `9b7447fdf48f6aec6cde61d23512b1f3c36b5f6f`. No push attempted.

### M3-A — visible launcher list rows

An app-owned variable-row geometry cache uses explicit result-generation invalidation, binary viewport lookup, absolute row identities, synthetic selected-row scrolling and bounded popup retention. Cold layout mirrors eager SelectableLabel wrapping and prefix horizontal overflow, including negative left offsets. Only visible/overscan rows plus one menu owner construct widgets and clone Actions. Grid remains eager in this checkpoint. Effective font/style, width, scale/PPI, layout and display inputs invalidate geometry; actual result replacements and folder reloads invalidate through their owners. Repeated warm frames do no complete-list formatting/hash/layout. Owned popup cleanup closes obsolete egui root state on the next active frame without closing an unrelated popup.

PASS: production `cargo check --lib` after a test-only accessor gate correction. PASS: final focused library filter `test(root_list_) | test(keyboard_navigation_is_consistent_between_grid_and_list_modes) | test(context_menu_resolves_semantic_actions_with_list_grid_parity) | test(context_action_is_deferred_until_all_result_rows_have_been_visited) | test(deferred_activation_from_results_)`: 9 passed/5,063 skipped, run `35380893-0ba9-4dad-92c5-4c928f8f9ae1`. Four new tests cover an independent actual eager/virtual response-rectangle oracle, empty/single/10k bounded selection and stable IDs, independent layout/display invalidations, and a real opened menu retained offscreen then closed on replacement. Initial targeted execution exposed a pre-existing stale parity fixture using opaque `clipboard:Regards` as a snippet; the fixture now uses the canonical `snippet_run_action("sig")`, preserving semantic assertions and production resolution. Independent review confirmed that correction and has no unresolved findings. Other reviewed fixes cover font-style keying, deterministic measured-selection settling, prefix overflow, isolated app fixtures and exact-owner popup cleanup.

PASS: small serial opt-in root owner, 1 passed/5,071 skipped, run `f10ceacf-3779-4946-95fa-7c0200d0232e`. List100 builds 28 widgets/sample (560 across 20 samples), one cold rebuild/100 rows taking 6.3061ms, zero warm rebuilds/rows measured; p50/p95/max 0.8649/0.9646/1.1013ms. Actual visible membership/order receipts are `e7ba78a87d74e725`/`7770e878af1fc1ec`; these intentionally differ from eager all-result receipts. Grid100 still builds 100/sample and has no list geometry payload. Cold time is one observation, not a distribution; small timings are smoke evidence only. Full exact-commit comparison follows before M3-B edits. Changed-file rustfmt and diff checks pass. Test-created default clipboard config was verified and removed. Local commit follows; no push will be attempted.

M3-A local commit: `89fe30f9f1295ddb69fe1d5672dd79501bfc3fd6`. No push attempted. Full root capture completed against this exact source before M3-B edits.

Full root owner PASS on that exact commit: 1 passed/5,071 skipped, 27.14s test duration; 6 scenarios with 20 samples each. Balanced power rechecked; Nextest run `49fe73ca-8248-4989-8a4a-a106b9c32085`. Raw ignored log `target/performance/track-a-m3a-root.log`; complete export `track_a_launcher_list_g1.json`. All fixture signatures match baseline; eager grid membership/order receipts remain identical. List receipts intentionally describe the actual ordered viewport subset rather than all results; complete results and selected identity assertions remain active.

| Results | Baseline warm list p50 / p95 (ms) | M3-A warm list p50 / p95 (ms) | Widgets / frame | Single cold geometry (ms) |
| --- | --- | --- | --- | --- |
| 100 | 1.7974 / 1.9538 | 0.8446 / 0.9445 | 100→28 | 6.3263 |
| 1,000 | 15.5868 / 16.2754 | 0.8261 / 1.2778 | 1,000→27 | 63.1000 |
| 10,000 | 155.1912 / 157.1399 | 0.8703 / 0.9719 | 10,000→28 | 631.9789 |

Each list case measures exactly one cold full geometry pass and zero warm rebuilds/rows measured. Cold O(N) work remains on invalidation and the large debug-test observation is a material 631.98ms stall; this is not a cold latency improvement claim. Frozen baseline contains warm frame distributions only, so it cannot establish a comparable cold regression or speedup. Warm results are headless debug-test CPU, not release GPU/input latency. No blanket claim that all UI stalls are removed. Grid remains eager at 100/1k/10k widgets; its warm p50/p95 are 1.6767/1.9335,14.5248/15.4167,142.5023/145.2030ms. Timing changes in that unchanged path are retained without optimization attribution. M3-B will measure its own cold/warm work separately.

### M3-B — complete visible grid rows

The same geometry cache now owns grid cell extents, global column alignment and logical row prefixes. Cold measurement preserves existing multiline text, nominal wrap width, 44-point outer allocation, literal8/6 spacing, scaled interaction/font metrics and conditional stripes. Column minima and full stripe width are distinct from actual clickable cell sizes and content bounds. Empty Grid keeps egui's half-scaled-interaction-height shell with zero cells. Visible queries combine logical overscan with a max-end interval tree for tall content spilling beyond its logical row, without constructing invisible intervening rows. Only complete actual rows and at most an open popup owner's row build widgets; incomplete rows create no placeholders. Absolute identities, exact-owner popup cleanup, pointer/deferred actions and A list behavior remain intact.

Independent scoped review has no unresolved findings. Corrections resolved cell-versus-column minima, actual content bounds versus stripe width, outer44 centering for selected scrolling, tall overflow visibility, empty Grid shell and click isolation. The real settled Grid oracle compares actual response rectangles, selected-cell dimensions, logical cursors, visual row bounds and parent content bounds. Tests cover1/2/3/5/6 columns, empty/single/partial, wrap/newline/wide/scale/style/interaction minima,10k complete bounded rows/IDs/selection, actual first/final clicks, invalidation, stripes and popup lifecycle. Selected grid cells retain legacy horizontal clipping from global nowrap widths in a vertical-only ScrollArea; normal selection must be vertically visible and horizontally intersecting. Click targets are checked inside the real ScrollArea inner_rect with centers inside the paint clip.

PASS: final targeted library filter `test(root_grid_) | test(root_list_) | test(keyboard_navigation_is_consistent_between_grid_and_list_modes) | test(context_menu_resolves_semantic_actions_with_list_grid_parity) | test(context_action_is_deferred_until_all_result_rows_have_been_visited) | test(deferred_activation_from_results_)`:14 passed/5,063 skipped, run `ad4ce7ca-d9fb-428e-855f-cf3953b278f2`. Initial test-only tuple/array compiler errors were corrected. Oracle-driven source fixes were verified against actual Grid, not relaxed expectations. Distant click triage corrected the fixture's max offset calculation (use output.inner_rect, not paint clip) and separated pointer movement from press to avoid synthetic dragging; the fixture keeps query/root visible between real dispatches. Actual target9999 activation passes; no production extent padding was added.

PASS: small serial opt-in root owner,1 passed/5,076 skipped, run `a0edc5d9-19f1-4346-aafe-beea284215db`. Across20 samples list100 builds560 widgets, grid100 builds960 (28/48 per frame); both warm caches rebuild/measure zero. List p50/p95/max0.8342/0.9673/1.0700ms; single cold1 rebuild/100 rows/6.518ms. Grid p50/p95/max1.2639/1.6245/1.6352ms; single cold1 rebuild/100 cells/5.851ms. List membership/order receipts remain A's `e7ba78a87d74e725`/`7770e878af1fc1ec`; grid viewport receipts `69549a2b42b301d8`/`080ec16305364a7e` intentionally differ from eager all-result baseline. Full exact-commit comparison follows before C source changes. Formatting/diff checks pass; local commit follows, no push will be attempted.

M3-B local commit: `ed106cd6626240f3cc998bf3191ffafc9c3f837c`. No push attempted. Full root comparison captures this exact source before M3-C edits.

Full exact-commit root owner PASS:1 passed/5,076 skipped,4.86s test duration; run `c5651274-763c-4feb-bb22-12ddf11b187c`. Six scenarios,5 warmups/20 samples each; Balanced rechecked. Full export `track_a_launcher_grid_g1.json`, ignored raw log `target/performance/track-a-m3b-root.log`. Fixtures remain baseline-identical; actual list membership/order receipts match M3-A. Grid viewport hashes intentionally differ from eager all-result receipts, with complete fixture/results and selected identity still asserted.

| Results | Baseline grid p50 / p95 (ms) | M3-B grid p50 / p95 (ms) | Widgets / frame | Single cold geometry (ms) |
| --- | --- | --- | --- | --- |
| 100 | 1.6690 / 2.5717 | 1.3522 / 2.1964 | 100→48 | 6.1833 |
| 1,000 | 14.1864 / 14.9405 | 1.2844 / 1.4815 | 1,000→48 | 58.0960 |
| 10,000 | 136.6896 / 142.3946 | 1.3456 / 1.5626 | 10,000→48 | 588.1519 |

Every grid size records one cold rebuild measuring exactly its cell count, zero warm rebuilds/cells measured. Cold O(N) work remains a material invalidation cost, not an eliminated stall; baseline has no comparable cold distribution. Actual list rerun p50/p95(ms) is0.8958/1.1884,0.8551/1.1220,0.8455/1.1000 for100/1k/10k, with28/27/28 widgets and zero warm geometry work; single cold observations6.7639/62.9023/646.1076ms. Those timing changes are retained without attribution to a list optimization or selecting a favorable rerun. Headless debug-test CPU measurements do not establish release GPU/input latency.

### M3-C — lightweight Quick Notes projection

NotesDialog owns revision-validated row metadata and ordered original-entry search indices. Backlink counts read the authoritative cache without cloning note bodies; previews stream normalized Unicode text to the existing120-character limit. Warm browsing borrows source entries and cached presentation; only actual actions copy full bodies. Search-only updates reuse metadata. External changes, including mutations while the real dialog is temporarily taken out of the app, recover through revision observation. Atomic candidate publication retains all last-good rows/keys on failure or race with bounded repaint retry. Editing freezes backing entries, including the new-note sentinel; failed writes retain drafts and successful persistence remains successful if presentation refresh later fails. Existing eager widgets and receipt meaning remain unchanged until D.

PASS: scoped library filter `test(gui::notes_dialog::tests::) | test(note_backlink_count)`22 passed/5,060 skipped, run `a2b30846-f9bb-4288-896a-831bf04f2ca3`. Subsequently strengthened populated-dialog wrap-links recovery test PASS1/5,081 skipped, run `38ea9c8d-17df-4003-83d4-175a574f84b3`. Independent review has no unresolved substantive findings after preview-leading-whitespace, atomic explicit-refresh, initial-edit capture, isolated hotkey fixtures and real draft/postcommit failure proof corrections. Initial compiler feedback from disjoint field borrowing and the final fixture's canonical saved heading were corrected; no failing assertions were removed.

PASS: small serial opt-in Quick Notes owner1/5,081 skipped, run `4fbeefd0-c73a-4923-86fa-c4d59ba5dcef`.100-note empty-filter p50/p955.2749/7.1078ms,100 widgets/frame; sparse-filter0.2734/0.2857ms,1widget/frame. Both20-sample windows take zero note snapshots. The owner initially caught a test-only metric-slot indexing error after adding NoteSnapshot; the row assertion now checks QuickNotesRowsBuilt. These small timings are smoke evidence only; exact-commit full comparison follows before D edits. Changed-file rustfmt/diff checks pass; verified test-generated default clipboard config removed. Local checkpoint commit follows; no push.

M3-C local commit: `369744ef9c697ea2cfddca3077ce61407b46b6b8`. No push attempted. Full Quick Notes owner PASS against this exact source before D edits:1 passed/5,081 skipped/11.151s, run `2417c7a1-65a4-4d27-a35f-b870790697e8`;6 scenarios20samples/5warmups, Balanced power. Raw ignored log `target/performance/track-a-m3c-quick-notes.log`; full export `track_a_quick_notes_metadata_g1.json`. Every fixture and eager actual-row output/order signature matches frozen baseline exactly. All warm scenarios take zero snapshots; widgets remain100/1k/5k for empty search and1 for sparse search. No C virtualization claim or release GPU/input latency claim.

| Notes / search | Baseline warm p50 / p95 (ms) | M3-C warm p50 / p95 (ms) | Widgets / frame |
| --- | --- | --- | --- |
|100 / empty|38.8283 /43.5323|5.2804 /8.7166|100|
|1,000 / empty|405.6476 /538.3299|50.1853 /72.2482|1,000|
|5,000 / empty|2004.7883 /2034.0226|268.1635 /343.1368|5,000|
|100 / sparse|2.5719 /2.7831|0.2784 /0.3251|1|
|1,000 / sparse|21.7628 /26.5616|0.2748 /0.3562|1|
|5,000 / sparse|106.0024 /122.6124|0.2963 /0.3475|1|

These distributions cover warm headless debug-test frames. Initial snapshot/index/metadata preparation remains O(N) and was outside these warm timing windows; it is not a measured cold improvement. D must measure layout invalidation separately and retain this limitation. M3-D sole writer dispatched only after this capture finished.

### M3-D — variable-height Quick Notes virtualization

NotesDialog now owns variable-row geometry keyed by accepted metadata/projection, actual available width, effective fonts/style/scale and spacing. An independent eager-widget oracle establishes body/header/preview/separator bounds and ordered prefix-width growth. Warm browsing queries visible rows plus overscan, keeps full two-axis extent and original-entry action identities, and retains at most one disjoint popup owner. Cold invalidation measures the complete projection; editor behavior and C's revision/error/draft publication boundary remain unchanged. Geometry timing instrumentation is test-only.

Focused execution initially exposed test assumptions about animation settling and selectable child-label hit ownership. Exact anchor offsets now use deterministic egui frames; real menu clicks target actual header gaps after viewport settling, preserving existing selectable label behavior. The two previously failing owner tests PASS (run a54fb27a-5b72-4c49-9488-0fa48f7dac57,2/2); full module and small owner remain pending. No production workaround or weakened action-identity assertion was used. Review clearance remains conditional on those required results. Native visible Windows Quick Notes smoke is NOT RUN (native computer control unavailable); headless geometry/interaction tests are narrower evidence.

Required M3-D gates PASS: full NotesDialog module25/25 (5,061 skipped), Nextest run b9b08c0b-b2f0-483b-8969-be4f6766a1aa; small serial ignored Quick Notes owner1/1, run69cbe33c-6054-4d74-9c3f-9b63eb8679a2. All three small scenarios build bounded widgets and take zero warm note snapshots/geometry rebuilds/rows measured. Standard actual viewport180.64pt versus small87.96pt establishes real height variation. Independent scoped final review has no unresolved findings; its execution conditions are satisfied. Exact-commit full9scenario capture follows local commit before M4 source edits.

Small owner smoke timings p50/p95/max(ms):100-note empty0.5969/1.0421/1.2368 with6 widgets/frame and single cold100-row1.188ms; sparse0.4488/0.5516/0.5567 with1 widget/frame and cold1-row0.150ms; small-viewport empty0.4807/0.5983/0.8231 with4 widgets/frame and cold100-row15.914ms. Both empty complete-projection signatures are c0f4d9448d8e533e; sparse8d02f29051af4bd2, fixturec8023bc0ea5e1570. Actual rendered-subset signatures vary with viewport as intended. Cold observations are individual rebuild timings, not quantile distributions; the differing cold values are retained. Small owner is smoke evidence; full exact-commit comparison remains next.

M3-D local commit:5068bfec9f304496613d5291d1fcafc42aeaf09d. Parent cumulative diff/check passed and only intended files staged. No push attempted. Exact-commit full Quick Notes capture starts before M4 source edits; Balanced power rechecked.

Full exact-commit M3-D owner PASS:1 passed/5,085 skipped/3.32s test duration, run3b58f369-f3c8-4c29-8767-16e3a58b96c7. Nine scenarios,5 warmups/20 samples each; Balanced power. Export track_a_quick_notes_virtualization_g1.json; ignored raw log target/performance/track-a-m3d-quick-notes.log. Fixture signatures match frozen baseline. Actual viewport output hashes intentionally differ from eager baseline; complete ordered projection membership is independently asserted. Its new signature domain differs from baseline, so those hashes are not compared as equal. Standard and small-height empty cases retain identical complete-projection signatures at each size. All nine warm scenarios take zero note snapshots and rebuild/measure zero geometry.

| Notes / search | Baseline p50 / p95 (ms) | M3-C p50 / p95 (ms) | M3-D p50 / p95 (ms) | Widgets/frame | Single cold layout(ms) |
| --- | --- | --- | --- | --- | --- |
|100 / empty|38.8283 /43.5323|5.2804 /8.7166|0.5926 /0.7296|100→6|0.7742|
|1,000 / empty|405.6476 /538.3299|50.1853 /72.2482|0.5749 /0.6180|1,000→6|8.2225|
|5,000 / empty|2004.7883 /2034.0226|268.1635 /343.1368|0.5924 /0.6256|5,000→6|39.5820|
|100 / sparse|2.5719 /2.7831|0.2784 /0.3251|0.2898 /0.3283|1|0.1381|
|1,000 / sparse|21.7628 /26.5616|0.2748 /0.3562|0.2884 /0.3018|1|0.1475|
|5,000 / sparse|106.0024 /122.6124|0.2963 /0.3475|0.3143 /0.3233|1|0.1480|

Standard actual viewport height180.6375pt; supplementary small-screen case87.9555pt builds4widgets/frame at every size. Supplementary p50/p95(ms): 100 notes 0.4532/0.4934; 1k notes 0.4511/0.4798; 5k notes 0.4614/0.4861. Single small-screen cold rebuild observations are12.7340/125.4593/627.0763ms, measuring100/1k/5k rows respectively. These new scenarios have no frozen baseline comparison. Every case records one cold rebuild. The material627ms cold invalidation cost remains explicit; warm responsiveness does not establish elimination of all UI stalls. Initial metadata/snapshot preparation is also outside warm distributions. Headless debug-test CPU measurements do not establish release GPU/input latency or visible native behavior. M4-A source edits begin only after this capture completed.

### M4-A — Actions-only reload retains the indexed tail

The changed Actions watcher branch now shares local persistence's custom-prefix/current-indexed-tail publication boundary. It no longer constructs or consumes the filesystem iterator. The final publication owner still updates action IDs/cache/query; local persistence and external watcher retain their separate single version-bump ownership. No root/cap/order/dedup reconstruction occurs on custom reload. Typed missing/malformed/equal handling retains published state.

A test-only thread-local scanner-factory entry counter checks zero traversal independently of opt-in telemetry. The existing watcher regression now exercises a nonempty ordered tail with a cap smaller than its size and an unavailable configured root. The real event owner benchmark asserts exact tail parity and zero scan calls/work/completions/errors/abandonment. Independent production review is clear; small action-ID consumer and duplicate-burst assertions are being added before final focused verification. Baseline changed10k reload p95 is1006.7269ms; post-change performance is pending exact-commit capture, with retained-tail clone/cache/query costs still timed.

M4-A targeted gates so far PASS: failed-save retention plus watcher regression2/2, run8f472266-6511-474d-8411-acfe30a8749c (5,084 skipped); strengthened watcher1/1, run377139e1-10e0-4717-8816-3c191dfb3b17 (5,085 skipped), including explicit new/removed/retained action-ID map entries and three-event equal burst. Integration watchers/actions_watcher_sends_event PASS1 (2 skipped), rundbdb9308-91c4-4c74-bad6-b5cf27c7c910; watcher_failures/invalid_actions_watcher_logs_error PASS1, runf8022e2d-8251-476f-81a3-f132814d84f8. Independent scoped production review clear; small owner/final source checks pending.

M4-A small serial actual event owner PASS1/5,085 skipped, runcb25edec-89d8-4ff0-846e-6475c5a507ad. Changed100-custom+16-indexed p50/p95/max7.0444/7.7525/7.9455ms;20reloads/2,320units. Unchanged6.8238/7.4029/9.9040ms;20reloads/2,000units. Both have zero IndexScan calls/work/completions/errors/abandonment, exact output929dbf43068c8bf2/order5cd26fe1187f45d1 matching baseline. These small numbers are smoke evidence, not full stress comparison. Parent cumulative diff check clean; final formatting/own-artifact cleanup precede local commit. Full exact-commit capture next, before B source changes.

M4-A local commit:6257e81795a2c50a0d8d39b128ce1f186f1816e3. No push attempted. Full exact-commit reload comparison started before M4-B edits, Balanced power rechecked; parent owns capture session47319.

Full exact-commit M4-A actual reload owner PASS1/5,085 skipped/20.72s test duration, run5c297f48-0f9d-4155-8a26-7a7282903644. Six scenarios,5warmups/20samples each; Balanced power. Full export track_a_actions_reload_g1.json, ignored raw log target/performance/track-a-m4a-actions-reload.log. All fixture/output/order signatures match frozen baseline exactly. Every scenario has zero IndexScan calls/work/completions/errors/abandonment. Changed units are20*(custom+retainedtail); unchanged20*custom. Typed file read, retained-tail cloning/cache/query remain timed; fixture writes/enqueue remain outside timing.

| Custom / indexed | Baseline changed p50 / p95(ms) | M4-A changed p50 / p95(ms) | Baseline unchanged p50 / p95(ms) | M4-A unchanged p50 / p95(ms) |
| --- | --- | --- | --- | --- |
|100 /16|9.1597 /9.7526|7.1251 /7.8382|6.6370 /7.6493|6.7668 /7.1554|
|1,000 /1,000|105.1468 /107.8851|16.1016 /17.7054|9.2982 /10.2060|9.4808 /11.0673|
|10,000 /10,000|997.2981 /1006.7269|107.8570 /110.5174|35.9452 /36.8010|36.4331 /39.4701|

Changed-path 10k max110.7552ms (baseline1019.6943ms); the remaining110ms debug-test event work is material and is not described as elimination of every stall. Unchanged 1k/10k p95 increases are retained without optimization attribution. These are CPU/event-drain timings, not native input/display latency. B will establish a separate worker foundation; M4-A does not yet migrate startup/config-triggered indexing. Its sole writer started only after this full capture completed.

### M4-B — bounded cancellable index worker

A GUI-independent coordinator owns one persistent worker with replaceable pending work, complete-result slot and coalesced notification. Public typed config retains exact root ordering and configured optional cap; generations and config identity are rechecked before result publication. Traversal cancellation reaches directory/duplicate/non-UTF8 skip loops and canonicalization boundaries. Failed scans discard partial aggregates. Existing index_paths/index_paths_batched interfaces/order/dedup/cap behavior remain available; no GUI/startup caller migration occurs in B.

Shutdown atomically takes worker+reaper reservation from one lifecycle slot, cancels/revokes under state lock and transfers live handles outside locks without traversal waits. Worker termination wakes waiters; known panic survives live reaper transfer, and supervisor failure remains an explicit degraded cleanup error. A blocked OS call cannot be forcibly cancelled; one worker and a held bounded reservation constrain that lifetime.

Review found and resolved concurrent split handle/permit ownership, known-panic reporting during callback epilogue, a test-hook lock lifetime, stale acknowledgement expectations and cancellable cap-completion accounting. Added deterministic gates retain exact behavioral assertions. Focused index_coordinator_ filter PASS11/5,086 skipped, rune096fe88-72a1-48fd-aa95-d7a32aa55825,0.092s test execution. Covers A/B/C replacement, skipped/canonical/cap cancellation, failure/recovery, bounded result/wake coalescing/ack races, off-thread legacy parity, reserve-before-spawn, nonblocking/concurrent shutdown and live panic. Existing domain/indexer_ compatibility and final review/formatting remain pending. No performance throughput or native blocking-call latency claim from these deterministic tests.

M4-B compatibility PASS:domain/indexer_3passed/98skipped, run7b99b6e8-c853-40de-83ea-c5f12b387d17,0.035s execution (4m03s build/link). Final independent review has no unresolved findings; its compatibility condition is satisfied. Parent diff check clean, final source formatting/inspection precedes local commit. Earlier initial coordinator run9/10 failed only the test-hook mutex lifetime; corrected isolated test8fb1b9cf-9aa7-46d7-a7b8-b7772259e68b PASS and full11-case run above PASS. No reaper implementation or GUI/startup path changed in B; no broad suite or native smoke claimed.

Final M4-B coordinator rerun after a test-only cleanup assertion PASS11/5,086 skipped, run6dda2ef8-94e7-4439-828f-f75e5fe13a31,0.096s tests. Existing domain3/3 remains applicable (no subsequent production change). Changed-file rustfmt/source whitespace checks clean; parent cumulative diff/status contains only intended indexer/coordinator plus parent measurement/docs. Local commit follows; no push.

M4-B local commit: `3468a51dec84c78298de28722aa1317a72b70992`. No push attempted. Final 11 coordinator and 3 domain indexer tests pass; review findings resolved. M4-C sole writer dispatched against this committed API; parent has no compiler jobs. Full original M4-C scope and actual API handoff are above; startup/catalog readiness, typed roots+cap, app-scoped notification, latest-custom merge, exact generation/config guards and nonblocking exit are the active scope.

### M4-C — startup transfer and guarded GUI publication

Main now obtains the complete startup index through the coordinator before plugin construction, acknowledges it and transfers that same worker into the app before its first frame. A narrow public install boundary attaches an app-scoped ready sender/wake. App-owned typed roots+cap config replaces the roots-only setter; both committed settings paths use it. Completions are acknowledged before validation, then require exact desired config and generation. Changed results merge the currently published custom prefix; equal tails preserve Arc/cache/query/version. Failures retain the last-good catalog with a separate index diagnostic. Empty roots clear the tail immediately and invalidate old requests; exit revokes and shuts down without waiting for traversal.

Review corrections in progress: distinguish desired config from accepted/current request so same-config failure can retry; validate startup identity/acknowledgement nonblockingly in the coordinator; isolate new GUI fixtures with inert plugins and disabled native enablement; exercise actual production full startup-catalog assembly in the consumer test; one bounded integration test for app-scoped wakes and closed-owner late events. Initial focused lib8/8 passed before this correction batch; final required execution remains pending. No C performance or completion claim yet.

M4-C corrected focused lib PASS11/5,094 skipped, runfc3d2253-b981-49f1-996e-26c06c85b8b4. Startup binary startup_index_ PASS2/2, run3ea62b2d-6963-4035-89b0-fc968cf6bdd0. Critical final review clear with all retry/validator/isolation/consumer/equal/late-event findings resolved, conditional on remaining domain/settings/bincheck gates. Parent has no compiler jobs; solewriter owns remaining checks. C still in_progress/uncommitted.

M4-C complete scoped verification: corrected GUI lib11/11 (fc3d2253-b981-49f1-996e-26c06c85b8b4); startup binary2/2 (3ea62b2d-6963-4035-89b0-fc968cf6bdd0); domain indexer3/3 (8533db3d-b923-453c-ae26-77ded5996196); settings editor1/1 (6dea57ac-186e-4f90-ae20-0003e8c77cf3). Production cargo check --bin multi_launcher, cargo fmt --all -- --check, and cumulative git diff --check PASS. Final independent review clear; all execution conditions satisfied. Recoverable scan/submission failures permit same-config retry; terminal failures stay explicitly diagnosed. Actual startup helper assembles the full catalog consumed by Omni proof. Source search confirms production GUI has no traversal/wait path; startup main wait and test-only fixture references remain. Confirmed test-created clipboard config removed. No broad suite/native manual check claimed; local commit follows.

M4-C local commit: `3c0446e88747820a6a32321df10f734a73f85f93`. No push attempted. All scoped tests/checks and critical review conditions satisfied; only intended files staged. M4-D sole writer dispatched; parent has no compiler jobs. Future captures will identify the exact D commit and preserve the A/baseline comparisons.

### M4-D — accepted-result consumers and phase measurements

The accepted-index integration now exercises the real IndexReady reducer through action cache/filter metadata, action IDs, unchanged-query results, history missing/current resolution, pin lookup and cached radial catalogs. A second publication leaves query results unchanged to isolate actions_version invalidation. Single-file roots encounter canonical duplicates before reaching the cap, preserving ordered output.

The consumer reproducer exposed stale same-query results: update_action_cache rebuilt the catalog but did not invalidate last_results_valid. The fix belongs to that cache owner; publication still owns refresh scheduling. Equal-tail publication bypasses both, preserving the no-churn behavior. This necessary correction may increase measured reload work because query recomputation now actually occurs; prior captures remain immutable.

An opt-in ignored workload alternates two ordered root configurations on one persistent worker. It separately records synchronous request acceptance, request-entry through completion (including background traversal/wait), and real ready-event publication/cache/query. Tree generation, assertions, signatures and notification waiting are outside publication timing. Completion timing includes synchronous submission, rather than omitting traversal overlapping submission. Test-only waits do not add production GUI waits.

Independent scoped review is clear, conditional on focused execution. Verification and exact-commit full capture remain pending; no native slow-directory responsiveness or display-latency claim is made. All commits remain local.

M4-D focused library gate PASS10/10, Nextest run3cb3aeae-650a-4729-8602-786c08769438. New consumer fixture uses exact launcher-action query syntax app needle and checks relevant retained-ID absence/presence, preserving unrelated command catalog behavior. The second publication still preserves complete query results while changing the action catalog, proving version-driven radial invalidation. Initial fixture assertions failed and were corrected without changing production search syntax/ordering. Reviewer confirmed both proofs remain meaningful. Domain/watcher/small-owner execution pending.

M4-D source/focused gates PASS:lib10/10 run3cb3aeae-650a-4729-8602-786c08769438;domain indexer4/4 c9b2b753-bb1c-43cb-a57d-373ee66cc8b5;watcher1/1 3f650daf-8516-40db-9102-8c9ea4c56683;watcher-failure1/1 5f97d286-cb17-4ee5-a064-ff48b1a85cdc;both small ignored owners2/2 430d785f-5e7e-45a4-ba9e-9758c35eb097. Formatting/diff PASS; independent review clear, execution conditions satisfied; confirmed test-generated clipboard catalog removed. Small1k request p50/p95/max .0092/.0153/.0172ms, request-entry-to-completion90.4130/93.0928/93.7895ms, publication3.2845/3.5765/3.6012ms;20samples,20completedscans/20kactions,60iteratorcalls,no errors/abandonment. Small100/16 changed reload7.0353/7.7827/7.9239ms, unchanged6.9808/7.9446/8.4270ms,zero scan. These are smoke timings, not the full stress comparison. Local source checkpoint commit precedes exact-commit full capture; M5 edits stay paused until capture completes.

M4-D local commit: `f5560cc3660b8187478623ff65fbe1e87c665166`. Full exact-commit capture PASS2/5,105 skipped/62.923s, runfa7d50bc-3eb5-4c7d-8dc0-3aa9239d9ae4, Balanced power. Export [track_a_index_integration_g1.json](track_a_index_integration_g1.json); ignored raw log target/performance/track-a-m4d-index-owners.log. Twelve unique summaries;5warmups/20samples. All six reload fixture/output/order signatures equal frozen baseline; all reload scan counters zero. Config phase signatures agree across request/completion/publication at each size;20 completed scans and20k/200k actions with no error/abandonment. IndexScan calls60/420 are iterator calls, not worker count or scan count.

| Custom / indexed | Baseline changed p50 / p95(ms) | M4-A changed p50 / p95(ms) | M4-D changed p50 / p95 / max(ms) | M4-D unchanged p50 / p95 / max(ms) |
| --- | --- | --- | --- | --- |
|100 /16|9.1597 /9.7526|7.1251 /7.8382|7.1946 /7.7032 /7.9818|6.7536 /7.3961 /8.5186|
|1,000 /1,000|105.1468 /107.8851|16.1016 /17.7054|16.8310 /17.6475 /19.8611|9.6398 /11.6088 /19.4109|
|10,000 /10,000|997.2981 /1006.7269|107.8570 /110.5174|107.6700 /116.3843 /121.9384|37.4080 /38.5512 /39.3862|

| Indexed files | Request p50 / p95 / max(ms) | Request-entry through completion p50 / p95 / max(ms) | Publication p50 / p95 / max(ms) |
| --- | --- | --- | --- |
|1,000|.0090 /.0117 /.0137|90.5515 /95.2359 /98.4456|3.1455 /3.6564 /3.7594|
|10,000|.0140 /.0193 /.0245|900.0218 /938.0480 /944.0389|36.9766 /39.0663 /42.8731|

The request owner excludes worker traversal/wait; completion includes synchronous submission and off-thread traversal/wait; actual ready-event publication includes cache/query but excludes waiting for notification. The private production request owner is timed, not entire settings save. Config phases have no pre-change timing baseline. The 10k scan is still roughly0.94s at p95, and roughly39ms publication/116ms custom reload remain material debug-test work. M4-D changed10k p95 is slower than A; corrected query invalidation changes legitimate work and host timing can vary, so no exact attribution is claimed. Unchanged reload p95 remains above baseline at1k/10k. No native slow-directory responsiveness, network filesystem, GPU or input-to-display latency measurement is claimed. Source edits for M5 began only after full capture and export verification.

### M5-A — actual-worker effect characterization

Only cfg(test) code in native_effects.rs changed. A channel-gated actual CoordinateToolController drives the actual CursorEffectsRuntime with existing fake native operations. Typed receipts distinguish poll/render, Halo/Zoom, complete source geometry, filter inputs/lists and refresh/presentation calls. Ordinary moving ticks dispatch one cached A refresh plus one current B presentation per effect; stationary ticks each refresh once per effect without full render or presentation. Frozen HUD does not freeze live effects. Topology forces current presentation after complete exclusions, failed samples hide invalid sources, disabled effects stop refreshing, and established independent failure/fallback behavior is retained.

Initial worker run241a69d4-2a68-4b6f-8f44-8315a6eed37d passed2/6. Review corrected fixture poll-versus-render phase placement, gate disconnection before join (including unwind), actual Halo fallback expectations, full Zoom geometry and resume/topology exclusion ordering. Corrected opt-in serial worker run9bdc032e-c83e-413f-9e1c-2a168cd71419 PASS6/6. Final scoped compatibility run e24468b4-d497-471b-b8a9-5b242659f331 PASS24/24,5,089 skipped, PERF=1/serial. Metrics calls/work_units agree with typed refresh/present receipts. Formatting and diff checks PASS; independent scoped review clear.

Exact final gate: `cargo nextest run --lib --test-threads 1 --no-fail-fast -E 'test(coordinate_tool::controller::tests::) | test(worker_effect_tick_) | test(native_effects::tests::stationary_) | test(native_effects::tests::resumed_outlines_) | test(native_effects::tests::topology_refresh_) | test(native_effects::tests::halo_fallback_coexists_) | test(native_effects::tests::halo_missing_live_sample_) | test(native_effects::tests::filter_input_invalidation_)'`, with MULTI_LAUNCHER_PERF=1.

An earlier overly broad filter, run287f0b1a-140e-42a0-8bde-91447fba7ecb, included unrelated screen_draw::controller::tests::new_capture_timeout_fails_safe_without_capturing_under_the_old_surface; that test failed and10 were cancelled. No screen-draw code changed, no cause attribution is made, and the correctly narrowed required gate passed. Counts above are deterministic runtime API-dispatch evidence with fake native operations, not measured Windows magnification GPU work or display latency. Native visual/mixed-DPI checks remain NOT RUN at this checkpoint. All commits are local.

M5-A local commit: `7696f0dc9faf99adfb4c7a74ab56169bcced8c00`. No push. M5-B sole writer dispatched with the explicit post-sample full-render/stationary/invalid branch contract and smoke CountingBackend forwarding. Parent has no compiler jobs. Native smoke remains for C.

### M5-B — one post-sample source phase

The controller now chooses full render for changed/forced/retry frames, stationary source refresh for equal successful frames, or no source work for equal invalid frames. Effects status is published after that phase. Windows polling only pumps messages/topology; stationary refresh delegates existing validated-source polling and stacking. Full render retains existing cheap-surface, outline, complete-filter and current-sample reconciliation ownership. The real smoke wrapper forwards the new trait phase. No sampler/cadence/session/fallback/resource ownership changed.

| Ordinary worker tick, per active effect | M5-A cached refresh / current presentation | M5-B cached refresh / current presentation |
| --- | --- | --- |
|Moving A to B|1 /1 (old A then current B)|0 /1 (current B)|
|Stationary valid frame|1 /0|1 /0|
|First invalid sample or disabling that effect|Old pre-sample refresh possible|No old-source refresh|

These are actual controller/runtime dispatch receipts with fake native operations, not GPU work or display latency. Exceptional native failure/fallback recovery retains its established semantics and is not claimed universally single-call. Full zoom geometry, topology/resume exclusion order, frozen HUD/live sources, sibling failures and all-off lifetime remain covered.

Final serial opt-in scoped gate PASS25/25,5,089 skipped, runb335f9f6-827a-4d3a-9403-e2078930e9ba; same precise controller/native filters as A plus new stationary controller coverage. Aggregate refresh/present metrics match typed receipts. cargo check --bin coordinate_tool_smoke, changed-file rustfmt and diff checks PASS. Independent scoped review clear and execution conditions satisfied. A moved fake-factory compile error and an overbroad lifetime assertion were corrected in tests; isolated disabled-owner rerun139414e7-bc8a-4660-901d-5df7b8337eba PASS, followed by the final full focused25-case pass. Parent cumulative diff contains only intended four Rust files and documentation. Native Windows smoke is next; all commits local.

M5-B local commit: `ab3eace9723280cbe0a00f625b1d0b3949035aac`. M5-C parent builds the actual coordinate_tool_smoke binary, then attempts the existing --cursor-effects-auto protocol in an isolated ignored working directory. No source edits during capture, no push. Parent owns build session52006.

### M5-C — real Windows native API/lifecycle smoke

Built the production `coordinate_tool_smoke` dev binary from exact M5-B source ab3eace9723280cbe0a00f625b1d0b3949035aac (3m31s build) and ran `--cursor-effects-auto` with MULTI_LAUNCHER_PERF=1. The sandboxed attempt could not run: GetCursorPos returned access denied0x80070005. Authorized desktop escalation succeeded. The16-stage fixture exited0; sanitized export [track_a_cursor_effects_native_g1.json](track_a_cursor_effects_native_g1.json). Raw logs/readbacks remain ignored under target/performance/m5c-native; no desktop images or raw HWND/path logs are committed. Same Windows/MSVC host; Balanced power rechecked. Log completed2026-10-10T03:11:19Z; nominal stage hold1.4s, not an exact tick-rate measurement.

Independent native-log review found all16 requested mode tuples match status, enabled effects Active/disabled effects Disabled, all source/transform/color queries successful and no logged native failure/unavailability. Halo120px/40px and zoom2x160px/1.7x163px source geometry is consistent with fixed-cursor settings. Returned exclusion lists exactly cover enumerated owned cheap surfaces, hosts and outlines (5/6/7/8 entries across configurations), including combined/recreated/ring phases. The stationary repeat retains source/filter geometry while samples increase416 to460 and full renders stay7. Disable removes effect HWNDs and re-enable creates fresh sessions. Final shutdown Ok:1sampler/1backend,851samples/15fullrenders,1backend shutdown,0remaining owned windows.

PASS means native API/source/exclusion/lifecycle evidence, supplemented by the A/B actual-worker deterministic moving-call comparison. The native fixture does not export per-tick source-dispatch distributions, movement p95, CPU/GPU cost or visible input-to-display latency; these are NOT MEASURED. Visible magnifier composition/recursion/flicker and actual mixed-DPI/monitor transitions are NOT RUN; BitBlt readbacks can omit magnifier composition and are not promoted to visual proof. Native failure injection remains covered by focused fake-operation runtime tests, not this real run. No blanket GPU/CPU or latency speedup is claimed. Independent critical B source and C native-evidence review have no unresolved findings. Native software execution capability is available outside the desktop-restricted sandbox, enabling the next measurement stage. No source change or push in C.

M5-C local commit: `f7049863d6ebe8968b04d4002441d936fc345a27`. M6-A is now in progress: opt-in bounded native phase/HUD/GDI profiling and an existing-binary CLI route. Real profile recording follows its source checkpoint; neither conditional optimization has been authorized by evidence yet. No cache, GDI retention or cadence change is part of this measurement stage.
