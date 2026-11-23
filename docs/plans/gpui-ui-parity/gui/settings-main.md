# Main Settings

G06 owns `settings-main`. `settings` is its canonical utility presentation and geometry key. The native egui toolbar and Settings search exposure are recorded in `G06-EGUI-SETTINGS-NAVIGATION`. `G03-C2B-REVERSE-SETTINGS-WINDOWS` verifies the idle restart footer, dirty draft denial, confirmation Cancel and one same-profile GPUI child. [Validation log](../VALIDATION_LOG.md)

## Routes and ownership

- The launcher **Settings → Edit Settings** menu toggles the editor. The Settings plugin routes `settings` to the typed dialog command and `settings <query>` to typed Settings search with the original query. Reentry can replace or clear transient search without discarding the editor draft. See `src/plugins/settings.rs`, `src/gui/command_host.rs`, and `src/gui/mod.rs`.
- `SettingsEditor` owns the draft, acknowledged baseline and search/navigation state. `SettingsWindowHost` hosts one editor in docked Settings or the resizable `Settings — {instance}` popout. It accepts effects using the retained frame, instance and host revision. The canonical settings save path queues `SettingsSaveIntent::general` and reconciles runtime state only after the accepted outcome.
- Theme, Mouse, Mouse Gestures, Plugin Editor, dashboard configuration, Hotkey Directory and Clipboard Modify have separate child hosts. `SettingsOpenOrigin` proves the accepted parent instance. Eligible children opened by a popout can use native presentation even if previously saved as docked. A stale or moved Settings host cannot deliver an old child request. Built-in plugin settings embedded in the parent edit its draft and track catalog revision.

## Parity criteria

| Criterion | Required behavior |
| --- | --- |
| SETTINGS-01 | Preserve toolbar, typed open and typed search. Repeat open focuses the current editor and retains the draft. A fresh session loads the committed baseline. |
| SETTINGS-02 | Preserve Search settings, Expand/Collapse all, all eight registered sections, scrolling, default expansion and section owners. The sections cover Hotkeys, Paste Sequences, Launcher Window & Appearance, Search & Results, Actions/Safety/Feedback, Dashboard, Radial and Plugin Settings. |
| SETTINGS-03 | Search shows relevant control matches, owner breadcrumbs, help and availability, with up to eight per page. Enter routes the current match to its exact control or child. Escape clears search while that input owns the key, then performs normal dismissal through the active container. Revealing a conditional prerequisite preserves its value. |
| SETTINGS-04 | Save validates and submits the accepted draft to the settings worker. During a pending save, Save and Cancel are disabled and status is shown. Success advances the baseline and applies runtime state while the editor stays open. Failure preserves the draft and presents the applicable error or reconciliation state. Cancel resets to the committed baseline and closes. |
| SETTINGS-05 | Dock and Pop Out retain the current draft, search and eligible pending state. Independent popout stays available with the launcher hidden. Normal close dismisses the current editor. Track presentation and geometry through canonical settings and retire stale viewport and child callbacks. |
| SETTINGS-06 | A controlled frontend switch honors the current main and child drafts and the actual save acknowledgement. Normal close can discard edits, and ordinary shutdown requests cancellation of pending Settings writes. A switch requiring Save must wait for the accepted save outcome before releasing ownership. |

## Verification

Scope the existing library regressions to the changed boundary. Relevant cases include `settings_search_entry_reaches_gui_host_losslessly_without_replacing_draft`, `settings_search_navigation_consumes_enter_before_focus_in_both_jump_settings`, `main_settings_popout_preserves_draft_with_launcher_hidden_and_docks_back`, `popped_settings_opens_children_over_saved_docked_preferences`, and `hidden_settings_editor_completes_and_a_second_save_uses_the_new_baseline`.

On Windows, exercise both entry routes, a search destination, an accepted scratch Save and Cancel of a separate unsaved change. Verify exact editor focus and Escape ownership with the actual focused child. Reopen the profile in both frontends, then verify a native popout while the root is hidden and one child opened from that popup. Check persistence and recovery using the selected runtime and disposable data.

## First native Settings save

Use one retained docked Settings editor with the existing Search & Results control **Enable query autocomplete** and its normal Save and Cancel behavior. Enter through the installed Settings route from G05. A repeat open focuses the current editor and keeps its draft. An unsaved toggle edits the draft. On Save, capture its current editor and request identity, freeze the acknowledged Boolean and desired value, and take the existing restart permit through acknowledgement. Pending disables edits and dismissal. A dirty idle draft needs Save or Cancel before a frontend switch.

Submit the typed preference edit via `SettingsSaveWorkerRegistry` on the canonical path and transaction lock. GPUI returns from startup before the egui main-loop hotkey reconfiguration receiver is installed. The general Settings worker always requests a hotkey ticket. The scoped non-invocation preference edit needs genuine store acknowledgement on its own transaction. Validate the current typed settings under lock, check the owned baseline, and atomically update that raw JSON leaf while preserving all other JSON values. Already committed desired state can acknowledge directly. A conflicting value or malformed input preserves the bytes and leaves the draft retryable. Follow existing missing and empty default behavior.

On the current terminal receipt, advance the editor baseline and apply the real GPUI completion preference. Reuse the live Search query to refresh suggestions and reset the cycle while keeping its existing text, caret, result selection and history. Hidden or unfocused root must still receive the worker receipt. Ordinary quit cancels eligible pending work and uses checked registry retirement. Test opaque values, conflicting owned data, malformed input, current acknowledgement and failure. Use the owned Unicode completion fixture for native Save, second Save, Cancel, write failure and same-profile reopen. This verifies one control within SETTINGS-04. The broader Settings controls and G06-D round trip are pending. The current egui close writer uses typed `Settings::update` and can drop unmodeled JSON fields, so G06-D must account for that writer.

`G06-EGUI-COMPLETION-PREFERENCE-SAVE` verified the exact search result focus, draft-only toggle, real save acknowledgement, Cancel and immediate live preference in the same egui process. [Validation log](../VALIDATION_LOG.md#g06-settings-completion-preference-native-baseline)
