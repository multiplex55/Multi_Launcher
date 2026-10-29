# Orchestrator Goal — Multi Launcher Sequential Research Program

**Prepared:** 2026-10-28  
**Repository:** `multiplex55/Multi_Launcher`  
**Execution:** Research and documentation, with a small isolated search prototype; no production implementation  
**Order:** Goal 01 Static Architecture Audit → Goal 02 Intelligent Search and Action Discovery  
**Entry point:** [ORCHESTRATOR_KICKSTART.md](ORCHESTRATOR_KICKSTART.md)

## 1. Outcome and scope

Execute two substantial, source-grounded research goals back to back. Produce detailed findings, architectural decisions, alternative designs, implementation approaches, and complete future Goal/Kickstart packages. The deliverable is a research program that can be inspected in GitHub throughout execution, not another production implementation campaign.

The architecture audit comes first because it should establish the actual owners, invariants, and cross-feature risks that constrain the search design. Goal 02 consumes that evidence without repeating the entire audit. Both goals remain useful independently: an unresolved audit question does not prevent an explicitly conditional search design.

Optimize for quality of engineering judgment and implementer clarity, not maximum document length or maximum number of findings. Spend effort on evidence, counterexamples, competing designs, failure paths, compatibility, and explicit future changes. Do not spend it on repeated agent dispatch, building the application, or elaborate research infrastructure.

The selected projects are **the static architecture audit and the search/action-discovery project**, not all five previously discussed ideas. Universal Actions, undo, and keyboard UX are examined only where relevant to these two scopes. Do not introduce workspace features, a new global workflow engine, or an unrelated feature queue.

### Program success

A successful run delivers:

1. A version-bound map and audit of the current application, including coverage limits and evidence-backed findings.
2. A deterministic, local-first search/action-discovery design grounded in existing owners and behavior.
3. A small standalone prototype and synthetic relevance evaluation when available, with truthful limitations when execution is unavailable.
4. A canonical, deduplicated library of fully specified future Goal/Kickstart pairs for every actionable recommendation.
5. Planner, implementer, reviewer, and orchestrator contributions attributed to the actual model/effort when observable.
6. Frequent checkpoint commits and normal pushes, with visible progress and explicit resume instructions.
7. A final prioritized implementation roadmap. Producing it does **not** authorize executing it.

## 2. Fixed manifest and instruction boundaries

Read these files in full before executing their corresponding stages:

| Purpose | File |
|---|---|
| Program contract | `ORCHESTRATOR_GOAL.md` |
| Program launch | `ORCHESTRATOR_KICKSTART.md` |
| Model and role policy | `MODEL_ASSIGNMENTS.md` |
| Goal 01 contract | `01_GOAL_STATIC_ARCHITECTURE_AUDIT.md` |
| Goal 01 launch | `01_KICKSTART_STATIC_ARCHITECTURE_AUDIT.md` |
| Goal 02 contract | `02_GOAL_SEARCH_ACTION_DISCOVERY.md` |
| Goal 02 launch | `02_KICKSTART_SEARCH_ACTION_DISCOVERY.md` |
| Future implementation goal template | `FOLLOW_ON_GOAL_TEMPLATE.md` |
| Future execution prompt template | `FOLLOW_ON_KICKSTART_TEMPLATE.md` |
| Findings, role provenance, and stop templates | `FINDINGS_AND_PROGRESS_TEMPLATES.md` |

Paths in this table are relative to `docs/plans/ai-research/`. If the supplied package is elsewhere, locate it read-only, establish `PLAN_ROOT`, and copy only these explicitly supplied plan inputs into the canonical directory after the first research branch exists. Do not overwrite differently edited versions; reconcile differences before proceeding. No source ZIP or old integration branch is a baseline.

The user's current request and this program explicitly establish these task-specific rules:

- Research-only verification replaces production build/native/Clippy gates for **these two research goals**. It does not waive future implementation acceptance.
- New Goal 01 starts from the actual new `master`; Goal 02 starts from Goal 01's recorded closing documentation commit.
- Missing agent capacity, missing browser access, unavailable optional prototype execution, and unresolved runtime measurements are **nonblocking research limitations**.
- Role fallback is authorized as described below; a fallback is never labeled an independent review or a different model.
- Current work creates detailed future work packages but does not execute those packages.

Respect platform permissions and current applicable `AGENTS.md`. Do not attempt to override a tool restriction, sandbox, or higher-priority agent instruction. Read-only agents return their Markdown; the parent or authorized documentation writer persists it.

## 3. Source baseline and branch ancestry

### 3.1 Establish the actual new master

Discover the repository root and intended remote rather than assuming a shell directory. Inspect current branch, HEAD, working tree, staged changes, worktrees, remotes, upstream, and any active merge/cherry-pick/rebase. Read applicable instructions and current agent configuration.

Perform one normal fetch when permitted. Resolve the actual `origin/master` commit and record it as `MASTER_START`. When the user has explicitly supplied a newer local master with intentional unpushed commits, record that discrepancy and the selected source rather than silently discarding it. Prefer the fetched intended master in the ordinary case. A clean checked-out master may be fast-forwarded normally; an isolated worktree can instead be created directly at the resolved source without moving the user's checkout.

The user intends to run after their PR has merged, but this package does not hardcode PR #1455's merge state or an old SHA. Read the current source. If the expected features are absent, record the actual observed baseline and analyze that baseline. Do not poll for a PR merge or import the old integration branch automatically. A missing expected feature is a research fact, not permission to fabricate it.

Record `MASTER_START`, repository identity, remote, date, and first-party source scope in `SOURCE_BASELINE.md`. All evidence cites this pinned source or an explicitly identified later research-only descendant. If master changes while this program runs, keep the original source snapshot; describe the drift at closeout rather than silently mixing versions.

### 3.2 Create one fresh branch per research goal

Choose an available run ID such as `<UTC-date>-r01`, adding a unique suffix only for a genuinely new run. Define:

```text
PROGRAM_ID = multi-launcher-research-<RUN_ID>
OUTPUT_ROOT = docs/research/<PROGRAM_ID>
G01_BRANCH = codex/research-static-audit-<RUN_ID>
G02_BRANCH = codex/research-search-discovery-<RUN_ID>
```

Required ancestry:

```text
new master / MASTER_START
  └─ G01_BRANCH: baseline + architecture audit + follow-on packages + closure
       └─ G02_BRANCH: its own baseline + search design + evaluation + follow-ons + program handoff
```

Create Goal 01's branch from `MASTER_START`. Create Goal 02's branch from Goal 01's exact recorded closing commit, **not** from a moving branch name, an old integration branch, or an unrelated master update. The second branch therefore contains both goals' documents without a later sibling-branch integration exercise. The application source remains the same master snapshot because neither research goal edits it.

This cumulative documentation handoff is an explicit program-specific exception to any default instruction to start each task from a freshly advanced master. A normal fetch remains allowed, but it does not silently change Goal 02's base.

Do not create a third orchestration branch. Bootstrap and program-administration documents belong to Goal 01, then carry into Goal 02. Never write directly to master, force-push, rewrite checkpoints, stash user work, delete worktrees, or automatically merge research results.

### 3.3 Resume existing work before creating anything

If `PROGRAM_STATUS.md`, either goal's execution-progress section, or a matching research branch already exists, verify identity and ancestry first. Resume the recorded branch and first unfinished checkpoint. Do not create another run because a previous session stopped or compacted context. Do not rerun completed research merely to reproduce the same outputs.

On a dirty user checkout, locate the program's owning worktree or use a clean isolated worktree from the correct base. Preserve unrelated changes. An active unrelated writer or unresolved Git operation requires a safe administrative pause, not destructive repair.

## 4. Strictly sequential scheduling and role ownership

Never run the two goals simultaneously. Within one goal, default to **one active child agent at a time**, plus the parent. This deliberately avoids the thread-limit behavior encountered in the previous implementation queue.

Use the sequence:

```text
Parent defines a checkpoint packet
→ Planner investigates and returns an implementer-ready research packet
→ Parent persists planner contribution and closes/retires that child when supported
→ Implementer writes the assigned research deliverables/prototype only
→ Parent captures result and retires child
→ Reviewer examines stable artifacts and returns findings
→ Writer resolves substantive issues or records explicit limitations
→ Parent checks, commits, pushes, records next action
```

Do not dispatch another role merely to fill idle time. Do not start Goal 02 reading/writing agents before Goal 01 closes. Small parent checks needed for bootstrap are not a parallel research goal.

### 4.1 Roles in this program

**Orchestrator:** owns Git, branch choice, authoritative program status, model selection, role dispatch, acceptance dispositions, cross-goal deduplication, and progress publication. It may perform bounded fallback research when an agent is unavailable.

**Planner:** identifies source owners, expected behaviors, assumptions, missing evidence, alternatives, and research subtasks. Its output is substantive architecture planning, not production code. It proposes future work in complete packages where justified.

**Implementer:** means *research author/prototype implementer* here. It writes the approved Markdown, structured inventories, and small offline search prototype under the permitted documentation roots. It does not modify the Rust application or run application tests.

**Reviewer:** independently challenges claims, source links, proposed invariants, benchmark methodology, and follow-on implementability. It returns concrete objections and counterexamples, not cosmetic demands for more prose. It creates recommendation packages for genuinely new actionable review findings.

### 4.2 Model resolution and honest attribution

Use the recommendation in `MODEL_ASSIGNMENTS.md`: Astra for the highest-reasoning roles; a deliberate Sol option for structured implementation/prototype work. Record requested model/effort and actual resolved identity when the runner exposes it. Unknown resolved identity stays `unverified`; a config file alone is not proof that a particular invocation used it.

Do not rely on a role label or prompt text to override a model pinned in an agent file. Inspect precedence and installed support once. Use supported per-task dispatch or task-specific role configuration when already available. The supplied model snippets are recommendations, not authority to rewrite global configuration. If a setting cannot be changed under existing permissions, record the limitation and use the available model rather than halting useful research.

On one dispatch-capacity failure, preserve the error. Retire completed children owned by this program if the tool supports that; make at most one recovery dispatch. If unavailable again, continue the checkpoint in the parent or with the available authorized worker. Do not restart the whole session, spawn substitute chains, or claim independent review from a parent self-check. At final handoff, label the review depth honestly.

### 4.3 Each role gets a persistent folder

Within each goal output directory create:

```text
roles/
  planner/ROLE_RECORD.md
  planner/ANALYSIS.md
  planner/follow-on/<candidate-id>/GOAL.md
  planner/follow-on/<candidate-id>/KICKSTART.md
  implementer/ROLE_RECORD.md
  implementer/ANALYSIS.md
  implementer/follow-on/<candidate-id>/GOAL.md
  implementer/follow-on/<candidate-id>/KICKSTART.md
  reviewer/ROLE_RECORD.md
  reviewer/REVIEW.md
  reviewer/follow-on/<candidate-id>/GOAL.md
  reviewer/follow-on/<candidate-id>/KICKSTART.md
  orchestrator/ROLE_RECORD.md
  orchestrator/DECISIONS.md
```

Use task roles as stable directory names; record model/version/effort inside `ROLE_RECORD.md`. A model change must not scatter the same role's history across unrelated folders. Save each checkpoint contribution with a clear date/heading or separate checkpoint file.

A read-only role returns the exact proposed file content and paths; the parent materializes it, recording `authored_by` and `persisted_by`. Do not falsely say the child wrote files. If a role has no new actionable recommendation, record that conclusion; do not invent recommendations to populate its folder.

## 5. Files, progress, and observable state

### 5.1 Allowed writes

Current work may write only:

- This supplied program's plan/goal/kickstart files under `docs/plans/ai-research/`, including their designated execution-progress sections.
- `docs/research/<PROGRAM_ID>/` and its goal, role, follow-on, prototype, evidence, and stop subdirectories.

No modifications to `src/`, `crates/`, `tests/`, Cargo manifests/lockfiles, production assets, vendored code, global settings, existing unrelated plans, or `AGENTS.md` are authorized by these research goals. Model configuration examples remain Markdown unless the user separately applies them. Existing local Git metadata operations necessary for branching/committing are allowed.

Use Markdown for human-readable output. JSON/JSONL/CSV and a few Python files may supplement Goal 02 under its documentation output root; every dataset/prototype has a Markdown explanation. Avoid large binaries and generated caches.

### 5.2 Progress belongs in the goal itself

**Yes: record progress in each main goal Markdown.** Keep its charter and acceptance requirements stable and update only the labeled `Execution progress — mutable` section during ordinary execution. Material scope changes require an explicit dated amendment, not a quiet rewrite of the success criteria.

Each goal progress section records:

- Actual goal branch/worktree, source baseline, program ID, current milestone and phase.
- Milestone status and links to real output files.
- Last committed checkpoint already known at edit time, push result, and exact next action.
- Current uncertainties, external limitations, selected fallback, and any stop report.
- A short append-only event log at substantive transitions.

`OUTPUT_ROOT/PROGRAM_STATUS.md` is the authoritative **orchestration** index: run identity, ordered goals, selected bases, active writer, role availability, branch/commit map, and next goal. The per-goal mutable sections are authoritative for their research milestones. Cross-link them; do not maintain several competing ledgers with independently invented statuses.

`OUTPUT_ROOT/README.md` is a human entry point, not another operational state machine. `SOURCE_BASELINE.md` records immutable source provenance. The original external implementation `QUEUE_STATE.md` is historical input only; do not alter or resume that old 17-goal queue.

A commit cannot record its own resulting SHA. Record the preceding known SHA in committed progress; publish the newly returned SHA in the parent response and the next natural update. Do not create recursive hash-only commits or require whole-tree manifests for every paragraph.

### 5.3 Commit cadence

Create one meaningful `M0-A` documentation baseline on each fresh research branch. Commit at each goal's natural checkpoint before moving to a materially different subsection. If a checkpoint becomes unusually large, split at a coherent artifact boundary, not arbitrary line counts.

Examples:

```text
docs(research): [G01/M0-A] establish static audit baseline
docs(audit): [G01/M1-A] map current architecture and source coverage
docs(audit): [G01/M2-A] record lifecycle and ownership findings
docs(audit): [G01/M3-A] produce reviewed follow-on goal packages
docs(search): [G02/M1-B] specify deterministic discovery and compatibility
feat(research): [G02/M2-A] add isolated relevance prototype
docs(research): [G02/M3-B] close sequential research and prioritize follow-ons
```

Before committing, inspect staged paths and the diff; stage only program-owned work. A truthful partial research checkpoint or stop report is valid, but never title it as completed acceptance when the content is incomplete.

Attempt one ordinary push after each checkpoint. Failed push does not stop local research; preserve the commit, record failure and remote visibility as unknown/unavailable, and continue. Never claim GitHub contains unpublished files. Do not repeatedly ask for push approval or change authentication, protection, or permissions to force delivery.

## 6. Research depth without a build campaign

### 6.1 Allowed activity

- Static file reading, `git` read-only inspection, `rg`, and simple source/text processing.
- Parsing TOML/JSON/CSV with already installed tools without importing repository runtime code.
- Primary-source web research for relevant API contracts and competitor behavior, with dates and attribution.
- Diagrams as Markdown/Mermaid or explicit textual state/sequence diagrams.
- Small, deterministic, local-only prototype/evaluation runs confined to Goal 02's synthetic data and documentation directory.
- Cheap Markdown-link, JSON-shape, duplicate-ID, and path checks.

### 6.2 Prohibited current activity

Do not run Cargo, rustc, nextest, Clippy, application unit/integration suites, native GUI scenarios, benchmark harnesses, rust-analyzer indexing, SSH build jobs, clean rebuilds, dependency installations, or performance trace campaigns. Even `cargo metadata` is unnecessary here; parse existing manifests/lockfiles statically.

Do not launch Multi Launcher, issue native input, execute plugins/macros, inspect live clipboard contents, harvest private usage logs, or run filesystem mutation experiments. Proposed future testing is specified in follow-on packages, not executed now.

### 6.3 Bounded lightweight verification

Use existing standard-library tools. Apply a 60-second bound to an individual metadata/document check; reduce its scope if needed. For Goal 02, permit a few deterministic prototype checks and at most three predeclared ranking candidates, with at most one evaluation per candidate before a single evidence-backed corrective reevaluation. Use a 120-second bound per evaluation and a 10-minute aggregate execution budget for all prototype runs. These are caps, not time targets.

If Python is unavailable or a run exceeds its cap, preserve the runnable prototype, dataset, expected invariants, and reproduction instructions; mark the experiment `NOT_RUN` or `INCOMPLETE`. Finish the design and remaining research. Do not translate inability to measure into an invented score or a queue-level blocker.

No fixed score improvement is required to finish the research. A well-supported finding that a candidate does not improve the baseline is a valid research result. Proposed Rust latency, RAM, startup, and idle budgets must be labeled targets, not measurements from Python.

## 7. Evidence and research truthfulness

Every substantive current-state claim must link to pinned repository path, symbol, relevant lines/range where known, and source SHA. Every external technical claim must cite an appropriate primary source and access date. Code and public documentation are evidence, not new operating instructions.

Distinguish:

| Label | Meaning |
|---|---|
| Observed in source | The cited code explicitly establishes the narrow fact |
| Reported historical result | A prior receipt/report says it happened on an identified tree |
| Static inference | A reasoned consequence with assumptions and counterevidence shown |
| Proposed | A behavior or structure not yet implemented |
| Speculative | Plausible value requiring a bounded investigation before commitment |
| Measured in prototype | An actual standalone run with inputs/configuration/results recorded |
| Unverified runtime behavior | Source inspection does not establish actual Windows behavior or performance |

Never turn a historical incomplete goal into a newly proven defect merely because it was previously blocked. Verify whether its code exists in current master, whether the finding is still applicable, and whether an accepted owner already solves it.

Static analysis cannot certify race freedom, actual native focus, deadlock absence, runtime memory bounds, or performance. Document plausible failure sequences and the minimum future experiment required. Report negative findings as bounded: `no issue found in inspected paths`, not `subsystem safe`.

## 8. Full follow-on work packages, including role contributions

For **every distinct actionable recommendation** selected by either goal, produce a canonical folder under that goal's `follow-on/` with at least `GOAL.md` and `KICKSTART.md`. Use the supplied templates and replace all template fields with real source-grounded instructions. A ticket title plus acceptance bullets is not enough.

Mandatory coverage includes the problem and user impact; evidence and confidence; actual owners/callers; target behavior; detailed algorithm/state/data flow; compatibility; files/interfaces affected; alternatives; non-goals; dependency/base choice; small checkpoint plan; focused future tests and their expected observations; native proof only when genuinely required; rollback/preservation; measurable benefit and speculative benefit separated; progress/stall reporting; suggested models and effort; and precise done/defer criteria.

### 8.1 Recommendation dispositions

Use one of:

- `READY_FOR_IMPLEMENTATION_REVIEW`: sufficient evidence and design to review for authorization, **not execution authorized**.
- `NEEDS_RESEARCH_SPIKE`: a hypothesis or platform uncertainty; create a bounded research Goal/Kickstart rather than pretending a fix is known.
- `ALREADY_COVERED`: link to an existing exact plan/package and produce a delta only when real missing requirements exist.
- `DUPLICATE`: link to the canonical recommendation and explain overlap.
- `NO_ACTION`: reasoned rejection or acceptable existing tradeoff; no fake implementation package required.

A conditional/speculative recommendation still gets a usable investigative package. The research program must never execute the future package merely because it created a kickstart.

### 8.2 Role proposals and canonicalization

A role with a new actionable recommendation supplies a complete pair in `roles/<role>/follow-on/<candidate-id>/`. The authoring model is recorded, not inferred. The implementer and reviewer may challenge planner packages, add more precise impacts, or propose distinct work.

The parent promotes or consolidates them into canonical `follow-on/<ID>-<slug>/` packages. Preserve role attribution and raw objections; do not duplicate three implementation goals for the same bug just because three roles mentioned it. For a duplicate role contribution, a `DISPOSITION.md` pointing to the canonical pair is enough after consolidation.

Maintain per-goal `FOLLOW_ON_INDEX.md` and final program `FOLLOW_ON_CATALOG.md`. Every finding/recommendation maps to a package or explicit disposition. Every canonical package maps back to evidence and requesting role. Use stable IDs, priorities, dependencies, owner scope, confidence, benefit, estimated implementation complexity, and validation burden. Avoid fake hour estimates.

Goal 02 may reference Goal 01's canonical work. It must not rewrite the frozen Goal 01 branch. Put conflicts, amendments, and dependency reconciliation on Goal 02's branch under `cross-goal/`. Shared ownership changes should be one package, not incompatible audit and search versions.

### 8.3 No recursive plan explosion

A follow-on package may name follow-on dependencies, but do not recursively generate another unbounded research program inside each one. Split only when ownership or independent delivery warrants it. Do not manufacture recommendations to satisfy a count. All accepted recommendations must be fully packaged; unsupported ideas must be clearly classified, not quietly dropped or promoted as facts.

## 9. Non-stalling execution and genuine stop handling

The program is designed to continue through ordinary research limitations. It cannot guarantee that unavailable tools, permissions, or session termination never interrupt execution.

| Situation | Required continuation |
|---|---|
| Preferred model unavailable | Record actual substitute and proceed with available authorized model |
| Child thread limit | Retire completed owned children once; one retry; parent fallback |
| Reviewer unavailable | Document parent self-review; preserve request for future independent review |
| Website unavailable | Use current source/other primary evidence; mark external comparison not verified |
| Optional experiment cannot run | Deliver code/data and unrun-method section; continue remaining artifacts |
| Runtime behavior cannot be proven statically | Label uncertainty; generate precise future validation package |
| Push denied/unavailable | Keep local commit, record unpublished state, continue research |
| Audit finding unresolved | Publish conditional analysis/spike; do not block unrelated audit/search sections |
| Review disagreement | One synthesis pass; keep alternate designs/assumptions and decision rationale |
| Source moved since discovery | Re-anchor affected citations once, or keep pinned baseline and note drift |
| Real data/Git/authorization risk | Stop mutation, preserve state, write a factual report when safe |

Do not declare a finding resolved to keep moving. Instead, close the *research item* with a useful disposition and complete other items. An inconclusive algorithm experiment or unresolved Windows guarantee is not failed research when its uncertainty is accurately captured.

No repeating read-review-retry loops. Allow one substantive correction pass per completed research checkpoint. Further material disagreement becomes a tracked unresolved item or narrower follow-on spike. This is a research closeout policy, not a license to publish false claims. Clearly unsupported claims are removed, narrowed, or labeled unknown.

Before a session/context boundary, update the active goal progress, program status, exact next action, and artifacts; commit coherent work and push when safe. State `PAUSED_RESUMABLE` when the session cannot continue. Never claim ongoing background execution without an active session or scheduled mechanism. Resuming starts at that exact point, not at M0-A.

### Stop-report location

```text
OUTPUT_ROOT/stalls/<goal-or-program>-<checkpoint>-<UTC-timestamp>.md
```

Include failure, scope, branch/worktree/current known SHA, affected artifacts, attempted recovery, remaining authority, skipped commands, ownership/cleanup observations, safe next action, and push visibility. An unavailable optional tool normally produces a limitation entry, not a full stop. One report per actual event, no unchanged-status report loop.

## 10. Handoff from Goal 01 to Goal 02

Goal 01 may close as `RESEARCH_COMPLETE` or `RESEARCH_COMPLETE_WITH_LIMITATIONS` only if its required documents, coverage dispositions, evidence, recommendation mapping, and usable follow-on packages exist. A missing optional experiment/reviewer does not bar closure. Missing substantive core content is `PARTIAL`, not completion.

If a session cannot finish all audit content, preserve the partial artifacts. Continue Goal 02 only after the audit has a useful minimum handoff: source map, coverage limits, the relevant search/input/async/persistence constraints, and explicit dispositions for unfinished sections. Label the program partial and do not claim a comprehensive audit. This fallback prevents an unrelated audit branch from permanently blocking search work.

The parent verifies docs-only diff, resolves staged work, confirms no child writer remains, records Goal 01's closing commit, and creates Goal 02's new branch from it. Do not reopen Goal 01 to chase optional improvements. Goal 02 reads the handoff and performs only targeted source revalidation needed for its own design.

## 11. Final program deliverables and review

On Goal 02's final branch, deliver:

- `PROGRAM_STATUS.md`: exact program outcome, two goal dispositions, branch/commit/push map, and next action.
- `README.md`: index of deliverables and how to read them.
- `SOURCE_BASELINE.md`: pinned new-master snapshot and any later drift.
- `FOLLOW_ON_CATALOG.md`: canonical cross-goal work queue and evidence links.
- `NEXT_IMPLEMENTATION_WAVE.md`: recommended dependency-safe order, what to do first, what not to start, risks, and suggested model assignment per package.
- `DECISIONS_AND_LIMITATIONS.md`: unresolved assumptions, rejected options, prototype-only measurements, model fallbacks, and coverage gaps.
- `HANDOFF.md`: what was researched, what was actually executed, what was not built/tested, artifact inventory, and ready-to-copy future launch instructions.
- `PR_SUMMARY.md`: documentation/research PR draft to master, including no-production-change statement and source baseline. Do not open or merge a PR unless separately authorized.

Verify local relative links and IDs once, parse structured files once, inspect diffs, and confirm no application files changed. Do not run Clippy to finish a documentation program. Publish one substantive final summary distinguishing research completed from future implementation proposed.

## 12. Execution progress — mutable

**State:** RESEARCH_COMPLETE_WITH_LIMITATIONS — both goals completed sequentially, reviewed and checked.
**Program:** multi-launcher-research-20261028-r01
**MASTER_START:** 59156d96adcc73939c7171487a0cb582e3286587
**Branch/worktree:** codex/research-search-discovery-20261028-r01; G:/Repos/rust/Multi_Launcher
**Active writer:** parent closeout; all child roles complete.
**Last known checkpoint/push:** 3a4924e20a95de38af3ba60aca6b86ade1e1c0f6; M3-A normally pushed.
**Next action:** publish closing checkpoint/report exact SHA and normal-push outcome; then await future package selection. No further evaluation, production work, PR or master merge.

[Authoritative status and ordered goals](../../research/multi-launcher-research-20261028-r01/PROGRAM_STATUS.md)

### Event log

- 2026-10-28: Program initialized on fresh branch from verified fetched master. Goal 02 remains NOT_STARTED until Goal 01 closing commit.

- 2026-10-28: Planner contributions and full parser proposal preserved; source/coverage/data-flow maps authored and accepted. Research-only scope maintained.

- 2026-10-28: M1-A e1943e90 and M1-B 9362ec5c accepted and pushed; source unchanged, no application execution. M2-A dispatched.

- 2026-10-28: Audit M3-B separate review completed; parent applied two factual report/package clarifications, preserved raw roles and closed research with limitations. Goal 02 has not started and must inherit exact closing documentation commit.

- 2026-10-28: Goal02 M3-B complete after separate review and one canonical F002 workflow-scope clarification; affected final checks pass. Both research handoffs and prioritized four-package catalog complete, all raw/frozen evidence preserved, no production/future-goal execution. Closing commit contains this status; exact final SHA and actual push outcome follow in final handoff.
