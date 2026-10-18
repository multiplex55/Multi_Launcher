# Multi Launcher — Usability, Paste Sequence, Capture, and Cargo Refinement Plan

**Status:** Approved requirements; ready for autonomous Codex implementation.  
**Product:** Multi Launcher, Rust 2024, Windows, eframe/egui 0.36.2.  
**Execution:** Follow the current repository checkout and `AGENTS.md`; inspect only the directly relevant code first, then implement immediately.  
**Changes covered:** six requested improvement areas (search-scope removal, Screenshot Pin input shielding, Paste Sequence scrolling, Paste Sequence configurable hotkeys, Paste-to-Advance/status tooltip/target-independent playback, Search and Replace UI, and Cargo bin selection—seven technical work areas, because Paste Sequence is split into two).  
**Code-first:** Avoid another broad discovery/planning round. The approved behavior below is the implementation contract.  
**Source authority:** Codex operates exclusively on its currently checked-out repository. The source map is an aid, not a substitute for checking current code.  
**No feature work concerning application workspaces.**

> **Non-negotiable sequence:** implement/commit small checkpoints → only directly affected focused tests/build checks → one full Nextest run in final integration → formatting, docs, ledger and all non-lint commits → **final maintained-crate Clippy cleanup and last successful lint command** → final report. No user-operated testing, intermediate approval requests, unrelated feature additions or speculative optimization initiatives.

## 1. Outcome and non-goals

The following behavior must be delivered together:

1. **Remove launcher scope UI:** Remove All / Apps / Files / Notes / Commands, its Ctrl+1–5 shortcuts, associated filter/cache/session state, scope-specific help and dead code; retain full unified launcher search, dashboard, pinned queries, explicit plugin prefixes, Omni Search, notes/files providers and radial lookups.
2. **Pin capture left-button isolation:** When `pinboard capture` uses rectangle selection, prevent the physical selection press, held drag and release from selecting text, pressing controls or clicking underlying applications; cancel/teardown restore ordinary input. Reuse/fix the existing rectangle overlay's Windows input-shield ownership rather than add a competing hook by default.
3. **Paste Sequence step-list layout:** The existing Ordered steps scroll container must *actually* remain bounded (~240–280 logical points maximum, shrinking in constrained windows), independent of number/size of steps, including multiline text.
4. **Configurable Paste Sequence controls:** Next/Cancel global key chords configurable from main Settings, accessible from the Paste Sequences dialog; independent optional per-sequence start chords; validation against all active hotkey families; coherent live reconfiguration; ability to save sequence contents without failing due to a conflicting built-in playback chord.
5. **Third playback mode and unobtrusive follower:** New `Paste-to-Advance (Ctrl+V)` mode, floating cursor-following, non-focus-stealing, mouse-pass-through status tooltip during **all active playback modes**, configurable tooltip and Esc cancellation; physical Ctrl+V advances exactly one step per real press in Paste-to-Advance, not on simulated repeats, and after the final step the last text remains in clipboard for ordinary Ctrl+V.
6. **User-directed targets:** *No Paste Sequence mode is bound to its original window.* The user may intentionally switch external applications between steps. Before any synthetic input, check the **then-current** eligible target/window and refuse launcher/self, secure/protected, invalid or ambiguous targets. Do not forcibly refocus the original application or cancel solely because the foreground process changed.
7. **Search and Replace UI:** Fully vertically scrollable, compact, independently collapsible basic/advanced settings, matched file list, match approvals, original/proposed previews and recovery; fixed/accessible action footer; bounded independent scrolling; preserve exact search/selection/apply/backup/restore/recovery behavior.
8. **Cargo default binaries:** Plain `cargo build` and `cargo build --release` compile `multi_launcher` as the only **application binary target**. Preserve `coordinate_tool_smoke`, `passive_overlay_smoke` and `radial_acceptance` as explicitly opt-in development binaries. Preserve integration tests, benchmarks, dependencies and build scripts.

**Do not**: add a settings toggle to hide scopes; retain scoped searching invisibly; alter the `fs` plugin's own Global/Directory scope; change dashboard/pinned-query data; build an arbitrary keyboard automation framework; interpret Ctrl+V as a global text-expansion tool outside active playback; reroute native input through new conflicting hooks without a proved need; alter underlying multi-file replacement rules; remove test sources; broaden into app-workspace features or performance overhauls.

## 2. Verified starting architecture: inspect and revalidate, do not blindly assume

These owners were visible in the approved planning source; Codex should search its current checkout for corresponding files rather than rely on exact line numbers.

| Feature | Source entry points / relevant boundaries | Existing behavior that must be preserved |
|---|---|---|
| Scope UI and search | `src/search_scope.rs`, `src/gui/search.rs`, `src/gui/render.rs`, `src/gui/mod.rs`, `src/plugin.rs`, `src/plugins/omni_search.rs`, `src/plugins/note.rs`, `src/plugins/folders.rs`, `src/gui/radial_actions.rs` | `SearchScope` and category-bearing `LauncherSearchResult` exist; category propagation spans plugin results and radial paths. Preserve independent Omni Search filtering and normal action ordering. |
| Pinboard | `src/gui/pinboard.rs`, `src/gui/pinboard_windows.rs`, `src/pinboard.rs`, `src/gui/mkmacro_dialog/visual_capture_workflow.rs`, `visual_overlay.rs`, `visual_overlay_windows.rs` | Pinboard borrows shared `RectanglePurpose::PinboardCapture` selection. Windows overlay renderer already creates hit-testable, non-activating `InputShield` windows for rectangle pickers; find why click still reaches underlying app. |
| Paste model / preflight | `src/paste_sequence.rs`, `src/gui/paste_sequence_dialog.rs`, existing snippet prompt/dialog/source loading | `PlaybackMode` is serialized as snake_case with `Automatic` and `ManualAdvance`; steps may be snippet references, clipboard history refs or literal text; `paste_sequences.json` version 1. Preserve source confidentiality, preflight prompts and stable IDs. |
| Paste controller / native | `src/paste_sequence/playback.rs`, `playback/windows.rs`, `src/main.rs`, `src/hotkey/launcher_invocation.rs`, `src/hotkey/validation.rs`, `src/hotkey/reconfiguration.rs`, `src/mkmacro/input/` | Existing controller captures clipboard and *fixed* target; `ensure_target` currently rejects other windows; native clipboard sequence numbers/conditional restore; main-owned shortcut routes currently hardcode Next PageDown and Cancel End. Need change the correct owner without duplicating hooks. |
| Settings | `src/settings/model.rs`, `src/settings_editor/state.rs`, `src/settings_editor/` + save/rollback and GUI settings sections | Main settings use persisted serde defaults and staged Save/Cancel. Do not bypass existing validation or save acknowledgement. |
| Replacement dialog | `src/gui/search_replace_dialog.rs`, `src/gui/mod.rs`, `src/search_replace.rs`, `src/replace_executor.rs` | Current dialog has `.vscroll(false)`; some child ScrollAreas already exist. A/B/C search/preview/write phases, immutable match IDs, guarded Apply and durable recovery must remain unchanged. |
| Cargo targets | `Cargo.toml`, `src/main.rs`, `src/bin/{coordinate_tool_smoke,passive_overlay_smoke,radial_acceptance}.rs`, `src/bin/radial_acceptance/`, `tests/` | Cargo currently discovers top-level `src/bin` automatically. The manifest also contains `[patch.crates-io]` vendored paths; these are *not* automatically workspace members. |
| Repository discipline | `AGENTS.md`, current CI/dev docs and targeted tests | Active checkpoint commit cadence, Rust/Windows hygiene, reviewed diffs, only scoped tests during feature implementation. |

**Important source-derived observations:** The Paste Sequence Ordered steps panel already contains an egui vertical `ScrollArea` with `max_height(210.0)`; diagnose actual expansion budget rather than adding a duplicate scroller. The input-shield WndProc already handles `WM_LBUTTONDOWN`, `WM_LBUTTONUP` and mouse movement; trace z-order, transparency, readiness, complete-monitor coverage, initial held-button and teardown. `LauncherSearchOutcome.categories` and `Plugin::search_results` form a category-carrying boundary; do not drop provider actions or change radial read-only search when removing scopes. There is only one regular Cargo package at the root, with patched vendor dependencies.

## 3. Delivery protocol and verification limits

### 3.1 Authority, agents, and pace

- Codex's **currently checked-out branch and working tree** are authoritative. Do not switch to `master`, merge a different branch, reset, force-push or overwrite user changes without explicit direction. Inspect `git status -sb`, `git log -1`, current `AGENTS.md`, relevant `Cargo.toml` and nearby tests; begin implementing the first feature immediately after bounded inspection.
- Use a **single writer** (implementer); read-only planner/reviewer passes are appropriate for tricky input-hook, clipboard and native-window checkpoints, but do not continuously replan approved requirements. Orchestrator passes one specific checkpoint packet at a time with scope, invariants and acceptance proof.
- **Checkpoint commits are mandatory** at natural boundaries, labeled `M1-A`, `M1-B`, etc. Commit only after changed behavior compiles/tests appropriately and `git diff --check` plus the touched diff are reviewed. Avoid pointless one-line/WIP commits, huge multi-feature commits, or rewriting checkpoint history.
- Keep an execution ledger (e.g. `docs/plans/refinement_execution_progress.md`) with checkpoint/status/hash/actual focused verification/known issues. Update at checkpoints and commit it naturally with work; no noisy minute-by-minute log.
- Users do **not** need to run builds, interact with the development app, supply screenshots, or approve any intermediate checkpoint. Codex may run owned development processes, isolated fixtures, non-sensitive GUI tests and screenshots autonomously. If desktop automation cannot access the app, record `not_run_environment` after **one bounded diagnostic attempt and one retry**, then continue with domain/egui/fake-native evidence.

### 3.2 Test selection and final ordering

- During M0–M7 run **only focused tests for directly changed code** and minimal compilation/formatting checks necessary to commit a coherent checkpoint. Usually one directly relevant Nextest slice, with a second slice only if the checkpoint crosses an additional runtime boundary. Do not run all Nextest mid-plan; do not run Clippy mid-plan.
- Use the repository's `cargo nextest` filters/targets where available; test fixtures must be self-owned and bounded. Tests involving keys, hooks, input shields and clipboard use fake boundaries plus *at most one owned native smoke per affected feature* when the execution desktop permits. Never modify users' real clipboard, files, windows or system shortcuts as test artifacts.
- After M1–M7 finish: **M8-A** performs direct integration, code cleanup, formatting, README/help updates and screenshots; **M8-B** runs **one full Nextest** (`cargo nextest run --workspace`, adjusted to actual packages/Windows target). If it fails, repair the first real regression and perform at most one bounded full rerun after focused proof; report continuing failures rather than looping. **M8-C** closes the progress ledger/documentation and all non-lint commits. **M8-D** is the only Clippy phase, scoped to maintained crates excluding vendor; fix findings in bounded passes and perform any focused lint-fix rechecks and formatting **before** the final successful Clippy command. That final command is the last automated verification and is followed directly by the report.
- Vendor directories named `vendor/`, `third_party/`, etc. must not be treated as maintained Clippy targets. If vendored packages appear in actual workspace membership, use verified `--exclude <package>` arguments. Normal vendored dependencies outside workspace are not explicit lint targets. Do not modify third-party vendor code for lint warnings.
- A real correctness/safety failure is not a lint warning. Do not mark a checkpoint complete if it could paste into the wrong window, clobber another app's clipboard, leak physical mouse clicks during capture, mutate files without approval or break normal launcher hotkeys.

### 3.3 Fixed acceptance vocabulary

- **Physical paste press** = an eligible human Ctrl+V key-down transition with a corresponding release, not keyboard autorepeat, an injected event, Ctrl+Shift+V or an unrelated copy/shortcut.
- **Owned clipboard state** = exact Windows clipboard sequence/generation plus matching data ownership recorded by this run; equal text content alone is insufficient proof of ownership.
- **Eligible target** = currently focused external interactive window/control on the normal input desktop, not Multi Launcher, a pinboard/tool overlay, a known password/protected control or unsupported/ambiguous target.
- **User-driven target** = target chosen by the user's present focus **for each step**, regardless of its relationship to the application/window used for a prior step.
- **Completed paste** = the feature has safely dispatched exactly one intended paste action and enforced an appropriate guarded consumption/settling boundary; Windows does **not** offer a universal acknowledgement that every target application has consumed clipboard data. Do not invent such an acknowledgement or advance the clipboard early.
- **Clippy last** = no Clippy command before M8-D, and no automated code-quality command after the final successful M8-D Clippy command.

## 4. Milestone overview / dependencies

| Milestone | Aim | Suggested checkpoints | Main dependency |
|---|---|---:|---|
| **M0** | Inspect current owners/fixtures; establish checkpoint ledger (no analysis campaign) | A | Existing checkout |
| **M1** | Remove Search Scope Filters and restore unified search | A–C | M0 |
| **M2** | Correct Pinboard region-selection mouse isolation | A–C | M0 |
| **M3** | Fix Ordered steps adaptive scrolling | A–B | M0 |
| **M4** | Configurable Next/Cancel hotkeys; decouple saving | A–E | M3 recommended |
| **M5** | Target-independent Paste-to-Advance, clipboard safety, Esc and floating tooltip | A–H | M4 |
| **M6** | Search and Replace compact/collapsible/scrollable GUI | A–E | M0, can follow M5 |
| **M7** | Opt-in-only three developer binary targets | A–B | M0 |
| **M8** | Integration, one full Nextest, docs/fmt, then final Clippy | A–D | M1–M7 |

Write-heavy checkpoints run sequentially. Independent M1–M3/M6/M7 work may be reordered for practical compile dependencies, but do not overlap writers or postpone all functional code until a long planning stage.

---

## M0 — Minimal execution setup (code-first)

### M0-A — Confirm checkout, targeted owners, and checkpoint discipline

**Deliverable:** A current-branch checkpoint ledger and a short owner/test matrix. No broad implementation proposal or preexisting benchmark.

**Instructions**
- Record branch, HEAD, dirty/clean status and preserve unrelated edits. Inspect current `AGENTS.md` precedence and exact relevant source owners in §2.
- Confirm `cargo nextest` availability, Windows toolchain, Cargo explicit/auto binary discovery and whether the build profile/feature defaults changed.
- Locate existing focused tests by name/module. Verify that `paste_sequences.json` currently deserializes old modes; identify current standard save rollback and shared input hook owners. Note no baseline Clippy or full Nextest is permitted.
- Create/update a brief ledger with status rows for M0–M8 and the evidence directory (use an ignored target fixture location). Do not create a speculative backlog or a huge baseline document.

**Verification:** Git/status and manifest/source inspection only. `cargo metadata --no-deps` if needed. No compilation mandatory solely for M0.  
**Commit:** `docs(plan): [M0-A] initialize refinement execution ledger` only if the file was actually created/changed.  
**Done:** Worktree ownership known; implementation begins at M1 without asking more questions.

---

## M1 — Remove main-launcher Search Scope Filters

**Required behavior:** There is one original unified search list; no All/Apps/Files/Notes/Commands UI or Ctrl+1–5 scope selection. Do not change `fs`, Omni Search user controls, dashboard, pinned queries, current provider actions, autocomplete/selection semantics, history or radial's read-only search path.

### M1-A — Remove scope presentation and reserved shortcuts

**Implement**
- Remove the scope strip and spacer/layout reservation from the query/list UI. Restore previous query/results separation and selection geometry in grid/list presentations, avoiding a dead strip or blank gap.
- Remove Ctrl+1–5 scope shortcuts and their event-consumption logic. Ensure Ctrl+1–5 return to whatever unmodified launcher behavior they had before scopes (do not introduce replacement shortcuts or suppress input).
- Remove scope-specific empty-state text, session reset hooks and visual state while preserving normal no-result feedback and query focus.
- Adjust only tests directly covering scope UI/shortcut/layout; add a narrow regression asserting the strip never renders and Ctrl+number no longer changes search behavior.

**Risk/notes:** The main query UI is also used by dashboard/grid and hotkey reopen paths. Recompute geometry from actual visible children, not from remembered scope-strip heights.  
**Verification:** Focused egui query-frame/UI shortcut tests; diff check.  
**Commit:** `refactor(search): [M1-A] remove launcher scope strip and shortcuts`.  
**Acceptance:** No scope UI, no retained vertical gap, dashboard/pinned entries and query focus remain reachable.

### M1-B — Remove scope filtering and unnecessary category state without losing providers

**Implement**
- Remove `SearchScope` state, `effective_search_scope`, category filtering, cache identity by scope and scope-only session invalidation; preserve stale-query/provider-revision protection, ranking, selected index and autocomplete behavior.
- Trace `LauncherSearchOutcome.categories`, `LauncherSearchResult`, plugin `search_results`, `SearchResultCategory`, Omni Search and radial dynamic search. Simplify category-only plumbing **only where verified unused after scope removal**; if a typed result wrapper still serves genuine independent consumers, keep that narrowly rather than rewriting provider APIs.
- Preserve action ordering and labels when All results previously displayed. Keep plugin-provided Notes/Files/Commands visible through their normal queries even if earlier scope badges were broken; never conflate scope removal with deleting Notes, Files or Omni Search.
- Keep explicit prefixes (`cs`, `note`, `fs`, `o`, `paste`, `rename`, `replace`, `xml`, `hash`, `pinboard`, `mouse`) and dashboard pinned-query execution unchanged. Do not change normal `fs` scope mechanics.
- Update the relevant test fixtures that constructed synthetic categories. Remove dead category tests rather than retaining tests for a retired feature.

**Verification:** One focused launcher search-provider/routing slice plus a directly relevant radial read-only search test where the type boundary actually changed.  
**Commit:** `refactor(search): [M1-B] restore unified result flow`.  
**Acceptance:** Unified query yields same actions/order as former All, with no scope cache invalidation or invisible category exclusions.

### M1-C — Compatibility smoke, help cleanup and deletion of dead scope artifacts

**Implement**
- Sweep only remaining scope-specific references, obsolete text and tests. Update README, in-app help and shortcut hints; remove `src/search_scope.rs` or reduce it only if the remaining types are truly unused (do not leave orphan exports/dead state).
- Test explicit prefixed queries, empty and nonempty unified query, dashboard pinned queries, notes/search provider, indexed file/app items, reactivation of launcher after hotkey, autocomplete and radial read-only search where impacted.
- Confirm scope removal does not affect unrelated words such as File Search "Global/Directory", regex match scope or screen draw tooltip scope.

**Verification:** A compact combination of directly affected routing/search integration tests; one owned UI screenshot *only if available*.  
**Commit:** `test(search): [M1-C] protect unified search and pinned actions`.  
**Acceptance:** Unified search fully replaces scoped searching, no new settings toggle, no regressions in specified consumers.

---

## M2 — Prevent underlying mouse interactions during Screenshot Pin selection

**Required behavior:** The selection's full physical left-button transaction (Down → moving while held → Up) belongs to the Screenshot Pin picker, not to the window underneath it. The normal mouse resumes immediately after complete capture/cancel/teardown. Other selection tools must not be changed unless their shared boundary requires the same narrowly justified fix.

### M2-A — Diagnose and correct existing picker input-shield ownership

**Implement**
- Follow `PinboardCapture` from command dispatch, launcher parking, shared visual selection reservation and `NativeOverlayRenderer` through per-monitor input shields. Verify that the input-shield windows are actually created for this purpose, are visible/hit-testable, cover every active monitor including negative origins, and occupy the correct z-order relative to passive/transparent picker visuals and target app windows.
- Verify that selection cannot be armed before the shield is installed and acknowledged; any failure to establish complete coverage must cancel safely **before consuming user selection** rather than allow transparent drawing over a live application.
- Fix the root cause in existing narrowly owned overlay/shield logic. Inspect layered/transparent styles, `WM_NCHITTEST`, `WM_MOUSEACTIVATE`, `WM_LBUTTONDOWN/UP`, mouse capture, initial held-button release and per-monitor geometry; don't add a global mouse hook or `BlockInput` as a shortcut.
- Maintain the normal rectangle geometry, border/tooltip rendering, cancellation and cross-monitor selection.

**Verification:** Focused native-overlay window-planning/shield existence/hit-test tests with fake Win32 seams where feasible.  
**Commit:** `fix(pinboard): [M2-A] establish exclusive selection input coverage`.  
**Acceptance:** When armed, there is no unprotected hit-testable area and no early click/drag leaking to the underlying window.

### M2-B — Handle complete press/drag/release lifecycle and cleanup

**Implement**
- Ensure Down, drag continuation and Up all remain owned by the shield/selector, including pressed-before-overlay, rapid double-clicks, release across monitors, Escape cancellation during an active drag and selector teardown during capture.
- Prevent teardown before a captured physical button is released/consumed. Do not leave native capture attached or swallow a later unrelated click after the operation finishes.
- Restore normal foreground/focus and mouse input after a successful selection, Escape, failed window creation, monitor change and app shutdown. Never leave a transparent topmost input shield orphaned.
- Keep Screen Draw and MkMacro rectangle/point selection behavior unchanged unless a verified shared code defect would otherwise compromise their input safety.

**Verification:** Focused reducer + native lifecycle tests for Down–Move–Up, cancellation, startup-held release, teardown and no delayed consumption.  
**Commit:** `fix(pinboard): [M2-B] complete guarded selection teardown`.  
**Acceptance:** No text selection/button activation underneath; pointer behavior returns normally on terminal outcome.

### M2-C — Safe native evidence and adjacent screenshot regression

**Implement**
- If an owned interactive Windows desktop is available, create a disposable target window containing selectable text/buttons, invoke `pinboard capture`, drag across it, assert target did not receive click/selection (including Up), and verify captured image + next ordinary click does reach target. Capture 1–2 redacted screenshots. Do not touch user apps.
- If native testing is unavailable, run deterministic overlay/shield message tests and record unverified native click-through as an evidence limitation; do not claim screenshot evidence that was not captured.
- Protect existing `ss` screenshot paths, Pin List/tooltip, cancellation, DPI and pin image lifecycle where the fix intersects shared selection.

**Verification:** One compact impacted screenshot/pinboard test slice; one bounded native smoke if feasible.  
**Commit:** `test(pinboard): [M2-C] verify selection input isolation`.  
**Acceptance:** Down/drag/Up blocked underneath and ordinary input restored, without regressing existing screenshot tooling.

---

## M3 — Paste Sequences Ordered steps adaptive scrolling

**Required behavior:** 100 steps and long multiline text do not make the Paste Sequences dialog exceed its available screen height. The Ordered steps editor remains independently scrollable, compact, keyboard/mouse accessible, and does not lose edits/reorder selection.

### M3-A — Fix actual layout ownership and height budget

**Implement**
- Inspect existing Ordered steps `ScrollArea::vertical().max_height(210.0)` and its surrounding egui Window/editor layout. Determine which container allows undesired expansion (parent window, adaptive row, inner multiline editor, preview below list, nested response size, first-frame measurement).
- Replace the ineffective budget with a genuinely bounded 240–280-point maximum that shrinks on short displays or smaller resized windows; allow a sensible lower bound without making Save/Run/Cancel unreachable.
- Contain long aliases/snippet labels, multiline temporary text and step rows horizontally; scroll the list rather than expanding whole dialog. Use stable ScrollArea IDs and do not rebuild all labels unnecessarily every frame.
- Keep existing visual theme, step order and prompt/hidden-text behavior unchanged.

**Verification:** Focused egui layout test covering 1 vs. 50 vs. 100 steps and a long multiline step; inspect actual used rectangles and scroll range.  
**Commit:** `fix(paste): [M3-A] bound ordered step editor height`.  
**Acceptance:** Dialog height is bounded by window size; list acquires internal scroll while footer is accessible.

### M3-B — Editor interaction and resize regression

**Implement**
- Exercise Select, Add Snippet, Add Clipboard Item, Add Temporary Text, Move Up/Down, Remove and editing inside the bounded viewport; newly added or moved selection must remain discoverable without jumping to the wrong row.
- Verify wheel routing within steps vs. outside dialog, keyboard tab navigation, rapid repeated editor changes, resize small/large, and opening/closing/reopening. Preserve draft contents and saved sequences.
- Keep preview/Run Once/Save/Next/Cancel buttons usable when the list is full; update only directly relevant UI help if needed.

**Verification:** One focused editor GUI-state test selection; no full Nextest or visual marathon.  
**Commit:** `test(paste): [M3-B] retain step editing in bounded viewport`.  
**Acceptance:** Correct row editing and stable scrolling with a large sequence.

---

## M4 — Configurable Paste Sequence hotkeys and independent saving

**Required behavior:** Next Step/Cancel Sequence shortcuts are user-editable in main Settings and accessible from the Paste Sequence editor. Optional saved-sequence start hotkeys continue to work. Existing launcher/radial/MkMacro/Screen Draw hotkeys cannot co-fire with applied paste shortcuts. A conflict with a built-in playback shortcut must never force a user to change the launcher hotkey merely to save *content*.

### M4-A — Persist validated optional playback bindings and migrate existing settings

**Implement**
- Add a small Paste Sequence preferences model through the existing Settings serde-default model/validation. Fields: Next shortcut (optional), Cancel shortcut (optional), floating tooltip enabled (default true) and Esc cancellation enabled (default true). Keep values separate from per-sequence start binding.
- Migration: old Settings JSON without the new block loads; old `paste_sequences.json` and existing `Automatic`/`ManualAdvance` values remain intact. No implicit disk rewrite just from loading. Do not store tooltip position per step, clipboard contents, focus target or active playback state.
- Defaults: preserve familiar Next (`Ctrl+Alt+Shift+PageDown`) and give Cancel a sensible conflict-aware default (the previously hardcoded `Ctrl+Alt+Shift+End` can collide with the configured launcher on End). Do **not** mutate the user's launcher shortcut or silently replace a saved custom shortcut. If a default conflicts in an existing profile, mark that optional playback route inactive with clear guidance until the user changes or disables it; content save remains possible.
- Treat empty/unset shortcuts as **disabled routes**, not invalid hotkeys; validate supported syntax on specified routes.

**Verification:** Focused settings serde/default/migration/test of custom, absent and invalid binding.  
**Commit:** `feat(paste): [M4-A] persist configurable playback controls`.  
**Acceptance:** Old user settings/paste documents round-trip without losing unrelated fields or sequences.

### M4-B — Main Settings section, editor affordance and correct Save/Cancel

**Implement**
- Add **Paste Sequences** section to the existing main Settings dialog, grouped with shortcuts/input behavior. Show current Next and Cancel values, enable/disable controls and clear conflict/syntax messages; include tooltip and Esc toggles. Use the established draft/Save/Cancel transaction, not immediate unmanaged writes.
- Add a small Settings/gear action in Paste Sequences that opens the main Settings dialog directly to that section (or scrolls/highlights it). Keep the sequence editor's `Optional hotkey` scoped strictly to the selected saved sequence's **start** shortcut.
- Any current settings edit is staged; Cancel leaves persisted bindings and runtime routes unchanged. A successful Save updates the displayed settings and in-app help/shortcut labels immediately without restart.
- Do not add unnecessary extra windows, a new global hotkey manager or a second JSON store.

**Verification:** Focused settings editor state test: edit, disable, cancel, commit, reopen; direct navigation from Paste dialog.  
**Commit:** `feat(paste): [M4-B] expose Paste Sequence shortcut settings`.  
**Acceptance:** Clear discoverability and intact Save/Cancel behavior.

### M4-C — Hotkey conflict validation and atomic route replacement

**Implement**
- Validate configured Next/Cancel and per-sequence Start bindings against the active launcher, quit/help, Screen Draw, radial global, MkMacro and other Paste Sequence routes. Reuse actual shared/fallback hook matching policy, including **co-fire with same primary key but different modifiers**; don't use string equality alone.
- Treat Esc's runtime-only cancellation as a separately scoped active-session route, not a globally reserved launcher chord while idle. Treat physical Ctrl+V's session-only interception likewise; do not globally reserve Ctrl+V permanently.
- Prepare/activate candidate hotkey routes using the existing main-owned hotkey transaction and generation fences. Reject a prospective conflicting route without replacing the previous valid active routes. If some optional defaults conflict, disable/omit those optional routes explicitly and surface the reason; don't make all save operations fail closed.
- Ensure the same candidate validation rules apply at startup, settings Apply, per-sequence route edits, radial toggles/plugin enablement and reloads. No simultaneous old/new callbacks, no orphan hooks, no duplicate registrations.

**Verification:** Focused real-validator tests with launcher `Shift+Alt+Win+End` vs old Paste Cancel `Ctrl+Alt+Shift+End`, radial/MkMacro conflicts, invalid syntax, disabled shortcut, Save rollback, shared/fallback modes.  
**Commit:** `fix(paste): [M4-C] validate and atomically rebind playback hotkeys`.  
**Acceptance:** Current launcher shortcut untouched; no overlapping live routes; old valid hotkeys retained on rejected new settings.

### M4-D — Decouple sequence-content persistence from shortcut activation

**Implement**
- Current `Save` goes through `commit_sequence_candidate_with_route_update` even when the optional start field is empty; restructure so ordinary name/step/mode/delay edits save atomically through the sequence store **without requiring a successful reconfiguration of unrelated global Next/Cancel routes**.
- Distinguish **content save succeeded** from **optional start shortcut binding accepted** and **global playback settings applied**. An invalid/conflicting new per-sequence shortcut must not save an unsafe binding, but must not silently discard valid authored content. Provide a clear partial-success indicator and keep prior valid binding (or disabled binding) coherent.
- Preserve revision-based atomic store updates, stable IDs, concurrent edit detection and no overwrite of another writer's data. When an explicit shortcut update fails, preserve the prior route and avoid rolling back unrelated saved text; if the previous route becomes stale, report it and fail safe.
- Verify both GUI Save and Remove Saved behaviors, including no redundant hotkey transactions for content-only changes.

**Verification:** Focused content-only save/no-hotkey and saved-start conflict tests; mixed rollback and concurrent-save cases.  
**Commit:** `fix(paste): [M4-D] save sequence content independently of global chords`.  
**Acceptance:** Save succeeds with no optional hotkey even if an unrelated default playback shortcut conflicts with launcher.

### M4-E — Shortcuts documentation and focused integration

**Implement**
- Update Quick Help and README to show configured (not hardcoded) Next/Cancel chords, disabled indicators, per-sequence starts, and the tooltip/Esc toggles (actual tooltip appears in M5).
- Verify startup/reload, Settings Save/Cancel, plugin enable/disable, no duplicated Next events, and existing hotkey toggles. Keep graceful errors if active shortcut changes cannot be applied.

**Verification:** One focused main route/settings integration slice and diff review.  
**Commit:** `test(paste): [M4-E] prove configured routes and content-only saves`.  
**Acceptance:** Users can change Next and Cancel without modifying launcher hotkey; conflicts explained before activation.

---
## M5 — Target-independent Paste-to-Advance, Esc and floating progress tooltip

**This is the highest-risk milestone.** Maintain a single authoritative playback controller, a single session-scoped physical hotkey owner and guarded native clipboard/input boundary. Do **not** treat `Ctrl+V` as permission to hijack all system paste actions outside this mode. No pasted step may be skipped, duplicated, routed to a stale window or unintentionally written over external clipboard changes.

### M5-A — Extend the typed playback contract and target policy

**Implement**
- Add backward-compatible `PlaybackMode::PasteToAdvance` (`paste_to_advance` persisted spelling or the verified serde equivalent). Existing `Automatic` and `ManualAdvance` sequences and defaults must deserialize and behave as before except the specifically requested removal of fixed-target confinement.
- Separate immutable **invocation context** (origin timestamp, snippet system variables and prompted values resolved before execution) from mutable **per-step destination**. A formerly captured external window may assist initial activation but is **never** a binding that forces subsequent steps into that same HWND. Allow a run to await the user's first eligible external target even if launched from the editor without a captured destination; show actionable pending guidance rather than `No external destination captured` as a terminal failure.
- For **all three playback modes**, resolve/check the *current* eligible foreground target just before each paste. Switching from application A to B between steps is legal. Never automatically pull focus back to A on later steps. If focus is temporarily Multi Launcher, a secure desktop, an unknown/invalid HWND or a known password/protected control, do not paste; wait for an eligible target or fail safely according to bounded policy, without consuming the next step.
- Keep existing preflight atomic: prompted snippets and dynamic variables rendered exactly once before Step 1, missing refs fail with zero pastes, hidden snippets masked, Unicode/multiline exact, no accidental re-reading of `sys.window_title` as the user changes target windows.
- Record a per-run generation token and explicit next-step/last-step ownership. Maintain old completed/error/cancel feedback for automatic and manual runs.

**Verification:** Pure model/store migration and fake-target playback tests covering A→B→A switch, unavailable target, launcher/self rejection, stale focus at paste boundary, and old-mode snapshots.  
**Commit:** `feat(paste): [M5-A] separate playback steps from original window`.  
**Acceptance:** Cross-application pasting is supported in all modes; old serialized sequences remain valid.

### M5-B — Admission for physical Ctrl+V, one press = one step

**Implement**
- Trace the current main-owned Windows keyboard-hook/hotkey event path and coordinate with launcher/radial/gesture/MkMacro ownership. While a **started Paste-to-Advance session** is armed, intercept physical Ctrl+V before Windows delivers it to the target, emit one typed action containing a generation/focus snapshot, and return immediately from the hook; perform clipboard IO/input injection asynchronously through the existing app-owned playback boundary.
- Suppress only the appropriate V-down/V-up transaction; avoid intercepting Ctrl alone, Ctrl+Shift+V, copy/cut, AltGr, unrelated shortcuts or keyboard input when no active Paste-to-Advance run exists. Track key state/debounce: holding V/auto-repeat produces **one** step, and a genuine fresh release/down may produce the next. Ignore all injected/owned replay events and do not recursively trigger the route.
- Hotkey observation/arming must change atomically at Start/Cancel/Complete and plugin disable/shutdown. Preserve existing launcher hotkey, radial hold-to-open, mouse gesture and MkMacro recorder policy. Do not install a second global hook unless direct inspection proves reuse impossible; justify any new native observer as an isolated, single-owner unavoidable change.
- Do not block `WH_KEYBOARD_LL` callbacks on disk, clipboard locks, UI redraws or worker completion. Handle early press before ready state, lost event queue, route delivery errors and ambiguous pressed modifiers without emitting a duplicate paste.

**Verification:** Deterministic physical/injected/auto-repeat/AltGr/key-up/focus-shift event tests using the actual hook reducer/route abstraction; one narrow main-dispatch integration slice.  
**Commit:** `feat(paste): [M5-B] claim Ctrl+V only for active user playback`.  
**Acceptance:** Ordinary Ctrl+V outside the session is 100% pass-through; an active physical press creates at most one admitted step.

### M5-C — Safe clipboard handoff and exact step advancement

**Implement**
- Implement a **single owner** for clipboard state, with original clipboard snapshot, last owned Windows clipboard sequence number, last published step and a monotonic per-run generation. On Start prepare Step 1 when safe. Prefer a **lazy per-press publication** strategy for later steps: keep current text available long enough for the target's asynchronous paste consumer, and stage Step N+1 only when the subsequent independent physical Ctrl+V is admitted. A tooltip may say "Next: Step N+1" before the next clipboard write actually occurs.
- For each admitted physical press: verify current clipboard ownership + eligible current target; publish the intended step only when safe; recheck ownership/target; inject one standard paste chord with the existing native input path; ignore that injected chord in the physical observer; after the guarded paste/settle boundary, advance the logical step by exactly one. Do not claim a universal OS paste-ack exists. Never change clipboard to Step N+1 immediately after N if that can race delayed clipboard consumers.
- If external copy/clipboard ownership changes at any stage, **do not overwrite the user's newer clipboard**. Stop/suspend with clear tooltip/status and conditional cleanup only of demonstrably still-owned clipboard data. If the intercepted Ctrl+V no longer belongs to the sequence, either safely forward/replay the user's normal paste or clearly fail the event without substituting sequence contents; never do both.
- After Step 3 of a 3-step sequence is safely dispatched, mark Completed, release the input route, **leave Step 3 text in clipboard**, dismiss tooltip, and let the fourth Ctrl+V be the operating system's ordinary paste of Step 3. If an unrelated actor changed the clipboard at completion, never forcibly restore Step 3 over that change.
- On user cancellation/failure, restore the pre-sequence clipboard only if this session still owns the current clipboard and after the necessary safe settling boundary; preserve existing unsupported/non-text clipboard fail-safe handling. On app shutdown, release hooks and clipboard ownership deterministically without waiting on UI thread.
- Support high-frequency Ctrl+V presses with a bounded queued action or explicit busy/retry state; never silently skip to N+2 or paste N twice while reporting N+1. Reject a simultaneous second sequence while one is active.

**Verification:** Fake/native-boundary tests for three steps, delayed clipboard reads, pre-paste clipboard changes, post-publish changes, first press, rapid press/repeat, clipboard locks/failures, conditional restoration, cancellation and final retained Step 3; no actual user clipboard in tests.  
**Commit:** `feat(paste): [M5-C] sequence clipboard writes per physical paste`.  
**Acceptance:** Ctrl+V maps deterministically to Step 1/2/3; 4th ordinary paste repeats Step 3; no external clipboard clobber.

### M5-D — User-directed Automatic and Manual Advance interoperability

**Implement**
- Adapt Automatic mode: it may paste successive steps wherever the user actually focuses at each scheduled input boundary; never reinstate an originating HWND requirement. Do not paste into the launcher on automatic timer wakeups. If focus is not eligible, wait (with finite bounded failure/cancel policy) or stop without skipping a step. Scheduling still respects configured delay.
- Adapt Manual Advance: existing configurable Next hotkey/Next button triggers **one** paste into the current eligible foreground window each time, with no automatic redirection to the original app. The chosen foreground window can change between presses. User intent on screen should prevail, not the editor's stale captured destination.
- Keep run ownership and state transitions coherent if user opens settings/launcher while a sequence is running, cancels and restarts, or switches between target apps. Preserve input hooks and clipboard cleanup of the existing two modes.
- Confirm `Run Once` memory-only semantics, Saved sequence invocation, preflight for prompted/hidden/dynamic `cs`, and history refs. No new scripting semantics.

**Verification:** Focused controller/app integration tests for auto A→B, manual A→B, cancellation on invalid/launcher target, rapid stop/restart, prompt followed by target switch.  
**Commit:** `fix(paste): [M5-D] enable user-driven targets in every mode`.  
**Acceptance:** Neither Automatic nor Manual Advance rejects a legitimate new target solely due to HWND/PID difference.

### M5-E — Active-session Esc cancellation and existing Cancel shortcut

**Implement**
- Add a session-scoped, setting-controlled Escape-to-Cancel route, **enabled by default**, active only while a sequence has been started and remains pending/running/awaiting. The user may switch among applications; Escape must not require the original target. During a preflight prompt or unrelated UI editing without an active started run, maintain the existing Escape behavior of that dialog.
- On one physical Escape press, signal cancellation on the owned controller, consume only the keys that belong to that admitted Cancel gesture, disarm Ctrl+V/other session-only interceptors, hide the tooltip, stop later steps and conditionally restore clipboard after safe settling. Avoid OS-wide Escape interception when no sequence is active.
- The configured global Cancel shortcut and in-editor Cancel Run remain usable regardless of Esc setting. If Esc cannot safely be admitted (secure desktop, state uncertainty), don't intercept it; keep the configured Cancel route as an escape path.
- Cover cancellation between two target apps, while a worker is pending, during an active paste settle interval, while the main launcher is hidden/shown, and on shutdown.

**Verification:** Focused low-level route/controller tests for Esc while running, idle Esc pass-through, input owner release, no following step after cancel, clipboard external actor, and cancellation during focus change.  
**Commit:** `feat(paste): [M5-E] add session-only Escape cancellation`.  
**Acceptance:** Reliable user escape without global keyboard interference when idle.

### M5-F — Cursor-following floating status tooltip for all modes

**Implement**
- Build a small, passive native/egui overlay (reuse existing tooltip, mouse coordinate and monitor helpers where fitting): non-activating, always mouse-pass-through, no keyboard focus, no forced launcher/root visibility or new permanent window. Show only while the sequence is active/prepared-as-started; hide immediately on completed/cancelled/failed/shutdown or plugin disabled. No tooltip when merely editing an unsaved draft.
- Place it near the physical cursor with a configurable-safe offset, flip/clamp to the containing/nearest monitor, account for signed virtual desktop coordinates, multiple DPI scales and cursor movement. Never cover the pointer click target; do not steal click, text selection, focus or topmost position from other native overlays.
- Show concise metadata: sequence name (bounded/truncated; no secrets), **current or next** step number/total, playback mode/next action (`Ctrl+V to paste`, `Next` or `Automatic`), and actual configured Cancel hotkey/`Esc`. Include useful waiting/clipboard-lost/busy statuses without leaking text. Explicitly **never show hidden snippet contents, actual step payloads, clipboard text, user-window titles or secret prompt values**.
- Tooltip enable/disable is in M4 settings; update live without restart. Repaint only when state, timer, cursor location or animation needs it; no persistent 16 ms paint loop while idle. Dismissing overlay must release native HWND/texture resource and leave other pinboard/mouse overlays unchanged.
- If inaccessible overlay desktop/native presenter, degrade to normal dialog status without starting a second overlapping overlay service; report environment limits.

**Verification:** Focused egui/native presentation model tests for modes/status text/hidden masking, cursor-follow/clamp/negative screens, DPI changes, focus/pass-through, complete/cancel lifecycle and zero idle frames.  
**Commit:** `feat(paste): [M5-F] show cursor-following sequence progress`.  
**Acceptance:** Tooltip tracks cursor, does not intercept input, and vanishes on completion/cancellation.

### M5-G — Editor mode selection, controls and live state consistency

**Implement**
- Add the third **Paste-to-Advance (Ctrl+V)** choice alongside Automatic/Manual Advance with a short in-dialog explanation: one real Ctrl+V per step; after final completion the last item remains on clipboard; pressing Esc/configured Cancel ends early.
- Update Preview/Run/Run Once, saved-sequence selection/editing, optional start hotkeys and status strings to support the third enum variant. Display meaningful Ready/Waiting for Ctrl+V/Running/Completed/Cancelled/Clipboard changed statuses without stale step counts. Do not show or reframe it as bound to the initially captured destination.
- Rename existing misleading "Captured destination" or "No external destination captured" copy to distinguish frozen invocation **context** from the freely chosen per-paste **target**. Avoid start failure due solely to absent foreground capture; prompt the user to focus an app when needed. Keep hidden snippet preview masking intact.
- Ensure long step list scrolling introduced by M3 remains bounded, and the tooltip does not prevent editor Close/Save/Cancel operations. Preserve disabled plugin behavior and hotkey route cleanup.

**Verification:** One focused editor+state presentation suite: round-trip new mode, old mode backwards compatibility, unsaved run, mode switch, status/preview masking, cross-app labels and settings shortcut.  
**Commit:** `feat(paste): [M5-G] expose paste-to-advance in sequence editor`.  
**Acceptance:** Users discover and activate the mode without new standalone configuration tools.

### M5-H — Cross-application acceptance and lifecycle proof

**Implement**
- Add deterministic end-to-end fake/owned target tests: authored 3-step sequence; physical Ctrl+V pastes Step 1 in app A, focus B, Ctrl+V pastes Step 2 in B, Ctrl+V pastes Step 3 in B, completes and disarms route; fourth Ctrl+V is ordinary paste of Step 3. Prove no accidental app A reactivation and no duplication if held/repeated Ctrl+V.
- Include already-prompted `cs` and temporary text steps, clipboard-history references, hidden source handling, fast consecutive presses, external Ctrl+C between steps, Escape and configurable Cancel, and clipboard ownership checks before/after cancellation. Explicitly check old Automatic/Manual behavior now follows current target.
- If native interactive desktop is available, use **only** test-owned text input windows and clipboard fixtures, with no real user data, to demonstrate cross-app focus and cursor-following tooltip. One smoke session/1–3 screenshots maximum; otherwise fake Windows input/clipboard or egui model proof marked `not_run_environment`.
- Run tests for hotkey route precedence against launcher/radial/MkMacro and check no orphan injected input/hook/tooltip workers. No broad Nextest here.

**Verification:** Two carefully chosen focused slices covering model/native adapter and app route; owned native smoke if available; no performance campaign.  
**Commit:** `test(paste): [M5-H] verify cross-window paste, Esc and final clipboard`.  
**Acceptance:** All required three-paste + fourth-repeat results, A↔B switching and clipboard safety established by actual tests with evidence limitations honestly recorded.

---

## M6 — Search and Replace compact, collapsible and scrollable dialog

**Required behavior:** The entire **Search and Replace** dialog has a user-scrollable body with bounded sections, even at 600×420 logical points; search controls remain reachable, results remain legible, and primary actions remain visible while scrolling. Existing search snapshots, match approvals, original/proposed/replacement candidate, guarded Apply and durable recovery are unchanged.

### M6-A — Establish explicit dialog scroll ownership and fixed action footer

**Implement**
- Replace the current `.vscroll(false)` unbounded content arrangement with an outer bounded vertical scrolling region inside the existing resizable `egui::Window`; allocate available height after reserving compact header/status/footer instead of letting content request unlimited height.
- Keep the explicit root path and **Search / Refresh** discoverable without enlarging the window. Keep **Review Apply…**, Close, active Cancel/operation status and entry to Restore/Recovery accessible in a compact footer or always-reachable action area, without duplicating mutation actions across egui passes.
- Use stable scroll IDs and content/viewport geometry. Avoid uncontrolled nested `ScrollArea`s, focus theft on relayout, clipping inaccessible bottom controls, repeated per-frame recomputation or changed action submission timing.
- Test small (600×420), typical (1120×820) and large windows; ensure content scroll reaches every control and footer stays visible on the shortest viewport.

**Verification:** Focused egui synthetic-layout test with screenshot only if native is available.  
**Commit:** `refactor(replace): [M6-A] bound main dialog scrolling and action footer`.  
**Acceptance:** No need to enlarge dialog; always possible to reach every editor/recovery action.

### M6-B — Collapsible search configuration and advanced filters

**Implement**
- Keep essential **Search for**, pattern mode, case sensitivity, **Replace with**, replacement mode and root selection easy to access. Make the main Search/Replace settings block collapsible if user wants to maximize results; by default basic fields visible.
- Group **Extensions**, **Include globs**, **Exclude paths**, and **Exclude globs** in an **Advanced Filters** CollapsingHeader, **collapsed by default**. Use labels and multiline/path editors as currently supported; re-opening must show unchanged draft values and disabled/enabled behavior.
- Preserve precisely when field changes invalidate search A and when replacement-only changes recompute B candidate. Folding/unfolding or scrolling must not trigger a new search, reset current matches or clear confirmation/recovery data.
- Maintain focus/tab order and show a concise advanced-filter-active indicator when collapsed to prevent accidentally overlooking active exclusions.

**Verification:** One focused dialog state test for collapsing advanced options, edit, reopen, invalidation counters and unchanged source search results.  
**Commit:** `feat(replace): [M6-B] collapse search settings and advanced filters`.  
**Acceptance:** Advanced controls don't dominate vertical space; filters remain functional.

### M6-C — Independent bounded File Results and Approvals sections

**Implement**
- Wrap **Files with Matches** in a collapsible, **expanded-by-default** section, with a clear count, selected count and current file. Its file result table gets a bounded independent vertical scroll with stable path/filter/sort selection.
- Wrap **File Approvals** in another collapsible section which becomes relevant when a result file is selected; ensure partial file-level selection, per-match checkbox state, selected-match inspector and line/byte metadata are preserved across collapse, filtering/sorting and scroll.
- Use adaptive maximum heights based on available dialog budget, not a fixed large minimum that defeats the outer scroll. Reuse existing virtualized `show_rows` models and stable match IDs. Only the visible rows should be laid out when thousands of matches exist.
- Do not conflate visual filtering with approval: collapsing/scrolling cannot add or remove approved matches. Existing per-file select/clear visible behaviors still apply exactly to intended visible matches.

**Verification:** Focused egui state/virtual row tests with hundreds of matches, re-sorting and fold/unfold, verifying exact selected IDs and no hidden changes.  
**Commit:** `feat(replace): [M6-C] bound file and match approval panels`.  
**Acceptance:** Large result lists remain usable and can be fully hidden without losing selections.

### M6-D — Collapsible Original/Proposed and diff inspector with bounded scrolling

**Implement**
- Group the original-versus-proposed contextual inspector and bounded unified file diff under a clearly named collapsible **Original vs. Proposed Preview** area. Keep this area expandable when reviewing a file; otherwise it may begin folded until file selection.
- Give **Original** and **Proposed** separate bounded vertical scroll regions; enable sensible horizontal scrolling or explicit no-wrap handling for long lines, retaining Unicode/CRLF representation and monospace readability. Unified diff gets its own bounded scroll and truncation disclosures.
- Selecting a distant/multiline match must still open its exact captured source/proposed text. Preserve existing bounded background diff preparation and stable candidate generation, not re-run expensive search on every frame or folding click.
- Handle source revision invalidation, no match selected, changing inspected file, and errors without stale old-file content.

**Verification:** Focused UI test for far-apart/multiline match, long lines, independent scrollers, collapse/reopen preservation, stale candidate cleanup.  
**Commit:** `feat(replace): [M6-D] make original and proposed previews collapsible`.  
**Acceptance:** Diff content is readable but never forces dialog beyond screen.

### M6-E — Action reachability and safety regression

**Implement**
- Ensure **Search / Refresh**, Cancel Search, **Review Apply**, confirmation, Cancel Apply, Restore and Recovery are always accessible with scrollable body/compact footer; recovery receipts can remain in a scrollable collapsible panel reached from footer.
- Revalidate eframe pass ownership so button intent fires once even with more nested panels; preserve current A→B→C worker transitions, stale-file checks, selected match IDs, confirmations, backups and durable receipts. No mutation on collapse/re-render/scroll.
- Check keyboard focus/tab order, mouse wheel routing, close/reopen and window resizing at minimum dimensions. Preserve disabled/active worker states and the exact existing plugin command `replace`.
- Update help/screenshots only for changed navigation. Do not refactor backend file IO.

**Verification:** One UI/confirmation test selection plus one directly related disposable tempdir replace/Restore test if UI event routing changed; no full suite.  
**Commit:** `test(replace): [M6-E] preserve approvals, Apply and recovery under folding`.  
**Acceptance:** Same exact files/bytes altered as before, with every action reachable and no unwanted vertical growth.

---

## M7 — Build only the launcher executable by default

**Required behavior:** `cargo build` and `cargo build --release` build one root application binary target: `multi_launcher`. The three smoke/acceptance executables are still present and can be compiled explicitly by developers. Dependencies, build scripts, benches and tests remain normal Cargo behavior.

### M7-A — Control Cargo binary discovery and declare opt-in developer binaries

**Implement**
- Inspect current target graph with `cargo metadata --no-deps --format-version 1`, confirm top-level `src/bin/*.rs` entry points. Do not delete the smoke sources or refactor enormous `radial_acceptance.rs` just to change build inclusion.
- Set manifest binary discovery appropriately (e.g. `autobins = false`) and declare explicit `[[bin]]` entries with paths for `multi_launcher` and the three named smoke targets. Introduce an **off-by-default** developer-only feature such as `dev-smoke-binaries` and put it in `required-features` on *only* the three smoke targets.
- Preserve `cargo run` default for `multi_launcher` (use `default-run` only if it solves observed ambiguity; don't mistake `default-run` for controlling `cargo build`). Confirm implicit root lib/test/bench targets still work. Preserve vendor patches/lockfile unless Cargo genuinely requires minimal updates.
- Document explicit opt-in commands: `cargo build --features dev-smoke-binaries --bin coordinate_tool_smoke`, same for `passive_overlay_smoke` and `radial_acceptance`; `cargo run --features dev-smoke-binaries --bin ...` when necessary.

**Verification:** `cargo metadata --no-deps` target-feature inspection, one ordinary `cargo check --bin multi_launcher` (or minimal `cargo build` if build exclusion can only be proved that way). Avoid building all smoke binaries during ordinary checks.  
**Commit:** `build(cargo): [M7-A] make smoke binaries opt-in targets`.  
**Acceptance:** Plain Cargo Build has exactly one application bin target; source files retained.

### M7-B — Verify default versus explicit target contracts

**Implement**
- Confirm `cargo build` and `cargo build --release` target selection without running redundant full release builds if Cargo metadata and focused target checks provide clear evidence; run the actual requested build form if needed for certainty, once per form maximum within reasonable environment budget.
- Confirm each smoke executable remains explicitly addressable via its developer feature using metadata and one selected smoke `cargo check --features dev-smoke-binaries --bin ...`; if any named bin has an unusual required feature/dependency, preserve it and document exact syntax.
- Preserve normal tests/benchmarks and `cargo nextest` discoverability. Do not change source of the three binaries except a directly necessary target declaration fix.

**Verification:** Manifest/target inspection and the narrowest necessary explicit check; no full Nextest.  
**Commit:** `test(cargo): [M7-B] verify default and opt-in binary graph` only if assertions/docs/tests changed; otherwise record verified result in M7-A ledger entry and avoid an artificial commit.  
**Acceptance:** Release and debug default builds exclude three smoke executables; dev targets remain available on demand.

---

## M8 — Integrated closeout: focused integration, one full Nextest, then Clippy last

### M8-A — Direct cross-feature regression, code cleanup and evidence

**Implement**
- Review accumulated changes and repository status. Re-run **one carefully chosen focused integration selection** across shared launcher command routing, Settings save/hotkeys, Paste Sequence session lifecycle, pinboard capture, Search and Replace guarded dialog and Cargo target registration. Reuse already passing checkpoint-specific proofs instead of building a giant intermediate matrix.
- Perform code-only cleanup strictly where new work left duplicate helpers, dead scope plumbing, accidental format churn, stale UI state, bad cancellation paths or unintended input interception. In particular, verify all native hooks/overlay HWNDs unregister on Cancel/Complete/Exit, and no background frame/clipboard timers remain active when idle.
- Complete `cargo fmt` and formatting check for project-owned touched files, ordinary targeted compiler/build checks, and README/in-app help (unified launcher queries, configured Paste hotkeys, cross-app Paste-to-Advance, tooltip/Esc, safe pin selection, Search and Replace folding, opt-in smoke binaries). Do **not** modify or format vendored crates.
- Collect a bounded set of screenshots when owned desktop automation works: Paste step-list and settings, Paste-to-Advance tooltip across two owned windows, screenshot-picking shield, Search and Replace compact/folded view. Record missing GUI evidence honestly; no user request or repeated app-approval investigation.

**Verification:** At most two directly connected focused integration slices; one `cargo check --bin multi_launcher` if needed; formatting and diff/status checks.  
**Commit:** `chore(refinement): [M8-A] integrate and document feature changes` (possibly split coherent code/docs commits if warranted).  
**Acceptance:** All feature checkpoints complete or honestly blocked; formatting and ordinary code changes finished **before full Nextest**.

### M8-B — One final full Nextest run

**Implement**
- Run a single full repository test pass using the current stable Windows toolchain and verified package graph, typically `cargo nextest run --workspace` (do not use `--all-features` unless needed and justified; that would intentionally enable dev-only smoke targets).
- Log test totals, errors, duration and meaningful skipped/platform-limited tests. If there is a real failure, inspect first independent root cause; fix the narrowest correct code/fixture, run its **focused** test, and permit **at most one** whole-suite rerun to confirm repair. Do not iterate indefinitely, broaden to performance benchmarks, or hide failures by weakening assertions.
- If any mandatory input-safety behavior cannot be proven or repaired within bounded attempts, mark its checkpoint `blocked` and report why; do not label it complete. Environmental limitations such as unavailable native GUI must be clearly distinguished from failing domain/integration tests.

**Verification:** This is the plan's **only scheduled complete Nextest run**.  
**Commit:** `test(refinement): [M8-B] resolve final integration regressions` only if actual files changed; otherwise ledger-only update attached to final docs.  
**Acceptance:** Full Nextest passed or precise unresolved failure recorded after bounded retry. Clippy not yet run.

### M8-C — Close ledger and finish every non-lint commit

**Implement**
- Finalize README/help and the execution ledger: checkpoint statuses and hash index; chosen user-facing shortcuts, app-independent target rules, current clipboard behavior and first/second/third/fourth paste example; screenshot limitations and full Nextest outcome; remaining genuine issues only.
- Check the changed-file inventory, `git status` and `git diff --check`. Commit documentation/ledger and any non-lint fixes; do not start new tests, UI tours, feature additions or code formatting after this checkpoint except narrow correctness requirements of final Clippy fixes.
- Record that patched vendored crates are dependencies, not automatically workspace members. Determine exact maintained Cargo targets for the final lint pass.

**Verification:** Doc/ledger/diff inspection, not another test sweep or Clippy.  
**Commit:** `docs(refinement): [M8-C] finalize user help and checkpoint ledger` if actual docs changed.  
**Acceptance:** All non-lint source work, ordinary tests, formatting, screenshots and documentation are complete.

### M8-D — Final maintained-crate Clippy cleanup and last verification

**Implement**
- **Only now** inspect current Cargo workspace membership (`cargo metadata --no-deps`) and run the maintained-application Clippy gate. Standard command if only `multi_launcher` is a workspace member:

```powershell
cargo clippy --workspace --all-targets -- -D warnings
```

- If actual vendored packages are workspace members, add appropriate verified `--exclude <vendored-package-name>` flags before `--`; do not lint vendored dependencies as independent targets or change vendor source. Keep supported Windows target/features; avoid enabling `dev-smoke-binaries` merely for the default lint pass unless a specifically maintained target must be checked separately before final command.
- Clean **existing and new maintained-code** Clippy warnings, including warnings caused by new manifest target selections, without broad refactoring or blanket `allow` annotations. Group findings by class, make at most two meaningful correction attempts for the same unchanged class, and commit focused fixes with `chore(clippy): [M8-D] clean maintained code lints` (additional small labeled fixes only if necessary). If a lint change affects behavior, run one focused correctness test and `cargo fmt` **before** the final Clippy rerun.
- Run the **final successful Clippy pass last**. After success do not rerun tests, reformat, edit code, create additional validation plans or rewrite documentation. If a genuine maintained-code diagnostic cannot be cleared within bounded attempts, record it in final user report as blocked—not as a false success. Vendor-only lints are ignored.

**Verification:** Final clean Clippy (`-D warnings`) on maintained targets; no work afterward except reporting.  
**Commit:** lint cleanup before the last verification pass.  
**Acceptance:** Final quality report accurate, vendor exclusions correct, no post-Clippy development work.

---

## 5. Focused acceptance matrix (tests should map to changed behavior, not an exhaustive new suite)

| Change | Direct proof | Important negative case |
|---|---|---|
| Scope removal | Original All ordering + explicit prefix/dashboard/radial read-only route | Ctrl+1–5 no longer modify search; no invisible scopes |
| Pin capture input shielding | Drag over disposable text/button target is absorbed, selection returns correct rectangle | No Down/Move/Up leakage or lingering shield after Esc |
| Paste step list | 1/50/100 steps, multiline, independent scroll and stable footer | No dialog growth, no item loss/reordered wrong row |
| Paste hotkeys | Edit/Save/Cancel Next/Cancel settings; omit conflicts; content-only save | Existing launcher End chord cannot block saving without custom hotkey |
| Cross-app paste | Step 1 in A, Step 2/3 in B, Ctrl+V fourth repeats Step 3 | No stale-window refocus or paste into launcher/protected controls |
| Ctrl+V physical route | Genuine press→one step, repeat/injected ignored; arm/disarm | Normal Ctrl+V outside run untouched; no hook deadlock |
| Clipboard ownership | Press-driven lazy publication, external copy, conditional restore | Never clobber user's newer clipboard; no next-step early overwrite |
| Esc/tooltip | Passive cursor-follow on all modes, global session-only Esc stop | No focus theft, clipboard leak, tooltip persistence or idle Escape hook |
| Replace UI | Small/large viewport; collapsed advanced/results/approval/diffs; pinned footer | Fold/scroll does not mutate selection, candidate or files |
| Cargo targets | `cargo build`/`cargo build --release` build only `multi_launcher` bin | Opt-in `coordinate_tool_smoke`, `passive_overlay_smoke`, `radial_acceptance` still addressable |

## 6. Hard stop conditions and truthful reporting

- **Input safety:** An unhandled click reaches underlying screen during pin capture; a physical Ctrl+V causes duplicate input; an injected paste re-enters the mode; a native event is consumed while no sequence is active; a paste goes into an unexpected or protected window. These are correctness defects, not optional UX tweaks.
- **Clipboard safety:** Newer user clipboard content is overwritten; final-step retention incorrectly restores the old clipboard; cancellation assumes ownership using text equality alone; the code claims universal target paste acknowledgement. These block M5 completion.
- **Data safety:** Scope removal deletes/fails to return genuine provider actions; File Replace changes files during read-only previews; a UI fold clears approval and enables unreviewed writes; settings save silently changes launcher hotkey; Cargo permanently loses developer smoke targets. These block affected checkpoints.
- **Bounded investigation:** For identical failures with unchanged evidence, at most two substantive correction attempts and one focused recheck per correction; stop, preserve diff/evidence, mark blocked rather than spiraling. Continue only unrelated isolated checkpoints. Native GUI access uses one diagnostic + one retry then `not_run_environment`.
- **No background promises:** Run autonomously only while the Codex session is active; do not ask the user to validate before finishing or schedule work outside the session.
- **Repository hygiene:** Do not force-push, reset away user edits, alter vendor source to silence lints, rewrite the settings subsystem, replace plugin search architecture wholesale, rename unrelated UI, or introduce app-workspace features.

## 7. Final user-facing handoff format

On completion report: current branch/HEAD and checkpoint commits; implemented behavior in each milestone; exact Paste Sequence settings and `Ctrl+V` UX; scope/search and pin capture safety; Search and Replace layout; default/opt-in Cargo commands; focused test and full Nextest outcomes; exact final Clippy command/result and vendor exclusions; any owned native screenshot evidence and explicit environment gaps; genuine unresolved failures only. Do not ask user to perform tests until the final optional review.

**End instruction to Codex:** Implement this approved work without a new requirements debate. Code first, small coherent checkpoint commits, narrow tests as you go, one full Nextest near the end, then final Clippy last. Preserve source truth from your current repository checkout and `AGENTS.md`, and stop after the final report.
