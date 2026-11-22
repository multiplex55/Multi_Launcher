# Unsaved Radial Changes

G20 owns `radial-unsaved-changes`, the retained Designer close decision titled Unsaved radial changes. A native close request is canceled while the checked close flow resolves outstanding work. The Designer shows the prompt only for the current retained parent.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-CLOSE-01 | Flush pending geometry, retire disposable requests and wait for required publication, package and native preview-stop receipts. Prompt when the authoritative draft, Cell Properties or Inspector buffer is dirty. |
| RADIAL-CLOSE-02 | Save remains unavailable while a child has unresolved local edits. Save uses the correlated close disposition. Discard requests a checked revert when an Apply checkpoint requires it. Keep editing clears the close intent and retains staged input. |

Select `retained_inspector_keep_editing_preserves_buffers_and_discard_tears_down_owner`, `dirty_cell_popup_survives_native_close_until_explicit_resolution`, `discard_close_terminalizes_disconnected_preview_stop_without_retrying` and `apply_resets_checkpoint_save_closes_and_dirty_close_prompts`. Native and GPUI remain pending.
