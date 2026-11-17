# Codex Goal — Calculation Pad Expansion and File Search Metadata Filters

**Goal slug:** `calc-pad-and-file-search`  
**Companion:** `CODEX_KICKSTART_CALC_PAD_AND_FILE_SEARCH.md`  
**Scope:** C4, C5, C6, C14, C18, C20, C21, C22, C24, C25, C26, C27, and F3.  
**Contract:** Implement all thirteen core features, preserve existing documents and unrelated behavior, and report implementation, automated verification, and native verification separately. This document is a plan, not evidence that implementation or verification has already occurred.

## A0. Mandatory chained Git bootstrap — M0-A, before all application work

Read `AGENTS.md`, applicable local instructions, this entire Goal, and its Kickstart first. The first execution checkpoint is **M0-A**. Until it is complete, only read-only inspection and the safe branch/baseline operations below are authorized; do not edit application code, run implementation builds/tests, or create feature checkpoints.

1. In the **single existing checkout**, inspect `git status -sb`, the current branch, `HEAD`, remotes/upstreams, staged/unstaged/untracked changes, and the active queue/execution records. Treat existing user changes as intentional. Do not infer the active predecessor from whichever historical plan happens to be newest on disk.
2. Establish whether this is a continuing chain or the first Goal in a genuinely new chain. For a continuing chain, the required base is **the exact final committed tip of the immediately preceding completed Goal/category branch**. Resolve and record `parent_branch` and `parent_tip_sha` from authoritative execution records and verify the commit exists. Fetch safely when the required ancestor is remote-only. A predecessor's actual local tip remains authoritative when its push failed. On another machine, block until that exact ancestor is available rather than using an older remote tip.
3. **Only the first Goal in a genuinely new chain may default to the up-to-date primary integration branch**, normally `master`/`main`, unless the user explicitly supplies another base. Do not return to `master`/`main` between Goals. If a first-task current-HEAD base is necessary, it must be safe, explicitly recorded, and must not lose or absorb user work. Conflicting or incomplete predecessor records are a bounded blocker, not permission to guess.
4. Choose one fresh, unique, lowercase kebab-case task branch, for example `feat/calc-pad-and-file-search`, with a suffix if needed. **The Goal and Kickstart share this ONE branch.** This plan does not introduce additional branch/category boundaries. Milestones, agents, and checkpoints remain on this branch.
5. Protect user changes. Do not automatically reset, stash, hard-pull, merge, rebase, cherry-pick, force-checkout, overwrite files, or stage unrelated changes to make switching convenient. If the exact parent cannot be selected safely, report the precise blocker instead of choosing another base.
6. Create and switch to the new child branch directly from the verified SHA, for example `git switch -c feat/calc-pad-and-file-search <parent_tip_sha>`. Verify its initial `HEAD` equals `parent_tip_sha`.
7. Make this Goal's own **M0-A baseline commit**. Prefer the minimal task execution ledger; an intentional empty commit is acceptable: `chore: [M0-A] establish calc-pad-and-file-search branch baseline`. Record `baseline_sha` separately from `parent_tip_sha`. Verify `git rev-parse <baseline_sha>^` equals the required parent tip. The predecessor's final commit is not this Goal's baseline.
8. Attempt one ordinary, non-force upstream push, normally `git push -u origin HEAD`, when a suitable remote and permissions exist. If unavailable, denied, or rejected, record the exact failure/unavailability and continue safe local execution. Do not retry in a loop, bypass permissions, rewrite history, or change ancestry to make pushing succeed.
9. **No Git worktrees, auxiliary checkouts, secondary/helper clones, or agent-specific implementation branches.** Do not run `git worktree add`, `move`, or `prune`. Agents share this checkout; only one writer/build owner operates at a time. Existing unrelated worktrees, if any, are not permission to use or alter them.
10. Record the ancestry and baseline in the existing authoritative ledger, or use `docs/plans/execution/standalone/calc-pad-and-file-search/STATUS.md` when no queue ledger exists. Record checkpoint SHAs, actual verification, push state, and any dirty files throughout execution. At completion, record the actual final `HEAD` as `final_tip_sha` in the final handoff. The next Goal must create its branch **directly from that SHA**, without intermediate merging, rebasing, squashing, or cherry-picking. The last child tip contains the cumulative chain; integration into the primary branch is a separate approved final boundary, not authorized by this Goal.

**M0-A acceptance:** unique child branch active; exact parent recorded and verified; baseline is its direct child; initial push attempted or concrete unavailability recorded; user changes preserved and not staged; no additional checkout/worktree created. Do not start M0-B or M1 before these conditions hold.

## A. Desired outcome and requirement coverage

Calculation Pad should become a more capable reusable calculator without replacing its current source language, making simple documents slower by default, or creating competing sources of input values. File Search should support meaningful date/size restrictions rather than only sorting by those properties.

| ID | Required user-facing outcome | Main checkpoints |
|---|---|---|
| C4 | Generate a non-destructive, single-input sensitivity table with multiple outputs and table/CSV/Markdown copying. | M10-A, M10-B |
| C5 | Open a separate dockable/pop-out Charts dialog for line, bar, and scatter visualizations. | M11-A, M11-B |
| C6 | Configure labels, descriptions, display suffixes, bounds, steps, and appropriate numeric controls for source-backed inputs. | M9-A, M9-B |
| C14 | Preview and safely apply scope-aware variable/parameter renames, reversible in one undo. | M8-A, M8-B |
| C18 | Declare pure, document-local formula functions with parameter hints and autocomplete. | M5-A, M5-B |
| C20 | Use inline comments and delimiter-based multiline expressions while preserving physical row references. | M1-A, M1-B |
| C21 | Use the ten requested everyday numeric helpers with clear diagnostics and examples. | M4-A |
| C22 | Use typed calendar dates, times, timestamps, and elapsed durations inside calculations. | M6-A, M6-B |
| C24 | Select Automatic or Manual calculation, with explicit Calculate/Cancel and truthful stale-result state. | M2-A, M2-B |
| C25 | Opt into bounded exact decimal and fraction arithmetic without changing existing floating-point semantics. | M3-A, M3-B |
| C26 | Use an optional Extended Document profile and see source/evaluation resource limits. | M7-A, M7-B |
| C27 | Use statement-level `if`/`elif`/`else`, `break`, and `continue` with existing loop safeguards. | M5-C |
| F3 | Filter filename/content searches by modification time and size, with consistent backend semantics. | M12-A, M12-B, M12-C |
| Cross-cutting | Preserve persistence, history, source identity, host ownership, and examples; complete independent review and truthful handoff. | M0-B, M13-A, M13-B |

### Scope choices made for this Goal

These are explicit planning defaults, not claims of separate user answers:

- Include all thirteen core features. Use the placements and conservative recommendations in this document.
- Sensitivity analysis is **one varied input** in this Goal. Two-input grids were an optional extension and are deferred.
- Formula functions have a single logical expression body, which may span parenthesized physical lines. Statement-bodied functions, recursion, global libraries, closures, and side effects are deferred.
- Date/time arithmetic is deterministic and explicitly typed. Optional live `today()`/`now()` functions, date-picker inputs, recurrence tools, and business-day calendars are deferred. Do not add a periodic clock refresh service.
- Exact arithmetic is opt-in by constructors. There is no document-wide replacement of legacy floating-point or fixed-width integer rules, and no exact-unit-system rewrite.
- Extended Document is opt-in; Standard remains the default. Larger source capacity does not automatically increase loop, vector, call, or arithmetic work limits.
- F3 targets **last modified time**, not creation/access timestamps. It does not add a new launcher query syntax or remove the requirement for search text.
- Do not add workspace features, framework migrations, unrelated optimizations, or new scripting access to files, networks, processes, or application commands.

## B. Current codebase findings — revalidate after M0-A

These findings identify the real integration points. Line numbers are intentionally not used as execution dependencies. Proposed new modules later in this document are explicitly marked as proposals.

| Existing location | Observed behavior | Planning consequence |
|---|---|---|
| `src/calculation_pad/document.rs` | `CalculationDocument` retains original source. `evaluate_inner` selects a loop evaluator when `loops::contains_loop` detects a loop; otherwise it prepares physical lines. | Comments, continuations, new blocks, functions, and typed calls must work through a shared source-analysis boundary, not incompatible ad hoc parsers. |
| Same file | `CalculationValue` currently includes scalar, Boolean, sequence, vector, typed integer, dimensional/physical quantity, and exact-base variants. `LineOutcome` has Blank, Comment, Value, Error. | New value/statement kinds require deliberate handling in formatting, copying, comparisons, inspectors, watches, and dependency consumers. |
| Same file | Limits include 64 KiB source, 256 physical lines, 2,048 expression bytes, nesting 32, 512 total dependencies, 64 identifiers per line, and 128-byte names. `literal_inputs`, `editor_bindings`, and `watch_name` already exist. | Parameterize relevant limits centrally; reuse source-bound input semantics and name rules. Do not change only a GUI limit. |
| `src/calculation_pad/script.rs` | Lazy `if()`, logical operators, comparisons, ranges/statistics, `WorkBudget`, and a shared `FUNCTIONS`/signature catalog exist. Work is capped at 50,000 units; ranges at 1,024 elements. | Reuse budget/dispatch/signature infrastructure. New operations cannot bypass cancellation or introduce independent counters that reset at each nested call. |
| `src/calculation_pad/loops.rs` | Indentation-based loops have 8-level nesting and a 10,000-iteration budget. Successful loop assignments/iterators remain available afterward; failures protect transaction boundaries. | New blocks and flow control must preserve those runtime binding semantics. Do not invent lexical loop-local scopes merely to simplify rename. |
| `src/calculation_pad/session.rs` | The session owns revisions, stable `LineId`s, save transitions, source history, cached results, input replacement, dependencies, and inspector guards. `EditableState` currently stores source/line IDs, not view metadata. | Manual mode needs a genuine scheduling policy. Rename needs an atomic source-plus-related-metadata transaction; existing source-only undo is insufficient for that requirement. |
| Same file | `evaluation()` may schedule background work or synchronously recalculate; observation APIs such as dependency/inspector reads intentionally require current cached results. | Audit every observation path. Opening a tool in Manual mode must not secretly calculate. Never combine old source ranges with current edited text. |
| `src/calculation_pad/worker.rs` | One coalescing worker carries document/incarnation/revision stamps, cancels retired requests, and wakes the UI on completion. | Extend the identity with an evaluation generation/semantic basis where needed; same-revision explicit runs must not accept an older completion. |
| `src/calculation_pad/persistence.rs` | Document format is v2; the reader also handles v1. `PadViewMetadata` stores selected input references and watch names. Strict unknown-field validation, bounded loading, atomic writes, and external-change detection exist. | Add an explicit versioned migration, not fields that silently change the v2 contract. Preserve v1/v2 reads and conflict detection. |
| Same file; recovery hooks in `src/gui/calculation_pad_dialog.rs` | Recovery format is v1 and source-only. Source/line limits are independently enforced in recovery, storage, session editing, and evaluation. | Extended sources need coherent recovery and storage support. Preserve the settings needed to interpret/reopen the recovered source; do not increase only one validator. |
| `src/gui/calculation_pad_dialog.rs` | Config Menu, source-backed editable inputs, watches, staged host effects, document receipts, and docked/native presentation already exist. | Integrate controls into the existing ownership/effect pipeline. Never commit UI actions during sizing/discarded/non-owning passes. |
| `src/gui/calculation_pad_editor.rs` | Folding/projection, syntax spans, completion, call hints, delimiter matching, and find/replace already exist. | Extend their shared source mapping; variable rename is not global text replacement. Preserve hidden text and Unicode byte/character conversions. |
| `src/gui/calculation_dependency_dialog.rs`, `src/gui/programmer_inspector_dialog.rs`, `src/gui/mod.rs`, `src/gui/utility_host.rs` | Companion utilities use document/host identity, staged actions, and presentation ownership. | Charts must follow this infrastructure rather than open an independent unguarded viewport. |
| `src/settings/utility_dialog_sizes.rs`, `src/settings/utility_presentations.rs` | Utility identities are validated keys with remembered sizes/presentation. | Register Charts in the actual catalog and lifecycle routing; do not assume an enum variant alone is enough. |
| `src/calculation_pad/examples.rs`, `src/calculation_pad/README.md` | The in-app examples already have a pinned search, grouping, copying, expected results, and executable validation; the current catalog test checks 74 examples. | Extend the existing searchable catalog. Update meaningful expectations/counts instead of creating a duplicate documentation browser or weakening tests. |
| `src/date_arithmetic.rs`, `src/date_arithmetic/studio.rs`, `src/date_arithmetic/zones.rs` | Deterministic calendar arithmetic, month-end clamping, named zones, explicit fold choices, and rejected nonexistent local times exist. | Reuse these domain semantics. Do not silently resolve DST ambiguity differently inside Calc Pad. |
| `Cargo.toml`, `Cargo.lock` | egui/eframe 0.36.2, `chrono`, `chrono-tz`, `image`, and `arboard` are present. `num-bigint` 0.4.6, `num-rational` 0.4.2, and `num-traits` 0.2.19 are already in the lockfile. | Use the current GUI. Exact arithmetic may promote the already-locked numeric crates to direct dependencies; no broad dependency update or second UI framework. |
| `src/file_search/model.rs`, `src/file_search/settings.rs` | Requests contain a content-scan maximum size, matching/type/extension criteria, but no general modification-time/size predicate. UI preferences have serde defaults. Filename results already expose optional size/modified metadata. | A new result filter is separate from the content-read safety cap. Unknown metadata cannot become zero. |
| `src/gui/file_search_dialog/filters.rs` | `build_search_request` composes criteria; `filters_changed_since_last_search` compares against the submitted request; changing filters does not automatically search. | Keep semantic relative-date choices separate from timestamps resolved at submission, or the dirty comparison will drift on each render. |
| `src/file_search/walkdir.rs`, `native.rs`, `ripgrep.rs`, `everything.rs`, `executor.rs`, `coordinator.rs` | Multiple backends enforce limits at different stages. Everything builds an ES `-n max_results` request before local post-filtering. Ripgrep groups/caps matched files in its stream parser. | Metadata rejection must happen before the accepted-result cap. Post-filtering an already capped result list is not a correct implementation. |
| `tests/suites/domain.rs`, `tests/domain_cases/file_search_e2e.rs`, `file_search_state.rs` | File Search integration cases are included in the `domain` test target. Many Calc Pad tests are module-local library tests. | Select actual Cargo targets/module filters; do not assume each case file is a separate test binary. |

## C. Functional and compatibility contract

### C0. Shared evaluation, source, and persistence rules

**One source of truth.** The document text is authoritative for values. Control drafts, tables, charts, and persisted presentation metadata are not independent variable stores. A successful Apply edits the original assignment; an analysis run does not.

**Identity and freshness.** Every derived result/action carries document ID, document incarnation, source revision, and an evaluation/configuration generation sufficient to distinguish runs. Changing documents, editing source, undoing, changing semantic options, canceling, closing, or replacing an analysis request must retire incompatible work. Saving as a new document must not leave old-document actions able to commit.

**Results and status.** Distinguish not calculated, outdated, queued/running, completed, canceled, and infrastructure failure. A completed evaluation containing ordinary line diagnostics is not an infrastructure failure. Retained old results remain explicitly labeled and tied to their original source. Old offsets cannot navigate/replace the new source. A canceled job cannot publish a partial document evaluation as current.

**Bounded work.** Retain shared cancellation/work limits and enforce new allocation/operand limits before expensive work. Count nested function calls, branches, exact-number arithmetic, and sensitivity scenarios. Expensive supported work must not run synchronously merely because it lacks the old `for` marker. Ordinary small legacy calculations should retain their inexpensive path.

**Versioning.** Introduce a deliberate new saved-document version after the current v2, with explicit v1/v2 migration and conservative defaults: Automatic, Standard, legacy arithmetic unchanged, no analyses/chart definitions, existing selected inputs/watches retained. Strictly reject unsupported future versions without rewriting them. New optional approved metadata fields must have deliberate defaults and validation. Keep source exactly, retain stable IDs and monotonic ID allocation, and preserve atomic writes/external-modification protection.

**Recovery.** Introduce a compatible reader for legacy source-only recovery and a newer recovery form that includes at least the document size profile and calculation policy. Restore a dirty scratch document, not an overwrite of a named saved document. Preserve existing debounce/clear/shutdown barriers. Full persistence of undo history or computed results is not required.

**Related-metadata transactions.** A rename must atomically update source and applicable selected input/watch/analysis/chart references. One Undo/Redo reverses/reapplies that transaction. Do not make undo revert a Save As document identity, reset the line-ID allocator, or roll back unrelated later preferences. Ordinary display preference changes do not need a new global undo system.

### C4. Sensitivity Tables

Place an independently hideable, initially collapsed **Sensitivity Tables** section inside **Calculation Pad → Config Menu**. Keep its controls/results scrollable and bounded; collapse/hide does not delete the configuration.

Required controls: one eligible top-level literal input selected by stable declaration/line identity; Start, End, and either Step or Sample Count; one or more selected named outputs/watches; Generate and Cancel; Copy Table, Copy CSV, and Copy Markdown. Persist one active definition with a named document; computed rows are transient and bounded.

- Step must be nonzero and point toward End. Include endpoints only when reached without overshooting. Generate from an index/count, not indefinitely accumulated floating-point addition. Sample Count includes both endpoints; use one case when Start equals End. Reject fractional samples for fixed-width integer inputs rather than rounding silently.
- Target the chosen assignment, not every textual occurrence or every later reassignment of its name. Preserve its constructor and genuine unit semantics. Later assignments still behave normally. Varying a quantity means varying its magnitude in that input's existing unit; a decorative suffix is not a unit conversion.
- Evaluate private immutable input overrides/copies. Never mutate the active document, its undo history, its cached evaluation, or its saved source to simulate scenarios. Changing the definition may dirty metadata; pressing Generate alone must not dirty the document.
- Use the same expression engine, pure functions, numeric rules, and limits as ordinary evaluation. Default caps: **1,000 cases, 8 output columns, and 1,000,000 aggregate work units**, in addition to each case's normal work/allocation caps. Check cardinality before allocating. Show progress; cancellation does not publish a complete table.
- Invalid cases remain rows with per-output diagnostics, including division by zero and missing/unsupported outputs. A bad case must not silently disappear or shift X/Y alignment. Supported output columns are scalar, exact numeric, fixed-width integer, and consistently dimensioned numeric quantities. Explain unsupported collections/calendar results rather than coercing them.
- Changing source, semantic settings, selected inputs/outputs, or analysis settings marks previous data outdated. In Manual mode, Generate is an explicit independent analysis action against the current source and does **not** mark the main Pad's previous evaluation current. It never triggers a hidden main calculation.
- Copy Table uses tab-separated cells; CSV quoting is correct; Markdown escapes delimiters/newlines. Labels include input/output names and applicable units. Preserve exact values in exports; do not route exports through chart floating-point conversion. Disable copying outdated/incomplete tables with a clear explanation.
- A completed table is available as a C5 data source. Charts do not implicitly regenerate sensitivity data whenever a source key is pressed.

Acceptance fixture: with `ore = 1000`, `xp = ore * 56.2`, and `remaining = 1000000 - xp`, vary the selected `ore` assignment from 1000 to 5000 by 1000. Expect five rows, displayed XP from 56,200 through 281,000, and remaining XP from 943,800 through 719,000. Use appropriate floating-point test tolerance; do not assert that binary arithmetic is exact. Source and source undo count remain unchanged.

### C5. Simple Charts

Add **Calculation Pad → Config Menu → Tools → Charts…**. Use a separate resizable companion dialog with the existing dock/pop-out, remembered size/presentation, parent ownership, focus, and close conventions.

- Support Line, Bar, and Scatter. Select a current numeric vector, X/Y vectors, or a completed C4 table. A single vector defaults to element positions **1 through N** on X. A sensitivity input supplies X; selected numeric outputs supply separate Y series. Bar labels may be edited in the dialog without adding plotting syntax to the language.
- Allow type changes, per-series visibility, axis labels/units, point-value tooltips, pan/zoom where appropriate, Reset View, and Copy Chart as Image. Keep one saved chart definition per document and up to 8 visible series; do not introduce a chart dashboard or global chart library.
- Require compatible vector lengths and compatible axes/units. Handle empty, singleton, constant-valued, negative, and extreme-range series. Invalid sensitivity cells create explicit gaps or excluded-point diagnostics, never invented zeros or interpolation across an unmarked error.
- Retain exact source values for tooltips/table export. Plot coordinates may be approximate, but disclose approximation and reject overflow/non-finite conversion. Do not collapse materially different exact values into an apparently precise plot without warning.
- Render from committed result data. Current new evaluations update vector charts; newly generated table data updates sensitivity charts. Outdated source/data shows an unmistakable banner; Copy Image is unavailable until data is current. A chart selection cannot mutate inputs.
- Image export must render the chart, not take a desktop screenshot containing unrelated windows. Reuse existing image/clipboard error patterns, including `src/gui/qr_dialog.rs`. Use one plot model for screen and image output, bounded pixel dimensions, and the accepted action's generation. No repeated clipboard side effects from egui passes.

### C6. Better Input Controls

Extend the existing Editable Inputs selection and Apply mechanism. For each selected eligible numeric literal, allow a friendly label, description, decorative suffix, minimum, maximum, positive step, and preferred control: numeric field, field with step buttons, or bounded slider plus field.

- Defaults reproduce existing input behavior: source name, no decorative text/range, ordinary field, explicit Apply. Step buttons/sliders adjust the draft; Apply creates one source edit and one undo entry. Do not recalculate on every drag position through hidden source edits.
- Enforce integer/unsigned/fixed-width rules exactly; use decimal-safe editing for exact types. Preserve constructors, literal validity, units, comments, whitespace, and unrelated source. Disable sliders for unbounded, unrepresentable, or inappropriate ranges; retain the field.
- Validate min ≤ max, positive step, type compatibility, and entered value within bounds. Explain invalid configuration/value; never silently clamp 25,000 to 20,000. A current source value outside newly configured bounds is flagged, not rewritten.
- Reuse revision/incarnation/stable-ID and original-source checks. A stale control draft cannot edit a different or changed assignment. Duplicate/reassigned names remain separately selectable by source location. Deleted/nonliteral inputs remain visibly unavailable until the user resolves/removes them.
- Save control configuration with the named document; recover semantic document options as specified in C0. Hiding inputs must not clear values/settings. A suffix such as `ore` is decoration, not a physical unit. Date/time pickers are outside this Goal.

### C14. Variable-aware rename

Provide **Rename Variable…** in the source context menu and F2 when the Pad editor owns keyboard input. Resolve the symbol at a declaration or reference, show the old/new name and affected physical lines, validate, preview, then apply atomically.

- Use parsed symbol identity, not replace-all or runtime dependency edges alone. Inspect inactive branches and function bodies without executing them. Never change substrings (`count` inside `discount`), comments, constructor string literals, function call names, or physical `rN` references.
- The runtime's document-scope reassignments of the same name form the rename family. This includes current loop iterator/reassignment behavior where bindings survive the loop. Explicitly state that scope in the preview. Function parameters are separate local symbols; a local parameter rename does not rename a same-spelled document variable or another function's parameter. Do not introduce new runtime scoping semantics.
- Include compound-assignment targets and references, Unicode/Greek/braced names, and all dependent metadata references. For ambiguous or malformed syntax where safe resolution cannot be established, explain and refuse rather than guess.
- Reject invalid/reserved/conflicting names and name capture before Apply. Preserve bracing or add required bracing without changing canonical identity. The full source-plus-related-reference change is one Undo/Redo transaction. Saving/reopening after rename retains valid references.
- Revalidate the document incarnation/revision before applying the preview; an edit or document switch invalidates it. Refresh completion/dependencies through normal revision handling, not a parallel mutable cache.

### C18. Pure user-defined formula functions

Support top-level declarations of the form `fn discounted_price(price, discount) = price * (1 - discount)`. The body is one logical expression and may use C20 grouping across physical lines.

- Definitions are available only after their declaration. Parameters are local to the call; no document-variable capture or writes. Bodies may use parameters, constants, existing pure built-ins, and previously declared pure user functions. Pass external values explicitly.
- Support the engine's applicable scalar, Boolean, numeric exact, quantity, vector/sequence, and calendar values through ordinary type rules; unsupported operations produce normal diagnostics. No string-manipulation language is introduced by constructor literal support.
- Require unique parameter names and unambiguous function names; reject collision with existing callable built-ins/constructors and duplicate function declarations. Do not globally reserve previously assignable variable words merely because a new callable exists. Preserve existing bare-variable and braced-name behavior.
- Bound definitions and calls: initial caps **64 functions, 16 parameters per function, call depth 16**, shared work budget, and existing expression/operand limits. Reject direct/indirect recursion, forward function references, and wrong arity. Never reset work/cancellation per call.
- Definitions are explicit non-value statements; referencing their `rN` row must explain that it has no value. A call's dependency information accounts for the definition, its arguments, and invoked definitions as appropriate. A failed definition/call cannot poison unrelated subsequent valid lines beyond existing dependency rules.
- Add user function names and signatures to completion/hints. `discounted_price(100, 0.10)` evaluates to 90; repeated calls do not modify a document-level `price` variable.

### C20. Inline comments and multiline expressions

Keep existing full-line `#` and `//` comments. Add inline `#` and `//` comments outside braced names and supported quoted constructor literals. Preserve source bytes/line endings; do not use naive string splitting.

Continue an expression while `(` or `[` grouping remains open. Respect quoted text, escapes, and braced variable names; braces used for names are not continuation delimiters. No backslash continuation syntax is needed. Indentation inside grouping is formatting, not a new loop/block.

- Preserve one result/status row per physical line. The expression's value belongs to its starting physical line. Continuation rows visibly identify their owning statement and have no independent value. `rN` remains physical and backward-only; referencing a continuation line gives a specific diagnostic naming the owning start line rather than silently rebinding.
- A logical expression, including functions/control headers where appropriate, still has bounded expression bytes, nesting, identifiers, and mapped source spans. Do not let splitting text bypass limits. Report meaningful physical locations for malformed/unclosed grouping or literal escapes.
- Update highlighting, matching delimiters, folding, completion, hints, editable-input discovery, dependencies, navigation, find/replace, and rename using the same source map. Preserve CRLF, Unicode offsets, blank/comment rows, and folded hidden text.
- Preserve the original behavior of already-valid single-line scalar, programmer, quantity, and vector documents. Inline comments and multiline syntax must work whether or not a `for` loop is present.

### C21. Everyday numeric helpers

Add these names to the real runtime/signature/example catalogs, with call-position behavior consistent with existing built-ins:

| Function | Contract and example |
|---|---|
| `round_to(value, step)` | Nearest positive increment; ties away from zero, consistent with the existing scalar `round` rule. `round_to(137,25)` → 125. |
| `floor_to(value, step)` | Toward negative infinity by positive increment. `floor_to(137,25)` → 125. |
| `ceil_to(value, step)` | Toward positive infinity by positive increment. `ceil_to(137,25)` → 150. |
| `clamp(value, min, max)` | Inclusive bounds; reversed bounds are invalid. `clamp(125,0,100)` → 100. |
| `percent_change(new, old)` | `(new-old)/old`; zero old is an error. `percent_change(120,100)` → 0.20, not 20. |
| `percent_of(part, total)` | `part/total`; zero total is an error. `percent_of(25,100)` → 0.25. |
| `choose(n,k)` | Nonnegative integer arguments, 0 ≤ k ≤ n; exact combinatorial result within bounds. `choose(10,2)` → 45. |
| `permutations(n,k)` | Same argument domain; exact ordered count. `permutations(5,2)` → 20. |
| `lerp(start,end,t)` | `start + (end-start)*t`; extrapolation is allowed, not silently clamped. `lerp(10,20,0.5)` → 15. |
| `map_range(value,a,b,c,d)` | `c + (value-a)*(d-c)/(b-a)`; equal input endpoints are an error; reversed ranges/extrapolation are allowed. `map_range(50,0,100,0,10)` → 5. |

Provide clear type/finite/overflow errors. Use type-preserving exact paths where an exact operand selects them; never silently approximate. Combinatorial results must remain exact beyond 2^53 (an exact rational with denominator one is acceptable). Cap work/intermediates before allocation; do not compute unbounded factorials. Existing physical-unit behavior remains unchanged; unsupported dimensional helper combinations must fail clearly, not strip units.

Extend the **existing** searchable Examples & Reference catalog with rounding, percentages, combinatorics, and interpolation examples. Search/filter already exists; do not build another browser.

### C22. Dates and times inside calculations

Add typed calendar values rather than disguising dates as floating-point numbers or reinterpreting existing physical time quantities.

Initial constructor surface: `date("YYYY-MM-DD")`, `time("HH:MM[:SS]")`, `datetime("RFC3339 timestamp with explicit offset")`, and `zoned_datetime("local ISO date-time", "IANA/Zone"[, "earlier" | "later"])`. Quoted literals are bounded constructor/function arguments, not an invitation to add general scripting strings or interpolation.

- Provide `add_days`, `add_months`, `add_years`, `days_between`, weekday lookup, elapsed-duration difference, and adding a physical time duration to a time/timestamp. Final helper names/signatures beyond those explicitly named must be documented consistently in the shared catalog; naming is an implementation detail, not a new product questionnaire.
- `days_between(start,end)` on calendar dates returns signed calendar-day count. `date("2026-11-15")` plus 45 days is December 30, 2026; the difference is 45. Weekday results are readably named; do not display an unexplained internal integer.
- Calendar month/year shifts use the existing date domain's month-end clamping. A calendar day shift on a zoned timestamp preserves local wall-time intent; an elapsed 24-hour duration changes the instant by exactly 24 hours. Reuse named-zone fold/gap handling: an ambiguous target requires an explicit earlier/later choice; a nonexistent target is a diagnostic, never an undocumented shift.
- Time-only duration addition must not silently lose a crossed date. For this version, reject a result outside the same day with an instruction to use a timestamp. Timestamp arithmetic handles date changes normally.
- Elapsed differences produce the existing compatible physical time quantity representation where possible, not a calendar-month approximation. Reject incompatible date/time kinds explicitly. Leap days, negative differences, out-of-range values, and explicit offsets are covered.
- Display recognizable dates/times/zones; canonical copy is a parseable constructor/expression preserving the represented kind, instant, and necessary zone/fold information. Support assignments, function arguments, conditions where meaningful, watches, and standard session behavior.
- Reuse `chrono`, `chrono-tz`, and the existing date arithmetic helpers. Do not read the clock from the pure evaluator or add polling. Existing `5 min`, `2 h`, unit conversion, and scalar arithmetic keep their semantics.

### C24. Manual calculation mode

Add **Config Menu → Calculation → Automatic / Manual**, remembered per named document. Automatic is the default for new/legacy documents. Manual exposes a prominent Calculate action; active expensive work exposes Cancel. No new global hotkey is required.

- Editing, applying inputs, renaming, undoing, redoing, opening companions, or repainting in Manual mode does not initiate a calculation. Results become visibly outdated or not calculated.
- Calculate evaluates the current source once through the ordinary engine. Coalesce repeated requests; an explicit new request has its own generation even at the same source revision. Cancel retires active work and retains last completed results only as stale.
- Switching to Manual does not discard source/results; retire incompatible queued automatic work. Switching to Automatic schedules the current dirty source once and resumes ordinary behavior.
- Dependencies, watches, Charts, comparison selection, and Programmer Inspector respect the same freshness state. Read-only old results can be displayed with their own source basis; source navigation/insertion from stale data is disabled. Do not return a stale result from an API documented to return a current source/result pair.
- No new idle timers, repeated polling/repainting while Manual is idle, or one worker per view. Existing cheap scalar behavior remains cheap in Automatic mode.

### C25. Exact decimal and fraction arithmetic

Introduce explicit `decimal("0.1")` and `fraction(1,3)` constructors and typed exact values. Use already-locked bigint/rational facilities where appropriate; promoting them to direct dependencies is permitted after verifying their current use. Do not implement arbitrary-precision integer arithmetic from scratch.

- Decimal addition, subtraction, multiplication, comparison, and finite decimal division preserve base-10 exactness. Fractions normalize signs, reject zero denominators, reduce to lowest terms, and support the ordinary four arithmetic operations and comparisons.
- Ordinary `/` on exact decimals with a nonterminating decimal result returns a useful diagnostic rather than silently rounding. Provide an explicit precision-controlled decimal division/conversion operation, such as `decimal_div(a,b,places)`, using documented half-even rounding. A rounded result is visibly identified as rounded; its stored decimal value is exact for the chosen rounded value, not a claim of exactness to the original quotient.
- Exact decimal plus fraction promotes to an exact fraction. Fixed-width/exact-base integers may convert losslessly when combined with an exact operand; their original arithmetic rules remain unchanged when no new exact operand is involved. A legacy scalar can participate implicitly only when it is a finite integral value within the existing exact-scalar-integer range. Nonintegral scalar mixing requires an explicit exact constructor or explicit approximate conversion; do not stringify an f64 to pretend its original input was decimal-exact.
- Provide an explicit approximate conversion for unsupported floating operations (for example `to_float`). Make conversion/rounding visible in result provenance/presentation. Never silently send exact arithmetic through the existing f64 evaluator.
- Support Exact, Decimal, and Mixed Number display where appropriate. A repeating decimal view is explicitly approximate/rounded, while canonical copy retains the exact constructor/fraction. `fraction(7,4)` may display `7/4`, `1.75`, or `1 3/4` without changing the underlying value.
- Initial hard safety ceilings: **4,096 bits per rational numerator/denominator or decimal coefficient; decimal scale and requested fractional precision at most 1,000**. Validate literals/exponents and estimate growth before allocating/multiplying/powering. Share work/cancellation. These are ceilings, not a promise to support every mathematically expressible operation beneath them.
- Required examples: exact `decimal("0.1") + decimal("0.2")` → 0.3; `fraction(1,3) + fraction(1,6)` → 1/2; large exact integer combinations beyond 2^53; rejected zero denominator and nonterminating unrounded decimal division.
- Existing `0.1 + 0.2`, `1 / 3`, fixed-width overflow/bitwise behavior, percentage display, quantity semantics, and existing documents retain their original rules.

### C26. Extended documents and visible limits

Add a per-document **Standard / Extended Document** setting and a compact expandable Document Usage indicator. Standard retains its existing evaluation limits. Initial Extended targets are **256 KiB source and 2,048 physical lines**. Measure them once with deterministic representative documents; retain them only if scoped responsiveness/memory checks support them. A lower validated Extended ceiling is acceptable only if still meaningfully above Standard and explicitly reported; do not silently raise all limits.

- Keep expression bytes/nesting, vector/range cardinality, loop iteration/nesting, dependency limits, and work budgets independently visible and bounded. New function/exact-arithmetic/analysis ceilings apply in both profiles. The Extended profile does not automatically increase computational budgets.
- Source admission, source evaluation, and serialized-file safety caps are distinct. Preserve text that exceeds the selected profile but remains within the absolute supported source ceiling; show an over-limit diagnostic and offer the appropriate profile choice. Such source can be kept/saved without pretending it evaluates successfully. Do not truncate to the selected profile or silently switch modes.
- Reject an edit/import beyond the absolute source ceiling atomically, keeping the previous complete document and explaining that the incoming content was not applied; never accept a partial prefix. Preserve rejected incoming content when the existing import/edit mechanism permits safe recovery. Do not claim preservation of bytes that were actually discarded.
- Set explicit serialized envelope limits sufficient for maximum permitted source after JSON escaping, line IDs, and bounded metadata; **4 MiB per saved document/recovery record** is the initial hard ceiling. Validate bytes before deserialization and semantic fields after it. Never make loading unbounded.
- Update all evaluator, session edit, input replacement, store, recovery, worker request, history, and editor assumptions consistently. Retain bounded undo memory (currently 2 MiB/64 entries) rather than multiplying it blindly; disclose reduced retained history for large edits.
- Show usage and approaching-limit warnings (initially at 90%) with distinct source-size/physical-line and evaluation-resource categories. Preserve responsive editing, folding, searching, scrolling, copying, and save/recovery. Extended calculations use bounded background evaluation even when they contain no old-style loop marker.

### C27. Conditional blocks and loop control

Add indentation-based `if condition:`, `elif condition:`, `else:`, and standalone `break`/`continue`. Keep expression `if(condition,a,b)` unchanged.

- Conditions require Booleans and use lazy branch selection. Unselected bodies do not execute; their physical rows show not executed. Parse structurally malformed statements even if a branch would be skipped, but do not evaluate skipped arithmetic such as division by zero.
- Use the current space-indentation rules; reject tabs/misalignment, orphan `elif`/`else`, missing bodies, and flow control outside a loop. New keywords are contextual: do not invalidate existing assignable names except where the full new statement grammar is matched.
- Assignments in selected branches follow document/loop binding semantics. A name assigned only in an unselected branch is not a new successful binding; a previously existing value remains available where current semantics permit. Do not introduce implicit branch-local variables.
- `break` exits only the nearest loop and is successful control flow, retaining work completed before it. `continue` skips the remainder of that iteration. Early exit must not be modeled as an evaluation error or trigger error rollback.
- Genuine errors/cancellation/work exhaustion retain existing failed-loop transaction guarantees; nested failures cannot publish inconsistent partial bindings. Preserve loop nesting/iteration accounting and do not let continued/broken iterations evade it.
- Update folding, indentation, highlights, completion, dependencies, and diagnostics. No `while`, general exceptions, statement-bodied functions, or global side effects.

### F3. File Search modification-date and size filters

Add independently resettable, initially collapsed **Date Modified** and **File Size** subsections to **File Search → Filters**. Preserve current search-button behavior; changing filters marks criteria changed but does not start a search.

**Dates:** Any, Today, Yesterday, Last 24 Hours, Last 7 Days, Last 30 Days, and inclusive date-only Custom Range. Today/Yesterday/Custom use local calendar boundaries. Last 24 Hours/7 Days/30 Days are rolling elapsed windows ending at the captured submission instant; label them clearly. Compile date-only ranges into start-inclusive/end-exclusive instants through the start of the day after the selected end date, not `23:59:59` guesses. Check invalid/reversed/out-of-range dates and timezone-boundary failures.

**Sizes:** Any, Empty (exactly zero), Smaller Than (strict `<`), Larger Than (strict `>`), Between (inclusive bounds). Offer B/KB/MB/GB and KiB/MiB/GiB with explicit decimal/binary multipliers. Parse entered decimals to checked integer-byte comparisons; no f64 rounding of large u64 file sizes. Invalid/negative/overflow/reversed bounds prevent Search with a readable error. Reject fractional-byte boundaries rather than silently rounding them.

**Composition and state:** AND date/size with current filename/content, extension, directory, hidden/type, scope, and match-mode criteria. Show the two new active-filter indicators outside the collapsed subsections and a clear reset for each/both. Persist semantic filter choices alongside existing UI preferences with defaults disabled. Do not persist resolved rolling-window timestamps as the next search's criteria.

**Submission time:** Capture the clock/local timezone once when Search is submitted. Preserve that resolved window for the whole search, including fallback and paging. Compare semantic criteria to detect UI changes; never recompute `now` during `filters_changed_since_last_search`, nor label unchanged controls dirty because time has passed. A new deliberate Search recomputes the rolling window.

**Directories and metadata:** A size filter applies only to files. Keep the user's normal type preference stored, visibly apply Files Only while size filtering is active, and restore the effective ordinary preference when it is cleared; do not use directory metadata length as recursive folder size. Date-only filtering may include directories when the ordinary type filter permits them, without pruning traversal merely because a parent directory fails its own date predicate. Unknown required metadata excludes the entry with an honest bounded diagnostic/count, not zero/epoch substitution. With both metadata filters disabled, preserve prior behavior.

**Backends and caps:** Apply the predicate before accepted-result counts/truncation in WalkDir, native content, ripgrep filename and content paths, and any indexed path used. In particular, do not filter Everything's already `-n`-capped output or ripgrep's already capped matched-file collection. For this bounded implementation, **route metadata-filtered filename searches that would use Everything to the existing WalkDir backend**, with a visible diagnostic explaining the filesystem-backed filter path. Unfiltered searches retain normal Everything behavior. Do not add an unbounded ES query or speculative CLI syntax. This may be slower than indexed search and must be disclosed in help/final reporting, not disguised as a failure or benchmarked into an unrelated project.

For ripgrep content, apply file eligibility in the streaming grouping path before accepted-file accounting; metadata failures must not turn parsing into a silently empty successful search. Preserve bounded stream storage, process cancellation/cleanup, and stderr handling. The filtering need not avoid every underlying ripgrep read, but rejected files cannot consume the result limit.

**Content cap:** `max_file_size_bytes` remains the independent content-reading safety cap. Do not raise or disable it to satisfy a result filter. Reject impossible size-filter/content-cap intersections with an explanation; warn when only part of a requested size interval can be scanned. Filename searches have no such content-read restriction. Explain that an Empty-files filter cannot produce content matches for a nonempty text query.

**Required evidence:** deterministic boundary tests with fixed clocks and metadata; a result cap of one with a rejected candidate first and an eligible candidate afterward; missing metadata; directory traversal; mixed existing filters; rolling/calendar windows across DST; backend fallback using the same resolved request; old preferences loading disabled defaults.

## D. Scope boundaries and regression invariants

Implement the required surfaces fully; do not replace difficult acceptance criteria with placeholders or silently defer a core feature. The optional exclusions in Section A are the only planned product deferrals.

| Invariant | Required preservation |
|---|---|
| I1 — Existing language | Existing valid scalar/programmer/vector/quantity syntax, reassignment, compound assignment, lazy expression `if`, contextual aliases, finite-number checks, and diagnostics retain their meaning. New contextual keywords do not invalidate ordinary existing variables. |
| I2 — Physical source | Original source/line endings remain authoritative. `rN` means an earlier physical row, never a renumbered logical statement. Stable line IDs and folded-source projection remain coherent. |
| I3 — History and saving | Undo/redo, Save/Save As, dirty prompts, atomic storage, external-change checks, line-ID non-reuse, and recovered scratch ownership remain correct. Rename restores its related metadata in the same transaction. |
| I4 — Current versus old data | No retired worker, preview, chart, inspector, dependency navigation, or analysis completion can commit into another source revision/incarnation/run. Stale data is never presented as current. |
| I5 — User input and hosts | Main launcher query remains interactable with popped-out tools. Existing focus routing, Escape ownership, menu behavior, hotkeys, Jump Mode, and dock/pop-out state remain intact. Do not add a global F2 binding. |
| I6 — Responsiveness | New analysis/exact/extended work is bounded/cancelable and cannot hold the UI thread indefinitely. No hidden continuous calculation, polling, or clock refresh while idle. |
| I7 — Search | Existing filename/content matching, ranks, highlighting, extension/exclusion behavior, root scopes, result actions, preview, sorting, cancellation, and search IDs remain unchanged except the explicitly enabled new predicates. |
| I8 — No unrelated work | No workspace features; UI migration; release-profile project; unrelated test repair, cleanup, or vendor edits; additional search query language; live automation; or broad framework replacement. |

Changing public/internal signatures is permitted where the feature actually requires it, but migrate every affected call site and preserve unrelated semantics. Do not leave an old evaluator or UI path that silently ignores the new settings.

## E. Engineering strategy and dependency order

Use the existing architectural layers: bounded pure calculation domain; app-owned session/state/storage; coalescing asynchronous work; then the existing GUI/utility-host effect pipeline. Date/time operations reuse their current domain owners. Metadata filtering lives in File Search's request/executor layer, not in the displayed result table.

**Proposed additions, not existing files:** small modules such as `src/calculation_pad/syntax.rs`, `exact.rs`, `dates.rs`, and `sensitivity.rs`, and `src/gui/calculation_chart_dialog.rs`, may give the new responsibilities clear ownership. A similarly scoped File Search metadata-filter module is appropriate. The implementer may choose equivalent idiomatic names; do not create a generic plugin framework, public scripting API, or new crate merely to satisfy this plan.

A shared source-analysis model should retain physical spans, logical statement boundaries, protected comments/literals, declaration/reference identity, and block structure. Reuse existing scalar/programmer/vector/quantity evaluators behind it; this is not authorization to replace `exmex` or redesign all arithmetic. Extend the current source-routing decision so documents containing functions or conditional blocks without loops are still evaluated correctly.

The session owns semantic options, run generations, current/stale snapshots, and atomic source transactions. Read-only companions consume immutable session-owned data and return guarded actions. Separate sensitivity request identity from main evaluation identity without creating competing ownership of the source or one worker per chart. Use bounded requests/results and retire work on lifecycle transitions.

Document metadata stores bounded input settings, selected watch references, calculation/profile choices, sensitivity definition, and chart definition. Derived result tables/images/evaluation caches are not saved. Explicit version migration and safe recovery are requirements, not optional polish.

**Execution order:**

`M0 bootstrap/baseline → M1 logical source → M2 session/persistence/manual mode → M3 exact values → M4 helpers → M5 functions/control flow → M6 dates → M7 Extended profile → M8 rename → M9 inputs → M10 sensitivity → M11 charts → M12 file filters → M13 documentation/review/handoff`.

F3 is architecturally independent but stays in the same planned sequential execution. M8 consumes the final lexical/function/block semantics, M10 consumes the input/type/session foundations, and M11 consumes current evaluation/analysis publications. Do not repeatedly reimplement those shared foundations inside later GUIs.

## F. Detailed milestones and checkpoint contracts

Every checkpoint uses the following fields as its implementer handoff. References to Section C include all of that feature's behavioral rules. Test/module names below refer to existing owners; add focused tests next to them rather than assuming proposed test names already exist. Commit only after checkpoint-appropriate evidence and diff inspection. A coherent checkpoint is not necessarily an independently released version.

### M0-B — Confirm the live baseline and bounded execution map

**Objective:** Establish actual current ownership and verification baseline after M0-A.  
**Code ownership:** `AGENTS.md`, `.codex/agents/*.toml`, relevant files in Section B, actual Cargo test targets, and the task ledger. No feature edits yet.  
**Tasks:** Reconcile source drift; map each requirement to owners; inspect callers of `CalculationValue`, `LineOutcome`, `EvaluationStatus`, storage validators, `SearchRequest`, and host keys. Record existing failures separately. Read configured planner/implementer/reviewer instructions; do not change their models.  
**Behavior:** No application behavior changes.  
**Edges:** Missing ancestry, unexpected dirty state, unavailable Windows execution, or baseline compilation failures must be classified precisely.  
**Regression constraints:** Do not repair unrelated failures, adopt old native acceptance campaigns, or fetch a different implementation base.  
**Verification:** Run/list only the targeted Calc Pad/File Search baseline appropriate to the current environment; inspect original example validation and representative legacy persistence/session cases. Use the budget in Section H.  
**Acceptance:** All thirteen IDs mapped; current capabilities and preexisting blockers recorded; required implementation path is executable.  
**Commit:** `chore(plan): [M0-B] record scoped baseline and execution map`, when meaningful ledger content exists.

### M1-A — C20 logical statements and physical source mapping

**Objective:** Support inline comments and grouped multiline expressions in the calculation domain.  
**Code ownership:** `document.rs`, `script.rs`, `loops.rs`, `mod.rs`, and a proposed shared source-analysis module.  
**Tasks:** Add protected lexical scanning and source mapping; feed logical expressions to existing evaluators; preserve physical outcomes; establish explicit non-value continuation/declaration handling needed by later features.  
**Behavior:** C20 works with and without loops; continuation references explain their non-value status.  
**Edges:** CRLF, Unicode/braced names containing comment characters, strings/escapes, nested grouping, missing delimiters, and logical-expression caps.  
**Regression constraints:** I1/I2; no source rewriting or logical renumbering.  
**Verification:** Focused document/script/loop tests plus representative scalar, quantity, integer, and vector cases; error spans map to physical bytes.  
**Acceptance:** Existing source fixtures retain values; comments/continuations pass in both routing paths; every physical row remains accounted for.  
**Commit:** `feat(calc-pad): [M1-A] add bounded logical source analysis`.

### M1-B — C20 editor and observer integration

**Objective:** Make the new source syntax usable without corrupting editor projections.  
**Code ownership:** `calculation_pad_editor.rs`, `calculation_pad_dialog.rs`, input discovery, dependency mapping, and affected formatter/inspector consumers.  
**Tasks:** Reuse source maps for highlights, matching/folding, completion/hints, input detection, go-to-source, and physical row rendering. Update all exhaustive outcome matches.  
**Behavior:** Continuation/folded lines remain editable and navigable according to C20.  
**Edges:** Edits adjacent to hidden ranges, combining Unicode characters, inline-comment literal inputs, and syntax errors while typing.  
**Regression constraints:** I2/I5; no fabricated result for a non-value row.  
**Verification:** Existing editor projection tests plus focused GUI/model tests for physical references, folds, and selected inputs.  
**Acceptance:** Typing/folding/copying preserves hidden text and line endings; no observer pairs a logical offset with the wrong physical source.  
**Commit:** `feat(calc-pad): [M1-B] integrate multiline editor mappings`.

### M2-A — Shared document options, versioning, and run identities

**Objective:** Establish the state/persistence boundary required by the approved features.  
**Code ownership:** `session.rs`, `worker.rs`, `persistence.rs`, `inspector.rs`, `comparison.rs`, `dependencies.rs`, and recovery hooks.  
**Tasks:** Add explicit calculation/profile options and run/configuration generation; migrate saved formats/readers; preserve legacy recovery and extend recovery options; introduce a narrowly scoped atomic edit-plus-related-metadata transaction seam. Add subsequent approved bounded metadata fields with explicit defaults as their features land.  
**Behavior:** Old documents load into conservative defaults; same-revision requests have distinguishable identity.  
**Edges:** Unknown versions, malformed metadata, failed atomic writes, Save As, older recovery, and canceled completion races.  
**Regression constraints:** I3/I4; strict validation, external-change detection, source bytes, ID allocation, and recovery barriers remain.  
**Verification:** v1/v2 migration fixtures, new round-trip fixtures, conflict/failed-save tests, and adversarial worker completion order.  
**Acceptance:** No unsupported-version overwrite; no stale run publication; approved defaults survive save/reopen/recovery.  
**Commit:** `feat(calc-pad): [M2-A] version document options and evaluation identity`.

### M2-B — C24 manual calculation and truthful observers

**Objective:** Complete Manual mode end to end.  
**Code ownership:** Session/worker scheduling, Calc Pad dialog, Dependencies, comparison/inspector contexts, and affected host routes.  
**Tasks:** Separate observation from scheduling; implement Calculate/Cancel/mode controls and status transitions; retain explicitly stale result/source snapshots; guard navigation/insertion/copy according to freshness.  
**Behavior:** C24; no calculations from manual edits or opening an observer.  
**Edges:** No prior result, cancel-before-start, mode switch during a run, rapid Calculate, same-revision reruns, close/reopen, and document switch.  
**Regression constraints:** I4/I5/I6; automatic small-document behavior and stale inspector protections remain.  
**Verification:** Evaluation-count tests, controlled worker races, persistence tests, and focused GUI interactions.  
**Acceptance:** Edit six inputs in Manual mode without six evaluations; Calculate produces one current run; canceled/old work never becomes current; idle observation does not schedule.  
**Commit:** `feat(calc-pad): [M2-B] add explicit manual calculation lifecycle`.

### M3-A — C25 bounded exact-number domain

**Objective:** Add exact decimal/fraction values and arithmetic without a floating-point detour.  
**Code ownership:** Proposed exact-number module, `document.rs`, `script.rs`, type dispatch, `Cargo.toml`/lockfile only as necessary.  
**Tasks:** Implement constructors, normalized arithmetic/comparison, explicit mixing rules, nonterminating-division diagnostics, rounding/conversion operations, and preallocation caps. Promote existing numeric dependencies rather than update the graph broadly.  
**Behavior:** C25 exactness and error contracts.  
**Edges:** Negative denominators/zero, exponent/scale abuse, integer width boundaries, huge intermediates, cancellation, and fractional f64 mixing.  
**Regression constraints:** I1/I6; existing scalar and fixed-width arithmetic unchanged.  
**Verification:** Exact equality tests, canonical normalization, rejected implicit approximation, caps, and representative legacy arithmetic.  
**Acceptance:** Exact 0.1+0.2 and 1/3+1/6 examples pass; no exact path routes through f64; oversized work fails boundedly.  
**Commit:** `feat(calc-pad): [M3-A] add bounded exact decimals and fractions`.

### M3-B — C25 display, copying, and tool integration

**Objective:** Expose exact values consistently throughout the Pad.  
**Code ownership:** `calculation_pad_format.rs`, document canonical formatting, dialog output controls, comparison/inspector adapters, input discovery, examples.  
**Tasks:** Add Exact/Decimal/Mixed views; preserve canonical exact copy; propagate rounded/approximate provenance; handle new types in watches and current-result observers. Reject unsupported programmer inspection clearly.  
**Behavior:** Display changes never alter calculation values; copied exact values evaluate back to the same value.  
**Edges:** Repeating decimals, negative mixed numbers, huge readable outputs, incompatible units, and unavailable integer-only actions.  
**Regression constraints:** I1/I4; existing formatter and bit-inspector semantics stay intact.  
**Verification:** Format/copy round trips, provenance tests, watch/session cases, and focused UI assertions.  
**Acceptance:** Exact values remain exact through session/cache/copy; approximation is visible; unsupported actions do not panic or coerce.  
**Commit:** `feat(calc-pad): [M3-B] expose exact result views and copy semantics`.

### M4-A — C21 numeric helpers and discovery

**Objective:** Complete all ten helpers and their documentation hooks.  
**Code ownership:** `script.rs`/function catalog, arithmetic dispatch, exact domain, editor hints, `examples.rs`.  
**Tasks:** Implement the C21 contracts with shared type checks; add bounded exact combinatorics; extend searchable categories, signatures, and executable examples.  
**Behavior:** Helpers work in assignments/expressions/current loops and are callable from later C18 functions.  
**Edges:** Negative rounding ties, zero/reversed bounds, nonintegral combinatorics, input range of zero width, extrapolation, and exact results above 2^53.  
**Regression constraints:** No duplicate implementations of existing helpers; no global reservation of previously valid variable names.  
**Verification:** One focused behavioral case per helper plus failure/type/boundary cases and existing function-hint tests.  
**Acceptance:** All ten table examples pass; large combinations are exact or explicitly bounded errors; existing examples search finds new content.  
**Commit:** `feat(calc-pad): [M4-A] add everyday numeric helper catalog`.

### M5-A — C18 function declarations and pure calls

**Objective:** Implement document-local pure expression functions.  
**Code ownership:** Shared syntax/binding model, `document.rs`, `script.rs`, worker dispatch, and function/dependency representation.  
**Tasks:** Parse top-level declarations; resolve parameter-local references and previously declared callable identities; implement bounded typed calls and dependency attribution. Ensure function-only documents enter the supported evaluation path.  
**Behavior:** C18, including grouped multiline bodies and no document-variable capture.  
**Edges:** Wrong arity, duplicate parameters/names, reserved callables, unknown/free variables, forward calls, recursion, depth exhaustion, and typed arguments.  
**Regression constraints:** I1/I2/I6; function calls cannot overwrite document or other call bindings.  
**Verification:** Discount example, nested prior functions, parameter shadowing, typed results, rejected recursion/capture, and shared-budget exhaustion.  
**Acceptance:** Calls yield correct reusable results; definition rows are non-values; legacy docs still run on their appropriate path.  
**Commit:** `feat(calc-pad): [M5-A] add pure document formula functions`.

### M5-B — C18 editor and reference support

**Objective:** Make user functions discoverable and maintainable.  
**Code ownership:** Source analysis, `calculation_pad_editor.rs`, dependency dialog/model, examples, and language README.  
**Tasks:** Add function completion/signature hints, declaration highlighting/folding, parameter scopes, and explanatory diagnostics; expose definition dependencies without inventing numeric row values.  
**Behavior:** Hints reflect the actual declaration visible at the caret.  
**Edges:** Incomplete declaration edits, same-spelled document variables/parameters, braced names, nested calls, and folded definitions.  
**Regression constraints:** Existing completion and contextual built-ins remain usable; no evaluation merely to populate hints.  
**Verification:** Editor/source-map cases and executable function examples.  
**Acceptance:** Function names/signatures update after edits; local parameters do not leak into unrelated scopes; references navigate only current source.  
**Commit:** `feat(calc-pad): [M5-B] integrate formula function editor tools`.

### M5-C — C27 blocks, break, and continue

**Objective:** Add structured conditions and correct non-error loop control.  
**Code ownership:** `loops.rs`, shared statement/binding model, `script.rs`, editor block handling, dependency/outcome adapters.  
**Tasks:** Extend statements to conditional chains and typed flow outcomes; preserve transaction handling for actual errors; support conditional-only documents; account for nested control flow in limits and source rows.  
**Behavior:** C27; nearest-loop break, skipped iteration remainder, lazy branches.  
**Edges:** Orphan branches, malformed/empty bodies, tabs, no-loop control statements, nested failure after earlier assignments, and empty ranges.  
**Regression constraints:** I1/I2/I6; successful early exit retains assignments, genuine loop errors retain rollback guarantees.  
**Verification:** Score 85 produces final score 95; first multiple of seven is 7; skipping even values in range(1,11) sums to 25; nested/rollback/skipped-error cases.  
**Acceptance:** Both new control paths and existing transactional loop tests pass; no `while` or statement functions were added.  
**Commit:** `feat(calc-pad): [M5-C] add conditional blocks and loop flow control`.

### M6-A — C22 typed calendar and elapsed-time operations

**Objective:** Add deterministic date/time values using existing domain semantics.  
**Code ownership:** Proposed Calc Pad date adapter, existing date arithmetic/zones modules only where reusable access is necessary, typed dispatch/function catalog.  
**Tasks:** Implement constructors and calendar/duration operations; preserve explicit offsets/zones/fold choices; enforce type/range rules and pure function interoperability.  
**Behavior:** C22, without live clock functions or silent time-only date loss.  
**Edges:** Leap years, month-end clamping, negative offsets, DST folds/gaps, incompatible kinds, timestamp overflow, and duration units.  
**Regression constraints:** Existing date tools and physical-unit expressions must not change semantics.  
**Verification:** Fixed-date fixtures, November 15 plus 45 days, month-end/leap cases, and one gap/fold/24-hour-versus-calendar-day set using the existing zone domain.  
**Acceptance:** Deterministic results, explicit ambiguity errors, and no evaluator clock access.  
**Commit:** `feat(calc-pad): [M6-A] add typed calendar and duration calculations`.

### M6-B — C22 presentation and language integration

**Objective:** Expose date/time results as usable Pad values.  
**Code ownership:** Value formatting/canonical copy, watches/comparisons, editor signatures, examples, session tests.  
**Tasks:** Add recognizable date/time/weekday output, lossless typed copying, supported comparisons, and type-specific error messages in unsupported tools.  
**Behavior:** Dates remain dates across assignment, functions, cache, and copy.  
**Edges:** Zone/offset preservation, multiple representations of the same instant, time-only overflow, and stale results in Manual mode.  
**Regression constraints:** No automatic numeric coercion or date-picker expansion.  
**Verification:** Constructor/canonical round trips, function/watch tests, and focused UI formatting.  
**Acceptance:** Copying a displayed timestamp preserves its meaning; calendar values do not break generic result rendering.  
**Commit:** `feat(calc-pad): [M6-B] integrate date results and reference examples`.

### M7-A — C26 end-to-end source-capacity profiles

**Objective:** Support larger documents consistently at all storage/edit/evaluation boundaries.  
**Code ownership:** Domain limits, session validation/history, persistence/recovery, worker requests, input replacement, and source editor admission.  
**Tasks:** Centralize Standard/Extended evaluation limits and absolute storage/admission caps; preserve over-profile source with diagnostics; update legacy/new loaders; route Extended work off the GUI thread.  
**Behavior:** C26, with Standard defaults and no computational budget multiplier.  
**Edges:** Byte versus character counts, JSON escaping expansion, exact-limit boundaries, >absolute-cap edits, stale profile work, and bounded undo.  
**Regression constraints:** I2/I3/I6; no partial truncation or unsupported old-file rewrite.  
**Verification:** Profile boundary tests through edit/save/open/recovery, escaped/Unicode source, old defaults, and background scheduling.  
**Acceptance:** A representative >256-line valid Extended document survives its full lifecycle; invalid oversized input never partially replaces source.  
**Commit:** `feat(calc-pad): [M7-A] add bounded extended document profiles`.

### M7-B — C26 visible usage and bounded responsiveness check

**Objective:** Make limits understandable and validate the selected Extended ceilings.  
**Code ownership:** Calc Pad usage/status UI, editor projection/rendering, targeted domain/session benchmark fixture or test harness.  
**Tasks:** Add compact/expanded usage, separate source/evaluation counters, 90% warnings, and profile control; measure representative near-ceiling edit/evaluation once in each relevant profile and one cancellation path.  
**Behavior:** A user can see which limit is approached/exceeded without losing source.  
**Edges:** Large folded documents, a small source with excessive work, and shrinking from Extended to Standard.  
**Regression constraints:** No broad benchmark initiative, new polling, or unbounded history.  
**Verification:** Deterministic capacity fixtures and one bounded before/after responsiveness/memory observation on the same machine/build; report measurements, not invented targets.  
**Acceptance:** Final ceilings are justified, visible, and consistent; source remains editable/saveable during an evaluation-limit diagnostic.  
**Commit:** `feat(calc-pad): [M7-B] expose document usage and validate capacity`.

### M8-A — C14 binding-aware rename planning and transaction

**Objective:** Implement a pure rename preview and atomic application boundary.  
**Code ownership:** Shared lexical/binding model, session edit/history, metadata references, and literal-input identities.  
**Tasks:** Identify document rename families versus function parameters; collect exact spans in active/inactive code; validate collisions/capture; produce a revision-bound preview; apply source and related metadata through one transaction.  
**Behavior:** C14, with preserved runtime loop/reassignment semantics.  
**Edges:** Duplicate/reassigned names, loop iterators, local parameter shadowing, braced Unicode, comments/literals, and malformed syntax.  
**Regression constraints:** `rN`, source IDs, unrelated names, Save As identity, and ID allocator are not rewritten or rolled back.  
**Verification:** Pure span/collision tests plus source-and-metadata undo/redo, stale preview, and save/reopen cases.  
**Acceptance:** `count` rename does not affect `discount`, comments, or unrelated parameters; one Undo restores all intended references.  
**Commit:** `feat(calc-pad): [M8-A] add scope-aware rename transactions`.

### M8-B — C14 rename interaction

**Objective:** Expose preview/apply/cancel safely in the editor.  
**Code ownership:** Calc Pad editor/dialog, existing effect/host receipts, completion and dependency refresh.  
**Tasks:** Add context action and editor-owned F2, current/new name, affected-line preview and errors; disable Apply on stale or invalid plans; route one accepted transaction.  
**Behavior:** Clear scope/count before changes; Cancel changes nothing.  
**Edges:** Popped-out editor versus launcher focus, document switch while open, folded target, and egui sizing/discard passes.  
**Regression constraints:** I4/I5; no global hotkey or unguarded second mutation.  
**Verification:** Focused GUI events and revision/host retirement tests, followed by the native interaction check in Section H when available.  
**Acceptance:** F2 only acts with appropriate editor focus; preview matches applied text; one action/undo works docked and popped out.  
**Commit:** `feat(calc-pad): [M8-B] add variable rename preview controls`.

### M9-A — C6 typed input configuration and validation

**Objective:** Add durable source-backed numeric input definitions.  
**Code ownership:** Input descriptors, session replacement validation, document metadata, and typed numeric helpers.  
**Tasks:** Support label/description/suffix/range/step/control metadata; validate configuration and typed drafts; preserve selected declaration identity across comments/multiline syntax and renames.  
**Behavior:** C6 defaults retain existing field/Apply semantics.  
**Edges:** Unsigned/large integers, exact decimals, physical quantities, invalid step, current value outside bounds, and missing/nonliteral assignments.  
**Regression constraints:** No alternate value store or silent clamping; constructors and units remain source-authoritative.  
**Verification:** Pure validation/replacement tests and save/reopen/rename compatibility.  
**Acceptance:** Legal metadata round-trips; invalid edits cannot mutate source; stale descriptors are rejected.  
**Commit:** `feat(calc-pad): [M9-A] add typed editable-input configuration`.

### M9-B — C6 input controls

**Objective:** Complete usable numeric fields, steps, and optional sliders.  
**Code ownership:** Existing Editable Inputs panel and guarded host actions.  
**Tasks:** Render friendly controls/descriptions, adjust local drafts, show precise errors/unavailability, and apply through the existing session transaction. Disable unsuitable sliders without losing the field.  
**Behavior:** Step/drag then Apply makes one source edit; Manual mode remains manual.  
**Edges:** Draft invalidation, integer endpoints, keyboard access, collapsible panels, and reassignments sharing a name.  
**Regression constraints:** I3/I5; hide/show does not clear settings; decorative units never convert values.  
**Verification:** Focused GUI/model cases for Apply/Undo, out-of-range rejection, automatic/manual modes, and persistence.  
**Acceptance:** Labels/controls work without changing formula semantics or leaking input to a non-owning viewport.  
**Commit:** `feat(calc-pad): [M9-B] add configurable numeric input controls`.

### M10-A — C4 immutable sensitivity execution

**Objective:** Generate bounded analysis rows without editing the live document.  
**Code ownership:** Proposed sensitivity domain/job module, shared evaluation context/budgets, session identity and typed input/output adapters.  
**Tasks:** Validate scenario cardinality; resolve one chosen declaration and named outputs; run immutable overrides on bounded work; retain per-cell diagnostics; publish completed results only to the matching analysis/source identity.  
**Behavior:** C4, including independent explicit analysis during Manual mode.  
**Edges:** Step/count endpoints, integer representability, later reassignment, invalid row, cumulative work exhaustion, cancellation, and replacement request.  
**Regression constraints:** Main source, main result cache, dirty source state, and source undo history remain unchanged by Generate.  
**Verification:** Five-row XP fixture, exact/unit cases, error-row preservation, generation races, unchanged source/history/cache, and aggregate limits.  
**Acceptance:** Correct rows or explicit diagnostics; no hidden main evaluation or stale publication.  
**Commit:** `feat(calc-pad): [M10-A] add non-destructive sensitivity evaluation`.

### M10-B — C4 panel, persistence, and export

**Objective:** Complete the requested Sensitivity Tables interaction.  
**Code ownership:** Calc Pad Config Menu, bounded metadata, analysis presentation and export helpers.  
**Tasks:** Add collapsed/hideable controls, progress/cancel, output selection, row/status display, and three copy formats; persist settings but not results. Prepare a read-only data adapter for Charts.  
**Behavior:** C4 placements/defaults and explicit stale/incomplete state.  
**Edges:** Reopen/rename/delete inputs, too many outputs, CSV/Markdown quoting, and large exact displayed versus copied values.  
**Regression constraints:** No source edits from viewing/export; no analysis on every UI frame; no duplicate clipboard effects.  
**Verification:** Metadata round-trip, export fixtures, stale copy gating, and focused UI state transitions.  
**Acceptance:** Generate/cancel/copy and hide/reopen work; settings survive; stale data cannot be exported as current.  
**Commit:** `feat(calc-pad): [M10-B] add sensitivity controls and table export`.

### M11-A — C5 chart model and bounded rendering/export

**Objective:** Provide line/bar/scatter rendering over typed committed data.  
**Code ownership:** Proposed chart data/rendering helpers, numeric conversion adapters, current image/clipboard infrastructure.  
**Tasks:** Build one plot model for screen/image; select vectors/table columns; implement series visibility, labels, tooltips, view reset, gaps, and bounded raster output. Prefer current egui painting primitives; do not add a second GUI framework.  
**Behavior:** C5 data/approximation/export rules.  
**Edges:** Empty/singleton/constant series, mismatched lengths/units, erroneous rows, extreme exact values, and render dimension overflow.  
**Regression constraints:** Source remains read-only; exact table/tooltips are not replaced by float-coerced values.  
**Verification:** Pure dataset validation and transform/raster bounds tests; inspect a representative exported chart image for correct labels and gaps.  
**Acceptance:** Three chart types render valid data; invalid data explains itself; image export contains only the chart and respects caps.  
**Commit:** `feat(calc-pad): [M11-A] add bounded chart models and rendering`.

### M11-B — C5 companion dialog and lifecycle

**Objective:** Integrate Charts with the existing application host system.  
**Code ownership:** Proposed `calculation_chart_dialog.rs`, `gui/mod.rs`, Calc Pad effects/runtime, `utility_host.rs` callers, utility key/size/presentation catalogs.  
**Tasks:** Add Tools entry, registered companion identity, dock/pop-out/resize state, current/stale publication, accepted Copy Image action, and close/document-switch retirement. Persist its bounded definition.  
**Behavior:** C5; no hidden sensitivity regeneration or main calculation from opening Charts.  
**Edges:** Owner retirement, Save As, source changes during export, hidden root, duplicate egui passes, and failed clipboard access.  
**Regression constraints:** I4/I5/I6; main query remains usable and independent companions retain their normal behavior.  
**Verification:** Headless lifecycle/action guards plus actual dock/pop-out/update/copy interactions when supported.  
**Acceptance:** Charts follows current data, marks old data stale, survives presentation changes, and never mutates the Pad.  
**Commit:** `feat(calc-pad): [M11-B] integrate dockable charts companion`.

### M12-A — F3 typed predicates and submission-time resolution

**Objective:** Define date/size filter semantics independently of UI/backend.  
**Code ownership:** `file_search/model.rs`, `settings.rs`, a proposed predicate module, request-building callers in GUI/plugin code.  
**Tasks:** Add semantic filter preferences and resolved request fields; compile with an injected/captured clock and local calendar resolver; implement checked byte-boundary parsing, predicate evaluation, and content-cap compatibility checks.  
**Behavior:** F3 date/size boundary contracts and disabled legacy defaults.  
**Edges:** DST/calendar ranges, fractional/overflow bytes, missing metadata, directory eligibility, and impossible content-size intersections.  
**Regression constraints:** Existing request matching remains unchanged when disabled; result filters do not alter the content safety cap.  
**Verification:** Pure fixed-clock/metadata boundary cases, serde defaults, semantic equality, and all affected `SearchRequest` construction sites compiling.  
**Acceptance:** Relative windows resolve once; equivalent controls compare equal across time; invalid filters prevent submission clearly.  
**Commit:** `feat(file-search): [M12-A] add typed date and size predicates`.

### M12-B — F3 backend correctness before caps

**Objective:** Apply filters consistently before accepted-result limits.  
**Code ownership:** WalkDir/native/ripgrep execution, Everything selection/fallback in executor/coordinator, diagnostic summaries.  
**Tasks:** Apply common eligibility before result accounting; extend ripgrep stream grouping with bounded file eligibility; route metadata-filtered Everything filename requests to WalkDir and explain it; preserve frozen request windows through fallback/cancellation.  
**Behavior:** Rejected files do not consume accepted-result caps; unknown metadata is explicit.  
**Edges:** Rejected-first/accepted-second with limit one, all-rejected streams, missing metadata, explicit file scopes, deleted files, and date-filtered directories with eligible descendants.  
**Regression constraints:** No cap increase, unbounded ES output, incorrect subtree pruning, or loss of subprocess cleanup/search-ID guards.  
**Verification:** Backend-focused unit/stream fixtures and relevant `domain` File Search cases; use deterministic fake metadata where necessary.  
**Acceptance:** Filtered and unfiltered paths are correct, truncation counts describe accepted matches, and fallback diagnostics are truthful.  
**Commit:** `feat(file-search): [M12-B] enforce metadata filters before result limits`.

### M12-C — F3 filter panel and persisted choices

**Objective:** Make date/size filtering visible and predictable.  
**Code ownership:** `gui/file_search_dialog/filters.rs`, dialog state, preference persistence, Jump Mode targets, diagnostics/help.  
**Tasks:** Add collapsed subsections, presets/custom range, units/comparators, independent reset and active indicators; mark semantic criteria changed without a search; capture resolution only on actual submission. Make effective file-only behavior and content-cap warnings clear.  
**Behavior:** F3 UI placement; repeated rendering never advances/changes a submitted window.  
**Edges:** Clearing filters, switching filename/content modes, stored Directories Only preference, restarting, invalid text, and offscreen/secondary UI passes.  
**Regression constraints:** Existing matching/exclusion/sort controls and keyboard targets retain behavior; no search on every keystroke.  
**Verification:** Request/GUI tests, old/new preference reload, clear/reset, active counts, and scoped native interactions.  
**Acceptance:** A user can reproduce each requested search combination and see why criteria or scanning limits exclude files.  
**Commit:** `feat(file-search): [M12-C] expose date and size filter controls`.

### M13-A — Documentation and cross-feature acceptance

**Objective:** Make the shipped behavior discoverable and prove the feature combinations.  
**Code ownership:** `src/calculation_pad/examples.rs`, its README, root `README.md`, existing File Search help surface, and focused integration/session/GUI tests.  
**Tasks:** Update the linked in-app reference first, then supporting READMEs; remove now-obsolete unsupported-syntax statements; add executable examples with expected results and resource/error examples. Verify C4+C5, C4+C6, C6+C24, C14+C18, C18+C20+C27, C22 typed arithmetic, and C24+C26. Explain exact-number conversion rules and filtered Everything fallback.  
**Behavior:** Documentation describes actual implemented syntax/options, not proposals.  
**Edges:** Misleading copied examples, old count assertions, constructor round trips, saved metadata after rename, and stale analysis during Manual mode.  
**Regression constraints:** Do not remove valuable legacy examples/tests to fit the expanded catalog.  
**Verification:** Run executable example validation and the focused affected subsystems; perform the native scenarios in Section H where supported.  
**Acceptance:** All thirteen IDs have concrete implementation and evidence entries; documentation/examples agree; unverified native behavior is explicitly marked.  
**Commit:** `docs(calc-pad): [M13-A] document and verify expanded calculation workflows`.

### M13-B — Independent review, final fixes, and handoff

**Objective:** Close substantive findings and hand off the exact cumulative branch tip.  
**Code ownership:** Full task diff, affected first-party code/tests/docs, and execution ledger only.  
**Tasks:** Invoke the configured read-only reviewer; fix relevant correctness/compatibility issues through the implementer; run one scoped final first-party Clippy pass and required affected rechecks; inspect staged paths, ancestry, commits, and actual push state.  
**Behavior:** No additional product scope.  
**Edges:** Preexisting warnings, unavailable native environment, push rejection, blocked required acceptance, and uncommitted partial work.  
**Regression constraints:** Do not suppress meaningful tests/lints, rewrite history, or manufacture native evidence.  
**Verification:** Review findings resolved or explicitly blocked; accepted checkpoints checked; ancestry and final checkout state recorded.  
**Acceptance:** Section J is met or final status is honestly partial/blocked/limited; final report contains actual `final_tip_sha` and next-goal instruction.  
**Commit:** Small `fix(...): [M13-B] ...` remediation commits as needed, then a factual reporting checkpoint if required. No ceremonial code commit. Attempt normal pushes after each created commit.

## G. Active checkpoint cadence and execution ledger

> Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.

The boundaries in Section F are the active cadence. M0-A is the mandatory first baseline commit, a direct child of `parent_tip_sha`. Commit coherent, appropriately verified increments on **the same shared Goal branch**. Do not make one branch per feature/checkpoint/agent, combine the entire language expansion into one final commit, squash existing history, or commit unrelated preexisting work.

Before each commit inspect the actual diff and staged paths. Record what passed and what remains, then attempt one ordinary push. Preserve successful local commits after push failure and continue without rebase/merge/pull retry loops. Additional small remediation commits are preferable to rewriting an earlier checkpoint.

Use the existing authoritative queue ledger when supplied. Otherwise the standalone status file may hold requirement status, active checkpoint, ancestry, committed checkpoints, verification commands/results, native evidence locations, blockers, and push state. Avoid competing ledgers. For a genuine exhausted stop, follow `AGENTS.md` and write a concise report under `docs/plans/execution/standalone/calc-pad-and-file-search/stalls/` (or the active queue's designated path), when tracked writing is safe. Do not paste private clipboard/source payloads or uncontrolled logs into reports.

**Final SHA self-reference rule:** Finish any tracked reporting commit first, then obtain the actual final `HEAD` and put it in the final response/authoritative external handoff as `final_tip_sha`. A commit cannot contain its own SHA. Do not make endless reporting commits trying to embed that same commit's hash inside itself. The final handoff must unambiguously distinguish the last recorded checkpoint from any later reporting commit and must identify the true required parent for the next Goal.

## H. Verification strategy and bounded budget

### H1. Focused automated checks

Start with real targets/module names. Typical existing filters are `calculation_pad::`, `gui::calculation_pad_dialog::editor::`, `gui::calculation_pad_dialog::`, `gui::calculation_dependency_dialog::`, `gui::programmer_inspector_dialog::`, and `file_search::` in the library, plus `file_search_e2e`/`file_search_state` cases in the `domain` target. Discover the actual names before selecting an exact test. The editor and formatter are path-included modules under `calculation_pad_dialog` (`editor` and `result_format`), not standalone GUI modules. A filter matching zero tests is not passing verification; correct the filter and record the actual executed cases. Include directly affected shared host tests when their production paths change, not every unrelated dialog test.

Representative commands, refined to the active checkpoint rather than run blindly at every stage:

```text
cargo nextest run -p multi_launcher --lib -E 'test(calculation_pad::)' --test-threads 1
cargo nextest run -p multi_launcher --lib -E 'test(file_search::)' --test-threads 1
cargo nextest run -p multi_launcher --test domain -E 'test(file_search)' --test-threads 1
```

Respect the repository's actual toolchain/profile setup. Use conservative build parallelism on the single machine, starting with one build job when memory pressure is plausible. Serialize builds, tests, and app ownership. Do not change global Cargo defaults merely to tune this task.

For constructed application/test profiles, set Paste Sequence `next_hotkey = None` and `cancel_hotkey = None` **before application construction**, as required by `AGENTS.md`; preserve production defaults.

Use focused behavior assertions, not duplicate existence tests. Prefer independently diagnosable cases. Keep original behavioral coverage when updating obsolete tests. Run an isolated failing test to distinguish interference from a deterministic defect; record the distinction rather than suppressing the failure.

### H2. Mandatory automated acceptance families

| Family | Minimum evidence |
|---|---|
| Legacy compatibility | Existing scalar, exact fixed-width integer, quantities, vectors, contextual aliases, reassignment/compound assignments, lazy expressions, loop rollback, source preservation, and physical references. |
| Source mapping | Inline comments/protected names/literals, multiline statements in both routes, physical continuation diagnostics, CRLF/Unicode/folds, and bounded malformed input. |
| Typed language | Exact arithmetic/mixing/rounding/limits; all ten helpers; pure functions/local parameters; block flow and transactional errors; date/month-end/DST semantics. |
| Session/state | No automatic work in Manual; correct Automatic restoration; current/stale/canceled transitions; same-revision generation races; document/host retirement; rename source-plus-metadata undo. |
| Persistence/recovery | Saved v1/v2 migration; new round trips; defaulted settings; unsupported version protection; external changes/failed writes; Extended recovery/storage; stable IDs and Save As. |
| Analysis/charts | No live source/history/cache mutation; exact export; retained diagnostic rows; bounded scenarios; current-only publication; compatible datasets; stale copy gating; raster bounds. |
| File filters | Fixed clock/calendar/rolling boundaries; u64-safe byte parsing; metadata unknown versus zero; directory traversal; content-cap intersection; accepted-result cap after filtering; fallback state preserved. |
| Examples | All linked executable examples evaluate to their documented results, including error cases. Update the catalog count deliberately when adding entries. |

### H3. Native interaction evidence

Build and launch the application on a supported Windows environment using the repository's current owned-process/launch practices. Use an isolated app-data/test profile with the required test hotkeys disabled. Capture screenshots or concise event evidence for actual actions; headless rendering is useful but is **not** proof of Windows focus, native clipboard, pop-out, or window-close behavior.

| Scenario | Actions and expected result |
|---|---|
| Manual and controls | Open a saved Pad; select Manual; change multiple configured inputs; results stay labeled old; Calculate updates once; invalid input is rejected; Apply/Undo restores the source. |
| Language and rename | Paste one documented combined function/multiline/block example; inspect output; F2 a parameter and preview local references only; apply/undo; verify physical-line navigation and copied exact/date values. |
| Sensitivity and Charts | Generate the five-row fixture; inspect an intentionally invalid scenario; copy each table format; open Charts; switch line/bar/scatter; hide a series; inspect labels/tooltips; copy the chart to an image-capable target. |
| Freshness and host lifecycle | Pop out Charts and Pad; edit source in Manual; charts/results show stale status; main launcher query still accepts focus/text; dock/close/reopen; switch documents while work is pending; old work/actions do not commit. |
| Extended save/recovery | Open a representative Extended document; edit/scroll/fold/search; save/reopen; inspect usage; exceed an evaluation limit without losing source; verify opted-in recovery in an isolated profile. |
| File Search | Use a deterministic fixture directory; apply date and size with an existing extension/name filter; inspect active badges, cap warning and filesystem-backend notice; clear each filter; test content mode; reopen preferences. |

Perform these as a small integrated final pass, not six independent full application campaigns. Reuse current build/artifacts when unchanged. A screenshot alone is not proof that an action worked: record what was interacted with and the observed state/output.

### H4. Budget and stop rules

- One scoped baseline pass; checkpoint-appropriate targeted checks; one consolidated affected-subsystem pass after integration; one independent final review; one scoped final first-party Clippy pass. Further runs require changed relevant inputs or a concrete failure.
- Keep cached focused test batches within a **100-second execution deadline**. Treat compilation separately with an explicitly bounded owned-process deadline appropriate to the machine; do not confuse a compiler timeout with a test assertion failure. Split relevant batches rather than silently expanding the deadline or running the whole repository.
- For a given reproducible feature failure, allow the initial diagnosis and **up to two focused corrective/retest cycles**. A genuinely new root cause may justify another explicitly recorded bounded check; repeated identical attempts do not. Report an exhausted blocker with evidence and preserved work.
- Native environment failures receive at most two materially different bounded setup/launch attempts. Do not resurrect historical capture/replay harnesses or spend the rest of the Goal repairing the desktop environment. Mark the exact native scenarios unverified; finish independent safe automated work without pretending native acceptance passed.
- Do not run the entire test suite automatically, repeatedly rebuild release, or start a repository-wide benchmark/lint cleanup. Broaden only for a documented dependency/defect that makes focused evidence insufficient. Exclude vendored dependencies from unrelated lint cleanup.
- No user-run intermediate tests/screenshots are prerequisites. The user performs a final review after execution. Do not weaken tests or acceptance to avoid reporting a blocker.

## I. Technical risk management and blocker handling

| Risk | Required mitigation |
|---|---|
| New syntax differs between scalar and loop paths | Shared lexical/logical source model, explicit structured-document routing, differential legacy fixtures, and tests with/without loops. |
| Manual mode leaves stale offsets/values usable | Immutable source/result basis, explicit scheduling policy, run generation, and current-only mutation/navigation guards. |
| Exact values leak through f64 or allocate excessively | Typed dispatch, explicit conversion/provenance, bigint/rational caps before work, and shared cancellation. |
| Rename changes the wrong binding or loses metadata on Undo | Static symbol identity across inactive code, preserved loop semantics, conflict preview, and atomic related-metadata history. |
| Larger sources pass one layer but fail another | Central limit profiles and explicit admission/evaluation/storage separation across session, worker, persistence, recovery, and UI. |
| New companion breaks focus or duplicates effects | Existing utility-host ownership/first-pass receipts, registered keys/routes, guarded clipboard actions, and actual native focus checks where available. |
| File filters silently miss eligible results | Eligibility before caps, semantic/resolved criteria separation, deterministic time tests, and explicit safe Everything-to-WalkDir routing. |
| Large scope hides incomplete features | Thirteen-ID evidence matrix, small coherent checkpoint commits, independent review, no completion claim for a merely visible button or written code. |

A blocker report distinguishes (1) a required feature defect, (2) a preexisting unrelated issue, and (3) an optional future improvement. Include the failed/unrun check, expected versus actual behavior, root-cause evidence versus hypotheses, attempted remedies, remaining budget, preserved changes/commits, and the smallest safe next action. Stop affected dependent work after the bounded budget; continue an independent checkpoint only if the same checkout is coherent and preserved partial changes cannot contaminate it. Do not create a side branch/worktree to hide a failure or call an unimplemented core feature optional.

## J. Definition of Done

The Goal is **COMPLETE** only when every required feature in Section A is implemented, its acceptance requirements pass, relevant existing behavior is preserved, targeted verification and applicable native checks have actual evidence, and no known feature-blocking defects remain. All implementation is committed on the correct chained Goal branch, review findings are resolved or explicitly classified, and the final tip/push/checkout state is truthful.

Report status along separate axes:

- **Implementation:** complete / partial / blocked for each of the thirteen IDs.
- **Automated verification:** passed / failed / not run, with actual commands and results.
- **Native verification:** verified / environment-blocked / not run for each relevant scenario.
- **Delivery:** commits created, pushed/local-only, remaining dirty paths, and ancestry verified/not verified.

A native environment blocker may justify **IMPLEMENTED — NATIVE ACCEPTANCE LIMITED**, not a claim of fully verified completion. A required unimplemented feature or failed core criterion makes implementation partial/blocked. Push rejection alone does not make correctly completed local implementation incomplete; report local-only delivery explicitly.

Document all planned optional exclusions and the metadata-filtered filename backend tradeoff. Do not merge into `master`/`main`, start another Goal, or build optional additions at the completion boundary.

## K. Final delivery report and exact-tip handoff

The final report must state:

1. A compact thirteen-ID outcome/evidence table, completed checkpoints, substantive choices, and remaining limitations.
2. Important changed paths; actual test/check commands, counts/results and timeouts; the scope of Clippy/review; preexisting failures separated from new ones.
3. Native scenarios performed, observed outcomes, evidence paths, and exact unverified scenarios. Do not claim a build, test, clipboard transfer, screenshot interaction, or live window behavior was verified unless it was executed.
4. `parent_branch`, `parent_tip_sha`, task branch, `baseline_sha`, significant checkpoint SHAs, final reporting commit if any, **actual `final_tip_sha`**, actual upstream/push state, and clean/dirty checkout with intentionally preserved paths.
5. Confirmation that this Goal used one existing checkout, no Git worktrees/auxiliary clones/agent branches, and no intermediate merge/rebase/cherry-pick to advance the chain.
6. The explicit handoff: **“The next Goal must create its fresh child branch directly from `<final_tip_sha>`, not from master/main or an earlier common base.”**
7. A brief final user review checklist covering the newly visible controls, not a request for the user to finish intermediate verification.

Stop after delivery of the approved work and factual handoff, or at a properly documented bounded blocker. Do not infer completion from the number of commits or from the absence of compiler errors.
