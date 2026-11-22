# Volume

G24 owns `volume`. Exact `vol` opens the volume dialog. The host reads the current Windows default endpoint and process sessions. Master level and each session have separate controls.

| Criterion | Source-backed behavior |
| --- | --- |
| VOLUME-01 | Preserve current endpoint fallback, scoped master draft and explicit Set/Cancel. Keep loaded data and draft on warm retained opens and reload on fresh open. |
| VOLUME-02 | Preserve exact process row Set and Mute actions. The current muted session slider also explicitly unmutes on movement. Verify with an injected session snapshot and effect sink. |

Select `transition_loading_preserves_pinned_cancel_draft_and_reloads_after_unpinned_close`, `long_process_rows_scroll_inside_the_window_without_mutating_the_snapshot`, `slider_change_unmutes` and `search_plain_vol`. Native and GPUI remain pending.
