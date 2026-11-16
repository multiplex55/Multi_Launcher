# Codex Goal — Calculation Pad Dynamic Vectors and Analysis Dialogs

**Feature:** CALC_PAD_DYNAMIC_VECTORS  
**Planning date:** November 15, 2026  
**Companion:** `CODEX_KICKSTART_CALC_PAD_DYNAMIC_VECTORS.md`  
**Scope:** Twelve collection functions, empty vectors, 1,024-element vectors, complete language/chart integration, a separate Sensitivity Tables dialog, optional Document usage, and adjacent Charts presentation/Close buttons.

This document is the feature-specific implementation contract. Read the current repository's `AGENTS.md` and applicable instructions first. Ordinary engineering decisions belong to Codex; this Goal does not require another questionnaire or milestone-by-milestone permission. This is one Goal and one work category, with one shared task branch for the Goal and Kickstart.

## A0. Mandatory chained Git branch bootstrap — before all application work

**The first execution checkpoint is M0-A: resolve the predecessor's exact final tip → create a fresh child task branch at that SHA → create this Goal's baseline commit → attempt the initial normal upstream push → record and verify ancestry.** Do this after reading instructions and before application changes, implementation builds/tests, or M1.

1. In the **single existing repository checkout**, inspect `git status -sb`, branch, `HEAD`, remotes, upstream, staged/unstaged/untracked changes, and authoritative prior-Goal/queue handoff records. Treat preexisting work as intentional.
2. For a continuing sequence, resolve the **immediately preceding completed/delivered Goal or category's final committed tip**, even if the checkout is currently on `master`/`main`. Record `parent_branch` and `parent_tip_sha`. Fetch safely if the required commit exists only remotely. A committed local predecessor remains authoritative when its push failed; on another machine, block until that exact ancestor is available rather than choosing a stale remote base.
3. The observed predecessor is `feat/calc-pad-and-file-search`, with delivered tip `df62fd792a7a20d5a1c55fb542eddc73dd37bad1`. Its previous implementation checkpoint is `17957d3679c814d4caaf679c80827ee290632294`; the later reporting commit is part of the predecessor's final tip. Verify current handoff records before execution. If a later completed category has superseded this predecessor, use that verified later final tip. Do not infer a new parent from commit dates or the currently checked-out branch alone.
4. **Only the first Goal in a genuinely new chain may default to the up-to-date primary integration branch**, unless the user explicitly gives another base. A safe current-HEAD exception for a genuinely first task must be documented. This Goal already has an identified predecessor; `master` is not its default starting point.
5. Choose a fresh, unique, idiomatic name such as `feat/calc-pad-dynamic-vectors`, adding a suffix if needed. The Goal and Kickstart use **the same branch**. Do not reuse an already-active task branch as a substitute for creating the required child.
6. Protect user changes. Never automatically reset, stash, hard-pull, rebase, merge, cherry-pick, force-checkout, overwrite files, or stage unrelated changes to manufacture a safe transition. If selecting the required parent is unsafe, preserve the checkout and report the precise ancestry/working-tree blocker.
7. Create and switch directly from the recorded SHA, using the equivalent of `git switch -c <new-task-branch> <parent_tip_sha>`. Verify the initial branch HEAD equals that SHA.
8. Create the task's **own M0-A baseline commit**. Prefer the minimal existing execution ledger convention; an intentional empty commit is acceptable: `chore(calc-pad): [M0-A] establish dynamic vectors branch baseline`. Record `baseline_sha` separately from `parent_tip_sha` and verify `git rev-parse <baseline_sha>^` equals the exact required parent tip.
9. Attempt one normal upstream push, normally `git push -u origin HEAD`. Never force-push. If rejected, denied, or unavailable, record the actual failure/unavailability, retain the local branch/commit, and continue safe local implementation. Do not retry in a loop or rewrite history to repair a push.
10. Keep all subsequent checkpoints on this branch. **No Git worktrees, `git worktree add/move/prune`, auxiliary checkouts, helper clones, secondary clones, or per-agent implementation branches.** Do not repurpose unrelated existing worktrees. Read-only analysis may run in parallel; source edits, branch transitions, builds/tests, and commits remain sequential.
11. At completion, finish all intended reporting commits, read the actual final HEAD, and record `final_tip_sha`. The following Goal must create its fresh child branch **directly from that final tip**. Do not merge/rebase/squash/cherry-pick intermediate category branches merely to advance the sequence. Final integration is a separate approved boundary.

**M0-A acceptance:** the unique child branch is active; recorded parent matches the verified predecessor; baseline is its direct child; initial push was attempted or concretely unavailable; no user work was lost or staged; no extra checkout/worktree/side branch was created. Push failure alone does not block implementation.

Use the existing authoritative execution ledger. If no queue ledger exists, maintain one compact status file at `docs/plans/execution/standalone/calc-pad-dynamic-vectors/STATUS.md`. Track requirements, checkpoint state, verification, Git identities, and actual push outcomes. Do not create competing progress documents.

## A. Goal and desired outcome

Let users build and transform finite numeric datasets directly in Calculation Pad, then chart the resulting named vectors without copying data into another tool.

A user can start with `[]`, append values inside existing loops, pass vectors through formula functions, edit and inspect them, and plot up to 1,024 points. Operations return new values; they never mutate another binding or earlier result snapshot. Existing arithmetic, physical source references, source editing, Manual calculation, rename, Undo/Redo, saved documents, and companion ownership remain coherent.

Also deliver all three requested UI changes:

- **Config Menu → Tools → Sensitivity Tables…** opens a dedicated companion dialog containing the existing sensitivity editor/results.
- **Config Menu → View → Document usage** controls the usage panel and remembers the choice; default hidden.
- The Charts **Dock/Pop out** and **Close** controls share one horizontal row.

The complete outcome includes executable in-app documentation, targeted verification, bounded native interaction checks when supported, and the exact final Git-tip handoff.

## B. Current codebase findings

### B1. Verified reference state

The inspected implementation is available at [commit df62fd7](https://github.com/multiplex55/Multi_Launcher/commit/df62fd792a7a20d5a1c55fb542eddc73dd37bad1) on `feat/calc-pad-and-file-search`. The current `master` observed during planning was `09ca2427c5da5782eb540a538ce0f1bb71a39434` and does not contain this feature branch's full implementation.

The predecessor's [status](https://github.com/multiplex55/Multi_Launcher/blob/df62fd792a7a20d5a1c55fb542eddc73dd37bad1/docs/plans/execution/standalone/calc-pad-and-file-search/STATUS.md) reports implemented features and focused automated verification, with Windows interaction acceptance limited by its environment. Those are historical results, not verification of this new Goal. Do not inherit its unrelated File Search work, old acceptance campaign, or exhausted native attempts as new task requirements.

Reconfirm paths and ownership after M0-A; adapt moved code without changing the contract.

### B2. Relevant owners and extension seams

| Owner | Current behavior and implication |
| --- | --- |
| [`src/calculation_pad/vector.rs`](https://github.com/multiplex55/Multi_Launcher/blob/df62fd792a7a20d5a1c55fb542eddc73dd37bad1/src/calculation_pad/vector.rs) | `CalculationVector(Vec<f64>)`, `MAX_VECTOR_COMPONENTS = 128`, `MAX_VECTOR_WORK = 1024`. Both the constructor and literal parser reject empty vectors. `Node::evaluate` and `apply` own vector arithmetic and products. The evaluator starts its own private work counter. |
| [`document.rs`](https://github.com/multiplex55/Multi_Launcher/blob/df62fd792a7a20d5a1c55fb542eddc73dd37bad1/src/calculation_pad/document.rs) | `CalculationValue`, prepared expression routing, physical rows, named references, final bindings, formatting and canonical Copy. `MAX_EXPRESSION_BYTES = 2048`; internal templates have a separate derived allowance. Vector evaluation currently does not receive the shared `WorkBudget`. |
| [`script.rs`](https://github.com/multiplex55/Multi_Launcher/blob/df62fd792a7a20d5a1c55fb542eddc73dd37bad1/src/calculation_pad/script.rs) | `FUNCTIONS`, `FunctionSignature`, `special`, `uses_script_syntax`, `ScriptExpression::evaluate`, `call`, `WorkBudget` and `AggregateWork`. This is the shared runtime/editor function catalog. Statistics already accept vectors/sequences; `sum(empty)` returns zero and other empty reductions fail. Formula arguments/results already carry `CalculationValue`. |
| [`loops.rs`](https://github.com/multiplex55/Multi_Launcher/blob/df62fd792a7a20d5a1c55fb542eddc73dd37bad1/src/calculation_pad/loops.rs) | Existing loops iterate vectors and sequences. `Execution::publish` copies outcomes into row/runtime/binding state. Failed loops restore snapshots and deliberately poison affected bindings, preventing partial results from escaping. |
| `syntax.rs`, `rename.rs`, `dependencies.rs` | Logical multiline statements retain physical offsets; binding-aware rename and dependency navigation already exist. No new multiline grammar is needed. |
| [`session.rs`](https://github.com/multiplex55/Multi_Launcher/blob/df62fd792a7a20d5a1c55fb542eddc73dd37bad1/src/calculation_pad/session.rs), `worker.rs` | Source/history, current evaluation basis, `SessionEvaluation::final_bindings`, Manual mode, stale results, worker retirement, and sensitivity lifecycle. `shared_evaluation()` is a non-evaluating observer. |
| `limits.rs`, `persistence.rs` | Standard: 64 KiB/256 physical rows. Extended: 128 KiB/1,024 rows. Edit/storage admission: 256 KiB/2,048 rows; serialized envelope: 4 MiB. Named documents are format v3; recovery is v2. Computed vectors are not a new persisted data source. |
| [`chart.rs`](https://github.com/multiplex55/Multi_Launcher/blob/df62fd792a7a20d5a1c55fb542eddc73dd37bad1/src/calculation_pad/chart.rs) | `ChartSource`, `ChartDefinition`, `ChartDataset::from_vectors`, shared scene/raster. Current cap is 1,000 points; empty vectors are rejected. Automatic X uses 1…N. Eight series, 20,000 primitives, 32,000,000 raster work units and maximum raster dimension 2,048 are separately bounded. |
| [`src/gui/calculation_chart_dialog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/df62fd792a7a20d5a1c55fb542eddc73dd37bad1/src/gui/calculation_chart_dialog.rs) | `CalculationChartDialog`, `ChartRuntime`, `ChartActions`, `chart_data` and current-basis guards. Selectors enumerate physical result rows and clone vector contents; saved vector choices currently use `LineId`. Presentation and Close are rendered separately in a vertical body. |
| [`src/gui/calculation_pad_dialog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/df62fd792a7a20d5a1c55fb542eddc73dd37bad1/src/gui/calculation_pad_dialog.rs) | `config_menu_ui` embeds `sensitivity_ui`. `PadView` owns retained definition/draft presentation. `CalculationPadDialogEffect::OpenCharts`, `AcceptedParentOpen`/`PadOpenReceipt` and retained actions establish companion ownership. Document Usage is always displayed. |
| `src/gui/calculation_pad_editor.rs`, `calculation_pad_format.rs` | Highlighting, completion, nested call hints, projected source and result display. Completion/hints consume the shared function catalog. |
| `src/gui/utility_window_host.rs`, `render.rs`, `mod.rs`, `src/settings/utility_dialog_sizes.rs` | Hosted-utility registration, parent/child lifecycle, retained action delivery, presentation/size persistence and panel routing. Use these for the new companion. |
| `src/settings/model.rs`, `src/settings_save.rs` | `CalculationPadDisplayPreferences` and retained settings publication are the durable application-wide preference path. |
| `src/calculation_pad/examples.rs`, `src/calculation_pad/README.md`, root `README.md` | Executable in-app examples and linked reference. The example catalog currently contains 104 examples and has behavioral validation/search tests. |

### B3. Dependencies that a constant-only change would miss

1. **Expression admission:** even `[0,0,…,0]` with 1,024 elements needs 2,049 bytes before assignment text. The 2,048-byte expression limit must get a bounded vector-specific accommodation; document source/storage limits stay unchanged.
2. **Work accounting:** a 1,024-element literal already exceeds the private 1,024-unit vector parser/evaluator budget. Repeated immutable append also copies growing datasets. Charging all that to an unchanged 50,000-unit document budget would reject the required example. Conversely, leaving vector work uncharged would bypass the shared safety model.
3. **Binding semantics:** current failed reassignment becomes the latest error binding. The requested preservation after a failed collection operation is an intentional narrow exception, defined in C4.
4. **Named chart sources:** a loop can leave an initial `values = []` row plus a later assignment to the same name. A named chart source must follow the current final binding, not accidentally remain attached to the initial empty row.
5. **Other chart limits:** `persistence.rs::validate_view_metadata` caps bar labels at 1,000. Vector-chart capacity and label admission must reach 1,024 while sensitivity remains capped at 1,000 samples.
6. **Copy versus preview:** vectors currently share `formatted_result` with `canonical_result`. Shortening display without separating canonical serialization would truncate copied values.
7. **Companion wakeup:** the Pad currently installs a wake callback for ROOT/Pad, and the sensitivity worker captures it. A popped-out Sensitivity Tables dialog needs current progress/completion even when the parent is not actively painting.

## C. Approved functional requirements

No questionnaire is required. The explicit request and existing conventions resolve the product behavior. The defaults below are part of this contract.

### C1. Requirement ledger

| ID | Required outcome |
| --- | --- |
| V1 | Valid finite dimensionless vectors of 0–1,024 elements through every producer. |
| V2 | All twelve collection functions with pure value semantics and validated indexes. |
| V3 | Required loop/function workloads, bounded work and cancellation, immutable snapshots. |
| V4 | Explicit outside-loop failure preservation; existing loop failure semantics and scalar failure behavior preserved. |
| V5 | Source/history/dependencies/rename/persistence/Manual integration and complete canonical Copy. |
| V6 | Editor discovery, compact vector previews and executable in-app documentation. |
| C1 | Unambiguous final named vector selection and 1,024-point line/bar/scatter charts. |
| C2 | Empty/one-point/mismatched datasets, zero-based automatic X, current-state and host lifecycle behavior. |
| U1 | Dedicated Sensitivity Tables companion under Config Menu → Tools. |
| U2 | Persisted Document usage checkbox under Config Menu → View; default hidden. |
| U3 | Adjacent Charts Dock/Pop out and Close controls. |

Track implementation, automated verification, native verification, and Git delivery separately.

### C2. Vector value and function contract

Vectors contain finite ordinary dimensionless `f64` scalar values. Keep vectors distinct from `CalculationSequence`. Existing `range()` remains a sequence and can feed a loop; the new manipulation functions do not implicitly convert sequences to vectors.

Reject nested vectors, strings, Boolean elements, units/quantities, dates, exact fractions, typed integers and other non-scalar values as elements. Do not silently discard exactness or approximation provenance. Preserve current `ApproximateScalar` rejection unless an already-explicit supported conversion yields the ordinary scalar type; do not broaden numeric coercion as part of this feature.

| Syntax | Behavior | Boundary contract |
| --- | --- | --- |
| `[]` | Empty vector. | Valid throughout evaluation, history, observers and charts. |
| `append(v, x)` | New vector with one scalar at the end. | Result length ≤1,024. |
| `concat(a, b)` | New vector containing a then b. | Both operands vectors; combined length ≤1,024. |
| `insert(v, i, x)` | Insert before position i. | `0 <= i <= len(v)`; insertion at the end is valid if capacity allows. |
| `set_at(v, i, x)` | Replace one element in a new vector. | `0 <= i < len(v)`. |
| `remove_at(v, i)` | New vector without element i. | `0 <= i < len(v)`; removing the only element yields `[]`. |
| `pop(v)` | New vector without its last element. | Empty input fails. It does **not** return the removed scalar. |
| `get(v, i)` | Element at i. | `0 <= i < len(v)`. |
| `first(v)` | First scalar. | Empty input fails. |
| `last(v)` | Last scalar. | Empty input fails. |
| `len(v)` | Ordinary scalar integral count. | Empty vector returns 0. |
| `slice(v, start, end)` | New vector from start, inclusive, to end, exclusive. | `0 <= start <= end <= len(v)`. Equal endpoints yield `[]`. No clamping or negative-index conventions. |
| `reverse(v)` | New vector with reversed element order. | Empty input returns `[]`. |

Indexes are finite, nonnegative integral scalar values; validate type, integrality and bounds **before** converting to a machine index. Reject fractions, negative values, nonfinite values, unsupported typed values and huge out-of-range indexes with useful diagnostics. Do not introduce lossy implicit conversions.

Follow existing case-insensitive builtin-call behavior and contextual names. A builtin word used as a bare variable remains legal where current naming rules allow it; calls are recognized through the shared parser/catalog. Formula declarations that collide with newly reserved builtin function names receive an ordinary explicit declaration diagnostic.

Every producer, including literals, arithmetic, collection functions and formula-returned values, must enforce the result cap through the vector domain owner. At length 1,024, `insert(v, 1024, x)` has a valid insertion position but fails because its result would contain 1,025 elements. Validate before allocating an oversized result.

All operations are pure: `append(v, 5)` returns a new value but does not change `v` without reassignment. Aliases, previous physical rows, published worker results and saved evaluation snapshots remain unchanged.

### C3. Arithmetic, empties and independent limits

Preserve element-wise `+`, `-`, `*` and `/`, existing scalar scaling/division, `dot`/`cross` and their operator forms. **`+` never means concatenation.**

Define empties consistently:

- `len([]) = 0`; `sum([]) = 0`; `concat([], []) = []`; `slice([], 0, 0) = []`; `reverse([]) = []`.
- `average`, `median`, `min`, `max` and `stdev` on empty vectors retain their existing empty-collection diagnostic.
- `first([])`, `last([])`, `pop([])` and `get([], 0)` fail locally.
- Matching empty element-wise operations yield `[]`; valid scalar scaling yields `[]`; `dot([], []) = 0`.
- Mismatched lengths still fail. `cross` still requires three components. `[] / 0` still reports division by zero.
- Keep malformed literals such as `[1,]`, missing components and nested vectors invalid.

The 1,024 cap is the same in Standard and Extended documents. Preserve current document byte/row admission, edit/storage ceilings, storage envelopes, range size, loop-iteration/depth limits, function count/arity/depth and unrelated numeric limits.

Introduce only the bounded expression allowance necessary for large vector expressions/canonical payloads. Keep the ordinary scalar-expression ceiling separate. Check direct expressions, logical multiline statements, formula bodies and prepared internal templates consistently. A bounded allowance around 32 KiB is a candidate only if verified against the chosen canonical encoding; the implementer must record the actual limit and boundary evidence. Oversized source, expression, capacity and work must remain distinguishable diagnostics.

Capacity does not override independent source, identifier, nesting or work bounds. Do not promise that every arbitrary expression containing 1,024 components is admissible regardless of its complexity.

### C4. Failure publication and transactions

Treat the request's atomic-operation rule as an intentional, limited migration:

1. **Outside an executing loop**, when an evaluated new collection operation or its argument evaluation fails during reassignment, retain the destination's immediately preceding valid binding from the **current evaluation**. Do not recover values from an older source revision or scan past a preexisting failed binding to resurrect an older success.
2. The failed physical source row remains an error, with its actual diagnostic. A physical `rN` reference to that row fails. Named references use the preserved binding and its actual origin.
3. With no preceding valid binding, the assignment fails normally. Unrelated failed scalar reassignment remains fail-closed.
4. Propagate collection-failure provenance through nested calls and formula wrappers using typed metadata/an equally reliable structural mechanism. Do not infer it solely from return type, since accessors return scalars; do not match error-message text.
5. Within an executing loop, genuine evaluation failure preserves existing whole-loop rollback **and poisoning of affected bindings**. Partial iterations must not become a usable dataset. Successful `break`/`continue` and empty iteration retain their existing semantics.
6. Work exhaustion and cancellation remain terminal evaluation conditions; this exception must never convert them into a successful current result or allow further unbounded execution.
7. Preserve both evaluator routes: the ordinary straight-line evaluator and the loop/block execution route, including top-level statements in a document that also contains a loop.

Reconcile runtime lookup, final bindings, watches, comparisons, dependencies, rename and named chart resolution. A consumer must not point to the rejected row while silently reading a value from somewhere else. Keep attempted-operation diagnostics and dependency provenance visible.

### C5. Required scripting examples

Restore proper newlines and indentation in examples. These are the required **post-implementation** scripts, not claims about currently supported collection syntax.

#### Build two 1,024-point datasets

~~~text
x_values = []
y_values = []

for i in range(1024):
    x_values = append(x_values, i)
    y_values = append(y_values, i * i)

point_count = len(y_values)
largest = max(y_values)
average_value = average(y_values)
~~~

Expected: `point_count = 1024`, `largest = 1046529`, `average_value = 349013.5`. Both named vectors are directly selectable in Charts.

#### Edit a vector

~~~text
values = [10, 20, 30]
values = append(values, 40)
values = insert(values, 1, 15)
values = set_at(values, 0, 5)
values = remove_at(values, 2)
values = pop(values)

count = len(values)
first_value = first(values)
last_value = last(values)
~~~

Expected: `values = [5, 15, 30]`, count 3, first 5, last 30.

#### Formula-generated dataset

~~~text
starting_balance = 10000
annual_growth = 0.07

fn projected_value(base, growth, year) = (
    base * (1 + growth) ^ year
)

years = []
balances = []

for year in range(31):
    years = append(years, year)
    balances = append(
        balances,
        projected_value(starting_balance, annual_growth, year)
    )

point_count = len(balances)
~~~

Expected: 31 values. Formula functions still receive all document-dependent values as parameters; no implicit document-variable capture is added.

Also verify a vector-returning helper, such as `fn add_sample(v, x) = append(v, x)`, and nested calls such as `len(append(values, 5))`.

### C6. Work, allocation and responsiveness

Keep a bounded total evaluation model and sensitivity aggregate accounting. Integrate vector work with `WorkBudget`/cancellation instead of retaining an uncharged private evaluator path.

Measure the required two-vector generation script, large literals, arithmetic/reductions, and one formula-wrapped collection workload. Select and document a narrow bounded accounting revision that admits these ordinary cases with reasonable headroom. The existing 50,000 figure is not an immutable requirement if accurately accounting for vectors makes it insufficient; neither is unlimited work acceptable. Prefer preserving ordinary scalar/script bounds while introducing appropriately bounded vector costs when that fits the current architecture.

Account for component traversal, copies/allocations, sorting where already relevant, parsing, binding materialization and function/loop publication as appropriate. Cheap shared immutable snapshots or another bounded representation are acceptable. **Replacing Vec with Arc alone does not prove append is efficient**, because old values may remain aliased.

Charge/check before expensive work or allocation where practical. Keep checked arithmetic, cancellation checks during bounded chunks, and aggregate sensitivity limits. Budget units and usage display must remain truthful and consistent. Do not bypass limits in tests, reset the document meter for each collection call, or special-case the exact acceptance script.

Use one short controlled measurement pass and, if needed, one targeted optimization/retest pass. Record workload, build profile, elapsed time, work usage and meaningful allocation/copy evidence. Avoid a standalone benchmarking framework or unrelated startup/idle optimization.

### C7. Results, editor, history and persistence

- Show small vectors completely and large vectors as a bounded readable preview plus total count, following existing sequence presentation conventions. Avoid constructing/laying out all 1,024 values on every repaint merely for a preview.
- Separate display formatting from canonical Copy. Canonical Copy contains all elements, no ellipsis or count annotation, and valid parseable vector syntax. Use a vector-specific complete serialization that preserves finite scalar values through copy/re-evaluation, including scientific notation where existing scalar admission requires it. Do not change application-wide scalar display precision.
- Verify large canonical payloads fit the vector-expression allowance and existing document admission; retain useful rejection if the surrounding document is independently too large.
- Add shared completion/signatures/highlighting for all twelve functions. Verify nested/multiline hints, contextual variable names, formula parameters, scope-aware rename and navigation.
- Preserve source-authoritative Undo/Redo and line IDs. Generated values are recomputed through current session policy, not stored as a new dataset file or hidden source rewrite.
- Save/reopen, opt-in recovery, current/stale snapshots and Manual calculation remain coherent. Opening an inspector, Charts or Sensitivity Tables must not calculate the document.
- Existing v1/v2/v3 named documents and supported recovery records remain readable. If named chart-reference representation changes require a format revision, implement the appropriate bounded migration/defaulting and unknown-future-version protection. Do not silently rewrite unsupported records.

### C8. Charts

1. Support named generated vectors as X and Y sources for line, bar and scatter charts, with name and count shown. No special final expression, export or conversion is required.
2. A named source follows `SessionEvaluation::final_bindings` across reassignment, loops, zero iterations, conditionals and formula returns. It must not be pinned accidentally to an older physical row bearing the same name.
3. Preserve existing explicit `LineId`/physical-result selections and sequence chart support. Add the narrowest typed reference representation needed to distinguish named versus physical sources; migrate saved definitions safely. Rename/Undo and independently changed chart selections must remain correct.
4. Admit 0–1,024 points per vector series and 1,024 bar labels, retaining the eight-series cap. Keep sensitivity's **1,000-sample** generation cap distinct.
5. Automatic X becomes **indexes 0…N−1**, as requested. Update model, selector text, default index/category labels, tests and documentation together. This intentionally changes the old 1…N automatic positions; explicit X values and custom labels remain unchanged. Physical `r1` source numbering is unaffected.
6. Matched empty datasets display a neutral **No data** message. Missing source, failed source, mismatched X/Y lengths and all-hidden series remain distinct states. Do not silently truncate mismatched data. Empty plotting does not attempt bounds division or export a misleading partial image.
7. One-point line charts remain valid and show a visible marker; no segment is required. Preserve tooltip values, pan/zoom/reset and existing nonfinite/extreme-transform protection.
8. Selectors inspect identity/name/count without cloning every vector each frame. Cache data/scenes against actual source/definition/evaluation bases, including final-binding changes.
9. Preserve Manual-mode staleness: observer reads never initiate evaluation; stale state is explicit; current-only copy/action checks remain. Do not present an old dataset as fresh.
10. Preserve chart-only image Copy and shared scene/raster semantics. Validate 1,024-point and eight-series behavior under existing render limits before changing any limit. Do not introduce silent decimation or an unrelated plotting library.

### C9. Sensitivity Tables dialog

Move the existing UI from the configuration menu into a dedicated hosted companion, opened only by **Config Menu → Tools → Sensitivity Tables…** for this scope.

The dialog retains all current capabilities: create/remove definition, eligible input selection, start/end and step/count modes, up to eight outputs, Generate, Cancel, progress, error/stale feedback, virtualized results, and TSV/CSV/Markdown copying. Preserve one input/one definition per Pad and the 1,000-sample bound.

Reuse the same `CalculationPadSession`, worker, definition metadata, run identity and table. Do not create a second evaluator, duplicate persisted definition, or new data store. A proposed `src/gui/calculation_sensitivity_dialog.rs` is a reasonable presentation owner; this path is a **new module proposal**, not a claim that it already exists.

Use existing hosted-utility contracts for default/remembered presentation, sizing, pin/close semantics, parent ownership and dock/pop-out. Keep definition and table available when merely closing the companion, so Charts can continue using a current table. Parent close retires the child. Switching documents rebinds safely and rejects stale actions.

Opening, closing, docking or displaying the companion does not evaluate the Pad or generate a table. Generate remains explicit and follows current eligibility rules.

Route edits, Generate, Cancel and exports through retained owner-validated actions, including source/definition/basis and document/host identity checks. Respect discarded/sizing passes and pending parent transitions. Do not mutate the session directly from an unretained render callback.

Extend existing worker wakeup and owner polling to repaint the current child viewport while the parent is not actively rendering. No timer polling loop, duplicate worker or extra background evaluator is permitted.

### C10. Document usage and Charts controls

**Document usage:** Add the checkbox to Config Menu → View using `CalculationPadDisplayPreferences` and the existing retained settings-save route. Default false, including missing/malformed legacy settings. Remember it app-wide across restart, document switching and dock/pop-out. Toggling visibility changes neither source nor its Undo history, calculation mode, evaluation basis or pending work. When shown, preserve the existing counters/collapsible details. Ordinary errors, over-limit diagnostics and calculation status remain visible independently of the usage panel.

**Charts controls:** Render the presentation button and Close inside one horizontal row in both docked and popped-out hosts. Preserve current owner/first-pass/visibility/discard checks, pinned Close disabling and deferred `host_request` behavior. Do not alter OS-close, Escape, focus or parent-close policy.

## D. Scope and non-goals

Included: the full contracts above, necessary local type/parser/budget/reference migrations, focused tests and user documentation.

Excluded:

- Arbitrary nested/mixed-type lists, string collections, typed vectors, mutable references, index-assignment syntax, a `push` alias or a second scripting language.
- New loops, recursion, statement-bodied functions, new function limits, expanded source/storage/loop limits or broad numeric-coercion changes.
- Additional chart types, chart dashboards, saved generated-data files, implicit evaluation, two-input sensitivity grids or increased sensitivity samples.
- New launcher hotkeys or command prefixes, workspace features, framework migrations, dependency upgrades, File Search changes, unrelated cleanup and general performance campaigns.
- Automatically merging into `master`/`main` or starting another Goal.

Preserve existing scalar/quantity/exact/date/integer behavior, element-wise arithmetic, source references, keyboard shortcuts, launcher query focus, old data readability, session ownership, save/recovery boundaries and current-only clipboard actions.

## E. Technical implementation strategy

### E1. Domain first

Keep one vector invariant/cap owner. Implement pure collection helpers alongside `CalculationVector` and register their signatures/typed dispatch in the existing script system. Reuse statistics. Make literal/vector-expression admission consistent with functions and internal prepared expressions without increasing every expression's allowance.

Separate immutable value storage from presentation. Audit cloning at vector references, `quantity_bindings`, script legacy-slot handling, user-function arguments, loop iterable snapshots and `Execution::publish`. Integrate actual vector work into the shared cancellation/aggregate path.

### E2. Publication and reference integrity

Separate a failed physical row from the destination binding retained by C4. Carry structural failure context through calls and apply it at the binding owner. Reconcile ordinary static reference targets and loop runtime slots so named consumers and dependencies agree. Do not overwrite old result rows to simulate a current named value.

Preserve loop poisoning. Prove both top-level evaluator routes using paired acceptance cases. A generic “all assignments keep last success” rewrite is outside scope.

### E3. Chart references and persisted metadata

Use current final bindings for named choices and stable physical IDs for explicit row choices. Update related-reference transactions and metadata validation together. Preserve old serialized `ChartSource::Vectors` selections with an explicit compatibility reader/migration rather than silently reinterpreting them as names.

Keep vector/chart-cap constants related while maintaining separate sensitivity-sample admission. Dataset/scene caching should rely on current basis and definition identity, not solely the variable's spelling.

### E4. Companion presentation

Use Charts/Dependencies as existing utility-host patterns. The Sensitivity dialog owns presentation and transient control drafts; Pad/session remains the authoritative analysis owner. Route accepted actions after the retained frame and extend wakeup/retirement to the active child viewport.

The small view/layout preferences use their existing owners. Avoid moving reusable domain logic into `LauncherApp` or duplicating the embedded sensitivity implementation.

## F. Dependency-ordered milestones and checkpoint contracts

Use the fewest practical commits within these meaningful boundaries. Each checkpoint must be coherent and receive scoped evidence before acceptance. M0-A is mandatory; M0-B is read-only confirmation and need not manufacture another empty commit.

### M0-A — Chained branch and baseline

- **Objective/ownership:** Establish the correct task branch and task-state ledger; orchestrator owns Git.
- **Tasks:** Execute A0 in order, preserving user changes and recording identities/push state.
- **Behavior/invariants:** No application edits/tests before the gate; one checkout and branch.
- **Edge cases:** Dirty index, unavailable local-only ancestor, conflicting handoff, branch-name collision, rejected push.
- **Verification/acceptance:** A0's direct-parent, unique-branch and safety checks all pass; push failure is recorded accurately.
- **Commit:** `chore(calc-pad): [M0-A] establish dynamic vectors branch baseline`.

### M0-B — Confirm live ownership and verification selection

- **Objective/ownership:** Planner confirms this contract against the now-selected source.
- **Tasks:** Verify B's owners, current agent configuration, actual test names, renderer/metadata reference seams and targeted build profile. Identify only concrete drift. Record the proposed bounded vector accounting/admission approach and test commands.
- **Behavior/invariants:** No broad baseline campaign, speculative refactor or new product questionnaire.
- **Edge cases:** Stale test names, existing failing fixtures, newer schema/host changes.
- **Verification/acceptance:** Every requirement has an owner and later checkpoint; any unrelated baseline issue is distinguished from task risk. No zero-test selector is accepted.
- **Commit:** Fold the brief ledger update into the next coherent checkpoint unless a separate substantive baseline record warrants a commit.

### M1-A — Empty/large vectors and expression foundation

- **Objective:** All vector producers share valid 0–1,024 invariants; large canonical-sized literals can be admitted safely.
- **Ownership:** `vector.rs`, `document.rs` and relevant `script.rs` declaration/admission seams.
- **Tasks:** Unify constructor/cap diagnostics; remove empty-literal barriers without admitting malformed/nested data; preserve arithmetic/products; add bounded vector-expression admission and consistent function/template checks; establish shared metering/cancellation seams needed by M1-B.
- **Behavior:** Empty vectors are typed values everywhere; 1,024 literals/arithmetic succeed within ordinary work limits; 1,025 fails before oversized allocation.
- **Edges:** Empty divide-by-zero, unequal lengths, nonfinite results, typed elements, full literal with scientific components, source versus expression limits.
- **Regression constraints:** Preserve scalar expression bounds, numeric types, doc/storage/loop limits and dot/cross behavior.
- **Verification:** Focused vector/document tests; migrate obsolete `[]`/129-element expectations while retaining malformed/type/nonfinite checks. Exercise 0/1/1,024/1,025 directly.
- **Acceptance:** No remaining vector producer bypasses the cap; the required large literal is not rejected by an obsolete private work/admission limit.
- **Commit:** `feat(calc-pad): [M1-A] admit empty and 1024-element vectors`.

### M1-B — Collection operations and bounded shared work

- **Objective:** All twelve functions work compositionally with bounded actual vector costs.
- **Ownership:** `vector.rs`, `script.rs`, `document.rs`; narrow clone/cost call sites in `loops.rs` and `sensitivity.rs` if required.
- **Tasks:** Add domain helpers and shared signatures/dispatch; implement index/type checks; integrate vector work and cancellation into the document and sensitivity aggregate meters; reduce unnecessary deep copies while preserving aliases; complete the bounded workload measurement and record selected limits/units.
- **Behavior:** Pure operations compose with arithmetic, reductions, multiline expressions and formula parameters/results. Required two-vector 1,024-point generation completes within chosen limits.
- **Edges:** Each operation's real boundary, argument evaluation error, extreme indexes, full-cap insertion, empty slice/pop, nested calls and cancellation before expensive work.
- **Regression constraints:** No scalar coercion expansion, range conversion, uncharged vector route, unbounded budget, changed arithmetic or silently mutable snapshots.
- **Verification:** One normal and meaningful error/boundary case per function; exact required generation outputs; deliberate work exhaustion/cancellation; sensitivity aggregate rejection without partial publication; measured alias/copy behavior.
- **Acceptance:** All functions are reachable through shared typed syntax, costs are bounded and reported, required workloads pass without disabling safeguards.
- **Commit:** `feat(calc-pad): [M1-B] add bounded pure collection operations`.

### M2-A — Atomic named updates, loops and session integration

- **Objective:** C4 failure semantics work through both evaluator routes and every final-binding observer.
- **Ownership:** `document.rs`, `loops.rs`, `script.rs`, `session.rs`, `dependencies.rs` and necessary reference/rename seams.
- **Tasks:** Carry collection-failure context through nested/formula calls; keep failed row outcomes separate from retained named bindings; resolve subsequent name dependencies to the retained origin; preserve loop restore/poison behavior; verify watches/comparisons and worker snapshots; integrate source history and metadata references.
- **Behavior:** Failed top-level collection reassignment preserves its immediate valid predecessor, while `rN` to the failed row fails. Failed loop updates do not publish partial datasets. Manual observers never trigger evaluation.
- **Edges:** No prior value, previous binding already failed, collection accessor returning a scalar, formula wrapper, skipped branch, break/continue, zero iterations, cancellation and same-revision worker retirement.
- **Regression constraints:** Existing scalar failed reassignment and normal function scope stay unchanged. No old-source fallback or mutation of prior rows.
- **Verification:** Paired straight-line and loop-bearing-document cases; failed update versus failed loop; alias immutability; actual dependency targets; Manual edit/Calculate; Undo/Redo, rename and source save/reopen.
- **Acceptance:** Named values, row errors, final bindings, dependency inspector and downstream expressions agree. Existing loop transaction tests retain their meaningful assertions.
- **Commit:** `feat(calc-pad): [M2-A] preserve atomic collection binding semantics`.

### M2-B — Editor support and complete value presentation

- **Objective:** Users can discover functions and inspect/copy large values without expensive rows.
- **Ownership:** Shared function catalog; `calculation_pad_editor.rs`, `calculation_pad_format.rs`, `vector.rs`, `CalculationValue::canonical_result`.
- **Tasks:** Use shared completion/highlight/hint entries; verify nested/multiline/contextual names; implement bounded previews/counts; separate full canonical vector serialization from visual formatting; route copy through canonical values.
- **Behavior:** A 1,024-element vector remains readable in one bounded result presentation and copies completely.
- **Edges:** Empty/one/large vectors, small and huge finite values, signed/scientific values, rounded display settings, copy/re-evaluate at capacity.
- **Regression constraints:** No new GUI function registry, global scalar-format change, hidden source rewrite or truncated copy.
- **Verification:** Existing editor signature tests plus the new names; bounded preview and complete parseable canonical payload; round-trip representative finite vector values and 1,024 components.
- **Acceptance:** Every new function is discoverable, preview work is bounded, and full Copy is independent of display truncation.
- **Commit:** `feat(calc-pad): [M2-B] expose collection hints and compact vector results`.

### M3-A — Chart capacity, empty states and automatic indexes

- **Objective:** The chart domain and renderer accept valid vector datasets through 1,024 points.
- **Ownership:** `chart.rs`, chart metadata validation in `persistence.rs`, necessary GUI state labels.
- **Tasks:** Separate vector/sensitivity capacity; admit empty datasets as a neutral state; preserve one-point markers; enforce exact X/Y length agreement; move automatic X/default index labels to zero-based; permit 1,024 bar labels; verify shared screen/raster boundaries.
- **Behavior:** Line/bar/scatter show complete data; empty means No data, not an invalid mathematical operation; custom labels and explicit X retain their meaning.
- **Edges:** 0/1/1,024/1,025, empty/nonempty mismatch, all-hidden series, extreme transforms, eight visible series, invalid persisted label counts.
- **Regression constraints:** Sensitivity remains 1,000 samples; existing renderer caps and chart-only export retained unless measured evidence requires a narrow justified adjustment.
- **Verification:** Domain/raster tests for boundaries and full series counts; representative 1,024-point scene and maximum-series work; persisted label admission.
- **Acceptance:** No hidden 1,000-point vector rejection, silent truncation, NaN bounds or unintended sensitivity increase remains.
- **Commit:** `feat(calc-pad): [M3-A] chart 1024-point and empty vector datasets`.

### M3-B — Current named sources and compatible chart references

- **Objective:** Chart choices follow final generated bindings while preserving saved physical selections.
- **Ownership:** `ChartSource`/`ChartDefinition`, `calculation_chart_dialog.rs`, `session.rs` reference transactions, `persistence.rs` and relevant `rename.rs` paths.
- **Tasks:** Add typed named/physical selection semantics as necessary; show names/counts without per-frame data cloning; resolve final bindings; implement bounded saved-definition migration/defaults; update rename/Undo reconciliation; key caches/actions to the correct current bases.
- **Behavior:** Selecting `y_values` yields its final loop value even when the producing row changes between runs. Existing row-based choices retain their row meaning.
- **Edges:** Zero-iteration transition, branch-dependent reassignment, duplicate names, removed/failed binding, retained value after failed collection update, document switch, saved old definition, independently modified series selection.
- **Regression constraints:** Preserve physical `rN`, old document loading, strict future-version handling, current-only image Copy and Manual-mode non-evaluating observers.
- **Verification:** Actual selector actions across frames; final loop source and zero-iteration change; old/new definition round-trip; rename/Undo with independent selection; stale/manual source and copy guards.
- **Acceptance:** No ambiguous duplicate “named” choices or stale physical binding masquerading as the current variable; migration and current-state tests pass.
- **Commit:** `feat(calc-pad): [M3-B] track final named chart vector sources`.

### M4-A — Dedicated Sensitivity Tables companion

- **Objective:** Move all existing sensitivity controls/results into a hosted dialog under Tools.
- **Ownership:** New sensitivity presentation module; `calculation_pad_dialog.rs`, `render.rs`, `utility_window_host.rs`, `gui/mod.rs`, panel/size preferences, session wake/poll seam.
- **Tasks:** Register the companion with established hosts; replace the embedded panel with one open action; reuse session/definition/worker; send retained typed actions to the parent; preserve draft/definition state appropriately; handle parent close/document switch and child repaint.
- **Behavior:** Opening does no work; Generate/Cancel/export behave as before; close retains definition/current table; dock/pop-out follows utility conventions; background completion reaches the visible child.
- **Edges:** Discarded frames, stale owner receipts, pending document switches, completion after close/retirement, parent hidden, rename/Undo while open, copied stale table.
- **Regression constraints:** No second session/worker/table store, timer polling, new hotkey or sensitivity model expansion. Main launcher query stays usable.
- **Verification:** Migrate existing retained sensitivity tests to the companion path; test open/current-owner actions, stale rejection, close/reopen retention, parent retirement and targeted wake behavior. Reserve actual desktop checks for M5-B.
- **Acceptance:** All old sensitivity functions are reachable in the new dialog; none remain embedded as a duplicate implementation; ownership/current-result guards and wake path are preserved.
- **Commit:** `feat(calc-pad): [M4-A] open sensitivity tables in a companion dialog`.

### M4-B — Usage visibility and Charts header layout

- **Objective:** Finish the two small requested presentation changes.
- **Ownership:** `CalculationPadDisplayPreferences`, Pad View menu and retained save effects; Charts `render_host`.
- **Tasks:** Add the default-hidden persisted usage preference and guard the existing usage panel; place presentation/Close controls in a horizontal row.
- **Behavior:** Usage stays hidden until enabled and survives restart. Dock/Pop out and Close are adjacent in both presentations.
- **Edges:** Legacy/malformed setting, discarded preference edit, pin-disabled Close, narrow window.
- **Regression constraints:** No source dirtiness/Undo/evaluation from visibility; no changed focus, close or host-request policy.
- **Verification:** Preference default/round-trip and retained-pass tests; confirm source/revision unchanged; one ordinary layout/interaction smoke check, not pixel comparison tests.
- **Acceptance:** All requested controls are in the stated menus/row, visibility persists, existing diagnostics remain independent.
- **Commit:** `fix(calc-pad): [M4-B] make usage optional and align chart controls`.

### M5-A — Executable reference and documentation

- **Objective:** The in-app Examples & Reference teaches the delivered behavior accurately.
- **Ownership:** `examples.rs`, `src/calculation_pad/README.md` and a short root README feature update.
- **Tasks:** Add compact executable examples for empties, append loops, transformations/slice/concat, vector-returning functions, purity, top-level failure preservation, loop failure, index errors and capacity; cover chart workflow and three UI changes. Update limits, automatic X, copy semantics and ordinary-versus-collection failed assignment explanation. Repair the catalog count assertion or replace it with a meaningful current expectation.
- **Behavior:** Examples are searchable and evaluable through the actual evaluator. Include the C5 scripts with correct newlines/indentation.
- **Edges:** Deliberate diagnostic examples must be marked and asserted as such; large examples should end with concise counts/reductions.
- **Regression constraints:** No stale “128”/“1,000 vector points” claims, unsupported syntax, duplicated external reference or unrelated documentation rewrite.
- **Verification:** `shared_examples_evaluate_to_their_documented_results`, catalog search and focused documented-boundary checks.
- **Acceptance:** Runtime, in-app catalog, linked reference and README agree on operations, limits and workflows.
- **Commit:** `docs(calc-pad): [M5-A] document dynamic vectors and analysis dialogs`.

### M5-B — Integrated evidence, independent review and handoff

- **Objective:** Establish the task's completion with bounded evidence and exact Git delivery.
- **Ownership:** Orchestrator/reviewer; implementer handles only concrete corrective changes.
- **Tasks:** Run the scoped consolidated checks, controlled performance evidence and native scenarios in H; review the cumulative task diff; resolve feature-related findings; perform one appropriate first-party Clippy pass; update final status.
- **Behavior:** All V/C/U requirements are implemented and each evidence level is stated truthfully.
- **Edges:** Native environment unavailable, unrelated old test/lint failures, push rejected, legitimate unresolved feature defect.
- **Regression constraints:** No full-suite obligation, unbounded remediation, weakened tests or expansion into the predecessor's unrelated features.
- **Verification/acceptance:** H–J satisfied or a precise bounded blocker recorded. Finish reporting commits before reading final HEAD; verify ancestry and actual push status.
- **Commit:** Use a focused `fix(calc-pad): [M5-B] …` only if corrections exist, then a concise verification/handoff documentation commit when needed. Do not manufacture an empty feature commit.
- **Final-tip handoff:** Record `parent_branch`, `parent_tip_sha`, task branch, `baseline_sha`, checkpoint SHAs and actual `final_tip_sha`. The next Goal branches directly from that final tip.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

M0-A is the required first baseline commit; never relabel the predecessor tip as the baseline. Use the F checkpoint boundaries and descriptive stage-tagged subjects. Commit coherent verified increments before dependent work; avoid one giant final commit and trivial per-line commits.

The orchestrator owns Git commits and one normal best-effort push after each checkpoint. Inspect status, complete intended diff and staged paths first. Stage only task changes. Preserve unrelated files and commits; never squash/rewrite history or force-push.

All checkpoints remain on the one new category branch. The same branch serves both documents and all agents. No worktrees, extra checkouts/clones, per-agent/per-checkpoint branches or intermediate merges are allowed. Local commits remain valid when pushing fails.

## H. Verification and testing strategy

### H1. Automated scope and execution

Prefer actual Calculation Pad inline library/module tests and narrowly affected GUI/settings tests. Inspect the live test list before selecting filters; an empty match is not evidence.

Relevant existing tests include:

- `vector_arithmetic_products_and_dependencies_use_scalar_components`; migrate `vector_invalid_shapes_types_and_nonfinite_work_are_local` without weakening unaffected errors.
- `vector_formatted_output_is_pasteable_with_display_precision` and new separate canonical-copy behavior.
- `formula_functions_are_pure_prior_only_and_track_definition_dependencies`.
- `failed_and_empty_loops_preserve_transaction_boundaries`; `nested_loop_control_preserves_bindings_and_real_errors_roll_back`.
- `loop_work_limits_and_cancellation_are_explicit`.
- `manual_mode_never_calculates_from_edits_or_observers`.
- `vector_values_follow_edits_history_save_reopen_and_inspector_rules`.
- `worker_cancels_old_work_and_coalesces_latest_pending_revision`.
- `aggregate_budget_and_cancellation_do_not_publish_partial_tables`.
- `nested_hints_and_syntax_use_shared_signatures`.
- `chart_open_requires_retained_current_pad_owner`; `chart_owned_popup_menus_accept_actual_selections_across_frames`.
- `chart_adapter_reports_length_errors_and_uses_only_current_snapshots`; `chart_copy_uses_scene_and_surfaces_clipboard_failure`.
- `chart_definition_roundtrip_and_rename_history_keeps_independent_selection`.
- `sensitivity_retained_generate_copy_and_stale_guards_in_manual_mode`, migrated to the new host path.
- `display_preferences_publish_only_after_retained_host_pass`.
- `shared_examples_evaluate_to_their_documented_results`.

Tests may be nested under different module paths; path-included editor tests reside under the Pad dialog's editor module. Use discoverable names, not assumed integration-test filenames.

Example focused command shapes, adapting to the live project configuration:

~~~text
cargo nextest list -p multi_launcher --lib -E '<selected actual test filters>'
cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev -E '<selected actual test filters>' --test-threads 1
cargo clippy -p multi_launcher --lib --bin multi_launcher --no-deps --profile fast-dev --locked
~~~

Use conservative build jobs on constrained hardware, one active build/test owner and one source writer. Test compilation is allowed; do not run extra builds that duplicate it without a reason. Build the native app once when needed for H3 and reuse it until a relevant fix requires rebuilding. No repeated release builds or full `cargo nextest run`.

Use a 100-second execution deadline for each cached focused batch, with compilation separately bounded to 900 seconds by an available process-owner wrapper. Do not apply the cached-test limit to a legitimate initial compile. If the environment demonstrably needs a different initial compile bound, record a concrete bounded adjustment rather than looping. Ensure owned processes are stopped and output drained on timeout. Do not change global test/compiler settings merely to mask failures.

### H2. Required behavioral evidence

| Area | Minimal meaningful evidence |
| --- | --- |
| Value/function boundaries | All twelve operations; empty/one/1,024/1,025; invalid element/index types and malformed syntax. |
| Pure semantics | Original alias/physical row unchanged; bare append does not rebind; prior worker snapshots unchanged. |
| Publication | Failed collection reassignment, failed physical reference, prior-error/no-prior cases; same behavior in both evaluator routes; loop restore/poison and ordinary scalar failure retained. |
| Language | Two 1,024-point vectors with C5 outputs; vector-returning formula and nested calls; source references/dependencies/rename. |
| Capacity and work | Literal/canonical admission; shared document and sensitivity aggregate work; cancellation; unchanged independent document/loop bounds. |
| Session | Manual edit versus explicit Calculate, stale observers/copy, Undo/Redo, save/reopen/recovery and retired worker results. |
| Charts | All three chart kinds; empty/one/1,024; exact length errors; zero-based X; final named sources, old physical sources, rename/Undo and persisted labels. |
| Dialog lifecycle | Retained open/actions, stale-owner rejection, close/reopen, parent close/document switch and child completion repaint. |
| UI preferences | Usage default/round-trip and source neutrality; current host/pin semantics preserved. |
| Documentation | Executable example catalog and discoverable function descriptions. |

Prefer focused tests with a clear behavioral assertion. Do not produce a combinatorial matrix duplicating coverage. Existing tests may be updated only for intentional contract changes, not to fit broken behavior.

### H3. Bounded real-application checks

When the environment supports the Windows application, build/launch it, perform these concise flows and capture relevant screenshots/evidence. For isolated app/test profiles, set Paste Sequence `next_hotkey = None` and `cancel_hotkey = None` before application construction, as `AGENTS.md` requires; preserve production defaults.

1. **Dynamic chart:** paste C5's 1,024-point script; inspect count/compact preview; copy a complete vector; select final X/Y names; use line, bar and scatter; pan/zoom/tooltip; verify one-point and empty messages.
2. **Manual freshness and source lifecycle:** select Manual mode, change an input/source, open Charts and Sensitivity Tables, confirm no implicit calculation and explicit stale state; Calculate/Generate appropriately; test a focused rename/Undo and save/reopen.
3. **Sensitivity companion:** open from Tools, edit a small definition, Generate/Cancel as applicable, view/copy a table; dock/pop out; confirm background completion appears with the Pad not actively rendering; close/reopen without deleting the definition/table; verify stale action/document-switch protection.
4. **Requested presentation and focus:** usage starts hidden, toggles under View and survives restart; Charts buttons are adjacent; dock/pop-out/close use correct ownership; the main launcher query remains editable while companions are popped out.

Combine compatible checks into one session. These are not pixel-comparison tests or a revival of the predecessor's six-scenario acceptance campaign.

If native interaction is unavailable, make at most two materially different bounded environment attempts, record exact unverified behaviors, and continue independent automated work. Do not ask the user for intermediate commands or screenshots. Report native acceptance as LIMITED/ENVIRONMENT_BLOCKED, never as passed from a headless image.

### H4. Performance and review budget

One controlled measurement pass covers required vector workloads and representative 1,024-point/eight-series chart data. Record profile/environment and observable results. Permit one focused optimization/retest pass if a concrete budget or responsiveness issue appears. Do not repeatedly benchmark once the acceptance evidence is sufficient.

Run one independent cumulative review after implementation. Resolve substantive findings through bounded implementer checkpoints and rerun only affected checks. Perform one final scoped first-party Clippy pass; fix task-introduced findings, record unrelated preexisting findings, and exclude vendored cleanup.

## I. Risks and bounded blocker handling

| Risk | Required mitigation |
| --- | --- |
| Raising capacity exposes hidden expression/parse/work caps | Treat literal admission, parser work, runtime work and canonical output as one tested foundation. |
| Immutable append amplifies cloning | Audit actual storage/publication copies; retain aliases; use measured bounded accounting and optimized ownership where justified. |
| Retained binding hides an error or uses old data | Keep failed physical rows, structural failure provenance, current-evaluation binding origin and precise dependencies; preserve terminal errors and loop poisoning. |
| Chart names select the initial/previous row | Distinguish named versus physical references, use current final bindings, test zero iterations and rename/Undo. |
| Saved chart references break old documents | Migrate/default explicitly; retain old physical selections and strict future-version protections. |
| New child acts on the wrong Pad or misses completion | Reuse retained owner receipts/run stamps and extend existing wake/poll ownership. |
| New UI preference dirties source or triggers calculation | Persist only through display settings; assert source/revision/Undo neutrality. |

For the same feature-related failure, use an initial diagnosis plus at most two focused corrective/retest cycles before recording a precise blocker. New concrete evidence may justify one explicitly bounded follow-up; ordinary compiler fixes are not reasons to stop for permission. Do not weaken acceptance to finish.

Distinguish an actual feature blocker, an unrelated preexisting issue, an environment-only evidence limit and a deferred optional improvement. Continue safe independent work when only one evidence channel is blocked.

At a genuine exhausted stop, follow current `AGENTS.md` stalled-Goal reporting under `docs/plans/execution/standalone/calc-pad-dynamic-vectors/stalls/` when safe. State failure/unrun check, expected/actual result, attempts/budget, current branch/ancestry/dirty state, preserved partial work and smallest next decision. Do not carry uncommitted work silently into a new category.

## J. Definition of done

The Goal is fully complete only when:

- Every V1–V6, C1–C2 and U1–U3 requirement is implemented through the intended owners.
- All required automated functional acceptance passes; no known feature-related blocking defect remains.
- Required native interactions have evidence, or the report explicitly distinguishes an environment-limited delivery from fully verified completion.
- The required 1,024-element generation, literal/canonical copying and chart paths are demonstrated without disabling bounded work/cancellation.
- Existing relevant arithmetic, loop transactions, scalar errors, source/history, Manual mode, persistence and ownership behaviors are preserved.
- In-app examples/reference and the brief README update match actual behavior.
- Independent review findings and task-introduced lint issues are resolved or accurately classified as blockers.
- The cumulative diff contains only intended task changes; small checkpoint commits exist on the correct child branch; normal pushes were attempted and reported truthfully.
- `parent_branch`, `parent_tip_sha`, `baseline_sha` and actual `final_tip_sha` are recorded, with ancestry verified and no worktrees/auxiliary checkouts/intermediate merges used.

Do not call missing functionality “native-limited.” An unimplemented requirement is partial/blocked. A push failure is local-only Git delivery, not an implementation failure.

## K. Final delivery report and next-goal handoff

Report concisely:

1. User-visible changes and V/C/U implementation/evidence matrix.
2. Important decisions: actual expression/work bounds and units, immutable value strategy, failure publication, named/physical chart references and format migration.
3. Completed checkpoints and relevant source paths.
4. Actual test/build/lint commands and results, measured workload evidence and native screenshots/check outcomes.
5. Remaining blockers/limitations and a short final user-review checklist.
6. Git: `parent_branch`, `parent_tip_sha`, task branch, `baseline_sha`, checkpoint SHAs, actual `final_tip_sha`, upstream/push state and clean/dirty checkout status.
7. Confirmation that no Git worktrees, auxiliary checkouts/clones, agent-specific branches or intermediate merges/rebases/cherry-picks were used.

Avoid a self-referential hash chase: finish the final tracked report commit, then read final HEAD and place that exact SHA in the authoritative external handoff/final response. A tracked ledger may reference the immediately preceding verified checkpoint and the final response as authoritative for final HEAD.

End with:

> The next Goal must create its fresh child branch directly from `<final_tip_sha>`. This tip contains the cumulative chain. Do not restart from master/main or an earlier common base. Final integration is a separate approved boundary.

Stop after this Goal's completed delivery or a documented bounded blocker. Do not merge, start another Goal or ask for an unnecessary approval round.
