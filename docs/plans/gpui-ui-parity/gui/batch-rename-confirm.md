# Confirm Batch Rename

G09 owns `batch-rename-confirm`, the frozen-plan decision in the current Batch Renamer viewport. `batch_rename_dialog.rs` stages review and `file_operations.rs` and `batch_rename_transaction.rs` retain the shared preflight and execution boundaries.

| Criterion | Source-backed behavior |
| --- | --- |
| RENAME-APPLY-01 | Open only with an eligible journal and actionable Ready rows. Bind the accepted list to exact canonical root, recursion, scan generation, rules revision and source/target names. Show the review and keep Confirm and apply and Cancel reachable. |
| RENAME-APPLY-02 | Only the current visible retained owner can dispatch. Cancel or close discards authority. A changed draft, root, scan or owner cannot submit the older plan. Jump Mode moves focus without performing the transaction. |
| RENAME-APPLY-03 | Accept once through the existing app-owned coordinator. Preserve exact source/parent/destination preflight and same-folder no-overwrite staging, cancellation outcomes and durable recovery receipts. |

Use `m8d_confirmed_plan_is_frozen_and_draft_edits_revoke_apply_authority`, `batch_rename_detached_confirmation_requires_a_retained_owner_pass`, and the scoped Jump Mode and transaction cases. Native confirmation and execution require a disposable tree. No native confirmation or GPUI implementation is recorded.
