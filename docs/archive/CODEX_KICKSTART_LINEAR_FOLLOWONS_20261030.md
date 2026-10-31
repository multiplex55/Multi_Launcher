# CODEX KICKSTART — Execute the Linear Multi Launcher Follow-on Program

**This is the execution entry point for `CODEX_GOAL_LINEAR_FOLLOWONS_20261030.md`. READ THE ENTIRE GOAL AND THIS ENTIRE KICKSTART BEFORE MAKING ANY CHANGES.** The two files form one bounded, authorized, strictly sequential program. The Goal's detailed safety/acceptance clauses prevail over this compact command script; the existing merged workstream Goals/Kickstarts remain functional contracts. Do not start another planning-only research exercise or independently fork all goals from master.

**Repository:** `multiplex55/Multi_Launcher`  
**Recommended checked-in plan location:** `docs/plans/linear-followons-20261030/` for both Markdown files; the separate execution root below is for runtime records.  
**Run ID:** `multi-launcher-linear-followons-20261030-r01`  
**Execution dir on the branch chain:** `docs/plans/execution/multi-launcher-linear-followons-20261030-r01/`  
**Expected default runtime models:** parent/orchestrator Sol medium, implementer Luna max. Read `.codex` files and accept configured planner/reviewer roles; **DO NOT EDIT MODEL CONFIG**.  
**Branch topology:** B1 from verified current `origin/master` **once**, B2 from B1's final accepted/docs-only commit, B3 from B2's final commit, etc., through B9. Each branch is a category label for cumulative history, not an independent feature fork. Normal commits/pushes at every accepted checkpoint. **One PR from B9 to master, NO automatic merge.**

## 0. Dispatch contract and precise start conditions

1. The user has already selected the workstreams listed in this Goal: G02, G05, conditional G03, G06, G08 history, G04, G07, and conditional G08 recipe decision; one independent final code/document review and PR. Do not ask for ordinary mid-run approval or manual tests. New external permissions, an unsafe native workaround, new storage/security model or an expanded feature boundary remain outside this authority and should be recorded as a deferred gate rather than silently assumed.
2. Inspect repository identity, applicable `AGENTS.md`, nested instructions, Git `status`, `HEAD`, `origin/master`, worktrees, ongoing merge/rebase/cherry-pick markers, and `.codex/config.toml`/role files. Record exact requested vs configured vs known effective models. Historical authored baseline `22c20ff87c65b0d90461da1b0a5a50c0e88176e5` and all old donor SHAs are **for evidence only**.
3. Read the merged `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/{PROGRAM_STATUS,INTEGRATION_MANIFEST,DEPENDENCY_MAP,FOLLOW_ON_CATALOG,FINAL_HANDOFF}.md` and every stage's binding existing pair before its source stage. Read G02/05/G03 originals first, then remaining pairs when reached. Prefer fresh/current source over guessed historical paths; source drift is not permission to widen the scope.
4. If the current checkout has unrelated edits or Git operations: do not stash, overwrite or reset them. Use an isolated clean worktree rooted at the fetched `origin/master` **only for the active chain** (not an independent per-goal fork), or leave a truthful `PAUSED_RESUMABLE` record if safe ownership cannot be established.
5. If a prior run already created this program ID, confirm its chain and resume the **first unfinished** stage. Never recreate B1, duplicate a baseline, or push a replacement branch over existing work. On name collision with an unrelated program, choose a recorded suffix (`-r02`) while keeping exact predecessor-tip ancestry.
6. At least one orchestrator and one designated Luna/max writer; the planner/reviewer remain source-restricted and read-only as configured. Only one child writes source and one owned build/native fixture operates at a time. The orchestrator, not a child, owns Git, status and final source acceptance.

## 1. Bootstrap the single straight branch chain (PowerShell-friendly example)

The following illustrates the **required ancestry**. Adapt only to actual supported PowerShell/Git and current clean worktree; check every nonzero exit. It is not a licence to overwrite a dirty repository or run commands from a home directory without `Cargo.toml`.

```powershell
# Run from the VERIFIED Multi_Launcher repository root, with a clean owned worktree.
git remote -v
git status --porcelain=v1
git branch --show-current
git rev-parse --show-toplevel
git fetch origin master
if ($LASTEXITCODE -ne 0) { throw 'fetch failed; preserve current state' }
$runStart = (git rev-parse origin/master).Trim()
if ($LASTEXITCODE -ne 0) { throw 'cannot identify origin/master' }
$branch = 'codex/linear-01-g02-table-publication-20261030'
git switch -c $branch $runStart
if ($LASTEXITCODE -ne 0) { throw 'branch collision or unsafe checkout; investigate, do not reset' }
# Record $runStart, branch, worktree, current Git status and upstream in SOURCE_BASELINE.md.
```

Initialize `PROGRAM_STATUS.md`, `BRANCH_CHAIN.md`, `SOURCE_BASELINE.md`, the stage folders/role provenance and a first, **documentation-only** baseline commit. Check staged files before commit; then `git push -u origin $branch`. Use scoped `git add <exact paths>` rather than `git add .`. If push fails, retain local checkpoint and record the failure; continue safe local work, do not force push.

**At every later stage boundary (this is mandatory):**

```powershell
# The preceding stage has already ended with a coherent commit and a push attempt.
$priorBranch = (git branch --show-current).Trim()
$priorTip = (git rev-parse HEAD).Trim()
# Record $priorBranch + $priorTip in BRANCH_CHAIN.md BEFORE switching.
# Verify task-owned worktree is clean; unresolved source is not allowed into the chain.
$nextBranch = 'codex/linear-02-g05-hotfolder-20261030'  # change ONLY to the next ordinal branch
git switch -c $nextBranch $priorTip
if ($LASTEXITCODE -ne 0) { throw 'stop; do not rebranch from master' }
$nextHead = (git rev-parse HEAD).Trim()
if ($nextHead -ne $priorTip) { throw 'chain broken: new branch is not based on previous tip' }
git merge-base --is-ancestor $priorTip HEAD
if ($LASTEXITCODE -ne 0) { throw 'missing predecessor ancestry' }
# Append branch-from-SHA receipt and make/push its documentation M0 commit.
```

The **full ordered branch names**, from which the script must take `$nextBranch` each time, are:

```text
B1 codex/linear-01-g02-table-publication-20261030
B2 codex/linear-02-g05-hotfolder-20261030
B3 codex/linear-03-g03-table-full-file-20261030
B4 codex/linear-04-g06-jump-native-20261030
B5 codex/linear-05-g08-mkmacro-history-20261030
B6 codex/linear-06-g04-screenshot-native-20261030
B7 codex/linear-07-g07-zip-extraction-20261030
B8 codex/linear-08-g08-recipes-policy-20261030
B9 codex/linear-09-final-review-20261030
```

Never do `git switch master; git pull; git switch -c ...` **between** stages. Never create nine branches from the same master SHA. Never merge B1 into B2, cherry-pick B1 into B2, create eight PRs or later select a new integration branch fork. Each new branch is *created at the previous final tip*; prior branches simply retain named pointers to earlier points on the very same line. B9's cumulative diff contains all and only accepted changes plus report documents.

## 2. Fixed stage execution matrix and priority

| Ordinal / branch | Required existing pair on current `master` | Gate / expected checkpoint | Explicit failure policy |
|---|---|---|---|
| **B1 G02** | `goals/G02/follow-on/native-parent-admission-proof/{GOAL,KICKSTART}.md`; original G02; safety contract | Diagnose OS5; 19 selected cases; prove native M1-A, then M1-B cancellation/recovery | No successful native admission + lifecycle = G02 not `ACCEPTED_COMPLETE`, G03 remains blocked |
| **B2 G05** | `goals/G05/follow-on/g05-native-read-retirement/{GOAL,KICKSTART}.md`; original G05 | Real pending regular-file read retirement → held source → queued start → editor/Test Rule/activation | No genuine native proof = leave conditions fail-closed and defer downstream work |
| **B3 G03** | Original `G03-table-full-file-operations/{GOAL,KICKSTART}.md` | Entire G02 accepted, then full-file streaming + unambiguous UI | Missing any G02 gate = docs-only `DEFERRED_DEPENDENCY`, no code |
| **B4 G06** | `goals/G06/follow-on/native-jump-focus-proof/{GOAL,KICKSTART}.md` | Owned exact app/window, real Save/Jump/focus observations | One supported recovery; no window = defer with cleanup, preserve accepted editor code |
| **B5 G08 history** | `goals/G08/follow-on/g08-history-verification-restart/{GOAL,KICKSTART}.md`; R1 and original G08 | Complete real toolbar privacy + History tests (anticipated 15), R1 review | Timeout/failed tests = source not accepted, docs-only closure |
| **B6 G04** | `goals/G04/follow-on/native-gesture-output-proof/{GOAL,KICKSTART}.md` | Physical right-button drag, actual image Save/Copy pixel oracle | Missing supported gesture/clipboard/binary = preflight defer; no fake native success |
| **B7 G07** | `goals/G07/follow-on/native-guard-publication-compatibility/{GOAL,KICKSTART}.md`; original G07 | Exact native alias/delete + guarded no-replace; then Stored writer and UI | No true native proof = all prototype/writer/UI source excluded; catalog remains |
| **B8 recipes** | `goals/G08/follow-on/explicit-invocation-recipe-policy/{GOAL,KICKSTART}.md` | G08 History accepted; decide R0; at most one explicit safe R1 if justified | History/policy missing = docs-only deferred/no-change; never replay from History |
| **B9 final** | This Goal and `BRANCH_CHAIN`, source manifest, all stage receipts | Cumulative independent review; one PR B9→master | Do not merge a deficient branch or claim empty CI means pass |

The `goals/...` paths in this table are **relative to** `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/`. The G03 original and G02/G05/G07/G08 original pairs are under `docs/plans/reliability-feature-closeout-20261029/goals/`.

## 3. Per-stage handoff and checkpoint discipline

**Common M0 handoff (every stage):** parent identifies exact previous-branch closing SHA, creates the next branch at that SHA, reads the authoritative existing Goal/Kickstart + actual current source, compares accepted current-source blobs to historical donor-only evidence, writes scoped `SOURCE_REVALIDATION.md`, lists acceptance/negative tests and records original failures. Child must not write application code before this source-backed packet. Commit+push this coherent M0 evidence with subject `docs(linear): [B0x/M0-A] establish <stage> source and gates`.

**Common M1 handoff:** parent gives Luna/max implementer exact current owners/symbols, chosen bounded algorithm/contract, fields/modes, preserved compatibility, test filter and negative cases, source scope, accepted predecessor tip and *one* verification budget. Author applies one narrow owner change, maintains pass/fail and fixture cleanup receipts, never alters test expectations to pass. Reviewer checks exact diff/first failure/current source. On success commit+push accepted milestone with subject like `fix(table): [B01/M1-A] prove guarded publication` or `feat(hotfolder): [B02/N1] retain conditioned source through effect`; no failure-labeled accepted commits. If a source checkpoint is not accepted, keep the main chain source-clean (see §5) and use a docs-only report.

**Common M2 handoff:** parent independently reviews each accepted code stage and its native/deterministic limits, runs only justified *affected* verification after reviewer correction, inspects combined staged files/manifest and makes one normal push. Child role must stop writing until parent accepts/rejects. One substantive reviewer correction cycle maximum. A recorded read-only/static review is not proof of actual native effects.

**Common closing M3:** parent updates per-stage `PROGRESS.md`/`ACCEPTANCE_MATRIX.md`, program `PROGRAM_STATUS.md`, accepted/excluded source list, branch-tip/push receipt, no-work/deferral and next stage. Publish stage closing commit `docs(linear): [B0x/M3-A] close <stage> with exact proof and limits`. If completely blocked before implementation, skip empty M1/M2 commits and make truthful M0+M3 documentation checkpoints. Only **after** that does parent create the next branch from current HEAD.

### B1 / G02 — concrete implementation instructions

1. Inventory the **19 previously selected** `table_publication_*` / `resource::resource_tests` cases from the merged G02 `selected-inventory.json`; separate native vs deterministic failures. Retain the original four named failures, not any custom test that merely catches an early error.
2. At the existing `NativeTablePublication::reserve::hold_parent` → `NativeRenameScanBackend::admit_root` path, preserve `BackendFailure.path` and identify the actual failing ancestor/access flags; record root/volume and task-owned TempDir. The first observed historical failure was `CreateFileW: Access is denied (os error 5)` **before** stage allocation. No guessed ACL cause, `runas`, sharing loosening, path-only fallback or native identity bypass.
3. Implement a minimal *existing owner* change only if proven. The positive cases must reach actual native `CREATE_NEW`, stream/flush/sync, handle-bound **no-replace** publication and exact source/destination identity. Negative cases: occupied/appeared output, wrong/same source, reparse parent/stage substitution, partial/cancel and cleanup. Keep quota charges and native handles until genuine retirement.
4. Under the new G02 M1-A budget in Goal §6, perform one locked/offline fast-dev list and exact matching run; there must be an actual successful native publication, not `no error` or mocked reservation. If successful and reviewed, commit M1-A. Then design/execute the *original* G02 M1-B cancellation, recovery and ownership proof with its **separate** documented bounded packet. G02 is not complete until both checkpoints and original final reviewer sign off.
5. If host restriction or incomplete ownership persists: preserve exact first failure and source patch off-chain, report `NATIVE_PROOF_DEFERRED`; do not expose Table full-file operation or start G03 source work. Move directly to B2.

### B2 / G05 — concrete implementation instructions

1. Confirm accepted M1-A/B1 in `master`: Hot Folder metadata/binding and fixed literal scan; tests previously **19/19 and 22/22**. Preserve `FILE_CONDITIONS_NOT_EVALUATED` production refusal.
2. Native N0: on task-owned admitted regular file prove an **already pending** actual native read and its later cancellation, terminal result and joined/retired buffer/worker/handle. A pipe-only fixture, cached completion, pre-cancel or `AtomicBool`-between-chunks test does not satisfy. Name the existing IO owner and native API path; do not create generic framework or driver.
3. If N0 accepted, N1 starts a single candidate deadline **before** read/acquisition, streams <=8MiB in <=64KiB chunks <=2s, fingerprints and scans B1 literals without separate path reopen. Validate root/parent/file, true EOF, mtime/length and rule revision, retain owned native source until effect consumes or safely refuses it.
4. N2 tests actual start-time source, rule revision/epoch/control and queued launch/MkMacro binding. Changed evidence => zero effects, one correlated terminal/refusal. Preserve existing unconditioned rules, dedupe/cooldowns, action dispatch and cancellation.
5. N3 tests size/age/literal editor controls, rejected values, side-effect-free Test Rule, stale/missing/denied/timeout conditions, an ordinary positive match, full lifecycle and final reviewer. **Only after all proofs** may the original production condition-refusal gate be replaced with validated behavior.
6. If N0 fails or a downstream proof fails, keep all unsafe effects refused, save exact docs/evidence, and do not force a UI activation just to claim feature completion. Advance B3 regardless.

### B3 / G03 — explicit dependency decision

At entry, check B1's exact `ACCEPTANCE_MATRIX` and accepted source hashes for **the entire** Table publication contract, including M1-B cancel/cleanup/recovery. If `G02 != ACCEPTED_COMPLETE`, write `G03/DEFERRED_DEPENDENCY.md`, record missing specific proof, commit+push the docs-only category branch, advance B4. No `src/`/`crates/` changes.

If fully accepted, read the original G03 contract and implement exactly one bounded full-file stream using the G02 publisher. Make page vs full-file scope explicit in UI, preserve current compact export, no whole-file Vec, no unchecked overwrite, no stale accepted result. Tests for selection provenance, source changes midstream, output occupancy, cancellation, partial cleanup, no memory blowup, correct progress, optional bounded large-file proof (only if fixture space/time explicitly safe) and independent review. Only accepted worker may expose UI and completion. Stop scope at actual supported formats, not inferred features.

### B4 / G06 — native test procedure

No Rust feature rewrite by default. Bind exact current build/source SHA/profile, disable fixture Paste next/cancel before app startup, and obtain one uniquely identified **task-owned window** using the supported native tool. Previous owned app attempt could not be discovered and returned desktop-check error `0x80070005`; one documented supported recovery maximum, no guessed HWND or raw input workaround.

If observed window exists, execute: invalid editor input and inline error; Save refusal (no persisted/live change); valid custom chord/hints Save with acknowledgement; reopen and verify canonical values; actual changed chord; default remains for fresh profile; Jump full hint **focus only**, fresh Enter activates; Esc cancels; launcher hotkey independently focuses query; Settings/Notes and owned foreign foreground preserve ownership and held-key release fence. Capture actual windows before/after and exact input/outcome. If no supported window, `NATIVE_PROOF_DEFERRED` without edits, then B5. G06 build <=600s, native <=180s, cleanup <=60s.

### B5 / G08 — MkMacro history with correct privacy proof

The only qualifying accepted baseline is `master`; G08 history/correction donor source is UNACCEPTED. Read original history and reviewer R1 contracts, inspect full toolbar composition, `SummaryHistory` bounds and existing `runtime` lifecycle. Preserve sanitized metadata-only history (100 records/4MiB/1024 rows/run); no executable arguments, disk store, raw diagnostic leak or new run command. The History tab must not expose arbitrary Editor diagnostic text/current draft name, while Editor labels/controls and actual AccessKit state remain intact.

Run one new explicitly authorized bounded selected list and run; derive the **8 G08 plus 7 existing controls (expected15)** from actual names, including `g08_history_r1_real_toolbar_composition_preserves_editor_diagnostics_and_controls`. Do not claim the historical 14/14 or late compilation as R1 pass. At most two localized fixes/reruns. Independent R1 reviewer accepts exact stable source and no privacy regression. If proof fails/timeouts, preserve unaccepted patch and docs-only closure. Recipes are **not** automatically authorized by History success; separate B8 decision.

### B6 / G04 — screenshot redaction

Preflight supported **physical held secondary drag**, exact-source app, safe owned fixture/profile, supported actual clipboard-image observation/restoration. If any absent, stop *before* app build; write capability receipt and continue B7. No invented tool schema, right-click substitute, OS helper or user desktop capture.

If available, run the exact **320x240 opaque synthetic PNG** and redaction/crop protocol in G04 follow-on: verify real capture baseline; primary Solid Black `[64,256)x[72,136)`; secondary Marker crop `[16,304)x[16,224)`; Undo/Redo; actual Save and Copy resulting **288x208** image. Pixel-perfect RGBA comparison against independent oracle, not self-computed app output, including crop/redaction/fiducials and clipboard. Record any zoom/scale mapping, no extra side effects and cleanup. A unit/raster pass is not native acceptance.

### B7 / G07 — ZIP native ownership then strictly conditional writer/UI

Use only owned `target/productivity_expansion/archive_native` fixtures. First resolve exact observed native error **32** under held ancestor and the historical ambiguous alias deletion endpoint; record actual source/alias names, object identity, post-delete state, stage handle and sharing. No releasing guards, relaxed sharing, path rename fallback, arbitrary existing files, repeat known unsupported relative `RootDirectory` ERROR87, new filesystem framework or false `no-clobber` claim.

Only once reviewed native R1 passes can the writer stream strict Stored physical entry bytes, validate physical/local records, quotas, CRC and genuine EOF, cancellation and partial cleanup; then complete original reviewed writer acceptance before adding any minimal UI. Preserve existing read-only ZIP catalog otherwise. Historical10/12 is not acceptance. Run one scoped `--features perf-harness` inventory+selected test batch including all existing 6 controls with the new selected tests; exact commands derived from receipt. Use R1 1200/180/600 caps and at most two localized correction runs. No Deflate/encrypted/nested/overwrite feature expansion.

### B8 / G08 recipe policy

If B5 History is not `ACCEPTED_COMPLETE`, document `DEFERRED_DEPENDENCY`. Otherwise R0 reads `src/mkmacro/{templates,invocation,runtime}.rs`, current `package_ui` and selected-macro launch owners. Compare existing template Preview/Copy with one possible named current-macro invocation workflow. Specify identity, subset/mode, current parameters without secrets, explicit user confirmation, disabled/stale refusal and original activation/cancellation owners. If ambiguity, new persistence/permissions or insufficient seam: `NO_CHANGE / POLICY_REQUIRED`, no source. A history summary cannot replay anything. At most one user-authored existing-owner workflow after a concrete reviewed R0; targeted selected tests and new independent review only. Do not create a generic recipe system.

## 4. Controlled compilation and process/fixture evidence

For each source stage, derive `$TestExpr` from the actual selected inventory. The following commands are **shapes**; choose the exact `-p`/`--lib`/`--bin`, `--features` and `-E` expression *after* verifying the current target and the bound existing Goal. Do not copy the placeholder literally.

```powershell
# Example for one scoped library stage with its CURRENT exact filter:
$TestExpr = '<replace with source-verified nonzero Nextest filter>'
cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $TestExpr --message-format json
if ($LASTEXITCODE -ne 0) { throw 'selection/build failed; preserve first diagnostic' }
# Parse/record actual inventory BEFORE executing; require intended non-ignored cases.
cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $TestExpr --retries 0 --no-fail-fast
if ($LASTEXITCODE -ne 0) { throw 'test failed; record receipt and use at most the stage correction allowance' }
```

G02 native table proof uses both `-p multi_launcher -p multi_launcher_table_core` and the **exact mixed app/core inventory** recorded in its source receipt, not a guessed library-only selection. G07 uses the existing `--features perf-harness` (fixture I/O, **not** permission for broad benchmarking). G08 uses `test(g08_history_)` **plus** seven actual historical controls. Verify the `fast-dev` profile and Nextest CLI help before use; if unavailable, record a supported equivalent without editing Cargo TOML. Use bounded owned process supervisors for compile/list/run; save UTC start/end, PID/process tree, caps, exit/timeout status, selected names and raw output hash under the active stage. Print periodic phase/elapsed/selected-count progress. On an overrun terminate **only** known task-owned processes if permitted; if denied, stop unsafe new edits and record actual drainage/status. No unrelated process kills, `cargo clean`, install, benchmark, full-suite test run or clippy campaign by default. For a code-change correction use only directly affected selected tests; do not renew an exhausted cap silently.

**Always separate:** `BUILD_PASSED`, `SCOPED_TESTS_PASSED`, `STATIC_CHECKS_PASSED`, `NATIVE_OBSERVED`, `RUNTIME_UNVERIFIED`, `NOT_RUN`, `TIMEOUT` and `REVIEW_ACCEPTED`. Compiler success, source inspection or a mock fixture never fills a native proof cell.

## 5. Failed prototype isolation — mandatory before creating the next branch

If a candidate fails its hard gate, the branch must finish **without** its unaccepted production source. Keep original donor branch untouched and a task-owned patch/receipt for attempted code. Prefer detached experimental worktree so the active line remains clean. If experimentation occurred as uncommitted edits directly in the main owned worktree, inspect its exact prior clean baseline and **only task-authored paths**, save their `git diff --binary` as an evidence artifact and copy task-created untracked files to task evidence; verify SHA/hashes; then revert **only** those known task-authored uncommitted changes. Never stash/reset/clean the user's work, never silently discard a source candidate, never commit an unaccepted candidate just to create a closing branch tip. Record any temporary fixture/process ownership and cleanup success/refusal.

A docs-only deferred outcome must include the exact failed case/tool/OS code, source/branch SHA, expected vs actual, remaining native/resource/policy gate, tests not run, accepted unrelated work unaffected and next viable owner proof. Stop this workstream at the finite bound, then **continue the next independent branch from the preceding docs-only closing tip**. G03 is not independently executed if G02 is deferred; B3 still gets an honest docs-only checkpoint.

## 6. Review and Git publication after every stage

Before source acceptance:

- Reviewer sees the **exact** proposed source diff, current base and named owner, failure/negative cases, test selection inventory, raw receipts and process/fixture retirement; rejects any missing mandatory proof, test weakening, source owner expansion or disguised unaccepted prototype.
- Parent records scoped accepted blob hashes and verifies `git diff --check`, allowed paths, no unwanted manifests/configuration and `AGENTS` compliance. On a successful accepted milestone, stage only permitted source/tests/docs and commit, then normal push and record actual SHA. On a failed stage, only vetted evidence/doc files are committed.
- Parent writes `STAGE -> DONE/PARTIAL/DEFERRED`, exact accepted source and next handoff into `PROGRAM_STATUS.md`; preserve actual source/prototype distinction in `ACCEPTED_SOURCE_MANIFEST.md`; source isn't accepted merely because its donor has a later closing docs commit.
- Before each `git switch -c` the parent confirms all task-owned compilation/native processes ended/drained and worktree is clean. Branch start equals the **exact previous closing SHA** even after a deferred stage.

For a real stop that cannot safely allow continuation (e.g. stuck owned process during source mutation, unidentifiable user state, inaccessible Git), preserve `PAUSED_RESUMABLE`, all known results, branch/tip, file status and exact safe resume instruction. Do not promise unattended background work or ask for repeated user checks.

## 7. B9 — one consolidated PR and single manual review

1. From B8's **closing commit**, create `codex/linear-09-final-review-20261030` and verify same initial HEAD and ancestor. Reconcile all stage statuses and source commits. Confirm G02/G05/G07 feature gates have not accidentally been enabled without full accepted proof; G03 and recipes blocked when prerequisites are absent; only accepted G08 history, if any, was carried. Preserve research results already merged into `master`—do not duplicate them or import old donor prototypes.
2. Materialize and independently check `ACCEPTED_SOURCE_MANIFEST.md`, `BRANCH_CHAIN.md`, `PROGRAM_STATUS.md`, `DEPENDENCY_MAP.md`, `RISKS_AND_DECISIONS.md`, `FOLLOW_ON_CATALOG.md`, `PR_SUMMARY.md`, `FINAL_HANDOFF.md`, and `MANUAL_REVIEW_CHECKLIST.md`. Validate exact committed Rust files, accepted test receipt source hashes, small branch-checkpoint commitments and relative links. Require separate acceptance for every substantive accepted source stage; otherwise document deferral. No new blanket Nextest/Clippy/native campaign here.
3. Freshly `git fetch origin master`; compare cumulative B9 HEAD to actual `origin/master`. Do NOT silently reset/rebase and break the branch lineage. If changed master introduces a true conflict or required check, record a narrow resolution gate and leave a ready PR if necessary. One normal push of B9 and one PR only, e.g. `gh pr create --base master --head codex/linear-09-final-review-20261030 --title "Multi Launcher: linear Table, Hot Folder and reliability follow-on closeout" --body-file <verified PR_SUMMARY.md>` after verifying `gh` authentication/permissions. If `gh` unavailable, use the current supported GitHub PR integration; don't fabricate a PR URL.
4. **Do not merge automatically.** Query/report actual PR number, head/base/mergeability, required review and check status. An empty workflow/reviews array is UNKNOWN, not PASSED. Give the user **one** final review checklist and list every unshipped functionality and follow-on pair with precise acceptance prerequisites. Do not ask for manual input mid-run.
5. Preserve every category branch as readable history. No separate intermediate PRs, branch deletion, forced merge or hidden source migration.

## 8. Final response required of Codex

Return in this structure, with links to the **actual** created branch/ref/file/PR (never guessed):

```text
Program: multi-launcher-linear-followons-20261030-r01
Run-start master: <actual SHA>
Final branch / tip: <B9 exact SHA> / push <success|failure>
One cumulative PR: <actual URL or NOT_OPENED reason> / <open|merged> / CI <observed|not observed>
Ancestry: B1 <- master, B2 <- B1, ... B9 <- B8; verification PASS/FAIL with branch tip table
G02 Table publication: ACCEPTED_COMPLETE | PARTIAL | NATIVE_PROOF_DEFERRED (actual native case counts)
G05 Hot Folder: ACCEPTED_COMPLETE | PARTIAL | NATIVE_PROOF_DEFERRED (gates still enabled/disabled)
G03 full file: ACCEPTED_COMPLETE | DEFERRED_DEPENDENCY (exact G02 gate)
G06 Jump native: NATIVE_ACCEPTED | PARTIAL | NATIVE_PROOF_DEFERRED (real owned-window proof)
G08 history: ACCEPTED_COMPLETE | DEFERRED (exact corrected privacy tests)
G04 Screenshot: NATIVE_ACCEPTED | NATIVE_PROOF_DEFERRED (actual secondary gesture, pixels)
G07 ZIP: ACCEPTED_COMPLETE | DEFERRED (actual native guard and writer/UI outcomes)
G08 recipes: NO_CHANGE | POLICY_REQUIRED | ACCEPTED (separate policy proof)
Accepted source files: <count, owners, blob references>
Excluded/unaccepted prototype files: <count, owner, preservation receipt>
Tests: <per-stage selection/count/exit, not a fabricated aggregate suite>
Independent review: <scope, accepted or pending>
Native/process/fixture cleanup: <exact status>
Next actions: <highest-value outstanding canonical follow-ons>
Single final human review: <link to checklist>
```

**Stop conditions that do NOT block independent later stages:** native Windows access denied, unavailable supported physical secondary drag, inability to observe owned GUI windows, a failed native no-replace archive proof, an exhausted bounded test budget, unapproved recipe policy or G02's impact on G03. Record each honestly and continue the linear chain with a documentation-only closure. **Never substitute a different feature implementation merely to mark a stage complete.**
