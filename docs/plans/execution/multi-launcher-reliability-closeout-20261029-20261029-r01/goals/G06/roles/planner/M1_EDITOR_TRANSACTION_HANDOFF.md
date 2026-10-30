# G06 coherent editor and transaction handoff

Date: 2026-10-30. Status: READY_FOR_IMPLEMENTATION. Parent-supplied baseline: `7fe0fdf0874e040d59ee0fa4a711e6f925434882`, branch `codex/closeout-g06-jump-settings-and-focus-20261029-r01`, independent from bootstrap `02d0ac64`; authoritative integration `cda36e81`. Parent owns Git and the global ledger.

## Objective and checkpoint

Complete the missing Jump preference editor and its validated, acknowledged save in one coherent source checkpoint covering M1-A/M1-B and directly relevant M2 tests. Native M3 evidence follows the tested source; it is not a prerequisite to implementing this safe editor.

One writer owns this checkpoint. Do not split field rendering, mapping, transaction ownership and live-application tests into separate incomplete migrations.

## Source and UX contract

- `src/gui/jump_mode/config.rs::JumpModeChord` already parses/canonicalizes supported local chords and rejects reserved Enter/Escape/Backspace, ambiguous Ctrl+Alt, unsupported modifiers and malformed input. Default is Shift+Space. `HintAlphabet` already accepts 2–26 unique ASCII letters, normalizes case and preserves order. Saved malformed fields independently fall back through their existing deserializers.
- `src/settings/model.rs::Settings` already persists `experimental_jump_mode_enabled`, `jump_mode_activation_chord` and `jump_mode_hint_alphabet`. No schema change is needed.
- `src/settings_editor/state.rs::SettingsEditor` currently has only the Jump enable flag. `mapping.rs::from_settings` initializes it; `to_settings` intentionally copies the current typed chord/alphabet because no corresponding draft controls exist.
- `render.rs::render_launcher_window_section` renders the enable checkbox with a fixed Shift+Space tooltip. Chord input, alphabet input, inline errors, defaults and sample hints are absent. `catalog.rs` has only `LAUNCHER_JUMP_MODE` for this feature.
- `save.rs::save_settings` validates, constructs a candidate and submits `SettingsSaveIntent::general`. `src/settings_save.rs::GENERAL_SETTINGS_FIELDS` owns only the enable flag, so merely adding editable fields to mapping would not persist them.
- `run_settings_save` reads current settings, verifies the invocation baseline, overlays intent, prepares/persists/applies through the existing acknowledged route. `drain_settings_save` calls `SettingsEditor::apply_saved_settings` only on success. Failure retains the open editor and reports the error.
- `apply_saved_settings` already calls `app.set_jump_mode_configuration` and then updates enabled state. `JumpModeUi::set_configuration` returns false for equal values; changed values advance the navigation epoch, retire the active prefix/collector/focus handoff and retain captured keys in the release fence. Reuse this implementation.
- `src/gui/jump_mode.rs::HintMap::assign_with_alphabet` is the existing deterministic allocator. Use it for a small fixed sample; do not create another allocation algorithm or a preview Jump session.

## Owned changes

Primary files: `src/settings_editor/{state,mapping,render,catalog,save}.rs`, `src/settings_save.rs`, and directly affected tests in these owners. Permit a narrow shared preference/conflict helper in `src/gui/jump_mode/config.rs` and the existing `src/hotkey/validation.rs` only where required to validate the final candidate. Controller/render focus production changes require a reproduced counterexample; none is established by this planning pass.

`src/settings_editor.rs` and `tests/settings_editor.rs` contain mapping callers if a necessary conversion signature change requires migration. Do not broadly rework their unrelated tests or the settings save protocol.

## Required implementation

1. Add raw chord/alphabet draft text initialized from the typed committed canonical values. Keep the enable toggle. Render labeled text inputs beside the existing Jump checkbox, bounded inline errors, a Reset Jump preferences action, and a stable small sample of allocated hints. Reset changes only the draft to existing defaults. Preview neither saves nor changes the live controller. Make the explanatory chord text follow the validated draft rather than hard-code Shift+Space.

2. Reuse the existing parsers for every validation and preview path. Keep invalid raw text visible; do not silently replace an invalid edit with a default/last-good value or serialize it through deserialization fallback. Produce typed values at one fallible save-construction boundary. Preserve the draft, baseline and live controller when validation fails. Ordinary helper naming and the minimal conversion signature are the writer's choice; no normal save caller may bypass validation.

3. Reject a newly configured enabled Jump chord that conflicts with active invocation bindings, especially the independently functioning launcher toggle, quit and help. Reuse the existing resolved route/reservation validation owner rather than inventing a shortcut registry. `validate_hotkey_routes` already enumerates process bindings and radial/MkMacro reservations; `parse_hotkey` and `hotkeys_can_cofire` supply current route semantics. Add Jump only as validation/reservation evidence, never to `legacy_triggers`, a global registration, or the native invocation hook. Respect the difference between an exact local modifier match and a global route's matching rule; do not substitute textual equality. Provide inline draft feedback for bindings available in the editor and validate the final merged candidate at the transaction preparation boundary. Existing unrelated global-route validation behavior stays intact.

4. Add both typed preference fields to general save ownership so copy, successful overlay, changed-owned-field reporting and conditional rollback all use the existing macro-generated machinery. Extend the existing baseline protection narrowly if needed to reject a stale Jump edit, rather than overwriting a later preference change. Plugin-selection saves continue to preserve these fields. Exercise rollback conflict behavior instead of adding another persistence path.

5. Save through the existing transaction and publish only its acknowledged committed result. Do not call `set_jump_mode_configuration` during editing, preview, validation, submission or a failed save. Keep the existing equal-value no-op and changed-value session/release-fence behavior. Successful application updates the committed baseline. Do not overwrite newer raw edits made while a save was pending merely to canonicalize the text; reopening can show committed canonical text.

6. Give the new controls/default action stable Settings IDs and catalog/search entries. Register their actual responses through `SettingsJumpTargets` and existing reveal/focus delivery. Text entries remain native text entries; selecting a hint focuses them without invoking Save or Reset. Keep footer pending-save controls and normal Cancel behavior.

## Invariants and non-goals

- Default Shift+Space and default alphabet remain unchanged; Jump remains disabled by default.
- Jump stays local to the visible, eligible launcher owner. No global Jump hook or hidden-launcher activation.
- Full hints focus/select; a distinct fresh Enter activates; Escape cancels. Preserve held-key release fences, query typing, modal/Notes ownership, foreign foreground refusal and launcher-hotkey query focus.
- Preserve saved malformed-field fallback, unrelated settings, schema, existing save acknowledgment/rollback and application shutdown ownership.
- No target-family expansion, global focus rewrite, background polling, direct JSON write, new dependency or historical patch import.

## Meaningful tests

Add focused tests prefixed `g06_jump_` for:

1. Draft initialization, valid canonical save, invalid chord/alphabet retaining raw text and committed/live state, Reset/Cancel, stable sample hints and searchable real control identities.
2. Relevant launcher/quit/help conflict and non-conflict cases, disabled-binding handling, and final-candidate rejection before persistence; prove Jump is not added to global trigger registration.
3. All three Jump fields through intent apply/acknowledged overlay/rollback, with an unrelated concurrent write preserved and a later conflicting owned write not silently destroyed.
4. Actual acknowledged save integration: no live change before acknowledgment, no change on failed preparation/apply, successful save/reopen uses committed values, and any newer pending draft remains intact.

Extend the current tests rather than duplicate their behavior:

- `settings_editor::mapping::tests::jump_preferences_survive_unrelated_settings_editor_save`
- `settings_save::tests::jump_mode_general_save_acknowledges_and_rolls_back_owned_setting`
- `settings_editor::save::tests::jump_mode_configuration_tracks_startup_and_committed_settings`
- `settings_editor::render::tests::jump_mode_focuses_real_settings_controls_and_later_egui_keys_activate_them`

Focused existing regression controls:

- `gui::jump_mode::config::tests::jump_configuration_*` (five current tests).
- `settings::model::tests::jump_preferences_default_round_trip_and_recover_independently`.
- `gui::jump_mode::ui::tests::committed_configuration_change_retires_prefix_and_fences_held_leader`.
- `gui::jump_mode::ui::tests::disabling_after_configuration_change_keeps_release_fence_without_blocking_ordinary_input`.
- `gui::jump_mode::ui::tests::settings_does_not_take_foreign_focused_window_and_stale_layers_are_ignored`.
- `gui::jump_mode::ui::tests::entering_settings_surrenders_only_the_background_query_focus`.
- `gui::render::tests::launcher_show_focus_requests_actual_query_for_empty_and_nonempty_queries_in_both_jump_modes`.
- `gui::render::tests::notes_query_activation_hint_completion_needs_release_before_fresh_enter`.

All app fixtures set Paste Sequences `next_hotkey=None` and `cancel_hotkey=None` before construction. Reuse inert plugin/owned temporary settings fixtures. Do not change production defaults or mute errors globally.

## Verification and completion

Use `cargo nextest list` and `cargo nextest run` with `-p multi_launcher --lib --cargo-profile fast-dev --offline --locked` and one union of the new prefix and exact directly relevant controls above. Record a nonzero inventory, exact source, durations, failures and retirement. Preparation1200s; execution180s; aggregate incremental preparation600s; only1–2 localized reruns for actual errors. No full suite/build, domain target, install or broad lint.

Inspect the final diff and normal save call path. Done means real controls, one validated candidate construction, both typed fields owned by the transaction, acknowledgment-only live application, focused passing proof and preserved focus/release invariants. Parent then commits/pushes and obtains the required independent review. M3 native status remains separate from unit acceptance.

## Narrow M3 acceptance after tested source

Keyboard tooling is potentially available; no native blocker is established. First bind an executable to the accepted source/build provenance and a disposable profile. Current `src/main.rs` uses `AppDataRoot::from_settings_path("settings.json")` and relative `actions.json`; `src/platform/app_data.rs` resolves the profile from the working directory. Keep all profile paths owned, disable Paste next/cancel shortcuts before startup, and use fixture-only actions. Do not run the installed application against the user's normal profile as proof of this source.

The existing optional `src/gui/qa_framebuffer.rs` supports `ML_QA_FRAMEBUFFER_REQUEST_MANIFEST`, validates a cwd-bound fixture manifest and contains Settings/Notes surface support. Reuse it only if the exact-source executable already supports it; do not import an old qualification campaign or require a broad build for editor implementation.

After source acceptance, use the supported native keyboard/screenshot tool for a small smoke: actual editor/errors/preview; valid Save and new chord; full hint focus then fresh Enter; Escape; launcher hotkey focusing query with Jump enabled; already-open Settings/Notes and a foreign foreground control. Capture actual editor and hints with source/profile/foreground provenance. A missing exact-source executable or unsupported specific interaction becomes a precise M3 NATIVE_PENDING item, not a unit PASS or blanket G06 deferral. No manual user testing mid-goal.

## Uncertainties and findings

No architecture blocker or source-supported focus defect was established. The writer must select the smallest final-candidate conflict helper consistent with current route matching and preserve the already implemented controller. Native executable/tool provenance is checked at M3, not assumed from file existence.

Distinct follow-on findings: NO_ACTION. The absent editor fields and transaction ownership are active G06 work.
