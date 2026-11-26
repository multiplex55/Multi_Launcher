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

### Launcher and typed entry

The first Settings checkpoint joins the relevant G05-C launcher handoff with this G06 editor. In `src/gpui_frontend.rs`, use Kit `Button::dropdown_menu` and a `PopupMenu` action targeting the current launcher root focus scope. The Settings menu item **Edit Settings** toggles the retained docked editor. The menu owns navigation and Escape, then restores the originating focus unless the selected action hands focus to the editor. Keep menu availability independent of the Settings plugin. Use the same idle cancel and close operation when the menu toggles an open editor. Pending Save keeps the editor and its admitted write active.

Admit the bare `DialogCommand::Settings` invocation through the existing installed route, shared provider admission and typed live result activation. The command opens or refocuses the editor and retains exact Search query and result state. Filter the Settings results to installed routes before composer formatting and cached fallback admission. The plugin's Search capability controls its **Open settings** provider row. Its enabled state controls the cached bare command and completion label, consistent with the existing launcher catalog. Explicit saved actions follow the shared local-action rules. Typed Settings search and its raw suffix are a later slice. `SettingsPlugin::search` accepts its suffix verbatim in direct tests, while the current launcher filter normalizes ordinary provider input through `split_action_filters`. Verify actual launcher input when supplying the full typed search route.

Fresh editor admission loads the current canonical baseline off the UI thread. Only the current editor can consume the result. The actual Kit checkbox can own the initial focus through `base::Checkbox::track_focus`. Opening or refocusing Settings reveals its panel in the current root viewport, including when a docked Hash panel is mounted. Build the panel as a child during layout to keep its builder frame out of the root render call stack. Editor-scoped Escape cancels an idle draft and respects the pending Save owner. On current active root close, return focus to Search when it is the original owner and the handoff has not been superseded. Keep the existing keyboard ownership boundaries for Hash, Jump Mode and popup or restart confirmation. Exclude the Settings editor from `root_tab_traversed_to_search` completion admission, and check the popup focus path for the same protection. Ordinary toolbar traversal remains eligible. Include current Settings admission in the restart snapshot so dirty or pending sessions cannot release the data owner. Preserve the existing checked registry retirement and real save acknowledgement below.

Verify the true menu keyboard and pointer routes, popup-only Escape, bare typed repeat-open preserving the draft, and idle toggling and cancellation. Use the owned Unicode completion profile for false and true writes, live suggestion changes with retained Search text and selection, and same-profile reopen. Exercise a write failure and dirty restart refusal within the existing pending and acknowledgement cases. This increment addresses the scoped Settings entry and typed dispatch in LAUNCH-06/08/09 and the first control and lifecycle guards in SETTINGS-01/04/06. Broader section and search behavior, additional editors, popout, geometry, and cross-frontend persistence remain separately tracked.

### Acknowledged preference write

Submit the typed preference edit via `SettingsSaveWorkerRegistry` on the canonical path and transaction lock. GPUI returns from startup before the egui main-loop hotkey reconfiguration receiver is installed. The general Settings worker always requests a hotkey ticket. The scoped non-invocation preference edit needs genuine store acknowledgement on its own transaction. Validate the current typed settings under lock, check the owned baseline, and atomically update that raw JSON leaf while preserving all other JSON values. Already committed desired state can acknowledge directly. A conflicting value or malformed input preserves the bytes and leaves the draft retryable. Follow existing missing and empty default behavior.

Retain raw unowned JSON values in the patch transaction so opaque numbers and nested values survive without precision loss. Use the existing atomic store writer and preserve the recorded recovery boundary.

The owned `settings-g06-typed-opaque-exact-number-seed.json` (`F7999AD7…`) extends the verified Settings-enabled profile with two synthetic signed large integers in an unknown nested object. It can verify the one-leaf write without using a production settings file.

## Reload application index

The GPUI Settings action reads the committed `index_paths` and `max_indexed_items` through the shared Settings transaction and shows the scan result. It is available with a clean or dirty Query Autocomplete draft when no Settings read, save or index scan is active. Reload keeps the draft and Settings bytes intact. The egui Settings Save applies its acknowledged index configuration through the stock coordinator. [Launcher index behavior](launcher.md#live-application-index-reload)

On the current terminal receipt, advance the editor baseline and apply the real GPUI completion preference. Reuse the live Search query to refresh suggestions and reset the cycle while keeping its existing text, caret, result selection and history. Hidden or unfocused root must still receive the worker receipt. Ordinary quit cancels eligible pending work and uses checked registry retirement. Test opaque values, conflicting owned data, malformed input, current acknowledgement and failure. Use the owned Unicode completion fixture for native Save, second Save, Cancel, write failure and same-profile reopen. This verifies one control within SETTINGS-04. The broader Settings controls and G06-D round trip are pending. The ordinary egui close prerequisite is recorded below.

For the scoped Windows write-failure check, hold the owned Settings file open for shared reads without delete sharing. Verify that the real atomic replacement reports failure, the prior bytes and live completion preference remain effective, and the editor keeps the draft. Release the test handle before retrying Save.

`G06-EGUI-COMPLETION-PREFERENCE-SAVE` verified the exact search result focus, draft-only toggle, real save acknowledgement, Cancel and immediate live preference in the same egui process. [Validation log](../VALIDATION_LOG.md#g06-settings-completion-preference-native-baseline)

`G06-EGUI-TYPED-SETTINGS-ENTRY` verified the actual installed **Open settings** result, fresh Enter into the real editor and Cancel with the launcher query and selected row retained. Its owned Settings seed also enables Unicode completion and preserves the synthetic opaque fields at input. [Validation log](../VALIDATION_LOG.md#g06-settings-completion-preference-native-baseline)

`G06-EGUI-CACHED-SETTINGS-CAPABILITY` verified that an enabled Settings plugin with an explicit empty capability list still offers its cached **settings** row. The row opens the actual editor and preserves the launcher query. [Validation log](../VALIDATION_LOG.md#g06-settings-completion-preference-native-baseline)

`G06-GPUI-SETTINGS-LAYOUT-SAVE`, `G06-GPUI-SETTINGS-FINAL-INPUT`, `G06-GPUI-SETTINGS-EMPTY-SOURCE-DEFAULTS` and `G06-FIRST-SETTINGS-FOCUSED` verify the scoped GPUI checkpoint. It covers menu and typed entry, an acknowledged autocomplete preference, UI retry after a failed Windows write, compact focus restoration and the current restart guards. The 19 focused Rust tests passed. Settings search, the remaining controls, popout, child editors and the full round trip are still pending. [Validation log](../VALIDATION_LOG.md#g06-settings-completion-preference-native-baseline)

`G06-OPAQUE-SETTINGS-CLOSE-BASELINE` confirmed on the same Windows executable and owned seed that egui ordinary close dropped both an unknown top-level object and an unknown nested Dashboard value. GPUI ordinary close preserved the exact seed. Verify the egui close writer and accepted writes when closing the G06-D cross-frontend round trip. [Validation log](../VALIDATION_LOG.md#g06-settings-completion-preference-native-baseline)

### Ordinary egui close prerequisite for the shared round trip

Both direct and deferred egui shutdown call `Settings::save_launcher_close_preferences`. The checked atomic transaction writes root size and pins, optional dirty File Search UI preferences, and any pending Calculation Pad display preferences. It retains raw sibling values under the canonical Settings lock. A missing or whitespace-only store starts from serialized `Settings::default`, and a sparse object keeps its unowned shape. Equal values still attempt the replacement and propagate failure through the checked restart receipt.

`G06-CLOSE-NATIVE-ROUND-TRIP` verified ordinary egui close on the archived cross-frontend receipt and GPUI reopen with the same unowned values and preferences. `G03-C2B-REVERSE-CLOSE-WRITE-FAILURE-REPLAY` verified the checked Windows refusal. Ten focused cases passed for the store and both shutdown paths. This prerequisite covers idle root close and deferred close-owned preferences. Utility resize, other immediate writers, explicit Settings Save and recovery have separate transactions and evidence. [Validation log](../VALIDATION_LOG.md#g06-d-ordinary-egui-close-preservation)

Existing JSON object roots keep unowned raw member values. A legacy positional root accepted by the typed Settings reader is canonicalized through that reader, preserving its decoded settings with the same normalization as the previous close writer.

## Live Actions and Feedback

The retained GPUI Settings pane offers Clear Query, Hide Window, Enable Toasts, Toast Duration, Inline Errors and Error Toasts. Save Actions and Feedback writes the changed fields in one checked transaction. Its acknowledgement applies the values to later launch admissions. Existing launch attempts keep their captured preferences.

Use paired owned profiles at 1516 × 937 with matching display scale and both Paste Sequences shortcuts disabled. Open Settings through Menu and Search, change two preferences, and check a subsequent owned application launch for the resulting query, visibility and feedback. Toggle toast notifications and edit duration using the visible control. Exercise Cancel and a controlled read-sharing replacement failure with a retry. Reopen the same owned profile through egui and compare the touched values and raw unowned siblings. Native replay remains open.

## Live Search Results

The Search Results section applies the acknowledged Autocomplete, Match Exact, weights, Page Jump and Grid preferences to the current launcher. Page Jump preserves the selected row, while Grid changes update the presentation of current results. Ranking changes refresh ordinary results while preserving Calculator scalar state. Grid Rows and Plugin Opt-Out authoring remain open.

Extend the paired Settings replay with a retained query and known result order. Change one ranking field and Grid, confirm the anchored query stays in place and check the updated rows. Verify Page Jump retains the selected result. Reopen through egui and verify the stored values and untouched layout keys. [Search Results checkpoint](../CHECKPOINTS.md#g06-and-g05-live-search-results-settings)
