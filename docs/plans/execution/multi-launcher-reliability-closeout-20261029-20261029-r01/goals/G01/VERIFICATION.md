# Verification

Latest focused result after G01-R1 correction: 32/32 selected tests passed; named reviewer confirmation and parent acceptance remain pending. The earlier domain-target compile error remains documented below. No native race claims. Source-checkpoint entries retain their historical status and do not imply acceptance.

## M1-A source checkpoint — verification PENDING

The parent approved one combined G01 focused preparation/run after M1-B. No Cargo, Nextest, compiler, ACL or native experiment was run for M1-A. This source checkpoint is not accepted implementation evidence until that batch and review succeed.

Six inline tests authored under `diff::file_ops::tests`:

- `destination_not_found_allows_new_copy`: injected absence reaches the actual executor and copies to an absent destination.
- `destination_read_errors_prevent_new_copy`: both PermissionDenied and InvalidInput return the exact relative path and contextual destination error; destination remains absent.
- `destination_read_errors_prevent_overwrite`: both read errors retain their context and preserve the existing destination bytes.
- `destination_missing_after_preview_prevents_overwrite`: expected existing destination plus injected NotFound refuses to copy, preserving the copyable destination.
- `destination_changed_identity_prevents_overwrite`: a real length-changing edit after preview refuses overwrite through the public executor.
- `destination_matching_identity_allows_overwrite`: unchanged existing destination is overwritten through the public executor.

Failure assertions also check no affected subtree and no cancellation claim. Injected-read counts/path assertions ensure the tested observations occur at the destination boundary while root/source validation stays real. Fixtures use only task-created temporary directories and ordinary bytes; no application is constructed. Existing domain tests remain unchanged and are included in the planner's combined selection.

Authored test count: 6; executed test count: 0 (`NOT_RUN`). Static diff review and `git diff --check` passed during implementation. Final scoped static checks are recorded in the implementer role record. Source checkpoint acceptance, test inventory, combined run and independent review remain pending.

## M1-B source checkpoint — verification PENDING

Starting HEAD `4d21b035a919df3b3627c7eea50a2dacb987a1f1`. No Cargo, Nextest, compiler or native automation ran for this checkpoint. Parent will commit the coherent source checkpoint before dispatching the combined M2-A preparation/run.

Eleven new `gui::diff_dialog::tests::folder_operation_` tests were authored:

- `mixed_results_keep_exact_counts_and_bounded_failures`: complete/failed counts, exact first failure, 20-detail cap and omitted count, no duplicate adoption, unrelated workspace error preservation.
- `cancellation_counts_only_delivered_successes`: cancellation with zero/prior successes and explicit Cancelled, without inventing unattempted work.
- `full_success_uses_neutral_completed_summary`: summary counts work without assuming copy versus recycle.
- `rejects_wrong_generation_view_and_ordered_roots`: report/dispatch generation, view ID, swapped roots and changed root token reject before completion rescan.
- `feedback_survives_rescan_preview_cancel_and_unchanged_parent`: independent comparison identity retains feedback through cleared scan identities and navigation.
- `new_dispatch_clears_previous_feedback_and_replaces_it_on_completion`: real tiny copy through preview dispatch; a test-only two-second receive waits for its event before temp-directory cleanup, then uses the production adoption seam.
- `stale_preview_cannot_dispatch_after_root_change`: no mutation of either old or new destination.
- `discard_and_same_id_side_swap_clear_feedback`.
- `retained_parent_swap_clears_feedback_while_child_is_open`.
- `changed_retained_parent_rejects_queued_completion_before_reconciliation`.
- `reconciliation_drops_changed_retained_comparison`.

The existing `principal_folder_workflow_uses_injected_controller_boundaries` now routes its actual execution report through `process_operation_events` and asserts retained completion feedback. The preview layout fixture was migrated to the new context field. No LauncherApp or native trash is used by the authored tests.

Static checks on 2026-10-29 around 19:41 America/New_York: `git diff --check` passed; `rustfmt --edition 2024 --check --config skip_children=true src/diff/folder_runtime.rs src/gui/diff_dialog/mod.rs src/gui/diff_dialog/operation_preview.rs` passed after scoped formatting. The first format check returned exit 1 for formatting differences; the same three files were formatted and checked again. No parse errors. Scoped diff/path review and old direct-handle/preview-ID reference search found the intended migration only. Informational line-ending conversion notices were retained without content changes.

New authored tests this checkpoint: 11; existing tests migrated: principal workflow and preview context fixture. Executed tests: 0 (`NOT_RUN`). Static evidence does not establish compilation, behavior, native safety or acceptance. The planner's existing combined selection includes all new tests by prefix; nonzero inventory and actual results are still pending.

## M2-A — focused verification result

The combined selection passed: **30 tests, 30 passed, 0 failed**, with no retries or source corrections. This supplies deterministic test evidence for the two source checkpoints; independent review and parent acceptance remain pending. Earlier PENDING/NOT_RUN entries above describe their source-checkpoint times.

Source throughout preparation and execution: `8d183f663b212f8a707ac16a4c056eef5fe641ff` on `codex/closeout-g01-diff-failure-information-20261029-r01`. Tool versions observed: cargo-nextest `0.9.135 (610eefb88 2026-05-14)` on `x86_64-pc-windows-msvc`; rustc `1.99.1 (58cf8f9c3 2026-10-13)`; cargo `1.99.1 (5f94df478 2026-08-27)`. Existing `fast-dev` cache, `--offline --locked`; no installation, manifest/configuration change, broad suite, lint or native test.

### Commands, caps and results

Exact commands including the full expression are preserved in the linked JSON receipts. All timestamps below are 2026-10-29 America/New_York (`-04:00`); elapsed is command wall time, including cached Cargo preparation.

| Phase | Start / end | Cap / elapsed | Result |
| --- | --- | --- | --- |
| [Initial list/preparation](roles/implementer/verification/list.receipt.json): `cargo nextest list -p multi_launcher --cargo-profile fast-dev --lib --test domain --offline --locked --color never -E <recorded expression> --message-format json` | 19:44:21.857 / 19:50:18.188 | 1200 s / 356.327 s | Exit 101, no inventory; E0308 below |
| [One approved incremental list](roles/implementer/verification/recovery-list.receipt.json): same command with `--test closeout_g01_diff_ops` | 19:52:00.352 / 19:52:09.631 | 600 s / 9.278 s | Exit 0; 30 selected, none ignored |
| [Single focused run](roles/implementer/verification/run.receipt.json): `cargo nextest run` with the same recovered targets/expression and `--retries 0 --no-fail-fast --status-level pass --final-status-level all` | 19:54:16.619 / 19:54:19.844 | 180 s / 3.221 s | Exit 0; Nextest execution 0.289 s, 30 passed, 6445 filtered out |

Each process was started hidden with redirected output, a wall-clock cap and task-owned process-tree termination on timeout. None timed out; all exited and output was drained. No additional Cargo command followed the successful run.

### Initial target limitation and authorized recovery

The first meaningful diagnostic was E0308 at `tests/suites/../domain_cases/file_search_e2e.rs:43:9`: `filenames` passes `&Sender<_>` to `search_filenames_in_directory`, which expects `&SearchEventSender` at `src/file_search/walkdir.rs:52`. The [initial compiler log](roles/implementer/verification/list.stderr.log) retains the diagnostic and final exit. Both files are untouched by G01. No G01 compile error occurred. The monolithic **`domain` target did not compile or pass** in this run.

Parent explicitly approved one incremental preparation using an auto-discovered temporary target, preserving the seven original Diff domain cases. Cargo successfully detected `tests/closeout_g01_diff_ops.rs` without a manifest edit. Its complete bytes are retained in [temporary-target.rs.txt](roles/implementer/verification/temporary-target.rs.txt):

```rust
#[path = "domain_cases/diff_file_ops.rs"]
mod diff_file_ops;
```

Wrapper Git blob: `30d4db3c9e908d5a875c616c443932f35d438365`. Original `tests/domain_cases/diff_file_ops.rs` blob before and after: `a6b2049cc249cc99921e8b293cba11facbba0dcd`. For an explicitly authorized future reproduction from the repository root, require the wrapper path to be absent, then `Copy-Item -LiteralPath 'docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/goals/G01/roles/implementer/verification/temporary-target.rs.txt' -Destination 'tests/closeout_g01_diff_ops.rs'`; verify the recorded hash and use the exact commands in the recovery/run receipts. Remove that task-created wrapper afterward. This is reconstruction guidance, not an additional execution request.

The first diagnostic yielded a separate proposed [Goal](roles/implementer/follow-on/file-search-sender-fixture/GOAL.md) / [Kickstart](roles/implementer/follow-on/file-search-sender-fixture/KICKSTART.md) for the unmigrated File Search fixture sender. It was not implemented here.

### Inventory and evidence

[Selected inventory](roles/implementer/verification/selected-inventory.json) records every exact selected name: 23 library tests (all 6 destination tests, all 11 `folder_operation_` tests, and the 6 named existing controller/runtime tests) plus all 7 unchanged Diff domain tests. Nonzero count and each subgroup count were checked before execution; none were ignored. [Selected run output](roles/implementer/verification/run.selected.stderr.log) retains warnings, one pass line per executed test and the final 30/30 summary.

The complete raw JSON inventory and verbose final status output include thousands of unselected cases. They were moved to ignored `target/closeout-g01-m2a-20261029-r01/`; [raw evidence locations](roles/implementer/verification/raw-evidence-location.json) records exact paths, sizes and SHA-256 hashes. The tracked selection and summary are derived evidence, explicitly excluding repeated final statuses and skipped-case detail.

Warnings are pre-existing and outside G01: nine lifetime-syntax warnings in `vendor/image`; three unused/dead-code warnings in `crates/multi_launcher_table_core`; unused `response`, `slot`, `results` in `src/gui/table_utility_dialog.rs`; unused `replaced_scan_job` in `src/table_utility_session.rs`. No warning-driven source cleanup was performed.

### Cleanup and proof limits

[Cleanup receipt](roles/implementer/verification/cleanup.receipt.json), checked at 19:55:30.841, confirms the temporary wrapper was removed after path/hash verification, its saved copy has the same blob, the original domain fixture is unchanged, no task-owned PID or compiler/runner/test process remains, and `git diff --name-only -- src tests Cargo.toml Cargo.lock` is empty. Successful fixtures own their temporary directories; the actual dispatch test receives its completion before its temporary-directory guard drops. No application or native trash fixture was constructed.

This is focused deterministic executor/controller evidence. It does not establish native identity binding, race freedom, worker retirement, rollback, atomic publication, pixel rendering, a passing full domain target or broader-suite acceptance.

## G01-R1 — pending cancellation retains mutation exclusion

Independent review correctly found that M1-B dropped the active operation at rescan immediately after requesting cancellation. The executor can still be completing an item at that point. The bounded correction retains the active handle for the same comparison; only a terminal report matching that dispatch context and generation releases it. A matching terminal from before the rescan releases the gate without adopting stale feedback or starting a completion rescan. Changed-comparison invalidation remains explicit.

`finish_operation` now checks ownership before releasing the handle and reports whether current-generation feedback was adopted. `process_operation_events` keeps view/ordered-root validation and calls completion rescan only for that adopted current report. Existing injected-report tests now install an active operation, and the wrong-context test also asserts that the gate stays held. The principal workflow retains its real synchronous execution and now establishes dispatch ownership before delivering its report.

Parent approved the eight-line `#[cfg(test)] pub(crate)` constructor `OperationHandle::with_test_channel` in `file_ops.rs`, taking an owned receiver/cancel flag. It adds no production API or behavior. Two new owned-channel tests require no worker scheduling or native effects:

- `folder_operation_rescan_holds_busy_gate_until_matching_terminal_acknowledgement`: same-root refresh requests cancellation while retaining busy state; both planning and a current preview at the dispatcher are blocked; progress cannot release the gate; a matching stale terminal releases it without feedback/model reset/generation advance; planning then succeeds.
- `folder_operation_wrong_terminal_cannot_release_a_newer_active_dispatch`: after the old operation is acknowledged, neither its delayed terminal nor a wrong-generation report releases the new active operation; its own matching current terminal completes normally.

Because the test-only constructor changed the executor source file, parent replaced the proposed smaller library selection with **one** fresh batch of the original 30 plus the two lifecycle cases. This was the sole correction preparation/run, not an extra verification layer. The saved two-line wrapper was recreated only after confirming absence and its exact blob `30d4db3c9e908d5a875c616c443932f35d438365`; the monolithic domain target was not retried.

| Phase | 2026-10-29 America/New_York start / end | Cap / wall elapsed | Result |
| --- | --- | --- | --- |
| [Correction preparation/list](roles/implementer/verification/r1/list.receipt.json) | 20:09:11.865 / 20:11:16.993 | 600 s / 125.124 s | Exit 0, no corrective compile iteration |
| [Single focused run](roles/implementer/verification/r1/run.receipt.json) | 20:11:45.937 / 20:11:48.990 | 180 s / 3.050 s | Exit 0, 32 passed, 0 failed; Nextest execution 0.327 s |

The receipts contain the exact original planner expression, full commands, HEAD and seven source/fixture/configuration blob identities. Commands use `--cargo-profile fast-dev --lib --test closeout_g01_diff_ops --offline --locked`; run uses `--retries 0 --no-fail-fast --status-level pass --final-status-level fail`. Toolchain versions are unchanged from M2-A above. [Selected inventory](roles/implementer/verification/r1/selected-inventory.json) was checked before execution: 32 total, 25 library (6 destination, 13 folder-operation, 6 existing controls), 7 unchanged Diff domain cases, no ignored cases, both new exact names present. [Run output](roles/implementer/verification/r1/run.stderr.log) retains all 32 pass lines and summary; 6445 unselected cases were skipped. No retries, timeout, compiler error, assertion failure or additional corrective run occurred. Only the same unrelated vendor/table warnings appeared; none were changed.

[Cleanup](roles/implementer/verification/r1/cleanup.receipt.json) at 20:12:09.284 confirms wrapper removal, identical prepared/run/final source blobs, unchanged original domain test/manifest/lockfile, and no owned or compiler/runner/test processes remaining. The complete inventory stays under ignored `target/closeout-g01-r1-20261029-r01/list.stdout.json`, with its size/hash in that receipt; selected-only inventory is retained here. The new channel fixtures spawn no workers and own their temporary files. Scoped formatting and diff inspection passed before preparation; final static receipt follows in the same evidence directory.

This closes the implementer's reproduction and focused proof for G01-R1; it is not reviewer confirmation or whole-goal acceptance. No join, worker-retirement mechanism, delivery change, native overlap claim or broader cancellation guarantee was added. Parent owns the reviewer contingency disposition; the File Search fixture proposal remains unexecuted.
