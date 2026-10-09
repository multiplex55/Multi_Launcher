# Track A runtime results

Status: implementation in progress. No optimization checkpoints have completed, and no before/after speedup has been measured.

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
