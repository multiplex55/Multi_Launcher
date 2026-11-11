# Codex Goal — Utility Usability and Visual Insights

Companion: `CODEX_KICKSTART_UTILITY_USABILITY_AND_INSIGHTS.md`  
Goal key: `utility-usability-and-insights`  
Suggested task branch: `feat/utility-usability-and-insights` (make unique if it already exists)  
Scope: 17 requested outcomes across launcher results, window presentation, File Search, Diagram Studio, Programmer Inspector, Dashboard Editor, recording, Folder Size, and Calculation Pad.

This is an implementation contract, not evidence that the implementation or tests have run. Read the current repository instructions and verify the source locations below. Named **proposed** modules/types do not exist yet unless implementation has already introduced them. The currently checked-out repository, after the correct Git bootstrap, is the execution source of truth.

## A0. Mandatory chained Git branch bootstrap — before all application work

**One Goal/Kickstart pair means one fresh task branch in one existing checkout. Its M0-A baseline must be a direct child of the immediately preceding completed Goal/category's actual final tip. Only the first Goal in a genuinely new chain may default to up-to-date `master`/`main`.** A checkout currently on master, or a predecessor already merged to master, does not change this rule.

Execute in this order:

1. Read `AGENTS.md`, applicable local instructions, this complete Goal, and the Kickstart. Perform read-only Git inspection: `git status -sb`, current branch, `HEAD`, remotes/upstream, uncommitted/untracked work, and the queue/orchestrator/execution ledgers identifying the previous completed category. Do not edit application code, run implementation builds/tests, or make implementation checkpoints yet.
2. Resolve and record `parent_branch` and `parent_tip_sha` from the immediately preceding completed category's final handoff. Verify the commit exists and is committed. Fetch safely when necessary to inspect a remote predecessor; do not substitute an older remote tip for an unavailable newer local predecessor. The prior Studio and Utility Polish ledger is a useful lead, not authority to ignore more recent work. **Do not hard-code an earlier conversation's SHA.** If multiple records disagree, document the exact ancestry blocker rather than guessing.
3. Check whether the required parent's source incorporates the intervening work that must be preserved. An integration branch can contain additional user work even after the predecessor was merged. Do not silently lose that work or solve a conflict by merging/cherry-picking/rebasing an intermediate branch. Resolve the actual predecessor handoff from the authoritative records; report a bounded blocker when that cannot be established safely.
4. Protect all existing changes. Never automatically reset, hard-pull, stash, rebase, merge, force-checkout, or overwrite them to manufacture a clean checkout. Never stage unrelated files. An unsafe switch to the required parent is a blocker, not permission to select another base. A safe current-HEAD initial base is an exception only for the first task with no predecessor, documented without losing or committing user work.
5. Select a unique lowercase kebab-case branch with the appropriate `feat/` prefix. Create and switch directly from the verified SHA, for example `git switch -c feat/utility-usability-and-insights <parent_tip_sha>`. Verify the branch starts at that exact commit. Do not implement on master/main or on the predecessor.
6. Create this branch's own **M0-A** baseline commit: a small task execution ledger is preferred; an intentional empty commit is acceptable. Record `baseline_sha` separately from `parent_tip_sha`, and verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`. Do not count the predecessor's last commit as this Goal's baseline.
7. Attempt one ordinary initial upstream push, such as `git push -u origin HEAD`, subject to available permission/approval. If unavailable, denied, or failed, preserve the branch/commit, record the actual outcome, and continue safe local work. No force push, bypass, or retry loop. A later category must use the actual final local commit, even if pushing failed; execution on another machine blocks until that ancestor is available.
8. All later checkpoints, agents, code changes, builds, and tests for this Goal use this same task branch and checkout. **No Git worktrees, `git worktree add/move/prune`, auxiliary checkouts, secondary clones, or per-agent/per-checkpoint branches.** Allow bounded parallel read-only reasoning; serialize writes, branch changes, and Cargo/native execution.
9. Commit coherent verified increments and attempt normal best-effort pushes after checkpoints. Do not squash, rewrite, reset, or discard prior checkpoints. No intermediate merge/rebase/cherry-pick is permitted merely to advance this Goal or the next category.
10. At completion record branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs, `final_tip_sha`, push state, and checkout state. The next Goal must start a new branch **directly from this actual final tip**, not master/main or an older common ancestor. Only the final approved integration boundary may integrate the cumulative chain.

**M0-A acceptance:** unique active branch; independently verified predecessor; direct-parent baseline; initial push attempted or accurately recorded as denied/unavailable; user work intact; no alternate checkout/worktree; ledger records the identities. No later milestone starts before these conditions are satisfied.

Suggested ledger location, subject to current conventions: `docs/plans/execution/standalone/utility-usability-and-insights/STATUS.md`. Maintain one concise ledger, not an expanding collection of planning reports.

## A. Goal and desired outcome

Make the existing utilities easier to discover, arrange, and inspect without replacing their engines or changing unrelated workflows. The user should see bounded, consistently sized search cards; retain preferred detached windows; open companion tools beside detached parents; clear File Search results and request literal filename-only matching; work in a better-organized Diagram Studio; exchange bit-label definitions; identify Dashboard preview slots; record without an automatic time stop when requested; explore measured disk usage visually; and inspect calculation dependencies in a separate optional dialog.

Success is visible behavior, not merely additional configuration fields or passing model tests. New views must consume the application's existing authoritative data. No treemap-triggered rescanning, independent calculation evaluator, full-document rewrite for presentation changes, or restoration of closed windows at startup.

## B. Current codebase findings

These are source-inspection findings. Reconfirm their locations and callers after M0-A; do not assume old documentation perfectly describes current UI behavior.

| Area | Verified source anchors | Existing behavior and implementation implication |
|---|---|---|
| Launcher descriptions and grid width | `src/plugins/mouse.rs::MousePlugin::action`; `src/gui/render.rs::root_grid_result_text`, `root_grid_cell_response`, `RootListGeometryCache` grid measurement | Mouse actions share a long explanatory paragraph. Grid measurement defaults to `TextWrapMode::Extend` and uses `nominal_width.max(galley.size().x + margin.x)`. A short description alone cannot guarantee bounded cells, particularly for long unbroken text. Measurement and rendering need the same compact, bounded policy. |
| Action data | `src/actions/mod.rs::Action` | Actions carry `label`, `desc`, `action`, and `args`. Preserve wire payloads and saved action formats. Do not introduce a repository-wide Action serialization migration simply to truncate display text. |
| Presentation lifetime | `src/gui/utility_host.rs::UtilityHostState`, `HostIdentity`, `ViewportPassProvenance`, `retire_to_docked` | Closing can deliberately retire the active host to Docked, clearing bootstrap/focus state. This is not a persistent preference. Keep retirement fencing and store the remembered choice separately. |
| Generic and custom hosts | `src/gui/utility_window_host.rs::HostedUtilityDialog`, `UtilityWindowHost`, `ProgrammerInspectorWindowHost`; custom hosts in Calc Pad, notes, Diagram Studio, File Search, etc. | The generic wrapper does not cover every tool. A shared policy must also reach custom ordinary hosts; fixing only Calc Pad is insufficient. |
| Settings ownership and geometry | `src/settings/model.rs`; `src/settings/utility_dialog_sizes.rs::UtilityToolKey`, `UtilityDialogSizes`; `src/settings_save.rs::SettingsSaveIntent` and utility-size mutation handling | Stable bounded tool keys and owned settings writes already exist. Preferred sizes/native geometry are persisted, but presentation is not represented in that schema. Keep size and presentation preferences separate unless a narrowly justified compatible schema change is demonstrably simpler. General Settings saves must not overwrite newer per-tool preference writes. |
| Child Inspector opening | `src/gui/calculation_pad_dialog.rs::CalculationPadDialogEffect::OpenInspector`, `complete_pad_host`; `src/gui/render.rs` handling that effect | The child-open effect currently conveys no origin presentation. Capture a typed origin from the accepted parent interaction rather than looking up whichever viewport happens to have focus later. |
| Notes | `src/gui/notes_dialog.rs`; `src/gui/note_panel.rs` | Notes list/editor have their own host state and close paths. Apply the policy to both, including fresh editor instances. Use stable tool-kind keys, not filenames/titles/IDs. |
| File Search results | `src/gui/file_search_dialog.rs::{start_search,cancel_search,clear_results_and_selection,apply_event}`; `src/file_search/coordinator.rs::SearchCoordinator` | The clear helper exists but does not by itself retire the active request. Events are accepted against `active_search_id`; a visible Clear action must invalidate that identity and cancel work or late results can reappear. |
| Filename semantics | `src/file_search/model.rs::FilenameMatchMode`; `src/file_search/matching.rs::{filename_highlight_match,ranked_substring_match_ranges}` | Ranked substring includes a full-path fallback; Fuzzy is a different mode. Literal basename-only matching is a distinct option, not a rename of Ranked Substring. |
| Backend routing | `src/file_search/executor.rs::plan_for_request`; `walkdir.rs`, `everything.rs`; `src/gui/file_search_dialog/filters.rs`; `src/file_search/settings.rs` | Backend routing explicitly handles Ranked Substring for Everything. The new mode must either be qualified there or use the existing correct traversal path; a late UI-only filter can discard already-capped candidates and is not an adequate implementation. |
| Mermaid layout | `src/gui/diagram_studio.rs::{source_editor,draw_mermaid_colors}` and source/preview split layout | The source editor uses 14 desired rows; Suggestions is rendered inside its column. Colors is currently rendered after the entire workspace. Move the existing color surface into the source column immediately below Suggestions, not into a second copy. |
| Visual layout and endpoints | `src/gui/diagram_studio.rs` Visual rendering, `visual_object_label`, `visual_connector_endpoint_editor`; `src/diagram/visual.rs::{VisualObject,VisualObjectKind,VisualConnectorEndpoint,VisualOperation}` | Arrangement/properties follow the canvas in the current flow. Endpoint pickers use object labels that omit primitive `owned_label` text. Use owned text and IDs for meaningful, unambiguous endpoint names. |
| Visual gestures | `src/diagram/visual_canvas.rs::{CanvasMapping,CanvasHandleKind,CanvasHit}`; `src/gui/diagram_studio.rs` visual gesture and inline-label handling | Existing connector-point handles edit routes; moving an endpoint object reroutes its connector. The test named `diagram_studio_visual_connector_endpoint_drag_previews_and_commits` exercises moving the object, not reconnecting an endpoint to a different object. Do not mistake it for coverage of the requested reconnect gesture. Primitive double-click already edits owned labels and must remain intact. |
| Bit labels | `src/calculation_pad/inspector.rs::{InspectorAnnotation,ProgrammerInspector::set_annotation,annotation_is_valid}`; `src/gui/programmer_inspector_dialog.rs` Bit labels section | Labels are transient half-open ranges. Existing bounds are 128 annotations and 128 UTF-8 bytes per label; overlapping ranges are supported. Labels outside a narrowed working width can remain visible but invalid. Export/import should exchange labels, not numeric bits, source bindings, or comparison state. |
| Dashboard preview | `src/gui/dashboard_editor_dialog.rs::{preview,slot_label}`, slot tokens/presentations; `src/dashboard/editor_presentation.rs` | Preview paints slot fills/borders; the computed slot label is used for a hover tooltip, not an in-cell text label. Existing click/drag behavior positions the selected slot. Improve identification without silently changing those editing semantics. |
| Recording limits | `src/gui/recording_dialog.rs`; `src/recording/model.rs::{RecordingOptions,FrameTimeline}`; `src/recording/session.rs`; `gif.rs`; `mp4_windows.rs` | UI/options/timing currently impose 120 seconds. Both encoders also enforce the cap. GIF additionally stores absolute centisecond buckets in `u16`, so simply raising the cap to 1,000 seconds is incorrect: 65,535 centiseconds is only 655.35 seconds. The entire duration/timestamp path needs coherent treatment. |
| Folder Size tree | `src/folder_size/model.rs::{FolderSizeProgress,FolderSizeResult}`; `src/gui/folder_size_dialog.rs::{FolderSizeProjection,ProjectionKey,FolderSizeAction}` | Accepted progress/results already retain parent indices, directory subtotal/completeness, and generation/publication identity. The GUI builds children and has Select/Drill/Back actions. Reuse them; no new filesystem traversal is needed for the treemap. |
| Pad dependencies | `src/calculation_pad/document.rs::CalculationLineResult::dependency_indices`; `src/calculation_pad/session.rs::{line_ids,document_incarnation,revision,comparison_context}`; Calc Pad GUI context/effect handling | The evaluator already records physical-row dependencies and the session tracks stable line IDs. Build a revision-owned dependency read model from cached evaluation; comparison rows alone do not contain all dependency edges. Preserve physical `rN` semantics and distinguish document incarnation from a reused revision number. |
| Bounded I/O and execution | `src/common/atomic_file.rs::save_atomic`; `src/calculation_pad/persistence.rs`; `dev/run_owned_process.ps1`, `dev/workflows/owned_process.ps1`; `Cargo.toml` fast-dev profile | Reuse established explicit-path, bounded-read, atomic-write and process-ownership patterns. Do not invent a second generic persistence or process-runner framework. |

## C. Functional requirements and resolved defaults

### C1. Traceable requirement inventory

The 17 outcomes below are mandatory. Their identifiers remain stable if milestones are reordered.

| ID | Required outcome | Primary checkpoint |
|---|---|---|
| FS1 | An explicit File Search **Clear results** action. | M2-A |
| FS2 | Optional literal filename-only contains matching. | M2-B |
| H1 | Ordinary child dialogs opened from a popped-out parent open popped out. | M3-B |
| H2 | Ordinary dialogs remember their last accepted Dock/Pop Out presentation, including Calc Pad and notes. | M3-A/B |
| L1 | Short inline descriptions for all launcher grid results, with seven words as the strict maximum for the rendered description and no description-driven width expansion. | M1-A |
| D1 | Mermaid Colors immediately below Suggestions in the source column. | M4-A |
| D2 | A distinct grouped menu row, separate from direct-action buttons. | M4-A |
| D3 | Visual connector From/To choices include the objects' owned text. | M4-C |
| D4 | Visual arrangement controls above the canvas; object properties/actions in a pinned, collapsible right sidebar. | M4-B |
| D5 | Easier interactive connector creation and endpoint reconnection. | M4-D |
| I1 | Export/import bit-label definitions inside the Bit labels section. | M5-A |
| B1 | Clearly associate Dashboard preview rectangles with their slot/widget labels. | M1-B |
| R1 | Recording duration `0` means no automatic duration stop. | M6-A/B |
| R2 | Timed recording accepts 1–1,000 seconds; scroll/step changes use 25 seconds while typed values remain exact. | M6-A/B |
| F1 | Folder Size treemap beside the table, with selection and drill-down using accepted scan data. | M7-A/B |
| F2 | Independently show/hide or collapse the table and treemap; use the existing Folder Size pop-out host. | M7-B |
| C1 | Separate optional Calc Pad Dependency Explorer; normal Pad remains otherwise unchanged. | M8-A/B |

### C2. Presentation and scope defaults

These resolve details not fully specified in the request. They are explicit planning choices, not additional user requests.

**Window presentation precedence.** A valid open originating in a popped-out parent requests Pop Out for an ordinary detachable child, even if the child was previously docked. An existing docked child may be promoted without losing its session; an already popped-out child is focused, not duplicated. A docked/root origin uses the child's remembered preference, then its existing default when none exists. It does not demote an already open pop-out. Explicit user Dock/Pop Out actions remain available. Remember the effective accepted presentation by stable tool kind across close/reopen and application restart, but do not reopen any tool automatically. Multiple note editor instances share a note-editor preference; use last accepted user presentation/close ordering, not unbounded per-note records.

**Boundary of “child UI.”** Include app-owned ordinary companion/editor/help dialogs that already have or need an ordinary tool host. Menus, combo popups, inline sections, tooltips, native Open/Save pickers, modal confirmation surfaces, recording overlays, radial overlays and screen-capture overlays retain their existing ownership/behavior; they are not converted into independent tool windows. Native pickers and confirmations must still be correctly parented to the originating window. Inventory actual caller/child pairs before implementation instead of silently omitting custom hosts.

**Search options.** Preserve existing default Ranked Substring and Fuzzy modes. Add **Filename contains** as an explicit alternative: the literal query must occur contiguously in the leaf name, including its extension; directory path text cannot qualify a result. Keep current case-sensitivity, type filters, roots and limits. The type filter still determines whether directory names are eligible. Clearing results keeps the search query, roots, mode, include/exclude filters and sorting preferences; it clears result-specific selection, preview, refinement and count/diagnostic state and does not delete files or saved configuration.

**Compact launcher descriptions.** Curated built-in summaries should be meaningful, command-specific, and at most seven whitespace-delimited words. The common rendering boundary also caps arbitrary/plugin/user descriptions to seven words for the inline preview, collapses newlines, and pixel-ellipsizes to the fixed cell. A single enormous word still cannot expand a card. Preserve the full underlying user description and any detailed help; do not rewrite saved user actions. Keep detailed built-in guidance in Help/tooltips rather than deleting it. Titles get their own bounded line without the seven-word rule.

**Diagram placement.** File/Edit/View/Templates/Help and other top-level menu launchers occupy a dedicated menu strip. Direct tools, mode toggles and zoom controls occupy a visually separate action strip. In Mermaid Source/Split, Colors is immediately beneath Suggestions inside the left/source column. In source-hidden Visual view, Colors remains reachable via View or a clearly designated inspector section; never leave an invisible-only control. In a Visual Document, Align/Distribute occupy the action strip above the canvas, and property editing/apply controls occupy the right inspector. The inspector collapses/reopens and scrolls independently without forcing the canvas below it. There is no new docking framework.

**Treemap hosting.** Use one Folder Size dialog, one scan/session, one current directory and one shared selection. Provide Table, Treemap and Both views (or equivalent independent visible toggles with at least one visible). The user can collapse/hide either pane. Existing Dock/Pop Out and the new remembered-presentation policy provide the detached experience; do not create a second chart window or scanner. The table remains initially visible. Layout preferences can survive host changes and be stored with existing presentation preferences when there is a natural owner.

**Label files.** Use a small versioned UTF-8 JSON document containing label strings and explicit `start`/`end_exclusive` bit ranges, with an optional source-width hint. Import replaces the label collection after successful full validation and explicit replacement confirmation when nonempty labels would change. It never changes width, signedness, numeric bits, clipboard, Pad source, bindings or comparison inputs. Preserve out-of-current-width definitions as visibly invalid labels rather than truncating them or silently changing the working width. Do not add automatic label persistence or a profile manager.

**Dependency Explorer.** Add one optional companion dialog per active Pad session, launched by a compact Dependencies action/menu entry, not a permanent result button on every row and not a new panel occupying the normal Pad. It lists/selects rows and shows direct/transitive upstream and downstream relations. A click on a reference jumps to the matching source line in the Pad. Relationship highlighting lives in the explorer; any Pad navigation/temporary line emphasis is transient and cannot alter source, history, normal result layout or dirty state.

**Unlimited recording.** `0` removes the product-level automatic duration stop, not resource, disk, encoder-format, target-loss or user-stop safeguards. Keep streaming/bounded frame retention and the current Stop command/hotkey/control. No countdown or ordinary duration cap is used as a hidden replacement. The default remains 30 seconds. Typed `17` remains 17; a scroll/step increment from 17 is 42, clamped to 0–1,000, not rounded to the next multiple of 25. Countdown seconds remain a different setting.

## D. Scope, non-goals, and invariants

Implement only the inventory above and directly necessary integration. Do not add workspaces, a new plugin framework, arbitrary cross-tool pipelines, a generic docking engine, new Mermaid families, new bit arithmetic, a full graph editor for Pad dependencies, disk-cleanup/delete actions, a new recording format, remote recording/upload, or automatic file operations. Do not upgrade egui or the Mermaid renderer as a shortcut to UI work. No unrelated vendored changes.

Preserve:

- Existing action wire strings, ranking/activation, global hotkeys, Jump Mode semantics, keyboard focus, row/context actions and the usability of the main query while tools are detached.
- Source text, document IDs/incarnations, revision checks, undo/redo and Save/Discard/Cancel behavior. Presentation changes never dirty documents or bypass a pending decision.
- Accepted-pass/host-generation/epoch validation. Rendering a discarded frame or a stale child window must not write settings, open another tool, change labels, apply a connector edit, copy text or issue a file operation.
- Existing native geometry restoration/clamping, multiple-monitor handling, single-session identity and child teardown. Remembering presentation is not keeping a retired viewport alive.
- File-search result budgets, exclusions and backend cancellation. No search or scan while merely rendering a view, resizing, changing selection, navigating retained folders, or showing a window.
- Existing Folder Size logical-byte accounting, omitted/reparse/cloud handling, complete-versus-partial semantics and path-verified Explorer actions. Do not claim the historical cross-drive reveal issue is repaired by this new visualization.
- Bit-label overlaps and current bounded integer semantics. Export/import does not become a way to change the Inspector value silently.
- Recording target identity, bounded mailbox/buffers, output staging/atomic publication, stop/cancel distinction and shutdown cleanup.
- Idle behavior: no new periodic pollers for hidden tools; recompute projections only when their inputs or geometry change.

## E. Technical implementation strategy

### E1. Repair the owning boundary, reuse existing state

Keep data production in its current domain owner and add read-only presentation projections. Root text measurement/painting share one display policy; filename matching is enforced before result admission; remembered window state is separate from active viewport lifetime; treemap rectangles refer to retained row identities; dependency edges come from cached evaluator results; connector gestures preview locally and commit through existing document transactions.

New narrow modules are acceptable where they reduce coupling. Suggested **new** modules, only if they fit the current layout: `src/settings/utility_presentations.rs`, `src/folder_size/treemap.rs`, `src/calculation_pad/dependencies.rs`, `src/calculation_pad/bit_labels.rs`, and `src/gui/calculation_dependency_dialog.rs`. These are proposed names, not claims that such modules already exist. Do not build a general-purpose framework around them.

### E2. Persistent presentation and origin routing

Use a bounded stable-tool-key preference map with tolerant missing-field defaults and explicit schema/version handling. A settings-level enum/value should not depend on GUI internals. Preserve unknown/future settings subtrees instead of overwriting them during unrelated saves, following existing settings ownership practices. Integrate per-key mutation/acknowledgement with `settings_save.rs`; avoid saving an old cloned whole Settings value from a close callback.

Capture the actual effective presentation before successful close retirement. Discarded UI passes, a cancelled close, cancelled picker or failed native bootstrap cannot silently persist a fallback Docked state. Apply remembered presentation at the open boundary early enough to avoid a docked flash. Route child-open context explicitly through accepted effects; carry identity/version facts needed to reject an obsolete originating request.

M3-B must inventory and cover generic wrappers and custom ordinary hosts. The inspected `UtilityHostState` users include File Lock, Diff, Duplicate Finder, notes list/editor, XML, Calc Pad, settings, mouse forms, Date & Time Studio, ZIP, Markdown Studio, Dashboard Editor, Image Comparison, Recording, Regex, Folder Size, File Search, Timer, Diagram Studio, Calendar, Table Utility and MkMacro. The generic wrapper also covers Batch Rename, Search & Replace, Clipboard History/Modify, Paste Sequences, Snippets, JSON and Programmer Inspector. Inspect other custom Dock/Pop Out paths as well; this is a targeted host inventory, not permission to refactor every tool.

Record tools/caller edges in the execution ledger as covered, fixed-native/not applicable, or genuinely blocked. Existing close/visibility/dirty decisions remain with their owners. Integrate the new Dependency Explorer with this policy instead of creating a special-case host.

### E3. UI geometry and ownership

All bounded result card painting, hit rectangles, virtualized geometry, hover/selection backgrounds and keyboard reveal offsets must agree. Do not retain offscreen-width hit areas after changing text layout. Regression assertions that intentionally expected content-width expansion must be updated to the new explicit contract, not disabled.

Use existing egui child layouts/panels for Diagram Studio. Bound source, inspector and color-area scroll regions; menus/status cannot cause repeated source movement while typing. Clamp sidebar width and collapse on a narrow host rather than making the canvas unusable. Reuse the same content renderer in docked and native hosts.

### E4. Pure projection identities

Treemap projection keys include scan generation, accepted publication/result identity, current directory, applicable filters and viewport size. Build from immediate children so ancestor subtree sizes are not counted again alongside descendants. A folder uses its retained subtree bytes; a direct file uses its retained logical bytes. Zero/unknown sizes are explained, not represented as invented area. A partial subtotal is not a claim of completeness.

Dependency snapshots include document ID/incarnation, revision, stable LineId per row, physical row number, source range, assignment name, result/diagnostic and resolved dependencies. Build the reverse adjacency once per revision. Traversals use a visited set because diagnostic/cyclic documents can still contain graph edges. Never re-parse expressions with a second ad hoc grammar.

### E5. Recording duration and encoding

Represent duration as a type such as a proposed `RecordingDurationLimit::{Unlimited, Timed(Duration)}` or an equally explicit optional limit. Convert UI zero at the request boundary; never overload zero to mean both immediate stop and unlimited, or emulate unlimited with `Duration::MAX`. Update `RecordingOptions`, `FrameTimeline`, idle/no-frame checks, terminal claims, session snapshots, encoder validation and all callers together.

For GIF, use wide absolute centisecond positions and checked arithmetic; keep the format's per-frame delay field bounded. Split a long held image into valid delay chunks while streaming the same image, with bounded memory, correct final elapsed time and cancellation/error handling. Do not fail a 1,000-second recording because cumulative ticks exceed `u16`. For MP4, remove the arbitrary 120-second timestamp guard and validate actual timebase/sample type conversion limits (including signed API fields where applicable); never wrap or saturate into a plausible but wrong timestamp. Keep true representational/resource failure explicit.

## F. Milestones and task-specific checkpoints

The sequence below is recommended, not an artificial priority barrier. After M0-A, independent M1/M2/M4/M6/M7 work may be reordered for locality or build efficiency. **M3's shared presentation policy precedes integration of new companion dialogs**, M4 layout precedes final gesture UI validation, and the pure M7-A/M8-A work precedes its UI checkpoint. Record any reordered sequence before using it. Stay on the same branch, serialize writes and Cargo jobs, and continue through all milestones without an approval pause.

### M0-A — Establish the branch, baseline and execution inventory

**Objective and ownership:** Perform Section A0 in Git and the task ledger before application work.

**Implementation tasks and behavior:** Resolve the exact predecessor and create the new direct-child baseline, verify parentage and attempt the initial push. After this gate, confirm the source anchors, map the 17 requirements to checkpoints, inventory actual host/child-open paths, and identify the focused test families and available native tooling. Record original defaults and unrelated baseline warnings without starting a full-suite campaign.

**Edges/invariants:** Dirty user files, missing local-only ancestors and conflicting handoffs must be preserved and reported precisely; no alternate base, worktree or hidden integration.

**Verification/acceptance:** The recorded branch, predecessor, baseline-parent check and actual push outcome satisfy A0. The inventory covers generic and custom host families and identifies new modules as proposed. Do not claim source-only observations as executed tests.

**Commit:** `chore: [M0-A] establish utility usability and insights baseline`. This must be the direct child of `parent_tip_sha`; later inventory updates can accompany the first relevant checkpoint.

### M1-A — Short launcher descriptions and bounded result cards (L1)

**Objective and ownership:** Correct root result presentation in `src/gui/render.rs` and shorten verbose built-in summaries in their producer modules, starting with `src/plugins/mouse.rs` and the Hot Folder/Search & Replace actions shown by the affected queries.

**Implementation tasks and behavior:**

1. Centralize the inline description projection: normalize whitespace, cap to seven words, preserve valid Unicode, then ellipsize to the available pixel width. Produce clear curated summaries for built-in commands rather than repeating the same clipped mouse paragraph on every action. Retain long-form explanatory content in the existing help/details surface; preserve user-authored `Action.desc` data and action wire strings.
2. Render a bounded title line and a bounded smaller description line. Keep configured/adaptive grid columns and theme typography, but prevent content from expanding a cell/column beyond its allocated width. Handle a viewport narrower than the current nominal minimum without horizontal spill.
3. Use identical text/layout rules in geometry-cache measurement and actual painting. Update cache keys/invalidation where presentation inputs change. Clip painting and interactions to the intended cell and viewport, not merely a parent window that lets one cell draw over its neighbor.
4. Apply the shared short-description projection to applicable launcher list/result presentation too; retain intentional full-path/detail behavior outside the compact grid.

**Edges/invariants:** Empty descriptions; a single unbroken path/URL; embedded newlines/tabs; Unicode; long titles; explicit `Extend` style; font/DPI changes; selected/hovered last rows; custom/user/plugin actions. Preserve ranking, activation, selection/navigation indices and tooltips/full details. Do not simply insert arbitrary line breaks or shorten only the mouse provider.

**Verification/acceptance:** Render representative `mou` and `folder` results plus synthetic long/unbroken cases at normal and narrow widths and enlarged text. Assert all inline descriptions meet the seven-word maximum, cell widths agree with allocated columns, text cannot cover neighbors, and keyboard navigation/activation still resolves the same Action. Exercise directly affected virtualized geometry/last-row paint tests; no benchmark campaign. Native review should confirm both example queries have consistently bounded cards.

**Commit:** `fix(launcher): [M1-A] bound result cards and shorten descriptions` after focused geometry/presentation verification.

### M1-B — Identifiable Dashboard preview slots (B1)

**Objective and ownership:** Improve `DashboardEditorDialog::preview` and its slot list in `src/gui/dashboard_editor_dialog.rs`, using existing slot tokens/presentation identities.

**Implementation tasks and behavior:** Draw a clipped label inside each preview rectangle containing a compact slot marker and the meaningful widget/custom label. Repeat the same marker in the associated slot editor/list row. Provide full name, row/column and span in a tooltip. For a tiny slot, show its marker and expose the complete association in a compact legend/list instead of overflowing another rectangle. Synchronize hover/selection emphasis between preview and list. Use text/border cues as well as colors so selection and conflicts remain distinguishable.

**Edges/invariants:** Duplicate names/widgets, overlapping slots, spans, tiny cells, long labels, reordering/removal and scrolled lists. Match exact slot identities, not label equality. Preserve existing preview click-to-place/drag-to-resize semantics and occupancy/conflict checks. Do not render live executable dashboard widgets in the editor or change saved layout geometry through passive hover.

**Verification/acceptance:** Fixtures with duplicate labels, multiple spans and collisions show an unambiguous marker-to-list mapping; painted text is clipped to its own slot. Reordering/removal retains correct identity associations. Existing placement/drag/conflict behavior passes. Passive selection/hover does not make a new configuration edit.

**Commit:** `feat(dashboard): [M1-B] label and correlate preview slots`.

### M2-A — Explicit, race-safe File Search clear (FS1)

**Objective and ownership:** Add the accepted **Clear results** action to `src/gui/file_search_dialog.rs`, its keyboard/host integration as needed, and the coordinator retirement boundary in `src/file_search/coordinator.rs` if required.

**Implementation tasks and behavior:** Place Clear results with Search/Cancel, not as an ambiguous replacement for the refinement field's existing Clear. Enable it when results or an active search can be cleared. On an accepted action, cancel any active request, invalidate/retire its GUI request identity, and clear rows, selection, result refinement, preview context, transient counts and result-bound errors. Set an empty ready/cleared view without changing query, roots, filters or persisted preferences. Ensure coordinator/task activity reaches a truthful cancellation/terminal state and worker cleanup does not depend on a now-hidden result view.

**Edges/invariants:** Queued Started/Result/Progress/Completed events after clear; same-frame clear versus deferred row open/export; rapid clear/start; sort/filter changes; discarded UI passes; popped-out hosts; empty repeat clears. Keep the ordinary Cancel behavior that preserves partial results. No deletion and no implicit new search.

**Verification/acceptance:** Inject late events from the cleared request and prove rows remain empty. Start a new request and prove only its events publish. A stale selection/context action cannot open/copy/export an old row after clearing. Form fields remain unchanged, selection/preview disappear, no active worker is orphaned, and repeated clear is harmless.

**Commit:** `feat(file-search): [M2-A] add safe clear-results action`.

### M2-B — Literal filename-only contains mode (FS2)

**Objective and ownership:** Extend `FilenameMatchMode`, shared matching, filters/preferences and backend planning in `src/file_search/{model,matching,settings,executor,walkdir,everything}.rs` and `src/gui/file_search_dialog/filters.rs` as necessary.

**Implementation tasks and behavior:** Add one clearly named matching option **Filename contains**. Match a literal contiguous substring of the leaf name using the existing case/highlight rules, never path fallback or fuzzy subsequences. Preserve exact/prefix relevance ranking among admitted basename matches if convenient. Serialize the new preference compatibly without changing old defaults. Apply matching before result-budget admission. Reuse the correct existing WalkDir plan when Everything support has not been qualified; make fallback/backend reporting truthful rather than weakening specificity.

**Edges/invariants:** `gcc.exe` and `libgcc.a` match `gcc`; `C:\gcc\readme.txt` and `g_config_c.txt` do not. Check extension matching, literal regex/wildcard punctuation, Unicode case mapping, case-sensitive mode, directories under the existing Type filter, empty-query validation, roots and exclusions. Content search semantics and existing Ranked Substring/Fuzzy modes remain unchanged.

**Verification/acceptance:** Pure matcher tests prove positive and negative cases and original byte highlight ranges. Request/preference roundtrip and backend tests show the intended mode reaches the actual search. A path-only match cannot consume the new mode's visible result limit. UI selection persists only as an ordinary preference and does not trigger search until the existing explicit Search/Enter action.

**Commit:** `feat(file-search): [M2-B] add literal filename contains mode`.

### M3-A — Remember presentation separately from viewport lifetime (H2 foundation)

**Objective and ownership:** Add the bounded remembered-presentation model and mutation path near `src/settings/{model,mod,utility_dialog_sizes}.rs` and `src/settings_save.rs`; integrate the lifecycle boundary in `src/gui/utility_host.rs` and generic host wrapper.

**Implementation tasks and behavior:**

1. Define stable-tool-kind preferences, compatible missing defaults, and explicit ownership for writes. Prefer a separate small versioned presentation map so existing v1/v2 geometry records do not require migration. Preserve unsupported/future data and avoid resetting unrelated settings when one preference changes.
2. Keep live `HostIdentity`/generation and retirement semantics unchanged. Restore remembered presentation only at a real open boundary. Capture the effective presentation before accepted close retirement; never serialize the retirement fallback itself as a user Dock action.
3. Record accepted explicit Dock/Pop Out changes and successful close/open transitions according to C2. Coalesce unchanged values; retain the in-session choice and show a bounded failure if persistence fails. Do not write every frame or auto-retry forever.
4. Use the existing native geometry/focus/bootstrap helpers. Restored Pop Out must not briefly flash a docked surface, leave a phantom root modal, duplicate a viewport, or reopen a closed document at startup.

**Edges/invariants:** Missing/old/corrupt/unknown settings data; save failures; stale general Settings saves; multiple independent tool changes; Cancel close; failed save; stale/discarded UI effects; shutdown; user closes two note editors with different presentations. Per-tool preferences must have deterministic accepted-event ordering without growing per-document keys.

**Verification/acceptance:** Tests prove pop-out close/reopen and restart restoration, docking/reopen restoration, absent-field fallback, no startup auto-opening, stale-writer protection, preserved unknown data, and separate retirement generation behavior. Cancelled close cannot record Docked or destroy a draft. The generic host applies the preference once at open, not on every frame.

**Commit:** `feat(windows): [M3-A] persist ordinary tool presentation preferences`.

### M3-B — Custom host coverage and parent-to-child inheritance (H1/H2)

**Objective and ownership:** Integrate M3-A into actual ordinary hosts and accepted child-open callers, particularly Calc Pad → Programmer Inspector/reference, notes list/editor → existing companion tools, and the custom families listed in E2. Relevant files include `src/gui/render.rs`, `calculation_pad_dialog.rs`, `notes_dialog.rs`, `note_panel.rs`, `utility_window_host.rs`, `utility_dialog_size.rs`, native geometry helpers and the tool-specific host adapters.

**Implementation tasks and behavior:** Carry a typed, validated originating presentation in open effects where the parent currently emits only a bare open command. Apply C2 precedence. Promote/focus an existing child without destroying its contents. Initialize newly created note/editor instances from the stable tool preference. Route ordinary child pickers/editors that already use app hosts consistently; native file dialogs retain the correct owner instead of becoming a synthetic host. Complete the inventory in the ledger, including exclusions and their concrete reasons.

**Edges/invariants:** Parent closes or docks before its delayed child request is accepted; parent and child have independent dirty documents; multiple note instances; child already open; hidden launcher; restored offscreen native geometry; pending Save/Discard/Cancel; read-only children. Reject stale opens rather than inferring origin from current global focus. A popup/tooltip is not a new window preference.

**Verification/acceptance:** Table-driven policy tests cover popped parent + remembered docked child, docked parent + remembered popped child, no preference, and already-open targets. Rendered host tests cover Calc Pad/Inspector/reference and both notes paths; representative generic and custom hosts verify restart/close handling. Existing detached-tool/main-query coexistence remains intact. Every ordinary Dock/Pop Out path is accounted for; do not deliver just the three example tools while calling the change global.

**Commit:** `fix(windows): [M3-B] inherit and restore presentation across utility hosts`. A second same-branch remediation commit is acceptable for a distinct integration defect, not a new branch or new goal.

### M4-A — Separate Diagram menus and place Mermaid Colors correctly (D1/D2)

**Objective and ownership:** Reorganize the existing render paths in `src/gui/diagram_studio.rs`, especially `source_editor`, `draw_mermaid_colors`, top menu/action strips and source/preview split composition.

**Implementation tasks and behavior:** Place top-level menu launchers together in one menu strip, with recognizable dropdown affordances using supported glyphs/widgets. Put direct tools and zoom/mode controls in a separate strip; menus must not look like interspersed execution buttons. Move the existing Colors surface directly below Suggestions inside the source column in Source/Split modes. There must not be a blank full-workspace-height spacer between them or a duplicate color editor after the canvas. Retain draft/apply state and current source-preserving transactions. Bound or independently scroll an expanded color area so it remains usable in short windows. Keep source-hidden access to Colors through a clearly designated View/inspector surface.

**Edges/invariants:** Narrow and tall hosts, menu overflow, expanded Colors, multiple Mermaid families, invalid/stale source, long diagnostics, font scaling, docking/undocking, pending dirty decisions. Preserve Tab/Shift+Tab source editing, focus return after Escape, explicit Apply colors, undo/redo and exact unsupported source text. Status changes must not move the source editor on each keystroke. Local property combos remain property controls; do not flatten them all into the top menu strip.

**Verification/acceptance:** Layout assertions place Colors immediately after Suggestions within the same source pane, inside its clip and above no artificial full-height spacer. Menu controls occupy their own strip, direct controls another. Existing color transactions and File/Edit/View/Templates/Help routes remain reachable at normal/narrow sizes. Re-run directly affected status stability, Tab/history, colored source and dirty-template tests. Confirm both docked and popped-out presentation.

**Commit:** `fix(diagram): [M4-A] separate menus and anchor source color controls`.

### M4-B — Visual arrangement toolbar and pinned inspector (D4)

**Objective and ownership:** Recompose the Visual Document UI in `src/gui/diagram_studio.rs` using existing property editors, selection and canvas mapping.

**Implementation tasks and behavior:** Place Align/Distribute and related arrangement controls above the canvas. Give the body a flexible central canvas and a right-hand properties sidebar with a persistent visible collapse/reopen control. Put selected-object fields and their Apply/edit controls in that sidebar; scroll the inspector independently and keep it anchored to the right, not below a long canvas. Preserve draft values when a panel collapses or moves between hosts. Use a bounded adjustable width or existing resizable panel convention, with a useful canvas minimum and a compact narrow-window fallback. Remove obsolete vertical-space accounting that assumed all properties were below the canvas.

**Edges/invariants:** No selection, multi-selection, connector properties, containers, long text, large fonts, small window, sidebar collapsed, active inline text edit, active transform gesture and resize. Layout/view changes do not change object coordinates, document revision, undo history or dirty state. Existing canvas pan/zoom, double-middle reset, read-only viewer primary pan and keyboard editing continue to work. Do not move editable-object drag behavior into a viewer-style pan gesture.

**Verification/acceptance:** Tests show toolbar above the canvas and property rects beside it, independent scrolling, reachable Apply/collapse controls and no clipped lower properties. Resizing/collapsing changes the canvas viewport but not scene content. Test coordinate mapping after resizing and preserve existing arrangement/undo, camera reset and inline-label behavior.

**Commit:** `feat(diagram): [M4-B] pin visual properties beside the canvas`.

### M4-C — Human-readable connector endpoint choices (D3)

**Objective and ownership:** Update `visual_object_label` or a dedicated endpoint-display helper and `visual_connector_endpoint_editor` in `src/gui/diagram_studio.rs` using authoritative `VisualObject` content.

**Implementation tasks and behavior:** Display endpoint choices as an owned text excerpt plus a stable object ID, with shape/type fallback for unnamed targets. Prefer primitive `owned_label.text`; use actual Text object text or container/swimlane labels as applicable. Include the same text in both the selected From/To caption and dropdown rows. Normalize line breaks/whitespace for the compact label, truncate safely, and show full text in a tooltip. Refresh captions after a committed label edit. The stored endpoint remains `VisualObjectId` plus anchor, never the display string.

**Edges/invariants:** Duplicate/empty labels, multiline and Unicode, very long text, missing targets, labelled containers, title exclusions and non-connectable objects. Do not infer labels by proximity to an unrelated free-text object. Keep current eligibility/validation and ID identity unchanged.

**Verification/acceptance:** A labelled rectangle and ellipse show their text; two identically named objects remain distinguishable by IDs. Rename updates captions without reconnecting the edge. Empty and missing targets have truthful fallback labels. Selection/apply still edits the intended object.

**Commit:** `feat(diagram): [M4-C] name connector endpoints from owned labels`.

### M4-D — Create and reconnect Visual connectors interactively (D5)

**Objective and ownership:** Extend existing gesture/handle ownership in `src/gui/diagram_studio.rs` and pure geometry in `src/diagram/visual_canvas.rs`; reuse `VisualConnectorEndpoint` and document transactions in `src/diagram/visual.rs`.

**Implementation tasks and behavior:**

1. Keep the working existing two-selected-object Add connector path. Make Add connector usable when two objects are not preselected: arm a bounded connect tool, show a clear “Choose source, then destination” hint and candidate highlight, and let the user pick valid source/target anchors. Escape cancels without adding an object.
2. Add distinct source/destination endpoint handles for a selected connector. Double-clicking its line should expose/focus this endpoint-edit state; a selected connector's visible handles should also be directly draggable, so the gesture is discoverable rather than double-click-only. Do not confuse them with existing interior route handles.
3. While dragging, preview the reconnect and highlight the eligible target/anchor. Commit one transaction on a valid release. Escape, invalid release, stale document/host, hidden target or loss of ownership cancels the preview, leaving the old connection intact.
4. Preserve connector identity, labels, direction, style and manual interior route/control points when changing an endpoint. Reuse current route projection after attachment. Keep current supported self-loops and eligibility rules rather than imposing a new silent restriction. Do not move/unlock the target object to attach to it.
5. Keep gestures tied to current mapping, clip, host identity, document incarnation/revision and accepted input. Toolbar/inspector/minimap clicks cannot start canvas edits. Reset or cancel an active gesture appropriately when those facts change.

**Edges/invariants:** Straight/elbow/curved/manual routes; zero movement; same endpoint; overlap/z-order; rotated targets; clipping; pointer down/up in one input batch; invalid release; object deletion during a drag; dock/close/resize during preview. Primitive double-click still edits its label. Read-only Diagram Viewer cannot create/reconnect objects. One completed gesture is one undo step; a cancelled/no-op gesture is none.

**Verification/acceptance:** Pure hit/anchor and transaction tests plus UI input tests demonstrate creation without preselection, From and To reconnection, correct preview/commit, exact undo/redo, cancellation and stale-pass rejection. Keep the existing connected-object movement and manual-route tests; they do not substitute for a test that changes `endpoint.object_id`. Verify manual route points and unaffected connector data are preserved.

**Commit:** `feat(diagram): [M4-D] add connector creation and endpoint drag gestures`.

### M5-A — Import/export Programmer Inspector bit labels (I1)

**Objective and ownership:** Add a narrow codec/store near `src/calculation_pad/inspector.rs` (proposed `bit_labels.rs`) and accepted UI effects under the existing Bit labels section in `src/gui/programmer_inspector_dialog.rs`; use the generic host and existing file-picker/atomic-write patterns.

**Implementation tasks and behavior:** Put **Import labels…** and **Export labels…** inside the expandable section, not on the main bit-grid toolbar. Define version 1 UTF-8 JSON with an ordered labels array of `label`, `start`, `end_exclusive`; an optional `source_width` hint can aid validation/explanation but must not change the active mode. Bound file bytes (128 KiB), labels (128), label bytes (128), nesting/schema and integer ranges (`0 <= start < end <= 128`). Reject unsupported versions, malformed/duplicate-range entries and over-limit files atomically. Preserve overlapping but nonidentical ranges, current display order and empty-label behavior where already supported.

Use bounded regular-file reads and explicit destination selection. Export the captured label collection, including labels invalid only because the current mode is narrower. Do not include working bits, paths to Pad documents, expression text or comparison state. Support an empty collection. On import, preview/count the valid replacement; require a normal confirmation if existing labels would change, including replacement by empty. Validate the originating Inspector session and draft/label revision again before replacement after a picker/confirmation returns. Export should use a clearly captured snapshot and report what was written. Reuse durable writes and normal overwrite confirmation; cancelled/failed file operations leave the Inspector unchanged.

**Edges/invariants:** Unknown version, too many labels, huge/invalid UTF-8 file, range beyond 128, duplicate exact ranges, labels beyond current working width, Unicode, disk failure, changing Inspector while picker is open. Successful import changes annotations only; no implicit Pad insertion or numeric mode change.

**Verification/acceptance:** Roundtrip empty/Unicode/overlapping/out-of-current-width labels; exact start/end semantics; malformed import leaves all state unchanged; replacement Cancel is lossless; stale completion cannot overwrite a newer label draft. A successful import reproduces grid annotations and does not alter bits, source or comparison. Buttons are reachable in docked/popped-out and narrow layouts.

**Commit:** `feat(inspector): [M5-A] exchange bounded bit-label definitions`.

### M6-A — Correct timed/unlimited recording across the pipeline (R1/R2 foundation)

**Objective and ownership:** `src/recording/{model,session,gif,mp4_windows,mod}.rs` and immediate callers/tests; adapt the GUI request builder for compilation while leaving the visible control change to M6-B.

**Implementation tasks and behavior:** Implement E5's explicit duration limit throughout request validation, frame timeline, idle/no-frame observation, terminal claims, session view and encoding. Timed requests accept 1–1,000 seconds; Unlimited has no automatic time stop. User Stop, target loss, shutdown and encoder failure still reach the appropriate terminal state and output handling. Distinguish a valid large elapsed duration from a representational overflow.

Widen GIF's absolute time buckets, retain the format's bounded individual delay, split long holds correctly, and check all total/count/time conversions. Preserve cadence coalescing and the current terminal rounding rules. Remove MP4's arbitrary duration guard while validating the real sample-time/duration types. Keep memory proportional to the existing bounded frame pipeline, not recording duration. No buffering the full clip, hidden timeout at 120/1,000 seconds, or fake maximum-duration sentinel.

**Edges/invariants:** No frames for a long interval; delayed final frame; delayed/idle Stop; cadence drops; monotonic clock regression; zero-frame recording; last frame at the exact finite boundary; huge timestamps; disk failure; queue backpressure; simultaneous stop/target loss. Countdown and output publication remain separate and unchanged.

**Verification/acceptance:** Use injected clocks, fake capture/output and in-memory/small-stream encoder tests rather than waiting for long wall-clock recordings. Prove Unlimited crosses 120, 655.35 and 1,000 seconds and stops only for an authorized terminal condition. Timed boundaries stop at the selected duration, including 17 and 1,000. Decode/check GIF timing after a hold beyond 65,535 centiseconds; verify multiple valid delays represent the total without wrap. Test MP4 timebase conversion beyond old limits and true overflow failures. Existing empty-clip, finalization/cancel and bounded-mailbox tests remain valid.

**Commit:** `feat(recording): [M6-A] support explicit unlimited duration safely` after coherent domain/encoder verification. Do not ship only a larger constant.

### M6-B — Exact typed durations and 25-second UI steps (R1/R2)

**Objective and ownership:** `src/gui/recording_dialog.rs` and directly affected request/UI descriptions/help.

**Implementation tasks and behavior:** Allow integer input 0–1,000. Explain `0 = Unlimited`; show an unlimited duration/elapsed indicator without a false countdown/progress percentage. Scroll/step interactions change the current value by 25 with range clamping; typed values remain exact and are not snapped on commit or focus loss. Use the actual widget input semantics rather than assuming drag speed alone guarantees an exact wheel step. Keep the 30-second default, countdown setting and selected frame rate/cursor/scale behavior. Freeze the selected duration in the accepted recording request as with other recording options.

**Edges/invariants:** 0, 1, 17, 999, 1,000; negative/over-range text; step from a nonmultiple; switching formats; active recording; blocked focus; discarded frame. Invalid typed values must be rejected or visibly corrected before Start; no contradictory displayed versus submitted value. Scrolling elsewhere in the dialog must not edit the duration.

**Verification/acceptance:** Rendered input tests prove 17 is submitted as 17, a step produces 42, 0 submits Unlimited, 1,000 is accepted and out-of-range values cannot start. Verify a normal short GIF/MP4 smoke and a short Unlimited recording stopped manually where native tools permit; long-duration correctness is established by M6-A's simulated-time tests. Start/Stop controls and configured stop hotkey remain reachable while popped out or launcher-hidden.

**Commit:** `feat(recording): [M6-B] expose unlimited and exact duration controls`.

### M7-A — Pure treemap over the retained Folder Size tree (F1 foundation)

**Objective and ownership:** `src/folder_size/model.rs` read interfaces and the existing GUI projection, with a small proposed `src/folder_size/treemap.rs` for pure layout if helpful. No scanner change without a concrete missing datum.

**Implementation tasks and behavior:** Build deterministic rectangles from immediate children of the currently browsed directory using retained nonnegative measured bytes. Represent folders by subtree totals and direct files by logical length; exclude the root itself and avoid ancestor/descendant double counting. Use a bounded deterministic layout such as a squarified treemap; no new plotting framework is needed. Keep row identity in every rectangle, percentages scoped to the clearly labelled displayed measured total, and completion/omission metadata in tooltips/status.

Cache by projection generation/publication, directory, filters and available geometry. Stable tie-breaking uses row identity, not pointer addresses. During scanning, consume only accepted published progress, label partial/provisional subtotals honestly, and freeze the last accepted data on cancellation. Zero-byte/unknown/excluded items remain represented in counts/table/status rather than receiving fictitious proportional area. For a directory with direct files, include them explicitly or in a clearly labelled aggregate with an exact sum; do not silently label folder-only area as all disk usage.

**Edges/invariants:** Empty/all-zero directory, one huge child, many tiny children, partial/inaccessible folder, nonrecursive scan, integer totals near limits, filtered results, resize and stable ties. Convert to floating layout values only after safe integer accounting; reject nonfinite geometry. Tiny items can omit text and be batched/aggregated for drawing, but accounting and identity behavior must remain explicit. Do not run a filesystem lookup to paint or drill into a retained row.

**Verification/acceptance:** Pure tests show finite nonnegative in-bounds nonoverlapping rectangles, proportional areas within pixel/layout tolerance, deterministic ordering, no double count, honest zero/unknown handling and exact aggregates. Cache invalidation follows publication/directory/size/filter changes; unchanged frames reuse it. A spy/fake scan service proves layout and cached navigation submit no scan.

**Commit:** `feat(folder-size): [M7-A] project measured folder usage into a treemap`.

### M7-B — Linked table/treemap interaction and visibility (F1/F2)

**Objective and ownership:** `src/gui/folder_size_dialog.rs`, reusing `FolderSizeAction::{Select,Drill,Back}`, existing projection identity, reveal actions and host behavior.

**Implementation tasks and behavior:** Add the treemap beside the exact-value table at comfortable widths, with an adaptive split/stack layout for narrower windows. A single click selects the same row as the table; double-click on a folder drills to its retained children. Add/reuse Back/Up/breadcrumb navigation and synchronize current directory, selection and status between views. Double-click on a file must not unexpectedly launch it; ordinary explicit context actions remain available.

Provide Table, Treemap and Both visibility choices or equivalent collapsible panes with an always-reachable recovery control and at least one view visible. Keep the normal table initially available and preserve view state across Dock/Pop Out. Use one existing Folder Size window; no second scanner or new chart host. Add compact label/size/percentage and full path/completeness tooltip. Use text/outline selection cues, not color alone.

**Edges/invariants:** Data publication between press and release; refresh/generation replacement; partial/cancelled data; hidden table; changed filters; directory not scanned because recursion was off; folder missing on disk. Drill-down traverses retained data only. Explain unavailable children and require an explicit Scan/Refresh to obtain them; never launch an implicit scan. Validate current generation/publication/row identity before applying deferred interactions or Explorer actions.

**Verification/acceptance:** UI tests prove linked selection, two-level drill/back, Table-only/Treemap-only/Both, no navigation-triggered scan and stale-click rejection. Partial/cancelled data stays truthful. Existing sort/filter/Reveal/Copy actions still address the correct original path after visual selection, including a cross-drive fixture. Native viewing may confirm geometry and reveal behavior; source-path tests alone are not a native Explorer pass.

**Commit:** `feat(folder-size): [M7-B] link treemap navigation and table visibility`.

### M8-A — Revision-owned calculation dependency read model (C1 foundation)

**Objective and ownership:** `src/calculation_pad/document.rs` existing evaluation metadata, `src/calculation_pad/session.rs` cached session boundary, and a proposed `src/calculation_pad/dependencies.rs`.

**Implementation tasks and behavior:** Expose a bounded immutable dependency view of the current cached evaluation without copying/re-evaluating the entire Pad per frame. Map each resolved `dependency_indices` entry to the snapshot's stable LineId and current physical `rN`; build direct upstream and reverse downstream indexes and bounded transitive traversal. Include source ranges, assignment names, formatted value/type or diagnostics. Keep unresolved/duplicate/forward/cyclic reference errors visible rather than inventing edges/values or treating diagnostic relationships as successful evaluation.

Key the view by document identity, incarnation and source revision. Rebuild only when relevant accepted Pad state changes while the explorer is needed. Keep the selected LineId through insertions where identity survives; deletion or document replacement clears it explicitly. All navigation requests must be resolvable to the latest accepted line/range in the same document incarnation.

**Edges/invariants:** Blank/comment physical rows; references to named/braced Unicode variables; duplicate names; deleted rows; forward/cyclic references; failed result dependencies; vectors, exact integers and quantities; document-wide limit errors; reopened file with the same ID but new incarnation. Use the authoritative graph and diagnostic policy; do not parse `rN` substrings out of arbitrary source strings or compare numeric values to infer dependencies.

**Verification/acceptance:** Fixtures exercise a chain, fan-out, disconnected rows, comments/blanks, reassigned positions, named versus physical references, errors and cycles. Direct/transitive upstream/downstream sets are correct and finite; duplicate paths are deduplicated. Selection survives unrelated line insertion but does not drift after deletion/reopen. Read-only analysis does not change source, cache semantics, result precision, revision, history or dirty state.

**Commit:** `feat(calc-pad): [M8-A] expose bounded dependency snapshots`.

### M8-B — Separate Dependency Explorer and safe source navigation (C1)

**Objective and ownership:** A proposed `src/gui/calculation_dependency_dialog.rs` using the existing ordinary utility-host contract; `src/gui/calculation_pad_dialog.rs`, `src/gui/render.rs`, panel/state/command declarations and stable tool registry as actually required.

**Implementation tasks and behavior:** Add a compact **Dependencies…** entry in the Pad's existing toolbar/menu. Open one companion dialog with its own Dock/Pop Out and remembered presentation. Use the accepted origin policy from M3. Seed selection from the Pad's current selected/caret row when available, otherwise show an explicit row chooser without assuming an integer result. Provide a compact current-row/source/result header, labelled Upstream inputs and Downstream dependents, and direct/transitive distinctions using collapsible sections. Show physical row labels and meaningful variable names, not raw internal IDs.

Clicking a reference navigates to that line in the existing Pad: bring the correct host forward, scroll/caret-select the source range through its owner, and optionally emphasize it transiently. The explorer remains open and the Pad source/result layout remains as before. Keep explorer selection/highlights inside the companion; opening or browsing it must not select an Inspector comparison value or modify the source. Handle Pad close/document switch by clearing or marking the old context unavailable, not reopening a retired document or continuing to show old relationships as current. Do not close the Pad when the companion closes.

**Edges/invariants:** Typing in either window, stale navigation after edits, owner switch/reopen, dirty close decisions, already-open explorer, hidden main launcher, Jump Mode, large font/narrow viewport. New callbacks are accepted only for the current host/document context. Navigating the same surviving LineId after an edit may re-resolve against the latest snapshot; it must never jump using an obsolete byte range.

**Verification/acceptance:** UI tests open from docked and popped Pad, retain explorer presentation, browse variable and physical-row dependencies, jump correctly after line insertions, reject deleted/stale targets and preserve source/history/dirty state. Closing the companion leaves the Pad unchanged; closing/reopening or replacing the Pad cannot display the previous incarnation as current. Main query remains usable beside both windows. No hidden-window polling or second evaluator.

**Commit:** `feat(calc-pad): [M8-B] add a separate dependency explorer`.

### M9-A — Independent review, focused acceptance and final handoff

**Objective and ownership:** Review only this Goal's cumulative first-party changes and directly affected documents/tests; update the existing relevant help/README sections and task ledger.

**Implementation tasks and behavior:** Conduct one cumulative independent review, prioritizing presentation write ownership, deferred/retired effects, clear/search cancellation, connector atomicity, label-file validation, long recording timestamps, treemap accounting and Pad navigation identity. Resolve feature-related findings with small same-branch remediation commits and targeted retests. Run a final scoped first-party Clippy pass, preserving documented unrelated baseline warnings and excluding unrelated vendor cleanup. Build the current runnable application once for consolidated native smoke when supported; do not repeat release builds per milestone.

**Edges/invariants:** Missing native capture/input, inaccessible second drive, absent encoders, unrelated historical tests/lints or unavailable push are separately classified. Source inspection and simulated UI tests cannot be reported as actual native interactions. No unrequested next feature starts at completion.

**Verification/acceptance:** Complete H/J below, publish the exact completed versus limited requirement matrix, and verify all intended changes are committed. Record final branch/parent/baseline/checkpoint/final-tip identities and actual push/clean state. The exact last commit becomes the next Goal's required parent. Read the final SHA after the actual final commit; do not create recursive “record my own SHA” commits.

**Commit:** `docs: [M9-A] record utility usability and insights handoff`, plus bounded labelled remediation commits only when needed.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** Section F supplies those boundaries. M0-A is mandatory and its parent is the recorded predecessor tip; all subsequent checkpoints stay on the same branch.

Commit only coherent, checkpoint-verified work. Use the labelled subjects above or equivalent accurate subjects. Do not stage unrelated changes, rewrite/squash prior history, create per-agent/checkpoint branches, or use a merge to move to the next milestone. Attempt an ordinary best-effort push after each checkpoint. Record failure and continue locally without repeated prompts or bypasses. Keep the checkout state and significant SHAs in the ledger.

Small remediation commits may use labels such as `M4-R1` or `M9-R1`; they do not reset the baseline or restart the Goal. A later distinct authorized Goal must create its own branch from this Goal's exact final commit. One final integration of the cumulative chain occurs only at the approved boundary.

## H. Verification and testing strategy

### H1. Automated verification proportional to each checkpoint

Use focused domain, persistence, rendered egui and integration tests for the changed boundaries. Each new test should express one coherent behavioral contract with useful assertion diagnostics; do not impose a repository-wide single-assert refactor. Do not weaken a legitimate test to conceal a regression. Tests that previously required intentionally changed UI layout must be updated to assert the new requested layout and preserved data behavior.

The checkpoint packets define the required cases. The final consolidation must cover these especially important cross-feature interactions:

| Boundary | Required verification |
|---|---|
| Result layout | A seven-word summary, a one-word 1,000-character path, Unicode/newlines, long title, normal/narrow width and enlarged font all remain in their own card; cached and rendered geometry agree. |
| Presentation preference | Explicit Pop Out/Dock, accepted close, cancelled close, startup without auto-open, failed save, stale general Settings save, multiple tools and notes instances. |
| Child open | Popped-out Calc Pad opens Inspector/reference/Dependency Explorer popped out; stored preference is used on direct opening; existing child is focused/promoted without session loss. |
| Clear | Clearing while a worker is running prevents late results and old deferred actions from repopulating/acting; a subsequent search accepts only its own events. |
| Filename mode | Literal leaf-name matching before result admission; path-only/fuzzy negative cases; case and Unicode highlight correctness; truthful backend routing. |
| Diagram layout | Color area directly under Suggestions; separate menu/action rows; pinned independent inspector; narrow/large-font controls reachable; source/camera/history invariants. |
| Connectors | Actual endpoint reassignment, not merely moving the attached object; valid/invalid/cancelled drag; correct anchors, manual-route preservation, single undo, stale-host/document rejection. |
| Label files | Exact bounded roundtrip, overlaps, width mismatch, Unicode, unknown schema, over-limit data, cancelled replacement, stale completion and write failure. |
| Dashboard | Exact slot-marker mapping, duplicate names, tiny cells, conflicts and unchanged placement semantics. |
| Recording | Finite exact boundaries; unlimited beyond 120/655.35/1,000 seconds; no-frame idle path; Stop/target loss; GIF delay splitting; MP4 real type limits; bounded streaming and output finalization. |
| Treemap | Immediate-child accounting, deterministic finite geometry, partial/zero/unknown handling, drill/back with zero extra scans, hidden table, rejected old-generation clicks. |
| Dependencies | Direct/reverse/transitive relations, cycle safety, physical rows and Unicode names, stable LineId navigation after edits, document incarnation rejection, no source/history mutations. |
| Shared focus | Main query clickable/editable with detached tools; parent/child keyboard ownership; approved clipboard/file effects cannot come from a discarded or retired pass. |

### H2. Execution budget and process ownership

- Use **one Cargo/build/test lane** on the user's machine; one active source writer. Bounded read-only planning/review can overlap safely, not source modification or another Cargo job.
- Prefer the existing `fast-dev` profile for local verification. Discover actual test selectors and package ownership, then compile the relevant test target separately, for example `cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-run` where appropriate. Compilation time is **not** subject to the 100-second execution deadline.
- Run selected tests using the repository's existing owned-process workflow and a **100-second maximum execution budget per focused batch**. Read `dev/run_owned_process.ps1` / its shared implementation before invocation. Use `--no-fail-fast` when useful for a small independent batch. Split a genuinely larger focused batch rather than silently remove the deadline. Do not rely on a guessed runner flag or invent a harness unavailable in the checkout.
- For a failed case, reproduce that case and directly affected neighbors. Use at most two focused remediation/retest rounds per concrete failure before reassessing and documenting a precise blocker. This is not permission to leave known blocking correctness bugs unreported or to skip independent work.
- Do not run the full repository suite, repeated expensive release builds, benchmark campaign, screenshot-diff infrastructure project or historical acceptance queues by default. New features must still compile in the normal application build and preserve the ordinary release configuration.
- One final scoped first-party Clippy pass is appropriate: `cargo clippy -p multi_launcher --profile fast-dev --lib --bin multi_launcher --no-deps`, adjusted only to actual package/profile structure. Fix introduced findings; record unrelated baseline categories. Do not reformat/clean vendored crates.
- Use fake clocks for long recordings and deterministic fixtures for filesystem results. Never make a 1,000-second real-time recording a required test.
- Use a task-owned disposable profile/paths for native tests, with side-effecting playback/global next/cancel hotkeys disabled as required by current fixtures. Identify and retire only task-owned processes by PID/path/start identity; drain runner output. Never terminate the user's unrelated launcher/recording to clear a test environment.

### H3. Consolidated native smoke when supported

Build and launch the current executable once after implementation and review remediation. Use the available supported capture/input tools, not guessed coordinates. Capture evidence for these changed flows, reusing one session where practical:

1. **Launcher and Dashboard:** type `mou` and `folder`; inspect card widths/descriptions; resize and enlarge text. Open Dashboard Editor Preview and correlate markers, labels, selection and conflicts.
2. **File Search:** select Filename contains, run positive/path-only negative fixtures, clear completed results, clear during search and rerun. Confirm no old rows return.
3. **Windows:** Pop Out Calc Pad, open Inspector/reference/Dependencies, close/reopen them; repeat with notes list/editor. Restart the isolated application and explicitly reopen to verify remembered presentation without automatic window spawning. Click/type in the main launcher query beside them.
4. **Diagrams:** verify menu versus action rows, Colors under Suggestions, no source/status jump, Visual sidebar collapse/scroll, Align/Distribute, text-bearing endpoint selectors, interactive creation/reconnect and undo. Confirm existing pan/reset/inline-label operations still behave correctly.
5. **Inspector files:** export labels to a disposable file, modify the draft, import with Cancel then confirm replacement; verify the numeric value/mode and Pad text are unchanged.
6. **Recording:** enter 17, exercise a 25-second step, choose 0, start a short task-owned recording and stop it using the existing control/hotkey. Confirm finalized playback for supported GIF/MP4 routes. Long-duration paths are validated with simulated time, not a long native wait.
7. **Folder Size and Dependencies:** scan one disposable nested tree once; link table/treemap selection, drill/back, toggle views and Pop Out. Browse dependency relationships, jump to source, edit/insert a Pad line and confirm current references without changing source merely by navigation.

If the supported capture/input path fails, record the actual failure and allow only one justified refreshed-selection/retry or one evidence-backed correction, not repeated identical attempts per tool. Continue independent code/test work. Distinguish **implemented**, **automatically verified**, **natively observed**, and **not verified** per requirement. An unavailable native environment yields **LIMITED native acceptance**, not fabricated success or an indefinite stop. The user is not responsible for intermediate manual execution; provide a final concise review checklist only at handoff.

## I. Risk management and bounded decisions

| Risk | Mitigation and stop boundary |
|---|---|
| Persistent choice mistaken for live host state | Separate preference from retirement; capture actual presentation before close; test accepted versus cancelled/discarded paths. Do not remove generation checks to make reopening appear fixed. |
| Global host policy only partially wired | Inventory ordinary Dock/Pop Out implementations and child-open call sites. Cover generic/custom families, with explicit legitimate exclusions. Do not claim global coverage based only on Calc Pad. |
| Main query/focus regression | Reuse existing origin/owner fences, one-shot focus requests and root-query regression. Never use global per-frame focus forcing. |
| Grid fix recreates painting/scroll flicker | Keep cached measurement, clipped painting, hit testing and virtualized offsets consistent; test directly affected long/last-row cases. |
| Search clear visually empties but still accepts work | Retire GUI request identity and cancel worker/activity; test delayed events and stale row actions. |
| Native recording limit survives in an encoder | Audit all duration/timestamp uses, including GIF cumulative `u16` buckets and MP4 signed sample fields; simulated large-time tests and correct delay chunks. |
| Long recording consumes unbounded memory | Preserve bounded mailbox, streaming output and staged-file lifecycle; never accumulate all frames. Fail disk/format/resource errors explicitly. |
| Treemap misrepresents partial data | Use accepted immediate-child measurements, label provisional/omitted data, keep exact table/status and generation checks. Navigation never rescans. |
| Dependency navigation targets a different row after editing | Stable LineId plus document incarnation and current source-range resolution; bounded cycle-safe traversal; no stale byte ranges. |
| Imported labels alter unrelated state or overwrite newer work | Bounded atomic parse/validation and replace-only label transaction; explicit replacement confirmation and draft identity guard. |
| UI restructuring grows into engine rewrite | Reuse source/visual renderers, property controls and transactions; keep new geometry/read models narrow. No egui/renderer upgrade or new workspace/docking framework. |
| Historical/native limitations absorb the Goal | Test changed behavior only; record specific unavailable evidence and continue independent work. No automatic replay of old review queues, stale acceptance matrices or unrelated cross-drive bug campaigns. |

Routine implementation details are delegated to Codex. A genuine ancestry/data-loss blocker or unresolved requirement that makes two materially incompatible implementations unavoidable must be reported precisely. An unrelated warning, optional optimization, unavailable push or unavailable native screenshot is not permission to abandon independent approved features. Do not silently reduce scope to the easy items.

## J. Definition of done

Full completion requires:

- All 17 outcomes in C1 are implemented at their actual owning boundaries, including global ordinary-host coverage and both recording encoder paths.
- Explicit defaults in C2 are reflected consistently in UI, persistence, tests and documentation. Old saved actions/settings/documents still open without silent rewriting or loss.
- Targeted tests and appropriate build checks pass. Any unrelated baseline failures/warnings are identified separately; introduced relevant findings are fixed. Independent review closes feature-related findings without repetitive review cycles.
- Visible behavior is inspected natively when the environment permits. If any required native observation is unavailable, record the final acceptance as LIMITED and name the unverified flows; do not equate state tests with physical Windows acceptance.
- No known feature-related blocking defect is omitted from the final report. A genuine unresolved defect means partial/blocked delivery with the unaffected completed work preserved, not “done” with an undisclosed limitation.
- No added periodic hidden-tool work, implicit filesystem search, automatic clipboard overwrite, source mutation through read-only tools, or unexpected document/window restoration.
- All intended changes are committed on the one correct child branch, using M0-A plus same-branch checkpoints and best-effort pushes. Dirty user work remains intact and unstaged.
- Final branch, parent, baseline, checkpoints and exact `final_tip_sha` are recorded truthfully; that final tip is the required next Goal base. No worktrees, helper clones, per-agent branches or intermediate integration steps were used.

## K. Final delivery report

Provide a concise factual report containing:

1. A requirement/checkpoint matrix for FS1/FS2/H1/H2/L1/D1–D5/I1/B1/R1/R2/F1/F2/C1: implemented, automated verification, native observation and remaining limitation.
2. Important implementation choices and relevant changed source paths, particularly presentation precedence/coverage, filename matching/backend handling, label-file schema, recorder duration/timestamps and read-only projection identities.
3. Exact test/build/Clippy commands or recorded runner selectors, results and evidence locations. Clearly distinguish tests actually executed from proposed/manual checks.
4. Review findings and their resolution; unrelated baseline warnings; genuine blockers versus environmental limits. Do not dump raw private paths/source/clipboard data into public reports unnecessarily.
5. Git: `parent_branch`, `parent_tip_sha`, task branch, `baseline_sha`, significant checkpoint SHAs, `final_tip_sha`, current upstream, actual push state and checkout state. State whether any user changes remain untouched.
6. A short manual review checklist grouped by the seven H3 flows, for the user's final review only.
7. Explicit next-Goal handoff: **create the next fresh child branch directly from this `final_tip_sha`; do not restart from master/main; no Git worktrees or intermediate merges are needed or permitted just to advance the chain.**

Stop after this approved Goal and its bounded verification/reporting. Do not start the next optional feature, a general cleanup, a new benchmark effort, or an integration merge without separate authorization.
