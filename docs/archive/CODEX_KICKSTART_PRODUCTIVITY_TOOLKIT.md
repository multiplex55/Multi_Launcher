# Codex Kickstart — Productivity Toolkit Expansion

Execute the complete `CODEX_GOAL_PRODUCTIVITY_TOOLKIT.md` for `multiplex55/Multi_Launcher`. This is authorization to implement the approved Goal, not a request for another questionnaire or a planning-only response.

**Q1-A is settled:** implement the internal Notification Inbox and a real, supported external Windows notification capture route. Its installation/registration/permission setup is optional for the end user, **not optional implementation scope**. Ordinary launcher builds and internal notifications must remain usable without that setup. The Goal specifies all 14 features and the selected routine defaults.

## 1. Read instructions, then perform M0-A before application work

Read the current repository's `AGENTS.md`, any applicable local instructions, the entire companion Goal, and this Kickstart. Locate the Goal by its exact filename in the repository or available task documents; it is self-contained. The Goal is the feature-specific implementation contract; current repository instructions define engineering and agent orchestration. Historical plans, old stalled-goal procedures and older model preferences are not additional authority unless adopted by the current Goal.

**Your first execution checkpoint is the chained Git bootstrap below. Do not edit application code, run implementation builds/tests or begin another checkpoint before completing it.** Read-only source/Git inspection needed to select the correct parent is allowed.

### Mandatory linear ancestry, baseline and initial push

1. In the **single existing checkout**, inspect `git status -sb`, branch name, `git rev-parse HEAD`, remotes/upstream, staged/unstaged/untracked files and the prior Goal/category branch/tip recorded by the queue, orchestrator or execution ledger. Identify user changes before doing anything that could affect them.
2. Determine whether this is a genuinely new chain or a continuation. For a continuation, the **required parent is the exact `final_tip_sha` of the immediately preceding completed Goal/category branch**, even when the checkout currently points to master. Capture `parent_branch` and `parent_tip_sha` and verify the predecessor's intended changes are committed. Fetch a remote-only parent safely if necessary. Never substitute a stale remote branch for an unpushed predecessor tip.
3. **Only the first Goal in a genuinely new chain may default to the up-to-date primary integration branch**, ordinarily master/main. The Goal's inspected SHA `c4170d0853ce462b44840be54c25c4a8d1b339ac` is a planning reference, not a command to reset there or skip inherited work. A safe documented current-HEAD fallback is permissible only for a new chain with no predecessor and no user-work loss or accidental inclusion. If parent records conflict, report the exact blocker rather than guessing.
4. Select a **fresh, unique** lowercase kebab-case task branch, such as `feat/productivity-toolkit-expansion`, with a suffix when necessary. The Goal and Kickstart share **one branch**, and all milestones, checkpoints, agents and tests use that branch. An existing active task branch is not permission to omit the new child branch. The Goal does not authorize internal category branch transitions.
5. Preserve user changes. Never automatically reset, hard-pull, stash, rebase, merge, force-checkout, overwrite files or stage unrelated work to manufacture a safe transition. If selecting the required exact parent is unsafe with the current worktree, stop with the precise Git blocker instead of branching from the wrong place.
6. Create and switch directly from the recorded SHA using `git switch -c <unique-task-branch> <parent_tip_sha>`. Verify the new active branch and starting HEAD. Do not implement on master/main or on the predecessor.
7. Create the new branch's own **M0-A baseline commit**, using a small task ledger or an intentional empty commit where appropriate. The default ledger location is `docs/plans/execution/standalone/productivity-toolkit-expansion/STATUS.md`; reuse an authoritative existing task ledger rather than creating competing trackers. Use `chore: [M0-A] establish productivity toolkit branch baseline`. Stage only task-approved files. Record `baseline_sha` separately from `parent_tip_sha`; verify `git rev-parse <baseline_sha>^` equals that required parent. The predecessor's last commit is not this Goal's baseline.
8. Attempt one normal, non-force upstream push, normally `git push -u origin HEAD`. If unavailable, rejected or denied, retain the local branch/commit, record the exact failure and continue safe local work. Do not bypass restrictions, reset/rebase/merge to repair a push, or retry in a loop.
9. Complete every later checkpoint on this same branch. Commit small coherent increments and attempt one ordinary best-effort push per checkpoint. Do not create per-agent/per-checkpoint branches, squash checkpoints, rewrite history or perform intermediate merges/cherry-picks merely to advance work.
10. After the last commit, record the literal `final_tip_sha`, actual branch, parent, baseline, checkpoint SHAs, push status and clean/dirty state. **The next Goal must create its new branch from this exact final tip, not from master/main or an old common base.** A different machine without an unpushed required ancestor must block until it is available, not silently break the chain. Final integration into master is a separately approved boundary.

**NO GIT WORKTREES:** Never run `git worktree add`, `git worktree move`, or `git worktree prune` for this workflow. Do not create auxiliary checkouts, secondary clones, agent-specific repository copies or independent side branches. All source writes and branch transitions happen sequentially in the current checkout. Safe read-only investigation may be parallel, but source edits, commits and build/test processes must not compete.

M0-A is accepted only when the correct child branch is active, its baseline is a direct child of the recorded parent, the initial push attempt/status is recorded, and unrelated user work is preserved. Do not continue feature work after a failed ancestry check.

## 2. Reconcile current source without reopening approved requirements

After M0-A, execute M0-B. Confirm the actual affected files, call sites, tests and existing abstractions against the Goal's source map. The checked-out task parent is execution truth; adapt a moved path or already-implemented portion without discarding inherited work or reducing the approved behavior.

The following are deliberate requirements, not questions for the user:

- Preserve the existing numeric-vector model, 1,024-component limit, old function meanings, formula-function no-capture behavior and evaluation/work budgets.
- Use explicit callback parameters for vector mapping/filtering; retain pure operations and accurate error publication.
- Extend existing charts, their host and shared rendering rather than inventing a separate plotting tool.
- Expand the current in-app examples/reference into complete API documentation, including all old and new APIs.
- Support Markdown draft/undo behavior and shared Notes/Studio export/reordering.
- Preserve ZIP safety while making its destination convenient.
- Enforce password privacy at the actual generation/history/clipboard boundaries, not merely by masking a label.
- Keep file transfers explicit and identity-checked; interrupted/partial work is not completed work.
- Keep external notification setup optional, collection opt-in, and capture demonstrably functional on a supported route.
- Preserve ordinary keyboard/focus behavior and standard application builds.

Confirm the bounded native notification consent/identity and PDF/print path early. Use current official platform documentation and a minimal focused native proof when needed. A sparse/identity package alone does not prove the consent UI works in this desktop host. A narrowly scoped optional consent/listener companion is permitted only if required by the supported route, without migrating the main GUI or requiring its setup for ordinary use.

Do not convert the initial reconciliation into a broad audit, full-suite baseline, benchmark project, framework investigation or new questionnaire. Record routine choices and proceed. A consequential unexpected incompatibility must have concrete evidence; preference uncertainty is not a blocker.

## 3. Use the configured planner, implementer and reviewer

Read `.codex/config.toml` and the actual `.codex/agents/` definitions. Use their current roles and model/reasoning settings without replacing them based on old conversations. Do not invent unavailable tools or claim an independent review occurred when no independent reviewer ran.

**Orchestrator:** Own sequencing, the authoritative ledger, branch safety, bounded verification, commits, push attempts and final reporting. Maintain one active source writer and one active shared build/test process.

**Planner:** Validate the next checkpoint's real ownership, dependency and test scope. Hand the implementer an explicit packet containing objective, current source facts, affected files/types, required tasks/behavior, edge cases, invariants/non-goals, verification, acceptance and commit boundary. Reuse the Goal's existing packets; do not re-plan all 14 features repeatedly or pre-implement the work.

**Implementer:** Perform the actual code changes for one assigned checkpoint at a time in the shared checkout. Inspect relevant callers, implement through existing owners, add/update meaningful tests and run focused verification. Return changes, decisions, exact commands/results and remaining issues to the orchestrator. Do not start a new milestone independently or change branches.

**Reviewer:** Perform bounded read-only review of completed relevant changes and a final cumulative review. Concentrate on correctness, scope, state ownership, user-data safety, privacy, compatibility and missing acceptance. Avoid speculative redesign and unrelated tests. Resolve substantive findings before claiming completion.

Read-only investigations can overlap only when the snapshot is understood and they cannot interfere with the active writer. No worktrees, helper clones, alternate implementation branches or competing file edits. If a role/tool is unavailable, report that limitation and perform the equivalent work sequentially without falsely labeling it independent.

## 4. Execute the complete Goal, not only the convenient features

Follow the dependency-ordered checkpoint packets in Goal section F:

| Stage | Work |
|---|---|
| M0-A/B | Exact-parent branch baseline, source/ownership reconciliation and bounded native choices |
| M1-A | Automatic ZIP destination through guarded extraction |
| M2-A/B | Pure vector sorting/uniqueness and named-callback transformations |
| M3-A/B/C | Rolling/cumulative calculations, quantiles/dispersion and paired statistics/linear fits |
| M4-A/B/C | Histogram/CDF data, compatible chart options and existing companion UI |
| M5-A/B | Complete executable API reference and searchable/navigation/insertion UI |
| M6-A/B | Source-preserving section moves and Notes/Studio interactions |
| M7-A/B | Shared paginated PDF output, preview and native printing |
| M8-A/B | Secure generation/privacy and password/passphrase utility |
| M9-A/B/C/D | Transfer planning, execution, recovery and File Operations Center |
| M10-A/B | Downloads scan/review state and explicit triage tool actions |
| M11-A/B/C/D | Inbox records, internal events/UI, optional Windows setup and real external capture |
| M12-A/B | Remaining existing-utility/editor keyboard gaps and cross-tool focus consistency |
| M13-A | Targeted final review, evidence, documentation and final-tip handoff |

All 14 feature IDs remain in scope: **L11, P5, P6, P7, P10, P11, CP-DOC, N3, N5, U13, ZIP-DEST, X8, X9 and X25**. The new UI checkpoints must include their keyboard behavior immediately; M12 is not permission to leave those dialogs mouse-only until the end.

Continue automatically after successful checkpoints. Do not stop after the first feature group, after a review, after writing a plan, or after a push failure. Keep milestones pending/in_progress/complete/blocked in one ledger, and distinguish implemented from verified. A feature's button, skeleton, mock, TODO or Unsupported message is not its completion.

A genuine environment blocker may leave a dependent checkpoint blocked while independent work continues on this same branch. Preserve the blocker and its acceptance requirements. Do not silently remove external capture, print, recovery or another difficult requirement to produce a green summary. Do not carry uncommitted partial work across a future branch/category boundary.

## 5. Work autonomously and preserve behavior

You are authorized to inspect source/configuration, make the scoped application and documentation changes, add justified narrowly selected dependencies, run targeted checks, build/launch the app, interact with its UI when supported, capture appropriate evidence, fix goal regressions, commit and attempt normal pushes.

Do not ask the user to run intermediate commands, provide screenshots, test a checkpoint or approve routine Rust/UI choices. The user will perform a final review after your work. Ask for intervention only for a genuinely non-inferable consequential decision, missing external authority or an unavoidable destructive action outside the approved contract. Ordinary compile errors and failed tests are implementation feedback to resolve.

The optional external listener may need native registration/consent; that is authorized feature work, not permission to bypass Windows security, silently trust certificates, access other users' notifications or install services. Test in disposable/authorized application data and desktop contexts. No production signing purchase or account credential use is authorized. Record an unavailable SDK/identity/permission/printer environment honestly rather than manufacturing evidence or burdening the user with repeated intermediate tests.

Preserve command semantics, existing document formats, root-query focus, dock/pop state, hotkeys, ordinary clipboard capture, ZIP/rename/replace/Hot Folder safeguards, startup/idle efficiency and unrelated tools. Avoid workspace-related features, GUI framework changes, automatic Downloads cleanup, new generalized runtimes or unrelated refactors. Do not change `.codex` model settings or vendored libraries without a concrete approved necessity.

## 6. Verify proportionally, using real evidence

Follow Goal section H and each checkpoint's focused verification. Discover actual test names/targets before selecting Nextest filters. Prefer relevant unit/integration targets and cached development builds, with a final ordinary production build at the appropriate boundary. Do not run a full suite or repeated release builds by habit.

Keep native/clipboard/hotkey tests isolated and serial where needed. Follow `AGENTS.md`'s test-fixture rule disabling Paste Sequence Next/Cancel shortcuts without altering production defaults. Bound potentially hanging individual tests to roughly 100 seconds of execution after compilation using supported runner settings; do not impose that as a whole-build deadline. An apparent concurrent/interference failure may receive one isolated rerun before diagnosis; never delete, ignore or weaken valid tests to make results pass.

For visible UI changes, build and launch when possible, exercise the affected controls and record a few representative screenshots or equivalent evidence. Include dock/pop, keyboard and current-draft behavior; do not build a pixel-perfect automation platform. Keep generated secrets, unrelated notifications and private files out of screenshots/logs. Inspect representative rendered PDF pages and actual output metadata, not just writer success.

Verify at least one real supported external notification capture after opt-in, as well as denied/revoked/off paths; mocks alone cannot close Q1-A. Confirm actual printer submission when a suitable printer/virtual printer is available; do not claim physical printing from an API return. Mark unavailable native checks explicitly, and keep the Goal partial when mandatory acceptance remains unmet.

Use one final first-party Clippy pass covering affected targets, then focused reruns for corrections. Fix new goal-related findings; record pre-existing unrelated ones without broad suppressions or cleanup. Inspect cumulative source/config/data changes for unintended edits.

## 7. Commit at active checkpoints and keep the ledger truthful

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** Use Goal section F's labels/subjects. M0-A is the first separate baseline commit; later verified increments stay on that branch. Natural additional splits or remediation are small labeled commits, not new branches or excuses to expand scope.

Before committing verify branch, HEAD, staged paths, scope and relevant validation. Stage only task changes and protect user edits. After committing attempt one ordinary non-force push; preserve the commit and continue on rejection. No force push, rewrite, squash, unrelated staging or intermediate integration.

Record parent and baseline separately, checkpoint outcomes, actual commands/results, native limitations and push state. Do not copy secret content into the ledger. After the last commit, capture literal `final_tip_sha` in the final response and any existing external orchestration record. Do not repeatedly amend/commit to put a commit's own SHA inside its tracked contents; the tracked ledger can identify the final checkpoint and the final response supplies its actual SHA.

## 8. Handle genuine blockers with a finite budget

Fix goal-caused compiler/test failures at their owning boundary. For a persistent problem with no new evidence, use the Goal's limit of two materially different focused remediation attempts after initial diagnosis before reassessing; do not repeat identical commands or native-capture loops indefinitely. New evidence may justify a narrowly recorded additional attempt, not a new qualification project.

Separate goal defects, unrelated pre-existing issues and optional improvements. Push failures are not implementation blockers. Missing native prerequisites are not proof that a feature works. Never relax acceptance to hide a blocked check.

At a genuine stop, preserve all work, record the precise affected checkpoint, expected/actual behavior, attempted remedies, relevant evidence and current Git state, and write the concise `AGENTS.md`-required stall report in the Goal's execution directory when safe. Continue independent work where possible. Do not promise asynchronous completion, invent successful checks or ask the user to take over routine debugging.

## 9. Final review, report and next-Goal handoff

Reconcile every feature row against Goal section J. Have the configured reviewer examine the final cumulative diff for actual defects and missing scope; resolve relevant findings in small commits. Run only the appropriate final targeted union/lint/build/UI checks, not a new unlimited campaign.

The final response must follow Goal section K and include: actual completion status; a 14-row implemented/verified/remaining table; milestone/checkpoint outcomes; significant engineering choices and source areas; exact verification commands/results and native evidence; entry points and optional listener setup instructions; remaining limitations; and complete Git ancestry/push details.

Report `parent_branch`, `parent_tip_sha`, current task branch, `baseline_sha`, checkpoint SHAs, literal `final_tip_sha`, push status and clean/dirty state. State that no Git worktrees, auxiliary checkouts or intermediate branch merges were used. **Tell the next Goal/orchestrator to create its fresh branch directly from this `final_tip_sha`, never master/main between Goals.** Do not call a blocked/partial branch a completed predecessor without explicitly recording that unresolved status.

Stop when the approved work and its bounded validation are complete, or at a precisely documented genuine blocker. Do not merge to master, open an integration PR, release the application, begin another feature or perform unrelated cleanup without separate authorization.
