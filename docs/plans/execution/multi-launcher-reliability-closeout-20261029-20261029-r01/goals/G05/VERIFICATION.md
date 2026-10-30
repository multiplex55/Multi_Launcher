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

## M1-B1 focused verification

B1 source base is `1c5b3b88acc2c285a08b00d3aeb6406bbe08c045`; authoritative parent integration `a6f862a4`. Prior M1-A evidence above remains historical and intact. One selected preparation and one run used `-p multi_launcher --lib --cargo-profile fast-dev --offline --locked`. Exact commands, full filter, tool versions, timestamps, caps, process IDs, exits and source/manifest blobs are in [list.receipt.json](roles/implementer/verification/m1b1/list.receipt.json) and [run.receipt.json](roles/implementer/verification/m1b1/run.receipt.json). Run flags were `--retries 0 --no-fail-fast --status-level pass --final-status-level fail`; no broad target, build, lint, installation or native probe was run.

The exact filter is:

```text
test(hotfolder_condition_literal_) | test(hotfolder_condition_metadata_) | test(=hotfolder::model::tests::metadata_conditions_use_inclusive_bounds_and_one_captured_time) | test(=hotfolder::model::tests::modified_age_bounds_preserve_subsecond_precision) | test(=hotfolder::model::tests::file_conditions_validate_ranges_bound_utf8_and_defer_content) | test(=hotfolder::capability::tests::enabled_condition_rules_fail_closed_until_the_shared_evaluator_is_installed)
```

| Phase | Exact start → end (UTC offset shown) | Elapsed / cap | Result |
| --- | --- | --- | --- |
| Selected preparation | 2026-10-29T22:45:38.2350845-04:00 → 22:48:41.5789257-04:00 | 183.339s / 1200s | Exit 0; 20 selected, none ignored |
| Selected run | 2026-10-29T22:49:14.3154679-04:00 → 22:49:17.9523701-04:00 | 3.633s / 180s | Exit 0; **20 passed, 0 failed**, 6447 skipped |

Nextest's test execution elapsed was 0.786s; run ID `d339c4df-e386-48ac-8d8d-ecc47dcbb72d`. The selected inventory contains seven new literal/composition cases, all nine accepted metadata-prefix cases and the four exact named controls. There was no compiler/test failure, retry, correction preparation or correction run; the 600s incremental allowance was unused. Tools: cargo-nextest 0.9.135 (`610eefb88`), rustc 1.99.1 (`58cf8f9c3`), Cargo 1.99.1 (`5f94df478`), host `x86_64-pc-windows-msvc`.

[Selected inventory](roles/implementer/verification/m1b1/selected-inventory.json) retains only the 20 names and counts. Raw unselected JSON remains Git-ignored at `target/closeout-g05-m1b1-20261029-r01/list.stdout.json`; SHA-256 `0CB8F364CA117A0B6E44D907AA87A561BAB7C456A127BA0349BAD5B296CDD412`. [The run log](roles/implementer/verification/m1b1/run.stderr.log) retains each pass and summary. Warnings are the same unchanged vendor/table owners recorded for M1-A: epaint, egui, image, table-core scanner/stream fields and `table_utility_session.rs:3521`; none arose in the edited B1 files.

Final tested Git blobs are `conditions.rs` **507beafd2b79df82c5a2bd1923fe179938cf70e0**, `model.rs` **306d2cb9b4295e0e0a26cd38a9f782113294df76**, `mod.rs` **1bac86b002a8b90eaf0ec9855b757006ad43f38d**. Accepted execution blob **4de797b4626218d13dcc1e080c24276f8350d7f3**, runtime/capability/store, native file operations, GUI owners, manifest and lockfile remain unchanged from HEAD. Source hashes were checked before the run and again in [the static/cleanup receipt](roles/implementer/verification/m1b1/static-cleanup.receipt.json).

Proof scope: deterministic in-memory chunk streams and injected monotonic/cancellation observations. Tests simulate explicit EOF, read failure and final source change; they do not claim real filesystem EOF, native identity, blocked-read interruption, handle retirement or effect ownership. The 8 MiB case shares one scan and byte count across three literals; exact/one-over bounds, invalid input after hits, final cancellation and sticky failure are asserted. Common composition preserves `Invalid > Changed > Unavailable > NoMatch > Match`, the accepted metadata API and configured-condition refusal. Existing execution metadata tests use their unchanged owned temporary store/journal fixtures; no application or native fixture was added.

Scoped `rustfmt --edition 2024 src/hotfolder/conditions.rs src/hotfolder/model.rs`, `git diff --check`, cumulative diff inspection, changed-path allowlist and local-link checks pass. Guard search confirms `FILE_CONDITIONS_NOT_EVALUATED` remains before public scope evaluation and in the unchanged capability owner. The new module contains no filesystem/Read implementation, path admission, worker spawn or action call. Both owned Cargo processes exited and drained output; final process snapshot has no remaining Cargo/rustc/Nextest/Multi Launcher test process. In-memory resources and unchanged test-owned `TempDir` fixtures completed with their test processes; no manual temporary resource or external application was created or removed.

Disposition: **M1-B1 implementation and focused proof ready for parent checkpoint/review; no whole-M1-B or G05 acceptance.** All condition gates remain closed. B2 native acquisition, cancellation/retirement, source-through-effect and actual queued-start binding remain required by the existing contract; no B2 source was implemented. Distinct follow-on `NO_ACTION`.

## M1-B1 R1 correction verification

Independent review at `bab30968` withheld B1 acceptance for [R1 terminal byte accounting](roles/reviewer/M1-B1_REVIEW.md). The original 20/20 evidence above is preserved; it missed genuine EOF after a supplied chunk was refused before counting. This was a source-backed counterexample, not a failed Cargo run or a native defect. Correction base `6cbb6e43f8655b62e3cd29b836a24ba9335ef0b4`, authoritative integration `c385ca55`.

Only `conditions.rs` accounting/finalization and its direct tests changed. `received_bytes` includes every supplied chunk before parser/cancellation/deadline/size refusal, with saturating arithmetic. Sticky parser refusal still prevents completion; actual overrun can strengthen a prior Unavailable to Changed. Genuine EOF compares received length with admitted and final lengths, preserving Changed for real short/long streams or changed source. The existing extra-byte assertion now requires Changed for the actual overrun, with a new empty-feed assertion retaining its original sticky-resource-refusal coverage. Two both-mode regression tables assert exact categories and absence of completed evidence after resource/entry-cancel/deadline EOF and competing real source changes. No failure was weakened to a generic non-Match assertion.

One incremental preparation and one corrected run used the **same B1 filter**, now selecting **22 tests**: all original 20 plus the two `hotfolder_condition_literal_received_eof_` regressions. No ignored tests, retries, compiler feedback correction or additional test run. Commands remain `cargo nextest list/run -p multi_launcher --lib --cargo-profile fast-dev --offline --locked --color never -E <exact B1 expression>`; list uses `--message-format json`, run uses `--retries 0 --no-fail-fast --status-level pass --final-status-level fail`. The full commands and exact source/toolchain identities are in [list.receipt.json](roles/implementer/verification/m1b1-r1/list.receipt.json) and [run.receipt.json](roles/implementer/verification/m1b1-r1/run.receipt.json).

| Phase | Exact start → end (UTC offset shown) | Elapsed / cap | Result |
| --- | --- | --- | --- |
| R1 correction preparation | 2026-10-29T23:04:03.6672721-04:00 → 23:05:39.6122517-04:00 | 95.941s / 600s aggregate incremental | Exit 0; 22 selected, none ignored |
| R1 corrected run | 2026-10-29T23:06:06.8447125-04:00 → 23:06:10.7574188-04:00 | 3.909s / 180s | Exit 0; **22 passed, 0 failed**, 6447 skipped |

Nextest test execution 0.779s, run ID `a4760c9a-a32a-4e09-a3a3-10963d9775dd`. Incremental preparation consumed 95.941s total; no cap renewal. Toolchain unchanged: cargo-nextest 0.9.135 (`610eefb88`), rustc 1.99.1 (`58cf8f9c3`), Cargo 1.99.1 (`5f94df478`), `x86_64-pc-windows-msvc`. No compiler/test error occurred in this correction; logs preserve the same unchanged vendor/table warnings, with no warning in the edited scanner.

Final scanner Git blob **0559e8f9cc77430b210ab7949967ab8582cd6c99**. All other twelve recorded source/manifest blobs remain identical to HEAD, including model/mod, accepted execution, runtime/capability/store, native/GUI and Cargo files. [Selected inventory](roles/implementer/verification/m1b1-r1/selected-inventory.json) records all 22 names and original-20 retention; raw unselected inventory is Git-ignored at `target/closeout-g05-m1b1-r1-20261029-r01/list.stdout.json`, SHA-256 `128481CD339AEFA3A0F9A26A650C2D43FBED3319ECBA78A5510E008B6C832E64`. [Run log](roles/implementer/verification/m1b1-r1/run.stderr.log) contains every pass and summary.

Scoped rustfmt, cumulative correction diff inspection, `git diff --check`, changed-path allowlist, source hashes, retained guard search and local links pass. [Static/cleanup receipt](roles/implementer/verification/m1b1-r1/static-cleanup.receipt.json) records these and both owned process exits/output drains; no Cargo/rustc/Nextest/Multi Launcher test process remains. New regressions use only in-memory chunks and fake observations; unchanged metadata tests' owned temporary store/journal fixtures finished with their test processes. No native/app/manual fixture or external resource was created, and no unrelated temporary path was removed.

Disposition: **R1 correction implemented and focused proof passed; parent checkpoint and R1-only reviewer confirmation pending.** Reviewer contingency files remain untouched for parent/reviewer disposition. No new follow-on queue, B2 work, Git mutation, guard removal or native/G05 acceptance. Original M1-A acceptance is unaffected.
