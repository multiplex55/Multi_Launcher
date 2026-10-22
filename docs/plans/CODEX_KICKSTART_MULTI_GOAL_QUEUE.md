# CODEX KICKSTART — Multi Launcher Multi-Goal Execution Queue

**Mission:** Execute all three supplied Codex Goal contracts, sequentially, on **three different freshly created Git branches**, preserving accumulated work, their individual acceptance criteria, and the repository's actual configured sub-agent workflow. This is an **implementation kickoff for Codex**, not permission to rewrite the Goals, merge branches into `master`, or substitute a new feature plan.

**Operating mode:** Autonomous, code-first, bounded verification, Active Checkpoint Commit Cadence, local commits before best-effort pushes, and **one user review after the queue**. Do not stop between Goals to ask whether to continue.

**Read this entire master Kickstart, every Goal, `AGENTS.md`, and the actual Codex runner configuration before starting. Perform the first Goal's Git bootstrap before any feature edits, implementation tests, or build.**

---

## 1. Authoritative inputs, selected order, and why

The archives were provided in this attachment sequence: `goal1.zip`, `goal3.zip`, `goal2.zip`. Their **six Markdown documents** consist of three Goals and three companion Kickstarts. The execution environment must use the **Goal Markdown files directly**; it must not require these archives, an old source ZIP, historical snapshots, or the ChatGPT conversation.

### Exact required Goal documents

1. `CODEX_GOAL_LAUNCHER_QUERY_FOCUS.md` — **Launcher Query Focus, Hidden-to-Visible Autofocus, and Notes/Jump Mode Interoperability**.
2. `CODEX_GOAL_CAPTURE_AND_PRODUCTIVITY_EXPANSION.md` — **Capture Stability & Productivity Expansion**.
3. `CODEX_GOAL_PRODUCTIVITY_TOOLS.md` — **Productivity Tools**.

Read these in full, in addition to this master Kickstart. Their optional companion prompts are `CODEX_KICKSTART_LAUNCHER_QUERY_FOCUS.md`, `CODEX_KICKSTART_CAPTURE_AND_PRODUCTIVITY_EXPANSION.md`, and `CODEX_KICKSTART_PRODUCTIVITY_TOOLS.md`. If supplied, consult them for feature-specific explanations, **not** for queue order or permission to create a second branch for the same Goal.

### Dependency and branch-base contract

| Execution | Goal / classification | Preferred **new** branch | Required base | Reason |
|---|---|---|---|---|
| **G1 — first** | Query Focus — **Independent** | `fix/launcher-query-focus-jump-notes` | Safely verified current default integration tip (`master`/`main`), or current HEAD/clean worktree when required to preserve existing work and needed source capabilities | Establishes legitimate root/query/Notes/Jump focus ownership before the later new dialogs, note editing, and navigation changes. |
| **G2 — second** | Capture & Productivity Expansion — **Independent in feature design; deliberately chained for cumulative delivery** | `feat/capture-productivity-expansion` | **G1's final verified local commit SHA**, not necessarily its remote branch | Its initial **P0 topmost Pinboard input-shield repair** is needed before reliable static/live pin capture and opacity verification; adds screenshot-toolbar layout, hotkey directory, MkMacro capabilities, monitor brightness, and the first version of four shared features. Inherit G1's focus fixes instead of losing them. |
| **G3 — third** | Productivity Tools — **Dependent on G2** | `feat/productivity-tools` | **G2's final verified local commit SHA** | Builds on G2's shield/pin, Settings, Paste Sequence and table infrastructure; completes the additional seven unique features and the stronger/alternate requirements on four shared features. |

**This is an intentional dependency-driven reorder** of the received G1 → G3 → G2 attachment sequence. Do not undo it merely to preserve ZIP numbering. G3's native pin tests depend on G2's P0 capture reliability, and implementing G3 first would duplicate substantial feature work that G2 is already required to deliver. This order is both safer and less wasteful.

**Branch invariant:** The final G3 branch is cumulative: it contains the verified G1 and G2 commits in its ancestry, followed by G3's own new baseline and implementation commits. Earlier branch names/HEADs remain unchanged. **Never auto-merge, cherry-pick, rebase, reset, squash, force-push, or merge any Goal into `master`/`main`.** There is **one branch and one initial baseline commit per Goal**, not a branch for each feature or each milestone.

**Implementation source authority:** Each Goal describes source observed during its planning. G1 and G2 were prepared from October 20, 2026 source; G3 cites a later October 21 `jump` planning commit. Neither historical commit is automatically the correct Git base. The **actual safely established checkout at each Goal's bootstrap** is the source of truth. Adapt affected seams narrowly without silently importing unrelated historical commits. If a required capability is absent, determine whether it can be implemented within the Goal's scope; otherwise record a genuine blocker rather than merging an unrelated feature branch.

---

## 2. Instruction ownership and conflict-resolution rules

Apply instructions in this order, subject to non-overridable platform/tool safety and authorization rules:

1. Explicit current user request: the **three-Goal autonomous queue**, distinct branches and baselines, safe cumulative handoffs, one final review, and truthful reporting.
2. Each `CODEX_GOAL_*.md`: **what to implement**, feature behavior, milestone/checkpoint ordering **within that Goal**, code boundaries, exclusions, safety requirements, acceptance and test restrictions.
3. This master Kickstart: **inter-Goal ordering**, cumulative branch bases, bootstrap exactly once, handoff gates, overlap handling, shared ledger/resume rules, and queue-wide report.
4. Repository `AGENTS.md`, applicable nested instructions, `.codex/config.toml` and agent runners: approved engineering conventions, permissions, actual agent capability and ordinary Git/test policy where compatible.
5. Companion individual Kickstarts: supplementary execution context only. Their instruction to remain on one branch **ends at that Goal's completion**; it does not prohibit the next separately authorized Goal from making its own new branch.

Resolve apparent conflicts by preserving both contracts where practical, especially for already exposed user-visible commands. A later Goal may extend an earlier implementation, **never silently regress or remove it**. Do not relax security, focus ownership, privacy, data-integrity or acceptance checks in the name of integration. If there is no safe reconciliation of two mandatory requirements, isolate the actual conflict, report it precisely and treat the affected checkpoint as blocked; do not invent consent or change a Goal file.

Read the Goal's precise implementation steps rather than treating this master as an abbreviated replacement. The summaries in Sections 5–7 are **orchestration constraints and cross-goal acceptance**, not substitutes for task packets constructed from the original Goal.

---

## 3. Mandatory Git bootstrap at the start of **each** Goal

**Execute G1's `B0`, then G2's `B0`, then G3's `M0-A`, each exactly once, on entry to that Goal.** The individual Goal and master instruction to branch describe the **same** bootstrap checkpoint. No implementer may write feature code before that Goal's baseline exists.

### 3.1 Read-only preparation

Before creating any branch or running implementation/build/test commands:

- Read `AGENTS.md` and any more-specific repository instructions; read `.codex/config.toml` and `.codex/agents/{planner,implementer,reviewer}.toml`, verifying the actual runners and their access.
- Read **all three Goals** to understand overlap and the cross-goal dependency, plus the current Goal in full. Confirm exact filenames and, if useful, record SHA-256 digests to prevent a later resume from consuming altered contracts.
- Inspect `git status -sb`, `git branch --show-current`, `git rev-parse HEAD`, staged/unstaged/untracked changes, `git remote -v`, upstream, `git branch -a`, and the actual default integration branch. Never print remote credentials/tokens into a ledger.
- Identify the entry base: for G1 use the safe appropriate current integration code; for G2 and G3 use the **exact final verified SHA of the specified predecessor**. A failed predecessor push is not a reason to replace its local SHA with `origin/master` or a stale remote-tracking commit.
- Confirm the predecessor is at its legitimate terminal gate before a handoff; see Section 9. Never branch from its partially implemented HEAD just to keep the queue moving.

### 3.2 Preserve dirty or conflicting checkouts

- Do not discard or automatically stage unrelated files; do not auto-stash, `reset --hard`, `clean`, force-checkout, fast-forward over dirty content, auto-merge or rebase to make a branch base appear clean.
- On a clean G1 checkout, a normal fetch and safe use of the default remote-tracking tip is preferred; there is no requirement to move the user's local integration branch. Fetch failure is documented, not a reason to overwrite local state.
- For G2/G3, **queue dependency base overrides the individual Goal's usual `master`/current-HEAD preference**. Starting from the prior final SHA is a narrow authorized inter-Goal exception, not a mid-Goal branch switch.
- Prefer `git worktree add -b <new-branch> <new-safe-worktree-dir> <verified-base-sha>` when there are unrelated staged/unstaged changes or another active worktree. Use an isolated, uniquely named directory you own; don't delete another user's worktree or assume the prior working tree may be repurposed.
- If a safe new branch/worktree cannot be created, mark **Bootstrap BLOCKED** with precise evidence. Do not implement on `master`, an unrelated existing branch, or a predecessor branch as an expedient workaround.

### 3.3 Create the new branch and its first commit

- Confirm both local and remote branch-name availability when access permits. If a preferred name is occupied, choose a short descriptive suffix; **never reuse or overwrite** an existing task branch.
- Create/switch to the chosen fresh branch from the correct explicit base SHA and verify `git branch --show-current` and `git rev-parse HEAD` before writing.
- Initialize the Goal's specified task ledger when required:
  - G1: `docs/plans/launcher_query_focus_execution.md`.
  - G2: `docs/plans/capture_productivity_expansion_execution.md`.
  - G3: an existing suitable execution record or a concise `docs/plans/productivity_tools_execution.md`.
  - The compact **queue index** described in Section 10 may be initialized alongside G1's B0 and inherited on chained branches. Do not generate a second family of full plans.
- Stage **only** task/queue-owned bootstrap files after inspecting the staged diff. If no legitimate tracked bootstrap change is necessary, use an explicitly labeled **empty-only** baseline commit; beware that ordinary `git commit --allow-empty` may still include pre-existing staged user files. Prefer a clean worktree and use an index-safe empty-only invocation supported by the installed Git.
- Use the current Goal's preferred commit identifier: G1/G2 **`[B0]`**, G3 **`[M0-A]`**. Example subjects:

```text
chore(plan): [B0] establish launcher query focus baseline
chore(plan): [B0] establish capture productivity baseline
chore: [M0-A] establish productivity tools branch baseline
```

- Record `base_sha` **before** the baseline and `baseline_sha = git rev-parse HEAD` **after** it. A commit cannot include its own SHA inside the tree that commit records; persist that value in the next legitimate ledger update.
- Verify no pre-existing unrelated staged changes entered the baseline commit. Preserve existing source commits and the user's previous branch history.

### 3.4 Push without making GitHub approval a blocker

- Verify the intended remote. Attempt one regular **non-force** initial upstream push (`git push -u origin HEAD` when `origin` is correct), subject to the runner's normal authorization process.
- If denied, offline, or no authorized push exists: record `local_only`, the actual error/absence of authorization, and **continue from the local baseline**. No approval bypass, remote replacement, force flag or repeated retry loop.
- After each coherent **verified** checkpoint, inspect changes, stage only owned files, commit with that checkpoint label, and attempt **one** ordinary best-effort push if permitted. A failed push never invalidates the local commit.
- Keep every implementation, test and ordinary checkpoint within its Goal's newly created branch. No mid-Goal switch, merge, rebase, baseline amendment, or push-driven scope change.

### 3.5 Bootstrap acceptance gate

Mark B0/M0-A **COMPLETE** only when the new branch is active; its own initial commit exists; original base and new baseline SHAs and relevant worktree are recorded; push was attempted or its actual lack of authorization is recorded; unrelated work is preserved. Only then may the planner or implementer receive implementation tasks.

---

## 4. Actual Codex sub-agents and bounded checkpoint execution

Use the **real** runner definitions in:

```text
AGENTS.md
.codex/config.toml
.codex/agents/planner.toml
.codex/agents/implementer.toml
.codex/agents/reviewer.toml
```

Do not assert that historical model names, sandbox privileges or tool permissions still apply: discover them from those files and obey them. If a runner is unavailable, perform its responsibility with authorized available means and record the limitation, rather than inventing an agent.

**Planner (read-only):** After bootstrap, inspect the exact relevant current source, identify required owner/seam/dependency, and produce a small implementer-ready packet for the next meaningful checkpoint. No new 100-page plan, architecture initiative or duplicated research.

**Implementer (only source writer):** Take **one checkpoint at a time**; implement it idiomatically in Rust/egui/Windows APIs, run or arrange its focused checks, and return changed-file/test/error facts. No concurrent implementers on the same worktree; do not launch a separate implementation branch, change queue order or rewrite a Goal.

**Reviewer (read-only, independent):** At concrete risk boundaries, inspect actual diff and tests for regressions, safety and acceptance coverage. Prioritize Focus/Notes ownership (G1), native shield/hotkeys/pin opacity/MkMacro/brightness (G2), persisted repair, delayed capture, native opacity, note undo, table identity and DFF (G3). Give actionable findings; don't restart an entire planner/reviewer cycle for a single bounded fix.

**Parent orchestrator:** Own all Git/worktrees, queue ledger, agent packets, ownership of shared files, test budgets, small commits/pushes, blocker classification, per-Goal final verification, and automatic transition to the next new branch. Never let parallel agents write overlapping source or run conflicting native input/capture sessions.

### Implementer packet (mandatory minimum)

For every checkpoint send: **(i)** Goal filename and **its** checkpoint ID; **(ii)** current branch/base/baseline SHA; **(iii)** already completed dependent checkpoints; **(iv)** precise user-observable behavior; **(v)** current owners/paths; **(vi)** exclusions and regression/privacy invariants; **(vii)** focused nonzero-match test selectors, native evidence only if allowed, and deadlines; **(viii)** acceptance/commit boundary. State whether an overlapping feature is new work, an extension, or a previously implemented behavior requiring re-verification.

**Code-first cadence:** short read-only source check → implementation → focused verification → review when warranted → checkpoint commit → best-effort push → update compact ledger → next checkpoint. No intermediate user approvals, no unrelated cleanup/performance tracks, no repeated exploratory baseline campaign. Use labels `[M1-A]`, `[M1-B]`, etc., without inventing commits for checkpoints that required no change.

---

## 5. Execute **G1** in order — Launcher Query Focus

**Goal:** `CODEX_GOAL_LAUNCHER_QUERY_FOCUS.md`  
**Fresh branch:** `fix/launcher-query-focus-jump-notes` (unique suffix if necessary).  
**Prerequisite:** G1 `B0` completed on the fresh branch.

This is a **narrow correctness/focus fix**, not an invitation to redesign the launcher. Execute its checkpoints in **this exact within-Goal order**:

| Checkpoint | Required result and protected behavior |
|---|---|
| **M1-A** | Correct interactive hidden→visible **one-shot** autofocus of the actual `egui::Id::new("query_input")` TextEdit. Protect late native activation with only a visibility-revision-bound pending handoff; no permanent refocus loop. |
| **M1-B** | Protect active Notes editing, prompts, unsupported windows and true modals; nonmodal window presence alone must not block eligible autofocus. Respect `RootFocusIntent::PreserveForeground`. |
| **M2-A** | Make a **visible and unobstructed** root query mouse-focusable beside nonmodal Quick Notes when Jump Mode is enabled, without enabling unrelated background Dashboard/commands. |
| **M2-B** | Provide a strictly scoped Notes→query **Jump hint → real Response focus** handoff. Hint completion only focuses/selects; later Enter is a separate action. Reject stale owners/hints and occluded query targets. |
| **M2-C** | Restore query-owned search, result navigation and **one deliberate fresh Enter activation** while nonmodal Notes stays open. Notes editor Enter and actual modals must never execute root actions. |
| **M3-A** | Focused egui/Nextest integration: repeated show/hide, empty/nonempty query, Jump on/off, mouse/hint→query→Notes, preserved draft, native preserve-foreground, current result identity and no ghost Enter. |
| **M3-B** | Final maintained-code lint gate **last**, accurate Git/test/native evidence report, and stop work on G1. |

**Additional G1 hard rules:** Preserve Shift+Space, default-off Jump, deterministic hint epochs, native focus ownership, nonmodal vs modal boundaries, Note panel/editor drafts, Note Find shortcuts, current query/results behavior, and the existing Settings/File Search/MkMacro/Paste input safety. Do not add synthetic OS input, global hook, second Jump engine, focus polling thread, preference/schema change, arbitrary background control navigation or workspaces.

**G1 testing rule overrides generic defaults:** **NO scheduled full Nextest**. Use the Goal's tightly selected egui/Nextest slices (normally one per checkpoint plus an adjacent ownership slice; at most two final composed slices). Existing direct tests must remain meaningful, even if their old expectation requires an intentional update. An owned native focus smoke is **optional**, at most one controlled session with 1–2 redacted screenshots; environmental denial is reported after a bounded attempt. New test profiles must disable Paste Sequence Next/Cancel shortcuts. No release build/benchmark campaign. Finish all tests/format/commits before final maintained Clippy; no more G1 implementation after the final successful lint gate.

**G1 handoff:** Record G1's status, branch, original base SHA, `[B0]` SHA, all verified checkpoint commits, actual test filters/results, final Clippy, optional native evidence, final HEAD and push outcome. If status is **COMPLETE** or safe **COMPLETE_WITH_LIMITATIONS** as defined in Section 9, mark this SHA immutable as G2's intended base and proceed directly to G2's **new** B0. Do not merge G1 into `master`.

---

## 6. Execute **G2** in order — Capture Stability & Productivity Expansion

**Goal:** `CODEX_GOAL_CAPTURE_AND_PRODUCTIVITY_EXPANSION.md`  
**Fresh branch:** `feat/capture-productivity-expansion` (unique suffix if needed).  
**Base:** Exact verified final **G1 local HEAD**, not `origin/master` (unless the exceptional blocked-G1 independent fallback in Section 9 is proved safe and explicitly recorded).  
**Prerequisite:** G2 `B0` fresh branch + baseline + initial push attempt. G2's companion Kickstart remains subordinate to this queue's base/transition authorization.

**Execute P0 screenshot fixes before the eight features.** Preserve this internal milestone/checkpoint order:

| Checkpoint(s) | Required result and special safeguards |
|---|---|
| **M0-A** | Short post-B0 owner inspection/ledger; no full baseline testing campaign. The separately committed G2 `[B0]` already exists. |
| **M1-A/B/C — P0 shield** | Reproduce the repeated static/live Pinboard `Rectangle input shield monitor 1: topmost style missing` error; restore **actually topmost, layered, nonactivating, hit-testable** rectangle shields, including repeat/recovery and multi-monitor bounds. Never remove `WS_EX_TOPMOST`/coverage validation or make a safety shield click-through. Confirm cleanup and later capture after a transient failure. |
| **M2-A/B — P0 toolbar** | Zoom and Text Size sliders occupy the same compact row **above** screenshot Tool buttons, with responsive narrow-size behavior and unchanged markup/Save/Copy/Undo/Redo. |
| **M3-A/B/C — Settings search** | Typed destinations for Settings and plugins, a search field, exact expand/scroll/focus navigation, draft-safe Save/Cancel, disabled/unavailable labeling, dedicated-dialog destinations and Jump compatibility. This is **navigation only**, not settings mutation. |
| **M4-A/B/C — Hotkey Directory** | One read-only searchable inventory with owner, action, chord, conflict, **acknowledged effective registration**/Unknown statuses and Edit navigation; no new shortcut manager or hooks. Reconcile live route generations and plugin disable/reload. |
| **M5-A/B — Paste source pickers** | Search/scroll/select/preview snippets and clipboard history with explicit Insert; conceal hidden snippet bodies including search; ephemeral clipboard content search; preserve existing stable source references, Step serialization and all playback modes. |
| **M6-A/B/C — Pin opacity** | Native **per-pin 10–100%** opacity, default 100%, static/live, Pin List control and independently maintained topmost/click-through, real desktop compositing and untouched Copy/Save pixels. G2's P0 shield must remain healthy. |
| **M7-A/B/C/D — MkMacro forms** | Add backwards-compatible String parameter Text/File/Folder/Choice **presentation metadata**, author options in Signature Editor, use safe file/folder/choice controls in existing invocation prompt, and preserve typed values, stable IDs, CallMacro, Cancel and legacy JSON. |
| **M8-A/B/C — MkMacro clipboard steps** | Add explicit text-only **Read Clipboard → Variable** and **Write Text/Variable → Clipboard** model/catalog/editor/executor steps, with runtime-only effects, existing cancellation/Hot Folder capability checks and no secret logging/implicit paste. |
| **M9-A/B/C — `table` CSV/TSV utility** | Parse quoted CSV/TSV, bounded view/projections, sort/filter, column/row selection, Copy/JSON/explicit Save As, responsive table UI and unchanged source file. **Use `table` as the feature's route; reserve `csv` for G3** rather than taking its optional alias here. See Section 8 for requirements that must survive G3. |
| **M10-A/B/C — Per-monitor brightness** | Enumerate named physical monitors including multiple physical per logical, truthful DDC/CI capability, native min/max conversion, explicit per-device Set, handle destruction, bounded refresh and preserved legacy `bright` behavior. **Never change real user brightness in autonomous tests.** |
| **M11-A** | Small cross-feature seam checks (Settings/Hotkeys; macro forms/clipboard; screenshot/shield/pin; table/bright routing), then close the implementation loop. |
| **M11-B** | **Exactly one scheduled full `cargo nextest run --workspace` near the end** on appropriate actual Windows workspace/toolchain; at most one justified corrected full rerun after a focused repair. Distinguish new regressions from pre-existing/environment failures. |
| **M11-C** | Finish bounded UI evidence, Help/README, formatting, ledger and **all non-lint implementation/checkpoint commits**. Verify task-only diff. |
| **M11-D** | **Final maintained-crate Clippy with `-D warnings`**, using verified Cargo packages and excluding vendor as independent targets. Necessary lint fixes are bounded and committed **before the last successful Clippy invocation**. No more G2 code/test/format changes after that clean gate. |

**G2 core priorities must not be diluted:** P0 shield error is not a diagnostic-only suggestion; disabling the style probe or calling a successfully returned `SetWindowPos` “verified” without re-probing fails the contract. P0 toolbar must actually fit normal and narrow dialog geometry. Hotkey Directory must not claim configured=registered. Unknown/native monitor support is an honest state, not grounds for fabricated values. These are acceptance boundaries even when later feature work is easier.

**G2 native and tests:** During M0–M10 use one or at most two focused Nextest slices per checkpoint, and at most one controlled owned native smoke per affected milestone (1–3 non-sensitive captures, repeated captures allowed inside the one P0 session). On unavailable desktop: one diagnostic + one retry, then `not_run_environment`. For hardware brightness, use **fake Set** and no mutation of unowned monitors. **One full Nextest is mandatory at M11-B** unless genuinely blocked by the environment; do not repeat full runs at checkpoints. **Clippy is M11-D, last.** Do not attempt a release build, broad performance project or vendored-source cleanup to satisfy this queue.

**G2 handoff:** Mark G2 COMPLETE only when both P0 tasks and all eight features meet their actual required acceptance, with truthful full-nextest/lint and native/hardware limitations. Record complete checkpoint commits, `table` route, original+baseline+final SHAs and last push result. If safely terminal per Section 9, create G3's fresh branch **from G2 final HEAD**, preserving G2 history; never merge into `master`.

---

## 7. Execute **G3** in order — Productivity Tools

**Goal:** `CODEX_GOAL_PRODUCTIVITY_TOOLS.md`  
**Fresh branch:** `feat/productivity-tools` (unique suffix if needed).  
**Base:** Exact verified final **G2 local HEAD**.  
**Prerequisite:** G3's own mandatory **M0-A** new branch + baseline + initial non-force push attempt. This is not a second G2 branch bootstrap.

G3 has **eleven features**, four of which overlap G2. For those four, reuse the predecessor's implementation when it satisfies the contract, explicitly test the inherited behavior, and make **narrow additive changes** for the unmet requirements in Section 8. Do **not** replace G2's Settings, picker, native pin or table implementation wholesale. Do not skip a G3 acceptance item merely because a class or button exists already.

| Checkpoint(s) | Required result, including inherited behavior |
|---|---|
| **M0-B** | Source/test/profile reconciliation on the new G3 baseline; verify inherited G2 feature contracts and actual current paths. Configure opt-in bounded nextest profile only if supported; do not enumerate every test target. |
| **M1-A/B — Settings search** | Reuse G2's search catalog/navigation, then cover **all existing built-in settings including disabled built-ins** without enabling them, `settings <query>`, stable exact-control focus/highlight, Theme/Mouse/Gesture/PluginEditor, and honest owner-level links for opaque external plugins. Preserve G2's Save/Cancel and focus behavior. |
| **M2-A/B — Target repair** | Explicit repair of saved application/file/**folder** targets, preserving original metadata, aliases, order, usage and active references through guarded owner/version preflight and coordinated publish/rollback. Add proper picker and missing-entry UI. Do not silently convert indexed results or rewrite history. |
| **M3-A — Clipboard Modify lines** | `cm` remove-blank-lines, number-lines, reverse-lines, join-lines, split-lines through parser, catalog, preview, Apply/Undo, saved pipelines and Help, with literal separator rules and old pipeline compatibility. |
| **M4-A — Paste Sequence pickers** | Reuse G2's ephemeral snippet/history picker. Verify hidden text cannot affect body matching, labels or counts; preserve raw snippet preview (no prompt evaluation), exact alias/digest references, Add-once, selection lifetime, Escape/Enter/Jump and all existing playback policies. Amend only missing contract behavior. |
| **M5-A — Screenshot markers** | Add one-click 1,2,3… annotations using existing `MarkupHistory`/raster output, stable numbering on undo/redo and undo→new branch, multi-digit legibility, identical preview/copy/save/crop/zoom behavior. Preserve G2's compact toolbar row. |
| **M6-A/B — Delayed capture** | Add **3/5/10-second** `ss` delay variants and `ss cancel`, one monotonic generation-bound pending request, a reachable nonactivating countdown while launcher is hidden, target resolution **at expiry**, countdown-free pixels and accurate post-capture history. New delayed routes are explicitly GUI-only; immediate headless path remains. |
| **M7-A/B — `imgdiff`** | Read-only A/B image compare with shared zoom/pan, wipe, exact RGBA differences including alpha, unequal-size nonoverlap, bounded decode/texture budgets, stale result rejection and a distinct prefix preserving existing text/binary `diff` path grammar. |
| **M8-A/B — Pin opacity** | **Revalidate and extend** G2's native per-pin 10–100% static/live opacity, especially explicit **Reset to 100%**, last successfully applied value on native failure, lifetime checks, Pin List recovery, composited desktop evidence and unchanged source pixels. No second opacity controller or vendor rewrite. |
| **M9-A/B — Current note Replace** | Literal case-insensitive Replace/Replace All over **live unsaved Markdown**, byte/character revision checks, source-only Preview mapping, single-step undo/redo, correct dirty/save/discard semantics and two-note isolation. Preserve G1 query/Notes/Jump focus ownership. |
| **M10-A/B — `csv` CSV/TSV utility** | Extend/reuse G2 table engine with a **separate `csv` plugin entry** and G3's strict/string-preserving defaults, explicit JSON array/object modes and larger bounded limits. Keep the prior `table` command and its specified behavior working. Exact reconciliation in Section 8 is mandatory. |
| **M11-A/B/C — `dff`** | New finder-only duplicate scanner: explicit roots, safe traversal and Windows file identity/hard-link handling, size→streaming SHA-256→bounded byte equality, stale-file detection, cancellation/nonblocking worker cleanup, grouped results, Open/Reveal/Copy Path. **No delete/recycle/dedup writes.** |
| **M12-A** | Independent final review, focused cross-tool tests, scoped docs/evidence, formatting/checkpoint closure and **final first-party Clippy**. G3 does **not** require a full Nextest sweep; preserve its bounded selected-suite policy. |

**G3 verification:** Scope nextest **list and run** to the actual binary/library/test target; do not compile every test target just to discover one filter. Use terminating per-test and overall process limits, zero retries by default, isolation diagnosis once for group interference, and truthful results. Use the G3 Goal's bounded harness/owned-process requirements. Native evidence is necessary where its acceptance depends on actual desktop compositor/focus behavior; otherwise report the remaining limitation and do not call it fully verified. No forced user participation.

**G3 closeout ordering clarification:** The M12-A Goal enumerates final tasks, including lint and documentation. Preserve **Clippy-last** by doing substantive reviewer fixes, focused tests, help/doc/ledger edits, formatting and all task code/commit steps **before the last successful Clippy invocation**. Thereafter perform only read-only Git/status verification, any permitted final ordinary push, factual queue status recording and reporting; do not edit/test/format the completed G3 implementation again. If lint fixes are necessary, fix/format/test/commit them **before** a final clean Clippy rerun. This is an execution-order reconciliation, not a functional Goal modification.

---

## 8. Shared-feature acceptance and **non-destructive compatibility**

**Do not double-implement G2's four shared features.** Treat them as a series of contract deltas. G2's implementation is the first authoritative delivered version; G3 must retain it, verify it against **both** Goals and extend only missing behavior. Record the exact tests and code proving a reused checkpoint. A no-change checkpoint may be marked `REUSED_VERIFIED` rather than manufacturing an empty feature commit; mandatory fresh-branch **baseline** is the only required empty commit. Any genuinely necessary delta gets G3's specified labeled commit.

| Shared feature | G2 contract already delivered by design | Required G3 additive acceptance / preservation |
|---|---|---|
| **Settings search** (G2 M3 ↔ G3 M1) | Real Settings search; section/plugin/dedicated dialog discovery; one-shot expand/scroll/focus; Save/Cancel safe; truthful unavailable states. | Add or verify `settings <query>` routing, disabled **built-in** searchable metadata without enabling plugins, complete built-in catalog/Theme/Mouse/Gesture/PluginEditor coverage and exact current control highlighting. Keep G2 navigation semantics, stable IDs and dirty drafts. |
| **Paste source pickers** (G2 M5 ↔ G3 M4) | Searchable Snippet and Clipboard history menus, ephemeral selected previews, stable reference Insert, concealed hidden-body search, stale protection and nested keyboard handling. | Verify raw, unevaluated normal-snippet preview; hidden text also excluded from excerpts, accessible labels, logs and query-dependent result counts; selected clipboard preview may be full **within a bounded active scroll area**; Add exactly once, no new stored body, correct lifecycle. Keep G2's explicit Insert and search-by-history-text workflow. |
| **Pin opacity** (G2 M6 ↔ G3 M8) | Per-pin 10–100% native/window translucency, default 100%, independent static/live state, Pin List control and click-through/topmost. | Explicit 100% reset in both surfaces, native failure retains last acknowledged value, restored/hidden/live lifecycle safety, actual composited background visibility and original Copy/Save pixel equality. Keep the G2 P0 shield coverage repair intact. |
| **CSV/TSV** (G2 M9 ↔ G3 M10) | `table` entry; UTF-8/BOM/CSV+TSV, Auto delimiter on paste, tolerant/warned short rows, optional numeric-aware stable sorting, Copy Selected Columns, JSON Copy and explicit Save As; limits approx. 10 MiB / 50,000 rows / 128 columns. | `csv` entry; CSV/TSV explicit choice (comma/header default, filename-derived `.csv`/`.tsv`), **lexical string sort without implicit numeric coercion**, **strict ragged-record rejection with retained last-good table**, exact string values and explicit object-vs-array JSON, zero-selection handling, plus 16 MiB / 100,000 rows / 256 columns / 1,000,000 cells / 64 MiB output bounds. Preserve `table`'s existing behavior and source-file read-only guarantees. |

### The CSV/TSV compatibility problem is real; resolve it explicitly

G2 and G3 are **not identical specifications**. G2 requests `table`, Auto delimiter, numeric-aware sorting for uniformly numeric columns, and allowed short-row padding with warnings. G3 requests `csv`, explicit delimiter, **lexical sorting**, and **errors on ragged records**, with different size limits. Do not silently replace the first contract with the second or build two independent parsers.

**Required integration design:**

1. **One maintained parsing/model/worker/virtual-table engine** shared by both entry points. Source values remain `String`; no inferred output types, formula interpretation or hidden source mutations.
2. **Two distinct visible command routes**, `table` and `csv` (ensure real parser token boundaries; do not reserve optional `csv` alias during G2). Preserve any existing valid launcher commands. The `csv` route belongs to G3's normal built-in plugin enablement; `table` remains discoverable and usable.
3. Represent the differing modes as **explicit visible behavior**, not invisible magic. A practical design is a bounded table utility with visible **Delimiter**, **Sort mode** (`Lexical` / `Auto numeric` where valid), and **Row-width policy** (`Strict` / `Lenient with warning`), initialized from the invoking command. `table` defaults to its G2 modes; `csv` defaults to its G3 modes. Do not change mode merely by repainting or opening a filename. G3's filename-extension delimiter selection applies to `csv` input.
4. **Preserve route-specific processing ceilings where the contracts demand them**: `table` must not silently exceed G2's stated max rows/columns; `csv` must enforce G3's larger limits and total cell/output bounds. Exceeding limits produces a clear error, not truncation or a frozen GUI. If an explicit shared cap is safer, it may only be used when it demonstrably satisfies **both** sets of mandatory acceptance; otherwise keep the route policy explicit.
5. Lenient short rows pad for display with a warning and do not discard extra values; if longer rows cannot be represented safely, diagnose them rather than silently dropping cells. In `csv` Strict mode any inconsistent width is a record-specific **error**; keep prior valid table plus its original successful delimiter/header state and label it `previous valid data`.
6. `table` retains its numeric-aware compare when its selected numeric policy applies; `csv` defaults to stable **case-insensitive lexical** sort with stable original-row ties. A visible user-invoked switch to the other mode is okay, but neither entry path may silently coerce strings, change the JSON types or lose leading zeros.
7. Preserve both data workflows: explicit clipboard Copy, correctly quoted selected-column output, JSON object output only for valid unique nonempty headers, array output available otherwise, `table` Save JSON As with explicit overwrite safeguards, and source files unchanged. Add exact route-specific tests for both commands and for opening the same input under the two advertised policies.

If an earlier `table` implementation accidentally captured the `csv` prefix, correct the prefix conflict narrowly on G3 without deleting the old `table` route or changing existing user-saved actions. If a current checkout already has an unrelated command with either name, detect the **actual** conflict; do not overwrite an existing namespace blindly. Escalate only an irreconcilable compatibility conflict as a specific blocker.

### Cross-Goal interaction sentinels

- **G1 Focus ↔ G2 Settings/Hotkeys and G3 Notes:** Settings/Quick Notes/nav dialogs never steal a legitimately focused launcher query; independent modal/popup owners still block. G3 Replace/Find doesn't bypass the G1 Notes→query/Enter safety guard.
- **G2 P0 Shield ↔ G2/G3 pin opacity and G3 countdown/markers:** Repeated static/live rectangle capture retains a verified hit-testable topmost shield; overlays/opacity/countdown cannot accidentally make the selection shield click-through or steal foreground target ownership. G3 markers coexist with G2's two-slider toolbar row.
- **G2 Paste ↔ G3 Clipboard Modify and delayed input:** Picker browsing never mutates clipboard or Paste Sequence playback. Explicit Macro clipboard actions and Clipboard Modify Apply retain their own guarded writes. A new `ss cancel` command is not a global shortcut registration.
- **G2 Hotkey Directory ↔ G3 new commands:** New commands do not count as acknowledged hotkeys unless an actual runtime route was registered and acknowledged. No duplicate listeners or unexpected cofire.
- **G2 Brightness ↔ G3 UI/worker additions:** No new per-frame physical monitor polling or accidental global brightness changes.
- **G2 Table ↔ G3 CSV:** `table`'s old saved commands remain meaningful; `csv` uses its own defaults and both share a correct domain engine.
- **G3 new `imgdiff` ↔ old `diff`:** Existing filenames beginning with `image` retain legacy Diff grammar; never steal them through a new `diff image` subcommand.
- **G3 target repair ↔ saved action identity:** Preserve exact persisted Universal Action references and usage/favorite/pin associations or fail with a documented recovery conflict; no wrong-record repair after list reorder.

---

## 9. Mandatory Goal completion, failure, and handoff gate

**At every end-of-Goal boundary** classify each required checkpoint and the Goal itself using these statuses:

| Status | Meaning | Queue decision |
|---|---|---|
| `COMPLETE` | Mandatory behavior and applicable acceptance verified; changes committed; final lint/test rules honored. | Advance to next Goal on this local final SHA. |
| `COMPLETE_WITH_LIMITATIONS` | All requested implementation and deterministic safety checks passed; **only specifically optional/environment- or unsupported-hardware-dependent** evidence was unavailable. No known correctness/safety defect remains. The missing proof and downstream dependency risk are identified. | Advance only if the next Goal is safe with that limitation; retain an explicit acceptance gap. Never relabel an unverified **required** native property as fully passed. |
| `BLOCKED` | Mandatory safety/correctness/acceptance or safe Git baseline failed despite bounded remediation; source implementation incomplete/unsafe; dependency cannot legitimately be consumed. | **Do not base a dependent Goal on this unfinished HEAD.** Follow the independent-only fallback below if safe; otherwise stop and report. |
| `NOT_STARTED` | No verified bootstrap/execution for this Goal. | Do not invent branch, test or commit results. |

**Before any handoff:** Require a terminal Goal status; review the diff against **that Goal's own new baseline**, confirm all intended changes committed, last local HEAD SHA recorded, remaining unrelated dirty state identified, actual final push attempted/recorded, and no owned test process/capture state left running. For `COMPLETE_WITH_LIMITATIONS`, verify exactly what is unproven and that the next Goal will not assume it was verified.

**If G1 is blocked:** Do not branch G2 from incomplete G1 code. G2 is independently specified, so the orchestrator **may** proceed to G2 only if a clean, suitable current integration base can be established without depending on unverified G1 behavior or discarding any G1 work. Record the changed base/dependency and that the final cumulative result **does not include completed G1**. If G2 would need those blocked focus changes to operate safely, halt. No silent fallback that makes the queue look complete.

**If G2 is blocked:** G3 is dependent on G2's shared implementation and P0 shield correctness. **Do not start chained G3 from G2's unfinished branch.** Stop with a blocker report. Do not clone or reimplement the whole unfinished G2 inside G3, use a fake success receipt, or silently base G3 on `master` and claim accumulated completion. A future explicit authorization could separate independent G3 features, but this queue does not authorize silently splitting the Goal.

**If a Goal is `COMPLETE_WITH_LIMITATIONS`:** An optional denied desktop screenshot or unsupported real brightness device may be recorded as such when safe fake/domain behavior passes. An unresolved **actual unsafe shield**, pin opacity that does not genuinely show the background, potential mis-targeted file repair, clipboard privacy leak or wrong duplicate grouping is a **BLOCKED mandatory capability**, not an optional screenshot gap. For G2→G3, if compositor acceptance is unproven due to environment, G3 must treat that as pending verification, not “reused and complete”; only advance if this does not conceal a known unsafe implementation.

**Push denial is never in itself a blocked Goal** once local commits are safe. Do not claim remote integration: locally chained branch ancestry is sufficient for this authorized queue. A branch-creation authorization denial that prevents even a local baseline **is** a blocker.

**Do not invent checkpoint commits.** A checkpoint that is fully satisfied by inherited G2 code may be `REUSED_VERIFIED` in the G3 ledger with its predecessor source commit and actual verification, while new changes get G3-labeled commits. No second baseline and no empty fake feature commit for it.

---

## 10. Durable, compact queue progress and interruption recovery

Keep a small **queue index** plus the detailed per-Goal ledgers required by the Goals. A suitable tracked queue-index location is `docs/plans/CODEX_MULTI_GOAL_QUEUE_LEDGER.md`, created only **after** G1's branch has been created and included in its B0 baseline commit if appropriate. Because G2/G3 branch from their predecessors, this index is inherited naturally; update it only at meaningful checkpoints or transitions. If the runner provides a more appropriate durable artifact/ledger location, use it and record the exact path. Do not create three new copies of a full implementation plan.

Recommended compact queue index (update factual values during execution):

| Queue | Contract | Dependency | Branch | Base SHA | Baseline SHA | Current checkpoint | Checkpoint SHAs | Focused tests | Final Nextest | Final Clippy | UI/hardware evidence | Push | Final HEAD | Status |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| G1 | `CODEX_GOAL_LAUNCHER_QUERY_FOCUS.md` | None | Pending | Pending | Pending | B0 | — | Pending | **Not required** | Pending | Pending | Pending | — | NOT_STARTED |
| G2 | `CODEX_GOAL_CAPTURE_AND_PRODUCTIVITY_EXPANSION.md` | Cumulative G1 | Pending | G1 final | Pending | B0 | — | Pending | **Required once near end** | Pending | Pending | Pending | — | NOT_STARTED |
| G3 | `CODEX_GOAL_PRODUCTIVITY_TOOLS.md` | G2 final | Pending | G2 final | Pending | M0-A | — | Pending | **Not required** | Pending | Pending | Pending | — | NOT_STARTED |

For each meaningful commit, retain exact test **command/selector**, intended selected count, actual pass/fail/timeout, the owning checkpoint, commit SHA, push attempt/outcome and any evidence path/receipt. Identify `REUSED_VERIFIED`, `IN_PROGRESS`, `COMPLETE`, `COMPLETE_WITH_LIMITATIONS`, `BLOCKED` and `NOT_STARTED` separately. Omit secret snippet bodies, clipboard content, credentials, personal paths, executable bytes or screenshot payloads from these logs.

**Commit timing:** Keep real implementation/checkpoint commits on the active branch and carry the queue index along with them. Finish code/docs/test modifications and tracked ledger closure **before the last clean Clippy** required by each Goal. A successful final Clippy may be noted in the final report or an external factual execution receipt without reopening that Goal's code, tests or formatting. The next branch bootstrap may inherit/advance the queue index after it has legitimately begun. Do not amend a completed predecessor baseline or final checkpoint just to insert metadata.

### Resume algorithm (no duplicate branches)

On a resumed Codex session/context reset:

1. Re-read this master, all Goal contracts, `AGENTS.md`, actual agent configs, and the queue index/Goal ledgers.
2. Verify with Git: current worktree, exact branch/HEAD, recorded base and baseline SHA, staged/unstaged files, commit parents and ancestry, and locally present predecessor final SHAs. If they differ, diagnose; never reset/rebase/overwrite to force a match.
3. For any Goal already bootstrapped, **resume the queue-owned existing branch** after verifying its identity. **Do not create a second branch or B0/M0-A baseline** just because the conversation, process or orchestration session changed.
4. Check that claimed completed checkpoints actually have commits and nonzero-match verification receipts. An in-progress checkpoint with uncommitted task work is reviewed and continued safely; previously committed checkpoints are not blindly reimplemented.
5. If resuming after a completed Goal's final Clippy, do not rerun/edit that Goal for bookkeeping. Confirm its terminal result from available evidence and proceed to the authorized next new branch if the handoff gate permits.
6. If a dependency is absent, a Goal document has changed materially, or Git ancestry cannot be established, report the precise blocker before writing. Do not silently start a different queue.

**Automation realism:** An interrupted session does not imply that Codex continues in the background. The ledger makes a **future, explicitly resumed execution** possible; never claim that unattended work happened while no execution process was active.

---

## 11. Testing, visual evidence, limits, and final quality discipline

**G1:** Limited owner-focused egui/Nextest as specified; no full suite. G1 final Clippy last. No custom global input hooks or user-owned GUI automation.

**G2:** At most two focused Nextest slices per checkpoint. Perform the one required workspace-wide Nextest **only after** all features, in M11-B, with at most one justified corrected rerun. Finish evidence/docs/format/commits, then final `-D warnings` first-party Clippy in M11-D. Avoid repeated full compilation and vendored lint targets.

**G3:** Follow its explicitly bounded selected-suite policy: individual normal test **60s** (narrow native/process exception up to **120s**), selected batch at most **10 minutes**, separate build/check ceiling initially **30 minutes**, native scenario normally **90s** (justified up to **180s**) plus about **10s** owned cleanup, native-evidence batch about **20 minutes** after build, final first-party Clippy build-aware ceiling **30 minutes**. A supported opt-in nextest profile may enforce `slow-timeout` termination and, only when installed nextest supports it, `global-timeout`; do not mistake a SLOW warning for termination. G3 **does not mandate full Nextest**. Do not silently use a zero-match filter or run tests against every test target before selecting the relevant one.

**For all three:**

- Run only the active Goal's required checks. Verify test selection via scoped `cargo nextest list` and selected-count evidence. Use zero retries by default. Only rerun an exact failure in isolation when shared global state/interference is plausible; preserve both group and isolated results.
- Keep logs in UTF-8, without ANSI colors where practical for SSH review; include exact commands/exit codes and timeout state. Run compilation with an independently bounded owned process tree; `nextest` test timeouts do not bound Cargo's compilation or link stage.
- If the same issue repeats, perform **at most two substantive correction rounds**, with a focused recheck after each, unless the Goal prescribes a stricter budget. Classify unrelated or pre-existing failures separately; do not rewrite tests or lower assertions merely to report green.
- Run isolated, owned Windows GUI fixtures only where each Goal permits. Never run native keyboard input against the user's ordinary windows, capture personal desktop data, change real monitor brightness in automated tests, mutate real clipboard/history when a fake adapter is available, or kill by executable name. Retain exact owned PIDs/start identities and close/clean only those processes.
- Evidence must prove effects, not button presence: source vs output bytes for images/table, actual composed desktop visibility for pins, focus/ownership for Notes and Jump, post-deadline target selection for screenshots, no clipboard mutation on preview, file contents unchanged for DFF, exact live registration acknowledgment for hotkeys.
- A screenshot of the root framebuffer alone **cannot** verify per-window desktop transparency; a pointer/control screenshot alone cannot prove shield hit-testing or absence of underlying clicks.
- Use existing QA/native harnesses economically; do not build a second universal GUI driver. On denied desktop access, perform one diagnostic and one retry at most where the Goal permits, then label `NOT_RUN_ENVIRONMENT` or `BLOCKED_NATIVE` and continue only safe work.
- **Clippy last, per Goal.** Complete substantive code edits, relevant format/diff checks, focused tests, reviews, docs and checkpoint commits first. Run maintained-crate Clippy near the end of **that Goal**. If a lint fix is needed, make it and commit before the eventual last successful Clippy. After that successful gate, no more edits/tests/format of the completed Goal; the **next Goal** may start a separate cycle on its new branch. Don't lint third-party `vendor/` packages as independent targets unless actually required by verified workspace membership and permitted instructions.

Avoid a full release build, all-day benchmark effort, unrelated performance project, broad feature backlog, unowned background worker, or repeated final verification unless mandated by a concrete new regression. Never claim a skipped native action, failed push, timeout or passed-alone flaky test as a verified success.

---

## 12. Handoff safety and final consolidated report

At queue completion or a genuine stopping condition, produce **one factual final report**; no automatic new planning initiative or speculative follow-on improvements.

### Required queue table

| Goal | Branch | Baseline SHA | Final SHA | Status | Tests | Push |
|---|---|---|---|---|---|---|
| G1 — Query Focus | *actual new branch* | *actual B0 SHA* | *actual final local HEAD* | COMPLETE / LIMITED / BLOCKED / NOT_STARTED | *exact focused results; no full Nextest* | *actual initial/final outcome* |
| G2 — Capture Expansion | *actual new branch* | *actual B0 SHA* | *actual final local HEAD* | COMPLETE / LIMITED / BLOCKED / NOT_STARTED | *focused + actual full Nextest + final Clippy* | *actual initial/final outcome* |
| G3 — Productivity Tools | *actual new branch* | *actual M0-A SHA* | *actual final local HEAD* | COMPLETE / LIMITED / BLOCKED / NOT_STARTED | *focused + final Clippy; no mandatory full Nextest* | *actual initial/final outcome* |

Also report, concisely but completely:

1. **Git provenance:** Starting branch, base SHA and justification for **each** Goal, B0/M0-A baseline, all meaningful checkpoint commit SHAs, final local HEAD, actual upstream branch/push result and whether any work is local-only. State explicitly that `master`/`main` was **not merged into or modified by the queue**.
2. **Delivered checkpoint coverage:** G1 M1–M3; G2 **both P0 issues and eight feature areas**; G3 **eleven features including four inherited/extended contracts**, with `REUSED_VERIFIED` where appropriate. Distinguish implemented, verified, limited and blocked.
3. **Cross-goal decisions:** Reason for reordered execution, chained branches, resolved `table`/`csv` behavior, settings/picker/pin deltas, preserved original `diff` grammar and all prefix compatibility.
4. **Code ownership:** Meaningful changed modules/files by area and important implementation decisions, without dumping unneeded source diffs.
5. **Verification:** Actual selected filters/target counts, pass/fail/timeouts, group-vs-isolated observations, G2 final full Nextest total and failures, each Goal's final Clippy command/outcome, and any pre-existing unrelated failures.
6. **Native evidence:** Actual screenshot/receipt paths and what each proves; explicit missing interactive desktop, unsupported brightness hardware, compositor limitations and other unverified acceptance.
7. **Reviewer findings:** Material issues discovered and bounded fixes, plus remaining blocking findings with evidence.
8. **Final safety state:** Owned process/tree cleanup, no leaked input shields/pins/countdowns or test workers, no user data mutations, protected original Git index/worktrees, and preserved earlier Goal histories.
9. **Remaining manual review considerations:** Only the **single post-queue user review**, focused on any genuinely unverified native behaviors. Do not ask for intermediate verification or suggest a new initiative.

**Stop after the final report.** Do not auto-merge any branch into `master`, open new workstreams, or continue writing code for optional enhancements.

---

## 13. Immediate execution directive

1. Read **all three exact Goal Markdown files** and the applicable repository instructions/agent runner definitions now. Use the Goal files themselves, not zipped source snapshots or this conversation.
2. Establish the queue index and select the verified G1 base through **read-only** Git inspection. **First actual task write must occur on G1's newly created `fix/launcher-query-focus-jump-notes`-style branch.** Create its distinct `[B0]` baseline commit, record its SHA, and attempt its normal initial push.
3. Use the configured planner/implementer/reviewer workflow to execute **all of G1 M1–M3**, with the Goal's focused-only testing and Clippy-last stop. Verify the G1 completion gate.
4. If safe, create G2's **different new branch from verified G1 final local HEAD**; commit its own `[B0]` baseline and attempt its own initial push. Execute **G2 M0–M11**, starting with both P0 screenshot requirements and including **exactly one scheduled full Nextest** near its end, followed by final Clippy. Verify G2 completion.
5. If safe, create G3's **third different new branch from verified G2 final local HEAD**; commit its own `[M0-A]` baseline and attempt its own initial push. Execute **G3 M0-B–M12**, reusing verified earlier features and implementing all missing deltas, with **selected tests only** and final Clippy.
6. Maintain truthful compact queue progress. Resume from verified ledger/Git state if interrupted, never duplicating an already completed bootstrap or rerunning the entire queue blindly.
7. Produce the single consolidated report specified above and stop. **Do not ask for milestone-by-milestone approval, merge into master, or drop an attached Goal.**

**START EXECUTION NOW: G1 read-only Git inspection → fresh branch → distinct B0 baseline → best-effort initial upstream push → G1 M1-A.**
