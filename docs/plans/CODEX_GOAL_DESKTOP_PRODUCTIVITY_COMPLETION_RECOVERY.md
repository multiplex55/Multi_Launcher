# Codex Goal — Desktop Productivity Completion & Acceptance Recovery

**Goal ID:** `desktop-productivity-completion-recovery`  
**Companion Kickstart:** `docs/plans/CODEX_KICKSTART_DESKTOP_PRODUCTIVITY_COMPLETION_RECOVERY.md`  
**Status:** Authorized to implement after the previous execution has completed; its last recorded delivery is already frozen.  
**Required exact predecessor:** `feat/silent-screen-recorder` at `1852f264dae69056f79bc0c3b593c70331fa620f` (Desktop Productivity Expansion M17-B).  
**Scope:** Complete the unfinished, originally selected Desktop Productivity Expansion requirements (original IDs 1–46, 61, 88, 89 and 119), recover the original stopped implementation increments, and provide truthful native/user acceptance. Preserve all accepted functionality. **This is not another general feature-expansion goal.**

> **Explicit user authorization — READ FIRST:** The user authorizes Codex to **implement, compile, test, review, commit and push features even when its current Windows machine cannot perform their final Windows 11 or interactive GUI acceptance tests**. The user is willing to carry out **a single consolidated set of final manual verification checks** after Codex finishes the implementation. **Do not withhold code, abandon an implementable feature, or stop an otherwise independent milestone solely because the development machine is Windows 10, the screenshot/interaction observer is unavailable, an external draw.io reader is absent, or a particular monitor/DPI configuration cannot be reproduced.** Implement the supported-path functionality and its deterministic proof; clearly mark the remaining real-system check `USER_VALIDATION_PENDING`. **Never call an unperformed check passed.** Do not request intermediate user testing, screenshots, restarts, or confirmations. Finish independent work first, then deliver one precise final checklist for the user. This authorization renews the original feature-correction budgets as specified below; it does not relax safety or permit fabricated verification.

---

## A. Source of truth, inherited state, and non-rework

Read `AGENTS.md` and repository-specific instructions, then these tracked source documents:

1. `docs/plans/CODEX_GOAL_DESKTOP_PRODUCTIVITY_EXPANSION.md` — original C.1–C.7 requirements, E.4 selected-ID map, H.4 N1–N12 flows, and original 38 milestones.
2. `docs/plans/CODEX_KICKSTART_DESKTOP_PRODUCTIVITY_EXPANSION.md` — original execution constraints.
3. `docs/plans/execution/standalone/desktop-productivity-expansion/EXECUTION.md` and `INTEGRATION_STATUS.md` — accepted source and evidence for all seven category branches.
4. `docs/plans/execution/standalone/desktop-productivity-expansion/stalls/20261106T135856Z.md` — final stop.
5. The original focused stop reports: `20261105T035337Z.md` (C01/M1-B.2), `20261105T130929Z.md` (C03/M9-B.2), `20261105T154148Z.md` (C03/M10-A.1), `20261105T190244Z.md` (C03/M10-B.2), and `20261105T221741Z.md` (C04/M12-A.2).
6. Final C07 handoff at the exact SHA above, plus any available external receipts under `target/desktop-productivity-expansion/`.

**Inherited accomplishments; do not reimplement them:** exact integer/programmer grammar and unit arithmetic; source-preserving Calculation Pad; Mermaid and rich visual authoring foundations, transforms/connectors/assets, basic ungrouped draw.io, internal rich SVG/PNG encoder; Explorer read-only observation/ordered operation and versioned folder-list store; Folder Size Analyzer scanning/docked UI; File Lock Inspector read-only Restart Manager and docked UI; recorder WGC session, GIF/H.264 MP4 encoders, compact controls and optional Stop shortcut. The original final M17-A already corrected the visual `.mld` Save As late-collision path and the legacy `min` versus minutes expression ambiguity and passed 444 distinct cumulative focused tests (443 first pass + 1 corrected fixture). **Verify the regression tests remain, but do not open a redundant repair campaign for issues already fixed.**

**Inherited truth:** The prior execution ended `PARTIAL_BLOCKED_FROZEN`; it reached M17-B but did **not** satisfy the original full goal. All native N1–N12 remain unverified as end-to-end flows, even though narrower real Windows tests passed. Preserve existing results and do not count them as full N-gate verification. The previous correction budgets are spent; this **new user-authorized recovery goal explicitly grants new, bounded budgets** for the named original failures.

### A.1 Preserved patches (conditionally available)

The prior ignored artifacts may be present at:

- `target/desktop-productivity-expansion/stalls/M9-B.2-preserved-source.patch` — SHA-256 `93F5E267100D22CF83B9BB26784A70470D1C824C0C5840D190899555C07B53FE`.
- `target/desktop-productivity-expansion/stalls/M10-A.1-preserved-source.patch` — SHA-256 `DAA2CC401B7F5E57CB8DFF9034F10B4C0EDD3153927BF29EBAE95C434669DA63`.
- `target/desktop-productivity-expansion/stalls/M10-B.2-preserved-source.patch` — SHA-256 `747CB18B2CF569922F7C607A0ADCA4513721EE5F17881FACFABC77E41AA44929`.
- C01/M1-B.2 preserved patch location/hash: **read the exact C01 stop report and receipt; do not guess either.**

These are *unaccepted historical candidates*, not automatically safe code. If present, verify actual file hashes and applicability, inspect and extract only relevant changes. Do not wholesale apply a failed patch, overwrite newer accepted code, or bypass a failing test by disabling it. If artifacts are absent, reconstruct from current accepted source plus tracked stop evidence; **missing ignored artifacts are not grounds to halt the goal**.

## B. Git/branch execution contract — mandatory

### B.1 Exact start

1. Inspect `git status -sb`, `git branch -vv`, `git rev-parse HEAD`, remotes, index/untracked paths and in-progress Git operations; verify predecessor SHA exists, belongs to the intended repository, and is the final pushed C07 tip. Read the previous final receipt. A read-only `git fetch origin master` is permitted to refresh remote awareness but must **not** replace the recorded parent with `master`.
2. Protect user work. Do not reset, force-checkout, stash, force-push, rebase, merge, cherry-pick, clean, overwrite, or stage unrelated files. Resolve actual dirty-state conflicts safely and record genuine blockers without inventing another parent.
3. In the **existing single checkout**, create the first recovery branch directly from `1852f264dae69056f79bc0c3b593c70331fa620f`. Use a fresh unique name such as `fix/productivity-host-core-recovery`.
4. Before source edits/builds, create a real **R01/M0-A** baseline commit updating this recovery execution ledger. Verify the baseline commit's parent is the exact predecessor. Attempt `git push -u origin HEAD` once, non-force. Push failure is recorded but does not block safe local work.
5. Every following category gets its own fresh chained branch from the exact **final committed tip of the previous recovery category**, including reporting commits. Establish a new category M0-A and push before application edits. Never fork siblings from `master` or from the original delivery tip; never use Git worktrees, helper clones, agent-only branches, or intermediate category merges.
6. Use one source writer and one Git owner at a time. Read-only planning/review agents may work in parallel without editing the checkout. Keep small reviewable checkpoint commits and best-effort normal pushes after every checkpoint; if push fails, preserve local commits and continue.
7. Never automatically merge into `master`. Prepare a final cumulative PR/readiness report if useful, but obtain the user's final review/merge authorization. Record the **actual final SHA after the last tracked reporting commit** in an ignored/external receipt and the final response; do not self-referentially amend it into the same commit.

### B.2 Planned sequential category branches

| Recovery category | Suggested branch stem | Direct parent |
| --- | --- | --- |
| R01 — Host core recovery | `fix/productivity-host-core-recovery` | Exact original final tip `1852f264...` |
| R02 — Required host migrations | `feat/productivity-host-migrations` | R01 final tip |
| R03 — Programmer tools completion | `feat/productivity-programmer-inspector` | R02 final tip |
| R04 — Diagram completion | `feat/productivity-diagram-completion` | R03 final tip |
| R05 — Explorer/utility completion | `feat/productivity-explorer-utility-completion` | R04 final tip |
| R06 — Acceptance and handoff | `test/productivity-native-acceptance-closeout` | R05 final tip |

Names are suggestions, not authority to reuse an existing branch. Pick an idiomatic fresh unique suffix if necessary. A blocked native check must not break this branch chain.

## C. Verification and continuation policy — user-approved change

Separate five independent dimensions for every milestone/selected ID: **implementation, automated tests, local native evidence, user Windows 11 validation, Git delivery**.

Statuses:

- `IMPLEMENTED_VERIFIED_LOCALLY`: intended code is present; focused tests and all locally available checks pass.
- `IMPLEMENTED_USER_VALIDATION_PENDING`: intended code is present, builds, focused tests pass, but the final native Windows 11/GUI/DPI/observer/independent-reader proof requires the user or another capable host. **This is an authorized deliverable, not a reason to stop coding.**
- `VERIFIED_BY_USER`: only after the user actually reports a passing final manual check; do not pre-label.
- `IMPLEMENTATION_BLOCKED`: known correctness failure, unsafe native behavior, missing required implementation, persistent failed test, compile error, or unsupported implementation design. Requires actual code investigation; **do not misclassify as only native-pending**.
- `NOT_STARTED`, `IN_PROGRESS`, and `DEFERRED_NONBLOCKING` as appropriate. Do not label an omitted required feature `COMPLETE`.

**Mandatory operating rule:** If a Windows 11-only observation or the inherited interactive observer is unavailable, deliver the maximum safe, reviewed, tested implementation; add deterministic fakes/contract tests, mockable ports, compile checks, feature-capability messages, and a final user test with expected results. Then proceed to the next independent checkpoint. **Do not stop, freeze all categories, repeatedly reprobe an unchanged host, or ask the user to run a test while Codex is mid-goal.** Do not simulate a native positive, suppress a failing functional test, or claim real Explorer tabs work without observing them.

A native test being unavailable is categorically different from an actual failed product behavior: actual failures must be fixed or precisely marked `IMPLEMENTATION_BLOCKED`; the unavailability of a UI observer alone never justifies abandoning implementable code. For Explorer, develop a supported Windows 11 path carefully: verify identities and ownership; choose documented Shell/UIA capabilities, and allow guarded input fallback only after binding a specific Explorer window and expected target. Never blindly send keys to the foreground, rely on private controls without evidence, or open unrelated windows while reporting tab success. If exact native selected-tab attribution cannot be substantiated on this host, keep the operation fail-closed and mark it `USER_VALIDATION_PENDING` with a reproducible Windows 11 diagnostic/test path; **do not ship an unsafe unconditional success path**.

### C.1 Correction budgets and stopping rules

This new goal grants a **fresh initial implementation/test run plus up to three focused correction/retest cycles per independently documented root cause** for R01 host click boundary and the three named C03 failed GUI paths. New defects receive the same bounded cycle. Source/harness diagnostic compilation failures that execute no tests are recorded separately; neither zero-match filters nor compilation failures count as passing tests. A materially distinct root cause may have its own recorded budget after review, not merely a renamed checkpoint. Independent review may authorize a narrowly bounded additional diagnostic attempt with concrete new evidence, and must record why; do not enter an unbounded loop.

When a real feature correctness budget is exhausted: retain accepted source, archive candidate safely, write a source-grounded stop report identifying the failure and the smallest unblock, continue any independent work, and deliver the unimplemented remainder as **not complete**. Do not let a repeated environment probe consume a feature correctness budget. Do not silently skip originally selected requirements because the schedule is long.

For each checkpoint, use the narrowest real test selector (nonzero `cargo nextest list`), one corresponding run, exact failing-test reruns, production checks where test cfg differs, scoped formatter/diff review, and independent source review. Reuse cached `fast-dev` and pinned dependencies; no broad profile/TOML changes, vendored-crate lint cleanup, or repeated full-suite rebuilds. At final, run a cumulative affected-test union and first-party Clippy; classify known unrelated warnings separately. **444/444 was the previous baseline, not a claim that new recovery code passes without rerunning its affected cases.**

### C.2 Codex/local vs user validation responsibilities

**Codex owns:** all source coding, interface design, tests, safe Windows API integration, compiling, application startup where the environment permits, test fixtures, reports, artifact inspection, worker cleanup, relevant local native probes, source review, commit/push, and a complete end-of-goal manual script. Codex should execute native tests itself *where supported*; it should not transfer ordinary testing work to the user.

**User agrees to do at final handoff only:** a **single consolidated optional manual acceptance session** on their Windows 11 machine for the N-flows that Codex could not run or observe. Codex supplies exact menu/query action, disposable test data setup, expected outcome, how to record PASS/FAIL/UNRUN, and what diagnostics/screenshots to send back (redacted). Missing user results at handoff are `USER_VALIDATION_PENDING`, not a coding blocker or a fabricated success. If the user later reports a failing native test, that is new actionable evidence for a subsequent focused fix; do not assert ahead of time that it passed.

## D. Detailed recovery milestones and acceptance

### R01 — Repair the shared Dock / Pop Out host foundation

#### R01/M0-A — Exact branch and evidence baseline

- Create first fresh child from exact original final tip, commit and push baseline ledger under `docs/plans/execution/standalone/desktop-productivity-completion-recovery/EXECUTION.md`.
- Reconcile all original 38 milestone statuses, 50 selected IDs, C.2 surface inventory, N1–N12, and final outstanding artifacts. Source evidence is stronger than older summaries.
- Confirm the two already-fixed M17-A regressions remain protected; do not reimplement unless a focused regression fails.

#### R01/M1-A — Diagnose and repair M1-B.2 click-path failures

- Analyze C01 stalled 52/54 focused cases: the docked Diagram Studio **Pop Out** click and Diagram Viewer **open** click did not produce expected transitions. Locate exact egui event, clip/visibility, viewport owner, accepted-pass and focus gates; identify whether the harness, real UI, or both are wrong. A test-only adjustment is insufficient unless the actual user click pathway is demonstrated.
- Restore or safely reconstruct the intended shared control wiring from accepted source; preserve one session/document identity, dirty/history/selection/worker state, already-working native viewer semantics, generation fencing and pending prompts. Keep click target visible and enabled at realistic viewports and text scaling; avoid fake actions/directly changing internal state in tests as a substitute for interaction.
- Add host-driven click tests, discarded/retired pass checks, hidden-root and Dock/Pop Out transitions. Test actual native/embedded viewport logic where available. Independent review of pointer, focus, and close ownership. M1-B.1 stable geometry storage must not regress.
- **Acceptance:** both original focused interaction failures and related host regression tests pass, with source-level proof that the wired user actions change host state correctly. Local native limitations can remain `USER_VALIDATION_PENDING`.

#### R01/M1-B — Complete shared host geometry, focus, and presentation contract

- Restore ordinary Dock/Pop Out labels, Dock bringing launcher/root into focus, native child independence with launcher hidden, correct root/child keyboard ownership, Jump Mode and Escape behavior, owned file pickers and prompts, persistence of sensible positions/sizes across DPI/monitor changes including negative coordinates and clamped views, and exactly-once retained effects.
- No new workspace/tiling model, no forced restoration at startup, no duplicate global hotkeys or unconditional background repaint. Cover close/cancel while hosting changes and monitor removal; preserve named dialog defaults.
- Create a reusable explicit checklist/test helper for each subsequent migrated host, without considering a generic test sufficient by itself.

### R02 — Migrate every originally required ordinary host

R02 implements original C01 M2–M4, plus the mandatory common hosting of the newly created ordinary utility surfaces. **Do not summarize 29 named dialogs as covered because a representative worked.** Record a separate matrix row for every actual host/child surface from C.2 and the original ledger: owner, migration, button visibility, state/worker test, native check, and status.

#### R02/M2-A — Documents and authoring hosts

- Quick Notes; each independent note editor; Markdown Studio; MkMacro Editor. Preserve each logical session, note undo/dirty/conflict and attachment child prompts; MkMacro execution and hotkey assignment must not trigger again during migration. Respect genuine multiple note instances.

#### R02/M2-B — Settings, Dashboard and Mouse hosts

- Main Settings; Dashboard Settings/editor; Mouse Settings; Mouse Gestures and Gesture Settings. Preserve pending save/cancel, recording sessions, pointer suppression and Root Jump/hotkey focus. No automatic settings commit on docking.

#### R02/M2-C — File, clipboard, developer and time hosts

- File Search, Diff, Batch Rename, Search & Replace; Clipboard History, Snippets, Clipboard Modify, Paste Sequences; JSON, XML, Regex, Table Utility; Timer/alarm authoring; Calendar root/event editor/event detail; Date & Time Studio; Image Compare, Duplicate Finder and ZIP Extractor.
- For each, test actual open → mutate representative state → Pop Out → operate while root hidden → Dock → close/cancel and exactly-once side effects. For running workflows, preserve worker/cancellation behavior. Paste Sequences must not start a second run, hijack shortcuts, or drop the clipboard just because its host moves.
- Split checkpoints into small slices (e.g., file, clipboard, developer, time/image) with commits after each; stay on the same R02 branch until its final reporting commit.

#### R02/M2-D — Reference and new ordinary hosts

- Confirm Calculation Pad and Diagram Studio/editor/viewer share correct hosting *without creating a second document/session*. Finish common host integration for **Folder Size Analyzer, File Lock Inspector, and Silent Recorder setup**. The dedicated transient rectangle-selection and compact recording overlay are specialized and not ordinary dockable surfaces.
- Host moves must not restart scans, reset inspected-file identity, cause capture ownership loss or duplicate hotkey registration. Tests for each new UI.
- **Acceptance:** every required surface has concrete wiring and one owner-level state/interaction test; any unavailable real-native observation is transparently `USER_VALIDATION_PENDING`. No missing surface can be counted as migrated.

### R03 — Complete Calculation Pad Programmer Inspector and comparison

#### R03/M3-A — Independent inspector owner and bit editing model

- Follow a selected **exact integer result**, retain width/sign (required 8/16/32/64; keep optional 128 as existing feature), support decimal/binary/octal/hex simultaneously, typed signed-vs-unsigned interpretation, and **numbered, clickable bits from least significant bit 0**. A bit toggle edits an inspector-only working copy, not the original source or cached result. Support optional labels for individual bits or ranges. Do not silently overwrite a dirty working copy when the selected source/result changes: show stale/refresh controls and make refresh explicit.
- Use existing `src/calculation_pad/{integer,programmer,document,session}.rs` engine for all semantics. Test e.g. decimal `123 >> 12`, default `u64`, signed right-shift, width masking and bit toggles; do not convert through `f64`. Include wide values above 2^53.

#### R03/M3-B — Separate dockable dialog, formatting, Copy and explicit Insert

- Build the **separate** dockable/native inspector with stable source selection, responsive bit grid and copy options (base prefixes, width padding, grouping, plain digits). Bounded settings persistence for preferences; inspector edits are not implicit document save. Explicit Copy writes only the requested formatted representation.
- Explicit Insert must validate active document identity, revision, selection/insertion target, and pending dirty decisions; refuse stale result/retired host and preserve undo as one step. The source remains text-first; don't permanently shrink the pad into a programmer panel.
- Verify real click action from UI tests and command discoverability; native pop-out final proof may be user validation pending.

#### R03/M3-C — Side-by-side comparison and acceptance

- Compare two expressions or selected results in immutable current reference context using the **same existing engine**; show both values and representations, explicit type/width/sign/unit mismatches, and differences without changing pad source. Preserve scalar `^`/`/`/`mod`/`min` behavior and unit arithmetic.
- Update in-app examples/reference and focused signed/unsigned/decimal/bit/Copy/Insert/comparison tests. Ensure original IDs 5, 6, 8, 11 and residual 12 are traceable. Final C02 N5 manual validation is at end, not a mid-goal stop.

### R04 — Complete Visual Diagram Studio

#### R04/M4-A — Containers, grouping and swimlanes (M9-B.2)

- Diagnose the preserved 15/16 GUI fixture: drag/multi-select pointer located outside its canvas mapping before modifier input. Investigate the actual canvas mapping, clipping, hit testing and coordinate transforms, not just fixture expectations.
- Implement persistent containment/membership, nested transformations and cycle prevention, group/ungroup, swimlane editing, descendant movement, undo/redo, resize/rotate, hit testing and export/persistence agreement. Retain accepted connectors, assets, layers and source authority. Do not flatten unsupported groups silently.
- Tests: real visible drag/selection interactions; move/resize/rotate with descendants, connector anchors, negative positions/zoom, save/reopen and deterministic error paths.

#### R04/M4-B — Five working visual starter templates (M10-A.1)

- Diagnose disabled `visual_authoring_template_flowchart` menu item (14/15 original tests). Correct the actual enablement/owned popup gating; do not make a disabled button pass merely by direct invocation.
- Wire all five visual starters: flowchart, architecture, mind map, sequence-style and dependencies. Preserve existing Mermaid templates. Real user click must create a usable visual document through dirty/current-document confirmation, undo and safe cancel. Preserve independently accepted Mermaid assistance and conversion.

#### R04/M4-C — Studio rich SVG/PNG export workflow (M10-B.2)

- Reuse accepted `DiagramVisualSnapshot` and SVG/PNG encoder. Diagnose the original PNG test: scale ×2 option was clipped below the visible content (`[389.7 53.0]–[412.4 71.0]` outside `[0.0 8.0]–[1180.0 58.8]`) before click. Fix actual layout, scroll/overlay/clip and owner behavior; do not lower the test to avoid the control.
- Implement reachable Export PNG and Export SVG, scale, transparent/chosen background, source/current revision, separate viewer camera, background worker lifecycle, cancellation and safe atomic no-clobber destination publication. Preserve current `.mld` Save As race fix and existing Mermaid export. Test real menu interaction, decoded output, alpha/z-order, late destination collision and stale document/picker, and production cfg compile.

#### R04/M4-D — draw.io grouped subset and complete Diagram acceptance

- Extend accepted **uncompressed, one-page** draw.io subset to supported group/container/swimlane representation alongside primitives, labels, styles, layers and connectors, with explicit pre-import/export reporting for nonrepresentable features. Compressed pages remain optional; multipage input may be explicitly rejected rather than silently discard pages. No active XML/HTML execution or network resources.
- Verify native file preservation, independent parser/reader where locally available, and current diagram reimport fidelity for the supported subset; if no independent reader is installed, provide a **user final check** with a small disposable `.drawio` fixture.
- Verify old Mermaid `.mld` compatibility, current-source snapshots, dirty/save/discard decisions, editable visual scene, assets, containers, templates, undo/redo, recent-document library and exports. N6/N7 full native proof can remain `USER_VALIDATION_PENDING`.

### R05 — Complete Explorer Tabs and new utility integration

#### R05/M5-A — Safe Windows 11 Explorer tab backend

- Work from accepted `src/explorer_tabs/{operation,identity,windows,folder_lists}.rs`; keep the exact target-authority and per-tab identity distinction. **Implement the supported Windows 11 path even if the development host is Windows 10.** Guard availability by OS/build and observable capability; use observed HWND, Shell view and per-tab UIA identity/selected state where possible. Bind each requested folder and ordinal to the intended *new* Explorer window; do not infer it from the launch PID or a raw shared HWND alone.
- Support ordered folder paths including spaces, Unicode, intentional duplicates and documented UNC paths; bound job/cancellation and observe partial verified prefixes, failed/unknown states. Never silently report five tabs from five independent windows. Never alter a pre-existing unrelated Explorer window.
- If the Windows 11 native selected-tab/owning-view behavior is impossible to validate on the current host, **write the best safe, reviewable implementation and deterministic attribution tests**, record specific remaining uncertainty, and ship feature-gated fail-closed behavior where identity is unproven. **Do not halt the rest of R05/R06.** The user's Windows 11 N8 test is explicitly authorized at final handoff. If a safe actual tab mutation cannot yet be implemented without host evidence, label that **specific code capability** `IMPLEMENTATION_BLOCKED` rather than pretending `USER_VALIDATION_PENDING` covers missing code.

#### R05/M5-B — Saved-list editor, launcher command, progress and cancellation

- Turn the accepted versioned folder-list store into an ordinary dockable list editor: add/remove/reorder/name/path validation, durable safe persistence, immutable run snapshot. Add a discoverable typed launcher command and action-catalog/hotkey integration without inventing a conflicting global query prefix or changing MkMacro single-folder semantics.
- Provide progress, Stop/Cancel, clear per-folder outcome, unsupported-platform explanation, repeat invocation behavior and recoverable partial result. Verify a real launcher action through deterministic tests and call-chain source review; only final positive Windows 11 same-window tabs wait for user.

#### R05/M5-C — Accepted new utilities and recorder regression closure

- Review Folder Size Analyzer ID88, File Lock Inspector ID89, Silent Recorder ID119 user-facing commands and shared hosting from R02. Exercise scan/refresh/cancel and File Search handoffs; guarded read-only Restart Manager, release/refresh; capture target/region/window/display, compact Stop, GIF/MP4, cleanup and still-image/Pinboard coexistence.
- Do not redo accepted backend development unless a new regression is proven. Add relevant owner/UI wiring, release/retained-result and cross-feature tests. Native N9–N12 as locally possible; pending Windows 11/monitor/DPI or observer checks go on final user list.

### R06 — Cumulative acceptance, user test handoff and final tip

#### R06/M6-A — Consolidated regression and feature audit

- Reconcile **all 50 selected IDs**, all original 38 milestones, **every required ordinary C.2 host** and N1–N12 in an evidence matrix. Do not let a passed generic host test replace per-host wiring. Distinguish committed code, checked state, actual native proof and unresolved user validation.
- Run affected focused test union with exact nonzero filter, then relevant production lib/bin build and task-related Clippy/format/diff checks. Measure real accepted checks, not imaginary pass percentages. Inspect accidental behavior changes across chained branches. Fix regressions with recorded focused budgets; keep unrelated historical warnings separate.

#### R06/M6-B — Final user-validation package, one time only

- Create `docs/plans/execution/standalone/desktop-productivity-completion-recovery/MANUAL_WINDOWS_11_ACCEPTANCE.md` with **one consolidated, ordered checklist of N1–N12**, exact application run instructions and clean disposable fixtures. The user has explicitly volunteered to perform the native cases Codex cannot complete. **Do not ask the user to do these before implementation/verification has been finished.**
- For each flow, include prerequisites, query/command to run, click/keystroke sequence, expected visible outcome, expected data/worker state, PASS/FAIL/NOT_RUN checkbox, requested sanitized evidence/logs, and safe cleanup. Prioritize a compact high-value smoke path before the optional long checks. Include Windows 11 Explorer-tab build requirement, isolated profile, non-primary/DPI only if available, and external draw.io optional installed-reader check. Do not request private clipboard/notes/files, credentials or unredacted desktop screenshots.
- Codex performs all tests locally achievable. `USER_VALIDATION_PENDING` remains pending until the user reports results; do not claim N1–N12 passed. The user may run the list **after** final source handoff and respond with failures, which become a separate targeted follow-up fix rather than a mid-goal interruption.

#### R06/M6-C — Final reporting, push and handoff

- Commit final implementation/docs, independently review requirement matrix and branch chain; record branch/parent/M0-A/checkpoints/push and actual post-report final tip in external receipt. Provide user-facing feature summary, exact automated/native evidence, explicit user-pending list, build/run steps, and one consolidated verification request. No automatic merge to master.
- Possible honest terminal statuses: `IMPLEMENTATION_COMPLETE_USER_VALIDATION_PENDING` (all intended features coded/tests pass, some native checks genuinely unavailable); `COMPLETE_VERIFIED` (all mandated native checks actually performed successfully); `PARTIAL_IMPLEMENTATION_BLOCKED` (known missing/unsafe/failed code remains). **Do not make pending user tests a reason to refuse to produce a final branch/summary.**

## E. Original acceptance coverage trace

The original 38 milestones and original 50 selected requirements remain authoritative; this recovery maps to them without changing scope:

| Original IDs / milestones | Recovery owner | Required closure |
| --- | --- | --- |
| 24–26; M1-A/M1-B | R01, R02 | Dock/pop-out correctness for every ordinary host |
| 13–15; M2-A/M2-B | R02 | Notes, Markdown, MkMacro |
| 16–18; M3-A/M3-B | R02 | Main/Dashboard, Mouse/Gestures |
| 19–23; M4-A…M4-D | R02 | File, clipboard, developer, time and file/image utilities |
| 1–4, 7, 10; M5-A/M5-B | R03 regression | Preserve accepted integer/calculator semantics |
| 5–6, 8, 11; M6-A/M6-B | R03 | Standalone inspector bit editing/copy/insert and comparison |
| 9, 12; M7-A/M7-B | R03 regression/documentation | Preserve dimensional arithmetic; complete user reference |
| 27–28, 34–35; M8-A/M8-B | R04 regression | Preserve dual authoring and rich model |
| 29–33, 36–37, 39–41; M9-A/M9-B | R04 | Group/container/swimlane and accepted editor behavior |
| 38, 42–43, 45–46; M10-A/M10-B | R04 | Visual starters, export controls, conversion/recent behavior |
| 44; M11-A/M11-B | R04 | Supported grouped draw.io subset and honest reports |
| 61; M12-A/M12-B | R05 | Targeted Explorer tabs and user-facing saved-list editor |
| 88; M13-A/M13-B | R02/R05 | Completed analyzer UI and common hosting |
| 89; M14-A/M14-B | R02/R05 | Completed file-lock UI and common hosting |
| 119; M15-A…M16-B | R02/R05 | Recorder user flow, common setup host and acceptance |
| All; M17-A/M17-B | R06 | Cumulative evidence and exact final SHA |

No new workspace feature, unrelated redesign, internet-backed content, audio recording, admin driver, process-killing file unlocking, force-merge, or new macro DSL is authorized. Preserve backward compatibility and current command prefixes unless existing requirements demand an explicit addition.

## F. Definition of Done and truthful final response

Implementation can be accepted for delivery **before** the user performs Windows 11/manual verification, provided it compiles, passes appropriate focused checks, has no known unsafe correctness defect, and the user-required native flow is clearly documented as `USER_VALIDATION_PENDING`.

The follow-up goal reaches **implementation handoff** when: (1) all implementable original selected behaviors have real user-facing integration; (2) every required host is individually wired and tested; (3) the Inspector and Diagram Studio omitted work is restored and verified via focused tests; (4) Explorer lists, launcher and safe supported-target action exist with exact capability/attribution safeguards; (5) available native tests and cumulative tests pass; (6) the final manual package is ready; and (7) all task changes are committed on the verified linear Git chain. A genuinely missing/unsafe native implementation remains `PARTIAL_IMPLEMENTATION_BLOCKED` with its exact gap, even if the other work is handed off.

Only label **full native completion** after N1–N12 are actually observed and pass, whether by Codex on a supported host or by explicit user test feedback. End-of-goal manual testing is a planned user contribution, not a reason to stall implementation, and never a reason to invent evidence.

The final answer must show the exact current branch/final SHA, category matrix, original ID and host matrix links, automated test counts/results, locally verified native checks, required user verification steps, remaining genuine implementation failures (if any), and the exact next safe action. **The user should need to perform no more than one consolidated final manual review session.**
