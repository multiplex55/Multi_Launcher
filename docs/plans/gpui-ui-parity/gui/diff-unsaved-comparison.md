# Diff Unsaved Comparison

G08 owns `diff-unsaved-comparison`. The current Diff host requests an owner-viewport decision if closing would discard edited comparison data. `src/gui/diff_dialog/mod.rs` renders the form and routes it to the current workspace and picker owner.

| Criterion | Source-backed behavior |
| --- | --- |
| DIFF-DIRTY-01 | Report an unsaved side or sides and expose Save modified, Discard and Cancel within the active Diff viewport. The pending decision prevents ordinary workbench and preview input. |
| DIFF-DIRTY-02 | Save through the existing source policy and an owned Save As picker when required. Cancel the picker or surface a failure while keeping the workbench and draft. Discard closes only the reviewed owner. Cancel leaves the current workbench and edits intact. |
| DIFF-DIRTY-03 | A stale decision cannot close a replacement comparison. Back or replace that would drop edited memory fails with actionable feedback until the current draft is resolved. |

Use `host_close_keeps_dirty_owner_until_confirmation` and the affected save or replacement tests. Actual Windows smoke needs an edited scratch comparison, Cancel and preservation, then an approved close or save. GPUI implementation and native evidence are pending.
