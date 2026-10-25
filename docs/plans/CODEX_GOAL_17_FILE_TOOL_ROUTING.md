# Codex Goal 17 — Drop data files into an appropriate tool

**Companion execution prompt:** [CODEX_KICKSTART_17_FILE_TOOL_ROUTING.md](CODEX_KICKSTART_17_FILE_TOOL_ROUTING.md)  
**Requested coverage:** #1  
**Program position:** 17 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `feat/file-tool-routing` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish file-tool-routing branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Dropping a supported JSON, XML, CSV/TSV, text or image file onto the launcher opens a concise chooser of suitable enabled utilities and an Add as launcher action option. The chosen tool opens that file through its real source-admission path without using the clipboard as transport or silently overwriting an existing draft.

### Dependency and branch-base contract

**Required predecessor capabilities:** [09 — Large CSV files with bounded memory and complete-file operations](CODEX_GOAL_09_LARGE_CSV.md), [10 — Table transformations summaries and JSON/Markdown exchange](CODEX_GOAL_10_TABLE_OPERATIONS.md), [12 — Redact screenshots with solid fill, blur and pixelation](CODEX_GOAL_12_SCREENSHOT_REDACTION.md).

**Recommended earlier work:** [05 — Keep Plugin Settings usable with long descriptions](CODEX_GOAL_05_PLUGIN_SETTINGS_SCROLL.md), [06 — Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md), [11 — Configure Jump Mode activation and ergonomic hint letters](CODEX_GOAL_11_JUMP_CONFIGURATION.md), [13 — Explore folder sizes with bounded background scanning](CODEX_GOAL_13_FOLDER_SIZE.md), [14 — Preview ZIP contents and extract an explicit selection safely](CODEX_GOAL_14_ZIP_EXPLORER.md), [15 — Completed macro run summaries and practical starter recipes](CODEX_GOAL_15_MACRO_SUMMARIES_RECIPES.md), [16 — Hot Folder size, age, and text-content conditions](CODEX_GOAL_16_HOT_FOLDER_CONDITIONS.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/gui/mod.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/mod.rs)** — `LauncherApp::handle_dropped_files`. Current file drops open the add-action editor; directory drops add folder aliases. This goal intentionally changes ordinary drops only for the approved supported data-file categories.
- **[src/gui/render.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/render.rs)** — `first_ui_pass drop processing; hidden_logic_ticks_progress_requests_without_replaying_keys_or_drops`. Drops are consumed in the root first UI pass; hidden logic ticks have a regression test ensuring they do not replay drops.
- **[src/gui/json_utility_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/json_utility_dialog.rs)** — `JsonUtilityDialogState::open; open_with_clipboard; reopen_existing`. Current JSON open initializes through clipboard state. File input needs an explicit source entry point; setting the clipboard to mimic opening would alter unrelated user data.
- **[src/gui/xml_toolkit_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/xml_toolkit_dialog.rs)** — `request_load`. XML already has a path-loading path that should be adapted/exposed through its existing session rather than duplicated.
- **[src/table_utility_session.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility_session.rs)** — `file source admission and export receipts; Goal 09 successor owner`. The original table uses bounded in-memory file reading. This routing goal requires the integrated large-file foundation so a dropped 500 MB+ CSV stays file-backed.
- **[src/gui/clipboard_modify_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/clipboard_modify_dialog.rs)** — `replace_loaded_source; commit; source baseline`. replace_loaded_source clones text into baseline/reset/source. The large-source acknowledgment is a preview gate after allocation, not a file-admission cap. File input also needs origin-aware clipboard conflict behavior.
- **[src/gui/image_compare_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/image_compare_dialog.rs)** — `replace_file; ImageCompareSession; ImageSide`. The production GUI replace_file wrapper delegates to session.replace_file. Adapt that owner for input-side selection; load_smoke_inputs remains QA-only.
- **[src/gui/screenshot_editor.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/screenshot_editor.rs)** — `ScreenshotEditor; apply_edits; save_image`. Image editing has one applied-pixels export path. Opening an existing file must retain a safe source/output distinction, especially after redaction.
- **[src/clipboard_modify/clipboard.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/clipboard_modify/clipboard.rs)** — `ClipboardService::commit_dialog`. Clipboard-origin commits compare the baseline with current text and preserve undo. A file-text baseline cannot be substituted without a false external-change warning or nontext clipboard failure.

## C. Approved functional requirements

1. The approved default is a tool chooser for supported data files, with Add as launcher action always available. Preserve the prior behavior for executables, shell shortcuts, directories and unsupported file categories. Do not reinterpret a directory drop as a folder-size scan.
2. Support at least JSON -> JSON utility or Table import; XML -> XML toolkit; CSV/TSV -> their appropriate existing table profile; ordinary UTF-8 text -> text transformation and relevant existing text tools; supported raster images -> image editing/redaction and Image Comparison. Offer only actual enabled/capable destinations; a missing/disabled destination is not silently enabled.
3. Type detection combines normalized extension and a small bounded file signature/UTF-8 probe where needed. Never execute dropped content, follow embedded commands, automatically extract archives, or fully load/decode a large file simply to populate the chooser.
4. Passing a file to a tool uses a typed request/source descriptor and that tool’s existing session/persistence/limit rules. Do not synthesize query strings, place file text on the system clipboard, call QA/smoke-only APIs, or assign public dialog fields to bypass validation.
5. A dropped large CSV reaches the Goal 09 file-backed path directly; detection reads at most 8 KiB beyond metadata and table loading retains no second full-file text copy. JSON-to-table uses Goal 10 import semantics; image editing uses Goal 12 applied pixel behavior.
6. One supported file opens one chooser. Repeated delivery of the same drop event/egui discarded pass must not reopen it or import twice. For multiple files, use a bounded queue of at most 32 explicit items with visible counts and per-item destination choices; explain excess input rather than silently truncating.
7. For two images, the user can explicitly assign comparison inputs A/B; a single image can target an indicated empty/selected side. Do not silently replace the other image or infer a destructive batch operation.
8. Keep existing draft/session ownership: if loading would replace edited data, present an in-app replace/discard confirmation using the tool’s existing unsaved-change policy. Cancelling leaves that draft unchanged. The chooser itself performs no file modification or clipboard write.
9. Use existing modal/focus ownership and Jump Mode conventions. Escape closes the chooser and leaves unprocessed queued items untouched; selecting a destination transfers focus to that tool. Hidden root ticks must not replay or process new data drops.
10. Use non-lossy paths internally. Show invalid/missing/unreadable files, changed source identity, unsupported encoding and tool size limits as actionable errors; retain a return-to-chooser path instead of silently loading partial data.
11. Opening an existing image for editing must default to Save As/new output, preserving the source unless an existing explicit overwrite confirmation is deliberately chosen. Opening a file never triggers save/apply, macro execution or external shell launch.
12. Do not broaden this into clipboard-to-tool routing, a file shelf, archive dropping, general batch conversion or new workspace behavior. The approved directory/executable behavior remains intact.
13. Run file metadata/signature probing in a bounded generation-owned worker; root first-pass handling only admits events and updates chooser state. The 8 KiB cap is not a latency guarantee on slow/network storage. Support cancellation, stale-result rejection and finite probe ownership without blocking painting.
14. For file-origin text adapters enforce admission before full reads/clones: 8 MiB input maximum and 64 MiB aggregate accounted working text/preview budget, with clear rejection when limits would be exceeded. For JSON utility file input enforce the stricter of 16 MiB and its existing tool limit, and bounded parser depth/node/retained-buffer limits before publication; if absent, add a narrow file-input budget rather than relying on a post-allocation warning. JSON-to-table retains Goal 10's separate 16 MiB import policy. These restrictions do not reduce 500 MiB+ CSV/TSV support.
15. Make Clipboard Modify source origin explicit. Opening a file performs no clipboard access. A file-result Copy action uses the proper clipboard owner with intentional overwrite behavior and truthful undo availability, including when the clipboard is nontext. Preserve existing clipboard-origin conflict/undo semantics; never fabricate a clipboard baseline from file contents or claim an undo image/text value that was not retained.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- Clipboard-to-tool chooser (#2), persistent drop history or global drag interception.
- Automatic processing on drop, arbitrary executable opening, background directory scanning or archive extraction.
- A replacement command bus, second parsing system or unbounded multi-file import.
- New editing or parsing capabilities beyond the selected destination tools.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Create a small typed drop-routing coordinator near the launcher root that owns bounded pending items, detection and modal handoff. It delegates source loading to existing tools; proposed new types/modules must be clearly named as new.
2. Expose narrow production file-input entry points where a tool only supports clipboard/open dialog today. Reuse parsing, limits, worker/source revisions and unsaved-change checks. Preserve normal command-open behavior unchanged.
3. Resolve tool capability at chooser display and again at activation, so disabling a plugin or changing a session cannot activate stale UI choices.
4. This goal depends on 09, 10 and 12 being in the selected branch base; do not reimplement those features or assume another branch’s files are present. Optional activity reporting integrates only if its existing API is available.
5. For CSV/TSV call Goal09's typed opener and preserve compact-versus-file-backed admission; only large inputs are required to use the file-backed path. For text/JSON establish file-input budgets before allocation; source origin is part of the adapter contract.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Define bounded file classification and routing

**Objective:** Introduce pure destination selection and owned drop requests with preserved legacy fallback.

**Code ownership:**

- src/gui/mod.rs handle_dropped_files
- src/gui/render.rs first UI pass
- src/commands/model.rs and host boundary if required
- proposed file-routing types

**Implementation tasks:**

1. Inspect actual drop handles/paths and define stable per-event/request identity plus a bounded 32-item queue.
2. Implement extension/signature-based classification with an 8 KiB probe cap, enabled-destination lookup and explicit legacy routes.
3. Define a typed tool-source request and error/confirmation outcome rather than command text or clipboard transport.
4. Keep metadata/signature I/O off the root UI path in one bounded generation-owned worker. Cancellation or a new drop invalidates stale probes; a slow read is never allowed to freeze painting.

**Behavioral requirements:**

- Supported files can be classified cheaply; directory/executable/shortcut/unsupported cases retain the existing route.

**Edge cases:**

- Uppercase extension, extensionless text, binary pretending to be text, non-Unicode path, duplicate event, too many files, disappeared source.

**Regression constraints:**

- No full reads, decoder work, data writes or global clipboard mutation during detection.

**Verification:**

- Pure classification/capability/bounds tests and the existing hidden-drop replay regression.

**Acceptance criteria:**

- Approved category routing and legacy fallback are explicit, bounded and exactly-once at the root event boundary.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(drop): [M1-A] classify bounded file-to-tool requests` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Add production source-loading adapters

**Objective:** Open each destination through its real validated source owner.

**Code ownership:**

- src/gui/json_utility_dialog.rs
- src/gui/xml_toolkit_dialog.rs
- src/table_utility_session.rs or its Goal 03/09 successor owner
- src/gui/clipboard_modify_dialog.rs
- src/gui/image_compare_dialog.rs
- src/gui/screenshot_editor.rs

**Implementation tasks:**

1. Add or expose narrow path/source entry points preserving current command/clipboard behavior and using existing worker limits.
2. Route CSV/TSV directly to file-backed table admission and JSON-to-table to the approved importer; use current source identity and cancellation receipts.
3. Provide bounded image decoding/input-side selection and source-preserving editor Save As defaults. Wire explicit draft-replacement outcomes rather than silently replacing state.
4. Apply finite text/JSON admission budgets before full allocation/cloning, and add origin-aware file-result copy behavior without a fake clipboard baseline. Preserve current small-table profile selection and adapt the real Image Compare replace_file wrapper.

**Behavioral requirements:**

- Every advertised destination receives the chosen file and shows load/error state without a clipboard detour.

**Edge cases:**

- Existing dirty draft, stale load after close/reopen, huge CSV, image dimension budget, malformed JSON/XML/text, source replaced during load.

**Regression constraints:**

- Original files and clipboard unchanged by open; no QA-only API in production; existing parsers and persistence semantics remain authoritative.

**Verification:**

- Focused destination tests with injected file input and stale/dirty cases; verify a >500 MiB CSV descriptor is passed without whole-file reads.
- Test oversized text/JSON rejection before cloning, slow probe cancellation, nontext clipboard during file load/copy, and unchanged clipboard-origin conflict/undo behavior.

**Acceptance criteria:**

- All advertised routes either load correctly or return a precise supported error; no bypass of limits, revisions or draft checks.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(drop): [M1-B] open files through tool source owners` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Present chooser and transfer input ownership

**Objective:** Make the approved ordinary-drop workflow discoverable and safe.

**Code ownership:**

- src/gui/mod.rs
- src/gui/render.rs
- proposed chooser UI
- existing Jump Mode/modal ownership

**Implementation tasks:**

1. Render file name/type, suitable destinations and Add as launcher action; indicate disabled/unavailable choices clearly.
2. Support queued items, cancel, return after error, explicit comparison A/B choice and existing-tool replace confirmation.
3. Guard first-pass event consumption and update focus/modal ownership only after a fresh accepted activation.

**Behavioral requirements:**

- Dropping supported data files immediately offers useful actions without changing other drop defaults.

**Edge cases:**

- Chooser open during another modal, input owner changes, plugin disabled before activation, repeated drop, Escape during async load.

**Regression constraints:**

- No background dialog focus steal, hidden event replay, silently replaced drafts or automatic operations.

**Verification:**

- Focused egui input/ownership checks with Jump on/off and one native drop/choose/cancel smoke.

**Acceptance criteria:**

- The ordinary supported-file path, legacy fallback and all cancel/draft states behave as approved.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(drop): [M2-A] add file-to-tool chooser` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Verify integrated routes and document behavior

**Objective:** Prove the end-to-end feature against real destination entry points.

**Code ownership:**

- affected routing/destination tests
- existing bounded native harness
- file/tool help

**Implementation tasks:**

1. Use generated JSON/XML/CSV/text/images and one actual >500 MiB CSV to exercise all destination categories; reuse the large fixture generator from09.
2. Verify source hashes and clipboard remain unchanged until explicit user actions, and changing/cancelling a route releases owned resources.
3. Capture chooser, a loaded small tool and loaded large table; update help to explain supported types, legacy drops and draft protection.

**Behavioral requirements:**

- All claimed routes are usable and large-file behavior stays bounded after integration.

**Edge cases:**

- Wrong format, multi-file queue, dirty tool, invalid file, source change and legacy folder/executable drop.

**Regression constraints:**

- No full-suite campaign or additional format/tool expansion.

**Verification:**

- One bounded native route batch and targeted source/ownership tests; final changed first-party lint.

**Acceptance criteria:**

- Every supported category and preservation invariant has actual evidence; the large file is not fully materialized by routing.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `test(drop): [M2-B] verify tool routing and legacy behavior` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use pure classification/typed-handoff tests for broad category coverage and one native drop sequence for OS drag/input ownership.
- Native route batch execution ceiling fifteen minutes plus one bounded large-file open, with compilation tracked separately. Large CSV data can be reused from09 without rerunning all transformation benchmarks.
- Check source hashes and clipboard before/after opening and cancelling; inspect bounded probe/read counts rather than relying solely on process RSS.
- Verify actual enabled destinations and exact current test targets after earlier goals may have moved code; do not invoke unverified smoke APIs as production entry points.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| The chooser breaks the established add-action/drop-folder behavior. | Use the approved category-specific change, always offer Add as launcher action and preserve legacy routes for other categories. |
| A convenience adapter bypasses parser limits or modifies clipboard. | Typed file-source handoff through existing owner; source/clipboard invariants tested. |
| Large files are materialized before reaching the table backend. | Metadata plus 8 KiB detection cap and direct path/descriptor handoff; verify read/allocation behavior. |
| Source loading destroys a user draft. | Explicit existing-owner replacement confirmation and generation-safe cancellation. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Approved supported data types open a chooser with working enabled destinations and Add as launcher action.
- Large CSV follows09, table import follows10 and redaction follows12 without duplicate engines.
- Legacy drops, drafts, clipboard, original files and hidden-input ownership remain protected.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Supported category-to-destination mapping and actual enabled-capability behavior.
- Native screenshots, source/clipboard preservation evidence and large-file handoff measurements.
- Any explicitly unsupported format or route with a precise reason.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
