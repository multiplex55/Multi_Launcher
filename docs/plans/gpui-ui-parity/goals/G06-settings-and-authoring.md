# G06 — Settings, plugins, action authoring, and recovery

**Status:** The first GPUI Settings menu, bare typed route and retained autocomplete preference passed the scoped Windows and Rust verification. The broader editor, its child surfaces and the full settings round trip remain in implementation. [First native Settings save](../gui/settings-main.md#first-native-settings-save)

The ordinary egui close prerequisite passed native and focused Rust verification. EgUI close saved root geometry while preserving unowned Settings values. GPUI reopened the same profile and the checked reverse restart continued to refuse a failed close write. [G06-D ordinary close evidence](../VALIDATION_LOG.md#g06-d-ordinary-egui-close-preservation)

GPUI Favorites has scoped native and focused evidence for List/Grid prefill, typed Browse, exact-case named drafts, Add/Edit/Remove, compact layout, saved pin, and malformed-store refusal and recovery. Folder and bookmark authoring passed the scoped GPUI and egui round trip. The saved custom App Editor catalog and checked Edit/Remove passed the compact Windows replay, including stale refusal and refresh, exact field round trip and same-store egui readback. Indexed App Editor rows, Browse, Repair, Explorer drop, full provider coverage and Data Recovery decisions remain open. [Favorites evidence](../VALIDATION_LOG.md#g06-folder-and-bookmark-favorites-authoring) · [App Editor evidence](../VALIDATION_LOG.md#g06-saved-custom-app-editor-catalog-and-checked-editing)

**Owner:** G06 settings/authoring implementer, with G02 for persistence and plugin contracts. Use the existing migration branch.

## Objective and dependencies

Provide all configuration and authoring interfaces in GPUI Kit while retaining exactly the same saved data, validation, runtime application, and recovery semantics. The GPUI frontend must let the user configure the whole product, including plugin-specific settings and uncommon options; a reduced settings panel does not satisfy parity.

G00–G04 are prerequisites: inventory, compatible Windows runtime, shared operations, selector/data ownership, and Kit foundation. G05 supplies launcher routes. Feature owners own their specialized plugin controls and child dialogs; this goal owns the settings shell, page registration, embedding, lifecycle, general controls, and explicit evidence handoff. G06 shell acceptance does not depend on every later feature being finished, and cannot accept that feature's controls on its behalf. Full settings parity is reconciled in G25. Follow [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md).

## Verified source entry points

Existing pinned sources at `f7bc610a86bcad1c14b69dce65b1d804df1b7934` include:

| Responsibility | Existing source |
|---|---|
| General settings | [src/settings_editor.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/settings_editor.rs), `src/settings_editor/{catalog,mapping,render,save,state}.rs`, `src/gui/settings_window_host.rs` |
| Storage and acknowledgement | `src/settings/{model,io,store,utility_dialog_sizes,utility_presentations}.rs`, `src/settings_save.rs`, `src/gui/utility_presentation.rs` |
| Plugins | [src/plugin.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/plugin.rs), `src/plugin_editor.rs`, `src/plugins/settings.rs` |
| Authoring and recovery | `src/actions_editor.rs`, `src/gui/{add_action_dialog,alias_dialog,bookmark_alias_dialog,add_bookmark_dialog,tempfile_alias_dialog,fav_dialog,shell_cmd_dialog,data_recovery_dialog}.rs`, `src/target_repair.rs` |
| Theme and hotkey registration | `src/gui/theme_settings_dialog.rs`, `src/gui/theme/`; `src/gui/hotkey_directory_dialog.rs` is G24's GUI, linked from this shell |

These paths are verified entry points; the full setting/control catalog must come from G00 and local source/runtime inspection, not this abbreviated table.

## Per-GUI scope

Recreate Settings and all categories, editor navigation/footers, validation, save feedback, reset/default and conflict decisions, plugin enablement/capabilities, and action authoring. Include Add Action and its Repair mode, aliases, bookmarks, temporary-file aliases, favorites, shell-command authoring, and confirmations. Include Theme Settings, Data Recovery, and safe-startup diagnostics. G06 owns the Hotkey Directory registration/link only; G24 owns its actual GUI and acceptance.

Feature-specific mouse, gesture, MultiManager, radial, dashboard-widget, or tool settings must appear in the shared inventory with one responsible goal. G06 accepts their shell registration against the approved page contract; their actual controls and end-to-end behavior retain the feature owner's pending status. G00 assigns any otherwise unowned custom plugin settings. Temporary-file content creation belongs to G13; macro bodies and radial authoring belong to their dedicated goals. G06 owns ShellCmd authoring; G24 owns shell runtime integration.

## Bounded checkpoint packets

### A — Define shared edits, persistence, and plugin presentation contracts

**Owner:** G06 with the engine/plugin owner. Map draft → validation → accepted edit → save request → acknowledgement → runtime application. Separate accepted local intent from the latest persisted revision. Identify egui-owned business behavior before moving it to shared operations.

The current `Plugin::settings_ui` accepts `&mut egui::Ui`, and the plugin editor uses egui context and `LauncherApp`. Establish a concrete GPUI presentation route for every affected built-in and supported dynamic plugin; preserve its engine capability and loading lifetime. Do not infer UI compatibility from successful plugin search.

**Invariants:** Unknown/unsupported stored values survive unchanged; failed saves cannot silently replace persisted data or label an unsaved draft as saved. Both frontends use the same data root and formats. **Evidence and acceptance:** Contract/ownership tables and focused old-path persistence checks. **Commit boundary:** Required neutral settings/plugin contracts and egui adaptations.

### B — Recreate Settings, theme, and plugin management

**Owner:** G06 frontend implementer. Build all catalogued controls with Kit, including numeric/text validation, selectors, collection editors, dependent enabled states, descriptions, and errors. Port plugin catalogue selection, capability toggles, and per-plugin settings. Include the frontend preference through G03's existing configuration route without silently changing the development default.

**Invariants:** Tab/focus order and discovery remain familiar; an asynchronous save applies to its originating revision. Theme changes update actual GPUI surfaces and preserve user text scaling. Host movement cannot reset drafts or submit duplicate saves.

**Evidence and acceptance:** Per-category/control mapping, representative invalid/pending/failure cases, and comparison of normalized persisted data. Every plugin callback has a registered GPUI page contract and evidence owner; unfinished specialized controls remain pending in that owner's goal. **Commit boundary:** Complete shared Settings/Plugins shell and generic controls, with unfinished specialized children clearly linked.

### C — Complete action authoring, recovery, and diagnostic children

**Owner:** G06 frontend implementer, using authoritative action/recovery services. Recreate add/edit/remove forms, Add Action's Repair mode, alias validation, favorites ordering, shell fields, confirmations, import/export where present, and Data Recovery decisions. Preserve selection and drafts across external changes. Repair must retain target identity/revision and parent native choosers to the active owner; stale chooser completion cannot repair another target.

**Invariants:** A stale row cannot edit/delete another item. Cancel performs no save. Recovery acts on the reviewed source and retains current safe-startup behavior. Hotkey Directory links preserve G24's route without claiming its admission/effective-state UI accepted here.

**Evidence and acceptance:** Disposable-data demonstrations cover create/edit/cancel/remove, invalid aliases, save failures, repair target changes during chooser use, and recovery decisions. **Commit boundary:** Authoring and diagnostic surfaces with feature-scoped fixes.

### D — Verify cross-frontend configuration round trips

**Owner:** G06 acceptance owner. Open existing settings/data in GPUI, make bounded edits, restart in egui, verify behavior, then perform the reverse. Exercise a settings save while the root is hidden and another save against the newly acknowledged baseline. Check dock/popout geometry and remembered presentation without conflating them with persistent business data.

**Invariants:** Unknown fields remain intact; startup diagnostics do not overwrite damaged inputs with defaults; plugin runtime changes occur once. **Evidence and acceptance:** Before/after data comparisons, selected regression output, and Windows interaction evidence. **Commit boundary:** Final corrections and accepted inventory rows; shared changes invalidate affected prior acceptance.

## Focused validation

Existing entry points include `tests/settings_editor.rs`, `tests/settings_plugin.rs`, and domain cases for theme settings/dialogs. The validation survey verified the inline test `settings_save::tests::hidden_settings_editor_completes_and_a_second_save_uses_the_new_baseline`; list it before selecting it with `--lib`. Discover exact plugin-save, data-recovery, and action-authoring filters from the current source rather than assuming each file is a separate Cargo target. Use small isolated configuration fixtures for migration/opaque-value and save-failure cases. Native settings-host movement, focus, and visible save feedback still require Windows evidence.

## Exit and continuation

Every G06-owned general settings, shell, plugin-management, and authoring row has an implementation and evidence; every specialized setting has a registered page contract and a separate named feature/evidence owner. No plugin UI is silently omitted or drawn with egui inside GPUI. Scoped G06 acceptance does not accept pending specialized pages; those remain open in the global inventory until their feature goals and G25 validate them. All owned data round trips pass the agreed semantics. Continue the feature program and revalidate affected shell/persistence behavior as later tools land.
