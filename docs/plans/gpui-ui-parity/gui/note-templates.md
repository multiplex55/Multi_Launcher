# Note Templates

G12 owns `note-templates`. Quick Notes opens the manager with Templates when `templates_enabled` and the current owner are valid. The `note templates` query lists creation inputs. `TemplateManagerState` owns the selected template and draft. `NoteTemplatesWindowHost` owns Dock/Pop Out. The Notes plugin owns atomic writes, sorted names and the template cache.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-TEMPLATES-01 | Select loads the exact name/content. New Template and Clear reset the draft. Refresh reloads stored names and selected content, or clears the selection if it disappeared. |
| NOTE-TEMPLATES-02 | Save validates and normalizes the edited name as the storage key, writes its content, updates selection/cache/search and preserves committed state after failure. Open launches the selected file association. |
| NOTE-TEMPLATES-03 | Preserve the retained manager, accepted Quick Notes origin, host moves and current draft. Delete stages the exact row in [Delete Template?](note-template-delete.md). Root and owner receipts gate the decision. |

Select `retained_notes_auxiliary_open_reuses_drafts_and_rejects_retired_source`, `failed_template_save_preserves_destination_and_committed_cache`, `note_template_helpers_trim_names_for_crud_operations` and `note_templates_command_still_lists_cached_templates`. Use explicit `ML_NOTE_TEMPLATES_DIR` and `ML_NOTES_DIR` scratch roots for native tests. Native and GPUI evidence are pending.
