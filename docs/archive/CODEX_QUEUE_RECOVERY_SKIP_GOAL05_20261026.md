# Multi Launcher — Queue Recovery: Defer Goal 05 and Resume at Goal 06

**Execution request — not a new implementation plan.** This document is an explicit user-authorized, narrowly scoped override to the multi-goal orchestrator's strict handoff/stop policy. Execute it now; do not respond only with another plan or diagnosis. All platform permission rules and each Goal's actual correctness, safety, and acceptance requirements remain in force.

- **Queue:** `multi-launcher-queue-20261025-eedc19d2`
- **Repository:** `G:\Repos\rust\Multi_Launcher`
- **Plan root:** `docs/plans`
- **Old blocked Goal:** 05 / M1-A, Plugin Settings horizontal scrolling and native filter restoration
- **New active Goal:** 06, Application activity, cancellation recovery, and resource stability
- **Authoritative carried implementation anchor:** `ceff1f17fb09545d8b9a6010aae9eca7892a7ea7` (`CARRY04`, accepted Goal 04 M1-B; includes Goals 01–03 and accepted Goal 04 M1-A/M1-B)
- **Goal 05 branch:** `codex/plugin-settings-scroll-queue-20261025`
- **Goal 05 pushed M0-A baseline:** `ff9552991ab4007e6265f4e9d2312deb36ce4944`, whose parent is `CARRY04`. M0-A is **not** accepted Goal 05 feature work.

These identities come from GitHub at prompt preparation. Before acting, reconcile them against current local Git, recorded queue state, and other queue-owned active processes. The repository and durable ledger, not this snapshot, establish whether another Goal has since started. Never invent or overwrite a later accepted commit.

## 1. Authorized decision and status classifications

1. **Stop Goal 05 now.** Do not repair the native filter-restoration test again, renew its exhausted verification budget, run another native acceptance campaign, or expand its UI harness. No final Goal 05 pass or M1-A commit exists in the supplied status.
2. Record Goal 05 as `DEFERRED — mandatory native acceptance unresolved`. Preserve all implementation edits, partial tests, receipts, screenshot evidence, agent reports, source hashes, and Git metadata. Do **not** call it `Completed`, invent `FINAL05`, or imply the horizontal-scrolling behavior is verified.
3. Keep Goal 04's M2-A/M2-B editor work `DEFERRED` and preserve its original branch/worktree and local evidence; retain accepted M1-A/M1-B work in `CARRY04`. Do not resume Neovim/Lua/DAP observer campaigns.
4. This is explicit authorization to **advance from an uncompleted, functionally independent Goal**. It overrides only the master queue's rule that `BASE06` must equal a verified `FINAL05`. For this one transition set `BASE06 = CARRY04` and record `GOAL05_DEFERRED`, `FINAL05=UNAVAILABLE`, `BASE06_OVERRIDE=CARRY04` in the durable ledger. This is not an approval to weaken any acceptance gate.
5. Do not merge, cherry-pick, squash, stash, reset, checkout over, or force-push any Goal 04/05 work. Do not rewrite any previous branch or baseline history.

## 2. Minimal preflight — preserve all existing work

1. Read `AGENTS.md`, this override, the master `CODEX_KICKSTART_MULTI_GOAL_QUEUE.md`, the external `QUEUE_STATE.md`, and Goal 06's Goal/Kickstart in `docs/plans`. Use prior verified plan fingerprints and queue records rather than re-planning all 17 pairs. Read more only if a concrete compatibility issue demands it.
2. Verify the repository remote, old branch/HEAD/parent, `CARRY04`, local uncommitted work, and current queue status read-only. Verify no queue-owned tests or native app sessions are still active; do not terminate unrelated processes. If Goal 06 already has a legitimate active branch or accepted checkpoint, **resume it** instead of recreating another baseline.
3. For Goals 04 and 05, preserve the existing dirty worktrees exactly. A checkpoint that failed mandatory native acceptance remains uncommitted in its original worktree. Take any administrative progress records **outside those source worktrees** (or on the new Goal 06 branch after bootstrap), never by staging the dirty partial implementation.
4. If `CARRY04` is unavailable or its ancestry/accepted-source identity disagrees with the ledger, stop with the exact discrepancy. Do not guess another source base.

## 3. Start Goal 06 without touching the blocked checkout

1. Create a **new clean, isolated Git worktree** in a previously nonexistent safe sibling directory outside the repository (a Git *worktree*, not an application workspace) from the exact verified `CARRY04` commit. Do **not** switch the dirty Goal 05 checkout or reuse its baseline commit. Confirm the new worktree starts clean and contains the accepted Goals 01–03 and Goal 04 M1-A/M1-B capabilities, without any unaccepted Goal 04 M2-A or Goal 05 M1-A code.
2. In that isolated worktree, choose a **fresh unique task-specific branch**, proposed `codex/activity-cancellation-queue-20261026`. Check local/remote collisions; choose a new unique suffix if needed. It must not equal any earlier Goal branch.
3. As Goal 06's first checkpoint, create its **own M0-A baseline commit** on this new branch before making feature changes. Use one appropriate branch-specific ledger entry or an explicitly labeled empty baseline commit. Record original `BASE06`, new `BASELINE06`, and the new branch name; never use `ff955299` as the source baseline for Goal 06.
4. Attempt exactly one normal, non-force upstream push of that new baseline when permitted; a denied push is nonblocking. Record the result truthfully.
5. Execute Goal 06's existing checkpoints in their prescribed order, with normal feature-specific tests, review, process ownership, native verification, cleanup, commits and normal push attempts. Use the existing one-writer rule; if sub-agent dispatch fails, the earlier user-authorized parent-as-sole-Implementer and labeled parent-review fallbacks remain valid. Do not retry exhausted agent dispatch in a loop.

## 4. Revised queue handoff policy to prevent repeated stalls

This section supersedes the master queue's **unconditional halt at every blocked independent Goal** for the remaining queue, but not its acceptance standards.

- **Never equate failure with success.** If required native/UI/functional evidence is missing, label that Goal `DEFERRED — INCOMPLETE`, not `Completed`, `FINAL_NN`, or `Completed with limitations` if mandatory verification is absent.
- Keep the original Goal's specified focused correction budget. Once that budget is exhausted, perform **one** final preservation/reconciliation and stop work on the failed boundary. Do not begin repeated no-change audit turns, new observer frameworks, unrequested model/editor/tool configuration work, or repeated permission prompts.
- If the blocked Goal is **functionally independent of subsequent Goals**, safely defer it, retain its own incomplete branch/worktree/evidence untouched, and advance to the next eligible Goal on a **new clean branch based on the latest verified accepted cumulative implementation commit** (`CARRY`). Record an explicit skip-base override. Never inherit unverified local edits merely because they are in the current checkout. The new Goal's M0-A and initial push still apply.
- If a Goal finishes every mandatory criterion and is verified, commit it normally; its final accepted SHA becomes the next `CARRY` anchor. Previously deferred Goals stay deferred and are not silently reincorporated.
- **Mandatory dependencies:** Goal 10 requires completed Goal 09. Goal 17 requires completed Goals 09, 10 and 12. If any required accepted capability is missing, mark the dependent Goal `SKIPPED — MISSING PREREQUISITE` and continue only to other independent Goals when safe. Never implement a partial substitute or invent a dependency completion.
- **Safety stops remain absolute:** Do not advance when process ownership/cleanup is unknown, a live queue-owned process might affect another run, user data or source integrity is uncertain, the accepted branch base cannot be verified, Git operations would overwrite dirty work, a required authorization is missing, or moving forward itself risks damaging the repository. Report the actual blocker once, not through recurring no-change status loops.
- The original scope and required verification for any Goal **actually attempted** remain unchanged. Do not suppress failed tests, weaken native checks, promote fixture-only evidence into native acceptance, silently omit mandatory features, or mark partially implemented code fully delivered.
- The policy is an approved **queue prioritization/deferral** change, not an across-the-board relaxation of testing. Require a short factual handoff receipt (current/previous state, last accepted commit, preserved partial branch, next base, and tests actually run), not a new planning project.

## 5. Practical execution discipline

- **Prioritize source and user value over harness development.** Reuse existing test tools. Do not spend an unlimited number of cycles making new verification observers or Lua/editor profiles. If a necessary native test remains blocked after the existing fixed budget, preserve and defer its Goal as above.
- Keep compilation, test execution, native UI interaction, and cleanup deadlines separate. Avoid full Nextest suites between checkpoints unless explicitly required by that Goal. Run the final relevant first-party Clippy pass only at its actual end-of-Goal boundary. Preserve original failures.
- Send concise progress reports only when there is a new accepted checkpoint, a substantive evidence change, a deferral/handoff, or a genuine safety stop. A repeat of unchanged blocked status is not progress; do not get trapped in loops of rereading/reconfirming the same failure.
- Never automatically merge into `master`; no release or forced push. The final report must distinguish completed accepted Goals, deferred Goals, skipped dependent Goals, and active Goals.
- Do not return automatically to deferred Goals 04/05; they may be handled later in standalone, carefully scoped tasks only after user authorization.

## 6. Immediate actions and required result

Perform the following now, not merely describe them:

1. Reconcile and close Goal 05 administratively as **deferred**, without modifying its preserved source worktree. Maintain Goal 04's deferred editor status.
2. Verify `CARRY04=ceff1f17fb09545d8b9a6010aae9eca7892a7ea7` is the accepted source anchor and does not include the blocked edits. If the ledger shows a later already accepted independent source commit, do not replace it—reconcile and report before selecting a base.
3. Bootstrap Goal 06 on a fresh branch/worktree from that accepted anchor and attempt its initial upstream push.
4. Execute Goal 06 autonomously under the unmodified functional requirements and this revised deferred-Goal handoff policy; then proceed through the remaining queue in dependency-safe order.
5. For the user, report concise facts: deferred Goal 04/05 status, actual Goal 06 branch/base/baseline, latest accepted cumulative SHA, pushed commits, next active checkpoint, and any genuine blocker. No claim of Goal 05 completion.

**Do not ask for another repair allowance for Goal 05, propose yet another rescue plan, or wait for a `FINAL05` that will not exist. This document authorizes the explicit deferral and clean-branch continuation.**
