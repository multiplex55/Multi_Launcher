# Diff Folder Rules

G08 owns `diff-folder-rules`. The Folder workbench opens a separate form in the workspace/view slot in `child_hosts.rs`. The form edits `FolderRulesDraft`. The shared Folder model validates candidates and owns rescans.

| Criterion | Source-backed behavior |
| --- | --- |
| DIFF-FRULES-01 | Edit file-size and modified-time comparison, timestamp tolerance, Metadata only/Contents on demand/Always compare contents, multiline include and exclude patterns, and optional text comparison rules for case, whitespace, blank lines and line endings. Preserve inactive text options in the draft. |
| DIFF-FRULES-02 | Show invalid input and disable Apply + Rescan. Cancel closes the editor without mutating the active rules. Keep action controls reachable as the body scrolls or wraps. |
| DIFF-FRULES-03 | Accept one application against the original workspace/view, ordered roots and rule basis, then rescan. Reject stale decisions, preserve the editor on validation errors and keep its draft through a valid host move. |

Use `rendered_folder_cancel_then_apply_changes_live_rules_once` and shared folder rules validation. Native proof needs a small two-directory tree, Cancel, invalid and valid inputs, a confirmed rescan, and one supported host change. GPUI implementation and native evidence are pending.
