# Orchestrator Kickstart — Execute Both Research Goals Sequentially

Read [ORCHESTRATOR_GOAL.md](ORCHESTRATOR_GOAL.md) completely, then execute this research program. This is an execution request for research deliverables, not a request to write another high-level plan or to modify the application.

## 1. The two goals and their fixed order

1. [Goal 01 — Static Architecture Audit](01_GOAL_STATIC_ARCHITECTURE_AUDIT.md), using [its kickstart](01_KICKSTART_STATIC_ARCHITECTURE_AUDIT.md).
2. [Goal 02 — Intelligent Search and Action Discovery](02_GOAL_SEARCH_ACTION_DISCOVERY.md), using [its kickstart](02_KICKSTART_SEARCH_ACTION_DISCOVERY.md).

Do not execute both at once. Run planner, research implementer, and reviewer sequentially within the active goal. Default to one active child at a time; do not let automatic delegation create parallel goal work.

## 2. Start from the user's actual new master

Find the real repository root and intended origin. Inspect Git status, current branch, worktrees, active Git operations, applicable AGENTS instructions and actual model/agent configuration. Fetch normally when allowed and resolve the current intended master source. Do not use an old integration SHA or a supplied source archive.

Read the source and record what is actually present. The user intends a post-merge master, but do not invent a merged PR or wait indefinitely for one. If expected features are absent, record that source fact and analyze the actual baseline.

Check for an existing matching research program before creating anything. Resume a legitimate interrupted program rather than making duplicate branches or redoing completed milestones.

For a new program, select a unique run ID and create a fresh `codex/research-static-audit-<RUN_ID>` branch from the resolved master. Preserve unrelated dirty work; use a safe isolated worktree rather than stashing, resetting, or overwriting it. Create the program's docs and M0-A only after its branch is active.

The second research goal gets its own new branch, `codex/research-search-discovery-<RUN_ID>`, from Goal 01's exact closing documentation commit. It therefore carries the audit and plans forward while preserving the same application source snapshot. Do not branch Goal 02 from an unrelated newer master or the old implementation queue.

## 3. Read all contracts once and resolve role configuration

Read the two goal contracts, both companion kickstarts, MODEL_ASSIGNMENTS.md, the follow-on templates, and progress/finding templates. Retain the constraint map instead of rereading all files every turn.

Preferred quality-oriented assignments:

- Parent/orchestrator: `gpt-6-astra`, `xhigh`.
- Planner: `gpt-6-astra`, `max`.
- Research implementer/author: `gpt-6-astra`, `xhigh`.
- Reviewer: `gpt-6-astra`, `max`, in a separate role context.

`gpt-6.1-sol` with high or xhigh effort is the documented alternative when availability or throughput warrants it. Do not substitute a label for actual model configuration. Record requested and observable resolved identity. Do not modify global models/permissions just to force a recommendation.

Existing planner/reviewer restrictions remain respected. Those agents return their proposed Markdown and complete follow-on pairs; the parent or authorized documentation writer saves them with accurate authorship. Do not give a read-only role file-write instructions it cannot perform.

If a child hits a capacity limit, retire completed owned children once and make at most one retry. Then continue with the available authorized role or parent fallback. Label a parent self-review honestly. Do not stop useful research solely because an independent reviewer or preferred model is unavailable.

## 4. Make progress visible immediately

Create `docs/research/<PROGRAM_ID>/PROGRAM_STATUS.md`, `SOURCE_BASELINE.md`, and `README.md`. Initialize the active goal's `Execution progress — mutable` section. Record actual branch/worktree/source, next checkpoint, expected outputs, and role provenance.

Commit the initial coherent baseline and attempt a normal push. At every substantive checkpoint, update the goal progress section and program index, inspect staged files, commit with the specified milestone label, and attempt one normal push. Push failure is recorded but does not block local research.

Each goal has role folders for planner, implementer, reviewer and orchestrator. Save their analysis, decisions/review, actual model record, and complete proposed follow-on work. Save documentation throughout the work; do not wait for one giant final answer.

## 5. Perform the audit, not production implementation

Execute Goal 01's checkpoints in order: source inventory; command/input flows; lifecycle/file safety; change-cost alternatives; prioritized findings; complete follow-on pairs; critical review and handoff.

Use source-grounded paths/symbols/citations. Distinguish static observations, historical evidence, inferences, hypotheses, and proposals. Search for counterevidence before asserting defects. A good audit may preserve a sound existing design rather than invent a refactor.

Do not run Cargo, rustc, nextest, Clippy, the GUI, native automation, or benchmark campaigns. Do not modify Rust source, manifests, tests, assets, AGENTS.md, global settings, or live user data. Current implementation means writing research artifacts, not changing the launcher.

For each actionable recommendation, create a detailed future `GOAL.md` and `KICKSTART.md` in a named folder. Include the actual owners, behavior change, migration steps, interfaces, invariants, dependency/base selection, small commit boundaries, focused future testing, rollback and benefit/uncertainty. A vague backlog item is not sufficient.

Close Goal 01 with a useful SEARCH_HANDOFF.md, complete package index and explicit limitations. Record its exact closing commit. Do not revisit optional audit work indefinitely before starting Goal 02.

## 6. Execute search research on the next fresh branch

After Goal 01's handoff, create Goal 02's branch from that recorded commit and establish its own M0-A.

Reuse the audit. Trace current query dispatch, ranking, usage, typed command and Universal Actions owners. Build a source-backed command/capability catalog. Preserve exact prefixes and literal data. Propose deterministic discovery/ranking, safe action eligibility, stable selection, local personalization limits, keyboard-first behavior and bounded caches.

Create a small standard-library prototype and a meaningful synthetic evaluation corpus. Prefer roughly 300 diverse cases, with explicit labels, family-level splits, forbidden outputs and limitations. Run only bounded standalone experiments under the goal contract; no Rust builds or application tests. If optional execution is unavailable, finish code/data/protocol and label results NOT_RUN. Do not invent benchmark improvements.

Produce complete future Goal/Kickstart pairs for all accepted search recommendations. Reconcile duplicates and shared-owner conflicts with audit packages. Preserve raw role opinions while selecting one canonical package per actual piece of work.

## 7. Continue through limitations; stop only for genuine safety/authority issues

Do not stall on model access, child capacity, missing optional measurements, unavailable webpages, or theoretical certainty. Use the explicit fallback, narrow claims, document the limitation, generate a bounded follow-on spike, and continue unrelated work.

Do not weaken acceptance by pretending an unavailable measurement or independent review passed. A negative result or unresolved research question is useful when precisely recorded.

If the actual repository state, permissions, or source safety prevent writing, stop mutation, preserve all existing work and write a Markdown stop report when safe. Before context/session end, record exact next action, commit coherent docs, and say Paused/resumable rather than promising background work.

## 8. Close the program without starting the follow-ons

Finish both goal dispositions, the cross-goal FOLLOW_ON_CATALOG.md, NEXT_IMPLEMENTATION_WAVE.md, DECISIONS_AND_LIMITATIONS.md, HANDOFF.md and PR_SUMMARY.md. Run cheap link/schema/ID and docs-only diff checks only.

The final branch contains both goals' artifacts. Report actual commits, pushes, missing evidence, model fallbacks and recommended next packages. Do not open/merge a PR, execute generated future kickstarts, or restart the old 17-goal queue without separate authorization.

**Start now:** reconcile the new master and any existing research state; create or resume Goal 01's research branch; write the initial visible progress checkpoint; then execute the two goals sequentially through their research handoffs.
