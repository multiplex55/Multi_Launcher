# Radial Asset Picker

G20 owns `radial-asset-picker`. [Radial Designer](radial-designer.md) Resources and scoped style/cell media controls expose managed assets, external/search-path references and icon resources. `ResourceChoice`, the authoring session and the runtime asset service retain existing validation and output boundaries.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-ASSET-01 | Import supported bounded media through the expected owner/target. Preserve managed content-addressed staging, source validation and editable independent external/icon references. Nonportable references remain excluded from package assets. |
| RADIAL-ASSET-02 | Bind selected scope, field and bulk target to the current request. Cancel leaves draft. A valid asset assignment and managed bytes stage atomically and participate in undo. Reject referenced asset deletion with usage. |
| RADIAL-ASSET-03 | Keep explicit Audition separate from preparation/import and actual runtime. Staged assets publish through Designer Apply/Save with exact current revision and disk basis. |

Select `usage_query_blocks_delete_and_lists_references`, `external_search_and_icon_references_are_explicitly_nonportable`, `icon_resource_index_roundtrips_through_typed_style_control`, `asset_staging_participates_in_bounded_undo`, `authoring_asset_and_document_publish_together_and_return_inverse` and `authoring_revision_or_sha_conflict_creates_no_partial_asset`. Native and GPUI remain pending.
