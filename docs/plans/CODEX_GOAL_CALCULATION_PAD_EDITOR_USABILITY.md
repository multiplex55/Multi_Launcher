# Codex Goal — Calculation Pad Editor Usability

## A0. Mandatory Chained Git Branch Bootstrap — Before All Application Work

**First execution checkpoint: M0-A.** Read the repository's `AGENTS.md`, applicable local instructions, this entire Goal, and its companion `CODEX_KICKSTART_CALCULATION_PAD_EDITOR_USABILITY.md`. Then perform the read-only Git inspection below. Do not edit application code, run implementation builds/tests, or create implementation checkpoints until the child branch and its baseline exist.

**One Goal/Kickstart pair means one fresh task branch.** Suggested name: `feat/calculation-pad-editor-usability`; choose a unique lowercase kebab-case variant if necessary. Checkpoints and agents share this branch.

1. In the **single existing repository checkout**, inspect `git status -sb`, current branch, `HEAD`, remotes, upstream, staged/unstaged/untracked changes, and the actual queue/orchestrator/execution handoff for the immediately preceding completed Goal/category. Preserve these observations.
2. Identify and record `parent_branch` and `parent_tip_sha`. In a continuing chain, the required parent is the immediately preceding completed Goal/category's exact `final_tip_sha`, including a locally committed tip whose push failed. Verify it is committed and available. If available only remotely, fetch safely and verify the exact tip. **Only the first Goal in a genuinely new chain may default to the up-to-date primary integration branch, normally master/main.** Current checkout, old archived plans, or a stale remote are not proof of the correct predecessor. A reference to master for code inspection does not override this ancestry contract.
3. If parent records conflict or the required commit is unavailable, report a bounded ancestry blocker. Never guess, substitute an older ancestor, or silently restart the chain from master/main. For a first task with no predecessor, a documented current-HEAD base is permissible only when safe and no user work is lost or inadvertently committed.
4. Protect user work. Never automatically reset, hard-pull, stash, rebase, merge, force-checkout, or overwrite changes to manufacture a safe transition. Do not stage unrelated changes. If local work makes switching to the required parent unsafe, preserve it and report the precise blocker.
5. Select a fresh unique task name and create/switch directly from the verified SHA, for example `git switch -c feat/calculation-pad-editor-usability <parent_tip_sha>`. Verify the new branch is active and starts at that exact commit.
6. Create the branch's own **M0-A baseline commit**, preferably a small task execution ledger; an intentional empty commit is also acceptable: `git commit --allow-empty -m "chore(calc-pad): [M0-A] establish editor usability baseline"`. The predecessor's final commit is not this Goal's baseline. Capture `baseline_sha` and verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`.
7. Attempt one ordinary initial upstream push, such as `git push -u origin HEAD`, when a suitable remote/access exists. Record the actual result. A rejected/unavailable push does not block safe local implementation: retain commits, do not bypass restrictions, and do not enter retry loops.
8. Perform all subsequent implementation, tests, and checkpoint commits on this same branch. Attempt one ordinary best-effort push after each required checkpoint. Never force-push or rewrite checkpoint history.

**No Git worktrees:** do not run `git worktree add`, `git worktree move`, or `git worktree prune`; do not create auxiliary checkouts, helper clones, agent-specific clones, or independent side branches. Source writes and Git operations are sequential in the existing checkout. Read-only planning/review may run in parallel.

Use the current execution ledger if one already governs this task. Otherwise, the proposed task-local location is `docs/plans/execution/standalone/calculation-pad-editor-usability/EXECUTION.md`. Record task state, parent identity, baseline, checkpoints, evidence paths, push state, and blockers without creating a competing queue ledger. Capture the baseline SHA after its commit and persist it in the next normal update.

**M0-A acceptance:** the branch is new, unique, and active; the correct parent is recorded; the baseline directly descends from that parent; initial push was attempted or its precise unavailability recorded; no user work was staged/lost; no worktree or parallel implementation branch was created.

**Final-tip handoff:** after the final commit, record `final_tip_sha` in the final report and existing external execution state, with all available SHAs in the tracked ledger. Do not amend endlessly to embed a commit's own SHA inside itself. If another reporting commit is made, its tip becomes the final handoff. The next Goal must create its branch directly from this exact final tip. Do not merge, rebase, squash, or cherry-pick intermediate branches merely to advance the chain. The last branch contains cumulative history; final integration is a separate approved boundary.

## A. Goal / Desired Outcome

Make Calculation Pad source editing and result navigation comfortable for longer scripts while preserving its existing evaluation, identity, persistence, and host-lifecycle behavior.

The user can show or hide physical source line numbers, choose wrapping or horizontal scrolling, drag the Source/Results divider, and right-click a result to reveal its source line with a short-lived highlight. Results display meaningful values and diagnostics without comment/blank placeholder rows. Full-line `//` comments work alongside `#`. Balanced nested function calls work and have focused regression coverage.

The intended example is:

~~~text
x = min(100, min(50, 75))
~~~

It produces `50`. The supplied example omitted the outer closing parenthesis; this Goal treats that as a typo. **Automatic parenthesis repair, permissive evaluation of unfinished expressions, and source rewriting are outside scope.**

This is one bounded Goal, with one task branch and seven checkpoints including bootstrap and final verification. Implement the actual changes through the repository's configured implementer; the parent/orchestrator coordinates planning, verification, review, and commits.

## B. Current Codebase Findings

Source review reference: [master commit 0b93e0ded23d7671feab4525cbea89a1c55170e6](https://github.com/multiplex55/Multi_Launcher/commit/0b93e0ded23d7671feab4525cbea89a1c55170e6), inspected November 12, 2026. This is a code reference, **not** a required execution parent. After M0-A, revalidate the relevant symbols in the actual checkout and adapt locations without expanding scope. Source inspection established the following; it did not execute builds or tests.

| Current owner | Relevant behavior and implication |
| --- | --- |
| `src/calculation_pad/document.rs`: `prepare_line`, `declared_names`, `CalculationDocument::evaluate`, `LineOutcome` | Only full-line `#` is recognized. Both preparation and declaration collection classify comments independently. Evaluation retains every physical source line, including blanks/comments. |
| `document.rs`: `scalar_from_scan`, `scan_expression`, `normalize_scalar_tokens`, `lower_mod_function_calls`; `operators.rs`: `CheckedOperators` | Existing exmex parsing and nested `mod` lowering already support nested structure. Binary `min` and `max` are registered. Unbalanced parentheses are rejected. Characterize the exact requested balanced case before changing parsing. |
| `src/calculation_pad/session.rs`: `SessionLineResult`, `evaluation`, `resolve_dependency_line` | A row has stable `LineId`, physical `line_index`, and UTF-8 byte `source_range`. Evaluation is cached per revision. Navigation can resolve a stable line with document/incarnation checks. |
| `src/calculation_pad/dependencies.rs`: `DependencySnapshot::from_evaluation` | Dependency identity comes from the complete physical-line evaluation. Filtering this model would break established semantics. |
| `src/gui/calculation_pad_dialog.rs`: `contents_ui` | At width ≥680 points, two fixed equal-width columns; below that, Source is stacked above Results at roughly 48% height. There is no user-dragged divider. |
| `calculation_pad_dialog.rs`: `editor_ui` | One multiline `TextEdit` inside `ScrollArea::both`, currently sized to available width. Horizontal scrolling infrastructure exists, but layout still wraps ordinary long lines. There is no source gutter. |
| `calculation_pad_dialog.rs`: `results_ui`, `copy_all_results` | Results currently render explicit blank/comment groups. Only the small value-row `rN` label has the comparison context menu. Copy all already omits blanks/comments in its Results-only section while retaining source text in source-containing sections. |
| `calculation_pad_dialog.rs`: `navigate_dependency`, `DependencyEditorUpdate`, `HostEffects`, `complete_pad_host`, `reset_transient_ui` | Existing navigation stages character-based caret/selection and scroll state, then publishes only for a retained/current host pass. It already handles Unicode offsets and stale identity. Reuse this boundary for result navigation. |
| `src/settings/model.rs`, `src/settings_save.rs`, `src/settings_editor/save.rs`, `src/gui/mod.rs`, `src/gui/render.rs` | Existing recovery/Programmer Inspector preferences demonstrate initialization, typed save intents, pending-save coalescing, completion, and settings-editor reconciliation. Reuse this for the small editor-preference addition. |
| `src/gui/diagram_studio.rs`: `validated_split_ratio`, `split_pane_widths`, `split_ratio_from_pointer`, `draw_splitter` | Existing bounded split-pane interaction is a useful pattern. Its helpers are private and mostly horizontal; do not couple Calculation Pad to Diagram Studio's domain. |
| `vendor/egui/src/widgets/text_edit/builder.rs`; `vendor/epaint/src/text/text_layout_types.rs` | Inspect the pinned TextEdit layouter and actual Galley/PlacedRow geometry for wrapping, clipping, and line positions. These are reference files, not planned modification targets. |
| `src/calculation_pad/README.md`, `src/calculation_pad/examples.rs` | Authoritative syntax documentation and executable in-app examples. Update only the directly affected behavior. |

Important existing tests include:

- `supports_remainder_and_contextual_euclidean_mod_without_reserving_mod` in `document.rs`.
- `dependency_navigation_applies_current_utf8_caret_and_scroll_only_after_retention`.
- `editor_updates_dependencies_errors_and_session_owned_keyboard_undo_redo`.
- `explicit_copy_buttons_return_single_result_and_source_order_summary`.
- `accepted_copy_resolves_after_same_frame_source_edit` and `retired_line_copy_is_refused_after_same_frame_edit`.
- `rejected_child_or_retired_host_effect_does_not_apply_source_or_copy`.
- `studio_usability_calculation_host_migration_preserves_cached_session_and_pending_decision`.
- `constrained_window_keeps_editor_and_result_controls_inside_the_viewport`.
- `all_results_summary_preserves_physical_line_numbers_and_comments`.

The GUI tests above are in `src/gui/calculation_pad_dialog.rs`. Extend relevant behavioral coverage; do not duplicate the whole historical host-validation program.

## C. Approved Functional Requirements

These requirements implement the requested behavior with conventional defaults. No product questionnaire is required.

### C1. Source line numbers

Provide an obvious **Line numbers** toggle near the Source controls, initially **on**. Display one-based physical source lines, including comments, blank lines, and the trailing empty line when present. Numbers are a gutter, never inserted into source or copied text.

The gutter stays vertically aligned with the actual editor layout and remains visible while the text scrolls horizontally. Wrapped continuation rows do not receive new physical line numbers. Toggling numbers preserves source bytes, caret/selection, dirty status, evaluation revision, and undo/redo.

### C2. Result-to-source navigation

Right-click a visible result row and select **Go to source**. Make the action available from the row's normal body, including the result text/label and unused row area, for both successful results and error rows.

Reveal the correct physical source line in the current Calculation Pad host, give its Source editor focus once, position the caret at the target line, and show an approximately **1.5-second** highlight over the target line's visible visual rows. Normal caret/selection behavior remains available; highlight expiry must not overwrite a newer user selection.

Target stable `LineId` plus document/incarnation and host provenance. Never target the ordinal of the filtered grid. Re-resolve or reject according to the existing current-source acceptance contract; a stale/deleted line or retired host must never jump to an unrelated line.

Preserve **Select for comparison**, Copy, and Inspect bits where currently applicable. A row context menu must not accidentally activate these actions or steal focus on subsequent frames.

### C3. Comment syntax

A line whose first non-whitespace characters are `//` or `#` is a full-line comment. Comment bodies may contain assignments, unknown identifiers, or malformed expression text without affecting evaluation or declared names.

Preserve source text and line endings. Ordinary `/` division stays unchanged. This requirement does not introduce inline comments, block comments, integer division, or comment stripping inside expressions/braced names.

### C4. Results visibility

Omit `LineOutcome::Blank` and `LineOutcome::Comment` before creating result-row UI containers. Also omit a successful result whose actual formatted display text is empty or whitespace-only, if such a value is representable.

Keep numeric zero, all valid nonempty values, errors, incomplete-expression diagnostics, and document-level diagnostics visible. A formatter returning `None` is an unavailable/diagnostic state, not a successful blank; retain visible feedback rather than silently hiding it. Do not manufacture a new empty-value domain type just to test this condition.

This is a **Results presentation filter**. Do not remove lines from source, evaluation, session state, dependency snapshots, or stable IDs. Keep `rN` equal to physical source line N, so visible result labels may have gaps.

Preserve the source-containing Copy all sections, including comment/blank text and line separators. Its Results-only section retains its established behavior and physical labels.

### C5. Horizontal scrolling and optional word wrap

Provide **Word wrap**, initially **off**, near the Source controls. With wrapping off, long lines remain single logical display lines and can be reached by a real horizontal scrollbar; the outer pane/window remains bounded. With wrapping on, text wraps to the available editor width without changing source bytes.

Use the same TextEdit, session history, and widget identity. Changing wrap or gutter visibility must not reset the document or select a different line. Clamp/reset an obsolete horizontal offset when enabling wrapping so the start of the source remains reachable. Vertical scrolling and cursor-driven reveal must still work.

### C6. Resizable Source/Results panes

Preserve the existing responsive orientation:

- Wide window: Source left, Results right; drag the vertical divider left/right.
- Narrow window: Source above Results; drag the horizontal divider up/down.

Start near the current 50% wide and 48% stacked source allocations. Maintain independent ratios for the two orientations. Provide a visible, practical drag target and appropriate resize cursor. Clamp to finite, usable pane sizes, accounting for headings, controls, divider, padding, and scrollbars.

Both panes and their controls remain reachable at minimum window sizes. Preserve the existing bounded outer-scroll fallback when available space is insufficient. Resizing must not change calculation contents or continuously force source focus.

### C7. Display preferences and lifecycle

Remember line-number visibility, wrapping, and both split ratios across close/reopen, dock/popout, and application restart. These are small application-level Calculation Pad display preferences, not per-document content.

Add backward-compatible defaults through the existing settings owner. Missing/invalid preference values fall back safely; invalid ratios must not produce NaN, negative, or unusable layouts. Do not migrate saved-pad/recovery formats or change the existing Remember last source opt-in.

Changes take effect through the retained/current GUI pass boundary. Persist explicit toggle changes and completed divider drags through the existing settings-save mechanism; coalesce pending preference changes and avoid disk writes for every drag frame. Preserve unrelated settings and open Settings editor drafts.

### C8. Nested functions

Support balanced calls nested in either operand position and with other arithmetic/variables, within the current nesting/size limits. First verify:

~~~text
min(100, min(50, 75))
min(min(100, 50), 75)
max(min(100, 50), min(75, 60))
~~~

Expected values are `50`, `50`, and `60`. If these already work, preserve the parser and satisfy this requirement with focused coverage and documentation/examples. Fix only a demonstrated relevant defect.

The malformed original `x = min(100,min(50,75)` remains an error. Preserve error propagation for invalid inner arithmetic, current operator arity, case normalization, units, vectors, typed integers, and contextual `mod`.

## D. Scope and Non-Goals

Included: Calculation Pad syntax classification, Results presentation, source-editor geometry/preferences, pane resizing, stable row navigation/highlighting, narrow settings integration, relevant examples/docs, and focused verification.

Preserve source-authoritative evaluation, stable identities, existing saved-document formats, session-owned undo/redo, dependency navigation, comparison/inspector behavior, source recovery, current launch commands, global hotkeys, and root-query interactivity while utilities are popped out.

Do not add a new editor framework, parser, dependency, configurable hotkey, workspace feature, language autocomplete, per-document settings schema, syntax-highlighting redesign, search/replace UI, or application-wide split-pane migration. Do not modify vendored egui/epaint, release profiles, unrelated failing tests, or configured agent models as part of this Goal.

Keep existing bounds, including 64 KiB source, 256 physical lines, 2048-byte expressions, and nesting depth 32 unless the current checkout has intentionally changed them. This request does not raise limits.

## E. Technical Implementation Strategy

### E1. Keep model semantics separate from grid visibility

Use one consistent private full-line-comment classification in `prepare_line` and `declared_names`. Keep all source/evaluation rows. Perform visibility classification in the Results presentation layer, before row allocation. Format once per rendered result when practical; do not reevaluate the document on repaint just to decide visibility.

Include a regression document where comments/blanks precede named and `rN` references. A hidden row must not shift dependency labels, copy targets, inspector selection, or navigation.

### E2. Reuse one editor and its actual text geometry

Keep the existing TextEdit and session-owned history. Derive gutter and temporary-highlight positions from the actual Galley/PlacedRow layout, including newline boundaries and final empty rows. Account for UTF-8 byte ranges versus egui character indices, CRLF, clipping, viewport scale, horizontal offset, and wrapped continuations.

Use the pinned egui API to ensure true no-wrap layout. Inspect the builder and layouter behavior rather than assuming a width setting alone is sufficient. A small custom layouter is permissible if needed. Reserve gutter width separately so it does not horizontally scroll away or obscure selection. Do not create independently editable copies of the source.

### E3. Stage UI changes through the existing accepted-effect boundary

The renderer may run sizing, discarded, docked, or native passes. New preference changes, divider updates, navigation requests, and highlight startup must not publish from an unauthorized/discarded pass or retired host.

Extend `HostEffects`/completion handling or equivalent existing typed intents narrowly. Source-dependent actions must carry/check document basis. If source changes in the same frame, follow existing acceptance ordering and reject invalidated actions safely. Do not bypass existing deferred focus behavior or repeatedly request focus.

### E4. Generalize existing navigation locally

Reuse `navigate_dependency`, `resolve_dependency_line`, the UTF-8-to-character mapping in `editor_ui`, and the retained-pass scroll/caret publication in `complete_pad_host`. A focused rename/generalization of dependency-specific internal state is acceptable; preserve the dependency dialog's existing behavior.

Result navigation should place the caret and paint temporary highlighting separately from ordinary text selection. Associate the highlight with current document/line identity and expiry. Clear it on invalidating edits, replacement, closure, or host retirement as appropriate. Request a bounded repaint for expiry; do not create a timer thread or permanent repaint loop. Wrap changes may recompute geometry without changing the target's physical identity.

### E5. Keep pane resizing and settings integration local

Adapt the existing Diagram Studio splitter pattern without importing its state or refactoring that plugin. Handle both orientations with current available geometry and minimum sizes.

Add a compact typed preference object in `src/settings/model.rs`; proposed field/type names are implementation choices, not existing APIs. Initialize via `src/gui/mod.rs`, emit a committed dialog effect through `src/gui/render.rs`, and integrate with `SettingsSaveIntent`/`SettingsSaveSource`, pending saves, and `src/settings_editor/save.rs`. Follow the Programmer Inspector preference pattern for field ownership, concurrent general saves, open-draft reconciliation, failure feedback, and newer pending values.

No source data goes into the new preference fields. A settings-write failure should leave the current source safe, give normal visible feedback, and not claim restart persistence succeeded. Persist the latest completed drag/toggle once the existing save pipeline can accept it.

## F. Detailed Milestones and Tasks

### M0-A — Establish the correct chained task branch

- **Objective:** Establish authorized ancestry and a separate baseline before implementation.
- **Code ownership:** Existing checkout, task execution ledger, Git state only.
- **Implementation tasks:** Execute every step in A0; capture current user changes; resolve parent; create child; commit baseline; attempt upstream push.
- **Behavioral requirements:** All later work belongs to the one fresh branch.
- **Edge cases:** Dirty index, ambiguous predecessor, unavailable local-only ancestor, missing upstream, rejected push.
- **Regression constraints:** Preserve user work and history; no worktrees/auxiliary clones or implicit integration.
- **Verification:** Read-only branch/status checks; baseline parent equality; actual push receipt.
- **Acceptance criteria:** All A0 bootstrap conditions satisfied; source reference differences are recorded after safe bootstrap.
- **Commit boundary:** `chore(calc-pad): [M0-A] establish editor usability baseline`.

### M1-A — Comment syntax and nested-call coverage

- **Objective:** Recognize full-line `//` consistently and establish correct nesting behavior.
- **Code ownership:** `document.rs`, directly relevant `operators.rs` only if a defect is proven, `README.md`, `examples.rs`.
- **Implementation tasks:** Characterize balanced nested min/max; unify comment classification in preparation/declaration scanning; add a small set of meaningful tests; update syntax and one suitable executable example.
- **Behavioral requirements:** Indented `//` and `#` are inert comments; division and valid nesting retain semantics.
- **Edge cases:** Comment text containing `=` or malformed syntax; `// pi = 7` must not shadow the `pi` alias; unbalanced parentheses; invalid inner arithmetic.
- **Regression constraints:** No parser replacement, implicit repair, new dependencies, source normalization, or raised limits.
- **Verification:** Run exact new/affected document tests and the existing contextual-mod nesting test; verify expected nested results and malformed-input error.
- **Acceptance criteria:** New comments do not enter declarations/dependencies; nested examples pass; malformed input remains visible as an error; docs match implementation.
- **Commit boundary:** `feat(calc-pad): [M1-A] support slash comments and verify nested calls`.

### M1-B — Hide non-result rows without changing identities

- **Objective:** Remove blank/comment/actually empty result rows from the Results grid.
- **Code ownership:** `calculation_pad_dialog.rs::results_ui`, relevant existing GUI/copy tests.
- **Implementation tasks:** Filter before building row groups; distinguish blank-success formatting from unavailable/error states; retain physical labels, IDs, and existing copy/inspector behavior.
- **Behavioral requirements:** Only meaningful values and diagnostics have rows; zero stays visible; gaps in `rN` labels are expected.
- **Edge cases:** Empty source, all comments, whitespace-only lines, CRLF, `x =`, unavailable formatting, hidden predecessors.
- **Regression constraints:** No filtering of session/evaluation/dependency collections or source-containing Copy all text.
- **Verification:** Focused mixed-source rendered-row test plus existing copy-all/physical-numbering coverage; validate references after hidden lines.
- **Acceptance criteria:** Visible rows are correct and all targets retain original physical identity; errors and diagnostics remain accessible.
- **Commit boundary:** `feat(calc-pad): [M1-B] hide blank and comment result rows`.

### M2-A — Line numbers, horizontal scrolling, wrap, and display persistence

- **Objective:** Provide usable source display controls through the existing editor.
- **Code ownership:** `calculation_pad_dialog.rs`; `settings/model.rs`; narrow initialization, rendering-effect, settings-save, and settings-editor completion paths named in E5.
- **Implementation tasks:** Add compatible typed display preferences; implement Line numbers and Word wrap controls with the chosen defaults; derive gutter positions from the editor galley; ensure no-wrap content truly overflows horizontally; integrate accepted preference saves.
- **Behavioral requirements:** Gutter numbers physical lines once; no-wrap scrolls; wrap fits pane; settings survive restart and host changes.
- **Edge cases:** Unicode/braced names, CRLF, trailing newline, long lines, stale horizontal offset, malformed/missing preferences, settings save while another save/draft exists.
- **Regression constraints:** One TextEdit and stable identity; source/dirty/history/revision unchanged by display controls; no saved-pad schema change; no dropped unrelated settings.
- **Verification:** Focused geometry/state tests for physical mapping and wrap/scroll behavior; preference default/round-trip and narrow concurrent-save/ownership regression; check host migration with existing session tests.
- **Acceptance criteria:** Controls work in both hosts; source bytes and history are preserved; old settings load; completed preference changes are saved correctly or report failure honestly.
- **Commit boundary:** `feat(calc-pad): [M2-A] add line numbers and source wrapping controls`.

### M2-B — Drag the Source/Results divider in both layouts

- **Objective:** Let the user allocate more space to either pane.
- **Code ownership:** `calculation_pad_dialog.rs::contents_ui` and local splitter geometry; extend the existing editor preferences with both orientation ratios.
- **Implementation tasks:** Replace fixed column/height allocations with a bounded splitter; implement appropriate drag cursor/hit area; budget headings and scrollbars; stage accepted ratio changes; persist on drag completion.
- **Behavioral requirements:** Wide divider moves horizontally; stacked divider moves vertically; each layout remembers its own ratio.
- **Edge cases:** Window resize across 680-point breakpoint, extreme drag, tiny usable area, invalid stored ratios, open chooser/dirty decision, docking mid-interaction.
- **Regression constraints:** Keep both panes reachable; preserve outer bounded scrolling and editor/copy controls; no source changes or per-frame settings writes.
- **Verification:** Focused geometry/drag tests for both orientations and clamping; existing constrained-window and host-migration checks as affected.
- **Acceptance criteria:** Source can be visibly enlarged in both layouts, both panes remain usable, ratios survive close/reopen/restart, and discarded passes cannot publish a drag.
- **Commit boundary:** `feat(calc-pad): [M2-B] resize source and results panes`.

### M3-A — Row-wide Go to source and temporary highlighting

- **Objective:** Navigate from any visible value/error result to its actual source line.
- **Code ownership:** Results context menu, existing navigation state/resolution, `editor_ui`, `complete_pad_host`, transient lifecycle cleanup, and relevant GUI tests.
- **Implementation tasks:** Add Go to source to the full row interaction; carry stable identity; reuse accepted navigation/caret/scroll machinery; add an expiring paint highlight; preserve existing comparison/copy/inspection actions.
- **Behavioral requirements:** Correct target is revealed once; Source receives focus in its current host; highlight expires after about 1.5 seconds without clearing a later selection.
- **Edge cases:** Hidden predecessors, error rows, wrapped/Unicode/CRLF targets, offscreen line, same-frame edit, deleted line, new/open/close, discarded pass, retired host.
- **Regression constraints:** Do not use displayed ordinal; no stale focus steal, source edit, history change, extra evaluation, or permanent repaint. Preserve dependency navigation.
- **Verification:** Extend retained-navigation test patterns to value/error result menus; assert current target, stale rejection, bounded expiry, unchanged source/revision/history, and existing comparison action.
- **Acceptance criteria:** Right-clicking row text/space exposes the command; accepted navigation reaches the correct line in docked and popped-out modes; stale requests have no unintended effect; highlight is temporary.
- **Commit boundary:** `feat(calc-pad): [M3-A] navigate result rows to highlighted source lines`.

### M4-A — Focused acceptance, final review, and handoff

- **Objective:** Establish scoped completion evidence and deliver the committed final tip.
- **Code ownership:** Only feature-related fixes/docs/tests, task ledger, and evidence; configured reviewer reads the full change set.
- **Implementation tasks:** Complete H; run the bounded real-UI smoke when supported; review all feature changes; resolve relevant findings; finish source/help documentation and ledger; inspect final status.
- **Behavioral requirements:** All C requirements pass with truthful automated/native status.
- **Edge cases:** Unrelated warnings/test compilation failures, unsupported native environment, failed push, missing evidence, unresolved feature defect.
- **Regression constraints:** No automatic full suite, unrelated remediation, repeated release builds, or historic acceptance campaign.
- **Verification:** Selected affected cases, one appropriate final first-party Clippy pass, scoped diff review, UI evidence, and final ancestry/status checks.
- **Acceptance criteria:** J is satisfied, or explicitly report limited/blocked items with precise evidence; all intended changes committed; exact final tip and push state recorded for the next Goal.
- **Commit boundary:** `docs(calc-pad): [M4-A] record editor usability verification and handoff`. If review fixes are needed, use small `fix(calc-pad): [M4-A]` remediation commits before the final report commit; do not rewrite earlier checkpoints.

## G. Active Checkpoint Commit Cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

The sequence is M0-A → M1-A → M1-B → M2-A → M2-B → M3-A → M4-A, all on the same Goal branch. These are coherent changes, not separate work categories or branches.

Before each commit, inspect diff/staged paths and current branch, exclude unrelated work, and perform checkpoint-appropriate verification. Use inexpensive local evidence where sufficient; do not rebuild everything at every checkpoint. Run substantive focused verification at its relevant milestone and close remaining coverage in M4-A.

Commit before a materially different subsection. Attempt one normal push after each required checkpoint; preserve local progress on failure. Record checkpoint SHAs, evidence, and push outcomes. No force-push, squash, rebase, or history rewrite. Parent identity, this Goal's distinct baseline, and final handoff remain separately recorded.

## H. Verification and Testing Strategy

### H1. Target selection and resource budget

Use Cargo Nextest, restricting compilation to the needed package/target and execution to exact/module filters. `--lib` avoids building unrelated integration-test targets, but filtering tests does **not** eliminate compilation of other tests in the selected library binary. If unrelated test compilation blocks the target, identify it honestly; do not turn this Goal into a full-suite repair task.

Start with at most **two Cargo build jobs** and **one test process at a time** on the execution machine. These are distinct settings. Use one build/test process concurrently; agents do not compete for the machine.

For this Goal, use a **100-second wall-clock bound per selected test-execution batch, excluding compilation**. Split meaningful groups if needed. Prepare test binaries separately, reuse supported Nextest binary metadata, and apply the existing owned-process timeout mechanism on Windows (`dev/run_owned_process.ps1`) or the execution environment's equivalent. Inspect current script/CLI signatures before invoking; do not invent a Nextest timeout flag or alter global test configuration. Allow a separately recorded initial compilation budget up to 15 minutes; investigate one concrete resource/compile issue if it fails.

Existing `dev/test_perf/run.ps1` illustrates target selection, binary metadata, and nonempty filter checks. Its combined build/run path needs separate execution timing to meet the above budget; do not import unrelated workflow selections or benchmark apparatus.

Useful selection boundaries are:

- Exact new comment/nesting tests plus directly affected document examples.
- Exact Results visibility/copy/physical-identity tests.
- Exact new editor geometry, preference, splitter, and result-navigation tests.
- Existing retained-pass, host-migration, undo/redo, and constrained-window tests only where affected.
- Narrow settings serialization/save ownership tests for the new fields.

Record actual commands, selected case counts, exit codes, and outcomes. Zero selected tests is not a pass. If a case appears flaky under grouped execution, rerun that case once alone and report both outcomes. Do not disable, weaken, or ignore meaningful tests.

### H2. Required behavior coverage

| Behavior | Evidence required |
| --- | --- |
| Full-line comments | `#`, `//`, indentation, inert assignment-like comment text, and ordinary division |
| Nesting | Balanced min/max examples, one nested variable/arithmetic case, malformed original remains error |
| Grid filtering | Mixed hidden lines plus value zero/error; physical labels and reference meaning preserved |
| Copy/identity | Source-containing copy sections unchanged; stable LineId and comparison/dependency targets retained |
| Gutter/wrap | Physical-versus-visual rows, Unicode, CRLF/trailing line, no-wrap overflow, wrap transition |
| Splitter | Both orientations, minimum bounds, breakpoint transition, drag persistence |
| Go to source | Value/error row context menu, offscreen target, hidden predecessors, correct character mapping |
| Lifecycle | Retained-pass publication, stale target/host rejection, one-shot focus, temporary highlight expiry |
| Preferences | Legacy defaults, round-trip, malformed ratios, field ownership and pending-save behavior |

Use existing test helpers and state/geometry assertions. Do not create exhaustive combinations or pixel-snapshot machinery. Prefer narrowly named tests with clear failure messages and one primary behavioral invariant.

For any test/profile that constructs the app, follow `AGENTS.md`: set Paste Sequence `next_hotkey = None` and `cancel_hotkey = None` before construction. Preserve production defaults and ordinary error reporting.

### H3. One bounded real-UI smoke

When the execution environment supports the application, build one appropriate runnable binary, launch it through the existing isolated test/profile mechanism, and perform these checks autonomously. Reuse the binary unless source changes require rebuilding. Do not require a release build merely for these controls.

Use this source, preserving the blank line:

~~~text
# Inputs
x = 2

// A comment with ignored = missing_name
y = min(100, min(50, 75))
z = r2 * 4
0
bad =
~~~

Expected visible rows: `r2 = 2`, `r5 = 50`, `r6 = 8`, `r7 = 0`, and the error at `r8`.

1. Toggle line numbers and verify physical labels/gaps. Add a long valid expression and an indented comment; check no-wrap horizontal scrolling and wrapping.
2. Drag the divider in wide and narrow layouts, including near each usable limit.
3. Right-click value text/unused row area and the error row; Go to source must reveal the correct line, focus once, and show a temporary highlight.
4. Check Copy, Select for comparison, and Inspect bits on an applicable existing integer example; check one undo/redo after an edit.
5. Pop out and dock the Pad; confirm display state and navigation still work. Click the main launcher query while the Pad is popped out to verify it remains interactable.
6. Close/reopen and perform one isolated-profile restart to confirm saved display preferences. Confirm ordinary saved-source behavior follows the existing recovery setting.

Capture a small number of useful screenshots or equivalent interaction evidence covering wide/narrow layout, scrolling/wrapping, and navigation. Evidence must reflect actual interaction, not just an unlaunched build. A screenshot alone does not establish highlight expiry; observe it or use a behavioral timer/state test.

If native interaction is unavailable, make one reasonable attempt and at most one evidence-driven corrective attempt. Record the exact environmental limitation, retain automated evidence, and label those interactions unverified. Do not substitute a historic native harness or ask the user to perform intermediate validation.

### H4. Final lint and review

Run one appropriate final first-party Clippy pass for affected targets, using `--no-deps` where appropriate. Address warnings introduced by this Goal. Record unrelated preexisting findings without cleaning vendored crates or unrelated subsystems. Apply formatting only to touched code as needed, avoiding repository-wide churn.

The configured reviewer independently examines requirements, source changes, tests, accepted-pass behavior, settings ownership, and scope. Resolve feature-related findings, then rerun only the checks affected by the correction. Stop optional testing once required evidence is sufficient.

## I. Risk Management and Bounded Validation

| Meaningful risk | Mitigation |
| --- | --- |
| Filtering changes `rN` or stable identity | Filter presentation only; physical-line/copy/navigation regression |
| Gutter drifts on wrap/CRLF/Unicode | Use actual galley geometry and established byte-to-character conversion |
| Go to source targets stale content or steals root focus | Stable IDs, current document/host checks, retained-pass publication, one-shot focus |
| Highlight expiry overwrites selection or adds idle work | Separate transient paint state, bounded expiry repaint, lifecycle invalidation |
| Splitter hides controls or triggers write storms | Real geometry budgets, min-size fallback, per-orientation ratios, persist completed drag |
| Preference save overwrites other settings/newer changes | Existing typed save ownership, pending coalescing, completion/draft reconciliation |
| Existing nesting works but gets rewritten unnecessarily | Characterization first; narrow fix only for demonstrated failure |

Use at most two focused correction-and-recheck rounds for the same persistent blocker, each based on new evidence. Continue independent safe work when possible. If exhausted, record what failed, exact commands/outcomes, attempted remedies, preserved state, remaining requirements, and smallest safe next step. Normal compiler feedback should be addressed within the current stage; unrelated historical failures are not new work.

Distinguish a feature defect from an unrelated baseline problem and an optional improvement. Never claim an environment limitation proves native acceptance. Push failures remain nonblocking.

For a genuine exhausted/blocked stop, follow the current `AGENTS.md` stalled-goal policy: a concise report under `docs/plans/execution/standalone/calculation-pad-editor-usability/stalls/<UTC-timestamp>.md` when safe, or the authoritative external ledger if a tracked write is unsafe. This report supplements the ledger; it is not a new implementation milestone. Preserve uncommitted partial work and accurately report its location/status.

## J. Definition of Done

The Goal is fully complete only when:

- C1–C8 are implemented, with the balanced-parentheses interpretation explicit.
- Comments/blank rows are hidden without changing source, physical `rN`, stable IDs, dependencies, or copy semantics.
- The gutter, horizontal scroll/wrap, both split orientations, and value/error Go to source work in the shared docked/popout path.
- Navigation is current and retained, highlights expire safely, and display actions preserve history/revision/source.
- Display preferences are backward-compatible and persist through the established save owner.
- Relevant tests pass with nonempty selection, required supported UI checks have evidence, and feature-related review/lint blockers are resolved.
- All intended changes are committed with the required cadence on the correct child branch; no user changes were lost/staged accidentally.
- `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs, `final_tip_sha`, actual push state, and checkout state are recorded.
- No worktree, helper checkout/clone, independent implementation branch, or intermediate integration operation was used.
- The next Goal is explicitly instructed to branch from this Goal's exact final tip.

Use **Complete** only when these conditions are met. Otherwise report **Limited / Environment blocked** for unavailable interaction evidence or **Blocked / Incomplete** for a remaining required feature defect, naming each unverified/unimplemented requirement. Do not present partial acceptance as full completion.

## K. Final Delivery Report

Provide a concise factual report containing:

1. Overall status and C1–C8 coverage, with implemented/automated/native states distinguished.
2. Completed checkpoints and important decisions, including defaults, physical-line preservation, existing-versus-changed nesting behavior, and preference ownership.
3. Relevant files changed.
4. Actual tests/build/lint/review results, case counts, failure/isolated-rerun outcomes, and evidence paths.
5. Native interaction evidence and exact limitations.
6. Git handoff: `parent_branch`, `parent_tip_sha`, current branch, `baseline_sha`, checkpoint SHAs, `final_tip_sha`, pushed/local-only commits, and clean/dirty state.
7. Explicit instruction: **The next Goal must create its new branch from this `final_tip_sha`, not master/main.** State that no worktrees, auxiliary clones/checkouts, or intermediate branch merges were used.
8. Remaining issues and a short optional final user-review checklist: toggle numbers/wrap, drag each divider, right-click a value/error → Go to source, verify comments disappear from Results.

The user performs their own final review after execution; no intermediate manual work is delegated to them. Stop at this Goal's completion/blocker boundary and do not begin another feature or integrate the final branch without the appropriate instruction.

