# Codex Kickstart — Execute Security, Duplicate Finder and Pinboard Fixes

**Authoritative Goal:** `CODEX_GOAL_SECURITY_DUPLICATE_FINDER_PINBOARD.md`  
**Task:** Fix affected rustls dependency resolution, Duplicate Finder dialog vertical size creep, screenshot pinboard native movement latency and slow live-frame refresh. Work autonomously with scoped, verifiable commits. **Do not wait for user input between milestones.**

## 0. Mandatory first execution: safe Git bootstrap (M0-A)

**Before any feature-code changes, builds, or tests**, read `AGENTS.md`, this Kickstart and the entire Goal. Read-only inspect Git status/branch/HEAD, current default integration branch, remotes/upstream and any pre-existing modifications (`git status -sb`, `git rev-parse HEAD`, relevant remote/default-branch checks). A safe fetch to inspect the latest default remote tip is permitted, but no destructive Git operations.

Then **yourself create and switch to a fresh, unique, idiomatic task branch**. Preferred name if available: `fix/rustls-duplicate-finder-pinboard`; otherwise append a meaningful unique suffix. Do **not** use or reuse an existing task branch, and do not carry out implementation on `master`, `main`, or the previously active branch. When safe with a clean tree, branch from the up-to-date actual default integration branch. With pre-existing dirty work, preserve it and branch from current HEAD when safe, recording the base; never automatically reset, pull destructively, stash, rebase, merge, overwrite or stage unrelated user work. If no safe branch creation is possible, report the precise blocker instead of destroying work.

On that new active branch, create its own initial **M0-A baseline commit** before doing application work. A task-specific tracked execution ledger or a deliberate `git commit --allow-empty -m "chore: [M0-A] establish remediation branch baseline"` is acceptable; never stage unrelated files to make a fake commit. Record both **original base SHA** and **new M0-A baseline commit SHA**. Attempt **one normal, non-force upstream push** (`git push -u origin HEAD` if appropriate), recording success/failure. A denied/offline initial push is nonblocking: **continue locally**, with no retry loop, force push or authorization bypass. Every later implementation, test, commit and push remains on this new branch. Report branch, base, baseline and individual push outcomes at handoff.

**Do not proceed to M1-A until branch creation, baseline commit and the initial push attempt are complete.**

## 1. Revalidate current source and apply the Goal

After M0-A, read the current checkout's relevant files and tests, using the Goal's **B. Codebase findings** as an inspection map. The live Git checkout is the implementation source of truth; do not substitute external planning snapshots. If files/functions shifted, locate the same ownership boundary while keeping the specified behavior and narrow scope.

Use the Goal as the feature-specific technical contract; use `AGENTS.md` and `.codex/agents/{planner,implementer,reviewer}.toml` (when available) as the repository's engineering/orchestration contract. Scope is strictly these three issues; no general UI rewrite, network migration, new settings or unrelated fixes.

## 2. Coordinate agents intentionally

Act as the **orchestrator**. When configured, use:

- **Planner (read-only):** Quickly validate the three independent code ownership areas, checkpoint dependencies, meaningful safety hazards and narrow test seams. Provide explicit implementation packets; do not undertake a prolonged re-planning campaign.
- **Implementer (single source writer at a time):** Implement checkpoint by checkpoint, following the Goal's tasks and invariants; inspect compiler errors and repair real regressions within that checkpoint. Do not allow parallel writers to touch shared source or lockfiles.
- **Reviewer (independent):** Review each materially risky checkpoint as helpful, and the combined diff at the end, looking for accidental dependency churn, egui layout feedback, regressions in native window identity/opacity/click-through, broken live frame lease/budget ownership, and tests that merely hide a failing behavior.

If an agent role or tool is unavailable, execute its responsibilities directly; do not block or fabricate agent work. Parallelism is acceptable only for genuinely read-only, nonconflicting investigation. Do not create sprawling deliverables or additional plans.

## 3. Execute and commit the checkpoints in order

Follow the entire Goal; the summary below is an execution index, **not** a replacement specification:

| Stage | Required outcome | Commit rule |
|---|---|---|
| **M0-A** | Fresh branch, new baseline commit, initial upstream push attempt, original base + new baseline SHAs recorded | **Mandatory first commit** |
| **M1-A** | Patch any `rustls >=0.23.13, <0.23.45` resolution in the tracked lockfile with minimal dependency changes; verify reverse dependency graph | `fix: [M1-A] resolve patched rustls security dependency` |
| **M2-A** | Reproduce actual Duplicate Finder rect growth, correct sizing feedback, demonstrate long-run stable geometry plus user resizing and footer accessibility | `fix: [M2-A] stabilize Duplicate Finder window geometry` |
| **M3-A** | Measure static/live/paused pin drag independently, correct proven main/native window bottleneck, preserve HWND/lifetime and input safety | `fix: [M3-A] reduce screenshot pin movement latency` |
| **M4-A** | Raise bounded active live capture cadence from known 100 ms max-delay toward ~33–34 ms; keep coalescing, memory limits, child wake, cancellation and idle efficiency | `perf: [M4-A] increase bounded live pin frame cadence` |
| **M4-B** | Reuse existing owned native Glow and WinRT fixtures for one bounded post-fix acceptance pass if GUI session supports it | Commit only if meaningful fixture/fix changes |
| **M5-A** | End-only first-party lint as appropriate, targeted integration check, final reviewer pass and factual report | Remediation commit only if needed |

Before every later stage, verify you remain on the **new M0-A branch** and the current worktree does not contain unrelated staged files. Commit each coherent verified checkpoint. After each commit, attempt one normal non-force push when supported; if denied, record and proceed locally. Do not pause to ask whether to continue. Do not rewrite previously committed checkpoints or make a large unverified last-minute squash.

## 4. Test like a focused Windows/Rust maintenance task

- **Security:** Check `cargo tree -i rustls --locked` and the actual resolved `Cargo.lock` graph. Prefer a precise targeted patch; do not add a direct `rustls` dependency unless the live graph proves necessary. Keep `reqwest`'s `blocking` and `rustls-tls` features intact.
- **Duplicate Finder:** Extend its current real egui dialog tests to check **actual window dimensions/edges over dozens of steady repaint frames** after convergence, plus manual-resize retention, crowded diagnostics, small viewport footer/scroll, current actions and Jump Mode fences. Do not claim fixed from a screenshot of only one frame.
- **Pin movement:** Establish a concise pre-change measurement of static, active live and paused live pins; fix only confirmed blocking paths. Recheck native position and responsiveness, including topmost/click-through/opacity and cleanup. If the native desktop is unavailable via SSH, report that limitation and keep unit/owned-fixture coverage truthful.
- **Live refresh:** Include deterministic `FrameCadence` and frame publication tests, fake-frame/display preparation/texture tests, bounded worker wake checks, and when possible actual native source/Glow smoke. Generated frame throughput must not be misrepresented as tested OS capture speed. Preserve CPU/GPU byte reservations, old-frame retirement limits, generation fences and latest-frame semantics.
- **Validation budget:** Prefer targeted `cargo nextest` filters, at most one focused failure remediation/rerun cycle per checkpoint, **one necessary first-party compile** after compatible changes, optional end-only Clippy for first-party changes. No unconditional complete Nextest suite, repeated release builds, or refactors for unrelated warnings. Never disable an existing test or weaken a safety assertion to make results green.
- **Visual automation:** Build/run the app or existing native fixtures, interact with owned dialogs/windows and collect bounded screenshots/logs **when the environment supports safe GUI access**. The existing guarded smoke harnesses take precedence over ad-hoc `SendInput` or unrestricted screenshots. Do not request intermediate screenshots or test runs from the user. Cleanup created windows/processes.

The Goal names appropriate modules and filters. Adapt test commands to current workspace, Cargo target and Nextest version; label tests as PASS/FAIL/GAP only according to what was actually executed. If an intended live refresh target is infeasible on the host, report actual measured source vs application limitation rather than claim a guaranteed 30 FPS.

## 5. Operational safety and stop conditions

Make normal engineering decisions independently. Preserve root-window parking, rectangle capture exclusivity, exact HWND/PID/lifetime validation, pin/session limits, click-through recovery, Copy/Save semantics, Duplicate Finder worker cancellation/draining, results safety and existing launcher/hotkey behavior.

If a real blocker occurs, try a **small, bounded, directly relevant** remedy. Never solve a failed push with destructive history operations, bypass native input/screen ownership protections to pass a smoke test, or enter indefinite performance tuning loops. Distinguish product defects from environmental graphical-session restrictions and pre-existing unrelated failures.

Stop **only after** the three scoped corrections, their focused verification, checkpoint commits and final review are complete—or after accurately documenting an unavoidable blocker. Do not start another goal automatically.

## 6. Final handoff — single manual review at the end

Respond with a compact but evidence-based completion report:

1. Actual **new branch name**, original/default base SHA, M0-A baseline SHA, later stage commit SHAs, one-attempt initial and per-checkpoint push outcomes, final branch/status.
2. Security: before/after `rustls` versions, relevant dependency changes, advisory no-longer-affected evidence and compile result.
3. Duplicate Finder: measured old cause and new geometry stability, steady-frame/viewport/resize regression results, optional owned screenshots.
4. Pinboard: exact drag bottleneck fixed, before/after static/live/paused responsiveness and native verification status.
5. Live refresh: before/after source/processing/presentation rate (generated vs actual native identified), memory/idle/cancellation regression evidence.
6. Exact focused test/build/lint commands and results, owned smoke artifacts where created, any pre-existing failures or environmental gaps, remaining feature-related blockers.

Do not claim success for absent tests or native interactions. Present a **single optional manual verification checklist** for the user's end-of-run review; never require intervention during execution.
