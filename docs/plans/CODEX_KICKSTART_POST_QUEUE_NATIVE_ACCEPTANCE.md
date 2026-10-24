# Codex Kickstart — Post-Queue Native Acceptance & Stabilization

**Primary contract:** `CODEX_GOAL_POST_QUEUE_NATIVE_ACCEPTANCE.md`  
**Companion starter:** `POST_QUEUE_MANUAL_VERIFICATION_TEMPLATE.md`  
**New isolated branch:** `qa/post-queue-native-acceptance`  
**Required cumulative base:** `7e0f74f117975024b25e7c7cbadef32f29937965` (`feat/productivity-tools` final G3 commit)  
**Review policy:** Autonomous Planner → Implementer → Reviewer, one source writer, **one user review at the very end**.

## Read this before acting

You are the Codex orchestrator operating on the user's existing Multi Launcher Windows/Rust repository. **This is NOT G4 feature development**. The three-Goal cumulative implementation is complete and pushed; Goal 3 ended `BLOCKED_NATIVE_ACCEPTANCE` because specified *real Windows interactions*, not implementation or Clippy, remained incompletely verified. G2 also retained exactly one failing and one timed-out parallel Nextest case. Your task is to finish bounded acceptance where safe, make only causally demonstrated corrections, and provide an **excellent manual verification checklist** for what a machine could not safely test.

Do **not** infer PASS from a previous compilation, an isolated unit test, a mock, an old native screenshot, a corrected driver predicate, or an off-screen/root framebuffer. Existing G3 default and dev+QA Clippy already passed at `7e0f74f1`, so new changes must preserve those clean gates.

### Files to read, in order

1. Current `AGENTS.md`, applicable nested instructions, `.codex/config.toml`, agent runner configuration. Use actual available runners and permissions—not historical model names.
2. Entire `CODEX_GOAL_POST_QUEUE_NATIVE_ACCEPTANCE.md`, then this Kickstart, then the manual-checklist starter. The Goal is authoritative for behavior, scope, proof thresholds, budgets and terminal status. This Kickstart governs sequencing/orchestration; it cannot waive a Goal requirement.
3. `docs/plans/productivity_tools_execution.md`, `docs/plans/capture_productivity_expansion_execution.md`, `docs/plans/CODEX_MULTI_GOAL_QUEUE_LEDGER.md`, and final handoff/QA evidence if present. Do not require a historical uploaded ZIP to execute. The relevant existing code on the exact verified final G3 commit is the implementation source of truth.
4. The available project-native harnesses and verification commands: `dev/run_owned_process.ps1`, `dev/jump_mode/run.ps1`, the existing smoke binaries, and the current `target/multi-goal-queue` Nextest config. Determine real CLI args and feature gates from the repository, not from guesses.

## Hard rules

1. **Create the new branch before application edits or execution.** From the exact verified cumulative G3 final SHA, *not* `master`, *not* an earlier Goal's final SHA, and never by resetting/rebasing history. If already on a newer expected commit, document it and ensure it is user-intended and safe; otherwise stop with a bootstrap discrepancy.
2. Preserve the existing `master`, `big-start`, `fix/launcher-query-focus-jump-notes`, `feat/capture-productivity-expansion`, and `feat/productivity-tools` branch tips. No merge, PR, cherry-pick, reset, stash, rebase, force-push or opportunistic cleanup. Do not stage unrelated data. Five unrelated/unknown untracked files were explicitly preserved in the predecessor, including `clipboard_modifiers.json`—do not access or change them.
3. **One source writer at a time** and one native input/capture session at a time. Parent owns Git, Cargo, native test tools, ledger, source/binary manifests and final report. Planner and Reviewer remain read-only.
4. Use compact Milestone `[B0-A]`, `[B0-B]`, `[M1]` ... `[M9]` records. Every checkpoint gets a truthful terminal classification and named evidence. If it produces no code, no pretend feature commit is required; update the ledger in the next legitimate checkpoint commit.
5. **Native safety precedes completeness.** Synthetic test windows/data only. Check exact HWND/PID/birth/owner/current Response/foreground/hitbox/coverage before injecting clicks, keys or reading pixels. A failed check means **no input** and an explicit gap; never weaken focus/ownership guards merely to achieve green tests.
6. No personal clipboard, personal screenshot, saved notes, actual action files or scanned user directories. Use isolated temporary profiles and generated data. No real monitor brightness modifications, including a one-percent test, without explicit separate user authorization. DFF stays read-only.
7. Do **not** introduce new commands/plugins, change persisted configuration semantics, modify vendor crates, add workspaces, refactor the app across unrelated subsystems, change the default Nextest profile or inflate global timeouts/stack sizes. Preserve `table` vs `csv` defaults, `diff` vs `imgdiff`, existing pastes/macros, launcher focus and user data.
8. Every test/builder/smoke must be bounded and own all of its subprocesses; exact orphan cleanup must use recorded PID+birth/path/parent evidence, **never kill by process name**. Failed approval for a Git push is not permission to bypass controls and not a reason to stop local commits.
9. **No user intervention until end.** Do not ask the user to interact with target windows, enter passwords, manipulate monitors, click file dialogs, or approve destructive actions halfway through. If a native prerequisite fails, classify and continue the next independent checkpoint.
10. The final `docs/plans/POST_QUEUE_MANUAL_VERIFICATION.md` is **mandatory even when tests fail**, and must be generated from actual evidence rather than copying unchanged starter statuses. It needs explicit checkboxes, setup, executable steps, expected result, and failure capture instructions.

## Mandatory Git bootstrap — run now

Use actual Git APIs/CLI. The following commands illustrate the verification sequence; choose a safe worktree if the current one is dirty.

```powershell
# Read-only inspection first.
git status --short --branch
git branch --show-current
git rev-parse HEAD
git remote -v
git branch --list qa/post-queue-native-acceptance
git branch -r --list '*/qa/post-queue-native-acceptance'

# Fetch only if permitted; NEVER pull/rebase/reset/force-update the current branch.
git fetch origin

# Verify that feat/productivity-tools resolves to:
# 7e0f74f117975024b25e7c7cbadef32f29937965
# The following is allowed ONLY if the current index/worktree is safe.
git switch -c qa/post-queue-native-acceptance 7e0f74f117975024b25e7c7cbadef32f29937965

git branch --show-current
git rev-parse HEAD
```

If the branch name is occupied, select a unique descriptive suffix; if the checkout is dirty, use a separate owned `git worktree add -b ... <verified-base-SHA>` and don't modify existing unrelated work. Make `[B0-A]` baseline using only your new ledger or an index-safe truly empty commit. Attempt **one ordinary** `git push -u origin HEAD`, then continue on local commits if the host refuses. Record base/baseline SHA, branch, remote and push result. Source writes **must wait** for this gate.

## Agents and ownership

**Planner (read-only):** For each next checkpoint inspect only its current Rust/egui/native-owner files, determine actual native runner and signatures, identify which phases were already proven, and issue a **bounded packet**. Produce *no code*, no new general framework, no repetitive codebase census.

**Implementer (sole writer):** Execute the current packet. Change production only for a **reproduced, causally demonstrated** defect. Treat fixture/harness bugs as separate from app bugs. Prefer current safety/ownership patterns, egui `Response` current-pass ownership, immutable revision and generation tokens, finite worker shutdown; add exact minimal regression. Preserve source boundaries.

**Reviewer (read-only):** Review dangerous boundaries before native input and after production fixes: strict overlay hit protection, pin clickthrough, target repair transactional writes, note native Undoer, DFF identity/read-only, Settings focus and worker races. Findings must be actionable with an exact owner. No repeated broad reviews after a test fixture typo.

**Parent orchestrator:** Run all Cargo commands sequentially, record exit/status/actual list count and runtime, own native sessions and cleanup, stage only task files, commit/push after coherent verified checkpoints, maintain the ledger and manual-checklist state. A source writer's final message and **all** rustfmt/process tools must finish before starting Cargo.

### Mandatory implementer packet fields

`checkpoint ID`, base/current SHA, owner paths, prior proven phases versus current missing phases, exact deterministic tests, native scenario/fixtures, required pre-input/pixel guards, deadlines/attempt budget, disallowed changes, expected receipts, test/commit gate and native failure-classification rule.

## Execute checkpoints in this fixed order

| Order | Checkpoint | Purpose | Native test needed? |
|---|---|---|---|
| 0 | **B0-A/B** | Verified G3 branch base, independent baseline; runner/environment inventory | No |
| 1 | **M1** | Rectangular selection topmost/input isolation and release/retry | Yes, if safe |
| 2 | **M2** | Pinboard clickthrough actual mouse routing and return to normal input | Yes |
| 3 | **M3** | Saved action/file/folder Repair chooser Cancel/confirm | Yes |
| 4 | **M4** | Notes Replace/Undo/Redo/Save/Discard with Jump on and off | Yes |
| 5 | **M5** | Fresh-binary DFF Cancel/drain/restart/Open/Reveal/Copy and footer | Yes |
| 6 | **M6** | Settings Search slider/numeric/Cancel with Jump on/off | Yes |
| 7 | **M7** | XML Pending race and query-oracle full-load diagnosis; bounded full run | Focused + one full Nextest |
| 8 | **M8** | Read-only physical-monitor capability inventory; no brightness change | Read-only, only if supported |
| 9 | **M9** | Independent final review, cross-owner checks, evidence report, user checklist, Clippy last, Git handoff | No new native campaign |

You may reorder **M1–M6** only when a concrete same-owned-window dependency forces it; preserve their own independent attempt budgets and ledger identity. Never let a failed/blocked native scenario prevent progress on *unrelated* phases, and never interpret a past test as proof for a newly changed binary.

## Uniform checkpoint routine

1. Read current owner and previous receipts. Confirm no concurrent source writer or owned build/native scenario.
2. Ask planner for the smallest precise packet. The checkpoint should start **with a native/evidence attempt or current-source comparison**, not a speculative rewrite.
3. Build current-source fixture using installed repo tooling: prefer current `--cargo-profile fast-dev` with explicitly required `dev-smoke-binaries,qa-framebuffer-capture` only where needed. **Build ≤1,800s**; record binary/source/diff and synthetic fixture hashes.
4. Before input/screenshot, check actual PID/birth/HWND/native owner/current visible `Response`, claimed canvas and foreign occlusion. If denial, send no input, close owned fixture, classify exactly.
5. Run once. **Native budget ≤90s +10 cleanup**, or prejustified ≤180s +10 for multi-leg scenario; bounded campaign ≤20m after build. If a causally demonstrated harness error stops before intended action, correct only harness and make at most the one authorized recheck. If a real product defect is observed, make a narrowly reviewed source fix with a focused regression, build **new current binary**, recheck affected native phase once. No open-ended retries.
6. For code changes: list expected tests before running using Nextest; require nonzero actual selection. At most two relevant slices plus one exact failure recheck. Use zero retries, standard stack and built-in 60s/test, 10m group; don't silently use a different Cargo profile or force serial full-suite.
7. Record explicit statuses for **every required phase**, including `PASS_NATIVE`, `PASS_AUTOMATED_ONLY`, `PRODUCT_DEFECT`, `HARNESS_DEFECT`, `ENVIRONMENT_BLOCKED`, `INCOMPLETE_BUDGET`, `NOT_RUN`. Preserve original failures; a corrected test doesn't erase its cause/evidence.
8. Review/touched Rust formatting/diff checks and commit only the verified owner files plus your ledger. Best-effort one normal push; don't halt if denied. Update manual-checklist evidence and next manual priority incrementally, but do not deliver an incomplete draft as if final.

**No invented metrics:** Record measured test durations or observed process counts only. Native window compositor samples and image comparisons must come from actual owned Desktop composition with full guard; input must come from correctly controlled direct interactions, not a posted proxy substituted for real routing.

## Full Nextest and final lint

**M7** can run one normal full workspace suite *after* focused XML/query corrections; no third full run. Example of permitted intended form (adjust only to currently installed config):

```powershell
cargo nextest list --workspace --cargo-profile fast-dev --config-file target/multi-goal-queue/nextest.toml
cargo nextest run --workspace --cargo-profile fast-dev --config-file target/multi-goal-queue/nextest.toml --profile queue-focus --retries 0 --no-fail-fast
```

The user has previously seen a full run with 6,793 pass/one XML fail/one Track C timeout. **Do not** claim an unchanged isolated PASS is an all-suite PASS. If global time/cap prohibits a full run, record exact limitations and a suggested command in the handoff; do not mask failures with retries, exclusions, increased timeouts or test-thread overrides.

**M9** does the two last small cross-owner groups, draft final evidence/docs/manual checklist, independent review, and source commits/pushes; then Clippy is the **last code-verification gate**:

```powershell
cargo clippy -p multi_launcher --all-targets --profile fast-dev -- -D warnings
cargo clippy -p multi_launcher --all-targets --profile fast-dev --features dev-smoke-binaries,qa-framebuffer-capture -- -D warnings
```

Use separate owned ≤1,800s ceilings. If lint fails, correct only the real first-party findings, rerun affected scoped regression when behavior changes, commit/push correction and repeat required affected lint gate. After both clean gates, you may update only outcome-only metadata in the evidence ledger, results report and manual checklist to record actual lint status and final SHA; no further production Rust, tests, dependencies, app/help documentation or formatter changes without rerunning affected gates.

## Mandatory end-of-task output: manual human acceptance checklist

Use `POST_QUEUE_MANUAL_VERIFICATION_TEMPLATE.md` as a *starter* and write/revise the final repository file **`docs/plans/POST_QUEUE_MANUAL_VERIFICATION.md`**. It must be readable as a standalone checklist, not a QA log dump. Follow these rules:

- Mark **all** current native gaps in G3 and inherited G2 as `REQUIRES_MANUAL`, `ENVIRONMENT_BLOCKED`, or specific verified `PASS_NATIVE`; no silent omissions.
- Rank the remaining human tasks **P0/P1/P2**, and put the **five high-risk interactions first**: input shield/click-through, exact file/folder repair, Notes replacement undo/save, DFF cancel/restart/actions, Settings numeric Cancel/Jump OFF. If a check becomes completely accepted by automation, move it to an *optional sanity check* with actual receipt—not a mandatory repeat.
- Each manual test has checkboxes, a **synthetic/disposable fixture setup**, real discovered UI/command invocation, chronological actions, expected UI and disk outcomes, a failure signal, screenshot/log path, and cleanup. Include normal/minimum size, Jump on/off, DPI/multi-monitor where material.
- Include **one separate optional** real monitor brightness Set step only with explicit user consent, baseline and restoration. Never perform this step autonomously.
- At end include a simple results template (`PASS` / `FAIL` / `BLOCKED` / `NOT_TESTED`), Windows build/DPI/monitors/branch/commit and failure reproduction fields. The user should be able to complete the top five in one focused review session.
- Final file must be updated **after** real verification. Merely copying a prefilled PENDING starter and declaring success is a failure of this Goal.

Also create `docs/plans/POST_QUEUE_NATIVE_ACCEPTANCE_RESULTS.md` with the actual phase evidence table, test and native counts, original/corrected outcomes, final Git and lint facts; and `docs/plans/post_queue_native_acceptance_execution.md` with compact chronological checkpoints. Do not commit giant ignored screenshots/logs unless the repo already explicitly versions small evidence summaries.

### Final reply to the user — required structure

Produce a concise factual summary:

1. Branch base/baseline/final SHA and push outcome; confirm master/previous Goal branches untouched.
2. **Native verified effects** versus **remaining blocked native phases**; state whether any actual product defect was fixed and how it was verified.
3. Focused and full Nextest outcome, default and QA Clippy outcome, owned worker/process cleanup, unrelated untracked file preservation.
4. Link the committed **manual verification checklist**, results report and execution ledger by relative repository path. Highlight **the first five user actions**, not merely feature names.
5. State final outcome exactly `COMPLETE_NATIVE_ACCEPTED`, `COMPLETE_WITH_NATIVE_GAPS`, `BLOCKED_PRODUCT_DEFECT`, or `BLOCKED_BOOTSTRAP` based on evidence. Do **not** call the original Goal 3's native acceptance PASS unless every required previously missing phase was actually demonstrated.

**Do not ask the user for a new goal or manually intervene mid-task. Stop once this bounded contract is exhausted, hand off every remaining uncertainty in Markdown, and do not automatically merge or start new feature work.**
