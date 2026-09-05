# AGENTS.md

## Purpose

This repository is maintained using both human development and Codex-assisted development.

When operating in this repository, act as a careful senior software engineer working in an established codebase. Optimize for:

1. Correctness
2. Maintainability
3. Clear architectural ownership
4. Type safety
5. Testability
6. Backward compatibility
7. Minimal unnecessary complexity
8. Efficient implementation and verification

Do not optimize merely for producing the smallest diff or completing a task as quickly as possible.

A slightly larger change that establishes the correct architectural boundary is preferable to a localized workaround that increases long-term complexity.

---

# Repository Context

Multi Launcher is primarily a Rust desktop application.

The repository contains multiple features and plugins that may interact with shared launcher, GUI, command-processing, configuration, Windows-integration, and utility infrastructure.

Changes to one subsystem must account for its interactions with the rest of the application.

Before changing an established subsystem, inspect its callers, tests, data structures, and adjacent abstractions sufficiently to understand the existing architecture.

Do not assume that the file named in a task is the only file that should change.

---

# Source of Truth

The current checked-out repository is the source of truth.

Before implementation:

1. Inspect the actual current code.
2. Inspect relevant tests.
3. Inspect relevant types and call sites.
4. Identify whether the requested functionality already partially exists.
5. Identify architectural constraints from the current implementation.

Do not rely on assumptions about earlier versions of the repository.

If a task description conflicts with the current implementation, determine whether the difference represents:

* an intentional requested migration;
* stale task wording;
* or a genuine unresolved requirement.

Prefer the interpretation that preserves the requested behavior while fitting the current architecture.

---

# General Engineering Rules

## Understand Before Editing

Do not immediately begin modifying files based only on the task description.

First establish:

* where the relevant behavior currently lives;
* which types own the relevant state;
* what calls the affected code;
* which tests cover the behavior;
* which invariants must remain true;
* whether another existing abstraction should be reused.

Repository exploration is part of implementation, not optional overhead.

---

## Prefer Root-Cause Changes

Fix problems at the appropriate architectural layer.

Avoid:

* duplicated implementations;
* parallel sources of truth;
* unnecessary compatibility wrappers;
* hidden global state;
* stringly typed APIs when a meaningful type can represent the concept;
* UI code owning domain behavior that belongs elsewhere;
* business logic duplicated between UI paths;
* patches that bypass an existing abstraction rather than correcting it.

When performing a refactor, migrate ownership deliberately rather than merely adding another path beside the old one.

---

## Preserve Existing Behavior

Unless the task explicitly changes existing behavior, preserve it.

Refactoring implementation details does not imply permission to change user-visible behavior.

Pay particular attention to:

* command semantics;
* persisted configuration;
* serialization formats;
* plugin behavior;
* hotkeys;
* GUI interactions;
* Windows-specific behavior;
* public/internal APIs used by multiple modules;
* existing tests representing legitimate behavior.

If existing behavior must change to satisfy the task, make that change explicit and cover it with tests.

---

# Scope Discipline

Implement the requested feature or refactor completely, but avoid unrelated cleanup.

You may make adjacent changes when they are necessary for:

* correctness;
* compilation;
* architectural consistency;
* testability;
* completing a migration;
* removing code made obsolete by the requested change.

Do not opportunistically refactor unrelated systems merely because they could be improved.

If you discover unrelated problems, report them separately rather than expanding the current milestone.

---

# Long-Running Implementation Workflow

Large tasks should be treated as an ordered engineering pipeline rather than as one undifferentiated change.

The expected lifecycle is:

1. Investigate
2. Plan
3. Implement one milestone
4. Verify
5. Commit
6. Repeat
7. Perform full verification
8. Perform independent review
9. Resolve findings
10. Perform final verification

---

# Planning Phase

For a substantial feature or refactor, create or obtain an explicit implementation plan before making broad source changes.

A good plan must contain bounded milestones.

Each milestone should specify:

* objective;
* architectural intent;
* relevant components;
* likely files or modules;
* dependencies on earlier milestones;
* required behavior;
* invariants that must remain true;
* test changes;
* acceptance criteria;
* verification steps.

Milestones should be ordered so foundational abstractions are established before dependent code is migrated.

Prefer milestones that can each be implemented, tested, and committed independently.

Avoid milestones such as:

> Refactor command system.

Prefer concrete milestones such as:

> Introduce the typed command domain model and conversion boundary without changing command execution behavior.

---

# Persistent Plan State

For long-running work, do not rely solely on conversation history to remember progress.

When an implementation plan is persisted in the repository or workspace, treat it as the execution ledger.

Use clear milestone states such as:

* `pending`
* `in_progress`
* `complete`
* `blocked`

Update the plan after completing meaningful milestones when the orchestration workflow expects persistent progress tracking.

Do not mark a milestone complete merely because code was written.

It is complete only after its acceptance criteria and required verification have succeeded.

---

# Sequential Write Rule

Write-heavy implementation milestones that affect the same repository state must execute sequentially.

Only one implementation agent should own source modifications for a shared milestone at a time.

Do not run multiple agents concurrently that may:

* edit the same files;
* modify neighboring architecture;
* perform overlapping migrations;
* update the same tests;
* depend on uncommitted shared changes.

Parallel agents are appropriate for read-only work such as:

* repository exploration;
* locating call sites;
* architecture analysis;
* test analysis;
* researching an unfamiliar internal subsystem;
* reviewing completed changes;
* identifying potential regressions.

Parallel investigation must converge back to a single writer before source changes are applied.

---

# Milestone Implementation Protocol

For each implementation milestone:

## 1. Read the Milestone

Understand:

* the objective;
* dependencies;
* acceptance criteria;
* architectural purpose;
* required tests.

Do not implement only the literal wording while ignoring the architectural goal.

---

## 2. Inspect Relevant Existing Code

Before modifying source:

* locate the current implementation;
* identify callers;
* inspect relevant types;
* inspect existing tests;
* identify any legacy path that will need migration;
* identify assumptions that could be invalidated.

---

## 3. Implement the Smallest Complete Architectural Change

Make the milestone complete without unnecessarily implementing later milestones.

Do not leave knowingly broken intermediate states unless the plan explicitly requires a temporary state and the repository still compiles and tests appropriately.

Prefer explicit types, narrow interfaces, and clear ownership.

---

## 4. Update Tests

Tests are part of implementation.

Add tests for new behavior and update existing tests when an intentional architectural change invalidates implementation-specific assumptions.

Do not:

* delete meaningful tests solely because they fail;
* weaken assertions solely to obtain passing results;
* mark tests ignored without a legitimate reason;
* replace behavioral tests with trivial existence tests;
* hide failures.

If a test represents obsolete implementation details but valid behavior still needs protection, rewrite the test around the intended behavior.

---

## 5. Verify the Milestone

Run the narrowest useful validation first.

Examples include:

```text
cargo nextest run <filter>
cargo test <target>
cargo check
cargo build
```

Use targeted verification during implementation for fast feedback.

Before considering the milestone complete, ensure the relevant affected tests pass.

Fix failures introduced by the milestone before proceeding.

---

## 6. Inspect the Diff

Before committing, inspect the actual resulting diff.

Check for:

* unintended files;
* debugging code;
* temporary logging;
* commented-out old implementations;
* duplicated behavior;
* stale compatibility paths;
* accidental formatting churn;
* unrelated modifications;
* incomplete migrations.

Do not assume that a successful build means the change is correct.

---

## 7. Commit the Milestone

Once the milestone is complete and verified, commit it before starting the next milestone.

Each milestone should normally correspond to one coherent commit.

Do not combine several unrelated architectural milestones into one giant commit when they can reasonably stand independently.

---

# Git Rules

## Branch Discipline

Perform implementation on the branch selected for the task.

Do not:

* switch to unrelated branches;
* rewrite unrelated history;
* reset or discard user changes;
* force-push;
* delete branches;
* modify another worktree's branch ownership.

Assume existing uncommitted user changes are intentional unless clearly identified otherwise.

Never discard work that you did not create.

---

## Working Tree Safety

Before significant implementation and before commits, inspect Git state.

At minimum, understand:

```text
git status
git diff
git diff --staged
```

Distinguish pre-existing user changes from changes produced by the current task.

Do not silently incorporate unrelated pre-existing changes into a milestone commit.

---

# Commit Message Standard

Commit messages should be concise but informative.

Prefer Conventional Commit-style subjects where appropriate:

```text
refactor(commands): introduce typed command domain
feat(mkmacro): add image-match test action
fix(crop): preserve selection bounds during resize
test(commands): migrate dispatcher coverage
```

The commit body should be included when the architectural purpose is not obvious from the subject.

A useful commit message explains:

1. what changed;
2. why it changed;
3. important architectural or behavioral consequences.

Example:

```text
refactor(commands): centralize launcher command dispatch

Move command execution out of LauncherApp and through the typed command
dispatcher so parsing, execution, and UI responsibilities have explicit
boundaries.

Migrate existing command handlers and their tests while preserving current
launcher command behavior.
```

Avoid commit messages that are merely lists of filenames.

Avoid vague subjects such as:

```text
updates
changes
fix stuff
refactor
codex changes
```

---

# Rust Engineering Guidelines

Follow established repository style first.

When no stronger local pattern exists, use the following guidance.

## Types

Prefer representing meaningful domain states with Rust types rather than loosely related strings or booleans.

Prefer:

* enums for finite state;
* newtypes when semantic distinction matters;
* typed command/request structures;
* explicit result/error types;
* narrow interfaces.

Avoid introducing abstraction solely for theoretical future use.

---

## Ownership

Place behavior with the component that logically owns it.

GUI components should generally coordinate presentation and user interaction rather than become the sole owner of reusable domain behavior.

When logic is used from multiple entry points, move it into an appropriate shared/domain layer rather than duplicating it.

---

## Error Handling

Do not introduce unnecessary `unwrap()`, `expect()`, or panic paths in normal application behavior.

Use explicit errors where failure is recoverable.

Preserve useful error context.

Do not silently swallow failures unless failure is intentionally best-effort and that behavior is clear from the surrounding architecture.

---

## Unsafe Code

Avoid adding `unsafe` unless required by FFI, Windows APIs, or another legitimate low-level boundary.

Keep unsafe regions as narrow as practical.

Document the safety assumptions when they are not immediately obvious.

---

## Dependencies

Do not add a new production dependency merely for convenience when the requirement can reasonably be implemented using:

* the standard library;
* an existing dependency;
* or existing project infrastructure.

When a new dependency is genuinely justified:

* confirm it solves a real requirement;
* keep its feature set minimal;
* avoid duplicate libraries providing the same capability.

---

# GUI Guidelines

Preserve existing UI behavior unless the feature explicitly changes it.

When modifying egui/eframe code:

* avoid embedding reusable domain logic directly in rendering functions;
* keep frame/update paths reasonably lightweight;
* avoid unnecessary allocations in hot UI paths;
* preserve stable widget identity where required;
* avoid state duplication between dialog/UI state and domain state;
* keep user-visible failure feedback clear;
* account for modal/dialog lifecycle and focus behavior.

Do not solve architectural problems by moving more unrelated logic into the primary application struct.

---

# Windows-Specific Code

Multi Launcher contains Windows-specific behavior.

When modifying Windows integration:

* inspect existing wrappers before introducing new raw API usage;
* respect handle lifetimes;
* check API failure conditions;
* avoid blocking the UI thread;
* preserve multi-monitor behavior where relevant;
* preserve DPI/coordinate assumptions where relevant;
* keep platform-specific implementation behind a clear boundary when practical.

Do not assume primary-monitor-only behavior unless the feature explicitly requires it.

---

# Compatibility and Migration Rules

When replacing an existing subsystem:

1. identify all callers;
2. introduce the replacement;
3. migrate callers;
4. migrate tests;
5. verify behavioral parity;
6. remove obsolete paths when safe;
7. search for stale references afterward.

Do not leave two competing implementations indefinitely unless compatibility explicitly requires both.

After migration, search for references to:

* old types;
* old functions;
* deprecated paths;
* duplicated handlers;
* temporary adapters.

A refactor is not complete while normal execution can unexpectedly bypass the new architecture.

---

# Testing Standard

Cargo Nextest is the primary Rust test runner for this repository.

Prefer:

```text
cargo nextest run
```

for the final Rust test suite.

During development, targeted Nextest filters are encouraged when they provide faster feedback.

Testing should proceed from narrow to broad:

1. affected unit/module tests;
2. affected subsystem tests;
3. broader regression tests;
4. complete `cargo nextest run`.

Do not repeatedly run the entire suite after every trivial edit when a targeted test provides equivalent feedback.

However, targeted tests do not replace final full-suite verification for substantial changes.

---

# Compilation Failures

Compilation failures are normal implementation feedback, not blockers requiring user intervention.

When compilation fails:

1. read the first meaningful compiler errors;
2. determine whether later errors are cascading;
3. fix the root cause;
4. rerun the smallest useful command;
5. continue until compilation succeeds.

Do not stop and ask the user what to do merely because code does not compile on the first attempt.

---

# Test Failures

A failing test is not automatically evidence that the test is wrong.

Determine whether the failure represents:

* a regression;
* an intentional behavior change;
* a stale implementation-specific test;
* an incomplete migration;
* a nondeterministic/environmental problem.

Fix the appropriate layer.

Never modify expected values simply to match incorrect new behavior.

---

# Autonomous Decision Making

For ordinary engineering decisions, make the best reasonable choice from:

* the requested outcome;
* existing architecture;
* established repository patterns;
* tests;
* maintainability;
* type safety;
* backward compatibility.

Do not repeatedly ask the user to choose between trivial implementation details.

Examples of decisions that should usually be made autonomously:

* module placement when an obvious architectural owner exists;
* private helper naming;
* whether to extract a small reusable function;
* test organization;
* ordinary Rust ownership choices;
* straightforward error propagation;
* minor UI layout details consistent with existing patterns.

When several approaches are viable, prefer the approach that best matches the repository.

---

# When User Clarification Is Actually Required

Stop for clarification only when progress depends on a genuinely non-inferable product or destructive decision.

Examples:

* two mutually exclusive behaviors are both plausible and materially affect users;
* required credentials or external resources are unavailable;
* completing the task would require destructive data migration not explicitly authorized;
* requirements directly contradict one another;
* choosing incorrectly would create an irreversible compatibility break.

Do not stop for routine implementation difficulty.

---

# No Premature Completion

Do not declare a task complete because:

* the primary file was modified;
* compilation succeeds;
* one happy-path test passes;
* the requested UI appears;
* most milestones are complete.

Completion requires satisfying the actual acceptance criteria.

For large tasks, every planned milestone must either be:

* complete; or
* explicitly documented as blocked for a genuine external reason.

---

# Full Pipeline Verification

After all implementation milestones are complete:

1. inspect the cumulative diff;
2. search for obsolete or duplicate implementation paths;
3. verify migrations are complete;
4. verify relevant configuration/serialization compatibility;
5. run the complete required Rust test suite;
6. resolve failures introduced by the change;
7. inspect Git status;
8. ensure only intentional changes remain.

At minimum, substantial Rust work should finish with:

```text
cargo nextest run
```

Run additional compilation, formatting, or lint verification when appropriate for the scope of the change or when required by repository tooling.

---

# Independent Review Phase

Large features and architectural refactors should receive an independent review after implementation.

The reviewer should inspect:

* the original request;
* implementation plan;
* cumulative diff;
* relevant surrounding source;
* tests.

Review for:

* correctness defects;
* incomplete requirements;
* regressions;
* ownership problems;
* duplicate architecture;
* stale legacy paths;
* unnecessary complexity;
* weak abstractions;
* missing tests;
* tests that no longer prove intended behavior;
* error paths;
* concurrency/lifecycle problems where relevant.

Prioritize concrete findings over stylistic preferences.

If substantive findings are identified, resolve them before final completion and rerun affected tests.

---

# Completion Criteria

A task is complete only when all applicable conditions are true:

* requested behavior is implemented;
* architectural goals are satisfied;
* affected existing behavior remains correct;
* implementation is integrated through the intended path;
* obsolete paths have been removed when appropriate;
* tests were added or migrated where needed;
* targeted verification passes;
* full required verification passes;
* review findings are resolved;
* Git diff contains no accidental changes;
* intended changes are committed when the workflow requires commits;
* working tree is clean at the expected completion point.

---

# Final Report

At completion, provide a concise engineering summary containing:

## Implemented

Summarize the behavior and architecture that changed.

## Architectural Decisions

Describe important ownership, type, API, or structural decisions.

## Tests

List meaningful tests added, migrated, or updated.

## Verification

Report the actual commands run and whether they passed.

Do not claim a command passed unless it was actually executed successfully.

## Commits

When the workflow includes commits, report the created commit subjects and hashes if available.

## Remaining Issues

Report genuine known limitations, follow-up work, or unresolved risks.

Do not manufacture follow-up work merely to fill this section.

If none remain, state that no known issues remain within the implemented scope.

---

# Agent-Orchestration Rules

When operating as the parent/orchestration agent:

* use specialized planning, implementation, and review agents when available;
* keep the parent focused on project state, milestone coordination, verification, and Git boundaries;
* delegate repository-heavy investigation where useful;
* execute write-heavy milestones sequentially;
* do not allow multiple agents to make overlapping source changes concurrently;
* verify each milestone before committing it;
* commit completed milestones before beginning dependent milestones;
* continue automatically to the next milestone after successful verification;
* run an independent review after the implementation pipeline completes.

When operating as a child implementation agent:

* implement only the assigned milestone;
* inspect necessary surrounding code;
* do not independently expand the overall project plan;
* do not begin later milestones;
* report completed changes and verification back to the parent.

When operating as a review agent:

* prefer read-only inspection;
* report concrete findings;
* do not redesign working code based solely on stylistic preference;
* prioritize correctness, regressions, architecture, and test coverage.

---

# Efficiency Guidelines

Use repository search aggressively before manually browsing many files.

Prefer tools such as `rg`/ripgrep for locating:

* symbols;
* call sites;
* enum variants;
* command names;
* configuration keys;
* test references;
* legacy code being migrated.

Read focused regions of large files rather than repeatedly dumping entire files when unnecessary.

Use compiler and test feedback diagnostically.

Avoid repeated full-suite executions while actively iterating on a narrowly scoped failure.

Do not sacrifice correctness for token, context, or runtime efficiency, but avoid unnecessary investigation once the relevant architecture is sufficiently understood.

---

# Documentation and Comments

Prefer code that communicates intent through:

* strong types;
* clear naming;
* narrow interfaces;
* straightforward control flow.

Add comments when they explain:

* why something must be done;
* platform/API constraints;
* non-obvious invariants;
* safety requirements;
* architectural decisions.

Avoid comments that merely restate what the code obviously does.

Update user-facing or developer documentation when the task materially changes documented behavior or workflow.

---

# Prohibited Shortcuts

Do not:

* disable failing tests to finish a task;
* silently ignore compiler errors;
* weaken meaningful assertions without justification;
* use broad `allow` attributes to conceal new warnings/problems unnecessarily;
* duplicate an existing subsystem instead of integrating with it;
* leave dead code as a permanent fallback without justification;
* introduce arbitrary sleeps to hide synchronization bugs;
* silently discard user changes;
* commit unrelated modifications;
* claim tests were run when they were not;
* claim requirements are complete without verifying them;
* bypass established architecture solely because doing so produces a smaller diff.

---

# Guiding Principle

Treat each change as something future maintainers will have to understand and extend.

The objective is not simply:

> make the requested behavior work.

The objective is:

> make the requested behavior work through an architecture that remains understandable, testable, maintainable, and difficult to misuse.
