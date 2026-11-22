# Radial Inspector Scope Change Guard

G20 owns `radial-scope-change-guard`, with the Unsaved Inspector action and Stale Inspector action displays. The Designer binds one queued intent to the exact session, draft generation and stable selection.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-SCOPE-01 | Allow Apply and continue for current, complete bindings and replay-safe intents. Revalidate and rebase only within the same session and selection. Replay-unsafe edits require a fresh operation after Apply action. |
| RADIAL-SCOPE-02 | Discard and continue restores the prior assignment and performs a current intent. The stale variant can discard the action and cancel the queued change. Cancel change preserves the buffer. Keep ownership and cancellation scoped to the matching retained Designer child. |

Select `scope_change_anchor_rejects_a_stale_session_generation_or_target`, `stale_inspector_scope_change_cannot_run_after_generation_changes`, `skins_tab_preserves_inspector_text_and_skin_navigation_can_be_cancelled` and `retained_dirty_inspector_properties_handoff_requires_explicit_discard`. Native and GPUI remain pending.
