# Codex Goal — Desktop Productivity Expansion

**Status:** Approved for implementation.  
**Companion:** `docs/plans/CODEX_KICKSTART_DESKTOP_PRODUCTIVITY_EXPANSION.md`  
**Goal ID:** `desktop-productivity-expansion`  
**Decision record:** Q1 A — exact programmer arithmetic, preserving existing decimal calculations. Q2 A — animated GIF and MP4, without audio. All other defaults in this contract are authorized.  
**Scope:** Selected items 1–46, 61, 88, 89, and 119. There are seven sequential work categories, not seven independent integration branches.

## A0. Mandatory Chained Git Branch Bootstrap — Before All Application Work

### First execution step: instructions → exact parent → child branch → M0-A → push

Read `AGENTS.md`, applicable local instructions, this entire Goal, and the companion Kickstart. In the **single existing repository checkout**, perform read-only Git inspection before application edits, implementation builds/tests, or implementation checkpoints:

```powershell
git status -sb
git branch --show-current
git rev-parse HEAD
git remote -v
git branch -vv
```

Also inspect the index, untracked files, any merge/rebase operation, upstream/default-branch information, and the existing queue or predecessor handoff. Do not interpret the current branch as proof of the correct parent.

**Recorded continuation for this request:** the accepted/frozen Studio Usability Recovery handoff is branch `fix/studio-usability-root-jump-recovery`, final tip `7c49f083474afcdc0eb57eb3e88bbd32561165a8`. Its implementation and focused review were accepted; its native gate remained `ENVIRONMENT_BLOCKED`. Verify this commit and the handoff against the actual repository/queue before creating C01. Relevant historical records are under `docs/plans/execution/standalone/studio-launcher-usability-recovery/`. They identify ancestry and inherited verification limitations, not additional work for this Goal. Do not restart that recovery campaign or declare its outstanding native checks passed.

If a later explicit user-authorized handoff supersedes this recorded predecessor, use that verified exact final tip and record the authority. Conflicting or incomplete records are a bounded ancestry blocker; do not guess. **This is a continuing chain, not an invitation to restart from master.** Only the first task of a genuinely new chain may default to the up-to-date primary integration branch. A clearly documented current-HEAD base is permitted for a genuinely new chain only when no predecessor exists and user work is protected.

For **every** category in Section F:

1. Identify `parent_branch` and the exact committed `parent_tip_sha`. For C02 onward, this is the preceding category's actual final tip, including any final reporting commit. Fetch safely when the required ancestor exists only remotely; do not substitute a stale remote tip for a newer local predecessor.
2. Select a fresh, unique, lowercase kebab-case branch using the category's suggested `feat/` prefix/name. The Goal and Kickstart describe **one shared task branch per category**, not one branch for each document or agent.
3. Protect existing work. Never automatically reset, hard-pull, stash, rebase, merge, force-checkout, overwrite, or stage unrelated changes. If switching to the required parent is unsafe, record the exact blocker; do not select another parent. Never commit user changes as a task baseline.
4. Create and switch directly from the verified parent SHA, for example `git switch -c feat/utility-dock-popout <parent_tip_sha>`. Confirm the new branch's initial HEAD equals that SHA. Do not implement on the predecessor, `master`, or `main`.
5. Create this category's own **M0-A** baseline commit: a small execution-ledger update, or an intentional empty commit when needed. Example subject: `chore(productivity): [C01/M0-A] establish utility-host branch baseline`. Verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`; record `baseline_sha` separately.
6. Attempt one normal non-force upstream push, normally `git push -u origin HEAD`, when the intended remote and permissions are available. Record success or the exact failure/unavailability. Retain local commits and continue safe local work after push failures; do not bypass permissions or enter retry loops.
7. Keep all subsequent checkpoints of that category on the same branch. No branch per milestone, checkpoint, or agent. Attempt one ordinary best-effort push after each checkpoint commit.
8. At category handoff, commit the intended changes and reporting before recording the actual `final_tip_sha`. The next category branches directly from that exact commit. Do not carry uncommitted partial work silently across categories.

**No Git worktrees:** do not run `git worktree add`, `git worktree move`, or `git worktree prune`. Do not create auxiliary checkouts, secondary/helper clones, or agent-specific implementation branches. All edits and branch transitions are sequential in the existing checkout. Read-only agents may work concurrently when safe; there is one source writer and one Git owner.

**No intermediate integration:** do not merge, rebase, squash, or cherry-pick category branches to advance the sequence. Their ancestry already contains preceding work. One final cumulative integration PR/merge may be prepared at the approved integration boundary; do not automatically merge into the primary branch or rewrite this chain to accommodate upstream movement.

### M0-A acceptance, repeated for C01–C07

The unique category branch is active; its initial parent is the verified predecessor; the baseline is a direct child of that parent; the initial push outcome is accurately recorded; no unrelated work was staged/lost; no extra checkout/worktree/agent branch was created. No implementation starts before these conditions are satisfied, except that push failure is explicitly nonblocking.

### Execution records and truthful hash recording

Use the existing authoritative queue ledger when one exists. Otherwise maintain one tracked execution record at `docs/plans/execution/standalone/desktop-productivity-expansion/EXECUTION.md`. Keep requirement coverage, category state, checkpoints, verification, and inherited limitations there rather than creating competing plans.

Record `parent_branch`, `parent_tip_sha`, category branch, `baseline_sha`, significant checkpoint SHAs, actual push status, and checkout state. After the final tracked commit, obtain the actual `final_tip_sha` and write it to the external queue receipt or an ignored receipt under `target/desktop-productivity-expansion/`, and the final response. The next category's M0-A can record its predecessor's now-known final SHA in tracked history. **Do not create an endless sequence of commits attempting to embed a commit's own hash inside itself.** The report/receipt must identify the actual final HEAD, not an earlier implementation commit.

## A. Goal / Desired Outcome

Expand Multi Launcher in seven concrete directions while preserving existing behavior:

| Category | User-visible outcome |
| --- | --- |
| C01 — Dock / Pop Out | The requested editing and utility dialogs move between launcher-hosted and native windows, using Calculation Pad's interaction model and preserving the same live session. |
| C02 — Calculation Pad | Text-based exact programmer arithmetic, a separate programmer inspector, comparison, useful unit algebra, and accurate examples/reference material. |
| C03 — Diagram Studio | Existing Mermaid authoring remains intact; a richer independent visual canvas supports direct diagram editing, safe conversions, exports, and bounded draw.io interoperability. |
| C04 — Explorer tabs | An ordered saved folder list opens as verified tabs in one deliberately targeted Explorer window. |
| C05 — Folder size | Read-only, cancellable folder-size analysis with progressive results and honest accounting. |
| C06 — File locks | Read-only inspection of processes reported as using a selected file, without force-unlock or termination. |
| C07 — Screen recording | Short, local, silent GIF and MP4 recordings of a region, window, or display, with bounded resources and dependable cleanup. |

Success means usable commands and interfaces, saved-data compatibility, focused tests, meaningful native verification where available, and a cumulative committed branch chain. A mock backend, an inaccessible implementation, a screenshot alone, or a permanently unsupported stub does not satisfy a feature.

## B. Current Codebase Findings

These are source-inspection findings, not claims that a build or native interaction passed. Reconfirm the relevant owners after M0-A. Adapt renamed paths without changing approved scope. New module names suggested later are **proposed**, not existing files.

| Existing source anchors | Relevant current behavior / constraint |
| --- | --- |
| `src/calculation_pad/document.rs`: `CalculationDocument`, `CalculationValue`, `evaluate()` | Source-authoritative, line-oriented evaluation. Values include `Scalar(f64)`, `PhysicalQuantity`, and `ExactBase { value: u128, ... }`. Existing scalar-prefixed integers are bounded to 2^53; exact base conversion is separate. Limits include 64 KiB source, 256 physical lines, 2,048 bytes/expression, and bounded dependencies/nesting. |
| `src/calculation_pad/operators.rs`: `CheckedOperators`, `ModuloOperators`; `src/calculation_pad/README.md` | `^` means exponentiation. `/`, `%`, and contextual `mod(a,b)` have established semantics/precedence. Bare `mod` can remain a variable. New syntax must not reinterpret old documents. |
| `src/calculation_pad/session.rs`, `persistence.rs`, `examples.rs`; `src/common/numeric_base.rs` | Session-owned edits/history/revision caching, saved documents, bounded source-only recovery, executable examples, exact unsigned parsing/formatting. Recovery is opt-in and loaded on explicit open, not startup. |
| `src/gui/calculation_pad_dialog.rs`: `PadHost`, `PadHostIdentity`, `HostEffects`, `RootEffects`, `on_ui_frame_complete()` | Docked/native host migration is already implemented with generation and accepted-pass checks, root-effect routing, focus handling, pending transitions, and one session. `needs_root_ui_when_hidden()` keeps the necessary host work alive. |
| `src/gui/render.rs`, `src/gui/jump_mode.rs`, `src/gui/jump_mode/`; `src/gui/command_host.rs` | Launcher/child input ownership, accepted-frame effects, hidden-root execution, command dispatch, and Jump Mode integration. Preserve the prior Root Jump correction. |
| `src/settings/utility_dialog_sizes.rs`, `src/gui/utility_dialog_size.rs` | Bounded stable tool keys and preferred sizes. Unknown/malformed settings subtrees are preserved opaquely while unsafe geometry writes fail closed. Positions/native host geometry need additional work, not a competing preference store. |
| `src/gui/notes_dialog.rs`, `markdown_studio.rs`, `mkmacro_dialog/`; `src/settings_editor.rs`, `src/settings_editor/`; `src/gui/dashboard_editor_dialog.rs` | Existing documents, drafts, editor histories, explicit settings save flows, and macro runtime/editor ownership must survive host movement. |
| `src/unit_conversion.rs`: `Category`, `Unit`, `Approximation` | Shared unit catalog includes linear, affine, and reciprocal conversions and approximate calendar durations. Current pad conversion is not general unit algebra. |
| `src/diagram/model.rs`, `session.rs`, `scene.rs`, `layout.rs` | Bounded native flowchart model: rectangles, rounded rectangles, diamonds, labeled edges, flat groups, positions, history, scene geometry, and layout. Existing multi-selection and grid behavior are foundations, not missing features. |
| `src/diagram/mermaid.rs`, `rendering.rs`, `templates.rs`; `src/gui/diagram_studio.rs` | Source/Visual/Split views, native flowchart editing, and bounded source rendering for additional Mermaid families. Invalid/unsupported edits retain source and show stale previews rather than authorizing stale exports. A separate read-only viewer already exists. |
| `src/diagram/document.rs`, `codec.rs`, `persistence.rs`, `export.rs`; `docs/DIAGRAM_STUDIO.md`, `DIAGRAM_MERMAID_SUBSET.md`, `DIAGRAM_EXPORT.md` | `.mld` native documents, version-1 session encoding, exact source/history preservation, pending Save/Discard/Cancel transitions, current-revision SVG/PNG exports, scaling, bounded output, and collision protection. |
| `src/commands/{model,parser,bus,host}.rs`, `src/commands/handlers/`, `src/plugins/diagram_studio.rs`, `src/plugins/duplicate_finder.rs` | Existing typed command/plugin integration. New user-facing tools must use these boundaries instead of ad hoc string dispatch in rendering code. |
| `src/mkmacro/executor.rs`: `dispatch_open_folder()`; `src/mkmacro/{uia,windows,input}.rs`; `src/actions/folders.rs`, `src/plugins/folders.rs` | Existing folder opening validates a directory then launches Explorer with a path. It does not establish a targeted multi-tab workflow. UI Automation/window/input wrappers are candidates for bounded reuse. |
| `src/duplicate_finder/{discovery,session,model}.rs`; `src/file_operations.rs`: `NativeFileIdentity`, `ScanCancellation` | Bounded worker scanning, path/file identities, cancellation, partial diagnostics, and generation ownership. Duplicate discovery groups/hashes are not themselves a folder-size analyzer. |
| `src/plugins/processes.rs`; `src/platform/windows_api.rs` | Process inspection and platform wrappers exist. No completed file-specific lock inspector was located in the relevant source. |
| `src/gui/screenshot_capture.rs`: `ScreenshotCaptureOwner`; `src/actions/screenshot.rs`; `src/screen_draw/` | Still-capture ownership, target identity, cancellation, selection and publication patterns. These are not a video encoder. |
| `src/pinboard/live.rs`: `PinLiveCaptureSession`, `PinLiveCaptureBackend`, native capture module; `src/pinboard/source.rs` | Existing Windows Graphics Capture/D3D11 capture, target identity, bounded frame publication, GPU budgeting, and worker retirement. Reuse lower-level capabilities where appropriate without turning recorder ownership into Pinboard state. |
| `src/activity.rs`; `src/common/atomic_file.rs`; `src/platform/app_data.rs` | Activity/progress ownership, safe file publication, and application-data rooting already exist. |
| `Cargo.toml`, `.codex/config.toml`, `.codex/agents/*.toml`, `AGENTS.md` | Rust 2024; package `multi_launcher`; `multi_launcher_table_core` workspace member; `fast-dev` profile; egui/eframe 0.36.2 with local patches; windows 0.58 capture/UIA capabilities; existing image GIF support; configured planner/implementer/reviewer roles and sequential-write policy. Do not upgrade these foundations merely for this Goal. |

**Important correction to broad feature brainstorming:** templates, grid snapping, diagram saving, multi-selection, export scaling, and a diagram viewer already have implementations. Enhance and integrate these; do not rebuild parallel replacements.

## C. Approved Functional Requirements

### C.1 Calculation Pad: exact arithmetic without losing the text workflow

**Q1 A is final.** Ordinary decimal/scientific/trigonometric calculations retain existing `f64` behavior. No arbitrary-precision decimal mode, financial-calculation claim, or silent global arithmetic-mode switch is included.

1. Add exact `<<`, `>>`, `and`, `or`, `xor`, `not`, with compatible symbolic aliases `&`, `|`, and `~`. **Never repurpose `^` as XOR.** Contextual word operators must not invalidate previously legal variable names or braced names. Resolve tokens using grammar, not textual replacement.
2. Exact integer results carry width/signedness. Required widths: 8, 16, 32, 64 bits. Support 128-bit inspection/operations when practical with the existing `u128` base-conversion support; preserve all existing exact-base values through `u128::MAX` regardless of that optional arithmetic width.
3. Choose deterministic, source-expressed integer semantics. Default newly introduced bare programmer expressions to unsigned 64-bit; provide explicit exact constructors such as `int(...)`, `u8(...)`/`i8(...)` through `u64(...)`/`i64(...)` and corresponding optional 128-bit forms. Constructor spelling may change only to avoid an actual grammar conflict; keep a simple documented way to express each width and sign in text. A constructor argument's integer literal must be parsed exactly, not first evaluated as `f64`.
4. Preserve old scalar-only expression interpretation. Programmer operators/exact constructors opt into the typed integer path. Old `/` stays division, `^` stays power, `%` stays signed remainder, `mod` stays Euclidean remainder, and old precedence stays intact for old expressions. Use named `idiv(a,b)` and `rem(a,b)` or equally explicit nonconflicting syntax for exact quotient/remainder. Do not use `//` without proving it cannot conflict with current grammar.
5. Define and test: `not` masks to width; unsigned right shift zero-fills; signed right shift sign-extends; left shift preserves the selected width and discards shifted-out bits as documented; shift counts must be integers in `0..width`; rotate counts are nonnegative and reduced modulo width. Numeric casts are checked, not silent truncation; signed/unsigned *reinterpretation* in the inspector preserves the bit pattern and is labeled distinctly.
6. Checked integer `+`, `-`, `*`, division, and power must report overflow rather than wrap silently. Reject zero divisors, negative shifts, mixed incompatible integer modes, unsupported float/integer combinations, and signed minimum divided by -1. Use exact construction or explicit safe conversion, never rounding to make an operand fit. Wider exact values must survive named and `rN` references.
7. Add rotate-left/right, population count, bit extraction, and masks. Define extraction as `start` from least-significant bit 0 plus a bit count; reject ranges extending beyond width. Mask construction at full width must not shift by the type width. Keep one documented semantics shared by UI and evaluator.
8. The **Programmer Result Inspector is a separate dockable dialog/native window**, not a permanent panel replacing or shrinking the pad editor. Follow a selected current integer result; show binary, octal, decimal, hex together, width/sign, and numbered bits. Toggle bits and assign optional labels to bits/ranges in the inspector's working copy. Use explicit Copy/Insert actions to alter the clipboard/source. A dirty inspector working copy is not overwritten silently by a new source result; offer an explicit refresh from the current selection. Tie inserts to a current document/revision and valid insertion location.
9. Copy modes include prefixes, padding to chosen width, digit grouping, and plain numeric digits. Formatting does not change values. Preserve existing exact-base output defaults for old syntax. Persist inspector preferences boundedly; transient inspector edits are not an implicit pad save.
10. Add a comparison view for two expressions or selected results, with both values and representations visible together. Evaluate using the same engine and an immutable current variable/reference context. Report type/width/unit mismatch; do not mutate the source to perform comparison. The normal pad remains text-based.
11. Add useful unit-aware arithmetic: compatible addition/subtraction; scalar scaling; multiplication/division and small integer powers of linear quantities with dimension tracking; compatible target conversions; quantities through row/name references. Minimum examples: `10 m + 20 cm`, `(2 m) * (3 m)`, and `(5 m) / (2 s)`. Render unnamed compound dimensions clearly instead of inventing a catalog unit.
12. Treat affine temperature and reciprocal fuel-economy conversions separately from linear algebra. Preserve current direct conversions; reject undefined compound operations with clear diagnostics rather than applying linear factors incorrectly. Do not equate energy and torque, or erase other catalog distinctions merely because dimensions coincide. Approximate month/year provenance must survive accepted operations and conversions. No live exchange rates or new measurement catalog expansion is required.
13. Update executable examples, the in-app reference, grammar documentation, and discoverable command descriptions. Include precedence, masks, width/sign, overflow, integer division, exact limits, row/name references, inspector-only edits, and unit-algebra boundaries.

**Compatibility examples that must remain true:** `2 ^ 3` produces 8; scalar `7 / 2` produces 3.5; `-7 % 3` is -1; `mod(-7,3)` is 2; existing `mod` variables still work; exact `9007199254740993 dec to hex` remains exact. New acceptance examples include `0xFF << 8` → 65280, `u8(255) << 1` → 254, `not u8(0)` → 255, `i8(-1) >> 1` → -1, and exact construction of 9007199254740993 followed by integer arithmetic without floating-point loss.

### C.2 Dock / Pop Out: same live session, consistent host behavior

The required migration inventory is below. Each listed surface needs an explicit row in the execution record with implementation, state-test, and native-verification status. Do not satisfy a group by migrating only its easiest member.

| Group | Required surfaces / existing owners |
| --- | --- |
| Documents | Notes/quick-note surface and note editor (`notes_dialog.rs`); Markdown Studio (`markdown_studio.rs`); MkMacro editor (`mkmacro_dialog/`). |
| Settings | Main Settings (`settings_editor.rs`, `settings_editor/`, root integration); Dashboard Settings/editor (`dashboard_editor_dialog.rs`); Mouse Settings (`mouse_settings_dialog.rs`); Mouse Gestures (`mouse_gestures_dialog.rs`) and its settings (`mouse_gesture_settings_dialog.rs`). |
| Files | File Search (`file_search_dialog.rs` and submodules), Diff (`diff_dialog/`), Batch Rename (`batch_rename_dialog.rs`), Search & Replace (`search_replace_dialog.rs`). |
| Clipboard | Clipboard History (`clipboard_dialog.rs`), Snippets (`snippet_dialog.rs`), Clipboard Modify (`clipboard_modify_dialog.rs`), Paste Sequences (`paste_sequence_dialog.rs`). |
| Developer | JSON (`json_utility_dialog.rs`), XML (`xml_toolkit_dialog.rs` and submodules), Regex (`regex_tester_dialog.rs` and submodules), Table Utility (`table_utility_dialog.rs`). |
| Time | Timer/alarm authoring (`timer_dialog.rs`), Calendar (trace the existing calendar root/editor hosts), Date & Time Studio (`date_time_studio_dialog.rs`). |
| File/image utilities | Image Compare (`image_compare_dialog.rs`), Duplicate Finder (`duplicate_finder_dialog.rs`), ZIP Extractor (`zip_extractor_dialog.rs`). |
| Integration | Preserve Calculation Pad as the reference; reconcile existing Diagram Studio and viewer hosting; give the new programmer inspector and ordinary utility windows created by this Goal the same host controls. |

Required behavior:

- Match the existing **Pop Out** / **Dock** wording and interaction. Dock brings the launcher forward and restores the tool's usable docked presentation. Do not add workspaces or a different tiling/docking framework.
- There is one authoritative session per existing logical tool/document instance. Migration preserves draft text, dirty state, history, selection, scroll/camera, pending decisions, and running operations. Respect pre-existing multiple-note/document instances rather than collapsing them into one singleton.
- Never re-execute an action, restart a scan/macro/paste sequence, save settings, register duplicate hooks/hotkeys, or discard changes just because the host changes. Child dialogs/file pickers belong to the actual host window; confirmations remain confirmations, not independently detachable tools.
- Native windows remain responsive while the root launcher is hidden. Closing a child does not close the launcher; hiding the launcher does not destroy detached sessions. Preserve each tool's existing close/cancel/run semantics, including dirty-close decisions. Migration itself is not a Close event.
- Preserve Calculation Pad's fresh-open/default-docked behavior and each existing tool's startup/open defaults. Geometry persistence does not auto-open windows, restart operations, or silently introduce global session restoration.
- Remember preferred sizes and positions with stable bounded tool keys; distinguish logical coordinates from native physical pixels and preferred size from temporary clamping. Restore visibly after monitor removal/DPI changes, including monitors at negative desktop coordinates. A forced small viewport must not overwrite the user's larger preference.
- Generation/document/revision and accepted-pass checks reject retired callbacks and duplicate effects. Apply accepted effects once through the appropriate owner, not separately from root and child drawing.
- Query focus, hotkey toggling, native Alt+Space, Escape, editor shortcuts, and Jump Mode must respect the active window/control. No root command execution from keys typed in a child editor. Do not activate hints behind a modal or over a foreign application.
- Use lazy window/service creation and event-driven worker wakeups. No permanent capture hook, scan, frame polling, or unconditional root repaint just because pop-out support exists.

Full-screen selection overlays, transient prompts, pointer effects, capture overlays, and specialized Pinboard windows are not forced into ordinary detachable hosting. Additional ordinary surfaces may use the shared host only when directly necessary for the listed tool's functioning; this is not authorization for an unrelated application-wide redesign.

### C.3 Diagram Studio: two authoring experiences, not two conflicting truths

**Mermaid Authoring** preserves Source/Visual/Split, raw text, current native flowchart editing, existing source-rendered diagram families, templates, source-copy handoff with Markdown Studio, and the read-only viewer. **Visual Canvas Authoring** is a richer structured document with its own authoritative geometry/style. These are authoring modes, not a relabeling of the existing preview layouts.

Required visual capabilities:

- Rectangles/rounded rectangles/diamonds plus circles, ellipses, cylinders, parallelograms, triangles, and reusable custom shape presets. Bound custom shapes to supported primitive composition; no scripting or general-purpose vector-code execution.
- Straight, elbow, curved, directed/bidirectional, and labeled connectors; anchors attach to shapes and update on movement/resize. Bounded orthogonal routing avoids obstacles where feasible; unrouteable paths are shown honestly rather than hanging or claiming a perfect route.
- Drag, resize, rotate, marquee/multi-select, group transformations, snapping/grid, alignment guides, align/distribute, stacking/layers, free text, notes/callouts/titles, containers, and swimlanes. Maintain stable identities and one undo operation per completed drag/transform.
- Fill/border/connector colors, line style, font/size, opacity, and simple shadows. Hit testing, selection bounds, preview, export, and saved state must agree after transforms.
- Imported image/icon nodes and reusable components, with bounded embedded/local assets and predictable missing-asset diagnostics. Do not fetch remote resources or execute active SVG/HTML content.
- Useful templates for flowcharts, architecture, mind maps, sequence-style diagrams, and dependencies. Preserve existing Mermaid templates; distinguish them from visual-canvas templates. A visual sequence template is not a promise of full UML semantics.
- Minimap, fit/pan/zoom, and a searchable local diagram library/recent-document view supporting reopen and duplicate/reuse. Do not introduce projects/workspaces, remote storage, collaboration, or background filesystem indexing for this library.
- Mermaid syntax assistance/autocomplete compatible with the actual supported renderer/parser: keywords, snippets, IDs, and useful diagnostics without deleting unsupported source.
- Current-document PNG/SVG export with transparent or chosen background, scaling/resolution controls, and visual bounds including rotations, arrowheads, text, and shadows. Preserve existing safe destination and stale-preview restrictions.
- Explicit **Convert Mermaid to Visual Copy** for the supported native subset. The original source document remains intact; conversion creates a distinct document or an explicit undoable transition with a preserved original. Unsupported conversion gives a reason. Do not require rich visual documents to convert back to Mermaid.
- Versioned `.mld` persistence reads existing version-1 data without destructive eager rewrite. A new discriminated authoring payload must round-trip rich properties, assets, and appropriate history. Unsupported future versions fail without truncation or rewrite. Save/Discard/Cancel and conflict handling remain consistent.
- **draw.io interoperability is required as a documented subset, not full compatibility.** Support uncompressed XML import/export of primitives, labels, geometry, supported styles, groups/layers, and connectors. Support bounded compressed-page decoding when feasible using the documented file representation; if not supported, explicitly reject it and explain the uncompressed export requirement. Multi-page input must allow selecting a page or explicitly reject multiple pages before import; never silently retain only page one. Unsupported elements/styles produce a pre-import/export report. Preserve originals and never claim lossless round-trip for unsupported content. Uncompressed output is sufficient.

### C.4 Explorer tab launcher

Provide a searchable launcher tool and editable saved **folder lists**: name, ordered absolute folder paths, reorder/add/remove, validate, and launch. These are folder lists, not workspaces or application-layout restoration.

The default invocation opens a **new deliberately identified Explorer window**, then opens each folder in order as tabs in that same window. Do not replace or close unrelated windows/tabs. Preserve duplicates when explicitly listed. Validate paths (including spaces, Unicode and UNC) as paths, not command strings. Do not use a shell to interpret them.

Run as a bounded cancellable job with progress and per-folder outcomes. Establish window identity rather than assuming the PID returned by launching Explorer is the new window. Check supported tab controls, expected active target, and navigation completion. Loss of target/focus, permission restrictions, or unsupported tab behavior stops safely with partial results; it must not send subsequent folder text to another application. Never silently open five independent windows and report multi-tab success.

Prefer observed UI Automation control patterns and supported shell interfaces; validate actual runtime behavior rather than assuming an undocumented tab API. A guarded keyboard fallback is acceptable only for an explicitly verified Explorer target and expected focused control. No blind sleep-and-key scripts, hardcoded screen coordinates, private-message dependency without evidence, clipboard destruction, elevation, or injected shell extensions. Keep existing single-folder/MkMacro Open Folder semantics unchanged. Expose the new operation through the typed command/action system so configured hotkeys and existing action pickers can invoke it without adding a new macro language.

### C.5 Folder size analyzer

Read-only analysis of an explicitly selected directory, with progressive tree/sorted list, largest files/subdirectories, size/count/share, drill-down, path copy, and reveal/open through existing actions. Provide Refresh and Cancel. Keep scan ownership independent of whether its dialog is docked or detached.

Default metric is **logical file size summed per directory entry**, clearly labeled. Hard links may count at each directory entry; display that accounting policy rather than mislabeling it unique physical disk usage. Allocated-size or deduplicated metrics are not required. Show inaccessible/skipped/changed entries and partial totals; “0 bytes” must not conceal unreadable content. Reparse/junction recursion is off by default, with cycle protection and explicit diagnostics. Use metadata without deliberately reading file contents or hydrating cloud files. Guard depth, entry counts, path memory, totals overflow, and result publication. No deletion, cleanup, recursive content hashing, or permanent watcher.

### C.6 File lock inspector

Inspect a selected regular file, showing processes/services reported as using it: identity, PID, application/service name, available executable path, and inspection time. Refresh is explicit; copy details and safe reveal are sufficient actions. Directory-recursive inspection is not required.

Use documented Restart Manager resource registration/enumeration behind a Windows boundary [W1–W3]. Results are **processes reported as using the file**, not guaranteed proof of the exact denying handle/share mode. An empty result means “No processes reported,” not “Guaranteed unlocked.” Handle disappearing files, changing process lists, access denial, process identity reuse, and incomplete metadata. Do not call shutdown/restart APIs, close another process's handles, kill processes, request admin privileges automatically, or install a driver/service.

### C.7 Silent GIF / MP4 recording

**Q2 A is final:** local animated GIF and video-only MP4. No microphone, system audio, audio permissions, webcam, streaming, upload, or video-editing timeline.

Choose region, window, or individual display; show the target and output settings before starting. Region selection respects desktop coordinates and DPI, including non-primary displays. If a capture backend cannot support a region spanning displays, reject that selection clearly before recording; never silently crop to the primary display.

Provide start/countdown, elapsed time, visible recording state, Stop & Save, and Cancel/Discard. Use a compact controller usable with the launcher hidden. Expose a stop command through existing command/hotkey infrastructure; do not reserve a new conflicting global shortcut by default. Escape cancels the selection/countdown when owned by that flow; do not swallow unrelated application Escape while recording.

Routine defaults: GIF 10 fps, MP4 30 fps, a 30-second initial duration limit, configurable up to a bounded 120 seconds, and aspect-preserving output up to 1920×1080 by default. Codex may choose additional bounded presets, but must show applied scaling and validate dimensions before allocation. Optional cursor inclusion must be explicit. This is a short-clip utility, not an unlimited background recorder.

Stream encoding through bounded queues; avoid retaining an entire clip as raw frames. On overload, use a documented dropping/coalescing policy that preserves elapsed playback time. Keep capture identity stable; do not switch to whatever window becomes foreground. Define window resize handling with a fixed output canvas and aspect-preserving scaling. If a target closes/becomes unavailable, stop/finalize available data with a clear result rather than silently capture another target. Display/adapter loss and encoder failure terminate cleanly.

Save only after encoder finalization to a collision-protected destination, using task-owned temporary output. Stop finalizes; Cancel discards only the current temporary recording. Disk-full/finalize failure must not report success or destroy an existing user file. A surviving partial artifact is clearly labeled with recovery/cleanup details. Closing/shutdown must leave no hook, capture session, GPU resource, or writer running. Recordings do not resume on startup.

## D. Scope and Non-Goals

Implement all selected features above. No workspaces, unrelated utility backlog, network service/cloud accounts, decimal arithmetic engine, audio recording, full draw.io clone, general IDE, forced file unlocking, disk cleanup, or new automation language.

Do not add ZIP creation, universal preview, log viewer, HTTP client, arbitrary-window pinning, or other unselected brainstorming items. Existing ZIP extraction safety, clipboard playback, file operation protections, hotkey toggle/hold behavior, startup recovery, and unrelated plugins are preservation obligations, not expansion projects.

No broad egui/eframe/windows upgrade, public API reshuffle, global settings rewrite, or change to vendored code without a concrete feature-blocking need. Document any unavoidable shared change and test its actual consumers. Unknown persisted data must not be rewritten merely because a new tool exists.

## E. Technical Implementation Strategy

### E.1 Reuse first, with clear ownership

Extract the minimum reusable host lifecycle from Calculation Pad into a proposed common GUI-host module, retaining stable tool/session identities and tool-specific effects. Move a live session between presentations rather than reconstructing it. Keep domain state and workers outside viewport callbacks. Integrate once with root hidden-frame work, input ownership, settings geometry, and accepted effects. Avoid a generic widget framework or moving every subsystem into `LauncherApp`.

Implement typed exact integers and unit algebra in the calculation domain, not the inspector widget. Preserve the scalar evaluator as a compatibility path; parse original integer tokens exactly. Maintain expression/document resource bounds. Persist semantics in text or versioned document data; inspector display settings must not silently change reopened calculations.

Extend the diagram domain with an explicit authoring discriminator and a rich visual scene model. Reuse shared document safety/history, scene/export, text/font, and host capabilities where they actually match; do not force richer visuals through the limited Mermaid graph serializer. Bound object counts, asset bytes, connector routing, and XML decompression. Keep renderer results tied to document/revision.

Use small proposed domain/platform/UI modules for Explorer tabs, folder sizes, lock inspection, and recording. Keep Windows API/COM lifetimes behind owned interfaces with RAII cleanup and explicit errors. Reuse `ActivityCenter`, cancellation, typed commands, `AppDataRoot`, and atomic publication. GUI drawing must not perform directory traversal, COM enumeration, capture conversion, or encoding synchronously.

### E.2 Native integrations and researched API constraints

The following are engineering guidance from official documentation, distinct from repository findings. Verify exact bindings against the pinned `windows` crate and supported Windows environment; add only required features/dependencies.

- Restart Manager registers resources, enumerates users of those resources, and requires session cleanup. `RmGetList` can need a larger buffer; bound allocation/retries. Do not equate enumerated resource users with a proven exclusive lock. [W1–W3]
- `IShellWindows` enumerates shell windows; that documentation alone does not establish an Explorer tab-control API. UI Automation exposes only patterns actually supported by the target controls. Any keyboard fallback is subject to input integrity restrictions. Capability detection and real tab/window/path verification are therefore mandatory. [W4–W6]
- Windows Graphics Capture supports window/display frame acquisition and has a support check. Existing Pinboard code is a valuable starting point. Reuse low-level device/frame conversion only where lifetime ownership stays correct. [W7]
- Media Foundation sink writing supplies a native video-encoding route; MP4 can contain only video. Prefer H.264/MP4 through a supported local encoder, without downloading executable encoders or requiring FFmpeg on PATH. The cited introductory tutorial uses WMV, so adapt the media types/container rather than copying its format assumptions. [W8–W10]
- draw.io supports XML and compressed representations. Begin with explicit, tested uncompressed interchange; bounded compressed decoding is an extension of that parser, not permission to trust input. [W11–W12]

### E.3 Cross-cutting correctness contracts

Workers carry request/session generations; retirement prevents stale completion publication. Cancellation does not manufacture success, and cannot promise interruption of every blocking OS call: bound outstanding jobs, retire late results, and keep the UI responsive without accumulating abandoned workers. Record any truly uninterruptible native wait rather than spinning up replacement threads indefinitely.

All new filesystem outputs have explicit destinations, collision/overwrite rules, bounded content, and ownership-aware cleanup. New preferences use tolerant backward-compatible migrations and defaults; inspection must not rewrite user data. Test profiles use isolated paths and disable Paste Sequences `next_hotkey`/`cancel_hotkey` **before** constructing the app, per `AGENTS.md`; production defaults stay unchanged.

### E.4 Requirement traceability

Use original selection numbers in the execution ledger. Required coverage mapping:

| Selected IDs | Owning milestones |
| --- | --- |
| 1–4, 7, 10 | M5: exact integer types, grammar, operations and compatibility |
| 5–6, 8, 11 | M6: separate inspector, bitfields, copying and comparison |
| 9, 12 | M7: dimensional arithmetic, examples and references |
| 13–15 | M2: Notes, Markdown and MkMacro hosting |
| 16–18 | M3: Settings, Dashboard and mouse/gesture hosting |
| 19–23 | M4: remaining required utility families |
| 24–26 | M1 plus every host migration: one session, geometry, focus and return-to-dock |
| 27–28, 34–35 | M8: authoring model, shapes/style/layers and compatibility |
| 29–33, 36–37, 39–41 | M9: direct manipulation, connectors, containers, assets and navigation |
| 38, 42–43, 45–46 | M10: templates, assistance, export, conversion and library |
| 44 | M11: documented draw.io interchange subset |
| 61 | M12: Explorer multi-tab folder lists |
| 88 | M13: folder size analyzer |
| 89 | M14: file lock inspector |
| 119 | M15–M16: silent recording and native integration |
| All | M17: scoped cumulative review, evidence and final-tip handoff |

No requirement may disappear into “foundation complete.” Optional 128-bit arithmetic and compressed draw.io-page import must be reported separately from required behavior. Ordinary implementation choices may be resolved autonomously; scope reductions require an honest incomplete status, not a hidden acceptance rewrite.


## F. Detailed Milestones and Tasks

### Ordered categories and branch handoffs

Each category begins with its own **Cxx/M0-A** bootstrap in Section A0. M0-A is a real new baseline commit even when the preceding branch already has a baseline. These are the only planned branch boundaries; ordinary checkpoints stay on their current category branch.

| Category | Suggested unique branch stem | Checkpoints | Required parent |
| --- | --- | --- | --- |
| C01 | `feat/utility-dock-popout` | M0-B, M1–M4 | Verified accepted predecessor from A0 |
| C02 | `feat/calculation-programmer-tools` | M5–M7 | C01 actual `final_tip_sha` |
| C03 | `feat/diagram-visual-canvas` | M8–M11 | C02 actual `final_tip_sha` |
| C04 | `feat/explorer-folder-tabs` | M12 | C03 actual `final_tip_sha` |
| C05 | `feat/folder-size-analyzer` | M13 | C04 actual `final_tip_sha` |
| C06 | `feat/file-lock-inspector` | M14 | C05 actual `final_tip_sha` |
| C07 | `feat/silent-screen-recorder` | M15–M17 | C06 actual `final_tip_sha` |

Append a short unique suffix only when a name already exists. Do not reuse an already-active branch as permission to omit the new child branch. After a category's last accepted implementation increment, record review, outstanding verification, push and clean-state receipts before creating the next child. M17 closes the cumulative Goal on C07; it does not create an eighth cleanup branch.

The preceding category must be committed and coherent. Section I explicitly permits a truthful **implementation-complete / native-environment-blocked** handoff so independent work does not stall; it does not permit declaring missing acceptance complete or inheriting a known shared correctness failure without addressing its dependency.

### C01 — Shared hosting and required dialog migrations

#### M0-B — Bounded baseline and host inventory

- **Objective:** Confirm the affected owner map and preserve a usable baseline after C01/M0-A.
- **Code ownership:** AGENTS.md; source anchors in B; existing execution/queue records; required surfaces in C.2.
- **Implementation tasks:** Inventory every required host, its authoritative session, close policy, native/docked implementation, settings key, worker ownership and child prompts. Capture relevant existing test names and cached build profile. Classify inherited failures separately. Probe supported native verification capability once; record exact limitations rather than starting host repair.
- **Behavioral requirements:** No application behavior changes in this checkpoint; each selected feature has an owner and verification route.
- **Edge cases:** Different live file names; already-native tools; missing older evidence; pre-existing dirty user files; native observer unavailable.
- **Regression constraints:** Do not import historical full-suite or exhausted native-acceptance campaigns. Do not claim the inherited eight native flows passed.
- **Verification:** Read-only source/test inspection plus a narrowly justified baseline test filter when useful. Verify filter matches before interpreting a run. Do not build every smoke binary.
- **Acceptance criteria:** All C.2 surfaces and the 50 selected IDs are mapped; baseline differences and environment status are recorded; no unresolved ancestry ambiguity.
- **Commit boundary:** After the stated focused checks and diff review: `docs(productivity): [M0-B] map approved scope and bounded verification`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M1-A — Reusable single-session host lifecycle

- **Objective:** Establish reusable dock/pop-out ownership using Calculation Pad as the working reference.
- **Code ownership:** src/gui/calculation_pad_dialog.rs; src/gui/render.rs; src/gui/jump_mode/; proposed small common host module.
- **Implementation tasks:** Extract host identity/generation, migration intent and native bootstrap/focus/close coordination without moving domain logic into the host. Preserve accepted-pass effect application. Migrate Calculation Pad to this boundary and reconcile Diagram Studio/viewer integration points without changing document semantics.
- **Behavioral requirements:** Dock ↔ Pop Out retains the same session and pending decisions; hidden-root native windows continue to function; child effects apply once.
- **Edge cases:** Native close during migration; discarded passes; late callbacks from retired viewports; embedded viewport fallback; child menus and file pickers; rapid repeated host requests.
- **Regression constraints:** Preserve pad cached evaluation, editor history, source recovery, copy provenance, Root Jump and editor input isolation. No duplicate pad implementation remains.
- **Verification:** Run existing pad host/accepted-pass tests and new generic transition tests. Include stale-generation rejection, close cancellation and root hidden behavior. Use the existing host-migration regression by name.
- **Acceptance criteria:** One authoritative session is demonstrated across repeated host transitions; retired/duplicate effects cannot mutate it; root and child input cannot both execute the same command.
- **Commit boundary:** After the stated focused checks and diff review: `refactor(gui): [M1-A] share accepted single-session utility hosting`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M1-B — Geometry, focus and host controls

- **Objective:** Provide common Pop Out/Dock controls and bounded monitor-aware geometry.
- **Code ownership:** src/settings/utility_dialog_sizes.rs; src/gui/utility_dialog_size.rs; host module; settings model/save integration; native viewport builders.
- **Implementation tasks:** Extend existing geometry ownership with position/native size where needed; preserve opaque future/malformed settings; use stable bounded keys. Distinguish preferred dimensions from clamping. Route Dock to show/focus root and Pop Out to the intended native host. Keep last native position independently of docked window placement.
- **Behavioral requirements:** Geometry survives ordinary reopen/restart without auto-opening tools. Off-screen placement recovers visibly. Closing retains each tool’s existing default-open behavior.
- **Edge cases:** Negative monitor coordinates; mixed DPI; disconnected displays; minimized/zero-size events; NaN/infinite geometry; stale monitor IDs; large text and small viewports.
- **Regression constraints:** Do not overwrite preferred dimensions with a temporary constrained size or introduce a second settings writer. Preserve default hotkeys and launch visibility.
- **Verification:** Unit-test validation/migration/clamping and input ownership. Native smoke: move/resize/dock/pop out Calculation Pad, hide/show root, verify visible controls and focus on a supported monitor configuration.
- **Acceptance criteria:** Existing size settings still read; malformed data survives unrelated saves; no focus theft on passive repaint; control names/behavior match the reference.
- **Commit boundary:** After the stated focused checks and diff review: `feat(gui): [M1-B] persist utility geometry and consistent host controls`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M2-A — Notes and Markdown hosting

- **Objective:** Migrate note browsing/editing and Markdown Studio without losing drafts.
- **Code ownership:** src/gui/notes_dialog.rs; src/gui/markdown_studio.rs; src/markdown_studio/; their root/panel routes.
- **Implementation tasks:** Integrate common hosting for required note surfaces and Markdown. Keep existing logical instance identities and document stores. Bind file choosers and dirty prompts to the active host. Preserve Markdown-to-Diagram source-copy handoff.
- **Behavioral requirements:** Edit, detach, continue editing, dock, undo, save and reopen with the same intended content; repeated open focuses rather than resets a live document.
- **Edge cases:** Multiple note editors; unsaved untitled Markdown; external file changes; pending save conflict; host migration while a chooser/dirty decision is open.
- **Regression constraints:** No duplicate note store or source authority; tags/attachments and current save/discard conventions remain intact.
- **Verification:** Focused existing note/Markdown state tests plus host migration with dirty/undo/selection state. Native representative flow uses disposable documents and verifies actual saved content.
- **Acceptance criteria:** Both note browsing/editing and Markdown rows are implemented; drafts and history survive migration; close Cancel preserves the editor in either host.
- **Commit boundary:** After the stated focused checks and diff review: `feat(notes): [M2-A] detach notes and markdown without session loss`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M2-B — MkMacro editor hosting

- **Objective:** Detach the existing macro editor while preserving runtime and authoring ownership.
- **Code ownership:** src/gui/mkmacro_dialog/; src/mkmacro/{runtime,store,editor_mutation}.rs; macro invocation/prompt integration.
- **Implementation tasks:** Migrate the editor host, authoring children and prompt parentage. Keep runtime/recorder state outside host lifetime. Preserve selected macro/step, expanded sections, drafts and validation. Integrate Jump Mode for the actual active host.
- **Behavioral requirements:** Moving the editor does not start/stop/replay a macro or record duplicate input; native prompts remain reachable.
- **Edge cases:** Running harmless macro; recorder active; nested parameter/window/asset pickers; edited step with validation errors; child dialog open during migration.
- **Regression constraints:** No new macro language, hook or runtime owner. Existing hotkey scope, execution cancellation and editor Save/Discard behavior remain unchanged.
- **Verification:** Authoring/runtime-state tests and a no-side-effect fixture macro. Native smoke verifies detached editing and one bounded controlled run with no duplicate steps.
- **Acceptance criteria:** Editor state and selection survive; runtime effects occur exactly once; focus/key input stays in the correct host; child pickers never become orphaned.
- **Commit boundary:** After the stated focused checks and diff review: `feat(mkmacro): [M2-B] host macro editing independently of playback`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M3-A — Main Settings and Dashboard Settings hosting

- **Objective:** Allow independent settings windows without implicit commits or stale reloads.
- **Code ownership:** src/settings_editor.rs; src/settings_editor/; src/gui/dashboard_editor_dialog.rs; root settings/panel integration.
- **Implementation tasks:** Host each existing editor session. Preserve draft/committed separation, pending save/cancel and settings validation. Ensure saving a host preference cannot overwrite an unrelated dirty settings draft. Keep dashboard editing separate from displayed dashboard state.
- **Behavioral requirements:** Pop Out/Dock is presentation-only; Apply/Save/Cancel retains existing semantics. The dashboard stays usable while its editor is detached.
- **Edge cases:** Unsaved settings while geometry changes; configuration reload; invalid fields; dashboard edits after root hide; two settings-related dialogs open.
- **Regression constraints:** Do not commit drafts on window movement, reinitialize settings on every frame, or change plugin/theme/hotkey defaults.
- **Verification:** Focused save/cancel/configuration tests with dirty drafts across host moves; native main-settings and dashboard-editor save/cancel checks using isolated profiles.
- **Acceptance criteria:** Both editors preserve unsaved changes and save exactly the intended fields; geometry writes do not clobber settings; no duplicate settings UI owner remains.
- **Commit boundary:** After the stated focused checks and diff review: `feat(settings): [M3-A] detach settings and dashboard editors safely`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M3-B — Mouse Settings and Gesture hosting

- **Objective:** Migrate mouse settings, gesture authoring and gesture settings while preserving input ownership.
- **Code ownership:** src/gui/{mouse_settings_dialog,mouse_gestures_dialog,mouse_gesture_settings_dialog}.rs; existing gesture/window-picker owners.
- **Implementation tasks:** Adopt the common host for all three ordinary editors; parent their pickers/capture prompts correctly. Preserve draft changes, gesture recording state, bindings and active application scope.
- **Behavioral requirements:** Host movement does not install another observer, enable a gesture, or capture mouse input outside an explicitly active recording flow.
- **Edge cases:** Gesture recording/picker active during migration; cancelled recording; losing foreground; held leader/hotkey; root hidden; save validation error.
- **Regression constraints:** Crosshair/cursor effects, gesture-only radial behavior and global launcher hotkey remain unchanged. Capture overlays retain specialized ownership.
- **Verification:** State tests for hook/recorder lifetime and accepted actions; native controlled gesture edit/record/cancel followed by ordinary desktop mouse input.
- **Acceptance criteria:** Each required mouse/gesture surface is covered; no stuck suppression or duplicate observer; Escape and close affect only the intended flow.
- **Commit boundary:** After the stated focused checks and diff review: `feat(mouse): [M3-B] unify mouse and gesture editor hosting`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M4-A — File workflow dialogs

- **Objective:** Add shared hosting to File Search, Diff, Batch Rename and Search & Replace.
- **Code ownership:** src/gui/file_search_dialog*; src/gui/diff_dialog/; src/gui/batch_rename_dialog.rs; src/gui/search_replace_dialog.rs; existing domain sessions.
- **Implementation tasks:** Migrate all four while retaining query/filter/sort/selection, compared sources, pending rename/replace previews, approvals, worker generations and scroll state. Keep domain execution decisions independent of host.
- **Behavioral requirements:** Running scans continue or cancel exactly as before; host changes never apply rename/replace operations or discard reviewed previews.
- **Edge cases:** Completion during host migration; folder/text/binary Diff modes; queued replace approval; unavailable file; close while busy.
- **Regression constraints:** Preserve file identity checks, native safety validation, operation approvals, clipboard copying and existing search keyboard behavior.
- **Verification:** Focused per-dialog integration tests with disposable files, including a stale worker result and a pending approval across detach/dock. Native one representative file workflow plus at least one safe preview-only operation.
- **Acceptance criteria:** All four inventory rows pass state coverage; operations are not duplicated or implicitly approved; visible row/control actions work in each host.
- **Commit boundary:** After the stated focused checks and diff review: `feat(files): [M4-A] detach file workflows while preserving operations`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M4-B — Clipboard workflow dialogs

- **Objective:** Detach Clipboard History, Snippets, Clipboard Modify and Paste Sequences.
- **Code ownership:** src/gui/{clipboard_dialog,snippet_dialog,clipboard_modify_dialog,paste_sequence_dialog}.rs; their domain sessions and clipboard owner.
- **Implementation tasks:** Add host adapters preserving selection, draft transformations and sequence state. Route copy/paste effects through existing one-shot owners. Retain prompt parentage and keep-visible behavior.
- **Behavioral requirements:** Moving a window does not publish another clipboard item, advance a paste step, or intercept Ctrl+V outside active playback.
- **Edge cases:** External clipboard change; self-generated change; sequence running or cancelled; second playback; pending snippet input; hidden root.
- **Regression constraints:** Preserve deferred next-step publication, existing cancellation rules, final clipboard content and global shortcut registration. Do not use real clipboard payloads in logs.
- **Verification:** Clipboard-owner/state tests and existing playback regressions. Controlled native clipboard fixture checks once-per-action effects and second run after host migration.
- **Acceptance criteria:** All four surfaces retain state; no duplicate clipboard publication or phantom sequence advance; normal paste outside playback remains unaffected.
- **Commit boundary:** After the stated focused checks and diff review: `feat(clipboard): [M4-B] detach clipboard tools without replay effects`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M4-C — Developer utility dialogs

- **Objective:** Detach JSON, XML, Regex and Table Utility with unchanged document behavior.
- **Code ownership:** src/gui/{json_utility_dialog,xml_toolkit_dialog,regex_tester_dialog,table_utility_dialog}.rs and related submodules; existing utility engines.
- **Implementation tasks:** Integrate host/session adapters and tool-specific children. Preserve input text, parsed state, errors, selections, test cases, current table page/query and copy/export intent.
- **Behavioral requirements:** Copy/format/query actions use current accepted input exactly once; host changes neither re-import files nor reset results/history.
- **Edge cases:** Invalid JSON/XML/regex; huge bounded inputs; table file-backed paging in flight; partial parse; save chooser cancel; child popup open.
- **Regression constraints:** No parser/formatter semantics change and no table publication or file-backing safety regression. Do not modify table-core merely to add window chrome.
- **Verification:** Each utility’s focused state tests plus common-host parity tests, including current-revision clipboard/export and in-flight table completion. Native representative editor and table interaction.
- **Acceptance criteria:** All four hosts render and operate with preserved state; accepted effects come from the current tool instance; bounded file-backed behavior remains intact.
- **Commit boundary:** After the stated focused checks and diff review: `feat(devtools): [M4-C] detach structured-data and text utilities`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M4-D — Time and image/file utility dialogs; host coverage closure

- **Objective:** Finish every remaining requested host migration and close C01 coverage gaps.
- **Code ownership:** src/gui/{timer_dialog,date_time_studio_dialog,image_compare_dialog,duplicate_finder_dialog,zip_extractor_dialog}.rs; existing Calendar hosts; shared host inventory.
- **Implementation tasks:** Migrate Timer/alarm authoring, Calendar, Date & Time Studio, Image Compare, Duplicate Finder and ZIP Extractor. Preserve scheduled/running operation owners and subordinate editor parentage. Audit every C.2 row for an actual implementation and test; resolve accidental duplicate host paths.
- **Behavioral requirements:** Detaching a timer cannot schedule it twice; inspecting a ZIP cannot extract it; duplicate/image jobs retain their inputs and status.
- **Edge cases:** Alarm fires during migration; calendar edit validation; image decode completion; duplicate scan cancellation; ZIP unsafe entry; collapsed child close.
- **Regression constraints:** Preserve timer parsing/scheduling, existing ZIP path safety, comparison accuracy and scan cancellation. No ZIP writer or unrelated time features.
- **Verification:** Focused tests per family, native representative timer/calendar and image/file utility flows, and C01 reviewer inspection. Re-run only common-host tests affected by late fixes.
- **Acceptance criteria:** Every required C.2 surface has explicit implementation/state/native status. No missing surface is hidden under a family-level pass. C01 is committed with a truthful final-tip handoff.
- **Commit boundary:** After the stated focused checks and diff review: `feat(gui): [M4-D] complete requested utility host coverage`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.


### C02 — Text-first Calculation Pad and programmer tools

Run C02/M0-A directly from C01’s actual committed final tip before M5. Reuse the C01 host; do not copy it.

#### M5-A — Exact integer domain and checked operations

- **Objective:** Create exact typed integer evaluation primitives independent of GUI rendering.
- **Code ownership:** src/calculation_pad/document.rs; src/common/numeric_base.rs; proposed calculation integer-domain module; relevant session value handling.
- **Implementation tasks:** Implement exact literal construction, width/sign normalization, checked arithmetic, masks, shifts, rotations, population count and extraction. Separate numeric casts from bit reinterpretation. Keep existing ExactBase support intact.
- **Behavioral requirements:** Values greater than 2^53 never pass through f64 on the exact path; every operation has documented width and overflow semantics.
- **Edge cases:** 8/16/32/64 endpoints; u128::MAX conversion; optional 128 arithmetic; full-width masks; shifts at width; signed minimum; negative zero/scalar conversions; zero division.
- **Regression constraints:** No changes to ordinary scalar evaluation or persisted source. Do not globally widen/narrow a referenced value based on inspector preferences.
- **Verification:** Table-driven unit tests at every required width/sign, cross-check bit identities and exact formatting; exercise invalid operands and range endpoints.
- **Acceptance criteria:** Integer operations and diagnostics are deterministic, bounded and exact; optional 128-bit arithmetic status is explicit without dropping old u128 conversions.
- **Commit boundary:** After the stated focused checks and diff review: `feat(calc): [M5-A] add exact width-aware integer operations`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M5-B — Grammar, references and compatibility

- **Objective:** Expose the integer domain through text syntax without changing old expression meaning.
- **Code ownership:** src/calculation_pad/{document,operators,session}.rs; grammar tests and existing example harness.
- **Implementation tasks:** Integrate contextual word/symbol operators and exact constructors, define programmer precedence, route named/rN values without coercion and provide exact division/remainder. Preserve old grammar/precedence on legacy expressions. Parse tokens rather than replacing strings.
- **Behavioral requirements:** 0xFF << 8 and named intermediate masks work directly; constructors permit exact large integers; bad lines do not stop independent later lines.
- **Edge cases:** Existing variables named and/or/xor/not or constructor-like names; Unicode/braced names; scientific literals; missing digits; reference cycles; mixed unit/float/integer expressions; resource limits.
- **Regression constraints:** ^ stays power; / and legacy %/mod behavior stay intact; no silent conversion of an existing scalar document to width-limited arithmetic.
- **Verification:** Run current document/grammar/reference tests and new precedence/exactness fixtures. Verify all compatibility and new examples in C.1 as executable assertions.
- **Acceptance criteria:** Approved operators work in real pad documents; exactness survives dependencies; old valid documents retain values, source and error isolation.
- **Commit boundary:** After the stated focused checks and diff review: `feat(calc): [M5-B] integrate programmer syntax without scalar regressions`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M6-A — Separate programmer result inspector

- **Objective:** Provide the requested independent dockable inspector with working-copy bit manipulation.
- **Code ownership:** Calculation Pad selection/result handoff; proposed inspector dialog/session; common host; settings preferences; typed command integration.
- **Implementation tasks:** Add current-result binding, representations, required width/sign controls, bit grid/labels, toggle/refresh, copy format controls and explicit source insertion. Preserve a dirty working copy until explicit refresh. Fence source inserts by current document/revision.
- **Behavioral requirements:** The pad editor remains text-based and unshrunk. Inspector edits do not mutate source or clipboard until the user explicitly requests it.
- **Edge cases:** Source row deleted/replaced; pad document changed; dirty inspector while source reevaluates; noninteger selection; output width too small; retired native copy request.
- **Regression constraints:** Do not introduce a second calculator engine, implicit save or stale insertion. Preserve pad undo as one intentional insertion edit.
- **Verification:** Inspector state/format tests and native flow selecting a result, popping out, toggling bits, copying hex, inserting deliberately, then undoing in the pad.
- **Acceptance criteria:** All representations agree bit-for-bit; labels and formatting do not alter values; source remains unchanged during passive inspection and toggling.
- **Commit boundary:** After the stated focused checks and diff review: `feat(calc): [M6-A] add detached programmer inspector and bitfields`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M6-B — Side-by-side calculation comparison

- **Objective:** Compare two calculations without rewriting the document.
- **Code ownership:** Calculation engine/session snapshot; inspector or a compact separate comparison surface using common hosting.
- **Implementation tasks:** Provide two expression/result slots and show evaluated values/representations together. Resolve variables from an immutable current document context. Indicate incompatible types, integer modes and physical dimensions; keep comparison evaluation bounded.
- **Behavioral requirements:** Two approaches can be inspected together; comparison is a view/explicit input operation rather than a hidden source edit.
- **Edge cases:** One side invalid; source revision changes mid-evaluation; stale selected row; equal bit patterns with different signedness; mismatched physical units.
- **Regression constraints:** Use the same engine and diagnostics. No new comparison syntax that changes old expressions, duplicate evaluator, or automatic source insertion.
- **Verification:** State tests for equal/different exact values and incompatible/error states; native or recorded UI evidence of both sides readable in a constrained host.
- **Acceptance criteria:** Both results remain independently visible; errors identify their side; the document source, dirty state and history are unaffected by comparison alone.
- **Commit boundary:** After the stated focused checks and diff review: `feat(calc): [M6-B] compare expressions in a non-destructive view`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M7-A — Dimensional arithmetic

- **Objective:** Extend physical conversions into useful dimension-preserving arithmetic.
- **Code ownership:** src/unit_conversion.rs; src/calculation_pad/document.rs; proposed quantity algebra helpers; existing conversion plugin consumers.
- **Implementation tasks:** Reuse unit aliases/factors while retaining category distinctions. Add compatible sums/differences, scaling, linear products/quotients and bounded integer powers. Carry units and approximation metadata through references and target conversions. Diagnose affine/reciprocal cases that lack defined algebra.
- **Behavioral requirements:** Length sums normalize correctly, length×length produces area, distance/time produces speed, and invalid dimensions never silently become scalars.
- **Edge cases:** Compound unit parsing; unit names colliding with variables; temperature offsets; reciprocal fuel economy; month/year approximations; overflow/nonfinite factors; dimensional exponent limits.
- **Regression constraints:** Existing direct conversion syntax/catalog output and scalar arithmetic remain unchanged. Do not remove approximation warnings or equate distinct catalog meanings accidentally.
- **Verification:** Unit tests for C.1 examples, compatible conversion, cancellation of dimensions and invalid sums/products; run directly affected unit-conversion and pad quantity tests.
- **Acceptance criteria:** Accepted unit operations retain meaning and provenance; incompatible/unsupported cases are clear; old conversions still pass their existing fixtures.
- **Commit boundary:** After the stated focused checks and diff review: `feat(calc): [M7-A] add bounded unit-aware arithmetic`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M7-B — Examples, reference and calculator acceptance

- **Objective:** Make the new calculator behavior discoverable and close C02 coverage.
- **Code ownership:** src/calculation_pad/{examples,README.md}; in-app example/reference UI; plugin help/command descriptions; relevant tests.
- **Implementation tasks:** Add executable examples and accurate precedence/width/sign/exactness reference. Document inspector isolation, copy formats, integer division, comparisons, unit algebra and exclusions. Review all selected IDs 1–12.
- **Behavioral requirements:** A user can discover and reproduce the new operations without external instructions; old examples remain valid.
- **Edge cases:** Examples with large exact values; escape/token formatting; unsupported optional 128-bit operations; negative bit patterns; stale documentation after grammar changes.
- **Regression constraints:** No claim of exact decimal arithmetic; no examples that rely on inspector-global state to reinterpret a saved document.
- **Verification:** Run every new example through the real engine, one focused calculator integration filter, and independent C02 diff review. Record native inspector/editor checks or their precise environment gap.
- **Acceptance criteria:** IDs 1–12 are accounted for; examples agree with code; C02 handoff includes real tests and actual final tip.
- **Commit boundary:** After the stated focused checks and diff review: `docs(calc): [M7-B] document and verify programmer and unit workflows`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.


### C03 — Rich visual Diagram Studio

Run C03/M0-A directly from C02’s actual final tip. M8 establishes persistence and scene authority before richer UI.

#### M8-A — Authoring modes and backward-compatible document format

- **Objective:** Separate Mermaid authority from rich visual authority without damaging existing documents.
- **Code ownership:** src/diagram/{model,session,document,codec,persistence}.rs; src/gui/diagram_studio.rs; existing .mld fixtures.
- **Implementation tasks:** Introduce explicit authoring kind and bounded visual payload. Read version-1 source/graph/history faithfully; define a new version for visual documents. Keep mode selection separate from Source/Visual/Split presentation. Preserve transactional save/conflict and dirty-transition behavior.
- **Behavioral requirements:** Existing Mermaid files reopen unchanged; visual documents save/reopen as visual documents, not regenerated Mermaid.
- **Edge cases:** Invalid/unsupported Mermaid; unknown future format; interrupted save; oversized payload; stale async result; save-as conflict; session history migration.
- **Regression constraints:** No eager rewrite on open, lossy source normalization, dual authoritative copies, or forcing unsupported Mermaid into a native graph.
- **Verification:** Round-trip old fixtures, malformed/future version and bounded decode tests; test new visual save/reopen and cancelled authoring transition.
- **Acceptance criteria:** Both authoring modes have explicit ownership; version-1 compatibility is demonstrated; no import/open failure mutates the active document.
- **Commit boundary:** After the stated focused checks and diff review: `feat(diagram): [M8-A] separate visual authoring and versioned documents`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M8-B — Rich scene objects, styles and layers

- **Objective:** Establish the shared model/scene foundation for the approved visual canvas.
- **Code ownership:** src/diagram/{model,scene,session,export}.rs; proposed visual object/style types; visual inspector.
- **Implementation tasks:** Add required shapes, bounded custom primitive presets, dimensions/rotation, styles, layers/z-order, free text and stable IDs. Use shared scene geometry for rendering/hit bounds/export. Implement validated, undoable property edits.
- **Behavioral requirements:** Shape/style/layer changes persist and render consistently; custom presets are reusable without executing arbitrary code.
- **Edge cases:** Degenerate geometry; nonfinite coordinates; long Unicode labels; missing font glyphs; translucent/shadow bounds; duplicate IDs; object/layer limits.
- **Regression constraints:** Keep existing native Mermaid geometry/export behavior valid and preserve exact source authority in Mermaid mode.
- **Verification:** Model validation, scene geometry, save/restore and undo tests; rendering/export consistency checks for each shape and transformed bounds.
- **Acceptance criteria:** All required primitive shapes, styling and ordering are real model properties, not transient paint effects; saved/reopened documents retain them.
- **Commit boundary:** After the stated focused checks and diff review: `feat(diagram): [M8-B] model rich shapes styles and layers`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M9-A — Direct manipulation and navigation

- **Objective:** Make the visual canvas productive for precise interactive layout.
- **Code ownership:** src/gui/diagram_studio.rs; diagram session/scene geometry; existing camera, snapping and selection controls.
- **Implementation tasks:** Add resize/rotate handles, marquee selection, group transforms, smart alignment guides, align/distribute and minimap. Reuse existing grid/pan/zoom/Fit behavior. Batch a completed gesture as one undoable edit and retain stable selection.
- **Behavioral requirements:** Users can arrange multiple objects without source editing; keyboard and pointer operations target the focused canvas only.
- **Edge cases:** Zoomed/rotated hit testing; negative coordinates; drag leaving viewport; Escape mid-drag; empty selection; locked/hidden layers if exposed; high DPI.
- **Regression constraints:** Preserve Mermaid editor keyboard semantics, native Alt+Space, existing camera and viewer independence. No canvas Delete while a text field owns input.
- **Verification:** Geometry/selection/history tests plus native drag/resize/rotate/marquee/align/undo at normal and scaled presentation; screenshots show actual handle/control reachability.
- **Acceptance criteria:** Every approved manipulation works with deterministic history; guides/minimap reflect the current scene; source/text input is not hijacked.
- **Commit boundary:** After the stated focused checks and diff review: `feat(diagram): [M9-A] add visual transforms alignment and navigation`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M9-B — Connectors, containers and reusable assets

- **Objective:** Complete connected diagrams with grouping and bounded images/components.
- **Code ownership:** Diagram scene/model/session; layout/routing helpers; visual canvas and inspector; asset persistence.
- **Implementation tasks:** Implement anchored straight/elbow/curved arrows and labels, bounded rerouting, swimlanes/containers and group transforms. Add image/icon import and reusable components with bounded asset storage and safe resource handling.
- **Behavioral requirements:** Connected endpoints follow moved/resized shapes; containers move their contents intentionally; imported images and components survive save/reopen.
- **Edge cases:** Self-loops; parallel edges; rotated anchors; deleted endpoints; container cycles; impossible route; damaged/oversized image; external-resource references; missing assets.
- **Regression constraints:** No endless routing search, network asset fetch, executable SVG/HTML, dangling IDs, or repeated rerouting on unchanged idle frames.
- **Verification:** Connection geometry/routing budget tests; nested/container validation; safe asset decode and serialization tests; native connected diagram with move/resize/undo and imported icon.
- **Acceptance criteria:** Required connector styles and containers are usable; asset round-trip succeeds; routing failure is bounded and visible; no stale path survives a current geometry change.
- **Commit boundary:** After the stated focused checks and diff review: `feat(diagram): [M9-B] add anchored connectors containers and assets`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M10-A — Mermaid assistance, templates, conversion and library

- **Objective:** Connect the two authoring experiences without destructive synchronization.
- **Code ownership:** src/diagram/{mermaid,rendering,templates,document}.rs; Diagram Studio/Markdown handoff; proposed local diagram library index.
- **Implementation tasks:** Add context-appropriate Mermaid completion/snippets and diagnostics; expand visual templates for requested diagram categories; provide explicit supported Mermaid-to-visual copy; add bounded searchable recent/saved diagram entries and duplicate/reuse.
- **Behavioral requirements:** Users can choose the right authoring mode, start from a useful template and reopen/reuse a diagram without losing original Mermaid source.
- **Edge cases:** Unsupported source family; invalid source draft; duplicate file path; missing library file; stale async preview; dirty replace-template action; Markdown-owned source copy.
- **Regression constraints:** Do not promise bidirectional rich-canvas Mermaid conversion. Preserve existing templates, source-rendered families, Markdown handoff identity and Save/Discard/Cancel.
- **Verification:** Autocomplete token/context tests; every template opens and renders; conversion keeps original byte content; library missing-file/duplicate/reopen tests and one native end-to-end conversion.
- **Acceptance criteria:** All requested template categories exist in an appropriate mode; assistance never corrupts source; conversion/library operations are explicit and nondestructive.
- **Commit boundary:** After the stated focused checks and diff review: `feat(diagram): [M10-A] add authoring assistance templates and reuse`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M10-B — Current-scene exports and viewer parity

- **Objective:** Export the rich canvas accurately while preserving existing source/render protections.
- **Code ownership:** src/diagram/{scene,export,persistence}.rs; Diagram Studio export UI and read-only viewer.
- **Implementation tasks:** Add transparent/chosen backgrounds and bounded resolution controls for rich scenes; render shapes/text/images/connectors/rotation consistently to PNG/SVG. Bind export to current document/revision. Extend viewer to visual documents without a second editable session.
- **Behavioral requirements:** Exports and viewer depict the actual current document; changing the authoring mode never exports an unrelated last-valid image.
- **Edge cases:** Large transformed bounds; shadow clipping; unsupported font glyphs; unsafe SVG resources; zero objects; destination collision; source edit during export; stale render.
- **Regression constraints:** No overwrite of native source documents; preserve unoccupied destination rules and exact Mermaid-source export; viewer has independent camera only.
- **Verification:** PNG decode/SVG structure and geometry tests, current-revision/occupied-path tests, and visual comparison of one rich diagram with its exported images.
- **Acceptance criteria:** All rich properties export as supported or produce explicit diagnostic; alpha/resolution controls work; stale exports are rejected and original files untouched.
- **Commit boundary:** After the stated focused checks and diff review: `feat(diagram): [M10-B] export current rich scenes and sync viewer`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M11-A — Bounded draw.io XML interchange

- **Objective:** Implement honest import/export interoperability for a useful documented subset.
- **Code ownership:** Proposed draw.io adapter under src/diagram/; existing XML safety utilities where suitable; visual model; export/import dialogs.
- **Implementation tasks:** Map primitive vertices, labels, sizes/positions/rotation, supported style/layer/group properties and connectors using stable IDs. Write uncompressed XML. Parse input with strict resource/entity policies; support or explicitly reject compressed pages. Provide page selection or preflight rejection and a report of unsupported objects/styles.
- **Behavioral requirements:** Import creates a new reviewable visual document; export does not overwrite the original .drawio file by default; loss/unsupported content is disclosed before acceptance.
- **Edge cases:** XXE/DTD; malformed XML; expanded/decompression bombs; cyclic parents; missing endpoint IDs; HTML labels; external assets; multi-page content; unknown styles.
- **Regression constraints:** No silent dropping, remote resource resolution, full-compatibility claim, or conversion through Mermaid that loses geometry.
- **Verification:** Golden uncompressed fixtures, unsupported/malicious input tests, supported property round-trips; when compressed decoding exists test its expansion bounds. Verify one generated file opens in an available draw.io reader.
- **Acceptance criteria:** The subset is real and documented; supported geometry/connectivity survive interchange; unsupported input cannot mutate the current document or original file.
- **Commit boundary:** After the stated focused checks and diff review: `feat(diagram): [M11-A] add safe drawio subset interchange`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M11-B — Diagram usability acceptance and documentation

- **Objective:** Close C03 against all selected diagram requirements and legacy workflows.
- **Code ownership:** docs/DIAGRAM_STUDIO.md; docs/DIAGRAM_MERMAID_SUBSET.md; docs/DIAGRAM_EXPORT.md; relevant GUI/session tests and host integration.
- **Implementation tasks:** Update user guidance and capability tables; verify two-mode UX, tool availability, dock/pop-out, viewer, dirty close and library. Resolve feature-related independent review findings. Account individually for IDs 27–46.
- **Behavioral requirements:** A user can author and reopen both kinds of diagrams, inspect/export them and understand conversion/interchange boundaries.
- **Edge cases:** Constrained host sizes; mixed DPI; root hidden; dirty source while switching tools; current/stale preview; optional compressed-page import unavailable.
- **Regression constraints:** No broad unrelated theme/editor rewrite and no removal of valid old Mermaid behavior to simplify visual authoring.
- **Verification:** One scoped C03 regression filter and native representative Mermaid plus rich-canvas flows from H; independent review of model/version/export boundaries.
- **Acceptance criteria:** IDs 27–46 are mapped to evidence; any environment-only gap is explicit; C03 final tip is committed and handed off without intermediate merge.
- **Commit boundary:** After the stated focused checks and diff review: `docs(diagram): [M11-B] verify and document dual authoring workflows`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.


### C04 — Explorer multi-tab folder lists

Run C04/M0-A directly from C03’s committed final tip.

#### M12-A — Targeted Explorer tab operation

- **Objective:** Build a bounded native operation that proves one-window tab targeting.
- **Code ownership:** src/mkmacro/{uia,windows,input,executor}.rs as reference; existing platform wrappers; proposed Explorer-tab domain/backend.
- **Implementation tasks:** Create/identify a new Explorer window, detect supported tab controls, open validated folders in order and verify each navigation. Model progress/cancel/partial outcomes and target identity. Prefer target-bound UIA; guard any keyboard fallback with current ownership/control checks.
- **Behavioral requirements:** Operation success requires observed folders in the intended tab/window target, not merely a successful explorer.exe spawn.
- **Edge cases:** Explorer process reuse; pre-existing windows; foreground change; slow UNC; invalid/unavailable path; user closes target; unsupported tab UI; locale differences; UIPI restrictions.
- **Regression constraints:** No mutation of unrelated tabs, forced activation loop, blind sleep, clipboard clobber, private injection or change to existing single-folder macro behavior.
- **Verification:** Fake-backend sequencing/cancellation/identity tests plus bounded native proof with at least three disposable directories and a pre-existing unrelated Explorer window.
- **Acceptance criteria:** Known supported environment demonstrates actual same-window tabs; unsupported/partial states are honest and safe. A stub returning Unsupported everywhere is incomplete.
- **Commit boundary:** After the stated focused checks and diff review: `feat(explorer): [M12-A] open verified folder tabs in one target window`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M12-B — Saved folder lists and command integration

- **Objective:** Expose Explorer tabs as a usable, persistent launcher tool.
- **Code ownership:** Proposed folder-list store/dialog/plugin; src/commands/ and GUI command host; existing action catalog/hotkey routing; common host.
- **Implementation tasks:** Add named ordered list CRUD/reordering, validation, launch progress and explicit cancellation. Use bounded versioned local persistence and typed action invocation from launcher and existing action pickers.
- **Behavioral requirements:** List order and intentional duplicate entries survive save/reopen. Launching does not modify list data, unrelated folders or window layout state.
- **Edge cases:** Empty list; duplicate names; malformed/future persisted data; edited list during active run; missing folder; two invocations while busy.
- **Regression constraints:** No workspace/project concept, implicit retries or new macro DSL. Preserve folder bookmarks and current command prefixes.
- **Verification:** Persistence/command/discoverability tests; native launch from the actual query UI with five paths including spaces/Unicode where available; independent C04 review.
- **Acceptance criteria:** Users can save/reopen/launch/cancel a folder list; window/tab results are verified or explicitly partial; C04 tip and native status are handed off.
- **Commit boundary:** After the stated focused checks and diff review: `feat(explorer): [M12-B] expose saved folder tab lists in launcher`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.


### C05 — Read-only folder size analysis

Run C05/M0-A directly from C04’s committed final tip.

#### M13-A — Bounded progressive scan and accounting

- **Objective:** Produce honest folder-size results without reading or modifying file contents.
- **Code ownership:** src/file_operations.rs; src/duplicate_finder/{discovery,session,model}.rs as reusable patterns; proposed folder-size domain/worker.
- **Implementation tasks:** Implement cancellable metadata traversal and incremental subtree aggregation. Reuse identity/reparse/cancellation capabilities without running duplicate hashes. Bound entries/depth/path bytes/diagnostics and totals. Track complete/partial/cancelled state and logical per-entry accounting.
- **Behavioral requirements:** Large scans report progress; unreadable entries remain explicit; hard-link accounting is labeled and consistent; no unwanted recursion/hydration.
- **Edge cases:** Junction cycles; root replacement; file size changes; disappearing files; denied subdirectories; UNC stalls; sparse/hard-linked/cloud files; u64 sum overflow.
- **Regression constraints:** No deletion/content reads, duplicate-finder behavior changes, or UI-thread traversal. Retire late old-generation results.
- **Verification:** Deterministic fake-tree totals; real temporary directory sizes; cancellation/reparse/partial permission fixtures when supported; bounded-memory/result-count checks.
- **Acceptance criteria:** Totals reconcile with declared metric on a known tree; partial status cannot be mistaken for complete; limits/cancel stop further accepted publication.
- **Commit boundary:** After the stated focused checks and diff review: `feat(files): [M13-A] scan folder sizes with bounded truthful accounting`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M13-B — Folder-size interface and launcher actions

- **Objective:** Make scan results useful for finding large files and directories.
- **Code ownership:** Proposed folder-size dialog/plugin; typed commands and file-search actions where existing architecture fits; common host; ActivityCenter.
- **Implementation tasks:** Add root selection, progressive sorted tree/list, top entries, size/count/percentage, drill-down, Refresh/Cancel, Copy Path and Reveal/Open. Preserve active job across host migration and provide explicit partial-result notices.
- **Behavioral requirements:** User can identify and navigate to a large directory without a cleanup/deletion flow or waiting for final scan completion.
- **Edge cases:** Zero-byte tree; partial parent percentages; re-sort during updates; stale selection after refresh; closed host; unavailable destination; huge names.
- **Regression constraints:** No permanent background scan/index, implicit file mutation or unbounded UI rendering. Existing File Search/Duplicate Finder stays unchanged.
- **Verification:** Command/UI state tests; native disposable directory scan, cancellation, refresh and detach/dock; independent C05 review and documented metric.
- **Acceptance criteria:** Largest entries match the fixture; controls stay responsive; every output distinguishes completeness; C05 final tip and verification status are recorded.
- **Commit boundary:** After the stated focused checks and diff review: `feat(files): [M13-B] expose progressive folder size analysis`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.


### C06 — Read-only file lock inspection

Run C06/M0-A directly from C05’s committed final tip.

#### M14-A — Restart Manager inspection boundary

- **Objective:** Enumerate reported file users with correct native lifetimes and bounded failure handling.
- **Code ownership:** Proposed Windows lock-inspection adapter; existing process/platform wrappers; Cargo.toml Windows features only as required.
- **Implementation tasks:** Own Restart Manager session with guaranteed cleanup, register the selected regular file, enumerate results with bounded ERROR_MORE_DATA retries and preserve process start identity. Retrieve optional metadata without turning metadata failure into an empty successful result.
- **Behavioral requirements:** Inspection is read-only and asynchronous; errors and empty results are distinct; stale refreshes cannot replace current results.
- **Edge cases:** File missing/renamed; directory selected; access denied; process exits/reuses PID; result set grows; COM/native initialization failure; session cleanup error.
- **Regression constraints:** Never call RmShutdown/RmRestart, kill processes, close remote handles or auto-elevate. No claim that all reported users necessarily deny the attempted file action.
- **Verification:** Fake/native-wrapper error and cleanup tests. Native disposable helper holds a file using incompatible share flags; inspect it, release it and refresh. Clean up only the known fixture process.
- **Acceptance criteria:** Known supported native fixture is reported; lifecycle cleanup is covered; no-results wording does not imply a guaranteed unlocked file.
- **Commit boundary:** After the stated focused checks and diff review: `feat(files): [M14-A] inspect reported file users without unlocking`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M14-B — File lock inspector UI and actions

- **Objective:** Expose actionable inspection details without destructive controls.
- **Code ownership:** Proposed file-lock dialog/plugin; file-search action integration; typed commands; shared host; existing process metadata presentation.
- **Implementation tasks:** Add file selection/path invocation, Refresh, identity/PID/name/path/time columns, partial/error states and Copy Details/Reveal. Present limitations directly in the result view without alarmist or absolute claims.
- **Behavioral requirements:** Users can see which applications are reported and decide what to do themselves; host movement preserves the selected file and current result.
- **Edge cases:** Long/Unicode path; unavailable metadata; prior process exits before reveal; input changes during inspection; unsupported platform behavior.
- **Regression constraints:** No terminate/unlock button disguised as an inspection action; do not change existing process-plugin actions or global permissions.
- **Verification:** Command/format/state tests and native query→inspect held fixture→release→refresh flow; independent C06 review.
- **Acceptance criteria:** Inspector is discoverable and read-only, empty/failed/partial states are accurate, and final tip/verification results are handed off.
- **Commit boundary:** After the stated focused checks and diff review: `feat(files): [M14-B] expose file lock inspection in launcher`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.


### C07 — Silent GIF / MP4 capture and cumulative closure

Run C07/M0-A directly from C06’s committed final tip. M17 stays on this branch.

#### M15-A — Recording target and bounded capture lifecycle

- **Objective:** Establish recording ownership and a frame source with bounded resources.
- **Code ownership:** src/pinboard/live.rs native capabilities; src/pinboard/source.rs; ScreenshotCaptureOwner/selection integration; proposed recording domain/platform adapter.
- **Implementation tasks:** Model Idle/Selecting/Countdown/Recording/Finalizing/Completed/Failed/Cancelled states and stable request/target identity. Reuse suitable D3D/WGC low-level operations without coupling the recorder session to a pin. Implement bounded frame delivery, timing, resize policy and cancellation.
- **Behavioral requirements:** Capture begins only after explicit selection/start; root hide does not lose control; target loss never retargets the foreground application.
- **Edge cases:** Unsupported capture; secondary/negative-coordinate display; mixed DPI; spanning region; lost target; resize/minimize; device loss; backpressure; late frame after stop.
- **Regression constraints:** No change to still screenshots/Pinboard capture semantics, permanent idle capture, duplicate suppression owner or full-clip raw-frame buffer.
- **Verification:** Synthetic frame-source/state tests with an injected clock; queue/resource bound tests; supported native window/display capture to a controlled sink. Test shutdown and retired completion.
- **Acceptance criteria:** Frame source observes target identity, queue bounds and timing; all termination paths retire resources; platform failure is explicit rather than an empty success.
- **Commit boundary:** After the stated focused checks and diff review: `feat(recording): [M15-A] add bounded silent capture lifecycle`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M15-B — Streaming animated GIF output

- **Objective:** Encode a real time-correct GIF without buffering the entire recording.
- **Code ownership:** Existing image/GIF dependencies; proposed recording encoder interface; temporary-output/publication helpers.
- **Implementation tasks:** Use bounded GIF quantization/encoding with correct frame delays and dimensions. Support selected fps/scaling, preserve elapsed time when frames are dropped/coalesced, finalize on Stop and discard only task-owned output on Cancel.
- **Behavioral requirements:** Output animates for approximately the recorded interval and is published only after finalization; visible scaling/settings match actual file properties.
- **Edge cases:** Identical frames; irregular timestamps; zero/one frame; palette quality; long clip within bounds; disk full; encoder error; filename collision; cancel during finalize.
- **Regression constraints:** No external executable download or overwrite of an existing recording; no audio path or off-thread unsafe UI access.
- **Verification:** Decode generated multi-frame GIF fixtures and check frame count/delays/dimensions; cancellation/collision/failure tests; native short moving fixture when capture is available.
- **Acceptance criteria:** GIF is independently decodable and animated; duration survives frame dropping; cleanup is correct on every state transition.
- **Commit boundary:** After the stated focused checks and diff review: `feat(recording): [M15-B] encode bounded animated gif clips`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M15-C — Video-only MP4 output

- **Objective:** Encode a real finalized silent MP4 using a supported local backend.
- **Code ownership:** Proposed recording encoder boundary; Windows Media Foundation bindings/features; temporary-file finalization/publication.
- **Implementation tasks:** Add H.264/MP4 sink configuration with exactly one video stream and no audio. Negotiate supported frame size/pixel format/stride, timestamp samples monotonically and finalize the sink before publication. Report missing codec/media capabilities rather than silently falling back to another format.
- **Behavioral requirements:** An MP4 selection produces a playable MP4 with the displayed dimensions and elapsed duration; unsupported systems receive a clear actionable capability result.
- **Edge cases:** Odd output dimensions; row stride; color conversion; missing codec/Windows media components; encoder backpressure; finalization failure; zero frames; disk full; cancellation.
- **Regression constraints:** Do not claim GIF satisfies MP4; no ffmpeg PATH requirement, audio capture or elevated install. Keep new Windows/dependency features narrowly scoped.
- **Verification:** Encode deterministic timed frames, independently inspect/decode container metadata and confirm one video/zero audio streams; check failure cleanup and one native clip playback.
- **Acceptance criteria:** Output is genuinely playable video-only MP4 on a supported target; timestamps/duration are correct; finalization failure cannot report saved success.
- **Commit boundary:** After the stated focused checks and diff review: `feat(recording): [M15-C] encode finalized video-only mp4 clips`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M16-A — Recorder controls and command integration

- **Objective:** Expose both formats in a quick usable recorder with reliable stopping.
- **Code ownership:** Proposed recorder setup/controller/dialog; typed commands/plugin; common host; existing region selector; ActivityCenter and hotkey configuration.
- **Implementation tasks:** Add target/format/fps/scale/duration settings, countdown, elapsed/status, Stop & Save and Cancel/Discard. Keep a reachable compact controller while root is hidden. Register optional stop hotkey only through existing infrastructure and only when configured; disable conflicting capture actions with an explanation.
- **Behavioral requirements:** User can start and stop a bounded clip without opening a large editor; selecting a format never starts recording by itself.
- **Edge cases:** Cancel selection/countdown; hotkey conflict; root hidden; main dialog closed while controller active; source window changes focus; save chooser cancel; busy capture owner.
- **Regression constraints:** No unconditional global Escape capture, duplicate hotkeys or silent audio settings. Preserve normal pointer/click behavior after selection and recording.
- **Verification:** Controller/action/state tests and native flows through actual launcher query for GIF and MP4. Verify stop stays reachable when root hides and ordinary keyboard/mouse input still works.
- **Acceptance criteria:** Both formats are discoverable and operable; displayed state follows actual capture/finalization, and no output is announced before success.
- **Commit boundary:** After the stated focused checks and diff review: `feat(recording): [M16-A] expose silent recorder and reachable stop controls`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M16-B — Recording interoperability and cleanup acceptance

- **Objective:** Prove recording coexists with existing captures and leaves no active resources.
- **Code ownership:** Recorder, screenshot/Pinboard/Screen Draw owners, shared host shutdown and native frame/encoder cleanup; user documentation.
- **Implementation tasks:** Exercise accepted/denied concurrent capture transitions, repeated record/stop, cancellation and app shutdown. Confirm first-party user messages describe limits and optional cursor handling. Ensure unrelated existing live pins are not retired to clean up recorder resources.
- **Behavioral requirements:** After completion/cancel/failure the desktop and launcher behave normally, and later recording/still capture can start again.
- **Edge cases:** Stop and shutdown racing; stale frame/result; minimized target; permission/capability denial; device removal; partial output; another existing live capture session.
- **Regression constraints:** No orphan helper, stuck click suppression, leaked native callback registration, infinite hidden repaint or deletion of another operation’s file.
- **Verification:** Focused lifecycle tests, one bounded repeated-run native check and actual GIF/MP4 artifact inspection; independent C07 review of resource/publication ownership.
- **Acceptance criteria:** No known feature-related lifecycle defect remains; real format evidence and any native-environment gaps are separate; user docs reflect actual behavior.
- **Commit boundary:** After the stated focused checks and diff review: `fix(recording): [M16-B] verify capture coexistence and cleanup`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M17-A — Scoped cumulative verification and final review

- **Objective:** Check the cumulative delivered features without restarting every historical test campaign.
- **Code ownership:** All task-touched owners; requirement/host matrices; configured independent reviewer; first-party lint targets.
- **Implementation tasks:** Audit all selected IDs and every host row; resolve feature-related review findings; run the union of impacted focused tests once, reusing relevant earlier native evidence. Perform one final scoped first-party Clippy pass and formatting/diff review. Inspect total changes against the original parent and category boundaries.
- **Behavioral requirements:** Existing semantics and new user workflows remain compatible across the final cumulative branch.
- **Edge cases:** Historical unrelated warnings/failures; final shared-host fix invalidating earlier evidence; missing native capability; empty test filter; optional features reported as mandatory.
- **Regression constraints:** Do not suppress assertions/warnings broadly, edit nextest TOML to hide failures, upgrade dependencies, or clean vendored/unrelated code to obtain a green summary.
- **Verification:** Use H’s verification matrix and I’s bounded remediation. Re-run only checks invalidated by a final fix; record literal commands/counts and native evidence provenance.
- **Acceptance criteria:** No known feature-related blocker is mislabeled complete; every requirement is implemented+verified or explicitly incomplete/unverified with evidence; reviewer findings are resolved or classified.
- **Commit boundary:** After the stated focused checks and diff review: `chore(productivity): [M17-A] verify cumulative desktop productivity features`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

#### M17-B — Final delivery, status and next-tip handoff

- **Objective:** Freeze a truthful final state and hand off the cumulative exact tip.
- **Code ownership:** Execution ledger; required docs/stall report only when applicable; current branch/index/upstream; external final receipt and response.
- **Implementation tasks:** Commit remaining task documentation, then obtain actual final HEAD and push result. Record category ancestry/checkpoints and per-feature implementation/test/native status. Classify full completion versus partial/environment-blocked. Provide exact next-goal parent instruction and a concise final manual review list.
- **Behavioral requirements:** The final branch contains the full sequential history; the user can distinguish working implementation from unperformed native acceptance.
- **Edge cases:** Push rejected; local predecessor unavailable on another machine; reporting commit advances tip; unrelated user changes remain; genuinely blocked requirement.
- **Regression constraints:** No forced push, automatic integration merge, worktree, recursive self-hash commits, or promotion of unrun checks to Passed.
- **Verification:** Inspect git status/index/upstream; verify each recorded baseline parent and ancestry; verify final SHA after the last tracked commit and compare actual push state.
- **Acceptance criteria:** Final report includes parent_branch, parent_tip_sha, baseline_sha, checkpoint SHAs, final_tip_sha, clean/dirty state and push status. The next task must branch from this exact final tip.
- **Commit boundary:** After the stated focused checks and diff review: `docs(productivity): [M17-B] record final evidence and chained handoff`. Attempt one normal push; record the result. Remediation uses a new commit, not rewritten history.

## G. Active Checkpoint Commit Cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** Section F defines those boundaries; each category additionally starts with its own M0-A from Section A0.

Commit coherent, checked increments before starting materially different subsections. Large listed checkpoints may use additional small same-branch subcommits such as `M4-C.1`/`M4-C.2` when they are independently understandable; these do not create new branches, new categories, new scope, or permission to postpone all verification. Never treat the predecessor's final commit as the new category baseline.

Before each commit, the parent/orchestrator verifies current branch, HEAD, staged paths and diff, relevant check results, and user-change isolation. Stage named task paths, not an indiscriminate `git add -A`. Commit locally first, then attempt one ordinary non-force push. If no safe remote/upstream is available, record it as unavailable rather than inventing a successful push. Later checkpoints get their normal single attempt; no retry loops or history rewriting to repair push rejection.

A feature checkpoint with mandatory native acceptance still unavailable may be committed as an **implemented, focused-tests-passing increment with native verification pending**. Do not label it fully accepted/complete. An interrupted partial patch may be preserved in a clearly identified recovery commit only when coherent and safe, but that commit is not an accepted checkpoint. Do not carry unrelated/uncommitted work into another category.

Final category handoffs contain: category/branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs, actual `final_tip_sha`, verification status, push state and checkout state. One direct-child transition advances the chain. No merge/rebase/squash/cherry-pick is required or permitted merely to start the next category.

## H. Verification and Testing Strategy

### H.1 Verification hierarchy and budget

Use pure/domain tests for arithmetic, formatting, scene geometry, persistence, scanning, state machines and timing. Use owner/GUI tests for accepted-pass effects, host migrations and command routing. Use native Windows checks for actual focus/windowing, Explorer tabs, Restart Manager behavior, capture/encoding and cleanup. These layers prove different things; none substitutes automatically for another.

For each checkpoint, run the narrowest useful test/build selection once after a coherent change. A failing check allows targeted diagnosis and at most two correction/revalidation cycles for the same failure before the bounded-blocker policy in I. A normal new checkpoint is not a retry of an unrelated older failure. Do not automatically run the entire test suite, every smoke binary, every feature combination, repeated release builds, or repository-wide benchmarks.

Use the existing `fast-dev` profile and cached target when suitable. Reuse a successful native build while it corresponds to the current tested code; rebuild only when relevant source changes invalidate it. A final cumulative affected-test run is justified by this multi-category change, but it is still not automatic authorization for the whole repository suite. Broaden only when a concrete dependency or regression requires it; record why first.

Inspect the existing configuration instead of editing `.config/nextest.toml` or Cargo profiles to make checks fit a budget. Keep compilation progress and test-execution progress distinct. Record literal commands, exits, matching counts, pass/fail/skip totals and relevant output paths. A zero-match filter is not success. Do not kill another task's build/app process to meet a local timeout.

### H.2 Command examples, not claimed results

Resolve exact current test names before running. These commands illustrate the existing package/profile and focused filters; add newly created modules and directly affected tests rather than assuming these filters cover the complete category:

```powershell
cargo nextest list --locked -p multi_launcher --lib --cargo-profile fast-dev -E 'test(calculation_pad::)'
cargo nextest run --locked -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(calculation_pad::)'
cargo nextest run --locked -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(diagram::) | test(gui::diagram_studio::)'
cargo nextest run --locked -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(studio_usability_calculation_host_migration_preserves_cached_session_and_pending_decision) | test(rejected_child_or_retired_host_effect_does_not_apply_source_or_copy)'
cargo check --locked -p multi_launcher --lib --bin multi_launcher --profile fast-dev
cargo build --locked -p multi_launcher --bin multi_launcher --profile fast-dev
```

Do not run all examples mechanically. A check/build that repeats the same expensive compilation without new information is not required. If a justified new dependency changes the lockfile, update it intentionally, review the diff, then resume locked commands. Use offline mode only when dependencies are known to be available.

Format only task-touched Rust files using repository conventions. At final closure, run one appropriate first-party Clippy selection, normally:

```powershell
cargo clippy --locked -p multi_launcher --lib --bin multi_launcher --profile fast-dev --no-deps
```

Include relevant changed integration-test targets and `multi_launcher_table_core` only if actually affected. Fix newly introduced task-related findings. Classify unrelated baseline findings separately, using existing evidence or a bounded baseline comparison when needed; do not assert that every warning is pre-existing without evidence. Do not modify vendored crates merely to clean their warnings or use broad lint allowances to hide regressions. An environment-only inability to run Clippy is an unrun check, not a pass.

### H.3 Automated acceptance matrix

| Area | Minimum meaningful checks |
| --- | --- |
| Common host | Same session identity across migration; state/history/pending decision retention; native/embedded fallback; close vs host move; discarded passes; retired generations; exactly-once effects; root hidden scheduling; parented prompts; hotkeys/Jump Mode focus; geometry compatibility and monitor recovery. |
| Each migrated surface | Open and use actual tool; modify representative state; detach/dock; verify state and side-effect count; close/cancel policy; active worker retention or documented cancellation. Parameterized common-host tests do not replace a tool-specific wiring check. |
| Exact calculator | All required widths/signs; endpoint values; masks/shifts/rotations/count/extraction; overflow/invalid shifts; division zero/MIN; exact values beyond 2^53; references; contextual names; legacy precedence/power/modulo and source preservation. |
| Inspector/comparison | Bit-pattern/representation agreement; dirty working copy preservation; copy format; explicit insert with revision check; no passive source mutation; two-side errors/mismatches; host migration. |
| Units | Compatible sums/products/quotients/powers; row/name references; target conversion; invalid dimensions; affine/reciprocal rejection; approximation propagation; finite/bounded execution. |
| Diagrams | Version-1 read and new rich round-trip; strict limits; history; all primitive/style/transform properties; geometry/hit testing; connector updates/routing bounds; container cycles; asset decoding; current-only exports; source/visual authority and safe conversion. |
| draw.io | Supported mapping fixtures; independent-reader compatibility where available; malformed XML/entities/expansion; cyclic references; missing endpoints; unsupported-style/page reports; no mutation on failure. |
| Explorer tabs | Target identity; path validation/order/duplicates; capability checks; navigation completion; cancellation/focus loss; foreground-safe fallback; partial results; no unrelated-window mutation; persistence and typed commands. |
| Folder sizes | Known logical totals; hard-link policy; partial/cancelled status; inaccessible/deleted/reparse roots; changing metadata; bounds and overflow; stale result rejection; no file content reads/writes. |
| File locks | Native-session cleanup; buffer growth bounds; missing/directory/access errors; process identity; empty-vs-failed results; no destructive API path; controlled held-file fixture. |
| Recording | State transition table; fixed source identity; physical/logical coordinates; bounded queues/temporary output; timestamp/duration correctness; resize/target loss; valid GIF and MP4; zero audio streams; Stop vs Discard; finalization/collision failures; repeated-run/shutdown cleanup. |
| Integration | Real command/plugin discovery and existing action routes; isolated settings/store roots; no idle workers for unused tools; shared host/worker completion does not replay effects. |

### H.4 Native Windows interaction evidence

Codex performs these checks autonomously using supported tools and isolated fixtures. Do not request intermediate user screenshots, command execution, or manual acceptance. Use temporary folders/documents and an isolated application profile. Set test-only Paste Sequences next/cancel shortcuts to `None` before app construction. Do not capture personal notes, clipboard contents, credentials or unrelated desktop data in evidence.

Record Windows build/session, application executable/build identity, active category/checkpoint, tool/capture capability, relevant DPI/monitor configuration, steps, observed result, and sanitized screenshot/artifact paths. Screenshot evidence supplements interaction and state results; a screenshot alone does not prove preserved drafts, correct tabs, or resource cleanup.

| Flow | Required observation |
| --- | --- |
| N1 — Reference host | Dirty Calculation Pad → pop out → hide launcher → edit/undo/copy → dock → cancel close. Same source/history remains, focus/Jump Mode belongs to the correct surface. |
| N2 — Document and macro hosts | Notes and Markdown editable detached sessions; MkMacro harmless fixture retaining selection and exactly-once execution. Owned child prompts are reachable. |
| N3 — Settings/mouse hosts | Main and Dashboard Settings save/cancel, Mouse/Gesture settings edit/record/cancel; no draft auto-commit or residual input suppression. |
| N4 — Remaining utility families | Representative file workflow, clipboard workflow including second sequence run, structured editor/table, timer/calendar and image/ZIP inspection. Every requested surface still needs an automated wiring check even when native evidence is sampled by family. |
| N5 — Programmer tools | Large exact result and shifted mask → independent inspector → bit toggle → copy chosen format → explicit insert/undo → comparison. Ordinary scalar pad remains text-first. |
| N6 — Mermaid legacy | Existing `.mld`, supported source editing, invalid/stale preview, viewer and current export; exact source/dirty decisions survive. |
| N7 — Rich diagram | Add shapes/text/image, connect, resize/rotate, multi-select/align, use container/layer, undo, save/reopen, PNG/SVG export and supported draw.io interchange. |
| N8 — Explorer tabs | Five ordered test folders in one identified new window with another Explorer window already open. Verify paths, tab order and unrelated window preservation; cancel/foreground-loss test stops safely. |
| N9 — Folder sizes | Known temporary tree → progressive sorted results → cancel → refresh → detach/dock; totals and incomplete status match expected accounting. |
| N10 — File users | Hold a disposable file in a known helper → inspect → release → refresh. Confirm identity and truthful empty/failed presentation. |
| N11 — Recording | Record moving controlled content to GIF and MP4; hide root and stop; independently inspect/play artifacts, duration/dimensions and absence of audio. Include non-primary/DPI geometry where available. |
| N12 — Cleanup | Cancel selection/countdown/recording, close target, repeat start/stop, exit task app; normal mouse/hotkeys/still screenshot/live Pinboard remain usable. Inspect only task-owned resources/processes. |

When the environment lacks a second display, a particular DPI mode, an independent draw.io reader, or an interactive observer, distinguish deterministic coverage from **unverified native behavior**. Do not fabricate the unavailable configuration. The supported reader may be used locally with synthetic data; do not upload private diagrams to a remote service for validation.

### H.5 Performance checks are focused, not a new campaign

Check that new services are lazy, idle capture/scan/encoding work is absent, and unchanged scenes/results do not recompute on every frame. Exercise one bounded large fixture per affected algorithm to establish configured caps and responsiveness. Document the selected limits and test sizes; do not claim arbitrary production-scale performance from a small fixture.

Use existing instrumentation when available for hidden-window wakeups/worker retirement. Do not create a general benchmark initiative, retune the entire build, or repeatedly measure startup unless an actual regression implicates these changes.

## I. Risk Management and Bounded Validation

### I.1 Consequential failure modes and mitigations

| Failure mode | Required mitigation |
| --- | --- |
| Many dialogs acquire competing owners | Common host/state contract; staged migrations; explicit per-surface inventory; stale-generation/accepted-pass tests; no copied sessions. |
| Calculator breaks existing documents or loses bits | Keep scalar compatibility path, exact token parsing, explicit width/sign semantics, contextual names, old grammar fixtures and boundary tests. |
| Rich diagrams lose source/assets on conversion or save | Discriminated authoritative format; versioned migration; explicit conversion copy; bounded assets; originals untouched; collision-safe writes. |
| Explorer sends input to the wrong window | Observed target identity and UIA patterns, guarded fallback, expected focused control, cancellation/ownership checks, no blind timing or false success. |
| Size totals or lock results overstate certainty | Explicit accounting and partial status; “reported users” wording; no physical-size/unlocked guarantee without evidence. |
| Recorder leaks resources, consumes unbounded memory, or produces corrupt output | Bounded queues/output, explicit lifetime owners, timestamp tests, format inspection, finalization before publication and owned cleanup. |
| Windows/UI observer is unavailable | One bounded capability check and supported recovery; preserve unverified gates; continue independent source work without repeated environment probing. |
| Push/branch mismatch stalls or fragments the chain | Exact parent/baseline checks, single checkout, commit locally first, best-effort push, recorded cumulative final tip. |

### I.2 Classification and continuation rules

Track **implementation**, **automated verification**, **native verification**, **review**, and **delivery/push** separately. Useful overall states are `NOT_STARTED`, `IN_PROGRESS`, `COMPLETE`, `IMPLEMENTED_NATIVE_PENDING`, and `BLOCKED`. Push pending is a delivery state, not automatically a feature blocker.

- **Feature correctness blocker:** a failing requirement/test, data-loss path, unsafe targeting, noncompiling feature, unavailable required implementation, or missing typed integration. Diagnose and fix within the bounded correction budget. Do not proceed with a dependent milestone that requires the broken boundary. Independent categories may continue only from a coherent committed state that does not silently inherit an unsafe active feature; record exact omissions and dependencies.
- **Native environment blocker:** supported interaction/capture/permission capability unavailable, without evidence of a product defect. Run one capability probe and at most one supported recovery/reprobe for the unchanged environment. If the same blocker was already established and remains unchanged, preserve it rather than restarting the two-attempt allowance for every category. No approval bypass, repeated host/service resets, alternate unauthorized capture paths, or provisioning loop.
- **Unrelated pre-existing issue:** retain evidence and report separately. Do not consume this Goal's implementation scope fixing unrelated Clippy findings, historical nextest failures or prior native checklists.
- **Optional enhancement:** explicitly defer; do not turn it into a new required milestone. The approved 50 selections themselves are not optional.

A category whose implementation, focused automated checks and review are sound may be **frozen at a committed tip with native-environment gaps recorded**, then hand off to the next category. This is authorized to avoid repeating the prior stalled-goal pattern. The category and overall Goal remain `IMPLEMENTED_NATIVE_PENDING`, not `COMPLETE`, until mandatory native gates actually pass. Later dependent UI work may use the reviewed implementation with that disclosed limitation; known correctness failures are not treated the same way.

A capability check returning Unsupported is a valid user-facing error path but **not proof that a feature was implemented successfully for its intended supported Windows environment**. In particular, Explorer same-window tabs and actual GIF/MP4 output need positive supported-target evidence. Distinguish a machine that lacks an encoder from an implementation that never added MP4 encoding.

### I.3 Bounded blockers and stall reports

For the same concrete failure, use one initial validation and at most two focused correction/revalidation cycles. Stop expanding the investigation once a proven external blocker or exhausted budget is reached. Record actual attempts, not just “tried several things.” Do not reset a budget by renaming the checkpoint or creating a new branch. New evidence or an explicitly changed environment can justify a new narrowly recorded attempt; routine repetition cannot.

When a genuine stop, pause, or exhausted mandatory gate occurs, follow `AGENTS.md`'s stalled-goal report policy. Use the existing queue's `stalls/` directory, or `docs/plans/execution/standalone/desktop-productivity-expansion/stalls/<UTC-timestamp>.md`. Include category/checkpoint, timestamp/status, checkout and parent/base/HEAD/upstream at preparation time, last accepted checkpoint, proven vs unproven cause, exact failed/unrun command or native step, diagnostics, correction budget, cleanup, staged/unstaged state, preserved partial work, affected dependencies, and the smallest safe next action/authority.

Keep one concise human status index when required by repository policy, not a second competing operational ledger. Do not include secrets or uncontrolled logs. Commit a safe documentation-only report on the intended category branch and attempt one normal push; label it as reporting, not accepted implementation. If writing/committing is unsafe, record in the existing external ledger and stop without manufacturing a clean checkout.

## J. Definition of Done

The overall Goal is **Complete** only when all of the following are true:

1. All required selected items (1–46, 61, 88, 89, 119) have working user-facing implementations and traceable acceptance evidence. Optional 128-bit arithmetic/compressed import are explicitly labeled and do not hide required omissions.
2. Calculation Pad remains text-based; scalar compatibility and exact integer semantics are verified; the separate inspector, comparisons, unit arithmetic and documentation work as specified.
3. Every required C.2 host surface has been accounted for. One live session, exactly-once effects, dirty/undo state, worker ownership, geometry and focus survive migration; family-level evidence does not silently omit a named dialog.
4. Both Diagram Studio authoring experiences work; old documents and source survive; rich geometry/assets round-trip; conversions, exports, library and documented draw.io subset are usable and nondestructive.
5. Explorer tabs, folder size analysis, read-only lock inspection and both silent recording formats are actually implemented and integrated; positive supported-platform checks exist, not just Unsupported/error-path tests.
6. Appropriate focused automated and mandatory native checks have completed successfully, with no known feature-related blocking defect. Evidence corresponds to the code under review. Environment gaps are not passed by inference.
7. Relevant prior behavior remains preserved, first-party task-related lint/review findings are resolved, and unrelated baseline limitations are documented without scope expansion.
8. All intended implementation/documentation changes are committed through the required checkpoint cadence. Every category's M0-A directly descends from the recorded prior tip; no worktree, auxiliary checkout, intermediate merge/rebase/cherry-pick or independent master fork was used.
9. Actual final HEAD is recorded as `final_tip_sha` in the final receipt/report and becomes the next Goal's required base. Push and clean/dirty status are reported truthfully; local-only completed commits are not falsely described as on GitHub.

When code is implemented and reviewed but mandatory native checks remain environment-blocked, final status is **Implemented — native verification pending**, with precise missing flows. When required implementation or correctness is missing, final status is **Partial / Blocked**. Do not call either state “Done” without qualification. A final user manual review is welcomed after delivery but does not retroactively replace required Codex verification.

## K. Final Delivery Report

Provide a concise factual report, organized around what the user can use rather than a log dump:

- Overall status and a seven-category table with implementation, focused tests, native verification, review and delivery state.
- Completed milestones/checkpoints and selected-feature coverage; any incomplete requirement identified by its original number.
- Important decisions: exact integer compatibility, separate inspector, shared session hosting, independent visual-authoring authority, draw.io supported subset, folder-size metric, read-only file-user semantics, and no-audio recording.
- Relevant files/modules changed, preserving the distinction between reused owners and newly added modules.
- Literal commands/counts/results for meaningful verification, build/profile identity, and sanitized screenshot/encoded-file evidence. List unrun steps separately with reasons.
- Each category's branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, significant checkpoint SHAs and `final_tip_sha`; actual pushed/local-only state and checkout status at the report timestamp.
- Final cumulative `final_tip_sha` after the last tracked commit, and the instruction: **“Start the next Goal on a fresh child branch directly from `<actual final_tip_sha>`, not from master/main. Use one checkout, no Git worktrees, and no intermediate branch merges.”**
- Confirmation that no worktrees/auxiliary clones/history rewrites were used; any existing unrelated checkout changes left untouched.
- Remaining limitations and the smallest necessary action for genuine blockers. Offer one compact end-of-task manual review checklist covering the user's most important visible flows, not an intermediate verification assignment.

Stop at the authorized boundary. Do not begin unselected enhancements, repair unrelated historical gates, or automatically integrate/merge the final branch.

## Reference Notes — Official Platform Documentation

These references support the platform constraints in E.2; repository source remains the authority for existing behavior. Verify availability against the repository's pinned bindings rather than assuming a current documentation example compiles unchanged. References were consulted while preparing this plan on November 4, 2026.

- **[W1] Microsoft — RmRegisterResources:** `https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmregisterresources`
- **[W2] Microsoft — RmGetList:** `https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist`
- **[W3] Microsoft — RmEndSession:** `https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmendsession`
- **[W4] Microsoft — IShellWindows:** `https://learn.microsoft.com/en-us/windows/win32/api/exdisp/nn-exdisp-ishellwindows`
- **[W5] Microsoft — UI Automation Control Patterns Overview:** `https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controlpatternsoverview`
- **[W6] Microsoft — SendInput:** `https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput`
- **[W7] Microsoft — Screen capture:** `https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture`
- **[W8] Microsoft — Tutorial: Using the Sink Writer to Encode Video:** `https://learn.microsoft.com/en-us/windows/win32/medfound/tutorial--using-the-sink-writer-to-encode-video`
- **[W9] Microsoft — MPEG-4 Support in Media Foundation:** `https://learn.microsoft.com/en-us/windows/win32/medfound/mpeg-4-support-in-media-foundation`
- **[W10] Microsoft — MFCreateMPEG4MediaSink:** `https://learn.microsoft.com/en-us/windows/win32/desktop/api/mfidl/nf-mfidl-mfcreatempeg4mediasink`
- **[W11] draw.io — Export your diagram to an XML file:** `https://www.drawio.com/docs/manual/export/export-to-xml/`
- **[W12] draw.io — Diagram XML generation / format reference:** `https://www.drawio.com/docs/reference/diagram-generation/`
