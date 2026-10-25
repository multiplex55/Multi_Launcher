# Codex Goal 09 — Large CSV files with bounded memory and complete-file operations

**Companion execution prompt:** [CODEX_KICKSTART_09_LARGE_CSV.md](CODEX_KICKSTART_09_LARGE_CSV.md)  
**Requested coverage:** Explicit 500 MB+ CSV request; approved complete-dataset processing  
**Program position:** 09 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `feat/large-csv` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish large-csv branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Open and work with CSV/TSV files larger than 500 MiB without retaining the entire input, every cell, or every row identifier in RAM. Browse chosen columns and pages, then filter, sort and export the complete dataset through cancellable jobs. Existing small Table/CSV workflows keep their current defaults. Temporary disk work is approved, subject to explicit quotas; the original source remains read-only.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [06 — Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/table_utility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility.rs)** — `TableResourceLimits; TableUtilityProfile; TableDocument; TableProjection (16–34, 126–220, 429–460, 1596–1707)`. Table currently admits 10 MiB/50,000 rows/128 columns; CSV admits 16 MiB/100,000 rows/256 columns. Both cap cells at one million. Documents retain every cell as String and projections retain all matching row IDs. CSV and Table share the engine but differ in header, delimiter, width and sorting policy.
- **[src/table_utility_session.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility_session.rs)** — `read_table_file; TableSessionSnapshot; TableWorker; set_row_selected (332–502, 889–931, 1555–1590)`. File reads append chunks into one complete buffer. Session source text and parse input are retained, while deselecting one row from all-selected can allocate a set of all other visible rows. A single Condvar worker already provides latest-job cancellation, completion publication and idle sleep.
- **[src/gui/table_utility_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/table_utility_dialog.rs)** — `show_source; show_results; save_export_target (888–1218, 1973–2030)`. The UI already virtualizes rendered rows and bounds raw-source preview. It renders every column for each displayed row. Export is explicit, source replacement is refused and existing destination replacement is confirmed.
- **[src/table_utility_session.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility_session.rs)** — `TableExportReceipt; export_receipt_is_current; close; shutdown`. Lifetime/source/view/selection identities reject stale work and exports. Closing cancels work but retains loaded session data; shutdown joins the worker.
- **[src/table_utility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility.rs)** — `export_delimited_inner; export_json_inner; BoundedOutput`. Current exporters build complete bounded output strings. Full-file disk export needs a streaming sink while clipboard output remains bounded.
- **[src/settings/model.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/settings/model.rs)** — `ClipboardModifyPluginSettings and Settings serialization conventions`. Non-sensitive tool preferences already use serde defaults; large-data preferences can follow that convention. No existing configurable large-table policy was found.

## C. Approved functional requirements

1. Implement the approved full-dataset contract: file sizes above 500 MiB support projected/paged browsing, whole-file filtering, stable sorting and streaming CSV/TSV/JSON file export. A loaded page or sample must never stand in for the complete dataset.
2. Keep the existing compact inline/paste parser and current Table versus CSV defaults. Large files use the shared table domain with a file-backed storage mode, not an increased whole-buffer limit. Use 64-bit file offsets and checked row/count arithmetic.
3. Default aggregate table working-data budget: 256 MiB, configurable from 64 to 1024 MiB. This covers active and retained table-owned buffers, indexes, selections, caches, previews and job scratch; it is not a promise about total process RSS. Reserve capacity before allocation and account for overlapping old/new generations.
4. Default aggregate owned temporary-data budget: 8 GiB, configurable from 1 to 64 GiB, while retaining at least 1 GiB free on the temporary volume. Charge staging outputs, spill runs, indexes and retained derived data to the quota; handle actual disk-full races. No silent fallback to unlimited RAM or another volume.
5. Bound individual input shapes as well as totals: initially admit at most 1024 source columns, 8 MiB of encoded/decoded bytes per logical record including the header, and 4 MiB per decoded field. Apply checks incrementally, including during header/delimiter sampling; reject excess with a precise explanation. Keep small-route historical limits unchanged.
6. Expose resource preferences through existing settings conventions with serde defaults and validated ranges. Apply changed limits to the next admitted operation, explain active-job settings, and never persist source contents, temporary paths or job receipts as preferences.
7. Page/cache allocation is subordinate to the aggregate budget. Projected columns reduce retained cell data; explain that a CSV scan may still read the full byte stream. Do not build an unbounded offset vector, selected-row set, sort-key vector or diagnostic list.
8. Opening/scanning and full-file operations run outside the UI thread. Show scanned bytes/rows, phase and cancellation. Label early pages as a scanning preview until source validation completes; unknown totals remain unknown. No success/export publication from a partially validated file.
9. Whole-file filter/sort jobs require explicit application in large mode and preserve existing string/ordering semantics. Sorting is stable by original row identity for equal keys; AutoNumeric eligibility follows existing Table policy, while CSV remains lexical by default.
10. Detect source replacement/change during reads and prevent mixing revisions. Bind accepted data to a consistent source snapshot or equivalently validated file-backed handle; invalidate stale pages and offer explicit reload. Never write the source.
11. Full-file export streams into an owned temporary destination and atomically publishes only successful output, respecting source-path refusal and overwrite confirmation. Clipboard copy stays capped and never silently truncates. Cancellation or quota failure leaves the accepted result and existing destination unchanged.
12. Provide reusable bounded row cursors, exact selection predicates and spill/sort primitives needed by Goal 10 for exact deduplication and distinct counts. Do not implement those user features in this goal.
13. Reuse Goal 06 activity/cancellation registration if its implementation is present, without introducing a hard dependency or a duplicate global scheduler. Local table progress and cancellation must work independently.
14. Preserve any integrated independent core compilation boundary from Goal03. Resolve re-exports before edits, keep one implementation owner, and use the actual core package/test targets; Goal03 remains recommended rather than a hard prerequisite.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- No database server, SQL query language, broad DataFrame framework, XLSX/Parquet support or generic data-platform redesign.
- No change to small-file Table/CSV parsing defaults, implicit clipboard access, automatic source overwrite, approximate whole-file answers or page-only substitution.
- Do not add transformations, summaries, JSON import or Markdown export here; those belong to Goal 10.
- No repository-wide performance campaign, unrelated test consolidation or full-suite requirement.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. After bootstrap locate the actual pure table implementation owner. If Goal03 moved it into an internal core package, extend that owner and retain src/table_utility.rs as a thin public facade; otherwise keep the current engine ownership. Session/generation/job ownership remains at the actual TableUtilitySession owner and presentation at the dialog. New focused engine submodules belong under the actual core, not a duplicate application engine.
2. Introduce one storage-neutral document/row-access boundary serving the compact resident document and bounded file-backed document. Preserve typed source/view/row identities. Prefer the existing csv/serde_json/tempfile facilities; demonstrate necessity before adding a dependency.
3. Use quote-aware streaming records, bounded caches and on-disk indexes or sparse checkpoints. Chunked merge processing provides stable full-file sort and later exact aggregate/dedup primitives without storing all keys in memory.
4. Represent all-selected/excluded selections and filtered views compactly. A single deselection must not allocate one entry per source row. Store large explicit selections or result indexes under the same disk quota.
5. Carry the existing latest-generation cancellation/receipt model across scans, pages, filters, sorts and streaming outputs. Keep worker ownership explicit; completion, cancellation and cleanup update resource accounting exactly once.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Establish bounded source storage and resource policy

**Objective:** Admit large files through a resource-accounted file-backed document owner while preserving compact parsing.

**Code ownership:**

- src/table_utility.rs
- src/table_utility_session.rs
- src/settings/model.rs
- src/settings/defaults.rs and existing settings read/save owners after inspection
- Proposed focused src/table_utility/ storage and resource-policy modules

**Implementation tasks:**

1. Map existing document/selection consumers and introduce a storage-neutral access contract without exposing a full-source string requirement to file-backed documents.
2. Implement aggregate RAM/disk accounting, validated configurable defaults and incremental record/field/header/column guards. Include retained last-good data and active replacement work in admission decisions.
3. Implement bounded quote-aware scan/source identification and a disk-backed or sparse index with checked 64-bit offsets. Bound delimiter detection and diagnostic examples.
4. Expose a typed source-opening session method that future file routing can call. Preserve current small-profile policies and reject invalid large input without relabeling last-good data.

**Behavioral requirements:**

- Large files enter a cancellable scan rather than the 16 MiB whole-buffer path.
- Old settings deserialize to conservative defaults; resource refusals explain the limit and preserve the prior result.

**Edge cases:**

- Quoted embedded newlines, escaped quotes, BOM/Unicode, malformed tail, enormous first record, wide header, source replacement, arithmetic overflow, unavailable temp directory and disk-full.

**Regression constraints:**

- Existing compact parser semantics and original source bytes remain unchanged.
- No new source-content logging or implicit clipboard reads.

**Verification:**

- Focused engine tests compare compact and streaming parse results on identical tricky CSV/TSV fixtures.
- Inject small RAM/disk/shape limits to prove admission and overflow fail before unbounded allocation or result publication; test legacy preference decoding.

**Acceptance criteria:**

- All large-mode data owners have accounted limits; one oversized field cannot evade them.
- A valid file beyond the former limit is scanned through the new owner; malformed input preserves last-good identity.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(table): [M1-A] add bounded file-backed table sources` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Connect projected pages and compact selections to the dialog

**Objective:** Browse a large source without whole-file UI snapshots or unbounded row-selection metadata.

**Code ownership:**

- src/gui/table_utility_dialog.rs
- src/table_utility_session.rs
- src/table_utility.rs

**Implementation tasks:**

1. Replace file-backed source TextEdit/raw-string assumptions with bounded previews, phase labels and asynchronous page requests bound to source/view generations.
2. Add projected-column selection and bounded paging or virtual access, including horizontal rendering only for relevant visible columns. Do not read/parse pages on the GUI thread.
3. Replace large-mode all-but-one HashSet allocation with compact inclusion/exclusion predicates or quota-accounted backing storage.
4. Keep first-page previews explicitly provisional during scanning; make total counts and export eligibility truthful, and register new controls through existing Jump Mode ownership.

**Behavioral requirements:**

- The dialog remains interactive while scanning and requesting pages.
- Changing projection/page cancels obsolete work; default all-selected and a single deselection stay bounded.

**Edge cases:**

- Rapid page changes, filter changes during fetch, close/reopen, no selected columns, page at EOF, huge cells, no complete page yet and keyboard focus while jobs finish.

**Regression constraints:**

- Small-file interaction and profile switching retain existing behavior.
- Only matching lifetime/source/view data may replace displayed state.

**Verification:**

- Extend the existing virtualization/idle-frame test with page-provider counters and tiny budgets.
- Exercise stale page completion and all-minus-one selection against many logical rows without constructing all rows in the fixture.

**Acceptance criteria:**

- Idle frames submit no new scans; rendered/page data obeys bounded cache budgets.
- Page and column changes do not publish stale data or allocate metadata proportional to every row.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(table): [M1-B] browse projected large-file pages` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Implement complete-file filtering sorting and streaming export

**Objective:** Provide real full-dataset operations under the same quotas.

**Code ownership:**

- src/table_utility.rs and new bounded row/spill helpers from M1-A
- src/table_utility_session.rs
- src/gui/table_utility_dialog.rs

**Implementation tasks:**

1. Implement whole-file filter scans and stable external sorting with bounded merge fan-in, record/key buffers and quota-accounted intermediate runs.
2. Preserve existing lexical/AutoNumeric policy and source-order ties. Bound filter text and treat excluded columns needed as sort/filter keys as temporary job inputs.
3. Refactor exporters around streaming sinks; retain bounded clipboard outputs and current JSON string serialization/fallback rules.
4. Stream file output to a same-destination-volume staging file, revalidate the captured receipt before atomic publication and preserve current overwrite/source refusal.
5. Expose row cursors and exact view/selection operations that Goal 10 can reuse without reintroducing whole-dataset materialization.

**Behavioral requirements:**

- Operations apply across every valid source record, including matches beyond the initial page.
- Large outputs can exceed clipboard limits through file export; partial output never becomes the final destination.

**Edge cases:**

- High-cardinality keys, equal sort keys spanning runs, empty results, long keys, quota exhaustion mid-merge, destination race, source change and cancellation before publish.

**Regression constraints:**

- No silent sampling, approximation, truncation or source overwrite.
- Output order and literal cells match existing small-engine behavior on equivalent inputs.

**Verification:**

- Run compact-versus-file-backed result equivalence tests for filter/sort/export, forcing multiple runs with tiny budgets.
- Test stale receipt, cancellation, disk-full and destination replacement with fault injection; compare complete streamed output to deterministic expected digests/row counts.

**Acceptance criteria:**

- Matches in late file regions and cross-run sort ties are correct.
- Every intermediate allocation/file belongs to the resource policy and unsuccessful jobs preserve accepted outputs.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(table): [M2-A] process and export complete large datasets` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Finish cancellation cleanup and verify 500 MiB operation

**Objective:** Deliver measured resource behavior and a usable complete large-file workflow.

**Code ownership:**

- src/table_utility_session.rs
- src/gui/table_utility_dialog.rs
- Existing developer/native smoke harness owners after inspection
- Scoped documentation and settings/help integration

**Implementation tasks:**

1. Add explicit cancellation for scan/page/filter/sort/export, ensure obsolete work relinquishes resources, and remove owned temporary files on cancellation, close/replacement and normal shutdown according to retained-result policy.
2. Use a scoped manifest for stale owned-temp cleanup; never recursively delete arbitrary temp paths. Retain only bounded metadata needed to reopen, and revalidate the source before reuse.
3. Surface phase, owned working-data and temporary-disk usage; integrate the existing activity view only if available.
4. Generate one deterministic CSV larger than 500 MiB with late matching rows, duplicate keys, quoted fields and a known expected result; perform the native workflow and record actual resource/cancellation measurements.
5. Update help with full-file versus preview state, limits, temporary disk use, source safety and resource preferences.

**Behavioral requirements:**

- Cancel returns the dialog to usable state and a subsequent operation starts without stale publication.
- Resource failures are actionable; large-mode limitations are explicit.

**Edge cases:**

- Cancel during merge/output, app close with work active, normal reopen, crashed prior session temp files and insufficient free-space reserve.

**Regression constraints:**

- Only owned processes/files are cleaned up; existing data and unrelated temporary files remain untouched.
- Measured evidence is distinguished from estimates and unsupported environment claims.

**Verification:**

- Run one targeted native open/project/filter/sort/export/cancel/reopen scenario, capture relevant screenshots and exact generated-file size.
- Record tracked peak allocations, observed process private-memory/working-set delta, temporary-disk peak and release behavior. Confirm budget compliance rather than calling 256 MiB a whole-process RSS cap.

**Acceptance criteria:**

- A real file larger than 500 MiB completes full-file filtering, stable sorting and streaming export under default quotas with verified late-row results.
- No growing table-owned allocations or owned-temp accumulation remains after bounded repeated cancel/reopen operations; any unverified native requirement is explicitly blocked.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(table): [M2-B] finalize large-file lifecycle and acceptance` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use focused existing table engine/session/dialog and typed route tests plus new behavior tests. Preserve meaningful previous-valid/export identity assertions. Do not invoke the full repository suite.
- Use tiny injected budgets and synthetic row providers for routine limit/spill/cancellation tests. Generate the >500 MiB acceptance fixture on demand outside ordinary unit tests and remove only owned artifacts.
- At most one baseline inspection and one complete candidate large-data run; allow a second size or up to three repeats only to resolve a concrete memory-scaling or variance question. Do not invent a current-engine 500 MiB speed baseline: it currently rejects that input.
- Budget the initial targeted compile/build separately (up to 45 minutes on the reference Windows machine), later changed-target builds up to 25 minutes, selected test execution up to 10 minutes and the complete large-data acceptance pipeline up to 20 minutes. Timeout means investigated/blocked evidence, never automatic success. Use owned-process cleanup.
- Take one bounded native screenshot set showing resource settings, provisional versus complete data, projected page, full-file operation and export result. No intermediate user execution is required.
- After at most two focused remediation attempts for a concrete failure, report its exact command, elapsed time, source/result identities, retained resources and smallest blocking issue. Final first-party lint/review follows the package common policy.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| Paging hides full-source and per-row metadata retention. | Account all source/row/selection/output owners and exercise large logical row counts with tiny budgets. |
| CSV record boundaries or source edits produce skipped/duplicated rows. | Use quote-aware records, checked offsets, consistent source identity and small-engine equivalence tests including multiline input. |
| Sort/index/spill work fills disk or survives cancellation. | Aggregate quota/free-space reserve, owned manifests, atomic output publication and injected failure/cancellation tests. |
| A heavyweight data engine undermines compilation goals. | Prefer existing libraries and narrow row/spill primitives; justify any new dependency with demonstrated need and measured build impact. |
| Process RSS is mistaken for the configured working-data budget. | Define the budget as aggregate tool-owned working allocations; report native process overhead and peak evidence separately. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Actual >500 MiB input completes whole-file browse/filter/stable-sort/export under default resource policy; output includes verified records beyond all displayed pages.
- Memory and temporary disk are bounded across source, cache, index, selections, jobs and outputs; quota/shape violations never silently truncate or publish partial complete results.
- Small Table/CSV behavior, source-read-only rules and revision-safe exports remain protected by focused tests.
- Cancellation, new-job readiness, close/reopen and owned-file cleanup have actual evidence.
- Goal 10 can consume the row/selection/spill contract without requiring a full-source buffer.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Report resource defaults/ranges, largest verified file size and shape, tracked peak memory, observed process overhead, temp-disk peak and cancellation result.
- List preserved compact-route behavior, supported full-file operations, explicit shape limits, source consistency model and remaining verified limitations.
- Provide exact focused commands, native screenshots, output count/digest evidence and owned-temp cleanup result; separate measured outcomes from hypotheses.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
