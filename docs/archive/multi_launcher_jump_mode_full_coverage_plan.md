# Multi Launcher — Jump Mode Expanded Coverage (All Eight Priority Areas)

**Status:** Approved implementation objective; autonomous Codex execution.  
**Scope:** Extend the *existing* Jump Mode to **all eight requested areas** in the table below, using the established Rust/egui focus and modal-ownership infrastructure.  
**Planning source checked:** GitHub `multiplex55/Multi_Launcher`, `master` at `f0ed8f5657055e29814e9edcedcdfc561195cde2` (2026-10-20).  
**Execution source of truth:** Codex's **current repository checkout** and its current `AGENTS.md` supersede the planning snapshot; inspect before editing.  
**Companion prompt:** `multi_launcher_jump_mode_full_coverage_kickoff.md`.  
**No historical attachment required by Codex.**

> **Product contract:** Make as many meaningful, currently visible and keyboard-operable controls as practical navigable with the **existing** Shift+Space Jump Mode. Implement **every listed priority area**, in one long controlled execution. Focus/select with a hint; only a distinct later Enter/Space/arrow/text event acts. Do not hijack recording, drag-only interactions, active screen capture, file mutation confirmations, background windows, sensitive content or unsupported native popups. Preserve the original implementation; extend it.

## 0. Deliverables and explicit scope

| Priority | Work area | Required minimum coverage | Plan milestone |
|---|---|---|---|
| **1** | File Search (`fs`) | Search/refinement/root fields; Filename/Content options; Search/Cancel, visible result selection, Open/Copy and other enabled read-only actions | M2 |
| **2** | Snippets (`cs`) | Visible saved entry selection; filter/alias/tags/body; Save/Cancel, Test/Preview, controlled confirmations and hidden-content protection | M3 |
| **3** | Clipboard History | Visible history entries; select/edit/copy/remove; edit Save/Cancel and confirmation; no plaintext hint leaks | M4 |
| **4** | Remaining Dashboard controls | Pinned Commands, Command History and currently missing *simple visible* Todo/Scratchpad/Calendar controls | M5 |
| **5** | JSON, Regex, XML | Text/search inputs; view/mode selection; visible tree/result/case rows; Previous/Next; normal Copy/Save/Close actions | M6 |
| **6** | Mouse Settings / Gestures | Toggle/choice/value fields, gesture rows, simple binding editing and safe Save/Apply/Cancel; active capture remains exclusive | M7 |
| **7** | Batch Rename / Search & Replace | Safe parameters, rules and filters, visible file/match row selection, previews, Apply/Undo/Restore entry points with existing explicit confirmations | M8 |
| **8** | Radial Designer / Dashboard Editor | Existing navigable lists, cell/slot selections, property forms, relevant tabs/folds, Save/Discard/Cancel, safe nested editors | M9 |

Foundational modal routing, shared safeguards and fixtures are M1; bounded integrated evidence is M10; closeout is M11. Every area must receive implemented support, a meaningful directly focused regression and a documented target inventory. **Do not silently skip an area because it has a complicated subcontrol**; register its viable controls and explicitly record which *unsupported individual interactions* cannot be represented as focusable egui responses without redesign.

### Non-goals

- No new Jump Mode engine, second global hook, different leader key, programmable hint mappings, persisted per-dialog hint codes, or automatic activation on hint completion.
- No generalized operating-system mouse click injection, drag/drop automation, coordinate targeting or live UI scraping. A canvas is not automatically a keyboard control.
- No support for unrelated utility plugins outside the eight groups (e.g. Hash/QR/Clipboard Modify as separate workstreams). No automatic conversion of all windows into Jump destinations.
- No changes to how File Search searches, snippets render, clipboard data is stored, macros record, Batch Rename applies, Search & Replace restores, or radial menus execute.
- No activation of a target in a background/covered dialog, of a disabled widget, of an unrecognized popup, or of a stale result row.
- No broad egui upgrade, dependency migration, performance campaign, unrelated compilation refactor or app-workspace feature.

## 1. Verified architecture and owner map

At the source revision above:

| Existing component | Source, data ownership and constraint |
|---|---|
| Hint engine and typed IDs | `src/gui/jump_mode.rs`: `SurfaceId`, `SemanticTargetId`, `TargetId`, `TargetGeneration`, `TargetMetadata`, `TargetKind`; uppercase A–Z prefix-free session assignment. **Reuse unchanged**. |
| Registry and navigation | `src/gui/jump_mode/ui.rs`: `JumpModeUi`, `JumpModeScope`, `NavigationOwner`, `NavigationWindows`, top/foreground egui layer resolution, focus handoff, popup owners, input capture, frame reconciliation. `NavigationOwner` currently supports Root, Settings, Notes, Todo, Timer, PasteSequence, MkMacro and Blocked. **This is the shared expansion boundary.** |
| Target registration | `JumpModeScope::register`, `register_with_paint_label_rect`, `register_metadata`, `child_namespace`, `child_popup_scope`/recognized combo popup scope; preserve actual `egui::Response`, clip and disabled checks, exact SurfaceId/generation. Do not retain Response/callback beyond current pass. |
| Root and dialog routing | `src/gui/mod.rs` (LauncherApp, dialog ownership/state, currently supported panel lists), `src/gui/render.rs` and related frame dispatch; `src/gui/jump_mode/ui.rs` owner and layer policy. Preserve same-frame transition blocking, multipass event consumption and query-focus surrender. |
| Existing dialog examples | `src/gui/notes_dialog.rs`, `todo_dialog.rs`, `timer_dialog.rs`, `paste_sequence_dialog.rs`, `mkmacro_dialog/`. Pass an optional `JumpModeScope`, register real Responses with stable semantic IDs and actual widget kinds; nested confirmations receive a child scope and suppress parent. |
| Dashboard | `src/dashboard/dashboard.rs` already provides `DashboardContext.jump_mode_scope`, `jump_mode_activation_allowed`, `child_namespace("slot.../header|body")` and target generation. `src/dashboard/widgets/{pinned_commands,command_history,todo,scratchpad,calendar}.rs` are especially relevant. |
| File Search | `src/gui/file_search_dialog.rs` and `src/gui/file_search_dialog/{keyboard,filters,results,diagnostics}.rs`; existing `search_field_id`, `root_field_id`, `root_text_field_id`, `refinement_field_id`, selected-row model and open/copy actions. |
| Snippets | `src/gui/snippet_dialog.rs`, snippet prompt dialog and `src/plugins/snippets.rs`; existing name/tags/filter/masked-content editor and delete confirmations. |
| Clipboard History | `src/gui/clipboard_dialog.rs` plus `src/plugins/clipboard.rs`; history row browse/edit/save/remove/copy. No clipboard payload in hint labels, IDs, diagnostics or QA artifacts. |
| JSON / Regex / XML | `src/gui/json_utility_dialog.rs`; `src/gui/regex_tester_dialog.rs` + its submodules; `src/gui/xml_toolkit_dialog.rs` + tree/XPath/schema submodules. Existing bounded scroll/list/tree views, edits, and internal transitions. |
| Mouse | `src/gui/mouse_settings_dialog.rs`, `mouse_gestures_dialog.rs`, `mouse_gesture_settings_dialog.rs`; gesture/binding pickers and pointer capture/recording paths. |
| File operations | `src/gui/batch_rename_dialog.rs`, `search_replace_dialog.rs`; separate explicit apply/undo/recovery confirmations and stable file/match identities. |
| Designers | `src/gui/dashboard_editor_dialog.rs`; `src/gui/radial_editor/{mod,work_area,canvas,action_editor,appearance,asset_picker,skin_editor,preview,import_export}.rs`. Large dynamically arranged control surfaces; don't assume their nested popup/window owners match normal dialogs. |
| Reusable QA | `dev/jump_mode/run.ps1`, `dev/jump_mode/fixture_generator/src/main.rs`, `src/gui/qa_framebuffer.rs`, `dev/jump_mode/README.md`. They already launch isolated owned app processes and emit receipt-bound frame observations; **extend minimally**, never create an independent GUI automation framework. |
| Agent configuration | `AGENTS.md` and `.codex/agents/{planner,implementer,reviewer}.toml`. Configured planner/reviewer are GPT-6.1 Sol High read-only; implementer GPT-6 Luna Max workspace-write; one writer at a time. |

**Critical boundary:** A supported egui `Window` is not navigable merely because its controls call `register()`. The owner resolver first recognizes the foreground **topmost visible Middle-layer window** as the correct typed dialog/instance, and unknown/foreign/modals map to `Blocked`. Add accurate window identity routing before adding child target registrations; do not make `Blocked` fall back to Root.

**Existing verification baseline:** The prior Jump Mode work completed M00–M15 and verified 109 focused tests, a final 9-test UI case and native root/Settings/Notes/disabled workflows. Pre-existing full-repository formatting and two Pinboard `never_loop` Clippy findings were documented; they are **not** new Jump regressions. Existing native examples used 96 DPI; preserve signed-geometry tests for other displays rather than claiming a real multi-monitor smoke.

## 2. Universal interaction contract for all milestones

### 2.1 Keys, selection and activation

1. **Shift+Space** enters/cancels Jump Mode only when `experimental_jump_mode_enabled` is committed on. Escape cancels; Backspace removes the current hint prefix; letters A–Z are case-insensitive and rendered uppercase. Preserve deterministic session-frozen hints and prefix-free assignment.
2. Completing a hint **focuses or selects** the real widget. It does **not click, toggle, mutate a file, issue a search, copy a secret, run a sequence, play a macro, submit a form or delete an item.** A separate new keypress uses the widget's ordinary semantics: Enter/Space for eligible buttons/selectables, typing for inputs, arrows for sliders/combos, etc.
3. On hint resolution, suppress same-frame Enter/Space and held-repeat leakage. After focus, ordinary input must go to the intended active control, not the root query. A focus handoff to a selectable row must use the existing widget's native focus owner (which may differ from its selected row's semantic ID).
4. Hint registry is composed of **actual current-pass visible, enabled, nonclipped egui responses** only. No fake invisible hit zones to increase coverage. Long labels must use bounded, readable badges that do not cover adjacent text or columns.
5. Stable control identity must be derived from **domain identity + surface/window instance + content generation**. Never use a display label or bare row index if it can change after search, sort, reload, reordering or duplicate titles. Revalidate at completion and before a later action. Only current viewport rows get hints; don't eagerly enumerate offscreen virtual rows.
6. Opening/closing/raising a window, switching tabs/panes, changing a list generation, or opening a nested confirmation/popup invalidates stale hints and freezes the transition frame across discarded UI passes.
7. Unsupported windows, transient native selectors, modal popups not recognized by current ComboBox policy, input-capture/recording and unknown foreground layers **block background navigation**, even if a previously supported dialog remains open behind them.
8. When Jump Mode is disabled, target registration should be inert with negligible additional per-frame work and **no change** to existing Tab/Enter/mouse workflows. Never add a per-widget subscription, global keyboard hook or idle repaint loop.

### 2.2 Data/privacy and intentional writes

- Never expose clipboard contents, hidden snippet contents, private regex inputs, file bodies, matched replacement previews, user-authored macro text, filesystem data or secret prompts in hint text, IDs, logs, screenshots, external events, or debug output. Existing visible aliases/file labels can be displayed only under the current source's existing concealment policy.
- Confirm/Apply/Delete/Undo/Restore are *permitted targets only when the existing UI itself presents them as enabled and in the correctly owned confirmation*. Jump Mode does not bypass confirmation or synthesize extra approval. A focused destructive action is still a separate explicit user action; a second Enter must not accidentally propagate to the newly opened confirmation in the same frame.
- Editor drafts, Save/Cancel, history sorting, file selections, and disk changes use the existing transaction/worker boundaries. Navigating must never write by itself.
- No personal Desktop/Documents/Downloads or live clipboard contents in automated tests. Use isolated fixture files and inert stores. Keep `paste_sequence.next_hotkey = None` and `cancel_hotkey = None` in **all new test profiles** per current `AGENTS.md`.

### 2.3 Verification and no-spiral budget

- One **read-only planner** handoff per milestone grouping; **one writer implementer** per checkpoint; one focused reviewer pass for each architecturally meaningful group (M1, M2–M4, M5–M7, M8–M9, M10–M11), and one bounded correction review only if a concrete serious finding remains. Parent/orchestrator owns branch, commits, ledger and handoffs.
- During M0–M9: generally **one focused Nextest slice per checkpoint**, at most **two adjacent directly affected slices** where owner routing crosses existing Jump tests; run small compiler checks as necessary. No full Nextest or full Clippy during those milestones. No repeated broad test unions every checkpoint.
- Implement a **minimal directly relevant regression** per new domain type of target (fields/rows/confirmations) using existing egui input and renderer tests; avoid hundreds of trivial per-button tests. Existing Jump tests may be updated as the intentional supported-owner inventory expands.
- Reuse `dev/jump_mode` fixture/native tools when desktop evidence materially helps. **At most one bounded native smoke per priority area**, normally 1–2 screenshots, plus up to 2 end-to-end cross-dialog smoke runs in M10; total default cap **12 native sessions/24 evidence images** for the entire initiative. Run one diagnostic plus one retry for a single environment blocker; then `not_run_environment`, use egui/domain proof and continue—no approval loops, new runner rewrite or screenshot pixel-golden campaign.
- For identical unresolved failure signatures: at most **two substantive fixes**, exact focused recheck after each; if still blocked, record the smallest cause and continue independent groups without erasing scope or inventing a green result. Safety regressions prevent completion of affected milestone.
- After all implementation and docs, M11 performs **one final full Nextest** (up to one repaired rerun if required), then final formatting/ledger closure and **only then final maintained-crate Clippy**, ignoring vendored dependencies as lint targets and documenting unchanged baseline diagnostics. No general test/format work after the final successful Clippy pass.
- Never request intermediate user manual validation, screenshots, permission to run tests, or separate confirmation between milestones. The user reviews the whole result once at completion.

### 2.4 Branches, commits and ledger

- Inspect `git status -sb`, HEAD/upstream and actual `AGENTS.md`. Work from the **currently selected checkout**. If the worktree is clean and on `master`, create `feat/jump-mode-expanded-coverage` (or a nonconflicting suffixed name) from current master before writing; if the user already selected a feature branch, retain it. Never reset, switch away from a dirty worktree, silently pull over user changes, alter another worktree or force-push.
- Make **small coherent labeled commits** such as `feat(jump): [M2-A] add file-search query navigation`. Parent attempts one normal best-effort push after each commit; push failure leaves local commit and work continues, per AGENTS.md.
- Keep a compact checkpoint ledger `docs/plans/jump_mode_expanded_execution.md` with M0–M11 status, source/test evidence, commit IDs, target inventory and truly unsupported controls. Update at natural checkpoints only, not in a stream of per-test minutiae. Do not claim unrun native evidence.

---

## M0 — Confirm authority and organize execution

### M0-A — Current checkout, pre-existing targets and tests

**Goal:** Confirm the exact code actually present; no new implementation design marathon.

- Record Git state, actual branch, current Jump Mode setting, `NavigationOwner` and `NavigationWindows`, current panel render/owner routing, the eight dialog families, and relevant tests/fixture commands. A short owner matrix and risk inventory belong in the ledger.
- Determine which requested controls are already supported; do not re-implement them. In particular, basic Todo/Calendar/Scratchpad dashboard actions and existing Settings widgets have partial Jump coverage; this initiative fills gaps only.
- Verify the `dev/jump_mode/run.ps1` fixture profile never touches personal clipboard or files; preserve privacy options, enabled-gate and disabled-gate tests. Do not run a baseline full build/test/Clippy campaign.
- Read all of this plan once, then hand M1 to `planner` for a bounded implementation packet. Codex works autonomously; no user questions.

**Verification:** Read-only source/status and, if useful, `cargo metadata --no-deps` only.  
**Commit:** `docs(jump): [M0-A] begin expanded coverage ledger` when substantive ledger added.  
**Done:** Current owner map and acceptance criteria understood; start M1 immediately.

---

## M1 — Extend safe owned-dialog routing once, not eight times

**Depends:** M0. **Source:** `src/gui/jump_mode/ui.rs`, `src/gui/mod.rs`, relevant root frame renderer/call sites; nearby existing dialog adapters/tests. **No new engine or keyboard hook.**

### M1-A — Add supported owner identities and topmost-layer resolution

- Expand `NavigationOwner`/`NavigationWindows` with a **typed representation** for the additional owned dialogs (File Search, Snippets, Clipboard History, JSON Utility, Regex Tester, XML Toolkit, Mouse Settings, Mouse Gestures, Batch Rename, Search & Replace, Dashboard Editor, Radial Designer), grouping new variants under a small typed dialog kind if this is cleaner than 12 near-identical enum arms. Retain existing owner names/semantics for existing supported dialogs.
- Bind each to its **actual `egui::Window` area-layer ID**, current visible/open state and **per-open instance**/semantic generation; verify window titles/dynamic `Window::new` IDs in current checkout rather than guessing. Recognize only root viewport and actual visible top-middle window; do not accept an open bool as proof of foreground ownership.
- Distinguish model-instance changes from parent window reordering, and ensure any unknown topmost ordinary Window blocks root and other supported dialogs. If a dialog is implemented in an independent native viewport, scope it precisely or fail closed rather than pretending it is a root window.
- On opening, closing, changing supported dialogs, popping confirmation or raising an unrecognized window, invalidate active hints, stale target focus and old keyboard guards; require a fresh leader after stable layer ordering. Preserve current observed combo-popup owner semantics.
- Include a real-dialog-owner transition regression: Root → File Search → Snippets → Settings → unsupported modal; in each state only the correct foreground scope may receive a hint and subsequent new key.

**Verification:** One focused `jump_mode` owner/layer test slice.  
**Commit:** `feat(jump): [M1-A] recognize expanded dialog owners`.  
**Done:** No new dialog can route Jump input into a background/window of the wrong kind or instance.

### M1-B — Reuse registry and input adapters across new panels

- Reuse `JumpModeScope::register`/`register_with_paint_label_rect` and the current `TargetKind` policies; introduce at most a **small presentation helper** for the repeated "scope + actual response + stable identity + visual order" pattern if existing `register_*_jump_target` wrappers would otherwise be duplicated extensively. No parallel registry/focus state, no per-frame retained closures.
- Define a clear local target-ID policy for simple controls (`field/query`, `button/save`, `toggle/include-hidden`), saved entities (stable ID, alias, file/match ID under current generation), tabs (`tab/source`) and scoped confirmations (`confirmation/<instance>/confirm`). Do not retain sensitive values in IDs.
- Preserve current combo-popup provenance and valid native focus. If a new ComboBox displays real options, use existing `combo_popup_scope` and actual `selectable_value` responses; do not make every open `egui::Popup` navigable.
- For custom table/virtualized rows, register the real visible row response only; handoff to the table's actual focus owner and its current stable selected entity. Only introduce a `TargetKind` when current semantics truly cannot express the control; a new kind requires a focused adapter test (activation, disabled, stale identity, multipass).
- Preserve `register()` on every UI pass but commit side effects only on owner-authorized first passes. Never create keyboard activation just to make a control respond to hint completion.

**Verification:** Existing core Jump target/scroll/disabled/keyboard tests plus one new exact-owner example.  
**Commit:** `refactor(jump): [M1-B] standardize supported dialog registration`.  
**Done:** One current-pass registration/focus protocol is sufficient for every following milestone.

### M1-C — Establish isolated new-dialog QA fixtures with no harness expansion

- Extend only the fixture metadata necessary for future focused tests (temporary roots, typed empty stores, fake rows and safe selected controls). Prefer pure egui `Context::run`/existing `run_ui` tests over native entry points.
- Preserve existing runner, QA capture format, app ownership and its safe profile rules. Optionally add a typed list of **new supported surface identifiers** to the existing fixture runner; do not add a new automation language, general observer or screenshot backend.
- Ensure plugin inactive/default-off means zero visible hints, open unknown modal means zero stale targets, and all test profiles keep Paste Sequence Next/Cancel unset without disabling error toasts.

**Verification:** `-FixturesOnly` plus one focused registry test if fixture code changed; no native GUI required.  
**Commit:** `test(jump): [M1-C] extend safe owner fixture inventory` when changed.  
**Done:** Feature milestones can write narrowly focused tests without rebuilding infrastructure.

---

## M2 — Priority 1: File Search (`fs`)

**Depends:** M1. **Source:** `src/gui/file_search_dialog.rs`, `file_search_dialog/{keyboard,results,filters,diagnostics}.rs`; app's FileSearch dialog rendering/owner. Preserve filename/content search engine and `fs` commands unchanged.

### M2-A — Search form, modes and action controls

- Pass File Search's correctly owned optional `JumpModeScope` into the actual window/inner UI, preserving normal mode/default autofocus. Register currently visible Search input (`search_field_id()`), optional folder/root path field (`root_field_id()`/`root_text_field_id(index)`), refinement (`refinement_field_id()`), Filename/Content and matching-mode selection controls, Search/Cancel/Open/Copy/Clear and other *visible enabled* read-only actions.
- Give each real field a stable semantic ID and correct TextEntry/Selectable/Button/ComboBox kind; if switching modes changes the search form, finish old hint session and generate a fresh registry. Preserve existing implicit Enter-in-search behavior and Escape search-cancel/close precedence when Jump Mode is inactive.
- Exclude inactive advanced controls, disabled Open/Copy and native file pickers; ensure the root query cannot activate behind the File Search window.

**Verification:** One real-egui field/mode/button registration test, including fresh hint followed by separate Enter.  
**Commit:** `feat(jump): [M2-A] navigate file-search fields and actions`.  
**Done:** Key search inputs/actions can be reached and edited without mouse while normal search semantics remain.

### M2-B — Visible result navigation and safe row identity

- Register each currently rendered **selectable** Filename/Content result row against a stable key: canonical result identity (path + current match identity when needed) bound to search generation/refinement/sort/view revision. Do not use result label or bare index for stale result reactivation. For duplicate names, distinct paths/matches must remain distinct.
- Use the real row `Response`, clipping and current viewport; ensure offscreen rows are absent. Hint completion selects/highlights row and focuses the actual result-list owner without opening files. A *later* Enter/Open button triggers the existing `open_selected_result` policy; Copy buttons likewise retain existing semantics.
- If results refresh/reorder while the user is typing a hint, reject stale target rather than selecting a different file. Cover a filtered list with path containing Unicode and 2 equal basename rows. Preserve horizontal/vertical result scroll and existing mouse/keyboard row selection.

**Verification:** One focused result-table/search-generation case.  
**Commit:** `feat(jump): [M2-B] select current file-search results safely`.  
**Done:** Hint cannot target wrong file after query changes or reveal offscreen rows.

### M2-C — File Search integration and optional owned native proof

- Test Search/Cancel, reopen, mode switch and nested missing-ripgrep prompt blocking. Verify default-off Jump Mode still uses ordinary Tab/Enter and no registered hints.
- If a native desktop is available, extend an existing `Phase2`-style owned File Search fixture using only a temporary directory and harmless files: focus query hint, type search, select a fixture result, later open **only if it is a safe owned fixture path**; 1–2 screenshots max. Otherwise rely on existing egui/fixture proofs and record `not_run_environment`.
- Update brief in-app help only where dialog-specific instructions changed.

**Verification:** One focused `fs` + core ownership slice and optional one native owned smoke.  
**Commit:** `test(jump): [M2-C] prove file-search routing and stale-row safety`.  
**Done:** All priority-1 minimum controls work on real File Search UI.

---

## M3 — Priority 2: Snippets (`cs`)

**Depends:** M1. **Source:** `src/gui/snippet_dialog.rs`, `snippet_prompt_dialog.rs`, snippets persistence/model. Preserve concealment/explicit reveal, tag semantics and prompt handling.

### M3-A — Snippet selection, filter and editor fields

- Scope the top-level Snippets dialog to its own open instance. Register visible saved-snippet rows using alias/stable source identity + current list/filter generation rather than index or duplicate label; completion only selects it.
- Register filter field, alias/name, tag input/add/remove options, any existing tag filters, text/body editor **only when the actual source UI permits editing/reveal**, and currently enabled Add/Edit/Save/Cancel/Close buttons.
- Reuse current rich/multiline TextEdit and combo policies; preserve cursor, newline/typing and input focus even after selecting a snippet and switching to edit mode. Tag updates do not execute during hint selection; user presses a fresh Enter/Space.

**Verification:** Real-egui select→edit→type/Cancel, duplicate-label and filter/reorder test.  
**Commit:** `feat(jump): [M3-A] navigate snippet list and authoring fields`.  
**Done:** Normal authoring and selection are keyboard-reachable without editing side effects from hints.

### M3-B — Hidden snippets, confirmations and prompt ownership

- Do not use `hide_contents` payload or plaintext for hint labels, `SemanticTargetId`, QA observations or debug output. The `Reveal to Edit` button can receive a hint; the sensitive body becomes a target only under the same explicit reveal/edit policy already in the dialog.
- Treat delete confirmations, Preview/Test and prompted snippet dialogs as **their own temporary owners**. Focus parent selection may open a child only after the later deliberate activation; root/parent hint sets are dropped in the same frame. If an existing prompt window is not supported safely yet, block all background hints rather than route typing into the wrong parent.
- If `snippet_prompt_dialog.rs` is already accessible as a regular typed egui window, register its simple visible prompted fields and Submit/Cancel in the existing dialog owner model, but do not broaden into a new snippet template engine or shortcut text expansion.

**Verification:** Hidden-body non-disclosure + confirmation handoff + no double-Submit tests.  
**Commit:** `feat(jump): [M3-B] protect snippet privacy and nested prompts`.  
**Done:** Hidden source and modal behavior remain correct; no accidental Delete, Reveal, or prompt submission.

### M3-C — Snippet lifecycle and focused proof

- Exercise list filter, choosing an item, edit changes, Save/Cancel, reopening and deletion confirmation with disposable fake snippets; validate existing store bytes unchanged until real Save.
- One optional native editor focus/Cancel smoke with generated snippets and redacted screenshot; do not disclose contents from a real saved collection.

**Verification:** One focused Snippets+Jump integration slice; optional one owned native smoke.  
**Commit:** `test(jump): [M3-C] verify snippet edit and concealment`.  
**Done:** Priority-2 controls reachable; persisted snippet semantics unchanged.

---

## M4 — Priority 3: Clipboard History

**Depends:** M1. **Source:** `src/gui/clipboard_dialog.rs`, current Clipboard History view/editor and model; no new clipboard observer, history engine or system clipboard ownership changes.

### M4-A — Browse list and privacy-safe entry selection

- Scope Clipboard History's top-level window and its browsing subview. Register actual visible history list rows with stable per-entry identity or opaque snapshot-token+revision (if current store exposes no stable ID), never with clipboard plaintext, content digest displayed to users, or arbitrary row index alone.
- Hint completion selects/focuses one entry; a fresh separate action may use the existing Copy/Edit/Remove buttons. Register filter/navigation controls, basic list actions and Close; exclude disabled/unavailable entries.
- Handle history changes between hint paint and completion (new entry added at top, truncated/removed oldest entry, duplicated text) by invalidating the session rather than selecting a shifted neighbor.

**Verification:** One fake history list test covering duplicates, insertion/reorder, selection and masking.  
**Commit:** `feat(jump): [M4-A] navigate private clipboard history rows`.  
**Done:** Only the intended selected entry receives later actions; hints do not expose payload.

### M4-B — Edit, copy/remove and confirmation controls

- Register actual multiline edit field, Save/Cancel, and enabled Edit/Copy/Remove actions in the respective browsing/editor UI state. Register only controls visible in the active state; do not register both hidden browse and edit copies of a control.
- Child delete confirmation receives exact instance-bound focus; Enter used to select history row must not carry into Delete confirmation. Preserve Escape Cancel/editor-close policy and system clipboard unchanged unless the user subsequently activates Copy.
- Keep clipboard payload redacted in target IDs, logs, native screenshot fixture and status. No secret-bearing fixture data.

**Verification:** Real-egui selected history→Edit/Cancel, remove confirmation, no-copy-on-hint cases.  
**Commit:** `feat(jump): [M4-B] cover clipboard editor and confirmations`.  
**Done:** User can navigate edit/copy/remove without unintended clipboard mutation.

### M4-C — Clipboard-specific lifecycle and optional smoke

- Check Jump disabled, background dialog blocked, rows scrolled out, text edit multiline focus, repeated open/close and parent window focus restoration.
- Optional one owned native Clipboard History smoke with fixed generated non-sensitive entries, no read of current user clipboard and no real Copy/Delete activation; capture at most two safe images.

**Verification:** One directly relevant focused test slice and optional owned smoke.  
**Commit:** `test(jump): [M4-C] prove clipboard focus and privacy`.  
**Done:** Priority-3 coverage accepted without changing clipboard data policies.

---
## M5 — Priority 4: Remaining Dashboard targets

**Depends:** M1. **Source:** `src/dashboard/dashboard.rs` and `src/dashboard/widgets/{pinned_commands,command_history,todo,scratchpad,calendar}.rs`, plus any actual helper modules those widgets use. Reuse `DashboardContext.jump_mode_scope` and the existing slot/header/body namespace. **Do not redesign the Dashboard editor here; that is M9.**

### M5-A — Pinned Commands and Command History

- Add Jump targets for every visible, enabled discrete command row/card in Pinned Commands and Command History, as well as currently visible related Refresh/Clear/Expand buttons. Respect any existing per-row action capability and the Dashboard's `jump_mode_activation_allowed` first-pass guard.
- Bind IDs to slot stable ID, widget type, entity identity/action full argument identity, current generation and visible row identity; do **not** resolve a saved command later from display label or row index. Equal-looking commands with different targets must remain distinguishable.
- Hint completion selects the exact visible entry, and a fresh Enter executes the existing Dashboard action exactly once. If clicking a row already invokes a command through a pointer-only container, reuse its actual clickable Response/native focus path rather than invent a second executor.
- Preserve loading/reordering/deleting widget slots and top-level dashboard focus; no hidden actions shown when the plugin/provider has no capability.

**Verification:** One dashboard fixture testing stable IDs across slot reorder, duplicate titles and fresh Enter once.  
**Commit:** `feat(jump): [M5-A] navigate pinned and historical commands`.  
**Done:** Both previously unsupported widget families have visible row hints without action duplication.

### M5-B — Complete eligible Todo, Scratchpad and Calendar controls

- Compare current widget-specific target lists against Jump Mode M15 inventory. **Add only genuinely missing simple visible controls**, for example compact Todo row actions/filters, Scratchpad edit/append/copy/clear controls, and Calendar visible event rows, Open/Add controls, view switches or date selectors *that are already ordinary egui Responses*.
- Keep Calendar picker/native popup boundaries distinct: an event editor/confirmation may be an unsupported foreign owner until M1 explicitly recognizes it; don't let the parent dashboard receive hints behind it. For a native date popup without reliable owner, block instead of trying coordinate-based clicks.
- Scratchpad content is user-authored private text: never embed its contents in hint codes/labels or QA logs. Focus a TextEdit normally; typing happens afterward, not during hint completion. Separately validate that Copy/AppendTimestamp are executed once only on a fresh activation.
- Todo rows preserve stable todo ID under filtering, priority edits, completion toggles and list refresh. Mixed checkbox/row interactions require their exact actual response (do not assign two hints to a single click target).

**Verification:** One focused test slice for missing controls with list-filter/slot-reload/content privacy cases.  
**Commit:** `feat(jump): [M5-B] cover remaining simple dashboard interactions`.  
**Done:** Those three families expose their keyboard-usable visible controls; existing widget semantics unchanged.

### M5-C — Dashboard composition and safe native representative

- Exercise multiple populated slots at once: PinnedCommands + CommandHistory + Todo + Scratchpad + Calendar with current Root owner, dark theme and smaller viewport. Check clipping, hint collisions, first-pass one-action guard, repeated render, toggling dashboard off, and slot/wrapper reorder.
- Reuse existing phase-1 dashboard fixture and observation; optionally add one native run with a safe generated `FillQuery`-style command. No real file launch, clipboard copy, calendar event creation or user data in automation.
- Record per-widget included/excluded target list in ledger; avoid stale badges when same viewport body changes after an action.

**Verification:** One combined dashboard-only focused slice; one native owned dashboard smoke optional.  
**Commit:** `test(jump): [M5-C] prove expanded dashboard target integrity`.  
**Done:** Priority-4 gaps closed without changes to dashboard execution or cache policies.

---

## M6 — Priority 5: JSON, Regex and XML tool dialogs

**Depends:** M1. **Source:** `src/gui/json_utility_dialog.rs`; `src/gui/regex_tester_dialog.rs` and its `test_cases`, `presets`, `history`, `examples`, `keyboard` children; `src/gui/xml_toolkit_dialog.rs` and `xml_toolkit_{tree,xpath,schema}.rs`. Preserve parsing/evaluation/XSD engine, document workers and editor contents.

### M6-A — JSON Utility: source, views, tree search and copy

- Integrate a typed JSON Utility dialog owner/instance with source TextEdit, Text/Tree view choices, Format/Minify actions, Paste/Clear/Copy Result, tree key/value/both search field, Previous/Next, Expand/Collapse Branch, Copy Value/Subtree/JSON Pointer, Close and other visible enabled simple controls.
- JSON tree rows use their existing JSON Pointer or authoritative parsed-node path **plus document generation**, never the displayed preview label or array index detached from the current tree. Only current expanded, visible nodes get hints; selecting a leaf focuses its actual row and details; a fresh action activates its existing copy/select behavior.
- Editing the source or switching Text/Tree invalidates active old-source hints; malformed JSON has no stale tree/action hints and keeps normal parse diagnostics. Copy never fires from hint alone; clipboard access remains explicit.
- Ensure longer multiline source input receives normal text/caret focus without root query leakage. Avoid per-frame tree reparsing or materializing every node just for hints.

**Verification:** One JSON-specific egui/search/row/navigation/copy test slice, including invalidated tree generation.  
**Commit:** `feat(jump): [M6-A] navigate JSON utility source and tree`.  
**Done:** Both JSON views and meaningful visible tree controls are keyboard accessible without changing JSON computation.

### M6-B — Regex Tester: pattern, cases, results and preset workflow

- Scope Regex Tester window and register pattern input, flags/modes, text/examples, replacement text, selected information panels, source/results Previous/Next, visible result/test-case rows and expected-match controls, and enabled Save/Load/Close/Copy actions.
- `src/gui/regex_tester_dialog/test_cases.rs` case rows must use actual persisted authored case identity (or current document revision+opaque stable row key), not raw input/test text. Reordering/adding/removing cases and switching presets invalidates old maps. Pattern changes invalidate search-result hints until current results render.
- Register multiline test input TextEdit only for visible edit rows; separate Tab/Enter behavior from old editor's keyboard shortcuts. Selected case and regex match navigation should land on the correct current item, but do not simulate a click to force scrolling: use existing scroll-to/selection facilities.
- Information/reference and auxiliary collapsible sections may receive hints for their real headings/buttons; do not register rendered explanation-only text as fake actions. Handle preset delete/overwrite confirmations through proper child popup ownership.

**Verification:** One Regex egui fixture for pattern field, 2 case rows, reorder, Next/Previous, explicit preset Save and Cancel.  
**Commit:** `feat(jump): [M6-B] navigate Regex Tester cases and presets`.  
**Done:** User can use normal Regex test workflow from hints without unwanted preset mutation.

### M6-C — XML Toolkit: tabs, source, tree, XPath and schema

- Scope XML Toolkit window and register source multiline field; Source/Tree/XPath/Validation tabs; Open/Paste/Clear/Copy/Format/Minify/Apply Output/Save As/Close buttons where enabled; XPath expression and namespace fields; schema path selector; visible tree/XPath result rows; Next/Previous and copy selected attributes/text/fragments.
- For parsed XML nodes, bind current accepted document revision, canonical node ID/path and view. `src/gui/xml_toolkit_tree.rs` and `xml_toolkit_xpath.rs` should hand exact selected source nodes to the existing selection/inspector, never re-select from text label. Only rendered tree/result rows are hintable; offscreen nodes are not.
- Switching source document, refreshing schema or changing XPath invalidates stale target set; unavailable/parse-error states must not hint inactive Copy/Apply controls. Preserve worker cancellation and no extra evaluation on hint focus.
- Native file pickers and the **Confirm XML file overwrite** nested window must retain existing decision requirements. Confirm/Cancel hints may exist in the exact active confirmation owner, but completion of Save As hint cannot itself accept an overwrite.

**Verification:** Focused XML dialog/input/selection test over a generated XML string, scalar XPath case and overwrite Cancel; no network/DTD or real user files.  
**Commit:** `feat(jump): [M6-C] navigate XML tools without executing file actions`.  
**Done:** Core XML workflows and safe visible selections are accessible using Jump Mode.

### M6-D — Cross-tool focus, performance and representative evidence

- Test actual Root → JSON → Regex → XML owner transitions; changing a view, opening a ComboBox, closing a tool and returning to root must clear stale hints. Mixed source fields must accept normal Unicode multiline input without losing caret or causing duplicate first-Enter actions.
- Verify result virtualization has no proportional per-frame work increase from offscreen large JSON/Regex/XML lists. Reuse existing sampled-size egui fixtures; no new Criterion suite necessary.
- Optional one **combined tool-group native smoke** demonstrating focus for a JSON source editor and one visible Regex or XML control, using only generated text and at most two screenshots. If renderer runner cannot support grouped surfaces with small adjustment, take one representative native tool and rely on focused tests for the others, recording the limitation honestly.

**Verification:** One direct owner-group integration test slice and optional one owned native session.  
**Commit:** `test(jump): [M6-D] verify JSON Regex XML transitions`.  
**Done:** All three tools meet their supported-control inventories; no false native claims.

---

## M7 — Priority 6: Mouse Settings and Mouse Gestures

**Depends:** M1. **Source:** `src/gui/mouse_settings_dialog.rs`, `mouse_gestures_dialog.rs`, `mouse_gesture_settings_dialog.rs`. Native mouse/gesture overlay ownership must remain unchanged.

### M7-A — Mouse Settings controls

- Scope Mouse Settings window to its own exact instance. Register visible click visualization toggles, crosshair/cursor effects controls, coordinate details, style/space selectors, sliders/numeric inputs, color controls implemented as ordinary responses, and Apply/Cancel. Use current `TargetKind` for checkbox/slider/text/ComboBox and the true response bounds.
- When a color picker opens a native/system picker or overlay, invalidate parent Jump Mode and block unknown popup; do not turn the color screen sampling into a Jump operation. Tab/change of page may invalidate stale hints, but preserve draft/Apply/Cancel semantics.
- A focused checkbox must not toggle until the user presses a subsequent Space; focus-only slider changes no value until arrows; a typed numeric control remains current over legitimate frame revisions.

**Verification:** Focused settings form test covering toggles, sliders, mode switch, Cancel→no persistent write.  
**Commit:** `feat(jump): [M7-A] navigate mouse settings controls`.  
**Done:** Mouse Settings ordinary fields are Jump accessible with zero unintended live effect changes.

### M7-B — Mouse Gesture dialog lists and binding rows

- Add Jump targets for visible gesture entries, Rename/Delete/Move binding controls, Add Gesture/Binding, active list selections, edit/rename text, and Save/Cancel. Use stable gesture and binding identity plus current list/selection generation; avoid mapping by list position after reorder.
- Register only visible binding selectors/category dropdown options in their exact current egui popup/child owner. A `Bind Action` dialog that currently has native ComboBoxes and categorized action lists should allow focusing the actual anchor and selected visible option; never implicitly execute the action upon selection.
- **Active mouse gesture capture/recording, hit path drawing, raw pointer tracking, global gesture hooks and modifier assignment capture remain exclusive.** Jump Mode cancels/blocks upon entering those states; it never steals keys from gesture recognition or synthetic input.

**Verification:** Focused gesture-row identity test with reorder, Rename/Cancel and category popup; active-capture blocked test.  
**Commit:** `feat(jump): [M7-B] navigate gesture and binding management`.  
**Done:** Management actions accessible; physical gesture capture unaffected.

### M7-C — Mouse settings/gesture cross-owner regression and native smoke

- Test Root → Mouse Settings → Mouse Gestures → Bind Action → Cancel, including a dismissed native picker and return to root; no stale background target and no accidental action dispatch on hint completion.
- Exercise an owned fake gesture catalog; verify no new native hooks/recorder frames or pointer overlays started merely by navigation. Existing gesture settings save acknowledgment remains correct.
- Optional one benign native Mouse Settings smoke on an isolated profile: select an enabled checkbox and later Cancel without persisting; no actual gesture drawing/recording or pointer capture.

**Verification:** One narrow combined input-owner slice; optional one owned native session.  
**Commit:** `test(jump): [M7-C] protect gesture input ownership`.  
**Done:** Priority-6 controls work safely and no gesture runtime behavior changed.

---

## M8 — Priority 7: Batch Rename and Search & Replace

**Depends:** M1. **Source:** `src/gui/batch_rename_dialog.rs`, `search_replace_dialog.rs` plus their existing domain result and safe apply/restore boundaries. **Safety-sensitive**: focus, selection and intent submission must never bypass preview/stale-file checks or confirmations.

### M8-A — Batch Rename root, rules and preview controls

- Scope Batch Renamer parent window and register only visible, enabled root chooser, folder scope, Rescan, filter, rule-kind/prefix/suffix/number/literal/regex/case editors, extension-policy selector, rule Add/Move/Remove, Preview and Close controls. Reuse `JumpModeScope` and currently proven ComboBox owner behavior.
- A hint focuses a field/selector; it doesn't click root chooser, change rule order, preview, or mutate disk. Subsequent real keyboard input uses existing rule-draft commands.
- Where multiple list/editor copies of controls appear in separate modes, give them **disjoint semantic namespaces** and only register the visible active mode; invalid forms exclude Apply.

**Verification:** Real-egui field/rule/preview focus and no-draft-mutation-on-hint test.  
**Commit:** `feat(jump): [M8-A] navigate batch rename planning`.  
**Done:** Rule authoring and Preview entry are keyboard-reachable before any write path.

### M8-B — Batch Rename row selection, confirmation and Undo

- Register actual visible result rows using current immutable plan **file identity** (lossless source path/verified ID) + preview revision/row generation, not displayed filename or unsorted index. Hint completion selects/deselects only according to the existing row widget's focus/select semantics; a separate later Space may toggle its checkbox when applicable.
- Expose enabled Apply selected renames/Undo entry points as normal buttons; parent Jump Mode must be replaced by the **actual Confirm batch rename / Confirm Undo** child's scope when opened. Only visible Confirm/Cancel may receive child hints; a fresh second Enter must be required.
- Reload, rule editing, re-sort, stale/occupied files, Undo conflict and cancellation invalidate stale row IDs. No Jump code may call the executor or bypass the native `Apply`/`Undo` preconditions.

**Verification:** Focused generated-root test for row identity, preview/confirm/Cancel, stale file and zero writes on hints.  
**Commit:** `feat(jump): [M8-B] preserve rename selection and confirmation safety`.  
**Done:** User can navigate the full safe rename flow without keyboard-triggered premature Apply.

### M8-C — Search & Replace search form, filters and file results

- Add typed owned Window for Search and Replace. Register root selection, pattern/replacement TextEdits, literal/Rust regex choices, case and advanced filters, Search/Refresh, files-with-matches collapse, visible file rows, refinement controls and enabled footer actions. Preserve the existing compact outer-body scroll and pinned action footer.
- Actual file/match result identity must include authoritative search generation, immutable file identity and match ID; avoid private match-text in target labels/IDs. Filtering, rescanning, changing replacement, or switching the selected file invalidates old rows rather than selecting a different result.
- Virtualized results register only drawn rows with actual clip rectangles. Nested scroll areas should retain wheel routing and keyboard caret/Tab behavior. Fold/unfold actions only change presentation, not approval.

**Verification:** Focused Search & Replace UI test for filters/folding/result identity and zero file writes.  
**Commit:** `feat(jump): [M8-C] navigate replacement fields and file results`.  
**Done:** Search/result controls are hintable without mutating backend search/selection semantics.

### M8-D — Per-match approvals, diff inspection, Apply and Restore

- Register visible per-match approved checkboxes, selected-match rows, Original/Proposed/Diff region controls where actual actionable Responses exist, and independent panel folds/scroll controls. Use stable `MatchId` + snapshot generation, not line preview text/byte offset alone.
- A hint selecting a file/match must **never toggle approval** unless the actual focused checkbox later receives Space. Selecting another file must preserve its preexisting approved subset. No stale proposal may be silently recommitted from a saved target.
- Expose Review Apply/Restore-Recovery entry buttons; protect the separate confirmation child layer with exact parent/file-revision binding, and require a later fresh Enter for Confirm. Popup with confirmation not recognized must block navigation to parent instead.
- The actual write path must continue through existing guarded execute/backup/recovery. For QA exercise Cancel/Keep only, not real live Apply/Restore.

**Verification:** Disposable fake-root UI test: select some matches, navigate away/back, cancel confirmation, compare exact selection and fixture bytes.  
**Commit:** `feat(jump): [M8-D] navigate approvals and recovery safely`.  
**Done:** No accidental write/restore, selection change or secret leakage from keyboard hint completion.

### M8-E — File operations safety regression and optional native evidence

- Run one targeted combined file-operation integration slice across renamed/removed/filtered rows, stale underlying file, multiple Window owners, nested confirmations, Escape precedence and disabled root owner. Include one test asserting a completed hint from file A cannot be activated after new preview selects file B.
- If a native environment is available, run **one owned smoke for Batch Rename and one for Search & Replace** (the two distinct products of this priority area), with temp directories and focus-only/Cancel semantics. Max two screenshots per run; never point to real workspace files or perform mutating Apply in native smoke.
- Record which diff panes are purely passive scroll/render views and correctly have no hints; preserve their normal keyboard scrolling and selective inspection behavior.

**Verification:** One focused integration slice + up to two native owned sessions (counted within overall 10-session cap).  
**Commit:** `test(jump): [M8-E] verify safe file-operations navigation`.  
**Done:** Priority-7 flows have bounded safe focus support and all mutation boundaries remain user-driven.

---
## M9 — Priority 8: Radial Designer and Dashboard Editor

**Depends:** M1; reuse M5 dashboard slot identity where appropriate. **Source:** `src/gui/dashboard_editor_dialog.rs`; `src/gui/radial_editor/{mod,work_area,canvas,action_editor,appearance,asset_picker,skin_editor,preview,import_export}.rs`. This is the most complex group. **Do not rewrite the designers or invent pointer drag simulation.**

### M9-A — Dashboard Editor: slots, cells and existing property fields

- Scope `Dashboard Editor` with a typed visible owner/instance. Register existing Save, Reload from disk, Auto-place nearest, Compact layout, Add slot, Expand/Collapse all, Split H/V and relevant enabled property form controls, using their real egui `Response`s.
- Expose visible slot widgets, selectable cell/slot headings, Widget type and Overflow ComboBoxes, active property/size settings, and Swap with selected. Use existing stable slot IDs (fall back to current layout revision + row/column/widget type when no persistent ID exists), and derive nested control IDs from slot ID and property path—not display order.
- A hint focuses/selects a slot first. A separate Enter/Space activates its existing Select/Edit action; typing in property fields subsequently changes only the draft. Explicit Save remains separate. Reordering, deleting or switching slots invalidates old code mappings and selected semantic IDs.
- Treat `Confirm remove` and Reset to defaults confirmation as child owner surfaces; never allow Enter from the old slot hint to delete immediately. If the native panel contains a purely drag-based resizing affordance, leave that affordance non-hintable rather than synthesizing a pointer drag.

**Verification:** Existing dashboard-editor fixture + focused egui test for two equal-looking slots, reflow, property input, Save/Cancel and no mutation on hint.  
**Commit:** `feat(jump): [M9-A] navigate dashboard editor safely`.  
**Done:** Visible dashboard slot selection, properties and management actions are keyboard reachable without unintended persistence.

### M9-B — Radial Designer: menu/ring/cell selections and properties

- Determine the actual primary Radial Designer/Editor window(s), their dynamic titles, editor instance/session and foreground layer; register only the active designer owner, not other launcher windows behind it. Reuse the existing optional scoped jump bridge.
- Register real visible menu/ring/cell **list or card** responses, nested submenu entries, editable labels/aliases, action-selection controls, appearance/settings panels, Save/Apply/Cancel/Discard, and navigation folds/tabs that already have an actionable egui Response.
- Link selected ring/cell targets to the existing radial document's stable authored identity + editor/draft revision, not a ring position or wedge coordinate alone. Duplicate labels and menu reorder must not target the wrong cell. Selection and later activation should use existing designer ownership—no live radial activation or macro execution simply by navigating the editor.
- Preserve the raw Radial **canvas**, angular drag, cursor-motion ring editing and other pointer-only interactions as pointer-only if not backed by a real discrete response. Do **not** add a synthetic click onto the wheel canvas. If an existing cell overlay already supplies discrete click responses, it can be registered using exact hit geometry and model ID after evidence; otherwise record it as a non-keyboard canvas affordance while making its nearby existing list/properties navigable.

**Verification:** Focused radial editor test: selecting one of duplicate-label cells, editing one property, switching ring/submenu, Cancel/discard, no live command execution.  
**Commit:** `feat(jump): [M9-B] navigate radial menu and cell properties`.  
**Done:** Core Radial Designer selections and forms are accessible while existing radial runtime/canvas semantics remain intact.

### M9-C — Radial nested editors, skin/assets and import/export confirmations

- Extend owner handling for the existing nested `Cell properties` window, `action_editor.rs`, `asset_picker.rs`, `skin_editor.rs`, appearance tabs and any actual explicit import/export confirmation. Register only **existing visible, enabled ordinary controls** such as labels, resource/list choices, skin parameter fields, sliders/toggles, asset selection and Save/Cancel.
- Each nested dialog must take foreground ownership before hints are offered; parent editing targets are blocked until child closes. For a native file picker or unsafe action/property capture state, block rather than fake supported Jump scope. ComboBox options require current anchor/native popup ownership; unknown context menu remains blocked.
- Keep skin imports, exports, resource rewrites and the radial publishing lifecycle unchanged. Merely focusing Save/Import/Apply cannot persist or apply; a separate action still flows through existing explicit validation, preview and confirmation.
- Reuse current labels/geometry clipping controls to avoid hint badges obscuring tiny property icons and densely packed editing rows.

**Verification:** Focused nested-owner test with Cell properties → action picker → Cancel, plus one existing skin/asset form read-only selection regression.  
**Commit:** `feat(jump): [M9-C] safely navigate radial nested editors`.  
**Done:** Radial property subdialogs and their safe visible fields are navigable without ever routing background input through child windows.

### M9-D — Designer integration, layout, focus and limits

- Exercise owner changes `Dashboard Editor → Confirm remove → Dashboard Editor` and `Radial Designer → Cell properties → Action editor → Radial Designer`, including tab changes, large property arrays, viewport clipping, duplicate labels, dirty drafts and changing selected entities.
- Compare state **before hint completion**, after hint completion and after a *new distinct* activation: focusing a control cannot mutate a draft, save a file, publish a menu, execute a cell, or select a stale element.
- Run one representative owned native smoke **per designer only if necessary to establish a previously untested owner**; respect the overall 12-session cap and prefer headless tests for remaining controls. Native screenshots only of generated harmless menus/layouts, no user-config content. Report canvas/drag-only/foreign-popup controls as deliberately non-hintable, not incorrectly `fully supported`.

**Verification:** One focused designer-owner and identity slice; at most two native owned sessions if budget remains.  
**Commit:** `test(jump): [M9-D] verify designers and nested focus integrity`.  
**Done:** Priority-8 editors meet safe coverage inventory and exact modal safety criteria.

---

## M10 — Integration, native evidence and resource sanity (no new feature area)

**Depends:** M2–M9. **Owners:** existing Jump engine, panel owners, `dev/jump_mode` harness and documentation. No new production behavior unless a real integration defect is found.

### M10-A — Cross-dialog ownership integration and no-leak focus proof

- Build one **combined targeted** egui/controller test selection covering representative Root→File Search→Snippets→Clipboard→JSON/Regex/XML→Mouse→Rename/Replace→Dashboard/Radial Editor transitions. Include an unknown topmost child window, recognized ComboBox popup, nested destructive confirmation and disabled feature gate.
- Assert that exactly the active owner can advertise hints; parent and root cannot receive leader/key input while child is foreground. Stale selected entries from earlier windows cannot be activated; fast open/close/reopen and discarded UI passes do not leak a held completion/Enter into the next owner.
- Confirm current source Jump tests for query, grid, dashboard, Settings, Notes/Todo/Timer/Paste/MkMacro remain green where directly impacted; **do not rerun all previous native smoke cases** merely for their names.
- Verify no new per-frame tasks, input hooks or UI repaints were added; hint registration work should scale with **rendered visible widgets**, not total hidden rows. Take one bounded contemporary visible-grid CPU sample only if evidence suggests actual user-facing slowdown; do not create a new benchmark track.

**Verification:** At most **two** deliberately curated Jump-related integration Nextest slices.  
**Commit:** `test(jump): [M10-A] protect all dialog owner handoffs`.  
**Done:** Cross-dialog focus, stale-ID, modal, disabled and control activation invariants verified.

### M10-B — Bounded native smoke, evidence index and user documentation

- Use existing `dev/jump_mode/run.ps1` owned-app/QA mechanisms, with typed fixture profiles and absolute isolation. For each priority group, pick **one most valuable safe, verifiable representative state**; prefer test-owned focus/Select/Cancel over mutations. Reuse M2–M9 native evidence already captured. Do not demand all 12+ dialogs receive separate native sessions.
- Limit cumulative native smoke to 12 sessions/24 images, reusing earlier milestone evidence first. Record actual HWND/PID, owned process cleanup, active scope, real hint→focus, later separate valid action where benign, the screenshot receipt and a redacted image. Do not call source-level `selected` assertions a real OS focus proof unless the existing fixture confirms actual native focus.
- If a surface cannot be captured because of an environment restriction, one diagnostic and one retry are the whole budget; record `not_run_environment` with the actual reason and trust focused Rust/egui fixtures, not fabricated screenshots or synthetic pass statuses.
- Document succinctly in README/Quick Help: Jump Mode covers the eight groups with supported visible controls, separate activation, row highlighting, safe confirmations, and intentionally excluded recording/canvas controls. Preserve existing Shift+Space configuration/default-off behavior.
- Finish changes to fixture runner and documentation before final verification; avoid building a large screenshot archive or updating old feature plan files unrelated to this initiative.

**Verification:** Fixture-only parser/manifest checks, actual native outcomes when accessible, one review of the screenshot/evidence index.  
**Commit:** `docs(jump): [M10-B] document expanded coverage and owned evidence`.  
**Done:** User has a truthful control inventory and representative evidence for all eight areas.

---

## M11 — Closeout: one complete Nextest, final formatting, Clippy last

**Depends:** all unblocked groups. This project is substantial enough for **one** complete test pass near the end; no repeated broad validations. The final source/audit result is recorded in the response if it arrives after the ledger is closed.

### M11-A — Complete integration and one final full Nextest run

- Inspect combined diffs, source ownership and known blocked cases. Verify all eight groups have a checkpoint, supporting tests and per-area target inventory; for every explicitly unsupported control record why it is pointer-only, out of viewport, dangerous or lacks a real response. Do not silently drop a plugin.
- Run `cargo fmt`/scoped rustfmt on maintained changed files only, leaving untouched original formatting in `src/bin/radial_acceptance/native.rs`, `src/lib.rs`, `src/paste_sequence/playback.rs` intact. Run final ordinary `cargo check --bin multi_launcher` and correct actual compile errors *before* full Nextest.
- Perform **one** `cargo nextest run --workspace --no-fail-fast` on the supported Windows host, respecting the existing feature/target rules (do not enable `--all-features` and thereby promote dev/perf binaries unless specifically needed). If it fails for introduced behavior, repair the exact root cause and rerun **once at most** after a focused regression. Record existing unrelated failures separately; never weaken tests to report success.
- Avoid a new multi-hour benchmark or automatic retry loop. A known environmental inability to launch native GUI is separate from Rust suite test failures.

**Verification:** One full Nextest; at most one repaired full rerun.  
**Commit:** `fix(jump): [M11-A] resolve final integration failures` only when real source/test changes.  
**Done:** Correctness outcome recorded; all code correction/fmt done before final lint step.

### M11-B — Finish docs, commits and clean execution ledger

- Finalize `docs/plans/jump_mode_expanded_execution.md`: all milestone statuses, commit hashes, coverage per plugin, snapshots, focused tests, final Nextest result and genuine exclusions/blocked behavior. Finish README/Quick Help and relevant annotations. Don't modify historical implementation logs or giant screenshot galleries solely to imply more coverage.
- Review staged and unstaged diffs, `git diff --check`, working-tree ownership and upstream. Finish all non-lint code/docs commits. Distinguish one-time push failures from committed work, then move to M11-C.
- Prepare exact maintained workspace Clippy command without accidentally invoking it early. The known untouched Pinboard `never_loop` lint and unrelated baseline rustfmt differences should be called out if still present, not treated as this feature's faults.

**Verification:** Source/ledger/diff inspection; no second full suite.  
**Commit:** `docs(jump): [M11-B] finalize expanded coverage handoff`.  
**Done:** All code, formatting, test fixtures, help and ordinary verification are complete.

### M11-C — Final maintained-code Clippy and stop

- **Only now** run Clippy on maintained application/workspace targets, e.g. `cargo clippy --workspace --all-targets -- -D warnings` when applicable. Confirm actual workspace membership; vendored path dependencies outside membership must not become explicit lint targets. Do not edit vendored egui or other third-party source for lint-only reasons.
- Repair **introduced/affected** maintained-code warnings in bounded passes; do not launch broad unrelated cleanup for the previously recorded unchanged Pinboard `never_loop` lint or unrelated source formatting. A genuinely failing project-wide Clippy command may remain `blocked_existing_baseline` with the exact diagnostics reported; never falsely call it green. To establish that the Jump Mode additions themselves are warning-free, additionally use the narrowest appropriate maintained-target lint scope permitted by the current compiler/toolchain, without blanket `allow` annotations.
- If Clippy fixes require formatting or directly affected focused tests, execute those before the **last Clippy attempt** and commit the lint cleanup. After the final successful maintained scope Clippy command (or after a bounded unresolved documented lint gate), do not continue coding, rerun format, add test harnesses or start new verification. Report final status immediately.

**Verification:** Only final Clippy phase, recorded exact command/output.  
**Commit:** `chore(jump): [M11-C] clean expanded coverage lints` *before* last lint attempt if source changed.  
**Done:** User receives final implementation/verification report; no further work in this initiative.

---

## 3. Acceptance matrix — must be visibly fulfilled

| Area | Focus-only proof | Later native/egui action proof | Stale/unsafe negative proof |
|---|---|---|---|
| File Search | Query/Refine/modes/visible result rows | Separate Search/Select/Open/Copy in safe fixture | Re-sort or new search cannot activate stale path; root blocked |
| Snippets | Saved entry/alias/tags/body after reveal | Fresh Save/Cancel/Preview | Hidden content and prompts never leaked/submitted |
| Clipboard | Visible opaque entry/edit focus | Fresh Edit/Copy/Remove confirmation | External history insert/duplicate text cannot redirect; no Copy on hint |
| Dashboard | Pinned Commands/Command History/Todo/Scratchpad/Calendar | Fresh exact-once FillQuery/benign action | Slot reorder and hidden widgets reject stale targets |
| JSON/Regex/XML | Source fields/tabs/search/result nodes and case rows | Fresh Next/Previous/select/copy where safe | Old parse/preset/document results don't retain actionable hints |
| Mouse | Settings fields/checkboxes/sliders, gesture/binding rows | Fresh Apply/Cancel/binding selection | Recording/capture/native pickers cannot route Jump keys |
| File operations | Rename/Replace fields/results/approvals | Fresh Preview/Cancel; confirm only through child owner | No Apply/Undo/Restore from hint alone; no file changed in fixture |
| Designers | Dashboard slots; radial rings/cells lists; property forms | Fresh Save/Discard/Cancel in generated config | No synthetic canvas drag, no stale cell selection or background modal action |

The final report must identify **all eight groups** separately with their supported control inventories, tests, commits, native evidence or environment limitation, and genuinely excluded individual controls. No generic claim that “Jump Mode now works everywhere.”

## 4. Stop conditions and scope safeguards

- Wrong target activated after a hint, stale result writing a different file, background window handling input, same-frame implicit Enter/Space, clipboard/prompt payload leaked into hint labels or screenshot, active gesture/MkMacro recorder competing for keys, confirmation bypass, or parent focus surviving child modal: these are **blocking correctness defects**. Preserve/disable only the unsafe affected route until fixed; never declare it complete.
- A dialog without safe native owner/Response should be recorded as **specific unsupported subcontrol**, not silently assigned a synthetic clickable area. Cover every other safe visible action in that plugin as required.
- Repeated timing/pixel failures, test harness scaling issues and pre-existing unrelated Clippy failures do not justify speculative framework redesign. Document after bounded attempts; move to independent implementation areas.
- Never invoke unrestricted pointer/keyboard control on real user applications. New fixtures read/write only generated roots and do not execute unsafe existing macros, hot folder rules, Screenshot Pin capture or destructive file operations.
- Preserve all previous Jump Mode settings, leader/badges, known supported surfaces and plain launcher operation when Jump Mode is disabled. No new persistent navigation state or separate settings toggle per plugin.

## 5. Final handoff contract

Deliver a concise summary with: execution branch/HEAD; checkpoint commit index; implementation status for priorities 1–8; target inventories and real exclusions; one final full Nextest outcome; final maintained-crate Clippy/unchanged baseline outcome; native fixture evidence (or precise `not_run_environment`); changed-source inventory; no significant owner/input/privacy regressions known. The user performs a single optional manual review **after** autonomous Codex completion; do not ask them to operate the launcher during implementation.

**Final instruction:** Implement every priority 1–8 in this one plan, with one writer, small checkpoints, reusable `JumpModeScope`, unchanged action semantics, focused verification throughout and final bounded integration. Do not stop after File Search/Snippets or turn the task into another independent Jump engine. Finish, commit/push best effort, report and stop.
