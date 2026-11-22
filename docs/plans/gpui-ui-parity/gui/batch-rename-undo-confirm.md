# Confirm Batch Rename Undo

G09 owns `batch-rename-undo-confirm`, the receipt-bound decision shown by the current Batch Renamer viewport. The existing transaction owner uses the eligible durable receipt and its remaining entries for undo and recovery.

| Criterion | Source-backed behavior |
| --- | --- |
| RENAME-UNDO-01 | Enable Undo only for an eligible receipt with undoable remaining entries. Review the exact operation and submit or cancel from its owning viewport. |
| RENAME-UNDO-02 | Reject obsolete owner and receipt decisions. Rehosting retains only the current accepted decision. Cancel or close leaves both files and the durable receipt intact. |
| RENAME-UNDO-03 | Execute through the shared guarded compensation/Undo operation. Preserve already completed and remaining outcomes, progress, cancellation and recovery feedback. |

Use `m8e_launcher_route_previews_applies_and_undoes_with_guarded_recovery` when changing this route and relevant shared transaction tests for receipt/identity handling. Native proof requires an accepted rename of a disposable file before acting on its receipt. No native Undo or GPUI implementation is recorded.
