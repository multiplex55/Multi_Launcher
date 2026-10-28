# Codex Orchestrator Kickstart — Multi Launcher Goal Queue

**Filename:** `CODEX_KICKSTART_MULTI_GOAL_QUEUE.md`  
**Location:** `docs/plans/CODEX_KICKSTART_MULTI_GOAL_QUEUE.md`  
**Execution scope:** All 17 Goal/Kickstart pairs below, in order, with one fresh branch and one M0-A baseline per Goal.  
**Operating mode:** Autonomous sequential execution through the repository's configured planner, implementer and reviewer runners.  
**Branch policy:** A cumulative chain of separately named task branches. Goal 01 starts from the safely selected integration tip; each later Goal starts from its immediately preceding completed Goal's recorded final verified commit.  
**User review:** One manual review after the queue finishes or reaches a genuine stopping condition.

You are the parent orchestrator. Execute this queue; do not stop after summarizing it or creating another implementation plan. Each individual Goal remains the authoritative feature contract, and its companion Kickstart supplies its execution directions. Read both completely before executing that pair.

This master authorizes the inter-goal branch bases, handoffs and verified resumption described here. It does not authorize changes to the plan documents, unrelated features, an automatic merge into master/main, history rewriting, release publication or an indefinite validation campaign.

## 1. Instruction ownership and the narrow queue overrides

Follow platform permissions and tool restrictions. Within the project's task instructions, apply the user's current explicit instructions first, then these responsibilities:

| Authority | What it controls |
|---|---|
| Individual Goal | Functional requirements, scope, existing checkpoint order, safety invariants, acceptance criteria, verification restrictions and resource limits. |
| Companion Kickstart | Goal-specific execution directions and delegation requirements, except for the explicit queue overrides below. |
| This master Kickstart | Queue order, cumulative branch-base choice, exactly-once bootstrap, inter-goal handoff, progress/resumption and consolidated reporting. |
| AGENTS.md and current runner configuration | Ordinary engineering conventions, actual available agent roles, configured models/reasoning, permissions and compatible repository workflow. |

Apply these explicit reconciliations without rewriting the individual documents:

1. **Pair-only stopping instructions:** “Execute this pair only,” “stop after this Goal,” and “do not start another Goal” mean finish and close that Goal's work. After the handoff gate passes, this master authorizes the parent to begin the next listed pair.
2. **Default integration-branch base:** Goal 01 uses the safe integration base. Goals 02–17 use the cumulative bases in Section 4. This is the combined-execution override anticipated by the individual pairs, not an inferred functional dependency.
3. **Fresh branch and baseline:** Fulfill the individual Goal's M0-A exactly once. The parent's bootstrap satisfies the Goal and its Kickstart together. Do not add a second B0, second baseline or second branch.
4. **Dirty work:** Unrelated uncommitted user work stays untouched in its original checkout. Use a clean isolated worktree when needed, or report the exact blocker. The standalone permission to fall back to an arbitrary current HEAD does not override this queue's selected base.
5. **Verified resumption:** A confirmed interrupted execution of this same queue resumes its recorded branch and baseline. This is not a new Goal entry or permission to reuse an unrelated branch with a similar name.
6. **Final lint sequencing:** Complete source/documentation changes, formatting, tests, native checks and reviewer corrections before the final successful Clippy pass for that Goal. After it, only non-source administrative closure and handoff remain.

Every other requirement remains binding. In particular, a queue-level summary cannot relax required native evidence, large-file coverage, performance evidence, file safety, cancellation ownership or a prohibition on broad testing.

## 2. Read-only startup and required inputs

Find the repository root using Git, not an assumed shell directory. Read:

- `AGENTS.md` and applicable nested instructions.
- `.codex/config.toml`.
- `.codex/agents/planner.toml`.
- `.codex/agents/implementer.toml`.
- `.codex/agents/reviewer.toml`.
- This complete master Kickstart.
- Every Goal and companion Kickstart in Section 3, in full. Bounded parallel read-only reading through configured agents is acceptable; retain a complete per-pair constraint map and inspect their findings.

Resolve `PLAN_ROOT` to the absolute `docs/plans` directory containing these supplied files. Record exact paths and content fingerprints. All 34 files must exist and match the intended pairs; never silently substitute an older similarly named plan.

The source checkout used for implementation is authoritative. Historical source citations in the Goals are planning provenance, not an instruction to reset or check out that old commit. No source ZIP, earlier chat or planning attachment is needed.

Before any application edit, build, test, benchmark or implementer write:

1. Inspect repository identity, `git status -sb`, staged/unstaged/untracked changes, current branch, HEAD, remotes/upstream and the actual default integration branch.
2. Determine whether a matching queue ledger already exists. If so, perform Section 12's resume checks before creating anything.
3. On a new run, use a normal safe fetch of the intended remote where available. Determine the appropriate current default-branch tip without changing the user's branch or history. If remote verification is unavailable, record the last known local integration commit and the limitation.
4. Resolve actual runner dispatch, writable execution location, owned evidence location and the required Windows/native capabilities. Capability discovery is not permission to run application verification before M0-A.
5. After the read-only safety checks, create the minimal external queue-intent record described in Section 11 before the first branch/worktree mutation. This administrative record is the only pre-branch write authorized here; no tracked repository edits, feature changes, builds or tests occur.
6. Read the exact current pair's requirements again when entering it. Inspect only enough current source to validate its owners and prerequisite capabilities; then proceed to its stated implementation checkpoints.

Do not spend the startup phase generating another architecture proposal, an exhaustive repository inventory or a new set of feature plans. The approved Goals already supply those contracts. A performance Goal's explicitly required baseline checkpoint still must be performed after M0-A.

If a clean execution worktree does not contain untracked supplied plans, continue reading the verified absolute `PLAN_ROOT` in the original control checkout. Pass those absolute document paths and the active execution-worktree path to every agent. Do not stage, copy into commits or rewrite the user's plan inputs merely to make them appear in another worktree.

## 3. Fixed ordered manifest

All paths below are repository-relative. Links are relative to this master in `docs/plans`. The exact Goal title and filenames identify each pair; the number fixes execution order.

1. **01 — Bounded developer workflows, build evidence, and focused test feedback**

   Goal: [docs/plans/CODEX_GOAL_01_DEV_WORKFLOWS.md](CODEX_GOAL_01_DEV_WORKFLOWS.md)  
   Kickstart: [docs/plans/CODEX_KICKSTART_01_DEV_WORKFLOWS.md](CODEX_KICKSTART_01_DEV_WORKFLOWS.md)

2. **02 — Faster isolated tests and efficient GUI reference coverage**

   Goal: [docs/plans/CODEX_GOAL_02_TEST_EFFICIENCY.md](CODEX_GOAL_02_TEST_EFFICIENCY.md)  
   Kickstart: [docs/plans/CODEX_KICKSTART_02_TEST_EFFICIENCY.md](CODEX_KICKSTART_02_TEST_EFFICIENCY.md)

3. **03 — Measured compilation boundaries, dependency cleanup, and test target consolidation**

   Goal: [docs/plans/CODEX_GOAL_03_COMPILE_DEPENDENCIES.md](CODEX_GOAL_03_COMPILE_DEPENDENCIES.md)  
   Kickstart: [docs/plans/CODEX_KICKSTART_03_COMPILE_DEPENDENCIES.md](CODEX_KICKSTART_03_COMPILE_DEPENDENCIES.md)

4. **04 — Faster Windows linking and responsive Neovim rust-analyzer workflows**

   Goal: [docs/plans/CODEX_GOAL_04_LINK_EDITOR.md](CODEX_GOAL_04_LINK_EDITOR.md)  
   Kickstart: [docs/plans/CODEX_KICKSTART_04_LINK_EDITOR.md](CODEX_KICKSTART_04_LINK_EDITOR.md)

5. **05 — Keep Plugin Settings usable with long descriptions**

   Goal: [docs/plans/CODEX_GOAL_05_PLUGIN_SETTINGS_SCROLL.md](CODEX_GOAL_05_PLUGIN_SETTINGS_SCROLL.md)  
   Kickstart: [docs/plans/CODEX_KICKSTART_05_PLUGIN_SETTINGS_SCROLL.md](CODEX_KICKSTART_05_PLUGIN_SETTINGS_SCROLL.md)

6. **06 — Application activity, cancellation recovery, and resource stability**

   Goal: [docs/plans/CODEX_GOAL_06_ACTIVITY_CANCELLATION.md](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md)  
   Kickstart: [docs/plans/CODEX_KICKSTART_06_ACTIVITY_CANCELLATION.md](CODEX_KICKSTART_06_ACTIVITY_CANCELLATION.md)

7. **07 — Launcher readiness, large-result display, and startup responsiveness**

   Goal: [docs/plans/CODEX_GOAL_07_LAUNCHER_RESPONSIVENESS.md](CODEX_GOAL_07_LAUNCHER_RESPONSIVENESS.md)  
   Kickstart: [docs/plans/CODEX_KICKSTART_07_LAUNCHER_RESPONSIVENESS.md](CODEX_KICKSTART_07_LAUNCHER_RESPONSIVENESS.md)

8. **08 — Lower idle work and configurable live-pin performance**

   Goal: [docs/plans/CODEX_GOAL_08_IDLE_LIVE_PINS.md](CODEX_GOAL_08_IDLE_LIVE_PINS.md)  
   Kickstart: [docs/plans/CODEX_KICKSTART_08_IDLE_LIVE_PINS.md](CODEX_KICKSTART_08_IDLE_LIVE_PINS.md)

9. **09 — Large CSV files with bounded memory and complete-file operations**

   Goal: [docs/plans/CODEX_GOAL_09_LARGE_CSV.md](CODEX_GOAL_09_LARGE_CSV.md)  
   Kickstart: [docs/plans/CODEX_KICKSTART_09_LARGE_CSV.md](CODEX_KICKSTART_09_LARGE_CSV.md)

10. **10 — Table transformations summaries and JSON/Markdown exchange**

    Goal: [docs/plans/CODEX_GOAL_10_TABLE_OPERATIONS.md](CODEX_GOAL_10_TABLE_OPERATIONS.md)  
    Kickstart: [docs/plans/CODEX_KICKSTART_10_TABLE_OPERATIONS.md](CODEX_KICKSTART_10_TABLE_OPERATIONS.md)

11. **11 — Configure Jump Mode activation and ergonomic hint letters**

    Goal: [docs/plans/CODEX_GOAL_11_JUMP_CONFIGURATION.md](CODEX_GOAL_11_JUMP_CONFIGURATION.md)  
    Kickstart: [docs/plans/CODEX_KICKSTART_11_JUMP_CONFIGURATION.md](CODEX_KICKSTART_11_JUMP_CONFIGURATION.md)

12. **12 — Redact screenshots with solid fill, blur and pixelation**

    Goal: [docs/plans/CODEX_GOAL_12_SCREENSHOT_REDACTION.md](CODEX_GOAL_12_SCREENSHOT_REDACTION.md)  
    Kickstart: [docs/plans/CODEX_KICKSTART_12_SCREENSHOT_REDACTION.md](CODEX_KICKSTART_12_SCREENSHOT_REDACTION.md)

13. **13 — Explore folder sizes with bounded background scanning**

    Goal: [docs/plans/CODEX_GOAL_13_FOLDER_SIZE.md](CODEX_GOAL_13_FOLDER_SIZE.md)  
    Kickstart: [docs/plans/CODEX_KICKSTART_13_FOLDER_SIZE.md](CODEX_KICKSTART_13_FOLDER_SIZE.md)

14. **14 — Preview ZIP contents and extract an explicit selection safely**

    Goal: [docs/plans/CODEX_GOAL_14_ZIP_EXPLORER.md](CODEX_GOAL_14_ZIP_EXPLORER.md)  
    Kickstart: [docs/plans/CODEX_KICKSTART_14_ZIP_EXPLORER.md](CODEX_KICKSTART_14_ZIP_EXPLORER.md)

15. **15 — Completed macro run summaries and practical starter recipes**

    Goal: [docs/plans/CODEX_GOAL_15_MACRO_SUMMARIES_RECIPES.md](CODEX_GOAL_15_MACRO_SUMMARIES_RECIPES.md)  
    Kickstart: [docs/plans/CODEX_KICKSTART_15_MACRO_SUMMARIES_RECIPES.md](CODEX_KICKSTART_15_MACRO_SUMMARIES_RECIPES.md)

16. **16 — Hot Folder size, age, and text-content conditions**

    Goal: [docs/plans/CODEX_GOAL_16_HOT_FOLDER_CONDITIONS.md](CODEX_GOAL_16_HOT_FOLDER_CONDITIONS.md)  
    Kickstart: [docs/plans/CODEX_KICKSTART_16_HOT_FOLDER_CONDITIONS.md](CODEX_KICKSTART_16_HOT_FOLDER_CONDITIONS.md)

17. **17 — Drop data files into an appropriate tool**

    Goal: [docs/plans/CODEX_GOAL_17_FILE_TOOL_ROUTING.md](CODEX_GOAL_17_FILE_TOOL_ROUTING.md)  
    Kickstart: [docs/plans/CODEX_KICKSTART_17_FILE_TOOL_ROUTING.md](CODEX_KICKSTART_17_FILE_TOOL_ROUTING.md)

## 4. Functional dependencies and the chosen branch bases

The original functional dependencies are unchanged: Goal 10 requires Goal 09; Goal 17 requires Goals 09, 10 and 12. The other Goals are functionally independent of this set.

**For this combined run, use cumulative inheritance even for functionally independent Goals.** This preserves completed tooling and feature work throughout the queue and gives the final branch the cumulative implementation. It is a deliberate queue policy, not a claim that each feature requires all preceding features.

Define:

- **START**: the safely selected integration commit for this new queue.
- **FINAL_NN**: the immutable final HEAD recorded after Goal NN satisfies its mandatory acceptance, review and verification requirements and completes administrative closure.
- **BASE_NN**: the exact source commit from which that Goal's new branch is created.
- **BASELINE_NN**: that Goal's new M0-A commit, created on its own new branch.

Set `BASE_01 = START` and `BASE_NN = FINAL_(NN-1)` for Goals 02–17. Use the recorded SHA, not a moving branch name, a predecessor's M0-A baseline, an unverified working HEAD or a newly advanced master tip.

For this queue, “based on a required predecessor” includes a **verified cumulative descendant containing that predecessor's final verified commit and intact capabilities**. In particular, Goal 17 starts from FINAL_16, which must contain FINAL_09, FINAL_10 and FINAL_12. This interpretation is explicit so that the later completed Goals 13–16 remain in the cumulative result.

| Goal | Functional classification | Required feature predecessors | Exact normal queue base | Proposed unique branch stem |
|---|---|---|---|---|
| 01 | Independent | None | `START` | `perf/developer-workflows` |
| 02 | Independent | None | `FINAL_01` | `perf/isolated-test-fixtures` |
| 03 | Independent | None | `FINAL_02` | `perf/compile-boundaries` |
| 04 | Independent | None | `FINAL_03` | `perf/windows-link-editor` |
| 05 | Independent | None | `FINAL_04` | `fix/plugin-settings-scroll` |
| 06 | Independent | None | `FINAL_05` | `perf/activity-cancellation` |
| 07 | Independent | None | `FINAL_06` | `perf/launcher-responsiveness` |
| 08 | Independent | None | `FINAL_07` | `perf/idle-live-pins` |
| 09 | Independent | None | `FINAL_08` | `feat/large-csv` |
| 10 | Dependent | 09 | `FINAL_09` | `feat/table-operations` |
| 11 | Independent | None | `FINAL_10` | `feat/jump-configuration` |
| 12 | Independent | None | `FINAL_11` | `feat/screenshot-redaction` |
| 13 | Independent | None | `FINAL_12` | `feat/folder-size-explorer` |
| 14 | Independent | None | `FINAL_13` | `feat/zip-explorer` |
| 15 | Independent | None | `FINAL_14` | `feat/macro-summaries-recipes` |
| 16 | Independent | None | `FINAL_15` | `feat/hot-folder-conditions` |
| 17 | Dependent | 09, 10, 12 | `FINAL_16` | `feat/file-tool-routing` |

These are branch stems, not permission to reuse existing branches. Check local and known remote collisions and add a meaningful unique suffix, such as the queue identifier, when necessary.

Before every handoff, confirm the preceding Goal is eligible to supply FINAL_NN. Before Goals 10 and 17, additionally verify each required recorded final commit is an ancestor of the proposed base, for example with `git merge-base --is-ancestor <required-final-sha> <base-sha>`, and inspect that its required APIs/behavior still exist. An ancestry check alone does not establish that a later change preserved the capability.

The cumulative chain also creates a **handoff requirement**: the prior Goal must be complete before the next branch is created. This does not change the functional classifications in the table. The selected queue policy does not automatically skip blocked Goals or create an independent bypass line.

A denied push does not invalidate a verified local FINAL_NN. Later branches may inherit it locally. Record remote availability separately.

Do not automatically merge, rebase, cherry-pick, reset, force-push or move an existing branch to establish these bases. Do not update master/main. Preserve every completed Goal branch at its recorded final commit. Updates to the integration branch during the queue do not silently change START or the cumulative chain.

## 5. M0-A — Bootstrap exactly one fresh branch for the current Goal

This is the first execution checkpoint for **every newly entered Goal**. The parent owns it; the implementer must receive proof that it is complete.

### 5.1 Inspect and select a safe execution location

Recheck Git state, current worktree and owned processes. Confirm the intended BASE_NN and that no previous agent is still writing.

When the checkout contains unrelated uncommitted work, leave it in place. Use a separate clean Git worktree at the required BASE_NN when safe and allowed. Reuse that owned execution worktree for later Goals so paths and caches remain consistent. Do not copy unrelated dirty changes, stash them, stage them, discard them or make their current HEAD choose the queue's base.

A clean existing checkout may be used when no unrelated work will be disturbed. Never switch away from active user work. If a safe worktree/branch transition cannot be established, record the blocker before implementation. An inability to create a clean execution location is not permission to use the wrong base.

### 5.2 Create and switch to the unique task branch

Choose the table's idiomatic stem or an equally descriptive lowercase name with an appropriate prefix. Confirm it is unused. Record the intended branch, BASE_NN, worktree and bootstrap phase in the external queue ledger before mutation. Create the new branch from the exact BASE_NN and make it active in the execution worktree.

A normal `git switch -c <new-branch> <base-sha>` in a safe clean worktree, or `git worktree add -b <new-branch> <owned-path> <base-sha>` when creating an isolated worktree, can perform this step. Resolve and quote actual arguments safely; these are templates, not commands with literal angle-bracket placeholders.

Do not use force checkout, overwrite a branch, change upstream to an unrelated repository or reuse a branch merely because its name matches the intended feature.

### 5.3 Initialize the branch record and commit the baseline

After the new branch is active, confirm its previously recorded intent and initialize the branch-specific execution section in the durable queue ledger described in Section 11. Record queue ID, Goal/Kickstart fingerprints, source branch, BASE_NN, actual worktree and new branch name. Any tracked branch-specific execution record is created only now.

Prefer a small legitimate tracked task execution record following repository conventions. If a new tracked record is appropriate, a narrowly owned path such as `docs/plans/execution/<queue-id>/goal-NN.md` may be used; this is runtime bookkeeping, not a new feature plan. Include the queue ID, pair identity, original base and intended checkpoint list.

If no appropriate tracked bootstrap change is needed, use one explicitly empty baseline commit. For example:

```text
chore(plan): [M0-A] establish goal NN <short-name> execution baseline
```

Inspect the staged diff first. Stage only the legitimate task record, if any. Do not commit the supplied plans or pre-existing user changes to manufacture a baseline.

Commit on this new branch and record BASELINE_NN. Confirm its parent is the selected BASE_NN and the branch is active. Every later per-Goal diff is measured against BASELINE_NN; retain BASE_NN separately for provenance. Never rewrite either.

A commit cannot contain its own resulting SHA. Record that SHA in the external queue ledger immediately, and include it in the next legitimate tracked execution-record update if one exists. Do not create recursive hash-recording commits.

### 5.4 Attempt the initial upstream push

After the baseline exists, attempt one normal non-force upstream push when the intended remote, access and platform approvals permit, for example `git push -u origin HEAD` only after verifying origin.

Record the command, destination, commit and actual result. If denied or unavailable, retain the branch and baseline and continue locally. A known continuing policy denial is recorded without repeatedly requesting the same approval or attempting a bypass. Never alter security settings, force-push or recreate a branch to solve a push failure.

### 5.5 Bootstrap acceptance

Do not admit feature edits, application builds, tests, benchmarks or later checkpoints until all of these are true:

- The actual unique Goal branch is active in the intended execution worktree.
- Its exact source base matches the queue table and verified prerequisite state.
- Its own M0-A baseline commit exists and its SHA is recorded.
- The initial push was attempted once when permitted, or its genuine policy/access unavailability is recorded.
- Unrelated user work is intact and unstaged.
- The parent has recorded pair identity and the first implementation checkpoint.

Stay on this same branch throughout this Goal: all implementation, tests, source modifications, checkpoint commits and push attempts use this new Goal branch. Only Section 10's completed-Goal handoff or Section 12's verified resumption permits the corresponding transition. Do not silently return to master/main or another feature branch.

## 6. Use the actual configured sub-agent runners

Use the real `planner`, `implementer` and `reviewer` role definitions from the current .codex configuration and the actual supported runner-dispatch mechanism. Preserve their model, reasoning, sandbox, permission and concurrency settings. Do not treat a commented-out alternative as active, change configurations to bypass a restriction, or invent a CLI/tool invocation.

At the planning revision, these roles were configured as read-only planner, workspace-write implementer and read-only reviewer, with a four-thread overall cap. Rediscover the current values at execution rather than hard-coding those historical settings.

| Role | Responsibilities and limits |
|---|---|
| Parent orchestrator | Own queue state, branch/worktree/bootstrap operations, checkpoint acceptance, commits and normal push attempts, bounded corrections, handoffs and final reporting. Write administrative records only; delegate feature/source edits to the implementer. |
| Planner | Read-only, bounded current-source inspection. Confirm the next checkpoint's owner, prerequisites, invariants, limits and focused verification. Produce one implementer-ready packet; do not redesign or re-plan the entire queue. |
| Implementer | Sole source-writing agent. Implement exactly one assigned checkpoint or bounded correction packet, run its permitted focused verification and report actual results. Do not create/switch branches, make independent commits/pushes, start the next checkpoint or broaden scope. |
| Reviewer | Independent read-only inspection of the checkpoint diff and relevant surrounding source against the Goal. Report concrete defects, missing mandatory evidence and scope violations. Do not edit code or initiate an unrelated test campaign. |

Keep at most one implementer writing in the execution worktree. Do not run overlapping builds/tests that compete for the same target directory, clipboard, hooks, native windows or other shared resources. Read-only analysis can run concurrently when it saves time, but review a stable submitted checkpoint state; do not treat observations of a moving worktree as final review.

If the configured role dispatch is unavailable, inspect the supported mechanism and permitted setup once. Do not silently impersonate a configured role with an unrelated default runner or change model/permission settings. Report a precise capability blocker if the required workflow cannot actually be used.

Every implementer packet must identify:

- Queue ID and qualified checkpoint ID, for example `G06/M2-A`.
- Absolute execution-worktree path, active branch, BASE_NN and BASELINE_NN.
- Exact Goal and companion Kickstart paths; confirmation that M0-A is already complete.
- Checkpoint objective, authorized owners/files, tasks, behavior, invariants and non-goals.
- Concrete required tests/native evidence, profiles, resource/process ownership and deadlines.
- Acceptance conditions, expected result report and explicit stop at this checkpoint.

Use the Goal's existing checkpoint contract as the packet's source. Do not ask the implementer to reconstruct requirements from a broad narrative or the earlier chat.

## 7. Execute each Goal's existing checkpoints in order

After M0-A, follow the sequence below. The full tasks, acceptance criteria and commit subjects remain in the individual Goal; these identifiers are navigation, not substitutes.

| Goal | Ordered checkpoints after M0-A |
|---|---|
| 01 | `M1-A` → `M1-B` → `M2-A` → `M2-B` → `M3-A` |
| 02 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |
| 03 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |
| 04 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |
| 05 | `M1-A` |
| 06 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |
| 07 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |
| 08 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |
| 09 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |
| 10 | `M1-A` → `M1-B` → `M2-A` |
| 11 | `M1-A` → `M1-B` → `M2-A` |
| 12 | `M1-A` → `M1-B` → `M2-A` |
| 13 | `M1-A` → `M1-B` → `M2-A` |
| 14 | `M1-A` → `M1-B` → `M1-C` → `M2-A` |
| 15 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |
| 16 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |
| 17 | `M1-A` → `M1-B` → `M2-A` → `M2-B` |

For each checkpoint:

1. The planner performs a short delta check against current code and the already approved checkpoint. Reuse previous findings unless the state changed materially.
2. The parent sends one explicit packet to the implementer. The implementer codes and runs only the checkpoint's relevant verification.
3. On return, inspect actual changed files, test discovery/results, process cleanup and evidence. An implementation claim is not proof that an acceptance criterion passed.
4. Obtain an independent bounded review of meaningful changes. A small single-checkpoint Goal may combine this with its final review; avoid duplicating the same review without new evidence.
5. Send concrete relevant findings back as a bounded correction packet. Keep the Goal's remediation budget and do not reopen unrelated work.
6. When the checkpoint is coherent and its applicable acceptance is satisfied, inspect the staged diff, commit with the Goal's existing checkpoint label/subject, and attempt one normal push when permitted.
7. Record the commit SHA, verification receipt and push status, then advance within the current Goal.

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The individual Goals already define those boundaries; preserve them. Do not compress a large Goal into one final commit or invent empty implementation checkpoints for unachieved requirements.

Stage explicit task-owned paths/hunks. Preserve prior commits and user work. Use additive remediation commits when necessary; never amend away M0-A, squash checkpoint history, or rewrite a completed Goal branch.

Where a Goal expressly allows an evidence-backed no-change disposition, record the investigation and actual result. That allowance does not override a specific checkpoint requiring a measured improvement. No benefit, uncollected measurements or unavailable mandatory evidence cannot be presented as an achieved performance objective.

## 8. Goal-specific constraints that the queue must preserve

These reminders identify important integration and verification differences. Read each complete pair; requirements not repeated here remain binding.

### 01 — Developer workflows

Reuse existing build profiles, command construction, measurement and owned-process harnesses. Prefer sufficient installed Nextest record/replay capabilities. Preserve resolved command arguments and the original failure receipt; zero discovered tests fails. Verify controlled success, nonzero exit, timeout and ownership-failure cases before trusting cleanup. Do not widen the restricted performance runner to arbitrary commands or add feature 136.

Specific ceilings: 30 minutes compilation, 10 minutes selected tests, 5 minutes post-build native smoke; at most three smoke captures. No full Rust suite merely to validate these tools.

### 02 — Efficient tests and isolation

Preserve independent GUI reference calculations, meaningful large-list cases, real watcher delivery and necessary service integration. Inert test constructors must not become unconditional production switches. Retire owned workers before restoring environment/current directory or deleting fixture data. Coordinate real shared Windows resources across processes; a process-local mutex does not isolate separate Nextest processes or the user's personal launcher.

Specific ceilings: 30 minutes compilation and 10 minutes per selected execution batch. Retain the required efficiency evidence.

### 03 — Compilation and dependencies

Use measured costs to choose dependency/feature changes, at most eight initially consolidated compatible tiny test targets, and at most one initial pure-core boundary with two justified structural approaches. Follow the inspected table-engine candidate only if measurements justify it. Preserve the public table API, exactly one implementation owner and discoverability of moved tests.

Specific ceilings: 30 minutes per compilation command and 10 minutes affected test execution. Update Goal 01's test-selection catalog only where changed target identities require it. File splitting and fewer dependency versions alone are not accepted speed evidence.

### 04 — Linker and Neovim

Keep release/full-debug defaults and Windows resources intact. Limit exploration to two supported linker candidates. Apply project-scoped Neovim rust-analyzer settings without overwriting global preferences or bypassing local-config trust. Validate the actual editor's effective settings and behavior; configuration text or a headless analysis command is insufficient.

Specific ceilings: 30 minutes compile/link, 5 minutes per editor scenario and 2 minutes post-build application smoke. Respect the required link and editor evidence.

### 05 — Plugin Settings scrolling

Keep one surgical implementation checkpoint. Bound the window/list width and provide both scroll axes while preserving filtering, async saves, Settings Search and Jump identities. Use the existing fixture/harness; no new scroll framework or preference.

Specific ceilings after compilation: 3 minutes focused tests and 5 minutes native inspection. Required evidence proves actual horizontal movement and reachable controls, not merely scrollbar presence.

### 06 — Activity, cancellation and stability

Keep execution ownership in existing tools. Read bounded owner-published snapshots; do not call synchronous worker request/reply while painting. Distinguish cancellation request from worker retirement and safe admission of replacement work. Report real accounting and unknown values truthfully; do not fabricate per-tool CPU/RSS.

Keep terminal history bounded and view-driven refresh inactive when closed. Use 30 warm lifecycle cycles with a 10-minute execution ceiling, scoped owner tests and required native activity/cancellation evidence. Later Goals reuse these current adapters where appropriate.

### 07 — Readiness and result display

Preserve existing focus/visibility ownership, passive-note and modal behavior, search ranking, result identity and geometry. Do not replace ownership with unconditional focus requests or synthetic key workarounds. Distinguish useful CPU frames from actual native accepted typing/presentation.

Freeze workload/profile definitions before candidates; one candidate batch per checkpoint and at most three matched repeats only for meaningful variance. Use the specified normal/large synthetic profiles and focused tests. Missing required native typing evidence is a blocker.

### 08 — Idle work and live pins

Preserve the whole publish/enqueue/wake contract, including work queued before attachment. Reuse the current Hot Folder cached owner state if Goal 06 established it. Rate changes must affect the worker's wait predicate; notification alone is insufficient.

Preserve the 34 ms legacy cadence, explicit 5/10/15/30 refresh choices, bounded frame ownership, pause/source-loss behavior and child-only repaints. Use 60-second idle samples, up to three repeats only when inconclusive, controlled-clock cadence tests and one native smoke capped at 10 minutes.

### 09 — Large CSV foundation

Provide complete-file filtering, stable sorting and streamed exports alongside paged/projected browsing. Preserve compact-tool behavior and read-only sources. Enforce the Goal's aggregate memory, temporary-file, source-shape and free-space budgets before excessive allocation; its 256 MiB default table budget is not a whole-process RSS promise.

Required acceptance includes an actual deterministic file larger than 500 MiB, late-row evidence, cancellation/reopen and resource retirement. Specific ceilings: 45 minutes initial build, 25 minutes later changed-target build, 10 minutes selected tests, 20 minutes large-data pipeline. No full repository suite. Preserve the generated fixture's provenance for reuse by 10 and 17.

### 10 — Table operations

Verify Goal 09's actual row/source/selection/resource/spill/export contracts before implementation. Follow the current engine owner if Goal 03 moved it behind a facade. Do not create a second resident-only engine or restore an obsolete implementation path.

Preserve complete operation scopes, source-bound preview/apply/undo/discard, exact deduplication/distinct counts, checked decimal behavior and the bounded JSON import/Markdown rules. Reuse the verified large fixture for independent full-data expected results. The 45/25/10/20-minute build/test/workflow ceilings apply as specified in Goal 10; no automatic full suite.

### 11 — Jump configuration

Preserve disabled-by-default Jump, Shift+Space and A–Z defaults, local activation scope, prefix-free hints, input release/text fences and Enter execution. Apply a configuration change only after successful persistence and retire the current session safely. Hotkey Directory must describe local applied status truthfully.

No new global hooks, multi-chord sequences or Jump target expansion. Cap the selected test batch at 10 minutes and native smoke at 10 minutes. Required alternate-chord/alphabet evidence includes text-input preservation.

### 12 — Screenshot redaction

Solid, Blur and Pixelate must share composed preview/export pixels and bounded history/cache ownership. Solid is initially opaque black. Preserve crop, zoom, markup ordering and undo/redo; later effects must not restore pixels hidden by earlier layers. Blur/pixelation are visual obscuring, not guaranteed removal.

Cap targeted screenshot tests at 8 minutes and native smoke/export inspection at 10 minutes. Inspect real exported files. Clipboard-byte inspection is conditional where supported, but the required native editor/export workflow is not optional.

### 13 — Folder-size explorer

Scan metadata from an explicitly selected root without following descendant reparse paths or reading file payloads. Logical length is counted per included path; unknown/inaccessible subtrees remain visibly incomplete. Keep aggregation/sorting, retained metadata, queues and cancellation ownership bounded.

Use generated known trees, representative Windows no-follow/hard-link checks and required native drill-down/cancel/reopen evidence. Cap selected tests and native smoke at 10 minutes each. An unavailable privilege-dependent variant is reported individually and cannot waive the representative acceptance.

### 14 — ZIP preview/extraction

Keep listing separate from explicit selected extraction. Retain duplicate-entry identity, parser/metadata bounds, Windows path and collision rejection, race-resistant destination confinement, actual output caps, integrity checks, no-clobber publication and cleanup of only identity-verified owned partials.

Do not add overwrite, archive creation, other formats, password support or automatic execution. A failed confinement or overwrite check blocks handoff. Cap targeted tests at 12 minutes and native smoke at 12 minutes; those specific limits supersede the generic fallback.

### 15 — Macro summaries and recipes

Capture one terminal summary from actual run admission, including failed preparation/pre-start cancellation and truthful source/subset. Measure real pause transitions and instruction visits; distinguish inclusive timing and truncation. Enforce history/detail/string budgets while runs are active, using a diagnostic whitelist without source/clipboard/variable payloads.

Recipes use the existing template-copy mechanism, new identity, cleared hotkeys and editable drafts; no autorun. Use controlled event/time tests and one native workflow capped at 10 minutes. Do not expand verification into all recording/OCR/image suites.

### 16 — Hot Folder conditions

Use AND-combined size, event-time modified age and bounded literal UTF-8 predicates. Legacy rules default to disabled conditions; preserve deliberate migration and old-reader fail-closed behavior. The first fully matching authorized rule wins; an earlier filename match that fails or is unevaluated cannot suppress a later match.

Share the safe rooted reader/evaluator with Test Rule. The 64 MiB content cap and two-second cooperative processing deadline are aggregate across candidate rules, not per rule; native blocking I/O still retains ownership until retirement. No aging scheduler or pre-existing-folder scan. Use injected time/streams, disposable data and one 10-minute native fixture.

### 17 — File-drop routing

Verify recorded 09/10/12 ancestry and current capabilities in FINAL_16. Supported data files open the chooser with Add as launcher action; preserve executable, shortcut, directory and unsupported-file handling. Use production source adapters, off-UI bounded probing and the explicit 32-item queue. Opening a file must not silently execute, extract, overwrite a draft or change clipboard contents.

Keep the file-origin text/JSON limits separate from large CSV support. Pass a large CSV descriptor without loading a duplicate full file. Preserve image Save As safety and truthful Clipboard Modify baseline/copy behavior.

Use one 15-minute native route batch plus one separately bounded large-file open. Reuse Goal 09's fixture, inspect actual >500 MiB routing evidence, and do not rerun the earlier transformation benchmark campaign.

## 9. Focused verification, budgets and Clippy-last closure

### 9.1 Verify the affected behavior

Use the selected Goal's Sections F, H, I and J as the verification contract. Specific per-Goal budgets take precedence over shared defaults and over a helper's built-in default. Configure an explicit bounded override when needed; a five-minute native-wrapper default must not silently replace a Goal's authorized 10/12/15-minute scenario.

Select the actual current Cargo package/target/features as well as Nextest cases/filtersets. Validate nonzero discovery. Preserve resolved argument arrays, compiler profile, runner profile, tool versions, source identity, exit status, timeout/cleanup state and first failure evidence. Use installed tool capabilities rather than unverified CLI flags.

Use Goal 01's integrated workflow tools where they provide the required semantics. Adapt their test catalog when later approved changes move targets. Do not rerun a completed Goal's acceptance campaign merely because a later Goal uses its helpers.

Run focused tests during implementation. Broaden only for an explicit Goal requirement or a concrete affected boundary/failure that narrower checks cannot cover; record the reason and respect any prohibition. **This master adds no per-Goal or queue-end full Nextest requirement.** If a permitted full suite becomes necessary, run it near that Goal's end, not between checkpoints.

### 9.2 Native and resource evidence

Use owned disposable profiles and fixture data. Before constructing every app test/profile, apply the repository's required Paste Sequence shortcut isolation, including `next_hotkey = None` and `cancel_hotkey = None` where its current instructions require them. Keep production defaults unchanged.

Reuse the owned native harnesses to build, launch, exercise, capture, inspect and close the selected surface when permitted. Tie screenshots/receipts to the owned process/window, coordinate shared resources and verify cleanup. Do not terminate unrelated processes, mutate personal files or overwrite newer clipboard contents while restoring a fixture.

Keep compilation deadlines separate from test/native execution. Enforce finite owned-process deadlines; a timeout, missing receipt, zero-case selection or uncertain cleanup is not a pass. Use the Goal's limits; if none apply, the shared fallback is 45 minutes for one Windows compile and 10 minutes for a selected test process, with a finite recorded native-probe budget.

At most two focused remediation attempts per failing acceptance boundary are allowed unless the Goal sets a different limit. First diagnose the meaningful root cause and use the smallest useful recheck. Do not repeat the same failed command indefinitely or widen the project to avoid admitting a blocker.

Missing Windows/native capability does not turn a required check into an optional one. Finish unaffected work within the current Goal and record the unverified criterion accurately.

### 9.3 Final verification order

Complete the following before declaring a Goal ready for handoff:

1. All implementation checkpoints and documentation changes.
2. Cumulative diff review against BASELINE_NN, including directly affected inherited interfaces.
3. Meaningful reviewer corrections, relevant formatting and all required focused tests, measurements, native evidence and owned-process cleanup.
4. Preparation of any final tracked execution record. Final hashes/results that cannot yet be known belong in the external queue ledger.
5. Stage all intended, reviewed task changes and confirm that the index matches the working-tree source, tests, build/configuration inputs and documentation that were verified. No unstaged task inputs or untracked source dependencies may remain. Owned generated logs, caches and external fixtures are separate. Record the exact staged tree identity, for example with `git write-tree`; if everything is already committed, record the matching HEAD tree.
6. Run the appropriate affected first-party Clippy invocation against that same working-tree state as the last verification phase. Exclude vendored crates from independent cleanup and keep unrelated pre-existing findings separate.

A failed Clippy may lead to bounded relevant fixes, formatting and necessary focused rechecks, followed by another final Clippy attempt. Do not edit the source after a successful final Clippy and continue calling that earlier result final.

Once final Clippy succeeds and the Goal is complete, **freeze its source/test/documentation state**. Administrative inspection, committing already verified staged changes, normal push, external queue-ledger updates and final reporting are allowed. No further tracked edits, tests, native scenarios or optional benchmarks are allowed for that completed Goal.

If a Goal permits a nonblocking pre-existing lint limitation, record the exact command/result and why the Goal permits it; never label a failed command clean. A mandatory clean-lint criterion remains a blocker. Do not use broad allow attributes, vendored cleanup or weakened assertions to manufacture success.

Record which commit or exact staged tree the successful verification covered. Confirm after Clippy that no tracked input or untracked source dependency changed. If the last implementation checkpoint is committed after final Clippy, verify read-only that the final HEAD tree is exactly the recorded verified staged tree, not merely that a subset of source paths looks similar. A commit hook or external writer that changes those inputs invalidates the pending handoff: do not freeze or advance; resolve that change within the Goal and repeat only affected verification, ending with Clippy again.

## 10. Completed-Goal handoff

The parent performs this gate after the individual Goal's required work is complete. Do not create or switch to the next branch early.

1. Confirm all mandatory functional, safety, checkpoint, review and verification criteria are satisfied. Classify any expressly allowed limitation under Section 13.
2. Confirm all coherent intended work is committed on the current Goal branch, with M0-A and checkpoint history intact. No task-owned changes remain uncommitted. Do not create a success checkpoint for an unfinished requirement.
3. Verify that no agent or owned application/test worker is still writing and required cleanup has completed.
4. Record the exact final HEAD and its verification identity. That immutable commit becomes FINAL_NN only if this gate succeeds.
5. Attempt the final normal push when permitted. An already successful push of this exact final commit satisfies the final-push requirement; do not duplicate it merely for ceremony. A continuing denial/unavailability is recorded without approval loops. Local verified completion may advance.
6. Write the Goal's required final report into its external queue-ledger section, including its feature-specific handoff evidence. Record branch/base/baseline/checkpoint/final SHAs, tests, Clippy, native evidence, review outcomes, limitations and actual push state.
7. Freeze the completed branch at FINAL_NN. Set the next expected base to that exact SHA and update the next action in the queue ledger.
8. Without asking for permission to continue, enter the next numbered pair and execute its own M0-A. If this was Goal 17, produce Section 14's consolidated report and stop.

The freeze applies to the completed Goal's branch and evidence. A later Goal may legitimately modify shared inherited code on its new branch and run that later Goal's affected checks. It must preserve prior capabilities and must not rewrite an earlier branch or restart its optional optimization campaign.

When a current Goal introduces a regression in inherited behavior, fix the directly caused regression under the current Goal and its budget. Discovery of an unrelated pre-existing issue does not authorize a new feature plan or history rewrite.

## 11. Durable queue ledger and evidence

Maintain **one concise queue ledger outside all source worktrees**, so actual final hashes, push results and final Clippy receipts can be recorded after a source freeze without another commit. Use an existing approved durable execution-state location if available; otherwise choose a writable owned directory near the repository, for example:

```text
<repository-parent>/Multi_Launcher.queue/<queue-id>/QUEUE_STATE.md
```

After read-only preflight, choose and record its absolute path and create a minimal external queue-intent record before the first branch/worktree mutation. Include repository/queue identity, manifest fingerprints, selected source SHA, intended branch/worktree and bootstrap phase. This is administrative state outside the repository, not a tracked source edit or an additional checkpoint. Create tracked branch-specific records and M0-A only after branch activation. This ordering makes an interruption immediately after branch creation recoverable.

Do not overwrite an unrelated directory or use a temporary location that will vanish at a normal session boundary. The ledger is local execution state, not a background service. Keep it available for resumption and include its path in progress/final reports.

Use a unique queue ID tied to this repository and manifest. Store:

- Repository identity, original control checkout, active execution worktree, absolute PLAN_ROOT and ledger path.
- Exact 34 input paths/fingerprints and the master fingerprint; actual AGENTS/runner configuration identity.
- START, actual integration branch/remote, fetch result and preserved pre-existing work.
- Ordered Goal table; current Goal/checkpoint, phase, last verified cumulative anchor and exact next action.
- For every Goal: functional dependencies, queue predecessor, source branch/BASE_NN, new branch, BASELINE_NN, initial push, checkpoint subjects/hashes, acceptance/evidence results, final Clippy, FINAL_NN, final push, status and blockers.
- Owned evidence/fixture locations, command profiles, first failures, deadlines, cleanup receipts and reviewer dispositions.

Use qualified checkpoint IDs such as `G14/M1-C` because different Goals reuse labels. Keep raw logs, screenshots and large fixtures in owned evidence locations; the ledger contains small summaries and paths. Never include credentials, clipboard contents, private source payloads or an uncontrolled environment dump.

Suggested compact table:

| Goal | State / phase | Branch | Source base | M0-A baseline | Last checkpoint / commit | Final SHA | Mandatory evidence | Push | Next action |
|---|---|---|---|---|---|---|---|---|---|
| 01 | Not started | — | START | — | — | — | Pending | — | Bootstrap 01 |

Create all 17 rows; expand each with a short section for its checkpoint records and the required Goal-specific final report. Keep operational phase separate from terminal status.

Update the ledger after each state transition, command outcome, checkpoint commit and push result. Record intent immediately before branch creation or a long owned command, then record the actual result. This allows an interrupted bootstrap or still-running process to be reconciled without repeating it.

Do not create a complex workflow engine, database or new orchestration application. A maintained Markdown ledger and existing evidence conventions are sufficient. If tracked per-Goal records are used, prepare their final content before final Clippy; keep post-freeze facts in this external ledger.

Preserve one deterministic large-file fixture across Goals 09, 10 and 17 when safe, with path, size, generator/seed, identity and expected-result provenance recorded. Reuse the data, not stale verification claims. Remove owned temporary artifacts only after their consumers no longer need them and only within their recorded ownership.

## 12. Resume safely after interruption

Interruption or context limits do not create a new queue. Read this master, the durable ledger, current AGENTS/runner instructions and the active pair, then inspect actual Git/process state.

1. Verify repository and queue identity, exact manifest fingerprints, worktree path and recorded branch. Matching branch names alone are insufficient.
2. Verify BASE_NN, BASELINE_NN and completed checkpoint commits exist, have the expected ancestry and represent this pair. Check the cumulative predecessor chain and mandatory prerequisites.
3. Inspect status and diffs. Preserve both committed work and legitimate interrupted task-owned changes. Do not reset, discard or recommit unrelated changes, and do not assume a clean ledger means a clean worktree.
4. Reconcile any command left running using its recorded process/session identity and deadline. Reattach/inspect it when supported or safely stop only owned work. Do not launch a duplicate test/build/native job or kill processes by name.
5. Compare ledger state with actual commit and evidence records. If a commit exists but the ledger update was interrupted, record it after verifying its contents; do not create a duplicate checkpoint.
6. If branch creation completed but M0-A did not, verify the intended new branch/base and finish that same bootstrap. If M0-A exists but its SHA/push receipt was not recorded, recover the actual commit and remote state. Do not create another baseline. Unknown push state is unknown, not a reason for automatic push loops.
7. If interrupted mid-checkpoint, resume the existing confirmed branch at its current safe state, using the last verified checkpoint as a reference. Re-run only evidence invalidated by changed state or lost results; do not blindly restart completed milestones.
8. If the Goal already passed handoff, do not reopen it. Confirm FINAL_NN and advance to the next not-started Goal. If the next branch was already created, reconcile its bootstrap instead of creating a second branch.

A confirmed queue-owned branch is resumed even though a *newly entered* Goal normally requires a fresh branch. This exception requires the above identity checks and does not apply to an unrelated existing feature branch.

If supplied plans changed, compare the exact changes to completed and pending scope before acting. A purely administrative correction may be recorded; a material requirement change must not silently reinterpret completed acceptance or restart work. Stop only at a real conflict requiring a decision.

Before a session boundary, checkpoint coherent verified work when possible, record unfinished task-owned changes and the exact next action, and quiesce owned processes safely. Do not invent passing acceptance merely to make a checkpoint. State that execution is paused and resumable; do not claim to continue indefinitely without an active session.

## 13. Status, blockers and autonomous continuation

Use these terminal/queue statuses consistently:

| Status | Meaning | May supply a normal cumulative handoff? |
|---|---|---|
| Completed | All mandatory implementation, safety, review, checkpoint and verification criteria are satisfied. | Yes. |
| Completed with limitations | All mandatory criteria are satisfied, but a specifically optional or expressly conditional check could not run, or the Goal explicitly permits a documented nonblocking limitation. | Yes, only with the exact permitted limitation recorded. |
| Blocked | A mandatory implementation, correctness, safety, capability or acceptance-evidence requirement remains unresolved. | No. |
| Not started | This Goal's bootstrap and implementation have not begun. | No. |

Use **In progress** or **Paused** as operational phases for an active, unfinished Goal; neither is a completion claim.

“Environment-dependent” describes why evidence is missing. It does not make required native behavior, a >500 MiB acceptance run, a required speed improvement or a confinement test optional. A generic instruction to report limitations never overrides a concrete mandatory acceptance criterion.

A denied push alone is not an implementation blocker and does not downgrade otherwise completed local work. Report it in the separate Push field.

For a real failure, classify it as a feature blocker, an unrelated pre-existing issue or an optional follow-up. Use the stated correction budget, complete unaffected work within the current Goal where its checkpoint dependencies allow, then record the exact unresolved criterion, command/scenario, evidence, attempts, current safe state and smallest essential external action.

**This queue's default is a strict sequential cumulative chain.** At a mandatory blocker, preserve the active partial branch and last verified cumulative anchor, stop inter-goal advancement, and leave later Goals Not started. Do not inherit incomplete code, silently skip a Goal, or create an independent bypass branch. An explicitly revised future queue may authorize an isolated later Goal; that is not the policy selected here.

Do not request routine permission for the next checkpoint, next Goal, branch creation, normal permitted push or ordinary engineering choices. Do not ask the user to run Cargo, launch the app, capture screenshots, exercise controls or review intermediate work. Essential unavailable capabilities and genuinely unresolved consequential requirements are the only reasons for intervention.

Do not weaken tests, hide stderr, change expected values to fit a defect, reduce dataset coverage, count an empty run as a pass or fabricate results to keep the queue moving.

## 14. One consolidated final report

When all 17 Goals have completed, or the queue reaches a genuine stopping condition, produce one factual report. Each individual Goal's Section K requirements are retained in the ledger and summarized here; no separate intermediate user approval is required.

Include:

| Goal | Branch | Source base SHA | Baseline SHA | Final SHA | Status | Tests / native / Clippy | Push |
|---|---|---|---|---|---|---|---|
| NN — title | actual branch | actual source | actual M0-A | actual final or unfinished | actual status | actual evidence and limitations | actual destination/result |

For every Goal report completed/blocked checkpoints and commit hashes, important implementation decisions, relevant changed owners/files, significant reviewer findings and fixes, actual verification commands/results, native evidence paths and remaining limitations. Distinguish implemented code from verified behavior. Do not invent a FINAL_NN for a blocked Goal; report its current partial HEAD separately.

Also identify:

- Queue ID, START, durable ledger path and active execution-worktree path.
- The final cumulative branch/commit, or the last verified cumulative anchor when blocked.
- Which completed commits/branches were pushed and which remain local.
- All remaining Not started Goals and the actual reason they did not begin.
- The bounded list of real manual-review considerations and relevant screenshots/exported artifacts for the user's one review.

Do not automatically merge any branch into master/main, publish a release, delete completed branches/worktrees, create a follow-up feature plan or start a new optimization initiative. Preserve the result and stop after the report.

## 15. Start execution

Begin with the read-only startup and resume check. On a fresh queue, read all 17 pairs, establish Goal 01's unique branch and M0-A baseline, attempt its permitted initial push, then delegate Goal 01/M1-A to the configured implementer.

Continue through the ordered checkpoints and handoffs without routine confirmation. Execute only this approved queue and its required verification.
