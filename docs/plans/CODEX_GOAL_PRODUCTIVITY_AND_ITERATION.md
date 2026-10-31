# Codex Goal — Productivity Tools and Faster Iteration

**Status:** Approved for implementation; not an implementation-completion report.  
**Companion:** `CODEX_KICKSTART_PRODUCTIVITY_AND_ITERATION.md`  
**Intended location:** `docs/plans/` for both documents.  
**Planning source:** `multiplex55/Multi_Launcher`, `master` at `261651ed24e55335c56ea807dd7334cb0da8a045`, verified October 31, 2026.  
**Authority:** The user's selected feature request and approval of both recommended questionnaire answers. Read current `AGENTS.md` and applicable local instructions before execution.

This is one approved Goal/Kickstart pair with seven explicitly defined, sequential work categories. It is not authorization to implement the earlier 190-item suggestion list. The Goal and Kickstart share the same category branch at every stage; neither document nor an individual agent receives its own implementation branch.

## A0. Mandatory chained Git bootstrap — before all application work

**First execution checkpoint: `C01/M0-A`. Repeat this bootstrap as `<category>/M0-A` at every category transition.** Reading instructions and read-only Git inspection come first; application edits, implementation builds, test runs, and implementation checkpoints must wait until the correct child branch and baseline exist.

1. **Inspect safely in the single existing checkout.** Read `AGENTS.md`, this complete Goal, and the complete Kickstart. Inspect `git status -sb`, current branch, `HEAD`, remotes, upstream, pre-existing tracked/untracked changes, and any current queue/execution ledger recording a predecessor. Do not stage the user's existing files or overwrite an existing plan. Uploaded or copied-in task documents are not automatically authorized staging targets; identify task-owned files explicitly.
2. **Resolve the exact required parent.** For a continuing sequence, use the immediately preceding completed Goal/category branch's recorded **`final_tip_sha`**, even when the current checkout is on `master`, `main`, or another branch. Capture `parent_branch` and `parent_tip_sha`. Verify the final checkpoint exists and has no unaccounted intended changes. Safely fetch a remote-only predecessor and verify its exact SHA. Never choose a stale remote tip when a newer local predecessor was not pushed. Ambiguous/conflicting ancestry is a precise blocker, not permission to choose another base.
3. **Only a genuinely new chain defaults to the up-to-date primary integration branch.** Determine its actual name, safely fetch it, and branch from its verified tip without an automatic pull, reset, or merge. The planning SHA above is an inspection reference, not an instruction to rewind a newer checkout. A safe current-HEAD exception is allowed only for the first task, with the reason and preserved user changes documented; it never overrides a known predecessor. An old closed queue does not automatically become this program's active predecessor.
4. **Choose a fresh, unique, idiomatic category branch.** Use lowercase kebab-case and `feat/`, `fix/`, `perf/`, `refactor/`, `chore/`, or `docs/`. The suggestions in Section F are stems, not guaranteed available names. Add a date/run suffix as necessary. One branch per approved category, not per checkpoint or agent.
5. **Use one existing working directory. NO GIT WORKTREES.** Do not run `git worktree add`, `git worktree move`, or `git worktree prune`; do not create agent worktrees, auxiliary checkouts, secondary clones, or independent implementation branches from a common base. All writes, branch switches, builds, commits, and handoffs are sequential in this checkout. Read-only delegation may be parallel only against a stable recorded revision.
6. **Preserve user work and the required ancestry.** Never automatically reset, stash, hard-pull, rebase, merge, force-checkout, force-push, delete branches, or overwrite pre-existing work. Never stage unrelated changes. When local changes make switching to the required parent unsafe, stop at that boundary and report the exact files/condition; do not silently start from a different base. Do not change another checkout's branch ownership.
7. **Create and switch directly from the resolved SHA:** `git switch -c <unique-category-branch> <parent_tip_sha>`. Verify its initial `HEAD` equals the required parent. Do not implement on the predecessor or integration branch.
8. **Make this category's own `M0-A` baseline commit.** Prefer a small task-owned update to the single execution ledger; an intentional empty commit is acceptable: `git commit --allow-empty -m "chore: [C01/M0-A] establish iteration category baseline"`. Adapt the category label. Record `baseline_sha` separately from `parent_tip_sha`. Verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`. A prior category's last commit is not this category's baseline. No implementation commit may precede this baseline.
9. **Attempt one ordinary upstream push.** Use the configured upstream or `git push -u origin HEAD` when `origin` is verified as the intended repository. If unavailable, denied, or rejected, record the failure and keep going locally. Do not bypass approvals, alter credentials, force-push, or retry in a loop. Each later coherent checkpoint gets its own one normal push attempt. A next category on the same machine must use the actual local predecessor tip even after push failure; moving machines without that commit is a blocker.
10. **Finalize and hand off the exact tip.** Commit all intended category changes, record significant checkpoint SHAs, actual push state, and clean/dirty disposition. Capture the resulting `final_tip_sha` and require the next category to branch directly from it. Do not merge, rebase, squash, or cherry-pick intermediate branches merely to advance. The last branch contains the cumulative program and is the only candidate for final integration.

**Bootstrap acceptance:** fresh active branch; verified correct parent; direct-child baseline; initial push attempted or inability explicitly recorded; no unrelated staging or lost work; no extra checkout/worktree; ledger and actual Git state agree. This rule applies equally to features, fixes, UI work, performance work, and cleanup.

**Hash recording without an infinite reporting loop:** a commit cannot contain its own final SHA. Put a just-created baseline/checkpoint SHA in the next ordinary ledger update. Record a category's actual closing SHA in the successor's baseline ledger entry and in the final response. Capture the last `final_tip_sha` after the last documentation commit and report it directly; do not create an endless sequence of commits just to record each preceding report commit. If another commit is made, the handoff tip must be updated.

## A. Goal and desired outcome

Deliver the user's chosen productivity improvements while preserving existing command semantics, file comparison behavior, hotkeys, data, and Windows ownership boundaries. Shorten the development feedback loop before the larger features, and measure startup with realistically large synthetic personal datasets at the end.

Successful user-visible outcomes:

- MkMacro can directly **Open Folder** and **Open URL**, without a saved Folder or Bookmark plugin entry.
- Diff can compare two pasted/typed texts in memory; reuse manual alignment; ignore selected whole-line blocks without deleting source content; compare a selection with clipboard text in a fresh temporary comparison; navigate line bookmarks; and expose identical-only/context display choices.
- Settings says **Jump Mode**, and each eligible Quick Tools dashboard button is independently reachable by a hint without auto-execution on hint completion.
- A local Unicode/invisible-character inspector explains explicitly supplied text without silently transforming it.
- Compact/Comfortable density is available through the existing theme system, and ordinary resizable utility dialogs remember their sizes across restarts.
- ZIP files can be inspected and selected entries extracted through a safe, bounded, explicit workflow with truthful progress, cancellation, and partial-result reporting.
- Incremental compilation, selected-test preparation, fixture setup, expensive GUI tests, and startup have measured baselines and focused, behavior-preserving improvements where demonstrated bottlenecks justify them.

**Approved questionnaire decisions:** Q1 **A**, whole-line ignored blocks; Q2 **A**, the focused two-way enhancement package. Do not reopen these decisions or expand them into arbitrary substring ignoring, a separate Isolate alignment command, or three-way merge.

## B. Current codebase findings and source map

The planning review was read-only. It did not run the Windows application, benchmarks, builds, or tests. Revalidate relevant locations after bootstrap, especially if the selected predecessor is newer. Existing symbols below were inspected; names explicitly labeled **proposed** are design suggestions, not claims that they exist.

| Owner and inspected source | Current finding | Required integration direction |
|---|---|---|
| [Latest merged PR #1460](https://github.com/multiplex55/Multi_Launcher/pull/1460) | The final follow-on integration records evidence/dispositions, but accepts no new production Rust/test source beyond the preceding source baseline. ZIP writer/UI and other older candidates remain unaccepted. | Do not mistake documentation completion or old passes for accepted features. Do not replay historical programs or blindly import their candidates. |
| [AGENTS.md](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/AGENTS.md) | Current task authority, proportional verification, sequential writers, and best-effort pushes are explicit. Historical initiative procedures are reference material unless adopted. | This Goal establishes the current scope/budget. Preserve technical invariants without inheriting unrelated old acceptance campaigns. |
| [MkMacro model](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/mkmacro/model.rs) | `MkAction` includes `Process`, `LauncherCommand`, typed variables, clipboard operations, and many other actions; no dedicated Open Folder/Open URL variant was present. `MkLauncherCommandPayload` preserves a legacy route. | Add typed actions to the existing schema/executor/editor/validation/package paths; do not translate direct opens through fuzzy launcher queries. |
| [Executor](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/mkmacro/executor.rs), [validation](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/mkmacro/validation.rs), [authoring fields](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/mkmacro/authoring_fields.rs), [editor mutation](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/mkmacro/editor_mutation.rs) | Action support is distributed across execution and authoring, not only the enum. | Find every exhaustive/action-classification match and existing package/validation caller. Existing `src/hotfolder/capability.rs` must not accidentally grant unattended permission to the new actions. |
| [Diff product boundary](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/diff/mod.rs) | Two-way text, read-only binary/hex, and controlled folder comparison/reconciliation exist; three-way text is separately planned. | Extend two-way text only. Keep archives ordinary binary files inside Diff; the new ZIP utility is separate. |
| [Diff model](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/diff/model.rs) | `TextCompareState` currently uses optional paths. `TextViewModel::load` creates documents from files or empty documents, starts comparison, and maintains revisions, navigation, scroll, and alignment state. | Add an explicit memory-text origin/construction path; do not use fake paths or temporary files to represent pasted text. Reuse the existing view model. |
| [Text engine](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/diff/text_compare.rs), [text document](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/diff/text_file.rs) | `TextComparisonRules`, `CompiledRules`, `ManualTextAlignment`, raw/source-line projections, importance, hunk/navigation indices, and lossless-policy file handling exist. | Keep raw document content, effective comparison, and display projection distinct. Preserve encoding/BOM/newline behavior for existing file-backed documents. |
| [Text view](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/gui/diff_dialog/text_view.rs), [rules dialog](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/gui/diff_dialog/rules_dialog.rs), [settings](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/diff/settings.rs) | Align With, alignment removal, previous/next differences, hunk copy, sync scroll, rules, Find, whitespace display, and `projection_context` already exist. The toolbar exposes All and Differences-only. | Expose and extend these owners; do not create another comparison engine or reinterpret existing regex rules. |
| [Quick Tools](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/dashboard/widgets/quick_tools.rs) | `QuickToolEntry { query, auto_submit }` supports legacy and current serialization. Settings rows register Jump targets. Actual rendered tool buttons use click handling. `action_for` distinguishes `query` and `queryexec`. | Add individual button targets using the existing dashboard Jump integration, preserving exact manual/auto-submit semantics and legacy config. |
| [Jump engine](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/gui/jump_mode.rs), [settings model](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/settings/model.rs), [settings renderer](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/settings_editor/render.rs) | Configurable activation/alphabet and the persisted `experimental_jump_mode_enabled` field already exist. | Rename user-facing terminology without renaming the serialized field or changing defaults/chords. |
| [Theme resolver](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/gui/theme/mod.rs), [theme editor](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/gui/theme_settings_dialog.rs) | `ThemeDensity::Compact`/`Comfortable`, theme preferences, committed/preview publication, and typography scaling exist. Inspected theme-editor sections lack a density selector. | Expose the existing setting through its current preview/Save/Cancel transaction, independently of typography. |
| [Archive boundary](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/archive/mod.rs), [catalog](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/archive/catalog.rs), [plan](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/archive/plan.rs) | `read_catalog`, `ArchiveCatalog`, `ArchiveSelection`, `ExtractionPlan`, source/revision identities, limits, and lexical path types exist. The boundary expressly excludes payload reads and destination writes. | Retain catalog/planner responsibilities; add an owned extraction runtime and UI after native destination/source checks work. A lexical-safe path is not filesystem authority. |
| [Startup preload](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/startup.rs), [startup catalog](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/startup_catalog.rs) | Recovery precedes settings load. `startup_indexed_actions` waits for and validates the initial index before plugin registration, then transfers its coordinator. `startup_action_catalog` assembles custom-first/indexed-tail data and records timing. | Measure this contract, not merely the first painted frame. Never remove readiness ordering to report an artificially fast startup. |
| [Cargo manifest](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/Cargo.toml) | `fast-dev`, `iteration`, feature-gated developer binaries, grouped tests, and `multi_launcher_table_core` already exist. | Improve measured bottlenecks rather than add duplicate profiles or extract every feature into a crate. |
| [GUI test fixtures](https://github.com/multiplex55/Multi_Launcher/blob/261651ed24e55335c56ea807dd7334cb0da8a045/src/gui/test_fixtures.rs) | `IsolatedAppWorkspace` owns an explicit root and calls `LauncherApp::new_isolated_for_test_at`; it measures construction/drop and restores environment/root state. | Extend the existing fixture owner where appropriate; use smaller domain fixtures when a full application adds no coverage. Do not mistake existing isolation for a cheap fixture. |

**Bounded reinspection still required:** locate existing runtime window-geometry persistence before adding a store; find the exact MkMacro editor/package/migration entry points; select measured slow-test families and compile-invalidating edits; find the current startup loader owners for Notes/Snippets/History. These are implementation investigations, not additional user questions. Do not assert defects or speedups from file size alone.

## C. Approved functional requirements

### C1. Direct MkMacro actions

Provide independently selectable **Open Folder** and **Open URL** actions. Folder input supports an explicit path, a browse control, and existing string-variable/interpolation conventions. URL input uses the same string conventions and opens through the default browser. No Folder/Bookmark record is required; neither action runs a fuzzy search.

Folder semantics: open a directory in `explorer.exe`; preserve spaces/non-ASCII and valid supported UNC paths; report missing/non-directory paths. Resolve variables at the normal execution boundary, then validate. Never concatenate user input into `cmd.exe`/PowerShell or treat it as switches. Authoring/browsing/validation does not launch anything. Any transient native operation stays off the UI path as required by the existing runtime.

URL semantics: accept well-formed absolute HTTP/HTTPS URLs, with visible validation for missing host, malformed input, controls, unsupported schemes, or unresolved variables. Do not prepend a scheme, search the web for malformed text, or dispatch `file:`, `javascript:`, executable, or arbitrary custom schemes. Preserve intentional query/fragment data. Report successful OS handoff separately from browser navigation/network success; do not claim that a page loaded.

Use current failure/retry/cancellation conventions; cancellation before dispatch prevents an open, while cancellation after OS handoff does not promise to close an external window. Avoid duplicating a successful open due to bookkeeping retries. New actions are interactive side effects and unavailable to unattended Hot Folder execution unless a separately approved policy already explicitly models them; this Goal does not widen unattended policy. Preserve legacy macros, package import/export, duplication, structured editing, search/replace field traversal, and call validation. Keep potentially sensitive URL/path values out of routine debug logs. Ensure derived parent-action diagnostics cannot expose them: use redacted payload Debug/display formatting and sanitize propagated error details in diagnostic paths, while allowing the explicit authoring UI to show the user their own input.

### C2. Direct text Diff and agreed two-way enhancements

**Input and origin.** Add a discoverable Text comparison entry in the existing Diff UI, with separately labeled left/right multiline editors and explicit Paste Left/Paste Right controls. Clipboard access is explicit and performed once per request. Empty text is a valid input; failed/non-text clipboard reads preserve the accepted input. Source bodies stay in memory unless the user explicitly copies/exports/saves. No implicit temp files, query-history payloads, recent-file records, or diagnostic bodies.

Represent in-memory and file-backed origins explicitly using a narrow extension of the existing document/view model. Exact type names are the implementer's choice. Reuse the existing alignment/comparison/scroll/Find/hunk-copy/Undo engine. New memory documents expose **Save As**, never a fabricated file path; no implicit promotion to a file-backed document on ordinary comparison. Keep existing file Save, read-only handling, external-change conflicts, encoding/BOM/newline behavior, and folder-parent reconciliation unchanged. Warn before discarding modified in-memory work when replacing inputs or closing if the existing dirty-document policy requires confirmation.

**Alignment.** Reuse `ManualTextAlignment`, validation, existing Align With and Remove alignment. Make pending origin and second-side choice apparent in the new text workflow. Reject duplicate/crossing/out-of-range anchors with a useful error; support cancelling a pending anchor. Anchors refer to source lines and revisions, not visible row positions. Swapping sides swaps their ownership; replacing a source invalidates affected anchors. Do not add a second alignment engine or separate Isolate command.

**Ignored whole-line blocks (Q1 A).** Select a contiguous range of source lines on either pane using mouse and keyboard, then **Ignore selected lines**. Show ignored lines with an unambiguous marker/style, expose **Restore selected ignored lines** and **Clear ignored ranges**, and preserve the underlying text exactly. Ranges are session-only, independent per side, validated against document revisions, and identified by source coordinates rather than rendered rows. Overlapping/adjacent ranges normalize deterministically. No arbitrary substring-ignore tool is in scope.

The effective comparison excludes ignored source lines, retaining a mapping to original lines; the full display reinserts/identifies ignored content rather than silently erasing it. If only one side's line is ignored, meaningful unmatched content on the other side can still be a reported difference. Ignored-only changes are excluded from default difference counts, next/previous difference, and bulk difference-copy eligibility. Do not label different raw texts byte-identical: distinguish **raw identical**, **equal under current rules/ignored ranges**, and **different**. Existing regex-based unimportant rules retain their own semantics. Do not hide real changes sharing a hunk with ignored content.

Required examples (line numbers are one-based in UI):

| Left | Right | Ignored | Expected effective result |
|---|---|---|---|
| `keep\nextra\nend` | `keep\nend` | Left line 2 | Equal under current rules; raw different; ignored row visible in All. |
| `keep\nold\nend` | `keep\nnew\nend` | Line 2 on both sides | Equal under current rules, not raw-identical. |
| Same as above | Same as above | Left line 2 only | Right `new` remains a meaningful unmatched line. |
| `keep\nnoise\nleft` | `keep\nother\nright` | Line 2 on both sides | Final-line difference remains counted/navigable/copyable. |

Edits must not silently move an ignore range onto unrelated text. Reuse existing edit mapping when unambiguous: shift unaffected source coordinates and invalidate intersected/ambiguous annotations with a visible notice. Integrate annotation changes with Undo/Redo where they share an edit transaction. Full-source replacement clears that side's range/bookmark state and dependent anchors. An anchor whose endpoint is ignored cannot silently constrain the effective comparator; reject or remove it with explanation. Stale async completions may not republish an obsolete projection or selection.

**Focused extras (Q2 A).**

- **Compare selection with clipboard:** an explicit command on an actual source-text selection opens a new temporary comparison, selected text on the left and one current clipboard text read on the right. It neither changes the original comparison nor writes the clipboard. No ambient capture from other applications. Empty/non-text/error cases are explained without losing the original session.
- **Line bookmarks:** toggle a bookmark on a source line, show a gutter indicator, and navigate next/previous deterministically through the current comparison. Bookmarks are session-only. Preserve source identity across unambiguous edits; invalidate ambiguous/remapped entries rather than jumping to unrelated text. Hidden bookmarks are revealed intentionally with their view state made clear.
- **View choices:** retain All and Differences-only; add Identical-only and an accessible surrounding-context count for Differences-only (default preserves existing `projection_context`, currently 3). Context changes affect only presentation. Ignored rows must not be mislabeled identical. Counts reflect the effective comparator, not a filtered view. Navigation to a hidden target must reveal it or clearly explain the active filter.

A selected diff hunk is not automatically a selected source-text range. Maintain separate typed state for text/line selection, active hunk, manual alignment, ignore ranges, and bookmarks so one gesture cannot accidentally merge or save content.

### C3. Jump Mode and Quick Tools

Replace user-facing **Experimental Jump Mode** with **Jump Mode** in the settings dialog and directly related help/search labels. Keep the stored `experimental_jump_mode_enabled` key compatible, current disabled/enabled default, configured leader/alphabet, save acknowledgment, and hotkey conflicts unchanged. Do not perform a repository-wide historical-document rename.

Each visible, enabled Quick Tools dashboard tool button gets its own stable Jump target. Use the existing dashboard/widget target namespace, including widget-instance identity and row identity; duplicate labels, duplicate queries, reordered entries, and multiple Quick Tools widgets must not collide. Skip blank queries and clipped/hidden/disabled buttons. Expire targets after reorder/config/visibility changes. Completing a hint selects/focuses only; a later ordinary activation produces exactly one existing `WidgetAction`. Preserve `query_override`, `query:` vs `queryexec:`, and the existing normal mouse/keyboard path. No new global shortcut.

### C4. Unicode and invisible-character inspector

Add a local tool with explicit multiline input/Paste, Clear, inspection results, and explicit copy operations. A command such as `unicode` is a **proposed** name; verify no existing owner claims it, then register it through the existing builtin/plugin/typed-command conventions without changing unrelated search ranking.

Display original offsets, escaped/display-safe representation, code point (`U+....`), available Unicode name/category, UTF-8 bytes, UTF-16 units, and useful flags for whitespace/control/combining/default-ignorable/bidirectional characters. Explain scalar count versus grapheme count; emoji sequences must not be misreported as a single scalar. Label zero-based byte offsets and one-based human line/column conventions. Handle CRLF explicitly, including both code points; do not replace line endings on import. Render directional controls and zero-width characters safely in result labels without executing their display effects in identifiers.

Inspect examples including NBSP U+00A0, ZWSP U+200B, ZWJ U+200D, BOM/ZWNBSP U+FEFF, combining accents, tab/newline, bidi overrides/isolates, and supplementary-plane emoji. Preserve raw text and normalization form. Do not silently strip or normalize. No network lookup, history, secret scanner, script engine, or file-writing feature. Use existing `unicode-segmentation`; if full names/properties need a small data dependency, document its need, Unicode data version, license, and compile footprint, and add only that scoped dependency. Unsupported/unassigned names must be labeled honestly.

Bound admitted text and rendered rows; virtualize/paginate results instead of one widget per scalar for an arbitrarily large paste. Show truncation/refusal explicitly and never quietly analyze only a prefix while claiming a full result. Proposed modules, subject to local conventions: `src/unicode_inspector.rs`, `src/plugins/unicode_inspector.rs`, `src/gui/unicode_inspector_dialog.rs`.

### C5. UI density and persistent utility sizes

**Density:** expose existing `ThemeDensity` Compact/Comfortable in the current Theme editor. Live preview, Save, Cancel, reload, and defaults use existing transaction ownership. Changing spacing must not change the saved text-size choice. Preserve current default and palette-specific behavior. Invalidate relevant layout/measurement caches through the existing theme revision path; do not add a second theme source of truth. Compact remains keyboard-accessible and readable; both modes retain reachable actions in narrow windows.

**Sizes:** remember dimensions for each ordinary resizable application-owned utility type across close/reopen and application restart. First locate existing geometry persistence; extend it instead of creating a competing store. If none exists, add a small versioned, bounded app-owned geometry store using current persistence/recovery conventions. Store a stable tool key and finite logical dimensions, not input text, document paths, queries, arbitrary dynamic IDs, screen captures, or all transient egui memory.

Do a bounded inventory of ordinary resizable utility windows and record a coverage table. Apply the shared helper consistently to that inventory, including new Unicode/ZIP dialogs; keep exceptional sizing rules explicit. Do not silently limit this requirement to just the new tools. Native pickers, capture overlays, radial menus, pin windows, main-launcher positioning, modal confirmations, and already fixed-size surfaces retain their existing behavior. This is size memory, not workspace/layout restoration or document-session restoration.

Restore once at opening rather than forcing size each frame. Persist a settled user resize or close through a bounded/debounced write; do not serialize on every paint. Clamp presentation to the current available viewport/monitor/DPI and minimum controls without overwriting a larger saved preferred size merely because a temporary screen is smaller. Support per-tool reset to default. Missing/old data uses existing defaults; malformed/unreadable data follows recovery policy without destructive overwrite.

### C6. Complete ZIP extraction utility

Use the accepted catalog/planner as the foundation. Add an explicit archive picker, searchable/selectable catalog, selection summary, destination picker, preview, **Extract**, progress, **Cancel**, and terminal result. Opening/browsing/selecting never extracts. Snapshot the exact archive, entry selection, destination, and output policy at confirmation; changes invalidate a prepared request. No extraction from a path that was silently replaced after inspection.

Keep scope to supported ZIP formats/compressions in the pinned dependency and accepted catalog. Label unsupported encryption, compression, links, or malformed entries; never silently reinterpret them. No ZIP creation, RAR/7z, shell-extension installation, archive editing, or Diff virtual folders.

**Destination/output contract:** no overwrite, merge-into-existing-file, or delete-existing behavior. Conflicts must be shown during preview and checked again at publication. Publish each file only after its bounded payload has completed and passed integrity/size checks. A run is not a multi-file atomic transaction: cancellation/failure may leave already completed files, and the final report must say so. Remove only temporary artifacts demonstrably owned by this run; never delete unrelated files or recursively clean a user-selected destination. Re-running a partial run reports existing-file conflicts rather than overwriting them.

**Filesystem safety:** reject escaping/absolute/drive/UNC/device paths, alternate data streams, invalid/reserved Windows names, case-normalized aliases/collisions, file-directory conflicts, and archive links/reparse entries as applicable to the existing catalog rules. Detect lexical and filesystem collisions. Keep access beneath the selected root despite existing or concurrently replaced junctions/symlinks/reparse points; path-string normalization and a precheck alone are insufficient. Source identity and destination-root/descendant authority must remain tied to the actual handles/objects used for I/O. Require `create-new`/no-clobber publication semantics, but do not treat that alone as proof of containment. Reuse inspected local native primitives where they meet this contract; implement only the missing ZIP-owned boundary otherwise.

Do not request write access to unrelated profile/drive ancestors simply to check containment. Native tests must use a known writable synthetic destination under their own temporary root. Unavailable permissions are distinct from malformed archive, cancellation, checksum failure, source change, and output conflict. Do not disable protections to make tests pass.

**Resource/lifecycle contract:** reuse existing catalog/plan limits and set finite per-entry/aggregate decompressed-byte and active-job bounds. If existing limits do not cover output, adopt explicit documented defaults (initial proposal: 1 GiB per entry, 4 GiB per run), display them in preview, and enforce actual emitted bytes rather than trusting archive headers. This is a safety default, not a benchmark target. Keep memory bounded with streaming. Check declared/actual sizes, supported integrity checks, and premature EOF. Cancellation stops new entries and safely retires active I/O; a late completion cannot write into a new dialog/run. Never mark cancellation complete until the owning worker has reached a terminal state and its resources have been retired/joined. Closing the dialog requests cancellation; app shutdown owns cleanup. Progress distinguishes entries planned/completed, bytes processed, cancellation requested, and terminal outcome; do not invent percentages when totals are untrusted.

Potentially blocking source reads and destination writes need a genuinely bounded/cancellable native ownership path, or explicit rejection of unsupported source/destination kinds. A token checked only between indefinitely blocking calls is not sufficient. Do not return from a timeout while abandoning a live I/O thread; verify the actual cancellation mechanism used by the implementation.

Use existing activity/worker ownership facilities where suitable. No detached writer, unrestricted background extraction, implicit elevation, or new broad asynchronous framework. Native source/containment/no-clobber/retirement evidence is mandatory before enabling the extraction action. Ordinary pixel automation is not a substitute for those tests.

### C7. Faster development and startup

Separate **compilation/preparation**, **test execution**, **fixture setup/teardown**, **first usable UI**, and **complete usable data**. A warm no-op build is not an edited incremental build. A test-name filter does not by itself prove unrelated targets were not compiled. Fewer source files or test binaries are not proof of speedup. Capture commands, commit/profile/toolchain, package/targets, cache state, workload, elapsed time, and results.

Use existing profiles, the table-core boundary, performance hooks, and test fixtures. Improve a measured compile boundary or linker/debug-data cost; reduce selected-test preparation; replace unnecessary full-application fixtures; optimize a named expensive GUI-test family; and isolate current-directory/environment/files/settings/native-resource dependencies. Preserve legitimate assertions and representative high-cardinality/interaction coverage. Do not delete tests, add ignores, raise limits to hide hangs, switch off safety checks, or serialize the entire suite to report fewer failures. At most one small, justified pure-code crate extraction is permitted in this program; no mass crate split, public-API redesign, vendored cleanup, or dependency upgrade campaign.

For startup, use synthetic large actions/notes/snippets/history and explicit configured indexing roots. Preserve recovery-before-load and custom/indexed catalog consumer parity. Measure existing startup phases before choosing the bottleneck. Prefer eliminating duplicate parsing, cloning, or redundant loading before changing readiness contracts. Deferring work is allowed only when UI/data availability is explicit, current-generation publications remain correct, the first usable query still works, and total readiness is measured alongside first paint. No silent dropping of entries, incomplete results labeled complete, private-data collection, or perpetual background polling.

An optimization counts as an improvement only with comparable before/after evidence beyond observed run-to-run noise and passing relevant behavior checks. No numerical speedup is promised by this plan. If an area cannot improve safely within its budget, document it as **assessed, no accepted optimization** rather than calling that requirement delivered or inventing a performance claim.

## D. Scope, non-goals, and compatibility invariants

**Included:** only C1–C7, their necessary source/test/persistence integration, concise current user/developer help, and the checkpoints below. Adjacent changes require a concrete dependency on this task. Existing implementation can satisfy a requirement after verification; do not rewrite it just to produce a new diff.

**Excluded:** workspaces/layout management; all other features from the earlier suggestion list; three-way text/folder merge; arbitrary substring ignoring; a new Isolate alignment mode; automatic synchronization; full-file Table work; Hot Folder condition completion; macro history/recipes; screenshot-redaction completion; search-ranker replacement; global text expansion; new external services; broad dependency/egui upgrades; mass architecture cleanup; broad test qualification; new model/agent configuration.

Preserve these relevant invariants:

- Existing launcher hotkey tap/hold behavior, Paste Sequence controls, radial ownership, focus restoration, and typed command dispatch are not redesigned.
- Direct opens cannot execute shell text or widen existing unattended capabilities. Existing Macro `LauncherCommand`, legacy resolved actions, and package content retain their behavior.
- Diff's file save/conflict/recovery protections and folder mutation confirmations stay intact. Ignoring content never edits a file, implicitly merges a hunk, or makes an unsafe path safe.
- A rendered hint, preview, browse action, dialog opening, or settings preview never authorizes the corresponding external side effect.
- New settings have backward-compatible defaults. Save/Cancel acknowledgment remains authoritative; old serialized keys remain readable. Recovery is not replaced by silently writing defaults over corrupt data.
- Workers own their I/O and cleanup; cancellation request, terminal publication, and actual resource retirement are different states. No callback may update a superseded generation.
- Performance work preserves data coverage, source order where meaningful, plugin registration, user-data isolation, debug/safety configuration, and benchmark comparability.
- No stage requires the user to run an intermediate test, provide a screenshot, create a branch, or resolve ordinary implementation questions.

## E. Technical strategy and execution organization

### E1. Reuse owners; add only missing boundaries

The implementation is deliberately additive at established boundaries: two typed Macro actions; memory origins and line annotations within Diff; individual dashboard Jump targets; one inspector domain/UI; the existing density preference; one shared size-persistence helper; a ZIP-owned writer/runtime behind the current catalog/planner; and measured changes to current startup/test/compile paths.

Separate pure validation/projection work from OS side effects so tests can be small without replacing integration tests. UI owns drafts and emits typed intentions; domain/runtime owns validation, execution, revisions, cancellation, and publication. Reuse current command and plugin registration; do not run a tool directly from render code to bypass its normal owner.

Use existing document revisions and job generations. Where additional identities are needed, introduce narrow typed identities local to that subsystem. Do not build a universal authorization, history, workflow, or job framework just for this plan.

### E2. Category chain and dependency order

| Category | Suggested unique branch stem | Checkpoints | Required predecessor | Reason for grouping |
|---|---|---|---|---|
| C01 — Development feedback | `perf/productivity-iteration` | M0-A, M0-B, M1-A–M1-C | Resolved previous Goal tip, or current integration tip for a genuinely new chain | Measure and improve the tools used by all later work. |
| C02 — Direct Macro opens | `feat/mkmacro-direct-open` | M0-A, M2-A–M2-B | C01 final tip | One schema/runtime/editor feature category. |
| C03 — Two-way text Diff | `feat/diff-text-workflows` | M0-A, M3-A–M3-D | C02 final tip | Shared document/projection/selection ownership. |
| C04 — Keyboard and UI polish | `feat/utility-ui-usability` | M0-A, M4-A–M4-C | C03 final tip | Jump coverage, existing density, shared size memory. |
| C05 — Unicode inspection | `feat/unicode-inspector` | M0-A, M5-A–M5-B | C04 final tip | One new local utility, using C04 conventions. |
| C06 — ZIP extraction | `feat/zip-extraction` | M0-A, M6-A–M6-C | C05 final tip | Catalog, native writer, worker, and UI remain one owner. |
| C07 — Large-data startup | `perf/large-data-startup` | M0-A, M7-A–M7-B, M8-A | C06 final tip | Measure final feature set and finish program-wide review on the last branch. |

These branches are categories within this one pair, not seven new Goal/Kickstart pairs. All later branches include prior changes through ancestry. M8-A is closeout on C07, not a new category. No intermediate branch merge/rebase/cherry-pick, no per-checkpoint branches, and no worktrees.

A dependent checkpoint must not consume unverified or known-unsafe behavior. An unrelated category may continue after an explicit **partial/blocked closure** only when the previous category's safe changes and limitations are committed, its final tip is recorded, unsafe task-owned experiments are not active, and it has no dependency on the blocked behavior. A blocked category is not marked complete. Never silently carry dirty partial work across a branch transition; never delete or revert pre-existing user work to make such a transition possible.

### E3. One compact execution ledger

Create or use one task-specific ledger, proposed `docs/plans/PRODUCTIVITY_AND_ITERATION_EXECUTION.md`. Keep category identities/parentage, checkpoint statuses, decisions, exact test selections/results, timing samples, evidence locations, and blockers there. One compact ledger plus necessary raw logs/screenshots is sufficient; do not recreate a large tree of stage reports, duplicate plans, candidate manifests, or research packages.

Statuses must distinguish `pending`, `in_progress`, `implemented_pending_verification`, `complete`, `blocked`, and `assessed_no_accepted_optimization`. An exception or missing native test cannot be converted to `complete`. Preserve prior raw results as historical evidence; a correction must identify the new source tested.

## F. Detailed milestones and checkpoint tasks

Each category begins with its own A0 bootstrap. Every checkpoint below requires diff inspection, a coherent commit, and one ordinary best-effort push after its listed checks. The listed files are owners to inspect, not permission to modify every file in a directory.

### C01/M0-B — Validate the execution baseline and available verification

**Objective:** convert the inspected source map into a short, current, runnable work map without repeating a repository-wide audit.

**Code ownership:** current `AGENTS.md`/local instructions; the files in B; actual agent configuration; existing dev/test/performance helpers; current command registration; application data-root selection and native test entry points.

**Tasks:** confirm changed facts since the planning SHA; locate Macro editing/package/migration owners and geometry persistence; identify current test binaries for Diff, Macro, Jump/dashboard, theme, archive, and startup. Inventory supported build/launch/window-interaction/screenshot methods once. Read the active model/role setup; use it without altering it or claiming an unobserved resolved backend. Record the current root, toolchain/profile, relevant selectors, and compilation-versus-runtime timeout mechanism. Select isolated writable test roots, not real user stores.

**Behavior:** the plan can be executed from this checkout without reconstructing the conversation. Known missing UI capabilities have an honest disposition and do not silently invalidate unrelated headless work.

**Edges:** newer parent source, pre-existing compiler errors, unavailable native desktop, previously unaccepted archive prototypes, missing external tools.

**Regression constraints:** no application change or broad build just to complete this map; no import of historical requirements/candidates; no environment/security reconfiguration.

**Verification:** read-only code/call-site checks, Git identity, installed-command help only where needed to disambiguate syntax, and scoped capability observations. Do not run a native tool against arbitrary user windows.

**Acceptance:** current owner/test map and budgets are recorded; no unresolved product decision; any environment limitation is specific. This is intake, not proof that an application test passed.

**Commit boundary:** `docs: [C01/M0-B] record current owners and focused verification route`.

### C01/M1-A — Establish comparable development measurements

**Objective:** obtain a small, reproducible baseline for incremental and selected-test cost before changing the build/test structure.

**Code ownership:** `Cargo.toml`, existing Cargo/Nextest configuration, `dev/README.md`, `dev/track_b/cargo.ps1` when present, `src/gui/test_fixtures.rs`, `src/gui/render.rs`, existing performance helpers and directly relevant test families.

**Tasks:** select one representative pure/module edit and one representative GUI edit; record no-op, edited incremental check/build, selected-test preparation, and already-built selected-test execution separately. Select two actual slow/over-coupled test families from scoped timings, including one GUI family. Use task-owned reversible edits or existing supported benchmark mechanisms; restore only your exact edit, not an entire file containing user changes. Record profile/cache/toolchain/targets and per-family fixture/setup/teardown where available. Reuse existing logging and commands rather than building a benchmark platform.

**Behavior:** subsequent performance claims can be compared against the same workload and correctness requirements.

**Edges:** cold cache, concurrent compiler load, stale generated artifacts, test inventory failure, fixture cost larger than assertion cost.

**Regression constraints:** no `cargo clean`, broad all-features build, full-suite baseline, or private user corpus. No numerical target invented from historic measurements.

**Verification:** one preparation per necessary target/profile, then up to three comparable samples under Section H. Confirm the test inventory is nonempty and the actual selected tests run.

**Acceptance:** raw command/results and at least one useful bottleneck are identified, or a specific measurement blocker is reported. This checkpoint alone claims no improvement.

**Commit boundary:** `perf: [C01/M1-A] capture targeted iteration and fixture baselines`.

### C01/M1-B — Improve incremental and selected-test preparation

**Objective:** reduce measured compile/link work for ordinary edits and a small test selection without reducing correctness checks.

**Code ownership:** the measured boundary from M1-A; manifest/test target ownership; existing `tests/suites` targets; first-party dev helpers. Inspect referenced consumers before moving code.

**Tasks:** choose at most two evidence-backed candidate changes, one at a time. Favor avoiding unnecessary target selection, redundant compilation, overly coupled pure-code dependencies, or proven link/debug-data overhead. Reuse `fast-dev`/`iteration` and existing core boundaries. If an extraction is necessary, limit it to one cohesive pure boundary and migrate its production callers/tests together; keep root behavior/API compatible. A file split within the same crate is an organization change until timing demonstrates otherwise. Document a single reliable command for the selected test family that selects package/target as well as names.

**Behavior:** normal launch/debug/release commands still work; selected tests still cover the same behaviors; no optional smoke binary becomes a default build burden.

**Edges:** public/internal exports used by integration tests, duplicate dependency versions required for compatibility, target discovery, features shared with the main binary, stale caches.

**Regression constraints:** no wholesale crate decomposition, unsupported linker/toolchain installation, blanket feature disabling, removal of debug checks, or vendored edits. Default production release semantics remain unchanged.

**Verification:** focused compile and relevant tests for the moved boundary, one root application build if ownership changes affect it, and the matching before/after workloads from M1-A. Do not compare a different profile without explicitly labeling the tradeoff.

**Acceptance:** a measured compile/preparation improvement with equivalent behavior, or a documented no-improvement/blocker disposition with unsuccessful task-owned experiments removed from active source. No structural change is credited as a speedup without data.

**Commit boundary:** `perf: [C01/M1-B] reduce measured incremental and selected-test preparation` (use `docs:` and truthful wording for a no-change result).

### C01/M1-C — Lightweight fixtures, expensive GUI tests, and isolation

**Objective:** make the selected test families cheaper and more deterministic without weakening assertions or application coverage.

**Code ownership:** `src/gui/test_fixtures.rs`, the isolated constructor owner in `src/gui/mod.rs`, selected cases in `src/gui/render.rs` or the measured family, existing performance/test root helpers, and runner configuration only where selected resource scheduling requires it.

**Tasks:** use domain fixtures for pure behavior rather than constructing `LauncherApp`; extend existing isolated fixtures when actual GUI behavior is required. Avoid repeated parsing/layout/setup not needed by the asserted invariant. Retain at least the representative large-data/layout/focus paths the family protects. Replace unnecessary sleeps with bounded event/completion waits. Give tests unique roots and store paths; restore process-global environment/CWD only after workers retire. Treat filesystem/native-resource coordination across separately executing tests explicitly; do not assume a process-local lock controls every runner process. Narrow serialization to actual shared OS resources. Keep ordinary portable/pure tests parallel.

**Behavior:** the same legitimate failure conditions remain detectable, tests do not touch real user settings/clipboard unless the selected isolated native contract explicitly requires it, and teardown leaves no owned workers/handles/files behind.

**Edges:** panic/failure teardown, concurrent runs, CWD/environment restoration, clipboard/native ownership, repeated fixture construction, asynchronous late completions.

**Regression constraints:** no deleting assertions to hit a time target, ignored tests, blanket serial execution, fake GUI tests replacing integration evidence, or a test-only fast path that skips the behavior under test.

**Verification:** run selected families individually and together at normal supported concurrency; include one deliberate error/teardown path. Compare preparation and execution separately with M1-A. A relevant previously failing test must retain its original failure evidence and corrected-source rerun.

**Acceptance:** selected tests preserve coverage, pass, and show measured setup/execution improvement or an honest no-improvement disposition; isolation is evidenced by unique roots and verified cleanup, not merely fewer failures.

**Commit boundary:** `test: [C01/M1-C] streamline isolated fixtures and selected GUI tests`. Close C01, record its actual tip, then bootstrap C02 from that tip.

### C02/M2-A — Direct-open domain, validation, and runtime

**Objective:** execute direct folder and HTTP/HTTPS opens through typed Macro actions without saved plugin entries.

**Code ownership:** `src/mkmacro/model.rs`, `executor.rs`, `validation.rs`, runtime-side OS adapter(s), existing string-variable expansion, `src/hotfolder/capability.rs` action classification.

**Tasks:** add typed payloads/variants following the existing schema convention; validate literal drafts and expanded runtime values; use a direct Explorer/default-browser adapter with separately passed arguments, no shell interpolation. Integrate error/cancel/retry semantics and deny unintended unattended invocation. Add mockable OS handoff only at the real side-effect boundary. Trace nested-call validation and action availability.

**Behavior:** direct opens follow C1, including no launch during preview and no success claim for browser network navigation.

**Edges:** spaces/quotes/non-ASCII, valid UNC paths, nonexistent paths, directory-versus-file, malformed/unsupported URLs, literal versus variable input, cancelled and repeated commands.

**Regression constraints:** existing Process/LauncherCommand and legacy migration keep working; no global new command interpreter; no arbitrary protocols or logging of URL bodies/secrets.

**Verification:** focused validation/executor/capability tests with exactly-once handoff and negative cases; serialization compatibility tests for a legacy document and new actions. No real browser launch in ordinary unit tests.

**Acceptance:** correct actions reach the correct adapter once; invalid/cancelled inputs have no OS side effect; old documents retain their content/meaning; new actions remain unavailable through unapproved unattended paths.

**Commit boundary:** `feat(mkmacro): [C02/M2-A] add validated direct folder and URL actions`.

### C02/M2-B — Macro authoring and complete integration

**Objective:** make both actions fully usable in the existing Macro editor and reusable document operations.

**Code ownership:** `src/gui/mkmacro_dialog/`, `authoring_fields.rs`, `editor_mutation.rs`, actual package/import/export/migration owners found in M0-B, current in-app Macro help.

**Tasks:** add action picker entries and clear forms; folder Browse fills input without launching; show variable support and validation feedback. Update action naming/search, duplication, clipboard step operations, template/package traversal, field replacement, and every exhaustive action match. Follow current schema-version policy only as needed; do not migrate unrelated fields. Include concise examples using a temporary directory and a benign URL.

**Behavior:** users can author, save, reload, duplicate/export/import, and run either action with no Folder/Bookmark entry. A changed draft still follows normal Save/Cancel behavior.

**Edges:** cancelled picker, unresolved variable, invalid imported action, nested macro use, disabled action, failed native handoff.

**Regression constraints:** no background launch on selection or validation; no route change to existing Macro actions or hotkeys.

**Verification:** targeted editor/package round trips; build/run the application and exercise both actions on owned fixtures when a supported desktop exists. For a URL use a benign reserved test address and verify OS handoff, not remote content. Capture the authoring UI and resulting error/success state without user data.

**Acceptance:** C1 is usable end-to-end, relevant tests pass, and native execution evidence is recorded or distinctly outstanding. A mocked OS call is not native proof.

**Commit boundary:** `feat(mkmacro): [C02/M2-B] expose direct opens across authoring and reuse`. Close C02 and branch C03 from its exact final tip.

### C03/M3-A — In-memory Diff documents and text input

**Objective:** compare two explicit text inputs without file indirection while preserving existing file-backed behavior.

**Code ownership:** `src/diff/model.rs`, `text_file.rs`, `text_compare.rs`, `src/gui/diff_dialog/mod.rs`, `text_view.rs`, relevant Diff save/export/persistence callers.

**Tasks:** introduce explicit memory/file origins and a source-based document constructor; integrate a text-input entry and explicit left/right paste. Keep stable document/revision identity and existing async compare ownership. Bind Save As for memory documents and preserve file Save/conflict logic. Keep raw text out of histories and diagnostics. Reuse the existing back/navigation/session mechanism without replacing the original comparison when opening a new scratch comparison.

**Behavior:** arbitrary valid Unicode, empty strings, trailing newlines, and differing line endings remain intact; the same comparison and hunk-copy engine works for memory documents.

**Edges:** clipboard unavailable/non-text, switching input modes, source replacement during computation, dirty close, unbounded pastes, read-only file mode.

**Regression constraints:** no fake filenames or temp files, no automatic clipboard write/read on opening, no change to folder-parent reconciliation or file encoding policies.

**Verification:** focused memory-document comparison and clipboard error tests; existing file-document load/save/external-conflict cases; one actual UI text comparison with explicit paste or typing.

**Acceptance:** two pieces of text can be compared and edited/copied using existing operations, without creating or overwriting any file unless explicitly saved; file-based regressions remain covered.

**Commit boundary:** `feat(diff): [C03/M3-A] support explicit in-memory text comparisons`.

### C03/M3-B — Whole-line ignored blocks and truthful projections

**Objective:** implement Q1 A with preserved raw content, meaningful counts, and stable source mappings.

**Code ownership:** `src/diff/text_compare.rs`, `model.rs`, existing `LineEdit` owner in `text_file.rs`, `src/gui/diff_dialog/text_view.rs`, and diff navigation/hunk-copy consumers.

**Tasks:** add source-line range selection and session-only ignored ranges; normalize overlaps; build an effective comparator with original-line mapping and visible ignored rows. Update counts/navigation/equality labels and hunk-copy eligibility consistently. Separate existing unimportant-regex classification from manually ignored content. Implement range restoration and clearing. Coordinate source edits, Undo/Redo, swaps, source replacement, anchors, and stale publications. Build the exact examples in C2 as focused tests.

**Behavior:** ignoring never changes text or saves; ignoring one side does not suppress independent nonignored content on the other; only ignored changes disappear from ordinary difference navigation/counts.

**Edges:** entire document ignored, empty/terminal lines, mixed ignored/nonignored hunk, ignore adjacent to anchor, overlapping ranges, wrap/filter/scroll, editing inside/before ranges, stale compare completion.

**Regression constraints:** no algorithm that silently merges nonignored changes into an ignored hunk; original source numbers/copy/export remain accurate; file-backed and memory-backed inputs share the semantics.

**Verification:** pure projection/navigation fixtures; focused model editing/Undo/staleness tests; GUI selection/restore checks including one mixed hunk. Assert raw bytes/text are unchanged by ignore/restore.

**Acceptance:** C2's examples and restoration work; raw versus effective equality is truthful; unignored changes stay actionable; stale mappings cannot affect new content.

**Commit boundary:** `feat(diff): [C03/M3-B] add reversible whole-line ignored ranges`.

### C03/M3-C — Selection comparison, bookmarks, and view controls

**Objective:** implement the approved focused extras, not a general comparison-suite expansion.

**Code ownership:** Diff model/view, clipboard request owner, `RowProjectionMode` and context projection, existing find/navigation/selection and session routing.

**Tasks:** add explicit selection-with-clipboard comparison to a new memory comparison; add source-line bookmarks and deterministic navigation; expose Identical-only and context count using the existing projection owner. Keep filters presentation-only; reveal hidden navigated targets intentionally. Keep selection and active hunk separate. Reuse existing shortcut conventions, checking conflicts rather than appropriating current commands.

**Behavior:** original comparison and clipboard remain unchanged; bookmarks refer to source lines; context count preserves the existing initial value; ignored rows are not called identical.

**Edges:** no selection, selected text containing ignored lines, empty/non-text clipboard, source edits, hidden bookmarks, swapped panes, no identical rows, zero-context projection.

**Regression constraints:** no selected-text capture from external apps, background clipboard monitoring, persisted scratch history, or three-way comparison.

**Verification:** focused selection/clipboard routing, bookmark navigation, source-edit mapping, and projection tests; UI checks for keyboard access and preservation of the original session.

**Acceptance:** all three extras function as C2 specifies; the existing Diff commands and raw-source operations remain unchanged.

**Commit boundary:** `feat(diff): [C03/M3-C] add selection compare bookmarks and view controls`.

### C03/M3-D — Alignment usability and integrated Diff verification

**Objective:** ensure the existing alignment feature and all new text workflows cooperate in the real interface.

**Code ownership:** existing Align With/Remove interaction, `ManualTextAlignment` validation, `TextViewModel`, Diff help and directly affected tests.

**Tasks:** expose the existing alignment flow clearly for memory comparisons; show first endpoint/pending state and cancel/remove controls. Check anchor behavior under ignored ranges, filtering, edits, side swap, Undo, and navigation. Improve only interaction gaps demonstrated by these flows. Add short current help covering raw/effective equality and the difference between ignored ranges and regex rules.

**Behavior:** alignment changes presentation, not file/text content; hint/selection/anchor actions cannot accidentally copy a hunk or save.

**Edges:** duplicate/crossing anchors, anchor line ignored or removed, cancelled second endpoint, repeated source replacement, hidden projection target.

**Regression constraints:** no new Isolate command, second alignment engine, unrelated folder mutation work, or changes to binary comparison.

**Verification:** one focused Diff selection covering the new domain and relevant existing file behavior, plus a short supported native run: paste two texts, align, ignore/restore, bookmark, change context, compare a selection, return to original. Capture useful UI evidence; no full-suite campaign.

**Acceptance:** C2's requirements are traceably tested, native limitations are explicit, and all observed feature-related failures are fixed or block the affected claim.

**Commit boundary:** `feat(diff): [C03/M3-D] finish aligned two-way text workflow usability`. Close C03; bootstrap C04 from its final tip.

### C04/M4-A — Jump Mode naming and individual Quick Tools hints

**Objective:** deliver the requested settings wording and per-tool keyboard reachability without changing activation semantics.

**Code ownership:** `src/settings_editor/render.rs` and related catalog/help labels; `src/dashboard/widgets/quick_tools.rs`; existing dashboard widget/Jump context and registration owners; directly related tests.

**Tasks:** change the visible label/help search wording; retain the serialized field and configured hotkeys. Register each eligible rendered Quick Tools button under its widget-instance/entry identity using current Jump infrastructure. Preserve `action_for`, `query_override`, `auto_submit`, and legacy string-entry loading. Distinguish hint focus from a subsequent genuine click/Enter activation and avoid registering duplicate targets during sizing/repeated UI passes.

**Behavior:** every visible usable tool has its own hint; finishing a hint focuses/selects only; normal activation produces exactly one original action.

**Edges:** duplicate queries/labels, blank entries, multiple widgets, reorder/delete during a hint session, clipped buttons, hidden dashboard, foreign-window focus.

**Regression constraints:** no persisted-key rename, default/chord changes, implicit auto-submit on hint completion, or global keyboard hook changes.

**Verification:** focused widget action/config tests, Jump target identity/visibility/staleness tests, and a supported native dashboard/settings interaction. Exercise one manual-query tool and one auto-submit tool.

**Acceptance:** C3 is met, existing widget serialization/action outputs stay compatible, and current UI labels no longer call the mode experimental.

**Commit boundary:** `feat(jump): [C04/M4-A] expose individual Quick Tools hints and update naming`.

### C04/M4-B — Expose the existing density preference

**Objective:** make Compact/Comfortable selectable with current theme preview and persistence behavior.

**Code ownership:** `src/gui/theme_settings_dialog.rs`, `src/gui/theme/mod.rs`, `src/settings/model.rs`, and directly affected layout-cache/tests.

**Tasks:** add the selector to the existing preference draft; publish previews through the existing owner; preserve Save/Cancel/default handling and typography independence. Verify affected measurement caches observe density changes. Fix only demonstrated spacing/clipping problems on the covered utility surfaces, not every theme aesthetic.

**Behavior:** both density choices persist after Save; Cancel restores the original appearance; text-size choice and current defaults remain unchanged.

**Edges:** narrow windows, large text, preview then cancel, reset to default, missing field in older settings, multiple density changes during one session.

**Regression constraints:** no second theme engine, per-widget ad hoc spacing policy, unrelated palette changes, or restart requirement for ordinary preview.

**Verification:** theme transaction/default/round-trip tests, representative narrow/wide layout tests, and native preview/Save/Cancel plus restart when supported.

**Acceptance:** C5 density behavior is available and tested; no inaccessible primary controls at supported minimum sizes.

**Commit boundary:** `feat(theme): [C04/M4-B] expose compact and comfortable density`.

### C04/M4-C — Shared persistent utility-dialog sizing

**Objective:** restore user-chosen sizes across sessions for the ordinary resizable utility inventory.

**Code ownership:** actual geometry/settings persistence found during M0-B, root dialog lifecycle, existing resizable `src/gui/` tool windows, a small shared helper if needed.

**Tasks:** record eligible/excluded surfaces and existing size behavior; extend the existing owner or add the bounded geometry-only store described in C5. Integrate stable tool keys, once-on-open restore, resize/close persistence, reset, clamping, and version/default handling. Preserve larger preferred size when only presentation is clamped. Keep native/overlay/modal/main-launcher exceptions explicit. Provide a simple integration point for later Unicode/ZIP dialogs.

**Behavior:** each tool recalls its own size independently; no document/query content is stored; user resizing remains possible and does not produce per-frame disk writes.

**Edges:** old/missing/corrupt store, NaN/infinite/negative dimensions, small viewport, DPI change, more than one instance of a tool, cancelled settings draft, abrupt close.

**Regression constraints:** no position/layout/workspace restore, serialization of transient egui memory, capture-overlay changes, or overwriting existing recovery evidence.

**Verification:** pure key/size/serialization/clamp tests, helper lifecycle/write-count tests, and representative restart checks for Diff, a simple text tool, a table/list tool, and an editor. The inventory establishes remaining helper coverage without a full native campaign for every dialog.

**Acceptance:** all eligible ordinary resizable utilities in the recorded inventory use the shared behavior or have a justified existing equivalent; exceptions match C5; restart and reset are evidenced.

**Commit boundary:** `feat(gui): [C04/M4-C] persist bounded per-tool dialog sizes`. Close C04 and bootstrap C05 from its final tip.

### C05/M5-A — Unicode inspection domain

**Objective:** produce correct, display-safe character observations without mutating input.

**Code ownership:** the proposed inspector domain module; existing Unicode helpers and `unicode-segmentation`; manifest/lockfile only for a justified missing names/property dataset.

**Tasks:** define a small pure observation model with explicit offset conventions, scalar/grapheme counts, code point/encoding information, and property flags. Separate original text from escaped result labels. Define input/output bounds and honest unavailable-name handling. Document Unicode-data version and dependency cost if new data is required.

**Behavior:** the original string is preserved exactly, including normalization, line endings, invisibles, and direction controls; reported values describe actual input rather than a cleaned copy.

**Edges:** empty input, embedded BOM, CRLF, combining sequences, emoji/ZWJ, supplementary characters, private-use/unassigned values, very long input, rendering of bidi controls.

**Regression constraints:** no normalizer, implicit clipboard reader, network lookup, arbitrary text transformer, or broad dependency upgrade.

**Verification:** pure tests using the C4 examples, boundary/offset checks, and input-limit cases; verify counts/UTF encodings against exact expected values. Check a new dependency's actual enabled features and license before accepting it.

**Acceptance:** C4's data is correct for representative Unicode cases and all limits/errors are explicit; no side effects are needed to test the domain.

**Commit boundary:** `feat(unicode): [C05/M5-A] add bounded Unicode character inspection`.

### C05/M5-B — Inspector UI and command integration

**Objective:** make the inspector a quick local utility consistent with current tool behavior.

**Code ownership:** proposed inspector plugin/dialog; `src/plugins_builtin.rs`, actual typed command/GUI owner, existing help/settings catalog, C04 shared sizing/theme/Jump conventions.

**Tasks:** register the inspected nonconflicting command; add input, explicit Paste/Clear, summary, searchable or virtualized results, and explicit copy controls. Show limits and property meanings. Preserve accepted input on failed paste. Apply shared size memory, normal theme density, and standard keyboard targets. Keep raw content out of history/action payloads/logs.

**Behavior:** type/paste, inspect, and copy useful character details without opening a separate application; opening the tool does not read or change the clipboard.

**Edges:** busy/non-text clipboard, empty results, many scalars, narrow display, hidden/closed dialog, input replacement during computation.

**Regression constraints:** no command prefix collision, default search/ranking modification, or unexpected retention of input across app restarts.

**Verification:** targeted command/dialog tests, clipboard error handling, sizing/accessibility checks, and a supported native sample containing zero-width and combining characters.

**Acceptance:** C4 works end-to-end, explicit copy content is correct, large inputs remain bounded, and the tool has current concise help.

**Commit boundary:** `feat(unicode): [C05/M5-B] expose local inspector UI and commands`. Close C05 and bootstrap C06 from its final tip.

### C06/M6-A — Native source/destination authority and no-clobber preparation

**Objective:** establish the actual safe I/O boundary missing from the accepted lexical ZIP planner before enabling extraction.

**Code ownership:** `src/archive/{mod,model,catalog,plan}.rs`; a ZIP-owned native preparation/writer adapter (proposed); existing file identity/native primitives only where they satisfy C6.

**Tasks:** trace the current archive types and validate planned entries against the exact opened source. Bind source identity and destination root/descendant operations to real owned resources. Implement no-clobber destination creation/publication primitives, collision checks, safe owned staging, and actionable errors. Preserve catalog/planner purity. Ensure ordinary writable temp descendants work; do not repeat a design requiring unrelated profile ancestors to be writable. Freeze finite output defaults and user-visible semantics in the ledger. Do not activate extraction UI yet.

**Behavior:** a prepared request cannot write outside its selected root or replace existing content, including after a path change between preview and execution.

**Edges:** `..`, absolute/drive/UNC/device/ADS paths, reserved names, case/trailing-dot/space collisions, link/junction/reparse substitutions, existing directories/files, held source replacement, denied access, directory file conflicts.

**Regression constraints:** no relaxation of accepted lexical validation; no blind cherry-pick of archived prototypes; no global file-operation refactor, Table publication dependency, elevation, or write probe outside fixtures.

**Verification:** focused pure collision/plan tests and Windows tests for actual containment/no-clobber/source binding. Include a deterministic boundary where a destination component changes between preparation and use; use owned test roots and supported fixture capabilities. Unsupported fixture creation must be reported, never counted as pass.

**Acceptance:** native source/containment/no-clobber behavior is established for the intended writer or this checkpoint is blocked. Path checks alone do not satisfy it. Do not expose a weaker writer to bypass the gate.

**Commit boundary:** `feat(archive): [C06/M6-A] add bound source and no-clobber destination preparation`.

### C06/M6-B — Streaming extraction, cancellation, and resource retirement

**Objective:** extract a validated selection with bounded resources and truthful terminal outcomes.

**Code ownership:** ZIP-owned writer/runtime, accepted catalog/plan types, existing activity/cancellation/worker retirement abstractions, dependency-backed ZIP payload reader.

**Tasks:** stream only confirmed supported entries from the bound source; enforce actual emitted byte/count ceilings, integrity, declared sizes, and EOF. Publish completed files without overwrite. Report monotonic useful progress and exact completed/failed/skipped/partial counts. Cancel before new entries, safely retire current work, join/acknowledge ownership before terminal teardown, and remove only owned temporary output. Bind every event to run/generation identity so close/reopen/source changes reject stale messages.

**Behavior:** cancellation may retain fully published files and says so; incomplete files never masquerade as completed output. Repeated starts and app shutdown do not leave a detached writer.

**Edges:** CRC/integrity failure, truncated archive, limits exceeded by actual output, cancellation during an entry, failure after some completed files, disk/permission error, source change, publication race, repeated start/cancel/close.

**Regression constraints:** no unbounded buffering, recursive cleanup of destination, deletion of foreign files, silent overwrite, or cancellation-complete before retirement. No caller outside the confirmed interactive route receives automatic write permission.

**Verification:** targeted extractor tests with deterministic I/O faults and cancellation points, actual Windows no-clobber publication tests, repeated lifecycle tests showing handles are released after terminal join. Use a representative multi-entry/large-entry fixture, not a performance campaign or huge disk consumption.

**Acceptance:** actual payload/output bytes match selected valid entries, failures are categorized, limits hold, and native retirement/cleanup obligations are evidenced. A naturally completed read is not proof of an interrupted pending read; state which path was exercised and verify every cancellation path the implementation actually uses.

**Commit boundary:** `feat(archive): [C06/M6-B] stream selected entries with owned cancellation and cleanup`.

### C06/M6-C — ZIP user workflow and final extraction acceptance

**Objective:** finish a useful selective extraction tool backed only by the accepted writer/runtime.

**Code ownership:** proposed archive dialog/plugin and typed command route; current file picker/help/activity ownership; shared sizing/theme/Jump infrastructure.

**Tasks:** add Open ZIP, catalog filtering/selection, explicit destination, output/limits/conflict preview, Extract, Cancel, and terminal summary. Snapshot confirmed inputs; invalidate a plan when source/selection/destination changes. Explain unsupported entries and selection conflicts without silently extracting a different set. Keep busy/error/partial status reachable. Integrate resource cleanup when dialog/app closes. Add short usage/limits/recovery help.

**Behavior:** a user can choose a subset, extract once, see progress/cancel, and identify completed outputs and failures. Nothing writes merely because the dialog, catalog, or destination picker opened.

**Edges:** unsupported compression/encryption, stale catalog, empty selection, existing output, invalid destination, early/late cancellation, partial rerun, close/reopen while retiring.

**Regression constraints:** no implicit overwrite, auto-retry with different destination, unsupported archive formats, or changed Diff archive behavior. Writer controls remain unavailable until M6-A/M6-B safety acceptance succeeds.

**Verification:** focused route/UI state tests plus one supported native end-to-end flow on owned fixtures: select subset, preview, extract, verify contents, trigger existing-file conflict, cancel a multi-entry run, and close/reopen. Screenshots support interaction evidence; filesystem assertions establish actual writes and cleanup.

**Acceptance:** C6 is complete and usable, safety/lifecycle checks pass on the final source, and no extraction effect can bypass explicit confirmation. Otherwise report the exact blocked boundary and do not mark ZIP delivered.

**Commit boundary:** `feat(archive): [C06/M6-C] complete selective ZIP extraction workflow`. Close C06 and bootstrap C07 from its actual final tip.

### C07/M7-A — Measure and improve large-data startup

**Objective:** reduce a demonstrated startup bottleneck while preserving usable-data completeness and first-query behavior.

**Code ownership:** `src/startup.rs`, `startup_catalog.rs`, `main.rs`, GUI construction/indexing and current Note/Snippet/History loader owners found in M0-B; existing performance hooks/workloads.

**Tasks:** build deterministic small/large synthetic profiles with documented action/note/snippet/history counts and bytes, plus a bounded explicit indexed root when enabled. Record first-frame, input-ready, catalog-ready and first-usable-query timings as supported; distinguish measured from unavailable phases. Inspect the measured owner and choose at most two focused candidate changes. Preserve recovery-before-load and coordinator transfer identity. Prefer redundant-work removal; any deferred initialization must explicitly represent pending data and retain correct eventual results and first-query handling.

**Behavior:** large personal datasets load faster without lost entries, changed result identity/order, stale generations, startup side effects, or background work that continues needlessly when idle.

**Edges:** missing/malformed store, partial loader failure, first query during readiness, cancellation/shutdown during initialization, configured index with zero/invalid roots, repeated startup, cold versus warm cache.

**Regression constraints:** no reduced dataset/entry limit to fake speedup, disabled plugin, readiness-order bypass, moved cost hidden from measurements, or broad rewrite of startup/runtime ownership.

**Verification:** focused startup loader/catalog parity, recovery ordering, first-query and shutdown tests; up to three comparable supported Windows launches before/after under H. Use the same final feature set and synthetic profile for each comparison.

**Acceptance:** a measured improvement beyond observed variation with unchanged data/behavior, or a precise no-accepted-optimization/blocker disposition. Faster first paint alone is insufficient when useful data or first action becomes slower.

**Commit boundary:** `perf(startup): [C07/M7-A] reduce measured large-data initialization cost` (use truthful no-change wording when needed).

### C07/M7-B — Recheck development and runtime gains on the final tree

**Objective:** ensure new features did not erase the accepted development improvements or introduce a material startup regression.

**Code ownership:** measurement helpers/ledger and only owners implicated by an observed introduced regression.

**Tasks:** repeat the same representative incremental/selected-test and selected fixture measurements with final profiles/commands. Run small/large startup profiles again. Attribute unavoidable added dependency/input changes explicitly; keep headline before/after numbers comparable. Fix only a demonstrated feature-introduced regression within the bounded correction allowance. Document variance, sample counts, and correctness evidence; avoid percentage claims from incomparable or missing data.

**Behavior:** reported gains describe the delivered tree and normal developer usage, not an earlier isolated prototype.

**Edges:** cache invalidation, newly added Unicode dependency, source changes after earlier timing, unrelated machine load, missing native desktop.

**Regression constraints:** no new benchmark platform, broad tuning cycle, weaker tests, or third optimization candidate disguised as final verification.

**Verification:** reuse H's sample/cap limits and nonempty exact selections. Record source revision and commands for each final measurement.

**Acceptance:** final measurements and limitations are present; unverified performance is called unverified; all retained optimizations have relevant correctness evidence.

**Commit boundary:** `perf: [C07/M7-B] verify final iteration and startup measurements`.

### C07/M8-A — Final focused verification, review, and cumulative handoff

**Objective:** deliver one coherent cumulative chain with accurate feature status and no unresolved introduced blocker hidden by documentation.

**Code ownership:** complete task diff and the single execution ledger; relevant current help; first-party code only for necessary final corrections.

**Tasks:** run the affected-target selection, one current application build where required, and end-only focused first-party Clippy. Review the cumulative diff independently when the configured role exists. Check C1–C7 against recorded evidence. Fix substantive task-related findings with small forward commits; do not rewrite checkpoints. Update concise help and a single final user review checklist in the ledger. Capture all branch parent/baseline/checkpoint/final tips, push states, and remaining dirty user files. Prepare one cumulative PR summary; opening one final PR is allowed only when the configured workflow permits it. Never automatically merge/approve it.

**Behavior:** the delivered branch contains all accepted earlier categories by ancestry, and the user can distinguish usable features, measured improvements, and remaining blocked verification.

**Edges:** final reviewer finding, changed source invalidating an earlier test, remote push failure, integration branch moved, pre-existing unrelated lint/test failures.

**Regression constraints:** no full regression/native campaign by default, no vendor cleanup, no branch rewrite/merge to manufacture acceptance, and no claim that all seven categories completed when one is partial.

**Verification:** Section H's final checks, diff scope inspection, per-category direct-parent baseline checks, cumulative ancestry, and no owned job left running. Review findings must identify current source, not merely old receipts.

**Acceptance:** the Definition of Done is met, or final status is explicitly partial with concrete remaining blockers. Commit all intended task changes; capture the actual final HEAD after the last commit. The next Goal's required parent is this `final_tip_sha`, not `master`/`main`.

**Commit boundary:** `chore: [C07/M8-A] finalize productivity and iteration delivery`. The final report records this actual closing SHA, no worktrees/intermediate integration, and all push outcomes.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** Sections A0 and F define those boundaries.

The mandatory first commit on every category is its own M0-A baseline, directly descending from the predecessor's recorded final tip. Subsequent checkpoints stay on that same category branch. Commit complete, verified increments; do not wait for a single enormous final commit. A small forward remediation commit may accompany a checkpoint, with the same label plus a clear correction description. Do not split each helper/test into another branch or create artificial stage committees.

Before each commit: inspect working/staged diffs; stage only task-owned paths; remove your temporary diagnostics; preserve unrelated user edits; record exactly what checks ran. After committing, attempt one normal push. Push failure never undoes a local commit or blocks safe local continuation. At category closure update ledger status, commit intended changes, and hand off the actual tip. Reporting a partial category is allowed; pretending it is accepted is not.

## H. Focused verification, evidence, and budgets

### H1. Command selection

Resolve existing targets/selectors in M0-B and keep a short command table in the ledger. These are valid starting patterns from the current manifest, not fabricated exact test names:

```powershell
# Type check the relevant library/package first when that resolves the question.
cargo check --locked --profile fast-dev -p multi_launcher --lib

# Select the real Cargo target before the exact test/module expression.
cargo nextest list --locked --cargo-profile fast-dev -p multi_launcher --lib -E '<resolved-filter>'
cargo nextest run  --locked --cargo-profile fast-dev -p multi_launcher --lib -E '<resolved-filter>'

# Use this form for an actual integration-test target selected during intake.
cargo nextest run --locked --cargo-profile fast-dev -p multi_launcher --test <existing-target> -E '<resolved-filter>'

# Some startup/resolver tests belong to the binary, not the library.
cargo nextest run --locked --cargo-profile fast-dev -p multi_launcher --bin multi_launcher -E '<resolved-filter>'

# Build the actual application for affected native/UI checks.
cargo build --locked --profile fast-dev -p multi_launcher --bin multi_launcher

# End-only first-party lint; add changed test/package targets only as relevant.
cargo clippy --locked --profile fast-dev --no-deps -p multi_launcher --lib --bin multi_launcher
```

Replace placeholders with inventoried names; never paste them literally. Use `multi_launcher_table_core` checks only if this task actually changes or relies on a modified core boundary. Do not build every package/test/binary merely because it appears in the manifest. Refuse zero-match selections and retain the selected count/target. Verify installed runner syntax when uncertain; do not invent CLI timeout flags. A lockfile update for one justified new dependency is intentional and reviewed; use locked verification after that update rather than pretending an unchanged lockfile can resolve a new dependency. Offline mode is appropriate only when dependencies are available and it does not conceal a required fetch.

### H2. Separate preparation from execution

| Activity | Default bound | Treatment |
|---|---|---|
| Source/target discovery per category | One focused pass, normally 20 minutes; one additional 15-minute ambiguity pass if necessary | Stop expanding when ownership, callers and tests are known. Do not turn intake into research. |
| First root application/test preparation for a needed profile | 30 minutes per distinct necessary preparation | Record compile/link phase separately; a slow compile is not a timed-out test. Reuse prepared artifacts. |
| Later targeted compile/build preparation | 20 minutes per command | One adjustment up to 30 minutes allowed only when observed ongoing compilation justifies it; record before rerun. No repeating tiny historical caps. |
| Ordinary selected test execution | 100 seconds per test; 10 minutes per selected invocation after preparation | Use supported runner timeout/termination or an owned watchdog. A specifically justified finite native lifecycle case may have a predeclared 180-second bound. |
| UI/native scenario | 10 minutes per focused scenario; one supported recovery attempt | Do not repeatedly relaunch because an unsupported UI capability cannot be made to exist. |
| Optional optimization measurements | At most two candidate changes per optimization track, three comparable samples per condition; 60 minutes measurement-only per performance category | First preparation is separate. No cold full rebuild unless essential and pre-budgeted. Stop unproductive tuning. |
| Correction of one blocking finding | Initial attempt plus two focused corrective attempts | Re-run only affected checks; recurring unrelated failures are separate. No endless candidate cycles. |
| Final first-party lint | 30 minutes preparation/execution for the chosen targets | Once at the end, then only the affected target after a correction. No vendored cleanup. |
| Owned-process shutdown/drain after a deadline | 2 minutes; terminate only task-owned processes through the supported mechanism if needed | Record what actually retired. Do not call a timeout a pass because a late exit eventually arrived. |

These are verification bounds, not a promise of total project duration. Do not interpret them as permission to wait silently. Report whether work is exploring, compiling, executing tests, or blocked. Budget exhaustion does not weaken acceptance criteria. An actual platform/source-safety blocker must remain explicit.

### H3. Native and UI proof

Codex should build the real application, launch it using its supported isolated-data mechanism, interact with changed controls, and capture screenshots/evidence when the environment supports them. Do not request intermediate user testing. Native OS fixtures may be ordinary focused tests or a small existing-style harness that operates only on owned temporary data; missing general desktop-automation features do not automatically prohibit such tests.

Do not alter security policy, install an undisclosed driver/tool, inject into unrelated processes, manipulate arbitrary user windows, or replace supported testing with an unsafe workaround. When a desktop capability is absent, record the exact observation after one recovery and proceed with independent work where safe. A screenshot of a window proves only what it visibly shows; it does not prove bytes saved, native handle retirement, focus delivery, or a successful OS action.

Mandatory ZIP filesystem proof cannot be waived because the UI can be drawn. Conversely, do not require the ZIP category to finish unrelated Table or Hot Folder native protocols. Focus on the actual writer's source/containment/publication/lifecycle guarantees. Test every safety path that the delivered implementation uses; do not invent an unused pending-I/O design solely to satisfy a historical matrix.

### H4. Evidence quality and performance comparison

For each actual command record command/working directory, branch/source revision, target/filter, profile/toolchain when relevant, preparation versus run time, exit code, selected/passed/failed/timed-out counts, and raw output location. For UI checks record the action sequence and screenshot/result locations. Never log pasted text, URLs with secrets, raw notes, or user-private archives into routine reports.

Reuse an earlier passing result only when the exact relevant source and inputs are unchanged; label it reused, not newly run. After an affecting source correction, re-run the affected check. Do not certify production from tests of an archived candidate. One passing narrow fixture is not proof of all workers or all UI interactions.

Performance reports must include samples and medians/ranges, not just a percent. Keep workload, profile, cache conditions, hardware/toolchain and command selection comparable. First-frame timing and complete-data/first-action timing are separate. No-op, edited incremental, full-debug, and reduced-symbol results are separate. Include memory/idle sanity observations when deferred startup work might trade latency for retained resources or wakeups. Document an unmeasured quantity instead of estimating it as fact.

## I. Risk management and bounded recovery

| Concern | Required mitigation and stopping boundary |
|---|---|
| Scratch Diff leaks into file saving/history | Explicit source origin, no fake paths/temp files, explicit Save As, and raw-content privacy checks. |
| Ignored blocks hide real changes or move after edits | Source/revision mappings, mixed-hunk tests, raw/effective equality labels, restoration, and visible invalidation of ambiguous annotations. |
| Quick Tools executes on hint completion or loses duplicate identities | Separate focus/selection from activation, namespaced entry identities, and repeated-pass/reorder coverage. |
| Density or persisted sizes make controls unreachable | Existing theme publication/cache ownership, logical-size clamps, reset, and representative narrow/DPI checks. |
| New Macro opens widen execution authority | Typed HTTP/HTTPS/folder validation, direct OS adapter, no shell text, and existing unattended capability review. |
| ZIP escapes/clobbers or leaves active writers | Bound native source/destination ownership, current no-clobber checks, streaming limits, owned staging cleanup, and terminal retirement before reuse. |
| Optimization weakens correctness or merely moves latency | Fixed workloads, preserved assertions/readiness, separate preparation/run timings, and first-usable plus full-readiness measurements. |
| Pre-existing compile/test issue blocks a chosen target | Preserve original failure; make only the minimal necessary in-scope dependency correction, or mark that target blocked. Do not weaken a legitimate test or repair unrelated subsystems. |
| Missing native tool or exhausted budget | One supported recovery and two focused corrections at most; distinguish implemented-unverified from complete. Continue independent safe categories only after an explicit committed closure. |

A blocker report must name the exact boundary, source/command, observation, attempted correction(s), protected invariant, and smallest remaining step. Distinguish a product defect from an environment limitation and from an optional optimization. Do not produce a fresh multi-goal remediation program automatically. Do not revert accepted current behavior or silently import a previously rejected candidate to satisfy a deadline.

## J. Definition of Done

Full completion requires all of the following:

1. C1–C6 user-facing behavior is implemented and the corresponding functional criteria pass; the requested C7 improvement areas have accepted, measured outcomes. An area assessed without an accepted optimization is a valid bounded investigation outcome but is not a delivered speedup; explicitly report partial fulfillment of the requested optimization work rather than advertising full completion or acceleration.
2. Both approved Diff choices are implemented exactly: whole-line ignored ranges and the focused two-way extras; existing alignment is reused; prohibited expansions are absent.
3. Existing Macro documents/routes, file/folder Diff, hotkeys, theme defaults, settings/recovery, command ownership, and user data are preserved.
4. Tests are selected by real target/name, nonempty, bounded, and relevant; changed behavior and material regressions have evidence. No known introduced blocking defect remains.
5. Required native ZIP safety checks pass before extraction is enabled. Visible UI/native behavior is actually exercised where supported; missing verification is explicitly reported. A mandatory unverified criterion means partial completion, not full Done.
6. First-party final lint/review findings introduced by this work are resolved or clearly nonblocking; no vendor/unrelated cleanup was added.
7. Current help accurately describes commands, ignored/raw equality, explicit clipboard behavior, size/density controls, and ZIP limits/partial outcomes.
8. All task changes are committed as coherent checkpoints on the correct seven-category ancestry chain; every M0-A baseline is a direct child of its required predecessor; no worktree/auxiliary checkout or intermediate integration was used.
9. Every push state is truthful. Failed pushes preserve local commits and do not invalidate an otherwise correct local result.
10. The final branch's actual `final_tip_sha` is captured after all intended commits and handed off as the next Goal's required parent. Only one cumulative final integration is contemplated; no automatic merge/approval.

If any requirement remains blocked, provide a partial delivery with a per-category outcome table. Do not use a program ledger's completion as a substitute for implemented, verified features.

## K. Final delivery report required from Codex

Keep the final report factual and useful. Include:

- A C01–C07/checkpoint status table distinguishing completed, implemented-unverified, blocked, and assessed-no-accepted-optimization.
- What the user can now do, concise usage entry points, and key implementation decisions.
- Principal files/owners changed and any narrowly justified dependency or compile-boundary change.
- Actual tests/build/lint/review results, selected counts, native/UI evidence, and explicit limitations; no aggregate invented from unrelated historical runs.
- Before/after compilation, selected-test, fixture, and startup measurements with profiles/workloads/sample counts; separate unmeasured outcomes.
- Per-category branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, significant checkpoint SHAs, `final_tip_sha`, actual pushed/local status, and dirty-file disposition.
- The final cumulative tip and the instruction: **the next Goal must create its fresh child branch directly from this `final_tip_sha`, not from `master`/`main`.** Confirm no Git worktrees, auxiliary clones/checkouts, or intermediate merge/rebase/cherry-pick were used.
- Remaining issues and one end-of-work manual review checklist for the user. Intermediate manual work must not have been delegated to them.

Stop at the approved boundary. Do not automatically begin another feature, optimization wave, follow-on Goal, or merge.
