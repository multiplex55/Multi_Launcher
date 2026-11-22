# File Operations Center

G11 owns `file-operations-center`. The built-in `file_operations` plugin matches `fileops`, `file operations` and `file operations center`. The accepted File Search toolbar or context action prefills the exact current source. Downloads Triage passes an exact Copy/Move source set and optional destination suggestion. Activity and Notification Inbox can focus an eligible exact job. Recovery decisions are part of the center's receipt History.

`src/gui/file_operations_center_dialog.rs` owns the retained draft, review and accepted host. `src/gui/file_operations_center_runtime.rs` reobserves exact source identity for review. The existing file transfer planner, `TransferJobs` and receipt services own queued and active work, interruption and durable receipts.

| Criterion | Source-backed behavior |
| --- | --- |
| FILE-OPS-01 | Open direct, source-prefilled, reviewed-source or exact-job routes. Preserve the accepted inputs and origin. Use the checked owning HWND for Choose files, Choose folder and Choose destination. Keep Copy, Move and Duplicate plus verification options, source/destination and readiness feedback. |
| FILE-OPS-02 | Review on its worker and reject source drift. Keep the exact draft revision, accepted review and target class. Display bounded plan rows and existing Reset, Skip, compatible Replace/Merge and validated Keep both decisions. Bulk choices apply to the exact compatible selection. |
| FILE-OPS-03 | Review is read-only. Queue reviewed transfer requires a current eligible plan. Start and reorder/pause/resume/cancel act on exact queued or active job identities. Preserve live phase/bytes/elapsed, verification state and terminal outcomes. |
| FILE-OPS-04 | History and receipt detail preserve exact job/item, evidence and pagination. Retry and Undo reopen a receipt-bound review with eligibility, one-shot queue and Cancel. Discard interrupted preview is explicit. A changed history owner or a consumed decision cannot retarget recovery. |
| FILE-OPS-05 | Preserve one retained dock/popout host, its draft and accepted presentation. Current session, host and retained pass govern actions and picker completion. Close retires pre-admission work and recovery review. The application-owned queue and active work remain observable. |

`G11-EGUI-FILE-OPS-DUPLICATE-REVIEW` records native File Search toolbar delivery of the exact result and a reviewed Duplicate target with Start still disabled and no output. Use `file_operations_center_file_search_handoff_is_exact_and_stale_safe`, `file_operations_center_picker_requires_current_owner`, `file_operations_center_stale_picker_and_close_preserve_draft`, and `file_operations_center_async_review_queue_and_start_are_separate` for the affected seam. Scoped execution or recovery proof uses disposable fixtures. GPUI and remaining native work are pending.
