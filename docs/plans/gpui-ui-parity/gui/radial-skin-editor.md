# Radial Skin Editor

G20 owns `radial-skin-editor`. `radial skins` and the [Designer](radial-designer.md) Resources route reach the shared native authoring viewport. The radial style/catalog owner supplies effective field provenance and bounded appearance previews.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-SKIN-01 | Show built-in/authored choices and bounded current appearance preview. Preview is separate from Apply appearance. Accept only the exact selected tile, menu, session, generation and digest. Cancel preserves existing draft. |
| RADIAL-SKIN-02 | Preserve User defaults and current Skin/Menu/Ring/Cell scopes, legal typed fields, Inherit/Set/Clear/reset and effective provenance. Create/duplicate/rename and merged edits retain undo/selection. Referenced skin deletion reports usage and refuses. |
| RADIAL-SKIN-03 | Preserve exact bounded resource/font catalog replies and live draft. Apply appearance changes the draft. Existing Designer Apply/Save publishes. |

Select `skins_tab_preserves_inspector_text_and_skin_navigation_can_be_cancelled`, `every_full_scope_field_roundtrips_inherit_set_clear_and_reports_provenance`, `creating_skin_undo_redo_restores_prior_and_created_selection`, `referenced_skin_delete_is_blocked_with_usage_paths`, `appearance_preview_cancel_and_single_apply_preserve_dirty_work_and_explicit_destination` and `destination_navigation_and_prepared_choice_identity_cannot_apply_an_old_menu_or_tile`. `G20-EGUI-RADIAL-AUTHORING-ROUTES` covers native route/chrome only. Native controls and GPUI remain pending.
