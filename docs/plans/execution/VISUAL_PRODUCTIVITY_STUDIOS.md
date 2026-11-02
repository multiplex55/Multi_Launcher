# Visual Productivity Studios execution

Binding scope: [Goal](../CODEX_GOAL_VISUAL_PRODUCTIVITY_STUDIOS.md) and [Kickstart](../CODEX_KICKSTART_VISUAL_PRODUCTIVITY_STUDIOS.md).

## Bootstrap / C01

- Single existing checkout: `G:\Repos\rust\Multi_Launcher`; clean before transition.
- `parent_branch`: `chore/native-acceptance-closeout-20261101-r01`.
- `parent_tip_sha`: `e929b0fc801c436f295ea74416969e9ba22c5dfa` (local/upstream equal; closing C05/M5-C commit matches predecessor handoff).
- Category branch: `feat/visual-studios-01-quick-tools` created directly at that SHA.
- The previous checkout was master `26602cd09c50c15c48cf587dba200989b200888a`. Its application sources match the predecessor. Only approved Goal/Kickstart copied into this baseline; later documentation relocation is not imported.
- C01/M0-A baseline: `313535c3ee7efbc52f504300ceb4d04a01901a1b`; `git rev-parse HEAD^` equals `parent_tip_sha` above. Initial `git push -u origin HEAD` succeeded and established upstream. Baseline preserves the approved documents verbatim, including existing Markdown hard-break whitespace reported by diff --check.
- No application edits/build/tests before bootstrap. No worktree, auxiliary clone, merge, rebase, cherry-pick or competing writer.

## Status

| Category | State | Accepted / remaining |
| --- | --- | --- |
| C01 Quick Tools | PARTIAL | M0-A and M1-A complete; M1-B source/tests/build verified, native gate BLOCKED |
| C02 Calculation Pad / Converter | pending | All checkpoints pending |
| C03 Date & Time | pending | All checkpoints pending |
| C04 Diagram / Mermaid | pending | All checkpoints pending |
| C05 Regex | pending | All checkpoints pending |
| C06 Markdown / Notes | pending | All checkpoints pending; Mermaid depends on accepted C04 |

## Execution rules

Follow Goal F checkpoints sequentially, with one writer, focused Nextest verification (100-second per-test execution bound), scoped GUI builds and real Windows interaction evidence. Native gates stay unverified until observed. Parent owns Git and command sequencing. Record checkpoint SHAs, checks, push outcomes and exact category final tips here; each following branch and baseline directly descend from that final tip. No automatic merge.

## C01/M1-A accepted

Read-only planner confirmed Quick Tools owns internal settings; registry derives typed defaults. Sole implementer changed `quick_tools.rs` and one existing registry defaults test. Initial implementation uses columns 1–8/default 3 and rows 1–12/default 2; query/action semantics unchanged. Focused inventory preparation started with `cargo nextest list --locked --offline --cargo-profile fast-dev -p multi_launcher --lib` and a Quick Tools module filter, explicit `target/visual-studios/nextest.toml` enforcing 100-second per-test execution termination. Compilation is separate from execution. Initial filter must be corrected to include existing `registry_defaults_are_deterministic_and_include_expected_widgets` before execution.

Independent review identified one P2 before acceptance: signed negative preference rejects whole config, allowing existing editor fallback to replace saved queries. Bounded correction plus editor-preservation regression required before commit; no tests/native acceptance claimed yet. Baseline documents retained their original Markdown hard-break spaces intentionally.

Initial inventory preparation completed successfully in 4m35s (exit 0), discovering 15 Quick Tools cases. No test execution was performed on this known-defective candidate. Existing warnings are in untouched vendor, table core, settings editor, table session, GUI render and timer test code; none originated in the changed Quick Tools/registry files. Logs: ignored `target/visual-studios/c01-m1a-{prepare.log,inventory.txt}`. Sole writer resumed only after compilation retired.

Native preparation: computer-use skill read and `@oai/sky` initialized; app inventory works. Isolated profile under ignored `target/visual-studios/native-c01` has next/cancel shortcuts explicitly null, empty index/plugin paths, only settings plugin enabled and one Clip Quick Tools slot. No native application launched yet.

Correction accepts signed/unsigned integers and clamps independently; actual settings UI regression proves negative preferences preserve legacy manual/automatic queries. Independent focused re-review confirmed original finding resolved, no remaining finding. Corrected inventory compilation succeeded in 1m41s, 18 cases selected (17 Quick Tools + existing registry defaults). Exact command: `cargo nextest run --locked --offline --cargo-profile fast-dev -p multi_launcher --lib -E 'test(dashboard::widgets::quick_tools::tests::) | test(registry_defaults_are_deterministic_and_include_expected_widgets)' --config-file target/visual-studios/nextest.toml --no-fail-fast`. Result **18/18 passed**, Nextest execution 0.186s, preparation check 0.96s, total command 2.35s; all far below 100s per-test limit. Full logs/inventory in ignored `target/visual-studios/c01-m1a-{corrected-prepare.log,corrected-inventory.txt,tests.log}`. Diff inspected and diff --check passed; only Quick Tools, registry test and this ledger changed. Native rendering remains M1-B, not accepted here.

M1-A commit `8ad35556c13cb9375fa2f037d815f6092a4c14c5` pushed successfully, clean before dependent source assignment. M1-B sole writer assigned bounded responsive rendering, contained scrolling and focused Quick Tools/Dashboard Clip integration tests. No change to other widgets' overflow behavior intended. Parent retains Cargo/Git/native ownership.

## C01/M1-B verified source, native BLOCKED

Initial source adds clip-bounded adaptive columns, row/height-limited vertical scrolling, truncated labels with hover and stable action identity container. Real dashboard Clip integration and geometry/resize/scaled-text tests added. Initial focused inventory compilation failed (no tests executed): `Ui.clone()` resolved via deref to `Context` at two identity arguments (E0308); scale test used unavailable `Context::style/set_style` rather than current themed APIs (E0599). Compiler output fully drained to `target/visual-studios/c01-m1b-prepare.log`, Cargo session retired before correction assignment.

Independent reviewer found one P2: Tab can focus clipped buttons without scrolling them into view. First consolidated correction assigned for those compiler roots and widget-owned focus reveal with actual Tab traversal regression. No shared dashboard rendering changes or native acceptance yet.

First correction independently reviewed clean: precomputed stable action IDs, current themed test API and `gained_focus` scroll reveal with real Tab test. Corrected inventory compilation passed 1m41s. Focused runtime command uses Quick Tools module plus exact `quick_tools_clip_slot_keeps_later_actions_scrollable_and_jump_actionable` and `jump_mode_designer_ownership_quick_tools_rows_follow_move_and_retire_on_remove`, same locked/offline fast-dev/lib/100s config. Runtime 0.974s: **17/21 passed, 4 failed**. Three new fixtures requested Jump scope outside active frame collection; existing two-widget identity case observed only 3/6 targets after first bounded grid consumed horizontal available width. Exact logs `target/visual-studios/c01-m1b-tests.log`. First cause-specific runtime fixture correction assigned; assertions retained, production-vs-fixture cause checked by independent reviewer. No application build/native claim yet.

Those fixture causes were independently confirmed and corrected without production edits: capture scopes in active frame; allocate each identity-test widget a bounded rect, preserving all six-target/distinct-response assertions. Preparation 1m24s passed; runtime **19/21 passed** in 1.008s (`c01-m1b-fixture-{prepare,tests}.log`). Two newly reached behavior assertions failed: wheel test inspected same-frame pre-scroll geometry and used unobserved pointer coordinates; Tab test stopped on partial last-row visibility before that row owned focus. Reviewer confirmed both test timing/termination causes from egui source. First corrections for these distinct boundaries derive wheel point from visible target and inspect subsequent frames; Tab continues until full visibility AND native focus, retaining final assertions. Only those two cases are selected for the next run; no production edits or weakened assertions in either fixture correction.

Narrow two-case rerun: Clip scroll/Jump activation passed (0.069s), Tab full-visibility/focus failed, Nextest 0.086s; preparation 1m19s. Further read-only diagnosis confirmed a distinct simulation timing cause: `Response::scroll_to_me` uses the global 0.1–0.3s scroll animation while this fixture advanced focus every two frames. Reviewer approved deterministic test-only `ScrollAnimation::none()` plus two geometry-followup frames, with full-visibility/focus assertions retained and production defaults unchanged. Exact Tab-only rerun passed (0.056s test, 0.067s Nextest; compilation 1m18s). Evidence `c01-m1b-scroll-{prepare,tests}.log`, `c01-m1b-tab-tests.log`. All 21 selected behaviors now have passing evidence across narrow runs on unchanged production rendering; this is not a claim that one aggregate 21-case run passed. One Windows app build started: `cargo build --locked --offline --profile fast-dev -p multi_launcher --bin multi_launcher`; no native acceptance yet.

Application build succeeded, exit 0, Cargo 2m00s / command 120.971s. Build uses M1-A HEAD plus source diff SHA256 `9AA53E0B5CABB4826CECEAFC44A3031E5CF14A8C0EA6AF4DA3F5B24D26BDB3DB`; only test fixture changes since initial source review. Build records: `target/visual-studios/c01-application-build{.log,-result.json}`. Existing untouched production/vendor warnings remain; no new Quick Tools compile warning.

Native **BLOCKED**, not PASS: initial Hidden launch in isolated CWD produced live owned PID18056/title `Multi Lnchr`, absent from `sky.list_windows`. One cause-specific Normal interactive relaunch produced PID17264, responsive, exact expected executable, MainWindowHandle46273734/session1/title `Multi Lnchr`; both `sky.list_apps` and `sky.list_windows` still lacked a targetable launcher window. The Explorer folder titled Multi_Launcher is not the app and was never selected. No native input or screenshot was executed. Both owned processes retired, confirmed absent. Profile persisted next/cancel shortcuts as null. Receipt `target/visual-studios/c01-native-result.json`; underlying observation failure cause remains unknown. Required native editor/dashboard narrow/expanded, default animation, scroll and Jump proof NOT_RUN. No further native retries at this boundary.

Independent final source review: no known unsafe behavior/unresolved source defect, safe to retain tested partial work. **M1-B full acceptance and C01 completion withheld**. This is a safe partial source checkpoint, not a completed native feature. Diff inspected, only Quick Tools implementation/tests, dashboard test helpers/case and ledger intentional; no shared dashboard production change. Normal push attempted after commit, actual SHA/result recorded next.
