# Multi Launcher — Paste Sequence Force-Clipboard Repair

**Status:** Approved for targeted Codex implementation  
**Priority:** Make **Paste-to-Advance (Ctrl+V)** paste the next sequence item instead of aborting when the clipboard changes.  
**Scope:** One small repair milestone, with two natural implementation/checkpoint phases.  
**Authority:** The **current checked-out repository** and current `AGENTS.md` are authoritative for Codex. Preserve unrelated work.  
**Supersession:** This request **explicitly supersedes** the previous `docs/plans/multi_launcher_paste_sequence_targeted_repair_plan.md` policy that *new clipboard copies during playback must stop the sequence*. The user now wants **intentional clipboard overwrite at each paste step**. Do **not** silently inherit the old policy or its conflicting tests.  
**Source references:** Paths and symbols in this plan were verified against the October 18 planning source. Reconfirm corresponding owners in the current checkout; do not require Codex to locate or read an uploaded archive.

## 1. Single desired behavior

**A user clicks Run on a Paste-to-Advance sequence, changes focus to another app, and presses physical Ctrl+V. Multi Launcher publishes that sequence step's text into the clipboard and pastes it. The next distinct Ctrl+V publishes and pastes the next step, even if other clipboard data was copied or published between presses. Clipboard revision changes must never, by themselves, cancel a run.**

The user explicitly accepts that intervening clipboard copies can be overwritten and does **not** request restoration, preservation, or notification about those copies. This is an intentional change in product semantics, **not a clipboard race that the implementation should try to defeat by repeatedly aborting**.

### Exact acceptance scenario

1. Compose or select a sequence containing two **Temporary Text** steps: `ALPHA` and `BRAVO`. Set **Paste-to-Advance (Ctrl+V)** and click **Run** or **Run Once**. Preflight completes, the editor hides, and a started session remains armed. No original clipboard capture/format/revision prerequisite prevents Start.
2. Focus an eligible external text application and press a **genuine physical Ctrl+V**: publish `ALPHA` into the clipboard, dispatch one paste, count Step 1 only after the existing settling boundary. No `ClipboardChanged` error.
3. Copy arbitrary text, file, rich content, image or another format **between steps**. Press a *new* Ctrl+V in that same or a different eligible application: overwrite whatever is now on the clipboard with `BRAVO`, dispatch one paste, finish the sequence. Do **not** stop because clipboard revisions differ from the last published step.
4. The next Ctrl+V after completion is **normal OS paste of the final step** (`BRAVO`), unless some external actor changed the clipboard *after* completion. The hotkey interception is disarmed.
5. **Automatic and Manual Advance** also publish their own next step regardless of interim clipboard copies, preserving their existing schedule/Next semantics and cross-application target selection.
6. Cancel/Escape or an actual write/input/observer error stops later steps, releases input ownership and tooltip, but **never restores an earlier clipboard value**. Whatever clipboard content exists at cancellation/failure remains; no silent retry or post-cancellation overwrite.
7. The existing passive cursor-following tooltip continues to show an active run and progress, then disappears on completion/cancel/error. It must not leak snippet/clipboard text or steal focus.

**Operational limitations kept honest:** One successful Windows clipboard publication per admitted step is the intended mechanism, followed by the existing paste dispatch. An unrelated process can still win a simultaneous write race *after* publication and before that target reads the clipboard; Windows provides no universal paste-consumed acknowledgement. Do not create polling, clipboard-fighting loops or a new native test rig to promise impossible atomic cross-process delivery. The crucial contract is **do not reject or abandon subsequent steps merely because clipboard revisions changed**.

## 2. Exact root cause and ownership map from the inspected source

The previous patch already removed full original-clipboard capture and restoration, but left a comprehensive **clipboard-revision ownership gate**. This gate now contradicts the requested behavior:

| Current owner | Verified remaining issue | Required policy change |
|---|---|---|
| `src/paste_sequence/playback.rs` → `start_with_admission` | Stores `initial_clipboard_revision` from `clipboard_revision()` before starting | Do not require or retain an initial clipboard revision to arm a run |
| `src/paste_sequence/playback.rs` → `paste_step_at` | Builds `expected_sequence` from initial/last revision; `publish_text` may reject a new copy; several `clipboard_sequence_matches` checks reject changes before/after the injected paste | Publish the **current step's text** without an expected-old-revision condition; preserve input and target/lease validation, not clipboard-change aborts |
| `src/paste_sequence/playback.rs` → `settle_physical_paste` | During the 180 ms settling period it fails and invalidates the session if the clipboard revision changed | Keep the 180 ms step/permit timing, but advance on valid session generation/permit regardless of clipboard revision |
| `src/paste_sequence/playback/windows.rs` → `publish_text` | After `OpenClipboard`, compares `GetClipboardSequenceNumber()` with caller's expected revision and returns `ClipboardChanged` before `EmptyClipboard` | Open clipboard and deliberately replace contents with the step's Unicode text; **do not compare to a prior revision** |
| `src/paste_sequence/playback.rs` → trait, session, errors/cleanup | Carries `clipboard_revision`, `expected_sequence`, `rollback_text`, last revision/text, `ClipboardChanged`, cleanup fields | Simplify only no-longer-needed fields/API and statuses; keep active-generation, cancellation, target and paste timing protections |
| `src/gui/command_host.rs` | Test-only `AppPastePlatform` implements the older platform contract | Update the existing fake implementation when the trait changes; leave main command routing/hotkey owners otherwise unchanged |
| `src/gui/paste_sequence_dialog.rs`, `src/paste_sequence/progress.rs` | `ClipboardChanged` is mapped to fatal status/error and tested | Remove or update stale fatal clipboard-change presentation and relevant tests as part of the behavior migration |

Useful already-present regression tests in `src/paste_sequence/playback.rs` include:

- `paste_to_advance_three_lazy_cross_app_steps_settle_then_fourth_is_normal_paste`
- `paste_to_advance_external_clipboard_changes_at_every_handoff_are_preserved` **(must be intentionally rewritten for forced overwrite)**
- `identical_external_clipboard_revision_stops_before_replacement_and_restore` **(must be intentionally rewritten)**
- `payloadless_clipboard_starts_all_modes_and_new_copy_before_first_paste_is_preserved` **(must be intentionally rewritten)**
- `automatic_playback_obeys_deadlines_and_retains_final_text`
- `manual_next_waits_for_current_target_without_restoring_launcher_origin`

The Rust tests use existing `FakePlatform` and callback seams. Reuse these; no new harness is necessary.

## 3. Mandatory agent runners (as configured in the repository)

The **parent Codex orchestrator** coordinates the task; it should not impersonate the separate runner roles. Use the **named agent runners already configured** by the current repository's `AGENTS.md`, `.codex/config.toml`, and `.codex/agents/*.toml`:

1. **`planner`** — actual named read-only planning sub-agent (`.codex/agents/planner.toml`; current config: GPT-6.1 Sol / high). Make **one bounded** read-only source pass and deliver **one implementer-ready packet** for the single repair milestone: files/calls, required changes, invariants, non-goals, exact focused tests and done criteria. Do not re-plan the whole launcher or modify source.
2. **`implementer`** — actual named workspace-writing implementation sub-agent (`.codex/agents/implementer.toml`; current config: GPT-6 Luna / max). The **only source writer**, given the planner's **full packet**. Make the coherent change, update the existing affected tests, run focused verification and report the diff/results. Do not independently launch further milestones.
3. **`reviewer`** — actual named read-only review sub-agent (`.codex/agents/reviewer.toml`; current config: GPT-6.1 Sol / high). **One** review after implementation: check the diff for accidental preservation of revision gates, incorrectly delivered or duplicated Ctrl+V, stale fake/test expectations, incorrect completion/disarm, target/input safety regression, and unintended changes outside Paste Sequence. Findings must be specific and ordered by severity; no code edits or broad second design.
4. **Parent** — coordinate sequential handoffs, route any **concrete** reviewer correction to the *same implementer*, inspect Git diff, create meaningful commit(s) and attempt a normal best-effort push under `AGENTS.md` policy. Pushing failure must not block local progress or trigger approval-bypass attempts.

**Do not merely say “acting as planner/reviewer.” Spawn/delegate to the configured distinct runner roles when Codex supports them.** Respect each `.toml`'s model, reasoning, write permissions and instructions; don't override models or make multiple agents edit files simultaneously. If a named runner cannot be started, record the exact limitation instead of pretending it was invoked; complete only what can be safely delegated without requesting user intervention.

## 4. One scoped implementation milestone — PS-FORCE

### PS-FORCE-A — Remove clipboard ownership as a paste precondition

**Objective:** Every admitted Paste Sequence step deliberately publishes its own text, irrespective of what happened to the clipboard before the step. The session never stops solely because a clipboard revision changed.

**Required changes:**

1. In `SequencePlaybackController::start_with_admission`, remove the need to obtain `clipboard_revision()` merely to arm a run. Keep existing `Ready → Running/AwaitPaste`, observer/lease admission and automatic-start behavior. Do not pre-publish a step just to solve startup.
2. In the `SequencePlaybackPlatform` abstraction, migrate away from `publish_text(expected_sequence, rollback_text, text)` to a **step-authored publication** contract with no prior clipboard-revision dependency (e.g. `publish_text(text) -> Result<..., ...>`). The implementer chooses the smallest idiomatic final result type. Retire `clipboard_revision`, `clipboard_sequence_matches` and old session fields **only where they no longer have a real independent use**; migrate all native/fake/non-Windows implementations and callers together.
3. In the Windows `NativeSequencePlaybackPlatform`, `OpenClipboard` / `EmptyClipboard` / `SetClipboardData(CF_UNICODETEXT)` remains the authoritative operation. **Remove** the expected-revision equality check. If the clipboard is busy, `EmptyClipboard` or Unicode publication actually fails, report a real clipboard-open/write error and stop that failed step—**do not transform it into a clipboard-change error**. Avoid unsafe publication rollback that silently reinstates older clipboard content after an unsuccessful write.
4. In `paste_step_at`, preserve preflight payload, **valid input lease/generation**, exact current physical admission for Paste-to-Advance, foreground HWND/PID/TID validation, launcher/password/exclusive-input refusal, normal SendInput failure handling and step order. Remove *only* the clipboard-change aborts before/after `paste_chord` and old-revision publication requirements. Do not add a new hook, force a previous target window, or implement a global Ctrl+V replacement.
5. In `settle_physical_paste`, preserve the **180 ms guard**, in-flight permit and atomic last-step disarm, but do not fail because an intervening clipboard read or copy produced a new sequence number. Continue to the next `AwaitPaste` state or `Completed` based on the admitted step.
6. On Cancel, failure, normal completion and process exit, keep the existing no-original-restore policy and release retained hooks/leases. If old clipboard fields are now obsolete, simplify their cleanup conditions without losing a necessary in-flight paste cooldown or making a canceled future tick publish a step.
7. Remove the `ClipboardChanged` fatal error/message or retain only if source inspection establishes a **truly independent unrelated use**; either way, a normal change to clipboard contents cannot surface that fatal status during Paste Sequence playback.

**Invariants:** Win32 clipboard publication remains real and errors remain visible; the user's focus and physical key are guarded; injected/repeated keys do not advance a step; no payloads in logs/tooltips; no changes to saved JSON formats, existing settings, preflight system-variable resolution, screenshot clipboard, clipboard history or MkMacro. Only the *policy inside Paste Sequence playback* changes.

**Suggested natural commit:** `fix(paste): [PS-FORCE-A] publish each sequence step regardless of clipboard revision` **once code compiles coherently**. If the trait migration and updated focused tests cannot be separated into passing independent commits, combine A/B into one functional commit; don't create broken checkpoint commits.

### PS-FORCE-B — Migrate focused tests and prove old modes unaffected

**Objective:** The existing test suite describes the *new approved forced-clipboard policy*, not the retired copy-detection policy.

**Required changes:**

1. Update the existing `FakePlatform`, test-only `AppPastePlatform` and non-Windows stub for the changed publication contract; do not build a new Win32/clipboard simulation layer.
2. Convert the previously “external copy must cancel” test cases into **“external copy is deliberately overwritten by the next step”** cases. Specifically cover an intervening copy **before first paste**, **between first and second**, and **during the physical settling interval**, demonstrating that a clipboard revision change does not cancel the run or block the next step. Verify focus/lease still gate actual publication.
3. Verify the two-step Paste-to-Advance case: `Run → AwaitPaste → Ctrl+V (ALPHA) → 180 ms settle → next Ctrl+V (BRAVO) → Completed`, final clipboard `BRAVO`, third paste is an ordinary OS operation, and one held/repeated/injected Ctrl+V cannot cause double step advancement.
4. Cover Automatic and Manual Advance only through the existing focused controller tests: after a deliberate intervening copy, the next scheduled/Next step still publishes its own payload. Keep delay/cooldown, current target selection and cancellation. Do not start a redesign of either mode.
5. Update stale error UI status assertions in `src/gui/paste_sequence_dialog.rs`/`src/paste_sequence/progress.rs` as necessary; tooltip is already implemented, so only touch its presentation if a concrete break is caused by this migration.
6. Remove contradictory prior test assertions rather than deleting the tests themselves. New tests must prove behavior, not merely compile.

**Suggested commit:** `test(paste): [PS-FORCE-B] cover forced clipboard overwrite across playback modes` if separate coherent checkpoint; otherwise include with A.

**Done criteria:** No `ClipboardChanged` stop path remains for ordinary external clipboard mutations; physical Paste-to-Advance survives two successive steps and finishes correctly; Automatic/Manual follow same clipboard overwrite rule; real clipboard-write/input errors remain errors; no unrelated code has changed.

## 5. Hard validation budget — prevent spiraling

- One planner source pass, **one implementation writer**, one read-only reviewer pass; **one bounded correction pass** through the same implementer if concrete findings or compilation failures require it. No secondary redesign, repetitive reviewer cycles, extra sub-agent roles or parallel writers.
- **At most two focused Cargo/Nextest test invocations during the primary implementation**, plus **one exact affected-test rerun** after a meaningful correction. Use existing `--lib` Paste Sequence test-name filters and optionally one directly related app-contract test; do not run the full playback-domain test matrix if narrow filters suffice. Compilation triggered by these tests is expected. Use `cargo check --bin multi_launcher` **only if** the edited native/main seam was not compiled by selected tests.
- **No** full Nextest run, Clippy, benchmark, cargo test all-target sweep, formatting of unrelated modules, new test runner/harness, synthetic desktop or native GUI smoke campaign.
- **No computer-use skills/tools/agents**, keyboard/mouse control, screenshot capture or live app testing. These restrictions also apply to **planner and reviewer sub-agents**. Named Codex `planner`/`implementer`/`reviewer` code agents **are allowed and required**; they are distinct from computer-control skills.
- For unchanged failure symptoms, at most one substantive correction pass and one exact focused recheck; then report a genuine blocker without fabricating results or requesting more time/test permissions from the user.
- Preserve user's dirty files, no branch switching/reset/rebase/force push. Local commit first, **one normal push attempt per meaningful commit**, continue when denied, as provided by `AGENTS.md`.
- No requests to the user for screenshots, manual testing, configuration, approvals or intermediate confirmation. The user performs **one final real-world evaluation** after Codex hands off.

## 6. Final handoff — brief and concrete

Report: current branch and commit hash(es); which revision checks and callbacks were removed or simplified; exactly how new clipboard contents are overridden at each step; the status of Paste-to-Advance, Automatic and Manual Advance; actual narrow test commands/results; one reviewer result; and explicit note that Windows interactive pasting/tooltip was **not** exercised by Codex under this task's test restrictions.

**Stop.** Do not propose a new testing architecture or extend the scope after the bounded pass. The goal is usable Paste Sequence playback, **not** preserving clipboard history or creating a more elaborate safety subsystem.
