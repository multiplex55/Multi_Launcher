# Import Macro Library

G19 owns `mkmacro-package-import`. [MkMacro](mkmacro.md) Reuse → Import Library uses the admitted Windows chooser for `.mkmacro` or `.json`, then an owned review. The package planner owns the immutable validated draft/disk/asset basis. The store owns transaction and publication.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-IMPORT-01 | Validate format/schema, nested models, bounds, dependency and asset identities before review. Show exact roots, added/reused/renamed items and hotkey warnings. Preview/Cancel has no publish effect. |
| MK-IMPORT-02 | Apply requires the exact current draft/revision and disk/asset basis. Allocate/rewrite stable typed IDs, closure links and safe unique asset names. Use no-clobber asset creation and exact owned rollback; publish document and live store only after success. |
| MK-IMPORT-03 | Accepted Apply publishes the reviewed import and current draft together, then adopts the exact stored baseline and selected root. Retired owner, changed basis and chooser cancellation preserve current state. |

Select `import_preview_cancel_is_lossless_and_apply_adopts_exact_store_snapshot`, `apply_rejects_stale_document_state_without_mutation`, `injected_failure_rolls_back_only_new_assets_and_never_publishes_document`, `external_asset_replacement_is_detected_and_never_deleted_by_rollback` and `probe_and_load_reject_unknown_nested_package_fields_without_rewriting`. Native and GPUI evidence are pending.
