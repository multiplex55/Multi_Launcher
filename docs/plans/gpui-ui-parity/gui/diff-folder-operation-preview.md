# Diff Folder Operation Preview

G08 owns `diff-folder-operation-preview`. `src/gui/diff_dialog/operation_preview.rs` renders the accepted Copy or Recycle Bin plan in the owning Diff viewport. The shared folder runtime and file operations own the plan and mutation.

| Criterion | Source-backed behavior |
| --- | --- |
| DIFF-OP-01 | Show copy direction or deletion side, file/directory counts and affected items. Copy shows overwrites, conflicts, skips and validation errors. Deletion states Recycle Bin behavior. Distinguish destructive Execute and leave footer controls reachable as details scroll. |
| DIFF-OP-02 | Disable Execute for fatal validation, a failed plan or active mutation. Cancel leaves the tree unchanged. Stale ordered roots, workspace/view or folder generation cannot redirect a reviewed plan. |
| DIFF-OP-03 | Execute exactly one accepted plan from the owning viewport. Preserve the current conflict and partial-failure feedback and keep foreground input ownership scoped to the decision. |

Use `folder_operation_stale_preview_cannot_dispatch_after_root_change`, `diff_forms_chrome_blocked_long_preview_keeps_actions_reachable` and the relevant `diff_file_ops` domain cases. Native evidence must use disposable trees and verify both Cancel and the planned result, including any overwritten bytes. GPUI implementation and native evidence are pending.
