Codex Kickstart — Resume and Complete the Existing 17-Goal Multi Launcher Queue
Suggested filename: CODEX_KICKSTART_RESUME_MULTI_GOAL_QUEUE_20261025.md
Repository: G:\Repos\rust\Multi_Launcher (multiplex55/Multi_Launcher)
Queue ID: multi-launcher-queue-20261025-eedc19d2
Purpose: Recover Goal 04 from its documented stopping point, then autonomously continue the original sequential queue through Goal 17.
Authority: One-time, narrowly scoped user authorization to renew the exhausted Goal 04 correction allowances described below. Not a new queue, new Goal 04, blanket test waiver, or authorization to rewrite history.

1. Execute; do not create another plan
You are the parent Codex orchestrator for the existing queue. Resume its verified state and execute the remaining approved work. Do not merely summarize, draft another kickstart, ask for routine permission, or terminate after fixing one script. Use the existing configured Planner, Implementer, and Reviewer runners and the current repository instructions.

Read and obey, in this order of specificity:

Platform/tool permissions and the user's explicit instructions in this recovery kickstart, limited to its stated recovery authorization.
The original master at docs/plans/CODEX_KICKSTART_MULTI_GOAL_QUEUE.md for queue identity, delegation, ancestry, handoffs, ledger, safety, resumption, and all post-Goal-04 execution.
docs/plans/CODEX_GOAL_04_LINK_EDITOR.md and docs/plans/CODEX_KICKSTART_04_LINK_EDITOR.md for Goal 04's required behaviors, measurements, checkpoint order, and verification.
Each subsequent Goal/Kickstart pair and the applicable AGENTS.md, .codex/config.toml, .codex/agents/{planner,implementer,reviewer}.toml when its Goal is entered.
The actual durable queue ledger, blocked-resume notes, raw receipts, and current on-disk Git/process state for historical facts.
Narrow precedence override: the prior exhaustion of the two-repair limit for the expressly identified Goal 04 failures is renewed by Section 4 below. All other acceptance criteria, operational ceilings, strict sequential handoff rules, process ownership requirements, and original plan constraints remain in force. Existing recorded failed runs stay failed; no historical result is rewritten or relabeled.

2. Verified historical anchor — confirm, do not blindly assume
The stopping report recorded:

Field	Recorded value
Repository	G:\Repos\rust\Multi_Launcher
Queue	multi-launcher-queue-20261025-eedc19d2
Completed and frozen	Goals 01, 02, 03, with all accepted checkpoints pushed
FINAL_01	dd79543aa58e97036014d4c78ecc990ce8f62900
FINAL_02	2b1e6a07d091ecdfdf5f85f660f57b53f5f2ab77
FINAL_03 / BASE_04	6a3d612fcceafe1423341f835cf1f5f2fa317ff5
Active Goal 04 branch	codex/windows-link-editor-queue-20261025
Goal 04 M0-A	48946184cb395d8bc501a251dc4ea77b4facf603
Last accepted checkpoint / recorded local and upstream HEAD	M1-A — 55d81350718adea3a8a4ca72f56e7695e836b7b9
Goal 04 remaining	M1-B blocked; M2-A blocked; M2-B dependent
FINAL_04	Does not exist in the stopping report
Goals 05–17	Not started in the stopping report
These are last-known recorded values, not a command to reset to those hashes. If the branch or ledger has genuinely advanced, reconcile and adopt verified newer queue-owned work; do not overwrite it. The original master already authorizes safe interrupted-queue resumption.

Do not recreate or rename Goal 04's branch; do not repeat M0-A or accepted M1-A; do not reopen Goals 01–03; do not start Goal 05 before a verified FINAL_04 exists. Never merge into master, rebase, reset, force-push, clean, stash, cherry-pick, or discard pre-existing user work to make recovery easier.

3. Mandatory read-only recovery preflight
Before writing code, starting any build, or launching a child process:

Discover the real Git root, execution worktree, current branch, local HEAD, index/working-tree status, remotes, remote branch HEADs, cumulative commit ancestry, and any other writer using the worktree. Verify the repository identity and intended origin. Fetch normally if safe and permitted; never force or hard-pull.
Locate the external durable QUEUE_STATE.md using the previous ledger/evidence location. It is outside the source worktrees; do not invent a path or initialize a replacement ledger when an existing one can be found. Read its exact current Goal/phase, last verified anchor, pending command, process leases, repair history, and evidence paths.
Read the existing Goal 04 branch record docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/goal-04.md, both Goal 04 source plans, the master queue plan, and the full local blocked notes g04-m1b/BLOCKED_RESUME.md and g04-m2a/BLOCKED_RESUME.md from the evidence root. Also read g04-m2a/parent-partial-review.md, M1-B r06 app-preview receipt, M2-A probe-inputs-3, and the final partial snapshot/diff. Use the ledger to resolve their actual full paths. QUEUE_STOP_REPORT.md, if present locally, is a useful overview, but missing a local copy is not itself a blocker when the authoritative ledger and raw records are available.
Verify and preserve the seven intentionally uncommitted/unstaged task files recorded at stop:
dev/README.md
dev/track_b/cargo.ps1
dev/track_b/linker_mode.ps1
dev/workflows/test_run.ps1
dev/editor/rust_analyzer_profile.lua
dev/editor/test_rust_analyzer_profile.lua
docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/goal-04.md
Compare the present file identities/diffs against final-partial-state-20261026/task-files.json and tracked-task.diff. Preserve unrelated, new, or concurrent user edits; do not revert to the snapshot blindly. Inspect git diff --check and confirm the originally accepted M1-A commit remains in ancestry.
Reconcile any recorded in-flight child processes by PID plus creation identity, ownership lease, deadline, and actual receipt. Do not kill by process name, launch duplicate Cargo/editor sessions, or presume an old process is still active. The stopping report said all known owned processes were retired, but verify present state.
If identity, ledger, or worktree state conflicts materially with the recorded queue, stop before mutation and report the exact conflict, safe state, and required decision. Do not manufacture a clean checkout.
Write a compact resume-intent entry to the existing external ledger: old anchor, observed actual state, preserved diffs, renewed repair scope, process reconciliation, next packet, and time. Do not modify frozen prior-Goal records.

4. Explicit, finite Goal 04 repair-budget renewal
This kickstart authorizes exactly two additional focused correction rounds for each of these previously exhausted acceptance boundaries:

G04/M1-B: app-preview/argument construction and its directly recorded case-validation/config-scan review findings.
G04/M2-A: Lua adapter self-check/default-admission behavior and its directly recorded workspace-fixture/reviewer findings.
A correction round is one targeted patch set addressing concrete findings, followed by the minimum bounded verification needed to evaluate it. A failure remains recorded even when later corrected. Combine related recorded findings in a single round where possible. The historic r05/r06 and earlier Lua failures do not consume this new allowance, but they must not be deleted, hidden, or relabeled. Do not restart the allowance upon changing the test name or symptom, and do not expand it into an open-ended retry campaign. For genuinely new, distinct defects, use only the ordinary repair policy in the applicable Goal/master.

This permission does not authorize: skipping a mandatory test, weakening meaningful assertions, marking a timeout/empty test as passing, relaxing the required measured-performance outcomes, silently skipping Goal 04, replacing the configured agent roles, making unsupported linker assumptions, or editing global Neovim/user environment settings. If the renewed corrections are exhausted without mandatory acceptance, finish any unaffected authorized work, preserve the exact state, report Blocked, and stop the strict cumulative queue. Do not silently bootstrap Goal 05.

4A. Recover G04/M1-B — Windows linker mode
Read the exact M1-B packet, failed app-preview r05/r06 inputs/receipts, preserved positive workflow contract r03 and unchanged Nextest forwarding r04, and the parent's partial review before patching.

The stopping report identifies these defects:

r05 treated optional ExternalPath as a property that always exists.
r06 treated support.Cargo as an object with .Sha256 even though it was a path string.
Earlier unsupported Cargo pre-build config placement was rejected; the helper was changed to place supported config pairs after build. Preserve proof of valid, actually resolved argument order rather than reinstating the rejected form.
Parent review also raised case-validation and overly conservative config-scan concerns, which were not accepted or corrected at stop.
Have the Planner prepare one bounded, source-specific correction packet for the Implementer covering missing optional fields, shape/type handling, argv and supported Cargo config placement, and the exact review findings. Make the smallest production/helper/test-fixture changes; preserve verified passing behavior. Independently review the resulting diff.

Once the preflight and verification adapter actually pass, perform the original M1-B acceptance work, which had not happened at stop: at most two supported linker candidates total, the same app and selected test-binary targets, supported opt-in mode, real executable/resources/icon and appropriately declared debug/PDB/backtrace evidence, a bounded native launch/close, selected tests, and a genuinely paired warmed edit/relink comparison. Do not claim an isolated link-phase measurement from whole-command timing. The original Goal's compilation, native-smoke, test, and process-cleanup deadlines remain binding.

Preserve canonical release and ordinary full-debug behavior, all global user config, target features, source bytes, and manifest/lock safety. If no safe candidate gives the required measured benefit, record the actual no-gain evidence and leave that mandatory criterion unachieved; do not label M1-B complete. On full acceptance, update the tracked execution record, obtain required review, make the existing [M1-B] checkpoint commit on the same Goal 04 branch, attempt a normal push, and record its SHA/outcome in the external ledger.

4B. Recover G04/M2-A — project-scoped Neovim/rust-analyzer mode
Read g04-m2a/BLOCKED_RESUME.md, the exact executed probe-inputs-3, original two Lua self-check failures, source review, and all preserved editor observations.

The stopping report identifies:

First corrected Lua self-check: duplicate named buffer.
Final corrected Lua self-check: two default-admission assertions failing. Scratch/nofile buffer handling is a hypothesis, not a verified root cause.
Parent source concerns: nested core workspace member exclusion and an unrelated hardcoded real-project fixture.
No verified actual effective-client configuration, compiler-error-and-clearance session, semantic navigation/isolation/revert observation, or Rust diagnostic fixture had yet run.
Have the Planner define the exact expected workspace admission semantics and the difference between actual file buffers and scratch/nofile fixtures; let the Implementer correct the adapter/test setup without turning meaningful failures into success by weakening assertions. Keep the configuration project-scoped, reversible, opt-in where required, trust-safe, and non-destructive to the user's existing Neovim setup. Address the recorded nested-member and fixture issues within this packet. Independently inspect both behavior and tests.

Then perform the original live/editor M2-A verification with an owned, actual UI-attached Neovim/rust-analyzer session, checking effective server/client settings, relevant compiler-error appearance and clearance, semantic navigation, selected workspace membership, isolation, and revert behavior. A Lua unit check, version probe, cargo check, or headless startup alone is not sufficient. Maintain original editor scenario deadlines, truthful failed receipts, controlled fixture restoration, and PID/creation-identity cleanup. Restrict to the single justified project-scoped editor profile and the originally permitted contention/cache comparison, not a broad editor tuning experiment.

On full acceptance, update the tracked execution record, obtain the required review, commit [M2-A] on Goal 04's existing branch, attempt a normal push, and log the exact result. A parent-only fallback review must be labeled as such, never falsely described as independent review; follow the original master for handling actual configured-Reviewer unavailability.

4C. Finish G04/M2-B and Goal 04 closure
After M1-B and M2-A each satisfy their real acceptance gates, execute the original M2-B comparative editor/linker responsiveness and final guidance. Match the scenarios and baseline provenance, respect repeat and time budgets, record actual improvements or limitations, remove rejected alternatives, and keep release/full-debug escape hatches.

Complete the independent/scoped reviews and any finite corrections first. Perform appropriate final first-party Clippy last (excluding unrelated vendored warning cleanup) after all tracked source/docs changes and scoped/native verification; capture the exact verified tree and leave its source inputs unchanged afterward. Finish checkpoint commits and best-effort normal pushes using the original checkpoint cadence and exact-tree/handoff checks. Do not manufacture a success checkpoint or FINAL_04 if mandatory performance/editor evidence remains missing.

Only when the Goal 04 handoff gate has actually passed: record immutable FINAL_04, freeze Goal 04's accepted branch at that commit, record its push result and inherited capabilities, and advance the external ledger to Goal 05. Administrative SHA/push/closure facts may live in the external ledger rather than requiring recursive tracked commits.

5. Continue the original queue, Goals 05–17, autonomously
Immediately after a verified Goal 04 handoff, continue the same queue through all remaining original pairs in their approved sequence:

Goal	Original subject
05	Plugin Settings scrolling with long descriptions
06	Application activity, cancellation, and resource stability
07	Launcher readiness and responsiveness
08	Idle work and live-pin performance
09	Large CSV handling
10	Table operations and exchange
11	Jump Mode configuration
12	Screenshot redaction
13	Folder-size explorer
14	ZIP explorer
15	Macro summaries and starter recipes
16	Hot Folder conditions
17	File-drop/tool routing
The master's exact 17-pair manifest and each Goal/Kickstart are authoritative; the table is only a navigation summary. For every newly entered Goal, re-read its full pair, check required capabilities and the prior verified final SHA, initialize one fresh branch and M0-A baseline at that immediate predecessor's FINAL_NN, push the baseline, then carry out its original checkpoints in order. Complete scoped tests/native evidence, proper review, final Clippy, immutable-tree proof, checkpoint commits and normal pushes, final handoff and ledger updates. Check Goal 10's Goal 09 dependency and Goal 17's Goals 09, 10, and 12 inherited capabilities explicitly.

Never bootstrap from a moving master or from a predecessor's partial commit. Never silently skip a blocked Goal or fork an independent bypass line. Do not repeat completed Goals 01–03 or reconsider accepted Goal 04/M1-A measurements without concrete invalidation. Reuse applicable prior receipts, but never substitute stale evidence for testing changed inputs.

At most one Implementer writes overlapping source at a time. Use the configured Planner and Reviewer roles as available and permitted; preserve the required distinction between independent review and an explicitly authorized parent fallback. No routine user permission, manual intermediate acceptance, unrelated restructuring, workspace feature, or full Nextest suite is added by this recovery instruction.

6. Ownership, progress, and robust session resumption
Keep the existing external QUEUE_STATE.md as the single durable source of operational truth, with each current phase, branch, HEAD, process owner/lease, correction-round budget, verification result, first failure, next action, accepted checkpoint and push. Preserve raw receipts and executed-input hashes. Before a lengthy command or possible session/context boundary, record intent and an exact recovery point; after it, record actual outcome. Emit concise, useful checkpoint progress (not merely an unverified “running” assertion), including the active goal/checkpoint, new commits, passed/failed gates, elapsed test/build stages when the owned runner reports them, and any blocker.

Use finite owned-process deadlines: for Goal 04, 30 minutes per compile/link command, 5 minutes per editor scenario, and 2 minutes per post-build native smoke, plus applicable separately bounded selected-test/cleanup limits from the original contracts. For later Goals, use each Goal's own budgets. Do not indefinitely block on a synchronous RPC, cargo lock, process-output reader, or subagent; enforce the existing owner cleanup and exact output-drain protocols. Do not launch duplicate jobs merely because progress output is quiet.

If interrupted, resume using the master's Section 12 reconciliation; do not start a second queue, baseline, implementer, test campaign, or branch. A Codex session boundary is not proof work continues in the background. If execution must pause, record the exact runnable next action and explicitly report Paused/resumable; do not claim autonomous background activity that is not actually running.

7. Stop conditions and final deliverable
Continue without intermediate user prompts until either:

All 17 Goals are demonstrably completed, accepted, committed, appropriately reviewed, pushed where permitted, and cumulatively inherited into the final branch; or
A genuine unresolved mandatory blocker, permission/capability problem, conflicted concurrent worktree, or exhausted finite repair boundary prevents a safe next step.
A declined push alone is not necessarily a Goal blocker under the master; record the local verified state separately. If a stopping condition occurs, do not falsely declare completion or skip ahead. Preserve the partial code and recorded process safety, explain the smallest necessary intervention, and leave the queue resumable.

On completion or a genuine stop, produce one consolidated factual report using the original master's Section 14 format: goals 01–17, branches, base/baseline/checkpoint/final SHAs, actual push outcomes, verification and Clippy/native evidence, observed performance outcomes, remaining blockers or limitations, final cumulative tip, external ledger/evidence paths, and exact next action if partial. Perform no merge into master, release, rebase, force-push, or repository cleanup beyond the original authorized scope.

Start now: Perform Section 3's read-only reconciliation, open both Goal 04 BLOCKED_RESUME.md files, record the renewed finite recovery budget, dispatch the scoped G04/M1-B correction packet, then execute through the remaining checkpoints and Goals without routine confirmation.