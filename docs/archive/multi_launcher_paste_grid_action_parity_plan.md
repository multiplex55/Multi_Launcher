# Multi Launcher — Paste Sequence Grid-Action Parity Repair

**Status:** Approved for Codex implementation  
**Scope:** Only running an existing saved Paste Sequence through its individual launcher-grid action (`paste:run:<id>`).  
**Source inspection:** User-provided October 18, 2026, 16:25 code snapshot. At execution, Codex verifies the equivalent paths in its **current checked-out repository**.  
**Agents:** Follow current `AGENTS.md` and `.codex/agents/{planner,implementer,reviewer}.toml`.  
**Output:** Two small code checkpoints, a narrow acceptance recheck, commits and a final report. No wider feature or QA campaign.

## 1. User-visible contract

For a saved **Paste-to-Advance** sequence containing two temporary-text steps—for example `First text` and `Second text`—the launcher-grid action must behave like the same sequence started through the **Paste Sequences dialog**:

- Click the saved grid tile **Test Sequence** (`paste:run:<stable-ID>`); the actual **saved** sequence is preflighted and started, not merely previewed. Pressing an eligible real `Ctrl+V` pastes the first text; a later distinct press pastes the second. The sequence completes and unarms the physical interception route.
- **Repeat**: activate the same grid tile a second, third and later time without restarting Multi Launcher. Each new run begins at Step 1, uses a new generation/lease, and completes normally. No stale `Waiting for Ctrl+V` session, leftover busy state or unhandled physical route. A third activation after completed runs must not report `Another paste sequence is already active`.
- If **Keep dialog visible during playback** is checked in Settings, executing a saved sequence from the grid **opens/keeps the actual Paste Sequences dialog and root launcher visible**, identifies the executed sequence and shows status/step progress. A user may switch focus to an external target for each paste without the dialog stealing it back. Do not change the setting's name, persistence or existing semantics when Run is clicked **inside** the dialog.
- If that setting is unchecked, preserve ordinary hide-on-run behavior for grid actions, **but playback must still receive physical `Ctrl+V` and complete correctly with the launcher hidden**. It must support another grid invocation after reopening the launcher.
- Starting from either route must respect existing `Ctrl+V` physical/injected/repeat filtering, user-directed current target selection, exact two-step order, clipboard-force-write policy, last-step clipboard retention, Escape/configured Cancel, and legitimate busy rejection while an actual previous run is still active.
- If a prompt is required, completing its existing prompt flow must auto-start once, apply the same visibility preference, and not leave an autostart flag or preparing session behind. Failed preflight/start must be reportable and recoverable; never hide the launcher in a way that conceals a failed run.

**Non-goals:** Do not redesign the Paste Sequence editor, change playback modes, modify the clipboard-overwrite policy, reintroduce the floating tooltip, add settings or hotkeys, change general grid execution/visibility policy for other actions, alter MkMacro/radial/Screen Draw, or optimize application performance.

## 2. Verified source-specific investigation map

This map is from the uploaded code. Codex should check nearby current code briefly, then implement; do not start a new repository-wide investigation.

| Owner | Observed behavior | Required investigation |
|---|---|---|
| `src/plugins/paste_sequence.rs` | Provider returns one `Action` with `action = "paste:run:<id>"` per saved sequence. | Keep stable IDs and catalog behavior; no new action type. |
| `src/commands/parser.rs`, `src/commands/handlers/paste_sequence.rs`, `src/commands/bus.rs` | Typed `PasteSequenceCommand::Run(id)` forwards to `DialogCommandHost::run_paste_sequence(id, source)`; success returns default command outcome. | Ensure direct action and dialog use equivalent run semantics, without breaking generic command outcomes. |
| `src/gui/command_host.rs::run_paste_sequence` | Loads saved sequence, resolves only required snippet/history data, sets `autostart_when_ready`, then calls preflight and `start_sequence_playback` directly for prompt-free sequences. **It does not open/select the Paste Sequences dialog.** | Make the route correctly present the active sequence if setting enabled, and ensure repeat invocations finish old state and arm a fresh session. Do not duplicate playback engine. |
| `src/gui/mod.rs::should_keep_paste_sequence_dialog_visible_during_playback` | Evaluates `keep_visible && (panel_states.paste_sequence_dialog || paste_sequence_dialog.open)`. | **Confirmed root cause of grid visibility bug:** a grid run with a closed dialog is treated as a hide request despite enabled preference. Separate the persisted preference from whether a dialog was previously open, and arrange safe dialog creation/selection when needed. |
| `src/gui/command_host.rs::start_sequence_playback_using` | After start and admission sync, hides dialog/root when `should_keep...` is false; dialog Run already has separate open-state handling. | Ensure the grid route does not hide in the opted-in case; do not make global launch visibility changes for unrelated commands. |
| `src/gui/paste_sequence_dialog.rs` | `open_with_context`, `begin_session`, `selected_id`, `draft`, `note_playback_started`, `set/take_autostart_when_ready`, `suspend_for_playback`, and `show_run_status` already exist. | Reuse these surfaces to show saved active run and status; don't clobber an already-open unsaved draft or reset a just-armed run by reinitializing editor state. |
| `src/paste_sequence/playback.rs` | `begin_preparing` returns Busy while `self.session` or active/preparing/ready status remains; `settle_physical_paste` completes and drops session; `tick` drives delayed settlement/cleanup. | Verify terminal transitions and cleanup after **saved grid actions**, not only after direct dialog Run. Never blindly clear a genuine running sequence. |
| `src/paste_sequence/admission.rs`, `src/gui/command_host.rs::sync_physical_paste_admission` | Shared physical-key bridge has exact owner/generation lease, invalidation, injected/repeat filtering and listener route availability. | Verify first run fully disarms and subsequent run establishes a fresh physical admission, with V-down/V-up lifecycle and correct GUI owner. Fix only a proved stale-lease/event path. |
| `src/gui/mod.rs::send_physical_paste_event`, `src/gui/watch.rs`, `src/gui/render.rs`, `src/main.rs::drain_physical_paste_notices` | Native notice is dispatched to an exact GUI owner with viewport wake; controller tick/reconciliation happens on GUI frames. | Prove paste notice delivery and settlement **when the launcher is hidden**. Check lost/stale events and frame scheduling before attributing failure to clipboard content or adding sleeps. |
| `src/gui/actions.rs::restore_for_new_launcher_interaction` | A newly opened root panel causes root visibility restoration after action dispatch. | Ensure a newly revealed Paste dialog is not hidden again by a later visibility update and does not alter ordinary grid-action behavior. |

**Important distinction:** The visibility bug is source-confirmed. The reported second/third-run failure is reproducible user behavior but its exact technical cause is **not** established by static inspection; Codex must identify that specific ownership/lifecycle seam rather than asserting an unproven cause.

## 3. Bounded delivery — two checkpoints

### G1 — Grid action obeys existing visibility setting and displays active run

**Objective:** Clicking a saved grid action takes the same effective Run path as the dialog, with correct opt-in editor visibility and legible status.

**Implementation instructions**

1. Inspect existing `run_paste_sequence`, `start_sequence_playback_using`, `should_keep_paste_sequence_dialog_visible_during_playback`, PasteSequenceDialog open/suspend and `activate_action` root-interaction restoration. Identify where to branch on **acknowledged** setting `keep_dialog_visible_during_playback`, not on the accidental precondition that the editor was already open.
2. For a grid `Run(id)` and preference enabled, preserve the already-captured invocation context and saved sequence identity, then make the Paste Sequences dialog visible and ensure the parent launcher viewport is actually visible. Prefer a small existing-dialog method to present/select an explicitly executed saved sequence; do not build a new dialog. The displayed status and steps must refer to the **executed ID**, not stale/unsaved draft data. If another edit is open, preserve its unsaved changes rather than resetting it behind the user's back; a small active-run identification/status display is acceptable.
3. Keep one canonical playback start/preflight/admission implementation. Don't copy the controller or have a second native input path. Make grid Run and dialog Run differ only in origin/selection and viewport presentation.
4. If preference is disabled, preserve current hide-on-run behavior for both grid and dialog, without closing the controller or invalidating its event owner. A hidden GUI must still receive `Ctrl+V` notices/wakes.
5. Preserve prompt-free and prompted autostart semantics; failed source resolution or start must not leave stale `autostart_when_ready` or a dialog that falsely reports started. Make errors visible through the existing mechanism.
6. Restrict changes to Paste Sequence-specific host, dialog and minimal action routing seams. Do not change global visibility defaults, unrelated plugin behavior, or persist any extra fields.

**Acceptance (directly checkable):**
- With pref **true**, clicking `paste:run:<id>` from the grid displays the real Paste Sequences dialog, keeps the root visible, and reports the correct active sequence/step/status.
- With pref **false**, the same action hides as before and leaves an independently functioning paste session.
- Dialog Run/Run Once, saved sequence identity, and old settings continue unchanged.

**Commit:** `fix(paste): [G1] honor dialog visibility for saved grid actions` after directly relevant focused verification and diff review. Push once if permitted; local commit is enough to continue.

### G2 — Repair repeat-run lifecycle for the individual grid action

**Objective:** Run → paste Step 1 → paste Step 2 → Complete → execute the **same saved tile** again, repeatedly, without a restart or stale session.

**Implementation instructions**

1. Trace saved-grid invocation → preflight `begin_preparing` → `Ready` → `start_with_admission` → physical `Ctrl+V` notice via GUI event owner → step settle via `tick` → `Completed` → terminal `session=None` and bridge disarmed. Compare to the working dialog path. Note exact status/generation/lease transitions **without logging clipboard content**.
2. Isolate the first real divergence on a second grid Run. Focus on: reused `autostart_when_ready`, previous `physical_paste` pending permit, held V key cycle after completion, missed wake when hidden, terminal cleanup not polled, session generation blocked, or route observer no longer live. These are **diagnostic candidates**, not preselected explanations.
3. Correct the narrow owner responsible for the regression. If it is the saved-action orchestration, fix it there instead of modifying core input hooks. Touch `admission.rs`, physical hook, GUI scheduler or controller only if the focused evidence demonstrates that an owner/settle bug exists there. Avoid timers/sleeps, hard resets, repeated retries, launching extra windows or recreating the keyboard listener merely to mask stale state.
4. Ensure each successful completion fully disarms the old physical Ctrl+V route; starting a new grid action creates a distinct valid generation/lease/GUI sink and Step 1 begins again. A normal Ctrl+V after completion must still paste the final clipboard text, as it does today.
5. Preserve safety invariants: ignore injected/repeated Ctrl+V and correctly match key-up; no duplicate/premature text; don't paste into the launcher or protected fields; no clipboard restoration or external-change cancellation reintroduced; correct Cancel/Escape cleanup. Reject concurrent Run while **truly active**, but not after terminal completion and safe settle.
6. Do not make repeat-run depend on keeping the dialog visible; test both `keep_dialog_visible_during_playback` values. Inspect only directly affected components and create no generic test framework.

**Acceptance (directly checkable):**
- From one live app instance, a saved 2-temp-text sequence performs three successive cycles, each with Step 1 then Step 2, from the **grid action**.
- On completion, controller/bridge transition to a state admitting another Run, without spuriously returning Busy.
- Both keep-visible and hide-on-run settings work, with correct session status and owned `Ctrl+V` handling.
- If startup/preflight fails, later retry is possible after cleanup; genuine overlapping runs still reject Busy.

**Commit:** `fix(paste): [G2] rearm saved grid paste sessions after completion` after directly relevant focused verification and diff review. Push once if allowed; failure to push never blocks completion.

## 4. Very small verification contract

**Do not** run full Nextest, Clippy, a full Cargo release build, benchmarks, interactive computer-control skills, screen automation, or screenshot capture. The user will manually validate after Codex's final handoff. No new harness, global hook simulator, settings framework or validation campaign.

Use **at most two focused `cargo nextest run` invocations total**, plus **one exact-failure rerun** only after a concrete correction. Prefer one focused `--lib` filter for the changed grid + dialog cases and one adjacent route/controller test only if it actually changes. Compilation implied by those tests counts as sufficient; avoid a separate expensive check without compiler errors. Run `git diff --check` and format touched project files; keep Rust formatting narrow. No Clippy.

**Focused test requirement:** Reuse `src/gui/command_host.rs`'s `inert_test_app`, `AppPastePlatform`, existing controller/admission test seams, and the `paste_sequence_run_and_run_once_respect_visibility_preference_and_restart_all_modes` dialog regression. Add or adapt a **single small grid-route parity/repeat-run regression** using the actual `paste:run:<id>` command path as far as an existing fake native input seam permits. Use only temp-owned sequence data, not a user's actual JSON, clipboard or applications.

The key regression checks are:

1. `keep=true`, dialog initially closed → grid Run(id) → dialog/root visible and running saved ID, status/step correct; `keep=false` → root hides but controller remains armed.
2. Two-step Paste-to-Advance: physical down/up #1 → Step 1; distinct down/up #2 → Step 2; settle/Complete; trigger same grid action again **at least twice** and prove Step 1/2 repeat, no Busy after completion. Confirm a physical notice reaches the exact owner while hidden.
3. Errors/prompt cleanup: failed preflight or rejected Run leaves no stuck `Preparing`/`Ready`/`autostart_when_ready`; legitimate active concurrent attempt still returns Busy; dialog Run regression remains passing.

If complete grid dispatch cannot be invoked safely under the existing fakes without new global/native infrastructure, keep one narrow test at the earliest fully testable shared boundary and **explicitly report** the unverified end-to-end keyboard behavior. Do not claim that deterministic tests established native Windows behavior.

**Bounds:** one planner read-only pass; one implementer working sequentially; one read-only reviewer pass after both checkpoints (or between checkpoints if a concrete risky seam requires it); at most two meaningful fixes for an unchanged blocker; no more than one directly related rerun; no personal desktop interaction. If an environment/tooling blocker prevents tests, report it rather than restarting a long compilation loop.

## 5. Agent-specific execution and final handoff

- **Planner**: use the configured named `planner` sub-agent (`.codex/agents/planner.toml`, currently GPT-6.1 Sol High, read-only). Inspect only G1/G2 ownership, hand the implementer explicit packets; **do not rewrite this approved plan**.
- **Implementer**: use the named `implementer` sub-agent (`.codex/agents/implementer.toml`, currently GPT-6 Luna Max, workspace-write). One writer makes all changes, runs focused verification, and reports actual results to parent.
- **Reviewer**: use named `reviewer` (`.codex/agents/reviewer.toml`, currently GPT-6.1 Sol High, read-only) to check the diff for root visibility, event/admission ownership, replay/reentry, unrelated UI and test evidence. One bounded review, then implementer addresses only concrete findings.
- **Orchestrator/parent**: owns checkpoint sequence, Git status/diff/commits, best-effort non-force push according to `AGENTS.md`, and final report. Do not let multiple agents edit simultaneously; preserve the current branch and user changes.

**Final report:** root cause of grid visibility (source-confirmed); actual cause and changed code for repeated-grid execution (or clearly documented residual limitation); G1/G2 commits and push status; exact focused commands and pass/fail; retained functionality and anything requiring the user's final manual review. Stop; no invitation for new work and no intermediate user questions.
