# Codex Goal — Productivity Tools Refinement

## A0. Mandatory chained Git bootstrap — first execution checkpoint M0-A

Read the repository's `AGENTS.md`, applicable local instructions, this entire Goal, and `CODEX_KICKSTART_PRODUCTIVITY_TOOLS_REFINEMENT.md` before acting. This Goal and its Kickstart describe **one shared task branch**. Use one stable branch for all milestones below; these milestones are not separate category handoffs.

**Before editing application code, running implementation builds/tests, or starting M1:**

1. In the single existing checkout, inspect `git status -sb`, current branch, `HEAD`, remotes, upstream, staged/unstaged/untracked work, and the current queue/orchestrator/execution records. These checks are read-only. Identify the owner and status of any preceding active or paused task.
2. Resolve `parent_branch` and the exact `parent_tip_sha`. For a continuing chain, the required parent is the **immediately previous completed Goal/category branch's recorded final commit**, verified against actual Git objects. Do not substitute the current checkout, an older historical tip, a stale remote, or `master`/`main`. Safely fetch a remote-only predecessor if needed. Conflicting records or an unavailable required ancestor are bounded blockers; do not guess.
3. **Only the first Goal in a genuinely new chain** may default to the up-to-date primary integration branch. For that first task only, a safe current-HEAD base is permissible when clearly documented and when no user changes are lost or accidentally committed. Do not infer that this is a new chain merely because the current checkout is on the integration branch.
4. Choose a fresh unique lowercase kebab-case branch, preferably `feat/productivity-tools-refinement` with a meaningful suffix if occupied. The Goal and Kickstart do not get separate branches; neither do agents or checkpoints.
5. Protect pre-existing work. Never automatically reset, hard-pull, stash, rebase, merge, force-checkout, discard files, or stage unrelated changes to make switching possible. If switching to the required parent would be unsafe, document the precise blocker and preserve the checkout.
6. Create and switch directly from the verified parent: `git switch -c <new-unique-branch> <parent_tip_sha>`. Verify that the new branch initially points exactly to that SHA.
7. Create this branch's **M0-A baseline commit**, preferably containing a small task-specific execution ledger consistent with the repository. An intentional empty commit is acceptable: `git commit --allow-empty -m "chore: [M0-A] establish productivity tools refinement baseline"`. Stage only intentional task files. Record `baseline_sha` separately from `parent_tip_sha`; verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`. The predecessor's final commit is not this Goal's baseline.
8. Attempt one ordinary upstream push, normally `git push -u origin HEAD`, where the remote and environment permit it. Preserve local commits and continue if access, network, approval, or remote state prevents the push. Record the actual failure/unavailability; do not retry in a loop or bypass restrictions.
9. Run all implementation, verification, and checkpoint commits on this same branch. Attempt one normal non-force push after each checkpoint. Do not rewrite checkpoint history.
10. At completion, record the final branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, significant checkpoint SHAs, `final_tip_sha`, actual push state, and clean/dirty state. **The next Goal must branch directly from this `final_tip_sha`.** No intermediate merge, rebase, squash, or cherry-pick is permitted merely to advance the sequence. The final branch of the approved sequence contains the cumulative work and can be integrated once at the final integration boundary.

**NO GIT WORKTREES.** Do not run `git worktree add`, `git worktree move`, or `git worktree prune`. Do not create auxiliary checkouts, secondary clones, agent-specific clones, or independent side branches. All mutations and Git transitions occur sequentially in the existing checkout. Read-only investigation/review may run in parallel.

**M0-A objective / ownership / tasks:** Establish the correct ancestry and task ledger; the orchestrator owns Git and the existing execution records. Prefer the active queue's existing ledger, or `docs/plans/execution/standalone/productivity-tools-refinement/STATUS.md` when standalone. Do not create competing ledgers.

**M0-A behavior, edge cases, and invariants:** No application behavior changes. An inaccessible push is nonblocking; uncertain ancestry or unsafe local changes blocks branch creation. A paused predecessor is not automatically a completed predecessor. A local unpushed final tip remains a valid parent on the same machine; execution elsewhere must wait until that exact ancestor is available.

**M0-A verification / acceptance / commit boundary:** A unique task branch is active; its baseline is a direct child of the verified parent; initial push outcome is recorded; user changes are intact and unstaged by this task; no extra checkout or worktree exists from this workflow. The baseline commit is the first checkpoint. Only then proceed.

## A. Goal and desired outcome

Make the requested Calculation Pad, Programmer Inspector, ZIP utility, visual Diagram Studio/Viewer, Explorer Tabs, Screen Recorder, and command discovery workflows work predictably through their existing architecture.

The user should be able to:

- Find concise executable Calculation Pad examples, search them while scrolling, and keep the reference in its own native window.
- Enter binary/mixed-base math, case-insensitive built-in spellings, `pi`/`pie`, infix `mod`, and bounded vector arithmetic.
- Change the Programmer Inspector's working width directly, group visible bits, and see named ranges where those bits appear.
- Extract ordinary Stored/Deflated ZIP files with all members initially selected; select all explicitly; build a new ZIP from files/folders accumulated from multiple locations.
- Use conventional visual diagram editing, visible marquee selection, centered shape labels, draggable minimaps, editable connector bends, additional templates, and a viewer that uses its available size.
- Open two requested folders as verified tabs in one attributable Explorer window and populate saved lists from existing `f` folder bookmarks.
- Select a recording window from a cold start, preview a selected region, and see an accurate active-region outline.
- Search Command Help without unrelated tutorial sections and type `record` without accidentally invoking the Recycle Bin provider.

Requirements are sufficiently specified to execute. The behavioral defaults in section C are part of this contract. Resolve routine engineering details autonomously; do not introduce a new questionnaire for implementation mechanics.

## B. Current codebase findings and architectural owners

Reconfirm these locations after M0-A. If paths move, locate their real successors; preserve the approved scope and behavior. The references below identify inspected code, not proposed new module names.

| Area | Source evidence | Consequence for implementation |
| --- | --- | --- |
| Calculation routing | `src/calculation_pad/document.rs`: preparation pipeline, `evaluate_prepared_expression`, `CalculationValue`, `parse_prefixed_integer`, `lower_numeric_literals`; `quantity.rs::Parser::parse_prefix` and `parse_unit_suffix` | Scalar, exact integer/base, and quantity paths coexist. Binary literals already exist on the scalar path. Quantity parsing can consume decimal `0` plus the bit unit `b`, then incorrectly win with definitive unit evidence. |
| Operators and compatibility | `src/calculation_pad/operators.rs::make_operators`; `document.rs::is_reserved_name`, `lower_mod_function_calls`; `quantity.rs::scalar_constant`; `src/unit_conversion.rs::unit_alias_boundary` | Existing `%` is signed remainder; `mod(a,b)` is Euclidean remainder. Bare user variables named `mod` are supported. Do not globally lowercase source or alter old precedence. |
| Value/result consumers | `src/calculation_pad/{document,session,comparison,integer}.rs` | `CalculationValue` has no vector variant. Existing dependency references are named variables and physical-line `rN` references. No new `ans` or statistics system is required. |
| Calculation reference | `src/calculation_pad/examples.rs`; `src/gui/calculation_pad_dialog.rs::examples_ui`, `examples_layer_if_root` | The catalogue already contains 31 tested examples, but introductory/reference text sits outside its ScrollArea. Help is ROOT-only even though the Pad itself has a native host. |
| Inspector | `src/gui/programmer_inspector_dialog.rs`; `src/calculation_pad/inspector.rs`; `integer.rs::IntegerWidth::ALL` | Width pills edit a conversion target; the grid renders the working value's mode. Existing grouping changes output strings, not the grid. Range annotations already exist with half-open bounds. |
| ZIP extraction mismatch | `src/archive/model.rs::EntryCompression`, `ArchiveEntry::is_supported`; `plan.rs`; `writer.rs` preflight around 892 and `write_stage_payload`; `session.rs::build_preview` | Catalogue/planner permit Deflated entries; writer currently rejects non-Stored files and copies raw stored payload bytes. A test explicitly expects Deflate rejection. This is a concrete compatibility defect. |
| ZIP selection and host | `src/archive/session.rs::request_open` and successful Open completion; `plan.rs::ArchiveSelection`; `src/gui/zip_extractor_dialog.rs::update_session_snapshot`; `src/plugins/zip_extractor.rs` | Successful loading does not initialize all entries selected. Selection belongs to the session; the UI mirrors it. Keep existing `zip`/`zip:open` routes and host ownership. |
| ZIP native I/O | `src/archive/native_io.rs::OverlappedArchiveReader`; `src/archive/native.rs::NativeArchiveDestination`, `HeldDirectory::create_stage`, `OwnedStage::publish`/`abort`; `Cargo.toml` | Existing ZIP 9.1.2 dependency enables Deflate. Reuse bounded held-handle, staging, and publication infrastructure; no production creation path exists yet. |
| Visual diagram UI | `src/gui/diagram_studio.rs::draw_visual_body`, `paint_visual_projection`, `canvas_gesture_mode`, `viewer_canvas_shortcuts` | Marquee is painted before an opaque canvas repaint. Visual mode lacks normal structural clipboard handling. Existing selection, transaction, and gesture systems should be extended. |
| Visual model and outputs | `src/diagram/{visual,visual_scene,visual_canvas,codec,drawio,export}.rs` | Primitives and text are separate; connectors have automatic Straight/Elbow/Curved routes but no persisted manual points. Draw.io import rejects custom waypoints. Geometry must survive the full model/output path. |
| Fonts, templates, viewer | `diagram_studio.rs` font controls, `draw_visual_viewer_body`; `src/diagram/visual_templates.rs::VisualAuthoringTemplate`; `src/diagram/export.rs` | Font entry is free text but renderer/export currently distinguish proportional/monospace only. Five native templates exist. Visual viewer height is fixed at 300 pixels. Minimap interaction is click-only. |
| Explorer capability | `src/explorer_tabs/native_uia.rs::ExplorerUia::connect`; `windows.rs::WindowsExplorerTabsPort::preflight`; `src/gui/explorer_tabs_dialog.rs::window_status` | Legacy `CUIAutomation` is created and cast to `IUIAutomation2`; failure yields the reported timeout-capability message before any window request. |
| Saved folders | `src/plugins/folders.rs::FolderEntry`, `FoldersPlugin::list_entries`, live registry/load logic; `src/explorer_tabs/folder_lists.rs`; `src/gui/explorer_tabs_dialog.rs` | Explorer lists persist paths, not bookmark foreign keys. The current editor accepts manual paths and caps lists at 32 folders. |
| Recorder discovery | `src/gui/recording_dialog.rs::refresh_targets`, `refresh_window_snapshot`; `src/window_catalog.rs::request_refresh`, `snapshot_and_refresh` | Recorder reads a generation-zero shared cache without requesting initial window enumeration. Display enumeration is a separate operation. |
| Recorder region | `src/recording/windows.rs::crop_for_request`, capture-resource start/copy path; `src/gui/mkmacro_dialog/{visual_overlay,visual_overlay_windows,visual_capture_workflow}.rs` | Encoded frames are already cropped. Capture uses a full-display WGC item, consistent with a monitor-wide OS border. Passive rectangle/edge-window infrastructure already exists for precise outlines. |
| Command discovery | `src/help_window.rs::HelpWindow::ui`, `matches_filter`; `src/plugins/recycle.rs::RecyclePlugin::search`; `src/plugin.rs::query_prefixes` routing; `src/gui/search.rs::search_plugins_from_results` | Command Help already pins its filter above scrolling commands, but prepends unrelated sections. Recycle accepts every query beginning `rec`, including `record`. Fix the provider boundary, preserving valid plugin results and fuzzy search. |
| Shared host lifecycle | `src/gui/{utility_host,utility_window_host,utility_dialog_size,utility_window_geometry}.rs`; `src/gui/{mod,render}.rs` | Reuse host identity, retained-pass provenance, ROOT input registration, native geometry, and accepted side-effect dispatch. A new help window must not create a duplicate Pad session. |

The exact ZIP that produced the user's error has not been independently identified. The Deflate defect is confirmed in source; do not claim that every possible layout error has the same cause. Likewise, source inspection identifies the UIA activation mismatch and recorder refresh omission; native validation still has to establish their runtime outcomes.

## C. Functional contract and acceptance requirements

### C1. Calculation Pad language

**CALC-01 — Prefixes and built-in aliases**

- Recognize `0b`, `0o`, and `0x` tokens before unit-suffix evidence can misclassify them, including uppercase prefixes already accepted by the scalar parser.
- Preserve authored source, comments, line numbers, Unicode/braced identifiers, and variable case. Normalize only recognized built-in tokens in their syntactic positions.
- Unbound `pi` and `pie`, case-insensitively, resolve to canonical `PI`. Existing canonical constants remain valid.
- Built-in function spellings such as `Floor`, `FLOOR`, and `floor` resolve consistently. A variable such as `floor_price` is never rewritten.
- Preserve previously valid declared identifiers: an existing user binding named `pi`/`pie` retains precedence, and a braced name remains an explicit user reference. Do not newly invalidate such documents.
- Keep the scalar route's existing exact-representation bound through `2^53`; exact integer/base routes remain available for larger values. Never silently round an exact large integer into a vector/scalar result.

**CALC-02 — Infix remainder**

`a mod b` is a contextual spelling of `a % b`, with the same precedence, associativity, zero-error behavior, and negative-operand behavior. Preserve `mod(a,b)` and a variable called `mod`; they are not rewritten as infix operators outside that syntactic position.

**CALC-03 — Vectors**

- Add typed, owned, one-dimensional nonempty vectors of finite dimensionless scalar values, written `[1, 2, 3]`.
- Bound a vector to **128 components**, within existing expression-byte, nesting, dependency, and document limits. Scalar expressions/references may appear in components.
- Support scalar multiplication in either order; equal-length vector `+`, `-`, `*`, and `/` are element-wise. Vector/scalar division uses the same finite/zero checks. Do not broadcast scalar addition or silently convert quantities/exact typed integers.
- `dot(a,b)` and `a · b` return a scalar for equal-length vectors. `cross(a,b)` and `a × b` require two three-component vectors and return a vector. Vector `*` continues to mean element-wise multiplication.
- Named variables and physical-line `rN` references may hold vectors. Reevaluation, source editing, undo/redo, save/reopen, line-copy and all-results-copy must remain coherent.
- Format results as pasteable vector expressions. Copied output must parse as the displayed vector value; preserve the shared scalar formatter's existing rounding policy. Use exact expectations for exactly represented examples and an explicit display-appropriate tolerance for rounded components. Reject mismatched dimensions, empty/nested vectors, unsupported kinds, excessive work, nonfinite values/results and division by zero with line-local diagnostics.
- Update exhaustive consumers of `CalculationValue`. Preserve Inspector access for exact integer results and existing legacy exact-base results (`InspectorSourceOrigin::LegacyExactBase`); vectors and scalar/quantity results do not acquire Inspector actions. Vector ordering/comparison may return a clear unsupported diagnostic instead of inventing an ordering.

Required examples:

| Input | Expected result |
| --- | --- |
| `0b10101` | `21` |
| `0b0101 + 123` | `128` |
| `0x10 + 0o10 + 0b10` | `26` |
| `sqrt(0b10000)` | `4` |
| `Floor(2.9)` | `2` |
| `pi`, `pie`, `PI` without a user binding | Same scalar constant |
| `7 mod 3` | `1` |
| `-7 mod 3` | `-1` |
| `mod(-7, 3)` | `2`, preserving its existing distinct behavior |
| `[1, 2, 3] * 3` and `3 * [1, 2, 3]` | `[3, 6, 9]` |
| `[1, 2, 3] + [1, 1, 1]` | `[2, 3, 4]` |
| `[1, 2, 3] * [4, 5, 6]` | `[4, 10, 18]` |
| `dot([1, 2, 3], [4, 5, 6])` | `32` |
| `cross([1, 0, 0], [0, 1, 0])` | `[0, 0, 1]` |

### C2. Calculation examples and reference window

**HELP-CALC-01:** Reuse and expand `examples.rs` as the single executable catalogue. Give each currently supported built-in function a discoverable example or a compact family example that includes its spelling/keyword. Add the new aliases, mixed bases, signed remainder versus Euclidean modulo, scalar/vector operations, dot and cross. Preserve useful existing integer, variable, line-reference and unit examples. Prefer source → result and one short explanation; keep detailed language/limit material in collapsed reference sections.

**HELP-CALC-02:** Keep search and window actions pinned above **one scrolling content body**. Include all examples and explanatory sections in that body. Search case-insensitively across titles, group names, source and explicit keywords/aliases; `ceil`, `floor`, `sqrt`, `binary`, `dot`, and `cross` must reveal relevant content. Active matches must not remain hidden inside closed groups. Empty search restores the catalogue; no matches provides a clear state and clear-search action.

**HELP-CALC-03:** Give the reference window independent Pop Out/Dock state and native identity. It can resize and remain open alongside the Pad; docking preserves its search and scroll state. Closing help closes help only. Preserve Pad source, dirty state, history, recovery state, and saved document. Help Copy is explicit and uses accepted viewport/copy provenance. Close or retire help coherently when its owning Pad session is actually retired; no orphan native window or stale event may affect a later session.

### C3. Programmer Inspector

**BITS-01:** Selecting 8/16/32/**64**/128 or signedness immediately attempts a checked working-mode change. When it succeeds, active mode and visible bit count agree. If the value cannot fit, preserve the working value/mode, explain the overflow, and offer the existing explicit Reinterpret bits action for the attempted target. Show active and pending target states unambiguously; do not silently truncate or display a narrow grid over an unrelated wide value.

**BITS-02:** Add a distinct **Bit grouping** control: 1, 4, 8, 16 and useful larger powers of two up to the width. Default to 4. It organizes the bit grid and wraps at narrow widths; bit 0 remains least significant and logical numbering remains stable. It must not change numeric value, signedness, endianness, or the existing output-string grouping/copy settings.

**BITS-03:** Render named annotations on their actual visible spans, using highlight/bracket/band plus label/range text or an equally clear treatment. Translate internal `[start,end)` bounds to inclusive display ranges, e.g. `Status [7:0]`. Overlapping and cross-group ranges remain distinguishable. Preserve annotations outside the current width and mark them out of range rather than deleting them or painting them as valid. Changes remain local until Insert; retain source/revision/selection guards and undo.

### C4. ZIP extraction and creation

**ZIP-01 — Compatible extraction:** Support ordinary unencrypted Stored and Deflated ZIP entries through the existing bounded streaming, source-validation, CRC, staging and publication flow. Include the descriptor/ZIP64 forms already supported by the catalogue/validated reader. Do not promise every compression/encryption/multidisk variant. A remaining standards-compatible failure must be diagnosed with a minimal fixture and specific reason; do not blanket-relax path, metadata, overlap or integrity validation.

**ZIP-02 — Selection:** After a successful *new archive load/reload*, select all physical entries once for the accepted catalog revision, including explicit directory records. Add Select All and Clear Selection. Select All covers the whole archive, regardless of current filtering. Subsequent polling/rendering/host changes must not overwrite the user's later choices within that loaded archive. Preserve existing clear-on-open behavior; failed/stale completions must not apply selection to a newer catalog. Keep unsafe/unsupported members visible with their issue labels; selection does not authorize bypassing validation. A blocked member may prevent the preview until deselected/resolved. Bulk selection invalidates the preview once and retains physical entry IDs, even for duplicate filenames.

**ZIP-03 — Create mode:**

- Keep `zip` and `zip:open` compatible. Present Extract/Create within the existing utility, with useful plugin help. Default existing invocations to Extract.
- Repeated Add files and Add folder operations accumulate sources from different directories. Allow removal and an editable archive-relative destination path or equivalent virtual folder layout.
- Files default to root basenames; folders default to their folder name with recursive contents and empty directories preserved.
- Detect unsafe names, normalized/case-folded collisions, and file/directory conflicts before writing. Let users edit/remove conflicting staged paths; never rename or overwrite silently.
- Choose an output `.zip` explicitly. **Create a new destination only**; if it already exists, ask the user through the UI to choose another name. No source files are modified.
- Use ordinary Deflate and UTF-8 filenames. No encryption, split archives, other archive formats, or persistent ZIP-project format is required.
- Scan/compress on bounded background work with progress and cancellation. Freeze the source manifest and relevant identities, enforce resource limits and detect meaningful source changes. Keep the UI responsive and prevent incompatible draft/mode changes while an operation runs.
- Refuse unsupported symlink/reparse/device inputs. Reject output/stage recursion when the destination lies inside a selected source tree, and reject an output alias that is itself an input.
- Stage alongside the destination; finish ZIP metadata/central directory and flush before a no-clobber publication. Errors/cancellation leave no published partial ZIP. Record and safely clean owned staging files; never delete someone else's file.
- A new ZIP must reopen in the existing catalogue and extract correctly, including the new Deflate path.

### C5. Visual Diagram Studio and Diagram Viewer

**DIAG-01 — Structural shortcuts:** In an active writable visual canvas, support Ctrl+C/X/V/A, Delete/Backspace, Ctrl+Z and Ctrl+Y/Ctrl+Shift+Z. Handle actual egui Copy/Cut/Paste events as well as appropriate key events exactly once. Text fields/inline labels retain text editing; menus/modals and inactive/stale viewports must not trigger canvas edits. The read-only viewer never mutates a document.

Clipboard content must be bounded and typed. Paste allocates fresh IDs and remaps selected groups, connector endpoints, layers/presets and required embedded assets without dangling references. Preserve relative geometry/order and apply a small visible paste offset. Copy is non-mutating; successful cut, paste and multi-delete each form one undoable operation. A failed copy must not delete the selection. Support roundtrips between visual documents through the normal clipboard path; unrestricted external draw.io clipboard interoperability is not required.

**DIAG-02 — Font choice:** Replace font-family textboxes with dropdowns for font families actually supported by rendering and export. At minimum expose Sans serif and Monospace. Preserve an unknown/imported current family with a clearly indicated fallback until the user chooses a replacement. Do not present arbitrary installed fonts as supported without implementing their rendering.

**DIAG-03 — Marquee:** Empty-canvas left drag visibly shows the selection rectangle throughout the gesture and replaces selection on release. Ctrl+empty-canvas drag adds to selection; preserve existing Shift behavior. Object drag continues moving selected objects. Paint gesture affordances after the opaque scene/background, clipped to the canvas. Escape/ownership loss cancels safely.

**DIAG-04 — Inline centered shape labels:** Double-click a primitive or its owned label to edit the same centered text directly with immediate keyboard focus. Resolve label hits through the explicit ownership association so text painted above a shape does not block re-editing. Re-edit its existing owned label rather than layering duplicates. A real persisted association/owned-label representation must keep text coherent through move, resize, rotate, copy and delete. Preserve unrelated free text and never infer old relationships merely from proximity. Establish associations for new templates and recognized imported vertex labels where appropriate. Use Enter to accept, Shift+Enter for a newline, Escape to cancel, and an explicit commit on ordinary canvas focus transfer. One accepted edit is one history transaction; cancellation is non-mutating.

**DIAG-05 — Minimap:** Support press/drag/release in editor and visual viewer. Dragging the existing view box preserves grab offset; pressing elsewhere recenters and may continue dragging. Use a stable mapping during a drag so moving-camera bounds do not cause feedback/jumps. Camera changes do not dirty the document.

**DIAG-06 — Bends:** Selecting a connector exposes useful bend/control handles. Users can drag geometry, add/remove applicable waypoints, and reset to automatic routing. Preserve current automatic Straight/Elbow/Curved behavior when no manual geometry exists. Endpoints remain attached; document how world-space interior points behave when endpoints move. Validate bounded finite geometry and commit gestures once on release; Escape/stale ownership cancels. Save/reopen, undo/redo, the viewer and SVG/PNG output must agree. Implement the narrow draw.io waypoint representation needed for supported manual routes. If a route cannot be represented faithfully, stop that export with a specific unsupported-geometry result; never silently flatten or drop bends. This is not a mandate for full draw.io parity.

**DIAG-07 — Templates:** Keep the existing Flowchart, Architecture, MindMap, Sequence and Dependencies templates. Add four useful editable examples: Organization Chart, Network Topology, Decision Tree and Swimlane Workflow. Use current primitives/styles and the completed label/connector behavior. Preserve the dirty-document decision before replacing a document.

**DIAG-08 — Viewer fit:** Allocate the visual preview from remaining available width **and height**, with finite bounds, clipping and a sensible minimum. Preserve aspect ratio. Opening/Fit frames the document; resizing adjusts an active fit view, while deliberate manual camera movement remains under user control until Fit is requested again. Preserve 100%, wheel zoom, panning and Mermaid preview behavior. Remove the fixed 300-pixel visual-preview constraint.

### C6. Explorer Tabs

**EXPLORER-01:** Correct UIA activation using a timeout-capable COM object, preferably documented `CUIAutomation8`/`IUIAutomation2`, through existing Windows bindings and worker ownership. Retain connection/transaction timeouts, cancellation, operation deadlines and preflight-before-window-creation. Verify the supported native path; do not treat changing a CLSID as runtime acceptance. On genuinely missing capability, report a specific failure without opening a window or discarding requested folders. Do not fall back to unbounded legacy calls, blind keystrokes, arbitrary sleeps or repeated Explorer windows.

**EXPLORER-02:** Add a searchable Add saved folder picker sourced from the existing `f` plugin records. Search/display label, alias and path, disambiguate duplicate labels, and retain manual path entry/reordering. Copy the selected current path into the Explorer draft; later bookmark edits/deletion do not silently alter saved Explorer lists. Reuse a bounded/live snapshot or load on opening the picker, never every frame. Preserve existing schema, 32-folder cap, path validation, stable row identities, dirty/save behavior, frozen run snapshot and host/chooser provenance.

The native success condition is two requested folders verified in tabs of one attributable Explorer window. Preserve verified, attempted-but-unverified and remaining-folder distinctions.

### C7. Screen Recorder

**RECORD-01:** Request asynchronous shared window enumeration on recorder open, on switching to Window, and on explicit Refresh. Consume published generations and request repaint without blocking the UI or adding another enumerator. Distinguish loading, empty, and failed states. Preserve/revalidate selected HWND/PID at start; stale windows must not be silently replaced. Avoid a continuous refresh/poll loop when idle.

**RECORD-02:** For Display/selected-region recordings, show an app-owned accurate active-region outline from the frozen region's physical desktop bounds. It must be passive, must not intercept input/focus, and must not appear in encoded output. Handle negative monitor origins/DPI correctly. Tie it to the recording generation; remove it on stop, discard, error, target loss, shutdown and before a later recording can inherit it. Preserve the existing correct crop and output dimensions. Window capture follows the current captured window extent: preserve its OS window indicator and do not draw a stale frozen rectangle from the window's initial desktop bounds. Adding dynamic app-owned window tracking is outside this request.

Windows' own WGC border belongs to the captured display/window item. Check supported borderless-capture capability within the existing app deployment. Use documented capability/consent only when available; do not create a packaging redesign or promise unconditional suppression. If unavailable/denied, retain the OS indicator and briefly distinguish it from the app's recorded-region outline. This is a declared platform limitation, not a reason to skip the accurate region indicator.

**RECORD-03:** Add Show region for a valid selected display region. Use the shared passive rectangle-preview infrastructure with a short automatic expiry. It must not start recording, change selection, interrupt an active selector, or leave an orphan outline. Close/source/region revisions invalidate obsolete preview ownership. Disable it when no valid display region exists.

### C8. Command Help and launcher relevance

**COMMAND-01:** In Command Help remove the fixed Query filters, Linked note/todo/calendar references, Search and Replace tutorial, and Launcher actions sections. Show plugin commands and their directly relevant descriptions/usage/examples. Preserve the actual Search and Replace plugin command entry. Keep the pinned filter and Show examples behavior. A search such as `folder size` must find the Folder Size plugin via its display/command metadata. Quick Help and command execution are otherwise unchanged.

**COMMAND-02:** Make Recycle's command matching respect the complete supported `rec` command, case-insensitively with surrounding whitespace. The no-argument command must not claim `record`, `recorder`, `recent`, or arbitrary longer words. Declare its query prefix consistently with `PluginManager` routing where appropriate. Preserve actual `rec` discovery, configured aliases, `record` recording discovery, exact/fuzzy settings and plugin-resolved actions whose labels do not contain literal query text. Do not impose a global label-substring filter or alter the shared `strip_prefix_ci` contract to repair one provider.

## D. Scope, non-goals and behavioral invariants

This is one bounded cross-tool refinement Goal. It does not authorize workspace features, a new application framework, a calculator replacement, a broad search/routing rewrite, full draw.io compatibility, general archive management, arbitrary compression formats, or a new recorder backend.

Preserve:

- Global hotkeys, launcher query focus and existing command/action IDs except the explicitly corrected Recycle prefix matching.
- Existing Calculation Pad scalar precedence, exactness limits, quantities, typed integer semantics, source bytes, physical-row references, persistence/recovery and dirty-document decisions.
- Supported existing `.mld` visual documents, including v2/v3 decoding; legacy optional fields must default compatibly. Add a format version only when its semantics require it, with focused compatibility tests. Never silently drop newly persisted labels/routes on save/export.
- Single-session native host ownership, accepted-pass effects, copy/chooser provenance, focus, close and retire behavior. No duplicate documents or unsafely shared mutable UI state.
- ZIP traversal, collision, CRC, quota, held-handle, cancellation and no-clobber guarantees.
- Explorer request attribution and tab-verification semantics.
- Recorder GIF/MP4 encoding, crop, countdown, stop hotkey and worker retirement.

Prefer existing dependencies. A minimal direct Deflate dependency is permissible only if the existing ZIP dependency cannot expose the needed bounded streaming integration cleanly; record why. Do not introduce a linear-algebra/font/diagram framework.

Do not change agent model settings, globally refactor tests into a new assertion style, fix unrelated Clippy findings, or revive historical acceptance campaigns. Existing history is reference material unless this Goal expressly adopts it.

## E. Technical strategy and dependency order

1. Repair parser dispatch/token handling before adding vectors and expanding executable help.
2. Extend the calculation domain with typed vector ownership and shared scalar-component semantics. Do not evaluate by rewriting result strings, duplicating the scalar language, or indiscriminately routing all expressions through a new engine.
3. Keep Inspector layout derived from the working value; make safe conversion selection explicit.
4. Align ZIP catalogue, plan and writer support. Stream decompression within verified source spans and output quotas. New creation types belong under the existing archive subsystem; UI coordinates them.
5. Keep visual mutations in `PendingMutation::VisualTransaction`/`VisualHistory` and `VisualDiagramDocument::apply_transaction`. Preview gestures without committing model state each frame. Extend codec/projection/export alongside labels and route geometry.
6. Fix Windows capability/refresh ownership at the first responsible boundary. Reuse the shared WindowCatalog and passive overlay controller.
7. Preserve shared-host architecture and keep command search changes narrow.

Milestones run in listed order. Independent later work may proceed after a documented external blocker, but no dependent checkpoint may assume the blocked behavior works. Carry the blocked requirement accurately to final status.

## F. Detailed milestones and checkpoint packets

Each packet is the scope for one implementer assignment. The orchestrator retains branch, ledger, verification and commit ownership. Native checks specified here are consolidated in M8 where practical; until then record native status as pending, not passed.

### M0-A — Correct branch and baseline

Execute section A0 in full. This is a hard prerequisite to every packet below. After bootstrap, confirm current source ownership, focused test selections and existing defects; record material deviations in the same execution ledger. No separate broad discovery project is required.

### M1-A — Scalar dispatch, aliases and infix remainder

**Objective / ownership:** Deliver CALC-01/02 in `calculation_pad/document.rs`, `operators.rs`, `quantity.rs`, with narrow `unit_conversion.rs` changes only if the owning lexical boundary requires them.

**Tasks:** Trace binary-first quantity dispatch; recognize complete prefixed tokens; implement contextual built-in spelling normalization and `pi`/`pie`; lower infix `mod` without changing function/bare-variable behavior.

**Behavior / edge cases:** The scalar example table passes. Cover binary alone/first/later/in a function, uppercase prefixes, negatives, malformed prefixes, signed/zero divisors and bound variables.

**Regression constraints:** Preserve `10 b`, `10 B`, `5 min`, min-function parsing, Unicode/braced names, `floor_price`, exact integers, original source/CRLF and existing precedence.

**Verification:** Focused document/operator/quantity regression tests, including a binary-first case that the existing hexadecimal-first test misses. Use meaningful expected values and line diagnostics.

**Acceptance:** New spellings behave as specified; old valid expressions retain results; invalid inputs remain bounded errors.

**Commit:** `fix(calculation-pad): [M1-A] repair numeric dispatch and contextual aliases`.

### M1-B — Typed vector evaluation

**Objective / ownership:** Deliver CALC-03 in the calculation domain, session/results and necessary comparison/copy consumers.

**Tasks:** Introduce a bounded typed vector value and expression owner; share scalar component rules; integrate named/`rN` dependencies, products, formatting and reevaluation. Update exhaustive matches and explicit unsupported comparisons.

**Behavior / edge cases:** All vector examples, symbolic products and equal-length element-wise operations work. Bound nested syntax and allocations; reject shape/type/zero/nonfinite errors locally. Verify a vector variable and physical-row reference after editing an upstream line.

**Regression constraints:** Ordinary scalar, quantity and programmer expressions retain their existing path. No new `ans`, statistics, matrices or lossy exact-integer coercion.

**Verification:** Focused vector success/error tests plus dependency/undo/reopen and formatted-copy roundtrip. Reuse existing session fixtures; add only necessary new cases.

**Acceptance:** Typed vectors survive the full Pad workflow and failures do not poison unrelated lines.

**Commit:** `feat(calculation-pad): [M1-B] add bounded vector arithmetic`.

### M1-C — Searchable examples and independent reference host

**Objective / ownership:** Deliver HELP-CALC-01/02/03 through `examples.rs`, `calculation_pad_dialog.rs`, and narrow host registration in `gui/mod.rs`/`render.rs`.

**Tasks:** Expand typed example expectations for vectors; expose compact examples/keywords; move all reference body content into the scroll area; add pinned search; give help its own UtilityHostState/identity with Pop Out/Dock and accepted effects.

**Behavior / edge cases:** Searches reveal matching closed-group content and no-match state; small windows remain usable. Open/copy/dock/OS-close/reopen help without changing Pad source, history or recovery. Preserve query/scroll through host migration.

**Regression constraints:** Do not use the Pad's own host flag, force unrelated ROOT focus, duplicate its session, or accept copy/close from a retired viewport.

**Verification:** Catalogue expectations run through the real evaluator; targeted search/layout and host-generation tests extend existing help-copy and rejected-pass cases. Consolidated native resize/scroll/pop-out smoke in M8.

**Acceptance:** Complete body is reachable, search remains fixed, examples are correct, and independent host lifecycle is coherent.

**Commit:** `feat(calculation-pad): [M1-C] add searchable detachable reference`.

### M2-A — Inspector working mode, bit groups and named spans

**Objective / ownership:** Deliver BITS-01/02/03 in `programmer_inspector_dialog.rs` and `calculation_pad/inspector.rs`.

**Tasks:** Connect mode selection to checked conversion; preserve a clear attempted target for explicit reinterpretation on overflow. Add distinct grid grouping and visual annotation span rendering.

**Behavior / edge cases:** Render exactly 8/16/32/64/128 bits for the actual working mode; handle bit 127 without invalid shifts. Exercise narrowing failure, reinterpretation, narrow layouts, overlapping/cross-group/out-of-range annotations.

**Regression constraints:** No silent truncation, hidden mode mismatch, output-format change, lost annotation or immediate source mutation. Preserve insertion/revision protections.

**Verification:** Focused model and egui interaction tests for mode/group/span behavior; native group/mode/label inspection in M8.

**Acceptance:** Successful mode selection immediately changes layout; failed conversion is explicit; bit groups and names accurately map to their ranges.

**Commit:** `feat(programmer-inspector): [M2-A] synchronize modes and visualize bit ranges`.

### M3-A — Deflate extraction and specific layout diagnostics

**Objective / ownership:** Deliver ZIP-01 in `archive/{writer,native_io,plan,model,session}.rs` and narrow GUI error mapping.

**Tasks:** Add bounded streaming Deflate decompression from the validated held source, reuse staging and CRC, keep output accounting on actual decompressed bytes, and check cancellation between bounded reads/writes. Inspect adapter requirements instead of assuming OwnedStage is Write+Seek.

**Behavior / edge cases:** Stored/Deflated, empty files/directories and supported descriptors/ZIP64 succeed. Truncation, corruption, excessive expansion, overlap and inconsistent metadata remain failures. If another valid layout fails, isolate its precise check and minimally reproduce it.

**Regression constraints:** Removing a Stored-only guard is insufficient; compressed bytes must never be published as plaintext. Do not bypass secure source/destination validation through pathname-based convenience extraction.

**Verification:** Replace the deliberate Deflate-rejection expectation with successful content/CRC verification; retain malformed-layout tests as meaningful separate cases. Add focused failure/cancellation tests where the new streaming boundary changes behavior.

**Acceptance:** Normal Deflated archives extract correctly with original safeguards intact and specific diagnostics for genuinely unsupported layouts.

**Commit:** `fix(zip): [M3-A] extract deflated entries through the bounded writer`.

### M3-B — Default and bulk archive selection

**Objective / ownership:** Deliver ZIP-02 in `archive/session.rs`, `plan.rs::ArchiveSelection`, and `zip_extractor_dialog.rs`.

**Tasks:** Initialize selection once on accepted new-load completion; add Select All/Clear Selection; make bulk changes one session revision and preview invalidation.

**Behavior / edge cases:** Include directory records and distinct duplicate-name physical entries. Select All is archive-wide despite filters. Initialize all-selection only for an accepted successful load/reload; failed or stale completions cannot select a newer catalog. Preserve current clear-on-open semantics. Repeated polls and dock transitions preserve later choices within the loaded archive.

**Regression constraints:** Unsupported/unsafe entries remain disclosed and validated; selection is never an extraction authorization bypass.

**Verification:** Focused session selection/revision/stale-completion tests and one GUI control test. No repeated filesystem extraction needed for selection-only logic.

**Acceptance:** A newly opened archive starts fully selected; user bulk/individual choices persist correctly.

**Commit:** `feat(zip): [M3-B] select archive members by default and in bulk`.

### M3-C — Safe ZIP creation domain and writer

**Objective / ownership:** Implement ZIP-03's manifest, collision validation, worker and output lifecycle under `src/archive/`, reusing native primitives and the existing ZIP dependency.

**Tasks:** Model accumulated source entries and archive-relative destinations; freeze/revalidate sources; recursively enumerate allowed folders with resource limits; implement cancellable compression and staged no-clobber publication.

**Behavior / edge cases:** Cover multiple roots, Unicode, empty directories, duplicate basenames/case aliases, file/directory conflicts, inaccessible/changed inputs, pre-existing destinations, output-inside-source recursion and cancellation at finalization.

**Regression constraints:** Sources remain untouched; output publication is atomic/no-clobber; a failure cannot report success or leave a published partial ZIP. Existing extraction lifecycle remains intact.

**Verification:** Temporary-directory manifest/collision tests; generated ZIP catalogue/extraction roundtrip; failure/cancel cleanup and output-recursion cases. Add a narrow I/O adapter only where existing types require it.

**Acceptance:** A creation job produces a valid bounded ZIP or a precise non-destructive failure.

**Commit:** `feat(zip): [M3-C] add staged multi-source archive creation`.

### M3-D — ZIP Create UI and end-to-end workflow

**Objective / ownership:** Expose completed creation through `zip_extractor_dialog.rs`, existing command/plugin metadata, and necessary accepted UI effects.

**Tasks:** Add Extract/Create, repeated file/folder selection, staged tree/path editing/removal, conflict feedback, output chooser, progress/cancel and clear success/failure reporting. Reuse picker/host provenance.

**Behavior / edge cases:** Selection dialogs may be canceled; stale picker completions cannot target a new draft/host. Source additions accumulate. Mode/source controls are coherently disabled or guarded during work. Existing output selects another name.

**Regression constraints:** Saved `zip:open` actions still open Extract; new creation does not implicitly extract, overwrite or modify sources.

**Verification:** Focused state/effect tests and one native multi-source create → reopen → extract flow in disposable folders at M8.

**Acceptance:** A user can build and inspect a ZIP from different directories entirely through the utility, with clear recoverable conflicts.

**Commit:** `feat(zip): [M3-D] expose create and extract workflows`.

### M4-A — Visual presentation, fonts and navigation

**Objective / ownership:** Deliver DIAG-02/03/05/08 through `gui/diagram_studio.rs` and `diagram/visual_canvas.rs`.

**Tasks:** Repair marquee painter ordering; retain existing selection semantics; implement stable minimap drag; replace font text inputs with supported choices; size visual preview from remaining area with coherent Fit/manual-camera behavior.

**Behavior / edge cases:** Ctrl+empty drag adds and ordinary drag replaces; object drag moves. Minimap preserves grab offset without feedback. Unknown imported fonts remain preserved. Resize both dimensions, narrow dialogs and zoomed scenes.

**Regression constraints:** Navigation changes neither document history nor dirty state. Preserve Mermaid behavior, middle pan, wheel zoom and actual renderer/export font agreement.

**Verification:** Egui pointer sequences and canvas-rectangle tests establish selection, camera movement and resize. Inspect emitted paint ordering for the marquee; no pixel-golden campaign.

**Acceptance:** Marquee stays visible, minimap drags smoothly, dropdown reflects real fonts and the viewer uses available height.

**Commit:** `fix(diagram): [M4-A] repair visual navigation and presentation`.

### M4-B — Structural clipboard and keyboard ownership

**Objective / ownership:** Deliver DIAG-01 using visual selection, accepted mutation/effect paths, document transactions and existing clipboard services.

**Tasks:** Implement bounded structural copy payload and dependency/ID remapping; consume actual Copy/Cut/Paste/key events exactly once; add multi-delete/select-all/history shortcuts under canvas ownership.

**Behavior / edge cases:** Copy/paste between visual documents, grouped shapes with internal connectors and embedded assets; repeated paste gets fresh IDs and offsets. External/unrecognized or oversized payloads fail clearly without mutation. Delete/cut preserve frame/title invariants.

**Regression constraints:** Focused text fields, inline labels, menus and read-only viewers keep correct ownership. Failed clipboard copy never triggers cut. Stale/discarded passes cannot edit a document.

**Verification:** Focused payload/remapping and atomic transaction tests; input-routing tests distinguish text editing and canvas commands. Native clipboard/key interaction in M8.

**Acceptance:** Conventional shortcuts work for real multi-selection and each destructive/edit operation has a coherent undo step.

**Commit:** `feat(diagram): [M4-B] add visual clipboard and keyboard editing`.

### M4-C — Persistent centered inline labels

**Objective / ownership:** Deliver DIAG-04 in the visual model, GUI, codec/projection/export and necessary import/template construction.

**Tasks:** Add a minimal validated owned-label/association representation. Do not misuse frame-only parent membership. Route double-click hits on either a shape or its owned text to the same focused editor; retain unrelated free-text behavior. Implement explicit accepted/cancel paths; connect movement/resize/rotation/copy/deletion and extend M4-B's clipboard payload/remapping for label ownership.

**Behavior / edge cases:** Repeated double-click edits one label, including an empty/new label. Multiline input follows Enter/Shift+Enter/Escape contract. Document replacement or ownership/revision loss cancels stale editing. Preserve old unassociated free text.

**Regression constraints:** Read existing documents; no geometry-based relinking of arbitrary old labels; no per-frame history entries or duplicate label objects.

**Verification:** Model serialization and movement/clipboard relationships; inline focus/commit/cancel tests, including double-clicking the visible existing label to re-edit; one save/reopen flow. Verify legacy supported files decode unchanged.

**Acceptance:** A shape's text is immediately editable, remains centered/associated and survives history, transforms and persistence.

**Commit:** `feat(diagram): [M4-C] add owned inline shape labels`.

### M4-D — Persistent editable connector bends

**Objective / ownership:** Deliver DIAG-06 across `visual.rs`, `visual_scene.rs`, `visual_canvas.rs`, GUI, codec and outputs.

**Tasks:** Add bounded manual routing/control geometry, selection handles, add/remove/reset operations, stable gesture preview and single release commit. Route viewer/export through the same model. Extend M4-B's clipboard payload, validation and paste transformation for manual routes: remap endpoints and translate interior/control geometry with the copied subgraph so its shape is preserved. Add narrow draw.io point mapping and precise unsupported-export reporting where necessary.

**Behavior / edge cases:** Edit a bend then move an endpoint; check self-loop/parallel/rotated-anchor cases directly affected by manual routes. Reject nonfinite/excessive points. Reset restores automatic routes. Escape and stale owner cancel.

**Regression constraints:** Automatic routing tests retain meaning; anchors stay attached; old documents default to automatic. Do not flatten custom geometry on save/export.

**Verification:** Extend directly affected connector geometry/hit tests; roundtrip save/reopen and undo; copy a bent connector with both endpoint objects, repeatedly paste it, and verify route translation, fresh endpoints, undo and an unchanged original. Inspect SVG/PNG geometry and supported draw.io waypoint roundtrip.

**Acceptance:** Bend edits are visible, durable and consistent in editor, viewer and outputs.

**Commit:** `feat(diagram): [M4-D] persist and edit connector bends`.

### M4-E — Expanded visual templates and workflow check

**Objective / ownership:** Deliver DIAG-07 through `visual_templates.rs` and existing template UI/catalogue tests.

**Tasks:** Add four named examples using existing primitives and new owned labels; keep unique slugs and the five existing entries. Ensure meaningful editable content, sensible initial Fit and valid routes.

**Behavior / edge cases:** Dirty-document replacement requires the current decision flow. Templates work at typical and narrow viewport sizes; no clipped invalid model content.

**Regression constraints:** No external assets, new rendering framework or replacement of existing templates.

**Verification:** Extend the current five-template catalogue test for all entries and valid projections; save/reopen/edit one representative new template. Include in the consolidated visual smoke.

**Acceptance:** Nine useful native templates are available and remain ordinary editable visual documents.

**Commit:** `feat(diagram): [M4-E] expand visual starter templates`.

### M5-A — Timeout-capable Explorer UIA initialization

**Objective / ownership:** Deliver EXPLORER-01 at `ExplorerUia::connect` and narrow native preflight/evidence integration.

**Tasks:** Activate the documented modern COM class/interface using existing bindings; set existing finite connection/transaction timeouts; retain worker apartment, cancellation and total deadline. Improve capability failure context.

**Behavior / edge cases:** Supported initialization proceeds; actual unsupported capability stops before opening any window. Preserve attempted-versus-verified outcomes and cancellation while a tab operation is in flight.

**Regression constraints:** No unbounded fallback, synthetic focus workaround, arbitrary sleep, repeated bootstrap windows or success without tab evidence.

**Verification:** Inject initialization failure/success where feasible; assert no window request on preflight failure. One native Windows 11 two-folder/tab run is mandatory for the supported-path claim.

**Acceptance:** The observed legacy activation failure is corrected on a supported host, and failure paths remain bounded and truthful.

**Commit:** `fix(explorer-tabs): [M5-A] initialize timeout-capable UI Automation`.

### M5-B — Existing folder bookmark picker

**Objective / ownership:** Deliver EXPLORER-02 through the Explorer dialog, folder-list domain and the existing folders plugin data owner.

**Tasks:** Expose a bounded read snapshot/accessor as needed; add searchable label/alias/path picker and copy chosen paths into the draft through existing validation/revision rules.

**Behavior / edge cases:** Duplicate names show distinct paths; missing/deleted entries are reported; canceled picker preserves the draft; repeated additions respect ordering and the 32-item cap. Existing saved path lists need no schema migration.

**Regression constraints:** Manual paths/reorder/save remain usable; bookmark changes after insertion never silently change the Explorer list.

**Verification:** Focused picker/import/order/cap tests and one native picker-to-two-folder run combined with M5-A.

**Acceptance:** Both manual paths and saved `f` links can populate a verified Explorer Tabs workflow.

**Commit:** `feat(explorer-tabs): [M5-B] select saved folder links`.

### M6-A — Recorder window discovery from a cold cache

**Objective / ownership:** Deliver RECORD-01 in the recording dialog and existing WindowCatalog notification path.

**Tasks:** Request enumeration at explicit lifecycle events; consume a coherent published generation; repaint on completion; distinguish discovery states and preserve identity validation.

**Behavior / edge cases:** Starting with an empty generation-zero cache must eventually populate eligible windows. Refresh while discovery is in flight must settle correctly. Closed/reused HWNDs and unchanged snapshots cannot start the wrong capture.

**Regression constraints:** No blocking UI-thread `refresh_and_wait`, duplicate enumerator, steady idle polling or display-list regression.

**Verification:** Fake/shared catalogue trigger→publish test with a cold cache, selection preservation and stale identity case. Native start with an eligible ordinary window in M8.

**Acceptance:** Window selection works without first using another feature to warm the catalogue.

**Commit:** `fix(recording): [M6-A] request and publish window discovery`.

### M6-B — Preview and active recording-region indicators

**Objective / ownership:** Deliver RECORD-02/03 through recording controller/dialog/capture integration and narrow reuse of the passive visual overlay service.

**Tasks:** Implement short Show region preview and a separately owned active-generation indicator for Display/region capture. Use its frozen physical region bounds; exclude overlays from captured content through the supported existing platform boundary or equivalent verified placement. Preserve the OS indicator for Window capture and do not reuse its stale initial rectangle. Clean up on every lifecycle exit. Check documented WGC border capability without expanding app packaging.

**Behavior / edge cases:** Negative monitor origins, mixed DPI, region change, preview expiry, active selector conflict, stop/discard/error/target loss/shutdown and immediately starting a later recording. The indicator must show the actual frozen request.

**Regression constraints:** Preserve correct encoded crop, existing stop/countdown/encoder behavior and overlay ownership for other tools. Never repeatedly restart a temporary preview every frame to fake an active lifetime.

**Verification:** Geometry/lifetime tests plus a short native region recording: inspect region outline, encoded dimensions/content, absence of the app overlay in output and cleanup. Record the actual OS-border capability/consent result separately.

**Acceptance:** Preview and active outline accurately identify the selected region; native capture/cleanup is verified or precisely marked unverified. OS-owned border limitations are disclosed without misleading success claims.

**Commit:** `feat(recording): [M6-B] show precise recording regions`.

### M7-A — Command-only help

**Objective / ownership:** Deliver COMMAND-01 in `help_window.rs` using current plugin metadata/filter helpers.

**Tasks:** Remove only the four static sections; keep plugin descriptions/commands/examples, pinned filter and normal settings behavior. Adjust scroll-test content targets to real command rows.

**Behavior / edge cases:** `folder size` finds the appropriate plugin; empty/no-match queries are clear; long plugin help remains reachable in a small window.

**Regression constraints:** Do not remove actual `replace` commands, rewrite Quick Help, change launch parsing or weaken the existing bounded-scroll test by removing its core assertions.

**Verification:** Focused filter and bounded-scroll tests plus native help filtering in M8.

**Acceptance:** The filtered display is command/plugin help, without unrelated fixed tutorial material.

**Commit:** `fix(help): [M7-A] focus command help on plugin usage`.

### M7-B — Recycle prefix boundary

**Objective / ownership:** Deliver COMMAND-02 in `plugins/recycle.rs`, narrow query-routing metadata and relevant launcher tests.

**Tasks:** Match the supported complete command and declare its prefix consistently. Trace the actual provider and launcher result path before changing any general search code.

**Behavior / edge cases:** `rec` and surrounding-whitespace/mixed-case variants work. `record`, `recorder` and `recent` do not emit `recycle:clean`. Verify the real recorder result remains discoverable.

**Regression constraints:** Preserve fuzzy search, aliases and valid resolved plugin results. Do not execute Recycle Bin cleanup during verification.

**Verification:** Focused `tests/recycle_plugin.rs` query tests and a launcher integration test under exact and fuzzy settings. Leave the destructive ignored cleanup test ignored.

**Acceptance:** Typing `record` no longer shows Clean Recycle Bin because of the `rec` prefix collision.

**Commit:** `fix(search): [M7-B] enforce recycle command boundaries`.

### M8-A — Focused native acceptance, review and final handoff

**Objective / ownership:** Establish complete feature evidence and deliver the recorded final tip; orchestrator and independent reviewer own this checkpoint.

**Tasks:** Use the final cached development build for the consolidated native flows in H. Review the cumulative diff from `baseline_sha`, resolve feature-related findings, perform final first-party lint review, update user-facing help/README only for changed behavior, and close the execution ledger.

**Behavior / edge cases:** Each requirement is marked implemented/verified/blocked accurately. Previously unknown environmental failures are diagnosed within the stated budget; independent completed work remains preserved.

**Regression constraints:** No broad suite, unrelated cleanup, old acceptance campaign, extra branches, intermediate merges or fabricated native evidence.

**Verification:** Selected affected test set once after final relevant changes; one independent review; final native flows; ancestry/staged-path/status inspection.

**Acceptance:** Section J passes, or final status explicitly reports LIMITED/BLOCKED with exact missing requirements. A push failure alone does not make verified local implementation incomplete.

**Commit:** `docs(productivity-tools): [M8-A] record verification and chained handoff`, when a coherent tracked report/help change exists. Use separate small fix commits for substantive review corrections; never disguise implementation as a docs-only commit. Report `final_tip_sha` after the last intentional commit. Avoid a bookkeeping loop trying to embed a commit's own SHA inside itself.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

The packet commit boundaries above are active. M0-A is first; all later checkpoints remain on its one branch. Commit a coherent increment after its focused checks before starting materially different work. Do not accumulate all requested tools into one final commit.

Before committing, verify the intended branch, HEAD, staged paths, and unrelated user work. Stage explicit paths. A small remediation commit is preferable to history rewriting. After each commit, attempt one ordinary push; preserve local progress and continue if it fails.

Maintain per-checkpoint implementation, focused-test, native-evidence and push status. Record the final branch, parent/baseline/checkpoints/final SHA and clean/dirty state. The next Goal's required base is this final tip, including local-only commits if push failed. Do not merge/rebase/squash/cherry-pick intermediate branches to advance the chain.

## H. Verification strategy and practical budget

### Focused automated checks

Use the existing `multi_launcher` package and cached `fast-dev` profile when suitable. Verify current filters match runnable tests. Run one Cargo/build/test job at a time; native stateful interactions run serially.

Illustrative commands, to be narrowed to the affected checkpoint and verified against current test names:

```powershell
cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(calculation_pad::)'
cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(gui::calculation_pad_dialog::) | test(gui::programmer_inspector_dialog::)'
cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(archive::) | test(gui::zip_extractor_dialog::)'
cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(diagram::visual) | test(diagram_studio_visual_)'
cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(explorer_tabs::) | test(gui::explorer_tabs_dialog::)'
cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(gui::recording_dialog::) | test(recording::windows::) | test(window_catalog::)'
cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(help_window::)'
cargo nextest run -p multi_launcher --cargo-profile fast-dev --test recycle_plugin --no-fail-fast -E 'test(search_returns_action)'
cargo build -p multi_launcher --profile fast-dev --bin multi_launcher
```

These are selection examples, not a mandate to run every listed module repeatedly. Add the newly named boundary/host/copy tests and affected codec/export tests to exact selections; the legacy single Recycle test alone is insufficient to validate M7-B. Keep destructive/real-profile tests ignored.

Use the repository's existing execution wrapper where it supplies needed process ownership/timeouts. Aim for focused pure-test execution batches of at most about 100 seconds, excluding compilation; narrow or split a slow batch instead of killing legitimate compilation or changing global test policy. Record the actual bound used for native waits. Do not invent unsupported nextest flags.

Before constructing test applications, set Paste Sequence `next_hotkey = None` and `cancel_hotkey = None` per `AGENTS.md`. Preserve production defaults and normal error reporting.

Tests should prove an observable behavior with useful failure names/messages. Avoid redundant existence tests, implementation-mirroring tests and unrelated assertion-style migrations.

### Consolidated native flows

Build and launch on a supported Windows environment. Use disposable documents, folders and output files. Reuse the same valid development build until code changes require rebuilding.

| Flow | Required observations |
| --- | --- |
| Calculation/Inspector | Evaluate representative scalar/vector examples; search `floor`/`sqrt`; scroll a small reference; pop out/dock/close without Pad changes; switch fitting/non-fitting widths, group bits and inspect named spans. |
| ZIP | Open a normal Deflated archive with default selection; clear/select all under a filter; add files from two roots plus a folder; resolve a collision; create, reopen and extract; verify contents and no source mutation. |
| Visual diagrams | Multi-select with visible ordinary/Ctrl marquee; copy/paste/delete/undo with canvas versus text focus; double-click/re-edit centered text; font choice; continuous minimap drag; bend/save/reopen/export; enlarge viewer vertically; open a new template. |
| Explorer Tabs | Add one saved `f` link and one manual path; run two folders; verify one attributable Explorer window and both requested tab identities; check failure reporting if capability is absent. |
| Recorder | Start with cold window cache; refresh/select a real eligible window and start/stop a short window recording; then preview a display region and record it briefly. Inspect output crop and absence of app outline in frames; stop/discard and verify indicator cleanup. |
| Command discovery | Search Command Help for `folder size`; verify removed sections are absent; type `record` and inspect recorder results without Clean Recycle Bin; query `rec` without executing it. |

Capture screenshots or equivalent interaction receipts when supported, plus textual facts screenshots cannot establish (actual archive bytes, recording output dimensions, document persistence, tab identities). No automated pixel-diff suite is required.

### Stop rules

- One normal scoped verification run per checkpoint, then rerun only affected failing checks after correction.
- For a native/environmental failure, allow the initial attempt plus **up to two evidence-driven corrective attempts for the same cause**. Stop repeated attempts when no new hypothesis or evidence exists.
- Compilation and task-caused test errors are implementation feedback to fix, not automatic reasons to ask the user. The retry budget is not permission to abandon ordinary correctable code defects.
- One independent final review, with focused follow-up only for concrete findings. One final first-party Clippy pass on affected targets when feasible; no repeated release builds, full-suite testing, performance campaign or vendored cleanup by default.
- Broaden checks only for a concrete uncovered risk introduced by the change and record why.
- If Windows/native capture or required permissions are unavailable, finish independent implementation and tests, preserve the evidence, and report those exact native checks as unverified. Do not claim complete functional acceptance or transfer intermediate testing to the user.

## I. Risks and bounded blocker handling

| Risk | Required mitigation |
| --- | --- |
| Calculator routing changes ordinary units/identifiers | Fix token ownership; keep existing scalar/unit/exact routes and source bytes; test the ambiguous binary-first and bound-name cases. |
| Vector values bypass dependencies or silently coerce types | Typed values and bounded common component evaluation; integrate named/`rN` flow and all result consumers. |
| New help host duplicates session or accepts stale input | Independent help identity with the existing retained-pass/owner lifecycle; exact copy/close guards. |
| Inspector narrowing loses data | Checked immediate conversion; explicit reinterpretation and accurate active/target state. |
| Deflate expands beyond declared or allowed output | Validate actual streamed byte counts, CRC, cancellation and quotas before publication. |
| ZIP creation overwrites/recurses/archives changing files | Frozen validated manifest, identity checks, collision feedback, no-clobber staging/publication and output-source exclusion. |
| Diagram edits are only UI-deep | Persist label/route ownership through validation, history, codec, projection, clipboard and outputs. |
| Clipboard event/focus errors edit the wrong document | Actual egui event ownership plus accepted transactions and fresh IDs; text/viewer/modal negative cases. |
| UIA support is falsely reported or deadlines removed | Modern documented activation plus native verification; keep preflight, finite calls and truthful partial outcomes. |
| Recording indicator depicts setup rather than active request | Freeze selected display-region bounds with the active generation and retire every owned outline; preserve Window mode's OS indicator rather than freezing its movable initial bounds; inspect actual encoded content. |

Distinguish a requested-feature blocker, an unrelated pre-existing issue, and optional future work. Only the first may block the affected acceptance condition. Continue independent packets when safe; never declare a blocked requirement done.

For an actual blocked/deferred/paused stop, follow the repository's concise tracked stall-report convention under `docs/plans/execution/<queue-id>/stalls/` or `docs/plans/execution/standalone/productivity-tools-refinement/stalls/`. Record exact checkpoint, time, branch/HEAD/upstream, expected/actual behavior, command/diagnostic, attempts, cleanup, preserved work, unresolved verification and smallest safe next action. The existing operational ledger remains authoritative. If Git reporting is unsafe, preserve the report in the existing external ledger until safe reconciliation. Do not leak private paths/payloads in uncontrolled logs.

### Primary platform references

These justify the narrow native strategy; verify actual runtime capabilities during implementation.

- Microsoft: [UI Automation objects](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-entry-objects) and [CUIAutomation8](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/legacy/hh448746(v=vs.85)) identify the timeout-capable implementation of IUIAutomation2.
- Microsoft: [IUIAutomation2](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nn-uiautomationclient-iuiautomation2) documents connection and transaction timeout properties.
- Microsoft: [GraphicsCaptureSession.IsBorderRequired](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.isborderrequired) documents supported border control, consent/capability requirements and cases where a border remains.

## J. Definition of done

Declare **COMPLETE** only when:

- Every applicable requirement in C is implemented and its required functional checks pass.
- All packets have completed focused verification, and the native flows have actual evidence on a supported environment. No known requested-feature blocking defect remains.
- Existing directly affected behavior/data formats/host lifecycles are preserved.
- The independent review's feature-related findings are resolved; new first-party lint issues are handled and unrelated pre-existing findings are identified separately.
- Code/doc changes are intentional and committed with the active checkpoint cadence on the correct child branch.
- Correct ancestry is verified, push state is factual, and `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs and `final_tip_sha` are recorded.
- The final report explicitly tells the next Goal to branch from this `final_tip_sha`, not from `master`/`main`.
- No Git worktree, auxiliary checkout/clone, per-agent branch or intermediate branch merge was used.
- Any capability-dependent Windows border limitation is accurately disclosed and the required precise app outline is verified.

Use **LIMITED** or **BLOCKED** with exact missing requirement IDs when code is complete but required native verification is unavailable, or a genuine external blocker remains. Never equate source review, compilation or mocked tests with native acceptance. Failed pushes alone do not invalidate complete local work.

## K. Final delivery report

Provide a concise factual report covering:

1. Overall status and completed milestones/checkpoints, with incomplete requirement IDs if any.
2. Main behavior changes and important defaults: contextual aliases/remainder, vector semantics, checked Inspector conversion, ZIP layout/collision policy, persisted diagram labels/routes, folder-path copies and native indicator limitations.
3. Relevant files/components changed.
4. Actual commands/tests and results, selected test counts where available, and scoped checks deliberately not required.
5. Native UI observations and screenshot/receipt paths; clearly separate verified, unverified and capability-limited behavior.
6. Parent branch/tip, task branch, baseline, checkpoint SHAs, final tip, actual push success/failure and clean/dirty state.
7. Exact next-Goal instruction: **create its fresh child branch directly from this Goal's `final_tip_sha`; do not restart from the integration branch or merge intermediate branches**. Confirm the single-checkout/no-worktree rule was honored.
8. Remaining issues and a short final manual-review suggestion for the user. Do not require intermediate manual testing.

Stop when this contract and its bounded validation are satisfied. Do not continue into optional features or unrelated cleanup.

