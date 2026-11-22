# Hotkey Directory

G24 owns `hotkey-directory`. Settings → Hotkeys opens the retained directory. Its provenance snapshots represent configured and acknowledged hotkeys while each feature owns its actual registration and editor.

| Criterion | Source-backed behavior |
| --- | --- |
| HOTKEY-DIR-01 | Preserve held host/session, filter, category and stable sort, configured and current state and conflicts. Preserve selected stable owner and entry on unrelated updates. |
| HOTKEY-DIR-02 | An accepted Edit validates source publication, row revision, owner, exact unique target and current host. Route to the correct Settings, Paste Sequence, macro, radial or gesture editor without dispatching a hotkey or replacing dirty state. Keep unavailable and unsupported targets explicit. |

Select `hotkey_directory_ui_opens_from_settings_and_edits_only_after_fresh_activation`, `hotkey_directory_ui_revalidates_owner_revision_and_current_edit_target`, `hotkey_directory_accepted_source_updates_without_retargeting_selection` and `hosted_directory_native_sort_popup_keeps_owned_input_after_discard`. Native and GPUI remain pending.
