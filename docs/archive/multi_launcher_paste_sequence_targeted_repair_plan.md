# Multi Launcher — Paste Sequence Targeted Repair

**Status:** Requirements approved. **Priority:** Make the existing Paste Sequence actually usable, especially **Paste-to-Advance**.  
**Scope:** Only Paste Sequence startup, clipboard handling, Run/Run Once playback, three playback modes, and the existing progress tooltip.  
**Source of truth:** Codex's current repository checkout and `AGENTS.md`. The inspected `master` implementation places the likely startup blocker in `src/paste_sequence/playback.rs` and `src/paste_sequence/playback/windows.rs`; inspect current equivalents before editing.  
**Execution:** One code-first implementation pass, one bounded correction pass if necessary, then finish. No user questions, no computer-control skills, no live desktop/GUI automation, no screenshots, no new input-simulation harness, no full Nextest, no Clippy, no benchmarks or unrelated cleanup.

## User-visible success criteria

1. A user creates a sequence containing two **Temporary Text** steps, selects **Paste-to-Advance (Ctrl+V)**, and clicks **Run** (or **Run Once**). The sequence becomes actively armed **regardless of what was previously on the clipboard**, including rich text, images, files, HTML or an empty clipboard. No `"cannot be safely preserved"` error.
2. On successful Run, the editor **hides without cancelling the sequence**, preserving any unsaved draft. The user focuses Notepad or another eligible application and presses `Ctrl+V`: first press pastes Step 1, next distinct press pastes Step 2. The final text remains in the clipboard; later Ctrl+V pastes the final text normally.
3. The user can change applications between steps. The sequence does not attempt to return to the application originally focused when Run was clicked. `Ctrl+V` outside a running Paste-to-Advance sequence remains completely normal.
4. **Automatic** runs its steps after the user focuses an eligible external application, according to its existing delays. **Manual Advance** runs the first step when eligible, then advances with the configured Next action. Both start when the original clipboard has non-text data.
5. The **existing** cursor-following progress tooltip appears when a run is armed, including while waiting for a destination/first Ctrl+V; it identifies the sequence, current step and next action without showing step text. It remains visible while the editor is hidden and disappears on completion, cancellation or failure. Escape and configured Cancel still work.
6. If the user copies new clipboard data during a run, **stop the remaining sequence without overwriting that newer data**. On ordinary Cancel/Failure, do not restore the clipboard content that existed before Run; leave the latest clipboard value as-is. No clipboard preservation/restore job should prevent app or dialog cleanup.

### Clear distinction about clipboard safety

**Discarding the *original* clipboard is intentional.** It must not be confused with permission to overwrite **new data copied after the sequence started**. Retain the existing per-step clipboard revision/ownership checks for the latter. Only sequence-authored text is published, and only at a safe playback boundary. Text/clipboard-backed snippet resolution still follows the existing preflight behavior; missing required snippet data should report its own actionable error rather than rejecting unrelated sequence runs because the clipboard had an unsupported format.

## Inspection and change budget

- Spend **at most one short source pass** tracing Run/Run Once → preflight → start → clipboard capture → input admission → playback and tooltip. This path is already implemented; do not redesign the general Windows hotkey system.
- Existing owners to inspect, if still present: `src/gui/paste_sequence_dialog.rs`, `src/gui/command_host.rs`, `src/paste_sequence/playback.rs`, `src/paste_sequence/playback/windows.rs`, `src/paste_sequence/admission.rs`, `src/paste_sequence/progress.rs`.
- The inspected `start_with_admission` currently requires `capture_clipboard()`, and native `capture_clipboard()` enumerates formats and rejects non-preservable data. This is the **first known cause to fix**. The Run button already attempts preflight plus autostart, and the tooltip already has a passive presenter. Verify both rather than replacing them.
- **No additional options or feature design discussions.** Use the agreed defaults. One implementer owns all edits. Read-only review is limited to directly modified behavior.

## Checkpoint R1 — Remove original-clipboard preservation as a startup requirement

- Adjust the existing playback state so Start needs only the clipboard information strictly required to avoid writing over a *newer* clipboard generation; it **must not require capturing or serializing the prior payload**. In particular, do not enumerate/validate every previous clipboard format as a prerequisite to arming a sequence.
- Preserve the guard that checks whether another actor copied something after the run began. On first publication, read/compare the expected clipboard generation; publish the sequence step into the clipboard only when it is still appropriate. If generation tracking is unavailable or access is blocked, show a specific operational error rather than the unrelated unsupported-format message.
- Remove the original-clipboard restoration requirement from normal Cancel, Failed, Completed and shutdown paths **for Paste Sequences only**. Simplify or retire now-unnecessary snapshot/cleanup requirements without rewriting generic clipboard services or upsetting existing target/input safety.
- On publication failure, do not retry by blindly overwriting the clipboard. If the user copied new data during playback, stop without clobbering it. Final sequence text must remain in the clipboard after successful completion.
- Preserve existing preflight, snippet/clipboard-history references, optional start hotkeys and saved sequence formats.

**Targeted proof:** Existing playback fake tests, plus at most a couple of focused assertions for initial non-text/empty clipboard, external clipboard changes and Cancel/final text retention. No new Windows clipboard harness.  
**Commit:** `fix(paste): [R1] start without preserving previous clipboard`.

## Checkpoint R2 — Make Run and all playback modes actually execute

- Trace and correct the smallest actual blocker between clicking **Run / Run Once**, finishing prompt preflight, activating the playback session, hiding the editor **without calling its Cancel/Close workflow**, and restoring ordinary focus to the user's chosen external application.
- Make an active Paste-to-Advance session visibly **Waiting for Ctrl+V** and ensure the existing physical Ctrl+V admission route is armed while the launcher/editor is hidden. One distinct physical press must paste one step; ignore injected/repeated presses and preserve normal Ctrl+V when idle. Do not add a second input hook or rewrite the route registry unless an observed defect absolutely requires a narrowly scoped repair.
- Ensure **Automatic** waits for a valid focused target (rather than stalling because the editor retains foreground focus) and executes successive steps as its existing timer advances; **Manual Advance** should use its existing Next button/configured shortcut and keep correct step ordering.
- Playback remains **user-driven and cross-application**. No origin HWND confinement, no forced refocus of the original target. Never paste into Multi Launcher itself, protected/password controls, or ambiguous/invalid targets.
- Confirm Run and Run Once both work with two literal text steps; Run Once must remain memory-only, and saved sequences must not be modified merely by running them. Keep existing Cancel and hotkeys working.

**Targeted proof:** Reuse the existing application/preflight/controller/physical-admission tests and add only minimal regression assertions for the Run → hidden editor → active sequence path, two Ctrl+V presses, and Automatic/Manual first-step execution.  
**Commit:** `fix(paste): [R2] activate and execute paste playback from Run`.

## Checkpoint R3 — Verify existing tooltip and finish bounded integration

- Inspect the **existing** passive progress tooltip's update/lifetime. Ensure the active and waiting states from R2 feed it immediately, and that hiding the editor does not destroy its presenter or prevent its scheduled updates. Keep mouse-pass-through, no-focus, cursor-offset placement and hiding sensitive step text.
- Do not create a second tooltip implementation. If a genuine small lifecycle issue prevents showing the existing tooltip, repair that directly. If it remains unavailable only because the execution environment lacks interactive desktop permissions, **record that limitation** instead of inventing a new overlay or repeatedly probing the desktop.
- Preserve Esc, configured Cancel, normal keyboard use outside a run, cross-app progress, and terminal tooltip cleanup. Do not introduce idle repaint loops.
- Run **one narrow, combined paste-specific test selection** proving the accepted outcomes. Only rerun an exact failing test after one meaningful correction; avoid re-running already-passing unrelated tests.

**Commit:** `fix(paste): [R3] keep playback status and tooltip visible` (only if code changes; otherwise use the R2 evidence and skip an empty commit).

## Verification ceilings and stopping rules

- **Code first.** Three small checkpoints; one implementation pass with **at most one targeted correction/review pass**. Do not reopen the architecture after acceptance unless a concrete compile/behavior failure demands it.
- **Focused checks only:** at most **two narrowly selected test invocations total** during the primary pass, plus **one exact-failure rerun if necessary**. Reuse existing in-repository fake platform/controller tests. A minimal `cargo check --bin multi_launcher` is allowed only if focused test compilation doesn't cover a changed main-route integration.
- **Absolutely no** complete Nextest run, `cargo clippy`, Rust-wide lint cleanup, Computer Use/desktop-control skills or agents, screenshots, native GUI simulation, new desktop harness, benchmark sweep, or generalized integration test framework.
- No user prompts for input, testing, permissions, intermediate confirmation or screenshots. After Codex's single bounded development/verification pass, **stop and hand it to the user for their own manual evaluation**.
- If one unchanged failure remains after the correction pass, report the exact failing boundary and remaining uncertainty. Do not mark it passing, do not fabricate GUI evidence and do not spiral to a new harness.
- Use checkpoint-sized commits. Best-effort push if authorized, but blocked pushes must not interrupt local implementation; keep local commit SHAs. Preserve unrelated edits and follow the checkout's `AGENTS.md` without overriding these task-specific scope/verification limits.

## Final report: short and factual

Summarize: **root cause fixed**, code behavior changed, commit hashes, exact focused test commands/results, and any unresolved native/tooltip uncertainties. Explicitly state whether Run/Run Once, Paste-to-Advance, Automatic, Manual, original-clipboard bypass, final-text retention, external clipboard preservation and tooltip were verified by code/tests versus unverified by live desktop input. **No next-phase proposal or request for user-operated testing.**
