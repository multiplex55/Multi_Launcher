# Follow-On Kickstart Authoring Template

**Authoring template only.** Instantiate alongside each canonical follow-on GOAL.md. The final kickstart must contain that package's actual identity, owners, dependencies, milestone names, evidence paths and constraints. Do not leave placeholder instructions for the user to complete. Its default state is proposed, not currently authorized.

---

# Kickstart <stable-ID> — <Specific outcome>

**Use only when the user explicitly selects this follow-on for execution.** Read the sibling GOAL.md completely. Execute that approved package, not the entire research roadmap and not the historical 17-goal queue.

## 1. Resolve source and authority

Discover the repository root, actual current master/approved dependency base, relevant AGENTS instructions, role configuration, local status, worktree ownership and active Git operations.

The research commit cited in GOAL.md is historical evidence. Revalidate the named owners, functions and assumptions against the current implementation before editing. If the issue is already fixed or the capability already exists, document the source-backed no-op or narrow remaining delta rather than duplicate it.

Resolve the package's actual dependencies. Use the then-current master for independent work or the explicitly accepted prerequisite tip for dependent work. Missing mandatory capabilities are not supplied by reading an old plan.

## 2. Create or resume the correct task branch

For a new authorized run, create one fresh idiomatic branch using the package-specific stem in GOAL.md. Preserve unrelated dirty work, use an isolated worktree when appropriate, and establish its M0-A/progress record before feature edits.

For a confirmed interrupted instance, resume its existing branch after identity/ancestry checks. Do not recreate the baseline, silently return to master, stash/discard user work, force-push or rewrite checkpoint history.

Record base, branch, worktree, original source and current checkpoint in the task's Markdown progress section. Attempt a normal initial push when permitted; local execution may continue after an honestly recorded push failure.

## 3. Use the specified models and sequential roles

Apply the package's planner/implementer/reviewer model recommendations where the runner supports them. Verify pinned role settings rather than assuming prompt text overrides them. Record actual identity or unknown. Do not change global configuration, sandbox or permissions implicitly.

Use bounded planner → implementer → reviewer packets and one writer for overlapping files. A role-capacity failure gets at most one supported recovery, then an explicitly labeled authorized fallback or precise capability report. Do not burn the task on dispatch loops.

## 4. Execute the concrete milestones

Replace this section in the generated kickstart with the package's actual ordered checkpoint instructions. For each checkpoint explicitly state:

- Which real owners and paths to inspect/change.
- The expected behavior or architectural boundary.
- The allowed implementation steps and interfaces/state transitions.
- Compatibility and non-goals.
- Narrow test/verification scope and expected observation.
- The done criterion and commit boundary.

Do not merely write “follow the goal.” The kickstart should be a useful execution directive while the Goal remains the detailed contract. Leave ordinary private helper names and Rust mechanics to the implementer.

Do not broaden into adjacent refactors, recreate existing owners, or execute other recommended packages without authorization.

## 5. Verify proportionately

Use the Goal's future verification matrix. Confirm installed tool/version/target support and nonzero selected test inventory. Keep compile, execution and cleanup bounds separate. Preserve the first failure and meaningful assertions. Do not run the full suite merely because a previous task did so.

For a research/documentation spike, do not run Cargo/native/Clippy unless the selected Goal explicitly authorizes it. For production changes, run the directly relevant tests and appropriate final first-party lint/review; do not conceal unverified native behavior behind unit tests.

If a required platform capability is unavailable, record exactly what remains unverified. Do not invent tool arguments, bypass permissions or repeatedly ask for the same approval. Continue unaffected approved work and stop only the boundary that cannot be satisfied safely.

## 6. Commit, push, and record progress

After each coherent checkpoint, inspect current branch and staged diff, update the mutable progress section, commit using the Goal's stage-tagged subject, and attempt one normal push. Preserve unrelated paths and earlier evidence.

Record actual completed changes, commands/results, review disposition, known commit/push, limitations and exact next action. Do not wait until the whole task is finished to publish useful progress.

A genuine stall gets a Markdown report in the package's declared docs path before stopping when safe. Describe the failure/uncertainty, branch/worktree, preserved state, attempted corrections, remaining allowance, evidence and safe resume instruction. Documentation-only failure commits are allowed when clearly labeled and Git-safe; they do not imply implementation acceptance.

## 7. Close the exact package

Complete required behavior, targeted proof, review and applicable final lint, then record the actual final source/tree and handoff. Reconcile every finding/acceptance item as completed, not applicable with reason, or explicitly incomplete. Do not automatically execute follow-on suggestions.

Report branch/base/checkpoints/final SHA, actual push status, changed owners, tests/native checks run or not run, outcome, limitations, rollback considerations and one next action. Do not merge master, release or perform additional external writes without authorization.

---

## Generated kickstart quality check

The final artifact must be executable after user authorization without another architecture session. It must agree with its Goal, name actual steps and owners, preserve narrow scope, and distinguish optional research from mandatory behavior. A user should not need to replace placeholders or infer which branch, files or tests were intended.
