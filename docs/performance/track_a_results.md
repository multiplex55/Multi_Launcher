# Track A runtime results

Status: implementation in progress. M1-B removes idle note snapshot/hash work; full before/after timing comparison follows in M1-C.

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

Final targeted Nextest run `7b606620-c99f-4e0f-9445-48fe57d94b87`: 21 passed, 5,031 skipped, 0.350s. Small owner run `5fcbe3d9-dbe7-46a2-bdf0-9fcdac410327`: 1 passed, 5,051 skipped, 0.902s. Small idle fixture/output signature `40142a5ec2bdddbe`; its timing is smoke evidence only. Exact selection: `cargo nextest run --lib -E 'test(backlink_) | test(link_menu_) | test(empty_link_menu_targets_are_cached) | test(unchanged_heavy_check_skips_note_snapshot_alias_hash_and_recompute) | test(pending_external_mutation_without_edit_time) | test(todo_revision_change_bypasses_local_edit_debounce) | test(persisted_note_revision_bypasses_local_edit_debounce) | test(changed_note_revision_and_setting_bypass_retry_deadline) | test(failed_heavy_snapshot_retains_rows) | test(new_draft_after_failed_refresh_uses_normal_edit_debounce) | test(edits_do_not_trigger_heavy_recompute_every_frame) | test(programmatic_content_replacement) | test(rendered_checkbox_toggle) | test(save_recomputes_derived_and_updates_links) | test(save_invalidates_backlink_rows_when_slug_changes)'`, with `MULTI_LAUNCHER_PERF=1`.

Local checkpoint commit follows this report; no push will be attempted.
