# Codex Goal — Calculation Pad Compact UI and Compound Assignments

**Status:** Approved for autonomous execution; no product questionnaire required.  
**Companion:** `CODEX_KICKSTART_CALC_PAD_CONFIG_MENU_COMPOUND_ASSIGNMENTS.md`  
**Goal slug:** `calc-pad-config-menu-compound-assignments`  
**Scope:** Calculation Pad only. This Goal is a focused follow-on to the completed Calc scripting/studio work, not a continuation of its native acceptance campaign.

> **One-off Git instruction from the user overrides any generic ancestry rule:** Start a **new Goal branch directly from the commit currently checked out in the existing feature-branch checkout at execution time**. Capture that exact `HEAD` as `parent_tip_sha`. **Do not switch to, pull, branch from, merge with, or rebase onto `master`/`main`, and do not substitute a historical handoff SHA for the current checked-out commit.** This exception applies only to this Goal/Kickstart pair. Use a single checkout and **no Git worktrees**.

## A0. Mandatory Git bootstrap — M0-A before application work

1. Read the repository's `AGENTS.md`, applicable `.codex` agent configuration, this Goal, and the companion Kickstart. Inspect the existing checkout **read-only**: `git status -sb`, `git branch --show-current`, `git rev-parse HEAD`, `git remote -v`, upstream tracking and index/untracked changes. Establish whether the checkout is a real feature branch, as the user specified.
2. Define `parent_branch` as **the branch already checked out** and `parent_tip_sha` as **its current `git rev-parse HEAD`**. Use this commit, even if it differs from older Goal handoff records or the remote's state. Do **not** fetch/pull or switch to `master` to choose a base. If detached, on the wrong branch, mid-merge, or unsafe due to uncommitted user work, report the specific blocker instead of guessing, resetting or stashing.
3. Select a unique child branch, e.g. `feat/calc-pad-config-menu-compound-assignments` (add a suffix if it exists), and run `git switch -c <new-branch> <parent_tip_sha>` in **the one existing checkout**. Verify the branch begins at the recorded `parent_tip_sha`. Do not modify application code beforehand.
4. Create a distinct **M0-A baseline commit** on the new child branch. Prefer a small task-specific execution ledger; an intentionally empty `git commit --allow-empty -m "chore(calc-pad): [M0-A] establish compact UI branch"` is acceptable. Verify `git rev-parse HEAD^` equals `parent_tip_sha`. Record `baseline_sha` separately from `parent_tip_sha`.
5. Attempt one ordinary non-force upstream push (`git push -u origin HEAD`) if available. If it fails, log the error and **continue locally**; never retry in a loop or force-push. Subsequent checkpoints stay on this same branch and receive one best-effort push after each commit.
6. Never use `git worktree add/move/prune`, agent-specific worktrees, side clones, auxiliary checkouts, parallel write branches, unapproved resets, rebases, merges, cherry-picks, or hard pulls. Never stage unrelated files. The final commit on this new branch is the handoff tip for any later Goal.

**M0-A acceptance:** Correct current-HEAD parent captured; one unique child branch active; baseline is its direct child; initial push attempted and accurately recorded; pre-existing user work untouched; no worktrees or alternate clones. **M0-A is a hard prerequisite** to implementation/tests.

## A. Goal / user-visible result

Clean up Calculation Pad's controls without removing functionality, and let users update a previously defined variable using common compound assignments.

1. Replace the spread-out top-level Calc Pad controls with **one visibly named `Config Menu`**. The source and results should have space to breathe; all existing commands remain discoverable from that single entry.
2. Replace inline **Copy** and **Copy formatted** buttons in **each Results value row** with a right-click menu ordered **Go to source → Select for comparison → Copy → Copy formatted**. Preserve the behavior of each action and result-row selection.
3. Support `+=`, `-=`, `*=`, `/=`, `%=`, `^=` using the Pad's existing operations and sequential binding semantics, including inside bounded `for` loops. They must not change the original source, result row numbering, comparison snapshots, exact integer precision, typed value handling or undo history.

This is not a Python interpreter, a toolbar redesign across other utilities, or a new computation framework.

## B. Actual source findings — current attached snapshot

| File / subsystem | Confirmed existing behavior / ownership |
| --- | --- |
| `src/gui/calculation_pad_dialog.rs::contents_ui` | Top row independently renders `New`, `Open`, `Save`, `Save As…`, `Examples & reference`, `Dependencies…`, `Programmer Inspector…`, utility-host Dock/Pop Out and `Remember last source`, plus document title and dirty indicator. Actions are staged as `DialogIntent`, inspector/dependency intents or host requests. |
| Same file: `panels_ui` | Standalone **View** and **Number format** menus control editable inputs, watches and display preferences; inputs/watches have their own useful Apply/Reset/Pin controls. |
| Same file: `editor_ui` | Separate Source-header line-number/word-wrap checkboxes and **Editor options** (Completions, Parameter hints, Auto-close delimiters, folding controls). Editor updates use retained host effects and persisted display preferences. |
| Same file: `results_ui`, `resolve_result_copy`, `ResultCopyIntent/Target` | Results row has inline Copy and Copy formatted and, for integer types, Inspect bits. Right-click menu already has Go to source and, on value rows, Select for comparison. Copy uses an accepted document/result snapshot; Copy all is a separate Results-header button. |
| `src/calculation_pad/document.rs` | `find_assignment_delimiter` identifies ordinary `=` while excluding comparison tokens. `prepare_line_internal` expects a name to the left of `=`; `total += 2` currently leaves `total +` as invalid LHS. `declared_names`, `assignment_definitions`, `resolve_references`, evaluation and comparison share naming/dependency concerns. |
| Same file: `editor_bindings`, `literal_inputs`, `source_requires_repeated_execution` | Completion uses discovered assignments; input panel only exposes literal assignments; background evaluation routing examines expression source. These must recognize or safely ignore new assignment operators as appropriate. |
| `src/calculation_pad/loops.rs` | Multi-line loop statements are prepared through the document parser and evaluated against mutable, source-provenanced bindings; loop failure rolls back affected assignments. The new operators must behave correctly in both non-loop and loop documents. |
| `src/calculation_pad/script.rs`, `programmer.rs`, `vector.rs`, `quantity.rs` | Script syntax and typed arithmetic have separate existing owners. `%` is signed remainder; `mod(a,b)` is Euclidean modulo; `^` means exponentiation (not integer XOR). Exact integer, quantity and vector operations enforce existing type/overflow constraints. |
| `src/gui/calculation_pad_editor.rs` | Syntax spans, symbol coloring and completion are source-based. Keep compound operators legible without altering text or caret mapping. |
| `src/calculation_pad/session.rs`, `comparison.rs`, `dependencies.rs`, `persistence.rs` | Own revision/line IDs, Undo/Redo, final bindings, watches, frozen comparison context, save/reopen and recovery; no new persistent wire format should be needed. |
| Existing tests in `calculation_pad_dialog.rs`, `document.rs`, `loops.rs`, `session.rs`, `calculation_pad_editor.rs` | Existing tests assert inline copy button locations, result context navigation, exact copying, stale-viewport rejection, source/row identity, shadowing, loop execution and compact layout. Adjust changed-UI tests, but keep their substantive safety assertions. |

The attached source is for planning; Codex must recheck the live checked-out code after M0-A. This Goal does **not** require access to an uploaded ZIP.

## C. Approved functional contract

### C1. Single Config Menu — UI-01

- Show **one top-level command/configuration menu**, labeled **`Config Menu`**, in place of the multiple top-row buttons and standalone configuration menus. Keep non-action status text (current document title and **Unsaved changes**) visible. Keep Source and Results headings and editable content visible.
- The single menu must expose **every currently top-level Calc Pad command** with its existing spelling and effects:
  - **Document:** New, Open, Save, Save As…;
  - **Tools:** Examples & reference, Dependencies…, Programmer Inspector…;
  - **Presentation:** the applicable **Dock** or **Pop Out** command, using the existing utility host;
  - **Configuration:** Remember last source, View (Editable inputs, Output watches), Number format (Auto/fixed/scientific, decimals, grouping, percent), Source display (Line numbers, Word wrap), Editor options (Completions, Parameter hints, Auto-close delimiters, per-region folds);
  - **Results-wide action:** **Copy all** (currently in the Results header). Keep its source-order/canonical summary behavior; moving it does not change the copy payload.
- Use a single top-level egui `menu_button("Config Menu", ...)`; simple separators or nested submenus are fine when they materially improve scanability. Do not leave duplicate top-level buttons/menus elsewhere for the moved actions. Avoid deep/duplicated menu state or a new window.
- **Do not hide essential in-context controls** inside an unrelated global menu: input-panel Apply/Reset/Pin, open Find/Replace/Go-to controls, explicit Save/Discard/Keep Editing confirmation, Save As name/confirmation, conflict remediation, and secondary window-specific controls remain usable in their own context. This resolves the request for a clean toolbar without making editing or close decisions inaccessible.
- Match the previous enabled/disabled behavior, accepted-pass effect routing, settings persistence, displayed feedback, focus, and Escape semantics. A menu click must not cause unintended Save, host switch, source edit, Clipboard action or launcher search activation.
- Works both docked and popped out; remains usable at compact sizes. Do not silently default `Remember last source` on or change named document persistence.

### C2. Results context menu — UI-02

- **No inline row buttons labeled `Copy` or `Copy formatted`.** They are accessible through right-click on the result row, including its value text, row label, or empty row background.
- Show the first four actions in this **exact visual order**, with these labels:
  1. `Go to source`
  2. `Select for comparison`
  3. `Copy`
  4. `Copy formatted`
- Value rows: all four are enabled when the corresponding accepted result is available. `Copy` yields the existing **canonical, pasteable** representation; `Copy formatted` yields the existing **display-formatted** representation. Existing line IDs, revision/host ownership, stale-result refusal and exactly-once clipboard effects remain intact. Menu closes after a successful action.
- Error rows: Go to source remains enabled. For a consistent four-action menu, **disable** value-only actions (comparison and copies) instead of copying an imaginary value or exposing invalid operations. Blank/comment rows still do not generate result cards.
- Preserve integer **Inspect bits…** functionality; for a cleaner row, place it as a separate fifth context action **after** the required four only on compatible exact-integer rows (or preserve its existing access if moving it introduces a demonstrated regression). Do not replace `Select for comparison` with `Inspect bits`—these are different actions.
- A normal left click on a row continues synchronized Source/Results navigation; the right-click menu does not mutate results, remove focus unexpectedly, bypass pending-state guards, or trigger actions from a retired pop-out viewport.

### C3. Compound assignment — CP-01

**Supported syntax for this Goal:** `+=`, `-=`, `*=`, `/=`, `%=`, `^=`. This is the conventional arithmetic set of already-supported binary operations; it avoids an open-ended expansion into bitwise/shift operators. Both `name += rhs` and `{Unicode name} += rhs` are valid if `name` is an assignable variable already bound at that point. Spaces around the **whole** operator are allowed; characters inside `+=` etc. are contiguous.

- Meaning: `x <op>= rhs` evaluates as **`x = x <op> (rhs)` using the latest successfully or unsuccessfully published preceding binding of `x` at the time this statement runs**. A failed prior binding yields the same dependency failure behavior as an explicit `x = x <op> rhs`. The expression is evaluated once; publish the new binding only after its result has been determined, using existing assignment error/publishing semantics. Earlier physical row results stay frozen.
- **Not initialization:** `missing += 1` before any definition must produce a local unknown/unbound-name error, even if `missing = ...` appears on a later line. Never resolve its implied prior value from a future assignment. Do not fabricate zero.
- **RHS rules:** Full existing expression grammar is allowed, including parentheses, references, functions, and variables. Operator precedence is that of the explicit underlying operation with **the entire RHS as the right operand**. For instance `x *= 2 + 3` multiplies by five. No double RHS evaluation and no string-based source rewriting that changes visible physical lines.
- **Operator meaning:** `%=` uses the existing `%` remainder with dividend-sign convention (not `mod(a,b)` Euclidean behavior). `^=` means exponentiation, not XOR. `/=` follows the existing type-specific division behavior; zero divisors and unsupported types yield normal line diagnostics. No `mod=`, `idiv=`, `rem=`, `and=`, `or=`, `<<=`, `>>=`, `&=`, `|=`, or invented operators in this Goal.
- **Preserve typing:** Use existing finite scalar, exact integer/checked-width, unit-aware quantity, vector, and programmer routes where each underlying operation is already supported. Operations unsupported for a given type fail with a precise diagnostic rather than silently converting to floats, truncating or normalizing into another type. Preserve integer width/sign, overflow handling and vector-length/unit compatibility.
- **Sequential semantics:** Chain reassignment (`x = 2; x += 3; x *= 4`) and later `x`/`rN` references behave as they do with their explicit `x = x + ...` equivalents. Dependency Explorer and Inspector/Comparison resolve the correct source-line provenance and latest binding.
- **Loops:** A compound assignment inside a `for` body reads that iteration's current binding, updates it for subsequent statements/iterations, respects loop rollback on failure, and does not introduce false source-level cycles. The loop iterator is freshly bound for each iteration as before. Retain 8-level / 10,000-iteration / 50,000-work-unit bounds and cancellation behavior.
- **Compatibility:** Plain `=`, `==`, `!=`, `<=`, `>=`, percent/infix `mod`, `mod(a,b)`, current programmer syntax, braced identifiers, reserved names and row-reference assignment exclusions retain their meanings. New operators must not cause comparisons or ordinary expressions to be misclassified as assignments. Invalid or ambiguous syntax yields a bounded line-local diagnostic.
- **UI/source integrity:** Keep exact source bytes, CRLF and Unicode, source/Result line identity, no output text rewriting, Undo/Redo, Save/Save As/reopen, optional scratch recovery, watches, dependency snapshots, and stable editor caret/selection. Source highlighting should recognize compound operator symbols; code completion recognizes already-defined compound-assigned names. **Editable inputs** must not treat a computed `x += 1` line as a literal editable input or overwrite it via the panel.
- **Comparison expression surface is read-only:** do not accidentally permit compound assignments there. The standalone comparison grammar must reject statements, while comparisons of actual evaluated named results still read the final binding.

### C4. Examples with exact expected outcomes

```text
x = 2
x += 3
x *= 4
x /= 2
x %= 6
x ^= 2
x
```

Expected physical results: `r1=2`, `r2=5`, `r3=20`, `r4=10`, `r5=4`, `r6=16`, `r7=16`.

```text
total = 0
for n in range(1, 4):
    total += n
total
```

Expected last result: `6`. A source file using CRLF and the same statements must produce the same results while preserving its original bytes.

```text
{顧客 名} = 4
{顧客 名} -= 1
{顧客 名}
```

Expected last result: `3`.

Negative modulo example: `x = -7` followed by `x %= 3` yields `-1`, matching `%` rather than `mod(-7,3)`. `missing += 1` yields an error. `x = 7; x /= 0` yields an error and a failed latest binding.

## D. Non-goals and invariants

- No general-purpose scripting interpreter, new variables scope model, bitwise compound operators, automatic fixes to old documents, Markdown studio redesign, toolbar migration in other utilities, new workspace features, or new dependencies without a specific demonstrated necessity.
- Do **not** change calculator numerical semantics, saved documents/versioning, clipboard payloads, default settings, global hotkeys, paste behavior, dock/pop-out lifecycle or launcher search focus.
- Do not reopen the older UI-6B native in-flight supersession / divider persistence campaigns as acceptance gates for this small Goal. If touched behavior regresses, report it; otherwise this Goal checks only the directly affected UI and expression paths.
- Do not run a full Nextest suite, benchmark suite, or release build merely for a small UI/parser extension. Do not format or refactor unrelated modules or vendored code.

## E. Technical strategy

1. **Keep one menu command owner.** Refactor only the Calc Pad UI builder so document/tools/presentation actions still generate their existing `HostEffects`, `DialogIntent`, `ResultCopyIntent`, display-preference and settings events. Consolidate the old `panels_ui`/`contents_ui`/`editor_ui` controls into one top-level menu while keeping context-local controls in their panels. The existing `utility_host::presentation_button` behavior must remain authoritative for Dock/Pop Out; match its label/state even if the caller moves to a menu item.
2. **Reuse existing result intent paths.** Relocate result-row actions into `row_response.context_menu` and invoke the same accepted-pass intents as the former inline buttons. Preserve row selection/highlighting and physical `LineId`; update QA/focused tests to click context menu rather than obsolete button rectangles.
3. **Parse compound assignment in a single domain owner.** Introduce a narrow typed assignment-head/operator representation in `src/calculation_pad/document.rs` or its immediate domain helper; share it across `prepare_line_internal`, `declared_names`, `editor_bindings`, `literal_inputs`, `source_requires_repeated_execution`, and loop preparation. Avoid a parallel regex/substring parser and avoid emitting rewritten user source. Ensure the implicit preceding-binding reference is properly represented for dependency and cycle detection, including loop runtime slot resolution.
4. **Evaluate through existing arithmetic owners.** Preserve typed scalar, exact programmer/integer, vector, quantity and script dispatch; the compound result must be indistinguishable from the corresponding explicit expression for its type and diagnostics. Avoid introducing unchecked arithmetic or an independent numerical evaluator. Make comparison context remain expression-only.
5. **Maintain retained effects and revision identity.** Menu actions and context copy/navigation must not be applied from discarded frames or stale document/host generations. In particular, a same-frame source edit plus copy or host switch must retain current agreed precedence.
6. **Update documentation/examples and owned tests only.** Show the supported operator set and semantics in `src/calculation_pad/README.md`, add focused executable examples only if consistent with the existing catalogue, update obsolete UI-geometry tests to the new menu location without weakening original correctness assertions.

## F. Ordered milestones, verification and checkpoints

### M0-B — Focused baseline after Git bootstrap

- **Objective:** Establish a small, current-source baseline without modifying code.
- **Ownership:** `src/gui/calculation_pad_dialog.rs`, `src/gui/calculation_pad_editor.rs`, `src/calculation_pad/document.rs`, `loops.rs`, `script.rs`, `session.rs`, directly owned tests and existing test runner.
- **Actions:** Identify exact existing menu, right-click, assignment, copy and loop tests. Run a representative bounded baseline test selector in the configured Windows lane; record results, source HEAD and any existing failures. Inspect the user's current settings/profile safely; use disposable profiles for native checks.
- **Edge cases/invariants:** No full suite, no production settings writes, no new code/format changes. Preserve preexisting baseline failures separately.
- **Acceptance/evidence:** Record selector, process exit code, pass/fail count and log/receipt location. **Commit:** a compact `M0-B` evidence/ledger update only if the project convention warrants it; otherwise include in first implementation checkpoint's ledger. Stay on the same child branch.

### M1-A — Consolidate Calc Pad's top-level controls

- **Objective:** Show the single `Config Menu` and retain every command/configuration action.
- **Ownership:** `src/gui/calculation_pad_dialog.rs` (`contents_ui`, `panels_ui`, `editor_ui`, results header, host state, display/persistence effects), focused dialog tests.
- **Tasks:** Move Document/Tools/Window/Remember-last-source controls to one top-level menu; move View, Number format, Line numbers, Word wrap and Editor options/folding controls into that menu's logical groups; move Results **Copy all** there. Keep doc name/dirty indicator and Source/Results headings visible, leave contextual editor and confirmation controls where needed. Preserve original intent/host/setting dispatch and disabled states; retain useful accessibility and keyboard navigation.
- **Regression edges:** Compact 640px-class window; native popped-out host while root query stays usable; Save As/dirty close and conflicts; display toggles do not corrupt source or Undo/Redo; menu does not swallow global Escape.
- **Verification:** Focused egui menu-activation, option-persistence, single-menu visibility and retained-host tests; native docked/popout menu screenshots when supported.
- **Acceptance:** One top-level `Config Menu` replaces former toolbar/menu clutter; all enumerated actions work from it in docked and popped-out modes; no surprise state change or inaccessible modal decision.
- **Checkpoint commit:** `feat(calc-pad): [M1-A] consolidate top-level controls in Config Menu`; best-effort push.

### M1-B — Result right-click actions, exact order

- **Objective:** Remove result-row inline copy buttons while preserving every result action.
- **Ownership:** `src/gui/calculation_pad_dialog.rs::results_ui`, `resolve_result_copy`, result QA selectors/tests.
- **Tasks:** Render exact ordered 4-item menu on each rendered result; gate value-only items for diagnostics, retain Go to source; relocate typed-integer Inspect bits after four items if safe; preserve Copy all in Config Menu. Reuse existing copy and comparison intents, menu close on selection, stable row selection and stale-host guards.
- **Regression edges:** Right-click background/label/value, error row, vector/exact integer/quantity, formatted versus canonical copies, same-frame result change, pending work, copy failure, popout retirement.
- **Verification:** GUI input tests assert exact visible order and absence of inline copy buttons, canonical/formatted clipboard outputs, error disabled items, Go to source highlight, comparison selection, typed Inspect bits and stale-result refusal. Update the old tests that click `test_line_copy_rects` rather than deleting copy semantics tests.
- **Acceptance:** The four actions are discoverable in the correct order; no per-row Copy buttons remain; successful actions act once on the correct accepted row.
- **Checkpoint commit:** `feat(calc-pad): [M1-B] move result actions to context menus`; best-effort push.

### M2-A — Compound assignment syntax, scalar and typed arithmetic

- **Objective:** Add six compound operators to the document grammar and sequential evaluation without rewriting source or changing existing operators.
- **Ownership:** `src/calculation_pad/document.rs`, immediately necessary `script.rs`/typed evaluator adapters, grammar and domain tests.
- **Tasks:** Implement robust assignment-head recognition with explicit operator identity and source ranges; handle implicit previous binding; preserve normal `=` and comparison tokens; evaluate with existing underlying arithmetic/type handling; produce source-local errors for absent/failed prior values and unsupported types. Bound parse complexity and expression/work limits.
- **Regression edges:** Braced/Unicode names, CRLF, reserved/row names, chained assignments, `x += x`, parentheses precedence, `-7 %= 3`, zero divisors, `^=` power, fixed-width integer overflow and mixed-mode errors, quantities/vectors, erroneous `x + = 2`, `==`/`<=`/`>=`/`!=` comparisons.
- **Verification:** Focused direct document tests with exact expected results, outcomes, source ranges, dependency edges, type representations, diagnosed failures and no accidental future-binding reads. Keep all current operator tests valid.
- **Acceptance:** Compound assignment examples evaluate correctly, preserve exact source lines and prior row results, and reject invalid operations without silent type conversion.
- **Checkpoint commit:** `feat(calc-pad): [M2-A] add typed compound assignment semantics`; best-effort push.

### M2-B — Loop, editor, session and comparison integration

- **Objective:** Make compound assignments safe in real scripts and all Calc Pad consumers.
- **Ownership:** `src/calculation_pad/loops.rs`, `document.rs`, `session.rs`, `dependencies.rs`, `comparison.rs`, `src/gui/calculation_pad_editor.rs`, existing input/watch/copy tests, `src/calculation_pad/README.md` and examples as appropriate.
- **Tasks:** Ensure compound LHS is recognized in loop preparation; runtime lookup reads current binding and preserves loop rollback, affected rows and cancellation. Teach source-aware assistance/grammar tools to recognize compound assignments as assignments without treating them as literal inputs. Preserve watches, result row IDs, Inspector, dependency lineage, comparison final binding, Undo/Redo and save/reopen. Disallow compound statements in comparison input. Document operators, examples, unsupported forms and type limitations.
- **Regression edges:** Multiple iterations, nested loops, zero iterations, failed second iteration/rollback, forward-only name, field watchers, stale/reopened saved documents, stored CRLF/Unicode, compaction/folds, source syntax colors, evaluator pending/coalescing.
- **Verification:** Selected loop/worker/inspector/dependency/compare/input-panel/session tests; relevant editor and persistence tests. Include source round-trip and a compact integration example. Use a bounded execution watchdog for Nextest batches (100 seconds for test execution where supported), and rerun legitimate flaky tests individually rather than weakening assertions.
- **Acceptance:** Loop accumulation works, errors roll back correctly, persisted source is byte-exact, all consumers reflect latest accepted binding and no separate evaluation engine exists.
- **Checkpoint commit:** `feat(calc-pad): [M2-B] integrate compound assignments across Pad scripting`; best-effort push.

### M3-A — Final focused review, Windows smoke, and handoff

- **Objective:** Deliver verified, regression-bounded behavior and an accurate completion receipt.
- **Ownership:** All touched first-party files, existing Calc Pad QA/native path, execution ledger.
- **Tasks:** Run a final focused set of directly affected tests; targeted compilation/check if needed, one bounded lint/format review on owned files only, independent code review with configured reviewer, and corrections of feature-related findings only. Launch Windows app when supported and exercise both docked/popout `Config Menu`, result menu order/copy, scalar + loop compound example and named-document save/reopen. Capture before/after evidence.
- **Regression edges:** No unwanted clipboard/system changes, root query focus, global hotkeys, undo/redo, compact 640-class UI, pending/dirty dialogs, previously validated operator grammar. Do not repeat prior unrelated native acceptance campaigns, full test suite or release builds.
- **Verification/evidence:** For each scenario record source commit/executable identity, exact input/actions, observed/expected values, screenshot paths and pass/fail/unverified; automated run selector, count, exit code, process cleanup. If desktop capture is blocked, mark **NATIVE UNVERIFIED**, not PASS. Review source diff for unrelated changes.
- **Acceptance:** All feature behaviors implemented; directly owned tests pass; no known blocking feature regression; known limitations accurately documented. Git branch descendants, commits and push status recorded.
- **Checkpoint commit:** `docs(calc-pad): [M3-A] record compact UI and compound assignment verification` or a small remediation commit then final documentation commit; best-effort push. Record actual `final_tip_sha` after last commit.

## G. Active checkpoint cadence and Git completion

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** This Goal uses `M0-A` (mandatory independent baseline), optional evidence-only `M0-B`, `M1-A`, `M1-B`, `M2-A`, `M2-B`, `M3-A`; these are checkpoints **on one branch**. For each, commit coherent verified changes, attempt one non-force push, and keep failures/non-push accurately logged. Do not rewrite/squash older history or stage unrelated files.

The final report must include `parent_branch`, **current-HEAD** `parent_tip_sha`, `baseline_sha`, checkpoint SHAs, `final_tip_sha`, final branch and upstream push/dirty state. The **next Goal**, if started later, branches directly from this Goal's recorded `final_tip_sha`. Do not merge to `master` as a part of this task.

## H. Verification strategy and risk controls

- **Targeted domain:** parser/lexer equality guard, missing/failed binding, scalar/type/overflow and source identity; loops and rollback; existing named assignments, dependency/comparison/watch behavior.
- **Targeted UI:** Config Menu actions and toggles; right-click menu order & value/error gating; canonical/format output; docked/native host and compact controls. Update UI geometry fixtures affected by the move while preserving behavioral assertions.
- **Native only where possible:** Click menu and right-click actions through actual Windows GUI, record results/screenshots. Do not treat a synthetic egui test as proof of Windows interaction. Use a disposable profile and non-sensitive synthetic source.
- **Work budget:** Prefer at most two focused attempts for an environmental native obstacle, document limitation, then stop. Do not chase an old unrelated divider or worker timing issue unless this feature itself regresses.
- **Review:** First-party targeted Clippy/rustfmt and independent peer review of implementation/scope. Existing unrelated warnings/format differences must be identified separately, not fixed wholesale.
- **Known technical risks:** split action controls across host passes; accidental menu Escape behavior; stale copy/popup; interpreting `+=` as invalid ordinary assignment; unintended forward-binding/false cycles; typed precision loss; invalid editable input selection; changed command persistence. Mitigate with explicit invariants, tests and staged effects.

## I. Definition of done

- `Config Menu` is the sole top-level Calc Pad command/configuration menu; all previously available top-level actions, settings and Copy all remain reachable, and source/results remain easy to use.
- Every rendered Results row has the first four right-click actions in the specified order, with valid value gating; per-row Copy/Copy formatted buttons are gone, canonical and formatted copy effects are preserved.
- All six listed compound assignment operators work with proper previous-binding semantics; invalid cases have correct diagnostics; loops, exact arithmetic, quantities/vectors where supported, comparisons, snapshots and persisted source remain correct.
- Targeted automated and available native checks are completed with actual evidence; feature-related review findings resolved; unverified native behavior is labeled honestly.
- All task-owned changes are committed to the one newly branched child of the **current checkout's original HEAD**; M0-A ancestry and final commit/push state are recorded, no worktrees, no unrelated rewrites.

## J. Final delivery report

Report concisely: completed checkpoints; exact user-visible behavior; code files changed; major semantic choices; targeted commands/pass counts; native scenario matrix with screenshot/receipt paths and PASS/FAIL/UNVERIFIED; residual feature issues versus environmental limits; Git `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs, final branch `final_tip_sha`, upstream status, dirty/clean state; reviewer disposition; and next-Goal tip handoff. Stop at the agreed scope—do not start another feature.
