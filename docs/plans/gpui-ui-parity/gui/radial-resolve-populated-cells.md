# Resolve Populated Radial Cells

G20 owns `radial-resolve-populated-cells`. Shrinking a ring with populated overflow presents the prepared resize resolution in the current Designer.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-RING-RESOLVE-01 | Preview move overflow, relocation or discard against the exact proposed geometry. Cancel leaves the draft intact. A subsequent explicit Apply accepts only the matching prepared and visible proposal, current generation, exact subject and validated complete candidate. Apply mutates the draft once and remains undoable. |

Select `shrink_requires_explicit_resolution_and_undo_restores_cells`, `stale_resize_plan_cannot_replace_newer_cell_content`, `populated_shrink_resolution_is_preview_only_until_atomic_apply`, `repeated_same_geometry_proposals_receive_distinct_preview_identities`, `proposal_preparation_uses_complete_candidate_through_normal_service_request` and `ring_resize_is_atomic_and_does_not_partially_discard`. Native and GPUI remain pending.
