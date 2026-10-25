# CODEX GOAL — Capture Stability & Productivity Expansion

**Project:** Multi Launcher · Windows · Rust · eframe/egui 0.36 (patched `vendor/` crates)  
**Status:** Approved scope for autonomous Codex implementation. This is an **implementation contract**, not a request for additional discovery or feature brainstorming.  
**Planning evidence:** Latest user-provided source snapshot examined October 20, 2026 and the supplied screenshot showing the error `screen selection failed: Rectangle input shield monitor 1: topmost style missing`.  
**Code authority during execution:** Codex's **current branch, repository checkout, and `AGENTS.md`**, not a historic source revision. Confirm filenames/APIs against that checkout before editing.  
**Companion:** `CODEX_KICKSTART_CAPTURE_AND_PRODUCTIVITY_EXPANSION.md`.

## B0 — Mandatory new-branch bootstrap (FIRST; before M0 or feature work)

**This task must begin on a newly created, uniquely named Git feature branch.** The new branch and its first committed HEAD—not `master`, an older working branch, or a historical ZIP—are the **immutable execution baseline** for every subsequent M0–M11 checkpoint. This bootstrap is a specific user instruction and overrides older advice elsewhere in this Goal to remain on the original branch.

1. **Read-only safeguards first:** Read `AGENTS.md` and this Goal, then inspect `git status -sb`, `git branch --show-current`, `git rev-parse HEAD`, the tracked/staged diff and `git remote -v`. Record original branch and source commit. Before the new branch/baseline exists, make **no application-code edits**, test runs, plan implementation commits or agent write assignments.
2. **Select the safe base:** Ordinarily branch from the **current checkout's HEAD**, which is the code source of truth. If it is a clean checkout of the default `master`/`main` branch, `git fetch origin` and advance it with **fast-forward-only** synchronization when safely available before branching; never merge/rebase/reset to manufacture a clean base. If fetch is unavailable, base on the known local HEAD and record that remote freshness is unverified. If other uncommitted/staged user changes are present, **do not commit, stash, reset, or move them automatically**. Prefer a separate clean Git *worktree* created at the existing HEAD on the new branch while leaving the original worktree untouched; if even that cannot be done safely, report the exact Git safety blocker rather than modifying user work.
3. **Create a fresh idiomatically named branch:** Prefer **`feat/capture-productivity-expansion`**. If it exists locally or remotely, use a unique, descriptive suffixed name such as **`feat/capture-productivity-expansion-20261020-2`**; **never reuse or overwrite** an older feature branch. Switch/create this new branch (or its clean Git worktree) as the **first task-specific repository change after any permitted fast-forward-only default-branch sync**.
4. **Make the branch's baseline commit before feature implementation:** Initialize a compact, meaningful `docs/plans/capture_productivity_expansion_execution.md` ledger recording the source branch/SHA, new task branch, B0 status and pending M0–M11 checkpoints. Stage **only this new, Codex-owned ledger**, then commit it with **`chore(plan): [B0] establish capture productivity branch baseline`**. If an existing ledger makes a distinct tracked bootstrap change genuinely inappropriate, an explicitly labeled `git commit --allow-empty` baseline commit is permitted **for this requested first commit only**; never accidentally include previously staged user changes.
5. **Push this initial commit immediately:** Verify `origin` targets the intended repository, then try **`git push -u origin HEAD`** once, using the normal supported approval route (no force/approval bypass). If GitHub/Auto-review denies the push, preserve the local commit, record `push_denied`, and **continue implementation locally** per `AGENTS.md`; later verified checkpoints may try their normal single best-effort push. Do not repeatedly retry or turn permission setup into a blocker.
6. **Pin the baseline:** Record **`git rev-parse HEAD` after the B0 commit** as `BRANCH_BASELINE_SHA` in the M0 ledger update; all future diffs/verification/commits are relative to this same new branch baseline. Do not switch branches, resync `master`, merge, rebase or reset after B0 unless separately instructed. M0-A then inspects current code from this new baseline; M1–M11 commit only to the new branch.

**B0 acceptance:** New branch is checked out; a distinct first B0 commit exists; one upstream push was attempted with its true result recorded; the baseline SHA is known; original branch and unrelated user edits remain intact. **No feature code changes occur before B0.**

## 1. Desired result and approved scope

Address **both priority screenshot issues first**. Then deliver **all eight named improvements** in the following order, with functional acceptance, small checkpoint commits, and bounded tests. This is a **single long Codex assignment**, but each coherent phase must be independently understandable and committed. Do not replace the existing application or introduce workspace functionality.

| Order | Change | Required user experience | Milestone |
|---|---|---|---|
| **P0-1** | **Pinboard input-shield topmost failure** | Repeated static and live pin captures do not permanently fail with `Rectangle input shield monitor 1: topmost style missing`; invalid shield cannot leak underlying mouse input; recovery permits later capture | **M1** |
| **P0-2** | **Screenshot Editor compact slider row** | Zoom and Text Size sliders **side-by-side**, **above** Tool/Pen/Arrow/Rect/Highlight/Text row; small-window responsive behavior | **M2** |
| 1 | **Search all settings** | Search by setting name, synonym or plugin name; open, expand, scroll to, and focus relevant real Settings control or dedicated plugin settings dialog | **M3** |
| 2 | **Consolidated hotkey directory** | Search configured bindings by chord/action/owner; inspect conflicts and **actual** registration/availability; open editor at the owning feature | **M4** |
| 24 | **Searchable Paste Sequence source pickers** | Search `cs` snippets or clipboard history while adding a step; see selected entry's safe preview; add exactly the intended source | **M5** |
| 56 | **Adjustable pin opacity** | Independently fade a static or live pinned reference over another application without affecting the parent launcher or another pin | **M6** |
| 76 | **Richer MkMacro parameter forms** | Macro invocation can present file picker, folder picker or predefined-choice selector, using existing validated parameters | **M7** |
| 77 | **MkMacro clipboard input/output steps** | Explicit **Read Clipboard to Variable** and **Write Variable/Text to Clipboard** actions, with clear type/error behavior | **M8** |
| 90 | **Compact CSV/TSV table utility** | Open/paste text, inspect/filter/sort rows, choose/copy columns or rows, convert selection/table to JSON without opening a spreadsheet | **M9** |
| 105 | **Per-monitor brightness** | See named supported monitors, current level/status and independently set each display's brightness | **M10** |
| Closeout | **Integration, compatibility, final quality** | Feature-scoped checks, one full Nextest near end, documentation and commits, then final maintained-code Clippy | **M11** |

**No material product ambiguity remains.** Apply the safe, explicit defaults below. Source-dependent engineering alternatives (e.g. native window style APIs, widget registration ownership) are Codex decisions, not reasons to ask the user questions.

### Defaults selected without questionnaire

1. Keep current commands, hotkeys, Settings sections, plugin names, layouts and behavior unless the specific milestone changes them. New utilities use the same prefix-scoped discovery model and existing UI theme.
2. Settings search is **nonmutating navigation**: typing a term does not apply a setting. Search results identify the owning section/plugin and open/focus the relevant **real control**. If a setting is in a separate dialog, open that dialog and focus the control where supported. Never claim precise focus when only navigation to the owning section was achieved.
3. Hotkey directory is a **read-only inventory and navigation surface**, not another binding registration system. “Registered” means acknowledged by runtime, not merely configured in JSON. Unavailable/unsupported/disabled entries are explicitly labeled.
4. Snippet picker preview protects `hide_contents`; masked snippets show alias, tags, existence and a hidden-content indication, **not plaintext**. Clipboard previews are explicit, user-initiated; don't log/store private contents or leave the picker showing secrets after close. No search index persists clipboard text.
5. Per-pin opacity: **100% default**, adjustable approximately **10–100%** in the existing pin controls and Pin List, per pin, session-only. Actual OS composition must make the background app visible through the pin **window/image**, not merely tint an image onto an opaque background. Keep existing Always on Top and Click-Through independent.
6. MkMacro chooser metadata: file/folder/choice are **presentation constraints** on current typed parameter values (normally String); keep old macro documents valid and ordinary string parameters unchanged. File/folder picker chooses existing items by default. Choices are authored, bounded lists of unique string values.
7. Clipboard MkMacro actions operate on **text**, not arbitrary image/file formats; they are explicit at execution time. Do not automatically restore previous clipboard text. Respect the existing macro cancellation mechanism and do not introduce UI prompts in unattended macros.
8. CSV/TSV utility is **read-only** for source files. UTF-8/UTF-8 BOM; quoted CSV and TSV fields, multiline CSV cells and escaped quotes; explicit bounded limits and parse diagnostics. No in-place source edits. Use `table` as a new command prefix unless occupied; `csv`/`tsv` aliases are optional only when conflict-free. JSON export is by explicit Copy/Save As.
9. Brightness changes require deliberate **Set** per selected display; unsupported monitors show unavailable/unsupported and are not silently altered. Keep existing `bright` discovery and legacy `bright <0-100>` behavior intact. No automatic brightness schedule, ambient-sensor polling or machine-wide brightness daemon.
10. Use one long plan with many small coherent commits, but **no workspaces features**, unrelated Rust redesign, speculative optimization track or repeated broad validation campaign.

## 2. Source-derived architecture and non-regression boundaries

The following **actual owner paths** were observed in the supplied code and should be verified in the current checkout. They are not invitations to refactor neighboring subsystems.

| Area | Confirmed existing owners / behavior | Preserve |
|---|---|---|
| Shield error | `src/gui/mkmacro_dialog/visual_overlay_windows.rs`: `ShieldProbe::failures` checks `WS_EX_LAYERED`, `WS_EX_NOACTIVATE`, **`WS_EX_TOPMOST`** and absence of `WS_EX_TRANSPARENT`; `show_input_shields` calls `CreateWindowExW`, `SetLayeredWindowAttributes`, `SetWindowPos(HWND_TOPMOST,...)`, followed by coverage verification. `show()`, `verify_shield_coverage`, cleanup and display-change repair own the lifecycle. | **Do not remove the style/coverage guard** or make transparent shields acceptable. Underlying clicks must remain blocked during rectangle selection. Failures must release input/capture and permit new attempts. |
| Static/live Pinboard | `src/gui/pinboard.rs` (static/live lifecycle, capture identity/generation, selection, root parking and restoration); `src/gui/pinboard_windows.rs` (pin-owned deferred child viewports, hide/restore ack, topmost/click-through, live frame lifecycle). Both source variants use shared rectangle picker. | Separate operation IDs, multiple pins, follow-up capture, live lifecycle, keyboard recovery, user-window input and parent focus. |
| Screenshot Editor | `src/gui/screenshot_editor.rs` UI currently renders two independent **full-width** `Slider`s `Zoom` (0.1..=4.0) and `Text Size` (6..=48.0) **before** `Tool/Pen/Arrow/Rect/Highlight/Text`; existing `screenshot_chrome_scaled_toolbar_controls_remain_reachable_without_editing_image` egui test. | Image edit state, zoom range, text size, markup tools, Save/Copy/Undo/Redo, crop, hit geometry and accessibility. |
| Settings and plugins | `src/settings_editor/render.rs`: `TopLevelSettingsSection`, `show_settings_section`, bounded scroll, plugin `plugin.settings_ui(ui, entry)` under dynamic `plugin_{name}` header; state/mapping/save in `src/settings_editor/{state,mapping,save}.rs`; plugin Settings own distinct dialogs. | Staged Save/Cancel, legacy settings, source-of-truth configuration, mouse/gesture and radial settings, existing Jump Mode focus and modal gating. |
| Hotkeys | `src/hotkey/validation.rs` `validate_hotkey_routes`, `hotkeys_can_cofire`; `src/hotkey/reconfiguration.rs` prepared/acknowledged transactions; `src/main.rs` listener/routes and Paste Sequence presentations; `src/mkmacro/hotkeys.rs`, `src/screen_draw/hotkeys.rs`, `src/paste_sequence.rs`, radial and mouse gesture stores. | One real hotkey owner per route, no duplicate registration, distinction between configured/pending/active/omitted/invalid, no leaked macro text. |
| Source pickers | `src/gui/paste_sequence_dialog.rs` has `ui.menu_button("Add Snippet")` with an unfiltered full list and `ui.menu_button("Add Clipboard Item")` showing generic `History item N`; saved snippets carry `hide_contents`, tags and aliases; clipboard entries are selected by reference/digest. | Sequence store/preflight, selected history identity, no plaintext persistence of history contents, app input focus and 3 playback modes. |
| Pin windows | `src/pinboard.rs` `Pin` owns zoom, bounds, topmost, click-through and in-memory static/live content (no opacity currently). `src/gui/pinboard_windows.rs`: `PinWindowState`, `PinControlChange`, `PinListEntry`, native owned HWND/viewport, static/live texture presenter, child toolbar and Pin List controls. | Handle/lifetime validation, DPI, multiple pins, independent root/pin topmost, click-through recovery, copy/save pixels unchanged. |
| MkMacro params | `src/mkmacro/model.rs`: `MkMacroSignature`, `MkMacroParameter { id, name, value_type, description, default_value }`; `src/mkmacro/variables.rs`: `MkValueType` String/Number/Boolean/Point and `MkValueSource`; `src/gui/mkmacro_dialog/{signature_editor,parameter_prompt,typed_value}.rs`, `src/mkmacro/{invocation,invocation_prompt,reusable_validation}.rs`. | Stable IDs, pre-run parameter broker, typed validation, CallMacro/Return binding, persisted legacy macro JSON, runtime cancellation. |
| MkMacro actions | `src/mkmacro/model.rs` `MkAction` includes SetVariable, UnsetVariable, PromptInput, screenshot clipboard destination; `src/gui/mkmacro_dialog/{action_catalog,action_editor}.rs`; `src/mkmacro/{executor,runtime,validation,authoring_fields}.rs`. | Existing recording/replay, variable typing, failure policy, action serialization and unattended capability checks; screenshot destination is not a replacement for explicit text clipboard steps. |
| CSV/TSV | No dedicated CSV/TSV utility found in `src/` in this source snapshot; existing typed plugin/parser/bus/dialog conventions in `src/plugins/mod.rs`, `src/commands/{model,parser,handlers}`, `src/gui/mod.rs`; table/scroll patterns in File Search, JSON, Regex and Diff. | Existing CSV-like unrelated uses, JSON formatter, native file search, clipboard behavior; no new spreadsheet engine. |
| Brightness | `src/plugins/brightness.rs` prefix `bright`, command `brightness:dialog` or `brightness:set:<level>`; `src/gui/brightness_dialog.rs` single slider; `src/launcher.rs` and `src/dashboard/data_cache.rs` use Windows DDC/CI `GetPhysicalMonitorsFromHMONITOR`, `Get/SetMonitorBrightness`, `DestroyPhysicalMonitors`. | Legacy brightness commands/dashboard behavior, handle cleanup, unsupported display behavior, no busy synchronous per-frame device scans. |
| Tests/agents | `AGENTS.md`, `.codex/config.toml` and `.codex/agents/{planner,implementer,reviewer}.toml`; `cargo nextest`, native/eGUI fixtures and existing QA hooks in project. | Current orchestrator rules and active checkpoint cadence; no unrelated baseline rewrite. |

### Technical risks to treat explicitly

- **P0 shield failure is a real user-facing error, not a theoretical test gap.** Avoid attributing it to just one Win32 call before tracing actual observed ex-style/z-order state on the second or later capture. A validation probe may reject a genuinely unsafe window; a fix must *establish* valid topmost hit-testable coverage rather than suppress the message.
- Windows monitor DDC/CI handles are temporary resources; each `PHYSICAL_MONITOR` acquired must be destroyed via `DestroyPhysicalMonitors`, including failure paths. No monitor handle should survive a re-enumeration or a display-disconnect event.
- A child pin window with an opaque background is not made visibly translucent merely by multiplying texture RGBA values. Confirm real native/viewport composition with focus and click-through unaffected.
- Hotkey diagnostics cannot be constructed solely by reading `settings.json`: need to correlate configured documents with **runtime registration acknowledgements**, disabled plugins and unsupported routes. “Unknown” is honest if acknowledgment cannot be proven.
- Macro parameter picker inputs must remain **typed String values** to the existing execution engine unless an explicit migration is justified. No breaking reinterpretation of existing `MkValueType` discriminants.
- CSV/TSV must not turn an internal parse result into a large per-frame recomputation. Sorting/filtering should be over one parsed snapshot with bounded visible rows.

## 3. Codex orchestration, commits, and validation — mandatory

### 3.1 Agents and autonomy

- Use the **real named sub-agent runners** from `.codex/agents/`: `planner` (read-only GPT-6.1 Sol High), `implementer` (GPT-6 Luna Max sole workspace writer), `reviewer` (read-only GPT-6.1 Sol High). The orchestrator supplies bounded milestone packets; **one writer at a time**, no child rewriting the whole plan. Use the current runner definitions if they change.
- The parent coordinates Git and state. **First execute mandatory B0**: create a fresh `feat/capture-productivity-expansion`-style branch, make the initial baseline commit, attempt the upstream push and record the baseline SHA **before any feature changes or implementer handoff**. After B0, keep all M0–M11 work on that branch. Do not switch, reset, merge, rebase or force-push afterward; preserve the original branch and any unrelated user edits.
- **Code first.** M0 is a short source-owner/read-only inventory, not a new exploratory architecture plan. Implement M1 and M2 before work on any of the eight utility features. Continue autonomously through M11 without asking the user for manual builds, approvals, screenshots or milestones.
- Small **Active Checkpoint Commit Cadence:** labels `[M1-A]`, `[M1-B]`, etc. Commit verified coherent changes, include short scope summaries, inspect changed diff and `git diff --check`, and avoid meaningless WIP commits. Attempt one best-effort non-force push after each commit if configured; failed pushes leave local commits and do **not** stop work.
- Continue the **B0-created** execution ledger `docs/plans/capture_productivity_expansion_execution.md` for source branch/SHA, new branch, first committed `BRANCH_BASELINE_SHA`, initial push outcome, checkpoint status, hashes, selected tests, native proof and blockers. One update per checkpoint, not minute-by-minute logs. Preserve any unrelated user changes and avoid formatting vendor crates.

### 3.2 Testing, screenshots and no-spiral rules

- **M0–M10:** at most **two narrowly focused test slices per checkpoint**, one typically sufficient. Prefer existing domain/egui/native reducer fixtures; add only the minimal regression missing for the new contract. Compile a relevant target only when necessary. **No full Nextest or Clippy mid-plan**, and no perf/benchmark campaign.
- For each P0 fix, directly verify the **actual reported issue**: for Pinboard, repeated static and live operations with topmost style/cleanup; for screenshot sliders, real egui layout under ordinary and constrained widths. Use **at most one controlled native owned-GUI smoke for each P0 fix** where the environment permits, in isolation; do not touch user documents/windows. Since repeated pin captures are involved, that *one session* may execute several consecutive safe captures.
- For larger new utility milestones, at most **one owned native UI smoke per milestone**, 1–3 non-sensitive screenshots, using disposable sources and respecting native device support. If GUI control is blocked by environment approval: **one diagnostic + one retry**, mark `not_run_environment` and use deterministic proof. No generic screenshot driver or repeated real-desktop retry loop.
- After M1–M10, M11-A performs a small cross-feature seam check, M11-B runs **one full `cargo nextest run --workspace` near the end** (at most one corrected full rerun after a concrete fix); M11-C finishes all formatting, docs, commits and ledger, then **M11-D alone runs Clippy** for maintained crates. Ignore vendor warnings and do not edit vendored source to satisfy lints. Final successful Clippy is the last automated validation command before reporting.
- **Budget on repeated failure:** two substantive corrections at most for the same unchanged symptom, then one exact focused recheck per correction. If still blocked, retain coherent work, record exact reason, continue only isolated independent stages. Do not falsify native evidence or delete important tests to make a stage appear complete.
- For screenshot safety, unsupported monitor hardware or malformed CSV, surface honest errors. No automatic overwrite of user files, root directories or clipboard until the user activates explicit Copy/Save/Set/Apply action.

### 3.3 Acceptance vocabulary

- A `verified pin shield` means hit-testable, layered, nonactivating, **actually topmost** across all physical monitor bounds, not merely a window whose creation or `SetWindowPos` returned success. Underlying apps cannot receive selection Down/drag/Up.
- An `effective hotkey` means a configured candidate **with a current acknowledged runtime route**. Missing/pending/failed acknowledgments may be shown as `Unknown`, `Pending` or `Inactive`, never as active by assumption.
- `Settings search result` navigates to one real existing control or its dedicated plugin dialog. It does not change a setting, toggle a checkbox, or trigger an unrelated action on hint completion.
- `safe preview` of a snippet/history entry is user-initiated and ephemeral. `hidden` always means its content is concealed; never leak through tooltip, log, search index or status.
- `independent pin opacity` changes the target pin's actual composited visibility on screen while leaving native input owner, click-through status and other pin windows unchanged.
- `MkMacro parameter file/folder/choice` yields an ordinary validated runtime value; the UI representation is additive metadata. Cancel never starts/continues a partial execution.

---

## M0 — Establish current ownership and lightweight ledger

### M0-A — Confirm current state and source owners

**Objective:** Verify the Rust/egui project and directly relevant source/test paths **from the new B0 branch baseline**.  
**Tasks:** Confirm the B0 branch, initial commit SHA and upstream-push status in the already-created ledger; inspect `AGENTS.md`, Cargo config, P0 overlay owner, screenshot editor, Settings, hotkeys, Paste sources, Pinboard, MkMacro and Brightness. Locate existing tests for topmost shield and screenshot layout; confirm no dedicated CSV utility. Document only necessary dependencies and current baseline; no exploratory full test/Clippy campaign.  
**Verification:** Read-only inspection, optionally `cargo metadata --no-deps`.  
**Acceptance:** B0 has a recorded baseline SHA and actual push outcome, plus a short owner table and selected test filters.  
**Commit:** `docs(plan): [M0-A] record source owners from branch baseline` only if M0 substantively updates the B0 ledger; **the mandatory B0 baseline commit was already made**.

---

## M1 — P0: Fix repeated Pinboard shield “topmost style missing”

**Goal:** After multiple **static and live Pinboard** captures, the picker continues operating, or reports a precise recoverable hardware/system error. No capture remains permanently wedged and no rectangle mouse transaction reaches an underlying app. Do not fix this by removing the style requirement or setting all overlays globally topmost.

### M1-A — Reproduce and locate native shield demotion

**Owner:** `src/gui/mkmacro_dialog/visual_overlay_windows.rs`, shared rectangle visual controller, `src/gui/pinboard.rs`, `src/gui/pinboard_windows.rs`, and existing overlay native reducer/guard tests.  
**Implementation tasks:**
1. Trace `Capture Screenshot Pin` **and Live Pin** through root/child parking, display enumeration, `show_input_shields`, `probe_shield`, `verify_shield_coverage`, `show`, `poll_input`, terminal cleanup, `display_change` shield repair, and the next operation's creation.
2. Capture **payload-free, bounded** diagnostic evidence at the actual failing acknowledgement (`operation_id`, `monitor index`, stage, expected/actual RECT, raw exstyle, `GetWindowLongPtrW`/`SetWindowPos` error if any, state of show/creation/close, thread/window DPI context and HWND lifecycle). Use existing logging only on failures, no per-frame spam or personal monitor/capture content.
3. Determine **why `WS_EX_TOPMOST` is absent** despite the existing `SetWindowPos(HWND_TOPMOST, ...)`: initial creation style, later `ShowWindow`, Win32 z-order demotion, owner/child window changes during repeated pin hiding/restoring, stale shield reuse or unsafe timing. Treat these as candidates, not asserted causes.
4. Add focused deterministic regression against the existing shield probe/reducer seam representing a valid first capture followed by a later shield without the required topmost bit. The failing state must be observable and must not be incorrectly marked safe.

**Acceptance:** One specific code ownership/root-cause finding or precisely bounded remaining uncertainty, with failing-state fixture; no weakening of the probe.  
**Verification:** One focused native shield-plan/probe/lifecycle test slice.  
**Commit:** `test(pinboard): [M1-A] capture repeated shield topmost regression` only if code/test changes are coherent.

### M1-B — Restore real topmost coverage and fail safely

**Implementation tasks:**
1. Correct actual shield creation/z-order ownership at the narrowest Win32 boundary. If appropriate, use `WS_EX_TOPMOST` at creation **and/or** checked `SetWindowPos(HWND_TOPMOST, ...)` reassertion after any operation that can demote the shield; only count coverage when a subsequent `probe_shield()` confirms the current state.
2. On transient topmost loss, allow **one bounded recovery/acknowledgement attempt** before terminal failure. Do **not** spin/recreate unlimited HWNDs, “retry every frame,” force focus, or skip the style check. Do not make the input shield mouse transparent as a workaround.
3. Ensure all physical monitor rectangles—including negative origins, mixed DPI and secondary monitors—retain hit-testable coverage throughout selection. Keep native Down/drag/Up shield ownership, held startup click release, Escape and display-change handling.
4. On *any* failure after partial shield creation, release capture/held input safely, destroy owned shield/visual HWNDs, signal lifecycle terminal once, restore previously hidden pins/root as appropriate, and permit a **new** capture operation without restarting the launcher.
5. Preserve independent topmost/click-through state for all already-open static/live pin windows. Do not toggle them globally as an expedient way to keep shields topmost.

**Acceptance:** Native acknowledgments mean actual coverage, no permanent failure loop, no orphan shield or `CaptureInProgress` after terminal error.  
**Verification:** One focused overlay+pinboard lifecycle slice including repeated success/failure/restart and cleanup; native smoke if available in M1-C.  
**Commit:** `fix(pinboard): [M1-B] maintain topmost rectangle shield coverage`.

### M1-C — Repeated static/live capture acceptance and recovery

**Implementation tasks:**
1. Extend existing fake/owned tests for `static → static → live → live → static` selection cycles with at least one simulated topmost demotion, failure cleanup and subsequent recovery. Assert no cross-operation operation ID/generation confusion and no coverless window presented as ready.
2. If an interactive **owned** Windows desktop is available, perform **one** bounded smoke session using a disposable target with selection-sensitive dummy text/button controls. Execute multiple captures; prove underlying target did not receive selection Down/Up, correct pin(s) created, and next ordinary click reaches target after cleanup. One to three screenshots at most. Avoid collecting personal desktop content.
3. If Win32 desktop/UI access is denied, use tests and record `not_run_environment` once. Do not change safety checks to make an inaccessible desktop pass or ask the user for an intermediate manual review.
4. Verify screenshot editor and shared MkMacro selection path weren't altered except a shared shield lifecycle correction actually required to keep capture safe.

**Acceptance:** No `topmost style missing` wedge on repeated supported captures; after a transient failure, another capture can start safely.  
**Verification:** Focused pinboard+overlay tests and optional one owned smoke.  
**Commit:** `test(pinboard): [M1-C] verify repeated capture and recovery` if substantive test changes.

---

## M2 — P0: Screenshot Editor Zoom and Text Size side by side

### M2-A — Rearrange only the two sliders

**Owner:** `src/gui/screenshot_editor.rs` `ScreenshotEditor::ui`.  
**Implementation tasks:**
1. Replace the two independent full-width rows for `Zoom` (0.1–4.0) and `Text Size` (6–48) with **one compact horizontal row containing two distinct labeled sliders**. Put this row directly **above** the existing `Tool` buttons row, after Save/Copy controls.
2. Split available width proportionally so both labels, thumb, value editor and keyboard focus are accessible at normal dialog width; on an actually narrow dialog, allow bounded responsive wrapping rather than clipping. Do not hardcode resolution-dependent pixel positions or widen the dialog involuntarily.
3. Preserve actual slider ranges/values, persistence within editor, keyboard access, tool button state, image zoom, text annotations, crop behavior and Save/Copy/Undo/Redo. Do not add a preference, change screenshot capture or repaint pipeline.

**Verification:** Adapt existing `screenshot_chrome_scaled_toolbar_controls_remain_reachable_without_editing_image` using actual egui rectangles: at normal width slider centers share a vertical band and each has positive width, both above Pen/Tool row; at small width all controls remain reachable without text/image mutation.  
**Acceptance:** Correct compact appearance and no control regression.  
**Commit:** `style(screenshot): [M2-A] align Zoom and Text Size above tools`.

### M2-B — Compact editor evidence and closure

**Tasks:** Run one directly focused screenshot-toolbar UI test at compact and normal widths, verify no editor state changes before a user slider interaction; optional one owned screenshot editor UI capture. Preserve the Screenshot Editor for normal `ss` screenshot commands.  
**Verification:** One focused screenshot editor test filter; a single owned screenshot if available.  
**Commit:** `test(screenshot): [M2-B] protect responsive slider layout` only if a meaningful test/doco change.  
**Acceptance:** Compact sliders coexist with the tool buttons and remain keyboard/mouse usable.

---
## M3 — Feature 1: Search all Settings, including plugins

**Goal:** One fast search field inside the existing Settings dialog helps users locate actual settings, including controls inside plugin settings and dedicated plugin dialogs. Example queries: `font`, `gesture interval`, `paste`, `always on top`, `hotkey`, `mouse sensitivity`. **This is navigation, not a second settings model.**

### M3-A — Typed searchable settings destinations

**Owner:** `src/settings_editor/{render,state,mapping}.rs`, existing plugin settings registry/UI and focused Settings tests.  
**Tasks:**
1. Inventory current `TopLevelSettingsSection::{Hotkeys,PasteSequences,LauncherWindow,SearchResults,ActionsFeedback,Dashboard,Radial,Plugins}`, nested plugin sections and dedicated settings destinations (mouse gesture, screenshot, radial, notes etc.). Determine each section's **existing stable egui ID** and whether its widget controls can be focused after expand/scroll.
2. Add a lightweight typed `SettingsSearchDestination` descriptor or equivalent mapping: canonical owner path, stable semantic control ID, display label, synonyms/keywords and a navigation callback/intention using existing Settings methods. Do not store arbitrary UI closures or egui Responses between frames. Avoid using displayed labels alone as identity when there are duplicates.
3. Index **real, existing** user-configurable controls—not every string visible on a screen—including plugin-owned settings. For dynamic `plugin.settings_ui`, give each plugin a minimal explicit destination/keyword description or reuse supported registration metadata; do not introspect private plugin config by serializing its values or derive indexes from secret content.
4. Treat dedicated plugin settings dialogs as typed destinations. Search should open the owned dialog **and focus its actual control**, introducing a minimal control-focus adapter where necessary. A genuinely unsupported native control may navigate to its specific owner/section only with an explicit limitation; do not claim precise focus was achieved.
5. Make descriptors data-only, built from current enabled/available plugin inventory, with bounded cache invalidation on settings/plugin reload. Search must not loop through every plugin and allocate new strings at every egui frame.

**Tests:** Index duplicates, synonyms, dynamic plugin enable/disable and owner lifetime; results never contain settings values or clipboard text.  
**Acceptance:** Search results can identify a unique owner/section/control; unavailable controls are not advertised as active.  
**Commit:** `feat(settings): [M3-A] index searchable settings destinations`.

### M3-B — Search UI, navigation and focus

**Tasks:**
1. Add a compact **Search settings…** field at the top of existing Settings dialog, before collapsible sections. Show filtered destination results with visible owning feature, setting label and relevant synonyms. Preserve current large scroll body and anchored Save/Cancel footer.
2. Query case-insensitively with trim and predictable token matching; optional fuzzy ranking may use the project's existing matcher. Empty query shows standard Settings without overlays; no hidden section disappears or loses a draft merely from searching.
3. Selecting one result expands its exact top-level section and nested plugin group, scrolls the current Settings body to the real control, and requests keyboard focus in the correct frame. Prevent scroll jumping on every repaint, particularly after the user begins editing the control. Support Enter to navigate and Escape to dismiss results without cancelling the entire Settings draft.
4. For settings in dedicated dialogs, open the existing dialog via its established action and hand focus to the actual control when possible, preserving the current Settings draft. Do not trigger a control's action merely by navigating. Preserve Jump Mode's Shift+Space ownership and existing default hotkeys.
5. When a result is stale due to plugin disable or setting removal, show a simple unavailable indication instead of navigating to a different control sharing its label. No automatic setting persistence.

**Tests:** Search `font`, `gesture interval`, `paste` and `always on top` against fixture descriptors; verify section expansion, one-shot scroll, focus and no staged Save.  
**Commit:** `feat(settings): [M3-B] search and focus existing settings controls`.  
**Acceptance:** Users can jump from a search term into the actual relevant editor without mouse navigation or accidental modification.

### M3-C — Plugin-specific coverage and backwards compatibility

**Tasks:**
1. Complete destination descriptors for all **currently exposed** plugin settings sections. At minimum cover mouse gestures, Paste Sequences, radial settings, clipboard controls, brightness-related current dialogs and other existing plugin settings from the actual plugin registry; list any independently hosted control not safely focusable with its owning dialog as fallback.
2. Make searching and filtering independent of staging/Save/Cancel: user can type, inspect, edit and then Save/Cancel normally. Ensure no disabled plugin is silently activated by search and no search query gets persisted as a setting.
3. Use existing Settings Jump Mode scope to focus result rows and current controls; no parallel global keyboard handler. Verify plugin reload or a different topmost dialog invalidates stale jump focus.
4. Add concise Settings help and one safe owned UI screenshot showing filtered results if permitted. No large settings refactor.

**Tests:** Existing main Settings Save/Cancel and two plugin-owner focus cases (including unavailable plugin); optional one native safe UI session.  
**Commit:** `test(settings): [M3-C] verify cross-plugin settings navigation`.  
**Acceptance:** New search works across the dynamic plugin inventory and does not break settings persistence/focus.

---

## M4 — Feature 2: Consolidated hotkey directory

**Goal:** Provide a read-only, searchable **Hotkey Directory** listing owner, action, configured shortcut, actual active registration status, conflict information and a direct **Edit** destination. It must cover launcher/quit/help, radial, MkMacro, Paste Sequences, Screen Draw and other currently user-configurable global hotkeys/gesture bindings. **Not** a second shortcut manager.

### M4-A — Canonical inventory with honest runtime status

**Owner:** `src/hotkey/{validation,reconfiguration,launcher_invocation}.rs`, `src/main.rs`, existing live route/reconfiguration receipts, `src/mkmacro/hotkeys.rs`, `src/screen_draw/hotkeys.rs`, `src/paste_sequence.rs`, radial settings/inputs and owner-specific settings adapters.  
**Tasks:**
1. Introduce a compact read-only `HotkeyDirectoryEntry`: stable owner/action ID, human-readable owning plugin, actual configured chord (or Disabled/None), route kind (shared hook, polling, macro/radial-specific or local-only), runtime registration/availability status, conflict list and typed editing destination.
2. Gather configured candidates from the **existing** validated sources and active document snapshots. Do not parse user action labels to infer hotkeys; use typed stable IDs/generations and independent owners.
3. Reconcile with actual prepared/applied hotkey route acknowledgments and available published presentations. Distinguish `Active`, `Pending`, `Disabled`, `Invalid`, `Conflict`, `Unsupported`, `Omitted by precedence`, `Unavailable` and `Unknown`. **Never label a configured chord Active without matching acknowledged runtime ownership**; use Unknown for a source that currently exposes no reliable acknowledgment, with no extensive hook refactor.
4. Use `hotkeys_can_cofire`/owner-specific validators for collision analysis and explain shared primary-key/subset-modifier collisions as the application actually treats them. Preserve macro and radial specificity; a configured internal conflict is not necessarily a live double callback.
5. Publish/update an immutable, payload-free bounded directory snapshot after settings, macro document, radial bindings, plugin enablement or route generation changes; no disk watchers dedicated to this feature, no hotkey registration/polling added.

**Tests:** Fixture with overlapping End chords, missing/disabled macro and unacknowledged route; one changed setting updates inventory without registering a new shortcut.  
**Commit:** `feat(hotkeys): [M4-A] expose effective hotkey inventory`.  
**Acceptance:** Inventory is honest about live vs configured status and includes relevant owners without modifying their registrations.

### M4-B — Directory UI, search and edit navigation

**Tasks:**
1. Add **Hotkey Directory** as an entry from the existing main Settings → Hotkeys area and optionally a discoverable `hotkeys` (or conflict-free `keys`) launcher query/action. Use a resizable, scrollable table: **Shortcut | Action | Owner | Status | Conflict | Edit**.
2. Search/filter by chord, action name, owner and status, with useful examples; group or sort by owner and place conflicts/inactive items in a predictable view. No private macro bodies or custom shortcut payloads logged.
3. Clicking **Edit** navigates via typed destination to existing owner (main Settings section, MkMacro properties, Paste Sequence settings/editor, radial settings, Screen Draw, gesture settings), preserving that editor's staged Save/Cancel. If a binding is fixed/unsupported, say `Not configurable` rather than fabricate an edit affordance.
4. Show conflict details only if grounded in the validators/runtime route resolution. Opening directory or selecting a row does not activate/disable/rebind anything. New Jump Mode hints may focus rows/buttons but never execute edit/remove on hint completion.

**Tests:** User can search `End`, `paste`, `macro`, identify conflict and navigate to correct owner; unchanged stores/hook registrations before Edit; all rows stable after sort.  
**Commit:** `feat(hotkeys): [M4-B] add searchable hotkey directory`.  
**Acceptance:** One place to understand registrations and reach the original editor.

### M4-C — Cross-owner update and reload verification

**Tasks:**
1. Verify enabled/disabled plugin, current route generation, hotkey configuration update (Save/Cancel), a conflicting optional Paste binding and a reloaded MkMacro/radial document. Confirm directory state follows **accepted** runtime changes, not unpublished candidates.
2. Check a fixed shortcut without edit target, an unavailable device/legacy listener, duplicate labels and invalid input. Ensure query/scroll preserves selected row identity and no orphan listeners.
3. Add short Quick Help and README entry. Optional one owned desktop screenshot using generated harmless bindings.

**Tests:** One focused hotkey validation/directory integration slice plus one Settings linkage case.  
**Commit:** `test(hotkeys): [M4-C] validate cross-plugin route reporting`.  
**Acceptance:** No direct reconfiguration change, no false Active claim, complete appropriate owner coverage.

---

## M5 — Feature 24: Searchable Paste Sequence source pickers with previews

**Goal:** Replace the bare overflowing `Add Snippet` and `Add Clipboard Item` menus with small searchable, scrollable selectors that show an appropriate preview **before** a step is added. Preserve the existing sequence source snapshot/references, lifetime and hidden-content rules.

### M5-A — Reusable bounded source selection and privacy rules

**Owner:** `src/gui/paste_sequence_dialog.rs` (current `ui.menu_button("Add Snippet")` and `ui.menu_button("Add Clipboard Item")`), `src/plugins/snippets.rs`, current history snapshot and `Step::clipboard_history_reference`.  
**Tasks:**
1. Give each menu an inline expandable picker/popup or small owned modal with a **Search...** field, bounded visible list (scrollbar), selected item row and simple preview panel. Reuse existing button semantics, theme and keyboard focus; don't introduce a new global dialog framework.
2. Snippet search matches alias and tags, case-insensitively, **and visible snippet contents only when the snippet is not marked hidden**, without matching hidden snippet text. Show alias, tags, template field count and a masked preview where `hide_contents` is true. For nonhidden entries, show a truncated scrollable text preview only after explicitly selecting the row, not automatically for all entries.
3. Clipboard history uses the existing in-memory list snapshot/digest reference with a safe temporary selection identity. **Search must match entry text as well as available metadata** (case-insensitive) within the currently open ephemeral picker, so users can find a previously copied phrase; never persist/index that text, include it in hint IDs/logs, or expose it on an inactive screen. List rows may show a generic title and a safe truncated excerpt under the current visibility policy. **Preview is explicitly selected** and may show a bounded plaintext portion only inside the active picker; unsupported/nontext items stay labeled as such. Clear cached previews/query content on picker close.
4. Selection/preview must not modify the draft or clipboard. Provide **Insert selected** and Cancel, with unavailable/stale history entries clearly disabled. Preserve existing order and `Step` encoding; never duplicate snippet bodies into saved sequences.

**Tests:** 100+ fake snippets/history rows; duplicate alias labels, hidden-body masked even in search and preview, stale clipboard entry.  
**Commit:** `feat(paste): [M5-A] search and preview sequence step sources`.  
**Acceptance:** Both pickers are searchable and useful in a large collection, without new persistent private data.

### M5-B — Editor integration, keyboard flow and source consistency

**Tasks:**
1. Attach picker to current `Add Snippet`/`Add Clipboard Item` affordances. Opening/closing preserves existing draft/step scroller, selected row and Save/Cancel. `Enter` on a selected picker row may choose the row, but must still require the separate **Insert selected** action or an unambiguous explicit Enter-on-Insert to add the step; don't accidentally insert when focus first changes.
2. Revalidate snippet alias and clipboard digest/existence **at Insert**, and rely on existing preflight to revalidate at Run; when history changes between selection and Insert, show stale reason instead of adding the wrong entry.
3. Preserve Jump Mode keyboard ownership for nested picker, concealed snippets, prompt fields, long aliases, long lines, and 600×420 dialog layout. Only visible rows receive hints; Escape closes picker first, not the whole dialog.
4. Verify a mixed sequence of SnippetRef + history reference + temporary text saves/runs identically before/after picker change. No changes to Playback Controller, input hook, clipboard overwrite policy or sequence JSON format.

**Tests:** One focused Paste Sequence editor/add/reopen/select/Cancel test and one preflight/reference test; optional single owned UI screenshot of a safe fixture.  
**Commit:** `test(paste): [M5-B] preserve source identity and sequence behavior`.  
**Acceptance:** Correct source inserted exactly once; existing Paste-to-Advance/Automatic/Manual unchanged.

---
## M6 — Feature 56: Independent opacity for Screenshot Pins

**Goal:** A pinned static or live screenshot can be visually translucent above other apps while keeping its source resolution, zoom, topmost and click-through independent. No opacity leaks into the original screenshot saved/copied. New pins start fully opaque.

### M6-A — Per-pin opacity state and native-composition proof

**Owner:** `src/pinboard.rs` `Pin`, `PinWindowState`, `PinWindowRuntime`, existing owned native window API in `src/gui/pinboard_windows.rs`.  
**Tasks:**
1. Add `opacity_percent` or a finite normalized alpha field to the **per-pin ephemeral model**, default 100%, clamped/validated to 10–100%. Keep no global persisted setting and no change to `PinSnapshot` pixel buffers or live capture frames.
2. Carry desired opacity with existing pin window state/updates and stable `PinId` + lifetime token. If child viewport and native HWND binding cannot be proven current, do **not** apply opacity to an unverified window, root, another pin or a stale reused HWND.
3. Briefly prove a supported native Windows technique (e.g. validated layered-window constant-alpha composition) actually makes the **entire displayed pin window transparent to underlying applications**, rather than blending image pixels onto an opaque egui background. Respect existing eframe/GPU child viewport ownership; detect incompatible styles/lifetime or compositor behavior rather than claiming a fake opacity effect.
4. Choose a minimal seam through the existing pin-only `NativeWindowApi`/deferred viewport update path; no global window transparency change, no replacement screenshot renderer and no background alpha animation loop.

**Tests:** One isolated model bounds/default/identity test and a fake-native HWND opacity update test (correct target, stale target rejected, visible/topmost unaffected).  
**Commit:** `feat(pinboard): [M6-A] add per-pin opacity state and safe native binding`.  
**Acceptance:** Model and native boundary support independent opacity without changing original pixel data. If actual native composition is impossible on this host, record exact bounded proof failure; do not substitute a misleading image tint.

### M6-B — Existing pin toolbar / Pin List controls

**Tasks:**
1. Add a compact **Opacity** slider and percentage label to the existing pin child toolbar, with the same value exposed for recovering click-through pins from **Show Screenshot Pins / Pin List**. Reuse the existing pending control-change/acknowledged native control pipeline; never force a hidden child visible just to set opacity.
2. Adjustments update only the selected pin, immediately when its native owner is ready, and report actual success/failure. 100% maps to original fully opaque presentation; 10–99% shows the app underneath. Click-through remains a separate checkbox, and Always on Top remains a separate control.
3. Support static/live pins, multiple child windows, native window re-creation/restore following new capture, close, Escape and app shutdown. Preserve the desired per-pin value over window hide/restore; release layered style/handles when pin closes.
4. Preserve image copied/saved at original RGBA data regardless of display opacity. No persistent image mutation, new files or extra capture worker.

**Tests:** Existing pinboard independent controls/lifetime test extended to two pins with opacity 30/90, topmost/click-through toggles, close and restore.  
**Commit:** `feat(pinboard): [M6-B] expose opacity in pin controls and list`.  
**Acceptance:** User can use translucent reference or recover controls from the Pin List without impacting another pin.

### M6-C — Live/static native presentation and compatibility

**Tasks:** Run one focused test slice plus **at most one owned native visual session** with a generated solid-color fixture underneath two pin windows, checking 100% vs reduced opacity and that the underlying app remains interactive whenever click-through is deliberately enabled. Verify normal and live pin capture still work after M1 and don't regress topmost/click-through. If native capture/control is unavailable, record truthfully without creating a new cross-platform windowing layer.

**Verification:** Focused pin native control tests; optional one safe image of independently transparent pins; `git diff --check`.  
**Commit:** `test(pinboard): [M6-C] verify opacity without changing pin input`.  
**Acceptance:** Real compositing when native environment permits, state/lifetime safety otherwise, and no pixel/persistence regressions.

---

## M7 — Feature 76: Richer MkMacro parameter forms

**Goal:** An authored macro can declare a String parameter as **Text**, **File path**, **Folder path**, or **Choice**. Invocation then shows the corresponding native file/folder picker or list of allowed choices, while supplying the existing typed `MkValue::String` through the established validated macro parameter broker. Existing Number/Boolean/Point inputs remain unchanged.

### M7-A — Backward-compatible parameter presentation metadata

**Owner:** `src/mkmacro/model.rs` `MkMacroParameter`, `src/mkmacro/invocation.rs`, `reusable_validation.rs`, stable signature IDs.  
**Tasks:**
1. Introduce a small `MkParameterInputKind` (or equivalent `#[serde(default)]` metadata) for Text/File/Folder/Choice attached to **String** signature parameters; default Text for all legacy macro JSON. Choice kind carries a bounded vector of **distinct, nonempty** display/value strings with stable order; do not add arbitrary expressions or executable option sources.
2. Validate compatibility: nonstring `MkValueType` cannot be silently assigned a File/Folder/Choice presentation; changing a parameter's type to nonstring either resets the incompatible presentation explicitly in editor draft or shows a diagnostic requiring user correction. All old macros decode unchanged and aren't rewritten on read.
3. Keep runtime `MkValue`, `MkValueType` and `MkValueSource` semantics intact. Parameter ID stays authoritative across reorder/rename; existing CallMacro bindings and defaults migrate harmlessly. Empty/invalid choice set or default not among options produces a clear authoring/invocation error rather than auto-substituting an unapproved value.
4. Define file/folder selection as **text path** populated by an ordinary current-user file/folder chooser; picker may choose existing path, and validation rejects required blank input. Do not globally require files exist at macro document load (paths may be portable/temporary); any action consuming a path retains its own operation validation.

**Tests:** Legacy macro JSON, new metadata round-trip, wrong type/invalid options/default rejection, ID-stable CallMacro references.  
**Commit:** `feat(mkmacro): [M7-A] add typed parameter UI metadata`.  
**Acceptance:** Old macros and normal strings operate exactly as before; new forms have deterministic valid config.

### M7-B — Authoring UI for parameter kinds and options

**Owner:** `src/gui/mkmacro_dialog/signature_editor.rs` (currently Name, Type, Description, Default); related authoring diagnostics/mutations.  
**Tasks:**
1. When a parameter's `MkValueType` is String, expose **Input control: Text / File picker / Folder picker / Choice list**. For Choice, show Add/Remove/Reorder option editor in a bounded, scrollable area, empty/duplicate error messages, optional default selector.
2. Keep this in the **existing Signature Editor**; respect stable parameter IDs, Add/Delete/Move, Show/Hide properties and MkMacro's Save/Cancel/dirty-draft rules. Do not introduce a separate macro-form plugin or fork signature definitions.
3. Ensure change to a parameter type, input kind, option text or default correctly marks authored draft dirty and routes through established validation/save; no auto-save while navigating via Jump Mode. Provide concise inline help explaining that File/Folder/Choice still feed a String at runtime.
4. When old macros load, render unchanged Text inputs; when new metadata loads, show actual settings/options. Preserve hidden/reveal and Call Macro property displays.

**Tests:** Existing signature editor test fixture extended for changing text→choice, add/reorder and invalid default; Save/Cancel preserves original bytes on cancel.  
**Commit:** `feat(mkmacro): [M7-B] author file folder and choice parameters`.  
**Acceptance:** Author can configure rich input without changing existing macro runtime actions.

### M7-C — Use real chooser controls in invocation prompt

**Owner:** `src/gui/mkmacro_dialog/parameter_prompt.rs`, `typed_value.rs`, `src/mkmacro/invocation_prompt.rs`, `prepare_parameters`.  
**Tasks:**
1. The existing **Macro parameters** child viewport renders TextEdit for String as before, File/Folder path TextEdit with an explicit **Browse…** button and selected path (use already installed `rfd`/native dialog helpers), or a ComboBox/selector for Choice with defined options. Preserve Name, Description, Required/Default guidance, Cancel/Run and focus ownership.
2. No picker is opened until a separate explicit click; choosing a path does not launch the macro. Declining a native picker keeps the prompt/draft open. If chooser is unavailable, keep an accessible text field and an honest error, not a hanging modal.
3. Validate required input and configured choices **at submission against current macro signature revision**. For CallMacro or direct typed invocation passing an invalid choice, report a clear failure; for legacy plain String parameters don't apply new restrictions. Reject stale updated signature/choice lists rather than silently running a different meaning.
4. Continue using existing async InvocationPromptBroker and child-window ownership; opening a system file picker must not cause root input to leak into another app or cancel/restart an active macro. Do not add background shell prompts.

**Tests:** Fake prompt with file/folder path and choice values; cancel/submit; choices changed during pending prompt; existing simple macro strings still accepted.  
**Commit:** `feat(mkmacro): [M7-C] render richer invocation parameter controls`.  
**Acceptance:** Macro users get usable parameter forms, with normal type-safe Run/Cancel.

### M7-D — Macro integration and minimal UI evidence

**Tasks:** Validate actual parameter resolution through `prepare_parameters`, CallMacro and nested macro execution, per-invocation choices/defaults, and UI not silently applying paths. Check edit/reopen, invalid pickers, missing paths and safe error reporting. Use focused tests and one owned UI fixture only if accessible, with disposable files/folders; never navigate user Documents automatically.

**Commit:** `test(mkmacro): [M7-D] protect typed parameters and prompts`.  
**Acceptance:** Existing Macro Creator features, hotkeys, recording and execution remain intact; old JSON remains readable.

---

## M8 — Feature 77: Explicit MkMacro clipboard input and output steps

**Goal:** In MkMacro's action catalog, users can intentionally choose **Read Clipboard (Text) → Variable** or **Write Clipboard (Text/Variable)**. Both are explicit text actions executed in order, not an extension of Paste Sequence and not a new clipboard-history watcher.

### M8-A — Add two typed action variants and validation

**Owner:** `src/mkmacro/model.rs` `MkAction`, `variables.rs`, `validation.rs`, `authoring_fields.rs`, `authoring_analysis.rs`, `reusable_validation.rs`.  
**Tasks:**
1. Define `ReadClipboardText { variable: String }` and `WriteClipboardText { source: MkValueSource }` or idiomatic equivalent; serialize via current MkAction tagged enum while keeping old documents/unknown action handling stable. Output read must target a writable variable name, not a built-in read-only key.
2. Write source is either a literal String or resolved String variable. Reject invalid/missing/nonstring variables with clear step ID context; do not coerce Point/Boolean/Number to string silently. Non-text clipboard formats cause a controlled read failure, not a panic and not a deletion of clipboard contents.
3. Plug into the existing macro action capability/validation matrix (including unattended/Hot Folder restrictions). Only permit clipboard-mutating steps in background automation where current capability policy explicitly authorizes them; otherwise mark them unavailable for unattended invocation, with an explicit reason.
4. No new loop/condition/goto semantics; preserve macro row IDs and last-action status propagation.

**Tests:** serde round-trip, invalid variable/type, missing clipboard text, explicit source resolution, disabled/unattended capability policy.  
**Commit:** `feat(mkmacro): [M8-A] model explicit clipboard variable actions`.  
**Acceptance:** Both steps are distinct and discoverable in the model, legacy actions unmodified.

### M8-B — Runtime clipboard behavior and action editor

**Owner:** `src/mkmacro/executor.rs`, existing clipboard access adapter (`arboard` or Windows wrapper), `src/gui/mkmacro_dialog/{action_catalog,action_editor,mod}.rs`.  
**Tasks:**
1. Add action catalog entries named **Read Clipboard Text** and **Write Clipboard Text**, with searchable tags and parameter editors. Read editor requires a destination variable; Write editor offers a literal String or variable reference using existing `MkValueSource` selector.
2. Execute clipboard operations **when the macro reaches the step**, on the existing execution worker; don't read or write while rendering form, previewing, recording, authoring or validating. Use finite/bounded OS access and clear errors for unavailable/busy clipboard or non-text data. Avoid holding clipboard lock through the UI loop or blocking an unrelated recorder.
3. After a successful read, assign exact UTF-8 text to the selected runtime variable; after a successful write, publish exact string and report step success. Do **not** restore previous clipboard automatically, reformat text, auto-paste into a window or capture clipboard into unrelated macro logs.
4. Check run cancellation before attempting work and avoid continuing later macro steps after an actual error unless existing macro error policy directs it. Keep any clipboard watcher from creating false extra hotkeys or recursion; no new clipboard observer.
5. Enforce security/privacy: text never in `Debug`, routine logs, tooltips or screenshots; tests use fake adapter and harmless constant strings.

**Tests:** Existing executor fake backend: read assigns expected value, write publishes desired string, missing type/busy/nontext returns specific failure, no write on cancelled run.  
**Commit:** `feat(mkmacro): [M8-B] execute clipboard read and write steps`.  
**Acceptance:** Read → variable transformation → Write works, with no implicit OS paste.

### M8-C — Runtime integration, serialization and existing feature regression

**Tasks:** Test a simple composed macro `Read Clipboard → Set/transform Variable → Write Clipboard`; assure saved/reopened macro retains steps, variables are available to later steps and wrong type aborts with existing error policy. Verify screenshot-to-clipboard and PromptInput's optional copy remain unchanged. One owned test may exercise disposable in-memory clipboard adapter, but no changes to personal clipboard during native automation. Update MkMacro action reference/help.

**Verification:** One focused macro domain/call/clipboard integration slice; optional one tiny native test only if previously available and isolated.  
**Commit:** `test(mkmacro): [M8-C] verify explicit clipboard action workflow`.  
**Acceptance:** No regression to recording, Paste Sequence, clipboard history or existing MkMacro hotkeys.

---
## M9 — Feature 90: Small CSV/TSV table utility

**Goal:** Provide a fast, read-only utility for opening/pasting tabular text, previewing parsed rows, filtering/sorting, selecting columns/rows and copying a result or converting it to JSON. No spreadsheet editing, external application, hidden source write, formula execution or multi-workbook engine.

### M9-A — Bounded parser and tabular model

**Owner:** New small domain module under `src/` following existing `json_transform.rs`/`xml_toolkit` worker/session conventions; new module names may be chosen by implementer. `Cargo.toml` dependency changes only if a maintained existing parser cannot satisfy quoted fields.  
**Tasks:**
1. Support CSV (commas and RFC-4180-style quoted fields, escaped `""`, quoted embedded newlines), TSV (tabs, quoted text when present), optional first-row header toggle, and both LF/CRLF. Default delimiter Auto for pasted text, explicit CSV/TSV override. Expose parse error row/column/description; invalid encoding must be an error, not silent lossy replacement.
2. Bounded source: recommended **10 MiB decoded UTF-8**, maximum **50,000 rows / 128 columns** for this utility, plus a modest per-cell preview limit. Enforce before allocating a very large tree or rendering thousands of rows; clearly report `too large / too many columns / malformed CSV` rather than freeze. Accept UTF-8 BOM; reject binary/UTF-16 with a helpful message. Use `csv` crate if needed for correct quoting instead of an ad hoc incomplete parser; don't add a heavyweight spreadsheet framework.
3. Represent one authoritative immutable parse generation plus typed headers/rows, column count and diagnostics. For uneven rows choose deterministic `missing cells → empty` display with explicit warning; additional cells beyond declared header width are not silently discarded (show diagnostics/adjust schema policy). Distinguish no header vs duplicate/blank header labels; never silently lose JSON object keys.
4. Filtering: case-insensitive literal substring across current visible/selected columns (no regex dependency needed). Sorting: stable ascending/descending by selected column, numeric comparison when every populated value in that column parses as a finite number, otherwise case-insensitive text order; preserve source row identities across toggles and ties.
5. Provide read-only selection of one or more columns and optionally row subset. **Copy Selected Columns** emits tab-delimited output with necessary quoted/escaped values so it can be pasted reliably. **Convert to JSON** uses an array of objects when headers are enabled and valid/unique; otherwise use array-of-arrays with an explicit UI note. Respect current filtered/sorted/selected result set, never select hidden data silently.

**Tests:** Quoted commas, embedded newline, tabs, escaped quote, BOM, Unicode, duplicate headers, uneven rows, empty file, numeric/text sort, filter, multi-column selection, JSON output, bounds and error diagnostics.  
**Commit:** `feat(table): [M9-A] implement bounded CSV TSV parsing and projections`.  
**Acceptance:** Input parse/selection/export is deterministic, bounded, read-only and byte/text safe.

### M9-B — `table` command and resizable dialog

**Owner:** `src/plugins/mod.rs` and new small plugin, `src/commands/{model,parser,handlers}`, `src/gui/mod.rs`, and existing dialog/render patterns from File Search/JSON/Regex.  
**Tasks:**
1. Add **`table`** prefix command to open a resizable **Table Utility** dialog. If `table` conflicts with another real existing command in the current checkout, use an unambiguous prefix and document it; optionally provide `csv`/`tsv` aliases if safe, not at the expense of existing prefixes.
2. Provide **Paste Text**, **Open File…**, multiline source editor with CSV/TSV/Auto selection, Has Header checkbox, Parse/Refresh, Filter field, column headers with sorting, bounded virtualized row table, row/column selection, result count, **Copy Selected Columns**, **Copy JSON**, **Save JSON As…** and Cancel/Close. Keep existing style, compact spacing, anchored action controls and independent source/results scroll areas.
3. File-open defaults to read-only UTF-8 source and never auto-writes the input. Save JSON asks for an explicit destination and confirmation for overwrite under the established file-policy helper; prefer UTF-8 pretty output via `serde_json`. Clipboard changes only after explicit Copy.
4. Parsing/filter/sort work must be generation-coalesced and never block the egui render frame for large accepted datasets. Cache parsed rows and derived sort/filter index, virtualize visible table rows and limit rendered cell previews. Canceled/closed workers must not publish stale results on re-open.
5. Support existing keyboard-first and Jump Mode where the dialogs already have adapters; ensure query fields and result row selection are keyboard reachable. No new global hotkey, no calculator or full CSV transformation DSL.

**Tests:** Search/query command routing, parse/file import, selected columns JSON conversion, changing delimiter/header state, invalid source, closed/reopened dialog, no file writes before Save.  
**Commit:** `feat(table): [M9-B] add CSV TSV table dialog and commands`.  
**Acceptance:** Users can perform requested workflow without another spreadsheet application.

### M9-C — Table utility validation, clipboard safety and docs

**Tasks:** Use disposable fixture files and clipboard stub to prove open/paste → filter/sort → select/copy → JSON conversion/save. Compare exact JSON bytes after explicit Save; source file unchanged, even for malformed input. Test 10 MiB bound and one moderate large table for virtual-row rendering/no repeated parsing on idle frames. Optional one owned native screenshot of harmless fixture data; no personal files/clipboard. Add prefix and example commands to Quick Help/README.

**Verification:** One table engine test slice + one dialog/plugin slice, optional one native smoke.  
**Commit:** `test(table): [M9-C] verify filtered table and JSON export`.  
**Acceptance:** Efficient small table work with no hidden edits or unsafe clipboard modifications.

---

## M10 — Feature 105: Individual monitor brightness controls

**Goal:** Open `bright` and choose a **named physical display** to read/set its individual DDC/CI brightness, while keeping the existing global/legacy command and Dashboard behavior intact. Unsupported hardware is honestly marked as such.

### M10-A — Physical monitor inventory and scoped brightness service

**Owner:** `src/gui/brightness_dialog.rs`, a narrow Windows display abstraction reusing relevant helpers from `src/launcher.rs` and `src/dashboard/data_cache.rs`; related GDI/Display native bindings already present in Cargo.  
**Tasks:**
1. Enumerate logical `HMONITOR`s with `EnumDisplayMonitors`, resolve zero or more `PHYSICAL_MONITOR` entries via `GetNumberOfPhysicalMonitorsFromHMONITOR`/`GetPhysicalMonitorsFromHMONITOR`, and expose **stable-enough current-session IDs**, friendly names and per-display read capabilities. A single `HMONITOR` may map to several physical monitors; don't return only the first or stop enumerating after the first logical display.
2. Read each physical monitor's actual min/current/max via `GetMonitorBrightness` when supported; normalize current level into 0–100 for UI and convert requested 0–100% back into the monitor's **native min/max brightness range** for `SetMonitorBrightness`. Clamp/validate and handle zero range, stale/disconnected devices and DDC/CI failures. Do not default an unsupported monitor to a made-up 50% and present it as actual.
3. **Always release every acquired monitor handle** with `DestroyPhysicalMonitors` on success/error/cancel/re-enumeration. Keep no raw `PHYSICAL_MONITOR` across re-enumeration, thread/shutdown boundary or user-visible session after device changes; use a short-lived owned worker/lease to perform physical I/O.
4. Poll only on opening/explicit Refresh and after acknowledged Set—not every frame. Keep UI responsive if a monitor's DDC/CI call is slow using a single bounded/coalescing worker, honest Busy/Timed out/Unsupported statuses and no detached per-click thread explosion; shutdown must not strand handles or hang the launcher.
5. Preserve the historical `bright <number>` command and Dashboard brightness action **without silently changing which displays it targets**. New per-monitor operations use typed identity and an unambiguous command payload if exposed through launcher search.

**Tests:** Fake 1/2/3-monitor enumerator, multiple physical monitors per HMONITOR, nonzero min/max conversion, unsupported/stale/disconnected monitor, explicit handle destruction on error, concurrent refresh/set request ordering.  
**Commit:** `feat(brightness): [M10-A] enumerate individual physical monitors safely`.  
**Acceptance:** Truthful named monitor inventory and per-device control seam; no leaked native handles.

### M10-B — Named monitor list, controls and errors

**Owner:** Extend `src/gui/brightness_dialog.rs` and existing `bright` plugin/actions/command parser only where needed.  
**Tasks:**
1. Replace the one-value **Brightness** dialog body with a resizable, scrollable list showing **Monitor name / identifier**, support state, current percent (if known), a per-row slider/value field and **Set** button. If no controllable displays, show a clear empty state and explain the supported physical-monitor limitation.
2. Name rows using the physical description plus logical monitor index when necessary; distinguish duplicate friendly names. On Refresh, retain a selected row only if its physical identity still resolves in the fresh generation; otherwise show unavailable/removed.
3. Per-monitor Set is explicit and affects only that device. Show Pending/Success/Failure for each; don't pretend Set worked on unsupported hardware. Do not change all monitors when adjusting one. Preserve normal legacy `bright 50` behavior and available `brightness:dialog` opening action.
4. In Settings search/hotkey directory, expose the **Brightness dialog opening action** only if it already has such a config control; do not invent brightness shortcut settings. Preserve Dashboard system controls and existing process volume behavior.
5. Handle DPI/multimonitor coordinates and display arrangement changes; do not bind display identity to physical screen x/y alone. No permanent brightness service or new polling timer.

**Tests:** Fake monitor list UI with supported/unsupported/duplicate names, selection/Set identity, stale refresh refusal, numeric conversion, no cross-device Set.  
**Commit:** `feat(brightness): [M10-B] present independent monitor brightness controls`.  
**Acceptance:** Users can independently adjust each supported display and see honest status.

### M10-C — Windows hardware compatibility and bounded evidence

**Tasks:** Verify `bright` discovery, `brightness:dialog`, legacy `brightness:set:<n>` command, fake independent display Set and handle release. If a safe owned Windows device session is available, list current monitor capability **without changing any real user's brightness**; do not execute Set on real devices without a dedicated disposable/simulated monitor endpoint. Native Set is proven by backend fakes unless physical test hardware is explicitly owned. Report unsupported DDC/CI hardware as `not_supported_hardware`, not feature failure. Add README limitations that some laptop/internal/USB-C/remote displays may not support these APIs.

**Verification:** One scoped plugin+backend test selection, no real brightness mutation in autonomous tests.  
**Commit:** `test(brightness): [M10-C] verify scoped monitor control and legacy commands`.  
**Acceptance:** Feature safe and useful on supported hardware without disturbing any unowned display.

---

## M11 — Integration, documentation, full Nextest then final Clippy

### M11-A — Direct cross-feature interaction checks

**Tasks:**
1. Review each completed checkpoint against its own acceptance. Reuse passing tests rather than rerun every microtest; select **at most two** directly relevant cross-feature slices covering shared Settings/Hotkey ownership, Macro parameters + clipboard steps, Pinboard + screenshot editor, and `table`/`bright` plugin command routing.
2. Validate on isolated test data that Settings search only navigates, Hotkey Directory doesn't register bindings, macro parameter pickers don't run macros merely upon choosing a file, and Clipboard steps have explicit side effects only when executed.
3. Verify M1 safety fix still guarantees a real topmost input shield and that both static and live pins can be recaptured after success or error. If M1 is blocked due to device/desktop access, do not falsely mark it complete; retain deterministic proofs and exact status.
4. Check that no new background polling was added to disabled features, no per-frame IO of physical displays or CSV file, and no unowned Windows handles/worker threads survive app exit.

**Verification:** Up to two focused shared-owner test slices; only owned native screenshots already planned, no additional full smoke tour.  
**Commit:** `test(integration): [M11-A] protect capture and productivity boundaries` only for actual tests/fixes.

### M11-B — One final full Nextest run

**Tasks:**
1. After all feature implementation and focused checks, perform **one** `cargo nextest run --workspace` on the actual Windows host/toolchain (respect verified package/feature configuration and developer-only bins). Keep the output summarized, with real test totals, failures and duration.
2. If introduced functional regression fails, identify its first independent cause, repair a narrow owner and run its focused case before a **single** full rerun at most. Do not start a weeklong retry or benchmark campaign. Unrelated existing/environment-specific failures are documented separately without weak assertions or deletion.
3. A hardware/display-dependent native exercise excluded by safety/environment is not silently considered passed; report `not_run_environment` or `not_supported_hardware` separately.

**Verification:** **Exactly one scheduled full Nextest**, at most one corrected rerun.  
**Commit:** `fix(integration): [M11-B] repair final regression` only if actual code/tests changed.  
**Acceptance:** Honest final integrated test status without infinite validation.

### M11-C — UI evidence, docs, formatting and all non-lint commits

**Tasks:**
1. Collect or reuse **bounded owned** screenshots of the compact Screenshot Editor, Pinboard capture/opacity, Settings search, Hotkey Directory, source picker, macro parameters, table and brightness dialog when environment permits. Use existing safe screenshots; no user data. If GUI control is denied, one diagnostic and one retry then `not_run_environment`.
2. Update current Quick Help/README with actual `table`/`bright` routes, settings search, hotkey directory, source pickers, MkMacro forms/clipboard actions, per-pin opacity and monitor-support limitations. Preserve historical plans and unrelated docs.
3. Complete normal `rustfmt`/format checks on **touched maintained files only**, minimal compile check if necessary, `git diff --check`, cumulative diff review, actual commits and ledger closure. No vendor formatting, unrelated rewrite, feature expansion or new tests after closure.
4. Mark Clippy pending for M11-D and record exact existing baseline blockers if already known from source, but **do not run Clippy** before M11-D.

**Verification:** Doc/diff/format and evidence review; no extra full tests.  
**Commit:** `docs(productivity): [M11-C] finalize commands and feature evidence`.  
**Acceptance:** All non-lint changes committed and documented before the final lint phase.

### M11-D — Only and final maintained-code Clippy gate

**Tasks:**
1. Inspect actual maintained workspace members with `cargo metadata --no-deps`. Do not lint third-party vendored crates as independent targets; exclude any actual vendored workspace members explicitly by **verified Cargo package name**, not by guessing from directories. Path-vendored dependencies outside workspace membership are not independent app Clippy targets.
2. Run one maintained-crate Clippy sweep, normally:

```powershell
cargo clippy --workspace --all-targets -- -D warnings
```

3. Fix relevant maintained-code warnings in bounded grouped changes. Do not blanket-allow warnings, weaken tests or perform a general app refactor just to hide warnings. Existing unrelated baseline warnings should be identified and handled under the current user policy (prefer first-party clean when bounded; report true irreducible blockers without pretending the gate passed). Do not edit `vendor/` source merely for Clippy.
4. If a lint fix changes logic, perform one focused test and any required formatting **before** the final successful Clippy invocation; commit lint fixes using `chore(clippy): [M11-D] clean maintained code warnings` **before** that invocation. No code/test/fmt commands after the final successful lint; report immediately.
5. Same unchanged lint failure class permits at most two substantive fixes and tightly scoped reruns. If unresolved, document exact command/diagnostic rather than extending the project.

**Acceptance:** Final verified maintained-code lint status with correct vendor exclusions, followed by final report and stop.

---

## 4. Acceptance checklist and truthful reporting

- [ ] P0 Pinboard shields remain **actually topmost and hit-testable** across repeated static/live captures, or exact safe native/environment blocker reported; recovery does not require restart and underlying Down/Up remain blocked.
- [ ] Screenshot Editor Zoom and Text Size share one row above Tool buttons at normal size and remain reachable at narrow size.
- [ ] Settings search finds and navigates to relevant existing controls, including plugin-owned settings; no silent settings edits.
- [ ] Hotkey Directory lists configured/effective shortcuts with truthful status, conflicts, owner and edit destination; never registers a hotkey itself.
- [ ] Paste Sequence Snippet and Clipboard pickers search/preview bounded current data, conceal hidden snippets, and insert stable correct references.
- [ ] Two pins can have different composited opacity; original saved/copied images unmodified; topmost and click-through still work.
- [ ] Rich MkMacro form parameter types, defaults, choices and file/folder controls work with legacy macros.
- [ ] Explicit MkMacro clipboard text Read/Write action variants work through existing variable and runtime engine.
- [ ] CSV/TSV utility opens/pastes data, handles quoting, filters/sorts/virtualizes rows and copies/exports selected data as JSON without editing source.
- [ ] Brightness lists named supported physical monitors, sets selected monitor independently and honestly reports unsupported devices; legacy `bright` command preserved.
- [ ] **B0:** Fresh idiomatic Git branch created before implementation, initial baseline commit made, upstream push attempted, and new-branch baseline SHA recorded; original branch/worktree protected.
- [ ] Small reviewed checkpoint commits **all on the B0 branch**, best-effort pushes, only focused tests during milestones, one final full Nextest and **Clippy last**, with actual results.
- [ ] No personal data was modified during testing; no unrelated features or app-workspace functionality added.
- [ ] Final report provides **original branch/source SHA, newly created branch, B0 baseline commit SHA and initial push result**, checkpoint commit IDs, per-feature completion, test commands/results, genuine blockers, evidence paths, vendor exclusions and native/hardware limitations.

**Final instruction to Codex:** Read `AGENTS.md` and this entire Goal; **execute B0 first** to create, commit, attempt to push and pin a new idiomatic feature-branch baseline **before any implementation**. Then use configured `planner`/`implementer`/`reviewer` runners, implement **P0-1 and P0-2 first**, then all eight features in one long but bounded pass **on that branch**. Commit frequently, push best effort, do not ask the user to operate/verify anything mid-run, do not run Clippy until the final phase, and stop after the factual final handoff.
