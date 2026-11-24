# G05 — Main launcher, query interaction, results, and help

**Status:** Shared scoring and composition, Hash result activation, single-line history and list arrows, [pointer and traversal Tab completion](../gui/launcher.md#tab-completion), [saved Query Set activation](../gui/launcher.md#saved-query-set-activation), [cached command fallback](../gui/launcher.md#cached-command-fallback), [grid and page navigation](../gui/launcher.md#grid-and-page-navigation-baseline), [keypad navigation](../gui/launcher.md#physical-windows-keypad-navigation) and [Search Jump Mode](../gui/launcher.md#search-jump-mode) have scoped Windows and Rust evidence. Actual keypad hardware, explicit Num Lock states, wider Jump owners and acknowledged global invocation remain pending. **Owner:** G05 frontend implementer, coordinating shared command/input changes with G02–G04. Work stays on the single migration branch established by G00.

The first [Settings menu and bare typed entry](../gui/settings-main.md#first-native-settings-save) also passed the scoped checkpoint. File Close and both Toast Log actions have native and focused Rust evidence, with G23 ownership for the remaining viewer geometry. [File and Toast Log evidence](../VALIDATION_LOG.md#g05g23-gpui-file-close-and-toast-log)

The first [Command Help checkpoint](../gui/command-help.md#first-native-command-help-checkpoint) passed native and focused Rust verification for Command List, Linking Guide and typed Help, shared filter and acknowledged Show examples for currently installed GPUI providers. [Quick Help](../gui/quick-help.md#first-native-quick-help-checkpoint) passed the menu, filters, Guide, compact scroll and scoped Escape replay. The first [result context and Favorites route](../gui/launcher-result-context-menu.md#first-native-result-context-menu-checkpoint) passed List Save, exact egui reopen and offscreen Grid with Cancel. The held shortcut, broader catalog and other action groups and geometry remain pending.

The first GPUI Universal Action Sheet passed focused and native evidence for its exact Search chord, filter, tabbed Close, nested Escape, minimize and restore, and both typed Execute and Add to Favorites destinations. Exact Favorites browse also passed under `fav`, `fav add` and sheet Execute. Global hotkey delivery, wider action groups and modal UI Automation remain open. [Batch evidence](../VALIDATION_LOG.md#g05-c-and-g06-action-sheet-and-favorites-browse-batch)

## Objective and dependencies

Recreate the complete launcher interaction in GPUI Kit: opening from the existing global trigger, editing a query, navigating the same results, invoking the same actions, and returning focus according to the existing policy. Preserve the application's compact working layout and command language while applying the agreed visual specification. A search box displaying plausible results is not this goal's acceptance result.

Depend on G00's inventory and pinned baseline, G01's Windows runtime proof, G02's shared operations, G03's selector/lifecycle, and G04's Kit controls and window adapters. Reuse their contracts; extend them only for observed launcher requirements. Follow [requirements](../REQUIREMENTS.md), [validation strategy](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md).

## Verified source entry points

The reference is commit `f7bc610a86bcad1c14b69dce65b1d804df1b7934`; these are existing sources, not proposed GPUI module names.

| Area | Existing source |
|---|---|
| Root state and rendering | [src/gui/mod.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/mod.rs), `src/gui/render.rs`, `src/gui/state.rs` |
| Search and input | `src/gui/search.rs`, `query_history.rs`, `query_observation.rs`, [query_navigation.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/query_navigation.rs), `native_keyboard.rs` |
| Action routing | `src/gui/action_sheet.rs`, `universal_action_catalog.rs`, `universal_action_executor.rs`, `src/commands/{host,outcome}.rs` |
| Visibility and help | `src/visibility.rs`, `window_activation.rs`, `launcher_parking.rs`, `src/help_window.rs`, `src/gui/jump_mode/` |

The large `src/gui/render.rs` could not be read through the drafting connector. Inspect it locally before closing the inventory; it is a substantial source, not an empty file.

## Per-GUI scope

Own the root launcher, query field, result list and grid modes, selection and details, supported action menus/sheets, query/history/completion surfaces, Jump Mode interaction, HelpOverlay, and HelpWindow. Inventory all root toggles, status indicators, context menus, hints, conditional plugin results, empty/loading/error states, and alternate activation routes. Preserve labels, ordering, result metadata, and any keyboard-only routes. Authoring dialogs belong to G06; utility bodies belong to their named goals, but their launcher routes remain in this goal's routing matrix.

Verified command outcomes include query changes, result invalidation, history, toasts, visibility, restoration, focus, caret movement, and first-result activation. Each needs an explicit GPUI interpretation. Trait defaults that silently do nothing are not implementations.

## Bounded checkpoint packets

### A — Freeze input, query, and command contracts

**Owner:** G05 with the shared-engine owner. Enumerate input-to-intent transitions, action identities, result identities, query revisions, and all command-outcome fields. Extract remaining behavior from egui handlers only where this launcher path needs it. Document event consumption separately from text mutation.

**Invariants:** Unmodified physical NumPad2/4/6/8 retains its navigation behavior regardless of Num Lock; only the immediately paired generated digit is consumed. List and grid navigation differ as in the baseline; modified keys remain editor-owned. Search completions cannot replace results for a newer query.

**Evidence and acceptance:** A contract table plus focused old-core/egui checks establishes expected transitions and side effects. Record no-op host coverage gaps explicitly. **Commit boundary:** Shared contract/extraction and its regression evidence, without an unrelated search rewrite.

**First source slice:** Extract the searchable entry and candidate scoring used by the local catalog in `search_actions` and by the cached fallback in `search_plugins_from_results`. Both actual egui loops must call the same borrowed scorer. Keep iteration, filter order, clone accounting and result installation with those callers. Preserve folder-before-bookmark alias lookup, exact matching when the fuzzy weight is nonpositive, the alias bonus, and the maximum label/description fuzzy score. Provider results retain their own admission rules. Usage weighting and stable ordering continue to operate on full actions, including rows that share an ID and differ in arguments.

Run the existing `gui::search::tests` cases `track_c2a_oracle_covers_local_filter_identity_usage_and_mode_edges`, `track_c2a_oracle_covers_provider_precedence_and_snapshot_outcomes`, `unified_launcher_and_read_only_snapshot_keep_order`, and `app_prefixed_qmarker_fixture_returns_alpha_before_beta` in the application library. Then extract read-only result composition behind `search_read_only_outcome` and its captured-provider and fallback paths. Keep widget selection and repaint in the frontend and provider admission and revision capture at the adapter boundary. GPUI row activation must use the result generation and row position before dispatching the complete action payload.

### B — Build the complete query/result working surface

**Owner:** G05 frontend implementer. Implement Kit query editing, composition/caret/selection, results, stable row selection, scrolling, grid/list layouts, metadata, and action dispatch through the authoritative engine. Add notifications/wakeups for actual changes; retain view state across GPUI renders.

**Invariants:** One accepted activation causes one operation; row identity survives filtering and virtualization. Text editing shortcuts are not stolen by result navigation. Rapid input, cancellation, and out-of-order results preserve the latest query.

**Evidence and acceptance:** Compare matched datasets and action traces with the baseline, including no results, errors, long Unicode text, and a large result set. **Commit boundary:** Functional launcher query/result path with targeted checks; secondary sheets may remain explicitly pending.

### C — Complete menus, help, and native focus behavior

**Owner:** G05 with the window/input adapter owner. Recreate action sheets, context menus, completion/help surfaces, Jump Mode targets, and launcher-to-tool handoffs. Preserve Escape ordering, focus return, native Alt+Space, pin/hide behavior, and external foreground restoration. Record ownership of every child surface and its dismissal path.

**Invariants:** A child owns its keyboard interaction; Escape does not also dismiss an unrelated ancestor. A tool request carries its correct source/action payload. No GPUI route opens an egui surface.

**Evidence and acceptance:** Windows keyboard/pointer recordings show trigger → query → action → expected focus, plus menu cancellation and help close. Route tests may verify an interface before a later utility exists, but end-to-end utility rows remain pending. **Commit boundary:** Secondary launcher surfaces and native interaction adaptations.

### D — Close launcher parity and integration evidence

**Owner:** G05 acceptance owner. Run the assigned inventory rows in both frontends against the fixed baseline, exercise repeated hide/show and close/reopen, and inspect visible focus, scaling, clipping, and scrolling. Revalidate earlier input/component evidence if this goal changes shared adapters.

**Invariants:** Hidden-root service ownership and foreground policy remain unchanged; inactive egui does not initialize during GPUI sessions. **Evidence and acceptance:** Attach row-level Windows results, selected test output, and query/result comparisons. **Commit boundary:** Remaining launcher fixes and updated evidence; do not bundle unrelated feature implementations.

## Focused validation

Existing integration sources include `tests/focus_visibility.rs`, `gui_visibility.rs`, `trigger_visibility.rs`, and `tests/pure_cases/query_autocomplete.rs`; the latter belongs to the aggregate `pure` binary. Discover the relevant inline query-navigation cases and list nonzero selections before execution. Select the affected visibility target and `cargo nextest run -p multi_launcher --test pure -E 'test(query_autocomplete)'` as justified by the actual change. Add GPUI interaction checks for physical NumPad versus text input and exactly-once command effects; don't treat preserved egui tests as proof of GPUI input handling. No full nextest run is requested by this goal.

## Exit and continuation

Every assigned launcher surface must be implemented, with evidence for its behavior and native lifecycle. All tool routes must have owners; routes awaiting another goal's real GUI remain visibly unaccepted until that handoff is exercised. Record remaining cross-goal evidence in the program ledger, then continue G06 and the utility goals. Completion here never means the overall GPUI program is complete.
