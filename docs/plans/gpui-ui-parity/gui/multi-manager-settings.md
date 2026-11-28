# MultiManager Settings

G24 owns `multi-manager-settings`. Exact `mm settings` opens the dedicated root settings form independently of the workspace editor.

| Criterion | Source-backed behavior |
| --- | --- |
| MULTIMANAGER-SETTINGS-01 | Preserve enabled, paths, autosave, save on exit, reconnect, poll interval, launcher exclusion and visibility settings. Accepted edits update their existing live owners. Save persists only the MultiManager block and preserves unrelated settings. |

Select `activating_mm_settings_opens_multi_manager_settings_dialog`, `settings_form_auto_shrinks_and_scrolls_to_save_without_mutating_settings` and `save_multi_manager_settings_preserves_unrelated_settings`. `G24-EGUI-MULTI-MANAGER-ROUTES` verifies the actual distinct form and exact scratch paths without edits. GPUI also admits exact `mm settings` and saves the two paths through the checked canonical Settings transaction. The next clean workspace reads the acknowledged pair. The stock Auto-reconnect windows on load checkbox shares the same checked Save and takes effect on the next launcher start. Paired desktop evidence, other Settings controls and live runtime behavior remain open. [Auto-reconnect setting checkpoint](../CHECKPOINTS.md#p010-multimanager-auto-reconnect-windows-on-load-setting) [Source paths checkpoint](../CHECKPOINTS.md#p010-checked-multimanager-source-paths)
