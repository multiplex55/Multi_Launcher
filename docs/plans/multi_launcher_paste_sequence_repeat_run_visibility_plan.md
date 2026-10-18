# Multi Launcher — Paste-to-Advance Repeat-Run, Dialog Visibility, and Tooltip Repair

**Status:** Approved for Codex execution; implementation **not** performed by this document.  
**Product:** Multi Launcher (Windows, Rust, eframe/egui).  
**Source authority:** The repository checkout at execution time. This plan was grounded in the October 18, 2026 source supplied for review; verify current owners before editing.  
**Primary outcome:** A two-step Paste-to-Advance sequence works on the **first run and every subsequent run without restarting Multi Launcher**.  
**Secondary outcome:** Add `Keep Paste Sequences dialog visible during playback` to the existing Paste Sequences section of main Settings.  
**Tertiary outcome:** Fix the existing cursor-following progress tooltip if a bounded investigation finds an actionable cause.  
**Priority:** **P1 repeat-run** > **P2 visibility preference** > **P3 tooltip**. Never let P3 delay completion of P1/P2.

> This is a **single focused bug-fix assignment**. Use the repository's configured `planner`, `implementer`, and `reviewer` runners; one writer only. No computer-control skills, UI automation, screenshots, new native test rig, full Nextest, Clippy, benchmarking, or unrelated cleanup. Codex works autonomously; the user performs the **first manual Windows acceptance test after delivery**.

## 1. Exact reported behavior and acceptance contract

The user reports:

- A saved or temporary two-step sequence containing two **Temporary Text** rows successfully pasted both texts into Notepad once using Paste-to-Advance.
- After completing that run and clicking **Run** again, the editor/launcher hides as usual. The status says it is **waiting for an eligible `Ctrl+V`**, but physical Ctrl+V does not advance the new run; an ordinary paste inserts the **old last step** still on the clipboard, or whatever else is currently there.
- The cursor-following progress tooltip has never been reliably visible. This is lower priority than actual playback.
- The user wants a persistent settings checkbox so the **Paste Sequences dialog can remain open/visible while playback runs**, particularly useful for repeated testing and progress visibility.

### Desired human workflow

**With "Keep Paste Sequences dialog visible during playback" OFF (default, preserves current behavior):**

1. Open `paste` / Paste Sequences, select or compose two Temporary Text steps: `ALPHA`, `BRAVO`; set `Paste-to-Advance (Ctrl+V)`.
2. Click **Run** or **Run Once**. Start succeeds. Launcher root and editor hide, while the session remains active.
3. Focus Notepad. One new physical Ctrl+V pastes `ALPHA`, release the keys; the next new physical Ctrl+V pastes `BRAVO`.
4. After the existing bounded consumption/settling boundary, the run becomes `Completed`. The hook no longer claims Ctrl+V. The clipboard retains `BRAVO` until another actor changes it; an ordinary next Ctrl+V pastes `BRAVO`.
5. Show the launcher, reopen the editor (which retains its draft per existing suspended-session behavior), click **Run** again **without restarting Multi Launcher**. The first **new** Ctrl+V must paste `ALPHA` again, not ordinary-paste `BRAVO`; the second must paste `BRAVO`. Repeat at least one additional cycle with the same app instance.
6. Starting after Cancel Run / Escape and reopening after the last run must also create an independent, fully armed session.

**With the new checkbox ON:**

1. Clicking Run / Run Once / Start **from the open editor** starts the same playback, but leaves the launcher root visible and the **Paste Sequences editor open**, retaining the authored draft/selected sequence and showing truthful current step/status.
2. Clicking or Alt+Tabbing to Notepad changes the intended **target focus**, without the launcher taking it back. Paste-to-Advance is claimed only when an eligible **external** window owns focus. Do not make `Ctrl+V` inside the launcher/editor inject sequence steps into the launcher.
3. On Completed / Cancelled / Failed, the dialog remains available to inspect status, edit, and click Run again. Manual hide/close remains possible; explicit Cancel Run still cancels playback.
4. Enabling the preference **does not automatically pop up the launcher** when a sequence is started by an optional global Start hotkey while the editor is closed. It only preserves an already-open editor/root. Do not add a new always-on-top mode, alter global launcher show/hide hotkeys, or forcibly focus Notepad.

**Tooltip:** The existing passive progress tooltip, if enabled in Settings, should show active name/step/next action near the cursor in **all three** playback modes, with no contents of text steps/snippets/clipboard; it must be click-through, non-activating and disappear on completion/cancel/error. If unavailable for a real native reason, report a precise actionable error once per run; the editor (when kept open) must still show status. **Do not claim real tooltip visibility was confirmed by pure Rust tests.**

### Preserve from prior repairs

- The current user-approved **force clipboard** policy: each step overwrites current clipboard text at its execution boundary, even if the user copied something different between steps. **Do not reintroduce clipboard revision/ownership enforcement or original-clipboard capture/restore.**
- After the final step the last value remains on the clipboard; after Cancel the last published value is retained; no original clipboard restoration.
- Human physical Ctrl+V is intercepted **only while an actively armed Paste-to-Advance session owns it**. Ignore injected/repeat events; consume a complete accepted Down/Up transaction; no duplicate pasted steps.
- Eligible currently focused external app is selected *per step*, never confined to original HWND. Respect established launcher/self, password/protected input, modifiers, conflicting hooks and exclusive-tool checks.
- Existing Automatic and Manual Advance modes, configured Next/Cancel shortcuts, Escape cancellation, prompts, saved sequences, Run Once memory-only behavior, hidden-snippet redaction and stable `paste_sequences.json` schema remain compatible.
- No arbitrary user clipboard preservation; **genuine OS failure to write text or inject input must still surface correctly** rather than falsely report success.

## 2. Current source map — code verified in the supplied snapshot

| Concern | Actual owner and existing behavior | Precise investigation target |
|---|---|---|
| Session state machine | `src/paste_sequence/playback.rs`: `SequencePlaybackController::{begin_preparing,start_with_admission,physical_paste,settle_physical_paste,tick,fail,cancel}`; `StartedSequence`, `SessionInputLease` | Terminal completion disarms and clears session. New run generation/preflight must rearm fresh state; no stale pending permit or terminal status. |
| Physical input admission | `src/paste_sequence/admission.rs`: `PasteAdmissionBridge::{configure_session,publish_session,reserve,finish,valid,invalidate_owner}` and `PhysicalPasteReducer::{process,invalidate}` | `armed`, `session`, `epoch`, `invalidated`, `in_flight`, key `known`, `owned_v`, held/released Ctrl/V and source/route policy across *two* consecutive runs. |
| Hook owner | `src/hotkey/launcher_invocation.rs`: shared hook calls `PhysicalPasteReducer::process` before other launcher routes; `observer_live`; physical modifier/key baseline | Confirm the second run's real physical Ctrl+V is admitted after first completion, while the shared service stays installed. Do not register another hook. |
| Delivery | `src/main.rs`: `drain_physical_paste_notices`, route policy; `src/gui/watch.rs`: `WatchEvent::PhysicalPaste`; `src/gui/command_host.rs`: `handle_physical_paste_notice`, `execute_physical_paste_using`, `sync_physical_paste_admission` | Ensure each newly admitted event is delivered to the **current** owner/generation when the root is hidden; stale notices cannot consume/destroy the successor session. |
| Run and visibility | `src/gui/paste_sequence_dialog.rs`: `prepare_current_using`, `suspend_for_playback`, `open_with_context`, `editor_footer`; `src/gui/command_host.rs`: `start_sequence_playback_using`; `src/gui/mod.rs`: panel state, close/open functions | Currently `start_sequence_playback_using` unconditionally `suspend_for_playback`, clears panel state/stack, sets both visibility and restore flags false. The dialog is temporarily moved out during rendering, so local and app-held paths must agree. |
| Settings | `src/settings/model.rs`: `PasteSequencePreferences` with Next/Cancel, `tooltip_enabled`, `escape_cancel_enabled`; `src/settings_editor/{render,state,mapping,save}.rs` | Add one serde-defaulted bool. Reuse staged Save/Cancel and existing Paste Sequences section; default **false** for backwards-compatible hide. |
| Tooltip | `src/paste_sequence/progress.rs`: `SequenceProgressTooltip::update`, `NativeProgressPresenter::{sample,measure,show_at}`; `src/platform/passive_tooltip.rs`; `src/gui/render.rs`: `refresh_sequence_progress_tooltip` | `update` requires a started progress snapshot **and** `sequence_routes`/enabled preference. It attempts cursor sample → measure → placement → native show; failure latches per generation. Hidden-root logic should keep receiving repaint and ticks. |
| Existing tests | Inline tests in the above modules, notably `paste_to_advance_three_lazy_cross_app_steps_settle_then_fourth_is_normal_paste`, `paste_sequence_run_and_run_once_hide_taken_editor_preserve_draft_and_arm_all_modes`, `physical_paste_generation_invalidation_preserves_owned_up_and_rejects_stale`, `sequence_progress_surface_follows_cursor_and_live_preferences_without_idle_wakes`, and `paste_settings_navigation_controls_and_cancel_preserve_committed_state` | Adapt the existing fixtures to prove **two/three whole runs**, and the settings/visibility state under the real editor start route. No new harness. |

**Diagnosis discipline:** Static inspection establishes candidate boundaries, **not a proven exact root cause**. The new plan requires the implementer to identify the specific reason the *second* physical Ctrl+V is passed through or is not delivered. Avoid guessing that clipboard data, hidden UI, or the tooltip alone caused it. The fact that old `BRAVO` appears is consistent with the second run's Ctrl+V not being claimed, but doesn't alone prove which layer failed.

## 3. Workflow contract, test budget, and agent handoffs

### Mandatory named agents from `.codex/agents/` and `AGENTS.md`

- Parent Codex orchestrates and owns milestone sequencing/commits; spawn **`planner`** exactly once for a brief **read-only** handoff/revalidation; configured GPT-6.1 Sol High.
- Delegate all source edits to **`implementer`**, the configured GPT-6 Luna Max writer. Exactly **one** implementation writer, sequential across stages. Parent does not race it by editing the same source.
- Invoke **`reviewer`** once read-only after implementation, focused on reuse of the session, lease/drain correctness, visibility preservation and native tooltip risk; configured GPT-6.1 Sol High. Repair only concrete P0/P1/P2 findings and recheck the affected slice, no cascading reviewer loops.
- Use agent definitions as they exist in the current checkout; don't substitute generic anonymous agents, and don't request user approvals between stages. Maintain small natural commits (`R1`, `R2`, `R3`). Git push may be attempted if authorized; if denied, retain local commits and continue; no approval bypass or push retry spiral.

### Strict execution cap

- **No computer-use plugin/skills, desktop control, screenshots, or native interactive test process.** User will manually evaluate after this Codex pass. Do not create a `PasteSequenceSmoke.exe`, GUI automation harness, new event recorder, or external text-editor fixture.
- **At most two focused Nextest invocations total** in the initial implementation pass, chosen from existing lib/main targets with narrow filters. **At most one extra exact-failure rerun** if a substantive fix changed the failed path. If compilation fails before tests, correct concrete diagnostics inside the same bounded verification window; do not reset progress or start a broad campaign.
- No full `cargo nextest run`, no Clippy, no benchmark run, no release build. Format only touched files, `git diff --check`, and minimally compile only what targeted Nextest already needs (one targeted `cargo check` only if required for an untested executable seam).
- No long-written execution ledger or scope-expanding design documents. A compact existing-plan checklist/commit message and final report suffice. No unrelated refactor, route redesign, new global hook, UI redesign, huge instrumentation, changes to app workspaces or other plugins.
- For one unchanged failure, permit at most **two concrete implementation corrections** and the limited rechecks above; then mark the blocker and stop or continue isolated lower-risk work. Never weaken key-safety assertions, falsely label `AwaitPaste` as proof of hook readiness, or report a live desktop success not observed.

---

## R1 — Repair consecutive Paste-to-Advance runs (P1, highest priority)

### R1-A. Confirm where the **second** physical Ctrl+V disappears

**Owner:** `planner` does a bounded source pass and hands the implementer this precise route:

`Run → prepare_current_using → start_sequence_playback_using → start_with_admission → configure_session/publish_session → root visible/hidden loop → PhysicalPasteReducer::process → PasteAdmissionBridge::reserve → main::drain_physical_paste_notices → GUI notice → physical_paste → settle_physical_paste → finish/disarm → Run again`.

**Required analysis (read-only, then code):**

1. Inspect whether first-run `Completed` leaves an obsolete `invalidated`, `cancelled`, `failure`, `in_flight`, `busy_reported`, `published_epoch`, pressed-key latch or stale route policy that interferes with the new generation.
2. Inspect how a new run's owner/generation replaces the old lease while the reducer has previously consumed key Down/Up; verify Ctrl and V released cleanly, and that injected paste events do not contaminate physical chord readiness.
3. Check `sync_physical_paste_admission()` while launcher root is hidden and whether a later repaint/sync improperly drops or masks an otherwise live new lease; inspect the exact owner sink and event pump, not unrelated hotkey registrations.
4. Check the success path in `start_sequence_playback_using`: reporting Start success/`AwaitPaste` is insufficient if bridge arming or delivery is not live. A false `AwaitPaste` must become an explicit actionable failure instead of waiting forever or passing through the old clipboard text.
5. Distinguish failure to **claim physical Ctrl+V** (OS paste of old clipboard) from failure to **deliver a claimed event** (no paste) and from rejection **after** delivery (visible status/error).
6. Root cause must be recorded as a short factual explanation based on source and focused regression proof, not an assumed culprit. Do not add permanent verbose keylogging; any trace fields must be payload-free and disabled/noisy only on actual failures.

**Acceptance for R1-A:** One clear root cause (or bounded remaining uncertainty), correction boundary chosen, one writer ready to patch. No standalone diagnostic harness.

### R1-B. Implement a fresh, atomic admission lifecycle for every run

**Owner:** `implementer`. Change the narrowest actual owner of the diagnosed failure.

1. On terminal `Completed`, `Cancelled` and `Failed`, retire the prior exact owner/generation and in-flight admission *without* poisoning a later run. Consume/drain a prior claimed V Up safely; do not swallow a fresh V press after release.
2. At a subsequent Start, issue a fresh generation/lease and ensure hook admission/owner queue policy is really ready **before** Run reports `AwaitPaste`. Do not simply set a GUI boolean or force-clear global hook state without the corresponding key/owner lifetime handshake.
3. Do not allow a leftover old notice, stale permit Drop, cancelled epoch or late setup acknowledgement to disarm a valid newly started run. Preserve exact owner/generation fencing.
4. Preserve original first-run success and the 180 ms guard that avoids publishing the next step too early; do not restore clipboard sequence-number checks from retired policy. Preserve last text on clipboard after completion.
5. If the observer really is unavailable, fail Start visibly and keep the dialog recoverable; no misleading indefinite "Waiting for eligible Ctrl+V" while OS paste silently passes through.
6. Do not re-register/restart the entire global hook for each run, add timers merely to fake readiness, or hard-reset the bridge with a new independent shared instance.

**Minimal regression:** Reuse `PhysicalPasteReducer`/bridge fake and current controller/main delivery tests. With **one live controller/bridge/reducer**, perform `Run 1 → two physical Ctrl+V presses → Completed → ordinary Ctrl+V → Run 2 → first fresh Ctrl+V is consumed and publishes step 1, second publishes step 2 → Completed → Run 3` (including release, timing and ownership transitions). Assert old clipboard content does not replace Step 1; check `run.generation` strictly fresh. Add one Cancel/restart path only if failure involves cancellation state. Cover hidden-root delivery if the found bug belongs to the main/GUI seam.

**Commit when coherent:** `fix(paste): [R1] rearm physical Ctrl+V across consecutive runs`.

**Done:** Same app/controller supports three correct sequential runs without a restart, no duplicate events or stale clipboard paste caused by pass-through; first run and fourth ordinary OS paste still behave normally.

---

## R2 — Configurable Paste Sequences dialog visibility (P2)

### R2-A. Add a backward-compatible preference in existing main Settings

**Owner:** `implementer`; no new settings store.

1. Add `keep_dialog_visible_during_playback: bool` (or an equivalently precise field name) to existing `PasteSequencePreferences` in `src/settings/model.rs`, with serde defaults/`Default::default()` **false**. Old `settings.json` without this field loads with the historical auto-hide behavior and is not rewritten on read.
2. Add a checkbox labeled **`Keep Paste Sequences dialog visible during playback`** under **Settings → Paste Sequences**, alongside the existing tooltip and Escape controls, with one short help sentence: `When started from this dialog, leave it open while you paste into another application.`
3. Reuse the Settings editor's existing staged Save/Cancel transaction, persisted load, live acknowledged settings publication and reload. Do not break Next/Cancel custom shortcut state, route conflict validation or settings rollback; modifying the checkbox must not trigger unnecessary hotkey registration work.
4. The existing **Playback Settings** link from the Paste Sequences dialog must reveal the checkbox. No new standalone dialog, separate file or command prefix.
5. For an already-running sequence, apply the saved preference on the **next Run** rather than forcibly changing the current run's visibility in the middle of an accepted physical key cycle. Document this only if helpful in the in-app hint.

**Focused checks:** One old/default/true JSON round-trip and one staged Settings Save/Cancel/reopen test using existing fixtures; no full settings suite.

### R2-B. Fix the actual root+dialog visibility transaction

**Required owner changes:** `src/gui/command_host.rs` `start_sequence_playback_using`, `src/gui/paste_sequence_dialog.rs` `prepare_current_using`/`editor_footer`/`suspend_for_playback`, relevant `src/gui/mod.rs` panel owner and existing visibility request API.

1. There are **two** active Run paths: UI actions performed while the editor is **temporarily taken out of `LauncherApp`** during rendering, and app-owned Start performed after snippet prompts. Make both read the same applied preference and apply an identical visibility decision. Don't mutate a detached placeholder `app.paste_sequence_dialog` and then overwrite the real dialog instance on return.
2. If preference **off**: retain exact current successful-run behavior—suspend the editor while preserving draft for reopening; remove panel state/stack and hide root/restore intent. The playback controller continues while hidden.
3. If preference **on** and editor open: **do not suspend the real editor**, clear panel ownership, or issue the root-hide request. Keep `Paste Sequences` rendered, with current status/Cancel/Next where applicable, until the user closes it. Preserve draft, selection, source references and scroll position. The root should not regain keyboard focus every frame while the user works in Notepad.
4. If started through a background saved Start hotkey with editor closed: don't auto-open launcher; allow external target focus and the same background sequence behavior.
5. When complete, leave the already-visible dialog open and editable; a fresh Run begins a fresh sequence. Explicit Cancel Run cancels; editor close/X uses existing intentional close semantics. Ordinary launcher visibility toggles still work and must not destroy an active sequence accidentally.
6. Prevent accidental editor hotkeys/clipboard shortcuts from masquerading as external `Ctrl+V`. When user focus is inside the launcher, physical Paste-to-Advance admission should not claim it. Other hotkeys and plugins are untouched.

**Focused checks:** Adapt the existing `paste_sequence_run_and_run_once_hide_taken_editor_preserve_draft_and_arm_all_modes` test to verify both checkbox values, UI Run and Run Once, Start after prompt completion, retained open panel/launcher flags, and successful next Run. Don't create an egui screenshot driver; current unit/UI-state harness is sufficient.

**Commit when coherent:** `feat(paste): [R2] optionally keep sequence editor visible during playback`.

**Done:** Checkbox false preserves existing behavior; true preserves root+dialog and draft on run and remains usable after completion. The setting persists and Save/Cancel behave correctly.

---

## R3 — Bounded tooltip repair, final verification and handoff (P3)

### R3-A. Inspect the already-existing tooltip path, no new native overlay

**Owner:** `implementer`, limited investigation and fix. No new presentation framework.

1. Confirm real applied `tooltip_enabled` (not merely an unsaved draft), `sequence_routes.as_ref()` presence, started `SequencePlaybackController::progress()` metadata and per-run update calls after Run, while root is hidden, during settle and after terminal state. The existing native `SequenceProgressTooltip` depends on all of these.
2. Check the existing `NativeProgressPresenter` sequence: cursor sampling (`current_mouse_position`), nearest monitor/work area/DPI, geometry `place`, `PassiveTooltip::new/measure/show_at`, native window visibility/style, ownership/thread affinity, and one-error-per-run failure latch. Identify a concrete return-`None` cause before changing behavior.
3. If a simple reproducible code defect is found, repair only that boundary while preserving non-activating, mouse-pass-through, privacy-safe text. Do not change global cursor overlays, MkMacro, Screen Draw, the radial tooltip, or allocate a second always-on-top service.
4. If tooltip rendering can't be proven without prohibited native interaction, do not pretend it is verified. At most, improve the existing **one-shot payload-free failure diagnostic** enough to say *which native presentation stage failed*, without raw clipboard/snippet contents or noisy per-frame logging. When the new keep-open preference is enabled, existing in-dialog run status remains a useful fallback.
5. Active presentation should persist through Paste-to-Advance AwaitPaste/settle and across launcher hiding; clear on Completed/Cancelled/Failed. Any per-generation failure latch should reset at a later Run rather than permanently disable all future tooltips. Respect existing tooltip enabled toggle.

**Focused checks:** Existing `sequence_progress_surface_follows_cursor_and_live_preferences_without_idle_wakes` and `sequence_progress_terminal_cleanup_and_failure_latch_are_per_run` may be extended **only** for a concrete regression. Do not create native screenshot tests.

### R3-B. One bounded reviewer pass and closeout

1. Invoke the configured **`reviewer`** read-only, with the cumulative scoped diff and these exact invariants: Run 2 rearmed; admitted Down/Up and no injected feedback; last clipboard retained; keep-open only when editor was open; root focus not stolen; settings migration/Save/Cancel; tooltip non-activating/secret-free/lifecycle.
2. Fix only substantive reported defects. Do not enter recurring planner/reviewer iterations; don't widen test scope unless it is needed to establish a correctness change.
3. Execute a maximum of **two directly focused Nextest commands in total** (not per checkpoint) and, if necessary, one exact-failed-test rerun; use only tests already present or minimal modifications of them. Choose the first focused filter for R1 physical admission/controller/main, and the second for R2 settings+dialog and any actually changed tooltip test. Compile only affected targets; no full suite or Clippy.
4. Run Rustfmt only for touched maintained source files and `git diff --check`; inspect `git status` and the actual cumulative diff. Commit coherent checkpoint changes if they haven't already been committed; optionally push where permitted, otherwise keep local commits without retry loops.
5. Stop. Do not run the app via computer-control skills, script UI automation, ask the user to execute commands, build a new harness, or start a fresh diagnostics campaign. Provide a concise final handoff for the user's **one manual review**.

**Commit if there is real code to change:** `fix(paste): [R3] restore bounded passive progress presentation`. Don't create an artificial commit for a diagnostic-only review.

**Done:** Three-run deterministic acceptance passes or exact unresolved failure is recorded; default/keep-open preference works; tooltip fix/limitation truthfully described; tests and commit IDs reported without claiming a live desktop test.

---

## 4. Minimal acceptance table and stop conditions

| Scenario | Required result | What must never happen |
|---|---|---|
| Two-step first run | First Ctrl+V `ALPHA`, second `BRAVO`, Completed | Initial Run stays `AwaitPaste` without real hook arm |
| Second and third runs in same app | First Ctrl+V again `ALPHA`, second `BRAVO` | Native OS pastes stale `BRAVO` as step 1, or no notice arrives |
| Between runs | Fourth ordinary Ctrl+V pastes final `BRAVO` | Completed session continues swallowing physical Ctrl+V |
| Run after Cancel | New session can claim first fresh Ctrl+V | Old key latch/permit cancels the new session |
| Keep-visible OFF | Dialog/root hide; playback still advances in Notepad | Closing root cancels run implicitly |
| Keep-visible ON | Dialog/root remain visible and editable, with Notepad receiving intended Ctrl+V | App reclaims focus or pastes sequence text into its own editor |
| Legacy settings | Missing checkbox field decodes as false | Old settings fail or are rewritten on load |
| Settings Save / Cancel | Save persists choice; Cancel retains old value | Shortcut binding, editor draft or plugin status silently changes |
| Tooltip enabled | Existing tooltip attempts visible active state, follows cursor, clears on terminal | Tooltip grabs focus/clicks, shows snippet content, or leaks native HWND |
| Tooltip unavailable | One useful error/status fallback; main tasks still completed | New overlay framework or repeated desktop-access attempts |

**Stop immediately / do not claim completion if:** accepted physical Ctrl+V is sent to an unintended window, a stale permit from run 1 can execute during run 2, global hotkey and Escape ownership are broken, or new setting fails to preserve legacy saves. Isolate the exact failure and report it; do not hide it by weakening a focused test.

## 5. Final Codex user report format

- **Root cause:** one paragraph distinguishing verified cause(s) from suspicion; identify first-vs-second run lifecycle.
- **Changed behavior:** repeat-run results, checkbox behavior/default, tooltip status and any fallback.
- **Files and commits:** R1/R2/R3 commit subjects and hashes; whether push succeeded or remained local.
- **Verification:** exact targeted Nextest commands, counts and pass/fail, formatting/diff checks. Explicitly note **no full Nextest/Clippy/native UI automation** was run by design.
- **Manual acceptance handoff:** say user can now conduct their single Notepad/real-desktop review; do not ask them to test before Codex stops.
- **Known limitations:** only real unresolved bugs or native-tooltip uncertainty, no speculative backlog.

**Final directive:** Prioritize a reliable second and third Run with actual fresh Ctrl+V ownership. Then keep-visible preference. Treat tooltip as lower priority. Use named agents, one writer, small commits, bounded focused checks, and stop for user review.
