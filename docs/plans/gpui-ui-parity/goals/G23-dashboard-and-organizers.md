# G23 — Dashboard, Calendar, Todo, timers, and activity surfaces

**Status: The Toast Log tail, Refresh, Close and OS handoff have scoped acceptance.** The native viewer is paired with the G05-C Help entries. Toast Log geometry and the larger Dashboard and organizer checkpoints remain pending. Execute on the initiative's single branch. Preserve default egui and opt-in native GPUI. [Toast Log evidence](../VALIDATION_LOG.md#g05g23-gpui-file-close-and-toast-log)

## Objective and dependencies

Recreate every dashboard widget and widget-specific settings form, the dashboard editor, Calendar and its children, Todo editors/views, timer/alarm controls, Notification Inbox, Activity, Toast Log, and associated notification presentation. Preserve existing scheduling, reminder, persistence, filtering, and command semantics. Dashboard completion means every registered widget works, not merely the grid shell.

Requires G00–G04 and G05 common routes, hosts, focus, actions, notifications, and settings integration. G06 owns the Settings shell; G23 owns these specialized settings controls. Reuse feature services owned elsewhere for notes, snippets, clipboard, windows, gestures, and system controls; record dependencies on their completed adapters. Follow [REQUIREMENTS](../REQUIREMENTS.md) and [VALIDATION_STRATEGY](../VALIDATION_STRATEGY.md).

## Source seeds and architectural ownership

Baseline: `multiplex55/Multi_Launcher`, `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

- [Dashboard registry](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/dashboard/widgets/registry.rs), `src/dashboard/dashboard.rs`, and every widget/render/settings implementation under `src/dashboard/widgets/`.
- [Dashboard editor](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/dashboard_editor_dialog.rs).
- [Calendar editor](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/calendar_event_editor.rs), `calendar_event_details.rs`, `calendar_popover.rs`, `calendar_window_host.rs`, and `src/plugins/calendar.rs`.
- [Timer/alarm UI](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/timer_dialog.rs), `todo_dialog.rs`, and `todo_view_dialog.rs`.
- [Notification Inbox](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/notification_inbox_dialog.rs), its runtime, `activity_dialog.rs`, and `toast_log_dialog.rs`.

Existing domain stores, schedulers, caches, notification producers, and service actions remain authoritative. GPUI owns widget/view instances and their presentation state. Replace egui-specific widget factories/settings callbacks with frontend-appropriate adapters while preserving widget keys, configuration, and shared data ownership.

## Per-GUI parity scope

Assign one parity row per registered widget and per settings form. The current registry includes weather_site, plugin_home, command_history, diagnostics, context_links, recent_commands, frequent_commands, gesture_cheat_sheet, gesture_recent, gesture_health, todo, layouts, recent_notes, pinned_commands, pinned_query_results, query_list, timers, clipboard_snippets, clipboard_recent, browser_tabs, calendar, processes, windows, windows_overview, system, system_controls, system_status, now_playing, snippets_favorites, scratchpad, notes_recent, notes_tags, notes_graph, todo_focus, quick_tools, recycle_bin, tempfiles, volume, and stopwatch. Reconcile the registry and other reachable widgets at implementation time.

Dashboard editing includes adding/removing/reordering/configuring widgets, layout spans, selection, preview/list marker correspondence, hover, conflicts, drag behavior, save failures, and docking/pop-out. Session markers must not become persisted widget IDs. Every `settings_ui` callback outside `src/gui` needs explicit GPUI ownership.

Calendar includes day/week/month, navigation, upcoming/overdue, event details/editing, recurrence, snooze, validation, reminders, and cross-tool prefills. Todo includes entry/edit/view, filters/tags/priority, completion, confirmation before clearing, persistence failures, and existing note synchronization. Timers/alarms include duration/date/time fields, keyboard focus, sound selection/preview, Start, running controls, and actual current-local-clock semantics.

Inbox includes search/filter/read/pin/clear, source indicators, related actions/snooze/Create Todo, settings and Windows-capture consent/status. Activity includes filters, safe metadata, owner-tool navigation, cancellations/actions where available, and throttled refresh. Toast Log and transient notifications retain actions, dismissal, and current enablement policy. Preserve separate privacy/retention settings and do not enable Windows notification capture during migration.

## Checkpoints

### G23-A — Inventory registry-driven UI and shared data

**Status: not started.** **Owner:** G23 implementer, existing stores and cache/scheduler owners.

**Work:** enumerate widget descriptors/settings callbacks, organizer children, and notification actions; map GPUI instances to existing data snapshots and intents.

**Invariants:** widget/configuration keys stay compatible; one scheduler/producer exists; opening a widget does not trigger its action.

**Evidence:** registry-to-parity mapping, saved dashboard fixtures, and scoped configuration tests.

**Done:** every reachable dynamic surface is owned, including forms not declared in `src/gui`.

### G23-B — Complete dashboard and editor

**Status: in implementation.** The retained Dashboard, Active Timers and manual Quick Tools passed focused verification. The checked Dashboard Editor also has focused evidence for primary layout editing and Save. Paired Windows replay and the remaining widgets, Properties and editor hosts are open. [Dashboard checkpoint](../CHECKPOINTS.md#p101-retained-dashboard-with-active-timers-and-manual-quick-tools) · [Dashboard Editor checkpoint](../CHECKPOINTS.md#p102-checked-dashboard-layout-editor) **Owner:** GPUI widget factories, settings adapters, and dashboard editor.

**Work:** implement every registered widget, settings form, layout/editor interaction, empty/error/loading state, and route into owner tools. Preserve visibility-driven updates and caches.

**Invariants:** resize/drag do not change unrelated settings; duplicate IDs/conflicts remain handled; failed saves retain retryable drafts.

**Evidence:** production-view layout/action tests, settings round trips, and representative dense-dashboard native review.

**Done:** all widgets and settings work; no placeholder or egui renderer remains on GPUI paths.

### G23-C — Complete organizers and activity/notification views

**Status: not started.** **Owner:** GPUI Calendar/Todo/timer/Inbox/Activity adapters.

**Work:** implement all listed parent/child views and settings, preserved clock/recurrence semantics, error recovery, and owner-action navigation.

**Invariants:** drafts do not schedule until committed; deleted events cannot be resurrected by stale editors; timer Start occurs once; safe metadata stays sanitized.

**Evidence:** focused state/clock tests, production-view keyboard interactions, and native reminder/action checks.

**Done:** all organizer and notification rows match existing behavior and persistence.

### G23-D — Close cross-tool and refresh gaps

**Status: not started.** **Owner:** G23 implementer; review targets registry completeness and background ownership.

**Work:** reconcile dynamic forms, monitor/DPI layouts, owner-tool handoffs, hide/reopen, restart state, idle refresh, and affected egui behavior.

**Invariants:** hidden views do not start redundant polling; closing UI does not discard scheduled work; GPUI never opens egui children.

**Evidence:** scoped logs, native captures, registry reconciliation, resource observations, and checkpoint commits.

**Done:** all applicable rows are verified and no unowned dynamic widget remains.

## Focused validation and continuation

Existing targets include `dashboard_config`, `todo_plugin`, `timer_plugin`, `timer_refresh`, and `toast_log`. Actual unit tests include `preview_and_slot_list_share_token_hover_without_editing_config`, `failed_coalesced_save_and_close_keeps_editor_open_and_draft_retryable`, `editor_commit_uses_latest_valid_event_and_rejects_corruption`, `clear_completed_requires_confirmation`, `alarm_preview_and_start_use_the_current_local_clock`, and `visible_refresh_is_throttled_and_closed_refresh_is_inert`. Discover current target filters and run only affected coverage.

Complete this goal only after the registry, child windows, settings, and required native evidence are reconciled. Commit accepted checkpoints, update the ledger, and continue; missing dependent owner-tool functionality remains a tracked blocker. Use the shared validation budget rather than full nextest after each widget.
