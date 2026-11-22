# G16 — Table Utility, Date/Time Studio, Convert Panel, and Password Generator

**Status:** Pending. **Owner:** G16 implementer with table/date/conversion/secret-operation owners. No source changes or tests have occurred. Use the program's single migration branch.

## Objective and dependencies

Recreate these four complete utilities in GPUI Kit, preserving their different data and interaction contracts. Keep the existing table engine, date arithmetic/time-zone rules, conversion catalog, and password/passphrase generator authoritative. Styling must not alter a value, date interpretation, export source, or secret-copy policy.

Depend on G00–G04 and G05 routing. Use G06's settings shell, G13's shared clipboard integration, and G23 for the real calendar handoff. G16 owns specialized controls and settings; Hash Generator remains G04. Follow [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md).

## Verified source entry points

Pinned baseline: `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

| Family | Existing source |
|---|---|
| Tables | [src/gui/table_utility_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/table_utility_dialog.rs), `src/table_utility_session.rs`, `src/table_utility.rs`, `crates/multi_launcher_table_core/src/` |
| Dates/times | `src/gui/date_time_studio_dialog.rs`, `src/date_arithmetic.rs`, `src/date_arithmetic/{studio,zones}.rs` |
| Conversion | `src/gui/convert_panel.rs`, `src/gui/convert_panel/preferences.rs`, `src/unit_conversion.rs`, related base/unit/convert plugins |
| Passwords/passphrases | [src/gui/password_generator_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/password_generator_dialog.rs), `src/password_generator.rs`, `src/platform/protected_clipboard.rs` |

The table core already has a separate crate, but `src/table_utility_session.rs` still imports egui: do not assume the entire table session is frontend-neutral. Source inspection confirms typed lifetime/source/view/selection/export identities, Date/Time's four modes, converter display preferences, and ephemeral generator results bound to policy revision/session.

## Per-GUI scope

Table Utility includes source/file/clipboard entry routes, delimiter/header/row-width settings as exposed, virtual table, filtering/sorting/selection, statuses, copy/export formats and JSON shape, current-versus-previous-valid export basis, output/picker/overwrite decisions, and limits/errors.

Date/Time Studio includes Between, Add/Subtract, Time Zones, and Schedule Preview; all date/calendar pickers, time fields, refresh/reference behavior, zone search, ambiguity decisions, occurrence selection, copy, and prepare-calendar action. Convert Panel includes physical categories and numeric bases, unit filtering, source/targets, swap/add/remove, precision/format preferences and explicit save/copy. Password Generator includes password and passphrase modes, all current policy fields, generation, hidden/revealed result, entropy/feedback, and private copy behavior.

## Bounded checkpoint packets

### A — Complete Table Utility and its session adapters

**Owner:** G16 with the table-session owner. Extract egui-specific wake/host mechanics while retaining typed session/revision/export operations. Recreate source controls, table/view interactions, copy/export, previous-valid decisions, file pickers, and overwrite confirmations. Keep the existing independent table core authoritative.

**Invariants:** Quoted delimiters, embedded newlines, exact cell strings, and parse limits retain their semantics. View filtering/sorting does not silently alter export basis or selection. An overwrite decision refers to the reviewed target/receipt; late exports cannot mutate another session.

**Evidence and acceptance:** CSV/TSV fixtures cover quoted/Unicode/large/invalid data, current versus retained valid output, selected exports, and stale confirmation. **Commit boundary:** Complete Table Utility slices with focused core/session and native GUI evidence.

### B — Complete Date/Time Studio

**Owner:** G16 frontend implementer using existing date/zone operations. Recreate all four modes, pickers/searches, result fields, ambiguity decisions, schedule rows, copy, and calendar payload creation. Preserve explicit refresh/reference semantics instead of reinterpreting “now” every render.

**Invariants:** Invalid input remains visible without stale copy output. Ambiguous times require the existing explicit fold choice, which clears when its source changes. Schedule gaps remain represented according to the domain; selection identifies the copied occurrence. Calendar preparation carries the selected resolved instant.

**Evidence and acceptance:** Deterministic clock/zone fixtures cover offsets, gaps/folds, bounded schedule counts, invalid input, and selection. Windows evidence covers picker/focus/host behavior. **Commit boundary:** Complete Date/Time GUI; actual calendar opening remains a linked G23/G25 integration row until exercised.

### C — Complete conversion and generator interfaces

**Owner:** G16 frontend implementer; track converter and generator as separate C1/C2 commit/evidence units. Recreate all converter categories/units/targets and preference controls; recreate both generator modes and all policy/reveal/copy states. Use existing domain functions and protected-copy service.

**Invariants:** Conversion formatting changes display without changing numeric value; approximation metadata and target caps remain. Saving preferences does not persist input/results. Generator policy changes retire old result/copy authority; stale tickets cannot copy. Preserve existing ephemeral-secret/private-copy behavior and distinct accepted-but-not-persisted feedback without introducing new product policies.

**Evidence and acceptance:** Conversion fixtures compare inline and panel results, base behavior, multiple targets, formatting, and preference round trips. Generator tests use controlled fixtures and verify actions/status without recording generated secrets in evidence. **Commit boundaries:** Converter and generator independently, each complete with its specialized settings and host behaviors.

### D — Validate shared interaction and frontend continuity

**Owner:** G16 acceptance owner. Check keyboard/Jump Mode operation, nested pickers/menus, tiny and scaled windows, scrolling, dock/popout, root-hidden completion, close/reopen, and restart into egui using the same saved preferences/data.

**Invariants:** Each effect belongs to a live owner/session; a repaint never copies, generates, saves, or exports again. Source/selection survive host changes according to the baseline. **Evidence and acceptance:** Row-level native records, output/data comparisons, and exactly-once traces. **Commit boundary:** Final family-specific fixes and evidence; revalidate affected shared controls used by earlier goals.

## Focused validation

The table-core case `tests::table_parse_csv_quotes_escaped_quotes_and_embedded_newlines_preserve_strings` is an actual verified test under `-p multi_launcher_table_core --lib`. Other verified inline cases include `date_time_studio_fold_choice_is_explicit_and_clears_when_source_changes`, `date_time_studio_schedule_selection_survives_and_copy_matches_selected_row`, `explicit_save_preferences_round_trip_without_persisting_input_or_results`, and `copy_effect_contains_the_displayed_target_unit_and_discarded_save_has_no_file_effect`. Discover/list exact affected table-session and generator/private-copy cases before execution. Conversion/date plugin cases use the aggregate `plugin_queries` target. Require nonzero selections, retain egui regressions, and add GPUI interaction evidence; do not run the full repository suite for every tool.

## Exit and continuation

All four utility families, their submodes, specialized settings, children, and assigned host modes have complete owned evidence. Table output and date/conversion semantics remain compatible; generator privacy behavior is preserved. Any dependent calendar route stays pending globally until tested. Continue G17 onward; G16 acceptance is a scoped milestone within the uninterrupted whole-application parity program.
