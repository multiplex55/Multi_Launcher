# G09 M1 — Prove the two existing retirement boundaries

Status: executable bounded handoff, no implementation or test execution by Planner. Read [OWNER_PROOF_CONTRACT.md](OWNER_PROOF_CONTRACT.md) first. Parent must persist/commit the M0 contract before the sole writer starts.

## Objective and ownership

Produce one coherent checkpoint proving the retained Duplicate Finder worker and cancellable Screenshot capture worker cannot be mistaken for retired merely because cancellation, terminal publication or owner close occurred. Exercise one actual Windows file-handle case through Duplicate Finder's existing native reader. Prefer current passing production behavior; there is no change quota.

Use the existing session/capture owners for admission, the existing reaper for join proof, and Activity only for observation. No architectural migration is planned. Production corrections require a concrete counterexample and the bounded M1-B decision; do not speculate one into this proof checkpoint.

## Exact scope and dependencies

Primary test scope: `src/duplicate_finder/tests.rs`, inline tests in `src/thread_reaper.rs`, `src/gui/screenshot_capture.rs`, `src/activity.rs`, `src/gui/activity_dialog.rs`. A narrowly scoped `#[cfg(test)]` observation/synchronization seam in those same owners or `src/duplicate_finder/{session,content}.rs` is allowed only if existing hooks cannot express an asserted ordering. It must not replace the actual join, owner admission or native file acquisition with a fake success.

Read-only contextual owners: `src/file_operations.rs::DuplicateDiscoveryContentFile` / `open_duplicate_discovery_content_file`; `src/gui/{duplicate_finder_dialog,command_host,render}.rs`. No production file-operations, GUI scheduling, reaper capacity or File Search changes are preapproved by this packet. Keep existing constructors/defaults and callers intact. No new dependency or full-app fixture.

Dependencies: source/relevant manifest identity matches the supplied G09 baseline, M0 committed, no concurrent writer, and ordinary Windows library tests are supported. Source tests are executable independently of the synthetic native result; report native failure separately if it occurs.

## Ordered work

1. Reconcile the existing tests below with the contract. Reuse them rather than adding a parallel fixture. Strengthen only missing assertions/synchronization. New test names are implementer choices; record the final nonzero inventory.
2. **Reaper receipt:** extend `completion_before_reap_is_latched_even_while_join_is_not_finished` to use `reap_with_callback` and a receipt channel. While the worker is deliberately held after notifier drop, assert no callback and no successful-join probe; release it and require the real after-join callback. Keep the callback outside locks. Preserve the existing capacity/hung-worker and supervisor-spawn-failure semantics; in the failure case no callback may falsely assert join. Use bounded channel waits and release-on-unwind.
3. **Duplicate Finder retained owner:** extend/reuse the cancel and coalesced-close tests. Enter the gated job, cancel, verify `WorkerRetiring` start refusal without generation/call-count change, then close while still held. Repeated reopen attempts must remain `WorkerRetiring`; capture `reaper_probe_for_test` before transfer. Release, require successful `reopen` plus actual reaped receipt, and assert Closed/Idle state never accepted the old result. Start one fresh generation and complete it; its identity/results must be current. Do not mistake `worker_reaped_for_test()` error states for successful join. Preserve the existing capacity-refusal test proving no job starts without a permit.
4. **Screenshot cancellable owner:** reuse the active-cancel and shutdown tests with injected one-pixel images. Prove direct cancel keeps Draining/busy and rejects scheduling while the worker is held, then returns no stale completion after release/join. Independently prove shutdown transfers the blocked worker, cannot schedule again, and does not claim retirement until the captured reaper probe is true. Preserve normal `tick` confirmation and exact Activity cancellation capability tests. An Activity cancel callback sets the flag; do not invent an immediate Draining UI requirement for that separate route.
5. **Activity/UI fences:** run the existing exact-key, late-retirement-after-clear and visible/throttled-safe-copy tests named below; source-confirm unchanged live-operation and omission-bound behavior. Do not force the Duplicate Finder retained loop or screenshot shutdown row from `Unavailable` to `Confirmed`. Add a combined assertion only if needed to cover a real missing connection; no full UI construction or screenshot testing.
6. **Single native handle fixture:** strengthen `activity_baseline_native_file_scan_cancel_and_warm_reopen` with the pre-read gate/exclusive-open protocol in the contract. Use existing actual reader plus Windows standard-library sharing observation, not new FFI or an OS-policy change. Baseline probes must succeed; the held source must specifically refuse with error 32; cancel/close must retain admission; successful join/reopen must precede successful exclusive probes and warm restart. Keep all probes short-lived. An unrelated access error is FAIL/INCONCLUSIVE, not proof of a held handle. Never call this an in-flight native read cancellation test.
7. Inspect the scoped diff and record actual evidence by owner. Existing production logic can remain unchanged. Do not relabel unexecuted native categories, old PR claims or test-name presence as acceptance.

## Existing selected tests

All are current `multi_launcher` library tests (source names below are not execution receipts):

- `thread_reaper::tests::completion_before_reap_is_latched_even_while_join_is_not_finished`
- `thread_reaper::tests::supervisor_spawn_failure_detaches_without_panicking_and_retains_capacity_until_exit`
- `duplicate_finder::tests::dff_lifecycle_cancel_invalidates_publication_while_io_drains`
- `duplicate_finder::tests::dff_lifecycle_coalesces_pending_scans_and_reopens_only_after_retirement`
- `duplicate_finder::tests::activity_baseline_native_file_scan_cancel_and_warm_reopen`
- `gui::screenshot_capture::tests::screenshot_delay_active_cancel_drains_and_discards_late_image`
- `gui::screenshot_capture::tests::screenshot_delay_shutdown_hands_blocked_worker_to_reaper_without_waiting`
- `gui::screenshot_capture::tests::screenshot_activity_publishes_actual_capture_before_gui_join`
- `activity::tests::cancel_requires_the_exact_current_owner_capability`
- `activity::tests::history_is_capped_and_clear_fences_late_retirement`
- `gui::activity_dialog::tests::process_memory_sampler_obeys_visible_open_250ms_refresh_gate`
- `gui::activity_dialog::tests::copied_text_contains_only_safe_owner_activity_metadata`

The first reaper ordering test plus two owner cases are the core new proof. Other selected tests protect directly affected invariants; do not expand to the whole modules/application. If test-only strengthening changes a name, update the filter once before preparing, rather than repeatedly building to discover names.

## Verification

Use existing `target/fast-dev`, package `multi_launcher`, library only. Suggested exact existing-name selection (add an exact new case only when created):

```powershell
$g09Tests = 'test(=thread_reaper::tests::completion_before_reap_is_latched_even_while_join_is_not_finished) | test(=thread_reaper::tests::supervisor_spawn_failure_detaches_without_panicking_and_retains_capacity_until_exit) | test(=duplicate_finder::tests::dff_lifecycle_cancel_invalidates_publication_while_io_drains) | test(=duplicate_finder::tests::dff_lifecycle_coalesces_pending_scans_and_reopens_only_after_retirement) | test(=duplicate_finder::tests::activity_baseline_native_file_scan_cancel_and_warm_reopen) | test(=gui::screenshot_capture::tests::screenshot_delay_active_cancel_drains_and_discards_late_image) | test(=gui::screenshot_capture::tests::screenshot_delay_shutdown_hands_blocked_worker_to_reaper_without_waiting) | test(=gui::screenshot_capture::tests::screenshot_activity_publishes_actual_capture_before_gui_join) | test(=activity::tests::cancel_requires_the_exact_current_owner_capability) | test(=activity::tests::history_is_capped_and_clear_fences_late_retirement) | test(=gui::activity_dialog::tests::process_memory_sampler_obeys_visible_open_250ms_refresh_gate) | test(=gui::activity_dialog::tests::copied_text_contains_only_safe_owner_activity_metadata)'
cargo nextest list -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E $g09Tests
cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E $g09Tests --test-threads 1
```

One preparation capped at 1200 seconds; one run capped at 180 seconds after preparation. Sequential test execution avoids unnecessary shared reaper interference. Allow 1–2 localized reruns only for actual errors, with aggregate incremental preparation/correction capped at 600 seconds. Parent owns timeout/process receipts. No full build, full suite, domain target, broad lint, install, clean rebuild or native campaign. If app construction becomes genuinely necessary, disable Paste Sequences next/cancel hotkeys in that test profile first; this packet does not require app construction.

Record command, source/manifest identity, timestamps, cap, exit status, actual selected names/count, first failure, controlled worker release/join and fixture cleanup. A zero inventory or timed-out worker is not PASS. Do not invent guaranteed counts from the planned 12-case list.

## Done, review and stop criteria

- Blocked work, cancellation/publication, actual join/reaper receipt, fresh admission and stale-close rejection have distinct assertions.
- The real synthetic file handle is observably held before release and closed after join, or that exact native row is truthfully unproven with its failure/protocol retained.
- Current informational `Unavailable` rows and visible-only UI remain truthful; existing production paths/defaults are preserved unless a reproduced defect justified the agreed narrow correction.
- Focused nonzero verification and reviewer accept the exact resulting checkpoint. No planning document itself satisfies this criterion.
- Parent can record owner-specific `NO_CHANGE_REVALIDATED`/test-strengthened acceptance and an explicit native scope, with no feature-wide retirement inference.

Genuine uncertainty is execution evidence: this branch has not run the proposed sharing probe or strengthened ordering tests. There is no demonstrated tool/architecture blocker. If the native case cannot establish the specified acquire/hold/release observations within this bounded fixture, stop that native claim, preserve exact error and have the discovering role author its narrow continuation pair. If a production defect appears outside one selected owner correction, report its exact boundary rather than expanding the goal. Do not absorb File Search, apartment, capture-pixel or unrelated goal gaps.
