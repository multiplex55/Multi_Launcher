# Launcher and Utility Fixes execution

Contract: docs/plans/CODEX_GOAL_LAUNCHER_AND_UTILITY_FIXES.md and companion Kickstart. One checkout, sequential source writers and Cargo jobs. Parent owns Git and publication.

## M0-A ancestry

- Preparation checkout: clean master at 7af8148a20559e8bd2b488dbd0510c4630b2d0fd, equal origin/master.
- Required parent_branch: feat/productivity-tools-refinement.
- parent_tip_sha: 16b740fec66a3c077b63ac22a31cfb9577896e65; local/origin refs and authoritative final report commit agree.
- Predecessor STATUS is terminal LIMITED: development finished; missing native evidence remains unaccepted. No later implementation handoff found. master subsequently merged that tip and archived documentation; source/Cargo/build inputs compare equal.
- Task branch: codex/launcher-utility-fixes, switched directly from exact parent. No user changes existed or were discarded. Initial Git lock denial resolved through authorized escalation.
- Goal/Kickstart intentionally copied from preparation master; documentation archival is not replayed. Baseline SHA and direct-parent verification follow after commit.
- Initial remote check failed: github.com:443 connection unavailable. Initial push follows baseline; failure will not stop implementation.
- No worktrees, auxiliary checkouts/clones, independent checkpoint branches or intermediate merges used.

## Ordered checkpoint plan

Full objective/ownership/scope/invariants/tests/acceptance packets are Goal section F. Each row is a coherent commit boundary followed by one ordinary push attempt. Tests are targeted Nextest fast-dev; app fixtures disable Paste Sequence next/cancel hotkeys. Native evidence is recorded independently from state tests.

| Checkpoint | Task and deliverable | State |
| --- | --- | --- |
| M0-A | Exact-parent child, own baseline, initial push | complete |
| M1-A | A: root query and intentional focus transfer | complete (native pending C smoke) |
| M2-A | B: root-results-local fade override | complete (native pending C smoke) |
| M3-A | C: release profile and bounded feature audit | complete (release pending M3-B) |
| M3-B | C: idle capability work and ordinary release build | pending |
| M3-R | Independent A-C review/report/GitHub publication | pending |
| M4-A | D: recovery persistence before scratch close | pending |
| M5-A | E: accepted viewer-only preview dragging | pending |
| M6-A | F: top-level pending decision controls | pending |
| M7-A | G: bounded details, typed cached sort/filter, reveal | pending |
| M7-B | G: bounded directory/path capacity | pending |
| M8-A | H: actual Shell baseline observation correction | pending |
| M9-A | Final independent review, scoped Clippy, exact-tip handoff | pending |

Continue directly from M3-R to D-H without approval or merge waits. Publication/native limitations remain explicit; they do not stop safe implementation. No broad suite or historical acceptance campaign.

M0-A receipt: baseline_sha c10da8ddd90d3ebc7cf477409deb7921921a10d8; immediate parent verified equal required parent. Elevated ordinary initial push succeeded; upstream origin/codex/launcher-utility-fixes. Initial sandbox network failure did not persist in authorized Git execution. M1-A planning active.


## Verification environment preparation

- Computer-use skill read and @oai/sky initialized through node_repl. App/window inventory succeeds; no native interaction claimed yet.
- Isolated synthetic settings/actions profile prepared at target/launcher-utility-fixes/native-profile, based on predecessor task-owned profile. Paste Sequence next/cancel and launcher global shortcuts disabled before construction; empty index roots and empty actions. No user profile edited.
- Host registry: Windows 10 Home 22H2, build 19045.6466 (receipt target/launcher-utility-fixes/evidence/os.json). Supported Windows 11 Explorer tab end-to-end behavior cannot be demonstrated on this OS; H observation investigation and safe refusal remain independently testable.
- GitHub connector confirms repository multiplex55/Multi_Launcher, default integration branch master and push permission; no existing open PR for task head. gh is unavailable; use connector for requested publication at M3-R.

## M1-A investigation (active, not accepted)

Planner identified global-open-state gates across navigation/Jump/focus plus incomplete actual-host classification. Implementer owns the sequential source changes and focused actual-frame regressions. Detached Note/Markdown and accepted active Jump query-click (including rejected discarded press) have passing focused evidence so far; Diagram embedded host tracking also passed. The detached Pad case exposed a press-frame interactivity gate despite stable query geometry, requiring root-cause correction before checkpoint acceptance. No later task source implementation has started.

The local test binary requires RUST_MIN_STACK=16777216 for these large application fixtures; an existing neighboring Pad/Dashboard test also overflows at default stack. A temporary test-only instrumentation lock panic ended through egui's own 10s protection, with no remaining Cargo/rustc/test processes. Subsequent compile and test execution are separated; the existing owned-process runner bounds pure-test execution to 100s and records cleanup/output drain. Final exact selectors/results follow on completion; diagnostic failures are not passes or native proof.


## M1-A result

Root presentation now comes from actual host layers for Notes/Markdown/Pad/Diagram and companion/viewer surfaces. Root navigation and unsupported Jump classification use ROOT presentation; detached-only contexts remain ordinary query contexts. Native Notes/MkMacro no longer register ROOT navigation ownership. A real query response stages cancellation of the exact active Jump generation; only retained completion after current identity/visibility/geometry/focus/blocker validation retires its hint and stale focus intent. Diagram owner embedded layer is tracked/cleared through accepted lifecycle effects. No global any_panel_open semantics or modal/capture/foreground protections changed.

New actual-frame coverage spans five detached hosts, Jump on/off, multiple detached Notes/Markdown, Unicode and persistent focus/drafts; active hint transfer rejects a discarded press. Embedded Diagram ownership assertion and explicit stateful-host policy classification retained. Investigation resolved Pad's synthetic first-click failure as prior docked-layer fixture state: the test establishes an actual retained interactive rectangle before sending the real click; production received no delay/workaround. Temporary diagnostics removed. Parent inspected full source/test diff; git diff --check passed.

Verification: RUST_MIN_STACK=16777216; cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E with exact selector below. 15/15 passed, 7,673 skipped, 9.363s execution, run 6d2c6a33-45bf-4a96-8ffa-30f237ad2bc2. Existing owned-process wrapper: 100s execution deadline excluding separately completed compilation; 11.467s actual, exit0, cleanup and output drain verified. Receipts target/launcher-utility-fixes/evidence/m1a-focused/{result.json,stderr.log}. Native verification pending consolidated release smoke after C; these are egui/state tests, not Windows UI proof.

```text
test(studio_usability_query_focus_) | test(studio_usability_diagram_viewer_embedded_fallback_keeps_root_host_identity) | test(notes_query_mouse_) | test(notes_query_hint_focuses_actual_query_and_returns_to_quick_notes_draft) | test(jump_mode_actual_query_capture_preserves_text_and_survives_discarded_pass) | test(notes_query_activation_mouse_workflow_runs_current_result_beside_notes) | test(notes_query_activation_autocomplete_tab_keeps_focus_through_discarded_pass) | test(launcher_show_focus_waits_for_root_focus_and_never_refocuses_an_already_visible_query) | test(launcher_show_focus_cancels_on_click_tab_and_protected_owner_takeover) | test(launcher_show_focus_rejects_replaced_root_generation)
```

M1-A commit 6ec61e5b1d765022ee06381cd419d6acd15455b8; ordinary git push succeeded; clean checkpoint tree. M2-A planning active.

## M2-A result

The existing root-results UI saves its fade strength, sets scroll.fade.strength=0 immediately before the shared list/grid ScrollArea, and restores it immediately afterward. No new child UI/widget identity, global/vendor styling, selection-scroll timing or geometry changes. Parent inspected exact scoped diff; git diff --check passed.

RUST_MIN_STACK=16777216; cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(root_list_viewport_builds_bounded_rows_with_absolute_ids_and_selection) | test(root_grid_viewport_builds_complete_bounded_rows_with_absolute_ids_and_click_targets)': 2/2 passed, 7,686 skipped, 2.962s execution, run 12b46689-66d7-4b5d-a08c-17e96e77db1d. Separate compilation 1m20s; existing100s owned runner exit0 in4.986s, cleanup/output drain verified. Receipts target/launcher-utility-fixes/evidence/m2a-root-results/{result.json,stderr.log}. Native visual check pending C release smoke; no evidence supports an extra selection-scroll change.

M2-A commit b5df0946752e10d597bc76f01d84df8fa83d754a; normal push succeeded, clean checkpoint tree. M3-A planning active.

## M3-A result

Cargo.toml directly defines release opt-level=3, lto=thin, codegen-units=16, strip=symbols, panic=unwind. iteration adds strip=none; existing opt2/cgu64/ltooff/incremental and effective debug/panic fields preserved; dev/fast-dev untouched. Parent inspected manifest diff; Cargo.lock unchanged. Thin LTO may increase build/link work; no benchmark/performance metrics claimed.

Bounded feature audit: cargo tree -e features -i eframe; cargo tree -e features -i image@0.24.9; cargo tree -e features -i image@0.25.6 all succeeded. No safe removal identified. Glow explicitly selected in main.rs4515; image024 serves screenshot/crop/diagram PNG/JPEG (screenshots also enables defaults), image025 serves egui_commonmark/extras and notes_markdown/html.rs541 decoder dispatch. Keep intentional decoder sets, accessibility/fonts/platform/link/web support; production catch_unwind in main.rs7191 and diagram/rendering.rs1488 requires unwind. No feature/version migration.

cargo metadata --no-deps --format-version1 parsed manifest; profile-only TOML comparison against HEAD confirmed effective iteration settings preserved; git diff --check passed. Initial image@0.25.10 audit query corrected to actual0.25.6. Python3.11 tomllib rejects existing Cargo-supported multiline inline tables, so comparison parsed profile sections only, with Cargo metadata validating the manifest. No behavior tests needed for unchanged dependencies. Ordinary release build pending M3-B.
