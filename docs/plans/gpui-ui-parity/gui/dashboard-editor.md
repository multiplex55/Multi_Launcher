# Dashboard Editor

G23 owns `dashboard-editor`. The `dashboard` query offers Dashboard Settings, which opens the retained layout editor. The owner keeps the same draft across dock, popout and launcher hide.

| Criterion | Source-backed behavior |
| --- | --- |
| DASHBOARD-EDITOR-01 | Preserve dimensions, slot ordering, drag/preview, current selection, widget choice and descriptor settings. Reject stale or invalid rows and colliding placement. Preserve separate per-section presentation memory. |
| DASHBOARD-EDITOR-02 | Save validates and publishes the current authorized draft through the layout store. Failure retains the editable draft. A corrupt source remains protected. Closing without Save leaves runtime configuration intact. Preserve both scoped destructive confirmations. |

Select `dashboard_editor_detached_launcher_route_renders_hidden_and_docks_back`, `dashboard_editor_visible_host_buttons_pop_out_and_dock_the_same_draft`, `failed_coalesced_save_and_close_keeps_editor_open_and_draft_retryable` and `corrupt_document_stays_read_only_when_editor_save_is_attempted`. Native and GPUI remain pending.
