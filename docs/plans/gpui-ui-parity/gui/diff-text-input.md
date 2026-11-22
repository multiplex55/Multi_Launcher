# Diff Text Inputs

G08 owns `diff-text-input`. Compare Text starts a new private memory comparison. Edit Inputs is available for an in-memory comparison and binds to its existing view/sources. `src/gui/diff_dialog/child_hosts.rs` owns one retained standalone or docked form per workspace.

| Criterion | Source-backed behavior |
| --- | --- |
| DIFF-INPUT-01 | Show Left and Right multiline editors, independent explicit Paste actions and byte counts. Preserve exact input bytes in memory. Empty is permitted. Each side may contain at most 8 MiB and 100,000 lines. Oversize input reports failure with the draft intact. |
| DIFF-INPUT-02 | Compare adds a new memory comparison. Apply Inputs revises the same intended memory sources only while their identities and revisions still match. Cancel leaves the prior view and sources intact. Memory contents reach durable storage through explicit save/export. |
| DIFF-INPUT-03 | Inherit an accepted popped-out parent when appropriate. Preserve draft and focus through dock/popout, fence stale paste and submit, and close the child when its actual owner retires. |

Use `native_diff_text_child_inherits_and_keeps_retained_draft` and `memory_text_edit_inputs_reject_equal_revision_swapped_sources`. A synthetic test viewport does not prove the Windows child. Actual smoke needs an empty side, accepted new comparison, later input edit and Cancel with a retained draft move. GPUI implementation and native evidence are pending.
