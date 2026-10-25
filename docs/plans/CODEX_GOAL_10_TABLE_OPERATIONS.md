# Codex Goal 10 — Table transformations summaries and JSON/Markdown exchange

**Companion execution prompt:** [CODEX_KICKSTART_10_TABLE_OPERATIONS.md](CODEX_KICKSTART_10_TABLE_OPERATIONS.md)  
**Requested coverage:** #91, #93, #94  
**Program position:** 10 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `feat/table-operations` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish table-operations branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
6. **Attempt the initial upstream push once.** After the baseline commit exists, perform one normal non-force push to this task branch when an appropriate remote and platform approvals permit, for example `git push -u origin HEAD` when `origin` is verified as the intended repository. If denied/unavailable, record the real reason and keep working locally. Do not retry in a loop, bypass protections, or require the user to resolve ordinary push failure.
7. **Verify the bootstrap and remain on this branch.** Record branch, original base, baseline SHA, pre-existing work and initial push result in the execution ledger. A ledger created by the baseline commit can record its resulting SHA immediately afterward and include that update in the next legitimate checkpoint; do not create recursive commits merely to record their own hashes. Do not silently return to the prior branch.

### M0-A acceptance and commit contract

**Objective:** A unique active task branch with its own committed baseline and an honest initial push result.

**Code ownership:** Git state and an optional task execution ledger only; no application code or test execution yet.

**Tasks:** Complete steps 1–7 above, check the dependency contract below read-only, and keep existing user work untouched.

**Behavior after this checkpoint:** All later feature work and verification happen on the new branch and are measured against the new baseline commit.

**Edge cases:** Missing remote/upstream, denied fetch/push, existing branch name, dirty checkout, stale local integration tip, missing prerequisite or unsafe transition.

**Regression constraints:** No user change staged/lost, no destructive Git command, no force push/history rewrite, and no implementation on `master`, `main` or the old feature branch.

**Verification:** Inspect branch/HEAD, baseline commit, worktree/staged diff, base provenance and actual push output. No application build is needed to prove bootstrap.

**Acceptance:** Unique branch is active; baseline commit exists on it; original base and baseline SHA are recorded; push was attempted or its policy/access unavailability is recorded; unrelated changes are preserved. If branch/baseline cannot be established safely, stop with a precise blocker. Push failure alone is nonblocking.

**Commit boundary:** The first commit is the M0-A baseline. Every later checkpoint stays on this new branch; normal best-effort push follows each commit.


## A. Goal and desired outcome

Use the existing Table/CSV dialog to preview and apply column cleanup, remove duplicate rows, summarize chosen data, import row-shaped JSON and export Markdown. CSV operations work across complete 500 MiB+ datasets through Goal 09's bounded storage and job contracts. The source remains read-only; derived changes can be undone or discarded before an explicit export.

### Dependency and branch-base contract

**Required predecessor capabilities:** [09 — Large CSV files with bounded memory and complete-file operations](CODEX_GOAL_09_LARGE_CSV.md).

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [06 — Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/table_utility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility.rs)** — `TableDocument; TableProjection; TableSelection; TableUtilityProfile`. The existing engine models literal string cells, profile-specific parsing/sorting, source-bound row IDs and selected rows in view order. It currently has no column transformation, deduplication or summary operation.
- **[src/table_utility_session.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility_session.rs)** — `TableExportReceipt; request_export_with_basis; export_receipt_is_current (93–107, 973–1124)`. Exports capture lifetime, source, view and selection identities and reject stale publication. This is the correct pattern for transform previews and summary results.
- **[src/gui/table_utility_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/table_utility_dialog.rs)** — `show_results; show_footer; save_export_target`. Current row/column selection feeds explicit TSV/CSV/JSON copy and JSON Save As. Source files cannot be overwritten; existing destinations require confirmation.
- **[src/table_utility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility.rs)** — `write_json_table; TableJsonShape; object_header_problem (2068–2099, 2243–2350)`. Current table-to-JSON exports all cell values as strings; object output rejects invalid headers or Auto falls back to arrays. Preserve this behavior.
- **[src/json_transform.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/json_transform.rs)** — `JsonDocument; resolve_pointer; JsonNodePath; copy_value (48–171)`. The JSON utility already has ordered object handling and typed JSON Pointer semantics. Its number model uses serde_json::Number; verify token preservation before reusing its parser for lossless number-to-text import.
- **[Cargo.toml](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/Cargo.toml)** — `serde_json and csv dependencies`. Existing JSON and CSV libraries are available. A full SQL/DataFrame dependency is not needed by default.
- **[src/table_utility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility.rs)** — `finite_number; table_projection_json_shapes_preserve_strings_and_report_header_fallbacks`. Existing sorting uses finite-number classification without rewriting source cells. New summary arithmetic needs an explicit precision/overflow policy rather than silently reusing floating-point conversion for exact integer totals.

## C. Approved functional requirements

1. Hard prerequisite: Goal 09's complete-file row cursor, selection/view identity, resource policy, spill/sort and streaming export contracts must exist in the chosen branch base. Do not silently merge a prior branch or implement a second resident-only pipeline.
2. Add literal-delimiter split, ordered join and trim for selected columns, plus exact duplicate-row removal. Preview an operation before Apply; Apply produces a derived table, and Undo last apply or Discard all derived changes restores the corresponding accepted result without modifying the source.
3. Transform scope is explicitly selected rows in the current complete filtered view; when no explicit row selection exists, it is every matching row. Unselected and filtered-out rows remain unchanged. Freeze the pre-operation scope by its source/view/selection identities so changed values cannot make membership shift mid-job.
4. Split uses a nonempty literal delimiter, preserves empty/trailing fields and appends named derived columns while retaining the input column. Join appends one named column in the chosen input-column order with a literal separator; empty cells remain meaningful. Trim removes surrounding Unicode whitespace only. Do not add regex transforms, formulas or automatic type conversion.
5. Preview shows the exact scope, intended schema, sampled before/after rows, complete affected-row count when known and truncation labels for the sample. It may be cancellable, but Apply stays disabled until required whole-file validation succeeds. New column names cannot silently overwrite existing ones.
6. Derived state retains an immutable source plus bounded operation/provenance descriptors and quota-accounted materializations when necessary. Share immutable inputs and retain only one Undo checkpoint; do not copy the entire dataset for every preview/apply/undo. Limit a retained chain to 32 operations and each descriptor to 64 KiB, failing explicitly or offering export-and-reopen at the limit.
7. Deduplication compares the explicitly chosen key columns, defaulting to all selected columns, with case-sensitive exact decoded text and no implicit trimming/Unicode normalization. Keep the first eligible row in the captured current view order; rows outside the captured eligible scope remain. Hash collisions must be resolved by exact key comparison.
8. Summaries offer Whole source/current derived table, All filtered rows and Selected rows with the active choice prominent; default to selected rows if explicitly selected, otherwise all filtered rows. Never summarize only loaded pages unless that page's rows were explicitly selected. Report total rows, nonblank/numeric/nonnumeric counts, numeric sum/average, blank counts and exact distinct-value counts per chosen column.
9. Blank means an empty decoded string; whitespace is not silently removed. Distinct counts compare the complete raw decoded cell text, case-sensitively, including the empty string as a distinct value. Numeric analysis may trim surrounding whitespace without changing the source or distinctness rules. Invalid/nonfinite numbers are counted separately.
10. Counts and distinct values are exact. Accumulate eligible decimal values with checked bounded decimal arithmetic, preserving integers beyond f64's exact range: support up to 18 fractional decimal places and a checked i128 coefficient after exponent normalization. Unsupported precision/range or sum overflow makes the affected numeric total/average explicitly unavailable, while counts remain valid. Do not silently omit an otherwise numeric out-of-range value. Retain the exact sum and numeric-count denominator; show a rounded average to at most 12 fractional places with an explicit rounding indicator, using half-even rounding.
11. Exact distinct/dedup operations use Goal 09 spill/sort primitives when needed. RAM/disk exhaustion or cancellation must not turn into an approximate answer, sampled answer or partially applied derived result.
12. Import JSON from explicit paste/file actions, initially bounded to 16 MiB input, 100,000 rows, depth 64 and Goal 09's column/field/record and aggregate budgets. Accept a top-level array of objects or arrays; allow an explicit JSON Pointer to a row array inside a wrapper. Reject mixed object/array/scalar rows and duplicate keys in a row object with a location.
13. JSON object columns form a deterministic union in first-encounter order; missing properties become empty text. JSON strings retain decoded text, numbers retain lossless token text without f64 conversion, booleans become true/false, null becomes literal null, and nested arrays/objects become compact JSON text without flattening or row explosion. Preserve exact number tokens inside nested values too.
14. Before JSON import Apply, state that this is a text-table conversion: missing/empty values can converge, null and the string null can converge, and later existing JSON export remains all-string. Show the mapping with a bounded sample. Do not claim typed or byte-identical round trips. Existing JSON formatter behavior is unchanged.
15. Add Markdown table copy and Save As using current complete view/selection order and captured receipts. Escape literal pipes, backslashes, Markdown control characters and HTML-sensitive text; encode cell line breaks as deliberate <br> separators after escaping source text. Generate clear positional headers for headerless sources without silently renaming existing labels.
16. Markdown clipboard output uses the existing bounded clipboard policy; large file export streams under Goal 09 quotas. It is a readable representation, not a byte-preserving serialization. No silent row/output truncation; original source paths remain prohibited export destinations.
17. All new jobs share the session generation/cancellation owner and optional existing activity registration. Update relevant Jump Mode controls and help; preserve current Table versus CSV defaults, JSON export fallback and previous-valid-result semantics.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- No spreadsheet formulas, arbitrary scripts/SQL, charts, joins between independent tables, Excel/Parquet formats or general ETL platform.
- No 500 MiB JSON-import promise: the approved large-file requirement is implemented for CSV/TSV through Goal 09; JSON input remains explicitly bounded.
- No automatic nested JSON flattening, array explosion, inferred date/currency types, approximate distinct counts or source overwrite.
- No unlimited undo history, dataset duplication per edit, broad JSON formatter migration or unrelated clipboard changes.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Resolve the actual engine/core package after bootstrap and after Goal09. Existing src/table_utility.rs references identify the reviewed public boundary; if it is now a facade from Goal03, implement in the real core and preserve independent engine test compilation. Never move domain logic back into the facade or duplicate it.
2. Extend the storage-neutral table engine with typed operation plans, source-bound scope descriptors and immutable derived-result generations. Keep presentation out of domain transformations and keep native side effects in the existing dialog/export owner.
3. Treat Preview, Apply and Undo/Discard as distinct state transitions bound to the captured source/view/selection. A preview result is not authority to publish against a later source or changed selection.
4. Use streaming transforms for split/join/trim and external exact-key processing for dedup/distinct. Reuse the large-file row/spill owner rather than converting its data to Vec<Vec<String>>.
5. Keep JSON conversion policy narrow and explicit. Reuse JsonNodePath/Pointer conventions where suitable, but do not pass lossless number text through an imprecise Number/f64 round trip. A narrowly justified raw-value capability is preferable to a parallel JSON framework.
6. Refactor existing exporters around shared output/receipt handling rather than duplicating overwrite checks for Markdown. Treat new settings/labels and help as current capability updates only.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Add bounded derived results and column transformations

**Objective:** Preview, apply, undo and discard split/join/trim through one source-bound operation pipeline.

**Code ownership:**

- src/table_utility.rs and focused table modules established by Goal 09
- src/table_utility_session.rs
- src/gui/table_utility_dialog.rs

**Implementation tasks:**

1. Inspect the integrated Goal 09 interfaces and define typed transform/scope/provenance descriptors with explicit resource admission and stable row/column identity.
2. Implement streamed split, join and Unicode trim with whole-scope validation, exact affected counts and a bounded preview sample. Determine new column width across the full eligible source before publishing a new schema.
3. Add Preview/Apply/Undo last/Discard state transitions. Freeze selected-row predicates, preserve unaffected rows and retain one bounded undo plan rather than complete cloned datasets.
4. Wire compact forms, new column naming, scope labels, progress and Jump Mode targets into the existing dialog. Cancel obsolete previews when options, source, selection or view change.

**Behavioral requirements:**

- Source bytes never change; Apply changes only the derived result.
- The UI distinguishes sample rows from a validated whole-file operation, and Undo restores the preceding derived plan.

**Edge cases:**

- Empty/repeated delimiter, trailing split fields, inconsistent split widths, column collisions/limits, empty eligible scope, Unicode whitespace, enormous output field, stale preview and quota exhaustion.

**Regression constraints:**

- Current Table/CSV policy and literal untransformed values remain unchanged.
- No per-edit full dataset copy or unbounded descriptor history.

**Verification:**

- Use small fixtures with exact before/after tables, selected/filtered-out rows and schema edge cases; force spills through tiny budgets.
- Inject stale Apply, cancel mid-preview and Undo after one apply; assert source checksum and original accepted result remain intact on failure.

**Acceptance criteria:**

- Each operation works through the same resident and file-backed row contract.
- Preview/Apply/Undo/Discard are identity-safe, resource-bounded and preserve excluded rows.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(table): [M1-A] add previewed column transformations` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Add exact deduplication and scope-aware summaries

**Objective:** Compute exact full-scope duplicate removal and column summaries without unbounded in-memory sets.

**Code ownership:**

- src/table_utility.rs and Goal 09 spill/sort helpers
- src/table_utility_session.rs
- src/gui/table_utility_dialog.rs

**Implementation tasks:**

1. Implement key-column deduplication with exact collision resolution and deterministic first-in-view survivor policy; keep noneligible rows and expose removed/retained counts in preview.
2. Implement explicit Whole/Filtered/Selected summary requests with generation-bound results, exact counts/distinct values and bounded external processing for high-cardinality inputs.
3. Implement checked decimal numeric classification/accumulation and the documented precision/overflow/average-rounding contract. Keep raw values and distinct keys unchanged.
4. Show scope, complete/pending status, blank/numeric/nonnumeric counts, exact sum and rounded average indicators. Cancel/revoke obsolete summaries after data/view/selection changes.

**Behavioral requirements:**

- Duplicate removal and distinct counts cover late rows and spill boundaries.
- A partial or stale summary never appears as the complete current result; numeric overflow is visible rather than silently rounded.

**Edge cases:**

- All identical/all distinct keys, empty key cells, case/whitespace differences, collisions, sorted view survivor order, blank-only/mixed numeric columns, large integers, exponents, sum overflow and repeating averages.

**Regression constraints:**

- Exact means exact for row/count/distinct/dedup results.
- Memory and disk quotas remain aggregate across transformation and summary owners.

**Verification:**

- Compare streamed/spilled results with a straightforward trusted small-fixture oracle, forcing duplicates across multiple spill runs.
- Verify 9007199254740993 and decimal cases without f64 loss, denominator rules, overflow unavailability and rounding indication; test stale/cancelled summary publication.

**Acceptance criteria:**

- Dedup survives collision and cross-run cases without dropping noneligible rows.
- Whole/Filtered/Selected scopes produce the documented exact counts and numeric behavior under tight budgets.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(table): [M1-B] add exact deduplication and summaries` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Integrate JSON import Markdown export and complete workflow evidence

**Objective:** Move API-style data into the table and selected complete results into Markdown through explicit safe actions.

**Code ownership:**

- src/table_utility.rs and table import/export helpers
- src/table_utility_session.rs
- src/gui/table_utility_dialog.rs
- src/json_transform.rs only for necessary reusable path/token boundaries
- Scoped command/help/settings documentation

**Implementation tasks:**

1. Implement bounded JSON admission and row-array/Pointer selection, deterministic union columns, duplicate-key and shape diagnostics, and the explicit text conversion mapping. Keep number lexemes lossless, including nested compact JSON.
2. Present a bounded conversion preview and type-loss explanation before adopting the imported table. Do not change the existing JSON formatter's duplicate-key or number behavior merely to add table import.
3. Add Markdown copy/Save As through streaming sinks and existing receipt/overwrite/source-safety checks; escape cell content and make headerless/newline representation explicit.
4. Complete Jump Mode/focus integration and help for transform scope, quotas, numeric precision, JSON type conversion and Markdown representation.
5. Run one bounded native scenario including large-file transform/dedup/summary with late rows, undo/discard, JSON import and Markdown output, then perform the focused final review.

**Behavioral requirements:**

- JSON files or pasted arrays become a previewed text table; wrappers work only through an explicit valid Pointer.
- Markdown exports exactly the chosen complete result scope with safe readable cell content.

**Edge cases:**

- Missing/null/empty/string-null values, numeric tokens beyond f64 precision, nested containers, duplicate row keys, heterogeneous rows, invalid Pointer/depth/size, malicious HTML-like text, pipes/newlines/backticks and destination races.

**Regression constraints:**

- Existing all-string JSON export and header fallback remain unchanged.
- No implicit clipboard reads, source overwrite, silent output truncation or typed-roundtrip claims.

**Verification:**

- Add exact fixture tests for JSON mapping/token preservation/schema order and Markdown escaping/row order; retain existing previous-valid/export receipt tests.
- Use the Goal 09 generated >500 MiB fixture or generate an equivalent owned fixture once; verify known late duplicates/counts/transforms, tracked resource peaks and cleanup.
- Capture a small native screenshot set for preview/apply/undo, complete summary scope, JSON conversion warning and Markdown export result.

**Acceptance criteria:**

- All #91/#93/#94 flows are usable through existing routes and preserve source data.
- Large CSV transformations and exact summaries complete within configured budgets, while JSON limits/type conversion and Markdown representation are explicit.
- Focused automated/native evidence covers both successful output and cancellation/stale refusal.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(table): [M2-A] add JSON import and Markdown export` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Run only identified table engine/session/dialog tests and the directly touched JSON path/token tests. Resolve exact target/features through the current manifest/Goal 01 tooling; no full repository suite is required.
- Use deterministic small or synthetic datasets with deliberately tiny budgets for routine spill/precision/schema/identity cases. Keep 500 MiB+ fixtures out of ordinary unit tests.
- Reuse one generated file larger than 500 MiB for a single full-scope transform, exact dedup/distinct summary and streaming export acceptance workflow. Expected late-row counts and output checksums must be independently known; a screenshot of a loaded page alone is insufficient.
- Budget the first changed-target compile/build separately up to 45 minutes, subsequent targeted builds up to 25 minutes, selected test execution up to 10 minutes and the full large-data workflow up to 20 minutes. Use process ownership/cleanup and report a timed-out gate accurately.
- At most two focused remediation attempts per concrete failure. Repeats or a larger fixture require a specific remaining uncertainty, not general reassurance. Record tool-owned memory/temp peaks and observed process overhead separately.
- Final review must inspect source safety, exact scope, token/precision preservation, stale receipts and bounded undo/provenance; apply end-only first-party lint policy.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| Whole-file operations accidentally operate only on the displayed page. | Every job takes a complete source/view/scope descriptor; test known matches and duplicates beyond the page and across spill runs. |
| Undo or preview duplicates enormous datasets. | Keep shared immutable source plus bounded plan/provenance and one undo checkpoint; account any necessary disk materialization under Goal 09 quotas. |
| JSON number conversion silently changes identifiers or values. | Retain number token text without f64; test values beyond IEEE-754 integer precision and exponent forms, including nested JSON. |
| Numeric summaries conceal precision loss or omit unsupported values. | Checked bounded decimal sums, explicit overflow/precision unavailability and labeled rounded averages; preserve exact counts and original text. |
| Markdown content breaks tables or becomes active HTML. | Escape source Markdown/HTML characters before inserting controlled newline markup; test hostile-looking literal content and round-trip selected value order. |
| New work violates existing last-good/selection identity guarantees. | Apply and summary/export publication use captured lifetime/source/view/selection/derived generations and revoke on change. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Split/join/trim/dedup are previewed, applied and undoable/discardable without source writes; unaffected rows remain intact.
- Whole/Filtered/Selected summaries have exact scope/count/distinct semantics and the documented numeric precision/overflow behavior.
- A >500 MiB CSV completes selected transformations and exact summaries through the integrated bounded engine with verified late-row evidence.
- JSON import and Markdown copy/Save As follow explicit shape/conversion/escaping rules; raw number tokens are preserved and current JSON exports stay compatible.
- All new work is cancellable, stale-safe and within aggregate RAM/temp quotas; meaningful targeted and native verification is complete or truthfully blocked.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Report transform scope/survivor policy, preview/apply/undo model, exact summary definitions and numeric precision limits.
- Show representative JSON-to-text mapping and Markdown escaping examples, plus explicit input/output limits.
- Provide large-file size/shape, independently verified counts/checksums, memory/temp peaks and cancellation/cleanup evidence.
- List precise focused commands, native screenshots, source checksum preservation and any unresolved feature-specific limits.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
