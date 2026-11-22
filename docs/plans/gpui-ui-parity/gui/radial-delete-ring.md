# Delete Populated Radial Ring

G20 owns `radial-delete-ring`, the Designer's confirmation for a populated ring. The reviewed subject is the exact menu and ring.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-RING-DELETE-01 | Offer explicit relocation of all occupied slots and spacers to another ring in the same menu, explicit discard and Cancel. Validate the full candidate and last-ring guard before one atomic draft mutation. Preserve stable relocated identities and keep cancellation free of edits. |

Select `populated_ring_delete_relocates_cells_atomically_with_stable_ids`. Native and GPUI remain pending.
