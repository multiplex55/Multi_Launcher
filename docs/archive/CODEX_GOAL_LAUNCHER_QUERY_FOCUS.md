# CODEX GOAL — Launcher Query Focus, Hidden-to-Visible Autofocus, and Notes/Jump Mode Interoperability

**Project:** Multi Launcher — Windows / Rust / eframe-egui 0.36  
**Status:** Approved narrow focus-management correction; autonomous Codex implementation  
**Companion:** `CODEX_KICKSTART_LAUNCHER_QUERY_FOCUS.md`  
**Planning inspection:** October 20, 2026 source snapshot and the supplied functional requirements. Codex's **current checkout** and current `AGENTS.md` govern implementation.  
**Priority:** Fix the two related focus defects without redesigning Jump Mode, Notes, window visibility, or the launcher search subsystem.

## 0. Desired behavior

### A. On launcher show, query receives the initial typing focus

When the configured launcher hotkey (or another explicit *interactive* launcher-show action) transitions the launcher from **hidden to visible**, automatically focus the actual **Main Launcher Query Input**: `egui::TextEdit::singleline` with widget ID **`egui::Id::new("query_input")`**, bound to **`LauncherApp.query`**. The text caret must be ready for the next typed character without a mouse click or Jump Mode. This applies with an empty or nonempty query, dashboard or results view, fixed or mouse-following placement, and Jump Mode enabled or disabled.

**One-shot condition:** This is a *hidden → visible* focus handoff, not an always-on focus enforcer. Do not reclaim focus on later frames, normal repaints, ordinary search result updates, window movement, resize, or unrelated dialog closure. An already-visible launcher hotkey action must preserve its existing semantics rather than unconditionally refocus the query.

**Respect input ownership:** A genuinely modal confirmation, active explicit prompt, native/system modal, screen capture/recording owner, foreground independent viewport, or an explicitly nonactivating `RootFocusIntent::PreserveForeground` must not have focus stolen. An open **non-modal** Notes window that is not actively being edited should not itself defeat the requested autofocus. If a note editor or another intentional input owner is actually active when restoring, preserve that owner's focus according to the existing typed ownership policy; never send keystrokes behind a modal. Recognize **window presence** and **active keyboard ownership** as different facts.

### B. With normal Notes open, query remains deliberately reachable

While **Experimental Jump Mode** is enabled and a **non-modal Quick Notes** dialog is open, the visible unobstructed launcher query remains usable through **mouse click** and an appropriate **Jump Mode hint/focus action**. Explicit focus on `query_input` must allow normal typing, editing, query history/autocomplete, results navigation, and intentional Enter execution of the selected launcher result **without closing Notes first**.

After focusing the query, the user must still be able to click or Jump back to Notes and continue the exact same note editing session/draft. Neither the Notes editor nor the query should repeatedly recapture focus. Notes must remain visible, with selected note, draft and unsaved state unchanged by focus transfer.

**Scope the note terminology to real code:** The source has both the ordinary `"Quick Notes"` `egui::Window` and individually opened **note panels** (`LauncherApp.note_panels`, `open_note_panel`). Codex must check both. Implement the requested safe explicit-query handoff for normal non-modal Notes surfaces displaying the query, without broadening Jump Mode to the full independent note-panel editor. If an independent native viewport cannot coexist with visible root controls, retain its existing foreground protection and document the precise case; do not manufacture a hidden query target.

### C. Safety and compatibility

- **Jump Mode hint completion = focus/select only.** A separate later user keypress executes a button or launcher command; no same-frame Enter leakage or synthetic click.
- A foreground true modal or unsupported/foreign window blocks underlying root navigation. Do not add general ability to navigate every background Dashboard/result/dialog control when a Notes window is on top.
- Outside active modals, *only* the visible, geometrically unobstructed `query_input` and the legitimately focused launcher search/results workflow may cross non-modal Notes ownership, by an explicit guarded handoff. Do not make `any_panel_open()` universally false or change all panels to non-modal.
- Preserve normal Notes Enter/Escape/Tab behavior, Note find/editor shortcuts, open/close, saved files and unpublished edits. Existing Settings, File Search, MkMacro, Paste Sequences, prompt and native picker protections remain in force.
- Preserve Jump Mode default-off setting, Shift+Space leader, hint generation/epoch, deterministic hints and the existing topmost-window/modal fail-closed policy for other controls.
- No user-visible setting, new global hotkey, separate focus polling thread, OS SendInput, synthetic mouse click, new persistence schema or UI redesign.

**Definition of success:** After the launcher first appears it normally accepts typing immediately; with Notes open a deliberate mouse or Jump interaction reliably moves typing to the query; a deliberate return to Notes works; commands can run from the focused query; true modals still prevent background activation.

---

## B0 — Mandatory new-branch and baseline bootstrap (FIRST; before implementation)

**Exact requested default:** Create and switch to a **fresh, uniquely named branch**. Prefer **`fix/launcher-query-focus-jump-notes`**, or a unique descriptive suffix if that exact branch already exists locally or remotely. Do not reuse another ongoing feature branch.

1. Read `AGENTS.md` and this Goal; inspect `git status -sb`, current branch, HEAD, staged/unstaged/untracked files, `git remote -v`, configured upstream and default integration branch. This inspection is read-only, with no feature edits/tests beforehand.
2. Prefer an up-to-date default integration branch (`master`/`main`) as source using safe fetch and fast-forward-only synchronization where possible. Preserve all user work. If the worktree is dirty or moving to the preferred base is unsafe, use a **separate clean Git worktree** at a justified known base if supported; otherwise branch safely from current HEAD while preserving and not staging user changes. Stop and report only if no safe branch transition is possible. **Never reset, auto-stash, force-pull, auto-merge, rebase, discard changes or force-push.**
3. Create and switch to the new branch **before application-code edits**. Initialize a small `docs/plans/launcher_query_focus_execution.md` ledger with original source branch/SHA, new branch name, safety condition and pending M1–M3. Stage only Codex-owned changes. Commit **`chore(plan): [B0] establish launcher query focus baseline`**. A single explicitly labeled empty baseline commit is acceptable if a ledger change is unnecessary; do not manufacture a commit by staging unrelated changes.
4. Record `BRANCH_BASELINE_SHA = git rev-parse HEAD` **after** this initial commit. Attempt **one normal upstream push** (`git push -u origin HEAD` after verifying origin), without bypassing security approval. If denied/offline, record outcome and continue locally. This commit becomes the immutable comparison baseline for the task.
5. **All** M1–M3 work, checkpoint commits, test runs, docs and push attempts occur on this new branch. Do not switch to the previous branch or sync `master` again mid-task.

**B0 acceptance:** new task branch active; first baseline commit exists; base SHA and baseline SHA recorded; initial push attempted; original work left safe. No implementation changes before baseline.

---

## 1. Current codebase findings to retain (verified at planning time)

| Source | Current behavior / implication |
|---|---|
| `src/gui/render.rs::show_root_query_editor` | Builds actual `egui::TextEdit::singleline(query).id(input_id).interactive(interactive)`; `input_id = egui::Id::new("query_input")`. The `interactive` flag is derived from broad root-navigation permission; with Jump Mode enabled and Notes owning navigation, this can disable mouse interaction outright. |
| `src/gui/render.rs::LauncherApp::launcher_query_focus_should_be_requested` and the query render block | Existing `just_became_visible || self.focus_query` predicate is guarded by `file_search_dialog.open`; **actual `query_response.request_focus()` additionally requires `root_navigation_allowed`**. There is already autofocus infrastructure—repair its policy/gate, do not add a second path. |
| `src/gui/render.rs::reconcile_root_visibility` / `begin_root_frame_logic` | Computes a one-frame `just_became_visible` from `!self.last_visible && should_be_visible` under `VisibilityRevision`. Root placement may be fixed or cursor-following. Respect request revisions and real activation, not a generic repaint heuristic. |
| `src/visibility.rs::RootFocusIntent` | Distinguishes `ActivateRoot` versus `PreserveForeground`. Nonactivating restoration intentionally does **not** steal focus from an independent editor; do not replace this with unconditional query autofocus. |
| `src/gui/render.rs::root_navigation_allowed` and `jump_mode_root_scope_for_current_results` | Currently `!jump_mode_enabled() || (owner == Root && !self.any_panel_open())`. Notes is a tracked panel, so enabling Jump Mode gates the root query and other root paths all at once. **Split query editability/explicit focus from ordinary root background navigation.** |
| `src/gui/jump_mode/ui.rs::{NavigationOwner,NavigationWindows,resolve_layer_navigation_owner_parent}` | Retained foreground Middle-layer order and actual focused-response layer determine owner. Quick Notes has its own `NavigationOwner::Notes{instance}`; unknown/true modals block navigation. The registry validates `owns_surface`, allowed interaction/clip, popup provenance and epochs; retain this safety boundary. |
| `src/gui/render.rs` query registration | Existing scope registers `"query_input"` / `"Search query"` as `TargetKind::TextEntry` under `SurfaceId::LauncherResults`. That scope is omitted whenever Notes owns navigation, so Jump Mode cannot currently hint the query behind Notes. Reuse existing registry and model a safe exact query handoff; do not add a second hint engine. |
| `src/gui/render.rs` Enter/action guards | Search result Enter is gated by `root_navigation_allowed` and also an explicit `!self.notes_dialog.open` and `self.note_panels.is_empty()` among other exclusions. **Making the textbox interactive alone will not satisfy “execute commands with Notes open.”** Allow intentional execution only when real query/search ownership is established and no blocking modal or unrelated dialog owns input. |
| `src/gui/notes_dialog.rs::NotesDialog::ui_with_jump_mode` | Normal Quick Notes uses `egui::Window::new("Quick Notes").open(...).resizable(...).show(...)`, not a top-level `.modal(true)` window; has editor TextEdit and note-row targets. Nested templates/context menus are separately identified as unsupported owners. Preserve notes drafts, per-note IDs and confirmation guards. |
| `src/gui/mod.rs::focus_input`, `open_note_panel` | `focus_input()` sets `self.focus_query = true`. Opening/activating a note panel intentionally clears this request and performs a **one-shot note-layer activation handoff**; preserve that explicit priority and permit later user query focus. |
| Relevant existing tests | `jump_mode_notes_dialog_escape_preserves_draft_and_fresh_enter_cancels_edit`, `jump_mode_notes_rows_use_visible_slug_identity_and_fresh_enter_opens_exact_note`, `settings_owner_blocks_root_autofocus_and_enter_activation`, `open_note_activation_hands_off_query_focus_once`, `jump_mode_actual_query_capture_preserves_text_and_survives_discarded_pass`, and Jump Mode owner/cancel/disabled tests in `src/gui/{render.rs,jump_mode/ui.rs}`. |

**Source verification:** Codex must recheck these paths and helpers in its own checkout rather than assuming the planning snapshot is frozen. `AGENTS.md` explicitly requires a focused scope and prohibits duplicate hooks/synthetic input workaround.

---

## 2. Implementation strategy

**Implement the smallest ownership-correct policy change**, not two independent focus patches.

- Distinguish **launcher-show focus intent** (one-shot presentation handoff), **query interactivity** (visible non-modal accessible textbox), **current keyboard/focused editor owner**, **Jump Mode navigation surface**, and **launch-result activation authority**. They overlap but are not synonyms. A broad `root_navigation_allowed` flag currently conflates them.
- Keep a single authoritative decision near existing root render/owner code, with small typed/pure helpers only when tests require them. If a modest focused enum/predicate (e.g. `QueryFocusPermission`) clarifies modal versus nonmodal ownership, use it; don't introduce general background-window activation policy.
- A mouse click on an actually visible query may request normal egui focus; a Jump hint for the query must come from a **narrow, exact non-modal Notes→query focus bridge** into the existing registry and owner transitions, never by making all root targets eligible while Notes remains the active owner. On successful focus handoff, revoke stale note hints, then allow Root query/results ownership *only while that focused session is legitimate*. Clicking Notes returns ownership and invalidates old root hints. Respect topmost/clip/occlusion rather than accepting a target based on open state alone.
- Establish the final owner from real `egui::Response` focus/layer and active modal guards; **never make `NavigationOwner::Blocked` a general fallback to Root**. An explicitly blocked target may not be bypassed by a stale focus request. An unrecognized foreground owner remains blocked.
- The existing query `Enter` activation path can be adjusted to permit an intentionally focused launcher query while Notes is nonmodal, but keep file-search, child confirmation, action-sheet and independent viewport exclusions that truly own input. Do not double-activate, execute when the note editor is focused, or bypass any native action guard.
- Avoid moving the caret to the end on *every* frame: preserve existing text and caret/selection where possible; use existing `move_cursor_end` only in the situations for which it was designed. No `ctx.request_repaint()` loop solely to reacquire focus.

---

## M1 — Correct the hidden → visible focus request (after B0)

### M1-A — Separate transition autofocus from persistent input ownership

**Owner:** `src/gui/render.rs`, `src/visibility.rs` *only if existing focus-intent semantics require a narrowly justified change*, and directly related root-focus tests.  
**Tasks:**

1. Trace global hotkey → visibility revision → `reconcile_root_visibility` → actual first rendered query response, including native foreground `ActivateRoot`, window show/restore, a retained Settings/Notes window, feature gate and the first/discarded egui UI pass.
2. Use existing `just_became_visible` and typed `RootFocusIntent` to request query focus **once when the launcher is newly interactively shown** and there is no active modal, prompt, capture or independent focus owner. **Account for asynchronous native activation:** if the first visible egui pass occurs before the ROOT viewport actually gains input focus, retain only a small visibility-revision-bound pending show-focus intent until the first eligible focused frame; consume it exactly once. Cancel it on hide, newer visibility revision, deliberate user focus transfer, Notes/modal takeover or stale HWND generation. This is a bounded one-shot handoff, **not** a per-frame `request_focus()` loop. Do not consume unrelated programmatic `focus_input()` intents incorrectly.
3. Preserve cached query text, results/selection and cursor state. Ensure no repeated autofocus after a user tabs/clicks into a dashboard control, opens Notes, interacts with a list item, or chooses a Jump target. Prevent a pending show intent from firing late after a dialog takes input.
4. Keep explicit preserve-foreground/nonactivating restores unchanged; never force Windows HWND activation merely to fix egui TextEdit focus. Fixed-position/follow-mouse placement follows the same query-focus policy.

**Verify:** targeted `render.rs` egui focus lifecycle tests: empty dashboard and nonempty query; Jump Mode enabled/disabled; hidden→visible once, next steady-state frames, subsequent hide→show again, delayed native ROOT focus acknowledgement (pending one-shot resolved once), superseding hide/revision cancellation, and explicit `PreserveForeground` rejection.  
**Done:** after interactive reveal, `ctx.memory().has_focus(query_input_id)` (and actual caret via TextEdit where testable); changing focus afterward is respected.  
**Commit:** `fix(launcher): [M1-A] request query focus once on interactive show`.

### M1-B — Protect active notes and true modals during initial focus

**Owner:** focused root focus policy, current Notes/foreground-layer modal classification and nearest tests; no global modal rewrite.  
**Tasks:**

1. Determine actual active control ownership at the show edge, rather than equating `notes_dialog.open`/`any_panel_open()` to “modal.” Non-modal, open-but-inactive Notes must not suppress first-launcher-query focus; actively edited Notes or a real confirmation/prompt must retain their deliberate keyboard input.
2. Assert query focus is **not** taken from real Settings/other foreground modal, `notes_dialog` nested delete/Template context popup, `snippet_prompt`, file picker, independent Radial Designer viewport, Screenshot capture/recording or unrecognized upper-layer window. Preserve one-shot note-panel activation when `open_note_panel` explicitly raises an editor.
3. Ensure query autofocus is not scheduled for a later unrelated frame after modal dismissal; preserve legitimate separately requested focus intents only where existing policy requires them.

**Verify:** one focused owner classification/modal regression slice, reusing the existing `settings_owner_blocks_root_autofocus_and_enter_activation` and `open_note_activation_hands_off_query_focus_once` tests as adjacent guards.  
**Done:** initial focus policy distinguishes ordinary nonmodal window visibility from intentional active input owner, without stealing focus.  
**Commit:** `test(launcher): [M1-B] guard show focus across notes and modal owners` (or combine with M1-A if too small for an independent coherent commit).

---

## M2 — Notes ↔ query focus transfer with Jump Mode enabled

### M2-A — Make visible query TextEdit clickable while non-modal Notes is open

**Owner:** `src/gui/render.rs` actual query `.interactive(...)` and keyboard edit/focus policy; `src/gui/jump_mode/ui.rs` only where exact owner semantics require it.  
**Tasks:**

1. Separate actual query **interactivity** from the broad `root_navigation_allowed` flag. With Jump Mode enabled and regular Quick Notes open, a visible unobstructed `query_input` response must accept deliberate mouse click/focus and subsequent typing, even when another normal Notes window exists.
2. Keep the query disabled/unavailable when genuinely obscured, behind an active modal, native picker, context popup, unsafe capture or unknown foreground owner. Do **not** globally enable background dashboard widgets, action rows or popups when Notes is foreground.
3. Preserve Notes editor draft, selected note, scroll and open state. After a query click, Notes must not auto-reclaim focus on subsequent frames; returning to Notes must restore ordinary editor typing without resetting the query.
4. Include ordinary `NotePanel` windows where they render on the root viewport with an unobstructed query; maintain any independent-viewport priority and note activation handoff.

**Verify:** real egui query/notes fixture: launch with Jump on; open Quick Notes/edit draft; click unobscured query response; insert Unicode text; verify only `LauncherApp.query` changes; click Notes editor and continue draft. Repeat with Jump off and with modal overlay (no root edits).  
**Done:** explicit mouse focus works and respects note edits/modals.  
**Commit:** `fix(jump): [M2-A] allow deliberate query focus beside notes`.

### M2-B — Add an exact, safe Jump hint → query handoff

**Owner:** `src/gui/jump_mode/ui.rs` registry/ownership and `src/gui/render.rs` `"query_input"` registration; use existing `TargetKind::TextEntry`, `JumpModeScope` and actual response semantics.  
**Tasks:**

1. When Notes is a valid ordinary non-modal owner, offer a **single dedicated, visually present and unobstructed query-focus target** using the existing Jump Mode hint system. Ideally reuse the real `query_input` semantic ID/actual TextEdit response; if direct cross-layer registration cannot be validated under the current engine, use a minimal explicit focus action that the hint can safely target, **not** a synthetic pointer click or a second launcher keyboard handler.
2. Validate exact Notes instance/foreground layer, query Response identity, root visibility, occlusion/clip, active popup/modal ownership and target generation at hint completion; retain one-surface input ownership. Do not permit any other root result/dashboard targets solely because this query shortcut exists.
3. Completing the hint must **focus only**, never change query, execute an action, select another note or deliver an Enter to two windows. Handoff Notes→query cancels stale note hints; deliberate Notes refocus cancels stale root hints; Escape still cancels a Jump session before invoking dialog close.
4. A currently unsupported note template/context popup, foreign window, native picker, true modal or separate foreground viewport must still block this cross-owner route. A query hidden behind a Note window receives no hint.
5. Keep Jump disabled behavior native and unchanged; preserve original Shift+Space leader, letter hints and same-frame key suppression.

**Verify:** focused `jump_mode` owner/target regression for Notes foreground and selected hint; verify `query_input` focused, source draft unchanged, no action fired, stale-hint rejection after window z-order change and fresh leader needed after returning to Notes.  
**Done:** user can reach a visible query through Jump Mode while Notes stays open; no broad background navigation bypass.  
**Commit:** `fix(jump): [M2-B] route notes query handoff through existing hints`.

### M2-C — Restore intentional launcher search results/actions under query ownership

**Owner:** `src/gui/render.rs` query keyboard paths (`query_has_focus`, `launcher_enter_activation_enabled`, result Enter/action gating, root navigation/output ownership) and only directly affected Jump Mode policies.  
**Tasks:**

1. Audit the `!self.notes_dialog.open` and `self.note_panels.is_empty()` Enter exclusions and related blanket `root_navigation_allowed` guards. Replace Notes' *presence alone* as a disqualifier **only when a real focused visible query legitimately owns normal launcher search input and no true modal/popup/other input owner is active**. Retain unrelated panel/prompt and File Search safety exclusions.
2. Allow normal query typing/field editing, history/autocomplete and existing visible result list/grid selection. A subsequent deliberate Enter may activate an exact current selected query result once through the existing `DeferredActivation` path, even if the nonmodal Notes window remains open.
3. When the Notes editor, context popup, confirmation or other legitimate input surface owns focus, Enter must **not** activate a launcher result. Do not permit hint-completion Enter to leak into selection/activation or change its target after result-generation refresh.
4. Ensure opening a launcher result that itself opens Notes or a separate dialog transfers focus only via its existing one-shot handoff, not a new auto-refocus loop. Non-note actions still use normal execution rules.

**Verify:** one focused egui regression with harmless fake launcher actions: click/hint to query → type query → select result → fresh Enter activates once; Notes editor focused → Enter edits note but activates nothing; nested modal remains protected. Check root-query caret and retained note draft after the activation.  
**Done:** full intended launcher query workflow functions beside nonmodal Notes, not just cursor display.  
**Commit:** `fix(launcher): [M2-C] permit query-owned commands beside notes`.

---

## M3 — Focused integration, review, and final quality gate

### M3-A — Bound the regression matrix and finish documentation

**Owner:** directly affected tests in `src/gui/render.rs`, `src/gui/jump_mode/ui.rs`, Notes test helpers and at most a short in-app Help/README note when user-visible guidance truly changes.  
**Tasks:**

1. Test both features together, using real `egui::Context` widget Responses rather than pure boolean-only tests. Minimum cases: (a) hidden→visible query focus empty Dashboard, (b) hidden→visible nonempty text/list view, (c) query focus remains user-changeable on later frames, (d) Quick Notes + Jump on mouse→query→Notes draft, (e) Quick Notes + Jump hint→query and stale hint blocked, (f) query-owned fresh Enter launches one safe action while Notes open, (g) exact modal/protected owner blocks, (h) Jump off retains normal query and note behavior, (i) focus-intent `PreserveForeground` doesn't steal independent viewer/editor focus, (j) delayed native ROOT focus acknowledges a pending show intent once and cancels it if superseded. Include repeated hide/show and discarded UI passes where existing test harness supports them.
2. Check launcher position variants (fixed vs mouse-following) at the **shared policy boundary** rather than implementing separate position-dependent focus paths. Keep result generation and query text preserved. Confirm one-shot focus and zero ghost Enter/click operations.
3. Preserve existing `jump_mode_notes_dialog_escape_preserves_draft_and_fresh_enter_cancels_edit`, `jump_mode_notes_rows_use_visible_slug_identity_and_fresh_enter_opens_exact_note`, `settings_owner_blocks_root_autofocus_and_enter_activation`, `open_note_activation_hands_off_query_focus_once`, and disabled-gate/foreign-owner regressions; update any old test only for an intentional change in Notes nonmodal query ownership, not to weaken real modal safety.
4. If the safe existing owned fixture can open both Notes and query, optionally perform **one** native session (1–2 redacted fixture screenshots) proving focus transfer and actual typed text. Do not add a general GUI automation framework or depend on screenshots for source correctness. On desktop limitation: one diagnostic plus one retry, record `not_run_environment`, then stop that avenue.
5. Inspect source/diff for accidental popup/root owner expansion, per-frame work, backdoor Enter routes, formatting churn and secret note data in logs/tests. Update the compact ledger with checkpoint commit IDs and exact checks performed.

**Verification policy:** During M1/M2, at most one directly affected Nextest slice per checkpoint and one adjacent owner slice where code crosses shared focus policy. In M3, run up to **two** tightly selected composed Nextest/egui regression slices, plus a narrow compile check only if needed. **No full Nextest, release build, broad benchmarks or full plugin suite** for this focused bug fix unless introduced failures demonstrate a concrete unavoidable cross-subsystem reason. Use first-party maintained Rust targets; never modify vendored crates for unrelated warnings. Preserve `next_hotkey=None` and `cancel_hotkey=None` in all new app test profiles, per `AGENTS.md`.

**Done:** all explicit focus/Notes/Jump cases pass with real widget/native focus semantics; all relevant existing owner tests still pass; no known input-security bypass.  
**Commit:** `test(jump): [M3-A] verify launcher notes query focus boundaries`.

### M3-B — Final maintained-code Clippy (last) and handoff

1. Complete all source, fixture, docs and ledger edits, touched-file Rust formatting and `git diff --check` **before final Clippy**; commit the verified work. No mandatory full Nextest for this task.
2. Run **one** appropriate maintained-target Clippy command near the end, chosen from the actual current Cargo workspace membership (typically `cargo clippy --workspace --all-targets -- -D warnings`). Do not promote vendored path dependencies into lint targets or change vendor code. If unchanged pre-existing first-party lint warnings obstruct a global clean result, report them accurately; prefer the smallest maintained-target check that proves new code is clean. Do not use blanket warning suppression.
3. At most two substantive corrections for a repeated identical lint failure; any necessary focused recheck/format/commit must occur **before the last successful Clippy invocation**. Stop verification after final Clippy and report the actual outcome; do not claim a failed/blocked gate passed.
4. Final report identifies original/source branch and SHA, new branch, first baseline SHA, checkpoint hashes, actual push outcomes, tests, whether native focus was observed, Clippy command/result, genuinely unsupported individual note-panel cases, and any remaining blocking defects. User conducts one manual review after Codex finishes; no intermediate manual testing requests.

**Done:** task is committed on the fresh branch, focused behavior is verified or any unsafe blocker precisely reported, final lint outcome honest, and no extra feature work undertaken.  
**Commit:** `chore(jump): [M3-B] finalize query focus lints` **only if** a bounded lint cleanup is needed, before the last Clippy pass. No artificial final commit solely for reporting.

---

## 3. Acceptance checklist

- [ ] **B0:** fresh `fix/` branch created and checked out, its own baseline commit made and initial push attempted; `BRANCH_BASELINE_SHA` recorded.
- [ ] Explicit interactive launcher hidden→visible show focuses `query_input` once, when safe; repeated frames don't recapture focus and text/caret remain usable.
- [ ] Fixed-position and mouse-following launch placements obey the same focus policy, regardless of Dashboard, query content or Jump enabled state.
- [ ] `PreserveForeground`, actual modals, native pickers and other active input owners keep focus; no delayed stale autofocus after closure.
- [ ] With ordinary Quick Notes open and Jump enabled, mouse can focus unobstructed query, type/search and return to Notes, retaining unsaved draft.
- [ ] An appropriate Jump hint can focus the unobstructed `query_input` without side effects while nonmodal Notes stays open; stale/unknown owners remain blocked.
- [ ] Under valid query ownership, ordinary result selection + later fresh Enter executes exactly once with Notes open; focused note editor Enter cannot run a launcher command.
- [ ] Related individual note panels behave safely according to actual viewport/ownership; no invisible-root focus or focus-stealing from independent native views.
- [ ] Jump disabled, Notes, Settings, Screenshot/recording, MkMacro, Paste Sequence and original keyboard behaviors are not regressed.
- [ ] Targeted egui/Nextest evidence, scoped review, small committed checkpoints, best-effort pushes and **Clippy last**, all with true results.

**Hard stop:** No expansion to arbitrary background windows, no second Jump engine, no additional global shortcuts, no duplicate visibility state machine, no permanent root focus, no synthetic native input. Complete this specific correction and stop.
