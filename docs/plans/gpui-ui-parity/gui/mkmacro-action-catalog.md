# MkMacro Action Catalog

G19 owns `mkmacro-action-catalog`, titled Add Action. The current [MkMacro](mkmacro.md) + Action effect opens the retained catalog. The existing action registry owns stable action definitions and explicit support. A configurable catalog selection stages a draft in the [Action Editor](mkmacro-action-editor.md).

| Criterion | Source-backed behavior |
| --- | --- |
| MK-ACTION-CATALOG-01 | Provide categorized action search and correct existing descriptor identity. Scope choices to current macro, after-step or structural selection. Preserve retained catalog/filter on eligible close and return to the parent. |
| MK-ACTION-CATALOG-02 | A configurable descriptor stages only its typed child draft. Current Apply belongs to the Action Editor. Cancel preserves macro and catalog, and invalid/unsupported/retired requests do not alter the draft. |

Select `configurable_catalog_selection_is_transactional_and_keeps_catalog_state`, `cancelling_catalog_editor_preserves_macro_and_catalog` and `native_catalog_opens_editor_and_commits_one_action`. `G19-EGUI-MKMACRO-DELAY-DRAFT` verifies actual Add Action, Delay route and child Cancel retaining the catalog. Other routes, host and GPUI remain pending.
