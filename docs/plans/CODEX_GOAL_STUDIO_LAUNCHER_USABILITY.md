# Codex Goal — Studio and Launcher Usability

**Companion:** `docs/plans/CODEX_KICKSTART_STUDIO_LAUNCHER_USABILITY.md`  
**Suggested location:** `docs/plans/CODEX_GOAL_STUDIO_LAUNCHER_USABILITY.md`  
**Prepared:** 2026-11-03  
**Scope:** Diagram Studio, Calculation Pad, Markdown note authoring, Dashboard Editor, and the shared launcher query workflow.

This is the feature-specific implementation contract. Read the entire document and the repository's applicable `AGENTS.md` before implementation. Reconfirm named source locations in the actual repository after the Git bootstrap. Adapt moved paths and ordinary implementation details without changing the approved behavior.

## A0. Mandatory Chained Git Branch Bootstrap — Before All Application Work

**The first execution checkpoint is M0-A: inspect Git → identify the exact predecessor tip → create a fresh child branch at that SHA → create this Goal's baseline commit → attempt the initial upstream push.** Do not edit application code, run implementation builds/tests, or begin M1 before completing this checkpoint.

The Goal and Kickstart describe **one shared task branch**. Use one stable branch for this entire Goal; the subsystems and checkpoints below do not create additional branch boundaries. A suggested stem is `feat/studio-launcher-usability`; choose a unique suffix if it already exists.

### Required procedure

1. **Inspect safely.** Read applicable instructions and both documents. In the single existing checkout, inspect `git status -sb`, the current branch, `git rev-parse HEAD`, remotes, upstream, staged/unstaged/untracked changes, and any queue/orchestrator/execution ledger that records the previous Goal. These checks are read-only.
2. **Select the exact parent.** For a continuing sequence, the required base is the immediately previous **completed** Goal/category branch's recorded `final_tip_sha`. Capture `parent_branch` and `parent_tip_sha`. Verify the commit exists and agrees with the handoff; reconcile any disagreement before proceeding. If only the remote has it, fetch safely and verify its exact SHA. Do not infer completion from a branch's name or use whichever branch happens to be checked out.
3. **Use the primary integration branch only for a genuinely new chain.** Only a first Goal with no predecessor defaults to the repository's up-to-date primary integration branch, normally `master` or `main`. Fetch and select its verified tip safely; do not hard-pull over local work. A safe current-HEAD initial base is allowed only when clearly documented and no user work is lost or inadvertently committed. Never fall back to `master`/`main` between Goals.
4. **Protect user work.** Do not reset, automatically stash, hard-pull, rebase, merge, force-checkout, overwrite, or stage unrelated changes to manufacture a clean transition. If local changes prevent safely selecting the required parent, report the precise bounded blocker. Never choose a different parent merely to keep moving.
5. **Create the branch before implementation.** Use the equivalent of `git switch -c <unique-task-branch> <parent_tip_sha>`. Confirm the new branch starts at exactly that commit. Do not implement on the predecessor or integration branch.
6. **Make M0-A a new commit.** Prefer a small task-specific execution ledger consistent with existing conventions. Otherwise, an intentional empty baseline is acceptable: `git commit --allow-empty -m "chore: [M0-A] establish studio launcher usability baseline"`. Stage only this task's new baseline material. Record `baseline_sha` separately from `parent_tip_sha`. Verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`; M0-A must be a direct child, not a merge.
7. **Attempt one initial non-force upstream push.** Use `git push -u origin HEAD` when `origin` is the intended repository, or the correct established remote. If access is unavailable or denied, retain the local commit, record the exact failure/unavailable condition, and continue locally. Do not retry in a loop or bypass access controls.
8. **Keep later checkpoints on this branch.** Commit coherent verified increments and attempt one normal best-effort push after each checkpoint. Do not force-push, rewrite checkpoints, or create per-agent/per-checkpoint branches.
9. **Use one checkout only.** No Git worktrees, auxiliary checkouts, or secondary/helper/agent-specific clones. Explicitly prohibited: `git worktree add`, `git worktree move`, `git worktree prune`, and new worktrees for agents. Read-only reasoning/review can run in parallel; branch transitions, source writes, staging, and commits are sequential.
10. **Handoff the final tip.** At completion record the current branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs, `final_tip_sha`, push results, and checkout state. The next Goal must branch directly from this `final_tip_sha`. No intermediate merge, rebase, squash, or cherry-pick is permitted merely to advance the chain. The final branch in the approved sequence contains cumulative work for one final integration PR/merge at the approved boundary.

If a predecessor push failed, its actual local final tip remains the required parent. On another machine, missing that commit is a blocker; stale remote state is not an acceptable substitute.

### M0-A acceptance and ledger

- **Objective / ownership:** establish correct ancestry and an auditable execution baseline; orchestrator owns Git and the task ledger.
- **Behavior / edge cases:** preserve dirty user work, handle missing/conflicting predecessor records explicitly, and continue after nonblocking push failures.
- **Verification:** inspect branch/HEAD, staged paths, baseline parent, and push result. No application build is required for M0-A.
- **Acceptance:** unique task branch active; verified parent recorded; baseline directly descends from it; initial push attempted or demonstrably unavailable; no unrelated work included; no extra checkout/worktree created.
- **Commit boundary:** `chore: [M0-A] establish studio launcher usability baseline`.

Use the existing authoritative queue ledger when present; otherwise use one compact execution ledger under `docs/plans/execution/standalone/studio-launcher-usability/`. Track checkpoint state, evidence paths, commands/results, ancestry, and push state. Record a commit's own SHA after committing, in the next ledger update or the external handoff/final report. Do not create a recursive series of commits just to put each commit's own SHA inside itself.

## A. Goal / Desired Outcome

Make the launcher usable while ordinary tools remain open, and complete the requested usability improvements without replacing working subsystems.

The user must be able to keep Calculation Pad or Dashboard Editor open, click the main **query entry field**, type `note list today`, inspect/activate the resulting note action, and return to the original tool with its draft intact. Diagram Studio must offer clear preview visibility, zoom, adjustable width, a separate viewer, and working templates for more Mermaid diagram families. Calculation Pad must offer a native popout, useful examples, remainder/modulo and mixed-base expressions, optional source restoration, and demonstrable reuse of its existing evaluation cache. Markdown notes need a custom table builder and safe Save Changes on close. Dashboard Editor needs successful-save feedback and durable expansion preferences.

No questionnaire is needed for routine implementation choices. The scope decisions below are intentional defaults grounded in current behavior.

## B. Current Codebase Findings

These are observed owners and behaviors, not proposed replacement APIs.

| Area | Actual source / symbols | Finding and implication |
| --- | --- | --- |
| Query field and ownership | `src/gui/render.rs`: `query_focus_owner_blocks_with_capture`, `query_mouse_focus_available`, `query_focus_session_context_is_eligible`, `render_root_ui`; `src/gui/mod.rs`: `QueryFocusNotesContext`, `QueryFocusSession` | Query interaction is restricted by open panels, top layers, focused-response ownership, and notes-specific exceptions. A shared ownership change is required; enabling one TextEdit alone will not fix keyboard execution. |
| Navigation and lifecycle | `src/gui/render.rs`: `resolve_jump_mode_navigation_owner`, `root_navigation_allowed` inside rendering, `revalidate_show_query_focus_after_dialogs`, `on_ui_frame_complete`; `src/gui/jump_mode/ui.rs` | Jump Mode and ordinary input must agree on the current owner. Retained-pass and visibility/native-root identity checks protect against stale focus and duplicate actions. |
| Open-panel bookkeeping | `src/gui/mod.rs`: `TRACKED_PANELS`, `any_panel_open`, `focus_panel`, `close_front_dialog`; Dashboard Editor is also handled explicitly in `render.rs` | Panel existence is not the same as keyboard ownership. Do not globally remove panel bookkeeping or Escape/close behavior. |
| Diagram Studio presentation | `src/gui/diagram_studio.rs`: `ViewMode`, `Camera`, `draw_body`, `visual_workspace`, `draw_canvas`, `canvas_shortcuts` | Source/Visual/Split modes, wheel and keyboard zoom, pan, Fit, and Reset already exist. Both source/visual and inspector/canvas use equal column splits, restricting actual graph width. |
| Diagram native window and cache | `src/gui/diagram_studio.rs`: `viewport_id`, `viewport_builder`, `show_impl`, `RevisionCache`, `refresh_cache`, `on_ui_frame_complete` | The Studio already has its own native viewport and revision cache. The requested popout is an additional viewer, not another copy of the editor/document. |
| Diagram grammar/templates | `src/diagram/mermaid.rs`: `parse_mermaid`, `parse_header`; `src/diagram/templates.rs`: `DiagramTemplate` | Native parsing accepts a bounded flowchart subset, including TD/TB/LR. Only Linear, Decision, and Process templates exist. Mindmap and sequence require real additional rendering capability. |
| Diagram state and I/O | `src/diagram/session.rs`, `document.rs`, `codec.rs`, `persistence.rs`, `export.rs` | `DiagramSession` preserves exact unsupported source; `DiagramDocumentController` owns safe document transitions. Native scene/image exports assume a flowchart graph. Generic rendering must not falsely claim native graph editability. |
| Calculation Pad state/cache | `src/calculation_pad/session.rs`: `CalculationPadSession`, `evaluation`, `replace_source`; `src/gui/calculation_pad_dialog.rs` | Evaluation already caches by source revision. The root-hosted window stages effects through `RootEffects` and `on_ui_frame_complete`. Hosting changes must preserve that protocol. |
| Calculation grammar | `src/calculation_pad/document.rs`: `prepare_line`, `evaluate_line`, `CalculationValue::ExactBase`; `operators.rs`: `CheckedOperators`; `src/common/numeric_base.rs` | `sqrt` and many other functions already work. Scalars use finite `f64` through exmex; exact unsigned base conversions use `u128`. Prefixed mixed-base scalar expressions and modulo are missing. |
| Calculation documents/settings | `src/calculation_pad/persistence.rs`: `PadDocumentStore`; `src/settings/model.rs`; `src/settings_editor/{catalog,mapping,state,render,save}.rs` | Named documents have bounded atomic persistence and conflict checks. Completed close drops the active session. There is no remember-last-source preference. |
| Note tables | `src/gui/note_panel.rs`: `TableEditOperation`, `prepare_current_table_edit`, `apply_prepared_markdown_edit_with_selection`; `src/notes_markdown/tables.rs`: `prepare_insert_table` | Existing insertion uses two hardcoded header names. The pure helper accepts arbitrary headers but no configurable data-row count. Prepared edits already reject stale source and integrate with note history. |
| Note close/save | `src/gui/note_panel.rs`: `request_close`, `save`, `finish_save`, `discard_changes`, overwrite dialog | Dirty close offers Discard Changes / Keep Editing. `save` returns `()`; auto-save-on-close checks only overwrite state and can close after a write error. The new button must use an explicit successful-save outcome. |
| Dashboard Editor | `src/gui/dashboard_editor_dialog.rs`: `open`, `reload`, `save`, `renew_slot_tokens`, `ui_with_jump_mode` | Save/reopen regenerate tokens used in collapsing IDs, resetting sections to open. Tokens also retire unsafe stale interaction targets, so removing token renewal is not a valid fix. |
| Dashboard save publication | `src/gui/render.rs`: Dashboard Editor handoff and `self.dashboard.reload()` | A successful Save reaches the root as `pending_save`/reload without a toast. Existing preview is a layout sketch; it does not satisfy access to the real dashboard. |
| Existing viewport integration | `src/gui/render.rs`: `viewport_bootstrap_state`, `needs_root_ui_when_hidden`, `run_root_frame_for_test`; `src/gui/native_keyboard.rs` | Reuse the existing native/hidden-root and accepted-frame machinery. Do not add a second event loop or broad polling. |

Repository instructions favor focused Nextest runs, sequential writes, active checkpoint commits, and one final first-party lint/review pass. Current named agent files configure planner and reviewer as `gpt-6.1-sol` / high and implementer as `gpt-6-luna` / max; `.codex/config.toml` sets the parent to `gpt-6.1-sol` / medium. Re-read actual configuration at execution time and use named roles where available; do not change model configuration as part of this task.

## C. Functional Requirements and Chosen Defaults

### C1. Main query entry while ordinary tools remain open

1. The real `query_input` TextEdit remains eligible for deliberate focus while any ordinary tool/editor/settings window is open. Cover all ordinary root-hosted panels, including Dashboard Editor; do not add exceptions only for the named tools.
2. Clicking an exposed query field transfers focus and permits text, selection, clipboard editing, history, autocomplete, result navigation, and activation. The query pipeline must work with Jump Mode both enabled and disabled.
3. Typing `note list today` with Calculation Pad open must produce the real query results; activating the note opens it without closing, resetting, saving, or discarding the pad.
4. When a tool editor owns focus, its typing, Enter, Tab, Ctrl+Z/Ctrl+Y, and shortcuts must not also modify the query or activate launcher results. Explicit launcher focus must not be immediately stolen back on the next frame.
5. Keep existing launcher hotkey tap/hold behavior, visibility intent, root HWND generation checks, and native child focus behavior. No new global keyboard hook or new mandatory hotkey is required.
6. Preserve real exclusive operations: active capture/recording that owns input, OS file dialogs, and actual modal or destructive confirmations. Such operations may temporarily reserve input; mere tool-window existence must not. Do not make every local pending state globally modal. A geometrically covered field must never receive click-through input.
7. Normal document actions must stay bound to their original document/owner even if query entry opens another tool. Close/Escape/Jump cancellation retain their current priorities; no background destructive action may receive a leaked Enter.
8. The real main dashboard remains viewable through existing query/dashboard rules while Dashboard Editor stays open. Saved edits update it. Live application of unsaved dashboard configuration is not required.

### C2. Diagram Studio

- **Visibility:** expose a clear Show preview action, integrated with existing Source/Visual/Split modes. Hiding reclaims space; showing restores the previous useful layout and camera. One authoritative presentation state must govern visibility.
- **Zoom:** visible − / percentage / + / Fit / 100% controls; retain canvas wheel zoom and pan. Use local graph-camera scaling, never global egui UI zoom. Reuse current zoom limits unless a concrete rendering bound requires adjustment.
- **Width:** replace rigid equal splits with resizable, clamped source/preview and inspector/canvas allocations. The drawing area must meaningfully widen. For source-rendered families, omit or collapse the unavailable native node/edge inspector and give the render the full preview allocation. Retain a usable stacked layout in narrow windows.
- **View state:** zoom, visibility, split positions, and viewer size are presentation preferences, not document edits. They must not rewrite source, dirty a document, or add undo entries. Preserve them through ordinary hide/show and resize; use existing utility preference conventions where durable persistence is appropriate.
- **Popout viewer:** open one additional native, resizable rendered-view window for the current Studio document. Share document identity and render data; use an independent viewer camera. The editor remains open. Viewer X closes only that viewer; owner document changes update it, and confirmed owner closure closes it. Main-launcher hiding must not accidentally kill the Studio or viewer.
- **New types:** add source-editable, genuinely rendered templates for **Mindmap, Sequence, Class, State, Entity Relationship, Gantt, Timeline, User Journey, Pie, and GitGraph**, in addition to the three existing flowcharts. At least one useful validated starter per new family is required.
- **Editing boundary:** existing supported flowcharts retain native visual node/edge/group editing. Added families are edited as Mermaid source with rendered previews. Do not imply drag-based visual editing or complete Mermaid.js syntax parity for these families.
- **Truthful state:** a generic render can be current even when the native flowchart parser does not support that family. Track renderer status separately from native graph editability. Invalid source remains editable and exact; a retained older render is visibly stale.
- **Save/export:** preserve exact source in native save/reopen, Mermaid export, and Markdown handoffs. Current SVG/PNG exports for the new families must use the same backend/revision as their displayed preview. After any accepted same-frame source edit, revalidate identity/revision/options/current-result status before opening an image picker or publishing bytes; defer/refuse export if the matching render is not ready. Freeze an immutable eligible export result before picker interaction. Never export a stale flowchart as if it represented current source.
- **Efficiency:** unchanged source/theme does not trigger parse/layout work when scrolling, zooming, resizing, hiding, showing, or opening a second viewer. A different raster resolution may be prepared from cached SVG within bounds; that is not a new Mermaid evaluation.

### C3. Calculation Pad

1. **Pop out / Dock back:** move the complete pad UI between its current host and an independent native viewport without duplicating the `CalculationPadSession`. Preserve exact source, document identity, dirty state, history, result cache, selection where feasible, and pending decisions. Docking is not document closure. Native X uses the existing Save/Discard/Cancel workflow.
2. **Examples & reference:** add a separate nonmodal scrollable window with practical copyable examples, grouped operator/function reference, constants, variables, physical-line references, units, exact base conversion, mixed bases, and error explanations. Opening help never overwrites source. Optional insertion is one explicit undoable edit, never an automatic replacement.
3. **Functions:** expose and document existing `sqrt`, `cbrt`, `abs`, rounding, logarithm, trig, min/max, and other currently supported functions. Add `%` remainder and `mod(a, b)` with documented Euclidean-remainder behavior. Preserve finite-operand/intermediate checks. Examples: `-7 % 3 = -1`; `mod(-7, 3) = 2`; zero divisor is an error.
4. **Mixed bases:** accept integer literals with `0x`, `0b`, and `0o` prefixes in ordinary scalar expressions. The requested expression `0x1235 + 0b0101010101` must yield **5002**. Preserve literal text, assignments, references, unary signs, functions, and decimal/scientific notation. Do not tokenize inside comments or braced names.
5. **Precision contract:** keep existing finite `f64` scalar semantics. Parse prefixed integers with checked integer validation, and reject values outside the explicitly supported exactly representable input range rather than silently rounding them. A conservative maximum of `2^53` is acceptable and must be documented. This does not promise exact arbitrary-size arithmetic. Preserve the separate exact `u128` base-conversion path and its output formatting.
6. **Remember source:** add **Remember last Calculation Pad source**, persisted and **off by default**. When enabled, lazily restore the last retained source on the next explicit opening, including after app restart. Do not auto-open the pad at startup. Use a bounded recovery record separate from named documents.
7. **Recovery safety:** remember only accepted source revisions; debounce/coalesce writes and flush on normal close/shutdown. Recovered unsaved text remains a draft. Never overwrite a named saved document with recovery data. Explicit Discard must retire the discarded recovery text; Cancel retains it; New records the new blank state. Turning the setting off clears only the recovery record, not saved documents. Invalid/oversized recovery data produces recoverable feedback without replacing a live draft.
8. **Caching:** retain the existing session cache. One accepted source revision evaluates once when needed; unchanged frames, focus changes, help, scroll, resize, and host migration evaluate zero additional times. Undo/redo, restored source, and document switches invalidate correctly. No per-line incremental engine is required.

### C4. Markdown notes

- Keep the existing quick insertion and add **Markdown → Table → Insert custom table…** to the existing context menu.
- The note-owned builder contains Columns, **Data rows (excluding header)**, editable column names, Insert, and Cancel. Defaults may remain two columns, one data row, and `Column 1` / `Column 2`.
- Support the requested small shapes, including 2 columns × 3 data rows and 5 columns × 2 data rows. A mandatory header and Markdown separator are additional to that count.
- Capture the original note/editor identity, source revision or exact source, and caret/selection-start position when opening. Insert there without deleting selected prose, matching current insertion semantics. Changing focus cannot retarget the operation.
- Apply one prepared edit and one undoable transaction; preserve Unicode, escaping, LF/CRLF, surrounding text, and caret placement. Reject stale source and invalid dimensions before mutation.
- When the existing unsaved-change confirmation is enabled, its prompt offers **Save Changes**, **Discard Changes**, and **Keep Editing**. Prompt X/cancellation retains the note. Preserve explicitly configured close-without-saving behavior when both `note_save_on_close` and `note_confirm_discard_unsaved_changes` are disabled.
- Save Changes means successful save followed by close. A write error keeps the note open and dirty. Overwrite/Save as New decisions retain pending-close intent and close only after success. Add a recoverable Cancel/Keep Editing route to the current conflict dialog, which presently lacks one. Cancellation clears pending-close intent and retains edits. Failed Save as New must preserve the original note slug/path association as well as its source; prepare a candidate identity or restore it on failure. Bind each prompt to the originating note.
- Route auto-save-on-close through the same explicit success/failure contract. Fix the identified close-after-write-failure path as directly related work.

### C5. Dashboard Editor

- Emit one normal settings-aware success toast after a real successful Save. Respect `enable_toasts` and duration; no success toast for failure, reload, reopen, or repaint. Stage the touched Save intent and new toast/preference effects through the final retained-root-frame boundary; `first_ui_pass` alone is not acceptance of a frame.
- Remember mixed collapsed/expanded slot state through Save, close/reopen, and application restart. Scope preferences by dashboard/profile path.
- Expand all / Collapse all update remembered state. State follows a logical slot through swap/reorder; new slots default open and removed slots are pruned.
- Presentation identity must be separate from `slot_tokens` / `target_generation` used for interaction safety. Continue retiring stale Jump Mode targets, property editors, combo anchors, and confirmations after save/reload.
- `SlotConfig.id` is optional and editable. Handle absent, duplicate, renamed, and externally replaced slot identities without assigning one slot's preferences to an unrelated slot or modifying user IDs.
- Keep the editor draft and expansion state intact while the user accesses the main query/dashboard or another tool. Saving updates the real dashboard through its existing reload path.

## D. Scope, Non-Goals, and Invariants

### Included

The behavior in C1–C5, narrowly necessary source/settings/persistence changes, a minimal offline Mermaid rendering adapter, focused documentation, targeted behavioral tests, and native evidence for affected Windows interactions.

### Excluded

- Universal visual editing for every Mermaid family, complete Mermaid.js parity, collaborative editing, workspaces, or a new diagram file format unless backward-compatible extension proves necessary.
- A browser/server/cloud rendering dependency or automatic fallback to WebView2/Node/Chromium.
- A symbolic calculator, arbitrary-precision arithmetic, bitwise programmer toolkit, general unit algebra, or per-line incremental dependency engine.
- A redesign of launcher search, dashboard content, all dialog hosting, all Markdown rendering, or global hotkeys.
- Unrelated test failures, broad UI cleanup, vendored-crate lint cleanup, full-suite campaigns, and repeated release builds.

### Preserve

Exact source and Unicode; current saved document compatibility; dirty/conflict/overwrite safeguards; note undo/redo and search refresh; dashboard persistence protections; accepted egui pass ownership; once-only external effects; hotkey tap/hold and capture ownership; native viewport close semantics; existing settings defaults unless explicitly changed; lazy startup and quiet idle behavior.

New private modules or fields are allowed when they give the requested behavior a clear owner. They are proposed implementation details, not assumptions about existing files.

## E. Technical Implementation Strategy

### E1. Generalize explicit query focus at the owning boundary

Use the existing query response, native-root identity, visibility revision, geometry/occlusion checks, and retained-pass processing. Replace the notes-only notion of permissible neighboring windows with a bounded representation of ordinary tool ownership where needed.

Separate three decisions:

1. Can the query be rendered as an interactive target for an explicit user transfer?
2. Does the query currently own keyboard input?
3. Is an exclusive operation actually preventing application input?

An open ordinary window must not answer all three with “blocked.” Conversely, keeping the query interactive does not authorize background keyboard execution. Reconcile `QueryFocusSession` and Jump Mode with actual accepted query focus, and preserve owner-instance/generation validation when returning to a tool.

Audit `TRACKED_PANELS`, explicitly handled editors, `jump_mode_unsupported_panel_open`, the top-middle-layer checks, result keyboard routing, history/autocomplete, and post-dialog revalidation. Update affected observation gates only where their documented meaning is actual usable query input; do not launch a performance-harness rewrite. Preserve `any_panel_open` for unrelated close/lifecycle uses.

### E2. Add rendering capability without replacing the native graph editor

The initial candidate is **`mermaid-rs-renderer` 0.3.1**, pinned and integrated as a library with default features disabled, behind an application-owned adapter. Its published API offers Rust parsing/layout and SVG rendering. It is a candidate requiring the M2-B Windows proof, not a claim that Multi Launcher has already integrated or qualified it. [R1]

Use a single deliberate in-memory SVG rasterizer stack for egui textures, with compatible versions and only required features. The current application has no general SVG loader/rasterizer to reuse. Avoid the crate's disk-path PNG helper in the interactive preview path. The upstream project warns that visual output is still developing, so qualify the required template corpus rather than promising full compatibility or copying benchmark claims. [R2–R3]

The adapter owns:

- Exact input source plus document/session identity, source revision, theme/render options, and capability/family metadata.
- Separate native-graph editability and generic preview status.
- One active render with at most one replaceable latest queued request; debounce typing using existing scheduling patterns.
- Cached SVG/geometry result and bounded raster texture variants for the editor/viewer.
- Stale-result rejection, error publication, cleanup, and lazy worker/font initialization.

Retain the native flowchart path for graphs it can edit. Use the generic renderer for approved non-flowchart families; richer flowchart syntax may receive a read-only rendered fallback only if it does not weaken native editability/diagnostic rules. Generic output must never be inserted into `DiagramDocument` as if it were a round-trippable native graph.

Make `DiagramTemplate::create_session` capability-aware: it currently insists on native `PreviewState::Current`, and `DiagramDocumentController::request_template` calls it before dirty-transition handling. Native templates retain that assertion; generic templates create exact-source sessions and qualify rendering through the adapter. Keep derived SVG/raster data transient. The existing codec already preserves unsupported drafts; its `LastGood` graph remains native-only.

In `draw_body`, separate the existing native-status gates for image export and stale/no-preview banners from native-edit controls. Displayed-preview status and export eligibility follow the selected renderer's current result; native editing/Auto-layout still follow native graph validity. A native unsupported-family diagnostic must not display as a render failure when the generic render succeeded.

Preserve existing source byte/line bounds, and add explicit parsed-structure, nesting, SVG-byte, dimension, and raster-pixel bounds using existing `src/diagram/export.rs` limits where appropriate. Inspect the renderer's parse output before costly layout when possible. Keep resource resolution offline; do not resolve user-provided file/network references from generated SVG. Do not log document source.

An in-process Rust worker is not safely force-cancelable. A UI timeout may stop accepting a result, but does not terminate the work. Do not spawn replacement threads on timeout or block the UI joining a live render. Define cooperative retirement, bounded admission, late-result rejection, and shutdown behavior honestly. If hard cancellation becomes necessary due to a demonstrated defect, treat it as a specific renderer blocker rather than inventing cancellation guarantees.

### E3. Preserve frame and document ownership across viewports

Reuse Diagram Studio's stable viewport IDs, explicit native close handling, hidden-root bootstrap, accepted child/root pass staging, and local keyboard ownership. Calculation Pad's existing `RootEffects` assumes ROOT counters; carry originating viewport/host generation and accepted-pass identity before using it in a popout.

A presentation migration cannot create a new document owner. Apply source edits before same-frame Save/Close/Copy intent, and reject effects from discarded passes or an obsolete host. Calculation Pad currently stages already formatted `CopyText(String)` during drawing; merely changing delivery order can still copy the old result. Stage a document/host/result-line-bound copy intent and resolve it from the authoritative evaluation after accepted edits/history changes, or explicitly refuse stale intent. Extend root `on_ui_frame_complete` and test lifecycle seams together. Keep worker results and clipboard/persistence effects outside speculative draw passes.

The detached Diagram viewer shares immutable current render data with the Studio. Do not duplicate the document controller or run independent render jobs per viewport.

### E4. Extend the calculator at its grammar boundary

Use the existing exmex operator factory and scalar preparation path. Add numeric-token awareness before both `validate_expression_bounds` and identifier binding/expression parsing, without broad text substitution. Prefix digits such as the `xff` in `0xff` must not consume identifier/dependency budgets; preserve raw byte and nesting limits. Reuse checked base parsing and retain exact source/diagnostic positions.

Preserve existing legal variable names. `mod` is currently a valid bare assignment name, so do not simply add it to `is_reserved_name` and break old documents. Recognize `mod(...)` in call context or use equivalent hygienic lowering while preserving `mod = 5` followed by `mod + 1`, plus `{mod}`. Any unavoidable compatibility conflict must be reported explicitly, not silently introduced.

Keep current precedence/associativity behavior for existing expressions. The current operator table gives `*` priority 2 and `/` priority 3, so adding `%` at either priority without checking actual grouping is insufficient. Require ordinary left-to-right multiplicative remainder semantics: `20 % 6 / 2` yields 1; `2 * 5 % 3` yields 1; `20 / 6 % 2` is approximately 1.3333333333333335 under existing `f64` arithmetic. Use focused characterization and a narrow lowering if necessary, preserving old expressions without `%`. A broad precedence cleanup is outside scope.

Build help examples from a small shared example catalog consumed by tests and UI. Explain existing scalar-versus-physical-versus-exact-base value categories and physical-line numbering. Correct the README's wording so “pure evaluation starts from source” is not mistaken for “the UI recalculates every frame.”

### E5. Persist user preferences and recovery through existing owners

Use additive serde defaults and current settings save/merge conventions. Expose the remember-source option through a discoverable pad settings entry and the existing settings/catalog path as appropriate; avoid competing preference copies or stale settings-editor saves overwriting it.

Recovery has a versioned, bounded record separate from named pads and only stores source/restoration metadata necessary for this requirement. Restore through the authoritative session, generate valid line identity/history baseline, and keep unsaved recovery dirty. Coalesce writes of accepted revisions, and serialize write/clear operations so a late write cannot resurrect data after Discard or disabling remembrance. Validate document identity when switching before an in-flight write completes.

Dashboard expansion preferences are UI data outside dashboard content/history. Use a separate runtime presentation identity per logical slot and a path-scoped persisted mapping. Unique existing IDs can help, but absent/duplicate IDs require safe reconciliation, not blind index reuse. Preserve state through known local moves; reset only ambiguous externally replaced entries to defaults. Keep interaction token renewal intact.

### E6. Reuse prepared note edits and explicit save outcomes

Extend `prepare_insert_table` or add a sibling pure helper in the same module for body-row count. Preflight checked cell counts and generated bytes; reuse escaping/newline handling and one `PreparedMarkdownEdit`. Apply through the actual originating note's existing history/mutation API.

Make note-save completion explicit enough to distinguish Saved, Awaiting decision, and Failed. Carry note-bound pending-close intent through overwrite/Save as New and clear it on cancellation. Only successful persistence updates the saved baseline and permits save-and-close. Preserve the existing disabled-confirmation close-without-saving preference. Add the missing conflict cancellation affordance; protect original identity on a failed Save as New. Preserve existing tags/links/title extraction, search refresh, and success feedback. A save must not unexpectedly steal focus from another active editor.

## F. Detailed Milestones and Checkpoints

Execute sequentially on the M0-A branch. A blocked renderer dependency does not prevent independent calculator/notes/dashboard work; document the dependency and continue only the unaffected checkpoints. Do not mark the whole Goal complete while a required checkpoint remains blocked.

### M1-A — Allow deliberate launcher query focus beside ordinary tools

- **Objective:** remove blanket ordinary-dialog blocking at the shared query owner.
- **Code ownership:** `src/gui/render.rs`, `src/gui/mod.rs`, directly affected `src/gui/jump_mode/ui.rs` ownership interfaces.
- **Tasks:** map ordinary versus exclusive owners; generalize the notes-only focus context where necessary; keep the real query response clickable; validate native root, accepted response, viewport, geometry, and owner generation; reconcile post-dialog focus without forcing focus every frame.
- **Behavior:** Calculation Pad, Dashboard Editor, File Search, Settings, notes, and other ordinary panels can coexist with explicitly focused query input.
- **Edge cases:** focus initially in another TextEdit; partially/fully covered query; hidden ROOT; child viewport focused; popup or confirmation opens in the same frame; stale HWND/visibility generation.
- **Regression constraints:** capture/modal ownership, notes focus handoffs, hotkeys, and discarded-pass rules remain valid; do not weaken all top-layer/native checks indiscriminately.
- **Verification:** focused owner-policy/state tests plus real query-response pointer focus with Jump Mode on/off and a representative exclusive owner. Audit all normal panel classifications without a 64-window interaction campaign.
- **Acceptance:** ordinary panel presence alone cannot disable explicit query focus; actual exclusive/occluded cases stay protected; input returns to a tool without losing its draft.
- **Commit:** `fix(gui): [M1-A] allow explicit query focus beside ordinary tools`.

### M1-B — Complete query execution and dashboard access after the focus transfer

- **Objective:** make the full query workflow work, not just cursor placement.
- **Code ownership:** query/history/autocomplete/result paths and Jump Mode handoff in `src/gui/render.rs`; narrowly affected command/panel routes.
- **Tasks:** route editing/navigation/activation according to actual accepted query focus; cancel or transfer stale Jump ownership; prevent tool and root from consuming the same event; retain the active tool while opening query results; keep dashboard visibility controlled by existing query rules.
- **Behavior:** type `note list today` with an unsaved pad open, activate the note, then return to the pad; clear/use the dashboard query while Dashboard Editor remains open.
- **Edge cases:** Enter held across a focus transfer, autocomplete Tab versus traversal, editor Ctrl+Z, two notes, confirmation creation, root show/hide and detached focus.
- **Regression constraints:** source editors do not activate root results; existing notes query bridge and front-dialog Escape behavior survive; no global shortcut redesign.
- **Verification:** extend the existing `notes_query_*` workflow tests and `ordinary_enter_cannot_activate_root_results_behind_timer_or_paste_dialogs` with intentional query ownership; assert actual command dispatch and unchanged tool draft.
- **Acceptance:** typing, result generation, activation, and return-to-tool succeed in both Jump modes; no duplicate/background action; real dashboard is reachable during editing.
- **Commit:** `fix(gui): [M1-B] route query actions by focused owner`.

### M2-A — Diagram visibility, zoom, and usable render width

- **Objective:** expose clear preview controls and reclaim available graph space.
- **Code ownership:** `src/gui/diagram_studio.rs` presentation functions and directly needed preference integration.
- **Tasks:** connect Show preview to existing view modes; add local zoom controls/percentage/Fit/100%; replace both equal-width splits with bounded resizable divisions; retain narrow-window stacking; preserve view state without graph mutations.
- **Behavior:** the user can hide/show, zoom, and widen the graph without editing source or losing selection.
- **Edge cases:** minimum window, high DPI, large/small zoom, zero/invalid persisted sizes, hidden preview, stale/no scene.
- **Regression constraints:** source-focused shortcuts remain source-owned; canvas pan/nudge works; view changes do not dirty/undo/rebuild the source scene.
- **Verification:** extend the existing default-split, minimum-width, canvas-keyboard and source-focus tests. Check native layout at normal and constrained sizes during the final smoke.
- **Acceptance:** actual graph width changes; controls remain reachable; hide/show preserves source/camera; scene-build count stays stable on presentation-only changes.
- **Commit:** `feat(diagram): [M2-A] add preview controls and resizable layout`.

### M2-B — Prove and integrate bounded offline Mermaid rendering

- **Objective:** produce real previews for new families through a small native adapter.
- **Code ownership:** proposed rendering adapter under `src/diagram/`, exports in `src/diagram/mod.rs`, `Cargo.toml`/`Cargo.lock`, narrowly necessary GUI publication hook.
- **Tasks:** resolve/pin the verified renderer candidate and minimum rasterizer dependencies; inspect license/API/resource behavior; compile for the project's Windows target; render and rasterize one mindmap, one sequence, and one flowchart through the production adapter. Then add bounded request coalescing, revision/identity tagging, current/stale/error status, SVG and raster caches, and lifecycle cleanup.
- **Behavior:** rendering is offline and does not run parse/layout in each UI frame; exact source survives success/failure.
- **Edge cases:** malformed/oversized/nested input, huge dimensions, missing glyphs, rapid edits, same numeric revision in a new document, closed owner, old theme result, worker error.
- **Regression constraints:** native graph/session/codec semantics remain authoritative for visual edits; no browser fallback, unbounded threads, disk-temp preview loop, or false cancellation claims.
- **Verification:** Windows compile plus adapter fixtures for the three proof diagrams; one stale-result test, one bounded-input test, and a cache counter test. Record actual resolved versions and material dependency additions.
- **Acceptance:** the production adapter generates bounded nonempty readable graphics for the proof diagrams, rejects obsolete results, and leaves UI input responsive.
- **Bounded blocker rule:** use at most two focused integration corrections; if the candidate fails, inspect at most one verified compatible published alternative version behind the same adapter. If required rendering still fails, record the concrete blocker and continue independent milestones. Do not substitute nonrendering templates or start a universal engine/browser project.
- **Commit:** `feat(diagram): [M2-B] integrate bounded offline Mermaid rendering`.

### M2-C — Add working template families and truthful document/export behavior

- **Objective:** ship the defined template catalog with usable preview and source persistence.
- **Code ownership:** `src/diagram/templates.rs`, `session.rs`/`document.rs`/`codec.rs` only where needed, `export.rs`, `src/gui/diagram_studio.rs`.
- **Tasks:** make `DiagramTemplate::create_session` and the `request_template` preflight capability-aware; keep existing templates and add the ten required families; retain dirty-transition handling; dispatch preview and image export to the correct backend; separate native editability from displayed-preview/export/banner status; omit the unavailable native inspector for generic families; keep source save/reopen/export exact with transient derived images.
- **Behavior:** every advertised required template renders; users edit its source and see current output; exports match that source and render revision.
- **Edge cases:** switching from native graph to generic family and back, comments/CRLF/Unicode, invalid draft with a last-good render, pending dirty decision, source copied from Markdown, and a same-frame accepted source edit plus image-export click before the new render is ready.
- **Regression constraints:** no forced conversion to native graph; no old preview exported as current; preserve native `.mld` compatibility and save conflict handling.
- **Verification:** one table-driven corpus validates all required templates through the shipped adapter/rasterizer; targeted save/reopen and source-export fixtures; existing unsupported-source and dirty-template tests; verify generic current rendering enables image export while native editing stays disabled and no false error banner appears. Test same-frame source-edit/export rejection until a matching result exists, immutable export capture, and existing destination-collision protections. Check image export by decoded dimensions and representative content, not merely file existence.
- **Acceptance:** three existing plus ten family starters work; generic current status is independent of flowchart editability; exact source round-trips; Save/Discard/Cancel still protects replacement.
- **Commit:** `feat(diagram): [M2-C] add rendered Mermaid template families`.

### M2-D — Detach the rendered viewer

- **Objective:** view the current diagram in a separate native window while editing its source.
- **Code ownership:** `src/gui/diagram_studio.rs`; `src/gui/render.rs` viewport bootstrap/frame completion; rendering adapter cache.
- **Tasks:** add stable viewer viewport identity and independent camera; share immutable current render results; stage viewer effects safely; handle viewer close/reopen, owner document switches, owner close decisions, hidden launcher, and fallback embedding.
- **Behavior:** one viewer follows the current document; closing it does not close the Studio; closing the Studio after successful dirty resolution retires the viewer.
- **Edge cases:** double popout request, native X during a pending render, owner close canceled, stale results after document switch, monitor/DPI change.
- **Regression constraints:** one document owner and one parse/layout request stream; no root geometry commands sent to the child or vice versa; viewer keyboard input never alters source unintentionally.
- **Verification:** immediate-viewport lifecycle tests, independent-camera state test, same-revision cache reuse, owner-close cancellation; final native smoke with editor and viewer visible together.
- **Acceptance:** correct close/focus lifecycle, shared current content, no duplicate evaluation and no lost source.
- **Commit:** `feat(diagram): [M2-D] add synchronized native preview viewer`.

### M3-A — Calculator operators, mixed bases, and examples

- **Objective:** make existing capabilities discoverable and add the missing grammar features.
- **Code ownership:** `src/calculation_pad/document.rs`, `operators.rs`, `README.md`, `src/common/numeric_base.rs` only if reusable extension is needed, `src/gui/calculation_pad_dialog.rs`.
- **Tasks:** add checked prefixed integer token handling in bounds scanning and expression preparation; implement documented remainder/modulo semantics with contextual function recognition that preserves legacy `mod` variables; characterize mixed `%`/`*`/`/` grouping; preserve source and old grammar; build a practical examples catalog and scrollable help UI.
- **Behavior:** requested mixed-base sum yields 5002; existing `sqrt` and functions are documented; negative remainder/modulo examples match the stated contract.
- **Edge cases:** invalid digits, missing digits, integer overflow/precision limit, unary negatives, scientific notation, names containing prefix-like text, braced names, zero divisors, non-finite operands, long/nested expressions.
- **Regression constraints:** exact `u128` conversions, current scalar precedence, physical-value typing, line references/limits and Unicode source remain valid; no new general math engine.
- **Verification:** focused evaluator/operator fixtures and one table-driven help corpus; source-preservation assertions; legacy `mod = 5` / `mod + 1` and `{mod}`; mixed remainder/multiplicative chains; many prefixed constants do not consume identifier budgets; help open/copy does not replace the draft.
- **Acceptance:** valid examples evaluate, invalid inputs produce bounded useful diagnostics, source is unchanged, and existing math stays compatible.
- **Commit:** `feat(calculation): [M3-A] add mixed bases modulo and examples`.

### M3-B — Native Calculation Pad host with cache preservation

- **Objective:** pop out/dock the pad using its one authoritative session.
- **Code ownership:** `src/gui/calculation_pad_dialog.rs`, `src/gui/render.rs`, `src/gui/mod.rs`, directly affected viewport/native keyboard integration.
- **Tasks:** introduce a host state and stable child viewport; extend effect staging beyond ROOT counters; preserve accepted source-edit-before-intent order; resolve result-copy intent against the post-edit authoritative evaluation instead of sending draw-time stale text; support native X/dirty decisions; participate in hidden-root bootstrap and frame completion; retain existing cache.
- **Behavior:** dock/popout preserves source, identity, dirty/history/result state; the pad stays usable while main launcher is hidden or query opens notes.
- **Edge cases:** child discard, late root discard, duplicate Save/Copy, stale host generation, pending close while docking, collapsed host, native focus transfer.
- **Regression constraints:** no duplicate session, root focus theft, accidental close on host switch, or evaluation per child frame.
- **Verification:** extend pad retained-pass/save/copy/close tests, including same-frame edit+Copy and stale-host/retired-line copy intent; state equality across two host migrations; evaluation counter stable across idle/help/resize/focus/dock/popout and advances once on accepted source change.
- **Acceptance:** actual native popout works; old and new hosts agree on effect ownership; cache behavior is measured, not inferred from a timer.
- **Commit:** `feat(calculation): [M3-B] support native popout without duplicating sessions`.

### M3-C — Optional last-source recovery

- **Objective:** restore source safely when the user enables the preference.
- **Code ownership:** `src/settings/model.rs`, `src/settings_editor/{catalog,mapping,state,render,save}.rs`, `src/calculation_pad/persistence.rs`/`session.rs`, pad UI and normal shutdown owner.
- **Tasks:** add default-off preference; implement bounded atomic versioned recovery; lazy restore on explicit open; coalesced accepted-revision writes; normal close/shutdown flush; ordered clear on Discard/disable; integrate setting saves without stale overwrite.
- **Behavior:** enabled source survives reopen/restart as an appropriate draft; disabled behavior remains current behavior and named documents remain untouched.
- **Edge cases:** recovery corruption/version mismatch, failed write, disable while write pending, document switch, New, Discard, Cancel, named-document conflict, Unicode/CRLF.
- **Regression constraints:** no startup directory scan/auto-open, per-frame I/O, saved-baseline fabrication, or resurrection of explicitly discarded content.
- **Verification:** preference default/round-trip; isolated recovery lifecycle tests including delayed-write-versus-clear ordering; one restart fixture; existing atomic/conflict tests.
- **Acceptance:** restored bytes match retained source, disabled/off cleanup is limited to recovery data, and failed writes preserve the live draft.
- **Commit:** `feat(calculation): [M3-C] remember the last source when enabled`.

### M4-A — Custom Markdown table insertion

- **Objective:** create requested dimensions and named columns from the note context menu.
- **Code ownership:** `src/notes_markdown/tables.rs`, `authoring.rs` only if needed, `src/gui/note_panel.rs`.
- **Tasks:** extend pure insertion with body rows and checked bounds; add note-owned builder; capture source/caret/editor provenance; apply one prepared mutation; retain the quick two-column command.
- **Behavior:** builder creates the requested table at the original caret, with one-step undo/redo.
- **Edge cases:** 2×3/5×2 shapes, empty/default/Unicode names, escaped pipes, invalid multiline names, LF/CRLF, zero/huge dimensions, changed source, two open notes, Cancel.
- **Regression constraints:** surrounding/selected prose preserved; stale insertion refused; existing table cell/row/column tools work; no repeated row-by-row history entries.
- **Verification:** extend table insertion bounds/source tests and contextual table history test; one owner-retargeting regression.
- **Acceptance:** parsed counts and headers match, builder remains reachable at small size, and undo restores exact original bytes.
- **Commit:** `feat(notes): [M4-A] add custom Markdown table builder`.

### M4-B — Save Changes and close only after successful persistence

- **Objective:** make all related note-close paths preserve unsaved content on failure.
- **Code ownership:** `src/gui/note_panel.rs` close/save/overwrite paths; `src/gui/note_mutation.rs` only for shared result propagation; directly related note tests.
- **Tasks:** introduce note-bound explicit save outcome/pending close; add Save Changes and conflict Cancel/Keep Editing; propagate success through overwrite/Save as New; preserve original slug/path on failed Save as New; keep failure/cancellation open; use the same contract for auto-save-on-close; preserve disabled-confirmation behavior, derived updates, and settings-aware feedback.
- **Behavior:** Save Changes saves then closes; Discard closes without save; Keep Editing/X/cancel keeps edits. Write failure never closes.
- **Edge cases:** read-only or failing destination, overwrite decision, Save as New, two notes, repeated passes, focused query while a save completes.
- **Regression constraints:** no implicit overwrite, duplicate save/toast, baseline update before success, or unintended focus theft.
- **Verification:** successful close and injected write-failure tests through actual save path; deferred conflict/cancel checks; existing auto-save integration and failed-open-note-save test.
- **Acceptance:** when saving is requested, close only after positive persistence success. When confirmation is enabled, retain edits until successful Save Changes or explicit Discard. Preserve existing close-without-saving behavior when both options are disabled. Failed conflict actions retain source and original note identity.
- **Commit:** `fix(notes): [M4-B] save before closing and retain drafts on errors`.

### M5-A — Dashboard presentation persistence and save feedback

- **Objective:** preserve the user's editor arrangement and confirm real saves.
- **Code ownership:** `src/gui/dashboard_editor_dialog.rs`, successful-save handoff in `src/gui/render.rs`, narrowly needed settings/UI preference owner.
- **Tasks:** separate collapse presentation identity from interaction tokens; persist/reconcile state per dashboard path; preserve local swap/reorder state; prune removed slots; support Expand/Collapse all. Narrowly stage the touched Save intent and its outcome through final retained-frame completion, then publish one success toast and accepted preference updates. Do not equate `first_ui_pass` with accepted root completion. Keep token renewal and ownership-before-`mem::take` ordering; do not transplant an unrelated dialog's whole architecture.
- **Behavior:** mixed expansion state survives Save/reopen/restart; successful Save updates real dashboard and shows one toast; query access from M1 preserves editor draft.
- **Edge cases:** optional/duplicate/renamed slot IDs, external replacements, another dashboard path, invalid preference data, failed save, disabled toasts, sizing/discard passes.
- **Regression constraints:** no stale Jump/property/popup/confirmation targets survive; no user slot-ID mutation or dashboard-file UI-state pollution; corrupted dashboard stays protected.
- **Verification:** mixed-collapse save/reopen/restart tests, swap/add/remove identity case, toast success/failure/disabled case, one late-discard/multipass regression proving no phantom or duplicate toast/preference write, and existing designer target-retirement/popup/confirmation tests.
- **Acceptance:** stable presentation and retired interaction identities coexist; save feedback is exactly once; real dashboard/query remain usable during editing.
- **Commit:** `fix(dashboard): [M5-A] retain editor folds and confirm successful saves`.

### M6-A — Focused integration, final review, and handoff

- **Objective:** verify the complete user workflow and deliver an accurate completion record.
- **Code ownership:** orchestrator/reviewer; only necessary remediation in touched owners; task execution ledger/evidence.
- **Tasks:** inspect cumulative `baseline_sha..HEAD` diff; run the targeted checks below; build/launch one representative Windows binary; perform bounded native smoke; resolve feature-related findings; finish first-party lint cleanup; update relevant help/README; commit remaining verified task changes and evidence index.
- **Behavior:** all C1–C5 requirements work together with no competing source/keyboard/document owners.
- **Edge cases:** genuine native environment limitation, dependency failure, preexisting unrelated lint/test failure, unavailable push.
- **Regression constraints:** no full-suite default, historical acceptance campaign, weakened tests, user-performed intermediate checks, or automatic final merge.
- **Verification:** H's matrix and one independent cumulative review, then only targeted revalidation for corrections.
- **Acceptance:** all required checks pass or are explicitly blocked/unverified with evidence; Complete is used only when J is satisfied; correct Git handoff recorded.
- **Commit:** `chore: [M6-A] record studio usability verification and handoff`, plus narrowly scoped remediation commits if needed. An evidence-only commit is not proof that an unrun check passed.

## G. Active Checkpoint Commit Cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

M0-A is mandatory and is this branch's own baseline. All later checkpoint subjects and boundaries are listed in F. Commit coherent, scoped, verified increments; inspect staged paths and current branch before committing. Use separate small remediation commits instead of rewriting history.

After each required commit, attempt one normal push to the task upstream, or establish it on the intended remote. Retain commits and continue if pushing fails. Record pushed versus local-only SHAs accurately. Do not stage user changes, squash prior checkpoints, create checkpoint branches, or use worktrees.

This Goal has **no internal branch handoff**. At its end, finalize committed work and supply `final_tip_sha` to the next Goal. Preserve one linear tip-to-base chain.

## H. Verification and Testing Strategy

### H1. Test environment and commands

Use the current project's cached build profile consistently; `fast-dev` exists and is a suitable initial choice. Do not change Cargo profiles or Nextest TOML merely to impose the verification budget.

All new app/test profiles must set Paste Sequences `next_hotkey = None` and `cancel_hotkey = None` **before constructing LauncherApp**, as required by AGENTS. Do not alter production defaults or mute all error toasts. Use isolated data/settings/note paths through existing fixtures; never touch the user's real documents.

Commands below are templates for focused selection, not an instruction to run every group after every checkpoint. Confirm current names and nonzero selection with `cargo nextest list` when needed. Use explicit targets, `-p multi_launcher`, `--no-fail-fast`, and the selected profile.

```powershell
cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(calculation_pad::) | test(gui::calculation_pad_dialog::)'
cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(diagram::) | test(gui::diagram_studio::)'
cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(notes_markdown::tables::)'
cargo nextest run -p multi_launcher --test note_panel_auto_save --cargo-profile fast-dev --no-fail-fast
```

For query, dashboard, and note GUI changes, select the exact directly affected existing/new tests instead of running the whole `gui::render` or `note_panel` module. New meaningful tests can use a task-specific `studio_usability_` prefix so selection is straightforward; that prefix is a proposal, not an existing test.

Existing tests especially relevant to this contract include:

- `notes_query_mouse_quick_notes_round_trip_in_both_jump_modes`.
- `notes_query_mouse_note_panel_round_trip_and_obstruction_refusal`.
- `notes_query_activation_mouse_workflow_runs_current_result_beside_notes`.
- `ordinary_enter_cannot_activate_root_results_behind_timer_or_paste_dialogs`.
- `diagram_studio_canvas_keyboard_pan_zoom_and_arrow_nudges_are_scoped`.
- `diagram_studio_child_discard_does_not_commit_visual_transaction`.
- `diagram_studio_root_discard_after_show_drops_staged_visual_transaction`.
- `diagram_studio_os_viewport_close_requests_cancel_until_dirty_decision`.
- `diagram_studio_imports_unsupported_mermaid_and_exports_exact_draft`.
- `evaluation_is_cached_until_the_source_revision_changes`.
- `pad_late_root_discard_keeps_edited_source_and_saved_file_before_save`.
- `pad_rejected_source_edit_aborts_same_frame_save_intent`.
- `contextual_table_edit_uses_parser_cell_position_and_note_history`.
- `failed_open_note_save_reports_error_and_preserves_unsaved_edits`.
- `jump_mode_designer_ownership_editor_slot_swap_and_reload_retire_targets`.
- `jump_mode_dashboard_editor_selection_and_reset_confirmation_need_fresh_activation`.
- `jump_mode_dashboard_editor_native_popup_requires_its_current_combo_anchor`.

Preserve behavioral intent when an old assertion encodes the intentionally changed blanket-dialog policy. Do not delete a negative test that protects unfocused Enter merely because focused query Enter is now allowed.

### H2. Build, lint, and review

A targeted test compile can satisfy a checkpoint's compile need; do not add a redundant full build after every small change. Near final integration, use one cached native build such as:

```powershell
cargo build -p multi_launcher --bin multi_launcher --profile fast-dev
cargo clippy -p multi_launcher --lib --bin multi_launcher --profile fast-dev --no-deps -- -D warnings
```

Run final Clippy once after implementation, remediate relevant first-party findings, then rerun only as needed to establish the result. Do not clean vendored crates or unrelated historical warnings. Report an unrelated failure accurately instead of claiming a clean lint run. Review formatting/diff for touched files without a repository-wide formatting rewrite.

### H3. Native smoke and evidence

Codex must build and launch the application, perform these interactions, and capture a small set of screenshots or equivalent native evidence when supported. Use one bounded smoke session, not a separate qualification campaign for every checkpoint.

| Flow | Required observation |
| --- | --- |
| Shared query | Open unsaved Calculation Pad, click main query, type `note list today`, activate a real result, return to unchanged pad. Exercise both Jump Mode states. |
| Focus protection | Enter/type in a tool must not execute a root result; an actual destructive/capture owner stays protected; return to query is deliberate. |
| Dashboard | Keep editor open, vary section expansion, view main dashboard/query, return, Save once, see toast and updated real dashboard, then reopen with same expansion. |
| Diagram controls | Hide/show preview, resize both allocations, zoom/Fit/100%, inspect narrow layout. Source and dirty state unchanged by view actions. |
| Diagram types/viewer | Open real Mindmap and Sequence templates; edit source; pop viewer onto a second window; verify synchronized current output and independent camera; viewer X leaves editor open. |
| Calculation UI | Pop out/dock, inspect scrollable examples, evaluate requested mixed-base expression, use modulo, and verify correct active editor owns shortcuts. |
| Calculation recovery | Enable preference, preserve source through a clean restart/reopen; disable and verify named documents remain; explicit Discard does not return. |
| Notes | Build named custom table from right-click, preview, undo/redo; dirty close Save Changes succeeds; controlled failure retains the open dirty note. |

Use normal and constrained/DPI-scaled views where they reveal actual risks. Do not require pixel-perfect screenshot assertions. Existing immediate-viewport/retained-frame tests are useful but are not proof of real Windows activation or multi-monitor behavior.

If native execution is unavailable, complete all supported implementation and tests, record exact missing checks and the environment limitation, and continue independent work. Do not ask the user to perform intermediate tests and do not label missing native acceptance as passed.

## I. Risk Management and Bounded Validation

| Risk | Required mitigation / boundary |
| --- | --- |
| Focus fix leaks commands into background tools | Separate interactive target from keyboard ownership; test focused query and focused tool, Jump on/off, and one real exclusive owner. |
| Native renderer accepts only part of Mermaid | Qualify required family templates through the shipped adapter; preserve source; expose capability/error state; no full-parity claim. |
| Render worker stalls or grows without bound | Source/structure/raster limits, bounded queue, latest-result publication, lazy lifecycle, no unbounded replacement workers or fake thread cancellation. |
| Multiple viewports duplicate effects | Origin host/generation plus accepted root/child staging; one session; same-frame source edit before Save/Close/Copy. |
| Recovery resurrects discarded data | Serialize/coalesce writes and clears, check revision/identity, and test delayed completion after disable/discard. |
| Note save failure loses edits | Explicit successful save outcome is the sole save-and-close authority. |
| Collapse fix enables stale actions | Persist presentation separately; retain target-generation and combo/confirmation invalidation. |
| Dependency/build work expands | One renderer candidate plus at most one compatible version check; no new browser platform or universal parser; use minimal features. |

**Budget:** one initial focused verification pass per checkpoint and up to two focused remediation/revalidation rounds for a concrete failing behavior. Ordinary compile corrections are part of implementation; do not treat the first compiler error as a user blocker. Keep the final native session to the listed flows, normally 30–45 minutes of interaction excluding compilation. Use one cumulative independent review and one final lint cleanup pass. Rebuild/retest only changed or implicated code.

If a required defect remains after bounded attempts, record evidence, attempted remedies, exact command/native failure, affected requirements, and the smallest next action. Distinguish feature blocker, environment-only missing verification, unrelated preexisting issue, and optional enhancement. Continue independent checkpoints when safe; never silently mark a blocked dependency complete.

Follow AGENTS' stalled-goal report convention under `docs/` when an actual stop occurs. A reporting commit is not an accepted implementation checkpoint. Do not inherit historical queue acceptance campaigns or relaunch unrelated failures.

## J. Definition of Done

The Goal is **Complete** only when:

- All C1–C5 functional requirements and their checkpoint acceptance conditions are implemented.
- The full main query workflow works beside ordinary tools while true input ownership safeguards remain valid.
- All required Mermaid templates render; native visual editing remains correct; presentation controls and the additional viewer work without duplicate document/render state.
- Calculation Pad popout, examples, mixed bases/modulo, optional recovery, and existing cache behavior are verified.
- Custom tables preserve note provenance/history; Save Changes and auto-save-close never discard on write failure.
- Dashboard expansion survives the required lifecycle and saves emit correct feedback without weakening target retirement.
- Focused tests/checks and supported native acceptance are complete; no known feature-related blocking defect remains.
- Final review/lint findings are handled or accurately categorized; unverified native behavior is not described as complete.
- Required checkpoints are committed on the correct fresh chained branch; M0-A directly descends from the predecessor's recorded tip.
- `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs, `final_tip_sha`, actual push state, and checkout state are recorded.
- No Git worktree, auxiliary checkout/clone, independent implementation branch, or intermediate branch merge was used.

When a required condition remains unmet, report **Partial / Blocked** or **Implemented; native verification pending**, as appropriate, with precise evidence. Never call partial template coverage or merely compiled UI complete.

## K. Final Delivery Report

Report succinctly, supported by actual execution:

1. Completed checkpoints and user-visible behavior.
2. Important decisions, including source-rendered versus visually editable diagrams and scalar precision limits.
3. Relevant files/modules changed and resolved dependency versions.
4. Exact targeted commands/results, meaningful test coverage, and native evidence locations.
5. Blocked/unverified requirements and unrelated preexisting failures, if any.
6. Git: predecessor branch and `parent_tip_sha`, task branch, `baseline_sha`, checkpoint commit subjects/SHAs, `final_tip_sha`, push results, and dirty/clean state.
7. Explicit handoff: **“The next Goal must create its fresh branch from <final_tip_sha>, not from master/main.”**
8. Confirmation that no worktrees/auxiliary clones or intermediate branch merges were used.
9. A short final user review checklist covering the affected workflows. The user performs their review after Codex's work; do not delegate intermediate verification to them.

Stop at this Goal's completion boundary. Do not merge into the integration branch or start a new category unless already authorized by the active orchestrator.

## References for the Renderer Decision

Source-specific internal references are in B and the checkpoint ownership sections. External references support only the proposed dependency's published capabilities; they do not constitute integration test evidence.

- **[R1]** [mermaid-rs-renderer 0.3.1 published library API](https://docs.rs/mermaid-rs-renderer/0.3.1/mermaid_rs_renderer/) — source-to-SVG API and declared family coverage.
- **[R2]** [Upstream project](https://github.com/1jehuang/mermaid-rs-renderer) and [manifest](https://github.com/1jehuang/mermaid-rs-renderer/blob/master/Cargo.toml) — dependency features, license, and stated early-development limitation. Use the resolved release manifest during implementation, not unpinned master features.
- **[R3]** [Published PNG helper](https://docs.rs/mermaid-rs-renderer/0.3.1/mermaid_rs_renderer/render/fn.write_output_png.html) — path-based output; use an in-memory preview path.
- **[R4]** [Official Mermaid syntax reference](https://mermaid.js.org/intro/syntax-reference.html) — authoritative language examples for the chosen families; shipped compatibility is bounded by the validated adapter/corpus.
