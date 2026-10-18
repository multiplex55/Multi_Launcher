# Codex Goal — Action Execution Parity, Live Pinboard, and In-Note Find

## A. Goal / desired outcome

Implement **only** these three enhancements in Multi Launcher, without breaking existing behavior:

1. **Consistent action execution across interfaces.** The same eligible launcher action must resolve to the same operation, arguments, target, safety checks, and configured feature behavior whether invoked from launcher search/list, grid, radial menu, or another existing supported invocation route. Deliberate **surface presentation policies** must remain deliberate: do not replace radial `KeepOpen` / `CloseCurrentMenu` / `CloseTree` semantics with grid behavior, and do not override settings such as keeping the Paste Sequences dialog visible during playback.
2. **Optional live cropped-window pins.** Extend the current **static screenshot** pinboard with a separately invoked, **live, window-attached cropped reference**. The cropped content refreshes as its source application changes, including when other windows cover the source where Windows capture permits. Existing static pins must remain the default, unchanged path. Live pins keep the current session-only, independent-native-window concept, recoverability, and explicit copy/save controls.
3. **Find within an open Markdown note.** Provide an in-note `Ctrl+F` search with next/previous occurrence navigation. It operates on the currently open note, not the global Quick Notes list. It works with the note panel's supported **Edit**, **Preview**, and **Split** modes, without changing note content, save status, or mode just to perform a search.

User-visible success: equivalent actions behave predictably, a live reference pin shows a changing source window region without trapping the user in a screenshot editor, and an open note can be searched and navigated from the keyboard.

### Approved decisions / sensible defaults

These settle routine behavior without requesting another product review:

- **Action parity means equivalent effects, not identical launcher/radial visibility.** Explicit action settings and existing surface/session policies win over a generic hide/show rule. Do not introduce a new global visibility preference.
- **Live is opt-in** via a clearly named pinboard command, such as `pinboard live` resolving to `pinboard:live`. `pinboard capture` remains static; `pinboard list` remains intact. Live pins are attached to a source **window**, not to a fixed desktop rectangle that might later show another application. If the system cannot obtain the source window/crop unambiguously, fail visibly rather than silently using a screen-polling substitute.
- Use the existing rectangle-pick flow for selecting the initial crop. Resolve a unique external top-level source window beneath the selection after native selection/overlay teardown. Represent the crop relative to the source window's captured content, not the pin's current screen location. Reject ambiguous/multi-window or self-capture selections with a concise explanation. The live pin follows source-window movement; source resizing must not accidentally show neighboring windows.
- Start live refresh only after the first valid frame. Default to a **conservative bounded refresh cap of approximately 10 frames/second**; no new global configuration or continuous polling on the launcher UI thread. Provide per-pin **Pause/Resume**. Paused, temporarily unavailable, or closed sources show the most recent valid frame and an understandable status, not unrelated desktop pixels. Once a source closes, the pin remains as a frozen reference until the user closes it; do not follow a recycled HWND. A minimized/protected/unavailable source may pause with a status and recover when feasible; do not bypass OS capture protections.
- **Copy Image / Save As** for a live pin capture the latest *completed* frame as a still image at the instant of the command, including when paused. Static pins retain their present immutable snapshot semantics.
- In-note search defaults to **literal, case-insensitive** matching against the note's current Markdown source, with Unicode-safe original-source offsets. Empty query means no results. Show current/total match counts; **Enter** = next, **Shift+Enter** = previous, wrap around, and **Esc** closes only the Find strip. Search UI state is transient, per note panel, and not persisted. Preview/Split should navigate to the relevant rendered content and visibly identify the active match/containing segment; when a raw-Markdown-only occurrence has no rendered glyphs, communicate that explicitly and offer a non-destructive way to reveal it in Edit mode. Never silently switch modes, expand unrelated UI, or modify Markdown as a highlighting technique.

## B. Current codebase findings (grounded in the reviewed repository)

| Existing owner | Relevant facts and inspection targets |
| --- | --- |
| `src/gui/actions.rs` | `LauncherApp::activate_action` parses primary `Action` via `commands::parse_command`, handles destructive confirmation and dispatch, and calls `restore_for_new_launcher_interaction` based on before/after dialog/panel snapshots. |
| `src/commands/model.rs`, `src/commands/parser.rs`, `src/commands/bus.rs`, `src/commands/outcome.rs` | `ActivationSource`, `CommandInvocation`, typed `Command`, `CommandBus`, `CommandOutcome`, `VisibilityPolicy` and `HistoryPolicy` are already central contracts. Pinboard currently has `Capture`, `List`, and `Unknown`. |
| `src/gui/command_host.rs` | `dispatch_command_invocation[_with_history]`, `apply_command_outcome_with_root_policy`, and the `CommandHost` implementation own result, visibility, focus, usage/history, and error application. Saved paste-sequence preparation/playback and `should_keep_paste_sequence_dialog_visible_during_playback()` already have regression tests, including grid/prompt cases. |
| `src/universal_actions/model.rs`, `src/gui/universal_action_executor.rs` | `ActionSurface`, `UniversalActionInvocationContext`, `RootLauncherPolicy::{Legacy,PreserveOrdinaryState}`, `execute_universal_action_with_context`, and UI intent handling already separate action effect from presentation surface. Preserve this architecture. |
| `src/gui/render.rs`, `src/gui/action_sheet.rs`, `src/gui/universal_action_catalog.rs` | Grid/list/search and Action Sheet use deferred primary and/or universal-action activation. UI callbacks should not independently reimplement action execution or visibility rules. |
| `src/gui/radial_actions.rs`, `src/radial/handoff.rs`, `src/radial/model.rs` | Radial dispatch prepares/revalidates bindings, carries `InteractionRequirement`, root preservation and `AfterActionPolicy` (`Inherit`, `KeepOpen`, `CloseCurrentMenu`, `CloseTree`). Radial overlay teardown must precede exclusive capture/confirmation. |
| `src/gui/watch.rs`, `src/gui/render.rs`, `src/gui/hotfolder_dispatch.rs` | Gesture `WatchEvent::ExecuteAction`, MkMacro launcher commands, and Hot Folder dispatch already have dedicated event/host paths. Keep their distinct trigger/ownership and cancellation semantics. |
| `src/pinboard.rs` | `Pinboard`, `Pin`, `PinSnapshot`, `PinImageHandle`, `PinboardLimits` own session snapshots with an **8-pin**, **256 MiB aggregate image**, **128 MiB per-capture** default budget. `PinSnapshot` is currently immutable; tracked `Arc` leases outlive closed pins until final references are released. There is no pin persistence. |
| `src/plugins/pinboard.rs`, `src/commands/handlers/pinboard.rs` | Search/command routing currently supports only `pinboard capture` and `pinboard list`, corresponding to `pinboard:capture` and `pinboard:list`. The typed command handler intentionally returns default `CommandOutcome` and leaves specialized UI lifecycle to the host. |
| `src/gui/pinboard.rs` | `PinboardCaptureLifecycle` coordinates the shared MkMacro rectangle overlay, `LauncherParkingTransaction`, native pin hide/restore acknowledgements, `ScreenCaptureBackend`, bounded initial-capture worker, and cancellation/restoration. Reuse the established capture-selection safety contract rather than creating a second overlay. |
| `src/gui/pinboard_windows.rs` | `PinWindowPresenter` owns independent `egui::ViewportId` children, exact native window identity, click-through/topmost controls, per-pin texture, zoom/fit/actual sizing, recover/close/list UI, async copy/save jobs, and shutdown/drain. Child callbacks must not mutate launcher root visibility or accidentally target a different HWND. |
| `src/mkmacro/screen.rs` | `ScreenCaptureBackend` / `WindowsScreenCaptureBackend` capture visible *desktop* rectangles; that is appropriate for an initial static snapshot but **does not establish unobscured, window-attached live capture**. A true live source needs a separate narrowly scoped native window-capture implementation. |
| `src/gui/note_panel.rs` | Full Markdown note editor/viewer, `NotePanel` per-open-panel state, `render_toolbar`, `render_bounded_editor`, `render_editor`, `render_preview[_document]`, `render_collapsible_preview`, `render_split`, and `handle_editor_response`. `TextEditState` cursor offsets are **char indices**; `pending_selection` and `pending_cursor_position` already exist. Native clipboard-image paste, inline links, outline navigation, collapsible sections and unsaved-state management need protection. |
| `src/settings/model.rs` | `NoteSettings` allows Edit/Preview/Split, defaulting to Preview when rich Markdown is enabled; `NoteViewMode` and capability fallbacks already exist. Do not add a persisted Find setting or change default view modes. |
| `src/gui/notes_dialog.rs` | Quick Notes has an existing **list-wide search/filter**, not the requested in-note navigation. Preserve it. |
| `AGENTS.md`, `.codex/config.toml`, `.codex/agents/*` | The project already configures planner, implementer and reviewer agents, active checkpoint commits, targeted verification, and best-effort push-after-commit with local continuation on push failure. |

**Technical reconciliation requirement:** these locations describe the reviewed source, not a license to assume every future checkout has identical line numbers. Before editing, Codex must validate names/signatures against its own active repository. Use the current repository's actual implementation if a location evolved.

## C. Functional requirements and invariants

### C1. Action parity

1. For the **same resolved action and captured inputs**, primary invocation from launcher list/search, grid, eligible radial bindings, gesture/hotkey, and eligible saved invocation surfaces must dispatch the same typed command or universal operation **once**. Preserve the originating `ActivationSource` for history/auditing rather than erasing attribution.
2. Confirmations must be enforced consistently with existing destructive-action policies; a cancel must perform no operation and restore the appropriate invoking UI state. Surface-specific *additional* radial confirmation rules remain valid.
3. Action-specific settings determine outcomes: especially Paste Sequences' **Keep dialog visible during playback**, foreground/paste target captured at the correct time, and clipboard-modify hide-on-success preferences. Incompatible input/capture/overlay state must reject or defer safely, never silently change target.
4. Actions opening launcher-owned dialogs should result in reachable, focused dialogs without bringing the grid over a native child editor or stealing external input. Commands needing external input/capture may close their invocation overlay while preserving root/panel state as their existing policy requires.
5. Failed/disabled/stale actions produce a useful error and do not record a false successful execution, launch twice, flip visibility twice, lose pending prompts, or apply an action to a stale runtime target.
6. Explicit `RootLauncherPolicy` and `AfterActionPolicy` semantics, radial revalidation/leases, unique input ownership, history/query attribution, and existing hotkeys are invariants. **Do not normalize intentionally different UI surfaces by forcibly setting one visibility value.**
7. Scope the work to concrete parity gaps found during a compact behavior matrix and associated tests. If a case is already correct, keep production code unchanged rather than performing a decorative refactor.

### C2. Live pinboard

1. Existing static `pinboard capture` and `pinboard list` commands, screenshot editor commands, immutable snapshots, and their tested limits remain unchanged.
2. Expose a discoverable opt-in **live** pin action/command. It uses the existing safe rectangle picker; cancellation does not produce an empty pin. Avoid showing the launcher/pin windows in captured frames during selection; always restore them on cancellation/failure.
3. Source identification is tied to one external window with validated lifetime/ownership and crop geometry. Store the source window's identity and crop in source-relative coordinates. A moved/covered source shows **its** content, not pixels at the old screen position; on resize/DPI change, constrain or suspend the crop rather than sampling unrelated windows.
4. Use a window-content capture facility appropriate for Windows 10/11 (prefer native Windows Graphics Capture with a window item and frame events, if supported in the checkout/platform). The existing `screenshots`/screen-rectangle backend may seed a preview or help selection but must not be misrepresented as unobscured window capture. Any required Windows crate feature activation must be narrowly justified. Unsupported/protected sources yield a clear status and no unsafe fallback.
5. Keep capture and conversions off the root UI/render thread. Bound active capture sessions, in-flight frames, allocations and repaint work. No unbounded queues; discard superseded frames. Target approximately **10 fps maximum per active pin**, with responsive pause, hidden/minimized handling, and without inducing constant idle repaints when no live updates exist.
6. Preserve at most **8 total pins** across static and live. Enforce the existing 256 MiB CPU image budget / 128 MiB single-image bounds including outstanding frame/copy/save references as applicable; do not leak old-frame `Arc`s or textures. Prevent rapid reallocation on unchanged geometry. Provide understandable capacity/errors.
7. Native pin controls (Move/Resize, Fit/Actual/zoom, Topmost, Click-Through, Recover, Close/Close All, Copy Image, Save As) remain functional. Add only necessary live status and **Pause/Resume**; paused/closed/unavailable source is visually distinguishable from a healthy refreshing source.
8. Copy/Save use one immutable lease to a **completed** frame. Saving is never automatic; closing one live pin cancels only its own session. Closing all, disabling/shutting down, and capture-picker cancellation drain workers/native objects without reviving closed pins or keeping background capture alive.
9. Pin contents and source binding remain session-only. No change to persisted settings, screenshot output destinations, or static file formats.

### C3. In-note Find

1. In an **open `NotePanel`**, `Ctrl+F` reveals/focuses a small Find control; typing updates matches in **that panel's current note** only. Add a visible Find affordance in the note panel toolbar if that matches current compact toolbar conventions. Do not commandeer `Ctrl+F` while another dialog or unrelated input owns focus.
2. Case-insensitive, **literal** matching by default; no regex, global notes indexing, replace function, or persisted search history. Derive matches from the note's **current unsaved** content, not a disk snapshot. Unicode-safe mapping back to byte and egui char indices is required.
3. Show `current / total`, including `0 / 0`. Enter advances, Shift+Enter reverses, wrapping; previous/next buttons provide mouse alternatives. Escape dismisses the Find control without closing the note or losing unsaved edits. A reopened Find may select its prior transient query in that same panel.
4. In Edit, scroll to/select the exact occurrence in the `TextEdit`; in Split, keep a meaningful indication in the editor and preview; in Preview, navigate to the relevant rendered content and provide a visible active-match locator. Rendered formatting, links, callouts, collapsible sections and source-only markup must be handled honestly: if a source-only match cannot be visually represented in Preview, show that it is a source match and make revealing it in Edit an **explicit user action**, not an automatic mode switch.
5. Edits or external note mutations invalidate/recompute match ranges safely; maintain sensible next/current position, or reset to the first valid match. Close/reopen of a note, multiple note panels, note slug switches, changed content, empty strings, overlaps and invalid Unicode/offset boundaries must not cause panics or stale highlighting.
6. Find state and highlighting do **not** alter Markdown data, mark the note dirty, save it, change view mode defaults, disturb clipboard-image paste interception, or break existing outline/collapsible links and keyboard behavior.

## D. Scope / explicit non-goals

**In scope:** only the three features above; new narrowly necessary Windows capture support; tests/documentation directly describing the changed behavior.

**Out of scope:** workspaces; redesign of the launcher, radial, pinboard, Notes or settings; a new macro engine; new global configuration pages; persistent live-pin restore/recording/GIF/video; audio capture; global Find Across Notes; find-and-replace; new search query providers; completely uniform root visibility; generic CommandBus rewrite; cross-OS live capture; unrelated optimization; unrelated Clippy cleanup; vendored dependency changes; release-performance benchmarking campaigns.

Preserve pre-existing file formats and command spellings. New typed command arms may be additive, but do not repurpose old actions or widen the meaning of unrelated commands.

## E. Technical integration strategy

**Action parity:** map the current invocation entry points (`activate_action`, deferred `render` callbacks, universal action executor, radial prepared handoff, gesture events, macro/scheduled dispatch) to shared typed dispatch and surface-aware outcome application. Add a **small, test-driven parity matrix** for a representative set of actions: normal external/input action; launcher-owned dialog/note; Paste Sequence with keep-visible true and false; clipboard-modify preference; destructive confirmation/cancel; capture/overlay-requiring action; unavailable/stale action. Only change demonstrated divergent paths. Prefer fixing a misplaced per-surface preprocessing/postprocessing step to inventing a new dispatcher. Preserve source-specific root policy, query, and history labels.

**Live pins:** keep `Pinboard` as the domain/lifecycle owner, `PinboardCaptureLifecycle` as selector/parking/restoration owner, and `PinWindowPresenter` as native child/UI/texture owner. Add a **typed mode or distinct creation intent**, never a boolean scattered through callbacks. Introduce a narrowly owned **per-live-pin capture session** with source-window identity, source-relative crop, generation/lifetime token, status, cancellation, bounded most-recent-frame handoff, and explicit resource accounting. Do not couple frame cadence to full root-window redraw. The UI displays completed frames with an updated texture; pin controls send commands to their exact owned child runtime. The capture backend should be faked in unit tests. Reuse the existing `PinActionWorker`/clipboard/save abstractions after selecting a stable frame snapshot. Verify that self-capture and native pin hide/restore ownership remain correct.

**Find:** keep query, match index, and visibility in `NotePanel` (or a small module owned by it). Use literal Unicode-safe matching with ranges into the original content; an escaped `regex` with Unicode case-insensitive support may reuse the existing dependency, but never use indices from a separately lowercased string directly against original content. Compute match data only on actual query/content changes. Translate byte ranges to character ranges for egui text cursors. Reuse `pending_selection`, `pending_cursor_position`, `handle_editor_response` and preview/outline scrolling conventions; avoid rewriting the Markdown renderer. Source-only preview match handling must be explicit and non-destructive. Match highlight is presentation state, not a Markdown mutation.

## F. Dependency-ordered milestones, checks, and commit boundaries

The three enhancements are independent. Complete their checkpoints in order; do not run overlapping agents that edit the same source. A checkpoint is complete only after its scoped behavior and verification pass. If it makes no production/test/documentation change, **do not fabricate an empty commit**.

### M0 — Confirm ownership and bound the baseline (no code-change commit)

- **Objective:** verify the current checkout and establish the smallest feature-specific regression baseline.
- **Inspect:** `AGENTS.md`, current branch/status, the ownership table in Section B, existing pinboard/notes/action tests, and README descriptions. Note unrelated dirty files without modifying them.
- **Tasks:** document internally the actual entry points and representative parity cases; identify available Windows capture APIs/dependencies; confirm the note's default view mode and native viewport model. Adjust source paths only if the repository differs from the reviewed version, not the approved scope. Determine the narrow per-component build/test commands supported by this checkout.
- **Invariants/edges:** preserve active branch, dirty worktree, and existing persisted data; no code modifications solely to "prepare" a refactor.
- **Verify/accept:** source ownership confirmed and checkpoint acceptance tests chosen. **No broad tests or build in M0** unless a concrete ownership question cannot otherwise be resolved.
- **Commit:** none.

### M1-A — Action parity contract and regression coverage

- **Objective:** make the requested cross-interface behavior measurable, distinguishing operation effects from legitimate presentation differences.
- **Ownership:** `src/gui/actions.rs`, `src/gui/render.rs`, `src/gui/universal_action_executor.rs`, `src/gui/radial_actions.rs`, `src/gui/command_host.rs`, `src/commands/{bus.rs,outcome.rs}`, and directly relevant existing tests.
- **Tasks:** (1) trace a shared primary action through list/search, grid, and radial prepared dispatch; include gesture/hotkey and macro/Hot Folder as representative secondary routes only where the same action contract is supported; (2) build a compact table of actual vs expected command identity, target, settings snapshot, confirmations, history source, root/focus policy, and cancellation; (3) add focused regression tests for **real** divergences and for preserving intentionally different radial and paste-sequence policies. Reuse existing GUI test fixtures and hooks; no real external app launch in unit tests.
- **Behavior/edges:** same operation once, correct target, no accidental double activation, stale resolution rejected, original source attribution retained. `AfterActionPolicy`, `RootLauncherPolicy`, and input leases remain honored.
- **Verification:** run the newly added action/CommandBus tests only, plus `cargo fmt --check` after formatting. A test intentionally showing a defect may be documented locally and immediately addressed in M1-B; do not commit a deliberately failing suite.
- **Acceptance:** equivalent-outcome matrix is explicit in tests; disallowed forced visibility normalization is absent; no unrelated command routing changes.
- **Commit:** `[M1-A] test(action): define surface-aware action parity cases` when tests are coherent/passing, otherwise combine the tests with M1-B.

### M1-B — Fix only confirmed action parity gaps

- **Objective:** remove observed mismatches without rewriting the dispatcher.
- **Ownership:** only the affected call sites from M1-A. Prefer existing `activate_action`, `execute_universal_action_with_context`, `dispatch_command_invocation`, and `apply_command_outcome_with_root_policy` seams.
- **Tasks:** (1) correct input/result handoff where grid/list/radial/gesture differs accidentally; (2) correct focus/visibility only if inconsistent with the action's explicit policy, notably keep-visible Paste Sequences and launcher-owned dialog opening; (3) keep source-specific root/confirmation/capture semantics intact; (4) protect error paths against double effect or false success; (5) update focused tests to prove each fix.
- **Behavior/edges:** action success, error, cancellation, repeat invocation, hidden root, visible root, radial keep-open, prompted Paste Sequence, and external paste target all remain deterministic.
- **Regression constraints:** do not alter global hotkey triggering, radial tap/hold behavior, unrelated app commands, persisted settings, or action history schema. If M1-A finds no production divergence, finish this checkpoint with a verified no-change conclusion rather than manufacturing edits.
- **Verification:** rerun only action parity tests and directly affected existing paste-sequence/radial tests; one scoped compile check if changes warrant it.
- **Acceptance:** all M1-A parity/intentional-difference cases pass and the production diff is limited to genuine issues.
- **Commit:** `[M1-B] fix(action): preserve equivalent execution across invocation surfaces` if production code changed.

### M2-A — Add typed live-pin intent and safe source/crop selection

- **Objective:** introduce a separate live creation route and associate each selection with a valid external source window, without changing static capture.
- **Ownership:** `src/plugins/pinboard.rs`, `src/commands/{model.rs,parser.rs,handlers/pinboard.rs,host.rs}`, `src/gui/{pinboard.rs,command_host.rs}`, `src/pinboard.rs`; inspect existing overlay selection type and root parking code.
- **Tasks:** (1) add an exact typed `pinboard:live` command surfaced by `pinboard live`, preserving current `capture` and `list` grammar, invalid payload behavior, and help; (2) send a live/static intent through the existing single outstanding selector, reserving normal pin capacity before parking; (3) safely identify the source HWND after overlay termination, validate external ownership/top-level identity and source-relative crop, and reject ambiguity rather than recording a desktop-fixed rectangle; (4) preserve native child hide/restore and root visibility/focus restoration on success, cancel, stale operation, and error; (5) add deterministic source/window-selection fakes as necessary.
- **Behavior/edges:** mixed-DPI or negative-coordinate virtual desktops, overlapping/self windows, source closing during selection, duplicate requests, user cancellation, and 8-pin/byte exhaustion must be handled explicitly.
- **Regression constraints:** `pinboard capture`, `pinboard list`, screenshot commands, original immutable-pin API, selection suppression, focus/hide acknowledgement, and click-through recovery unaffected.
- **Verification:** targeted parser/handler tests and lifecycle selection/restore tests; compile check if integration changed public trait arms.
- **Acceptance:** static path unchanged; live intent reaches source selection with a validated source/crop or clean error; no empty pin or leaked overlay/session.
- **Commit:** `[M2-A] feat(pinboard): add opt-in live capture intent and source binding`.

### M2-B — Implement bounded per-window live frame capture

- **Objective:** obtain and distribute correctly cropped, current frames of the bound **window**, including under ordinary desktop occlusion where Windows permits, without blocking UI.
- **Ownership:** a small Windows capture implementation owned by the pinboard subsystem (a new `src/...` file only if useful), `src/gui/pinboard.rs`, `src/pinboard.rs`, the narrow Windows crate features in `Cargo.toml` only if required.
- **Tasks:** (1) implement native window-content capture bound to the selected window identity (prefer Windows Graphics Capture and frame-arrived notifications; do not rely on repeated screen rectangle screenshots); (2) translate source-relative crop reliably across movement, resizing and DPI changes; (3) provide at most one pending frame per pin or similarly bounded latest-frame transfer; (4) cap active refresh to approximately 10 fps and suppress work while paused; (5) reserve/release current and in-flight RGBA memory through the existing limits, including contention from copy/save references; (6) tag frames/completions with generation/lifetime and discard late frames from closed or replaced sources; (7) expose source status `Live`, `Paused`, `Temporarily unavailable`, `Source closed/Frozen`, and `Error`, without leaking internal handles; (8) add a fake source/frame stream for deterministic tests.
- **Behavior/edges:** window covered, moved, resized or destroyed; HWND reused; unsupported/protected capture; no frames for an interval; multiple live pins; mixed static/live memory pressure; shutdown during pending frame delivery.
- **Regression constraints:** no capture or image conversion on the root UI thread; no unbounded queue, spin loop or high-frequency idle repaint; no bypass of Windows content protection; static snapshots immutable.
- **Verification:** focused fake-capture tests for frame replacement, budgets, pause, stale-generation discard, source closure and cancellation; one targeted Windows compilation after the capture API integrates. Where a real Windows UI environment exists, do a bounded live-source smoke check, but do not make a headless CI environment pretend it verified WGC.
- **Acceptance:** a source frame reaches a live pin as a source-bound cropped image, stale frames never replace a newer frame, limits remain bounded, and frames cease on pause/close without blocking the app.
- **Commit:** `[M2-B] feat(pinboard): stream bounded source-window frames`.

### M2-C — Live pin viewport, list, pause and still-image actions

- **Objective:** make live pins usable through the current independent child-window experience.
- **Ownership:** `src/gui/pinboard_windows.rs`, pin runtime state, pinboard domain/image interfaces, existing copy/save worker; `src/gui/pinboard.rs` only for the narrow UI-to-lifecycle bridge.
- **Tasks:** (1) display complete live frames with texture updates rather than making a new texture per frame; (2) keep sizing/zoom/fit/actual, native topmost, click-through and recovery controls working for both modes; (3) add unobtrusive live-status indicator and Pause/Resume to the pin (and list where needed for click-through recovery); (4) snapshot one completed current frame for Copy Image/Save As, preserving the existing asynchronous picker/clipboard semantics; (5) request repaints of the *affected child* on frame updates, not an unrelated root redraw; (6) preserve native pin identity/lifetime checks, including stale UI callbacks.
- **Behavior/edges:** source changes during Save As, paused frame copy, stale texture update after close, multiple independent pins, click-through pins managed from the list, pin owner/root hidden, scaling at non-100% DPI.
- **Regression constraints:** original static texture/action paths, `Recover All`, `Close All`, exact native window lookup, and pin list keyboard accessibility must remain correct.
- **Verification:** focused pin-presenter tests with fake native owners/frames and action clipboard/save fakes. If UI control is available to Codex, open a static pin and live pin and capture bounded screenshots of their controls and updating content.
- **Acceptance:** controls work on both modes; the live pin visibly updates; paused or closed source remains understandable; copy/save saves the frame that was actually displayed or most recently completed, not an earlier immutable seed.
- **Commit:** `[M2-C] feat(pinboard): display and control live pins`.

### M2-D — Native lifecycle, race, and resource regression gate

- **Objective:** ensure the substantial live-capture feature cannot destabilize normal launcher/pinboard lifetime.
- **Ownership:** only live pin owner/worker, `PinboardCaptureLifecycle`, `PinWindowPresenter`, and their direct tests.
- **Tasks:** (1) verify cancellation drains native captures, completed frames, and callbacks; (2) verify root/native child hide/restoration while a second static/live pin is selected; (3) ensure closing last live pin removes background activity; (4) confirm source death preserves the last valid frame and never rebinds a recycled handle; (5) confirm bounded resource accounting including outstanding copy/save jobs; (6) validate mixed static+live 8-pin cap, source exceptions, and teardown ordering.
- **Behavior/edges:** close while screenshot picker is active, close while save-picker is open, suspend/recover click-through, shutting down with frame in flight, late native callback after root dismissal.
- **Regression constraints:** no intentional extension to screenshot editor, Screen Draw, MkMacro capture, or unrelated Windows hooks.
- **Verification:** only pinboard model, lifecycle, presenter, and newly added live-capture tests; one real-window smoke session on Windows if environment permits.
- **Acceptance:** no pin/window resurrection, leaks, stale HWND capture, stuck worker, or unbounded idle refresh under the targeted scenarios.
- **Commit:** `[M2-D] test(pinboard): harden live capture lifetime and cleanup` (combine with preceding checkpoint if only tiny follow-up edits).

### M3-A — Per-note Find state, matching, and navigation

- **Objective:** implement deterministic source-text matching separately from UI side effects.
- **Ownership:** `src/gui/note_panel.rs` or a small adjacent note-find helper; only existing `regex`/Unicode utilities if useful.
- **Tasks:** (1) create per-`NotePanel` transient query/open/results/current-index state; (2) compute literal case-insensitive matches in original content with valid ranges, character offsets and a bounded recompute trigger; (3) define Enter/Shift+Enter wrap behavior, empty/no-match state and current-index update after edits; (4) keep all matches independent between multiple open notes; (5) expose simple APIs for Find UI, editor selection and preview location.
- **Behavior/edges:** Unicode case-fold length changes, multi-byte characters, line endings, punctuation, repeated/overlapping occurrences, empty string, no match, edit while Find open, large note, source replaced externally.
- **Regression constraints:** Find must never mutate or mark Markdown dirty; no new persisted fields; no interference with note cache/save paths.
- **Verification:** targeted matcher/state tests; formatter only. Use a safely bounded number of results or derived ranges if unusually large notes require it, but preserve correct total count.
- **Acceptance:** deterministic matching/index navigation with correct char/byte mapping and independently isolated panel state.
- **Commit:** `[M3-A] feat(notes): add transient per-note find navigation`.

### M3-B — Note panel Find controls across Edit / Preview / Split

- **Objective:** deliver keyboard and mouse-visible in-note navigation in the current note panel.
- **Ownership:** `src/gui/note_panel.rs`; inspect `src/notes_markdown/*`, egui text cursor/scroll helpers and the existing preview section layout only where necessary.
- **Tasks:** (1) add compact Find strip with query, count, Previous, Next and Close actions; (2) scope `Ctrl+F` to the active open note panel and use Enter/Shift+Enter/Esc without forwarding those keys as accidental edits/commands; (3) in Edit, scroll and select the active match with valid `TextEditState` indices; (4) in Preview, navigate to the corresponding visible rendered block/fragment and visibly indicate it, with honest source-only fallback; (5) in Split, keep the match target discoverable in both panes without changing split sizing or re-entering focus loops; (6) reset/reconcile active ranges after edits or view switches; (7) add focused interaction tests using current note-panel test fixtures.
- **Behavior/edges:** multiple notes open; other modal dialog focused; hidden/collapsed Markdown section; user editing while search active; other `Ctrl+F` handlers; cursor paste interception; character offsets with mixed Unicode; unsaved edits and close prompt.
- **Regression constraints:** note view-mode defaults, outline scroll/selection, rich Markdown links/images, task checkboxes, preview caching, accessibility focus, existing editor shortcuts and Quick Notes list filter unchanged.
- **Verification:** targeted `note_panel` tests and, if available, a bounded UI session with an Edit/Preview/Split note and `Ctrl+F`/Enter/Shift+Enter/Esc; capture screenshots of the Find strip and active match. Do not run all note/plugin tests by default.
- **Acceptance:** a user can locate, advance and reverse matches in any supported note view without modifying content, changing modes unexpectedly, or losing a prompt/selection.
- **Commit:** `[M3-B] feat(notes): add in-panel find controls and mode navigation`.

### M4 — Final focused integration review and documentation

- **Objective:** finish the three approved features, not start a general cleanup campaign.
- **Ownership:** changed files plus relevant README/help text, no unrelated source.
- **Tasks:** (1) run all feature-specific acceptance cases in Sections C and F; (2) reviewer inspect full diff for scope changes, source/crop identity errors, action policy regressions, UI focus traps, memory/frame leaks, and accidental note persistence; (3) update only README/help sections describing `pinboard live`, its temporary/paused/closed behavior, in-note Find shortcuts, and any user-visible action-policy clarification; (4) run final targeted lint/format checks; (5) record evidence, commit list and push outcomes.
- **Behavior/edges:** mixed-feature check: invoking `pinboard live` from search/grid/radial still honors the exclusive capture lifecycle; invoking note-open from multiple surfaces still presents a usable Note Find; existing Paste Sequence dialog visibility remains configured as before.
- **Verification:** see Section H; do not require complete nextest, unrelated benchmarks, or repeated release compilation. If the Codex host cannot start Windows native GUI, report that as *not manually smoke-tested*, not as passing.
- **Acceptance:** all three feature gates pass; relevant Clippy diagnostics handled or accurately classified; no scope creep or unresolved feature-related blocking defect.
- **Commit:** `[M4] docs(launcher): describe action parity, live pins and note find` **only if** there are actual documentation/cleanup edits; otherwise final report uses earlier commits.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M1-A/B, M2-A/B/C/D, M3-A/B and optional M4 above; combine adjacent micro-changes rather than fabricating WIP commits.

- Commit each coherent, verified checkpoint before beginning materially different work. Follow the existing `AGENTS.md` subject style: `<type>(<scope>): [M#-X] <clear description>`.
- Verify the focused behavior for that checkpoint, not the entire application. Never squash, reset, rebase, force push or otherwise rewrite/discard the user's history; preserve unrelated dirty changes.
- After **each completed local commit**, attempt one normal best-effort push if repository instructions permit it and a remote/upstream is available. On push failure, keep the local commit, note the failure, and **continue**—no repeated retries, forced push, upstream recovery project, or interruption for user intervention.
- Repair regressions through small follow-up commits when necessary. No empty checkpoint commits and no parallel writers touching the same files.

## H. Verification / testing strategy

Keep verification intentionally focused despite the application size and historical build times.

**Automated gates (suggested commands, adjust to the checked-out target):**

- `cargo fmt --check` after formatting each coherent code stage.
- `cargo nextest run -E 'test(pinboard)'` for pinboard model/command/presenter/lifecycle; add named test filters for the newly introduced live-window module. Filter more narrowly if that expands unexpectedly.
- `cargo nextest run -E 'test(note_panel)'` (and specifically named new note-find tests) for note interactions; include Quick Notes list-filter tests only if it was touched.
- `cargo nextest run -E 'test(paste_sequence)'` plus specifically named new action-parity/radial tests **only when those routes were modified**; use explicit selected test names if this selection is too broad.
- `cargo check --lib` and, if necessary for native startup routing, `cargo check --bin multi_launcher` at the staged integration gates.
- Final `cargo clippy --lib --bin multi_launcher -- -D warnings` **once** if practical after relevant compilation succeeds; if third-party/preexisting warnings block, report exact provenance, and never edit vendor code merely to quiet them.

**No full `cargo nextest` suite, all-targets build, unnecessary release build, benchmark pass, or performance parity report** is required. Prefer the repository's owned-process / native smoke guidance in `AGENTS.md` for UI sessions.

**Windows UI evidence, when feasible:**

1. Same representative action invoked from list/grid/radial/gesture produces equivalent effect, while radial KeepOpen and Paste Sequence visibility settings remain honored.
2. One static and one live pin coexist. Change a visible source-window region and confirm live pin refresh; move and partially cover source; Pause/Resume; Copy/Save latest frame; Recover click-through; close source and see frozen status; close pin; ensure idle returns.
3. Two different notes or one note across Edit/Preview/Split: `Ctrl+F`, Enter, Shift+Enter, Esc, no-match and a live text edit; note content and dirty status unaffected by Find itself.

Use deterministic fakes for native capture faults/resource/race cases. UI proof can include short screenshots and trace observations; do not require the user to run the application or provide logs before final delivery.

## I. Risk management and bounded validation budget

| Concern | Required guardrail |
| --- | --- |
| Cross-surface parity accidentally overrides intentional rules | Compare normalized **action effect** separately from radial/menu/root visibility; lock deliberate exceptions in tests. |
| Live captures sample unrelated pixels or stale HWND | Window-bound capture identity and relative crop; no silent fallback to desktop screen polling; reject ambiguous selection; drop late generation frames. |
| Main thread lag / memory blowup | Async capture events, approximately 10-fps cap, bounded latest-frame queue, documented image-budget behavior, child repaint only, release on close/pause. |
| Native overlay / capture teardown races | Reuse established pin hide/restore acknowledgements and parking; prove cancel, error, shutdown, closed native child and source-death paths with fakes. |
| Note UI focus/caret regressions | Find controls own only their keys; char-safe cursors; no Markdown edits for highlighting; preserve focused other dialogs and note view mode. |
| Preview-only Markdown markup matches | Explicitly indicate source-only match and offer user-controlled reveal; do not claim an invisible raw token is highlighted in rendered output. |

**Validation budget:** one focused compile/test pass per materially completed subsystem (M1, M2 capture, M2 UI/lifecycle, M3); one final targeted integration pass plus one final Clippy attempt. For an actual feature-related failing check, permit **at most two bounded diagnosis/remediation/recheck cycles per distinct failure** before reporting a genuine blocker with concrete evidence. A single native UI smoke session with a small fixed action script is enough; do not attempt an open-ended benchmark or restart release builds for unrelated warnings. Escalate only where missing verification would make a feature's core behavior unknowable or unsafe.

A blocker must be classified: (a) requested-feature blocker, (b) preexisting/unrelated issue, or (c) optional follow-up. Report the failing command, exact symptom, suspected owner, attempted remedies, and retained commit state; do **not** silently weaken acceptance requirements or claim success from a test that never ran.

## J. Definition of done

- [ ] Source checkout and architecture reconciled against current branch/`AGENTS.md`.
- [ ] Shared actions resolve/execute consistently across supported surfaces while preserving intentional source-specific presentation, settings and safety.
- [ ] Static pinboard behavior remains intact and an opt-in window-bound live pin can be created, updated, paused, copied/saved, recovered, frozen on source closure and closed without resource/lifetime defects.
- [ ] Note Find can search/navigate in Edit, Preview and Split with per-note transient state and safe focus, offsets and no content mutation.
- [ ] Targeted new and directly affected regression checks pass, or any unsupported native-only portion is precisely documented; no known feature-related blocking defect is dismissed as optional.
- [ ] Relevant first-party Clippy concerns are resolved or factually documented; no unrelated cleanup.
- [ ] Source/help changes and checkpoint commits completed; best-effort pushes attempted per repository policy and failures retained locally without blocking work.
- [ ] Final reviewer confirms scope, invariants and coherent diff; delivery report identifies completed/verified/not verified.

## K. Required final delivery report

The orchestrator must report, factually and compactly:

1. Three-feature status, each with completed checkpoints and user-visible outcomes.
2. The actual files/modules modified, plus any necessary Windows capture feature/dependency justification.
3. Significant decisions and exceptions—particularly radial root policy, selection/window capture behavior, minimized/closed-source handling, and Preview source-only matches.
4. Commands/tests run **with pass/fail/not-run results**; captured UI evidence where available.
5. Checkpoint commit subjects/hashes, active branch, and per-commit pushed or local-only status (brief reason for failures).
6. Remaining feature blockers, known limitations, and concise **final user manual-review steps only**. No intermediate manual requests.

**Stop after these three features and their bounded validation.**
