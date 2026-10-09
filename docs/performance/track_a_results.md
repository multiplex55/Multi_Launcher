# Track A runtime results

Status: implementation in progress. M1-A/B/C and M2-A/B/C are verified; M3 launcher virtualization follows. The full note comparison below establishes the idle fast path and retains the slower large-draft result.

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
