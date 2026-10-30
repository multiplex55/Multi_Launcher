# G09 KICKSTART — G06 Activity Center: Source-Grounded Native Retirement Acceptance

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Resolve the remaining G06 M2-B native owner-retirement acceptance gap for the activity view and its selected background owners, while keeping owner-specific cancellation/retirement contracts and existing low-overhead behavior.

**Hard gate:** Native proof when owner retirement is claimed; do not infer from terminal UI. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`src/activity.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/activity.rs) — `ActivityLifecycle`, `ActivityRetirement`, `ActivityCenter`, resource facts
- [`src/gui/activity_dialog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/activity_dialog.rs) — visible-only Activity UI, filter, cancel, copy sanitized diagnostics
- [`src/thread_reaper.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/thread_reaper.rs) — actual join/reap permits
- [`src/gui/screenshot_capture.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/screenshot_capture.rs) — screenshot worker cancellation/reaper
- [`src/duplicate_finder/session.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/duplicate_finder/session.rs) — owner-specific join/retirement if current path exists
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G06 M1/M2-A accepted, G06 M2-B not accepted

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G09/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Choose and document exact pending acceptance scenario:** Read PR #1455 and current owner lifecycle code; select one actual missing M2-B behavior that can be demonstrated (e.g. rapid Cancel/Reopen, stale completion post-close, join receipt). Distinguish retired, unavailable, and invalidated rows and root/window visibility.
   - **M1-A — Deterministic owner proof first:** Use blocked-worker and reaper probes with owned synchronization; show exact admission/terminal/retirement sequence and one cancellation handoff, without sleeps or polling-only assumptions. If existing contract passes, record NO_CHANGE and omit code.
   - **M1-B — Minimal correction when demonstrated:** If source and fixture identify a real violation (stale reopen, unmatched join receipt, wrong visible status), fix the first current owner boundary only. Do not install another scheduler or change global input/focus ownership.
   - **M2-A — Native specificity if needed:** For a claim involving Windows apartment/handle/capture cleanup, require a bounded supported native scenario that actually exercises that OS API. If no supported instrument/input is available, record `NATIVE_NOT_PROVEN` and do not claim feature-wide native completion.
   - **M3-A — Close with owner matrix:** Publish actual measured/proven status by owner, which UI rows are informational only, exact selected tests and any deferred targeted native proof Goal/Kickstart. No broad rebuild.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- Activity terminal status is not proof of joined/reaped thread or released native handle.
- Existing owner-specific retirement fences, reaper permits, cancel tokens, and stale-result rejection remain intact.
- Visible-only Activity sampling and sanitized metadata do not become hidden-window polling or private log export.
- Do not regress File Search ripgrep child kill/wait or Duplicate Finder reaper-held admission by changing generic Activity APIs casually.

## Exact test intent

- Deterministic owner cancellation→terminal→join/reap callback with one blocked worker and rapid reopen attempts.
- Retired/unavailable row labels, omission limits, sanitized copy and no hidden dialog refresh.
- One exact native apartment/handle case if a native claim is at issue; otherwise explicit NOT_RUN with portable protocol.

## Done report to orchestrator

Include `G09` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
