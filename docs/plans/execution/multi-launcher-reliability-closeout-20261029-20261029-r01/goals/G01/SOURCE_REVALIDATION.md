# Source revalidation

Source-supported two-boundary Diff correction to be revalidated against actual branch. Complete Goal/Kickstart read by parent. No historical patch import. Historical execution receipts absent; current source authoritative.

## M1-A implementer observation

- Revalidated 2026-10-29 on `codex/closeout-g01-diff-failure-information-20261029-r01`, HEAD `269508d431f81470f9db784e06c93b71749feeaa`. Parent-recorded master is `4a1283e6edd2c245f562a07dbe3dcc6a61061eed`; bootstrap is `02d0ac64d51655f43d6e958ea551f5bf658ee48d`.
- Baseline blob `src/diff/file_ops.rs`: `b3e181a7b5bffeb20e221314c4e775e494a31990`. Baseline blob `tests/domain_cases/diff_file_ops.rs`: `a6b2049cc249cc99921e8b293cba11facbba0dcd`.
- The current `fresh` implementation used `FileIdentity::read(&dst).ok()`. `add_file` already handled only `NotFound` as absence. `execute_copy` called `fresh` immediately before `fs::copy`, converting validation errors to `ItemOutcome::Failed`.
- Existing ownership is sufficient: the private freshness function classifies the destination observation; the existing executor reports failure and skips that copy. The public `execute_copy` signature remains unchanged and passes the real `FileIdentity::read` into a private destination-only seam. Source/root reads remain real and unchanged.
- Read full G01 Goal/Kickstart, current implementer configuration, approved planner packet, relevant planning/containment/freshness/execution code and all seven existing domain file-operation tests. Manifest identifies package `multi_launcher`, edition 2024, library tests and `domain` at `tests/suites/domain.rs`, plus the existing `fast-dev` profile.
- Initial worktree changes were only parent role documentation. Those files were preserved. This checkpoint changes no GUI, manifest, lockfile, configuration, historical report or public API.

## M1-B implementer observation

- Starting HEAD `4d21b035a919df3b3627c7eea50a2dacb987a1f1` on the same G01 branch; parent committed/pushed M1-A before this checkpoint. M1-A remains pending combined verification, not integrated.
- Baseline blobs: `src/diff/folder_runtime.rs` = `2ee63dbc4ec71a353d7c14b0b3ea482685e11a2b`; `src/gui/diff_dialog/mod.rs` = `9ee972b03accb215f8e6780756e86413ce9a833e`; `src/gui/diff_dialog/operation_preview.rs` = `14df2060ce0ae81df85ceac20294debd4f3e003a`.
- `poll_folder_runtime` consumed reports using generation alone before root reconciliation, dropped item/cancellation details and immediately rescanned. `prepare_rescan` cleared scan root fields. `DiffWorkspace::swap_sides` also swaps a retained folder parent while a file child is current, but dialog runtime cleanup previously covered only the current folder ID.
- Existing `FolderRuntime` now owns immutable dispatch context and transient bounded feedback. Ordered `RootIdentity` values plus view ID identify the retained comparison independently of scan fields. Preview context captures that identity plus generation, including both roots for delete plans.
- Current-view context is checked before dispatch and before report adoption/completion rescan. Retained runtime reconciliation compares folder identities, and explicit side swaps invalidate both current and affected retained-parent runtime resources. Current-view errors remain separate from workspace errors.
- Production adoption and tests share `process_operation_events`; the principal folder workflow was migrated from calling `refresh_after_mutation` directly. No worker-delivery, trash, persisted state, manifest or M1-A source changes.

## M2-A tested source identity

- Preparation and run used HEAD `8d183f663b212f8a707ac16a4c056eef5fe641ff`, containing parent-committed M1-A `4d21b035a919df3b3627c7eea50a2dacb987a1f1` and M1-B. No source correction was required.
- Tested blobs: `src/diff/file_ops.rs` = `dbe19867ae8d9c79bafd1a6f0c1cd88a34e69317`; `src/diff/folder_runtime.rs` = `bb37bcdb143b5cdeedd6f4200de891b1a241ad70`; `src/gui/diff_dialog/mod.rs` = `86af59fae63fc5168d36e896b78e8a72f8fb53dc`; `src/gui/diff_dialog/operation_preview.rs` = `a1d93a6951a0e335768ccbc3a7e39b6438440db8`.
- Unchanged Cargo blobs: manifest `994a55fea039ca79614fbb019ac1b05cfab7381b`; lockfile `648e143ad2ca902e04ae9b7d4cf57899a2db8380`. Unchanged Diff domain fixture `a6b2049cc249cc99921e8b293cba11facbba0dcd`.
- Parent-authorized temporary target included that exact Diff fixture after an unrelated File Search caller prevented `domain` compilation. The wrapper was auto-discovered without manifest edits, verified and removed after 30/30 focused results. See VERIFICATION.md and the cleanup receipt. Source worktree diff was empty afterward; parent-owned role documentation was preserved.

## G01-R1 correction source identity

- Starting HEAD `b09ecdcacfd97b5456692aab226b6a886f2c2164`; reviewer identified the immediate handle drop in `prepare_rescan`. Current caller inspection confirmed Refresh, rules and watcher rescans use this owner, while planning and dispatch use `mutation_active` for exclusion. Cancellation only requests stopping; it does not acknowledge the current effect's completion.
- Corrected blobs, unchanged throughout preparation/run/cleanup: `src/diff/file_ops.rs` = `7e9fab3d4eb09a735f4c347ab86e0832fd3f8a66` (test-only constructor); `src/diff/folder_runtime.rs` = `e7cac72dbc89977556a475045d7ab0410b5c59e3`; `src/gui/diff_dialog/mod.rs` = `ecfd6e76f38cbcf16c5b914b610cc47d1fa61fa8`. Preview, original domain fixture, manifest and lockfile retain the M2-A blobs above.
- Runtime retains the handle during same-comparison rescan cancellation. Terminal acknowledgement checks the stored immutable context and report generation before releasing the handle, and current-generation feedback remains separate from a stale acknowledgement. Changed roots/view still explicitly invalidate the old comparison. No executor production code, worker-delivery behavior or native cancellation mechanism changed.
- Two channel lifecycle cases plus all original selected tests passed (32/32). Original fixtures were not edited; the exact temporary wrapper was removed. Parent/reviewer documentation preserved. Current corrected source still requires named review confirmation and parent commit/acceptance.
