# Export Macro Library

G19 owns `mkmacro-package-export`. [MkMacro](mkmacro.md) Reuse → Export Library opens the retained root-selection child. Export Selected Macro offers a direct route for one root. The package builder owns the exact transitive closure and referenced assets.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-EXPORT-01 | Show selected root checkboxes and gate Export on nonempty selection. Follow authored transitive calls, folder metadata and referenced images from the exact current draft. Reject missing/invalid assets or oversized package. |
| MK-EXPORT-02 | Preserve exact child and parent owner, draft revision and root selection around the owned Save chooser. Use current safe suggested name and exact chosen path. Cancel preserves all stores. Atomic file export changes only the approved output. |

Select `mkmacro_native_parent_opens_ordinary_children_with_retained_origin`, `package_children_keep_distinct_presentation_and_live_drafts`, `export_is_exact_for_multi_root_transitive_closure_folders_and_images` and `export_missing_typed_asset_is_read_only_and_reports_the_filename`. Native and GPUI evidence are pending.
