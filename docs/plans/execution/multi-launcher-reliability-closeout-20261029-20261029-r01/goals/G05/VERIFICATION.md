# G05 verification

Baseline docs/source refs only. Cargo/native NOT_RUN. Planner must select nonzero narrow tests matched to changed owner, existing fast-dev/offline/locked library targets, budget1200s initial preparation/180s built execution/600s necessary incremental correction with1-2localized reruns. No full domain (known unrelated FileSearch fixture mismatch), full suite/build/install/broad lint, real unattended user-folder actions. App test profiles disable next_hotkey/cancel_hotkey before construction.

## M1-A focused verification

The baseline paragraph above records the earlier state. M1-A uses only `multi_launcher --lib`, existing `fast-dev` cache, `--offline --locked`, the nine new `hotfolder_condition_metadata_` tests and ten exact controls from [the planner handoff](roles/planner/M1-A_HANDOFF.md). No full domain target, broad suite, separate build/check/lint, dependency installation, application launch or native action was run.

Preparation command and full expression are preserved verbatim in [list.receipt.json](roles/implementer/verification/m1a/list.receipt.json). Execution uses that exact selection with `cargo nextest run` and `--retries 0 --no-fail-fast --status-level pass --final-status-level fail`, recorded verbatim in [run.receipt.json](roles/implementer/verification/m1a/run.receipt.json). Nextest is 0.9.135 (`610eefb88`), rustc 1.99.1 (`58cf8f9c3`), Cargo 1.99.1 (`5f94df478`), target host `x86_64-pc-windows-msvc`. Source HEAD is `c009b533059acb9404f453484b053363dd1d51af`; each receipt binds exact source Git blobs and toolchain to its command.

| Phase | Exact start → end (UTC offset shown) | Elapsed / cap | Exit and result |
| --- | --- | --- | --- |
| Initial selected preparation | 2026-10-29T22:00:41.7485543-04:00 → 22:07:03.1110772-04:00 | 381.358s / 1200s | 0; 19 selected, 9 new + 10 controls, none ignored |
| Initial selected run | 2026-10-29T22:07:37.2785461-04:00 → 22:07:41.9296389-04:00 | 4.648s / 180s | 100; 15 passed, 4 failed; Nextest execution 1.815s |
| Corrected selected preparation | 2026-10-29T22:09:13.3523413-04:00 → 22:10:42.1803694-04:00 | 88.824s / 600s aggregate incremental allowance | 0; same 19 selected, none ignored |
| Corrected selected run | 2026-10-29T22:11:06.4222897-04:00 → 22:11:11.1678096-04:00 | 4.742s / 180s | 0; 19 passed, 0 failed; Nextest execution 1.601s |

[Selected inventory](roles/implementer/verification/m1a/selected-inventory.json) contains only the 19 selected names and records the ignored full inventory's path/hash. Raw unselected inventory remains under `target/closeout-g05-m1a-20261029-r01/list.stdout.json`, confirmed ignored by Git; SHA-256 `46E3063B4EA975B2DC35CB9E189A34EE633CA66E42E2DDD30DBF35CC3A897D69`. No test was ignored or retried automatically.

### First failures and bounded fixture correction

No compiler error occurred. The initial [run log](roles/implementer/verification/m1a/run.stderr.log) preserves all four failures:

- `binding_refuses_every_stale_evidence_field`: assertion at initial execution.rs:3207, `candidate field 10`; adding 1ns did not make the candidate timestamp distinct on this Windows target.
- `actual_lease_transfer_cannot_reuse_old_match`: assertion at initial execution.rs:3346; the workerless `with_test_ready_candidate` fixture has no worker, and `pause()` returns before clearing its active lease. This was an incorrect fixture assumption, not a production pause finding.
- `inclusive_bounds_keep_fractional_age`: initial model.rs:956, `size=10, age=10.000000001s`, observed Match when NoMatch was expected; the 1ns subtraction was not represented by the constructed `SystemTime`.
- `precedence_and_optional_modification_time`: initial model.rs:987, observed event NoMatch instead of FutureModificationTime; the +1ns future fixture likewise did not create a future timestamp.

One localized fixture correction replaces those increments with representable 1-microsecond differences and adds exact round-trip/distinct/future assertions, preserving fractional-age rejection and stale-evidence rejection. The transfer test now sends an actual lease to another `HotFolderWatchService`, asserts the real `finish_lease` result is `WrongServiceLease`, and passes that result through the production-used validator. It does not manufacture an error result to stand in for transfer or bypass a production eligibility guard. No production logic changed for this correction.

The parent approved one corrected preparation with an aggregate 600s cap and one rerun of the same 19 selected tests, so final-file evidence covers the required controls as well as the changed fixtures. The original failed evidence remains intact. Corrected source blobs are model `b82c6849e03f90fdcd8762843ddf438c4b737f0d` and execution `4de797b4626218d13dcc1e080c24276f8350d7f3`; the exact correction commands and timing are in [correction-list.receipt.json](roles/implementer/verification/m1a/correction-list.receipt.json) and [correction-run.receipt.json](roles/implementer/verification/m1a/correction-run.receipt.json). **All 19 tests passed on those exact corrected source files.** This is a complete corrected batch, not a combination of passing tests from different source blobs. Incremental preparation used 88.824s of 600s; one localized correction rerun was used; no cap was reached or renewed.

[Corrected selected inventory](roles/implementer/verification/m1a/correction-selected-inventory.json) verifies the same 19 names and none ignored. Its full unselected dump is ignored at `target/closeout-g05-m1a-20261029-r01/correction-list.stdout.json`; SHA-256 `46E3063B4EA975B2DC35CB9E189A34EE633CA66E42E2DDD30DBF35CC3A897D69`. [The corrected run log](roles/implementer/verification/m1a/correction-run.stderr.log) preserves each pass and the final summary (run ID `38a75cbc-325e-40b3-96ad-403d036f3501`). No more Cargo execution followed this pass.

### Ownership and proof limits

Warnings originate in unchanged vendored `epaint` (unused variant), `egui` (deprecated EPSILON), `image` (nine lifetime spelling warnings), table core (two unused scanner items and `StreamRecord.end_offset`) and `src/table_utility_session.rs:3521` (`replaced_scan_job`). No warning was reported in the edited Hotfolder files. These warning owners are outside M1-A and were not modified.

The new tests use deterministic fake operation ports/clocks, actual owned watcher lease APIs and owned temporary rule-store/journal fixtures. They prove metadata precedence, full representable fractional-age comparisons, evidence and rule/control rejection, post-fingerprint/post-transfer admission, one captured wall time, and retained configured-condition refusal. They do not run native file actions or establish native handle lifetime/race/queued-through-effect claims. No `LauncherApp` or native watcher was constructed. Native/content/editor/Test Rule proof remains later G05 work.

### Final static checks and cleanup

Scoped `rustfmt --edition 2024 src/hotfolder/model.rs src/hotfolder/execution.rs` and `git diff --check` passed. Cumulative source/test diff was inspected. The explicit guard search `rg -n 'FILE_CONDITIONS_NOT_EVALUATED|conditions.is_configured\(' src/hotfolder/model.rs src/hotfolder/capability.rs src/hotfolder/runtime.rs src/hotfolder/execution.rs` confirms the model public guard and unchanged capability refusal; no condition-clearing bypass was added. Runtime/capability/store/mod/manifest/lockfile blobs in the final receipt match the preparation baseline. Only the two approved source files and own G05 receipts/role record changed.

[Static and cleanup receipt](roles/implementer/verification/m1a/static-cleanup.receipt.json) records source hashes matching the corrected run, local link checks and no remaining owned process. All four owned preparation/run processes exited with output drained; the final snapshot contains no Cargo, rustc, Nextest or Multi Launcher test process. Test-owned `TempDir` store/journal fixtures completed and dropped with the test processes; no separate manual/native fixture, application process or installed resource was created. No unrelated temporary paths were removed. Both raw inventories are Git-ignored, and only selected inventories are included in goal evidence.

Disposition: **M1-A source/test checkpoint verified within scope; parent checkpoint and independent acceptance pending. G05 is not accepted and configured-condition execution remains disabled.** No distinct follow-on finding (`NO_ACTION`); no M1-B work was started.
