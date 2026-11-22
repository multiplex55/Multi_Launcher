# G24 — MultiManager and system-control interfaces

**Status: not started.** All checkpoints are not started. Work on the initiative's single branch. egui remains the no-argument default and GPUI remains native and opt-in.

## Objective and dependencies

Recreate MultiManager and its settings, CPU/process-related views, volume/brightness controls, Hotkey Directory, and specialized system/plugin settings. Preserve saved layout and shell execution integrations and every existing system-action route. Complete the residual system-UI inventory without inventing new interfaces for command-only functions.

Requires G00–G04 and G05 common routes, hosts, focus, confirmations, and service boundaries. G06 owns the Settings shell and ShellCmd authoring UI; G24 owns specialized system settings and shell execution integration. G23 owns dashboard widgets using these services. G22 owns Mouse Settings/coordinate overlay appearance. Resolve overlapping entry points through these owners. Apply [REQUIREMENTS](../REQUIREMENTS.md) and [VALIDATION_STRATEGY](../VALIDATION_STRATEGY.md).

## Actual source seeds and ownership

Baseline: `multiplex55/Multi_Launcher`, `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

- [MultiManager UI and settings](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/multi_manager/ui.rs), outside `src/gui`; `src/gui/multi_manager_actions.rs` and the remaining `src/multi_manager/` modules.
- [Brightness UI](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/brightness_dialog.rs), `src/brightness_service.rs`, [Volume UI](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/volume_dialog.rs), `volume_data.rs`, and `cpu_list_dialog.rs`.
- [Hotkey Directory](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/hotkey_directory_dialog.rs) and `src/hotkey/directory.rs`.
- [Virtual desktop settings](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/plugins/virtual_desktop.rs), [network settings](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/plugins/network.rs), `src/plugins/layout.rs`, `layouts_storage.rs`, `processes.rs`, `task_manager.rs`, and `shell.rs`.

Existing Windows/process/audio/display/hotkey/desktop services remain authoritative. GPUI owns controls, snapshots, selections, and user intents. Native APIs and existing command behavior are reused, not reimplemented as UI-specific business logic.

## Per-GUI parity scope

MultiManager includes workspace cards, Add/Rename/Delete/reorder, Save/Reload, HWND snapshots, reconnect status, refresh titles/desktops, Send Home/Move Target, single/multiple capture, recapture, per-window home/target rectangle editing, swaps, ordering/deletion, hotkey editor, virtual-desktop selection, confirmations, status tones, and settings. Preserve distinction from simple saved layouts and the existing bounded explicit reconnect behavior. Geometry and window identity remain validated by existing services.

Volume includes master and process/session rows, current values, drafts, explicit Set/Mute, loading/error/empty states, bounded scrolling, and close/pin behavior. Brightness includes asynchronous monitor discovery, independent row drafts, explicit Set, Refresh, cancellation, unavailable capability, identity loss, and cleanup failures. A stale selected row must not apply to a different monitor. CPU/process displays retain update policy and action availability.

Hotkey Directory includes metadata search/filter/sort, categories/status, conflicts, configured versus acknowledged runtime state, and navigation to exact current owners. Editing routes must revalidate identity/revision before opening macro, radial, gesture, sequence, or Settings editors; directory navigation must not replace an existing draft.

Virtual-desktop settings include launch-discovery timeout and opt-in application auto-switch rules: enabled state, executable/path/title/class matching, target-desktop picker/refresh, add/remove, and failure states. Network settings include refresh and units. Discover remaining reachable system/plugin settings callbacks and assign each explicitly; G06's generic container is not their implementation.

Preserve layout save/load/flags, process actions, OS Task Manager launch, media/system actions, and shell invocation through existing routes. An external Windows utility remains external. Do not manufacture a replacement Task Manager or a layout editor absent from the baseline. Verify G06's ShellCmd UI reaches the unchanged execution service and existing error reporting.

## Checkpoints

### G24-A — Reconcile system surfaces and effect owners

**Status: not started.** **Owner:** G24 implementer and existing Windows service modules.

**Work:** map all listed UI and dynamic settings callbacks; distinguish application views from OS/external actions; establish identity-aware GPUI snapshots/intents.

**Invariants:** no duplicated enumeration/registration service; refresh is not action execution; command-only behavior stays compatible.

**Evidence:** source-to-parity map, settings inventory, and focused service tests.

**Done:** every residual system surface has an explicit owner and no implicit egui callback remains unassigned.

### G24-B — Complete MultiManager and layout integration

**Status: not started.** **Owner:** GPUI MultiManager adapters; existing workspace/runtime services.

**Work:** recreate manager/settings/cards, capture/reconnect flows, geometry, desktop/hotkey children, statuses, confirmations, and saved-layout entry points.

**Invariants:** no spontaneous reconnect loop; stale HWNDs cannot receive unintended actions; cancel preserves committed workspace state.

**Evidence:** focused workspace/action tests and native capture/recapture/reconnect/move with disposable windows.

**Done:** all manager/child/settings operations work through GPUI with compatible persistence.

### G24-C — Complete controls, Directory, and plugin settings

**Status: not started.** **Owner:** GPUI system utility/settings adapters.

**Work:** implement CPU/volume/brightness, Hotkey Directory, virtual-desktop/network settings, remaining inventoried forms, and shell/action integration.

**Invariants:** explicit Set preserves current effect boundaries; stale identities cannot retarget operations; failed enumeration preserves actionable diagnostics.

**Evidence:** production-view tests and bounded native monitor/audio/hotkey-owner navigation checks.

**Done:** all controls, settings, and cross-owner routes meet parity, with no egui substitution.

### G24-D — Verify failure recovery and residual coverage

**Status: not started.** **Owner:** G24 implementer; review focuses on native identity and settings completeness.

**Work:** reconcile the remaining system inventory, unavailable devices/desktops, canceled capture, refresh failures, saved state, egui fallback, and shutdown cleanup.

**Invariants:** no action targets a reused identity; optional services retain their enablement; UI changes do not expand existing execution authority.

**Evidence:** scoped logs, native observations, coverage reconciliation, and checkpoint commits.

**Done:** all assigned system rows and required Windows checks pass.

## Focused validation and continuation

Existing targets include `multi_manager_launcher_actions`, `multi_manager_plugin`, `processes_plugin`, and `window_manager`. Actual tests include `settings_form_auto_shrinks_and_scrolls_to_save_without_mutating_settings`, `brightness_dialog_rows_keep_independent_drafts_until_explicit_set`, `brightness_dialog_refresh_does_not_retarget_a_removed_selection`, `hotkey_directory_ui_revalidates_owner_revision_and_current_edit_target`, and `settings_desktop_dropdown_is_on_demand_not_polled_each_frame`. Discover current compiled filters; run affected subsets and add only meaningful GPUI boundary tests.

Completion requires all owned specialized settings and system surfaces, plus applicable native evidence. Coordinate G25's whole-application reconciliation instead of declaring unowned residual UI out of scope. Record accepted checkpoints and continue; report genuine blockers with preserved work. Full nextest follows the shared validation strategy.
