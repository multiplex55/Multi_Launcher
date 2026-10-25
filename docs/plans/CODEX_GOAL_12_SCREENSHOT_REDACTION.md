# Codex Goal 12 — Redact screenshots with solid fill, blur and pixelation

**Companion execution prompt:** [CODEX_KICKSTART_12_SCREENSHOT_REDACTION.md](CODEX_KICKSTART_12_SCREENSHOT_REDACTION.md)  
**Requested coverage:** #74  
**Program position:** 12 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `feat/screenshot-redaction` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish screenshot-redaction branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

The screenshot editor can obscure selected rectangles with an opaque fill, blur or pixelation, preview the actual resulting pixels, undo/redo those operations and export the same flattened result through Save and Copy. Existing annotations, cropping and screenshot save preferences keep working.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/gui/screenshot_editor.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/screenshot_editor.rs)** — `MarkupTool; MarkupLayer; MarkupHistory`. The editor supports Pen, Arrow, Rectangle, Highlight, Marker and Text, with layer-based undo/redo. No redaction operation is present.
- **[src/gui/screenshot_editor.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/screenshot_editor.rs)** — `render_markup_layers around line 236`. The export compositor clones the base image and applies existing annotation layers in order. This is the owner to extend for pixel-changing operations.
- **[src/gui/screenshot_editor.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/screenshot_editor.rs)** — `ScreenshotEditor::apply_edits; save_image; copy_to_clipboard`. Save and Copy both obtain output from apply_edits, which renders layers and then crops. Existing screenshot preferences may also save after copying.
- **[src/gui/screenshot_editor.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/screenshot_editor.rs)** — `ScreenshotEditor::ui; canvas layer preview around line 1104`. The live preview currently paints annotation layers separately through egui, so adding only an export effect would not produce faithful preview. Toolbar rows already wrap.
- **[src/gui/screenshot_editor.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/screenshot_editor.rs)** — `qa_write_applied_png and existing screenshot marker/export tests`. There is an applied-output QA PNG path and existing tests compare decoded saved output against expected raster pixels, including crop behavior.
- **[src/annotation/raster.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/annotation/raster.rs)** — `Shared annotation raster module used by screenshot_editor`. Existing drawing operations already use an annotation-layer raster boundary. A small image-effect module can live beside it without moving GUI state into the domain layer.

## C. Approved functional requirements

1. Add Solid, Blur and Pixelate redaction modes to the existing screenshot editor, with Solid selected when first choosing redaction. Use a distinct redaction operation, not the existing translucent highlight.
2. Select rectangular regions in image coordinates using the normal primary-pointer tool gesture. Normalize reverse drags, clip to the image, ignore empty selections and keep the existing secondary-button crop gesture.
3. Solid redaction replaces every covered output pixel with an opaque color, black by default. Blur and pixelation have bounded positive strength/block controls; label them as visual obscuring effects, not guaranteed removal of sensitive information.
4. Store a compact operation in MarkupHistory for each committed region. Parameters are captured with that operation; changing the current tool settings affects subsequent regions. Undo/redo follows the existing history rules and a new edit clears redo.
5. Apply effects in history order to the current composed pixels. An overlapping later redaction must not sample the untouched original and restore pixels hidden by an earlier effect. Later ordinary annotations remain usable.
6. Display a preview that matches the raster result at the selected zoom, including overlaps and crop. A selection outline during dragging is acceptable; committing a region must show its effect promptly.
7. Save, Copy, copy-triggered saving and the existing applied-output QA path must use the same flattened pixel result. A new effect must never exist solely as a UI overlay or as removable output metadata.
8. Keep the editable original only within the current editor/history workflow as needed for undo. This feature does not promise secure deletion of previously saved screenshots or clipboard history.
9. Bound work and memory: history stores region/parameter metadata, not full image snapshots per edit; cache completed rendering by revision and avoid full-image recomposition or GPU texture upload on every idle frame or pointer move.
10. Preserve capture behavior, existing save destination/preferences, crop ordering, markup tools, active-text commit behavior, keyboard undo/redo and wrapped toolbar usability.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- Automatic OCR-based detection, face detection, screenshot upload/sharing, redaction presets, image-library migration or redesign of the entire screenshot editor.
- Secure erasure of previously saved originals, editable project exports, arbitrary vector masks or adding screenshot tools unrelated to the requested three effects.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Represent redaction as a typed MarkupLayer with normalized image-space bounds and immutable effect parameters. Add pure pixel-effect code under the annotation ownership; src/annotation/redaction.rs is a proposed new module, not an existing file.
2. Extend render_markup_layers as the authoritative composed-pixel path. Compute solid coverage with explicit integer bounds and alpha 255; implement blur/pixelation on a clipped region using bounded buffers and checked sizes.
3. Give the canvas a revisioned preview of the same raster output for committed redactions. Reuse the existing GPU texture lifecycle, ensure older annotation overlays are not drawn a second time over an already flattened preview, and invalidate only for relevant edits.
4. Keep transient selection state separate from committed history. A drag outline avoids expensive recomposition on every movement; optional throttled effect preview is permissible only if its cost stays bounded.
5. Make output preparation commit any pending text or completed transient edit consistently before obtaining the shared raster result. Audit Save, Copy and QA paths together; preserve their existing destination and preference semantics.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Add deterministic raster effects and compact history layers

**Objective:** The editor's shared compositor can apply all three redaction types correctly with existing history and crop semantics.

**Code ownership:**

- src/gui/screenshot_editor.rs — MarkupLayer, MarkupHistory and compositor
- src/annotation/mod.rs
- proposed src/annotation/redaction.rs — pure raster effect code

**Implementation tasks:**

1. Define typed redaction mode/parameters and normalized image-space rectangles; validate finite coordinates, bounded radius/block size and checked dimensions.
2. Implement opaque fill, blur and pixelation over clipped regions, composing from the current output in layer order. Keep scratch allocation proportional to the selected region or a bounded row/chunk strategy.
3. Extend history and render_markup_layers without storing a full-resolution image in each operation.
4. Add deterministic small-image tests for complete solid coverage/opacity, reverse/out-of-bounds drags, empty regions, blur/pixelation boundaries, overlapping layers and undo/redo branch behavior.

**Behavioral requirements:**

- Pure rendering produces a flattened image containing the selected effects.
- Earlier redactions stay obscured when later layers overlap; ordinary annotations can still be applied afterward.

**Edge cases:**

- One-pixel selections, fractional edges, image-border selections, oversized/invalid strength, alpha in the source, completely clipped rectangles and arithmetic overflow.

**Regression constraints:**

- Unselected pixels remain unchanged by a region operation, subject only to existing other layers.
- Existing annotation ordering and crop-after-render behavior remain intact; history has metadata-scale storage.

**Verification:**

- Run the focused new raster/history tests plus directly affected existing screenshot raster/crop tests.
- Use images small enough to inspect exact pixels, and a generated larger image to verify checked size/allocation handling without a broad benchmark campaign.

**Acceptance criteria:**

- Solid output contains fully opaque replacement pixels across the chosen coverage.
- All modes are deterministic, clipped, bounded and correct under overlap/undo/redo.
- Existing raster expectations pass without weakening assertions.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(screenshot): [M1-A] add redaction layers and bounded raster effects` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Integrate region selection and faithful cached preview

**Objective:** The actual canvas and toolbar let the user create redactions and see the same result that will export.

**Code ownership:**

- src/gui/screenshot_editor.rs — toolbar, gesture state, image texture and canvas rendering

**Implementation tasks:**

1. Add wrapped redaction controls with Solid default and contextual strength controls. Keep existing color/thickness/text controls semantics intact.
2. Map pointer positions through the existing zoom/image origin into source pixels; commit one history operation per completed primary-button drag and keep secondary-button cropping separate.
3. Cache the rendered committed preview using image/history revision and reuse the texture between unchanged frames. Do not paint old markup twice if the preview already contains it.
4. Release/reuse obsolete preview buffers/textures when the editor closes, undo/redo changes history or a new image is opened.
5. Add a small focused gesture/state check using the existing screenshot fixture for zoomed selection, reverse drag, undo/redo and tool switching.

**Behavioral requirements:**

- Committing a rectangle visibly applies the selected effect; changing mode/strength affects subsequent regions.
- Undo/redo updates the preview promptly and idle frames do not repeatedly re-render the full image.

**Edge cases:**

- Dragging across image bounds, scrolling/zoom changes, release outside the canvas, switching from active text, closing mid-drag, narrow/high-scale toolbar and repeated open/close.

**Regression constraints:**

- Crop gestures and other tools remain reachable and functional.
- Existing control identity, canvas coordinate conventions and text commit rules remain preserved.

**Verification:**

- Run the selected canvas/history tests, retaining meaningful existing marker/toolbar coverage only where shared code changed.
- Inspect one generated large image during drag and idle using counters/profiling already available; demonstrate preview revision reuse rather than claiming a speedup from source alone.

**Acceptance criteria:**

- Canvas-selected source bounds match the compositor bounds at tested zoom levels.
- The preview matches committed pixel effects without duplicate annotations.
- Repeated idle frames reuse the rendered result and history does not accumulate image clones.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(screenshot): [M1-B] add region controls and cached redaction preview` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Verify all output paths and complete the screenshot workflow

**Objective:** Every supported output contains the visible edited pixels and the full user workflow is verified on Windows.

**Code ownership:**

- src/gui/screenshot_editor.rs — apply_edits, Save, Copy and applied-output QA
- README.md and current screenshot help

**Implementation tasks:**

1. Audit output preparation and ensure Save, Copy, optional save-after-copy and QA PNG all call the shared final-pixel path after required transient-text commitment.
2. Test decoded PNG equality against the composed/cropped result and inspect clipboard image bytes through an isolated clipboard/native test where supported.
3. Run one disposable native editor session covering each mode, overlapping markup, zoom/crop, undo/redo, Save and Copy. Capture the canvas and representative exported files for final review.
4. Document the new controls, solid-vs-visual-effect distinction and unchanged handling of previously saved originals. Review the focused diff and finish feature-specific fixes.

**Behavioral requirements:**

- Saved and copied images match the visible committed result, including all redactions and crop.
- Output failure reports an error without losing editable state or falsely announcing success.

**Edge cases:**

- Cancelled Save As, write failure, unavailable clipboard, auto-save enabled/disabled, a final edit immediately before export and image close after export.

**Regression constraints:**

- Existing paths/preferences and success/error feedback remain authoritative.
- No background original-image deletion, extra sharing destination or automatic upload is introduced.

**Verification:**

- Run the combined targeted screenshot selection once after integration; inspect export pixel equality.
- Use one bounded native session and generated non-sensitive screenshots. Coordinate clipboard access and restore only owned test state, never overwriting newer user clipboard content.

**Acceptance criteria:**

- All available output paths contain rasterized effects; solid-redacted regions cannot be recovered by removing an overlay from the output.
- Native evidence covers the controls, crop/undo and exports, with unavailable paths explicitly marked unverified.
- Help and results accurately describe the limits of blur/pixelation.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(screenshot): [M2-A] verify redacted exports and document editor controls` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use focused pure raster/history tests for exact pixels and a small existing canvas fixture for interaction. Do not construct the full application for each effect calculation.
- Separate compilation from execution. After the necessary build, allow up to 8 minutes for the targeted screenshot tests and up to 10 minutes for one native smoke/export inspection. At most two focused remediation attempts; do not add full-suite or multi-feature GUI qualification.
- Inspect memory ownership with a representative 4K generated image and several regions: source plus bounded current rendering/texture/scratch is acceptable, an entire image per history item or idle frame is not. Record actual observations rather than promising a universal RAM value.
- Use disposable settings, disable Paste Sequence next/cancel shortcuts in app fixtures, and keep native clipboard tests coordinated. End-only first-party Clippy follows the package policy.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| Preview and exported effects diverge because the editor currently has separate painter/raster paths. | Share effect calculations and composed pixels; verify decoded PNG/clipboard equality and avoid painting already flattened layers twice. |
| A translucent fill or cached original restores sensitive pixels. | Solid mode forces opaque replacement, effects compose from current output, and overlap tests inspect actual bytes. |
| Blur/pixelation cause expensive per-frame full-image allocations. | Use revisioned rendering, compact history and bounded region scratch; exercise drag/idle/open-close behavior on a representative large image. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- All three modes can be selected, drawn, previewed, undone/redone and exported.
- Solid regions are opaque in saved/copied pixels; blur/pixelation are described accurately.
- Shared markup/crop/save behavior and bounded preview/history memory are verified.
- No known feature-related output mismatch remains; native limitations are reported honestly.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Describe the effect controls, parameter units/defaults and preview caching decision.
- Provide native editor evidence and representative redacted PNGs plus output-equality test results.
- Report measured memory/idle observations only if performed; include commits, push outcomes and remaining limits.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
