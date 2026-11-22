# Hot Folder Manager

G21 owns `hot-folder-manager`. The typed `hotfolder:open` action opens or restores the root-layer manager. The rule store and application driver own effects, watcher state and runtime history.

| Criterion | Source-backed behavior |
| --- | --- |
| HOT-FOLDER-01 | Show rules, quarantine count, runtime status and terminal history. Preserve separate Pause and Resume, plugin disable and manager-close semantics. The manager opens an empty store without admitting watches. |
| HOT-FOLDER-02 | Preserve New/Edit drafts, watched and destination directories, filters, typed actions, settling controls and native chooser owner/request/draft checks. Test Rule only reads sample metadata. Save persists a disabled rule. |
| HOT-FOLDER-03 | Preserve deletion, undo, confirmed history clear and enablement authorization for the exact current revision, fingerprint and capability. The strong confirmation requires explicit acceptance. Stale choices and drafts cannot publish. |

Select `hotfolder_provider_and_payload_free_controls_use_the_typed_bus`, `hotfolder_editor_popup_and_path_ticket_are_bound`, `m10e_test_rule_reads_sample_metadata_without_live_runtime_effects`, `condition_edit_retires_a_previously_prepared_enable_confirmation` and `hotfolder_confirmation_jump_focus_does_not_accept`. `G21-EGUI-HOT-FOLDER-AUTHORING` verifies native empty manager, draft, picker cancellation and zero admitted roots. Save, runtime and GPUI remain pending.
