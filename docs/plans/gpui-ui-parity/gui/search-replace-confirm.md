# Search and Replace Confirmations

G09 owns `search-replace-confirm`. Apply, Restore and Recovery use distinct decisions and subjects within the current Search and Replace host. `src/gui/search_replace_dialog.rs` owns the visible review. The current parent and confirmation identity, kind, generation and source/receipt subject determine whether a decision can be accepted. `src/replace_executor.rs` owns I/O and recovery.

| Criterion | Source-backed behavior |
| --- | --- |
| REPLACE-DECISION-01 | Review Apply shows the accepted root, file/match counts, exact selection, generation and revision, source/proposed lengths and hashes and the existing partial-batch and recovery policy. Confirm and Apply freezes a non-writing request on its worker. Cancel before acceptance leaves no write authority. |
| REPLACE-DECISION-02 | Submit once through the accepted current host to the app-owned executor. A stale, busy or refused submission loses its old authority and requires a fresh review. Before writing, the executor revalidates the source, path and bytes and journals backups and partial outcomes. |
| REPLACE-DECISION-03 | Restore uses one Applied or Partial receipt and checks the recorded post-write identities. Changed files are left untouched. Recovery uses one RecoveryRequired receipt and reconciles the recorded interrupted operation. Preserve conflicts and exact durable outcomes. |
| REPLACE-DECISION-04 | Each variant owns foreground decision input and keeps confirmation controls reachable while details scroll. Cancel and close clear its authority. Rehost and a replacement modal cannot retarget an earlier subject. |

Use `replace_actions_and_confirmation_footers_stay_reachable_without_duplicate_submission`, `search_replace_detached_confirmation_applies_once_after_retention` and scoped guarded replacement/receipt tests. Native Apply, Restore and Recovery evidence requires disposable files and separate outcomes. No GPUI or native confirmation is accepted.
