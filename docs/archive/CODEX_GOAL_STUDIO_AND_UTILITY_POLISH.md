# Codex Goal — Studio and Utility Polish

## A0. Mandatory Chained Git Branch Bootstrap — Before All Application Work

**First execution checkpoint: M0-A.** Read AGENTS.md, this entire Goal, and CODEX_KICKSTART_STUDIO_AND_UTILITY_POLISH.md. Then perform the following bootstrap before editing application code, running implementation builds/tests, or making implementation checkpoints.

This Goal and its Kickstart describe **one shared, fresh task branch**. Use one stable branch for this Goal, provisionally **feat/studio-and-utility-polish**, with a unique lowercase kebab-case suffix if necessary. Milestones, checkpoints, agents, and the two documents do not receive separate branches.

1. **Inspect safely.** In the single existing repository checkout, inspect git status -sb, current branch, HEAD, remotes, upstream, staged/unstaged/untracked changes, and the authoritative preceding Goal/category handoff. Read relevant queue/execution records without modifying them.
2. **Resolve the required parent.** For a continuing chain, record the immediately preceding completed development Goal/category's branch name and final committed tip as parent_branch and parent_tip_sha. The new branch must start directly at that exact SHA. Only the first Goal in a genuinely new chain defaults to the up-to-date primary integration branch. Never silently return to master/main between Goals.
3. **Use the current handoff, not an assumption.** At planning time, the predecessor is codex/launcher-utility-fixes at d691888584d531d9167f035dd48e47d15dced483. Its implementation is terminal LIMITED because some native checks remain unverified. Master at 838f8a42ff4c938a314065d5d3b79f3ac5425195 subsequently merged that tip. That merge does not replace the predecessor-tip rule. Confirm the current records and refs before using this candidate; if a newer completed category exists, use its verified final tip instead. Historical timestamps in STATUS.md do not override current refs. Do not guess when records conflict.
4. **Fetch only when needed and safely.** If the required predecessor exists only remotely, fetch its ref and verify its exact tip. If the required ancestor exists locally but its earlier push failed, use the actual local ancestor. On another machine where that commit is absent, report the precise missing-ancestor blocker instead of using a stale remote tip.
5. **Protect existing work.** Never automatically reset, hard-pull, stash, rebase, merge, force-checkout, overwrite files, or stage unrelated changes to manufacture a clean transition. If local changes make selecting the required parent unsafe, stop at this prerequisite with a bounded blocker. For the first task of a genuinely new chain only, a documented current-HEAD base is allowed when safe and no predecessor exists.
6. **Create and switch to the fresh child.** Use the equivalent of git switch -c feat/studio-and-utility-polish <parent_tip_sha> after the preceding checks. Verify active branch and initial HEAD. Do not implement on the predecessor or on master/main.
7. **Create the new branch's own M0-A baseline commit.** Prefer the task-specific ledger proposed below, staging only intentional task files. An intentional empty commit is acceptable: git commit --allow-empty -m "chore: [M0-A] establish studio and utility polish baseline". Record baseline_sha separately from parent_tip_sha. Verify that git rev-parse <baseline_sha>^ equals parent_tip_sha; the baseline must be its direct child.
8. **Attempt one ordinary initial upstream push.** Use the configured upstream or git push -u origin HEAD when origin is verified. Never force-push. If access, authentication, network, approval, or remote state prevents the push, record the exact result, preserve the local commit, and continue safe local implementation. Do not enter retry loops or bypass restrictions.
9. **Keep the chain linear.** All subsequent checkpoints stay on this same task branch. No intermediate merge, rebase, squash, or cherry-pick is permitted merely to advance the Goal/category sequence. Do not create independent branches from a common master base.
10. **Hand off the final tip.** At completion, record parent_branch, parent_tip_sha, baseline_sha, task branch, significant checkpoint SHAs, final_tip_sha, actual push status, and checkout state. The next Goal must create its fresh child directly from this Goal's final_tip_sha. The final branch in a sequence already contains cumulative work; any final integration is a separate approved boundary, not an intermediate task step.

**Single checkout; no Git worktrees.** Do not run git worktree add, move, or prune, create auxiliary checkouts, or create secondary/agent-specific clones. All source writes, Cargo jobs, Git mutations, and branch handoffs occur sequentially in the existing checkout. Read-only investigations/review may run in parallel when useful.

**M0-A acceptance:** the new branch is unique and active; the predecessor and exact parent are verified; the baseline is a direct child; initial push outcome is recorded; no existing user work was staged, lost, or moved; no worktree or parallel implementation branch was created. A predecessor's last commit is not this Goal's baseline.

Proposed execution ledger: **docs/plans/execution/standalone/studio-and-utility-polish/STATUS.md**. This is a new task-owned path, not an assertion that it already exists. Use the current authoritative queue ledger instead if this Goal is already being managed by one; avoid competing ledgers.

## A. Goal / Desired Outcome

Improve the everyday editing and inspection workflows in Diagram Studio, Programmer Inspector, Calc Pad, and Folder Size:

- Visual diagrams have a substantially larger, adjustable canvas, compact document controls, a reliable reset-to-content gesture, and ergonomic left-drag panning in the Visual Document viewer.
- Mermaid editing keeps Tab inside the source editor, keeps the editor stationary when preview status changes, exposes conventional menus and useful in-app help, and supports persistent colors across the existing templates and chart families.
- Programmer Inspector presents recognizable groups of bits, uses space efficiently, and compares typed Calc Pad row references or variables without requiring a result-selection step.
- Calc Pad removes the default comparison button from every result and copies expressions together with their results.
- Folder Size reveals the correct right-clicked target across Windows volumes and renames the working toolbar action to “Reveal in Explorer.”

Successful delivery includes implementation, focused verification, independent review, coherent checkpoint commits, best-effort pushes, and one final user review checklist. Continue through all independent work even if a particular native check is unavailable; report that check truthfully rather than declaring full acceptance.

## B. Current Codebase Findings

Planning reference: repository **multiplex55/Multi_Launcher**, master commit **838f8a42ff4c938a314065d5d3b79f3ac5425195**, inspected on November 8, 2026. Recheck relevant locations on the correctly bootstrapped execution branch. Source paths and symbols below are existing unless explicitly marked proposed.

| Area | Current implementation and consequence |
| --- | --- |
| Visual canvas size | src/gui/diagram_studio.rs, draw_visual_body, allocates a 340-point-high canvas and caps width at 1,400 points. Enlarging the window does not substantially enlarge the editor canvas. |
| Visual title and recents | draw_visual_body places the full-width single-line title after draw_recent_documents. That helper owns both the header and expanded content, so the header/body responsibilities need a small separation. |
| Camera and reset | Camera::default starts at zoom 1 with fit_next enabled. fit_camera uses projected content bounds. handle_visual_canvas_camera_input already owns wheel zoom and middle pan; there is no double-middle reset. |
| Visual viewer drag | draw_visual_viewer_body currently creates viewer_preview_drag for primary drags. The accepted effect deliberately moves the viewer host. The requested Visual-only pan behavior changes that existing contract. |
| Mermaid editor | source_editor uses code_editor() followed by lock_focus(false). The vendored egui TextEdit already supports Tab insertion and Shift+Tab indentation reduction when focus is locked. |
| Preview status | Direction is on a horizontal row, followed by conditional pending/stale/error labels before the source workspace. Those labels change the editor's vertical position. |
| Diagram action routing | draw_body already uses DocumentIntent, PendingMutation, MenuFocusScope, menu_focus_request_for_ui, and accepted host/generation checks. Import/export/template menus exist; common document actions are still separate buttons. |
| Supported Mermaid scope | src/diagram/templates.rs defines 13 DiagramTemplate entries covering 11 families. Native flowcharts use mermaid.rs/model.rs/session.rs/scene.rs. Ten other families use rendering.rs and the pinned mermaid-rs-renderer 0.3.1 dependency. |
| Current color limits | RenderOptions exposes Modern and MermaidDefault only, without a user-facing persisted theme control. Native DiagramNode has no color fields. Family validators reject init directives/frontmatter. Native painters contain fixed colors. A picker alone cannot provide persistent color support. |
| Renderer constraints | The pinned dependency has theme presets and color fields, including MindmapConfig palettes, but some specialized rendering uses fixed colors. Qualify the actual path for each supported family; a successful render alone does not prove that a color choice was honored. |
| Comparison engine | src/calculation_pad/comparison.rs, ComparisonContext::compare, already accepts ComparisonSlot::Expression and ComparisonSlot::Result. document.rs::evaluate_comparison_expression resolves names and physical rN references. Reuse this evaluator. |
| Comparison ambiguity | InspectorDraft stores visible strings alongside optional selected-result references. Compare prefers the references; Use selected result does not replace the visible text. Editing a field clears its reference. This permits a displayed r2 to represent a selected r1 instead. |
| Bit grid | src/gui/programmer_inspector_dialog.rs uses 58-point cells labeled like “63: 0,” a small grouping gap, and a 220-point scroll cap. This produces very few bits per row in a narrow window. |
| Calc Pad actions/copy | results_ui displays Select for comparison beside successful rows. copy_all_results emits row/result summaries without source. SessionLineResult already has source_range and physical line_index; accepted copy resolution occurs after same-frame source edits. |
| Folder Size routes | The toolbar Open action resolves directories to open_path/open::that. Context reveal validates a retained FolderSizeRevealRequest and calls shared file_search::actions::reveal_path, which launches Explorer using a single /select,... argument. The source does not prove drive-letter loss. |

Important source anchors are linked in Section L. These are findings from inspection, not claims that native Windows behavior or new acceptance tests have already passed.

## C. Approved Functional Requirements

Requirement IDs are stable traceability labels, **not execution priority**. The user explicitly permits rearranging work to improve implementation speed or dependency order.

### Visual Diagram Studio

- **V1 — Reset gesture:** Double middle click inside a Visual Document canvas restores the existing initial fit-to-content presentation using all current projected content bounds. Apply in the editor and Visual viewer, including imported draw.io content. Empty documents return to a finite stable default. This does not mean fixed 100% zoom or a new “largest cluster” algorithm.
- **V2 — Larger canvas:** Make the editor canvas adjustable and able to use substantially more of an enlarged host. Remove the fixed-height bottleneck; preserve a practical minimum and access to lower controls. Increasing one hard-coded size is insufficient.
- **V3 — Compact header alignment:** Place the Visual document title field beside the Recent diagrams header at ordinary widths, with expanded recent entries beneath.
- **V4 — Bounded title width:** Cap the title field at a sensible width consistent with existing typography. Permit graceful wrapping in narrow hosts and horizontal editing of long titles; never truncate stored title text.
- **V5 — Visual viewer primary pan:** Left-button dragging inside the Visual Document viewer pans the camera, as middle dragging does. It must not start native window movement or move the embedded dialog. Normal window chrome remains draggable. The editable Visual Studio keeps its existing left-button object selection/movement behavior.

### Mermaid Diagram Studio

- **MD1 — Editor Tab:** Tab while the Mermaid source editor has focus inserts indentation and keeps focus in the editor. Use the existing TextEdit behavior; retain Shift+Tab support already provided by egui. Preserve ordinary Tab navigation outside that editor.
- **MD2 — Stable status row:** Put a compact preview-status area beside Direction, with reserved stable height. Ready, pending, stale, and error transitions must not move the source editor. Full diagnostics remain accessible without conditionally inserting a new row above the editor.
- **MD3 — Real color support:** Add usable presets and editable colors for all current templates/families. Include native flowchart node fill/border/text colors and mindmap root/branch palettes. Colors must actually affect preview and export, survive save/open and undo/redo, and be represented in the document/source rather than only in dialog runtime state.
- **MD4 — In-app help:** Provide Help with examples, syntax, indentation, colors, useful operations, and limits for the currently supported chart families. Reuse working templates and qualified examples; distinguish editable native flowcharts from source-rendered families.
- **MD5 — Menus:** Consolidate common actions into conventional File/Edit/View/Templates/Help menus or equivalent clear grouping. New, Open, Save, Save As, Close, Import, and Export belong under File. Preserve enablement, focus behavior, shortcuts, and dirty-document decisions.

### Folder Size

- **F1 — Correct cross-drive reveal:** The context menu must reveal the exact folder/file that was right-clicked, including when the scan target is on another drive from the launcher. For a normal folder, reveal means select that folder in its actual parent. A drive/share root opens as a root. Preserve full native absolute paths.
- **F2 — Toolbar wording:** Rename Open to **Reveal in Explorer**. Preserve the toolbar's currently working directory-opening behavior; this request does not require converting that separate action into parent-folder selection.

### Programmer Inspector

- **P1 — Visual bit groups:** Show clear monospace 0/1 cells, unmistakable group boundaries, and group-level indices resembling the readability of Windows Calculator. Retain exact individual hit targets/index tooltips. Default 64-bit/nibble presentation should fit 16 bits per row at an ordinary approximately 500–720-point width when typography permits. Adapt at narrower widths or larger text scale.
- **P2 — Typed comparison:** Type r1 and r2, or named variables, then Compare / Refresh without selecting an integer or using a Pad comparison button. Visible input must truthfully represent the source being compared. Preserve existing scalar, exact integer, and quantity semantics.
- **P3 — Compact layout:** Keep working value, mode/sign, bits, and essential actions prominent. Use collapsible Formatting and Bit labels sections; make Compare two values easy to find and initially expanded. Keep the entire dialog reachable at its supported minimum size, with drafts and section state retained across Dock/Pop Out.

### Calc Pad

- **C1 — Result actions:** Remove Select for comparison from always-visible result rows. Retain the optional shortcut in a result context menu if needed for existing selected-result semantics. Preserve Inspect bits…, per-result Copy, and the general Programmer Inspector… toolbar entry.
- **C2 — Complete Copy All:** Copy one plain-text report containing the following three labeled sections in source order. Exact punctuation can follow existing conventions; all three content forms are required:

~~~text
Source and results
r1 = 1 + 2 = 3
r2 = r1 * 4 = 12

Results only
r1 = 3
r2 = 12

Source and results without row labels
1 + 2 = 3
r1 * 4 = 12
~~~

The last section removes only the report's leading row labels, never genuine rN references inside expressions. Preserve complete assignment expressions, Unicode, actual physical row numbers, comments/blank-line structure in source sections, and explicit calculation errors. Results-only may omit non-result comments/blanks while preserving original row numbers. Per-result Copy remains result-only.

## D. Scope and Behavioral Invariants

Implement these requirements through existing owners. This is one cohesive UI/utility Goal, not an application-wide redesign.

- Keep launcher hotkeys, main query focus, plugin prefixes/routing, and unrelated utilities unchanged. Exercise the main query beside affected popped-out dialogs only as a focused shared-host regression check.
- Keep document identity, revision, accepted/retained-pass, owner-generation, and host checks for mutations, clipboard effects, file operations, and renderer publication. Menu moves and compact layouts do not bypass those checks.
- Camera changes are presentation-only: no content revision, dirty state, undo entry, object movement, or selection reset. Viewer and editor cameras remain independent.
- Only the Visual Document viewer changes from primary window-drag to primary camera-pan. Preserve the current behavior of the other Mermaid viewer paths unless an inseparable shared defect is demonstrated.
- Keep existing source text, content limits, document open/save conflict handling, recovery, and dirty-transition decisions. Old unstyled diagrams and existing Calc Pad documents must still open.
- Preserve exact integer widths 8/16/32/64/128, signedness, checked casts, explicit reinterpretation, shifts/rotates, annotations, and private Inspector edits. Do not route typed integer bits through f64.
- Comparisons remain read-only against one current ComparisonContext. No background continuous comparison loop. Insertion still requires the current Pad selection/caret identity and remains undoable.
- Preserve Folder Size scanning limits, cached sorting/filtering, root handling, and stale target refusal. If the shared reveal helper changes, review directly affected consumers rather than rewriting file operations.
- No new workspaces, plugin framework, global UI framework, browser/WebView, remote Mermaid service, custom expression evaluator, broad parser rewrite, dependency upgrade campaign, performance benchmarking, full regression campaign, or release-profile changes.
- Explorer Tabs' AmbiguousShellBindings preflight belongs to a separate subsystem. It is not the Folder Size reveal route and is not an additional requirement of this Goal. Do not relax its identity checks.

## E. Technical Implementation Strategy

### E1. Group shared changes and keep one writer

Recommended order is Folder Size, the combined Calc Pad/Inspector work, Diagram shell/navigation, then Mermaid styling/help. This groups shared GUI edits and lets the more substantial style work build on settled menus and layout. Independent feature groups may be reordered after M0-A if the planner records why. Preserve the dependencies in Section F and the stable checkpoint IDs.

Use configured planner/implementer/reviewer roles. The parent coordinates state, the single Cargo lane, commits, and pushes. The repository currently configures these roles under .codex/agents; follow their current configuration rather than hard-coding older model preferences.

### E2. Comparison input has one truthful authority

Reuse ComparisonSlot and ComparisonResultRef. Represent each UI side unambiguously as typed expression or explicit selected-result input, with a small private state adjustment if needed.

- Typed text is exactly what is evaluated. An edit removes that side's selected-result binding.
- Use selected result updates the visible field to the selected row's current physical rN and may show a compact “Selected result” badge. It cannot leave unrelated old text visible.
- Preserve stable LineId semantics for that optional selected-result mode. If rows move, derive its current displayed row/name from ComparisonContext. If removed or reopened, show unavailable; never silently bind to a coincident new row.
- Ordinary typed rN remains a physical row reference, as it is today.
- Retain the current input bound, document incarnation checks, explicit refresh/staleness, type/unit errors, and immutable comparison snapshot. Do not gate comparison on having adopted an integer for bit inspection.

### E3. Copy through the accepted effect path

Extend the current copy formatter using CalculationPadSession::source and SessionLineResult::source_range, resolved from the same accepted revision as the evaluation. Preserve accepted-copy processing after same-frame edits. Reuse CalculationValue::formatted_result and existing clipboard dispatch. Do not write to the clipboard from a rendering closure or reconstruct expressions from their calculated values.

### E4. Keep Diagram interaction context explicit

Separate the recents header from its expanded body, use bounded title sizing, and choose an adjustable/flexible canvas region within the existing host. Reuse fit_camera and existing zoom bounds.

Parameterize the existing camera input boundary by the allowed pan buttons/context. Primary pan is allowed in a Visual viewer, not universally in editable canvases. A double-middle reset takes precedence over any same-frame pan delta. In the Visual viewer, reuse the full existing Fit action semantics, including restoring visual_viewer_fit_follow and its fit-size bookkeeping, so later resizing behaves like the initial presentation until another manual pan/zoom. Scope gestures to a press that originated on the visible canvas, excluding minimap and toolbar controls, with the existing accepted-host/generation protections.

Use the existing TextEdit focus-lock feature for Tab. Reorganize actions through existing DocumentIntent/PendingMutation paths and MenuFocusScope. Preserve the shared pending-decision surface above scrolling content.

### E5. Source-authoritative, bounded Mermaid styling

Implement a small typed color/style layer in the diagram domain. Prefer existing serde/serde_json, validated color types where suitable, and renderer integration. Do not expose arbitrary renderer configuration.

1. **Source representation.** Support a documented, bounded color-only JSON init directive using ordinary Mermaid syntax and standard color keys where available, plus a bounded native flowchart direct-node style subset for fill, stroke, and text color. For example, a supported style statement can express the three colors of an existing node. The implementation must validate node IDs and values; it must not treat arbitrary CSS as supported. If a required family palette needs a renderer-specific key, document that key explicitly in Help rather than promising identical support in other Mermaid engines.
2. **Presets and defaults.** Preserve the current appearance for documents without explicit styling. Offer Mermaid default, dark, forest, and neutral presets, with custom hex colors. An “App default” action may remove the explicit override. Reuse the pinned renderer's presets; do not advertise a renderer-only name as portable Mermaid syntax.
3. **Single parsing/admission boundary.** Extract and validate the recognized color envelope before family admission. Keep family detection, original diagnostic locations, strict body coverage, and byte/line/nesting limits correct. Existing blanket directive rejection must become narrow acceptance of the supported colors, not blanket acceptance of arbitrary init/frontmatter. Unsupported configuration remains preserved with a useful diagnostic.
4. **Native flowcharts.** Carry styles through the native parser/formatter, typed graph/session semantics, scene construction/painting, and SVG/PNG export. Update same_graph_content, semantically_equivalent, and equality/revision decisions so a color-only edit cannot disappear as a no-op. Preserve recognized styles through later moves, renames, and structural edits. The existing conversion.rs::mermaid_visual_copy currently assigns default VisualStyle values; carry new effective node fill/border, owned-label text, and edge colors into the corresponding existing VisualStyle fields so conversion does not silently lose them. This is a small preservation change, not a new conversion mode or layout effort. Do not force a colored native flowchart into a source-only mode merely to avoid implementing its style model.
5. **Generic families.** Map the typed color model into actual Theme/LayoutConfig fields. Mindmap controls include bounded root and branch fill/text/edge palettes. Expose only controls that materially affect that family; presets must affect meaningful diagram colors across all current families.
6. **Hard-coded renderer colors.** Verify each family against the pinned 0.3.1 implementation. Where fixed colors prevent the approved behavior, a narrowly scoped patch of that existing dependency is justified using this repository's established vendor/[patch.crates-io] convention. A proposed vendor/mermaid-rs-renderer path is new, not pre-existing. Limit the patch to color configuration and rendering consumption; retain version, license, defaults, layout, parser behavior, and unrelated dependency behavior. Do not substitute broad SVG string replacement, a renderer upgrade, or a new rendering engine.
7. **Persistence and identity.** The source carries user color choices. Keep old document decode compatibility and bounded history; any added graph/style fields need default handling and appropriate codec compatibility. Save/reopen, undo/redo, source export, preview, viewer, and image export use the same effective colors. Derived style caches must be invalidated by accepted document identity/revision and all relevant options.
8. **Accepted mutations only.** A palette/preset edit creates one accepted source/document edit and one meaningful undo step. Repeated render frames must not rewrite the source. Preserve unsupported drafts; do not delete unknown syntax or overwrite a dirty document when applying templates or help examples.
9. **Asynchronous rendering.** Reuse the current debounce, worker, cache, cancellation, and publication fencing. A stale result from an earlier palette or a retired document must never be labeled current.

Default minimum color coverage is theme/palette customization across all 11 supported families, individual node fill/border/text for native flowcharts, and root/branch colors for mindmaps. Per-object color editors for every other chart family and full Mermaid.js/CSS/frontmatter parity are not required.

#### E5.1. Actual generic-family color mapping

The following paths were inspected in renderer 0.3.1 at commit 2f993bd79a55235eb59a34d807852276ba25bea7. Treat the palette as a bounded typed model and map user-facing controls to the field that actually paints the relevant shape, rather than assigning primary_color indiscriminately.

| Family | Meaningful color inputs and required handling |
| --- | --- |
| Mindmap | LayoutConfig.mindmap section_colors, section_label_colors, section_line_colors, root_fill, root_text, edge_color. Explicit presets must update these independent palettes; Theme alone leaves branch colors unchanged. |
| Sequence | Theme.sequence_actor_fill/border/line, message line_color, and primary_text_color. Preserve the currently admitted participants/messages subset. |
| Class | Primary fill/border/text and line_color. Theme controls are sufficient; a new per-class object editor is not required. |
| State | Primary fill/border/text, line_color, and background. Preserve start/end marker meaning. |
| Entity Relationship | Header cluster_background, body background, primary_border_color, cluster_border, and primary_text_color. A primary_color-only picker would not recolor the actual header. |
| Gantt | Current sectioned task bars derive from primary_border_color; text/axis use their actual theme fields. Expose these working controls. Preserve critical/done/active/milestone meanings; a general sectionless-bar palette editor is outside this bounded contract. |
| Timeline | Background/outline/text are theme-driven, but event-card fills are fixed. Implement the narrow configurable event-card palette patch below. |
| User Journey | Primary task-card colors, git_colors for actor/section palettes, and line/cluster colors. Preserve the semantic red-to-green score scale. |
| Pie | pie_colors plus pie title/section/legend text and outer-stroke fields. Set them before layout. Slice separators currently consume background; do not expose a nonfunctional pie_stroke_color control. |
| GitGraph | git_colors, git_inv_colors, git_branch_label_colors, commit/tag text/background/border fields. Verify a branch and its label, not only surrounding chrome. |

**Required Timeline patch:** In the pinned renderer, TimelineConfig has direction but no palette, and render_timeline uses a fixed six-color array for event cards. Add a small configurable event-card palette at that existing boundary, retaining the existing six colors as the no-override default and safe fallback for an empty palette. A field name such as event_colors is proposed, not an existing API. Route the launcher adapter's bounded explicit palette into that field. Preserve all event layout/geometry and ensure preset text remains readable against card fills. Retain the original dependency license/provenance and document this small vendor patch.

Verify Timeline changed fills, unchanged unstyled defaults, and unchanged geometry. For the other families, verify representative real shape colors through the production adapter: sequence participant, ER header, pie slice/legend, GitGraph branch, Gantt task, and mindmap root/two branches. This is a bounded data-driven fixture over existing supported families, not a broad renderer parity campaign.

### E6. Help is a tested part of the product

Provide a local Help surface reachable from the Studio menu. Organize by the 11 existing families and reuse DiagramTemplate examples. Include copyable examples, syntax essentials, indentation, the implemented color subset, current capabilities/limits, and the meaning of stale preview. Reuse current document intents if an example can be opened, protecting unsaved work. External official documentation can be an additional link; the in-app help must work without it.

## F. Detailed Milestones and Checkpoints

| Checkpoint | Deliverable | Requirements | Dependency |
| --- | --- | --- | --- |
| M0-A | Exact-parent child branch, own baseline, initial push receipt | Git contract | First, mandatory |
| M1-A | Cross-drive reveal repair and toolbar wording | F1–F2 | M0-A |
| M2-A | Truthful typed comparison and cleaner Pad result rows | P2, C1 | M0-A |
| M2-B | Readable grouped bits and compact Inspector | P1, P3 | M2-A |
| M2-C | Complete Copy All report | C2 | M2-A preferred; same Pad owner |
| M3-A | Diagram menus, stable status, Tab, compact header | V3–V4, MD1–MD2, MD5 | M0-A |
| M3-B | Larger Visual canvas, reset, Visual viewer pan | V1–V2, V5 | M3-A |
| M4-A | Native/source style model and persistence | MD3 foundation | M3-A |
| M4-B | Generic-family palettes and actual renderer support | MD3 foundation | M4-A |
| M4-C | Color controls, colored templates, in-app help | MD3–MD4 | M4-A/B |
| M5-A | Consolidated smoke, independent review, final handoff | All | All safe implementation complete |

Ordinary checkpoints do not create new branches. M1, M2, and M3 groups can change relative order; M4-A/B must precede publishing M4-C color controls. Do not stop after a subset of requirements or wait for user approval between successful checkpoints.

### M1-A — Folder Size reveal correctness

**Objective:** Make context reveal reach the exact cross-drive target and apply the requested toolbar wording.

**Code ownership:** src/gui/folder_size_dialog.rs; src/gui/render.rs; src/file_search/actions.rs. Inspect shared reveal consumers only if changing that helper.

**Implementation tasks:**

1. Reproduce with a small disposable target on another existing volume when available. Trace the retained full path through RevealRow, FolderSizeRevealRequest, take_reveal_path_for_dispatch, and reveal_path.
2. Compare with the working OpenRow/OpenPath route. Determine whether failure occurs in target resolution or Windows shell invocation; do not assume drive-letter loss or extended-prefix normalization is the cause.
3. Repair the first responsible boundary. If the correct absolute path reaches the shared reveal helper, fix that helper once. A suitable existing/native Shell selection API is acceptable when justified, with correct handle/COM ownership and without blocking the UI.
4. Rename the toolbar label, preserving its working open-directory semantics.

**Behavior:** A right-clicked ordinary folder is selected in its actual parent; roots open correctly; missing/inaccessible targets produce visible errors.

**Edge cases:** Other drive, same-drive control, spaces/Unicode, drive root, stale selection, and targets removed after scanning. Preserve UNC/native path representation when touching shared path handling; no network-share provisioning is required.

**Regression constraints:** Keep original-row mapping after sort/filter, host/publication/generation checks, discarded-frame refusal, and existing scanner limits. A process spawn is not proof that Explorer selected the target.

**Verification:** Reuse targeted Folder Size retained-reveal tests and file_search::actions path/action tests. Extend argument/dispatch coverage only for the actual fix. Perform the native cross-drive interaction when supported.

**Acceptance:** Correct target observed natively when available; focused route tests pass; new wording visible; no weakening of target checks. If native proof is unavailable, record it as unverified and continue other milestones.

**Commit boundary:** fix(folder-size): [M1-A] correct Explorer reveal and clarify toolbar action.

### M2-A — Comparison authority and Pad result actions

**Objective:** Make typed references the ordinary, trustworthy comparison workflow.

**Code ownership:** src/gui/programmer_inspector_dialog.rs; src/gui/calculation_pad_dialog.rs; existing comparison/session APIs and affected tests.

**Implementation tasks:** Apply E2; make optional selected-result bindings visibly truthful; remove the always-visible row action and retain the shortcut in the row context menu; keep Inspector entry points and per-row Copy/Inspect bits.

**Behavior:** With source 123 followed by 345, typed r1/r2 compares 123 and 345 with a difference of -222. Named variables work the same way. Comparison needs a current Pad context, not an integer selection.

**Edge cases:** Type r2 then choose selected r1; the field must visibly change. Edit it back to r2; the binding must disappear. Stable selected rows follow insertions; removal/reopen shows unavailable. Unknown/duplicate names and incompatible units retain meaningful errors.

**Regression constraints:** Preserve exact integer and quantity behavior, immutable ComparisonContext, current source/caret ownership, no Pad edits from comparison, and no automatic idle reevaluation.

**Verification:** Existing comparison domain tests plus a focused real-UI/accepted-effect test for the hidden-reference/display mismatch. Include 10 ft versus 120 in without an integer selection.

**Acceptance:** Typed-only workflows and selection-to-typing transitions pass; default result rows are less cluttered; source/undo state remains unchanged by comparison.

**Commit boundary:** fix(calc-pad): [M2-A] make comparison inputs explicit and simplify result actions.

### M2-B — Compact, visually grouped Programmer Inspector

**Objective:** Make bit patterns easy to scan and the dialog practical at ordinary sizes.

**Code ownership:** src/gui/programmer_inspector_dialog.rs; utility_window_host.rs only if an actual size/scroll defect requires it.

**Implementation tasks:** Replace repeated “index: value” cell labels with 0/1 cells, group-level indices and clear gaps/boundaries; size rows by actual available width and text metrics; introduce collapsed Formatting/Bit labels and an easily reached, initially expanded comparison section.

**Behavior:** Default 64-bit grouping of four is visually recognizable and normally uses 16 bits per row. Mode/sign/value and core operations remain discoverable. Section state and working drafts survive host moves.

**Edge cases:** 8 through 128 bits, groupings larger than a row, narrow hosts, enlarged text, long annotations, and 128-bit highest/lowest positions.

**Regression constraints:** Every bit appears exactly once in MSB-to-LSB order and maps to its original index. Grouping changes never change the value. Preserve checked cast versus reinterpretation, signed shifts, labels, private edits, and explicit insertion.

**Verification:** Adapt existing grid/annotation/host-retention tests to the new behavior. Check representative narrow and normal layouts and a real Dock/Pop Out interaction; do not require pixel-perfect replication of Windows Calculator.

**Acceptance:** Groups and indices are legible; all bits/actions remain reachable; toggles and copied bases stay exact; no clipped lower controls at supported minimum size.

**Commit boundary:** feat(inspector): [M2-B] group bits visually and compact the layout.

### M2-C — Source-and-result Copy All

**Objective:** Produce the three-section report specified in C2 through the existing clipboard effect path.

**Code ownership:** src/gui/calculation_pad_dialog.rs; use existing session/evaluation ranges. Add a small accessor only if needed to obtain matching source and evaluation safely.

**Implementation tasks:** Format actual source ranges beside formatted outcomes; retain physical row numbering and assignments; add results-only and no-leading-row-label sections; keep single-result Copy unchanged.

**Behavior:** The clipboard contains the complete report for the current accepted revision, including meaningful errors and original expressions.

**Edge cases:** CRLF, Unicode/braced names, source containing rN, comments/blanks, trailing blank line, vectors/quantities/exact integers, failed lines, and empty document.

**Regression constraints:** Source and results must be from one revision even when an edit and copy occur in the same frame. Copy must not modify source, dirty state, undo, selection, or evaluation semantics.

**Verification:** Update existing copy-format tests and accepted_copy_resolves_after_same_frame_source_edit. Assert whole representative report content and preserved per-result behavior; avoid repetitive tests of each formatting line.

**Acceptance:** All three sections are present and correct; row references inside source remain intact; source/result pairing is current and clipboard publication remains accepted-host fenced.

**Commit boundary:** feat(calc-pad): [M2-C] copy complete expressions and results.

### M3-A — Diagram shell and source editing

**Objective:** Consolidate document controls, reclaim space, and stabilize text editing.

**Code ownership:** src/gui/diagram_studio.rs, specifically draw_body, draw_visual_body, draw_recent_documents, source_editor, and existing menu/focus helpers.

**Implementation tasks:** Build conventional menus through current intents; separate recents header/body and bound the Visual title; place stable preview status beside Direction; enable TextEdit Tab locking. Establish the Help menu entry for M4-C content.

**Behavior:** Tab indents source, preview changes do not move the editor, and New/Open/Save/Save As/Close are available under File in the appropriate modes.

**Edge cases:** Invalid source, pending/stale generic renders, long diagnostics, narrow host, long document title, open popup during host transition, and unsaved documents.

**Regression constraints:** Keep application-owned undo, accepted source mutations, menu keyboard focus/Escape restoration, enabled states, source cursor restoration, and the existing top-level dirty-document decision surface.

**Verification:** Extend actual source-focus/menu tests, add a focused Tab edit/undo assertion and a source-rectangle stability assertion across status transitions. Check the compact header at ordinary and narrow widths.

**Acceptance:** Required actions are reachable with current semantics; Tab does not focus sibling controls; stale/error transitions keep the source position stable; title data is preserved.

**Commit boundary:** feat(diagram): [M3-A] simplify menus and stabilize source editing.

### M3-B — Visual canvas sizing and navigation

**Objective:** Give the editor usable space and make Visual viewing ergonomic.

**Code ownership:** draw_visual_body, draw_visual_viewer_body, Camera/fit_camera, handle_visual_canvas_camera_input, and the Visual-specific viewer drag effect path.

**Implementation tasks:** Replace fixed canvas sizing with an adjustable/flexible region; add double-middle fit-to-content; allow primary camera pan only in the Visual viewer; suppress Visual preview-generated window drag.

**Behavior:** Enlarging the host can substantially enlarge the canvas. Double middle click brings content back into view. Left drag pans a Visual viewer while normal chrome moves its window.

**Edge cases:** Empty scene, imported draw.io diagram, distant objects, maximum/minimum zoom, resize after manual navigation, drag beginning outside canvas, minimap interaction, and same-frame host/owner transition.

**Regression constraints:** No document edits from navigation; editor/viewer cameras independent; editor primary object movement preserved; existing middle pan/wheel/minimap retained; untouched Mermaid viewer drag behavior preserved.

**Verification:** Extend existing camera/resize tests and intentionally migrate the Visual branch of viewer-drag tests. Add a representative accepted/discarded primary-pan/reset check, then perform a real visual smoke when possible.

**Acceptance:** Gesture changes the camera rather than host position; reset restores current content; canvas uses available space; dirty/revision/undo/object state remains unchanged.

**Commit boundary:** feat(diagram): [M3-B] enlarge visual canvases and improve viewer navigation.

### M4-A — Native Mermaid styling and document semantics

**Objective:** Establish source-authoritative colors and native flowchart node styles.

**Code ownership:** src/diagram/mermaid.rs, model.rs, session.rs, codec.rs, scene.rs, export.rs, conversion.rs; small proposed style helpers within src/diagram as needed; native painting integration in the GUI.

**Implementation tasks:** Implement E5's bounded color-envelope parsing/serialization and direct node fill/stroke/text subset; carry typed values through native graph edits, semantic equality, scene painting, source output, history and codec compatibility.

**Behavior:** Changing a native flowchart node's color visibly changes it while retaining native graph editability. Theme and node overrides survive save/reopen and undo/redo.

**Edge cases:** Missing target ID, invalid hex, duplicate/conflicting style declarations, unsupported directives, Unicode labels, existing unstyled files, color-only edit, and malformed draft with a retained last-good graph.

**Regression constraints:** Preserve topology/layout/IDs, original unsupported source, bounded parsing/history, dirty transitions, and existing default appearance. No silent stripping of unknown syntax.

**Verification:** Focused parser/formatter and session/codec cases; one native scene/export color assertion; representative old-document round trip; extend the existing Mermaid-to-Visual copy test for representable colors without changing geometry/source. Check that style-only changes invalidate the right derived state.

**Acceptance:** Supported styles are explicit, persistent, undoable, and visible in preview/SVG/PNG; old files decode; unsupported input is preserved and diagnosed.

**Commit boundary:** feat(diagram): [M4-A] persist native Mermaid themes and node colors.

### M4-B — Qualified colors for source-rendered families

**Objective:** Make theme/palette choices work across every current generic family.

**Code ownership:** src/diagram/rendering.rs and shared styling admission; existing mermaid-rs-renderer dependency integration. A focused vendor patch/Cargo patch entry is permitted only where actual fixed colors require it.

**Implementation tasks:** Map validated colors to Theme/LayoutConfig, including mindmap palettes; update family detection/coverage around the admitted directive; qualify the ten families; repair only renderer color boundaries that otherwise ignore the approved settings; preserve cache/worker identity.

**Behavior:** A changed palette produces changed meaningful diagram colors for each existing family, consistently in preview, viewer, and exports.

**Edge cases:** Mindmap palettes/root/edge overrides, specialized family fixed colors, source change while rendering, rapid palette changes, stale completion, invalid directive, and reopen with equal numerical revision but new identity.

**Regression constraints:** No new chart families, unrestricted CSS/configuration, relaxed source coverage, extra background polling, dependency upgrade, or unrelated renderer/layout fixes. Defaults stay compatible when no colors are supplied.

**Verification:** Extend the existing all-thirteen-template production-adapter fixture with bounded representative style coverage. Assert actual accepted SVG/paint color changes, not just Ok results. Retain stale revision/document/options rejection and default-cache reuse tests.

**Acceptance:** All 13 current templates across 11 families have meaningful preset/color support; mindmap customization works; unsupported controls are not advertised; stale palettes cannot publish as current.

**Commit boundary:** feat(diagram): [M4-B] apply palettes across supported Mermaid families.

### M4-C — Color controls, templates, and in-app help

**Objective:** Make the completed styling and chart syntax discoverable in normal use.

**Code ownership:** src/gui/diagram_studio.rs; src/diagram/templates.rs; small task-owned help data/helper if useful; current relevant README/help documentation.

**Implementation tasks:** Expose presets and bounded custom colors through a compact menu/collapsible surface; provide native selected-node colors and mindmap root/branch controls; allow current templates to be created with a chosen preset; populate Help for all existing families with working examples.

**Behavior:** Users can start a colored diagram, change relevant colors, inspect source/examples, and save/export without hand-assembling undocumented configuration.

**Edge cases:** Apply to dirty or invalid source, open an example with unsaved work, choose a source-only family, narrow host, color-only undo, and copied examples with indentation.

**Regression constraints:** One accepted document mutation per user action; no rewriting during render; retain unknown draft text and existing dirty-transition decisions; no empty/nonfunctional color controls.

**Verification:** Exercise one native flowchart and one mindmap end to end. Reuse data-driven template/help fixtures for all families; verify the examples are accepted by the actual app adapter. Check save/reopen and both SVG/PNG colors.

**Acceptance:** Help is available offline and accurate; every promoted color control works; templates and help examples preserve unsaved-work rules; source and exports agree with the preview.

**Commit boundary:** feat(diagram): [M4-C] expose color presets and contextual Mermaid help.

### M5-A — Review, bounded final verification, and handoff

**Objective:** Resolve feature-related defects and deliver a truthful complete or limited result.

**Code ownership:** Cumulative task diff, focused touched tests, task ledger, and necessary task documentation. Only fix concrete findings.

**Implementation tasks:** Run one independent review of the cumulative change; resolve actionable findings; perform one consolidated affected-UI smoke and scoped final lint/format checks; verify Git ancestry and final state.

**Behavior:** All implemented workflows meet the requirements and remain coherent across docked/popped-out presentations.

**Edge cases:** Unavailable Windows/second volume/capture, unrelated historical warnings, partial push failure, and document-only final report changes.

**Regression constraints:** No full-suite escalation without concrete evidence, historical acceptance campaign, unrelated cleanup, or publication of unexecuted checks as passes.

**Verification:** Use Section H. Reviewer checks requirement coverage, input truthfulness, native reveal evidence, color persistence/export/cache identity, and interaction ownership.

**Acceptance:** No known feature-related blocking defect remains; required scoped checks pass or a precise external limitation is recorded. Fully Complete requires all required acceptance evidence. Otherwise report LIMITED with completed implementation distinguished from unverified native behavior.

**Commit boundary:** docs(studio-polish): [M5-A] record verification and final handoff, after any small labeled remediation commits. Read final_tip_sha after the actual last commit; report that exact SHA as the next Goal's required parent.

## G. Active Checkpoint Commit Cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

M0-A is mandatory and precedes all implementation. Each Section F checkpoint is a coherent commit boundary on the same branch. Verify the relevant behavior, inspect the diff/staged paths, commit, and attempt one ordinary push. Do not accumulate all unrelated changes into one final commit or create trivial line-by-line commits.

Preserve user work and checkpoint history. Use small remediation commits for actual findings rather than squashing or amending accepted history. A failed push leaves local progress intact and does not stop later work.

Record parent_branch, parent_tip_sha, baseline_sha, checkpoint subjects/SHAs, and actual push outcomes. The final report must include final_tip_sha captured after the last commit, checkout state, and the next-child instruction. Avoid self-referential documentation commits: do not create endless commits solely to write the hash of the file containing that same hash.

## H. Verification and Testing Strategy

### H1. One build/test lane and bounded execution

- Planning is read-only. After M0-A, use the existing fast-dev profile for focused development checks and the runnable verification build. No full suite, benchmark campaign, or repeated release builds is required by this Goal.
- Run only one Cargo/build/test process at a time. Separate compilation from the test-execution deadline. Bound each focused test execution batch to **100 seconds** with the repository's existing owned-process runner or equivalent supported mechanism; terminate/drain only task-owned processes on timeout.
- Use Cargo Nextest with package/module/exact-test filters and --no-fail-fast. Resolve the current exact selectors from source before running. A timed-out batch is not a pass; rerun only the relevant failure or split the affected batch when needed.
- App fixtures must disable Paste Sequences next_hotkey and cancel_hotkey before construction, as AGENTS.md requires. Preserve production defaults.
- Reuse RUST_MIN_STACK=16777216 only if the affected large app fixtures still require the documented test-process stack setting; do not alter production stack policy.
- Add meaningful behavioral tests for changed invariants. A label/menu move alone does not require a new testing framework or screenshot-diff suite.

Example focused invocation, after selecting the relevant tests:

~~~text
cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(comparison_reuses_frozen_named_and_physical_row_results_independently) | test(comparison_selected_result_rejects_same_document_reopen_collision)'
~~~

This command is an example for the comparison checkpoint, not a substitute for its new UI regression test or a requirement to run unrelated modules.

### H2. Existing focused coverage to reuse

| Area | Existing test anchors |
| --- | --- |
| Folder Size | folder_size_dialog_sorted_filtered_context_reveal_keeps_original_row; folder_size_dialog_reveal_root_route_is_explicit; windows_reveal_args_preserves_spaces_and_unicode_in_one_argument; directory_resolves_to_open; file_resolves_to_reveal |
| Comparison | comparison_reuses_frozen_named_and_physical_row_results_independently; comparison_selected_result_rejects_same_document_reopen_collision; comparison_integer_modes_and_full_width_differences_are_exact; comparison_preserves_duplicate_failed_names_and_unit_semantics |
| Inspector | inspector_group_rows_and_annotation_fragments_preserve_bit_indices; inspector_narrow_grid_renders_all_bits_and_keeps_long_low_label_in_row; programmer_inspector_retains_private_draft_across_new_context_and_move |
| Copy | explicit_copy_buttons_return_single_result_and_source_order_summary; vector_copy_buttons_return_pasteable_line_and_summary; accepted_copy_resolves_after_same_frame_source_edit; all_results_summary_preserves_physical_line_numbers_and_comments |
| Host ownership | programmer_inspector_comparison_and_preferences_use_retained_host_receipts; programmer_inspector_real_pad_selection_and_retained_bit_insert |
| Diagram input | diagram_studio_keyboard_menu_focus_is_retained_root_scoped_and_escape_restores_trigger; diagram_studio_source_focus_does_not_pan_or_zoom_canvas; visual_viewer_fit_tracks_resize_until_manual_zoom |
| Viewer behavior | diagram_viewer_preview_drag_moves_native_viewer_across_modes; diagram_viewer_preview_drag_moves_root_hosted_window; preserve untouched modes while changing the Visual expectations |
| Mermaid pipeline | all_thirteen_templates_render_through_the_production_adapter; publication_rejects_stale_revision_document_and_options; cache_skips_parse_layout_for_identical_and_raster_only_requests |

These are discovery anchors, not a mandatory combined mega-suite. Select the tests relevant to each actual change and add only missing protection.

### H3. Consolidated UI acceptance

Build and launch the actual application when the environment supports it, using an isolated synthetic profile and disposable documents/folders. Interact with real controls and capture useful screenshots when available.

1. **Folder Size:** Scan a small target on a genuinely different existing volume. Right-click a nested folder with a space/Unicode name and observe selection in its actual parent. Check a same-drive control and root. Check the toolbar wording and working open-directory behavior.
2. **Calc Pad/Inspector:** Compare typed rows and named variables, reproduce the previous selection/text mismatch sequence, toggle high/low bits in grouped layout, collapse sections, Dock/Pop Out, and paste Copy All into a disposable editor.
3. **Visual Diagram:** Enlarge/adjust the canvas, pan/zoom away, double middle click, and left-drag the Visual viewer. Confirm title-bar dragging still moves it, Studio object dragging still edits, and navigation does not dirty the document.
4. **Mermaid:** Indent a mindmap with Tab, undo, edit valid/invalid source without vertical editor movement, use File actions, apply a native node color and mindmap palette, save/reopen, export, and open Help.
5. **Shared host check:** While an affected utility is popped out, click and edit the main launcher query, then return to the utility. Check only this directly affected coexistence behavior.

A state test, successful subprocess launch, or queued shell operation is not native interaction proof. If capture or the required Windows/volume environment is unavailable, record the exact missing check and continue independent work. Do not require the user to execute intermediate commands or supply screenshots.

### H4. Final lint and review

Perform appropriate formatting/diff checks and one scoped first-party Clippy pass near completion. Fix newly introduced findings. Record unrelated pre-existing findings without absorbing them. Exclude unrelated vendored-crate lint cleanup; a vendor color patch still requires focused review of the changed behavior.

Review the cumulative diff from baseline_sha, source ownership, all 17 requirement IDs, and the actual evidence. Stop optional verification once the scoped acceptance criteria have sufficient evidence.

## I. Risks, Budgets, and Blocker Handling

| Risk | Required mitigation |
| --- | --- |
| Shell dispatch looks correct but Explorer selects elsewhere | Trace both paths and observe the actual cross-drive result; fix the first failing boundary. |
| Visible comparison text disagrees with hidden selection | One authoritative input mode per side, truthful display, and a focused transition regression. |
| Compact bit cells remap indices or hide controls | Preserve exact index mapping; test representative widths/groupings and reachable scrolling. |
| Diagram menu/layout changes bypass accepted actions | Reuse existing intents, focus scopes, and host/generation fences. |
| Visual primary pan also moves a host or edits objects | Context-specific button ownership; migrate only Visual viewer drag expectations. |
| New theme UI silently does nothing or loses colors | Prove actual color consumption for every current family and parity across source/save/undo/export. |
| Styling broadens parser acceptance accidentally | One typed allowlist, retained limits, explicit rejection of unsupported input, and preserved original source. |
| Native environment blocks acceptance | Record the precise unverified interaction and continue unaffected milestones; do not claim Complete. |

Use a bounded initial diagnosis and at most two focused corrective attempts for the **same external/native validation failure**. A changed hypothesis supported by new evidence may justify a further small step; repeated identical capture retries do not. Ordinary compiler feedback is not an automatic blocker.

If a feature defect remains, preserve the coherent work and report the exact missing behavior, evidence, attempted remedies, and smallest next action. Continue independent requirements where safe. Historical warnings, unrelated tests, and optional enhancements do not block this Goal.

For an actual blocked/limited/paused stop, follow AGENTS.md's concise tracked report policy. Proposed standalone stop path: docs/plans/execution/standalone/studio-and-utility-polish/stalls/<UTC-timestamp>.md. A documentation-only stop commit is not an accepted implementation checkpoint. Do not change AGENTS.md merely to restate these rules.

## J. Definition of Done

- All V1–V5, MD1–MD5, F1–F2, P1–P3, and C1–C2 requirements are implemented, with meaningful functional acceptance evidence.
- No known feature-related blocking defect remains; directly affected existing behavior and old document compatibility are preserved.
- Copy/comparison/camera/menu/style effects retain the correct source, host, identity, and revision ownership.
- Colors are real, persisted, undoable, and consistent across preview, viewer, source export, and image export for the approved current-family scope.
- Focused tests/checks and independent review pass; any external native limitation is explicitly unverified and changes the final disposition to LIMITED.
- New first-party lint issues are resolved; unrelated findings are accurately separated.
- The intended changes are committed on the correct fresh chained branch with M0-A as a direct child of the verified predecessor.
- Push successes and local-only commits are reported accurately. Push failure alone does not make completed local implementation fail.
- The final report records final_tip_sha as the next Goal's required parent. No worktrees, auxiliary clones/checkouts, independent agent/checkpoint branches, or intermediate integration merges were used.
- The user receives one final simple manual review checklist. No intermediate user validation is required.

## K. Final Delivery Report Required from Codex

Provide a concise factual report with:

1. Overall disposition: Complete, LIMITED, or Blocked, with implementation separated from native verification.
2. Completed checkpoints and requirement coverage; any uncompleted requirement by ID.
3. Important decisions and relevant files changed, including the reason for any narrow renderer patch.
4. Actual focused commands/results, UI evidence, and checks not executed. Do not report proposed tests as passes.
5. Concrete remaining issues or external limitations.
6. Git handoff: parent_branch, parent_tip_sha, task branch, baseline_sha, significant checkpoint SHAs, final_tip_sha, push state, and checkout state.
7. Explicit instruction: **The next Goal must branch directly from this final_tip_sha, not from master/main.**
8. Confirmation that no Git worktrees or intermediate branch merges were used.
9. A short user checklist covering the five consolidated workflows in H3, phrased as simple actions and expected results.

Capture the final SHA after all implementation/remediation/report commits. Record it in the final response and any authoritative external handoff ledger; do not chase a self-referential tracked-file hash.

## L. Source References

Local paths and symbols above are the execution guide. These immutable references document the planning findings:

- [Current application reference commit](https://github.com/multiplex55/Multi_Launcher/commit/838f8a42ff4c938a314065d5d3b79f3ac5425195)
- [Predecessor task tip](https://github.com/multiplex55/Multi_Launcher/commit/d691888584d531d9167f035dd48e47d15dced483)
- [Diagram UI and interaction ownership](https://github.com/multiplex55/Multi_Launcher/blob/838f8a42ff4c938a314065d5d3b79f3ac5425195/src/gui/diagram_studio.rs)
- [Mermaid rendering and admission](https://github.com/multiplex55/Multi_Launcher/blob/838f8a42ff4c938a314065d5d3b79f3ac5425195/src/diagram/rendering.rs)
- [Current templates](https://github.com/multiplex55/Multi_Launcher/blob/838f8a42ff4c938a314065d5d3b79f3ac5425195/src/diagram/templates.rs)
- [Native graph model](https://github.com/multiplex55/Multi_Launcher/blob/838f8a42ff4c938a314065d5d3b79f3ac5425195/src/diagram/model.rs)
- [Programmer Inspector UI](https://github.com/multiplex55/Multi_Launcher/blob/838f8a42ff4c938a314065d5d3b79f3ac5425195/src/gui/programmer_inspector_dialog.rs)
- [Calc Pad UI and copy formatter](https://github.com/multiplex55/Multi_Launcher/blob/838f8a42ff4c938a314065d5d3b79f3ac5425195/src/gui/calculation_pad_dialog.rs)
- [Comparison domain](https://github.com/multiplex55/Multi_Launcher/blob/838f8a42ff4c938a314065d5d3b79f3ac5425195/src/calculation_pad/comparison.rs)
- [Folder Size UI and retained reveal](https://github.com/multiplex55/Multi_Launcher/blob/838f8a42ff4c938a314065d5d3b79f3ac5425195/src/gui/folder_size_dialog.rs)
- [Shared Explorer actions](https://github.com/multiplex55/Multi_Launcher/blob/838f8a42ff4c938a314065d5d3b79f3ac5425195/src/file_search/actions.rs)
- [Pinned renderer 0.3.1 theme implementation](https://github.com/1jehuang/mermaid-rs-renderer/blob/2f993bd79a55235eb59a34d807852276ba25bea7/src/theme.rs)
- [Pinned renderer color/layout configuration](https://github.com/1jehuang/mermaid-rs-renderer/blob/2f993bd79a55235eb59a34d807852276ba25bea7/src/config.rs)
- [Pinned renderer event-card and family color consumption](https://github.com/1jehuang/mermaid-rs-renderer/blob/2f993bd79a55235eb59a34d807852276ba25bea7/src/render.rs)

Official Mermaid documentation is useful background, but the pinned Rust renderer and this app's own admission rules determine supported behavior. Do not copy newer Mermaid.js examples into Help without qualifying them through the actual production adapter.
