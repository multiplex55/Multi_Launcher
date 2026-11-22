# Theme Settings

G06 owns `theme-settings`, the theme editor and its canonical utility key. The Settings plugin routes `theme` through the typed Theme dialog command. Main Settings also offers Open Theme Settings and catalog search destinations. A child navigation request belongs to the current accepted Settings instance and transfers to the real child response without changing the target value.

`G06-EGUI-THEME-DIRECT-SAVE` records the actual direct route, Overall Text Size change, preview distinct from disk, Apply versus Cancel, reopen, and Save of a fixed mode and density. The parent Settings route reached the real search result and direct button. Opening/focusing the child through that parent remains unverified.

`src/gui/theme_settings_dialog.rs` owns the retained editor, baseline and preview. `src/gui/theme/mod.rs` distinguishes committed and previewed appearance across egui viewports. The canonical schema lives in `src/settings/model.rs`. The separate G04 appearance adapter consumes committed settings during GPUI startup.

| Criterion | Source-backed behavior |
| --- | --- |
| THEME-01 | Open via launcher and Main Settings direct/search routes. Default docked, remember saved presentation and honor an accepted popped-out Settings parent. Preserve the live draft, preview and owner epoch through rehost. Keep scrolling content and the wrapped footer reachable at narrow sizes. |
| THEME-02 | Edit System, Dark, Light or Custom, Compact or Comfortable, and Overall Text Size from 75% to 200% in 5-point steps. Reset to 100% and section resets affect their own scope. Color editors and swatches cover window/panel, primary text and links, control states, selection, success/warning/error and optional accent, with RGBA values retained. |
| THEME-03 | Browse curated groups separately from choosing a palette. Select a saved or curated palette and restore its typography. Save As validates a trimmed, nonempty unique name. Update Palette writes the intended saved identity. Manual edits switch to Custom without replacing a saved entry. Reset all resets the draft while retaining saved palettes and scales. |
| THEME-04 | Ordinary edits publish a live preview without writing settings. Apply persists and keeps the editor open. Save persists and requests close. Cancel or ordinary accepted close clears unsaved preview and restores the latest committed theme, including any earlier Apply. A refused pinned close retains the draft and preview. |
| THEME-05 | Keep the old baseline, edited draft and feedback on save failure. An external change to the saved theme requires explicit Reload saved theme before committing again. Initial load failure disables edits and offers Retry loading or Cancel. Save atomically updates `Settings.theme` and preserves unrelated settings. |

Use `theme_settings_dialog_apply_save_cancel_and_close_routes_restore_committed_theme`, `theme_settings_dialog_pinned_close_refusal_retains_draft_and_preview`, `settings_search_cross_owner_theme_waits_through_window_sizing_before_focusing_response`, `theme_conflicts_retain_draft_reload_explicitly_and_preserve_unrelated_settings`, and the scale/density and RGBA tests when those paths change. Native evidence needs the direct and child search routes, effective preview, Apply, later Cancel and reopen on a disposable profile. GPUI editor implementation and final visual acceptance remain pending.
