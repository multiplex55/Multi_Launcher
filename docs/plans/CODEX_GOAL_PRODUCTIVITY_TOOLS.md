# Codex Goal — Productivity Tools

Companion execution prompt: **CODEX_KICKSTART_PRODUCTIVITY_TOOLS.md**

This is one implementation contract for the eleven selected features. Execute it autonomously, using the current repository and its engineering instructions. The requested scope is fully specified below; routine engineering choices belong to Codex.

## A0. Mandatory Git Branch Bootstrap — Before All Application Work

### M0-A — Create and baseline a fresh task branch

**Objective.** Establish an isolated, uniquely named branch and a new baseline commit before application edits, builds, tests, implementation checkpoints, or source modifications. Read-only inspection is allowed first. This requirement applies even if the current checkout is already on a feature branch.

**Ownership.** The orchestrator owns Git operations, the index, branch selection, commits, pushes, and the execution record. No implementer may begin feature work before this checkpoint passes.

**Required sequence:**

1. Read the complete applicable AGENTS.md files, configured agent instructions, this Goal, and its Kickstart. Inspect git status -sb, git branch --show-current, git rev-parse HEAD, staged and unstaged differences, untracked files, remotes, upstream, and the repository's actual default integration branch. Record pre-existing work without exposing credentials in logs.
2. Select a task-specific name such as **feat/productivity-tools**, using lowercase kebab-case and an idiomatic prefix. Check local and remote branch names. If occupied, choose a meaningful unique suffix. Never silently reuse an existing task branch.
3. Fetch the selected existing remote normally when permitted. Recheck the chosen branch name against the refreshed local/remote refs before creating it. If remote access is unavailable, record that remote uniqueness could not be verified and choose a locally unique suffix; never force an initial push onto an existing remote branch. Prefer the up-to-date default integration branch as the base when the checkout is clean and switching is safe. A new branch may be created directly from the verified remote-tracking tip without moving or resetting the user's local master/main.
4. If a dirty checkout prevents safely changing the base, preserve its working tree and index and create the new branch from current HEAD when that is safe. Record the reason and actual base. Do not automatically stash, reset, hard-pull, rebase, merge, clean, force a checkout, or discard changes. An unresolved merge or genuinely unsafe transition is a bootstrap blocker.
5. Create and switch to the new branch. Confirm the active branch before the first write. Record the original base SHA.
6. Create the new branch's own **M0-A baseline commit**. Prefer a small legitimate task execution record, following project conventions. If no such tracked change is appropriate, use an intentionally empty commit. Never include unrelated staged or unstaged user changes. In particular, plain git commit --allow-empty can include an existing index: an empty baseline that excludes staged work can use the documented --only form:

~~~text
git commit --only --allow-empty -m "chore: [M0-A] establish productivity tools branch baseline"
~~~

7. Record the baseline commit SHA immediately after it exists. It cannot contain its own hash; put that hash in the execution record in the next task-owned update. Use this new baseline for all subsequent task diffs, and retain the original base SHA separately. Do not amend or rewrite the baseline.
8. Make one normal best-effort initial upstream push to the verified remote, for example git push -u origin HEAD when origin is actually the intended remote. If remote access is absent, an approval is denied, or the push fails, preserve the branch and local baseline, record the real outcome, and continue locally. Do not bypass approvals, change remotes, force-push, or retry in a loop.
9. Keep all implementation, tests, checkpoint commits, and later push attempts on this same new task branch. Never return silently to master, main, or the previously active feature branch. Attempt a normal best-effort push after each checkpoint when permitted; a push failure does not block local implementation.

**Acceptance criteria.**

- A unique new task branch is active.
- Its own baseline commit exists before any application work.
- The original base SHA, baseline SHA, chosen base/ref, initial branch, and actual initial push result are recorded.
- Pre-existing user files and index contents are preserved and excluded from task commits.
- The first implementer packet is released only after these conditions are verified.

**Commit boundary.** M0-A is mandatory and precedes every later checkpoint. A failure to create the branch or local baseline is a real blocker; a failed push is not.

The current source findings in Section B do **not** override this base-selection rule. Do not automatically branch from the historical planning reference or merge another feature branch merely to match these notes.

---

## A. Goal and Desired Outcome

Make existing launcher workflows quicker to discover and use, and add three focused utilities. Deliver all eleven selected items:

| Request number | Required outcome | Implementation milestone |
|---|---|---|
| 1 | Search application settings, including built-in plugin settings, and navigate to the relevant control. | M1 |
| 7 | Repair saved application, file, and folder targets while retaining their metadata and associated preferences. | M2 |
| 23 | Add remove-blank-lines, number-lines, reverse-lines, join-lines, and split-lines to Clipboard Modify. | M3 |
| 24 | Search and preview snippet/history sources while adding Paste Sequence steps. | M4 |
| 46 | Place automatically numbered markers in the screenshot editor. | M5 |
| 48 | Start a screenshot after a short cancellable countdown. | M6 |
| 54 | Compare two images using a wipe slider and highlighted pixel differences. | M7 |
| 56 | Adjust individual static and live pin opacity. | M8 |
| 61 | Replace one/all matches inside the current note. | M9 |
| 90 | Inspect CSV/TSV, filter/sort rows, copy selected columns, and convert to JSON. | M10 |
| 41 | Find files with identical contents through the new **dff** plugin. | M11 |

Success means the features work through ordinary launcher commands and existing UI ownership, preserve the relevant working behavior, and have scoped automated and visual evidence. A visible button with no complete execution path does not satisfy the contract.

There is no implementation authorization for unrelated workspace features, broad framework rewrites, or additional feature candidates from earlier discussions.

## B. Current Codebase Findings and Source Authority

### B1. Inspected reference

The planning inspection used repository **multiplex55/Multi_Launcher**, commit **04e51606c2e1862e262454f424ed576bb4bc0814**, on **jump**. Its commit time was October 21, 2026, 7:41:06 p.m. America/New_York. The repository's default branch was **master**, then at **0cb3dabf148dc720ed560626c32ef596b5e8ff9a**. The inspected jump head was 46 commits ahead and zero behind that master head.

These are provenance facts, not a demand to check out an old commit. After M0-A, reconcile the relevant locations with the actual new branch. Do not import the 46 unrelated commits or recreate their entire testing program. Preserve whichever relevant behavior is actually present on the safely chosen base. If a necessary seam differs, adapt this plan at that seam and record the difference.

Source links at the end of this document refer to the inspected commit. Proposed files and types below are explicitly proposals; their names are not claims about the existing repository.

### B2. Findings that affect the implementation

| Area | Verified current owner or seam | Implication |
|---|---|---|
| Settings | SettingsEditor in src/settings_editor/state.rs; TopLevelSettingsSection, SettingsJumpTargets, section rendering and reveal_paste_sequence in src/settings_editor/render.rs. | Stable section/widget identities and a reveal precedent exist. Search needs a catalog and navigation request, not a replacement settings editor. |
| Settings outside the main editor | ThemeSettingsDialogState; MouseSettingsDialog; MouseGestureSettingsDialog; PluginEditor; SettingsSaveIntent and src/settings_save.rs. | These editors have distinct draft, save, or live-application policies. Navigation must respect them. |
| Plugin settings | Plugin::settings_ui is an opaque UI callback; render_plugin_sections currently skips disabled plugins. DLL loading returns `Box<dyn Plugin>`. | Field metadata must be explicit for built-ins. Do not change the dynamic plugin trait/vtable merely to index unknown external plugin controls. |
| Saved targets | Action contains label, desc, action, args. AddActionDialog::open_edit already edits actions, but its picker selects files and DialogMode::Edit uses an index. FolderEntry contains label, path, alias. | Folder browsing and safe original-record validation are required. Do not rely on a stale vector index. |
| Target publication | actions::update_actions; LauncherApp::update_custom_actions; publish_custom_actions_with_indexed_tail; folders::update_folders. | Reuse owner transactions and live publication. A repair must not discard the indexed tail or trigger an unrelated full scan. |
| Target references | PersistableActionTargetRef::CustomAction embeds an Action; folder targets use paths. Usage, HistoryPin, and FavEntry also contain action/path associations. | Updating only the visible path can strand preferences and active references. Plan exact typed updates and collision handling. |
| Clipboard Modify | OperationId, StageArguments, the authoritative catalog/parser/executor, PreviewCoordinator, guarded clipboard writes, and saved pipelines already exist. | Extend this pipeline end to end. Existing line transforms use input.lines() and LF joins. |
| Paste Sequence sources | PasteSequenceDialog has simple menus; Step::SnippetRef stores alias; clipboard_history_reference stores SHA-256 and a neutral label. | New browsing must keep reference semantics. A clipboard preview is not permission to persist its text in the sequence. |
| Hidden snippets | SnippetEntry has hide_contents. The existing general snippet matcher includes bodies. | The new picker needs a masked search projection so hidden text cannot be inferred from result filtering. |
| Screenshot markup | MarkupTool, MarkupLayer, MarkupHistory, render_markup_layers, apply_edits, and shared annotation/raster helpers. | Add one retained annotation type and implement both preview and exported raster rendering. |
| Screenshot capture | ScreenshotPlugin → parse_screenshot → handle_screenshot → ScreenshotCommandHost → launch_editor → capture_raw. Capture is synchronous today. | A delay needs real pending/completed/cancelled state. Scheduling must not be reported as capture success. |
| Capture semantics | Window reads foreground at capture time. Region starts the existing selector and has a separate timeout. Desktop currently captures the screen at (0,0). | Preserve these meanings; countdown must not consume selection time or silently expand desktop capture to all monitors. |
| Existing Diff | DiffView has Start, TextCompare, BinaryCompare, FolderCompare. The query grammar accepts diff followed by file paths. | Use a separate imgdiff plugin/dialog. Reserving the filename image as a new diff subcommand would change valid existing input. |
| Pins | Pin, PinWindowPresenter/Runtime/State, PinControlChange, PinListEntry/Action, PinNativeApi, validated native child-window ownership. | Add opacity to the existing state/control flow and prove actual desktop translucency. Image tint over an opaque background is insufficient. |
| Notes | NoteFindState has literal Unicode case-insensitive matches with byte and character ranges. NotePanel owns dirty state and source mutation. | Replace must use the live unsaved Markdown source and existing save policy. LauncherApp::mutate_note_by_slug saves immediately and is unsuitable for inline Replace. |
| CSV/TSV | No table utility or direct csv parser dependency was present. serde_json, rfd, egui_extras and worker patterns exist. | Add a bounded table domain and normal plugin/dialog integration; a focused CSV parser/writer dependency is justified if still absent. |
| File scans and hashing | src/diff/folder_scan.rs has cancellable traversal/reparse checks; src/file_search/path_identity.rs has path helpers; hash_request and HashCancellationToken stream through a 256 KiB buffer. | Reuse suitable primitives, but keep duplicate scanning independent of the Diff pair model and Hash Generator dialog state. |
| Windows file identity | The inspected Diff RootIdentity supplies an inode token only on Unix; non-Unix returns None. | It is not enough to identify Windows hard links. Use a validated handle-based Windows file identity when available. |
| Shared routing | src/plugin.rs registration; commands model/parser/host/bus/handlers; GUI Panel/focus/render/close paths; Jump navigation. | A new utility must be integrated through every relevant owner rather than dispatched by an ad hoc string handler. |
| Existing QA | dev/jump_mode/run.ps1, its fixture generator, qa-framebuffer-capture, and tests/fixtures/pinboard_live_source.ps1. | Reuse selected fixture and capture capabilities. Their historical full scenario catalog is not a required gate. |

## C. Functional Requirements and Chosen Defaults

The selected features are sufficiently clear to proceed without a questionnaire. The choices below resolve ordinary product details conservatively and are part of this contract.

### C1. Settings search

- Add search to the main Settings experience and support **`settings <query>`** through the existing settings plugin. Empty settings keeps its current opening behavior.
- Search labels, descriptions/help text, owner/section names, and a small set of practical synonyms. Do not search current setting values, private paths, tokens, or clipboard content.
- Include all existing application-owned editable settings: main sections, built-in plugin settings, and dedicated Theme, Mouse, Gesture, and plugin-selection surfaces. Build a real inventory; the four requested queries are examples, not the coverage boundary.
- Show result labels and owner/section breadcrumbs. Selecting one opens or reuses its owner, expands ancestors, scrolls to the real rendered control, highlights it briefly, and focuses it when appropriate.
- Selection must not activate a button/toggle, alter a value, enable a plugin, apply a live setting, save a draft, or discard an open draft.
- Index disabled built-ins without starting their runtime. Render their setting controls safely without enabling the plugin where supported; otherwise explain the disabling prerequisite in the owning surface. A conditionally unavailable control must not be silently enabled by navigation.
- Search and ordinary keyboard operation work with Experimental Jump Mode on or off. Use stable control identities and current rendered geometry; do not depend on Jump Mode being enabled.
- Unknown external DLL plugins expose only an opaque settings callback today. Provide truthful owner/section discovery for those plugins unless compatible metadata already exists. Do not describe a section link as an exact setting result or break the plugin ABI to satisfy hypothetical external controls.
- Include newly added settings in the inventory if any later milestone genuinely introduces them. Per-request input fields in utilities do not need to become global settings.

### C2. Repair saved application, file, and folder targets

- Provide an explicit **Repair target…** operation for saved custom launch actions and saved folder entries. It is available from their existing item/editor actions and relevant missing-entry maintenance results.
- Display the old target and a replacement field. Offer file/application and folder selection appropriate to the record. Folder records must point to directories. A custom action originally targeting a directory can be repaired to a directory.
- Persist only after the user confirms a valid replacement. Do not execute the new target, auto-search drives, guess a similarly named file, or remove the original entry automatically.
- Preserve label, description, arguments, folder alias, ordering, and applicable existing preferences. Carry exact active saved references to the repaired target through their owning stores; historical execution records remain historical.
- Distinguish a missing local filesystem target from a URL, shell/internal command, access-denied path, offline/network target, or otherwise unknown inspection result. Do not classify every nonexisting-looking command string as a broken file.
- Capture and revalidate the original owner/record when committing. Reordering or external editing must never cause a different item at an old index to be repaired.
- Do not repair an indexed result by silently converting it into a saved custom action. Do not treat two actions with different arguments as one item.
- Reject ambiguous destination collisions before mutation. Do not overwrite an existing destination's metadata or merge unrelated preferences silently.
- Preflight all required exact reference updates. Reuse existing store locks/atomic writes and a narrow failure-recovery strategy so a later write failure does not leave a success notification with stale references. Defer the involved owners' normal immediate publication, including relevant watcher reloads, until the coordinated outcome is known. Do not promise that separate atomic file writes form one atomic multi-file transaction. If rollback cannot complete safely, show an explicit recovery conflict, retain recovery data, and do not describe the operation as unchanged or successful.
- If an active reference family cannot be updated safely, show a concrete repair conflict before committing; do not silently leave it stale. This is a runtime error state to implement, not a request to expand into a global identity migration.

### C3. Clipboard Modify line operations

Use new canonical names under **cm**, subject to the existing parser's token/quote rules:

| Operation | Required semantics |
|---|---|
| remove-blank-lines | Remove lines whose trimmed text is empty; preserve every retained line's contents. |
| number-lines | Prefix logical lines with 1. , 2. , and so on, starting at one; count existing interior empty lines. |
| reverse-lines | Reverse logical line order, retaining interior blank lines. |
| join-lines | Join logical lines using a literal separator; default one space; explicitly empty separator is valid. |
| split-lines | Split on a required nonempty literal separator, preserve empty fields, and join resulting fields with LF. |

- Follow existing line-operation conventions: CRLF normalizes to LF and ordinary line operations do not retain a final line terminator. split-lines deliberately preserves empty split fields, so a final empty field can produce a terminal LF. Do not rewrite old transforms to impose new newline semantics.
- Empty input produces empty output; no fabricated “1.” line. Numbering is ordinary decimal text, not a new formatting language.
- Add an optional typed separator argument; do not misuse prefix/suffix fields or globally reinterpret backslash escapes.
- Expose the same operations in commands, preview, Apply, Manage Pipelines, saved pipelines, and generated help. Invalid arguments never write the clipboard.
- Preserve old configuration loading, existing pipeline names and invocation, guarded clipboard writes, undo, cancellation, and stale-preview rejection. Adding a reserved operation name must not cause an old user pipeline/template to be reset or become unreachable.

### C4. Searchable Paste Sequence source pickers

- Retain Add Snippet and Add Clipboard Item entry points. Each opens a bounded picker with search, scrollable results, a read-only selected preview, Add, and Cancel.
- Normal snippets may match alias, tags, and body. Hidden snippets match alias/tags only and show a fixed concealment message. Hidden bodies must not appear in excerpts, accessible labels, tooltips, logs, highlights, or result-count side channels.
- Preview raw snippet templates without evaluating prompted fields. Browsing must not run a snippet, open its prompt, change the clipboard, or begin playback.
- Clipboard rows show useful excerpts; the selected preview may show the full content within a bounded scroll area. Keep full content distinct from truncated display text.
- Bind selection to source identity rather than a filtered row index. Add exactly one step through the existing Step constructors, reveal/select that draft step, and close the picker.
- Snippet steps continue to store aliases. Clipboard steps continue to store content fingerprints/references and neutral labels, with the same expiry and run-preparation behavior.
- Source reload, popup close, parent close, and playback transitions invalidate stale picker state appropriately. An unavailable source is distinguishable from no matches.
- Escape closes the picker first. Search typing and Enter must not fall through to playback or launcher execution. Preserve sequence ordering, saved drafts, keep-visible behavior, and all existing playback semantics.

### C5. Numbered screenshot markers

- Add a numbered-marker tool to the existing screenshot editor.
- One primary click creates one circular badge, beginning at 1 for each editor image and increasing thereafter. Use existing palette/size conventions.
- Retain each badge's assigned number, image-space position, color, and size as annotation state.
- Undo/redo uses MarkupHistory. Redo restores the original number. The next number follows retained marker history; undoing marker 3 and placing a new marker yields 3 and invalidates the abandoned redo branch.
- Switching tools does not reset numbering. A newly opened image does.
- Single and multi-digit numbers are centered and legible. Preview, copy, save, crop, and zoom agree because they share the same retained annotations.
- Preserve right-button cropping and the existing Pen, Arrow, Rectangle, Highlight, and Text interactions. No independent markup history or new image/text renderer.

### C6. Delayed screenshot capture

- Preserve all existing immediate ss actions and their `screenshot:<mode>` representations.
- Add delayed choices under **ss**, using **3, 5, and 10 seconds**. An additive query such as ss delay 5 should offer the existing applicable capture/destination choices with that delay attached. Keep original action ordering/behavior for plain ss. A custom delay editor is outside the initial scope.
- Add a visible delayed-capture entry and an **ss cancel** action while a countdown exists. The countdown must have a reachable cancel control even while the main launcher is hidden/parked; a small non-activating countdown window is an acceptable screenshot-owned implementation.
- Permit one pending delayed request. Further scheduling gets a clear busy response. Cancellation, application shutdown, or superseded lifecycle state prevents a later capture.
- Use a monotonic deadline and event/poll scheduling, never a sleeping GUI thread. Keep the deadline/update path active while the launcher root is hidden or parked; normal launcher hiding neither cancels nor suspends the request. Closing the countdown itself, explicit Cancel, application shutdown, or invalidating the request does cancel it. Pause no unrelated worker, hook, or live pin.
- New delayed screenshot actions and ss cancel require the GUI. A headless route returns a clear GUI-required error before capture or external execution. Never silently ignore a delay through an existing wildcard match or send an internal delayed-action string to an external-command fallback. Existing immediate headless screenshots retain their behavior.
- For Window, select the foreground target at expiry, allowing the user to switch applications or open a menu during the countdown. The countdown window must not become that target.
- For Region, start the existing selector only after the countdown. Its existing selection timeout starts then.
- Hide countdown UI and verify the relevant visibility transition before acquiring pixels. Preserve existing source/mode, destination, markup, editor, save, query, history, and focus policies.
- Scheduling is not success. Record success once after actual capture; cancellation before capture produces no file, clipboard write, or successful history entry. Once selection/capture has begun, retain existing platform cancellation semantics and reject stale completion.
- Restore owned UI correctly on cancel/error; do not steal focus from the user unnecessarily.
- Do not rewrite PinboardCaptureLifecycle or Screen Draw as part of adding a timer.

### C7. Image comparison

- Add **imgdiff** as a distinct plugin prefix. Plain imgdiff opens a dedicated comparison dialog. Do not change the existing diff filename grammar, automatic text/binary routing, or saved Diff sessions.
- Provide two independent read-only inputs, A and B, each loaded from a file picker or an explicit Paste Image action. Show source name and decoded dimensions.
- Implement shared zoom/pan, a wipe slider, and a difference-highlighting mode. At wipe endpoints show only A or B, with the missing area clearly represented.
- Compare decoded RGBA pixel values exactly; a pixel differs when any channel differs. Include alpha differences, including otherwise invisible channel differences, and label this exact-pixel meaning.
- Do not rescale or register images automatically. Align native pixels at the top left; for unequal dimensions, indicate the mismatch and mark non-overlapping regions distinctly.
- Corrupt, unsupported, or oversized input must show a useful error without replacing the last valid input with a partial result.
- Use existing image, arboard, rfd, texture and worker patterns. Respect the distinct image and image_025 dependencies. No new image codec/renderer is required.
- Initial defensive limits: at most 16,777,216 decoded pixels per input and comparison canvas, and a 512 MiB accounted combined working budget covering decoded buffers, difference buffers and retained texture/upload copies. Check dimensions and multiplication before allocation. These are internal limits, not new settings; a measured adjustment may be documented if needed.
- Inputs/results are session-only. Closing cancels work, rejects stale completions, and releases textures/buffers. No source edits, image registration, live file watching, perceptual similarity, comparison-report export, or saved image sessions.

### C8. Per-pin opacity

- Add **10–100% opacity**, initially **100%**, independently for every static and live pin.
- Provide a slider and an explicit 100% reset in the pin and Pin List. The list provides recovery when a pin is translucent or click-through.
- Change actual native-window/compositor opacity so the application behind the pin is visible. Tinting image pixels over an opaque pin background does not meet acceptance.
- Keep opacity session-only, matching existing pin lifetime. Copy/Save/live source frames retain their original pixels and alpha; opacity is a display property.
- Preserve click-through, topmost, zoom, drag/resize, close/reopen lifetimes, live texture reuse, capture suppression, and image budgets. Temporary hiding/restoration retains opacity.
- Apply native changes only to a currently validated owned pin child. Reject launcher-root, foreign, destroyed, stale-lifetime, and unregistered windows. On native failure, keep the last successfully applied value and show the real error.
- Use the existing PinNativeApi boundary or a verified existing transparent-child composition path. Do not undertake a vendored renderer rewrite.

### C9. Replace in the current note

- Extend the existing Find strip with a replacement expander/field, Replace, and Replace All.
- Preserve existing literal Unicode case-insensitive source matching. Replacement text is literal, including dollar signs, backslashes, punctuation, and empty text. No new regex mode.
- Work on the current note panel's live unsaved Markdown source. Never replace across other notes or only inside rendered display strings.
- Empty search disables replacement. Zero matches and no-content-change replacements do not dirty the note.
- Replace changes the selected match and advances beyond inserted text. Replace All computes nonoverlapping matches from the original current source, applies once, and never recursively replaces newly inserted text.
- Validate query/content revision before applying byte ranges. Update caret/selection using the correct character offsets. Recompute Find and Preview mappings after mutation.
- Replacement from Preview makes its Markdown-source effect clear and uses existing source-reveal behavior for source-only matches.
- Update through NotePanel's existing mutation/dirty path and preserve autosave/manual-save/discard behavior. Do not use the immediate-save mutate_note_by_slug route.
- Each Replace or Replace All is one undoable operation in the existing editor history. Undo/redo and subsequent normal typing continue to work, including Edit/Split/Preview transitions. No second note history or vendor changes.
- Preserve Find keyboard behavior and query focus. The Replace expander is sufficient; no new global hotkey is required.

### C10. CSV/TSV table utility

- Add the **csv** plugin, which handles both CSV and TSV through a visible delimiter selector. Plain csv opens the utility.
- Provide explicit Open and Paste Text, CSV/TSV selection, a header-row toggle, a read-only table, a literal case-insensitive filter across all columns, stable column sorting, column selection, Copy, and JSON conversion/copy.
- For files, initialize delimiter from .csv/.tsv; unknown extensions use the visible current choice. Paste uses the current choice. Default is comma with headers on. Do not silently guess a new delimiter after every edit.
- Keep cell values as strings: preserve leading zeros, large identifiers, whitespace, empty strings, and text resembling booleans/numbers.
- Filter and sort are view projections. Sort lexically with a documented case-insensitive comparison and stable original-row order for ties; never silently coerce cells to numbers.
- Copy/export uses filtered rows in current displayed sort order and selected columns in original displayed-column order. Initially all columns are selected. Show the output row/column counts. Copy includes headers when header mode is enabled. With zero selected columns, disable Copy/JSON and show a select-a-column hint.
- Use proper CSV/TSV parsing and quoting, including quoted separators, doubled quotes, and multiline fields. Never implement parsing by splitting lines or commas.
- JSON defaults to an array of objects when headers are nonempty and unique. Offer an explicit array-of-arrays shape, including for no-header input. Empty/duplicate headers disable object output with a clear explanation; arrays remain available without dropping or renaming data silently.
- Enforce a consistent row width. A ragged/malformed record produces a record-oriented error and keeps the last valid table usable. Label retained results as previous valid data and retain their successful delimiter/header interpretation; do not present newly failed parse options as if they describe that old table. Accept UTF-8 with an optional BOM; reject unsupported encodings clearly.
- Initial processing limits: 16 MiB input, 100,000 data rows, 256 columns, 1,000,000 cells, and 64 MiB per materialized output. Check limits incrementally and reject oversized work before unbounded allocation; do not silently truncate.
- Parse/filter/sort/export sufficiently large work off the GUI thread, with generation/cancellation checks and virtualized rows. Do not reparse on every frame.
- Opening a file never overwrites it. No cell editing, formulas, XLSX support, inferred types, database, saved table sessions, or background clipboard watching.

### C11. Duplicate-file finder

- Add a new plugin whose visible command prefix is exactly **dff**. Plain dff opens the finder; provide discoverable help consistent with the plugin conventions.
- The user selects one or more roots and explicitly starts scanning. Recurse within selected roots; never scan drives automatically on startup or merely while typing dff.
- Find **exact file-content duplicates**, regardless of filenames. Include zero-byte files as valid candidates. Similar images, near-duplicate text, and duplicate directories are outside scope.
- Start with regular-file discovery and size grouping; hash only candidate size groups using streaming SHA-256; confirm candidate equivalence with bounded byte comparison before reporting verified duplicate groups.
- Deduplicate overlapping roots and equivalent path spelling. Recognize hard links through reliable file identity where available; aliases of the same file are not independent duplicate copies. On Windows use volume plus file ID from a validated open handle, not a fabricated Unix identity. Unsupported/invalid identity is not proof that each path is a distinct file. Report content matches whose independence cannot be established separately as identity-unverified; exclude them from verified independent-copy groups/totals. Unknown/all-zero IDs must neither merge unrelated files nor count aliases as separate copies.
- Skip directory symlinks/junctions/reparse traversal and do not follow file indirections automatically. Report skipped/unavailable entries. Do not trigger a scan of linked trees, devices, named pipes, or cloud placeholders just because they were encountered.
- Revalidate file identity/size/modification state across reads. Files that change, vanish, or cannot be read are skipped with a reason, never reported as confirmed equal from stale metadata.
- Show stages, progress, Cancel, grouped paths/sizes/counts, and partial/completed status. Any duplicate-byte total is a logical-content estimate; do not claim exact disk space recoverable from sparse/compressed/hard-linked storage.
- Offer Open, Reveal in Explorer, and Copy Path(s) through normal existing file actions. **This delivery is a finder: no delete, recycle, automatic keeper selection, move, hard-link replacement, or deduplication writes.**
- Run one bounded scan worker at a time. Use bounded/coalesced progress, fixed-size I/O buffers, and finite metadata/results/error limits. An initial 250,000 regular-file ceiling is acceptable; visibly mark a limit-hit scan partial, never “complete, no duplicates.”
- Cancellation stops publication immediately and is checked at traversal/hash/compare boundaries. Native synchronous I/O may take time to return; do not promise instantaneous OS cancellation, block the GUI waiting for join, or accumulate workers after repeated Cancel/Start.
- Sessions are transient. Close cancels/reaps work through existing lifecycle patterns, releases resources, and prevents old results from appearing in a reopened session.

## D. Scope, Non-Goals, and Invariants

Implement the listed features completely through existing owners. Do not introduce workspaces, unrelated dashboard redesigns, global command rewrites, a new settings framework, a new clipboard service, or a general-purpose workflow/transaction engine.

The following invariants are mandatory:

1. **Command compatibility:** old valid queries/actions and saved action strings still resolve to their original behavior. New prefixes do not capture lookalike input or arbitrary filesystem paths.
2. **Input ownership:** text fields retain text input; Enter executes only the focused surface's intended action; nested Escape closes the appropriate surface first. New controls follow existing focus/Jump ownership, with no global input hacks.
3. **Persistence:** malformed stores are not silently reset; failed saves do not publish a successful mutation; defaults deserialize safely; unrelated records retain contents/order.
4. **Clipboard:** browsing/searching/previewing is not Apply; hidden snippets stay concealed in the new picker; sequence references and playback are unchanged.
5. **Capture and pins:** selection/cancellation, click suppression, destination handling, native child ownership, and live-frame image lifetime remain correct. Opacity never contaminates stored/exported pixels.
6. **Notes:** the live unsaved buffer is authoritative; save/dirty/discard/undo behaviors and other note panels remain intact.
7. **Responsiveness:** no filesystem traversal, repeated hash/parse work, countdown sleep, long join, or large image diff on the GUI frame path. Closing a tool invalidates its pending results.
8. **Isolation:** new tools do no work at application startup beyond ordinary lightweight registration. No always-running watcher, timer, hook, or drive scan is added.
9. **History:** cancellation/scheduling is not falsely logged as success. Do not rewrite historical execution rows merely to make active references easier. Current usage-count associations may be transferred only as required by the guarded repair contract.
10. **Repository discipline:** preserve user changes, use the new branch and labeled commits, and keep vendored code and unrelated warnings outside cleanup scope.

## E. Technical Strategy and Ownership

### E1. Reuse the existing command/domain/UI boundary

For dff, imgdiff, and csv, add a small typed domain plus normal plugin, command handler, and GUI state. Proposed naming families are **duplicate_finder**, **image_compare**, and **csv_utility**. These are proposed additions, not existing source paths.

Wire each through the currently inspected equivalents of:

- src/lib.rs and src/plugins/mod.rs;
- src/plugin.rs built-in registration/settings;
- src/commands/model.rs, parser.rs, host.rs, bus.rs, and handlers;
- src/gui/command_host.rs;
- src/gui/mod.rs Panel/state/focus/open/close/render ownership;
- the existing dialog navigation/Jump registration and Help/command catalog.

Do not assume every utility needs a public trait of its own. Match the established typed host pattern where a dialog-only command fits. GUI-only calls from a headless context must report that requirement without side effects. Search/catalog generation must never launch a scan or parse clipboard content.

Keep algorithms in their domains. GUI state owns user selections and projections; workers own immutable request inputs. Use generation/request identities for completed results and progress. Avoid retaining an egui::Response or raw widget rectangle across unrelated render passes.

### E2. Persistence and repair boundaries

Settings discovery is metadata, not a parallel settings store. Per-request delays, comparisons, table data, finder results, picker queries, and pin opacity are transient unless an existing owner already persists the relevant state.

For repair, build a typed change set over the selected persisted record and exact active references. Use original-record/version guards. Inspect the actual consumers of PersistableActionTargetRef, HistoryPin, FavEntry, usage, aliases, any per-action visibility preferences actually found on the execution base, and equivalent active bindings before promising identity preservation. The existing src/visibility.rs owns native window lifecycle; its name does not make it a repair preference store. Inspect only those owner families, not the whole application.

Path-keyed data shared by distinct records is potentially ambiguous: never transfer or erase a shared preference/usage key merely because two strings match. Preserve unrelated data and reject a truly ambiguous repair before committing.

The inspected actions::update_actions increments its version after its individual save; folders::update_folders publishes LIVE_FOLDERS after its save; LauncherApp::update_custom_actions publishes its catalog after owner mutation. Simply chaining these immediately publishing helpers is not a coordinated repair transaction. Provide a narrow deferred-publication path through touched owners, or an equivalent justified coordination mechanism, including the related watch-driven reloads. Revalidate original records/versions immediately before relevant writes, publish once after commit or successful restoration, and distinguish a rolled-back failure from an incomplete recovery conflict.

Existing recovery has store/group staging, but its inspected groups are MkMacro and Radial, not a generic live multi-store transaction. Reuse atomic-write/backup primitives where appropriate; do not invoke a broad application reset/restore to implement repair. If multiple owned stores require updates, use a narrow preflight, ordered write, preserved-backup, rollback/recovery operation. Any durable repair receipt must be limited to this operation, versioned, and contain only required recovery data. Test a later-write failure and interrupted recovery if such a receipt is introduced. Keep success publication after the committed outcome; expose recovery conflicts rather than overwrite later user edits.

### E3. Resource and lifecycle discipline

Use the existing cancellation, thread-reaper, bounded-result, and UI repaint patterns where appropriate. Hash Generator's streaming primitive is useful, but its per-dialog HashJob is not the duplicate finder's coordinator. Diff folder traversal demonstrates useful reparse checks but is not a multi-root duplicate database.

Do not mechanically reuse blocking bounded-channel sends that can strand a cancelled worker when its receiver stops draining. Use a cancellation-aware delivery/coalescing strategy, release receivers on shutdown, and make terminal-result delivery safe when the UI has closed.

For hard links, prefer handle identity using GetFileInformationByHandleEx/FileIdInfo on supported Windows filesystems and handle unsupported/invalid identities honestly. Do not deduplicate unrelated files because a filesystem returns an unusable all-zero identifier. Recheck the path-to-handle relationship during verification; this is a point-in-time scan, not a permanent filesystem lock.

### E4. Dependencies

Reuse the current image, sha2, walkdir, serde_json, rfd, arboard, and egui/egui_extras capabilities where suitable. A maintained CSV parser/writer is the only anticipated additional dependency, justified by quoted multiline parsing and correct output escaping. Confirm the actual current dependency graph and API before adding it; no upgrades or dependency cleanup unrelated to this goal.

Do not modify vendor/egui to add note undo or pin opacity. Existing public editor state and native-window boundaries must be used first.

## F. Detailed Milestones and Checkpoints

Execute in the sequence below. The order intentionally makes small improvements available before the two larger utilities. M0-A in Section A0 is the first checkpoint; it must already be complete before M0-B.

Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.

Every listed checkpoint is a coherent commit boundary. Do not combine several completed feature areas into one final commit. All checkpoint subjects below are suggestions with mandatory stage labels, not a requirement to use an inappropriate Git scope.

### M0-B — Reconcile the branch and establish bounded verification

**Objective:** Confirm the concrete integration points on the newly created branch and prepare only the verification infrastructure this goal needs.

**Code ownership:** This Goal's execution record; relevant AGENTS/agent configuration; existing nextest configuration if any; the existing fixture/harness entry points.

**Implementation tasks:**

1. Compare the actual base with Section B only for affected files. Record renamed/missing seams and adapt ownership without merging unrelated branches.
2. Identify current plugin registration, parser/host dispatch, Panel/focus lifecycle, persistence owners, and the existing notes/pin/screenshot verification seams.
3. Discover actual test names/targets with cargo nextest list after M0-A, scoped to the checkpoint's relevant --lib or --test target. Do not compile/enumerate all test targets as a preflight. The inspected Cargo.toml groups plugin cases and domain cases into suite targets; do not assume every nested test file is its own Cargo target.
4. Configure the opt-in bounded test profile in Section H, preserving existing profiles. Verify the installed nextest version before using version-dependent settings.
5. Select a small baseline subset of directly affected existing behavior. Record known failures once; do not start a full-suite repair.
6. Plan the owned temporary profile and fixture assets for UI evidence. Reuse suitable existing helpers; add only the scenario support needed as features arrive.

**Behavior:** The current application remains unchanged; the execution environment has real deadlines and known commands.

**Edge cases:** Dirty base, untracked user files, unavailable remote, older nextest, no interactive Windows desktop, and a base with fewer Jump helpers.

**Regression constraints:** Preserve existing test profiles, source files, user data, Git index, and unrelated harness scenarios.

**Verification:** Inspect the selected baseline outcomes and timeout configuration; build only the necessary target when required for enumeration or the selected checks.

**Acceptance:** The ledger identifies actual source locations, selected tests, existing failures, deadlines, UI capability, and a workable path for the first feature. A native-environment limitation is recorded, not represented as a passing UI check.

**Commit:** chore(dev): [M0-B] record source reconciliation and bounded verification setup

### M1 — Search all settings

#### M1-A — Catalog, matching, and search entry points

**Objective:** Introduce complete searchable metadata for application-owned settings without changing their values or save owners.

**Code ownership:** src/settings_editor/{state,render,mapping}.rs; src/plugins/settings.rs; relevant command types/handler; a proposed settings-search module beside the editor; settings/plugin tests.

**Implementation tasks:**

1. Inventory settings controls across the main editor, built-in plugins, Theme, Mouse, Gesture, and PluginEditor. Record stable keys, label, breadcrumb, synonyms, typed owner/destination, and conditional availability. Keep this inventory in the Goal's execution notes or a concise test fixture, not a new planning document.
2. Build metadata without rendering all controls or taking plugin write locks on each keystroke. Match using the existing fuzzy-matching convention or a small deterministic matcher.
3. Add the Settings search field/result list and route `settings <query>` with the full query preserved. Empty query returns to normal Settings content.
4. Keep result ordering deterministic and selection stable across filtering. Distinguish no matches from unavailable destinations.
5. Add exact matching/routing tests, including a disabled built-in and the four example queries.

**Behavior:** Search exposes human-readable setting results; opening or filtering search has no persistence or runtime side effects.

**Edge cases:** Whitespace query, Unicode, duplicate labels in different owners, hidden conditional controls, disabled plugins, and busy plugin access.

**Regression constraints:** Existing settings/dashboard/theme commands and settings file formats remain compatible; search never indexes current values or enables a plugin.

**Verification:** Focused catalog/matcher, settings_plugin, and relevant parser/handler tests; inspect that search does not call save/apply.

**Acceptance:** font, gesture interval, paste, and always on top find the correct owned controls; metadata coverage extends beyond those examples; old settings commands still behave normally.

**Commit:** feat(settings): [M1-A] add searchable settings catalog and entry points

#### M1-B — Exact-control reveal and draft-safe navigation

**Objective:** Deliver every indexed built-in destination through its actual editor with correct focus, scroll, and save behavior.

**Code ownership:** SettingsEditor reveal/render paths; existing Theme, Mouse, Gesture and PluginEditor owners; GUI command host/focus; settings navigation/Jump tests.

**Implementation tasks:**

1. Generalize the reveal_paste_sequence precedent into a typed pending destination. Open/reuse its owner and expand ancestor sections.
2. Fulfill navigation after the target's actual control renders. Scroll/highlight and request appropriate focus without clicking or toggling it.
3. Preserve dirty drafts and each owner's existing commit/live-update semantics. Reopening Mouse settings must not erase a draft.
4. Handle conditional/disabled controls truthfully. Unknown external DLLs get labeled section-level results only.
5. Make navigation independent of Experimental Jump Mode while sharing stable control identities where suitable. Avoid stale rectangles or focus requests after close.
6. Exercise representative destinations across different owners and verify the completed coverage inventory.

**Behavior:** Selecting a result visibly reveals the intended control, and the user can edit it through its ordinary UI.

**Edge cases:** Repeated navigation, already-open dirty editor, busy plugin, target hidden by a prerequisite, dialog closed before reveal, and large font/window constraints.

**Regression constraints:** Navigation alone never changes settings; Cancel/Save/Apply and existing native footer/key behavior retain their semantics.

**Verification:** Extend relevant Settings render, mouse draft, and plugin-busy tests. Run the bounded visual settings scenario with Jump enabled and disabled.

**Acceptance:** Exact-control destinations work across built-in owners; unsaved changes survive navigation; no value is toggled by choosing a result; unsupported external detail is not misrepresented.

**Commit:** feat(settings): [M1-B] reveal real controls while preserving settings drafts

### M2 — Repair broken saved targets, including folders

#### M2-A — Typed repair, original-record guards, and persistence

**Objective:** Make a confirmed target replacement preserve its selected record and exact active associations safely.

**Code ownership:** src/actions/mod.rs; src/plugins/folders.rs; src/usage.rs and actual preference owners; src/universal_actions/{target,persisted_resolver}.rs; referenced favorite/pin/binding stores; narrow proposed repair coordinator.

**Implementation tasks:**

1. Define eligibility and inspection outcomes for saved filesystem targets. Separate Missing, Available, Unknown/Unavailable, and NotFilesystem.
2. Capture the original record, owner and applicable version/identity. Resolve it again under the owner's mutation path; reject deletion, ambiguous duplicates or conflicting edits.
3. Validate the replacement's existence/kind off latency-sensitive search paths. Do not execute it.
4. Build an exact typed repair change set, preserving original label/desc/args/alias/order and identifying required usage, pins/favorites, PersistableActionTargetRef and any actual per-item preference updates.
5. Guard destination collisions and shared path-key ambiguities before writing. Preserve unrelated records even when they share a label or path.
6. Use owner-coordinated writes and the Section E2 recovery strategy. Do not just chain helpers that immediately publish each individual save: defer the touched owners/watchers until the coordinated outcome is known. Revalidate record/version guards before writes, retain recoverable old data, and report any incomplete restoration as a recovery conflict. No arbitrary JSON-wide substitution or UUID migration.

**Behavior:** A repair replaces the intended saved filesystem target and maintains its active associations. Cancel/preflight rejection leaves it unchanged. A write failure either restores the original state or exposes a concrete recovery conflict; it is never reported as success or falsely described as unchanged.

**Edge cases:** Same path, case-only spelling change, file versus directory, unavailable network path, reordered actions, two actions with different args, conflicting destination, and failure on a later store write.

**Regression constraints:** Atomic-save behavior, malformed-store protection, action versioning, folder live publication, and historical execution records remain intact.

**Verification:** Temporary-record/file tests for successful app/file/folder replacement; stale owner/index rejection; exact-reference resolution; shared-key collision; failed-save/runtime publication; and rollback/recovery where used.

**Acceptance:** Confirmed repair preserves metadata and resolvable active references without touching a different record. Conflicts are detected before success and failed writes cannot leave a false successful UI state.

**Commit:** feat(actions): [M2-A] add guarded saved-target repair transactions

#### M2-B — Repair UI and maintenance integration

**Objective:** Expose repair through existing action surfaces, including real folder selection.

**Code ownership:** src/gui/add_action_dialog.rs; src/actions_editor.rs; src/plugins/missing.rs; src/universal_actions/{providers,model}.rs and the current GUI intent executor; folder item actions; related Help.

**Implementation tasks:**

1. Add Repair target… to eligible custom and folder actions through the existing provider/intent/executor route.
2. Reuse the action editor's draft/picker patterns, but commit by original-record identity rather than DialogMode::Edit's stale index.
3. Offer the proper file/folder picker and an old/new path preview. Keep the replacement draft and actionable error after failed validation.
4. Extend relevant missing-entry discovery to saved custom filesystem actions. Bound/cache checks appropriately; do not make ordinary launcher queries probe every path synchronously.
5. Keep explicit Remove available with its existing behavior. Do not automatically repair after a failed launch.
6. Refresh committed action/folder snapshots through their existing publication owners, preserving indexed actions.

**Behavior:** A missing saved executable/file/folder can be located and repaired without re-entering its metadata or launching the replacement.

**Edge cases:** Cancelled native picker, Unicode/space-containing paths, missing checks on offline targets, already repaired entry, and indexed-only search result.

**Regression constraints:** Existing edit/remove behavior, folder aliases, custom arguments, search publication, and launcher focus remain correct.

**Verification:** Existing actions/folders/missing tests plus the new UI/intent tests. One owned file-picker and one folder-picker native smoke; do not launch arbitrary external programs.

**Acceptance:** Both file and folder repair work end to end, preserve associations, and expose clear conflicts. Picker Cancel changes nothing.

**Commit:** feat(actions): [M2-B] expose file and folder repair in existing actions

### M3-A — Clipboard Modify line cleanup end to end

**Objective:** Deliver all five line operations through the existing command, preview, apply, and saved-pipeline flow.

**Code ownership:** src/clipboard_modify/{model,catalog,parser,executor,help,config}.rs; src/gui/clipboard_modify_dialog.rs; relevant plugin_cases/domain_cases.

**Implementation tasks:**

1. Add typed operation IDs/catalog entries and the optional separator argument; update argument validation and any_supplied consistently.
2. Implement Section C3 semantics using the existing cancellation/executor boundary. Preserve existing newline handling and literal quote/pipe rules.
3. Add generated help and real parser-valid examples, including quoted separator and empty join separator.
4. Expose operations and separator input in both individual stages and saved pipelines.
5. Preserve previous schemas and named-pipeline invocation. Test a pre-existing user name that equals a newly added operation name and prevent it from being discarded/unreachable.
6. Keep existing preview generation and guarded Apply/Undo; do not create a second clipboard-write path.

**Behavior:** Commands, GUI and saved pipelines produce the same output.

**Edge cases:** Empty input; CRLF; whitespace-only/interior blank lines; Unicode; separator containing quotes or pipe; empty join; invalid empty split; new name collision.

**Regression constraints:** Old operations, old configuration and saved pipelines keep loading and executing; failed/partial parsing never changes the clipboard.

**Verification:** Focused parser/help/config/executor tests and one composed pipeline round trip; one preview→Apply→Undo UI smoke using a fixture clipboard value.

**Acceptance:** All five operations work through the shared executor, invalid arguments are inert, and old configuration survives unchanged.

**Commit:** feat(clipboard): [M3-A] add line cleanup operations across commands and pipelines

### M4-A — Searchable Paste Sequence source pickers

**Objective:** Replace opaque menus with searchable, inspectable source selection while preserving source/reference/privacy semantics.

**Code ownership:** src/gui/paste_sequence_dialog.rs; Step constructors/preflight in src/paste_sequence.rs only if a narrow seam is needed; snippet/history reading projections; existing picker/preview/ordered-editor tests.

**Implementation tasks:**

1. Add retained picker state for kind/query/selection/preview/session, with stable full-source identities.
2. Create separate safe projections for normal snippets, hidden snippets and clipboard entries. Never feed hidden bodies into filtering.
3. Integrate scrollable results, selected read-only preview, Add/Cancel and appropriate empty/error states.
4. Add exactly one Step through existing constructors; reveal its draft row. Preserve alias and SHA-256 reference serialization.
5. Own popup focus, Enter/Escape and Jump targets; prevent event fallthrough to Run or launcher activation.
6. Invalidate stale picker state on source/catalog/session/playback changes without losing sequence edits.

**Behavior:** Users can find the desired source and inspect allowed content before inserting it.

**Edge cases:** Duplicate clipboard text, long Unicode entries, concealed body matching the query, expired/reloaded sources, no matches, popup Escape, and changed parent session.

**Regression constraints:** No clipboard write or snippet prompting during browsing; no playback/keep-visible behavior changes; no stored clipboard body added to sequence JSON.

**Verification:** Filtering/identity/privacy tests; Add-once/cancel tests; existing hidden preview and ordered step lifecycle tests; one keyboard/Jump picker smoke.

**Acceptance:** The filtered selected source is the one added, hidden content cannot affect body search, and serialized source references retain their previous shape.

**Commit:** feat(paste): [M4-A] add searchable source pickers with safe previews

### M5-A — Numbered screenshot markers

**Objective:** Add numbered badges using the existing retained annotation model and output pipeline.

**Code ownership:** src/gui/screenshot_editor.rs; existing src/annotation/raster.rs helpers only when a genuine small shared helper is needed; editor tests.

**Implementation tasks:**

1. Add a proposed numbered-marker MarkupTool/MarkupLayer variant carrying source-space coordinates, number, color and size.
2. Derive the next label from retained marker history and preserve original labels during redo.
3. Implement matching egui preview and raster output using the current circle/font/text helpers, with actual multi-digit text metrics.
4. Add the toolbar tool and one-click placement; avoid duplicate insertion across frames or drag events.
5. Preserve raster-before-crop behavior and common apply_edits output for Copy/Save.
6. Include the control in existing constrained-toolbar reachability checks where useful.

**Behavior:** Clicks place 1, 2, 3…; undo/redo and saved/copied output agree.

**Edge cases:** Undo then new branch, 9→10, clipped image edge, crop through badge, zoom change, tool switch, and new editor image.

**Regression constraints:** Existing markup, text editing, secondary crop and destination settings remain unchanged; no new history engine.

**Verification:** Meaningful numbering/history and raster-placement tests, plus a fixture image showing single/multi-digit badges in preview and output.

**Acceptance:** Labels remain consistent after history changes; multi-digit markers are legible; preview and export use the same retained data.

**Commit:** feat(screenshot): [M5-A] add numbered markers to preview and export

### M6 — Delayed screenshot capture

#### M6-A — Typed delay and pending request lifecycle

**Objective:** Model a delayed capture separately from completed/cancelled capture.

**Code ownership:** screenshot plugin; commands model/parser/host/handler and headless screenshot dispatch; GUI screenshot request owner; a proposed narrow pending-request state and its tests.

**Implementation tasks:**

1. Add additive delayed action encoding for the 3/5/10-second choices; preserve every existing screenshot action. Parse invalid or unsupported delay input without executing.
2. Define request identity, monotonic deadline, mode/destination/markup, cancellation and completion state. Capture selection must happen at the deadline, not when queued.
3. Poll/schedule without blocking the GUI and allow only one request at a time. Keep this path and its deadline repaint active while the root is hidden/parked; hiding the main launcher is not cancellation.
4. Route terminal completion through existing command/history/outcome policy exactly once. Keep “scheduled” distinct from Completed.
5. Add cancel, stale-generation rejection and shutdown/countdown-close cleanup. Retain ownership until active work is safely cancelled or drained. Give new delayed/cancel commands explicit GUI-required headless handling before the current immediate/wildcard or external fallback; test that neither immediate capture nor external execution occurs.

**Behavior:** No capture happens before expiry; one happens after it; cancelled requests never fire later.

**Edge cases:** Zero-delay compatibility for old commands, unsupported number, pending request reentry, cancellation at the deadline, capture failure, and stale completion.

**Regression constraints:** Immediate captures and headless compatibility retain their meanings; no unrelated worker or global shortcut changes.

**Verification:** Injected clock/capture tests with no real multi-second sleeps, including expiry/cancel while root remains hidden; current screenshot parser/handler/cancellation tests; a focused new delayed-action headless rejection test.

**Acceptance:** The lifecycle is deterministic, no false history success is emitted, and the old immediate path remains valid.

**Commit:** feat(screenshot): [M6-A] add cancellable delayed capture requests

#### M6-B — Countdown UI, target timing, and capture integration

**Objective:** Make delayed capture usable while interacting with the intended target application.

**Code ownership:** GUI command host/screenshot UI, existing launcher parking/visibility helpers as needed, screenshot plugin catalog, focused native fixture scenario.

**Implementation tasks:**

1. Expose delayed choices under ss; display a screenshot-owned non-activating countdown and reachable Cancel. Add ss cancel through normal command routing.
2. Ensure countdown UI cannot become the window target. Preserve user foreground activity during the delay.
3. At expiry hide the countdown and acknowledge required visibility before reading pixels; then resolve Window foreground or start Region selection.
4. Preserve existing capture destination/editor/save settings and restore owned state on cancel/error.
5. Reuse appropriate parking primitives without replacing PinboardCaptureLifecycle. Reject competing capture ownership explicitly where existing gates require it.
6. Produce native evidence of a menu/tooltip prepared during a countdown and of cancellation with no output.

**Behavior:** The user has time to prepare a temporary UI state; the countdown itself is absent from the capture.

**Edge cases:** User changes target window, closes target, cancels via command, region selector times out, active pin/Screen Draw capture, and launcher reentry during countdown.

**Regression constraints:** Region timeout begins after countdown; current desktop monitor scope remains unchanged; pin selection suppression and Screen Draw capture paths are not rewritten.

**Verification:** One successful delayed-window scenario and one cancel/region scenario, plus directly affected handler/focus tests. Inspect file/clipboard/history effects, not only the countdown screenshot.

**Acceptance:** Real capture observes the intended deadline state, cancellation is accessible, and lifecycle cleanup leaves no hidden input owner or stale timer.

**Commit:** feat(screenshot): [M6-B] integrate countdown controls and target-safe capture

### M7 — Image comparison utility

#### M7-A — Bounded image loading and exact comparison

**Objective:** Build a read-only two-image domain with deterministic difference semantics and bounded resources.

**Code ownership:** Proposed src/image_compare.rs or src/image_compare/; existing image/clipboard conversion primitives; focused domain tests.

**Implementation tasks:**

1. Define immutable input identity/revision, decoded dimensions/pixels, and a comparison result/mask independent of egui.
2. Load file/clipboard images using existing enabled codecs, checking decode/canvas limits and allocation arithmetic first.
3. Compare exact RGBA channels in source coordinates; distinguish equal overlap, changed overlap, A-only and B-only regions.
4. Support cancellation and stale-result rejection. Retain the last valid pair on failed replacement input.
5. Account for decoded, mask/result, texture and upload retention within C7's budget. Avoid cloning a full image on every frame.

**Behavior:** Equal decoded images compare equal regardless of compressed file bytes; differing dimensions remain visible without implicit scaling.

**Edge cases:** Alpha-only difference, invisible RGB difference under zero alpha, corrupt file, enormous decoded dimensions, zero/invalid dimensions, unequal extents, unsupported codec, and input changed mid-job.

**Regression constraints:** Existing Diff and screenshot image types/paths remain unchanged; no new image crate version.

**Verification:** Small deterministic fixtures for equality, one-pixel/channel differences, dimension mismatch and rejected/stale work; no broad image benchmark.

**Acceptance:** Correct masks/regions, stable last-good inputs, enforced limits, and no source writes.

**Commit:** feat(imgdiff): [M7-A] add bounded image comparison domain

#### M7-B — imgdiff plugin and comparison dialog

**Objective:** Make the domain accessible as a normal launcher utility with useful interactive comparison.

**Code ownership:** Proposed image_compare dialog/plugin/handler; shared registration/routing/Panel lifecycle from E1; appropriate Jump/Help integration.

**Implementation tasks:**

1. Add the imgdiff prefix, explicit Open action and Help/catalog entries; protect token boundaries and lookalikes.
2. Add A/B file and Paste Image inputs, dimensions/source labels, view mode, shared zoom/pan, wipe and changed-area display.
3. Map rendering to original coordinates; show mismatch/nonoverlap on a clear background. Wipe endpoints display exactly the selected source.
4. Keep controls responsive, use generation-bound results, and release workers/textures on close.
5. Integrate focus, keyboard navigation, Escape and Jump where the current base supports dialog targets, without changing the experimental feature's activation.
6. Capture focused wipe and difference evidence from known images.

**Behavior:** The user can inspect image changes without opening an editor or modifying files.

**Edge cases:** Clipboard has text/no image, replacing only one input, reentering the existing dialog, close/reopen during load, extreme zoom, and constrained window size.

**Regression constraints:** diff image remains a valid ordinary Diff path argument; existing saved Diff modes and watches are untouched. Only explicit Paste reads clipboard.

**Verification:** Plugin/parser/handler tests, UI lifecycle/geometry checks, and one fixture pair with wipe plus highlighted differences.

**Acceptance:** The complete imgdiff route works, mode changes use the same data, input errors are recoverable, and all resources are released on close.

**Commit:** feat(imgdiff): [M7-B] expose image comparison through launcher and dialog

### M8 — Adjustable static and live pin opacity

#### M8-A — Owned opacity state and real translucency

**Objective:** Apply a bounded per-pin display opacity through the existing validated native-child boundary.

**Code ownership:** src/pinboard.rs; src/gui/pinboard_windows.rs state/control/native binding; focused native-api tests.

**Implementation tasks:**

1. Add validated session opacity with default 100% to Pin, runtime/window state and control changes; propagate it through the existing synchronization owner.
2. Verify which actual translucency path is supported by the current eframe/Glow/native pin window. Start at the existing native API boundary; if that window style cannot support native uniform alpha, evaluate the existing transparent-child composition capability.
3. Choose one working path after a small native feasibility check. Do not fake success with image tint, modify root-window opacity, or undertake a renderer replacement.
4. Preserve existing native style flags, click-through and topmost behavior. Revalidate child identity/lifetime before application.
5. Commit state only after application succeeds, and keep opacity independent from image/frame data and Copy/Save work.

**Behavior:** The controlled application behind a 50% pin is visible through the pin.

**Edge cases:** Stale/destroyed/foreign/root handle, embedded viewports, unsupported composition, native failure, out-of-range value, and live texture replacement.

**Regression constraints:** Source pixel ownership, frame budgets, valid child controls, root appearance and capture hide/restore remain correct.

**Verification:** Fake-native owner/failure tests plus one real composited static-pin probe. If both bounded existing paths fail, record a genuine native blocker rather than start a vendor rewrite.

**Acceptance:** Actual translucency works for the supported Windows pin path; invalid owners are untouched; source/export pixels remain identical.

**Commit:** feat(pinboard): [M8-A] apply per-pin opacity through validated window ownership

#### M8-B — Controls, recovery, and live-pin evidence

**Objective:** Expose usable opacity controls on both pin surfaces and validate live behavior.

**Code ownership:** PinControlChange, PinListEntry/Action, pin/list rendering, presenter polling; existing live-pin fixture.

**Implementation tasks:**

1. Add 10–100% controls and 100% reset to pin and Pin List. Retain existing focus/first-pass guards.
2. Keep two pins independent. Make list recovery/reset usable when a pin is click-through or difficult to see.
3. Retain opacity across temporary suppression/hide/restore and live-frame updates; a new independent pin starts at 100%.
4. Verify Copy/Save from a translucent static and live pin still exports the original image.
5. Capture desktop-composited evidence over a controlled contrasting window at 100% and 50%.

**Behavior:** Opacity can be adjusted and recovered immediately without changing captured content.

**Edge cases:** Closed/stale list row, live source ends, controls changed while child is hidden, click-through toggles, and reset at minimum opacity.

**Regression constraints:** Live texture reuse, native child identity, zoom, list keyboard control and close cleanup remain intact.

**Verification:** Targeted pin-control/list/live synchronization tests; one controlled static/live native scenario including export pixel comparison. A root-framebuffer PNG alone cannot prove desktop translucency.

**Acceptance:** Both pin kinds are translucent, independent, recoverable, and export unmodified pixels.

**Commit:** feat(pinboard): [M8-B] add opacity controls and verify static and live pins

### M9 — Replace in the current note

#### M9-A — Revision-safe literal replacement

**Objective:** Compute correct single/all replacement transactions over the current Markdown source.

**Code ownership:** src/gui/note_find.rs and a small adjacent pure helper if needed; NoteFindMatch ranges; focused note-find tests.

**Implementation tasks:**

1. Use the existing literal Unicode case-insensitive match set and byte ranges; carry content/query revision with the operation.
2. Compute a single replacement and its post-insert next-match/cursor position.
3. Compute Replace All from original nonoverlapping matches in one pass or safe reverse-range application. Never rescan inserted text during the same operation.
4. Treat replacement text literally, preserve surrounding bytes/newlines, and identify exact no-content-change results.
5. Return enough character-offset/cursor data for the existing editor without conflating UTF-8 bytes with egui character indices.

**Behavior:** Replacing content is predictable, finite and confined to the selected note.

**Edge cases:** Empty search/replacement, dollar/backslash text, replacement containing the search, Unicode folds with different byte lengths, CRLF, stale revision, and identical actual output.

**Regression constraints:** Existing Find match semantics and performance cache are preserved; no regex mode or disk write occurs in the computation.

**Verification:** Focused behavioral tests for single/all/no-op, Unicode, preserved bytes and stale input; use existing note-find fixtures.

**Acceptance:** Replacements affect exactly the original intended ranges, handle character indices correctly, and cannot loop over newly inserted content.

**Commit:** feat(notes): [M9-A] add revision-safe current-note replacement

#### M9-B — Find-strip UI, save policy and atomic undo

**Objective:** Connect replacement to the live note panel without changing save/discard or input ownership.

**Code ownership:** src/gui/note_panel.rs; note_find_preview.rs; the existing public egui TextEditState undo boundary; relevant note panel tests.

**Implementation tasks:**

1. Extend the existing Find strip with Replace controls, availability, match count and current-note/source wording.
2. Apply changes through replace_content_from_mutation/mark_content_changed or their current equivalents. Do not call the immediate disk-save mutate_note_by_slug path.
3. Add a single replacement undo unit through the existing public text-editor state. Verify the available undoer APIs on the execution branch rather than editing vendor code.
4. Preserve the editor's subsequent undo/redo and typing history, including when replacement starts in Preview or crosses Edit/Split state.
5. Invalidate Find/Preview caches once, update selection, and preserve the other open note panels.
6. Scope Enter/Escape and replace focus to the note strip. Keep launcher query editing and existing Find navigation usable.

**Behavior:** Replace/Replace All respects the user's current note save mode and can be undone in one step.

**Edge cases:** Unsaved note, manual-save mode, Preview source-only match, two panels, close with unsaved changes, undo then normal typing, and no-op replacement.

**Regression constraints:** Slug/title/path/metadata, auto-save timing, discard prompts, existing Find focus and other notes are unchanged.

**Verification:** Existing note-find/panel-save tests plus one-step undo/redo and two-panel isolation; one owned temporary-note UI scenario with Jump on/off representative focus.

**Acceptance:** A live unsaved note can be replaced, undone and saved/discarded normally. No forced save or query-focus regression occurs.

**Commit:** feat(notes): [M9-B] integrate replace controls with note undo and save lifecycle

### M10 — CSV/TSV table utility

#### M10-A — Parser, view projection and non-lossy output

**Objective:** Create a bounded table domain supporting correct CSV/TSV and explicit exports.

**Code ownership:** Proposed src/csv_utility.rs or src/csv_utility/; Cargo.toml/Cargo.lock only for a justified parser/writer dependency; focused domain tests.

**Implementation tasks:**

1. Add a maintained compatible CSV reader/writer only if the current graph lacks an appropriate one; record why the dependency is needed.
2. Parse UTF-8/BOM with explicit delimiter/header mode, correct quoted multiline semantics, uniform widths and incremental resource checks.
3. Keep source records immutable strings. Produce stable filtered/sorted row indexes and selected-column indexes; do not reorder source storage.
4. Generate quoted CSV/TSV for the current view and JSON objects/arrays according to C10, including duplicate/empty-header validation.
5. Enforce output limits before writing clipboard; avoid a transient unbounded full output on a preview path.
6. Add revision/generation-aware worker requests for larger computations and preserve the previous valid result after invalid input, including its successful parse options and explicit previous-valid-data status.

**Behavior:** Inspection and output preserve actual values and the selected view without implicit coercion.

**Edge cases:** Quotes, delimiter inside a field, embedded newline, CRLF/BOM, whitespace/empty strings, leading zeros, duplicate headers, ragged rows, invalid encoding and limit exceedance.

**Regression constraints:** Existing JSON tools and clipboard infrastructure remain unchanged. No source overwrite or formula evaluation.

**Verification:** Meaningful parse/write round trips; exact JSON and selected-column output; stable ties/filter/order; malformed input/limits and stale result.

**Acceptance:** All required data shapes are handled without dropped cells, duplicate-key overwrites or type loss; invalid work leaves the last valid table available.

**Commit:** feat(csv): [M10-A] add bounded table parsing and view exports

#### M10-B — csv plugin and responsive table dialog

**Objective:** Deliver the full utility through a normal prefixed launcher entry and a compact table UI.

**Code ownership:** Proposed csv plugin/dialog/handler; registration/routing/Panel/Help/Jump seams from E1; tests following JSON utility conventions.

**Implementation tasks:**

1. Register csv with normal enablement/capability behavior. Add a plain Open action and discoverable Help; do not consume general pasted table text as a launcher command.
2. Add Open/Paste, delimiter/header controls, filter, column sort/selection, table, view counts, Copy and JSON controls.
3. Virtualize rows and avoid rebuilding/parsing on every frame. Reflect busy/error/stale states without freezing other launcher features.
4. Reparse only when input/options change; maintain consistent selection semantics or explicitly reset an invalid selection.
5. Route Copy through the ordinary clipboard owner only on explicit request. Show invalid object-header feedback with array output available; disable Copy/JSON when no columns are selected. Label retained previous-valid data after a failed reparse and do not mismatch it with new parse options.
6. Complete focus/keyboard/close/reopen handling and cancel/reap resources.

**Behavior:** A small pasted/opened table can be inspected and reshaped in a few actions without editing its source.

**Edge cases:** Clipboard empty/nontext, 0 selected columns, filter yields 0 rows, source option changes during parse, wide columns, large table, and close while busy.

**Regression constraints:** Existing source files, clipboard history policies and other dialogs remain intact; no startup worker or source-file autosave.

**Verification:** Plugin/parser/handler/lifecycle tests; one CSV and one TSV fixture UI scenario; one bounded synthetic large table to check responsive virtualization. No full application benchmark.

**Acceptance:** Filter/sort/select/Copy/JSON operate on the displayed rows/columns, preserve strings, and remain responsive; source files are byte-identical.

**Commit:** feat(csv): [M10-B] expose CSV and TSV inspection in the launcher

### M11 — Duplicate-file finder

#### M11-A — Discovery, root identity and scan lifecycle

**Objective:** Discover regular files under explicit roots safely and efficiently, without duplicate traversal or runaway workers.

**Code ownership:** Proposed src/duplicate_finder/ domain/controller; suitable file-search path and traversal helpers; Windows file identity adapter; existing thread-reaper primitives.

**Implementation tasks:**

1. Define scan request/generation, selected roots, candidate metadata, progress, diagnostics and completion/partial/cancel state.
2. Normalize/deduplicate overlapping roots without lossy string identity assumptions. Preserve display paths while keeping real identity separately.
3. Traverse regular files with reparse/symlink/device exclusions, file-count/result limits and recoverable per-entry errors.
4. Add reliable handle-based identity for Windows hard links; use supported equivalent identity on other platforms. Distinguish unsupported identity from a real shared ID. Route independence-unverified content matches to a separately labeled status excluded from verified copy groups/totals; do not substitute per-path uniqueness or collapse unknown/zero IDs.
5. Group by size and track distinct file identities. Do not count the same object reached through another root as duplicate storage.
6. Implement single-worker ownership, cancellation checkpoints, cancellation-aware bounded delivery, generation checks and nonblocking cleanup. Do not stack new workers behind stuck OS I/O.

**Behavior:** Scans run only after explicit Start, remain cancellable, and disclose skipped/partial work.

**Edge cases:** Overlapping/case-varied roots, hard links, junction loops, empty directory, inaccessible root, long/Unicode paths, removed directory, cloud-placeholder flags, unsupported IDs and entry limits.

**Regression constraints:** Existing File Search and folder Diff behavior are preserved; no automatic watcher or drive scan.

**Verification:** Owned temporary trees with overlapping roots, equal names/different files, hard links where supported, synthetic filesystem errors/reparse cases and cancellation/full-queue cleanup.

**Acceptance:** Each distinct eligible file is represented appropriately, limits/errors are explicit, cancellation cannot deadlock publication, and worker count remains bounded.

**Commit:** feat(dff): [M11-A] add bounded duplicate scan discovery and identity

#### M11-B — Streaming hashes and verified equality groups

**Objective:** Produce exact-content duplicate groups with valid point-in-time evidence.

**Code ownership:** Duplicate-finder engine; existing hash_request/streaming code via a narrow reader/handle seam if required; byte-comparison helper; focused domain tests.

**Implementation tasks:**

1. Skip hashing unique-size groups. Stream SHA-256 over candidate files using fixed-size buffers and cancellation checkpoints.
2. Validate identity/metadata using the same opened object where possible. If the current hashing API cannot expose that seam, extract the smallest reusable reader/handle primitive while preserving Hash Generator behavior.
3. Revalidate before/after reads and comparisons. Reject changed, disappeared or unreadable candidates instead of accepting a stale digest.
4. Partition each size/digest bucket by actual byte equivalence. Do not assume digest equality alone is final proof or discard all mismatches against one representative without considering valid remaining subgroups.
5. If a representative becomes invalid, choose/reverify another before publishing a group. Require at least two distinct files, excluding hard-link aliases.
6. Calculate group counts and logical duplicate bytes with checked/saturating arithmetic and clear semantics. Include zero-byte candidates without unnecessary I/O.
7. Retain no whole-file buffers and no unlimited per-chunk progress messages.

**Behavior:** Reported groups contain different files with verified equal bytes at scan time.

**Edge cases:** Forced digest collision in a test seam, same-size different contents, changing representative, hard links plus real copies, empty files, I/O error, cancellation during comparison and large lengths.

**Regression constraints:** Hash Generator algorithms/output/progress remain compatible if a shared helper changes. No source data is written or locked against ordinary use indefinitely.

**Verification:** Deterministic content fixtures, injected hash collision partitioning, injected mid-read changes/errors, bounded-reader behavior and cancellation; targeted existing hash tests if its seam changed.

**Acceptance:** Confirmed groups have byte-verification and distinct-file-identity evidence; identity-unverified matches are labeled separately, corrupt/stale candidates are excluded, and memory does not scale with file sizes.

**Commit:** feat(dff): [M11-B] verify content duplicates with streamed hashing and comparison

#### M11-C — dff plugin, grouped results and ordinary file actions

**Objective:** Deliver the finder as a usable launcher tool with transparent progress and no destructive operations.

**Code ownership:** Proposed duplicate finder plugin/dialog/handler; shared registration/routing/Panel/Help/Jump seams; existing open/reveal/copy action owners.

**Implementation tasks:**

1. Register exactly the dff prefix and ordinary plugin enablement/help behavior.
2. Add root selection, explicit Start/Cancel, stage/progress/status, recoverable diagnostics and virtualized grouped result rows.
3. Present filenames/full paths, sizes, group membership and logical duplicate totals; visibly identify cancelled/partial/limited scans.
4. Offer Open, Reveal and Copy Path(s) using existing action owners. Revalidate disappeared results on activation and report failure safely.
5. Handle close/reopen/cancel/restart with the same scan generation and cleanup owner. A reopening must not show another session's completion.
6. Do not add Delete/Recycle/Keep/New hard link or any source mutation.
7. Exercise a temporary fixture tree containing known duplicates, nonduplicates, overlaps and skipped items; verify its bytes/paths remain unchanged.

**Behavior:** The user can find and inspect duplicates under chosen folders without filesystem cleanup side effects.

**Edge cases:** No roots, no duplicates, only skipped files, cancelled scan with verified partial groups, busy cancelling worker, result removed after scan, many groups and stale session.

**Regression constraints:** No search-time/background scanning, no external filesystem writes, no capture of arbitrary keyboard input, no changed File Search behavior.

**Verification:** Plugin/parser/handler and dialog lifecycle tests; one bounded native fixture scan, cancel/restart and path action scenario with post-run source comparison.

**Acceptance:** The full dff path works, no false complete result is presented, keyboard controls are usable, and fixture source data is untouched.

**Commit:** feat(dff): [M11-C] expose duplicate finder results and file actions

### M12-A — Final focused review, integration checks and handoff

**Objective:** Finish the complete pack with accurate evidence and a clean task-scoped change set.

**Code ownership:** All task-changed first-party files, this Goal's execution record, affected Help/README sections, and task-owned verification evidence.

**Implementation tasks:**

1. Have an independent reviewer inspect baseline-to-HEAD plus directly relevant surrounding code for missing requirements, incorrect ownership, persistence/privacy regressions and lifecycle leaks.
2. Reconcile each C1–C11 acceptance row with actual implementation and evidence. Recheck settings catalog coverage if later features added owned settings.
3. Verify representative command reentry/focus/close behavior across the three new utilities and changed dialogs. Avoid a combinatorial all-features matrix.
4. Run one final selected suite for changed and directly affected behavior, within Section H's budget. Preserve group and isolation outcomes for any diagnosed intermittent failure.
5. Run the final first-party Clippy pass and relevant formatting checks. Correct new issues; exclude unrelated vendored cleanup.
6. Update user-facing command/help details and the execution record with actual commits, push results, evidence paths, remaining limitations and any blocked checks.
7. Review staged/unstaged differences, branch identity and task-only files. Commit remaining coherent fixes/docs and attempt the final permitted push.

**Behavior:** Every feature is accounted for as complete or explicitly blocked; nothing is silently deferred.

**Edge cases:** Unavailable interactive desktop, pre-existing test failure, local-only commits, blocked native opacity, and changed repository instructions.

**Regression constraints:** No new feature expansion, history rewriting, broad cleanup, full-suite requirement, or additional implementation branch.

**Verification:** Independent review, targeted final tests/lint, evidence inspection, git diff --check scoped to task changes and final Git state. Review baseline-to-HEAD committed changes separately from preserved pre-existing working-tree/index changes. Rerun only checks affected by a real remediation.

**Acceptance:** Definition of Done passes, or the final report precisely distinguishes implemented work from genuine blockers/unverified acceptance. All task changes are committed on the fresh branch; push status is truthful.

**Commit:** docs(productivity): [M12-A] finalize acceptance evidence and handoff
Use separate small fix commits for substantive reviewer findings; do not hide implementation fixes in a documentation-only subject.

## G. Active Checkpoint Commit Cadence and Orchestration

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

The boundaries are M0-A, M0-B, M1-A/B, M2-A/B, M3-A, M4-A, M5-A, M6-A/B, M7-A/B, M8-A/B, M9-A/B, M10-A/B, M11-A/B/C, and M12-A.

- M0-A is the separately recorded baseline on the newly created branch. Every later checkpoint remains on that branch.
- Commit coherent, scoped increments after their focused checks. Do not wait for all eleven features to accumulate into one commit.
- Preserve unrelated changes and the user's index. Inspect staged content before each commit; never use blanket git add -A when it would capture unrelated work.
- Attempt a normal push after each checkpoint when supported. A rejection retains the local commit and is recorded once for that attempt; no retry loop or security bypass.
- Do not squash, amend, force-push, rebase or rewrite checkpoints. Add small corrective commits with their stage labels.
- Keep one source writer at a time for shared repository state. Registration, commands, GUI state and settings are shared files even when feature domains differ.
- Planner/read-only investigator, implementer, reviewer and orchestrator roles may be used if actually configured. Read-only investigation and review can overlap; overlapping source writes cannot.
- The orchestrator gives each implementer one checkpoint packet with objective, ownership, current facts, required behavior, edge cases, non-goals, tests, acceptance and commit boundary.
- Continue automatically after each completed checkpoint. Do not ask the user whether to begin the next milestone.
- On a genuine blocked feature, finish independent authorized checkpoints where safe. Do not mark the whole pack done or silently drop the blocked feature.

## H. Verification and Testing Strategy

### H1. Verify behavior at the owner

Use pure tests for matching, parsing, replacement, comparison and grouping; state/lifecycle tests for cancellation, stale generations and persistence; native checks for actual window behavior. Do not add tests that merely mirror every local helper, serialize an enum without proving behavior, or assert that a button exists.

Do not require the entire test suite. This pack touches many areas, but that is not permission to adopt all historical full-regression procedures. Test changed behavior and its directly affected invariants.

Discover actual selectors after M0-A. Representative existing areas include:

| Feature | Existing useful test owners or cases |
|---|---|
| Settings | tests/settings_plugin.rs; tests/settings_editor.rs; settings render navigation/slider/draft tests; plugin busy settings test. |
| Repair | Action/folder persistence module tests; universal_actions::persisted_resolver; tests/missing_plugin.rs; tests/folders_plugin.rs; GUI publication/watch tests when affected. |
| Clipboard operations | tests/plugin_cases/clipboard_modify_{parser,help,plugin}.rs; tests/domain_cases/clipboard_modify_{config,runtime}.rs; executor/catalog tests. |
| Source pickers | paste_sequence_editor_preview_masks_hidden_text_and_distinguishes_context_from_target; ordered_steps_editor_events_preserve_rows_scroll_and_lifecycle; sequence preflight. |
| Markers | screenshot editor model/raster tests and existing constrained-toolbar behavior. |
| Delay | tests/screenshot_plugin.rs; tests/screenshot_cancel.rs; screenshot parser/handler/command-host tests. |
| Image comparison | New pure comparison and UI lifecycle tests; unchanged Diff path-grammar sentinel. |
| Opacity | pin_controls_use_exact_child_commands_and_reject_invalid_owners; relevant pin list/live presenter tests; validated native-call tests. |
| Note replace | note_find and note_find_* panel tests; save/discard and new undo transaction cases. |
| CSV/TSV | New table/parser/output tests and plugin/typed-handler tests following JSON utility seams. |
| DFF | New discovery/identity/hash/byte-verification/lifecycle tests; existing hash tests only if shared code changes. |

These are source-derived starting points, not claims that every exact selector exists on the execution base. Do not silently replace behavioral verification with a zero-match filter. Capture selected test counts and ensure the intended tests actually ran.

### H2. Deadlines, nextest profile and failure handling

Inspect the installed nextest version and any current profile first. Prefer an opt-in **productivity-tools** profile; preserve default/CI behavior. A suitable starting configuration is:

~~~toml
[profile.productivity-tools]
retries = 0
slow-timeout = { period = "15s", terminate-after = 4, grace-period = "5s" }
global-timeout = "10m"
~~~

The slow-timeout setting without terminate-after would only report slowness; it would not bound a hung test. This profile terminates a test after four 15-second periods. On Windows, nextest's timeout termination is immediate and does not use that grace-period setting. Global timeout requires nextest support (documented from 0.9.100); confirm the installed version instead of assuming it. See the [nextest timeout documentation][s-nextest].

If a profile setting is unsupported, use an equivalent owned process-tree deadline or supported narrower configuration. Do not silently drop the bound or upgrade tooling without a concrete need. Keep timeout results as failures; do not configure on-timeout=pass.

Operational budgets:

- **Individual normal tests:** 60 seconds by default. A genuinely native/process test can receive a narrowly scoped, documented 120-second ceiling.
- **Selected test batch:** at most 10 minutes of test execution. Cargo compilation is separately bounded; nextest's run timeout is not a build timeout.
- **Build/check:** one normal development build/check path, with an initial 30-minute process-tree deadline; reuse artifacts. Do not repeatedly build release or every feature combination.
- **Native scenario:** normally 90 seconds, up to 180 seconds for a justified multi-window capture scenario, with a 10-second owned cleanup allowance.
- **Native evidence batch:** at most 20 minutes after build, using only the requested scenarios.
- **Clippy:** one final first-party pass, with a build-aware 30-minute ceiling and a targeted rerun only after an actual fix.
- **Unexpected test investigation:** one reproduction and up to two focused remediation rounds for the same unexplained failure before reassessing. A further attempt needs a concrete new cause and remains bounded; no open-ended stress loop. This does not limit ordinary compiler feedback while implementing a checkpoint.

Illustrative selection pattern, with real selectors substituted from enumeration:

~~~text
cargo nextest list -p multi_launcher --lib -E '<verified checkpoint filter>'
cargo nextest run -p multi_launcher --lib --profile productivity-tools -E '<verified checkpoint filter>'
~~~

Replace --lib with the relevant explicit --test domain, --test plugin_queries, or other verified target for integration tests, and use the same target restriction on list and run. Do not enumerate all targets merely to obtain a narrow filter. Inspect exit codes. UTF-8 logs without ANSI color are preferred for SSH review.

If a test fails in a selected group and shared state/interference is plausible, run that exact binary/test once in isolation with the same bounds. Report both results. A passing isolated run does not erase the grouped failure or establish suite stability. Do not automatically rerun the entire suite.

### H3. Autonomous UI evidence

Codex owns fixture creation, launch, interaction, screenshots and shutdown. Do not ask the user to run commands, supply screenshots, or test between checkpoints.

Use the existing dev/jump_mode fixture/capture infrastructure where economical; extend only what is needed for these scenarios. Its old full catalog is reference material, not a mandatory gate. New scenario support can live beside existing dev tooling if cleaner, but do not build a second general GUI automation framework.

Requirements for each native run:

1. Use an isolated, task-owned profile and synthetic files/notes/clipboard contents. Resolve all configured storage/output paths inside it; do not reuse production notes/settings.
2. Build or reuse the intended current binary with the needed existing QA feature. Record branch/commit/binary identity and scenario inputs.
3. Retain handles/PIDs of the exact processes launched. Wait for an observable readiness condition, not arbitrary long sleeps.
4. Interact with actual controls and verify resulting state. A screenshot of a dialog alone is not evidence its operation succeeded.
5. Save a small representative screenshot or output artifact and an exit/status receipt. Reuse the existing logging/evidence format.
6. Request normal shutdown; after the bounded cleanup deadline, terminate only the owned process tree. Never kill every process by executable name.
7. Ensure no pending capture worker, pin window, modal input owner or fixture process remains.

Suggested compact visual scenarios:

| Scenario | Evidence that matters |
|---|---|
| Settings | Requested queries reveal actual main/dedicated controls; navigate with a dirty draft; Jump enabled/disabled representative check. |
| Repair | One missing file/application record and one folder record repaired via their correct pickers; preserved metadata/references; cancellation unchanged. |
| Clipboard/Paste | New line operation preview/apply/undo; filtered source Add; hidden-body query stays concealed; reference JSON remains body-free. |
| Markers | Single/multi-digit badges, undo/redo, matching copy/save/crop output. |
| Delay | Temporary menu/tooltip captured after delay; countdown absent; cancelled request has no output/success. |
| imgdiff | Wipe endpoints, one known changed region, alpha/dimension mismatch clearly represented. |
| Pins | Composited 100%/50% static and live pins over a controlled background; reset/click-through recovery; unmodified exported pixels. |
| Notes | Replace All on unsaved source, one-step undo, normal save/discard, other panel/query still usable. |
| csv | CSV and TSV, filter/sort/selected columns, exact copied/JSON output, unchanged source, responsive larger fixture. |
| dff | Known duplicate/nonduplicate tree, hard-link handling if supported, partial/cancel state, read-only results and owned cleanup. |

When no interactive Windows desktop is available, record **NOT_RUN_ENVIRONMENT** or **BLOCKED_NATIVE** for the affected checks. Continue source/domain work and other checks without asking for intermediate manual help. Do not fabricate screenshots, claim native success from pure tests, or mark required native acceptance complete. The final report must expose the remaining verification gap.

### H4. Final Rust checks and review

Run formatting checks relevant to changed Rust files and git diff --check. Perform the final first-party Clippy pass only after feature work is assembled; use the repository's established invocation, scoped to the package/targets/features changed. For example, cargo clippy -p multi_launcher --all-targets --no-deps is a starting point to validate against current tooling.

Do not lint-fix vendored crates or absorb pre-existing unrelated warnings. Fix new first-party warnings attributable to this work, or report a specific legitimate blocker. Do not weaken tests, add broad allow attributes, disable assertions or change legitimate acceptance criteria to make a check green.

Use one independent final review plus reviews at concrete high-impact boundaries when needed (repair persistence, delayed capture, native opacity). Do not manufacture repeated “qualification rounds.”

## I. Risk Management and Bounded Work

| Concrete risk | Required mitigation | Stop boundary |
|---|---|---|
| Settings search saves/discards another editor's draft | Metadata-only matching; navigation fulfilled by the existing owner; explicit dirty-draft/Cancel tests. | No new global settings state/store. |
| Repair retargets the wrong item or strands preferences | Original-record/version guard; typed exact references; preflight collisions; owner writes and recovery evidence. | No application-wide UUID/redirect migration or global string substitution. |
| Hidden snippet leaks through search | Alias/tag-only projection for concealed snippets; masked preview/accessibility/logging. | Do not redesign the general cs plugin matcher outside this picker. |
| Clipboard pipeline compatibility | Additive typed arguments; old schema/name tests; preserve guarded Apply/Undo. | No parser rewrite or config reset. |
| Timer captures wrong window or outlives cancellation | Non-activating countdown, deadline target selection, request generations and owned cleanup. | No global hook or pinboard-controller rewrite. |
| Opacity only darkens the pin | Verify actual composited desktop background, not only widget alpha. | Bounded native/compositor evaluation; no vendored renderer project. |
| Replace corrupts UTF-8 or forces a save | Existing match ranges, revision guard, panel mutation and public editor undo. | No regex feature or note storage redesign. |
| Table parsing loses data | Real parser/writer, strings, strict widths, explicit JSON shape, output/resource limits. | No spreadsheet editor or type-inference engine. |
| DFF mislabels hard links or changing files | Reliable identities, metadata checks, streaming byte confirmation, partial status. | Finder only; no cleanup/deduplication mutations. |
| Worker cancellation blocks a UI or leaves old results | Single ownership, generation checks, cancellation-aware delivery, bounded cleanup. | No extra workers to hide a stuck operation. |
| Historical tests/harness consume the project | Explicit selected scope, finite deadlines, one isolation diagnosis where useful. | No automatic full suite, broad stress or performance campaign. |
| Planning reference differs from default base | Reconcile affected seams after fresh-branch bootstrap. | No unrelated branch merge/cherry-pick to make notes match. |

Classify incidents as **feature blocker**, **pre-existing/unrelated**, or **optional/deferred** with evidence. Only feature blockers prevent that checkpoint's completion. Finish independent work while preserving truthful status.

Report a blocker with the affected requirement, evidence/error, attempted bounded remedies, current branch/commit, safe remaining work, and the smallest necessary next decision. Ordinary Rust implementation difficulty is not a reason to ask the user routine design questions.

## J. Definition of Done

The complete goal is done only when all of the following hold:

- [ ] M0-A created a unique active task branch, its own baseline commit, and a recorded best-effort initial upstream push result.
- [ ] The actual base and baseline SHA are recorded, pre-existing changes preserved, and later work remains on that branch.
- [ ] All eleven C1–C11 behaviors are implemented through the intended owners.
- [ ] Built-in settings coverage is complete and any opaque external-plugin section links are described honestly.
- [ ] Repair covers application/file/folder records and preserves the specified active associations with tested failure handling.
- [ ] Clipboard and Paste Sequence privacy/reference/playback invariants hold.
- [ ] Numbered markers, delayed capture, image comparison and real static/live translucency meet their UI/output criteria.
- [ ] Current-note replacement preserves save/discard and one-step undo.
- [ ] CSV/TSV handles quoting, selected view output and JSON without value loss.
- [ ] dff reports verified distinct-file duplicates, handles partial/cancel states, and never mutates scanned content.
- [ ] Targeted automated verification actually ran and passed; required native evidence exists or is explicitly marked incomplete.
- [ ] No known feature-related blocking defect remains; reviewer findings are resolved.
- [ ] First-party final lint/review is handled proportionately; no unrelated vendor cleanup or scope expansion.
- [ ] Every coherent checkpoint is committed, actual push results recorded, and the baseline-to-HEAD committed task diff contains only intentional task work. Final working-tree/index state separately identifies preserved pre-existing user changes; a globally clean working tree is not required.
- [ ] User-facing Help/documentation and the execution record reflect actual delivered behavior.

“Implemented but native verification blocked” is a legitimate reportable status, but it is not equivalent to the complete goal passing this checklist.

## K. Final Delivery Report and Execution Record

At the end, give a concise factual report containing:

1. Completed milestones/checkpoints and a row for each of the eleven features.
2. Important decisions, especially settings coverage, repair identity/reference handling, new prefixes, literal line/note behavior, image limits, table JSON policy and finder-only scope.
3. Important changed files by owner, without dumping every touched line.
4. Actual test commands/selectors, selected counts, passes/failures/timeouts, any grouped versus isolated results, builds and lint outcomes.
5. UI evidence paths and what each proves; explicit NOT_RUN/BLOCKED status where relevant.
6. Branch name, original base SHA, new M0-A baseline SHA, checkpoint commit hashes/subjects, final HEAD, actual upstream and push/local-only status.
7. Remaining issues or verification gaps; distinguish them from optional future ideas.
8. A short final manual review guide for the user after Codex finishes, with the commands/controls to try. Do not require intermediate user testing.

Use this document's execution record or the repository's existing appropriate ledger; do not create a family of redundant plans. Evidence files and small test harness additions are implementation artifacts, not extra planning deliverables.

### Execution record — fill during implementation

| Field | Actual value |
|---|---|
| Initial branch / HEAD | Pending M0-A |
| Pre-existing work preserved | Pending M0-A |
| Verified default integration branch / remote | Pending M0-A |
| Actual base ref / SHA and reason | Pending M0-A |
| New unique task branch | Pending M0-A |
| New baseline commit SHA | Pending baseline creation |
| Initial push result / upstream | Pending M0-A |
| Source reconciliation | Pending M0-B |
| nextest / Rust environment and budgets | Pending M0-B |
| Native desktop capability | Pending M0-B |
| Current checkpoint | Pending |
| Latest completed commit / push result | Pending |
| Feature blockers / unverified acceptance | None established during planning; update with execution evidence |
| Final HEAD / task diff against baseline | Pending completion |

Append compact checkpoint rows with status, implementation commit, tests/evidence and push result as execution proceeds. Do not mark complete on the basis of code written alone.

## Source References

These references ground the planning facts. The checked-out execution branch remains authoritative after M0-A.

- [Inspected commit][s-head] and [master-to-jump comparison][s-compare].
- [Repository engineering and checkpoint instructions][s-agents].
- [Settings rendering and navigation][s-settings]; [settings state][s-settings-state]; [settings command entry][s-settings-plugin]; [settings save coordination][s-settings-save].
- [Plugin interface and built-in registration][s-plugin].
- [Saved action schema/transactions][s-actions]; [action edit dialog][s-action-dialog]; [folder store][s-folders]; [saved target resolver][s-targets]; [missing-entry plugin][s-missing].
- [Clipboard operation model][s-cm-model]; [clipboard executor][s-cm-executor]; [clipboard parser][s-cm-parser].
- [Paste Sequence picker UI][s-paste-ui]; [sequence reference model][s-paste]; [snippet model/search][s-snippets].
- [Screenshot markup editor][s-markup]; [screenshot plugin][s-screenshot-plugin]; [capture implementation][s-capture].
- [Existing Diff path grammar][s-diff-query]; [image dependencies and test targets][s-cargo].
- [Pin domain][s-pins]; [pin native/presenter/UI controls][s-pin-windows].
- [Note Find][s-note-find]; [note panel mutation/save/undo integration][s-note-panel].
- [Hashing and cancellation][s-hash]; [folder traversal][s-folder-scan]; [path identity helpers][s-path-identity].
- [Existing GUI fixture harness][s-harness].
- [Nextest timeout semantics][s-nextest]; [Windows file identity][s-file-id]; [Git commit --only/--allow-empty semantics][s-git-commit].

[s-head]: https://github.com/multiplex55/Multi_Launcher/commit/04e51606c2e1862e262454f424ed576bb4bc0814
[s-compare]: https://github.com/multiplex55/Multi_Launcher/compare/0cb3dabf148dc720ed560626c32ef596b5e8ff9a...04e51606c2e1862e262454f424ed576bb4bc0814
[s-agents]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/AGENTS.md
[s-settings]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/settings_editor/render.rs
[s-settings-state]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/settings_editor/state.rs
[s-settings-plugin]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/plugins/settings.rs
[s-settings-save]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/settings_save.rs
[s-plugin]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/plugin.rs
[s-actions]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/actions/mod.rs
[s-action-dialog]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/gui/add_action_dialog.rs
[s-folders]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/plugins/folders.rs
[s-targets]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/universal_actions/persisted_resolver.rs
[s-missing]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/plugins/missing.rs
[s-cm-model]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/clipboard_modify/model.rs
[s-cm-executor]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/clipboard_modify/executor.rs
[s-cm-parser]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/clipboard_modify/parser.rs
[s-paste-ui]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/gui/paste_sequence_dialog.rs
[s-paste]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/paste_sequence.rs
[s-snippets]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/plugins/snippets.rs
[s-markup]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/gui/screenshot_editor.rs
[s-screenshot-plugin]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/plugins/screenshot.rs
[s-capture]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/actions/screenshot.rs
[s-diff-query]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/diff/query.rs
[s-cargo]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/Cargo.toml
[s-pins]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/pinboard.rs
[s-pin-windows]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/gui/pinboard_windows.rs
[s-note-find]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/gui/note_find.rs
[s-note-panel]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/gui/note_panel.rs
[s-hash]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/hash_generator.rs
[s-folder-scan]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/diff/folder_scan.rs
[s-path-identity]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/src/file_search/path_identity.rs
[s-harness]: https://github.com/multiplex55/Multi_Launcher/blob/04e51606c2e1862e262454f424ed576bb4bc0814/dev/jump_mode/README.md
[s-nextest]: https://nexte.st/docs/features/slow-tests/
[s-file-id]: https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info
[s-git-commit]: https://git-scm.com/docs/git-commit

