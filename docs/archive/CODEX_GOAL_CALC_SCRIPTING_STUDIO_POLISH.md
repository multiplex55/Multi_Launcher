# Codex Goal — Calc Scripting and Studio Polish

**Status:** Approved for autonomous implementation. The user selected **Q1: A, Q2: A — proceed**.

**Companion:** `CODEX_KICKSTART_CALC_SCRIPTING_STUDIO_POLISH.md`.

**Goal slug:** `calc-scripting-studio-polish`.

This is one Goal with one shared task branch. The approved work is a practical expansion of editable Mermaid flowcharts, the sixteen selected Calculation Pad capabilities below, and consistent QR Generator docking. It is not the earlier, broader feature-ideas list. Read `AGENTS.md` and applicable local instructions before execution. This Goal is the feature contract; repository instructions govern engineering and agent operation. Historical initiatives do not add acceptance gates to this Goal.

## A0. Mandatory Chained Git Branch Bootstrap — Before All Application Work

### M0-A — Exact parent tip → fresh child branch → baseline → initial push

This is the **first execution checkpoint**, before application edits, implementation builds/tests, or later checkpoints. The Goal and Kickstart describe **one shared branch**, not one branch each. Suggested name: `feat/calc-scripting-studio-polish`, with a unique suffix if necessary.

1. **Read, then inspect without modifying source.** Read `AGENTS.md`, relevant local instructions, this complete Goal, and the Kickstart. Inspect `git status -sb`, current branch, `HEAD`, remotes, upstream/default branch, index, untracked files, and any current queue/orchestrator/execution-ledger handoff. Inventory pre-existing user changes. Do not implement or build while deciding ancestry.
2. **Resolve the actual required parent.** For a continuing sequence, use the immediately previous completed Goal/category branch's **exact final commit**, even when the checkout currently says `master` or `main`. Record `parent_branch` and `parent_tip_sha`; confirm the predecessor's intended changes are committed. Fetch safely if the required ancestor exists only remotely. Never substitute a stale remote ancestor for an unpushed local predecessor. Conflicting or incomplete active handoff records are a bounded blocker, not permission to guess.
3. **Only a genuinely new chain may default to the up-to-date primary integration branch.** Safe fetch and inspection are permitted; automatic pull/merge/rebase is not. A safe current-HEAD initial base is permitted only for a genuinely new chain, with explicit documentation and no lost or inadvertently included user changes. The source-reference commit in section B is **not** an instruction to branch from that commit or from `master`.
4. **Use the single existing checkout. NO GIT WORKTREES.** Do not run `git worktree add`, `git worktree move`, or `git worktree prune`; do not create auxiliary checkouts, secondary clones, or agent-specific clones/implementation branches. No parallel branches from a common integration base. Agents may reason/review read-only in parallel when safe, but source writes, tests/builds on this computer, commits, and branch transitions are sequential.
5. **Protect existing work.** Never automatically reset, stash, hard-pull, rebase, merge, force-checkout, overwrite files, or stage unrelated changes to manufacture a clean start. Do not proceed through an unsafe index or unresolved merge. If reaching the required parent is unsafe, preserve the work and report the exact blocker instead of selecting a different parent.
6. **Create and switch to a fresh, unique child branch at the resolved SHA**, for example `git switch -c <unique-task-branch> <parent_tip_sha>`, after replacing placeholders with verified values. Confirm the new branch is active and initially points at that exact commit. An already-active task branch is not an exemption for a newly started Goal. A documented restart of this very same Goal resumes its recorded branch/checkpoint; it does not manufacture another baseline.
7. **Create this branch's own M0-A baseline commit.** Prefer a small task-specific execution-ledger entry. Otherwise use an intentional empty commit such as `chore(studio): [M0-A] establish calc scripting branch baseline`. Stage only this Goal's intended files. Record `baseline_sha` separately from `parent_tip_sha`. Verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`. The predecessor's last checkpoint is not this Goal's baseline.
8. **Attempt one ordinary initial upstream push**, normally `git push -u origin HEAD`, subject to available permissions. Never force-push, bypass permissions, or rewrite history. Record success or the precise failure. Rejection, unavailable credentials/network, or missing permitted remote does not block safe local implementation; do not enter retry loops. If permission prevents issuing the command, record that it could not be attempted rather than claiming a successful attempt.
9. **Keep all subsequent checkpoints on this branch.** This Goal does not authorize internal category branch splits. Commit each verified coherent increment, then attempt one normal best-effort push. Do not create branches per milestone, agent, Goal document, or Kickstart document.
10. **Record execution state.** Reuse the current authoritative queue ledger when applicable. Otherwise use a compact ledger at `docs/plans/execution/standalone/calc-scripting-studio-polish/INTEGRATION_STATUS.md`. Record parent branch/tip, active branch, baseline, checkpoints, verification, actual push state, and preserved unrelated changes. Follow the repository's stalled-Goal reporting policy if a genuine stop occurs; do not create competing operational ledgers.
11. **Handoff the final tip.** After the last commit, obtain the actual `final_tip_sha` and record it in the authoritative ledger/final report as the **required parent of the next Goal**. Record clean/dirty status and whether that commit is available remotely. Do not merge, rebase, squash, or cherry-pick intermediate branches merely to advance the chain. A future final integration is a separate approved boundary. Do not carry uncommitted partial work silently into the next category.

**M0-A acceptance:** Unique active branch; verified correct parent; baseline is its direct child; initial push outcome accurately recorded; no user work staged/lost; no worktree, helper checkout, or independent side branch created. Implementation is forbidden before these conditions hold, except that an unavailable/denied push remains nonblocking as described above.

**SHA-recording detail:** A tracked file cannot contain the SHA of the commit that includes that file without changing the SHA. Record commit identities after creation in the external ledger or final report, and update the tracked ledger at the next ordinary checkpoint. After any final reporting commit, recalculate the actual final tip. Never amend/rewrite history to chase a self-referential SHA.

## A. Goal / Desired Outcome

Make Calculation Pad a small, bounded, live-evaluated scripting tool without turning it into a Python IDE or replacing its existing arithmetic language. The user can change selected inputs in a compact form, reuse variable names in top-to-bottom calculations, run bounded multi-line loops, use conditional expressions and collection statistics, pin final named outputs, and edit source with useful assistance. Existing calculations, exact integers, units, vectors, source identity, saved documents, comparisons, and programmer inspection remain usable.

Make common Mermaid flowchart source work in the editable Diagram Studio path: ordinary comments, `graph` headers, semicolon-separated statements, chained arrows, and right-to-left/bottom-to-top directions. Comments must survive visual editing and file round trips. Preserve the already-existing source-rendered diagram families.

Give QR Generator the same single-session **Dock / Pop Out** interaction as other hosted utilities. Switching presentation must not reset the payload, error-correction selection, or generated QR result. The main launcher query remains independently usable.

Success is visible in the application, covered by focused automated evidence, and honestly qualified where native Windows interaction could not be verified. There is no requirement for the user to run intermediate checks.

## B. Current Codebase Findings

### B1. Source reference and execution authority

The relevant source was inspected against reference commit `242d4a3cecf47b4be4ed8645d5ddb9f9ad598ae0`. This identifies the planning reference only. At execution, the **correctly bootstrapped current checkout** is authoritative. Revalidate symbols and directly relevant callers after M0-A; adapt moved paths without changing the approved scope. Do not reset to the reference commit or infer task ancestry from it.

### B2. Verified ownership map

| Existing location / symbols | Relevant current behavior and integration consequence |
| --- | --- |
| `src/calculation_pad/document.rs`: `CalculationDocument::evaluate`, `CalculationValue`, `LineOutcome`, `CalculationLineResult` | Source-authoritative evaluator; currently prepares physical lines, gathers all assignment definitions, rejects duplicates, resolves references, checks cycles, and evaluates in source order. A duplicate-check deletion alone cannot implement reassignment correctly. |
| Same file: `find_assignment_delimiter`, `assignment_definitions`, `mark_duplicate_assignments`, `resolve_references`, `evaluate_comparison_expression` | Assignment detection currently finds an unbraced `=`; comparison tokens need proper lexical separation. Comparison expressions rebuild named definitions from rows and therefore also need version-aware/latest-binding behavior. |
| `src/calculation_pad/programmer.rs`, `integer.rs`, `operators.rs`, `quantity.rs`, `vector.rs` | Existing typed paths own exact fixed-width arithmetic, bit operations, scalar behavior, units, and bounded vectors. `and`, `or`, and `not` already have programmer meanings; contextual names are tested. Do not replace these paths with a floating-point-only script interpreter. |
| `src/calculation_pad/session.rs`: `CalculationPadSession`, `SessionEvaluation`, `replace_source`, `replace_line`, `evaluation`, `comparison_context`, `dependency_snapshot` | Owns document state, line IDs, incarnation/revision identity, Undo/Redo, cached evaluation, dirty transitions, and frozen comparison/dependency snapshots. New editing controls must use this owner rather than maintain another editable source. |
| `src/calculation_pad/dependencies.rs`, `comparison.rs`, `inspector.rs` | Existing consumers bind to physical source rows and current immutable results. Shadowing and repeated loop execution require explicit provenance rather than pretending each name has one declaration. Preserve stale-context rejection and exact-integer inspection. |
| `src/calculation_pad/persistence.rs`: `StoredDocument`, `PadDocumentStore`, `PadRecoveryWriter` | Version-1 named documents store source, stable line IDs, and allocation state. Recovery is separate, bounded, opt-in, and debounced. Existing records reject unknown fields and unsupported versions; migrations cannot be hand-waved as automatically backward compatible. |
| `src/calculation_pad/examples.rs`, `README.md` | Own executable reference examples and documented grammar/limits. Update alongside implemented syntax; do not document aspirational syntax as already supported. |
| `src/gui/calculation_pad_dialog.rs`: `editor_ui`, `results_ui`, `contents_ui`, `complete_pad_host` | The editor uses an egui multiline `TextEdit` and custom layout. Source/result panes, line numbers, wrapping, split ratios, result-to-source navigation, and retained-pass effect publication already exist. New controls must respect that retained-host/revision boundary. |
| `src/settings/model.rs`: `CalculationPadDisplayPreferences`; `src/gui/render.rs`, `src/gui/mod.rs` | Existing settings and application effect/save plumbing own Calc Pad display preferences. Add defaults/validation without overwriting unrelated settings. |
| `src/gui/note_find.rs`: `NoteFindState`, `PreparedNoteReplacement` | Existing bounded literal-search/replacement design uses UTF-8/character ranges and owner/query/content revisions. Reuse suitable pure logic or the established transaction approach; do not force Calc Pad to own a NotePanel or refactor Notes unnecessarily. |
| `src/diagram/mermaid.rs`: `parse_mermaid`, `parse_header`, `parse_statement`, `StatementParser`, `format_mermaid` | Editable flowcharts currently accept TD/TB/LR, one statement per physical line, and a limited arrow grammar. Ordinary `%%` lines and chained/trailing statements are rejected. Formatting regenerates canonical source. |
| `src/diagram/session.rs`: `DiagramSession::apply_transaction`, `SessionSnapshot`, `LastGood`, `semantically_equivalent` | Visual edits regenerate source, reparse it, verify equivalence, and maintain last-good state/history. Accepting comments in parsing without preserving them during regeneration would lose user content. |
| `src/diagram/model.rs`: `FlowDirection`; `layout.rs`, `scene.rs`, `src/gui/diagram_studio.rs` | Current direction variants are TopDown, TopBottom, LeftRight. New directions must reach actual layout, arrows, bounds, UI choices, and serialized state—not only the header parser. |
| `src/diagram/style.rs`, `mermaid_assistance.rs`, `conversion.rs`, `codec.rs`, `persistence.rs`, `export.rs` | Existing style directives, bounded completion, conversion, versioned sessions, and explicit export paths need narrow compatibility updates. Do not mistake every `%%` directive for an ordinary comment. |
| `src/diagram/rendering.rs`: `generic_preview_family`, `is_generic_preview_candidate`, `render_mermaid_preview_with_options`, `RendererService` | There is already a separate bounded source-rendered path for several non-flowchart families. Editable flowcharts remain on their graph-session path. Preserve both paths and their admission/error checks. |
| `src/gui/qr_dialog.rs`: `QrDialogState::open`, `close`, `show_impl`, `body`, `refresh_generation` | QR currently owns an immediate native viewport and embedded fallback. A fresh invocation resets state; generation is cached; clipboard/file actions are explicit. A presentation switch must not call the reset-style open path. |
| `src/gui/utility_window_host.rs`: `HostedUtilityDialog`, `UtilityWindowHost`; `utility_host.rs`: `UtilityHostState`, `HostIdentity`, `ViewportPassProvenance` | Existing reusable single-session docking/native-host and retained-pass machinery. QR should join this ownership model, not build an independent second host system. |
| `src/gui/utility_presentation.rs`, `src/settings/utility_presentations.rs`, `src/settings/utility_dialog_sizes.rs` | Existing remembered presentation and geometry ownership; `qr-generator` already has a utility-size key. Reuse that key rather than invent an incompatible duplicate setting. |
| `src/gui/command_host.rs`, `src/gui/mod.rs`, `src/gui/render.rs`, `src/gui/qa_framebuffer.rs` | Actual QR invocation, panel lifecycle, rendering, focus, and optional QA viewport observations. Update directly affected adapters if viewport ownership changes. |
| `AGENTS.md`, `.codex/config.toml`, `.codex/agents/{planner,implementer,reviewer}.toml`, `Cargo.toml` | Existing sequential-write agent roles and checkpoint/push rules. Tests are primarily colocated in these modules. The package is `multi_launcher`, with a `fast-dev` profile; do not invent integration-test target names. Preserve configured agent models. |

### B3. Existing behavior to retain

- Source bounds currently include 64 KiB/document, 256 physical lines, 2,048 bytes/expression line, depth 32, 512 identifier occurrences/document, and 128-byte names. Existing vectors contain 1–128 finite dimensionless components. Keep these legacy limits unless a narrowly justified new syntax accounting change is documented and tested; generated runtime values need additional limits, not relaxed input limits.
- `rN` means the one-based **physical source line**, counting comments/blanks. Folded screen rows must not become row-reference numbers.
- Scalar `^` is exponentiation; exact integer modes and bit operations are distinct; units retain their dimensional and approximation behavior. `mod(...)`, infix remainder, contextual names, Greek/braced names, and exact-base conversion remain compatible.
- A session reevaluates on source revision changes, not on every paint, selection, or display-format change. Closing/reopening, saved-document conflicts, recovery, and undo history already have ownership rules.
- Current shared numeric formatting is in `src/common/number_format.rs`. It is used outside Calc Pad; do not change its global defaults to implement Pad-only formatting.
- Diagram source can be invalid while a previous preview remains explicitly stale. Preserve this distinction; unsupported source must not be silently dropped to manufacture a valid editable graph.

## C. Approved Functional Requirements

The user selected **Q1 A**: comments plus the defined practical flowchart expansion, not arbitrary Mermaid compatibility. They selected **Q2 A**: bounded multi-line `for` blocks in the existing Calc Pad language, not collection-only repetition and not a Python runtime.

### C1. Requirement inventory and traceability

| ID | Required user-visible outcome | Primary checkpoints |
| --- | --- | --- |
| MMD-01 | Ordinary full-line `%%` comments are accepted and preserved through editing, Undo/Redo, save/reopen, and source export. | M1-A, M1-B |
| MMD-02 | `graph` is accepted as an alternative to `flowchart`. | M1-A |
| MMD-03 | Semicolon-separated flowchart statements work, including a header followed by statements. | M1-A |
| MMD-04 | Chained supported arrows such as `A --> B --> C` work. | M1-A |
| MMD-05 | RL and BT work throughout the editable flowchart workflow. | M1-B |
| CP-01 | Variables can be assigned again; subsequent expressions use the latest preceding binding. | M2-A, M2-C, M2-D |
| CP-02 | Optional, customizable editable-input panel edits selected source inputs without crowding the UI. | M4-A |
| CP-03 | Context-aware variable/function autocomplete, with an explicit off switch. | M3-B |
| CP-04 | Theme-aware syntax highlighting. | M3-B |
| CP-05 | Optional named-output watchlist follows final current bindings. | M4-A |
| CP-06 | Ctrl+F / Ctrl+H Find and Replace inside Pad source. | M3-A |
| CP-07 | Source/result selection highlights the corresponding location. | M3-A |
| CP-08 | Actual function signatures and active-parameter hints. | M3-B |
| CP-09 | Decimal places, separators, scientific notation, percentage presentation, and original Auto formatting. | M4-B |
| CP-10 | Matching delimiters, optional auto-closing, and indentation assistance. | M3-C |
| CP-11 | Source section/block folding without changing source or row identity. | M3-C |
| CP-12 | Ctrl+G physical-line navigation and clickable diagnostic navigation. | M3-A, M3-C |
| CP-13 | Conditional expressions, comparisons, and logical conditions. | M2-B |
| CP-14 | Collection sum, average, median, minimum, maximum, and standard deviation. | M2-B |
| CP-15 | Bounded multi-line `for` loops with assignments and running totals. | M2-C, M2-D |
| CP-16 | Bounded numeric range/sequence generation. | M2-B, M2-C |
| QR-01 | Consistent Dock / Pop Out, remembered presentation, state preservation, and independent launcher interaction. | M5-A |

### C2. Calc Pad language contract

The following are **implementation defaults selected to resolve routine details**, not claims that the current application already supports this syntax. Implement and document them consistently; do not reopen the settled product questionnaire.

#### Bindings, rows, and failures

1. Evaluation starts from the current document, with no hidden variables carried over from a previous revision. Assignment RHS reads the environment immediately **before** that assignment; a successful assignment publishes a new binding for subsequent execution.
2. `x = 2`, `before = x`, `x = x + 3`, `after = x` produces `before = 2`, final `x = 5`, and `after = 5`. Earlier row values are not retroactively recomputed using the final binding. `x = x + 1` without an earlier binding is still an error.
3. A failed assignment becomes a failed latest binding. Later references must not silently fall back to an earlier successful value. Independent later top-level lines can still evaluate when the failure is local and the global work budget remains available.
4. Each executed assignment has a binding identity/version distinct from its source name. Exact values and types flow through bindings without converting integers or units to `f64`. Preserve braced/Greek names and existing contextual identifier behavior.
5. Physical `rN` references still target earlier physical lines, not variables or folded display rows. They are not a hidden mechanism for forward references or self-reference across iterations. After a successful loop, a body row's public result is its last execution in that loop; rows never executed have an explicit not-executed state, not a recycled old result.
6. Comparison expressions use the final current binding environment of their immutable evaluation snapshot. Selecting a particular result still selects that physical row's value, not a later same-named assignment. Watches use the final binding by name. Dependencies record the binding actually read.

#### Conditional expressions and logical operators

Use lazy `if(condition, when_true, when_false)` with `<`, `<=`, `>`, `>=`, `==`, `!=`, and Boolean `&&`, `||`, `!`. `if` is a special expression, not an eager ordinary math call. Only the selected arm is evaluated; Boolean short circuiting similarly skips the unused operand. Reject structurally invalid syntax even in an unexecuted arm, but do not report arithmetic or unresolved runtime references in an arm that is not evaluated.

Comparisons produce a Boolean value. Conditions require Booleans; do not silently apply numeric truthiness. Boolean literals `true`/`false` may be contextual unbound constants, while existing user bindings of those names remain valid. New function/statement words are contextual: do not globally prohibit previously legal names such as `sum`, `range`, `for`, or `if` merely because new grammar uses them in call/header position. Preserve the existing rules for already-reserved names.

**Do not repurpose `and`, `or`, `not`, `&`, `|`, or `~`.** Their current programmer behavior and contextual-name tests remain authoritative. Use the distinct Boolean spellings above to avoid this collision. The help must make the distinction explicit.

Compare same-type scalars, compatible units, and exact integers using type-preserving rules consistent with the current comparison machinery. Do not silently compare a large integer through floating point or erase a unit. Unsupported mixed modes/types get a clear local diagnostic. A conditional returns the selected arm's existing value type; downstream operations still enforce their normal type checks.

#### Ranges and collection statistics

- Provide `range(stop)`, `range(start, stop)`, and `range(start, stop, step)`. Use a half-open stop and default step `1`. Arguments are finite, exactly representable integral scalar values. Zero step is an error; an incompatible step direction yields an empty sequence; negative steps work. Reject a sequence that exceeds its limit before allocating or iterating it.
- Use a bounded sequence value for generated sequences, including empty sequences. Keep existing nonempty vector semantics and vector arithmetic intact. Do not reinterpret `[]` as a previously valid vector or silently broaden vectors to arbitrary types.
- `sum(values)`, `average(values)`, `median(values)`, `min(values)`, `max(values)`, and `stdev(values)` accept existing scalar vectors and generated scalar sequences. Preserve existing two-argument `min(a, b)` and `max(a, b)` and unit contexts such as minutes.
- `stdev` is **population standard deviation**; document the convention. Empty `sum` is `0`; the other listed statistics on an empty collection are errors. Single-element population standard deviation is `0`. Use numerically stable bounded calculations and reject non-finite intermediates. Do not coerce typed integers or quantities into a scalar collection implicitly.
- Render long sequences with a clearly labeled bounded preview. Never silently truncate the value or let a truncated preview masquerade as a complete copied collection. Preserve existing parseable vector and exact-integer copy behavior.

#### Multi-line loops

Use indentation-delimited blocks:

```text
total = 0
for amount in [10, 20, 30]:
    total = total + amount
final_total = total
```

`final_total` is `60`. A loop iterable is evaluated once at entry, is an existing scalar vector or bounded generated sequence, and is visited in order. The body may contain several assignments/expressions and nested bounded `for` blocks. Dedentation ends a block. Use consistent space indentation; editor insertion defaults to four spaces. Reject ambiguous tab/space mixing in new block syntax, but do not reject legacy whitespace-indented expression-only documents. Blank lines/full-line comments do not count as statements or terminate a block. Empty/malformed blocks produce source-located diagnostics.

Loop-body assignments and the iterator participate in sequential bindings. After a successful nonempty loop, their final bindings remain available, including the iterator's final value. A zero-iteration loop does not create or overwrite body/iterator bindings. No `while`, recursion, function definitions, breakpoints, debugger, comprehensions, `break`, `continue`, or external side effects are required.

Treat a loop as a bounded publication unit: do not expose partial iteration values as a successful final answer. On a loop runtime error, discard its speculative published results, mark its attempted outputs as failed/noncommitted, and ensure downstream expressions cannot quietly use a stale pre-loop value for a name the failed block was meant to produce. Preserve unrelated previously valid bindings. Diagnostics identify the body source line and iteration context. On global budget exhaustion, mark the rest of that revision not executed; do not pretend later work ran.

Dependency analysis must distinguish source-line identity from execution/binding identity. A legal accumulator is not a cyclic-reference error simply because multiple runtime assignments map to one physical line. The Dependency Explorer may show a bounded aggregate or last-execution view with an explicit loop/iteration label; it must not claim a false precise single execution or traverse indefinitely. Full step debugging is out of scope.

#### Bounded work and responsiveness

Keep all current source-size/depth limits. Add explicit initial ceilings of **1,024 generated sequence elements**, **8 nested loop levels**, **10,000 total loop iterations per evaluation**, and **50,000 charged evaluation work units**. Count nested iterations cumulatively; charge body expressions and collection work, not just loop headers. These are safety ceilings, not performance promises; Codex may lower them with measured evidence and updated documentation, but must not silently remove or raise them.

Use checked size arithmetic, bounded buffers, and cancellation checks. Do not pre-expand loops into unbounded source strings. The same pure evaluator serves tests and the UI.

Preserve synchronous cached evaluation for ordinary small legacy documents. Execute repeated script work away from the egui paint/input path, with **one active evaluator and at most one coalesced latest pending revision**. New edits supersede old work; close/reopen retires the old incarnation. Publish results only for the current document/incarnation/revision. Show pending/error states honestly; never present a prior revision as current. Worker completion should request a repaint; no permanent polling/repaint loop or thread per keystroke. A new scripting worker must be a small Calc Pad owner, not an application-wide job-system redesign.

### C3. Editor, inputs, outputs, and persistence

**Compact presentation.** Keep Source and Results as the primary interface. Put optional controls in a compact View/Editor menu rather than an ever-growing permanent toolbar. Input and watch panels start hidden and can be shown/hidden independently. Pinning an output may reveal its panel. Completion and parameter hints start enabled, have discoverable off switches, and follow the theme. Smart auto-closing is individually switchable; default it off to preserve existing typing until enabled. Folding starts expanded. Existing line-number, wrapping, split-ratio, host, and help controls remain available.

**Editable inputs.** Let users choose top-level literal input assignments to display. Initial support includes scalar numeric literals, checked typed-integer literal constructors, and simple supported quantity literals. Use source identity plus name/line labeling, not a final name-to-number overlay: two assignments with the same name must remain distinguishable. Do not expose loop-body runtime instances as editable source inputs. Do not replace a formula with a constant or write to a different shadowed binding. Preserve units/constructors and validate edits through the existing typed rules. An unfinished/invalid field edit remains a local widget draft with a message; it does not corrupt accepted source. Accept on a clear commit action or valid focus loss, producing one source/history transaction per committed edit. Undo restores the original source. Deleted or changed-to-formula inputs become unavailable until explicitly reselected.

**Output watchlist.** Pin/unpin/reorder named results. A watch follows the final current binding of that name, including a final failed binding. Distinguish pending, missing, error, and valid values; never retain an old success without a stale label. Display types/units as appropriate and provide explicit copy behavior. Panels observe one accepted evaluation and do not reevaluate independently.

**Persistence.** Application-wide editor/display toggles extend `CalculationPadDisplayPreferences` with validated backward defaults. Persist selected input references and watch names as bounded **per-document view metadata** through the existing document/session store; global display toggles are not source edits. Because existing documents deny unknown fields, use an explicit readable legacy/new-format migration where schema changes require it. Read old version-1 records; do not rewrite them merely by opening/browsing. Save upgraded metadata only on an explicit save, preserve unsupported future data by refusing unsupported writes, and keep existing conflict/atomic-write behavior. Pin/input-selection changes may mark view metadata dirty, but must not rewrite calculation source. Save As copies the chosen configuration to the new document; new blank documents start empty. Cursor, popup, current search match, and fold expansion are transient, not restored stale runtime state. Recovery remains source-only unless an existing validated schema extension is essential; do not start saving source when recovery is disabled.

**Find/Replace and navigation.** Ctrl+F opens literal search, Ctrl+H opens replacement; support next/previous, count, case sensitivity, Replace Current, and Replace All. Replace All is one undoable, size-checked transaction against the exact current source revision. Empty queries do not match everywhere. Preserve UTF-8, CRLF, final newline, and text outside replaced ranges. Ctrl+G accepts a one-based physical line and rejects out-of-range input without editing. Clicking a diagnostic reveals its source. Find/navigation automatically unfold a target. Shortcuts apply only to the focused Pad host, not the launcher query or another editor.

**Synchronized selection.** Source caret/selection highlights the corresponding Results row; result selection highlights the source line. Preserve existing explicit Go to source. Avoid cyclic focus/scroll requests: scroll only on a real navigation/selection change, not every frame or reevaluation. Selecting a result must not repeatedly steal keyboard focus from an input/search field. Same-frame edits, retired hosts, or stale result identities invalidate stale navigation.

**Autocomplete and hints.** Suggestions use identifiers available at the caret's execution scope, actual built-ins, and actual parameter signatures. Collapse repeated names to the nearest visible binding; do not suggest a forward-only definition as currently available. Handle loop iterators, braced names, comments, and partial syntax. Do not evaluate the entire script just to paint a popup. For existing functions such as `round` and `extract`, reflect the implementation's actual accepted parameters rather than assumptions based on another language. Accept/dismiss explicitly; turning assistance off removes its key interception. Escape dismisses the active local popup before triggering an outer close action.

**Highlighting and smart editing.** Highlight supported numbers, constants, names, calls, operators, comments, and new loop/conditional grammar with theme-derived colors. Highlighting must not mutate source. Delimiter matching respects comments/braced names and incomplete input. Optional insertion wraps selections or inserts matching parentheses/brackets without duplicate closing characters. Indentation assistance applies to loop blocks and preserves pasted text; it must not turn Enter into an unexpected run command.

**Folding.** Fold loop blocks and explicit comment regions (`# region <label>` / `# endregion`), which remain ordinary full-line comments for evaluation. This provides an unambiguous way to collapse inputs/calculation sections without inventing a new executable grammar. Maintain a source-to-visible-position map for physical rows, byte offsets, character offsets, wrapping, and hidden ranges. Folding never removes source, substitutes placeholder text into saved source, renumbers `rN`, or changes evaluation. Expand ranges before edits whose selection crosses a hidden boundary; Ctrl+A and full-source copy must include the real document, not only visible text. Caret navigation, diagnostics, Inspector insertion, and Undo/Redo must work with folded content. Unmatched region markers are harmless editor state, not calculation failures.

**Number formatting.** Preserve current Auto output by default. Add bounded decimal places (0–15), grouping, scientific notation, and percentage display for dimensionless scalars. Formatting affects presentation only: no changed stored values, reevaluation, or formula rewriting. Units remain attached; exact integer/base output must not pass through floating point. Disable inapplicable percentage/precision choices for incompatible types rather than corrupt values. Preserve existing Copy result/Copy all semantics where promised as parseable; offer an explicitly labeled formatted-copy choice for grouped/percent text. Display preferences must not change canonical Inspector copying or other utilities' shared formatting.

### C4. Mermaid compatibility contract

Support only the additional editable-flowchart constructs approved here:

- Ordinary full-line `%%` comments, including before the header and between statements. Preserve comment text and ordering through source/visual edits, save/reopen, Undo/Redo, and Mermaid source export. Whitespace-only/comment-only documents may behave as empty diagrams. `%%{...}%%` is a directive, not an ordinary comment: retain currently supported style behavior and explicit rejection of unsupported directives.
- `graph` as a header synonym, with TD/TB/LR/RL/BT. Canonical formatting may use `flowchart`; it must not change graph meaning.
- Semicolon separators outside quoted labels, label delimiters, directives, and encoded entities. Support trailing separators without inventing nodes. Preserve original diagnostic byte/line/column locations. Do not implement this with a blind `split(';')`.
- Chains of the currently supported directed arrow, including existing per-edge label forms where they already apply, for example `A -->|yes| B --> C`. Expand into ordinary bounded nodes/edges, preserving declaration rules and stable edge allocation. This does not authorize every Mermaid link operator, `&` shorthand, or additional node shape.
- RL/BT in actual auto-layout, rendering/scene geometry, export, direction controls, and codec round trips. Existing manually positioned nodes should not jump merely because an unrelated source edit occurs; follow current explicit auto-layout behavior.

Preserve comments near their surviving statement/group where possible. Canonical rewriting may change graph whitespace/layout, but must not drop comments. If their target is deleted, retain them deterministically as standalone comments rather than deleting or attaching them to an arbitrary new entity. Do not introduce executable metadata into comments. Prefer reconstructing comment/trivia ownership from the authoritative source rather than duplicating a separately mutable comment store.

Keep source/rendered versus editable capability explicit. No new Mermaid families, nested editable subgraphs, broad CSS/class support, arbitrary directives, external resource loading, or claim of full Mermaid conformance. **Do not remove or disable non-flowchart preview families already supported by `src/diagram/rendering.rs`.** Parser/renderer failures still preserve the exact draft and honestly mark the last-good preview stale.

External syntax cross-check: Mermaid's official flowchart reference, `https://mermaid.js.org/syntax/flowchart`, describes the header synonym, direction tokens, comments, and chaining. The bounded support above is this application's approved subset, not a promise to implement the entire reference.

### C5. QR host contract

Use one `QrDialogState` session across docked/native presentations, through the established utility host. A new session defaults to the normal docked utility presentation when no saved choice exists; an accepted saved presentation takes precedence on later fresh opens. Dock/Pop Out is a presentation transition, not a new invocation.

Preserve source bytes, error-correction level, advanced-section state, current validation/feedback, and generated raster across switches. Recreate GPU texture resources only as necessary for valid ownership; do not regenerate unchanged payloads or run clipboard/file actions simply because a frame or host is replayed. A genuine new QR command continues to use the existing fresh-invocation reset semantics; `ensure_open`/focus requests must not behave like that reset.

Preserve explicit Paste, Copy Text, Copy QR, and Save PNG behavior, including failure/cancellation behavior and export fidelity. Stage external effects until the owning host pass/session is accepted, exactly once. Native OS close, Escape, Ctrl+W, app close, re-invocation, and remembered geometry must retire the correct host without a ghost viewport or second live copy. The main launcher query and unrelated dialogs remain interactable while QR is popped out. No clipboard monitoring or persistence of QR payloads is added.

## D. Scope and Non-Goals

Included: all requirements in C1, their necessary typed-domain/editor/host integrations, directly relevant tests, examples/help, bounded compatibility migration, and final evidence/reporting.

Excluded: workspaces; an interactive REPL; Python embedding; package imports; filesystem/network/shell access from scripts; general user-defined functions; plotting; data-table/CSV/JSON import; a full IDE/debugger; new utility plugins; a replacement GUI framework; application-wide editor/command-bus/job-system redesign; unrelated cleanup; broad release/performance optimization; new Mermaid families; and a QR scanner. These appeared in general brainstorming or are adjacent possibilities, not this approved request.

Do not weaken resource limits, remove valid tests, or alter unrelated public APIs merely to finish. Narrow extraction of a pure helper is acceptable when it prevents real duplication; wholesale migration of Notes, Markdown Studio, or the application host system is not. Existing `eframe`/egui, arithmetic, rendering, serialization, and host dependencies are the starting point. A new dependency requires a concrete unmet need documented in the ledger; it is not the default solution.

## E. Technical Implementation Strategy

1. **Keep domain semantics out of UI.** Add bounded parsing/binding/execution support within `src/calculation_pad/`; suggested new module names are design proposals, not claims that files exist. Reuse scalar/programmer/quantity/vector evaluation. Parse statements once per revision and evaluate references against versioned execution bindings; do not textually substitute user values or use an unbounded general scripting runtime.
2. **Separate static source identity from dynamic execution.** Source retains original physical lines and stable `LineId`s. Runtime binding/execution records can be bounded internal data. Derive current row results, final names, and truthful dependency provenance from one evaluation; do not maintain independent mutable maps in the watchlist, Inspector, and console-like UI.
3. **Centralize language metadata.** Expose actual token/function/signature information for help, autocomplete, hints, and syntax categories. Preserve existing contextual operator routing and function aliases. Do not create a parallel undocumented parser in the UI.
4. **Use one accepted edit transaction.** Editor typing, replacement, completion, smart edits, input changes, and Inspector insertion all feed the existing session/history and retained-host publication boundary. Tag pending UI operations with document/incarnation/revision and relevant selection/host identity. Competing same-frame intents must be deterministically ordered or rejected, never overwrite a newer source.
5. **Introduce a focused editor view adapter.** Own source/visible mapping, folds, caret, highlighting, and search decorations without duplicating the authoritative source or undo history. Do not save rendered placeholders. Keep changes local to Calc Pad; do not patch vendored egui or introduce a new editor framework without a demonstrated unavoidable issue and bounded alternative analysis.
6. **Keep display and persistence distinct.** Formatting and panel visibility do not trigger evaluator work. Source edits do. Per-document pins/input configuration are bounded metadata, not runtime values; derive current values after open. Preserve stale-save protections and recovery opt-in.
7. **Extend Mermaid at both syntax and publication boundaries.** Tokenize separators with source spans, retain trivia, reuse graph validation/semantic equivalence, and regenerate supported syntax with comments intact. Add direction semantics to model/layout/scene/UI consumers. Preserve strict renderer admission and existing non-flowchart paths.
8. **Migrate QR into the established host.** Add a narrow `HostedUtilityDialog` adapter (or the exact current equivalent after inspection), move clipboard/file effects into accepted commands, and wire panel lifecycle/focus/QA adapters. Reuse `qr-generator` identity, presentation persistence, and utility sizing.
9. **Follow actual agents and checkpoint rules.** Planner checks each packet, configured implementer writes it, reviewer independently reviews substantive changes, orchestrator owns sequencing and Git. Preserve configured models and permissions. No agent-specific branches, checkouts, or competing writers.

Dependencies: Mermaid syntax precedes its comment-safe publication/direction completion; Calc binding semantics precede conditionals/collections and loops; loop execution precedes current-snapshot integrations; editor mapping/navigation precedes folding; panels depend on stable binding identity; final native review covers all three areas. QR is independent of Calc semantics but stays on the same branch and follows the listed execution order.

## F. Detailed Milestones and Tasks

M0-A is specified completely in section A0 and is mandatory first. The remaining checkpoints below stay on that same branch. Requirements in section C apply at every checkpoint; a checkpoint may establish prerequisites without exposing incomplete controls as finished features.

### M0-B — Confirm the affected baseline and executable handoffs

- **Objective:** Establish the actual current ownership, supported baseline behavior, and narrowly scoped verification commands after branch bootstrap.
- **Code ownership:** The files in B2, `AGENTS.md`, agent configuration, relevant colocated tests, and the execution ledger only. No broad source search campaign.
- **Tasks:** Confirm reference assumptions against the required-parent checkout; enumerate the four sensitive boundaries—binding resolution, retained editor effects, Mermaid source regeneration, QR host lifecycle. Discover real test names with filtered Nextest listing. Record existing relevant failures and environment capabilities. Finalize per-checkpoint implementer packets using this Goal, not another large plan.
- **Behavior:** No feature changes at this checkpoint. Establish representative legacy Pad inputs and existing Diagram/QR workflows as fixtures/evidence.
- **Edge cases:** Current source may already contain part of a requested capability; retain and test it instead of duplicating it. Missing native desktop access is a verification limitation, not proof of failure.
- **Regression constraints:** No unrelated baseline repairs, historical acceptance matrices, dependency/model changes, or full suite.
- **Verification:** Read-only diff/source review; discover actual selected tests and run only the useful affected baseline tests. Capture commands and outcomes. Do not run builds merely to fill this checkpoint.
- **Acceptance:** Ownership and selected test filters are concrete; known relevant baseline issues are separated from new work; all requirements remain assigned to checkpoints.
- **Commit boundary:** `docs(studio): [M0-B] record affected baseline and verification scope`; only task-ledger/fixture documentation actually needed. Continue immediately.

### M1-A — Editable Mermaid syntax expansion

- **Objective:** Accept the approved flowchart grammar without silently weakening the parser.
- **Code ownership:** `src/diagram/mermaid.rs`, `mermaid_assistance.rs`, `style.rs` where required, and colocated parser/assistance tests.
- **Tasks:** Add source-spanned scanning for comments and semicolon-separated statements; accept `graph`; support chains of existing arrows/labels; separate directives from comments; preserve exact original diagnostics. Enforce graph/source limits after chain expansion and before publishing a graph. Expose trivia needed by M1-B. Update assistance header/direction/edge contexts to recognize new input without parsing comments as identifiers.
- **Behavior:** The combined fixture in H2 parses into the expected bounded graph. Existing one-line declarations, quoted labels, styles, and flat subgraphs continue to work. RL/BT tokens may be introduced with their model variants here, but completion is not claimed until M1-B covers layout/publication.
- **Edge cases:** Comments before a header; blank/comment-only input; semicolons in quoted labels and entities; CRLF/Unicode; duplicate declarations; chain labels; dangling arrows; unknown directives; empty trailing separators; graph limits reached mid-chain.
- **Regression constraints:** No new diagram families/link operators/shapes, lossy preprocessing, ignored unsupported source, or changed last-good semantics.
- **Verification:** Focused parser and assistance tests, including parse/equivalence checks and precise diagnostics. Do not test only a stripped temporary string.
- **Acceptance:** MMD-02/03/04 pass; MMD-01 parsing passes; unsupported syntax still receives a meaningful diagnostic; original source remains available.
- **Commit boundary:** `feat(diagram): [M1-A] accept bounded practical flowchart syntax`.

### M1-B — Preserve comments through edits and complete direction support

- **Objective:** Make new syntax usable through the entire editable Diagram Studio workflow.
- **Code ownership:** `src/diagram/session.rs`, `model.rs`, `layout.rs`, `scene.rs`, `codec.rs`, `persistence.rs`, `export.rs`, relevant `rendering.rs`/`conversion.rs` paths, and `src/gui/diagram_studio.rs`.
- **Tasks:** Carry/reconstruct comment trivia during canonical visual transactions; preserve orphan comments deterministically; include generated comments in output size checks. Complete RL/BT geometry, arrows, UI choices, serialization, source/image export, and assistance. Preserve graph equivalence checks and last-good/current-state ownership. Cover current source-rendered family routing without expanding it.
- **Behavior:** Add/rename/delete/move or relayout graph content without losing comments; Undo/Redo and save/reopen restore the correct text and graph. RL runs right-to-left and BT bottom-to-top, not merely different header text.
- **Edge cases:** Comment target deletion; grouped graphs; supported init styles preceded by comments; empty graphs; manually positioned nodes; cycles/self-edges; generated output near limits; legacy session history; failed disk write/conflict; unsupported input after a valid preview.
- **Regression constraints:** Preserve existing TD/TB/LR layouts, existing file-format readers, strict render admission, draw.io/visual conversion restrictions, and non-flowchart previews. No automatic source overwrite on parse failure.
- **Verification:** Parser→session→visual edit→codec round trips, direction geometry assertions, export checks, and focused GUI interactions. Include representative non-flowchart preview regression tests only where a shared path changed.
- **Acceptance:** All MMD requirements pass at domain/session level; comments remain after actual visual edits; native evidence is recorded now or scheduled in M6-A without falsely marking it verified.
- **Commit boundary:** `feat(diagram): [M1-B] preserve comments and honor reverse flow directions`.

### M2-A — Sequential shadowing and binding-aware consumers

- **Objective:** Implement reassignment as sequential semantics, not global last-name substitution.
- **Code ownership:** `src/calculation_pad/document.rs`, `session.rs`, `comparison.rs`, `dependencies.rs`, associated result consumers and tests.
- **Tasks:** Replace global duplicate-name rejection/resolution with binding-aware lookup. Evaluate each RHS against earlier bindings; preserve physical row outcomes and references. Represent failed latest bindings explicitly. Update comparison-context capture and expression evaluation to carry the final current environment. Retain exact types and stale-context checks. Migrate only tests whose duplicate-name expectations intentionally change.
- **Behavior:** Reassignment and accumulators read the preceding value; earlier calculations retain their own results. Watches/panels can later consume a coherent final-name snapshot. An expression comparing `x` uses final `x`, while selecting the first `x` result uses that row.
- **Edge cases:** Self-read without a predecessor; failed reassignment; type changes between assignments; exact integers above 2^53; quantities; braced names; contextual aliases; forward references; edited/deleted source rows.
- **Regression constraints:** Preserve current arithmetic routes, reserved-name behavior, physical `rN`, line-local unrelated-error recovery, and source/history/cache ownership.
- **Verification:** Focused document/session/dependency/comparison tests; explicitly retain non-duplicate cases from `diagnoses_forward_self_unknown_duplicate_reserved_and_failed_references` and contextual-operator tests.
- **Acceptance:** CP-01 sequential behavior passes; no removal of diagnostics unrelated to shadowing; dependencies point to the version actually used.
- **Commit boundary:** `feat(calc-pad): [M2-A] resolve sequential variable bindings`.

### M2-B — Conditions, ranges, and scalar collection functions

- **Objective:** Add the bounded expression capabilities needed by small scripts.
- **Code ownership:** Calc Pad domain modules, `document.rs`, `vector.rs`, operator/quantity integration where necessary, function metadata, examples and tests.
- **Tasks:** Make assignment/comparison tokenization unambiguous. Add Boolean values, lazy `if`, comparisons and Boolean operators per C2. Add bounded generated sequences and `range`; add the six required collection statistics while retaining binary min/max. Publish actual signatures for later assistance. Apply checked allocation/work accounting and stable standard-deviation calculation.
- **Behavior:** `if(0 == 0, 7, 1 / 0)` returns `7`; a non-selected erroneous runtime arm does not poison the chosen result. Ranges and statistics operate on their documented scalar collection types.
- **Edge cases:** Empty sequences, zero/negative step, invalid/inexact bounds, limit+1, non-finite operands, wrong argument counts, type incompatibility, existing names equal to new keywords/functions, parentheses/commas inside nested calls, units containing contextual words.
- **Regression constraints:** No changed integer bitwise meaning or precision, no implicit quantity stripping, no changed vector emptiness/size rules, no eager conditional evaluation, no accidental `==` assignment parsing.
- **Verification:** Deterministic pure tests for all new operators/functions and boundary conditions; existing scalar, programmer, vector, quantity, and comparison tests selected by the paths actually touched.
- **Acceptance:** CP-13/14/16 pass; actual API/signatures and error rules are reflected in executable examples; legacy cases remain meaningful and passing.
- **Commit boundary:** `feat(calc-pad): [M2-B] add lazy conditions and bounded collections`.

### M2-C — Multi-line bounded loop execution

- **Objective:** Execute the approved multi-statement `for` language with explicit source/runtime identity.
- **Code ownership:** Calc Pad statement parsing/execution, diagnostics, result/provenance types, domain tests, and examples.
- **Tasks:** Parse indentation blocks with source spans; evaluate iterable once; implement ordered iterator/body bindings and nested iteration under all C2 limits. Track bounded execution/binding provenance. Publish final per-row results and final bindings only according to successful/failed-block semantics. Preserve physical references and mark nonexecuted rows. Keep the evaluator pure and cancellation-aware.
- **Behavior:** The approved running-total example returns `60`; body calculations can depend on earlier body assignments and update outside variables. Zero iterations leave prior bindings unchanged. Runtime errors do not publish a partial total as success.
- **Edge cases:** Nested loops, iterator-name reuse, zero iterations, empty/comment-only blocks, malformed indentation, tabs, CRLF, earlier physical references inside a body, illegal forward/self row references, error on a later iteration, every budget boundary, canceled work.
- **Regression constraints:** No textual source expansion, unbounded recursion/allocation, environment leakage between revisions, or source-line-cycle rejection of a valid accumulator. Legacy standalone line errors retain existing independence where execution remains possible.
- **Verification:** Pure finite fixtures with exact expected outcomes; adversarial bounded-input tests; deterministic budget/cancellation tests rather than sleep-based timing assertions.
- **Acceptance:** CP-15 domain behavior passes, every loop is bounded, failed/nonexecuted results are explicit, and dependency provenance has no false source-level cycle diagnosis.
- **Commit boundary:** `feat(calc-pad): [M2-C] execute bounded multi-line for blocks`.

### M2-D — Responsive current-revision execution and consumer integration

- **Objective:** Keep the UI responsive and publish one coherent accepted script result to all consumers.
- **Code ownership:** `src/calculation_pad/session.rs`, a narrowly scoped Calc Pad execution owner if needed, `comparison.rs`, `dependencies.rs`, `src/gui/calculation_pad_dialog.rs`, Inspector/dependency dialog adapters.
- **Tasks:** Integrate repeated-work execution off the paint thread with one worker/latest-pending revision, cancellation, bounded results, and event-driven repaint. Preserve the legacy small-document cache path through the same evaluator. Feed physical results, final names, dependency snapshots and comparisons from the accepted result. Display current pending/failed states and bounded iteration labels. Retire work on document/host lifecycle changes as appropriate; host switching alone must not create a second session.
- **Behavior:** Rapid source edits cannot install older results. Copy/inspect/compare actions either use a current accepted snapshot or explain that results are pending. Closing or reopening the same document invalidates old worker completions.
- **Edge cases:** Same-frame edit+copy/save; close while running; rapid Undo/Redo; document incarnation changes; worker panic/error; hidden launcher; popout transition; canceled stale completion; budget exhaustion.
- **Regression constraints:** Do not add perpetual polling, a thread per keystroke, independent UI evaluation engines, or stale-source acceptance. Preserve existing exact Inspector insertion receipts and retained-pass rules.
- **Verification:** Controllable worker/completion tests; current-revision publication, cancellation, cache-count, dependency and comparison tests; one targeted responsiveness observation, not a benchmark campaign.
- **Acceptance:** CP-01/15 integrate across all consumers; no stale valid result or unbounded background work; normal legacy documents do not regress into repeated evaluation.
- **Commit boundary:** `feat(calc-pad): [M2-D] publish bounded current-revision script results`.

### M3-A — Editor navigation, synchronized selection, and Find/Replace

- **Objective:** Establish a reliable source-position/edit-transaction foundation for the requested editor features.
- **Code ownership:** `src/gui/calculation_pad_dialog.rs`, focused Calc Pad editor helpers, session transaction APIs; inspect `src/gui/note_find.rs` for reusable pure logic/transaction design.
- **Tasks:** Centralize mapping of physical lines, UTF-8 bytes, character offsets and wrapped rows. Add Ctrl+F/H/G and inline local controls, literal search/replacement, clickable diagnostics, and bidirectional selection highlighting. Apply edits through accepted session transactions. Ensure real navigation changes drive scrolling, not recurring frame events. Prepare the mapping interface for folded projections in M3-C.
- **Behavior:** Search/replace works on unsaved source; Replace All is one Undo action. Source/results remain synchronized without a focus or scroll feedback loop. Navigation targets remain physical source lines.
- **Edge cases:** Empty/no-match queries, replacement containing the query, Unicode/braced names, CRLF, final newline, line bounds, word wrap, same-frame source change, inactive host, pending save/open dialogs, keyboard shortcut conflicts.
- **Regression constraints:** Existing Go to source, Inspector caret receipt, Undo/Redo, recovery, native host migration, and launcher query focus remain valid. No second undo stack or duplicate source model.
- **Verification:** Mapping and replacement unit tests plus focused synthetic egui tests for focus, navigation, stale passes and one-step history. Replace All tests assert unchanged text outside matches.
- **Acceptance:** CP-06/07/12 work before folding; controls remain usable at existing constrained sizes and across dock/popout.
- **Commit boundary:** `feat(calc-pad): [M3-A] add source search and synchronized navigation`.

### M3-B — Theme-aware highlighting, completion, and call tips

- **Objective:** Add accurate, optional language assistance without changing evaluation or source unexpectedly.
- **Code ownership:** Calc Pad language metadata and editor helper layer, `src/gui/calculation_pad_dialog.rs`, `src/settings/model.rs` and effect persistence wiring as necessary.
- **Tasks:** Produce theme-aware token spans for actual syntax; use shared function signatures and lexical/runtime scope for bounded suggestions. Implement autocomplete acceptance/dismissal and active-argument tips. Expose completion/hint toggles and preserve them through presentation changes. Keep partial invalid text editable and highlight conservatively.
- **Behavior:** `tot` suggests visible `total`; `sq` suggests actual `sqrt(...)`; hints for `round`/`extract` reflect their implemented arity. Disabled completion does not intercept navigation/Tab/Enter keys.
- **Edge cases:** Earlier versus later shadowed declarations, iterator scope, comments, braced Unicode names, incomplete calls, nested calls, empty suggestion lists, theme switching, selection replacement, popup at viewport edge.
- **Regression constraints:** No source mutation during painting, silent function renaming, hidden evaluation for suggestions, changed bitwise syntax, or global shortcut capture.
- **Verification:** Pure span/scope/signature tests and targeted egui keyboard/toggle/theme behavior. Verify assistance does not increase evaluation count on unchanged source.
- **Acceptance:** CP-03/04/08 pass; assistance can be disabled completely; theme colors remain readable without a hardcoded dark-only editor theme.
- **Commit boundary:** `feat(calc-pad): [M3-B] add optional syntax-aware editor assistance`.

### M3-C — Smart editing and non-destructive folding

- **Objective:** Complete the lightweight script-editor experience while keeping source identity authoritative.
- **Code ownership:** Calc Pad editor projection/mapping and transaction helpers, dialog controls, directly relevant session/UI tests.
- **Tasks:** Add delimiter matching, optional insertion/selection wrapping, closing-character skipping and loop indentation assistance. Implement loop/explicit-region folding through the mapping contract. Expand hidden navigation/edit targets. Reconcile fold regions after edits, undo, document transitions and stale host passes; keep expansion transient but stable across presentation switches.
- **Behavior:** Folded sections do not participate in the editable buffer as replacement text. Ctrl+G, search results, diagnostics, synchronized results, and Inspector insertion reveal the real source. Ctrl+A/full-source copy cover the complete document.
- **Edge cases:** Nested loops/regions, unmatched region markers, multi-line selections crossing folds, paste into a folded boundary, tabs in pasted content, CRLF/Unicode, word wrap, body edit changing indentation, deleting a fold header, undoing that deletion.
- **Regression constraints:** No source truncation, changed `rN`, multiple conflicting TextEdit histories, hidden selection loss, focus stealing, or evaluator work on fold toggles. Do not require a new application-wide editor framework.
- **Verification:** Mapping round trips and edit/copy/undo invariants with folds; targeted synthetic/native interactions for wrapped and nonwrapped layouts.
- **Acceptance:** CP-10/11 pass; CP-06/07/12 and Inspector insertion remain correct when targets are folded; disabling smart editing restores ordinary typing behavior.
- **Commit boundary:** `feat(calc-pad): [M3-C] add safe folding and optional smart editing`.

### M4-A — Editable inputs and named output watches

- **Objective:** Make scripts reusable without requiring repeated source edits or scanning long result lists.
- **Code ownership:** Calc Pad binding/session APIs, focused input/watch state, `src/gui/calculation_pad_dialog.rs`, and domain/UI tests.
- **Tasks:** Identify eligible literal source inputs by stable assignment identity; expose selection/visibility and safe validated edits. Add named output pin/unpin/order and current final-binding lookup. Keep both panels optional, compact, independently hideable, and usable in stacked/small layouts. Share current accepted results rather than evaluating in each panel.
- **Behavior:** Editing an input changes the intended assignment and recalculates dependents; one Undo restores it. A watch updates to the final successful/error/missing state of its name. Choosing panels never replaces Source/Results.
- **Edge cases:** Duplicate input names, typed/quantity literals, formula replacing a literal, renamed/deleted inputs, invalid incomplete numeric text, pending script evaluation, changing source while a field is active, braced names, final failed reassignment.
- **Regression constraints:** No formula-to-constant conversion, hidden parameter overrides, writing to the wrong shadowed definition, stale-result fallback, or panel-driven reevaluation.
- **Verification:** Source-identity/transaction tests and compact-layout tests; input+source Undo/Redo; watch latest-binding/error cases; same-frame stale action rejection.
- **Acceptance:** CP-02/05 work end-to-end in memory; all UI remains navigable with panels hidden or shown; document persistence follows in M4-C.
- **Commit boundary:** `feat(calc-pad): [M4-A] add optional script inputs and output watches`.

### M4-B — Pad-local formatting and explicit copy presentation

- **Objective:** Improve readability without changing calculation precision or external tools' output.
- **Code ownership:** Pad-local formatting helpers and GUI/result/watch rendering; inspect but do not broadly change `src/common/number_format.rs`; settings/effect plumbing.
- **Tasks:** Add Auto, fixed decimal, scientific, grouping, and compatible percentage display; validate options and handle signed zero/finite extremes. Keep typed integer/base formatting exact. Apply the same current display policy to Results and watches. Preserve canonical copy behavior and label formatted-copy actions clearly.
- **Behavior:** Changing display options changes only presentation. `1 / 3` remains the same underlying result when decimals change; exact integers above 2^53 remain exact. Percent display does not scale a stored scalar or a unit-bearing quantity.
- **Edge cases:** Zero/tiny/large numbers, incompatible types, vectors/sequences, unit annotations, grouped negative integers, decimal limits, clipboard error, stale result/copy request, exponent output.
- **Regression constraints:** No global formatter behavior change; no source/dirty-history mutation from formatting; no loss of canonical vector/Inspector copy guarantees; no reevaluation on a format toggle.
- **Verification:** Pure formatting matrices and exact/canonical copy tests; GUI tests for format switch/cache count and explicit formatted-copy labels.
- **Acceptance:** CP-09 passes; current default output remains unchanged until the user selects another format.
- **Commit boundary:** `feat(calc-pad): [M4-B] add precision-safe result display controls`.

### M4-C — Persist new preferences and per-document view metadata

- **Objective:** Retain useful configuration without corrupting legacy documents, source recovery, or unrelated settings.
- **Code ownership:** `src/calculation_pad/persistence.rs`, `session.rs`, `src/settings/model.rs`, existing settings-save effect ownership, and current-document transitions.
- **Tasks:** Persist validated editor/display preferences with legacy defaults. Add bounded input/watch metadata to named documents through an explicit compatible reader/new-writer version when necessary. Retain source-only recovery behavior. Support Save As, metadata dirty handling, conflict detection, and unsupported-version refusal. Refresh views from source identities on open rather than storing evaluated values.
- **Behavior:** Old documents open unchanged; explicit Save preserves selected inputs/watches; Save As carries their configuration. Changing a global display toggle does not dirty source. Lost/missing metadata references remain harmless and explainable.
- **Edge cases:** Legacy v1, malformed metadata, future version, unknown names/line IDs, failed atomic write, concurrent disk edit, new/scratch document, recovery disabled, same-file reopen, canceled close, metadata-only changes.
- **Regression constraints:** No silent format downgrade/overwrite, automatic document rewrite on read, new source persistence opt-in, or unrelated settings loss. Preserve existing source bytes and line IDs.
- **Verification:** Old/new round trips, bounds/malformed/future fixtures, conflict and atomic-write behavior, settings defaults, Save As and dirty-close tests. Inspect actual serialized output.
- **Acceptance:** CP-02/03/05/09 preferences and metadata survive the intended lifecycle; old source and recovery compatibility are demonstrated, not assumed.
- **Commit boundary:** `feat(calc-pad): [M4-C] persist bounded editor and script view preferences`.

### M5-A — QR Generator joins the shared Dock / Pop Out host

- **Objective:** Make QR presentation consistent without duplicating sessions or external effects.
- **Code ownership:** `src/gui/qr_dialog.rs`, `utility_window_host.rs`, `command_host.rs`, `mod.rs`, `render.rs`, existing presentation/size settings, and narrowly affected `qa_framebuffer.rs` observations.
- **Tasks:** Implement the current shared-host adapter; retain one dialog state across transitions. Add Dock/Pop Out and remembered presentation with existing `qr-generator` geometry. Separate accepted host actions from render-only work; execute clipboard/file effects once. Integrate root visibility, focus, panel close/ensure-open, and viewport retirement. Adjust QA targeting only where QR host identity requires it.
- **Behavior:** Editing source/correction and switching host preserves QR content. Explicit fresh commands retain reset semantics. The launcher query remains clickable/editable while QR is popped out. Closing QR closes only the intended session/viewport.
- **Edge cases:** Repeated toggles, root hidden, restored presentation, native close, Escape/Ctrl+W, same-frame edit+copy/save, discarded/replayed pass, failed clipboard/save, canceled file dialog, capacity error/recovery, fresh invocation while already open.
- **Regression constraints:** Preserve exact payload bytes, error-correction behavior, capacity errors, cached raster fidelity, existing clipboard/save helpers' failure invariants, and no new automatic clipboard access.
- **Verification:** Adapt existing QR pure/GUI/command-host tests; shared-host stale-pass and effect-once tests; save/copy raster fidelity; targeted native host/focus interactions.
- **Acceptance:** QR-01 passes across all lifecycle paths; no second live QR view, ghost viewport, duplicate export, or input reset during a host switch.
- **Commit boundary:** `feat(qr): [M5-A] adopt shared dockable utility hosting`.

### M6-A — Bounded final verification, independent review, documentation, and handoff

- **Objective:** Validate the integrated approved behavior, resolve relevant findings, and deliver a truthful final tip.
- **Code ownership:** This Goal's full diff and directly affected docs/tests; no unrelated subsystem expansion.
- **Tasks:** Run the selected consolidated checks in H, perform the focused native scenarios when supported, and capture evidence. Have the configured reviewer inspect the actual diff and C1 coverage. Fix feature-related defects in small labeled remediation commits. Update examples/reference documentation for delivered syntax, limits, toggles, copy formats and supported Mermaid subset. Record completion and ancestry in the ledger/final report.
- **Behavior:** All approved features are implemented and discoverable; users can execute the smoke workflows without intermediate guidance. Optional panels remain optional and legacy behavior is preserved except the explicitly requested reassignment semantics.
- **Edge cases:** Unavailable desktop/automation, known unrelated test/lint failures, push rejection, real feature failures, dirty unrelated files, final report commit changing the tip.
- **Regression constraints:** No weakened tests/criteria, full-suite default, recurring candidate cycles, release tuning, unrelated Clippy cleanup, or premature COMPLETE status.
- **Verification:** H1–H4, one scoped independent review and bounded remediation; final diff/index/ancestry inspection. Revalidate only affected checks after fixes.
- **Acceptance:** Every C1 row has an implementation and verification status; mandatory criteria pass for COMPLETE; unverified native behavior yields LIMITED rather than a pass; feature-blocking defects yield BLOCKED/PARTIAL with the required report. `final_tip_sha` and actual push/clean state are explicit.
- **Commit boundary:** `docs(studio): [M6-A] document delivered behavior and acceptance evidence`, with `fix(<scope>): [M6-A-R1] ...` remediation commits as needed. Do not fabricate a source commit when only verification occurred. The final commit's actual SHA is the next Goal's required parent.

## G. Active Checkpoint Commit Cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

The active order is:

`M0-A → M0-B → M1-A → M1-B → M2-A → M2-B → M2-C → M2-D → M3-A → M3-B → M3-C → M4-A → M4-B → M4-C → M5-A → M6-A`.

M0-A is a new baseline whose parent is the recorded exact predecessor tip, not a label attached to an old commit. All later checkpoints remain on the **same Goal branch**. The code ownership and commit subjects in F define coherent boundaries; do not hold all Calc work for one giant commit. An extra small remediation commit is acceptable when a real defect crosses a boundary; use a descriptive stage-tagged subject and do not rewrite the previous checkpoint.

Before each commit, inspect active branch/HEAD/status and the staged path list, run the checkpoint's appropriate narrow verification, and review the diff. Stage only this Goal's work. Preserve unrelated user changes. Never squash, amend, discard, or force-push checkpoint history. The orchestrator normally owns commits and push attempts; implementers must not race it.

After each required commit, attempt one ordinary push when permissions/remote configuration permit. Retain the local commit and continue if pushing fails; record the failure without treating remote availability as functional acceptance. Do not pull/rebase/merge/reset or create another branch to fix a push rejection.

Record `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint subjects/SHAs, `final_tip_sha`, and actual push state. The final tip is the next Goal's required base, including when the tip exists only locally. If execution later moves to another computer without that commit, the next Goal must block until the ancestor is available rather than silently using older remote history. No intermediate branch merge/rebase/cherry-pick is needed or authorized to advance the chain.

## H. Verification and Testing Strategy

### H1. Scope, commands, and evidence discipline

Prefer focused colocated domain/session tests and targeted egui host tests. Confirm actual names after M0-A. **There are no assumed `--test calculation_pad` or `--test diagram` targets.** Cargo's unit-test compilation may still compile a substantial library even when execution is filtered; explain compilation/environment failures separately from test failures.

Illustrative narrow discovery/run commands, to adapt to actual discovered names:

```powershell
cargo nextest list --lib -E 'test(calculation_pad::)'
cargo nextest run --lib -E 'test(calculation_pad::document::)'
cargo nextest run --lib -E 'test(calculation_pad::session::) | test(calculation_pad::comparison::) | test(calculation_pad::dependencies::)'
cargo nextest run --lib -E 'test(diagram::mermaid::) | test(diagram::session::) | test(diagram::layout::) | test(diagram::codec::)'
cargo nextest run --lib -E 'test(gui::calculation_pad_dialog::) | test(gui::qr_dialog::)'
```

Add narrowly selected settings, command-host, retained-pass, render, Inspector, and persistence tests actually touched by a checkpoint. A broad name match such as all `gui::render` tests is not justified merely because one call site changed. List filters first so zero tests is not mistaken for success. A no-tests run is not verification. Split the final selected test set into coherent runtime-bounded batches rather than repeatedly running earlier completed batches unchanged.

For memory-constrained machines, lower Cargo build concurrency in that process (for example `$env:CARGO_BUILD_JOBS = '1'`) when evidence supports it. Distinguish build concurrency from Nextest test concurrency. Do not change production stack sizes, test expectations, or repository-wide runner configuration merely to conceal an environment failure. Tests that fail only concurrently should get one individual/serial diagnostic rerun; report the interference and fix a feature-caused ownership issue instead of labeling it harmless. Preserve intentional existing serialization where applicable.

**Test runtime budget:** Bound each selected execution batch to 100 seconds after compilation, using the executor's supported timeout/watchdog. Do not invent a Nextest CLI flag or apply that cap to a cold build. If a test hangs, capture its identity, stop the test process tree, drain/retain available output, and inspect the relevant case. Do not repeatedly restart the full batch. Compilation failures receive focused root-cause correction; they are not an invitation to build every unrelated target.

Use the existing `fast-dev` profile for a native application build when appropriate, such as `cargo build --profile fast-dev --bin multi_launcher`. Reuse the built binary until a relevant change needs rebuilding. No repeated release builds, distribution validation, benchmarks, or startup/idle performance initiative is part of this Goal.

Run one appropriate final first-party Clippy pass for changed compilation targets, normally `cargo clippy --lib --bin multi_launcher`, and check formatting of touched first-party Rust files using the repository's normal tooling. Resolve introduced warnings; report demonstrably pre-existing unrelated warnings rather than refactoring the repository or vendored crates. Do not suppress warnings globally to obtain a nominal pass.

For application test fixtures, follow `AGENTS.md`: disable Paste Sequence next/cancel shortcut registration (`next_hotkey = None`, `cancel_hotkey = None`) **before application construction**. Preserve production defaults and do not mute error reporting instead.

### H2. Domain and persistence acceptance fixtures

These are fixtures for the **implemented new behavior**, not claims about currently valid syntax.

**Binding identity:**

```text
x = 2
old = x
x = x + 3
r1
x
```

Expected source-row values: `2`, `2`, `5`, `2`, `5`. A comparison expression `x` sees `5`; selecting the first physical result sees `2`. Dependencies of `old` refer to the first binding, not the final one.

**Small script:**

```text
unit_price = 12.5
count = 4
subtotal = unit_price * count
count = count + 1
later_subtotal = unit_price * count
safe = if(count > 0 && unit_price >= 0, later_subtotal / count, 0)
values = [10, 20, 30]
total = 0
for amount in values:
    doubled = amount * 2
    total = total + doubled
answer = total
sequence_total = sum(range(1, 6))
average_value = average(values)
median_value = median(values)
population_sd = stdev([2, 2])
```

Expected: `subtotal = 50`, final `count = 5`, `later_subtotal = 62.5`, `safe = 12.5`, `answer = 120`, final `amount = 30`, `sequence_total = 15`, average/median `20`, and population standard deviation `0`. Pin `answer`; expose `unit_price` as an input; editing it must not overwrite either `count` assignment. These expected values are deterministic fixture arithmetic, not benchmark claims.

**Lazy evaluation:** `if(0 == 0, 7, 1 / 0)` yields `7`; `false && (1 / 0 > 0)` yields false. Invalid structural syntax remains diagnosed even when its branch is not selected. A declaration called `range` or `sum` remains valid in non-call position.

**Failed loop:**

```text
total = 0
for divisor in [2, 0, 4]:
    total = total + 10 / divisor
after = total
independent = 7
```

The loop reports the divide-by-zero body line/iteration. `after` is an error, not `0` or the partial total `5`; `independent` still yields `7` when only the local runtime failure occurred. The initial physical `total = 0` row may retain its own historical evaluation, but it is not the final named result of the failed block.

**Zero iterations:** Predeclare an iterator/value, run a loop over `range(0)`, and verify neither prior binding changes; body rows are not executed. Test nested iteration and each resource ceiling at boundary/one-over with controlled budgets; work limits do not rely on wall-clock flakiness.

**Legacy numeric invariants:** Retain representative existing tests for `u64(9007199254740993)`, checked overflow, signed shifts, bit helpers, `not`/`and`/`mod` contextual names, `^`, scalar remainder versus Euclidean `mod`, vectors and vector copy, compatible/incompatible units, exact-base conversion, Greek/braced names, finite-intermediate checks, and physical `rN` counting comments/blanks. Do not rewrite their expected output unless the Goal explicitly changes that behavior.

**Mermaid combined fixture:**

```text
%% Keep this introduction
graph RL; A["Start; ready"] -->|yes| B["Work"]; B --> C["Finish"];
%% Keep the finish explanation
C --> D["Done"]
```

Assert four nodes and three directed edges with the expected label, RL semantics, preserved comment text, and correct quoted semicolon. Rename/add/delete a node through actual graph operations; reparse, save/reopen, and Undo/Redo. Repeat the direction test for BT; retain TD/TB/LR fixtures. Test original source spans for malformed chains, supported style directives, unsupported directives, comment-only input, and source/expanded-graph limits. Test a legacy saved Diagram session with history.

**Saved Pad and preferences:** Read version-1 documents; save/reopen new metadata; preserve source bytes/line IDs; reject unsupported future versions and failed writes without discarding the draft; verify metadata-only dirty handling, Save As, defaults, and opt-in recovery. Unknown/missing watched names are not deserialization failures or stale valid output.

### H3. Focused UI/native scenarios

Run these against a safe test profile or disposable fixture data. Use synthetic egui tests for ownership, revision, and layout assertions where they provide stronger deterministic evidence. Use the real Windows application for native window/focus behavior when available. A screenshot is visual evidence, not proof of arithmetic, clipboard fidelity, or correct filesystem effects.

| Scenario | Minimum interactions and expected observation |
| --- | --- |
| UI-1: Script and result ownership | Open Pad, run the small-script fixture, edit an input, inspect the updated watch/result, Undo/Redo, and use an old versus latest binding in comparison. Values and errors must match H2. |
| UI-2: Editor assistance | Type a partial variable/function; accept/dismiss suggestions; toggle completion off and verify normal keys; inspect a nested call hint; switch existing light/dark theme where available. Source must remain unchanged by painting/theme toggles. |
| UI-3: Find, selection, folding | Fold a `# region` and loop; search inside them; Ctrl+G to a hidden line; click a diagnostic/result; replace all and undo; copy the whole source. Targets reveal correctly, line numbers remain physical, and no placeholder enters source. |
| UI-4: Panels and persistence | Show/hide inputs and watches, select two same-named source assignments with different line labels, attempt an invalid input edit, save/reopen, and resize to the existing compact supported window size. No required control becomes inaccessible. |
| UI-5: Formats and exactness | Show decimal/grouped/scientific/percent scalar output, then exact integers/units/vectors. Use canonical versus explicit formatted copy and inspect content. Format changes must not alter calculations or source/history. |
| UI-6: Work cancellation | Start bounded repeated work, edit immediately, switch host, close/reopen, and inspect pending/current status. Old completions cannot overwrite a newer document, and normal input remains responsive. This is a targeted observation, not a benchmark gate. |
| UI-7: Mermaid round trip | Paste H2 Mermaid, edit visually, change RL/BT and run explicit auto-layout, save/reopen/export source, and use Undo/Redo. Comments survive and arrows/layout are directionally correct. Existing non-flowchart preview remains available. |
| UI-8: QR lifecycle and focus | Open QR, edit source/correction, Pop Out → Dock → Pop Out, edit the main launcher query while QR remains open, copy text/image and save PNG explicitly, cancel a save, close via native close, and invoke QR freshly. State persists across presentation switches, fresh-command reset stays intentional, and no duplicate effect or ghost window occurs. |

Record the actual scenario, build/commit, result, and evidence path. Use a small number of representative screenshots plus action/test logs; do not recreate historical multi-round native qualification campaigns. Ensure any optional QA feature used actually corresponds to the new QR viewport owner. Never show private clipboard payloads, credentials, or unrelated user data in evidence.

### H4. Regression and retained-pass checks

Cover discarded/replayed egui passes, retired host generations, same-frame edit+save/copy/navigation, pending dirty-document transitions, stale Inspector/comparison contexts, word wrap plus folding, ordinary query-field focus, and absence of continuous reevaluation while idle. Use existing tests as the starting point rather than reimplementing all test infrastructure.

Verify accepted state changes exactly once. A speculative render pass must not mutate source, persist settings, write the clipboard/file, or retire the wrong viewport. Document-level close/reopen must invalidate old receipts even when the source text or filename is the same. Presentation changes alone must preserve the same live data session.

## I. Risk Management and Bounded Validation

| Sensitive boundary | Required mitigation |
| --- | --- |
| Reassignment versus old dependency graph | Versioned binding reads and separate runtime/source identity; test comparison snapshots and accumulators rather than merely removing DuplicateName errors. |
| New comparison/Boolean syntax versus programmer operations | Proper lexical assignment recognition; separate Boolean operators; retain contextual-name/bitwise regression cases and exact values. |
| Repeated execution versus responsiveness | Preflight input limits, bounded sequences/iterations/work, cooperative cancellation, one coalescing worker owner, current-revision publication, no unbounded expansion. |
| Folding versus real source/Undo | Explicit projection mapping; one source/history owner; expand hidden edit targets; test full-source copy and Unicode/CRLF/row identity. |
| Persistence versus existing user data | Legacy readers, explicit versioned writes when necessary, no read-triggered migration, bounded metadata, atomic/conflict-safe writes, no future-version overwrite. |
| Mermaid parser versus destructive regeneration | Preserve trivia through actual session transactions and export; enforce limits on expanded graphs and formatted output; keep unsupported-source and stale-preview behavior. |
| QR host switching versus duplicate effects | Existing accepted-host ownership and action staging; preserve the session; verify clipboard/file effects once and main-query focus independently. |

**Verification budget:** Use one focused checkpoint verification pass, then targeted fixes. For a repeatedly unresolved validation/native/environment failure, allow an initial diagnosis plus at most two focused remediation rounds before recording a precise blocker or limitation. Ordinary compilation corrections are normal development; the budget prevents repeating the same unsuccessful broad checks, not responsible implementation.

Use one scoped independent final review. A second review is limited to checking fixes for material findings, not restarting feature design. Optional performance work, historical failing tests, unrelated lint, inaccessible external services, and remote push failures do not expand this Goal. A focused wider check is permitted only with a recorded concrete reason that the touched boundary requires it.

Classify findings as **feature-blocking**, **unrelated/pre-existing**, or **optional/deferred**. Do not continue dependent milestones that rely on a broken feature contract. Independent work may continue safely on the same branch while the limitation is recorded. Never mark the whole Goal complete simply because independent portions passed.

If a genuine blocker exhausts the bounded attempts, follow `AGENTS.md`: create a concise tracked stall report under the active queue's `stalls/`, or `docs/plans/execution/standalone/calc-scripting-studio-polish/stalls/<UTC-timestamp>.md`, and update the corresponding `INTEGRATION_STATUS.md`. Record exact failing/unrun checks, expected/actual results, attempted remedies, available evidence, current branch/base/tip, local changes, actual push state, and the smallest next safe action. Use a safe documentation-only stop commit when possible; it is not an accepted implementation checkpoint. Preserve partial work. If tracked reporting is unsafe, use the existing external ledger first.

## J. Definition of Done

**COMPLETE** requires all of the following:

1. Every MMD-01…05, CP-01…16, and QR-01 behavior is implemented, documented, and covered by the appropriate focused evidence. No approved requirement is silently substituted with a simpler feature, such as collection-only repetition instead of multi-line loops.
2. Existing successful expressions, exact integers, dimensional values, vectors, contextual names, source bytes/row references, comparison/Inspector behavior, saved-document/recovery ownership, and existing Diagram preview families remain correct except for the explicitly changed duplicate-assignment rule.
3. Optional panels/editor controls remain simple, hideable, theme-compatible, and usable in docked/native/compact layouts. Fold/search/format operations do not mutate calculation semantics. Main launcher query focus remains independent.
4. All added execution/data paths are bounded. Current results belong to the current document/revision; aborted, missing, pending, and stale work are never presented as valid current answers.
5. Relevant unit/session/integration/GUI checks pass; actual native host/focus scenarios pass when claiming native verification. Introduced lint/review findings are resolved, and any unrelated existing findings are explicitly separated.
6. The configured reviewer inspected the final affected diff. No known feature-related blocking defect remains. Source examples/help match implemented syntax and limits.
7. All intended changes are committed using the active cadence on the correctly chained shared Goal branch. The baseline directly descends from `parent_tip_sha`; no worktree, independent agent branch, intermediate branch merge/rebase/cherry-pick, or history rewrite was used.
8. The final report records the actual final tip and next-Goal ancestry handoff, checkpoint/push outcomes, and preserved unrelated work. A failed push does not invalidate a correct local completion, but must be clearly disclosed.

Use **LIMITED** when implementation and available automated checks pass but required native/environment verification is unperformed. Use **PARTIAL/BLOCKED** when implementation or feature-related acceptance remains incomplete. These are truthful delivery states, not equivalent to COMPLETE. Do not ask the user to do intermediate work or treat a requested final human review as a substitute for Codex's attainable verification.

## K. Final Delivery Report

At the stop/completion boundary, provide a concise factual report with:

- **Status:** COMPLETE, LIMITED, or PARTIAL/BLOCKED, with the exact reason.
- **Delivered behavior:** User-visible Mermaid, Calc Pad, and QR changes; no implementation claim without corresponding work.
- **Coverage:** Completed checkpoints and each unmet requirement ID; separate implementation, automated verification, and native verification status.
- **Important decisions:** Actual supported language syntax, population-standard-deviation convention, safety ceilings, loop error behavior, optional-panel defaults, schema migration, and any justified departures from routine engineering defaults.
- **Changed ownership:** Significant files/modules and why; explicitly mention any necessary public/serialization change.
- **Evidence:** Commands/filters, counts/results, native scenario outcomes, screenshot/log paths, review/lint outcomes, timeout cleanup, and known unrelated issues. Never claim a command ran if it did not.
- **Git:** `parent_branch`, `parent_tip_sha`, active branch, `baseline_sha`, significant checkpoint SHAs, actual post-reporting-commit `final_tip_sha`, upstream and actual pushed/local-only state, clean/dirty state, and preserved unrelated changes.
- **Required handoff sentence:** “The next Goal must create its fresh child branch directly from `<final_tip_sha>`, not from `master`/`main`. No Git worktrees, auxiliary checkouts, agent-specific clones, or intermediate branch merges were used.” Replace the placeholder with the real final SHA and qualify truthfully if any policy was violated.
- **Remaining limitations and final human review:** Only short, concrete click/type steps for this feature. No request that the user complete intermediate implementation/testing, no unsolicited next Goal, and no promise of background work.

Stop after this approved Goal and its required reporting. Do not continue into the earlier brainstorming list.
