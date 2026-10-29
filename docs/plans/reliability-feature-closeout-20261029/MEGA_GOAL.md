# Multi Launcher — Reliability, Safety and Incomplete Feature Closure (Mega Goal)

**Program ID:** `multi-launcher-reliability-closeout-20261029`; allocate exact `<RUN_ID>` on first invocation.  
**Status:** `NOT_STARTED` — this is an authored execution contract, not evidence that any changes/tests/branches have begun.  
**Current source reference:** [`master` @ `56223fdc123396f4c41c48e0542506d912c6c634`](https://github.com/multiplex55/Multi_Launcher/commit/56223fdc123396f4c41c48e0542506d912c6c634) after both [#1456](https://github.com/multiplex55/Multi_Launcher/pull/1456) and [#1457](https://github.com/multiplex55/Multi_Launcher/pull/1457) merged. Reverify `origin/master` at execution time.  
**Orchestrator:** [ORCHESTRATOR_KICKSTART.md](ORCHESTRATOR_KICKSTART.md).  
**Goal contracts:** eleven separate [Goal/Kickstart pairs](#2-fixed-goal-manifest) executed **strictly sequentially**, not concurrently.  
**Output root:** `docs/plans/execution/multi-launcher-reliability-closeout-20261029-<RUN_ID>/`; human-readable Markdown for progress, source decisions, reviews and stalls.  
**Scope authority:** current user request to plan a prioritized follow-on program. Starting the orchestrator authorizes these defined bounded workstreams; it does not automatically authorize merging PRs to protected master or installing tools/editing user files outside the source repo.

## 1. Mission and acceptance philosophy

Turn the completed Astra audit and prior 17-goal partial integration into **finished, verified, user-value work wherever the current source and supported testing permit**. Begin with the source-supported Folder Diff error-information loss, then address accepted foundations that were left incomplete in Table Utility, Screenshot Redaction, Hot Folder, Jump Mode, ZIP extraction, MkMacro run summaries and Activity Center. Close by reconciling earlier deferred work and building one honest, reviewable integration PR from independently accepted checkpoints.

The major distinction is **three outcomes, not one forced green checkbox**:

- `ACCEPTED_COMPLETE`: the requested bounded behavior exists and its mandatory algorithm/native/safety proof is actually available and passed.
- `PARTIAL_ACCEPTED` / `NO_CHANGE_REVALIDATED`: a safe, independently useful subset or accepted current behavior, with explicitly missing feature-wide guarantees. Only independently reviewed, safe default behavior may be integrated.
- `DEFERRED_DEPENDENCY` / `DEFERRED_WITH_DESIGN` / `NATIVE_PROOF_DEFERRED` / `FAILED_VERIFICATION`: preserve exact owner evidence and a new Goal/Kickstart or native proof protocol; **do not** import unsafe source, relabel a blocked scope as complete or stop other independent goals.

This program is meant to be **implementation-oriented, not another unlimited static architecture research campaign**. Plans/handoffs are detailed enough for one writer to implement, but source exploration stops when ownership and tests are known. No need to fill a quota of features, bugs, tests or commits. A justified no-change outcome is valid; a completely missing required safety gate cannot be waved away.

## 2. Fixed Goal manifest

| Goal | Product workstream | Priority | Dependency / gate | Initial status |
|---|---|---|---|---|
| [G00](goals/G00-baseline-and-continuation/GOAL.md) | Source Baseline, Stale Continuation Cleanup and Execution Ledger | P0 / mandatory | None; must complete before production work | `NOT_STARTED` |
| [G01](goals/G01-diff-failure-information/GOAL.md) | Folder Diff: Preserve Destination Read Errors and Operation Feedback | P0 / first production fix | None; primary source-supported bugfix | `NOT_STARTED` |
| [G02](goals/G02-table-publication-safety/GOAL.md) | G09 Table Utility: Full-File Publication Safety Foundation | P1 / high safety | None; G03 depends on this goal reaching ACCEPTED_COMPLETE | `NOT_STARTED` |
| [G03](goals/G03-table-full-file-operations/GOAL.md) | G09 Table Utility: Conditional Full-File Operations and UX Closure | P2 / hard-gated | G02 accepted safety foundation, otherwise DEFERRED_DEPENDENCY without source changes | `NOT_STARTED` |
| [G04](goals/G04-screenshot-native-redaction/GOAL.md) | G12 Screenshot Redaction: Native Crop, Output and Interaction Acceptance | P1 / output safety | No Table dependency; native proof required for end-to-end completion | `NOT_STARTED` |
| [G05](goals/G05-hotfolder-condition-evaluator/GOAL.md) | G16 Hot Folder: Safe Size, Age and Literal-Content Conditions | P1 / unattended automation | None, but fail-closed condition guarantee is mandatory | `NOT_STARTED` |
| [G06](goals/G06-jump-settings-and-focus/GOAL.md) | G11 Jump Mode: Finish Customization UI and Focus Regression Coverage | P2 / daily usability | None | `NOT_STARTED` |
| [G07](goals/G07-zip-extraction/GOAL.md) | G14 ZIP: Bounded Extraction Writer and Safe UI Integration | P2 / security-sensitive utility | A safe writer and exact output contract must precede extraction UI enablement | `NOT_STARTED` |
| [G08](goals/G08-mkmacro-run-history/GOAL.md) | G15 MkMacro: Bounded Run History and Conditional Safe Recipes | P2 / productivity | Recipes require accepted history and explicit policy; do not auto-run after mere display success | `NOT_STARTED` |
| [G09](goals/G09-activity-center-retirement/GOAL.md) | G06 Activity Center: Source-Grounded Native Retirement Acceptance | P2 / reliability | Native proof when owner retirement is claimed; do not infer from terminal UI | `NOT_STARTED` |
| [G10](goals/G10-residual-defect-triage-integration/GOAL.md) | Residual Defects, Deferred Goals and Final Integration Closeout | P0 at end / coordination | Follows individual goal dispositions, never requires all goals to pass | `NOT_STARTED` |

All 11 goals have a separate, complete `GOAL.md` and `KICKSTART.md`. G00 supplies baseline and safe continuation; G10 is final cross-goal triage and integration. **G02 → G03** is the only hard new implementation dependency: full-file Table operations must not run without accepted publication safety. Within G04 screenshot and G07 ZIP, a demonstrated native/writer gate must pass before effectful UI acceptance. G08 safe run history can close while recipes are deferred. G09 can close only its actually proved worker owner subset; do not invent universal retirement proof.

### Strict order and rationale

1. **G00** prevents stale Codex entrypoint from resurrecting the former queue; establishes source, roles, lineage, status.
2. **G01** delivers the small, source-corroborated Folder Diff correction with direct user value and focused tests.
3. **G02** tackles G09's incomplete transaction/pub safety *before* any tempting full-file UI work.
4. **G03** is explicitly conditional on accepted G02; if unmet, immediately publish dependency receipt and continue.
5. **G04** resolves screenshot/native output where supported; no fake right-click drag proof.
6. **G05** finishes fail-closed Hot Folder conditions and non-executing Test Rule only when effect authorization stays guarded.
7. **G06** finishes Jump editor/preferences and query focus using established input ownership.
8. **G07** adds bounded ZIP extraction only through an accepted safe writer.
9. **G08** exposes bounded MkMacro history; recipes only through a separate explicit contract.
10. **G09** proves selected Activity worker retirement/native acceptance without replacing the existing framework.
11. **G10** reconciles accepted code, deferred-original goals, new actionable defects and prepares PR + final handoff.

The last several are independent; an unresolved G02/G04/G07 safety gate must not prevent executing G05/G06/G08/G09. Avoid arbitrary percentage estimates: count accepted checkpoints and gates separately.

## 3. Verified sources and intentionally excluded historical assumptions

- The original [partial integration PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) includes completed original Goals 01–03 and accepted partial G04/G06/G09/G11/G12/G14/G15/G16 checkpoints. Historical original G05/G07/G08/G13 are deferred; original G10/G17 were skipped for unmet dependencies; old G04 M2 editor work was user-deferred. This new plan **does not automatically restart those historical goals**.
- The [Astra audit recommendations](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/multi-launcher-research-20261028-r01/01-static-audit/RECOMMENDATIONS.md) and [search recommendations](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/multi-launcher-research-20261028-r01/02-search-discovery/RECOMMENDATIONS.md) distinguish evidence from speculation. The terminal-escape bug is already fixed in merged master. No discovery metadata/ranker/UI should ship from the closed H0 admission decision.
- The new [Diff failure-information source contract](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/diff-mutation-authority-r01/proposals/diff-failure-information/GOAL.md) confirms ordinary destination error loss and current-report omission; G01 implements that *without reopening native object identity H0*.
- `docs/plans/continue.md` still points to a historical branch/old SHA; G00 must confirm and safely retire that instruction. Earlier final merge-plan/PR receipt statements were true when written, not live progress. **Do not overwrite dated receipts to make them appear newer.**
- No new workspace features; no fresh generic command bus, plugin ABI, universal worker scheduler, new Settings coordinator, native file-operation framework or unvalidated AI search runtime. These were not supported by the prior source audit.

See [SOURCE_AND_RISK_BASELINE.md](SOURCE_AND_RISK_BASELINE.md) for owner evidence and prior-goal facts. Every agent still revalidates the **actual** task base.

## 4. Branch ancestry, accepted-checkpoint integration and PR topology

### 4.1 Initial bootstrap

Parent fetches `origin/master`, verifies new SHA, local user edits, all worktrees and active Git operations, then creates:

```text
verified origin/master (MASTER_START; no stale reset)
  └── codex/closeout-integration-<RUN_ID>
       ├── first program documents/status commit (PROGRAM_BOOTSTRAP_SHA)
       └── accepted reviewed checkpoints imported one-by-one
```

The user extracts this plan package under `docs/plans/reliability-feature-closeout-20261029/` *before* running the Kickstart. The orchestrator commits the owned plan files after branching, never directly to master. If they already exist in the repository on the verified master, do not duplicate them.

### 4.2 Dedicated goal branches

For **every** G00–G10, create a new branch `codex/closeout-gXX-<slug>-<RUN_ID>` from `PROGRAM_BOOTSTRAP_SHA` (an explicit program-specific branch from a freshly verified master plus the plan skeleton). Dependent G03 starts instead from the exact recorded **accepted integration SHA of G02**; G10 may start from the current accepted integration tip because its work is integration-only. Record this ancestry; never claim a goal starts directly from master when it uses a dependent tip.

Ordinary independent goal branches do not stack unrelated unreviewed implementation. Goal-specific documentation roots are distinct; only the parent updates the global program ledger on the integration branch. For a true dependency in another goal, use a previously **accepted integration SHA**; never use the prior goal's unaccepted working tip. Do not fetch/pull into a dirty user worktree or stash/reset/rewrite unrelated changes. Use an isolated clean worktree if needed, respecting existing worktree ownership.

### 4.3 Incremental integration, preserving historical source

After each goal's accepted checkpoint, the parent reviews staged paths and tests, commits/pushes on its branch, and imports **only approved checkpoint commits** to `codex/closeout-integration-<RUN_ID>` using provenance-preserving `git cherry-pick -x` or exact reviewed patches when a dependent branch already contains previously integrated ancestors. Do not import failed/unaccepted code. Parent creates an integration receipt linking source branch commit, integration commit, changed files, rationale, test evidence and defer/accept scope. Source conflicts require one bounded correction/review; material new change is deferred, not forcibly merged.

Documentation-only stop reports and complete new future pairs may be imported as historical evidence **without** implementing blocked source. Parent updates `PROGRAM_STATUS.md` once per checkpoint with last accepted SHA and exact next action, then commits/pushes. Work beyond a safe gate remains disabled/unreachable; do not hide partially implemented unsafe features behind vague UI status.

At G10 prepare **one consolidated reviewable PR** from the integration branch to the then-current `master` (or a small number of independent PRs only if a genuine source-owner or protection boundary requires splitting). Do not auto-merge into master; the user performs one final review. No force-push, branch deletion or silent base rewrite.

## 5. Roles, models and bounded orchestration

The actual merged `.codex` config (observed on planning-time master) specifies `gpt-6-astra/xhigh` for the parent, `gpt-6-astra/max` planner/reviewer, `gpt-6-astra/xhigh` implementer, with older model pins commented immediately above. **Do not modify these files.** Models are requested/configured, not proven to be actual serving backends unless invocation receipts expose their identity.

- **Parent/orchestrator — Astra xhigh:** owns program state, exact source ref, branch/worktree Git state, task sequencing, role dispatch, acceptance gates, provenance, integration and final PR preparation. Never dispatch overlapping source writers.
- **Planner — Astra max:** inspect just enough current code/callers/tests to produce one executable milestone packet with precise owners, behavior, invariants, interactions, non-goals and proof. No source edit or broad build.
- **Implementer — Astra xhigh**, optionally user/config-supported Sol xhigh for small bounded code work: execute one packet at a time, write source/tests and role analysis. A runner may use a `worker` role to request a supported model explicitly; preserve actual role pins and note if backend identity unverified.
- **Reviewer — Astra max:** independent separate context for substantive diff review, source/caller/compatibility, safety and false-proof concerns. One substantive correction pass (not endless review loops).

The configured max concurrency is not a directive: **at most one active child agent** plus parent, and only one source writer. If tool dispatch hits thread-limit, retire owned completed children if supported, retry once, then use documented available parent fallback, preserving attribution. Never restart the entire goal or ask the user to recreate an already committed run. If model unavailable, continue with a recorded supported fallback; do not change global config. Never claim a self-review was independent.

## 6. Human-readable progress, role folders, follow-on goals and stop handling

Every goal creates:

```text
docs/plans/execution/multi-launcher-reliability-closeout-20261029-<RUN_ID>/
├── PROGRAM_STATUS.md              # authoritative current orchestration
├── SOURCE_BASELINE.md             # actual master, role pins, drift
├── BRANCH_MAP.md                  # exact parent/child accepted SHAs
├── ACCEPTED_CHECKPOINTS.md        # source-to-integration map
├── FOLLOW_ON_CATALOG.md           # all new proposed Goals/Kickstarts
├── RISKS_AND_DECISIONS.md         # no-action, partial, deferred reasons
├── PR_SUMMARY.md                  # accurate PR draft
├── FINAL_HANDOFF.md               # generated only at close
├── goals/
│   ├── G00/
│   │   ├── PROGRESS.md
│   │   ├── SOURCE_REVALIDATION.md
│   │   ├── VERIFICATION.md
│   │   ├── RISKS.md
│   │   ├── roles/{planner,implementer,reviewer,orchestrator}/
│   │   │   ├── ROLE_RECORD.md
│   │   │   └── follow-on/<candidate-id>/{GOAL,KICKSTART}.md
│   │   └── follow-on/<canonical-id>/{GOAL,KICKSTART}.md
│   └── G01/ ... G10/
└── stalls/<goal>-<checkpoint>-<UTC>.md
```

The main `goals/GXX-.../GOAL.md` file inside this supplied plan also has an explicitly marked **mutable progress** section. Treat it as that goal's acceptance ledger and append events as real checkpoints happen; leave its immutable charter/invariants unchanged without dated amendment. `PROGRAM_STATUS.md` is authoritative for orchestration/current active branch/last accepted integration SHA; child progress can point to that status and to its own role files. Never store raw secrets, private file contents or unsanitized captures in debug Markdown.

**Each role** must author a complete follow-on Goal/Kickstart pair in its role folder for a genuinely source-supported actionable finding; the parent deduplicates and promotes canonical pairs. If no actionable new finding exists, record **NO_ACTION** and why. Every future pair states exact current owner, needed changes, tests, native gates, models, risks, dependencies and acceptance. Generated future goals are **not automatically executed**; prevent recursive plan explosion or stale revision assumptions.

### Recovery without stalls

Unavailability of optional native drivers, reviewer contexts, Python, network publication or other noncritical tools is a **bounded limitation**, not an excuse to halt unrelated work. After one bounded recovery, record exact failure and complete the documentation/hand-off state; schedule next *independent* goal. Genuine repository/permission/safety conflict must stop mutation and preserve changes, not force a branch operation. Resume from saved `PROGRAM_STATUS.md`/known SHA and first unaccepted checkpoint, never from G00 unless baseline truly absent.

A `DEFERRED_*` finding may prevent a dependent goal, but it must not hold a worker or compiler forever. Use [PROGRESS_AND_STALL_TEMPLATES](PROGRESS_AND_STALL_TEMPLATES.md) with exact next action, attempted commands/time budgets, owned process cleanup and protected user work. Do not make automatic background monitoring claims; the run lasts only as long as the actual Codex session/execution permits.

## 7. Testing and native acceptance budget

[VERIFICATION_POLICY.md](VERIFICATION_POLICY.md) is the binding cross-goal budget. **No blanket full suite, no repeat of entire build-optimization queue, no repeated Clippy per minor docs checkpoint.** Selected compilations, nonzero focused tests, narrow scoped lint and one outcome-specific independent review. External native UI proof where necessary (screenshot crop/save, Windows output publication/ZIP, Activity retirement) must test the **actual** contract through supported tools, not substitute mocked unit/markup evidence. If unsupported, explicitly `NATIVE_PROOF_DEFERRED`; leave unsafe feature disabled/unmerged.

The parent may optionally run a minimal final application smoke over only newly accepted UI flows if the tool can build, launch, capture screenshot and close safely without needing user manual checks mid-run. Do not force lengthy rebuilds. The user gets one final manual review of any changes that actually reached the integration PR.

## 8. Detailed cross-goal acceptance and non-goals

| Concern | What is acceptable | Must remain excluded |
|---|---|---|
| Diff mutation | Preserve read-error distinction and current-view delivered failure/cancel feedback | Native handle-binding redesign or new trash claims |
| G09 table | Recovery/identity-safe publisher; full-file operation only after accepted gate | Unsafe unbounded in-memory export and skipped native proof presented as success |
| G12 screenshot | Tested actual primary redaction and secondary crop + save/copy when supported | Unit raster success mislabeled as native output proof |
| G16 Hot Folder | Evaluator under same candidate rule/action lease, fail closed on unknown, side-effect-free Test Rule | DoesNotContain success on partial content; default enabled but unsupported conditions |
| G11 Jump | Safe Settings UX, query focus and hint ownership | Global hotkey reimplementation or note/modal focus regressions |
| G14 archive | Traversal/overwrite/quota/CRC-bound extraction and only accepted UI subset | Blind `unzip` extraction, ZIP bomb, symlink/reparse escape or unproven atomic writes |
| G15 macro | Bounded metadata-only history UI; recipes only if separately justified | Secret/raw event logging or automatic replay |
| G06 Activity | Exact owner retirement receipt/cancel behavior | Treating terminal as join or adding polling every idle frame |
| Historical G04/05/07/08/13, G10/G17 | Honest reevaluation with dependency/no-action report and optional new pair | Re-execution of explicitly deferred work or skipped dependency goals automatically |
| Astra SEARCH-F001/P2 UI | Preserve historical decision, no production discovery code | Implementing unused infrastructure because synthetic ranking improved |

## 9. Final definition of done and publish policy

The orchestration program ends with 11 source-truthful goal dispositions, separately reviewed accepted checkpoints (if any), all blocked/partial items with precise **future complete pairs**, up-to-date `continue.md` or protected-edit status, current integration manifest, a dependency-safe next-wave catalog, and either a review-ready integration PR or an exact documented `BLOCKED_EXTERNAL`/`NO_ACCEPTED_CODE` result. A successful program is **not** defined by forcing all original features to be complete.

At close, ensure exact user-facing reports show accepted vs deferred and test/native evidence, not inferred success. Do not open multiple unnecessary PRs, merge into master without a separate explicit user decision, alter AGENTS/config, or delete preserved branch tips and evidence.

## 10. Execution progress — mutable

**State:** `NOT_STARTED`.  
**Actual source/master after runtime verification:** unset; planning-time ref `56223fdc123396f4c41c48e0542506d912c6c634`.  
**Program branch/worktree:** not created.  
**Active goal/role:** `G00 / none`.  
**Last accepted checkpoint and normal push:** none.  
**Next exact action:** read [ORCHESTRATOR_KICKSTART](ORCHESTRATOR_KICKSTART.md), verify repo/source, bootstrap this package on new integration branch and dispatch G00 only.  
**Next dependent gate:** G03 must wait for accepted G02; G04/G07 native/writer gates are independent.  
**Known blocker:** none reported at package creation.

| Goal | State | Actual goal branch/head/push | Accepted source or explicit defer | Next safe step |
|---|---|---|---|---|
| G00 | PENDING | — | — | Read G00 Kickstart at dispatch |
| G01 | PENDING | — | — | Read G01 Kickstart at dispatch |
| G02 | PENDING | — | — | Read G02 Kickstart at dispatch |
| G03 | PENDING | — | — | Read G03 Kickstart at dispatch |
| G04 | PENDING | — | — | Read G04 Kickstart at dispatch |
| G05 | PENDING | — | — | Read G05 Kickstart at dispatch |
| G06 | PENDING | — | — | Read G06 Kickstart at dispatch |
| G07 | PENDING | — | — | Read G07 Kickstart at dispatch |
| G08 | PENDING | — | — | Read G08 Kickstart at dispatch |
| G09 | PENDING | — | — | Read G09 Kickstart at dispatch |
| G10 | PENDING | — | — | Read G10 Kickstart at dispatch |

### Event log (append only)

- 2026-10-29: Planning artifact created from verified merged-master and prior research evidence. **No source modifications, branch operations, tests or PR creation were performed by writing these plans.**
