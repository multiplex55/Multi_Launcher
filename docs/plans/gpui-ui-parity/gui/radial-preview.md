# Radial Authoring Preview

G20 owns `radial-preview`. The Designer has an embedded Preview / Test board and an optional desktop preview lease. Both use the shared scene and navigation model with inert authoring actions.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-PREVIEW-01 | Preserve preset, camera, Fit, zoom, navigation, hover, tooltip and simulated drag controls without dirtying the draft. Keep generated rows read-only in Design and intercept action selection in Preview. |
| RADIAL-PREVIEW-02 | Default to synthetic context. External context sampling is explicit and excludes owned and preview windows. Start and Update validate the draft baseline, session, generation and exact lease. The checked native host has no action, history, audio or trigger ownership. |
| RADIAL-PREVIEW-03 | Preserve exact Open, Update and Stop status and owned retirement on close, save, conflict, feature and runtime transitions. Ignore stale results and wait for required terminal receipts before Designer retirement. |

Select `preview_zoom_and_presets_never_dirty_the_authoring_session`, `saved_query_and_exact_command_are_not_executable_in_embedded_preview`, `native_preview_same_center_survives_three_levels_drag_and_exact_back`, `lease_is_exclusive_stale_safe_and_never_dispatches`, `preview_boundary_has_no_audio_history_executor_or_reservation_owner`, `sampled_context_excludes_owned_and_preview_windows_and_preserves_external`, `closing_pending_native_preview_supersedes_with_terminal_stop` and `native_preview_lease_cannot_survive_generation_save_or_conflict`. Native and GPUI remain pending.
