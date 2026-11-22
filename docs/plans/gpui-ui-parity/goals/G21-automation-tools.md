# G21 — Paste Sequences, Mouse Gestures, and Hot Folder

**Status: not started.** Checkpoints A–D are not started. Use the initiative's single branch. Keep egui as the no-argument default and GPUI as the opt-in native frontend.

## Objective and dependencies

Recreate all three automation tool families, including editors, source/action pickers, runtime status, settings, confirmations, and hidden-launcher operation. Preserve their existing execution services and explicit action boundaries. Opening a manager or preview must never start automation as a side effect of the UI migration.

Requires G00–G04 and G05 common routes, utility hosts, settings integration, focus, and confirmations. Consume existing clipboard/snippet, macro, hotkey, and command services through their shared boundaries. Track dependencies on G19 for macro target authoring and G24 for Hotkey Directory navigation without duplicating those editors. Follow [REQUIREMENTS](../REQUIREMENTS.md) and [VALIDATION_STRATEGY](../VALIDATION_STRATEGY.md).

G06 owns the Settings shell; G21 owns specialized Paste/Gesture/Hot Folder controls and child pickers end-to-end. Their acceptance remains pending until this goal verifies them.

## Source seeds and ownership

Baseline: `multiplex55/Multi_Launcher` at `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

- [Paste Sequences UI](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/paste_sequence_dialog.rs), with `src/paste_sequence.rs` and `src/paste_sequence/` owning admission/playback.
- [Gesture editor](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/mouse_gestures_dialog.rs) and [gesture settings](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/mouse_gesture_settings_dialog.rs).
- [Gesture plugin settings](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/plugins/mouse_gestures.rs): contains additional settings UI and a window-ignore picker outside `src/gui`.
- [Hot Folder manager](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/hotfolder_dialog.rs), its dispatch/runtime adapters, and `src/hotfolder/`.

Application-owned services retain hotkeys, mouse hooks, clipboard ownership, file watchers, execution, persistence, and authorization fingerprints. GPUI owns draft editors, presentation, and user intents. Closing or docking a UI must not recreate its worker or change a service's run state.

## Per-GUI parity scope

Paste Sequences includes saved-list search, ordered steps, snippet/clipboard source pickers, literal-text editing, hidden previews, reordering, optional Start hotkey, Save, Run Once, runtime status, Cancel Run, and Playback Settings. Preserve Automatic, Manual advance, and Paste-to-Advance, including their distinct delay rules. Source selection does not paste or evaluate templates; insertion revalidates identity. Preserve content-save/shortcut-partial-success reporting, current-target behavior, draft retention, explicit clipboard actions, and newer-external-clipboard ownership.

Mouse Gestures includes the gesture list/tree, recording/editing, labels, all binding kinds, action/query choices, conflicts/search, delete confirmation, corruption/read-only recovery, statistics where exposed, dedicated settings, plugin settings, and ignore-window selection. Include the native gesture trail/hint presentation wherever the existing UI owns it. Preserve right-click pass-through/cancel/no-match behavior, stable tokens, source revision checks, and settings Save/Cancel semantics. Do not replace settings-only popup content with a placeholder.

Hot Folder includes rule list, new/edit forms, root/destination pickers, filters/conditions, action or exact macro selection, sample-file picker, Test Rule, enable/disable, strong confirmation, Pause/resume, runtime status, terminal history, and receipts. New/imported/changed rules remain disabled pending the existing enable flow. Test Rule remains read-only and cannot admit live work. Closing the manager leaves the app-owned service running; Pause and plugin disable retain their distinct semantics.

## Checkpoints

### G21-A — Establish explicit service/view boundaries

**Status: not started.** **Owner:** G21 implementer; existing automation services own execution.

**Work:** inventory every UI entry and settings callback; map draft, saved, acknowledged-runtime, and transient states; bind GPUI to existing service snapshots and typed intents.

**Invariants:** no duplicate hooks/watchers; rendering causes no execution; stale selections cannot bind replaced sources.

**Evidence:** source-to-parity rows, reference runs, and focused admission/state tests.

**Done:** three GPUI hosts can inspect the existing state without changing it.

#### A1 — Share the passive saved-sequence catalog

Project the existing `SequenceSnapshot` revision and ordered rows of stable `SequenceId`, name and step count in `src/paste_sequence.rs`. Preserve empty and load-error states. Keep `SequenceStore` as persistence owner. The projection excludes step bodies, protected clipboard content, prepared runs and shortcut state. In the existing egui `PasteSequenceDialog::saved_sequence_list`, render the lightweight rows and resolve an accepted ID against the owner snapshot before selecting it. Preserve directory reveal, native widget and Jump Mode identities, drafts and playback state.

The real Open route captures the foreground origin and protected clipboard context before focusing the editor. The current GPUI startup returns before common plugin, invocation and playback owner construction. The first production GPUI host depends on shared routing and an owned protected capture boundary. Runtime status comes from actual playback and main's acknowledged `SequenceRoutePresentation`.

Verify exact order, IDs, counts and exclusion of synthetic body data, then select `paste_saved_row_native_identity_survives_removal_of_an_earlier_row` and `search_actions_contain_only_stable_ids_and_command_opens_editor`. Use only a temporary data store and synthetic context with both Paste Sequence shortcut settings disabled before application construction. A1 ends when the actual egui list uses the projection, selection leaves playback idle and fixture bytes stay unchanged. G21-A still requires the three actual GPUI inspection hosts.

### G21-B — Complete Paste Sequences

**Status: not started.** **Owner:** GPUI sequence editor/pickers; shared playback remains authoritative.

**Work:** implement every source, mode, preview, shortcut, status, settings, and cancellation path. Connect normal and Hotkey Directory entry points without overwriting an open draft.

**Invariants:** hidden bodies stay masked; Run Once stays memory-only; newer clipboard contents are not restored over; configured and acknowledged shortcuts remain distinct.

**Evidence:** production-view tests, bounded playback with a disposable target, and clipboard ownership observations.

**Done:** every sequence/editor/settings row works and all three playback modes retain existing semantics.

### G21-C — Complete gestures and Hot Folder

**Status: not started.** **Owner:** GPUI gesture/Hot Folder adapters.

**Work:** implement recording/bindings, settings/ignore picker, Hot Folder authoring/test/confirmation/history, and live status. Reuse common pickers with source-specific validation.

**Invariants:** cancel preserves committed settings; edited rules revoke prior enable authorization; Test Rule has no live effects; source failures retain last-good state as currently specified.

**Evidence:** focused UI/service tests, temporary-folder rule fixtures, and native right-click/cancel behavior.

**Done:** both tools and all nested/settings-only surfaces match the baseline.

### G21-D — Verify hidden operation and cross-tool ownership

**Status: not started.** **Owner:** G21 implementer; review hotkey, clipboard, and service lifecycle changes.

**Work:** test hide/reopen, pop-out/dock, restart persistence, conflicting shortcuts, lost targets, and affected egui paths. Reconcile every settings callback against the inventory.

**Invariants:** UI closure does not duplicate or accidentally stop background services; no GPUI route opens egui; rejected actions do not silently mutate bindings.

**Evidence:** scoped results, native observations, parity closure, and checkpoint commits.

**Done:** all mandatory service, UI, and native evidence is recorded.

## Focused validation and continuation

Existing targets include `mouse_gestures_db`, `mouse_gestures_service`, and `mouse_gestures_ui`. Exact tests include `paste_settings_toolbar_opens_main_section_without_losing_an_open_draft`, `paste_sequence_editor_third_mode_preserves_delay_and_run_once_stays_prompted_memory_only`, `hosted_picker_checks_session_epoch_and_current_ignore_basis`, `gesture_delete_is_queued_until_confirmed`, `hotfolder_editor_popup_and_path_ticket_are_bound`, and `m10e_test_rule_reads_sample_metadata_without_live_runtime_effects`. Discover current compiled targets and select affected tests; add GPUI cases for changed behavior boundaries.

Finish only when all editors, runtime displays, nested pickers, and settings are verified. Commit coherent checkpoints, update the ledger, and continue the next ready goal. Keep blocked native evidence explicit. Do not run full nextest for each automation form or revive historical broad acceptance procedures.
