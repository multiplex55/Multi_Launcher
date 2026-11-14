# Codex Kickstart — Execute Visual Productivity Studios for Multi Launcher

**Binding Goal:** `docs/plans/CODEX_GOAL_VISUAL_PRODUCTIVITY_STUDIOS.md`  
**Repo:** `multiplex55/Multi_Launcher`  
**Execution:** one six-category program (C01–C06); autonomous, sequential, single checkout.  
**Approved design choices:** hybrid visual/Mermaid **supported-subset** editor, and shared Markdown capabilities with **separate**, clean Notes/Markdown Studio interfaces.

## 0. Mandatory first action — safe chained-branch M0-A bootstrap

**Before ANY source modification, application build/test, or implementation checkpoint:**

1. Read repository `AGENTS.md`, the entire Goal above and this Kickstart. Inspect read-only Git state (`git status -sb`, branch, `HEAD`, remotes/upstream, dirty/untracked files) and prior completed Goal/category's recorded branch and **`final_tip_sha`**. The reviewed GitHub master commit is planning evidence only; **do not assume master is the correct execution parent**.
2. If **continuing an existing goal chain**, identify and verify the exact final committed tip of the immediately preceding category/goal; use it as **`parent_tip_sha`**, record `parent_branch`, and **create the next branch from that SHA**, not master/main, stale remote, or a common ancestor. Fetch a remote predecessor safely when necessary. Only the **first category of a genuinely new chain** may default to the up-to-date primary integration branch. If prior-tip identity is ambiguous or absent, report a bounded blocker rather than guessing.
3. Preserve user changes. **Never** force checkout, hard reset, stash, auto pull, merge, rebase, cherry-pick, force-push, discard, or stage unrelated work to manufacture a clean transition. If dirty changes make exact-parent branching unsafe, stop and report. No intermediate PR/merge is needed to advance a category.
4. Use **the one existing checkout only**. **NO GIT WORKTREES** (`git worktree add/move/prune` prohibited), NO auxiliary checkouts/clones for agents, and NO parallel write agents or independent branches off master. A planner/reviewer may analyze read-only concurrently when safe; one implementer owns writes at a time.
5. Create one **new uniquely named, idiomatic category branch** (`feat/visual-studios-01-quick-tools`, then 02-calculation-pad, 03-date-time-studio, 04-diagram-mermaid, 05-regex-visualization, 06-markdown-studio; choose unique variants as needed). The Goal and Kickstart **share the active category branch**; do not create two document branches or per-milestone branches.
6. On that branch, create its own **`Cxx/M0-A` baseline commit** as a minimal execution ledger update or intentional empty commit. Verify that the baseline's **direct parent is `parent_tip_sha`**; record `parent_branch`, `parent_tip_sha`, category branch and distinct `baseline_sha`. The predecessor's commit itself is NOT this category's baseline.
7. Attempt **one normal initial upstream push** (`git push -u origin HEAD` if appropriate). If rejected/unavailable, record the exact result, retain local commits and **continue**; no bypass or retry loop. Only after this M0-A process may implementation begin.
8. After a category's verified/final accepted commit, record its actual **`final_tip_sha`**. The immediately next category must start its own fresh branch **directly from that committed tip** and create its own direct-child M0-A baseline. **All checkpoints within one category remain on the same branch**. Do not merge, rebase, squash or cherry-pick intermediate branches to proceed. At program end only one cumulative integration PR may be proposed for approval—**never automatically merge**.

This entire M0-A bootstrap is **mandatory at C01 and re-executed independently at C02, C03, C04, C05 and C06**. Branch creation comes before implementation code, tests or builds. The companion Goal independently defines the same bootstrap and final-tip handoff; follow it exactly.

## 1. Ground each category in the current checkout

After M0-A, read only relevant source, callers, tests and persistence/UI patterns identified in the Goal. The planning snapshot was GitHub `master` `ae6660996ddf212817fc505ecd719956b32c7678`; actual source may have moved. Do not require an uploaded ZIP. Reconcile code changes that are already complete before adding new implementations; avoid resurrecting rejected historical candidate patches. Use existing abstractions, particularly dashboard widget settings, calculator/converter/date domains, RegexSession/explanations, Notes Markdown, and native independent viewport ownership.

Verified planning source anchors include `src/dashboard/widgets/quick_tools.rs`, `src/gui/convert_panel.rs`, `src/date_arithmetic.rs`, `src/gui/regex_tester_dialog.rs`, `src/regex_tester/explanation.rs`, `src/gui/note_panel.rs`, and `src/notes_markdown/`. Check them on the actual task branch rather than assuming their exact contents are unchanged.

Do not substitute placeholders, empty actions, mere parser types, or headless-only tests for an implemented/usable feature. Existing functionality must remain the default unless the Goal explicitly changes it.

## 2. Execute the six categories strictly sequentially

| Category | Required feature | Ordered checkpoints on ONE category branch |
|---|---|---|
| **C01** | Quick Tools internal preferred rows/columns and responsive, scroll-reachable tools | `M0-A` → `M1-A` settings/migration → `M1-B` layout/Jump/native acceptance → `FINAL01` |
| **C02** | Multi-line Calculation Pad and improved Converter UI | `M0-A` → `M2-A` domain → `M2-B` Pad UI/persistence → `M2-C` converter UX → `FINAL02` |
| **C03** | Date & Time Studio with calendar and time-zone pickers | `M0-A` → `M3-A` date/time domain → `M3-B` usable Studio/UI/native acceptance → `FINAL03` |
| **C04** | Floating hybrid visual/Mermaid Diagram Studio | `M0-A` → `M4-A` safe flowchart model/parser → `M4-B` independent visual editor → `M4-C` persistence/export/templates → `FINAL04` |
| **C05** | Regex Tester interactive visualization, not replacement | `M0-A` → `M5-A` AST span-backed visualization → `M5-B` existing tester UI/visual builder → `FINAL05` |
| **C06** | Markdown Studio plus unobtrusive Notes enhancements | `M0-A` → `M6-A` shared Markdown/export/Mermaid support → `M6-B` standalone Studio → `M6-C` contextual Notes tools → `FINAL06` |

**Dependencies:** C06 Mermaid previews use accepted C04 behavior; if C04 fails its native or safe source-preservation gates, C06 may deliver independent Markdown features but **must not claim Mermaid integration complete** or import unaccepted diagram code. Other categories may proceed after an honestly documented, committed, clean boundary if independent of a blocked category. Keep a truthful status matrix.

## 3. Agent responsibilities and single-writer discipline

Use whatever planner / implementer / reviewer / orchestrator roles **actually exist in this repository**, without inventing roles or tool access:

- **Planner:** brief read-only confirmation of category owner, scope, dependencies, invariants and focused tests. Stop once the implementer can proceed; no speculative full-repository research.
- **Implementer:** implement exactly the active checkpoint in the one checkout, sequentially. Choose idiomatic typed Rust, explicit state, safe file effects and coherent UI.
- **Reviewer:** independently inspect the resulting diff and relevant behavior after an appropriate checkpoint or category. Identify real defects, not an unrelated refactor wishlist.
- **Orchestrator:** own Git actions, test/build sequencing, bounded remediation, verification records, and parent-tip handoff; one writer only.

Avoid parallel builds or tests that contend for the same build artifacts/native resources. No agent may create a separate checkout, clone, worktree, side branch or competing source modification.

## 4. Minimum functional outcomes (read the Goal for details)

- **Quick Tools:** separate widget-internal preferred columns and visible rows from dashboard's *slot* position/span; responsive rows without lost actions; preserve legacy queries, `query` vs `queryexec`, duplicate identities and Jump focus/activation.
- **Calculation Pad:** editable multiline calculations with variables, dependency updates, bounds and errors; safe save/reopen and copy; improved resizable Converter with swap, favorites/search and accurate unit/base outputs. Existing `=` and `conv` remain unchanged.
- **Date Studio:** visual differences/offsets, named offline time-zone comparison with DST fold/gap explanations, bounded schedule previews, explicit copies and no unrequested alarm scheduling. Existing `date`/`timer`/`alarm` behavior remains.
- **Diagram Studio:** actually independent floating window with node/edge editing, text-to-visual and visual-to-text sync for a supported Mermaid **flowchart subset**, lossless protection of unsupported text, explicit native documents and PNG/SVG/Mermaid export. Do not falsely claim every Mermaid diagram syntax works.
- **Regex:** extend current Regex Tester with syntax structure backed by its existing Rust regex AST/explanations; visual builder edits the existing session. No fake engine trace or unsupported regex features.
- **Markdown:** standalone polished Markdown authoring + safe HTML export, Mermaid-fence supported-subset previews and contextual Notes tools. Notes remains a **separate clean editor**; preserve links, images, find/replace, edits, autosave and close prompts.

## 5. Verification, checkpoints, and bounded corrections

1. At each checkpoint run **the narrowest meaningful unit/integration tests and compilation check** from Goal F/H, then inspect the diff. Use selected `cargo nextest` filters/test targets and `fast-dev` profile where appropriate; the user's preference is a **100-second per-test execution upper bound**, not a blanket 100-second compilation deadline. Distinguish compilation time from test execution. Avoid full suite, repeated release builds and running Clippy every milestone.
2. For visible behavior, **build and launch the actual Windows application**, interact autonomously with the feature (including small sizes, keyboard/Jump, save/reopen and error paths), and capture screenshots/other relevant evidence. The independent Diagram viewport, hiding the launcher and editing the still-visible Studio are required native scenarios. Headless/egui frame tests cannot replace those acceptance gates.
3. If native automation is unsupported or an owned window cannot be observed, document `NOT_RUN`/`UNVERIFIED`/`BLOCKED`, not a fabricated PASS. Attempt at most an initial proof plus one cause-specific corrective verification cycle per confirmed failure boundary by default; if still blocked, preserve evidence and commit only proven safe work while leaving the category incomplete.
4. Check code behavior, not just compilation: output exists and reopens; unsupported Mermaid input is not silently deleted; variables update; DST cases disambiguate; regex visualization agrees with current regex engine; note links and undo remain correct; button visibility survives constrained geometry.
5. Before committing each coherent verified increment, examine `git status`, `git diff`, and `git diff --check`, avoid unrelated changes, commit with the specified `Cxx/Mx-Y` label, and attempt one best-effort push. Keep actual results, source SHAs and push states in the single current execution ledger. No huge archived test logs unless needed to diagnose a genuine blocker.
6. After source stabilizes, run the Goal's limited cross-category regressions and **end-only first-party Clippy** with `--no-deps`; do not rewrite vendor code to resolve warnings. Do not delete/ignore/weaken legitimate assertions merely to obtain green status.

## 6. Autonomy, stopping, and reporting

Operate autonomously. Make routine engineering choices after source inspection; do **not** ask the user to execute tests, capture screenshots, or approve each checkpoint. The user will review once after the final handoff. Ask only if a consequential requirement truly cannot be resolved or an action requires separate authorization.

**Do not** broaden scope to unfinished historical ZIP, Unicode, persistence, or timer/alarm recovery, a search architecture redesign, a workspace system, or a speculative full Mermaid renderer. The user approved only these six categories.

At each `FINALxx`, commit accepted state, inspect cleanliness, record `final_tip_sha` and make it the next category's **exact parent**. If the category is blocked, explicitly state what remains incomplete; never hide unaccepted code behind a success label. At `FINAL06` provide the Goal's complete final delivery report: per-category acceptance table, tests/native evidence, files, unresolved gaps, six direct-child Git ancestry proofs (`parent_branch`, `parent_tip_sha`, category branch, `baseline_sha`, checkpoints, `final_tip_sha`), push statuses, no-worktree/no-intermediate-merge confirmation, and one end-only manual review checklist.

**Stop at the authorized boundary.** Do not auto-merge the final cumulative branch. The **next separately approved Goal** must branch from this program's recorded **`final_tip_sha`**, NOT from master/main, when the sequence continues.
