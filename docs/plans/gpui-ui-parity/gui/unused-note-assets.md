# Unused Note Assets

G12 owns `unused-note-assets`. `notes unused` and `note unused` emit `note:unused_assets`. The Quick Notes Unused Assets action also opens this utility through the current parent receipt. `UnusedAssetsWindowHost` owns Dock/Pop Out and retained selection. `src/plugins/note.rs` owns the note-reference and assets scan and deletion.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-ASSETS-01 | Fresh open scans immediate supported image files and Markdown `assets/` image references, stripping optional `\|width`. Show sorted filename checkboxes or No unused assets found. Same retained open keeps the inventory and selection. True reopen scans again. |
| NOTE-ASSETS-02 | Keep Delete Selected, Close and host controls reachable. Tab reveals the row and Jump Mode focuses without applying. The exact retained owner, open epoch, inventory generation and selected filenames gate deletion. |
| NOTE-ASSETS-03 | Delete Selected is the explicit current action. Successful deletions leave the inventory and failed entries remain with feedback. The existing path has no second confirmation, reference rescan or file identity recheck at deletion. A failed note read can leave an empty reference set today. |

Select `unused_note_assets_tab_reveals_the_current_offscreen_row`, `accepted_delete_uses_names_and_current_inventory`, `pinned_open_preserves_selection_until_explicit_close_and_reopen`, `unused_note_assets_hint_focuses_without_toggling_or_deleting`, `native_notes_buttons_open_independent_auxiliaries` and `retained_notes_auxiliary_open_reuses_drafts_and_rejects_retired_source`. `G12-EGUI-GRAPH-ASSETS` covers actual launcher route, one orphan with the referenced image excluded, and close with identical fixture bytes. Selection semantics, delete, retained host and GPUI remain pending.
