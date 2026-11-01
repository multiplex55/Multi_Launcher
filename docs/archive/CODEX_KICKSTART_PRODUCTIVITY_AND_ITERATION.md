# Codex Kickstart — Productivity Tools and Faster Iteration

Execute **`docs/plans/CODEX_GOAL_PRODUCTIVITY_AND_ITERATION.md`** completely and autonomously, within its approved scope and verification bounds. This Kickstart coordinates execution; the Goal is the detailed feature/acceptance contract. They are one pair, not separate implementation tasks.

Read current `AGENTS.md` and applicable local instructions, then this entire Kickstart and the entire companion Goal. Use the actual checked-out repository after the mandatory Git bootstrap as the source of truth. The planning reference is `multiplex55/Multi_Launcher` at `261651ed24e55335c56ea807dd7334cb0da8a045`; it is not an instruction to rewind a newer valid predecessor. No uploaded source archive or conversation reconstruction is required.

## 1. FIRST EXECUTION — exact predecessor, fresh child branch, M0-A, push

Before changing application code, running implementation builds/tests, or beginning M1, perform this complete bootstrap. Start with read-only inspection; repeat it at each of the seven explicitly approved category transitions.

1. **Inspect the single existing checkout.** Read `AGENTS.md`/local instructions. Check `git status -sb`, active branch, `HEAD`, remotes/upstream, integration-branch name, tracked/untracked user changes, and the active queue/ledger's predecessor record. Do not stage unrelated files or silently overwrite existing plans.
2. **Resolve the exact parent tip.** A continuing Goal/category must start at the immediately preceding completed Goal/category branch's recorded **`final_tip_sha`**, not `master`/`main`, an old common base, or whatever branch happens to be checked out. Record `parent_branch` and `parent_tip_sha`; verify the previous intended changes are committed and its tip is real. Fetch a remote-only predecessor safely and verify the exact SHA. A newer unpushed local predecessor remains authoritative in this checkout. Conflicting/incomplete ancestry records are a bounded blocker, not permission to guess.
3. **Only the first task of a genuinely new chain may default to the up-to-date primary integration branch.** Discover/fetch its actual tip safely without automatic pull/reset/merge. Do not treat an old closed queue as an active predecessor. A documented safe current-HEAD starting exception is allowed only for the first task when no predecessor exists and user changes are preserved; never use it to evade a known required parent.
4. **Choose a fresh, unique, idiomatic branch for this category.** Use lowercase kebab-case with an appropriate `feat/`, `fix/`, `perf/`, `refactor/`, `chore/`, or `docs/` prefix. Add a unique date/run suffix if needed. An already active branch does not remove the requirement to create the category's new child. The Goal and Kickstart use the SAME branch; agents and checkpoints do not get separate branches.
5. **NO GIT WORKTREES OR AUXILIARY CHECKOUTS.** Use the single existing working directory throughout. Do not run `git worktree add`, `git worktree move`, or `git worktree prune`; create no agent worktrees, secondary clones, helper checkouts, or parallel implementation branches. Read-only reasoning may be delegated on a stable revision, but only one source writer and one Git owner operate at a time.
6. **Protect user changes.** Never automatically reset, stash, hard-pull, rebase, merge, force-checkout, force-push, delete branches, overwrite pre-existing work, or stage unrelated changes. Do not alter another checkout's ownership. If switching to the required parent cannot be done safely, stop at that transition and report exactly what blocks it; never pick another base merely to keep moving.
7. **Create and switch directly from that exact SHA:** `git switch -c <new-category-branch> <parent_tip_sha>`. Verify the initial child `HEAD` equals the required parent before implementation.
8. **Make this category's own `M0-A` baseline commit.** Use a small task-owned execution-ledger update, or an intentional empty commit such as `git commit --allow-empty -m "chore: [C01/M0-A] establish iteration category baseline"`. Adapt the category label. Capture `baseline_sha` independently of `parent_tip_sha`. Verify `git rev-parse <baseline_sha>^` equals the recorded parent tip. The predecessor's last commit is not this category's new baseline; no implementation checkpoint may precede it.
9. **Attempt one ordinary initial upstream push.** Use the existing upstream or `git push -u origin HEAD` after verifying `origin`. If unavailable, denied, or rejected, record the exact outcome, retain the local branch/commit, and continue safe local work. No retry loop, security bypass, credential changes, force-push, or history rewrite. Every later checkpoint gets one ordinary best-effort push. If a later machine lacks an unpushed required predecessor, block instead of substituting a stale remote commit.
10. **At category completion, hand off the actual final tip.** Commit all intended changes, record branch/parent/baseline/checkpoint identities, push state, and dirty-file disposition, then capture `final_tip_sha`. The next category's new branch MUST start directly at this exact SHA. Do not merge, rebase, squash, or cherry-pick intermediate branches to advance. The final branch already contains the cumulative work; only one final integration PR/merge is contemplated, with no automatic merge or approval.

Bootstrap acceptance: correct fresh active branch, correct predecessor, direct-child baseline, recorded push attempt/unavailability, preserved user work, and no worktree/extra checkout. This is mandatory for every category, including performance/refactor/cleanup work.

Avoid self-referential commit loops: record a created SHA in the next ordinary ledger update or successor baseline, and record the last final tip in the final response after the last commit. Any later commit changes the tip and must change the handoff value.

## 2. Approved scope — decisions are settled

The user selected **both recommended questionnaire options and authorized proceeding**. Do not ask the questions again or request another routine confirmation.

The approved Diff behavior is:

- **Q1 A:** ignored, manually selected whole-line blocks on either side. Preserve original text; keep ignored content distinguishable and restorable; exclude ignored-only changes from ordinary difference counts/navigation. Do not add arbitrary character-substring ignoring or a separate Isolate alignment mode.
- **Q2 A:** the focused two-way package: direct text input; reuse and improve access to existing manual alignment; compare an actual source selection with one explicit clipboard read in a new temporary comparison; line bookmarks; Identical-only display and adjustable difference context. No three-way merge or synchronization.

The rest of the approved scope is direct MkMacro Open Folder/Open URL actions; Jump Mode settings wording and individual Quick Tools targets; a Unicode/invisible-character inspector; existing Compact/Comfortable density controls; persistent ordinary utility sizes; selective ZIP extraction; large-data startup; and faster incremental compilation, selected-test preparation, fixtures, expensive GUI tests, and test isolation.

The Goal's C1–C7 define exact behavior, defaults, edge cases, privacy, and acceptance. The old 190-item list is not implementation scope. Do not complete unrelated Table, Hot Folder, screenshot, macro-history/recipe, or search-ranking programs. Historical documents/candidates are evidence only; do not import them or inherit their verification matrices automatically.

## 3. Validate current owners once, then execute

After C01/M0-A, perform C01/M0-B's focused source/capability pass. Confirm the inspected owners, actual selected test targets, existing geometry persistence, Macro authoring/package integration, and native/GUI verification route. Revalidate only changed or uncertain facts at later category starts; do not repeat a whole-repository audit.

The current foundation already includes typed Macro/runtime owners, two-way Diff/manual alignment, configurable Jump settings, density preferences, an archive catalog/planner, iteration profiles, a table-core crate, and isolated GUI fixtures. Reuse them. Do not duplicate those features or assume an existing isolated fixture is lightweight.

A source-only review is not a native pass. A merged documentation PR is not a shipped writer. A test of an archived candidate is not a test of the current source. All new helpers/types named as proposed in the Goal remain implementation choices subject to the actual code.

Create/use one compact execution ledger, proposed `docs/plans/PRODUCTIVITY_AND_ITERATION_EXECUTION.md`. Record category parentage, checkpoints, focused commands/results, timings, relevant evidence, and blockers there. Do not generate extra Goal/Kickstart pairs, a large report hierarchy, or speculative follow-on programs. Keep raw outputs/screenshots only as necessary for reproducible evidence, without private data.

## 4. Agent orchestration and autonomy

Follow the repository's actual configured planner/implementer/reviewer/orchestrator workflow. Read its current role/model configuration; do not change it, hardcode stale preferences, or claim a resolved backend/effort without evidence. Do not require roles/tools that are unavailable.

**Orchestrator:** sole owner of Git transitions/commits, category order, ledger, verification scheduling, blockers, and final handoff. Maintain at most one active writer. Do not run competing Cargo jobs merely to appear parallel.

**Planner:** for each category, verify its current owner/callers, concrete checkpoint tasks, invariants, and smallest useful test selection. Use the Goal's existing contract rather than re-planning the project. Stop exploration once the implementer can act confidently; no broad builds or hypothetical debug campaigns during planning.

**Implementer:** one checkpoint at a time, in the shared checkout. Make the smallest complete architectural change; include necessary caller/test/schema migration. Use compiler feedback for ordinary Rust choices. Do not create another branch/check-out, update agent settings, stage user work, or broaden scope.

**Reviewer:** independently inspect meaningful completed category changes and the cumulative final diff for correctness, regressions, security/privacy, and honest acceptance. One review plus a focused correction/confirmation is sufficient absent a substantive new issue. Do not require a new review cycle for every trivial text change. Read-only reviews may run in parallel only against a stable recorded revision without conflicting source activity.

When a configured role is unavailable, perform the required work in the active context and accurately report that independent review was unavailable; do not fabricate delegation. Make routine engineering decisions autonomously. Do not ask the user to create branches, run commands, provide screenshots, or approve continuation after each checkpoint. They will perform one manual review at the end.

## 5. Execute this exact category sequence

| Order | Category/branch stem (make unique) | Checkpoints | Main responsibility |
|---|---|---|---|
| 1 | C01 `perf/productivity-iteration` | M0-A, M0-B, M1-A–M1-C | Baselines; compile/selected-test cost; fixtures/GUI tests/isolation. |
| 2 | C02 `feat/mkmacro-direct-open` | M0-A, M2-A–M2-B | Typed direct opens, runtime, authoring, packages. |
| 3 | C03 `feat/diff-text-workflows` | M0-A, M3-A–M3-D | Memory text, ignored lines, selected extras, alignment usability. |
| 4 | C04 `feat/utility-ui-usability` | M0-A, M4-A–M4-C | Jump naming/targets, density, shared size memory. |
| 5 | C05 `feat/unicode-inspector` | M0-A, M5-A–M5-B | Pure Unicode observations and local utility UI. |
| 6 | C06 `feat/zip-extraction` | M0-A, M6-A–M6-C | Native preparation, writer/lifecycle, selective-extraction UI. |
| 7 | C07 `perf/large-data-startup` | M0-A, M7-A–M7-B, M8-A | Startup optimization, final performance checks, cumulative closeout. |

Each row starts from the preceding row's actual final commit. The first row resolves an active predecessor or new-chain integration base under Section 1. M8-A stays on the C07 branch; it is not a new cleanup branch. Category branches are not seven new plan pairs.

For every checkpoint:

1. Read its objective, ownership, tasks, behavior, edge cases, regression constraints, verification, acceptance, and commit boundary in the Goal.
2. Inspect only relevant current source/callers/tests. Reuse established owners; do not create parallel sources of truth.
3. Implement the complete increment and meaningful tests. Preserve working behavior and stored formats; do not hide failures or weaken assertions.
4. Run the narrowest useful nonempty test/build selection. Keep compilation/preparation separate from test execution. Native/UI proof must correspond to the current affected source.
5. Inspect working/staged diffs for unrelated edits, temporary debugging, duplicate behavior, incorrect migrations, and raw/private-content logging.
6. Update the concise ledger with actual results and source identity; mark complete only when required acceptance succeeds.
7. Commit the coherent increment using the Goal's checkpoint label, and attempt one normal push. Continue locally after a push failure.
8. At a category boundary, commit intended work, finalize its status/tip, and execute the next M0-A bootstrap from that exact tip. Never carry dirty partial work silently.

Use the project's active checkpoint commit cadence and the task-specific boundaries in the Goal. No per-agent/per-checkpoint branches and no intermediate merge/rebase/squash/cherry-pick.

## 6. Critical implementation guardrails

### Direct opens

Use direct validated folder/HTTP(S) OS dispatch, never a fuzzy query or shell-command string. Authoring/pickers do not launch anything. Preserve existing Macro schema/reuse operations and error/cancel behavior. Deny unintended unattended capability; do not claim successful browser network navigation merely because OS handoff succeeded. Keep path/URL payloads out of routine debug logs.

### Diff

Keep raw document content, effective comparison, and view filtering separate. Ignored line ranges must never delete source text or suppress unrelated changes in a mixed hunk. Preserve existing file Save/conflict/encoding/folder behavior. Memory documents use explicit Save As and never fake paths/temp files. Reuse existing alignment. Selection, active hunk, ignored ranges, bookmarks, and anchors need distinct source/revision identities. Reject stale work after source edits or replacement. No implicit clipboard reads, history persistence, or external selected-text capture.

### Jump, density, geometry, and Unicode

Hints focus/select; subsequent normal activation executes once. Preserve Quick Tools' `query`/`queryexec` semantics and old config. Rename user-facing Jump wording without renaming its saved key. Use the existing density/preview owner and do not alter the text-size preference. Persist only bounded per-tool geometry, not content or transient UI memory. Apply geometry behavior consistently to the recorded ordinary resizable-tool inventory and keep overlays/native pickers/main position outside scope. Unicode inspection preserves input and shows truthful scalar/grapheme/encoding information without silent normalization.

### ZIP

Do not enable extraction until the actual native source binding, destination containment, no-clobber publication, and worker retirement are established. Lexical validation or create-new alone is not enough. Source/destination races and reparse paths need real protection. Use owned temporary fixtures and no write-access demand on unrelated ancestors. Stream bounded output; enforce actual bytes/integrity; preserve already completed outputs on partial failure; remove only owned temporary artifacts. Cancellation-requested is not cancellation-complete. Never use archived unsafe candidates or unrelated Table/Hot Folder completion as a substitute for solving the ZIP-owned boundary.

### Performance

Preserve checks, coverage, data completeness, and startup readiness. Do not count warm no-op as edited incremental or test filters as proof that compilation was reduced. Do not move work after first paint and hide first-query/full-readiness cost. Use existing profiles and performance hooks. Limit candidate changes and compare equivalent workload/profile/cache conditions. Report no-accepted-optimization honestly rather than force an architecture change for a claimed speedup.

## 7. Bounded verification and progress

Apply Goal Section H verbatim for verification scope and caps. Resolve actual commands/targets in intake; the Goal's placeholder expressions are not runnable test names. Enumerate selections and refuse empty matches. Prefer target/package selection before test-name filtering; select binary-owned tests when applicable. Do not run all targets/features or the full suite by default.

Important caps: first necessary preparation 30 minutes; later targeted preparation normally 20 minutes with one evidence-backed adjustment to 30; ordinary individual test execution 100 seconds and a selected run 10 minutes after preparation; specifically justified native test 180 seconds; focused native/UI scenario 10 minutes with one supported recovery; initial attempt plus two focused corrections for a blocker. Optimization tracks get at most two candidate changes and up to three comparable samples per condition under the Goal's measurement budget. Final first-party lint is end-only, normally 30 minutes. Use supported timeout tooling or an owned watchdog, not invented CLI flags.

Do not kill arbitrary `cargo`, compiler, or application processes by name. Stop/drain only task-owned process trees. Record actual retirement and timeout outcomes; a later successful exit does not rewrite a deadline failure. Reuse compiled artifacts. Do not run `cargo clean` or repeated release builds to reset inconvenient results.

Build and launch the real application for changed UI behavior when supported, using isolated data and owned fixtures. Capture useful screenshots and behavior evidence without involving the user. A narrowly scoped native test/harness may verify OS/file behavior even when general desktop automation is limited. Never bypass permissions, change security policy, install undisclosed tools, or interfere with arbitrary windows. Missing capability after one recovery is a specific limitation, not a fabricated pass or an invitation to infinite attempts.

During extended execution, provide compact progress updates naming the current category/checkpoint, whether work is compiling or testing, the latest meaningful result, and any blocker. Do not stop for approval between ordinary checkpoints.

## 8. Blockers and partial continuation

Distinguish a requested-feature defect, an unrelated pre-existing issue, an environment limitation, and an optional improvement. Record the exact current source, command/observation, attempted remedies, invariant that prevents an unsafe shortcut, and smallest remaining action.

Correct in-scope problems within the Goal's attempt budget. A narrowly necessary adjacent compiler/test correction is allowed when it directly blocks the requested feature; unrelated repair campaigns are not. Never delete legitimate tests, weaken assertions, ignore failures, loosen ZIP protections, or change project security to claim completion.

Use statuses `pending`, `in_progress`, `implemented_pending_verification`, `complete`, `blocked`, and `assessed_no_accepted_optimization`. Do not equate implementation, static review, ordinary tests, native checks, and independent review.

A dependent checkpoint waits for its required boundary. Independent categories may continue after an explicit committed partial/blocked closure when safe source state and the actual tip are recorded; the next branch still starts from that exact tip. Unsafe task-owned experiments must not remain active, but do not revert user or previously accepted work. No dirty partial changes cross a boundary silently. Any unfulfilled mandatory requirement keeps the overall report partial, regardless of whether the queue reached its last row.

Do not automatically create follow-on plan packages or reactivate old programs. State a bounded blocker rather than asking the user to run intermediate verification.

## 9. Final review and single cumulative delivery

After the last implementation checkpoint, perform C07/M8-A. Re-run only affected checks invalidated by later changes; reuse unchanged evidence with clear labels. Build the current application where required, run end-only first-party Clippy with relevant targets and no unrelated vendor cleanup, and obtain configured independent cumulative review where available. Address substantive introduced findings with small forward commits and narrow reruns.

Review the actual entire task diff for scope, compatibility, privacy, and dormant unsafe writers. Confirm user help, per-tool sizing inventory, ZIP limits/partial reporting, and raw/effective Diff equality are accurate. Verify every category's baseline parent and the cumulative ancestry. Keep one final human review checklist in the ledger; do not ask for a new mid-program acceptance session.

The final report must contain:

- Per-category/checkpoint outcomes and clear descriptions of delivered behavior, unverified areas, and blockers.
- Actual relevant tests/build/lint/review results and UI/native evidence locations; no invented aggregate of historical receipts.
- Comparable incremental/selected-test/fixture/startup measurements, with sample counts/profile/cache/workload and honest no-improvement outcomes.
- Principal changed owners and justified dependencies/compile boundaries.
- Each category's branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, significant checkpoint commits, `final_tip_sha`, pushed/local status, and working-tree disposition.
- The actual last cumulative `final_tip_sha`, captured after the final commit, and the explicit instruction that **the following Goal must branch directly from this tip, not from `master`/`main`**.
- Confirmation that no Git worktrees, auxiliary checkouts/clones, independent side branches, or intermediate integration merges/rebases/cherry-picks were used.
- Remaining limitations and one end-of-work manual review checklist.

Prepare one cumulative PR summary. Open only one final PR if the established workflow permits it; never automatically merge or approve it. If the integration branch has advanced, report that fact and preserve this chain rather than silently rebasing or merging each category.

Stop after the approved work and its bounded verification/reporting. Do not start additional features, a new optimization initiative, or another Goal. Begin now with the read-only inspection and mandatory **C01/M0-A** bootstrap.
