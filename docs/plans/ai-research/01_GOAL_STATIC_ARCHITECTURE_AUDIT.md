# Goal 01 — Source-Grounded Static Architecture Audit and Follow-On Engineering Library

**Program:** Multi Launcher Sequential Research Program  
**Repository:** `multiplex55/Multi_Launcher`  
**Goal type:** Deep research, static analysis, architecture design, and future-work authoring  
**Companion:** [01_KICKSTART_STATIC_ARCHITECTURE_AUDIT.md](01_KICKSTART_STATIC_ARCHITECTURE_AUDIT.md)  
**Governing execution contract:** [ORCHESTRATOR_GOAL.md](ORCHESTRATOR_GOAL.md)  
**Position:** First; do not run Goal 02 concurrently.

## A. Objective

Understand the actual application architecture on the new master and identify the highest-value changes that would improve correctness, maintainability, consistency, responsiveness, or future development efficiency. Produce detailed, implementable recommendations while preserving the distinction between a source-supported defect, a design concern, and a speculative opportunity.

The goal is not to redesign Multi Launcher from scratch or to manufacture a long list of refactors. The output must explain what already works, what must remain invariant, where responsibilities are fragmented, and which specific changes justify their cost.

For every actionable recommendation, create a complete future Goal and companion Kickstart. The user should be able to select one later without asking another model to rediscover its architectural owner, rationale, migration order, acceptance criteria, or testing boundaries.

### Research questions

1. Where are the true owners of commands, mutable state, long-running work, input focus, persistence, and file mutation?
2. Where can distinct activation routes produce inconsistent behavior for the same user action?
3. Which concurrency, lifecycle, source-identity, or UI-authority assumptions are explicit and enforced, and which are merely implicit?
4. Which architectural seams increase recompilation exposure, reasoning difficulty, or test coupling, without claiming speedups that have not been measured?
5. Which existing abstractions should be reused instead of replaced?
6. Which improvements can be delivered independently and safely after the current integration?
7. What constraints must Goal 02 honor when designing intelligent search and action discovery?

## B. Source of truth and task constraints

Use `MASTER_START` selected by the orchestrator. Verify every historical path against that tree. Current source and applicable instructions control factual claims; old PR reports and prior queue stopping records are provenance, not proof that the same bug exists now.

Historical navigation seeds include:

```text
src/plugin.rs
src/commands/
src/universal_actions/
src/common/query.rs
src/usage.rs
src/history.rs
src/completion.rs
src/gui/
src/settings/ and src/settings_editor/
src/persistence/ and src/common/atomic_file.rs
src/file_operations.rs
src/file_search/ and src/indexer/
src/mkmacro/ and src/hotfolder/
src/paste_sequence/
src/activity.rs
src/pinboard/ and src/ocr/
src/duplicate_finder/ and src/diff/
crates/multi_launcher_table_core/
dev/ and docs/plans/execution/
```

These are navigation hints observed during prior discussion, not a fixed coverage list or assertion that every file is unchanged. Follow moved owners and re-exports; do not create duplicate proposals to restore an older filename.

**Current authorized writes:** this goal's mutable progress section and its program-owned documentation output. **Current forbidden work:** production changes, Cargo/rustc/nextest/Clippy, app launch, native UI automation, performance benchmarking, dependency installation, touching live user data, or resuming the old 17-goal implementation queue.

The audit is deliberately broad in *analysis*, not in executable changes. Static scripts may read tracked sources and emit inventories/diagrams; they must not import application modules, run build scripts, or invoke external applications.

## C. Expected outputs

Resolve `GOAL_ROOT = docs/research/<PROGRAM_ID>/01-static-audit`.

| File or directory | Required substantive contents |
|---|---|
| `README.md` | User-facing summary, reading order, evidence limits, top decisions, links |
| `SOURCE_MAP.md` | Actual owners, entry points, packages, public facades, background services, source citations |
| `COVERAGE_MATRIX.md` | Every meaningful first-party subsystem classified by inspection depth and reason |
| `ARCHITECTURE_AND_DATA_FLOWS.md` | Component map plus actual command/state/work/persistence paths |
| `LIFECYCLE_AND_CONCURRENCY.md` | Ownership, cancellation, generations, retirement, shutdown, failure traces |
| `INPUT_AND_UI_OWNERSHIP.md` | Focus/modal/viewport/keyboard/Jump/radial/hotkey authority and transitions |
| `PERSISTENCE_AND_FILE_SAFETY.md` | Save/replace/identity/recovery/privacy contracts and uncertainties |
| `BOUNDARIES_AND_CHANGE_COST.md` | Dependency/compile/test boundaries and evidence-qualified improvement hypotheses |
| `FINDINGS.md` | Stable finding index with severity, confidence, source, and disposition |
| `findings/AUD-###-<slug>.md` | Full finding evidence and reasoning for each material finding |
| `DECISION_RECORDS.md` | Selected alternatives, rejected designs, rationale, and reversal conditions |
| `RECOMMENDATIONS.md` | Prioritized actions, expected value, risk, dependencies, and certainty |
| `FOLLOW_ON_INDEX.md` | Every actionable recommendation mapped to complete Goal/Kickstart pair |
| `follow-on/AUD-F###-<slug>/` | Canonical future `GOAL.md` and `KICKSTART.md`; supporting docs when useful |
| `SEARCH_HANDOFF.md` | Constraints, known owners, risky seams, and validated opportunities for Goal 02 |
| `REVIEW_AND_LIMITATIONS.md` | Independent or parent review clearly labeled, corrections and unresolved items |
| `roles/` | Attributed planner/implementer/reviewer/orchestrator work and role-proposed follow-ons |

Use additional Markdown when one file becomes hard to navigate; keep the index complete. Do not fill all files with the same generic warning paragraphs. Distinct documents should answer distinct questions. Do not create empty deliverables to imply coverage.

## D. Investigation method

### D1. Establish a structural inventory without building

Use tracked file listings and manifest parsing to identify first-party packages, modules, binaries, tests, production entry points, adapters, and vendored/generated exclusions. Record the method and its limits.

Map:

- Launch/bootstrap path and construction of persistent services.
- Query intake, plugin routing, matching/ranking, presentation, and activation.
- Typed command parsing/dispatch and legacy paths that still bypass it.
- Universal Actions target resolution, capability/context construction, provider ordering, and execution.
- GUI owner structures and large modules that contain multiple responsibilities.
- Async worker classes and which UI objects may issue/cancel requests.
- Configuration stores, transaction guards, backup/recovery paths, and local data boundaries.
- Platform-specific code and resource owners.
- Testing packages, fixtures, hooks, and dependency surfaces as source constructs.

A static module graph is not a fully resolved Rust call graph. Mark trait dispatch, macros, conditional compilation, re-exports, and platform-dependent paths that limit certainty. Explain edge semantics: import, call, state ownership, or message flow are not interchangeable.

For each subsystem, record `DEEP_TRACE`, `TARGETED_REVIEW`, `SURVEY_ONLY`, `OUT_OF_SCOPE`, or `UNAVAILABLE`. Justify survey-only areas and explain why their risk is lower or their evidence is absent. A comprehensive inventory with risk-weighted deep inspection is preferable to pretending every line was audited.

### D2. Trace important user workflows end to end

Inspect representative success, rejection, cancellation, and replacement paths. At minimum cover these workflow families when present:

1. **Query → result → activation:** keyboard Enter, mouse click, dashboard, radial, macro, hotkey, and unattended invocation. Look for semantic differences, inconsistent validation, query substitution, and confirmation bypasses.
2. **Query refresh → stale response:** async provider request, generation/request ticket, cache update, UI selection preservation, result invalidation, and focus changes.
3. **Jump/input ownership:** leader capture, typed hints, Enter execution, Escape, key release, text entry, hidden windows, foreign focus, native pickers, popups, and viewports.
4. **Worker lifecycle:** request admission, queued/running/cancelling/terminal states, retirement acknowledgment, resource release, restart, close/reopen, and shutdown.
5. **File operation lifecycle:** preview/confirmation, source and destination identities, staging, publication, interruption, rollback/undo, and cleanup.
6. **Settings/persistence:** loading, field recovery, migration, edits, validation, save transaction, live application, watcher reload, and competing changes.
7. **Macro/Hot Folder automation:** invocation source, eligibility, preview-only behavior, pause/resume/stop, nested calls, summaries, unattended safety, and bounded retained metadata.
8. **Clipboard and paste sequencing:** own vs external clipboard change, asynchronous publication, application switch, cancellation, repeated invocation, and remaining clipboard contents.
9. **Screenshot/pinboard/OCR:** capture ownership, native resources/threads, editor/output boundaries, cleanup, visibility, and memory retention.
10. **Read-heavy utilities:** File Search, Table/CSV, Duplicate Finder, Diff, Hash, Replace/XML or nearest actual counterparts. Compare common behavior without assuming they need one universal framework.

For each trace, write actual entry point → owners → state transitions → completion effects. Cite concrete symbols and branches. Include one plausible adverse sequence where useful. Do not claim a race exists merely because async code exists; identify the interleaving and the absent or insufficient guard, and search for counterguards before recommending a fix.

### D3. Classify architectural observations

For each material observation choose a category:

- Confirmed source inconsistency or directly supported defect.
- Plausible risk requiring runtime or platform validation.
- Maintainability/ownership concern, not an immediate correctness defect.
- Performance/change-cost hypothesis, not measured improvement.
- Product/UX opportunity, not a bug.
- Existing sound pattern worth preserving and reusing.
- Historical issue no longer applicable or already tracked.

Then state the mechanism, evidence, counterevidence, confidence, user impact, and minimal next action. Keep severity separate from confidence: a potentially severe issue with weak evidence is not a confirmed critical defect.

### D4. Evaluate alternatives before recommending structural change

For a proposed refactor, compare at least the relevant alternatives, not an arbitrary list:

- No change; improve documentation/invariants only.
- Small repair in the existing owner.
- Extract a narrow pure policy component while preserving facade/API.
- Consolidate duplicate decision logic behind the actual owner.
- Larger architecture change, only when smaller options demonstrably fail.

Explain migration cost, API churn, lifetime/ownership changes, compatibility and rollback. A new crate, central coordinator, event bus, state machine, or abstraction is not justified solely because it makes a diagram look cleaner. Name the existing owner that would otherwise remain responsible and why it is insufficient.

## E. Detailed audit domains

### E1. Command and action consistency

Inventory public command concepts and internal execution routes. Determine whether equivalent requests use one validation/dispatch path. Track typed vs stringly encoded payloads, source-specific overrides, capability checks, and action identity.

Questions to resolve:

- Can two surfaces execute the same action with different eligibility, arguments, or confirmation behavior?
- Is user-entered data distinguishable from command syntax and transport strings?
- Where is enabled/disabled state enforced, and is it rechecked at execution?
- Does target resolution depend on labels that can change or collide?
- Do stale selections retain authority after a query, setting, or foreground owner changes?
- Are contextual actions hidden, disabled with a reason, or routed inconsistently?
- Which common semantics already have a single owner and should not be duplicated?

Do not propose a replacement typed bus when the existing bus can be extended. Generated follow-ons should identify the exact bypass or inconsistent contract, not just say “unify commands.”

### E2. Input, focus, and UI state

Build a transition table for launcher hidden/visible, query typing, dashboard, active dialog, modal confirmation, native picker, external foreground, radial/gesture modes, and Jump Mode.

Review:

- Who may request or reclaim focus and at what frame/epoch.
- Key-down vs key-up fences, repeats, modifier ambiguity, and text events.
- Separate selection/focus from execution; preservation of ordinary typing.
- Dialog reopening, persistence of transient state, and stale controls.
- Hidden/clipped/disabled elements and Jump target eligibility.
- Context menu, secondary action, and Enter semantics.
- Whether a UI callback synchronously waits for worker state or filesystem work.

Source-defined focus behavior is not native proof. Recommendations must specify future manual/native checks only for changed observable behavior, using supported tools and a human-review fallback rather than inventing API capabilities.

### E3. Concurrency, cancellation, and shutdown

For each important worker class, produce a lifecycle table identifying owner, admission token, generation, cancellation signal, completion channel, retirement acknowledgment, and cleanup ownership.

Distinguish cancellation requested from worker stopped, result rejected from resources released, and UI closed from background work retired. Trace panic/error paths and owner destruction. Review whether lock acquisition order and callbacks can reenter, whether channel sends can block UI, and whether joins occur on latency-sensitive paths.

Investigate potential issues such as:

- Completion from an old request replacing a newer result.
- Cancellation tokens reused across operations.
- Workers outliving settings/environment/source leases.
- Thread-affine resources constructed or destroyed on the wrong thread.
- Holding locks while invoking callbacks or waiting for other owners.
- Double cleanup, orphan cleanup, or lost recovery ownership.
- Periodic work that continues when no consumer is visible or enabled.

Do not add a new process-management framework as an audit deliverable. Document the narrowest owner-level remedy and its eventual tests.

### E4. Persistence, mutation safety, and privacy

Trace actual save and destructive-operation paths. Separate a display path from file identity, preview approval from later operation authority, and scratch ownership from user-data ownership.

Study current handling of:

- Config schema evolution, legacy defaults, malformed records, and partial recovery.
- Atomic publication semantics, overwrite approvals, concurrent modification, and rollback.
- Windows path aliases, reparse points, hard links, source replacement, and retained handles where relevant.
- Temporary storage budgets, partial writes, cleanup failures, crash recovery, and durable records.
- Clipboard, query, file, macro, and diagnostic payload boundaries.
- Logs/history/summaries that might retain sensitive contents unexpectedly.

Use official platform documentation to interpret API guarantees when necessary. Do not infer guarantees from API names or an old capability probe. Do not execute dangerous filesystem tests. Create a bounded platform research spike when a safe design is not established.

Historical Goal 09 safety findings and Goal 12 native limitations are starting points only. Verify whether their candidates are actually merged; distinguish excluded patches from accepted code. Do not restart their exhausted implementation attempts through this research goal.

### E5. Performance, build exposure, and resource costs

Identify plausible cost centers statically: synchronous I/O during paint/search, repeated allocation, unbounded history, large retained buffers, polling without demand, repeated full scans, global initialization in small fixtures, and broad module/crate dependencies.

For each hypothesis record:

- Why the source suggests a cost.
- Frequency and expected data scale, if derivable.
- Conditions under which the concern would not matter.
- Existing caches/guards/bounds and their limitations.
- A minimal later measurement to confirm or reject it.
- A benefit statement that does not invent percentages, milliseconds, or RAM savings.

Parse manifest and module structure to discuss compilation exposure. A split file or crate is not automatically a build-speed improvement. Recommend a new boundary only when it has independent semantics and a plausible affected-build reduction. Future validation must separate build preparation, no-op, edited-target compile/link, and runtime cost, but none of that is run now.

### E6. Testability and maintainability

Inspect representative tests as documentation of contracts. Identify accidental integration dependencies, global state in nominally pure tests, hidden background services in fixtures, excessive sleeps, weak assertions, and coverage gaps around changed invariants.

Do not rank test quality by count alone. A small test proving a critical state transition may be more valuable than many repetitive cases. Where source restructuring would improve testability, name the exact seam and preserved public behavior. Recommend targeted tests, not default full-suite campaigns.

Review naming/organization only when they obscure ownership or contribute to defects. Avoid cosmetic “cleanup everything” recommendations.

## F. Finding and prioritization requirements

Each material finding uses `FINDINGS_AND_PROGRESS_TEMPLATES.md` and has an ID such as `AUD-001`. Include:

- Concise issue and category.
- Baseline SHA, paths, symbols, and cited source excerpts/ranges.
- Current behavior, affected workflow, and expected invariant.
- A concrete failure or maintenance-cost mechanism.
- Counterevidence and conditions that limit applicability.
- Severity, confidence, user impact, and operational likelihood as separate judgments.
- Candidate remedies and tradeoffs.
- Recommended action or explicit no-action disposition.
- Current verification: static only, historical result, or unavailable.
- Future validation required; no fabricated reproduction.
- Canonical follow-on package link or duplicate/existing-plan reference.

Prioritize with explained judgments rather than a single misleading numerical score. A useful order is data-loss/security correctness, widespread input/lifecycle regressions, frequently used workflow friction, then change-cost and maintenance opportunities. Adjust for confidence, implementation breadth, and dependency leverage.

Where a score is used for sorting, publish its components and show that a low-confidence severe possibility remains a research spike rather than automatically becoming a production fix. Do not recommend unrelated security work merely to increase severity.

## G. Follow-on work generation

Use `FOLLOW_ON_GOAL_TEMPLATE.md` and `FOLLOW_ON_KICKSTART_TEMPLATE.md`. Follow-ons should be sufficiently specific that a future implementer can begin with a bounded source revalidation, not a full rediscovery phase.

Examples of legitimate package shapes **only when supported by findings**:

- Repair one command route that bypasses shared eligibility validation.
- Consolidate duplicated transient focus invalidation while preserving key-release behavior.
- Make a particular worker's cancellation/retirement contract explicit.
- Replace an unsafe file-publication design with a source-backed identity-safe owner.
- Isolate a pure policy engine behind an existing facade with targeted build-scope validation later.
- Reduce a specific idle polling path by tying it to explicit demand.
- Repair a persistence migration conflict or sensitive diagnostic leak.
- Create a narrow investigation to decide whether a suspected lock-order or performance issue is real.

These are examples, not predeclared defects. Do not generate production fixes for them without evidence.

### Required future package details

1. **Why and what:** concrete user outcome, mechanism, evidence, non-goals.
2. **Where:** real owner types/functions, callers, persistence/UI/plugin boundaries, likely changed files, and forbidden neighboring scope.
3. **How:** stepwise data/control flow, interfaces, state transitions, migration order, error handling, cancellation/cleanup, and relevant pseudocode.
4. **Compatibility:** current defaults, existing behavior, serialized data, saved actions, keyboard paths, dynamic plugins, and Windows-specific assumptions.
5. **Verification later:** exact behavioral cases, nonzero selection, proportionate target scope, meaningful expected outcomes, optional native requirements, and explicit not-run-now status.
6. **Delivery:** one fresh branch, M0-A and small tagged commits, scoped end verification, pushes, progress section, Markdown stalls, rollback and handoff.
7. **Benefit:** proven defect removed vs anticipated performance/UX benefit; remaining speculative claims and measurement needed.
8. **Models:** planner/implementer/reviewer recommendation based on the actual difficulty; fallback and independence labeling.
9. **Readiness:** ready for authorization review vs needs spike; dependencies and base selection, without pinning an obsolete research SHA as a future production base.

Each role contributes its own justified recommendations in its role folder. The parent consolidates overlapping proposals, preserves disagreements, and publishes a canonical mapping. All actionable entries in `RECOMMENDATIONS.md` must have a full pair or an exact existing plan with a source-backed delta package. No unpaired “consider improving X” recommendations at final handoff.

## H. Checkpoint plan and commit boundaries

### M0-A — Branch, source baseline, and research state

**Objective:** establish this research run on its fresh branch from new master.  
**Outputs:** program status/source baseline, goal-root README skeleton with real scope, role records, and this goal's mutable progress initialized.  
**Actions:** inspect actual source/instructions; record branch/worktree/models; define subsystem coverage plan.  
**Verification:** identity and docs-only staged paths; no build.  
**Commit:** `docs(research): [G01/M0-A] establish static audit baseline`.

### M1-A — Source map and coverage inventory

**Objective:** identify owners and first-party boundaries.  
**Outputs:** `SOURCE_MAP.md`, `COVERAGE_MATRIX.md`, initial architecture map.  
**Actions:** inventory manifests/modules, actual entry points, state owners and relationships; label uncertain static edges.  
**Done:** every significant subsystem has a coverage disposition; factual paths/symbols are cited; deep-trace selection is justified.  
**Commit:** `docs(audit): [G01/M1-A] map owners and define coverage`.

### M1-B — Command, search, and input contracts

**Objective:** analyze high-frequency user interaction and cross-surface authority.  
**Outputs:** command/query flows in `ARCHITECTURE_AND_DATA_FLOWS.md`, `INPUT_AND_UI_OWNERSHIP.md`, initial findings.  
**Actions:** trace multiple activation routes, explicit prefix behavior, selected-action identity, modal/viewport ownership, and Jump transitions.  
**Done:** current contracts and concrete concerns distinguished from proposed improvements; source counterchecks recorded.  
**Commit:** `docs(audit): [G01/M1-B] trace command and input ownership`.

### M2-A — Lifecycle, persistence, and file safety

**Objective:** analyze asynchronous ownership and potentially destructive boundaries.  
**Outputs:** `LIFECYCLE_AND_CONCURRENCY.md`, `PERSISTENCE_AND_FILE_SAFETY.md`, detailed finding files.  
**Actions:** trace cancellation/retirement, shutdown, source/destination identity, recovery and privacy; consult primary API sources where needed.  
**Done:** each major concern includes mechanism, counterevidence, confidence and future proof; no runtime guarantee invented.  
**Commit:** `docs(audit): [G01/M2-A] document lifecycle and persistence risks`.

### M2-B — Change cost, alternatives, and prioritization

**Objective:** identify worthwhile improvements rather than a refactor wish list.  
**Outputs:** `BOUNDARIES_AND_CHANGE_COST.md`, `DECISION_RECORDS.md`, `FINDINGS.md`, `RECOMMENDATIONS.md`.  
**Actions:** compare smaller alternatives, separate measured history from hypotheses, estimate complexity qualitatively, reconcile known deferred work.  
**Done:** each recommendation has evidence, expected value, risk, and dependency rationale; rejected/no-action alternatives are explicit.  
**Commit:** `docs(audit): [G01/M2-B] prioritize evidence-backed architecture work`.

### M3-A — Complete follow-on library and search handoff

**Objective:** turn findings into executable future specifications.  
**Outputs:** full role proposals and canonical pairs, `FOLLOW_ON_INDEX.md`, `SEARCH_HANDOFF.md`.  
**Actions:** fill all future contracts using real owners and targeted acceptance; deduplicate overlapping role suggestions; classify speculative work as research spikes.  
**Done:** every actionable recommendation maps to a useful package, not a placeholder; search receives the constraints it needs.  
**Commit:** `docs(audit): [G01/M3-A] package follow-on goals and search constraints`.

### M3-B — Critical review and research closeout

**Objective:** finish an honest, navigable, useful audit.  
**Outputs:** reviewer file, resolved/narrowed claims, `REVIEW_AND_LIMITATIONS.md`, completed README and mutable progress.  
**Actions:** one substantive review/correction pass; verify cited paths/IDs and package coverage; perform cheap docs-only checks.  
**Done:** required content exists, review provenance is truthful, uncovered areas are explicit, no production files changed, next goal can proceed.  
**Commit:** `docs(audit): [G01/M3-B] close reviewed static audit`.

After each coherent checkpoint, update progress and attempt a normal push. A role thread limit or missing optional tool is handled by the orchestrator fallback; do not stop the audit merely because the preferred reviewer cannot launch.

## I. Acceptance and limits

This goal is accepted as research when the documents provide source-backed current architecture, meaningful risk-weighted coverage, actionable evidence with confidence, complete follow-on packages, and a useful search handoff. It does not require discovering a particular number of bugs or proving a predetermined architectural thesis.

A source-supported conclusion that an existing design is appropriate is valuable. Record what was examined and why no action is recommended. Do not invent work to fill a quota.

Permitted limitations: unverified runtime behavior, unavailable external documentation, optional static tooling unavailable, and non-independent review labeled honestly. These limit confidence, not the ability to complete useful documentation.

A material missing core analysis remains partial. Narrow the claim or produce a bounded follow-on investigation instead of claiming comprehensiveness. Stop mutations only for a real repository/permission/data-safety conflict. Preserve an exact resume point and continue unrelated permitted analysis when possible.

## J. Execution progress — mutable

**State:** NOT_STARTED  
**Program ID / GOAL_ROOT:** Resolved at execution.  
**Source SHA / branch / worktree:** Not yet resolved.  
**Active checkpoint / role / actual model:** None.  
**Last known commit / push:** None.  
**Next action:** Program bootstrap, then M0-A.

| Checkpoint | State | Actual artifacts/evidence | Known commit/push | Next action or limit |
|---|---|---|---|---|
| M0-A | PENDING | — | — | Bootstrap |
| M1-A | PENDING | — | — | Source inventory |
| M1-B | PENDING | — | — | Command/input traces |
| M2-A | PENDING | — | — | Lifecycle/safety analysis |
| M2-B | PENDING | — | — | Alternatives/priorities |
| M3-A | PENDING | — | — | Follow-on library |
| M3-B | PENDING | — | — | Review/closeout |

### Material events and decisions

- 2026-10-28: Goal authored. No audit findings, measurements, completed research, branch changes, or future implementation are asserted by this document.
