# Clipboard History

G13 owns `clipboard-history`. Bare `cb` offers `cb: edit clipboard`, which emits `clipboard:dialog` for `ClipboardDialog`. The Clipboard plugin owns protected OS sync and the canonical `clipboard_history.json` snapshot. The retained dialog owns selection, edit and inline removal review.

| Criterion | Source-backed behavior |
| --- | --- |
| CLIP-HISTORY-01 | Show ordered selectable entries with a 160-character flattened preview. Keep protected clipboard data out of history. Ordinary entries follow existing deduplication, capacity and last-good disk rules. Load the saved history for the session and refresh the current browsing snapshot by version. |
| CLIP-HISTORY-02 | Edit opens the exact full multiline entry. Validate nonempty Save against the complete captured disk snapshot and exact entry. Cancel leaves storage unchanged. Conflict or I/O failure remains visible and keeps the draft. Copy revalidates the same authority before the explicit OS write. |
| CLIP-HISTORY-03 | Remove opens the inline exclusive `clipboard-removal` scope with Confirm/Cancel. Exact owner, entry token and full snapshot gate removal. Browse rows cannot execute through an active removal decision. Keep shown draft/decision, reject stale effects and surface conflict. |
| CLIP-HISTORY-04 | Preserve remembered Dock/Pop Out and geometry, exact retained root/owner frame and current open epoch. True close rejects stale actions and reopen loads the current store. |

Select `protected_clipboard_history_skips_only_the_marked_item`, `ensure_open_loads_saved_history_once_and_preserves_edit_draft`, `jump_mode_clipboard_checked_persistence_preserves_draft_on_conflict_and_io_failure`, `jump_mode_clipboard_checked_persistence_keeps_pending_removal_after_external_change`, `clipboard_history_detached_save_and_copy_require_retained_passes` and `jump_mode_clipboard_actions_removal_has_an_exclusive_fresh_confirmation`. `G13-EGUI-CLIPBOARD-HISTORY-DRAFT` verifies the native disk-only route, Edit and Cancel, and Remove and Cancel.

## GPUI docked Browse and Edit

The exact stock `cb` provider opens the retained workspace. Browse uses the canonical file, byte-faithful full entries and bounded selectable previews. A captured occurrence owns Edit and keeps the draft after a refresh. Opening and Cancel leave the OS clipboard and history bytes untouched. Focused tests passed with the checked effects. Paired Windows evidence stays open. [Checkpoint](../CHECKPOINTS.md#g13-clipboard-history-browse-edit-and-cancel)

## GPUI checked effects

Save, Copy and Remove bind the active mount and exact ordered snapshot. Save and the confirmed removal use guarded atomic replacement. Copy explicitly writes the complete current entry and checks authority after native clipboard acquisition. Conflicts preserve the draft or decision. Focused tests passed and Windows replay remains pending. [Checkpoint](../CHECKPOINTS.md#g13-checked-clipboard-history-save-copy-and-remove)

## GPUI retained host

The checked host restores remembered Dock or Pop Out, retains the complete draft and current removal review, and saves native popup geometry only through current ownership. Focused host tests passed. Paired Windows replay, pins, legacy docked sizing and Jump remain pending. [Checkpoint](../CHECKPOINTS.md#g13-retained-clipboard-history-host)
